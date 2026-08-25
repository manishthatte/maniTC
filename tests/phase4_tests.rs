//! Phase 4 — F-1: SSA form, measured rather than assumed.
//!
//! Author: Manish Jagdish Thatte
//!
//! The unit tests in `src/ir/ssa.rs` and `src/ir/mem2reg.rs` check the
//! algorithms on hand-built functions. These check the claims that are only
//! true of the REAL compiler on REAL programs, and they are the ones that
//! would go quietly stale:
//!
//!   * the IR the lowerer produces is already in SSA form — F-1's premise, as
//!     stated in the recommendations, is wrong, and this is what says so;
//!   * `--mem2reg` does not change what an LLVM-compiled program prints;
//!   * the two T3 phi defects it exposed (report.txt P11, P12) stay fixed.

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static N: AtomicUsize = AtomicUsize::new(0);

fn manitc() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_manitc"))
}

fn example(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples").join(format!("{}.mt", name))
}

fn write(stem: &str, src: &str) -> PathBuf {
    // Unique per call, nested under one directory per process — see the note
    // in `expected_output_tests::temp_output` (report.txt P28).
    let slot = N.fetch_add(1, Ordering::Relaxed);
    let d = std::env::temp_dir()
        .join(format!("manitc_p4_{}", std::process::id()))
        .join(slot.to_string());
    std::fs::create_dir_all(&d).expect("temp dir");
    let p = d.join(format!("{}.mt", stem));
    std::fs::write(&p, src).expect("write");
    p
}

/// Compile with `--verify-ssa` and return the whole report.
fn ssa_report(src: &PathBuf, extra: &[&str]) -> String {
    let out = src.with_extension("out");
    let mut args: Vec<String> = vec![
        "compile".into(),
        src.to_string_lossy().into_owned(),
        "--target".into(),
        "t3".into(),
        "--verify-ssa".into(),
        "-o".into(),
        out.to_string_lossy().into_owned(),
    ];
    args.extend(extra.iter().map(|s| s.to_string()));
    let o = Command::new(manitc()).args(&args).output().expect("compile");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    );
    assert!(o.status.success(), "compile failed:\n{}", text);
    text
}

/// The violation count a `--verify-ssa` report gives for one stage.
fn violations(report: &str, stage: &str) -> usize {
    let line = report
        .lines()
        .find(|l| l.starts_with(&format!("ssa {} —", stage)))
        .unwrap_or_else(|| panic!("no `{}` line in:\n{}", stage, report));
    line.rsplit_once(", ")
        .and_then(|(_, tail)| tail.split_whitespace().next())
        .and_then(|n| n.parse().ok())
        .unwrap_or_else(|| panic!("cannot read a count from: {}", line))
}

/// A named `key=value` from the statistics line of a stage.
fn stat(report: &str, stage: &str, key: &str) -> usize {
    let prefix = format!("ssa {}   ", stage);
    let line = report
        .lines()
        .find(|l| l.starts_with(&prefix))
        .unwrap_or_else(|| panic!("no stats line for `{}` in:\n{}", stage, report));
    for field in line.trim().split_whitespace() {
        if let Some(v) = field.strip_prefix(&format!("{}=", key)) {
            return v.parse().unwrap_or_else(|_| panic!("bad number in {}", field));
        }
    }
    panic!("no `{}` in: {}", key, line)
}

fn run_t3(src: &PathBuf, extra: &[&str]) -> (i32, String) {
    let base = src.with_extension("");
    let mut args: Vec<String> = vec![
        "compile".into(),
        src.to_string_lossy().into_owned(),
        "--target".into(),
        "t3".into(),
        "-o".into(),
        base.to_string_lossy().into_owned(),
    ];
    args.extend(extra.iter().map(|s| s.to_string()));
    let c = Command::new(manitc()).args(&args).output().expect("compile");
    assert!(
        c.status.success(),
        "T3 compile failed:\n{}",
        String::from_utf8_lossy(&c.stderr)
    );
    let r = Command::new(manitc())
        .args(["run-t3", base.with_extension("t3b").to_str().unwrap()])
        .output()
        .expect("run");
    let text: String = String::from_utf8_lossy(&r.stdout)
        .lines()
        .filter(|l| !l.starts_with("[T3ISA]"))
        .map(|l| format!("{}\n", l))
        .collect();
    (r.status.code().unwrap_or(-1), text)
}

fn run_llvm(src: &PathBuf, extra: &[&str]) -> Option<(i32, String)> {
    let bin = src.with_extension("bin");
    let mut args: Vec<String> = vec![
        "compile".into(),
        src.to_string_lossy().into_owned(),
        "--target".into(),
        "llvm".into(),
        "-o".into(),
        bin.to_string_lossy().into_owned(),
    ];
    args.extend(extra.iter().map(|s| s.to_string()));
    let c = Command::new(manitc()).args(&args).output().expect("compile");
    if !c.status.success() {
        let blob = String::from_utf8_lossy(&c.stderr).to_string();
        if blob.contains("clang") {
            return None; // no toolchain in this environment
        }
        panic!("LLVM compile failed:\n{}", blob);
    }
    let r = Command::new(&bin).output().expect("run");
    Some((
        r.status.code().unwrap_or(-1),
        format!(
            "{}{}",
            String::from_utf8_lossy(&r.stdout),
            String::from_utf8_lossy(&r.stderr)
        ),
    ))
}

// ---------------------------------------------------------------------------
// F-1's premise, corrected
// ---------------------------------------------------------------------------

/// **The IR the lowerer produces is already in SSA form.**
///
/// The recommendations say it is not, and cite the optimiser's temp-name
/// `HashMap` as the evidence. Measured across the shipped examples the
/// violation count is zero — every temp is defined once, every use is
/// dominated by its definition, every phi is well formed. What the IR is not
/// is SSA over VARIABLES: they live in allocas, which is a different problem
/// with a different fix (`mem2reg`).
///
/// Pinned here because it is a statement about the compiler that could stop
/// being true without anyone noticing, and because every later item in the
/// performance tier is built on it.
#[test]
fn f1_the_lowered_ir_is_already_in_ssa_form() {
    for name in ["fibonacci", "ternary_sort", "oop", "three_valued_logic"] {
        let src = example(name);
        let report = ssa_report(&src, &[]);
        assert_eq!(
            violations(&report, "after lowering"),
            0,
            "{} is not SSA as lowered:\n{}",
            name,
            report
        );
        assert_eq!(
            violations(&report, "after optimisation"),
            0,
            "{} is not SSA after the optimiser:\n{}",
            name,
            report
        );
    }
}

/// …and the same holds with `--mem2reg`, which is the harder claim: the pass
/// inserts phi nodes, and a phi in the wrong place is exactly what a verifier
/// is for.
#[test]
fn f1_mem2reg_produces_ssa_too() {
    for name in ["fibonacci", "ternary_sort", "oop"] {
        let src = example(name);
        let report = ssa_report(&src, &["--mem2reg"]);
        assert_eq!(
            violations(&report, "after optimisation"),
            0,
            "{} is not SSA after mem2reg:\n{}",
            name,
            report
        );
    }
}

/// The measurement that sizes F-1: more than half of the IR is loads and
/// stores of locals, and `mem2reg` removes most of them.
///
/// The assertions are deliberately loose — this is a shape, not a golden
/// number, and tightening it would make every unrelated codegen change fail
/// here. What it catches is the pass silently doing nothing.
#[test]
fn f1_mem2reg_removes_most_of_the_memory_traffic() {
    let src = example("ternary_sort");

    let before = ssa_report(&src, &["--no-mem2reg"]);
    let allocas = stat(&before, "after lowering", "allocas");
    let promotable = stat(&before, "after lowering", "promotable");
    assert!(allocas > 100, "expected a lot of allocas, got {}", allocas);
    assert!(
        promotable * 10 >= allocas * 8,
        "at least 80% of allocas should be promotable: {} of {}",
        promotable,
        allocas
    );

    let after = ssa_report(&src, &[]);
    let mem_before = stat(&before, "after optimisation", "loads")
        + stat(&before, "after optimisation", "stores");
    let mem_after = stat(&after, "after optimisation", "loads")
        + stat(&after, "after optimisation", "stores");
    assert!(
        mem_after * 2 < mem_before,
        "mem2reg should remove most memory traffic: {} → {}",
        mem_before,
        mem_after
    );
}

// ---------------------------------------------------------------------------
// The two T3 phi defects mem2reg exposed
// ---------------------------------------------------------------------------

/// report.txt P11 — phi copies on one edge are a PARALLEL assignment.
///
/// `a, b = b, a + b` is the iterative Fibonacci loop, and after promotion it
/// is two phis at the loop head whose homes are each other's sources. The T3
/// backend copied them one after the other, so the second read a register the
/// first had already overwritten: `fib(10)` came out as a power of two.
///
/// The shape is written out in full rather than reduced, because what makes it
/// a swap is that BOTH variables are carried round the loop.
#[test]
fn p11_two_phis_that_swap_values_are_copied_in_parallel() {
    let src = write(
        "p11_swap",
        r#"
use std::io;
fn main() {
    let mut a: int = 0;
    let mut b: int = 1;
    let mut i: int = 0;
    while i < 10 {
        let t: int = a + b;
        a = b;
        b = t;
        i = i + 1;
    }
    io::println_int(a);
    io::println_int(b);
}
"#,
    );
    let expected = "55\n89\n";
    for flags in [vec![], vec!["--mem2reg"]] {
        let (code, out) = run_t3(&src, &flags);
        assert_eq!(code, 0, "T3 {:?}: {}", flags, out);
        assert_eq!(out, expected, "T3 with {:?}", flags);
        if let Some((code, out)) = run_llvm(&src, &flags) {
            assert_eq!(code, 0, "LLVM {:?}: {}", flags, out);
            assert_eq!(out, expected, "LLVM with {:?}", flags);
        }
    }
}

/// report.txt P12 — a phi's predecessor must end in a plain jump.
///
/// The T3 backend emits phi copies only in its `Jump` arm, so a phi reached
/// from a conditional branch got no value at all on that edge. Measured across
/// the 17 examples and thatteOS the lowerer never produces one — 616 phis,
/// zero such edges — so the defect is latent, not live. `mem2reg` produces
/// them by the hundred, and `split_critical_edges` is what keeps that from
/// mattering.
///
/// The report's `phi-edges-from-branch` count is the instrument. Zero with the
/// pass on is the claim.
#[test]
fn p12_no_phi_is_reached_directly_from_a_conditional_branch() {
    for name in ["fibonacci", "ternary_sort", "three_valued_logic"] {
        let src = example(name);

        let plain = ssa_report(&src, &[]);
        assert_eq!(
            stat(&plain, "after lowering", "phi-edges-from-branch"),
            0,
            "{}: the lowerer is not expected to produce these; if it starts to, \
             the T3 backend miscompiles them silently:\n{}",
            name,
            plain
        );

        let promoted = ssa_report(&src, &["--mem2reg"]);
        assert_eq!(
            stat(&promoted, "after optimisation", "phi-edges-from-branch"),
            0,
            "{}: critical-edge splitting must leave none:\n{}",
            name,
            promoted
        );
    }
}

