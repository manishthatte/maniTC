//! P36 — loop-invariant constant materialisation.
//!
//! © Manish Jagdish Thatte
//!
//! Every T3ISA data-processing opcode carries a balanced 3-trit immediate in
//! its third operand slot, so a constant in `-13..=13` is free (P40). Anything
//! wider is materialised with a `TLIT`, and the emitter materialised it at each
//! USE — so a loop body containing `acc + 24690` emitted `TLIT R7, #24690` on
//! every iteration.
//!
//! **The measurement that motivated the pass turned out to be an upper bound on
//! nothing, and that is the result worth keeping.** Executed `TLIT`s minus
//! static `TLIT` sites over the seventeen examples is 43,191 − 22,291 = 20,900
//! re-materialisations, 5.57 % of all instructions executed. Hoisting into
//! every loop captures none of it: it is **+4,880 (+1.30 %), 1 better and 9
//! worse**, `crypto_demo` alone +5,218. `regalloc` keeps nothing in a register
//! across a call, so a hoisted value spanning one is spilled and each use
//! becomes a frame `LOAD` rather than a `TLIT` — the same one instruction, plus
//! the preheader. In a call-containing loop the value must be spilled or
//! re-materialised and BOTH cost one instruction, so there was no win there to
//! take.
//!
//! Refusing call-containing loops turns it into **−932 (−0.25 %), 2 better and
//! 0 worse**, with `crypto_demo` moving from +5,218 to −312 and `float_demo`
//! to −7.78 %. Register pressure was the obvious explanation and was wrong for
//! the third time in this campaign: the per-loop limit sweeps FLAT, +5,159 at
//! one value per loop against +4,880 at eight.
//!
//! **T3 only.** On LLVM a constant operand costs nothing and clang folds the
//! temp straight back out, so the pass would move the emitted `.ll` for no gain
//! and break the byte-for-byte comparison `--no-mem2reg` exists to give. The
//! two backends therefore receive IR differing by this pass, and the rows below
//! assert on both that they still compute the same answers.

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

mod common;

static N: AtomicUsize = AtomicUsize::new(0);

fn manitc_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_manitc"))
}

fn workdir() -> PathBuf {
    let slot = N.fetch_add(1, Ordering::Relaxed);
    let d = common::suite_root("p36")
        .join(slot.to_string());
    std::fs::create_dir_all(&d).expect("temp dir");
    d
}

/// Compile and run on T3. Panics with the compiler's own output on failure.
fn run_t3(src: &str) -> String {
    let d = workdir();
    let path = d.join("p.mt");
    std::fs::write(&path, src).expect("write");
    let base = path.with_extension("");

    let c = Command::new(manitc_bin())
        .args(["compile", path.to_str().unwrap(), "--target", "t3",
               "-o", base.to_str().unwrap()])
        .output()
        .expect("compile");
    assert!(c.status.success(), "T3 compile failed:\n{}\n{}",
            String::from_utf8_lossy(&c.stdout), String::from_utf8_lossy(&c.stderr));

    let r = Command::new(manitc_bin())
        .args(["run-t3", base.with_extension("t3b").to_str().unwrap()])
        .output()
        .expect("run");
    String::from_utf8_lossy(&r.stdout)
        .lines()
        .filter(|l| !l.starts_with("[T3ISA]"))
        .map(|l| format!("{}\n", l))
        .collect()
}

/// Compile and run on LLVM. Returns `None` when clang is absent, so the row
/// degrades to T3-only rather than failing for an environment reason --
/// `conformance_tests.rs` documents why a clang MENTION is not a clang
/// ABSENCE, so this checks for the compiler's own "not found" line.
fn run_llvm(src: &str) -> Option<String> {
    let d = workdir();
    let path = d.join("p.mt");
    std::fs::write(&path, src).expect("write");
    let bin = d.join("p.bin");

    let c = Command::new(manitc_bin())
        .args(["compile", path.to_str().unwrap(), "--target", "llvm",
               "-o", bin.to_str().unwrap()])
        .output()
        .expect("compile");
    let msg = format!("{}{}", String::from_utf8_lossy(&c.stdout),
                      String::from_utf8_lossy(&c.stderr));
    if msg.contains("clang not found") {
        return None;
    }
    assert!(c.status.success(), "LLVM compile failed:\n{}", msg);

    let r = Command::new(&bin).output().expect("run llvm binary");
    Some(String::from_utf8_lossy(&r.stdout).into_owned())
}

