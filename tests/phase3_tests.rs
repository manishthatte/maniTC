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

fn manitc() -> PathBuf { PathBuf::from(env!("CARGO_BIN_EXE_manitc")) }

fn write(stem: &str, src: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("manitc_p3_{}", std::process::id()));
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
    if !c.status.success() {
        let blob = String::from_utf8_lossy(&c.stderr).to_string();
        if blob.contains("clang") { return None; }
        panic!("LLVM compile failed:\n{}", blob);
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