/// report.txt P14 — a phi OPERAND is live to the end of its incoming block.
///
/// The T3 allocator recorded a phi operand's last use at the PHI's index. On a
/// back edge the phi sits earlier in linear order than the instruction that
/// defines the operand, so the operand looked dead the moment it was computed,
/// its register was handed to the next temp, and the phi copy at the jump read
/// whatever landed there.
///
/// `a = a + b; b = b + a` in a loop is the smallest shape that shows it: with
/// three loop-carried values it printed 1 and 95 instead of 144 and 233.
///
/// The `--mem2reg` arm is the one that used to fail; the plain arm is here so
/// the two are checked to agree rather than merely to be individually
/// plausible.
#[test]
fn p14_a_phi_operand_survives_to_the_end_of_its_incoming_block() {
    let src = write(
        "p14_carry",
        r#"
use std::io;
fn main() {
    let mut a: int = 0;
    let mut b: int = 1;
    let mut i: int = 0;
    while i < 6 {
        a = a + b;
        b = b + a;
        i = i + 1;
    }
    io::println_int(a);
    io::println_int(b);
}
"#,
    );
    let expected = "144\n233\n";
    for flags in [vec![], vec!["--mem2reg"]] {
        let (code, out) = run_t3(&src, &flags);
        assert_eq!(code, 0, "T3 {:?}: {}", flags, out);
        assert_eq!(out, expected, "T3 with {:?}", flags);
        if let Some((code, out)) = run_llvm(&src, &flags) {
            assert_eq!(code, 0, "LLVM {:?}: {}", flags, out);
            assert_eq!(out, expected, "LLVM with {:?}", flags);
        }
    }
}

/// The same defect at width: eight mutually-dependent loop-carried values.
///
/// Worth its own case because the first version of the fix could have been
/// "keep the last operand alive" rather than "keep every operand alive to its
/// own edge", and two variables would not have told them apart.
#[test]
fn p14_many_loop_carried_values_all_survive() {
    let src = write(
        "p14_wide",
        r#"
use std::io;
fn f(m: int) -> int {
    let mut a: int = 0; let mut b: int = 1; let mut c: int = 2; let mut d: int = 3;
    let mut e: int = 4; let mut g: int = 5; let mut h: int = 6; let mut k: int = 7;
    let mut i: int = 0;
    while i < m {
        a = a + b; b = b + c; c = c + d; d = d + e;
        e = e + g; g = g + h; h = h + k; k = k + a;
        i = i + 1;
    }
    return a + b + c + d + e + g + h + k;
}
fn main() { io::println_int(f(6)); }
"#,
    );
    let expected = "2224\n";
    for flags in [vec![], vec!["--mem2reg"]] {
        let (code, out) = run_t3(&src, &flags);
        assert_eq!(code, 0, "T3 {:?}: {}", flags, out);
        assert_eq!(out, expected, "T3 with {:?}", flags);
    }
}

/// report.txt P13 — a call's IR type and its emitted type can differ.
///
/// `TernaryTrie::get` has `ret_ty: Trit` in the IR and is emitted as
/// `call i64 @TernaryTrie_get`, with the narrowing done at each use. A phi is
/// the one construct whose type comes from the IR, so promoting a variable fed
/// by such a call produced `phi i8 [ %t94, … ]` against an `i64` definition,
/// which clang rejects outright. `mem2reg` declines to promote those until the
/// backend is fixed.
#[test]
fn p13_a_variable_fed_by_a_narrow_native_call_still_compiles() {
    let src = write(
        "p13_trie",
        r#"
use std::io;
use std::collections;
fn main() {
    let grid: TernaryTrie<trit> = TernaryTrie::new();
    let rows: [trit] = [-, 0, +];
    let cols: [trit] = [-, 0, +];
    for r in rows {
        for c in cols {
            let key: Vec<trit> = Vec::new();
            key.push(r);
            key.push(c);
            let v: trit = grid.get(key);
            io::print_trit(v);
        }
    }
    io::println("");
}
"#,
    );
    let (code, plain) = run_t3(&src, &[]);
    assert_eq!(code, 0, "{}", plain);
    let (code, promoted) = run_t3(&src, &["--mem2reg"]);
    assert_eq!(code, 0, "{}", promoted);
    assert_eq!(plain, promoted, "promotion must not change what it prints");

    if let Some((code, l_plain)) = run_llvm(&src, &[]) {
        assert_eq!(code, 0, "{}", l_plain);
        let (code, l_promoted) = run_llvm(&src, &["--mem2reg"]).expect("clang was available above");
        assert_eq!(code, 0, "{}", l_promoted);
        assert_eq!(l_plain, l_promoted);
        assert_eq!(plain, l_plain, "the two backends must agree");
    }
}

// ---------------------------------------------------------------------------
// The pass is ON by default, and there is a way out
// ---------------------------------------------------------------------------

/// Promotion is the default since F-3, and `--no-mem2reg` is the way out.
///
/// Not a style point in either direction. `--no-mem2reg` is what reproduces the
/// pre-F-1 compiler's output byte for byte — measured, 17/17 on the LLVM IR —
/// and that is how a defect gets dated as pre-existing rather than newly
/// introduced. If that switch silently stops opting out, the dating tool is
/// gone and nothing else in the suite would notice.
#[test]
fn f1_mem2reg_is_on_by_default_and_no_mem2reg_opts_out() {
    let src = example("fibonacci");
    let default = ssa_report(&src, &[]);
    let opted_out = ssa_report(&src, &["--no-mem2reg"]);
    assert!(
        stat(&opted_out, "after optimisation", "allocas")
            > stat(&default, "after optimisation", "allocas"),
        "the default must promote and --no-mem2reg must not:\ndefault:\n{}\nopted out:\n{}",
        default,
        opted_out
    );
    assert_eq!(
        stat(&opted_out, "after lowering", "allocas"),
        stat(&opted_out, "after optimisation", "allocas"),
        "with --no-mem2reg, no alloca is promoted:\n{}",
        opted_out
    );

    // `--mem2reg` still parses and still means "promote", so every script and
    // invocation written while it was opt-in keeps working.
    let asked = ssa_report(&src, &["--mem2reg"]);
    assert_eq!(
        stat(&asked, "after optimisation", "allocas"),
        stat(&default, "after optimisation", "allocas"),
        "--mem2reg must be a no-op now that promotion is the default"
    );
}

// ---------------------------------------------------------------------------
// The two float defects the 1,147-file corpus sweep found
// ---------------------------------------------------------------------------

/// report.txt P19 — T3 had no float remainder at all.
///
/// `x % y` on two floats was missing from the emitter's float list, so it fell
/// through to the INTEGER path and emitted `TMOD` against two IEEE-754 bit
/// patterns: `7.5 % 2.0` was 0 on T3 and 1.5 on LLVM.
///
/// The operands go through a function so the constant folder cannot compute the
/// answer at compile time. With both operands literal the folder gets it right
/// for reasons unrelated to the defect, which is exactly how this first
/// appeared as a promoted-vs-unpromoted difference and misdirected the triage.
#[test]
fn p19_float_remainder_agrees_across_backends() {
    let src = write(
        "p19_frem",
        r#"
use std::io;
use std::fmt;
fn frem(x: float, y: float) -> float { return x % y; }
fn main() -> int {
    io::println(fmt::show_int((frem(7.5, 2.0) * 1000.0) as int));
    io::println(fmt::show_int((frem(-7.5, 2.0) * 1000.0) as int));
    io::println(fmt::show_int((frem(10.0, 3.0) * 1000.0) as int));
    return 0;
}
"#,
    );
    let expected = "1500\n-1500\n1000\n";
    let (code, out) = run_t3(&src, &[]);
    assert_eq!(code, 0, "T3: {}", out);
    assert_eq!(out, expected, "T3 float remainder");
    if let Some((code, out)) = run_llvm(&src, &[]) {
        assert_eq!(code, 0, "LLVM: {}", out);
        assert_eq!(out, expected, "LLVM float remainder");
    }
}

/// report.txt P20 — a NaN compares false to everything, itself included, and
/// `!=` is the single exception that is TRUE.
///
/// Both backends had this wrong, in opposite directions and for unrelated
/// reasons. T3's three-way `fcmp` cannot express UNORDERED, so it collapsed to
/// 0 — "equal" — and every comparison against a NaN said true. LLVM used
/// `fcmp one`, ORDERED not-equal, so `nan != x` said false.
///
/// The direction matters: a guard written as `if x != x { reject }` passed
/// exactly when it should have rejected.
#[test]
fn p20_nan_comparisons_follow_ieee_on_both_backends() {
    let src = write(
        "p20_nan",
        r#"
use std::io;
fn nan_of(a: float, b: float) -> float { return a / b; }
fn show(tag: str, b: bool) { io::print(tag); io::println_bool(b); }
fn main() -> int {
    let n: float = nan_of(0.0, 0.0);
    show("eq_self ", n == n);
    show("eq_one  ", n == 1.0);
    show("ne_one  ", n != 1.0);
    show("lt_one  ", n <  1.0);
    show("gt_one  ", n >  1.0);
    show("le_one  ", n <= 1.0);
    show("ge_one  ", n >= 1.0);
    return 0;
}
"#,
    );
    // Only `!=` is true. Everything else is false, `n == n` included.
    let expected = "eq_self false\neq_one  false\nne_one  true\nlt_one  false\ngt_one  false\nle_one  false\nge_one  false\n";
    let (code, out) = run_t3(&src, &[]);
    assert_eq!(code, 0, "T3: {}", out);
    assert_eq!(out, expected, "T3 NaN comparisons");
    if let Some((code, out)) = run_llvm(&src, &[]) {
        assert_eq!(code, 0, "LLVM: {}", out);
        assert_eq!(out, expected, "LLVM NaN comparisons");
    }
}

// ---------------------------------------------------------------------------
// The LLVM crashes the corpus sweep found (report.txt P23, P25)
// ---------------------------------------------------------------------------

/// report.txt P23 — `float as int` SATURATES, and was undefined behaviour on
/// LLVM.
///
/// Plain `fptosi` is UB when the value does not fit; on x86 it yields
/// `i64::MIN` for NaN and both infinities alike. T3's `ftoi` is Rust's `as`,
/// which saturates. Three of the corpus's five LLVM hangs were this one cast:
/// `exp(nan)` computed `n = (q - 0.5) as int`, got `i64::MIN`, and the scaling
/// loop after it — `while e < 0 { e = e + 1; }` — had 9.2e18 iterations to run.
#[test]
fn p23_float_to_int_saturates_on_both_backends() {
    let src = write(
        "p23_sat",
        r#"
use std::io;
use std::fmt;
fn conv(x: float) -> int { return x as int; }
fn main() {
    let z: float = 0.0;
    io::println(fmt::show_int(conv(z / z)));
    io::println(fmt::show_int(conv(1.0 / z)));
    io::println(fmt::show_int(conv((0.0 - 1.0) / z)));
    io::println(fmt::show_int(conv(1e30)));
}
"#,
    );
    // NaN to zero; the infinities and the overflow to the nearest bound.
    let expected = "0\n9223372036854775807\n-9223372036854775808\n9223372036854775807\n";
    let (code, out) = run_t3(&src, &[]);
    assert_eq!(code, 0, "T3: {}", out);
    assert_eq!(out, expected, "T3 float->int saturation");
    if let Some((code, out)) = run_llvm(&src, &[]) {
        assert_eq!(code, 0, "LLVM: {}", out);
        assert_eq!(out, expected, "LLVM float->int saturation");
    }
}

/// report.txt P25 — an out-of-bounds array WRITE is bounds-checked, not just a
/// read.
///
/// A2 guarded loads and not stores, which is the wrong way round: a bad read
/// returns a wrong value, a bad WRITE destroys something else's. On T3 it
/// silently corrupted emulator memory and the program carried on printing; on
/// LLVM it corrupted the heap and glibc aborted the process with
/// `malloc.c:2601 (sysmalloc): assertion failed`.
#[test]
fn p25_an_out_of_bounds_write_is_caught() {
    let src = write(
        "p25_oob_write",
        r#"
use std::io;
fn main() {
    let mut a: [int; 4] = [0, 0, 0, 0];
    let mut i: int = 0;
    while i < 20 { a[i] = i; i = i + 1; }
    io::println("survived write");
}
"#,
    );
    let (code, out) = run_t3(&src, &[]);
    assert_ne!(code, 0, "T3 must trap, got: {}", out);
    assert!(out.contains("out of bounds"), "T3 message: {}", out);
    assert!(!out.contains("survived"), "T3 ran past the write: {}", out);
    if let Some((code, out)) = run_llvm(&src, &[]) {
        assert_ne!(code, 0, "LLVM must trap, got: {}", out);
        assert!(out.contains("out of bounds"), "LLVM message: {}", out);
        assert!(!out.contains("survived"), "LLVM ran past the write: {}", out);
    }
}

