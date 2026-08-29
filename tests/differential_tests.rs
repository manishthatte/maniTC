//! Randomised differential testing of the ternary word operations, against a
//! THIRD implementation.
//!
//! Author: Manish Jagdish Thatte
//!
//! ## Why a third implementation
//!
//! N7 of the enhancement plan: *"do not trust agreement between the two
//! backends as evidence."* Two backends give a differential oracle, which finds
//! disagreements and is structurally blind to shared mistakes — anything
//! decided upstream of the split, in the parser, the analyser or the IR
//! lowering. The project has been burned by exactly that twice (section 31's
//! negative module-level constants, section 51's module-level bool3: both
//! backends agreed AND were both wrong).
//!
//! So `reference` below is written from the NORMATIVE TEXT of
//! `docs/t3isa-reference.md` §5, not from `codegen_t3::isa`. It deliberately
//! does not call the emulator's helpers, and it decomposes a word by a
//! different route than they do, so that a mistake in `trits27` cannot be
//! reproduced here and cancel out. It is the seed of A3's conformance suite at
//! a fraction of A3's cost.
//!
//! ## Why randomised, and why the values are large
//!
//! The `Trit*` width defect (report.txt P1) is invisible to any operand that
//! fits in 8 bits, and -1/0/+1 is what a trit test naturally reaches for. The
//! generator is therefore biased towards WIDE words, and every case is checked
//! three ways: T3, LLVM, and the reference.
//!
//! The seed is fixed. "Randomised" here buys coverage, not surprise — a test
//! that fails only on Tuesdays is not a test.

use std::path::PathBuf;
use std::process::Command;

const LANES: usize = 27;
/// (3^27 - 1) / 2 — the largest magnitude 27 balanced trits can hold.
const T3_MAX: i64 = 3_812_798_742_493;

// ---------------------------------------------------------------------------
// The reference implementation — from the specification, not from the emulator
// ---------------------------------------------------------------------------

mod reference {
    use super::{LANES, T3_MAX};

    /// Lane `i` of `w` is the digit `d_i` in `w = sum d_i * 3^i`, each in
    /// {-1, 0, +1}. The spec defines lanes by that identity, so this recovers
    /// them from it directly — descending from the top power of three rather
    /// than by repeated division from the bottom, which is the other obvious
    /// route and the one `isa::trits27` takes. Two different derivations of
    /// the same definition is the point.
    pub fn lanes(w: i64) -> [i8; LANES] {
        assert!(w.abs() <= T3_MAX, "outside the 27-trit range: {}", w);
        let mut out = [0i8; LANES];
        let mut rem = w;
        // Largest power of three that fits, working down.
        let mut pow = vec![1i64; LANES];
        for i in 1..LANES {
            pow[i] = pow[i - 1] * 3;
        }
        for i in (0..LANES).rev() {
            // The remaining lanes below i can express at most (3^i - 1)/2.
            let below = (pow[i] - 1) / 2;
            let d: i8 = if rem > below {
                1
            } else if rem < -below {
                -1
            } else {
                0
            };
            out[i] = d;
            rem -= d as i64 * pow[i];
        }
        assert_eq!(rem, 0, "lane decomposition did not close for {}", w);
        out
    }

    pub fn from_lanes(l: &[i8; LANES]) -> i64 {
        let mut v = 0i64;
        let mut p = 1i64;
        for i in 0..LANES {
            v += l[i] as i64 * p;
            p *= 3;
        }
        v
    }

    fn zip(a: i64, b: i64, f: impl Fn(i8, i8) -> i8) -> i64 {
        let (la, lb) = (lanes(a), lanes(b));
        let mut out = [0i8; LANES];
        for i in 0..LANES {
            out[i] = f(la[i], lb[i]);
        }
        from_lanes(&out)
    }

    pub fn tandw(a: i64, b: i64) -> i64 { zip(a, b, |x, y| if x < y { x } else { y }) }
    pub fn torw(a: i64, b: i64) -> i64 { zip(a, b, |x, y| if x > y { x } else { y }) }
    pub fn tcmpw(a: i64, b: i64) -> i64 {
        zip(a, b, |x, y| if x > y { 1 } else if x < y { -1 } else { 0 })
    }
    /// Balanced sum mod 3. Written as a table rather than as arithmetic, so an
    /// error in the modular reduction cannot be shared with the implementations.
    pub fn txorw(a: i64, b: i64) -> i64 {
        zip(a, b, |x, y| match (x, y) {
            (-1, -1) => 1,
            (-1, 0) | (0, -1) => -1,
            (-1, 1) | (1, -1) | (0, 0) => 0,
            (0, 1) | (1, 0) => 1,
            (1, 1) => -1,
            _ => unreachable!(),
        })
    }
    /// Łukasiewicz implication, min(+1, 1 - a + b), as a table for the same
    /// reason. The (0, 0) cell is +1: that is what makes it L3 and not K3.
    pub fn timpw(a: i64, b: i64) -> i64 {
        zip(a, b, |x, y| match (x, y) {
            (-1, _) => 1,
            (0, -1) => 0,
            (0, 0) | (0, 1) => 1,
            (1, -1) => -1,
            (1, 0) => 0,
            (1, 1) => 1,
            _ => unreachable!(),
        })
    }
    pub fn tnotw(a: i64) -> i64 { -a }

