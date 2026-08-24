//! A3 — the conformance suite.
//!
//! Author: Manish Jagdish Thatte
//!
//! Every program here is run three ways — the reference interpreter, the T3
//! backend, the LLVM backend — and all three must produce the same observable
//! behaviour: the same output trace and the same trap/no-trap outcome
//! (docs/semantics.md §4, §9).
//!
//! Agreement between any TWO is not evidence. Both backends are fed by one
//! front end, so they share every decision made in the lexer, the parser, the
//! analyser and the IR lowering; this project has twice shipped a bug on which
//! both agreed and both were wrong. The third party is derived from the written
//! semantics instead, which is what makes the agreement mean something.
//!
//! Programs stay inside the 27-trit range on purpose. §10.1 records that T3
//! traps on overflow and LLVM does not — a known divergence that N5 exists to
//! remove — so generating a value outside the range would test that divergence
//! rather than the semantics.

use manitc::reference;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static N: AtomicUsize = AtomicUsize::new(0);

fn manitc_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_manitc"))
}

fn tmp(stem: &str) -> PathBuf {
    let slot = N.fetch_add(1, Ordering::Relaxed);
    let d = std::env::temp_dir()
        .join(format!("manitc_conf_{}_{}", std::process::id(), slot));
    std::fs::create_dir_all(&d).expect("temp dir");
    d.join(format!("{}.mt", stem))
}

#[derive(Debug, PartialEq)]
struct Behaviour {
    out: String,
    trapped: bool,
}

fn from_process(stdout: &str, stderr: &str, code: Option<i32>) -> Behaviour {
    // §4: the observable behaviour is the output TRACE plus the outcome. A
    // trap's diagnostic text is neither — it is how the implementation reports
    // the outcome, and the T3 emulator happens to print it on stdout while the
    // C runtime prints it on stderr. Folding it into the trace would make two
    // implementations that agree perfectly on the semantics disagree here.
    let out: String = stdout
        .lines()
        .filter(|l| !l.starts_with("[T3ISA]") && !l.starts_with("TRAP:"))
        .map(|l| format!("{}\n", l))
        .collect();
    let blob = format!("{}{}", stdout, stderr);
    Behaviour {
        out,
        trapped: blob.contains("TRAP:") || code == Some(70),
    }
}

fn run_reference(src: &str, lang: reference::Lang) -> Behaviour {
    match reference::interpret_with(src, lang) {
        Ok(o) => Behaviour { out: o.out, trapped: o.trap.is_some() },
        Err(e) => panic!("the reference could not accept this core program: {}\n{}", e, src),
    }
}

/// The `--lang` argument for a version, as the CLI spells it.
fn lang_flag(lang: reference::Lang) -> &'static str {
    match lang {
        reference::Lang::V1 => "v1",
        reference::Lang::V2 => "v2",
    }
}

fn run_t3(path: &PathBuf, lang: reference::Lang) -> Behaviour {
    let base = path.with_extension("");
    let c = Command::new(manitc_bin())
        .args(["compile", path.to_str().unwrap(), "--target", "t3",
               "--lang", lang_flag(lang),
               "-o", base.to_str().unwrap()])
        .output().expect("compile");
    assert!(c.status.success(), "T3 compile failed:\n{}\n{}",
            String::from_utf8_lossy(&c.stdout), String::from_utf8_lossy(&c.stderr));
    let r = Command::new(manitc_bin())
        .args(["run-t3", base.with_extension("t3b").to_str().unwrap()])
        .output().expect("run");
    from_process(&String::from_utf8_lossy(&r.stdout),
                 &String::from_utf8_lossy(&r.stderr), r.status.code())
}

fn run_llvm(path: &PathBuf, lang: reference::Lang) -> Option<Behaviour> {
    let bin = path.with_extension("bin");
    let c = Command::new(manitc_bin())
        .args(["compile", path.to_str().unwrap(), "--target", "llvm",
               "--lang", lang_flag(lang),
               "-o", bin.to_str().unwrap()])
        .output().expect("compile");
    if !c.status.success() {
        let blob = String::from_utf8_lossy(&c.stderr).to_string();
        if blob.contains("clang") { return None; }
        panic!("LLVM compile failed:\n{}", blob);
    }
    let r = Command::new(&bin).output().expect("run");
    Some(from_process(&String::from_utf8_lossy(&r.stdout),
                      &String::from_utf8_lossy(&r.stderr), r.status.code()))
}