/// report.txt P22/F-2 — multiply and divide by a power of three become the
/// single ternary-shift instruction T3ISA has for them.
///
/// The two EXCLUSIONS are the correctness argument, and both are checked here:
///
///   * `x * 6` is not a power of three and must stay a multiply;
///   * `x / 3` under v1 is `Div`, which TRUNCATES, and `TSHR` ROUNDS. They
///     differ for every negative operand that does not divide exactly, which
///     is what `d3(-5)` pins: -1 truncating, -2 rounding.
#[test]
fn f2_ternary_strength_reduction_agrees_across_backends() {
    let src = write(
        "f2_tsr",
        r#"
use std::io;
use std::fmt;
fn m3(x: int) -> int { return x * 3; }
fn m9(x: int) -> int { return x * 9; }
fn m27(x: int) -> int { return x * 27; }
fn m6(x: int) -> int { return x * 6; }
fn d3(x: int) -> int { return x / 3; }
fn main() {
    io::println(fmt::show_int(m3(7)));
    io::println(fmt::show_int(m9(-7)));
    io::println(fmt::show_int(m27(2)));
    io::println(fmt::show_int(m6(7)));
    io::println(fmt::show_int(d3(-5)));
}
"#,
    );
    // d3(-5) is -1: v1 division truncates. A TSHR here would give -2.
    let expected = "21\n-63\n54\n42\n-1\n";
    let (code, out) = run_t3(&src, &[]);
    assert_eq!(code, 0, "T3: {}", out);
    assert_eq!(out, expected, "T3 ternary strength reduction");
    if let Some((code, out)) = run_llvm(&src, &[]) {
        assert_eq!(code, 0, "LLVM: {}", out);
        assert_eq!(out, expected, "LLVM must agree");
    }

    // …and under v2, where `/` is DivNear, the answers still agree — TSHR is
    // reachable there and is round-to-nearest, so d3(-5) becomes -2 on BOTH.
    let (c2, o2) = run_t3(&src, &["--lang", "v2"]);
    assert_eq!(c2, 0, "T3 v2: {}", o2);
    if let Some((c3, o3)) = run_llvm(&src, &["--lang", "v2"]) {
        assert_eq!(c3, 0, "LLVM v2: {}", o3);
        assert_eq!(o2, o3, "the backends must agree under v2 too");
    }
}

/// report.txt P22/F-2 — the CHECKED ternary shift keeps N5's overflow guard.
///
/// This is the whole reason `TShlT27` exists as a separate operation. On T3
/// there is no difference: `TSHI` traps via `checked27` whether or not the IR
/// asked. On LLVM `TShl` is a wrapping `mul i64` and `TShlT27` carries the
/// guard, so reducing `MulT27` to the UNCHECKED shift would have silently
/// removed the check that `--lang v2` exists to provide — and nothing else in
/// the suite would have noticed, because the answers agree right up until the
/// moment one of them is supposed to trap.
#[test]
fn f2_the_checked_ternary_shift_still_traps_on_overflow() {
    let src = write(
        "f2_tsr_overflow",
        r#"
use std::io;
use std::fmt;
fn m3(x: int) -> int { return x * 3; }
fn main() {
    io::println("before");
    io::println(fmt::show_int(m3(1270932914165)));
    io::println("after");
}
"#,
    );
    // 1270932914165 * 3 is 3812798742495, one past the 27-trit maximum.

    // Under v2 BOTH backends must trap: T3 through `checked27`, LLVM through
    // N5's guard call.
    let (code, out) = run_t3(&src, &["--lang", "v2"]);
    assert_ne!(code, 0, "T3 v2 must trap: {}", out);
    assert!(out.contains("overflow"), "T3 v2 message: {}", out);
    assert!(!out.contains("after"), "T3 v2 ran past the overflow: {}", out);
    if let Some((code, out)) = run_llvm(&src, &["--lang", "v2"]) {
        assert_ne!(code, 0, "LLVM v2 must trap — the N5 guard was dropped: {}", out);
        assert!(out.contains("overflow"), "LLVM v2 message: {}", out);
        assert!(!out.contains("after"), "LLVM v2 ran past the overflow: {}", out);
    }

    // Under v1 the guard does not exist by design, and T3 traps anyway because
    // its word IS 27 trits. That asymmetry is report.txt P21 cluster 1 and is
    // pinned here so the reduction is not blamed for it later.
    let (code, out) = run_t3(&src, &[]);
    assert_ne!(code, 0, "T3 v1 traps on its own word width: {}", out);
}

// ---------------------------------------------------------------------------
// F-2 — the two passes that measured at zero: `common_subexpression_eliminate`
// and `ternary_peephole` (report.txt P22)
// ---------------------------------------------------------------------------

/// The whole IR of a program, as `--emit-ir` prints it after the optimiser.
fn emit_ir(src: &PathBuf, extra: &[&str]) -> String {
    let out = src.with_extension("ir_out");
    let mut args: Vec<String> = vec![
        "compile".into(),
        src.to_string_lossy().into_owned(),
        "--target".into(),
        "t3".into(),
        "--emit-ir".into(),
        "-o".into(),
        out.to_string_lossy().into_owned(),
    ];
    args.extend(extra.iter().map(|s| s.to_string()));
    let o = Command::new(manitc()).args(&args).output().expect("compile");
    assert!(
        o.status.success(),
        "compile failed:\n{}",
        String::from_utf8_lossy(&o.stderr)
    );
    String::from_utf8_lossy(&o.stdout).into_owned()
}

/// Just one function out of that dump. Counting over the whole module would
/// count the standard library too, and its contents are not this test's
/// business.
fn ir_function<'a>(ir: &'a str, name: &str) -> String {
    let head = format!("fn {} (", name);
    let start = ir.find(&head).unwrap_or_else(|| panic!("no `{}` in the IR", name));
    let rest = &ir[start + head.len()..];
    let end = rest.find("\nfn ").map(|i| start + head.len() + i).unwrap_or(ir.len());
    ir[start..end].to_string()
}

fn count(hay: &str, needle: &str) -> usize {
    hay.matches(needle).count()
}

/// **report.txt P22/F-2 — `x > 0 / x < 0 / else` becomes ONE three-way branch.**
///
/// T3ISA has `TCMP Rd, Ra, R0` — sign(Ra) in one instruction — and `TBRANCH`,
/// which branches three ways on it. Both were already in the IR as `TritSign`
/// (C7) and `TritBranch`; what did not exist was anything that produced the
/// pair from ordinary source, so the sign trichotomy — which is how a
/// balanced-ternary standard library is naturally written — lowered to two
/// two-way comparisons and two two-way branches. On T3 the backend then
/// CLAMPED each three-way compare back to a boolean with `TMAX`/`TMIN` and
/// branched three ways with two arms pointing at the same label: the answer
/// was computed, thrown away, and recomputed.
///
/// All four orderings the examples actually contain are here, because the arm
/// mapping is where this gets silently wrong: `0 < n` is `n` positive and
/// `0 > n` is `n` negative, and reading either backwards sends every value
/// down the wrong branch while still type-checking and still running.
#[test]
fn f2_the_sign_trichotomy_becomes_one_three_way_branch() {
    let src = write(
        "f2_trichotomy",
        r#"
use std::io;
fn a(n: int) -> int { if n > 0 { return 1; } else { if n < 0 { return -1; } else { return 0; } } }
fn b(n: int) -> int { if 0 < n { return 1; } else { if 0 > n { return -1; } else { return 0; } } }
fn c(n: int) -> int { if n == 0 { return 0; } else { if n < 0 { return -1; } else { return 1; } } }
fn d(n: int) -> int { if n < 0 { return -1; } else { if n == 0 { return 0; } else { return 1; } } }
fn main() {
    io::println_int(a(7)); io::println_int(a(-7)); io::println_int(a(0));
    io::println_int(b(7)); io::println_int(b(-7)); io::println_int(b(0));
    io::println_int(c(7)); io::println_int(c(-7)); io::println_int(c(0));
    io::println_int(d(7)); io::println_int(d(-7)); io::println_int(d(0));
}
"#,
    );

    // It fired: one sign and one three-way branch per function, and neither
    // comparison survives.
    let ir = emit_ir(&src, &[]);
    for f in ["a", "b", "c", "d"] {
        let body = ir_function(&ir, f);
        assert_eq!(count(&body, "TritSign"), 1, "`{}` should have one sign:\n{}", f, body);
        assert_eq!(count(&body, "TritBranch"), 1, "`{}` should branch once:\n{}", f, body);
        assert_eq!(count(&body, "BinBranch"), 0, "`{}` still branches two ways:\n{}", f, body);
    }

    // …and it is the RIGHT three-way branch.
    let expected = "1\n-1\n0\n1\n-1\n0\n1\n-1\n0\n1\n-1\n0\n";
    let (code, out) = run_t3(&src, &[]);
    assert_eq!(code, 0, "T3: {}", out);
    assert_eq!(out, expected, "T3 arm mapping");
    if let Some((code, out)) = run_llvm(&src, &[]) {
        assert_eq!(code, 0, "LLVM: {}", out);
        assert_eq!(out, expected, "LLVM must agree");
    }
}

/// **A float trichotomy is left alone, and the reason is P20.**
///
/// A NaN answers false to `<`, to `>` and to `==` alike. It belongs in a
/// fourth arm, and `TritBranch` has three — so collapsing a float comparison
/// would send every NaN down whichever arm the sign of its bit pattern picked,
/// which is the wrong answer P20 fixed from the other side. 49 of the 107
/// collapsible sites in the shipped examples are float and stay as they are.
///
/// The NaN case is the whole test: without it a wrongly-collapsed float
/// trichotomy still gives 1, -1 and 0 for ordinary inputs.
#[test]
fn f2_a_float_trichotomy_is_left_alone_because_nan_has_no_arm() {
    let src = write(
        "f2_float_trichotomy",
        r#"
use std::io;
fn f(x: float) -> int { if x > 0.0 { return 1; } else { if x < 0.0 { return -1; } else { return 0; } } }
fn nan_of(a: float, b: float) -> float { return a / b; }
fn main() {
    io::println_int(f(1.5));
    io::println_int(f(-1.5));
    io::println_int(f(0.0));
    io::println_int(f(nan_of(0.0, 0.0)));
}
"#,
    );

    let body = ir_function(&emit_ir(&src, &[]), "f");
    assert_eq!(count(&body, "TritSign"), 0, "a float sign has no third answer:\n{}", body);

    // The NaN takes the else arm on both backends, as IEEE-754 says it must.
    let expected = "1\n-1\n0\n0\n";
    let (code, out) = run_t3(&src, &[]);
    assert_eq!(code, 0, "T3: {}", out);
    assert_eq!(out, expected, "T3: a NaN is not positive and not negative");
    if let Some((code, out)) = run_llvm(&src, &[]) {
        assert_eq!(code, 0, "LLVM: {}", out);
        assert_eq!(out, expected, "LLVM must agree");
    }
}

/// **The absorbed block must hold nothing but the comparison that feeds it.**
///
/// The collapse makes the second comparison's block unreachable. Anything else
/// in that block would simply stop happening — and on the two paths that
/// reached it, silently.
// ---------------------------------------------------------------------------
// report.txt P38 — the image, the stack, and a pass with no bound
// ---------------------------------------------------------------------------