    pub fn count(a: i64, k: i8) -> i64 {
        lanes(a).iter().filter(|&&t| t == k).count() as i64
    }
    pub fn sign(a: i64) -> i64 { if a > 0 { 1 } else if a < 0 { -1 } else { 0 } }
    pub fn abs(a: i64) -> i64 { if a < 0 { -a } else { a } }
    pub fn leading_zeros(a: i64) -> i64 {
        if a == 0 { return LANES as i64; }
        let l = lanes(a);
        let hi = (0..LANES).rev().find(|&i| l[i] != 0).unwrap();
        (LANES - 1 - hi) as i64
    }
    pub fn trailing_zeros(a: i64) -> i64 {
        if a == 0 { return LANES as i64; }
        let l = lanes(a);
        (0..LANES).find(|&i| l[i] != 0).unwrap() as i64
    }
}

// ---------------------------------------------------------------------------
// Case generation
// ---------------------------------------------------------------------------

/// xorshift64*, seeded fixed. No `rand` dependency and no wall-clock seed:
/// this test must fail for everyone or no one.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    /// A word in [-T3_MAX, T3_MAX], biased WIDE — see the module comment: the
    /// defect this exists to catch is invisible below 8 bits.
    fn word(&mut self) -> i64 {
        let magnitude = (self.next() % (T3_MAX as u64 + 1)) as i64;
        if self.next() & 1 == 0 { magnitude } else { -magnitude }
    }
}

fn cases() -> Vec<(i64, i64)> {
    let mut v: Vec<(i64, i64)> = vec![
        // Edges first, and named, so a failure report is readable.
        (0, 0),
        (1, -1),
        (T3_MAX, T3_MAX),
        (-T3_MAX, T3_MAX),
        (T3_MAX, -T3_MAX),
        (-T3_MAX, -T3_MAX),
        (9841, -9841),   // all-+1 in nine lanes
        (256, -256),     // just past 8 bits: the P1 trap
        (255, 128),
        (121, 40),
    ];
    let mut r = Rng(0x5EED_1234_ABCD_0001);
    for _ in 0..120 {
        v.push((r.word(), r.word()));
    }
    v
}

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

fn manitc() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_manitc"))
}

fn temp_dir() -> PathBuf {
    let d = std::env::temp_dir().join(format!("manitc_diff_{}", std::process::id()));
    std::fs::create_dir_all(&d).expect("temp dir");
    d
}

/// Emit one ManiT program that prints every operation for every case, one
/// whitespace-separated record per line.
fn program(cases: &[(i64, i64)]) -> String {
    let mut s = String::from(
        "use std::io;\nuse std::trit;\nuse std::ternary;\n\n\
         fn row(a: int, b: int) {\n\
         \x20   io::print_int(a tandw b); io::print(\" \");\n\
         \x20   io::print_int(a torw b); io::print(\" \");\n\
         \x20   io::print_int(a txorw b); io::print(\" \");\n\
         \x20   io::print_int(a timpw b); io::print(\" \");\n\
         \x20   io::print_int(a tcmpw b); io::print(\" \");\n\
         \x20   io::print_int(tnotw a); io::print(\" \");\n\
         \x20   io::print_int(ternary::trit_to_int(trit::sign(a))); io::print(\" \");\n\
         \x20   io::print_int(trit::abs(a)); io::print(\" \");\n\
         \x20   io::print_int(trit::count(a, +)); io::print(\" \");\n\
         \x20   io::print_int(trit::count(a, 0)); io::print(\" \");\n\
         \x20   io::print_int(trit::count(a, -)); io::print(\" \");\n\
         \x20   io::print_int(trit::leading_zeros(a)); io::print(\" \");\n\
         \x20   io::println_int(trit::trailing_zeros(a));\n\
         }\n\n\
         fn main() {\n",
    );
    for (a, b) in cases {
        s.push_str(&format!("    row({}, {});\n", a, b));
    }
    s.push_str("}\n");
    s
}

fn expected(cases: &[(i64, i64)]) -> Vec<Vec<i64>> {
    use reference as r;
    cases
        .iter()
        .map(|&(a, b)| {
            vec![
                r::tandw(a, b), r::torw(a, b), r::txorw(a, b), r::timpw(a, b),
                r::tcmpw(a, b), r::tnotw(a),
                r::sign(a), r::abs(a),
                r::count(a, 1), r::count(a, 0), r::count(a, -1),
                r::leading_zeros(a), r::trailing_zeros(a),
            ]
        })
        .collect()
}