/// Assert both backends agree on `want`.
fn both(src: &str, want: &str, what: &str) {
    let t3 = run_t3(src);
    assert!(t3.contains(want), "{what}: T3 gave {t3:?}, wanted {want:?}");
    if let Some(ll) = run_llvm(src) {
        assert!(ll.contains(want), "{what}: LLVM gave {ll:?}, wanted {want:?}");
    }
}


/// Compile to T3 and hand back the generated assembly.
fn t3_asm(src: &str, extra: &[&str]) -> String {
    let d = workdir();
    let path = d.join("p.mt");
    std::fs::write(&path, src).expect("write");
    let base = path.with_extension("");
    let mut args: Vec<String> = vec![
        "compile".into(), path.to_string_lossy().into_owned(),
        "--target".into(), "t3".into(),
        "-o".into(), base.to_string_lossy().into_owned(),
    ];
    args.extend(extra.iter().map(|s| s.to_string()));
    let c = Command::new(manitc_bin()).args(&args).output().expect("compile");
    assert!(c.status.success(), "T3 compile failed:\n{}{}",
            String::from_utf8_lossy(&c.stdout), String::from_utf8_lossy(&c.stderr));
    std::fs::read_to_string(base.with_extension("t3s")).expect("read .t3s")
}

/// The body of the first `while` loop in the emitted assembly.
fn loop_body(asm: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    for line in asm.lines() {
        if line.trim_end().ends_with("while_body1:") {
            inside = true;
            continue;
        }
        if inside {
            if line.contains("JUMP") { break; }
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

const HOT: &str = "
fn main() {
    let mut i: int = 0;
    let mut acc: int = 0;
    while i < 200 {
        acc = acc + 24690 + i;
        i = i + 1;
    }
    io::print(\"acc=\"); io::println_int(acc);
}
";

// ---------------------------------------------------------------------------
// It does the thing, and it does not change the answer
// ---------------------------------------------------------------------------

#[test]
fn p36_a_loop_invariant_constant_leaves_the_loop_body() {
    // Asserts the SHAPE, because this is an optimisation: the answer is
    // identical either way, so a value assertion alone cannot see the pass at
    // all. The next row asserts the answer.
    let on = loop_body(&t3_asm(HOT, &[]));
    let off = loop_body(&t3_asm(HOT, &["--no-hoist-constants"]));
    assert!(
        off.contains("#24690"),
        "P36: with the pass off the constant should be materialised in the \
         loop body, but it is not:\n{off}"
    );
    assert!(
        !on.contains("#24690"),
        "P36: the loop body still materialises the constant:\n{on}"
    );
}

/// GREEN on the compiler without the pass, necessarily: an optimisation that
/// changed an answer would be a defect, so no value row can distinguish the two.
/// It is here because the IR the backends receive now DIFFERS by this pass, and
/// that is the thing worth checking.
#[test]
fn p36_the_answer_is_unchanged_on_both_backends() {
    // The property that actually matters. The IR the two backends receive now
    // DIFFERS by this pass — it is T3-only — so agreement between them is a
    // real check here rather than a shared lowering agreeing with itself.
    both(HOT, "acc=4957900", "hoisted loop still computes the right sum");
}

#[test]
fn p36_the_flag_turns_it_off_and_changes_nothing_else() {
    let d = workdir();
    let path = d.join("p.mt");
    std::fs::write(&path, HOT).expect("write");
    let base = path.with_extension("");
    let run = |extra: &[&str]| -> String {
        let mut args: Vec<String> = vec![
            "compile".into(), path.to_string_lossy().into_owned(),
            "--target".into(), "t3".into(),
            "-o".into(), base.to_string_lossy().into_owned(),
        ];
        args.extend(extra.iter().map(|s| s.to_string()));
        assert!(Command::new(manitc_bin()).args(&args).output().expect("compile").status.success());
        let r = Command::new(manitc_bin())
            .args(["run-t3", base.with_extension("t3b").to_str().unwrap()])
            .output().expect("run");
        String::from_utf8_lossy(&r.stdout)
            .lines().filter(|l| !l.starts_with("[T3ISA]"))
            .map(|l| format!("{l}\n")).collect()
    };
    assert_eq!(run(&[]), run(&["--no-hoist-constants"]),
               "P36: --no-hoist-constants changed the program's output");
}

// ---------------------------------------------------------------------------
// The two refusals, which are the whole difference from a pessimisation
// ---------------------------------------------------------------------------

/// GREEN on the compiler without the pass, for the same reason. What it
/// guards is the refusal — the difference between −0.25 % and +1.30 %.
#[test]
fn p36_a_loop_containing_a_call_is_refused() {
    // THE REFUSAL THAT MAKES THIS A WIN RATHER THAN A LOSS. `regalloc` keeps
    // nothing in a register across a call, so a hoisted value spanning one is
    // spilled and every use becomes a frame LOAD instead of a TLIT — the same
    // instruction, plus the preheader. Hoisting into every loop is +1.30 %
    // over the examples; refusing these is −0.25 %.
    // The callee is RECURSIVE on purpose. A small single-block function is
    // spliced by the inliner, and once it is the loop contains no `Call` at
    // all — the first version of this row used one and the pass hoisted,
    // correctly. `collect` refuses a callee whose CFG has a back edge (P36's
    // own earlier rule), so this one survives to the loop.
    let src = "
fn side(x: int) -> int { if x <= 0 { return 0; } return 1 + side(x - 1); }
fn main() {
    let mut i: int = 0;
    let mut acc: int = 0;
    while i < 50 {
        acc = acc + 24690 + side(i);
        i = i + 1;
    }
    io::print(\"acc=\"); io::println_int(acc);
}
";
    let on = loop_body(&t3_asm(src, &[]));
    assert!(
        on.contains("#24690"),
        "P36: a constant was hoisted out of a loop that contains a call; every \
         use inside it will be a spill reload rather than the TLIT it replaced:\n{on}"
    );
    both(src, "acc=1235725", "call-containing loop still computes the right sum");
}

/// GREEN on the compiler without the pass, which hoists nothing at all. It
/// pins the boundary rather than the fix, and only means something beside the
/// row above it, where the same loop shape with a wider constant IS hoisted.
#[test]
fn p36_a_small_constant_is_not_hoisted() {
    // A constant inside the 3-trit immediate field rides in the instruction
    // (P40), so hoisting it would spend a register and a live range to save
    // nothing. `7` is representable; `24690` is not, and the row above shows
    // the same loop shape being hoisted — so this pins the BOUNDARY rather
    // than merely observing an absence.
    let src = "
fn main() {
    let mut i: int = 0;
    let mut acc: int = 0;
    while i < 20 { acc = acc + 7 + i; i = i + 1; }
    io::print(\"acc=\"); io::println_int(acc);
}
";
    let on = loop_body(&t3_asm(src, &[]));
    assert!(
        on.contains("#7"),
        "P36: a constant that fits the 3-trit immediate was hoisted:\n{on}"
    );
    both(src, "acc=330", "small constant stays in the instruction");
}

#[test]
fn p36_the_immediate_bound_matches_the_emitter() {
    // Two registries that must agree get a test, not a comment (permanent rule
    // 5): `hoist_const::IMM3_MAX` decides what is worth hoisting and
    // `codegen_t3::emitter::t3_imm3` decides what actually rides in the
    // instruction. Asked of the COMPILER at the boundary rather than by
    // comparing two constants, so the row crosses an origin boundary (P64).
    //
    // 13 must ride in the instruction and 14 must not.
    let at = |k: i64| -> String {
        loop_body(&t3_asm(&format!("
fn main() {{
    let mut i: int = 0;
    let mut acc: int = 0;
    while i < 20 {{ acc = acc + {k} + i; i = i + 1; }}
    io::print(\"acc=\"); io::println_int(acc);
}}
"), &[]))
    };
    assert!(at(13).contains("#13"), "P36: 13 should ride in the immediate field");
    assert!(!at(14).contains("#14"), "P36: 14 does not fit the immediate field, so it should be hoisted");
}

// ---------------------------------------------------------------------------
// What the pass must not break
// ---------------------------------------------------------------------------

#[test]
fn p36_a_nested_loop_hoists_to_the_inner_preheader() {
    // The definition must dominate every use. A nested loop is where a pass
    // that hoisted to the wrong block would produce a use with no reaching
    // definition — `--verify-ssa` reports zero over the examples, and this row
    // asserts the ANSWER, which is what a misplaced definition would change.
    both("
fn main() {
    let mut i: int = 0;
    let mut acc: int = 0;
    while i < 10 {
        let mut j: int = 0;
        while j < 10 { acc = acc + 24690; j = j + 1; }
        i = i + 1;
    }
    io::print(\"acc=\"); io::println_int(acc);
}
", "acc=2469000", "nested loops");
}