/// **A program that does not fit below the stack is REFUSED, not corrupted.**
///
/// The memory map puts code at 0 growing UP and the stack at `STACK_BASE`
/// growing DOWN, and nothing compared them. A program whose image reached
/// 60,000 words overwrote its own stack and eventually executed a return
/// address, reported as `TRAP: unknown opcode <n> at PC=<n>` — the symptom, not
/// the cause. Measured by lengthening one `main` under `--no-inline`: 59,991
/// words ran, 60,004 trapped, nothing in between.
///
/// The test builds an over-large program the cheapest way there is — one
/// enormous function of straight-line arithmetic — and asserts the COMPILER
/// says so. Asserting on the message rather than on a trap is the point: the
/// old behaviour also "failed", just silently and much later.
#[test]
fn f2_an_image_that_does_not_fit_below_the_stack_is_refused() {
    // Straight-line work that constant folding cannot collapse: the seed comes
    // from a recursive function, so every `a = a + k` survives as an
    // instruction. Folding it away is exactly what a first attempt at this
    // test did, and it then compiled cleanly and asserted nothing.
    let mut body = String::from(
        "use std::io;\nfn seed(n: int) -> int { if n <= 0 { return 1; } return 1 + seed(n - 1); }\nfn main() {\n    let mut a: int = seed(3);\n",
    );
    // 34,000 of these is ~63,000 T3 words, comfortably over the 60,000-word
    // stack base and still about a second to compile. 24,000 was the first
    // attempt and gave 44,600 words, which fits — the test then asserted
    // nothing at all.
    for i in 0..34000 {
        body.push_str(&format!("    a = a + {};\n", i % 7));
    }
    body.push_str("    io::println_int(a);\n}\n");
    let src = write("f2_image_too_big", &body);

    let out = src.with_extension("out");
    let o = Command::new(manitc())
        .args([
            "compile",
            src.to_str().unwrap(),
            "--target",
            "t3",
            "-o",
            out.to_str().unwrap(),
        ])
        .output()
        .expect("compile");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    );
    assert!(!o.status.success(), "an over-large image compiled:\n{}", text);
    assert!(
        text.contains("does not fit below the stack"),
        "the failure did not name the cause:\n{}",
        text
    );
}

/// **report.txt P38 — the pass has a growth budget, because a size limit on one
/// splice is not a bound on the pass.**
///
/// A twelve-instruction body spliced at 597 sites is 597 legal splices and a
/// module three and a half times its original size. That is measured, not
/// hypothetical: on one corpus file the IR went 4,084 → 12,978 instructions and
/// the emitted image 26,245 → 94,473 words, which no longer fits below the
/// stack — fourteen corpus programs stopped working.
///
/// The budget is `max(64, 20 % of the module's pre-inline size)`. The
/// assertion here is a RATIO rather than a site count, because the site count
/// depends on which callees happen to qualify and the ratio is the property
/// the budget exists to hold.
#[test]
fn f2_the_pass_has_a_growth_budget() {
    // One small multi-block callee, called from very many places: the shape
    // that has no bound without one.
    let mut src = String::from(
        "use std::io;\nfn pick(x: int) -> int { if x > 0 { return x + 1; } else { return 0 - x; } }\nfn main() {\n    let mut t: int = 0;\n",
    );
    for i in 0..400 {
        // Kept small deliberately: `int` is 27 trits and an unbounded
        // accumulator traps on overflow long before the budget is the thing
        // under test.
        src.push_str(&format!("    t = (t + pick(t + {})) % 1000;\n", i % 13));
    }
    src.push_str("    io::println_int(t);\n}\n");
    let src = write("f2_growth_budget", &src);

    let before = count_instrs(&emit_ir(&src, &["--no-inline"]));
    let after = count_instrs(&emit_ir(&src, &[]));
    assert!(before > 0, "no IR at all");
    assert!(
        after <= before + std::cmp::max(64, before / 5),
        "the pass grew the module from {} to {} instructions, past its budget",
        before,
        after
    );

    // And the answer is unchanged, so the budget did not break the splice.
    let (code, out) = run_t3(&src, &[]);
    assert_eq!(code, 0, "T3: {}", out);
    assert_eq!(out, run_t3(&src, &["--no-inline"]).1, "the budget moved the answer");
}

/// Instructions in a whole IR dump: every line that is an instruction rather
/// than a header, a label or a terminator.
fn count_instrs(ir: &str) -> usize {
    ir.lines()
        .filter(|l| l.starts_with("    ") && !l.trim_start().starts_with("->"))
        .count()
}

/// **report.txt P37 — the collapse does not absorb a comparison whose RESULT
/// is used somewhere else.**
///
/// The pass requires the second block to hold NOTHING but the comparison. That
/// says the block has no other work in it; it does not say the block's VALUE
/// has no other reader. Absorbing the block makes it unreachable and its
/// comparison goes with it, so any later block still testing that temp is left
/// reading a name nothing defines.
///
/// **This shipped, and it is the shape ordinary code has**: a sign tested once
/// to normalise a value and again at the end to put the sign back.
/// `to_balanced_ternary` is exactly that, copied into five thatteOS files —
/// and collapsed it returned the POSITIVE representation for every NEGATIVE
/// input on T3, silently, because a free temp gets a register. LLVM would not
/// link the module at all (`use of undefined value '%t7'`), which is the only
/// reason it was not silent everywhere.
///
/// Three assertions, because each catches a different half: the SSA verifier
/// (which names it directly), the T3 answer (which is what a user sees), and
/// the LLVM answer (which is what refused to build).
#[test]
fn f2_a_reused_comparison_is_not_absorbed_by_the_collapse() {
    let src = write(
        "f2_trichotomy_reused_cond",
        r#"
use std::io;
fn to_bt(n: int) -> str {
    if n == 0 { return "0"; }
    let mut val = n;
    let neg = val < 0;
    if neg { val = 0 - val; }
    let mut result = "";
    while val > 0 {
        let rem = val - (val / 3) * 3;
        if rem == 0 { result = str::concat("0", result); val = val / 3; }
        elif rem == 1 { result = str::concat("+", result); val = val / 3; }
        else { result = str::concat("-", result); val = val / 3 + 1; }
    }
    if neg {
        let s1 = str::replace(result, "+", "T");
        let s2 = str::replace(s1, "-", "+");
        return str::replace(s2, "T", "-");
    }
    return result;
}
fn main() {
    io::println(to_bt(5));
    io::println(to_bt(0 - 5));
    io::println(to_bt(4));
    io::println(to_bt(0 - 4));
}
"#,
    );

    let report = ssa_report(&src, &[]);
    assert_eq!(
        violations(&report, "after optimisation"),
        0,
        "the collapse deleted a comparison something else still reads:\n{}",
        report
    );

    let expected = "+--\n-++\n++\n--\n";
    let (code, out) = run_t3(&src, &[]);
    assert_eq!(code, 0, "T3: {}", out);
    assert_eq!(out, expected, "T3 — every negative input took the positive path");
    let llvm = run_llvm(&src, &[]).expect("LLVM backend should be available for this test");
    assert_eq!(llvm.0, 0, "LLVM refused the module:\n{}", llvm.1);
    assert_eq!(llvm.1, expected, "LLVM");
}

#[test]
fn f2_a_side_effect_between_the_two_comparisons_is_not_absorbed() {
    let src = write(
        "f2_trichotomy_side_effect",
        r#"
use std::io;
fn g(n: int) -> int {
    if n > 0 { return 1; }
    else {
        io::println("mid");
        if n < 0 { return -1; } else { return 0; }
    }
}
fn main() { io::println_int(g(5)); io::println_int(g(-5)); io::println_int(g(0)); }
"#,
    );

    let body = ir_function(&emit_ir(&src, &[]), "g");
    assert_eq!(count(&body, "TritSign"), 0, "the print would have been skipped:\n{}", body);

    let expected = "1\nmid\n-1\nmid\n0\n";
    let (code, out) = run_t3(&src, &[]);
    assert_eq!(code, 0, "T3: {}", out);
    assert_eq!(out, expected, "T3 dropped the print");
    if let Some((code, out)) = run_llvm(&src, &[]) {
        assert_eq!(code, 0, "LLVM: {}", out);
        assert_eq!(out, expected, "LLVM dropped the print");
    }
}

/// **report.txt P22/F-2 — CSE reaches across blocks, and stops where dominance
/// stops.**
///
/// The pass used to look inside one basic block only, and fired three times in
/// total across the 17 examples. It was not broken and it was not
/// inapplicable: this IR's mean basic block is 2.78 instructions and 65.8 % of
/// blocks hold none or one, so a block-local pass has nowhere to look.
///
/// `f` is the reach: `n * 31` in the entry block, recomputed in both arms of
/// an `if`, and both arms are dominated by the entry. `g` is the LIMIT, and it
/// is the half that fails silently — `n * 31` in the then-arm does NOT
/// dominate the code after the merge, so reusing it there would name a temp
/// that the else path never computed. Both must hold, and only the value check
/// catches the second going wrong on a machine with registers.
#[test]
fn f2_cse_reaches_across_blocks_but_not_across_a_sibling() {
    let src = write(
        "f2_cse_dominance",
        r#"
use std::io;
fn f(c: bool, n: int) -> int {
    let a: int = n * 31;
    let mut r: int = 0;
    if c { r = n * 31 + 1; } else { r = n * 31 + 2; }
    return a + r;
}
fn g(c: bool, n: int) -> int {
    let mut r: int = 0;
    if c { r = n * 31; } else { r = n * 17; }
    return r + n * 31;
}
fn main() {
    io::println_int(f(true, 10));
    io::println_int(f(false, 10));
    io::println_int(g(true, 10));
    io::println_int(g(false, 10));
}
"#,
    );

    let ir = emit_ir(&src, &[]);
    let (fb, gb) = (ir_function(&ir, "f"), ir_function(&ir, "g"));
    assert_eq!(
        count(&fb, "op: Mul"),
        1,
        "three multiplies of the same operands, one dominating the others:\n{}",
        fb
    );
    assert_eq!(
        count(&gb, "op: Mul"),
        3,
        "the then-arm's multiply reaches neither the else arm nor the merge:\n{}",
        gb
    );

    let expected = "621\n622\n620\n480\n";
    let (code, out) = run_t3(&src, &[]);
    assert_eq!(code, 0, "T3: {}", out);
    assert_eq!(out, expected, "T3 reused a value that does not reach");
    if let Some((code, out)) = run_llvm(&src, &[]) {
        assert_eq!(code, 0, "LLVM: {}", out);
        assert_eq!(out, expected, "LLVM must agree");
    }
}

/// **A `Load` is not a subexpression, because a `Store` can happen between
/// two of them.**
///
/// `Load` is deliberately absent from `cse_key`: the same address read twice
/// is the same value only if nothing wrote to it in between, and there is no
/// alias analysis here to say there was not. Half of the IR's remaining
/// memory traffic after promotion is heap, so this is not hypothetical.
#[test]
fn f2_cse_does_not_reuse_a_load_across_a_store() {
    let src = write(
        "f2_cse_load_store",
        r#"
use std::io;
fn zero() -> int { return 0; }
fn main() {
    let mut a: [int] = [1, 2, 3];
    let i: int = zero();
    let x: int = a[i];
    a[i] = 99;
    let y: int = a[i];
    io::println_int(x + y);
}
"#,
    );
    // 1 + 99. Reusing the first load would give 2.
    let expected = "100\n";
    let (code, out) = run_t3(&src, &[]);
    assert_eq!(code, 0, "T3: {}", out);
    assert_eq!(out, expected, "T3 reused a load across a store");
    if let Some((code, out)) = run_llvm(&src, &[]) {
        assert_eq!(code, 0, "LLVM: {}", out);
        assert_eq!(out, expected, "LLVM reused a load across a store");
    }
}

