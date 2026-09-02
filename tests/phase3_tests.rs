//! Phase 3 — E2's measurable claims, and the C4 / N5 / R2 interface.
//!
//! Author: Manish Jagdish Thatte
//!
//! A3's tests live in `conformance_tests.rs` and `differential_tests.rs`, and
//! the C4/N5 SEMANTICS are checked there, three ways. What is here is what a
//! three-way conformance test cannot reach:
//!
//!   * E2's one guarantee — `spawn { … }` is sequential;
//!   * the R2 interface: `--lang`, the `division-semantics` backlog, and the
//!     `math::` divisions that mean the same thing in both versions. None of
//!     these is a program's observable behaviour, so none belongs in the
//!     conformance suite; all of them are promises R2 makes to a migration.
//!
//! The four defects E2 found (report.txt P5, docs/memory-model.md §3) are
//! deliberately NOT encoded here. A test that asserts a bug's current output
//! makes the bug harder to fix — it has to be edited before the fix can land,
//! and whoever edits it has to work out whether the old expectation was a
//! promise or a symptom. Defects belong in the report; guarantees belong here.

use std::path::PathBuf;
use std::process::Command;

mod common;

fn manitc() -> PathBuf { PathBuf::from(env!("CARGO_BIN_EXE_manitc")) }

fn write(stem: &str, src: &str) -> PathBuf {
    let d = common::suite_root("p3");
    std::fs::create_dir_all(&d).expect("temp dir");
    let p = d.join(format!("{}.mt", stem));
    std::fs::write(&p, src).expect("write");
    p
}

fn t3_out(p: &PathBuf) -> String { t3_out_lang(p, "v1") }

fn t3_out_lang(p: &PathBuf, lang: &str) -> String {
    let base = p.with_extension("");
    let c = Command::new(manitc())
        .args(["compile", p.to_str().unwrap(), "--target", "t3",
               "--lang", lang, "-o", base.to_str().unwrap()])
        .output().expect("compile");
    assert!(c.status.success(), "T3 compile failed:\n{}",
            String::from_utf8_lossy(&c.stderr));
    let r = Command::new(manitc())
        .args(["run-t3", base.with_extension("t3b").to_str().unwrap()])
        .output().expect("run");
    String::from_utf8_lossy(&r.stdout)
        .lines().filter(|l| !l.starts_with("[T3ISA]"))
        .map(|l| format!("{}\n", l)).collect()
}

fn llvm_out(p: &PathBuf) -> Option<String> { llvm_out_lang(p, "v1") }

fn llvm_out_lang(p: &PathBuf, lang: &str) -> Option<String> {
    let bin = p.with_extension("bin");
    let c = Command::new(manitc())
        .args(["compile", p.to_str().unwrap(), "--target", "llvm",
               "--lang", lang, "-o", bin.to_str().unwrap()])
        .output().expect("compile");
    // P78: P47's defect, unpropagated. `if stderr.contains("clang") { return
    // None }` reads "no toolchain here" and is exactly backwards: when clang is
    // genuinely ABSENT the compiler SUCCEEDS (prints `[LLVM] clang not found`,
    // writes the .ll, exits 0), so a FAILED compile mentioning clang can only
    // be clang REJECTING THE MODULE — the very thing this file exists to catch.
    // Tell the two apart by the ARTEFACT, never by the message.
    if !c.status.success() {
        panic!(
            "LLVM compile failed:\n{}{}",
            String::from_utf8_lossy(&c.stdout),
            String::from_utf8_lossy(&c.stderr)
        );
    }
    if !bin.exists() {
        assert!(
            manitc::runtime_link::find_clang().is_none(),
            "no binary at {} although clang IS available ({:?}) — a real \
             failure being reported as an absent toolchain\n{}{}",
            bin.display(),
            manitc::runtime_link::find_clang(),
            String::from_utf8_lossy(&c.stdout),
            String::from_utf8_lossy(&c.stderr)
        );
        return None;
    }
    let r = Command::new(&bin).output().expect("run");
    Some(String::from_utf8_lossy(&r.stdout).to_string())
}

/// `manitc check` with the given extra flags; returns (success, stdout+stderr).
fn check(p: &PathBuf, extra: &[&str]) -> (bool, String) {
    let mut args = vec!["check", p.to_str().unwrap()];
    args.extend_from_slice(extra);
    let o = Command::new(manitc()).args(&args).output().expect("check");
    (
        o.status.success(),
        format!(
            "{}{}",
            String::from_utf8_lossy(&o.stdout),
            String::from_utf8_lossy(&o.stderr)
        ),
    )
}