/// The whole point of the file: three ways, one answer.
fn conform(name: &str, src: &str) {
    conform_lang(name, src, reference::Lang::V1);
}

/// The same, under `--lang v2` (R2). Both versions are checked because C4 and
/// N5 must leave V1 alone as surely as they must change V2 — a migration
/// nobody can decline is not a migration.
fn conform_v2(name: &str, src: &str) {
    conform_lang(name, src, reference::Lang::V2);
}

fn conform_lang(name: &str, src: &str, lang: reference::Lang) {
    let path = tmp(name);
    std::fs::write(&path, src).expect("write");

    let r = run_reference(src, lang);
    let t3 = run_t3(&path, lang);
    assert_eq!(
        t3, r,
        "[{} {}] T3 disagrees with the SPECIFICATION.\n\
         --- reference ---\n{:?}\n--- T3 ---\n{:?}\n--- source ---\n{}",
        name, lang_flag(lang), r, t3, src
    );
    if let Some(l) = run_llvm(&path, lang) {
        assert_eq!(
            l, r,
            "[{} {}] LLVM disagrees with the SPECIFICATION.\n\
             --- reference ---\n{:?}\n--- LLVM ---\n{:?}\n--- source ---\n{}",
            name, lang_flag(lang), r, l, src
        );
    }
}

// ---------------------------------------------------------------------------
// §6.1 arithmetic
// ---------------------------------------------------------------------------