/// **A repeated bounds check goes; an out-of-range one still traps.**
///
/// A2's `BoundsCheck` defines no temp and it traps, so a repeat is REMOVED
/// rather than rewritten — and removing it is safe for the same reason it is
/// redundant: same index, same length, and the check that dominates it has
/// already passed. What must not happen is the check disappearing altogether,
/// which is P25 (an out-of-bounds WRITE was not checked at all) waiting to
/// come back, and which no count of instructions would notice.
#[test]
fn f2_a_repeated_bounds_check_goes_but_the_trap_stays() {
    let ok = write(
        "f2_bounds_ok",
        r#"
use std::io;
fn idx() -> int { return 1; }
fn main() {
    let a: [int] = [10, 20, 30];
    let i: int = idx();
    io::println_int(a[i] + a[i]);
}
"#,
    );
    let body = ir_function(&emit_ir(&ok, &[]), "main");
    assert_eq!(
        count(&body, "BoundsCheck"),
        1,
        "the same index against the same length, checked twice:\n{}",
        body
    );
    let (code, out) = run_t3(&ok, &[]);
    assert_eq!(code, 0, "T3: {}", out);
    assert_eq!(out, "40\n", "T3");
    if let Some((code, out)) = run_llvm(&ok, &[]) {
        assert_eq!(code, 0, "LLVM: {}", out);
        assert_eq!(out, "40\n", "LLVM");
    }

    let bad = write(
        "f2_bounds_trap",
        r#"
use std::io;
fn idx() -> int { return 5; }
fn main() {
    let a: [int] = [10, 20, 30];
    let i: int = idx();
    io::println_int(a[i] + a[i]);
    io::println("after");
}
"#,
    );
    let (_, out) = run_t3(&bad, &[]);
    assert!(out.contains("out of bounds"), "T3 lost the check: {}", out);
    assert!(!out.contains("after"), "T3 ran past the trap: {}", out);
    if let Some((code, out)) = run_llvm(&bad, &[]) {
        assert_ne!(code, 0, "LLVM lost the check: {}", out);
        assert!(out.contains("out of bounds"), "LLVM message: {}", out);
        assert!(!out.contains("after"), "LLVM ran past the trap: {}", out);
    }
}

// ---------------------------------------------------------------------------
// F-2 — the inliner (report.txt P29)
//
// The pass splices a call to a small SINGLE-BLOCK function into its caller. It
// is the half of inlining that needs no control-flow surgery at all: the body
// goes where the `Call` stood and the caller's block is not even divided.
//
// These tests exist because the pass shipped its first version substituting
// NOTHING and looking entirely correct while doing it (P29) — the rename map
// was keyed on the bare parameter name and the body spells its parameters
// `param_<name>`, so every argument binding silently missed and every spliced
// body arrived with its parameters still free. It is not a subtle failure once
// seen: 13 of the 17 examples would not compile on LLVM. It is that nothing in
// the pass's own shape objects to it, which is why the first assertion below
// is about a NAME rather than about a number.
// ---------------------------------------------------------------------------

/// A caller that survives constant folding: `opaque(k)` is `k`, but no pass
/// can see that, so its result is not a constant the later passes can fold
/// through. Without it, a fully-folded `main` proves nothing about the splice
/// — the whole body would be gone either way.
///
/// **It is SELF-RECURSIVE, and that is the load-bearing part.** It used to be
/// multi-block (`if n > 0 { return n; } else { return 0; }`) and relied on the
/// inliner refusing multi-block callees. When the CFG path landed, that
/// refusal went — and every test below kept passing while proving nothing,
/// because `opaque(7)` folded to `7`, `scale(7, 3)` folded to `22`, and the
/// assertion "no `Call` to `scale` survives" is satisfied just as well by a
/// body that was never spliced as by one that was. Self-recursion is the one
/// refusal the pass will not drop: `collect` declines a body that calls itself
/// on principle, not for want of an implementation.
const OPAQUE: &str =
    "fn opaque(n: int) -> int { if n <= 0 { return 0; } return 1 + opaque(n - 1); }";

/// **report.txt P29 — a spliced body binds its parameters to the arguments.**
///
/// `main` takes no parameters, so after the splice ANY `param_` in its IR is a
/// temp that only the callee ever defined — a free variable the caller now
/// refers to. That is the assertion, and it is sharper than checking the
/// answer: on T3 the register allocator gives an unknown temp a slot and the
/// program computes something plausible from uninitialised memory, so the
/// arithmetic can come out right by accident. On LLVM the same IR is simply
/// rejected (`use of undefined value '%param_pad'`), which is how it was found.
#[test]
fn f2_a_spliced_body_binds_its_parameters() {
    let src = write(
        "f2_inline_params",
        &format!(
            r#"
use std::io;
fn scale(x: int, k: int) -> int {{ return x * k + 1; }}
{}
fn main() {{
    let a: int = opaque(7);
    io::println_int(scale(a, 3));
}}
"#,
            OPAQUE
        ),
    );

    let body = ir_function(&emit_ir(&src, &[]), "main");
    assert_eq!(count(&body, "func: \"scale\""), 0, "not inlined at all:\n{}", body);
    assert!(
        !body.contains("param_"),
        "a callee parameter leaked into a caller that has none:\n{}",
        body
    );
    // **The guard on the shared `OPAQUE` helper**, and it belongs here rather
    // than in a test of its own because every test that uses the constant
    // depends on it. If `opaque` is ever spliced, `a` becomes the constant 7,
    // `scale(a, 3)` folds to 22, and the assertion above is satisfied by a
    // `main` in which nothing was ever inlined — five tests going quietly
    // hollow. That is not hypothetical: it is what the multi-block CFG path
    // did to the previous, two-armed helper.
    assert_eq!(
        count(&body, "func: \"opaque\""), 1,
        "OPAQUE was spliced, so this test and four others now prove nothing:\n{}",
        body
    );

    // And the value is right, on both backends and with the pass off.
    let (code, out) = run_t3(&src, &[]);
    assert_eq!(code, 0, "T3: {}", out);
    assert_eq!(out, "22\n", "T3");
    assert_eq!(run_t3(&src, &["--no-inline"]).1, "22\n", "T3 --no-inline");
    if let Some((code, out)) = run_llvm(&src, &[]) {
        assert_eq!(code, 0, "LLVM: {}", out);
        assert_eq!(out, "22\n", "LLVM");
    }
}

/// **One callee spliced twice into one caller keeps the two copies apart.**
///
/// Every temp a body defines is renamed with a per-site prefix, which is the
/// only thing standing between two copies of the same instructions and a
/// single-assignment violation. Two sites with DIFFERENT arguments is what
/// makes a collision visible: if the second copy's temps landed on the first
/// copy's names, one of the two answers would be the other.
#[test]
fn f2_one_callee_spliced_twice_keeps_the_copies_apart() {
    let src = write(
        "f2_inline_twice",
        &format!(
            r#"
use std::io;
fn scale(x: int, k: int) -> int {{ return x * k + 1; }}
{}
fn main() {{
    let a: int = opaque(7);
    let b: int = opaque(2);
    io::println_int(scale(a, 3));
    io::println_int(scale(b, 5));
}}
"#,
            OPAQUE
        ),
    );

    let body = ir_function(&emit_ir(&src, &[]), "main");
    assert_eq!(count(&body, "func: \"scale\""), 0, "not inlined:\n{}", body);
    assert!(!body.contains("param_"), "leaked parameter:\n{}", body);

    let expected = "22\n11\n";
    let (code, out) = run_t3(&src, &[]);
    assert_eq!(code, 0, "T3: {}", out);
    assert_eq!(out, expected, "T3 — the two copies collided");
    if let Some((code, out)) = run_llvm(&src, &[]) {
        assert_eq!(code, 0, "LLVM: {}", out);
        assert_eq!(out, expected, "LLVM — the two copies collided");
    }
}

/// **The size limit is the switch, and `0` is off.**
///
/// A size limit is the whole of what keeps inlining from being a code-size
/// explosion: splicing an N-instruction body at S sites adds `S * (N - 1)`
/// instructions net of the calls removed. `--no-inline` is documented as
/// `--inline-limit 0`, and a test is the only thing that keeps two spellings
/// of one switch from drifting apart.
#[test]
fn f2_the_size_limit_is_the_switch() {
    let src = write(
        "f2_inline_limit",
        r#"
use std::io;
fn scale(x: int, k: int) -> int { return x * k + 1; }
fn main() {
    io::println_int(scale(7, 3));
    io::println_int(scale(2, 5));
}
"#,
    );
    let calls = |extra: &[&str]| count(&ir_function(&emit_ir(&src, extra), "main"), "func: \"scale\"");

    assert_eq!(calls(&[]), 0, "the default limit should admit a 3-instruction body");
    assert_eq!(calls(&["--inline-limit", "1"]), 2, "a body over the limit was spliced anyway");
    assert_eq!(calls(&["--inline-limit", "0"]), 2, "limit 0 should be off");
    assert_eq!(calls(&["--no-inline"]), 2, "--no-inline should be off");

    // Whatever the flag, the program prints the same thing.
    for extra in [&[][..], &["--no-inline"][..], &["--inline-limit", "1"][..]] {
        assert_eq!(run_t3(&src, extra).1, "22\n11\n", "T3 with {:?}", extra);
    }
}

/// **The two refusals stand, and each is a different wrong answer avoided.**
///
/// * `Alloca` — the callee's frame cell would become the CALLER's, and a call
///   site in a loop would then allocate once per iteration. On LLVM that is
///   unbounded stack growth, which no test of the ANSWER would ever show.
/// * self-recursion — one pass cannot loop, but a body that calls itself is
///   not a thing to duplicate on principle.
///
/// **There used to be a third, and it was multi-block.** That was the 56 % the
/// pass declined; the CFG path does it now, so the refusal is gone and this
/// test is where it went. Its case moved to
/// `f2_a_multiblock_callee_is_spliced_through_a_phi` below, which asserts the
/// opposite of what this one used to.
///
/// Each is asserted as a surviving `Call`, and each program is also RUN: a
/// refusal that silently broke the call it declined would pass the first half.
#[test]
fn f2_the_two_refusals_stand() {
    let cases: [(&str, &str, &str, &str); 2] = [
        (
            "f2_inline_no_alloca",
            "mk",
            r#"
use std::io;
fn mk() -> [int] { return [1, 2, 3]; }
fn main() { let v: [int] = mk(); io::println_int(v[1]); }
"#,
            "2\n",
        ),
        (
            "f2_inline_no_recursion",
            "fact",
            r#"
use std::io;
fn fact(n: int) -> int { if n <= 1 { return 1; } return n * fact(n - 1); }
fn main() { io::println_int(fact(5)); }
"#,
            "120\n",
        ),
    ];

    for (stem, callee, program, expected) in cases {
        let src = write(stem, program);
        let body = ir_function(&emit_ir(&src, &[]), "main");
        assert_eq!(
            count(&body, &format!("func: \"{}\"", callee)),
            1,
            "`{}` should not have been spliced:\n{}",
            callee,
            body
        );
        let (code, out) = run_t3(&src, &[]);
        assert_eq!(code, 0, "T3 {}: {}", stem, out);
        assert_eq!(out, expected, "T3 {}", stem);
        if let Some((code, out)) = run_llvm(&src, &[]) {
            assert_eq!(code, 0, "LLVM {}: {}", stem, out);
            assert_eq!(out, expected, "LLVM {}", stem);
        }
    }
}

// ---------------------------------------------------------------------------
// F-2, the CFG path — multi-block callees (report.txt P34, P35)
//
// The 56 % the single-block path declined. The caller's block is split around
// the call, the callee's blocks are copied between the halves, every `Return`
// becomes a jump to the continuation, and a phi in the continuation joins the
// returned values.
//
// Both defects it shipped with were INVISIBLE to a test of the answer on T3,
// and they were invisible in two DIFFERENT ways, which is why the tests below
// assert on three different things:
//
// * P34 — a terminator's OPERANDS were never renamed, only its labels. The
//   spliced branch tested a temp only the original callee defined; the copy
//   that defined it under its new name was then used by nobody, so dead-code
//   elimination removed it. On T3 the register allocator gives a free temp a
//   slot and the program prints something plausible. `--verify-ssa` saw it at
//   once: 65 violations across 7 of the 17 examples.
// * P35 — the phi arm for an exhaustive `match`'s trailing block was
//   `IRValue::Void`, which is not a value either backend can name in a phi.
//   `--verify-ssa` reported ZERO — `Void` is not a temp, so nothing is
//   undefined, and the arm is PRESENT, so the edge counts match. T3 ran it and
//   printed the right answer. Only LLVM caught it, and only because the arm
//   rendered as the empty string.
//
// So neither instrument catches both, and that is the reason each of these
// tests runs BOTH backends and one of them checks SSA as well.
// ---------------------------------------------------------------------------