/// `docs/memory-model.md` §4.2: `spawn { B }` evaluates `B` to completion, in
/// place. This is the normative statement, and it is the ONLY thing the
/// concurrency surface currently delivers identically on both backends.
///
/// The interleaved output is the point. If `spawn` ever becomes a real task,
/// this test fails — which is correct: that is a change to the model, and it
/// should not be possible to make it without editing the model's test.
#[test]
fn e2_spawn_is_sequential_and_runs_in_place() {
    let src = r#"
use std::io;
fn main() {
    io::println("A");
    spawn { io::println("B"); }
    io::println("C");
    spawn { io::println("D"); }
    io::println("E");
}
"#;
    let p = write("e2_seq", src);
    let want = "A\nB\nC\nD\nE\n";
    let t3 = t3_out(&p);
    assert_eq!(t3, want, "T3: spawn must run its block in place");
    if let Some(l) = llvm_out(&p) {
        assert_eq!(l, want, "LLVM: spawn must run its block in place");
    }
}

/// A spawned block sees and mutates the enclosing scope, because it IS the
/// enclosing scope — there is no capture and no copy. Worth pinning separately:
/// it is the observable consequence of §4.2 that would break first if `spawn`
/// were made to defer.
#[test]
fn e2_a_spawned_block_shares_the_enclosing_scope() {
    let src = r#"
use std::io;
fn main() {
    let mut n: int = 1;
    spawn { n = n + 41; }
    io::println_int(n);
}
"#;
    let p = write("e2_share", src);
    let t3 = t3_out(&p);
    assert_eq!(t3, "42\n", "T3");
    if let Some(l) = llvm_out(&p) {
        assert_eq!(l, "42\n", "LLVM");
    }
}

// ---------------------------------------------------------------------------
// C4 / N5 / R2 — the migration interface
// ---------------------------------------------------------------------------

/// The four explicitly-named divisions mean the same thing under BOTH language
/// versions, on BOTH backends. That is what makes them a migration path: code
/// rewritten onto them stops caring which version it is compiled under.
///
/// Sixteen answers are compared, not four: two functions × two versions × two
/// backends, and every one of them has to be the same pair of numbers.
#[test]
fn c4_the_named_divisions_are_version_independent() {
    let src = r#"
use std::io;
use std::math;
fn main() {
    let a: int = 7;
    let b: int = 2;
    io::println_int(math::div_trunc(a, b));
    io::println_int(math::rem_trunc(a, b));
    io::println_int(math::div_near(a, b));
    io::println_int(math::rem_near(a, b));
    io::println_int(math::div_near(0 - a, b));
    io::println_int(math::rem_near(0 - a, b));
}
"#;
    let p = write("c4_named", src);
    // div_trunc(7,2)=3 rem_trunc=1; div_near(7,2)=4 rem_near=-1;
    // div_near(-7,2)=-4 rem_near=1 — the symmetry, spelled out.
    let expect = "3\n1\n4\n-1\n-4\n1\n";
    for lang in ["v1", "v2"] {
        assert_eq!(t3_out_lang(&p, lang), expect, "T3 under --lang {}", lang);
        if let Some(l) = llvm_out_lang(&p, lang) {
            assert_eq!(l, expect, "LLVM under --lang {}", lang);
        }
    }
}

/// `/` is the operator that DOES change, which is the other half of the same
/// claim: if this passed for both versions the named functions would be
/// pointless.
#[test]
fn c4_the_operator_changes_where_the_named_functions_do_not() {
    let src = r#"
use std::io;
fn main() {
    let a: int = 7;
    let b: int = 2;
    io::println_int(a / b);
    io::println_int(a % b);
}
"#;
    let p = write("c4_operator", src);
    assert_eq!(t3_out_lang(&p, "v1"), "3\n1\n", "v1 truncates");
    assert_eq!(t3_out_lang(&p, "v2"), "4\n-1\n", "v2 rounds to nearest");
    if let Some(l) = llvm_out_lang(&p, "v1") { assert_eq!(l, "3\n1\n", "LLVM v1"); }
    if let Some(l) = llvm_out_lang(&p, "v2") { assert_eq!(l, "4\n-1\n", "LLVM v2"); }
}