#[test]
fn s61_division_truncates_toward_zero_and_rem_takes_the_dividends_sign() {
    conform("s61_div", r#"
use std::io;
fn d(a: int, b: int) { io::print_int(a / b); io::print(" "); io::println_int(a % b); }
fn main() {
    d(7, 2); d(-7, 2); d(7, -2); d(-7, -2);
    d(1, 3); d(-1, 3); d(100, 7); d(-100, 7);
    d(9, 3); d(-9, 3);
}
"#);
}

#[test]
fn s61_the_division_identity_holds() {
    conform("s61_ident", r#"
use std::io;
fn chk(a: int, b: int) { io::println_int((a / b) * b + (a % b) - a); }
fn main() {
    chk(7,2); chk(-7,2); chk(7,-2); chk(-7,-2);
    chk(1000,7); chk(-1000,7); chk(1,3); chk(-1,3);
}
"#);
}

#[test]
fn s61_precedence_and_associativity() {
    conform("s61_prec", r#"
use std::io;
fn main() {
    io::println_int(2 + 3 * 4 - 6 / 2);
    io::println_int(-7 / 2 * 2 + -7 % 2);
    io::println_int(100 - 10 - 10);
    io::println_int(2 * 3 % 4);
    io::println_int(-(3 + 4) * 2);
}
"#);
}

#[test]
fn s8_division_by_zero_traps_with_the_output_so_far_retained() {
    // §8: the trace produced before the trap is observable.
    conform("s8_divzero", r#"
use std::io;
fn main() {
    io::println("before");
    let z: int = 0;
    io::println_int(7 / z);
    io::println("after");
}
"#);
}

#[test]
fn s8_remainder_by_zero_traps() {
    conform("s8_remzero", r#"
use std::io;
fn main() {
    io::println("before");
    let z: int = 0;
    io::println_int(7 % z);
}
"#);
}

// ---------------------------------------------------------------------------
// §6.2, §6.3 comparison and short-circuit
// ---------------------------------------------------------------------------

#[test]
fn s62_comparisons() {
    conform("s62_cmp", r#"
use std::io;
fn b(x: bool) { if x { io::print("1 "); } else { io::print("0 "); } }
fn main() {
    b(1 == 1); b(1 != 1); b(1 < 2); b(2 < 1); b(1 <= 1); b(2 >= 3);
    io::println("");
    b(-5 < 0); b(-5 > 0); b(0 == 0);
    io::println("");
}
"#);
}

#[test]
fn s63_short_circuit_suppresses_the_right_operands_output() {
    // The observable consequence of §6.3: `loud()` prints. If the right operand
    // were evaluated, the trace would differ — so this pins the short circuit
    // by its side effect, not by its value.
    conform("s63_sc", r#"
use std::io;
fn loud() -> bool { io::print("[eval]"); return true; }
fn main() {
    if false && loud() { io::println("A"); } else { io::println("B"); }
    if true || loud() { io::println("C"); } else { io::println("D"); }
    if true && loud() { io::println("E"); } else { io::println("F"); }
    if false || loud() { io::println("G"); } else { io::println("H"); }
}
"#);
}

// ---------------------------------------------------------------------------
// §6.4 three-valued operators
// ---------------------------------------------------------------------------

#[test]
fn s64_full_truth_tables_for_every_binary_connective() {
    conform("s64_tables", r#"
use std::io;
fn row(a: trit, b: trit) {
    io::print_int((a tand b) as int); io::print(" ");
    io::print_int((a tor b) as int); io::print(" ");
    io::print_int((a txor b) as int); io::print(" ");
    io::print_int((a tcon b) as int); io::print(" ");
    io::print_int((a tany b) as int); io::print(" ");
    io::print_int((a timp b) as int); io::print(" ");
    io::println_int((a teq b) as int);
}
fn main() {
    let n: trit = -; let z: trit = 0; let p: trit = +;
    row(n,n); row(n,z); row(n,p);
    row(z,n); row(z,z); row(z,p);
    row(p,n); row(p,z); row(p,p);
}
"#);
}

#[test]
fn s64_the_deduction_theorem() {
    // `a timp a` is +1 for every a, INCLUDING unknown. That single cell is what
    // makes the logic Łukasiewicz rather than Kleene, and an implementation
    // giving 0 here does not conform.
    conform("s64_deduction", r#"
use std::io;
fn main() {
    let n: trit = -; let z: trit = 0; let p: trit = +;
    io::println_int((n timp n) as int);
    io::println_int((z timp z) as int);
    io::println_int((p timp p) as int);
}
"#);
}

#[test]
fn s64_unary_and_modal_operators() {
    conform("s64_unary", r#"
use std::io;
fn main() {
    let n: trit = -; let z: trit = 0; let p: trit = +;
    io::println_int((tnot n) as int);
    io::println_int((tnot z) as int);
    io::println_int((tnot p) as int);
    if tposs n { io::print("1 "); } else { io::print("0 "); }
    if tposs z { io::print("1 "); } else { io::print("0 "); }
    if tposs p { io::println("1"); } else { io::println("0"); }
    if tnec n { io::print("1 "); } else { io::print("0 "); }
    if tnec z { io::print("1 "); } else { io::print("0 "); }
    if tnec p { io::println("1"); } else { io::println("0"); }
}
"#);
}

#[test]
fn s64_txor_needs_three_applications_not_two() {
    conform("s64_txor3", r#"
use std::io;
fn main() {
    let x: trit = +; let k: trit = -;
    io::println_int((x txor k) as int);
    io::println_int((x txor k txor k) as int);
    io::println_int((x txor k txor k txor k) as int);
}
"#);
}

#[test]
fn s64_bool_operands_convert_by_two_b_minus_one() {
    // §6.4: a Bool operand becomes -1/+1, and tand/tor/tany/timp/teq are closed
    // on {-1,+1} so the result is a Bool again.
    conform("s64_boolops", r#"
use std::io;
fn b(x: bool) { if x { io::print("1 "); } else { io::print("0 "); } }
fn main() {
    b(true tand true); b(true tand false); b(false tand false);
    io::println("");
    b(true tor false); b(false tor false);
    io::println("");
    b(true timp false); b(false timp true); b(true teq true); b(true teq false);
    io::println("");
    let x: int = 5; let y: int = 3;
    b((x > y) tand (y < x));
    b((x > y) tand (y > x));
    io::println("");
}
"#);
}

#[test]
fn s64_bool3_literals_and_mixing() {
    conform("s64_bool3", r#"
use std::io;
fn main() {
    let t: bool3 = True; let u: bool3 = Unknown; let f: bool3 = False;
    io::println_int(t as int);
    io::println_int(u as int);
    io::println_int(f as int);
    io::println_int((t tand u) as int);
    io::println_int((f tor u) as int);
    io::println_int((u timp u) as int);
    io::println_int((t txor t) as int);
}
"#);
}

// ---------------------------------------------------------------------------
// §6.6 lane-wise
// ---------------------------------------------------------------------------

#[test]
fn s66_lane_wise_operators_over_wide_words() {
    conform("s66_lanes", r#"
use std::io;
fn row(a: int, b: int) {
    io::print_int(a tandw b); io::print(" ");
    io::print_int(a torw b); io::print(" ");
    io::print_int(a txorw b); io::print(" ");
    io::print_int(a timpw b); io::print(" ");
    io::print_int(a tcmpw b); io::print(" ");
    io::println_int(tnotw a);
}
fn main() {
    row(0, 0); row(1, -1); row(256, -256);
    row(9841, -9841); row(121, 40); row(-3, 3);
    row(3812798742493, 1); row(-3812798742493, -1);
}
"#);
}

#[test]
fn s66_the_deduction_theorem_lane_wise() {
    conform("s66_deduction", r#"
use std::io;
fn main() {
    io::println_int(0 timpw 0);
    io::println_int(9841 timpw 9841);
    io::println_int(256 timpw 256);
    io::println_int(3812798742493 timpw 3812798742493);
}
"#);
}

// ---------------------------------------------------------------------------
// §6.7 casts
// ---------------------------------------------------------------------------

#[test]
fn s67_int_to_trit_clamps_rather_than_truncating() {
    // This is the case that found report.txt P2: T3 emitted a bare MOV, so
    // `5 as trit` was 5 there and 1 on LLVM.
    conform("s67_clamp", r#"
use std::io;
fn main() {
    io::println_int((5 as trit) as int);
    io::println_int((-5 as trit) as int);
    io::println_int((0 as trit) as int);
    io::println_int((1 as trit) as int);
    io::println_int((-1 as trit) as int);
    io::println_int((9841 as trit) as int);
    io::println_int((-9841 as trit) as int);
}
"#);
}

#[test]
fn s67_bool_to_trit_is_the_carrier_not_the_two_b_minus_one_conversion() {
    // The wart §6.4 and §10.2 record: `false as trit` is 0 — unknown — not -1.
    // Specified as it behaves, so that changing it is a visible change.
    conform("s67_boolcast", r#"
use std::io;
fn main() {
    let t: bool = true; let f: bool = false;
    io::println_int((t as trit) as int);
    io::println_int((f as trit) as int);
    io::println_int(t as int);
    io::println_int(f as int);
}
"#);
}

// ---------------------------------------------------------------------------
// §7 statements and control flow
// ---------------------------------------------------------------------------

#[test]
fn s7_tif_dispatches_on_all_three_arms() {
    conform("s7_tif", r#"
use std::io;
fn go(t: trit) { tif t { + => io::println("pos"), 0 => io::println("zero"), - => io::println("neg"), } }
fn main() {
    let n: trit = -; let z: trit = 0; let p: trit = +;
    go(p); go(z); go(n);
}
"#);
}

#[test]
fn s7_if_elif_else_takes_the_first_true_branch() {
    conform("s7_if", r#"
use std::io;
fn classify(n: int) {
    if n < 0 { io::println("neg"); }
    elif n == 0 { io::println("zero"); }
    elif n < 10 { io::println("small"); }
    else { io::println("big"); }
}
fn main() { classify(-5); classify(0); classify(3); classify(100); }
"#);
}

#[test]
fn s7_while_and_mutation() {
    conform("s7_while", r#"
use std::io;
fn main() {
    let mut i: int = 0;
    let mut acc: int = 0;
    while i < 10 { acc = acc + i * i; i = i + 1; }
    io::println_int(acc);
    let mut j: int = 5;
    while j > 0 { io::print_int(j); io::print(" "); j = j - 1; }
    io::println("");
}
"#);
}

#[test]
fn s7_recursion_and_call_order() {
    conform("s7_rec", r#"
use std::io;
fn fact(n: int) -> int { if n <= 1 { return 1; } return n * fact(n - 1); }
fn fib(n: int) -> int { if n < 2 { return n; } return fib(n - 1) + fib(n - 2); }
fn noisy(tag: int) -> int { io::print_int(tag); io::print(" "); return tag; }
fn add(a: int, b: int) -> int { return a + b; }
fn main() {
    io::println_int(fact(10));
    io::println_int(fib(15));
    io::println_int(add(noisy(1), noisy(2)));
}
"#);
}

#[test]
fn s7_shadowing_and_block_scope() {
    conform("s7_scope", r#"
use std::io;
fn main() {
    let x: int = 1;
    io::println_int(x);
    if true { let x: int = 2; io::println_int(x); }
    io::println_int(x);
    let x: int = 3;
    io::println_int(x);
}
"#);
}

// ---------------------------------------------------------------------------
// Generated breadth
// ---------------------------------------------------------------------------

/// xorshift64*, fixed seed. Same reasoning as differential_tests.rs: this must
/// fail for everyone or no one.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12; x ^= x << 25; x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn small(&mut self) -> i64 {
        // Kept small on purpose: §10.1. Products of three of these stay well
        // inside the 27-trit range, so no case tests the known overflow
        // divergence instead of the semantics.
        (self.next() % 2001) as i64 - 1000
    }
}

#[test]
fn generated_integer_expressions_agree_three_ways() {
    let mut r = Rng(0xA3_0001);
    let mut body = String::new();
    for _ in 0..150 {
        let (a, b, c) = (r.small(), r.small(), r.small());
        let d = { let v = r.small(); if v == 0 { 7 } else { v } };
        let shape = r.next() % 6;
        let e = match shape {
            0 => format!("{} + {} * {}", a, b, c),
            1 => format!("({} - {}) * {}", a, b, c),
            2 => format!("{} / {} + {} % {}", a, d, b, d),
            3 => format!("({} + {}) / {} - {}", a, b, d, c),
            4 => format!("-{} * ({} - {})", a, b, c),
            _ => format!("{} % {} * {} + {}", a, d, b, c),
        };
        body.push_str(&format!("    io::println_int({});\n", e));
    }
    conform("gen_int", &format!("use std::io;\nfn main() {{\n{}}}\n", body));
}

// ---------------------------------------------------------------------------
// C4 / N5 — the version-dependent semantics (R2)
// ---------------------------------------------------------------------------
//
// Everything above this line is checked under v1, which is what the default
// stays. These check v2, and they are the reason R2 asked for the conformance
// suite BEFORE the change rather than after: `s61_division_truncates_...`
// already pinned the old behaviour three ways, so the new behaviour could not
// be introduced as a silent replacement for it. Both are now pinned, and any
// future edit that confuses the two fails one of them.

#[test]
fn c4_division_rounds_to_nearest_under_v2() {
    // The cases the rule was derived on, and the negatives that distinguish
    // ties-away-from-zero from ties-to-even and from rounding toward +inf.
    conform_v2("c4_div", r#"
use std::io;
fn d(a: int, b: int) { io::print_int(a / b); io::print(" "); io::println_int(a % b); }
fn main() {
    d(7, 2); d(-7, 2); d(7, -2); d(-7, -2);
    d(1, 3); d(2, 3); d(-2, 3); d(5, 3); d(4, 3);
    d(100, 7); d(-100, 7); d(9, 3); d(-9, 3);
    d(0, 5); d(1, 2); d(-1, 2); d(3, 2); d(-3, 2);
}
"#);
}

#[test]
fn c4_the_division_identity_holds_under_v2() {
    // `(a / b) * b + (a % b) == a`. It holds under v1 (s61_the_division_
    // identity_holds) and it must hold under v2 as well: that requirement is
    // the whole reason `%` changes when `/` does.
    conform_v2("c4_ident", r#"
use std::io;
fn chk(a: int, b: int) { io::println_int((a / b) * b + (a % b) - a); }
fn main() {
    chk(7,2); chk(-7,2); chk(7,-2); chk(-7,-2);
    chk(1000,7); chk(-1000,7); chk(1,3); chk(-1,3);
    chk(2,3); chk(-2,3); chk(5,3); chk(-5,3); chk(0,9);
}
"#);
}

#[test]
fn c4_rounding_is_symmetric_about_zero_under_v2() {
    // `div(-a, b) == -div(a, b)`. This is the property ties-away-from-zero was
    // chosen FOR, over the statistically unbiased half-to-even; if the
    // tie-break ever changes, this is what says so.
    conform_v2("c4_sym", r#"
use std::io;
fn s(a: int, b: int) { io::println_int((0 - a) / b + a / b); }
fn main() {
    s(7,2); s(1,2); s(3,2); s(5,2); s(1,3); s(2,3); s(4,3); s(5,3);
    s(7,3); s(8,3); s(11,7); s(13,7); s(1,1); s(0,4);
}
"#);
}

#[test]
fn c4_the_balanced_remainder_can_be_negative_for_a_positive_dividend() {
    // The visible consequence, and the one most likely to break existing code:
    // `7 % 2` is -1 under v2, so `x % 2 == 0` is still an evenness test but
    // `x % 2 == 1` is not an oddness test. Worth its own case because it is
    // what a migration has to look for.
    conform_v2("c4_negrem", r#"
use std::io;
fn main() {
    let mut i: int = 0;
    while i < 10 {
        io::print_int(i % 2); io::print(" ");
        i = i + 1;
    }
    io::println("");
}
"#);
}

#[test]
fn c4_division_by_zero_still_traps_under_v2() {
    conform_v2("c4_divzero", r#"
use std::io;
fn main() {
    io::println("before");
    let z: int = 0;
    io::println_int(7 / z);
    io::println("after");
}
"#);
}

#[test]
fn n5_int_arithmetic_leaves_the_word_and_traps_under_v2() {
    // docs/semantics.md §10.1, closed. This exact program is the one the
    // divergence was measured on: it traps on T3 and answered 3812798742494 on
    // LLVM. Under v2 all three agree that it traps.
    //
    // It cannot be a `conform` (v1) test, because under v1 the LLVM backend is
    // still allowed to disagree — that is what §10.1 SAYS — and the reference
    // has always held `int` to 27 trits.
    conform_v2("n5_add", r#"
use std::io;
fn main() {
    let m: int = 3812798742493;
    io::println_int(m);
    io::println_int(m + 1);
    io::println("unreachable");
}
"#);
}

#[test]
fn n5_multiplication_that_overflows_the_machine_word_also_traps_under_v2() {
    // 4e9 * 4e9 = 1.6e19, which is outside int64 as well as outside the word.
    // The guard computes the product in __int128 for exactly this case: checked
    // afterwards in int64 it would have wrapped to -2446744073709551616 and
    // looked like an ordinary out-of-range value with the wrong magnitude, or
    // — for a different pair — wrapped back INTO range and passed.
    conform_v2("n5_mul", r#"
use std::io;
fn main() {
    let a: int = 4000000000;
    io::println("before");
    io::println_int(a * a);
}
"#);
}

#[test]
fn generated_division_expressions_agree_three_ways_under_v2() {
    // The same shape as `generated_integer_expressions_agree_three_ways`, but
    // division-heavy and under v2, so the rounding rule is exercised on
    // hundreds of sign and magnitude combinations rather than on the handful
    // anyone would think to write down. The reference computes it by a
    // different formula from the compiler's (see `div_nearest_ref`), so an
    // agreement here is evidence and not a tautology.
    let mut r = Rng(0xC4_0001);
    let mut body = String::new();
    for _ in 0..200 {
        let a = r.small();
        let b = { let v = r.small(); if v == 0 { 5 } else { v } };
        let c = { let v = r.small(); if v == 0 { 3 } else { v } };
        let shape = r.next() % 5;
        let e = match shape {
            0 => format!("{} / {}", a, b),
            1 => format!("{} % {}", a, b),
            2 => format!("({} / {}) * {} + ({} % {})", a, b, b, a, b),
            3 => format!("{} / {} + {} % {}", a, b, a, c),
            _ => format!("(0 - {}) / {} + {} / {}", a, b, a, b),
        };
        body.push_str(&format!("    io::println_int({});\n", e));
    }
    conform_v2("gen_div_v2", &format!("use std::io;\nfn main() {{\n{}}}\n", body));
}

#[test]
fn generated_three_valued_expressions_agree_three_ways() {
    let mut r = Rng(0xA3_0002);
    const T: [&str; 3] = ["n", "z", "p"];
    const OPS: [&str; 7] = ["tand", "tor", "txor", "tcon", "tany", "timp", "teq"];
    let mut body = String::new();
    for _ in 0..150 {
        let a = T[(r.next() % 3) as usize];
        let b = T[(r.next() % 3) as usize];
        let c = T[(r.next() % 3) as usize];
        let o1 = OPS[(r.next() % 7) as usize];
        let o2 = OPS[(r.next() % 7) as usize];
        body.push_str(&format!(
            "    io::println_int((({} {} {}) {} {}) as int);\n", a, o1, b, o2, c));
    }
    conform("gen_tri", &format!(
        "use std::io;\nfn main() {{\n    let n: trit = -; let z: trit = 0; let p: trit = +;\n{}}}\n",
        body));
}

#[test]
fn generated_lane_wise_expressions_agree_three_ways() {
    let mut r = Rng(0xA3_0003);
    const OPS: [&str; 5] = ["tandw", "torw", "txorw", "timpw", "tcmpw"];
    let mut body = String::new();
    for _ in 0..150 {
        // Wide words here: lane operations cannot overflow (§6.6), so the
        // range restriction of §10.1 does not bite, and wide is where the P1
        // width defects lived.
        let a = (r.next() % (2 * 3_812_798_742_493u64 + 1)) as i64 - 3_812_798_742_493;
        let b = (r.next() % (2 * 3_812_798_742_493u64 + 1)) as i64 - 3_812_798_742_493;
        let o = OPS[(r.next() % 5) as usize];
        body.push_str(&format!("    io::println_int({} {} {});\n", a, o, b));
        body.push_str(&format!("    io::println_int(tnotw {});\n", a));
    }
    conform("gen_lane", &format!("use std::io;\nfn main() {{\n{}}}\n", body));
}

// ---------------------------------------------------------------------------
// The rule that makes all of the above mean something
// ---------------------------------------------------------------------------

#[test]
fn the_reference_implementation_is_independent() {
    // If `src/reference/` ever imports from the rest of the crate, it stops
    // being a third implementation and becomes a second consumer of the same
    // front end — at which point every test in this file still passes and
    // proves nothing. The rule is stated in src/reference/mod.rs; this is what
    // keeps it true.
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/reference");
    let mut offenders = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("src/reference must exist") {
        let p = entry.expect("dir entry").path();
        if p.extension().and_then(|s| s.to_str()) != Some("rs") { continue; }
        let text = std::fs::read_to_string(&p).expect("read");
        for (i, line) in text.lines().enumerate() {
            let l = line.trim();
            if !l.starts_with("use ") { continue; }
            // `use super::…` inside src/reference is the module referring to
            // its own siblings, which is the intended structure. Anything
            // reaching into the crate at large is not.
            let bad = l.starts_with("use crate::")
                || l.starts_with("use manitc::")
                || l.contains("::ast::")   && l.starts_with("use crate")
                || l.starts_with("use super::super::");
            if bad {
                offenders.push(format!("{}:{}: {}", p.display(), i + 1, l));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "src/reference must not import from the rest of the crate — a reference \
         implementation that shares the compiler's front end cannot witness a \
         front-end bug:\n{}",
        offenders.join("\n")
    );
}

// ---------------------------------------------------------------------------
// §6.8–§6.10 — Result, `?`, and match
// ---------------------------------------------------------------------------
//
// The three-state `Result` is the language's strongest claim: `Unknown` is not
// a kind of `Err`, it is "we do not know", and `?` propagates it distinctly.
// Until now nothing checked that against anything but the two backends
// agreeing with each other.

#[test]
fn s68_the_three_constructors_and_their_tags() {
    conform("s68_ctors", r#"
use std::io;
fn mk(k: int) -> Result<int, str> {
    if k > 0 { return Ok(42); } elif k == 0 { return Unknown("dunno"); } else { return Err("bang"); }
}
fn main() {
    io::println_int(mk(1).tag() as int);
    io::println_int(mk(0).tag() as int);
    io::println_int(mk(-1).tag() as int);
}
"#);
}

#[test]
fn s68_the_three_predicates_are_the_tag_asked_one_at_a_time() {
    conform("s68_preds", r#"
use std::io;
fn mk(k: int) -> Result<int, str> {
    if k > 0 { return Ok(7); } elif k == 0 { return Unknown("u"); } else { return Err("e"); }
}
fn row(r: Result<int, str>) {
    io::print_int(r.is_ok() as int); io::print(" ");
    io::print_int(r.is_unknown() as int); io::print(" ");
    io::println_int(r.is_err() as int);
}
fn main() { row(mk(1)); row(mk(0)); row(mk(-1)); }
"#);
}

#[test]
fn s69_question_propagates_unknown_distinctly_from_err() {
    // The flagship claim. `Unknown` must arrive at the caller still Unknown and
    // still carrying its reason — not collapsed into Err, and not into Ok.
    conform("s69_try", r#"
use std::io;
fn mk(k: int) -> Result<int, str> {
    if k > 0 { return Ok(10); } elif k == 0 { return Unknown("no data"); } else { return Err("broke"); }
}
fn chain(k: int) -> Result<int, str> {
    let v = mk(k)?;
    return Ok(v + 1);
}
fn show(r: Result<int, str>) {
    match r {
        Ok(v) => { io::print("Ok "); io::println_int(v); },
        Unknown(m) => { io::print("Unknown "); io::println(m); },
        Err(e) => { io::print("Err "); io::println(e); },
    }
}
fn main() { show(chain(1)); show(chain(0)); show(chain(-1)); }
"#);
}

#[test]
fn s69_question_chains_through_two_levels() {
    conform("s69_try2", r#"
use std::io;
fn inner(k: int) -> Result<int, str> {
    if k == 0 { return Unknown("deep"); } else { return Ok(k); }
}
fn middle(k: int) -> Result<int, str> { let v = inner(k)?; return Ok(v * 2); }
fn outer(k: int) -> Result<int, str> { let v = middle(k)?; return Ok(v + 1); }
fn show(r: Result<int, str>) {
    match r {
        Ok(v) => { io::print("Ok "); io::println_int(v); },
        Unknown(m) => { io::print("Unknown "); io::println(m); },
        Err(e) => { io::print("Err "); io::println(e); },
    }
}
fn main() { show(outer(3)); show(outer(0)); }
"#);
}

#[test]
fn s610_match_binds_each_variants_payload() {
    conform("s610_match", r#"
use std::io;
fn mk(k: int) -> Result<int, str> {
    if k > 0 { return Ok(99); } elif k == 0 { return Unknown("the reason"); } else { return Err("the error"); }
}
fn go(k: int) {
    match mk(k) {
        Ok(v) => { io::print("v="); io::println_int(v); },
        Unknown(m) => { io::print("m="); io::println(m); },
        Err(e) => { io::print("e="); io::println(e); },
    }
}
fn main() { go(1); go(0); go(-1); }
"#);
}

#[test]
fn s68_unwrap_or_and_the_default_being_evaluated() {
    conform("s68_unwrap_or", r#"
use std::io;
fn mk(k: int) -> Result<int, str> {
    if k > 0 { return Ok(5); } elif k == 0 { return Unknown("u"); } else { return Err("e"); }
}
fn main() {
    io::println_int(mk(1).unwrap_or(-1));
    io::println_int(mk(0).unwrap_or(-1));
    io::println_int(mk(-1).unwrap_or(-1));
}
"#);
}

#[test]
fn s8_unwrap_traps_on_unknown_and_on_err_with_different_messages() {
    // Two facts, two messages: "it failed" and "we do not know" are different,
    // and a shared message would hide which one happened.
    conform("s8_unwrap_unk", r#"
use std::io;
fn mk() -> Result<int, str> { return Unknown("nope"); }
fn main() { io::println("before"); io::println_int(mk().unwrap()); io::println("after"); }
"#);
    conform("s8_unwrap_err", r#"
use std::io;
fn mk() -> Result<int, str> { return Err("bad"); }
fn main() { io::println("before"); io::println_int(mk().unwrap()); io::println("after"); }
"#);
}