/// **A multi-block callee is spliced, and its branch tests the right temp.**
///
/// This is the case `f2_the_three_refusals_stand` used to assert was refused.
/// `pick` is two-armed, so the splice makes a real phi with two arms, and the
/// arms come from the two RETURNING blocks rather than from the caller.
///
/// **The two call sites are what make P34 visible in the answer.** With one
/// site, a branch left testing the callee's own free temp still gets a
/// register on T3 and can come out right by accident. With two, both copies
/// keep the SAME free name, so both branches read one register and the second
/// call returns the first call's decision: `pick(7)` printed `-7`.
#[test]
fn f2_a_multiblock_callee_is_spliced_through_a_phi() {
    let src = write(
        "f2_inline_multiblock",
        r#"
use std::io;
fn pick(x: int) -> int { if x > 0 { return x; } else { return 0 - x; } }
fn main() {
    let a: int = 0 - 4;
    io::println_int(pick(a));
    io::println_int(pick(7));
}
"#,
    );

    let body = ir_function(&emit_ir(&src, &["--no-merge-blocks"]), "main");
    assert_eq!(count(&body, "func: \"pick\""), 0, "a multi-block callee was not spliced:\n{}", body);
    assert!(!body.contains("param_"), "leaked parameter:\n{}", body);
    assert!(body.contains("il0_cont"), "no continuation block:\n{}", body);

    let expected = "4\n7\n";
    let (code, out) = run_t3(&src, &[]);
    assert_eq!(code, 0, "T3: {}", out);
    assert_eq!(out, expected, "T3 — the spliced branch tested the wrong temp");
    assert_eq!(run_t3(&src, &["--no-inline"]).1, expected, "T3 --no-inline is the reference");
    if let Some((code, out)) = run_llvm(&src, &[]) {
        assert_eq!(code, 0, "LLVM: {}", out);
        assert_eq!(out, expected, "LLVM");
    }
}

/// **report.txt P34 — the splice leaves the IR in SSA form.**
///
/// The assertion the pass most needed and did not have. A terminator has
/// OPERANDS as well as labels — `BinBranch` and `TritBranch` each test a value
/// — and the copy renamed only the labels, so the branch was left reading a
/// name that only the ORIGINAL callee defines.
///
/// `Undefined { temp, used_in }` is exactly that violation and the verifier
/// has reported it since F-1. Asserting the COUNT rather than the output is
/// what makes this test independent of whether the wrong register happens to
/// hold the right value, which on T3 it often does.
#[test]
fn f2_a_multiblock_splice_keeps_the_ir_in_ssa_form() {
    // Three shapes: a two-way branch, a three-way `match` on a Result whose
    // trailing block returns Void, and a four-way chain of early returns, so
    // the continuation's phi joins four arms from four different blocks.
    //
    // None of them contains a LOOP, and that is a constraint rather than an
    // oversight: P36 refuses a loop-containing callee outright, so a loop here
    // would test the refusal instead of the splice.
    let src = write(
        "f2_inline_multiblock_ssa",
        r#"
use std::io;
use std::str;
use std::fmt;
fn pick(x: int) -> int { if x > 0 { return x; } else { return 0 - x; } }
fn shows(r: Result<int, str>) -> str {
    match r { Ok(v) => { return fmt::show_int(v); }, Unknown(m) => { return str::concat("U:", m); }, Err(e) => { return str::concat("E:", e); } }
}
fn nested(n: int) -> int {
    if n < 0 { return 0 - 1; }
    if n == 0 { return 0; }
    if n < 10 { return n * 2; }
    return n + 1;
}
fn main() {
    io::println_int(pick(0 - 4));
    io::println_int(pick(7));
    io::println(shows(str::try_parse_int("13")));
    io::println(shows(str::try_parse_int("no")));
    io::println_int(nested(0 - 5));
    io::println_int(nested(0));
    io::println_int(nested(4));
    io::println_int(nested(40));
}
"#,
    );

    let on = ssa_report(&src, &[]);
    let off = ssa_report(&src, &["--no-inline"]);
    assert_eq!(violations(&off, "after optimisation"), 0, "the control is not clean:\n{}", off);
    assert_eq!(
        violations(&on, "after optimisation"),
        0,
        "splicing a multi-block callee broke SSA form:\n{}",
        on
    );

    // And the pass actually ran on this program, or the assertion above is
    // about a compilation that never spliced anything.
    let body = ir_function(&emit_ir(&src, &[]), "main");
    for callee in ["pick", "shows", "nested"] {
        assert_eq!(
            count(&body, &format!("func: \"{}\"", callee)),
            0,
            "`{}` was not spliced, so this test proves nothing:\n{}",
            callee,
            body
        );
    }

    let expected = "4\n7\n13\nE:not an integer\n-1\n0\n8\n41\n";
    let (code, out) = run_t3(&src, &[]);
    assert_eq!(code, 0, "T3: {}", out);
    assert_eq!(out, expected, "T3");
    assert_eq!(run_t3(&src, &["--no-inline"]).1, expected, "T3 --no-inline is the reference");
    if let Some((code, out)) = run_llvm(&src, &[]) {
        assert_eq!(code, 0, "LLVM: {}", out);
        assert_eq!(out, expected, "LLVM");
    }
}

/// **The SSA verifier reports a `Void` phi arm, which it could not before.**
///
/// P35 was invisible to `--verify-ssa` and it should not have been. `Void` is
/// not a temp, so `Undefined` never fired; the arm was PRESENT, so `PhiEdges`
/// never fired either. A malformed module verified clean while only the LLVM
/// backend objected — and the whole point of the verifier is that a defect in
/// a control-flow pass should be findable without building for a second
/// backend.
///
/// `Violation::VoidPhiArm` closes that. The assertion here is that the counter
/// EXISTS and reads zero on a program built exactly out of the shape that
/// produced P35 — three-armed `match` on a `Result`, whose trailing
/// `match_nextN` block returns `Void`, spliced through the CFG path. If the
/// sanitiser is ever removed the count goes to 2 and this fails on T3 alone.
#[test]
fn f2_the_verifier_now_reports_a_void_phi_arm() {
    let src = write(
        "f2_void_arm_verified",
        r#"
use std::io;
use std::str;
use std::fmt;
fn shows(r: Result<int, str>) -> str {
    match r { Ok(v) => { return fmt::show_int(v); }, Unknown(m) => { return str::concat("U:", m); }, Err(e) => { return str::concat("E:", e); } }
}
fn main() {
    io::println(shows(str::try_parse_int("13")));
    io::println(shows(str::try_parse_int("no")));
}
"#,
    );

    let report = ssa_report(&src, &[]);
    assert!(
        report.contains("void-phi-arm="),
        "the verifier does not report this class at all:\n{}",
        report
    );
    assert_eq!(
        violations(&report, "after optimisation"),
        0,
        "a Void reached a phi arm:\n{}",
        report
    );
    // And the splice really happened, or the count above is about nothing.
    let body = ir_function(&emit_ir(&src, &[]), "main");
    assert_eq!(count(&body, "func: \"shows\""), 0, "not spliced:\n{}", body);
}

/// **report.txt P36 — a callee that CONTAINS a loop is not spliced, and the
/// reason is the constant argument rather than the loop.**
///
/// Splicing binds each parameter to the argument VALUE — the whole reason the
/// pass runs before constant folding, since a constant argument makes the body
/// foldable. Inside a loop it inverts: a bound that was a PARAMETER lives in a
/// register and is compared once per iteration, and substituted it is a
/// LITERAL that the T3 backend re-materialises with `TLIT` every iteration,
/// because there is no loop-invariant code motion to hoist it.
///
/// **The assertion is a COST comparison, not an output comparison**, and it
/// has to be: the two implementations agree on every answer — that is why both
/// exist — so a pessimisation is invisible to every test of a value. Comparing
/// two profiles rather than asserting a magic constant makes the test
/// self-calibrating, which matters because the gap here is about one
/// instruction per iteration and not the order of magnitude the stdlib refusal
/// had.
///
/// The matching case in the other direction is `f2_a_loop_free_callee_is_still_spliced`
/// below: a loop in the CALLER is no reason to refuse anything.
#[test]
fn f2_a_callee_that_contains_a_loop_is_not_spliced() {
    let src = write(
        "f2_inline_loop_callee",
        r#"
use std::io;
fn accum(n: int) -> int {
    if n <= 0 { return 0; }
    let mut s = 0;
    let mut i = 0;
    while i < n { s = s + i; i = i + 1; }
    return s;
}
fn main() {
    io::println_int(accum(60));
    io::println_int(accum(61));
    io::println_int(accum(62));
}
"#,
    );

    let body = ir_function(&emit_ir(&src, &[]), "main");
    assert_eq!(
        count(&body, "func: \"accum\""),
        3,
        "a callee containing a loop should not have been spliced:\n{}",
        body
    );

    let (out_on, prof_on) = profile_of(&src, &[]);
    let (out_off, prof_off) = profile_of(&src, &["--no-inline"]);
    let on = profile_value(&prof_on, "total-instructions");
    let off = profile_value(&prof_off, "total-instructions");
    assert_eq!(without_banner(&out_on), without_banner(&out_off), "the answer moved");
    assert_eq!(without_banner(&out_on), "1770\n1830\n1891\n", "wrong answer");
    // Measured: refusing costs nothing here, and splicing cost +165 (+7.3 %).
    assert!(
        on <= off,
        "splicing a loop-containing callee cost {} instructions against {} with \
         the pass off — the refusal has stopped working",
        on,
        off
    );
}

/// **And the refusal is about the CALLEE, not the caller.**
///
/// A branch-only callee invoked 180 times from a loop in the caller is the
/// case the CFG path exists for, and it is worth −13.6 % on this program. A
/// refusal keyed on "is there a loop anywhere in sight" would decline it and
/// the pass would be worth nothing at all.
#[test]
fn f2_a_loop_free_callee_is_still_spliced() {
    let src = write(
        "f2_inline_loop_caller",
        r#"
use std::io;
fn grade(n: int) -> int {
    if n <= 0 { return 0; }
    if n < 10 { return 1; }
    if n < 20 { return 2; }
    if n < 30 { return 3; }
    return 4;
}
fn main() {
    let mut i = 0;
    let mut t = 0;
    while i < 180 { t = t + grade(i); i = i + 1; }
    io::println_int(t);
}
"#,
    );

    let body = ir_function(&emit_ir(&src, &[]), "main");
    assert_eq!(
        count(&body, "func: \"grade\""),
        0,
        "a loop in the CALLER is no reason to refuse a loop-free callee:\n{}",
        body
    );

    let (out_on, prof_on) = profile_of(&src, &[]);
    let (out_off, prof_off) = profile_of(&src, &["--no-inline"]);
    let on = profile_value(&prof_on, "total-instructions");
    let off = profile_value(&prof_off, "total-instructions");
    assert_eq!(without_banner(&out_on), without_banner(&out_off), "the answer moved");
    assert_eq!(without_banner(&out_on), "659\n", "wrong answer");
    assert!(
        on < off,
        "splicing should have paid here: {} instructions against {} with the pass off",
        on,
        off
    );
}