fn parse(out: &str) -> Vec<Vec<i64>> {
    out.lines()
        .filter(|l| !l.starts_with("[T3ISA]") && !l.trim().is_empty())
        .map(|l| l.split_whitespace().map(|t| t.parse().expect("integer")).collect())
        .collect()
}

fn run_t3(src: &PathBuf) -> Vec<Vec<i64>> {
    let base = src.with_extension("");
    let c = Command::new(manitc())
        .args(["compile", src.to_str().unwrap(), "--target", "t3",
               "-o", base.to_str().unwrap()])
        .output().expect("compile t3");
    assert!(c.status.success(), "T3 compile failed:\n{}",
            String::from_utf8_lossy(&c.stderr));
    let r = Command::new(manitc())
        .args(["run-t3", base.with_extension("t3b").to_str().unwrap()])
        .output().expect("run t3");
    parse(&String::from_utf8_lossy(&r.stdout))
}

/// `None` when clang is unavailable — the LLVM half is skipped rather than
/// failed, matching how `expected_output_tests.rs` handles the same problem.
fn run_llvm(src: &PathBuf) -> Option<Vec<Vec<i64>>> {
    let bin = src.with_extension("bin");
    let c = Command::new(manitc())
        .args(["compile", src.to_str().unwrap(), "--target", "llvm",
               "-o", bin.to_str().unwrap()])
        .output().expect("compile llvm");
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
    let r = Command::new(&bin).output().expect("run llvm");
    Some(parse(&String::from_utf8_lossy(&r.stdout)))
}

const OPS: &[&str] = &[
    "tandw", "torw", "txorw", "timpw", "tcmpw", "tnotw",
    "sign", "abs", "count(+)", "count(0)", "count(-)",
    "leading_zeros", "trailing_zeros",
];

#[test]
fn ternary_word_ops_agree_across_t3_llvm_and_the_reference() {
    let cases = cases();
    let src = temp_dir().join("diff.mt");
    std::fs::write(&src, program(&cases)).expect("write source");

    let want = expected(&cases);
    let t3 = run_t3(&src);
    assert_eq!(t3.len(), cases.len(), "T3 produced {} rows for {} cases",
               t3.len(), cases.len());

    let llvm = run_llvm(&src);
    if llvm.is_none() {
        eprintln!("note: clang unavailable — LLVM arm skipped, T3 vs reference only");
    }

    let mut failures = Vec::new();
    for (i, (a, b)) in cases.iter().enumerate() {
        for (j, op) in OPS.iter().enumerate() {
            let w = want[i][j];
            if t3[i][j] != w {
                failures.push(format!(
                    "{}({}, {}): T3 gave {}, reference says {}", op, a, b, t3[i][j], w));
            }
            if let Some(l) = &llvm {
                if l[i][j] != w {
                    failures.push(format!(
                        "{}({}, {}): LLVM gave {}, reference says {}", op, a, b, l[i][j], w));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} disagreement(s) over {} cases:\n{}",
        failures.len(), cases.len(),
        failures.iter().take(25).cloned().collect::<Vec<_>>().join("\n")
    );
}

/// Algebraic identities, checked against the reference alone.
///
/// These hold for reasons independent of any implementation, so they catch a
/// reference that is itself wrong — which the cross-backend test above cannot,
/// since it would simply agree with a consistently-wrong oracle.
#[test]
fn the_reference_satisfies_the_laws_the_specification_states() {
    let ones = reference::from_lanes(&[1i8; LANES]);
    assert_eq!(ones, T3_MAX, "the all-+1 word is T3_MAX");

    let mut r = Rng(0xC0FFEE_1234_5678);
    for _ in 0..500 {
        let (a, b) = (r.word(), r.word());

        // Lane decomposition round-trips.
        assert_eq!(reference::from_lanes(&reference::lanes(a)), a);

        // The deduction theorem, lane-wise: `a timpw a` is the all-+1 word for
        // EVERY a, including words with zero lanes. Under Kleene it would not be.
        assert_eq!(reference::timpw(a, a), ones, "a timpw a for a = {}", a);

        // min/max identities: the all-+1 word is the identity for lane-wise
        // min, the all--1 word for lane-wise max.
        assert_eq!(reference::tandw(a, ones), a);
        assert_eq!(reference::torw(a, -ones), a);

        // txorw takes THREE applications to recover, not two.
        let once = reference::txorw(a, b);
        let thrice = reference::txorw(reference::txorw(once, b), b);
        assert_eq!(thrice, a, "three applications recover: a = {}, k = {}", a, b);

        // The three lane counts partition all 27 lanes.
        assert_eq!(
            reference::count(a, 1) + reference::count(a, 0) + reference::count(a, -1),
            LANES as i64
        );

        // tnotw is an involution and negates every lane.
        assert_eq!(reference::tnotw(reference::tnotw(a)), a);
        assert_eq!(reference::sign(reference::abs(a)), if a == 0 { 0 } else { 1 });
    }
}
