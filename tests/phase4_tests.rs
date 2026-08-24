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
    let slot = N.fetch_add(1, Ordering::Relaxed);
    let d = std::env::temp_dir().join(format!("manitc_p4_{}_{}", std::process::id(), slot));
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