/// **report.txt P35 — every phi arm the splice makes is a value LLVM can name.**
///
/// An exhaustive `match` still gets a trailing `match_nextN` block that no arm
/// matched, and the lowerer terminates it `Return(Some(Void))`. On a `ret`
/// both backends coerce that — LLVM emits `ret ptr null` — but a PHI ARM gets
/// no coercion, and `IRValue::Void` renders as the EMPTY STRING:
///
/// ```text
///   %t1 = phi ptr [ %t8, %arm1 ], [ %t15, %arm4 ], [ %t22, %arm7 ], [ , %next8 ]
/// ```
///
/// which is not parseable. The lowerer has had `sanitize_phi_incoming` for
/// exactly this since it started emitting phis; the inliner is the compiler's
/// only OTHER producer of phis and did not use it.
///
/// **The assertion has to be that LLVM LINKS.** T3 accepted the malformed arm
/// and printed the right answer, and `--verify-ssa` reported zero violations —
/// `Void` is not a temp, so nothing is undefined, and the arm is present, so
/// the edge counts match. This is the one defect in the pair that only the
/// other backend could see.
#[test]
fn f2_an_exhaustive_match_arm_is_a_nameable_value() {
    let src = write(
        "f2_inline_void_arm",
        r#"
use std::io;
use std::str;
use std::fmt;
fn shows(r: Result<int, str>) -> str {
    match r { Ok(v) => { return fmt::show_int(v); }, Unknown(m) => { return str::concat("Unknown:", m); }, Err(e) => { return str::concat("Err:", e); } }
}
fn main() {
    io::println(shows(str::try_parse_int("  13  ")));
    io::println(shows(str::try_parse_int("12a")));
}
"#,
    );

    let body = ir_function(&emit_ir(&src, &[]), "main");
    assert_eq!(count(&body, "func: \"shows\""), 0, "not spliced:\n{}", body);
    assert!(
        !body.contains("Void"),
        "a Void reached a phi arm, which LLVM renders as nothing:\n{}",
        body
    );

    let expected = "13\nErr:not an integer\n";
    let (code, out) = run_t3(&src, &[]);
    assert_eq!(code, 0, "T3: {}", out);
    assert_eq!(out, expected, "T3");
    // The real assertion: this is the half T3 could not see.
    let llvm = run_llvm(&src, &[]).expect("LLVM backend should be available for this test");
    assert_eq!(llvm.0, 0, "LLVM refused the module:\n{}", llvm.1);
    assert_eq!(llvm.1, expected, "LLVM");
}

/// **A callee that returns a parameter goes through an `Assign`, not a rename.**
///
/// The result of a splice is usually renamed straight onto the call's
/// destination, which costs nothing. That is only available when the body
/// COMPUTED what it returns; a function whose return value is one of its own
/// parameters defines no temp at all, and its "body" after substitution is the
/// argument itself. The copy is what carries it to the destination, and
/// getting this case wrong renames a temp that the caller — not the callee —
/// owns.
#[test]
fn f2_a_callee_can_return_its_own_parameter() {
    let src = write(
        "f2_inline_passthrough",
        &format!(
            r#"
use std::io;
fn id(x: int) -> int {{ return x; }}
{}
fn main() {{
    let a: int = opaque(7);
    io::println_int(id(a) + id(2));
}}
"#,
            OPAQUE
        ),
    );
    let body = ir_function(&emit_ir(&src, &[]), "main");
    assert_eq!(count(&body, "func: \"id\""), 0, "not inlined:\n{}", body);
    assert!(!body.contains("param_"), "leaked parameter:\n{}", body);

    let (code, out) = run_t3(&src, &[]);
    assert_eq!(code, 0, "T3: {}", out);
    assert_eq!(out, "9\n", "T3");
    if let Some((code, out)) = run_llvm(&src, &[]) {
        assert_eq!(code, 0, "LLVM: {}", out);
        assert_eq!(out, "9\n", "LLVM");
    }
}

/// **Splicing does not reorder side effects.**
///
/// A `Call` runs unconditionally at its place in its block, and so does the
/// body that replaces it — the whole reason the single-block case needs no CFG
/// surgery. This is the test that would notice if a future version hoisted
/// anything, and printing is the side effect that cannot be optimised away.
#[test]
fn f2_splicing_does_not_reorder_side_effects() {
    let src = write(
        "f2_inline_order",
        r#"
use std::io;
fn shout(n: int) -> int { io::println_int(n); return n; }
fn main() {
    io::println("a");
    let z: int = shout(2);
    io::println("b");
    io::println_int(z);
}
"#,
    );
    let body = ir_function(&emit_ir(&src, &[]), "main");
    assert_eq!(count(&body, "func: \"shout\""), 0, "not inlined:\n{}", body);

    let expected = "a\n2\nb\n2\n";
    let (code, out) = run_t3(&src, &[]);
    assert_eq!(code, 0, "T3: {}", out);
    assert_eq!(out, expected, "T3");
    if let Some((code, out)) = run_llvm(&src, &[]) {
        assert_eq!(code, 0, "LLVM: {}", out);
        assert_eq!(out, expected, "LLVM");
    }
}

/// **On the shipped examples, inlining changes no output on either backend.**
///
/// The unit cases above are constructed; these are the real programs, and they
/// are the ones that exercise the standard library's forwarding wrappers —
/// which is where the call sites actually are. The three chosen splice 60, 33
/// and 32 sites; `--no-inline` is the reference, and the pass is only allowed
/// to make them faster.
#[test]
fn f2_inlining_changes_no_example_output() {
    for name in ["ternary_calculator", "database", "neural_net"] {
        // A COPY, in the temp directory. `run_t3` derives its `-o` from the
        // source path, so handing it `examples/<name>.mt` writes the `.t3l`,
        // `.t3b` and `.bin` into the repository — which is how four tracked
        // `.t3l` files come to be modified after a test run.
        let text = std::fs::read_to_string(example(name)).expect("example source");
        let src = write(name, &text);
        let (rc_off, off) = run_t3(&src, &["--no-inline"]);
        let (rc_on, on) = run_t3(&src, &[]);
        assert_eq!(rc_on, rc_off, "{}: T3 exit code moved", name);
        assert_eq!(on, off, "{}: T3 output moved under inlining", name);
        if let (Some((lrc_off, loff)), Some((lrc_on, lon))) =
            (run_llvm(&src, &["--no-inline"]), run_llvm(&src, &[]))
        {
            assert_eq!(lrc_on, lrc_off, "{}: LLVM exit code moved", name);
            assert_eq!(lon, loff, "{}: LLVM output moved under inlining", name);
            assert_eq!(lon, on, "{}: the two backends disagree under inlining", name);
        }
    }
}

/// Run a compiled program under an instruction budget; `Some(exit_code)`.
/// Exit 71 is the emulator's "budget ran out", which is what makes a step
/// budget usable as an assertion about COST rather than about output.
fn run_t3_capped(src: &PathBuf, extra: &[&str], max_steps: usize) -> i32 {
    let base = src.with_extension("");
    let mut args: Vec<String> = vec![
        "compile".into(),
        src.to_string_lossy().into_owned(),
        "--target".into(),
        "t3".into(),
        "-o".into(),
        base.to_string_lossy().into_owned(),
    ];
    args.extend(extra.iter().map(|s| s.to_string()));
    let c = Command::new(manitc()).args(&args).output().expect("compile");
    assert!(c.status.success(), "compile:\n{}", String::from_utf8_lossy(&c.stderr));
    let r = Command::new(manitc())
        .args([
            "run-t3",
            "--max-steps",
            &max_steps.to_string(),
            base.with_extension("t3b").to_str().unwrap(),
        ])
        .output()
        .expect("run");
    r.status.code().unwrap_or(-1)
}

/// **report.txt P30 — a stdlib wrapper the backend implements itself is not
/// spliced, and the assertion is about COST.**
///
/// `fmt::align_right` is the pass's single most attractive candidate: 91 call
/// sites across the corpus and a body of ONE instruction, forwarding to
/// `str::pad_left`. It is also a trap. The `fmt`, `str`, `ternary`, `math`,
/// `env`, `test` and `trit` modules are *mixed* — `stdlib_expand` merges a
/// ManiT body AND each emitter intercepts the call — so on T3 a call to
/// `fmt::align_right` is `SYSCALL #15`, one instruction, and the ManiT body is
/// compiled and never reached. Splicing it replaces the syscall with the
/// software implementation: 188 instructions per call in place of one.
///
/// **No test of OUTPUT can catch that** — the two implementations agree, which
/// is the whole reason both exist. So this asserts the budget instead. Twenty
/// iterations cost 564 instructions with the refusal in place and 4,324
/// without; 1,200 sits an order of magnitude clear of the first and a quarter
/// of the way to the second, so it neither flutters nor forgives.
#[test]
fn f2_a_native_backed_stdlib_wrapper_is_not_spliced() {
    let src = write(
        "f2_inline_native",
        r#"
use std::io;
use std::fmt;
fn main() {
    let mut i: int = 0;
    let mut acc: int = 0;
    while i < 20 {
        let s: str = fmt::align_right("x", 8, ' ');
        acc = acc + str::len(s);
        i = i + 1;
    }
    io::println_int(acc);
}
"#,
    );

    let body = ir_function(&emit_ir(&src, &[]), "main");
    assert_eq!(
        count(&body, "func: \"fmt::align_right\""),
        1,
        "the wrapper was spliced and its syscall lost:\n{}",
        body
    );

    assert_eq!(
        run_t3_capped(&src, &[], 1_200),
        0,
        "twenty padded strings took over 1,200 instructions — a native was inlined away"
    );
    // The same program with the pass off, to show the budget is the pass's and
    // not the program's.
    assert_eq!(run_t3_capped(&src, &["--no-inline"], 1_200), 0, "--no-inline");

    let (code, out) = run_t3(&src, &[]);
    assert_eq!(code, 0, "T3: {}", out);
    assert_eq!(out, "160\n", "T3");
    if let Some((code, out)) = run_llvm(&src, &[]) {
        assert_eq!(code, 0, "LLVM: {}", out);
        assert_eq!(out, "160\n", "LLVM");
    }
}

/// **And the refusal is not over-broad: a user function is still spliced.**
///
/// The module test would be worthless if it also refused ordinary code — the
/// pass would be off, and every other test here would still pass. This is the
/// one that says the pass does something.
#[test]
fn f2_the_stdlib_refusal_does_not_disable_the_pass() {
    let src = write(
        "f2_inline_user_still",
        &format!(
            r#"
use std::io;
use std::fmt;
fn my_align(s: str, w: int) -> str {{ return fmt::align_right(s, w, ' '); }}
{}
fn main() {{
    let n: int = opaque(6);
    io::println_int(str::len(my_align("x", n)));
}}
"#,
            OPAQUE
        ),
    );
    let body = ir_function(&emit_ir(&src, &[]), "main");
    assert_eq!(count(&body, "func: \"my_align\""), 0, "a user wrapper was not spliced:\n{}", body);
    assert_eq!(
        count(&body, "func: \"fmt::align_right\""),
        1,
        "splicing the user wrapper should leave the native call it forwards to:\n{}",
        body
    );
    let (code, out) = run_t3(&src, &[]);
    assert_eq!(code, 0, "T3: {}", out);
    assert_eq!(out, "6\n", "T3");
}

// ---------------------------------------------------------------------------
// P31 / P32 — the measurement instrument
// ---------------------------------------------------------------------------

/// `run-t3 --profile` for one compiled program: `(stdout, profile_lines)`.
fn profile_of(src: &PathBuf, extra: &[&str]) -> (String, Vec<String>) {
    let base = src.with_extension("");
    let mut args: Vec<String> = vec![
        "compile".into(),
        src.to_string_lossy().into_owned(),
        "--target".into(),
        "t3".into(),
        "-o".into(),
        base.to_string_lossy().into_owned(),
    ];
    args.extend(extra.iter().map(|s| s.to_string()));
    let c = Command::new(manitc()).args(&args).output().expect("compile");
    assert!(c.status.success(), "compile:\n{}", String::from_utf8_lossy(&c.stderr));
    let r = Command::new(manitc())
        .args(["run-t3", "--profile", base.with_extension("t3b").to_str().unwrap()])
        .output()
        .expect("run");
    (
        String::from_utf8_lossy(&r.stdout).into_owned(),
        String::from_utf8_lossy(&r.stderr).lines().map(str::to_string).collect(),
    )
}