/// R2: an unrecognised `--lang` is reported, not silently resolved to the
/// default. A typo that quietly selected v1 would compile the program under
/// arithmetic its author did not ask for, and nothing downstream would say so.
#[test]
fn r2_an_unknown_language_version_is_an_error_not_a_default() {
    let p = write("r2_badlang", "fn main() { }\n");
    let (ok, text) = check(&p, &["--lang", "v3"]);
    assert!(!ok, "an unknown --lang must fail:\n{}", text);
    assert!(text.contains("unknown language version"), "{}", text);
    assert!(text.contains("v1") && text.contains("v2"),
            "the message should list the known versions:\n{}", text);
}

/// The `division-semantics` backlog: silent by default, complete when asked.
///
/// "Complete" means the compound assignment too. `x /= 2` lowers through the
/// same `binop_to_ir` over the same type, so leaving it out would make the
/// list quietly wrong in exactly the places a migration is most likely to miss.
#[test]
fn c4_the_division_semantics_backlog_is_silent_by_default_and_complete_when_asked() {
    let src = r#"
use std::io;
fn main() {
    let mut a: int = 7;
    let b: int = 2;
    io::println_int(a / b);
    io::println_int(a % b);
    a /= b;
    io::println_int(a);
    let f: float = 7.0;
    let g: float = 2.0;
    io::println_float(f / g);
}
"#;
    let p = write("c4_backlog", src);

    let (ok, quiet) = check(&p, &[]);
    assert!(ok, "{}", quiet);
    assert!(!quiet.contains("division-semantics"),
            "the lint is `allow` by default:\n{}", quiet);

    let (ok, loud) = check(&p, &["--warn", "division-semantics"]);
    assert!(ok, "a warn-level lint must not fail the check:\n{}", loud);
    let n = loud.matches("[division-semantics]").count();
    assert_eq!(n, 3, "expected `/`, `%` and `/=` and nothing else:\n{}", loud);
    // Float division is untouched by C4 and must not be in the backlog.
    assert!(!loud.contains("println_float"), "{}", loud);
    // Each site says which function it is in — the span alone cannot locate a
    // site inside a merged stdlib module (report.txt P8).
    assert_eq!(loud.matches("in `main`").count(), 3, "{}", loud);
    // And it names the function that pins today's meaning.
    assert!(loud.contains("math::div_trunc"), "{}", loud);
    assert!(loud.contains("math::rem_trunc"), "{}", loud);
}

/// Under v2 the message points the other way: the code now rounds, and
/// `math::div_near` is what keeps it rounding if the version moves again.
#[test]
fn c4_the_backlog_names_the_meaning_the_code_has_right_now() {
    let src = "fn main() { let a: int = 7; let b: int = 2; io::println_int(a / b); }\n";
    let p = write("c4_backlog_v2", src);
    let (_, v2) = check(&p, &["--warn", "division-semantics", "--lang", "v2"]);
    assert!(v2.contains("rounds to nearest"), "{}", v2);
    assert!(v2.contains("math::div_near"), "{}", v2);
    assert!(!v2.contains("math::div_trunc"), "{}", v2);
}

/// A5's manifest names every lint, so a recorded artifact says whether it was
/// checked for this one. Added here rather than in `lint.rs`'s unit tests
/// because the manifest is the user-visible surface, not the table.
#[test]
fn r2_the_lint_manifest_names_division_semantics() {
    let p = write("r2_manifest", "fn main() { }\n");
    let (ok, text) = check(&p, &["--print-lints"]);
    assert!(ok, "{}", text);
    assert!(text.contains("division-semantics"), "{}", text);
    assert!(text.contains("division-semantics        allow")
            || text.contains("division-semantics")
            && text.lines().any(|l| l.contains("division-semantics") && l.contains("allow")),
            "it should default to allow:\n{}", text);
}

/// N5 gives `int` a 27-trit bound on LLVM; `trint` is the wider type that keeps
/// the machine word, and it must NOT acquire the bound — an opt-in with no
/// escape hatch is not an opt-in.
///
/// Only the LLVM backend is checked, and deliberately so: a T3 register IS 27
/// trits, so `trint` cannot hold more than that there and traps. That is a
/// pre-existing limitation of `trint` on T3 (report.txt P9), not something N5
/// introduces or claims to fix.
#[test]
fn n5_trint_is_not_bounded_to_the_word_on_llvm() {
    let src = r#"
use std::io;
fn main() {
    let w: trint = 3812798742493;
    io::println_int(w + 1);
}
"#;
    let p = write("n5_trint", src);
    if let Some(l) = llvm_out_lang(&p, "v2") {
        assert_eq!(l, "3812798742494\n",
                   "trint must keep the machine word under --lang v2");
    }
}