/// A profiled run's stdout with the emulator's own banner lines removed.
fn without_banner(out: &str) -> String {
    out.lines()
        .filter(|l| !l.starts_with("[T3ISA]"))
        .map(|l| format!("{}\n", l))
        .collect()
}

/// Pull one `[T3ISA] profile  <key> <value>` count out of a profile.
fn profile_value(lines: &[String], key: &str) -> usize {
    let want = format!("[T3ISA] profile  {} ", key);
    let line = lines
        .iter()
        .find(|l| l.starts_with(&want))
        .unwrap_or_else(|| panic!("no `{}` in profile:\n{}", key, lines.join("\n")));
    line.split_whitespace().last().unwrap().parse().expect("count")
}

/// **report.txt P31 — the emulator's execution profile has an exit.**
///
/// It was always collected and only `manitc bench` could print it, so an
/// optimiser pass had to be measured by bisecting `--max-steps` — about forty
/// emulator runs to recover one number the emulator was already holding, and
/// the per-opcode histogram discarded at the end of it.
///
/// The assertions that matter are that the profile is CONSISTENT (the category
/// counters and the histogram describe the same run) and that asking for it
/// cannot disturb the run: it goes to stderr, so stdout is byte-identical with
/// and without the flag.
#[test]
fn p31_run_t3_profile_reports_the_execution_profile() {
    let src = write(
        "p31_profile",
        r#"
use std::io;
fn main() {
    let mut i: int = 0;
    let mut acc: int = 0;
    while i < 10 { acc = acc + i * 3; i = i + 1; }
    io::println_int(acc);
}
"#,
    );
    let (out, prof) = profile_of(&src, &[]);
    assert_eq!(out.lines().filter(|l| !l.starts_with("[T3ISA]")).count(), 1, "stdout: {}", out);

    let total = profile_value(&prof, "total-instructions");
    assert!(total > 0, "nothing counted:\n{}", prof.join("\n"));

    // The histogram must account for every instruction, exactly. A missing
    // opcode name would make the two disagree silently, which is how P27
    // (`TritLane` absent from `instr_dst_name`) hid.
    let hist: usize = prof
        .iter()
        .filter(|l| l.starts_with("[T3ISA] profile  opcode "))
        .map(|l| l.split_whitespace().last().unwrap().parse::<usize>().unwrap())
        .sum();
    assert_eq!(hist, total, "histogram and total disagree:\n{}", prof.join("\n"));

    // The categories partition nothing — an opcode can be in none of them —
    // so each must merely not exceed the total.
    for k in ["arithmetic-ops", "ternary-native-ops", "control-flow-ops", "memory-ops"] {
        assert!(profile_value(&prof, k) <= total, "{} exceeds the total", k);
    }

    // Asking for the profile does not change what the program prints.
    let plain = run_t3(&src, &[]).1;
    assert_eq!(
        out.lines().filter(|l| !l.starts_with("[T3ISA]")).collect::<Vec<_>>(),
        plain.lines().collect::<Vec<_>>(),
        "--profile disturbed stdout"
    );
}

/// **report.txt P32 — a program that halts on its last budgeted instruction
/// ran to completion; it was not cut off.**
///
/// `run`'s exit test was `steps >= max_steps` alone, but the loop also exits
/// when the program halts — so a program halting on its `max_steps`-th
/// instruction left `steps == max_steps` and was reported as step-limited,
/// returning 71 instead of its own exit code. `run_debug` has always tested
/// `self.halted` first.
///
/// It made `--max-steps` off by exactly one, and since bisecting `--max-steps`
/// is how this project measures an optimiser (P22), **every dynamic
/// instruction count taken that way was one too high.** The two instruments
/// now agree, and that agreement is the assertion: the profile says N, and N
/// is exactly the smallest budget the program completes under.
#[test]
fn p32_the_step_budget_is_off_by_one_no_longer() {
    let src = write(
        "p32_budget",
        r#"
use std::io;
fn main() {
    let mut i: int = 0;
    while i < 5 { i = i + 1; }
    io::println_int(i);
}
"#,
    );
    let (_, prof) = profile_of(&src, &[]);
    let n = profile_value(&prof, "total-instructions");
    assert!(n > 2, "implausible instruction count {}", n);

    assert_eq!(run_t3_capped(&src, &[], n), 0, "a budget of exactly {} should suffice", n);
    assert_eq!(run_t3_capped(&src, &[], n + 1), 0, "a budget of {} should suffice", n + 1);
    assert_eq!(
        run_t3_capped(&src, &[], n - 1),
        71,
        "a budget of {} is one short and must be reported as cut off",
        n - 1
    );
}

/// **report.txt P33 — the instruction budget charges work done inside a
/// callback, because that work is still work.**
///
/// A syscall handed a maniT function pointer — `Vec::map`, `Vec::filter`,
/// `Vec::fold`, `for_each` — does not return to the emulator's main loop to run
/// it. It drives the callee in a RE-ENTRANT loop of its own, which used to
/// count against a private 1,000,000 rather than against `max_steps`. So every
/// instruction executed inside a callback was recorded in the profile and
/// charged to nothing: `examples/concurrency` executed 30,299 instructions
/// under a budget of 26,699 and exited 0.
///
/// Two consequences, and the second is why it matters here. `--max-steps` was
/// not a bound on a runaway program at all if the runaway was in a callback.
/// And bisecting `--max-steps` for a dynamic instruction count — how P22
/// measured every optimiser pass — under-reported exactly the programs that use
/// callbacks, silently, because the answer it gives is self-consistent.
///
/// The assertion is that the profile and the budget agree ON A PROGRAM WHOSE
/// WORK IS ALL IN A CALLBACK: N completes, N−1 is cut off. Before the fix the
/// callback body was free, so a budget far below N still ran to completion.
#[test]
fn p33_the_budget_charges_work_done_inside_a_callback() {
    let src = write(
        "p33_callback_budget",
        r#"
use std::io;
use std::collections;
fn heavy(x: int) -> int {
    let mut acc: int = 0;
    let mut i: int = 0;
    while i < 40 { acc = acc + x * i; i = i + 1; }
    return acc;
}
fn main() {
    let mut v: Vec<int> = Vec::new();
    let mut i: int = 0;
    while i < 8 { v.push(i); i = i + 1; }
    let w: Vec<int> = v.map(heavy);
    io::println_int(w.len());
}
"#,
    );

    let (_, prof) = profile_of(&src, &[]);
    let n = profile_value(&prof, "total-instructions");
    // Eight callbacks of a forty-iteration loop each: if the callback bodies
    // were not counted this would be a few hundred rather than thousands, so
    // the floor is itself part of the assertion.
    assert!(n > 3_000, "the callback bodies are not being counted: {}", n);

    assert_eq!(run_t3_capped(&src, &[], n), 0, "a budget of exactly {} should suffice", n);
    assert_eq!(
        run_t3_capped(&src, &[], n - 1),
        71,
        "a budget of {} is one short and must be reported as cut off",
        n - 1
    );
    // And far below: the callback work must not be free.
    assert_eq!(
        run_t3_capped(&src, &[], n / 2),
        71,
        "half the budget ran to completion — callback instructions are uncharged"
    );
}

// ---------------------------------------------------------------------------
// P26 — block merging
// ---------------------------------------------------------------------------

/// How many times one opcode executed, from `run-t3 --profile`.
fn opcode_count(prof: &[String], mnemonic: &str) -> usize {
    let want = format!("[T3ISA] profile  opcode {} ", mnemonic);
    prof.iter()
        .find(|l| l.starts_with(&want))
        .map(|l| l.split_whitespace().last().unwrap().parse().unwrap())
        .unwrap_or(0)
}

/// **report.txt P26 — merging a block into its single predecessor removes
/// executed JUMPs, and that is the whole of what it does.**
///
/// 35.9 % of blocks in this IR are empty and 27.7 % are empty with a plain
/// `Jump`. P26 was expected to pay by giving the block-scoped passes something
/// to look at; measured over the seventeen examples it changed **no downstream
/// pass's output on any of them**, because the blocks it merges are empty — the
/// predecessor gains no instructions, so no pass sees anything new. What goes
/// is the jump.
///
/// So the assertion is on the executed JUMP count, which is what `--profile`
/// exists for (P31), and on the answer being unchanged. A static instruction
/// count cannot see this at all: a `Jump` is a TERMINATOR, and `count_func`
/// counts instructions.
#[test]
fn p26_merging_blocks_removes_executed_jumps() {
    let src = write(
        "p26_jumps",
        r#"
use std::io;
fn classify(x: int) -> int {
    if x > 10 { return 3; }
    if x > 5 { return 2; }
    if x > 0 { return 1; }
    return 0;
}
fn main() {
    let mut i: int = 0;
    let mut acc: int = 0;
    while i < 12 { acc = acc + classify(i); i = i + 1; }
    io::println_int(acc);
}
"#,
    );
    let (out_on, prof_on) = profile_of(&src, &[]);
    let (out_off, prof_off) = profile_of(&src, &["--no-merge-blocks"]);

    // The `[T3ISA] running … (N words)` banner is not the program's output, and
    // it deliberately differs here: merging makes the program SMALLER, which is
    // the banner's whole content. Compare what the program printed.
    let printed = |s: &str| {
        s.lines().filter(|l| !l.starts_with("[T3ISA]")).collect::<Vec<_>>().join("\n")
    };
    assert_eq!(
        printed(&out_on),
        printed(&out_off),
        "merging changed what the program printed"
    );

    let jumps_on = opcode_count(&prof_on, "JUMP");
    let jumps_off = opcode_count(&prof_off, "JUMP");
    assert!(
        jumps_on < jumps_off,
        "no jumps removed: {} with merging, {} without",
        jumps_on,
        jumps_off
    );

    // And nothing ELSE moved. Merging relocates instructions between blocks;
    // it must not add, remove or reorder any of them, and every opcode but
    // JUMP holding still is the sharpest statement of that available.
    for op in ["TADD", "TSUB", "TMUL", "LOAD", "STORE", "CALL", "SYSCALL", "TLIT", "MOV"] {
        assert_eq!(
            opcode_count(&prof_on, op),
            opcode_count(&prof_off, op),
            "{} moved, so merging did more than delete jumps",
            op
        );
    }
}

/// **A loop's phis survive the merge on both backends.**
///
/// Merging deletes a block, and every phi downstream that took a value on an
/// edge from it still names it. Re-pointing those is the one dangerous step in
/// the pass — the phi keeps an arm and a value, so nothing looks wrong, and the
/// backend then hunts for a predecessor no block is called any more. That is
/// the phi-on-its-edge mistake behind P11, P12 and P14, and a loop-carried
/// dependency is where it shows.
///
/// `a, b = b, a + b` is the shape that caught P11: two phis whose homes are
/// each other's sources.
#[test]
fn p26_a_loop_carried_phi_survives_merging() {
    let src = write(
        "p26_phi",
        r#"
use std::io;
fn main() {
    let mut a: int = 0;
    let mut b: int = 1;
    let mut i: int = 0;
    while i < 20 {
        let t: int = a + b;
        a = b;
        b = t;
        i = i + 1;
    }
    io::println_int(a);
    io::println_int(b);
}
"#,
    );
    let expected = "6765\n10946\n";
    let (code, out) = run_t3(&src, &[]);
    assert_eq!(code, 0, "T3: {}", out);
    assert_eq!(out, expected, "T3 with merging");
    assert_eq!(run_t3(&src, &["--no-merge-blocks"]).1, expected, "T3 without merging");
    if let Some((code, out)) = run_llvm(&src, &[]) {
        assert_eq!(code, 0, "LLVM: {}", out);
        assert_eq!(out, expected, "LLVM with merging");
    }
}