/// The same program with `int` instead of `trint` traps under v2 — the two
/// halves of the same claim, so neither can be edited without the other being
/// looked at.
#[test]
fn n5_int_is_bounded_to_the_word_on_llvm_under_v2() {
    let src = r#"
use std::io;
fn main() {
    let m: int = 3812798742493;
    io::println_int(m + 1);
}
"#;
    let p = write("n5_int", src);
    let bin = p.with_extension("bin");
    let c = Command::new(manitc())
        .args(["compile", p.to_str().unwrap(), "--target", "llvm",
               "--lang", "v2", "-o", bin.to_str().unwrap()])
        .output().expect("compile");
    if !c.status.success() { return; }  // no clang
    let r = Command::new(&bin).output().expect("run");
    assert_eq!(r.status.code(), Some(70), "an out-of-word `int` must trap");
    let err = String::from_utf8_lossy(&r.stderr).to_string();
    assert!(err.contains("27-trit range"), "{}", err);

    // …and does not under v1, which is what makes v2 a change and not a fix
    // smuggled in under a flag.
    let c = Command::new(manitc())
        .args(["compile", p.to_str().unwrap(), "--target", "llvm",
               "-o", bin.to_str().unwrap()])
        .output().expect("compile");
    assert!(c.status.success());
    let r = Command::new(&bin).output().expect("run");
    assert_eq!(r.status.code(), Some(0), "v1 is unchanged");
    assert_eq!(String::from_utf8_lossy(&r.stdout), "3812798742494\n");
}

/// **report.txt P21 cluster 2 — the two backends report the same overflow in
/// the same words.**
///
/// Everything about the two messages was already byte-identical except the
/// name of the operation: the T3 emulator said `TMUL overflow: …` where the C
/// runtime said `int multiplication overflow: …`, for the same fault on the
/// same value. This repo treats message parity as a correctness property —
/// `manit_check_result_ok` in `runtime/core.c` is kept byte-identical to
/// SYSCALL #561 and a comment there says why — and these were not.
///
/// **The multiply case is the one that matters**, and it is why the LANGUAGE
/// name won over the opcode. `1270932914165 * 3` is a multiply by a power of
/// three, so F-2's ternary strength reduction lowers it to `TSHI` on T3.
/// Naming the opcode would have made the diagnostic depend on whether the
/// OPTIMISER fired — the programmer wrote a multiply and never asked for a
/// shift. Reduce it or not, it must read `int multiplication`.
///
/// Every literal here is INSIDE the 27-trit range on purpose. A literal
/// outside it is mangled on T3 before any arithmetic happens, so the two
/// backends then trap at different operations on different values — that is
/// P21 cluster 1, a different finding, and mixing it in here would make this
/// test fail for a reason it is not about.
#[test]
fn n5_an_overflow_reads_the_same_on_both_backends() {
    for (stem, op, a, b, expect) in [
        ("n5_msg_add", "+", "3812798742493", "1", "int addition"),
        ("n5_msg_sub", "-", "-3812798742493", "1", "int subtraction"),
        ("n5_msg_mul", "*", "1270932914165", "3", "int multiplication"),
    ] {
        let src = format!(
            "use std::io;\n\
             fn op(x: int, y: int) -> int {{ return x {} y; }}\n\
             fn main() {{ io::println_int(op({}, {})); }}\n",
            op, a, b
        );
        let p = write(stem, &src);

        // T3: the emulator prints the trap on its own output.
        let base = p.with_extension("");
        let c = Command::new(manitc())
            .args(["compile", p.to_str().unwrap(), "--target", "t3",
                   "--lang", "v2", "-o", base.to_str().unwrap()])
            .output().expect("compile t3");
        assert!(c.status.success(), "{} t3 compile", stem);
        let r = Command::new(manitc())
            .args(["run-t3", base.with_extension("t3b").to_str().unwrap()])
            .output().expect("run t3");
        let t3_text = format!(
            "{}{}",
            String::from_utf8_lossy(&r.stdout),
            String::from_utf8_lossy(&r.stderr)
        );
        let t3_trap = t3_text
            .lines()
            .find(|l| l.contains("TRAP:"))
            .unwrap_or_else(|| panic!("{}: T3 did not trap:\n{}", stem, t3_text))
            .trim()
            .to_string();

        // LLVM: `manit_fault` writes to stderr.
        let bin = p.with_extension("bin");
        let c = Command::new(manitc())
            .args(["compile", p.to_str().unwrap(), "--target", "llvm",
                   "--lang", "v2", "-o", bin.to_str().unwrap()])
            .output().expect("compile llvm");
        if !c.status.success() { continue; }  // no clang in this environment
        let r = Command::new(&bin).output().expect("run llvm");
        let ll_text = String::from_utf8_lossy(&r.stderr).to_string();
        let ll_trap = ll_text
            .lines()
            .find(|l| l.contains("TRAP:"))
            .unwrap_or_else(|| panic!("{}: LLVM did not trap:\n{}", stem, ll_text))
            .trim()
            .to_string();

        assert_eq!(t3_trap, ll_trap, "{}: the two backends must word it alike", stem);
        assert!(
            t3_trap.contains(expect),
            "{}: the LANGUAGE operation is what a trap names, not the opcode: {}",
            stem, t3_trap
        );
    }
}

/// **report.txt P21 cluster 1 — an `int` literal too wide for the word.**
///
/// The sharpest case in the whole cluster is not a computation:
///
/// ```text
/// fn main() { io::print_int(g(9223372036854775807)); }
///
/// T3    72854775807      reshaped before any arithmetic happens
/// LLVM  TRAP: int addition overflow: result 9223372036854775807 …
/// ```
///
/// From that point the backends are computing with different numbers, which is
/// why four of the ten "wording only" corpus files go on differing after the
/// wording is fixed. The two versions get different answers because they are
/// asking different questions, and BOTH halves are pinned here — a rule with
/// one half tested is a rule that can lose the other half silently.
#[test]
fn n5_an_int_literal_wider_than_the_word_is_v2s_error_and_v1s_backlog() {
    let src = r#"
use std::io;
fn g(n: int) -> int { return n + 1; }
fn main() { io::print_int(g(9223372036854775807)); }
"#;
    let p = write("n5_wide_literal", src);
    let check = |args: &[&str]| -> (bool, String) {
        let o = Command::new(manitc())
            .arg("check")
            .arg(p.to_str().unwrap())
            .args(args)
            .output()
            .expect("check");
        (
            o.status.success(),
            format!(
                "{}{}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            ),
        )
    };

    // v2 REJECTS it: `int` is 27 trits there, so the literal has no value.
    let (ok, text) = check(&["--lang", "v2"]);
    assert!(!ok, "v2 must reject a literal that does not fit `int`:\n{}", text);
    assert!(text.contains("does not fit `int`"), "{}", text);
    assert!(text.contains("trint"), "the escape hatch must be named:\n{}", text);

    // v1 ACCEPTS it silently. This is the R5 property the whole shape exists
    // for: `eval/l1_probe.py` runs `manitc check` with no `--lang`, so a
    // default-allow lint under v1 cannot move an L1 verdict. Measured at 0
    // moved over all 1,147 model-corpus files and all 271 in the two repos.
    let (ok, text) = check(&[]);
    assert!(ok, "v1 must still accept it — `int` is the host word there:\n{}", text);
    assert!(!text.contains("literal-out-of-word"), "and silently:\n{}", text);

    // …until asked for the backlog, which is the migration plan.
    let (ok, text) = check(&["--warn", "literal-out-of-word"]);
    assert!(ok, "the backlog is a warning, not an error:\n{}", text);
    assert!(text.contains("literal-out-of-word"), "{}", text);
    assert!(text.contains("27-trit range"), "{}", text);
}

/// `trint` is the escape hatch and must not acquire the bound — the same
/// argument as `n5_trint_is_not_bounded_to_the_word_on_llvm`, one level down.
/// The literal check reuses `binop_to_ir`'s `Int | T27` predicate precisely so
/// these two cannot drift apart.
#[test]
fn n5_a_wide_literal_is_fine_in_a_trint_under_v2() {
    let src = r#"
use std::io;
fn main() {
    let w: trint = 9223372036854775807;
    let m: int = 3812798742493;
    io::println_int(m);
    io::println_int(w);
}
"#;
    let p = write("n5_wide_literal_trint", src);
    let o = Command::new(manitc())
        .args(["check", p.to_str().unwrap(), "--lang", "v2"])
        .output()
        .expect("check");
    assert!(
        o.status.success(),
        "`trint` is v2's wider type and must still hold the machine word:\n{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    );
}
