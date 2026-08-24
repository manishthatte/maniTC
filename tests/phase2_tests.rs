//! Phase 2 — "buy the distinctiveness". A2's availability inference.
//!
//! Author: Manish Jagdish Thatte
//!
//! C1, C2 and C7 are behavioural and are tested where behaviour is tested:
//! `tests/30_lanewise.mt` and `tests/31_trit_intrinsics.mt`, each registered as
//! both an expected-output and a cross-target test. A2 produces no output — it
//! decides whether a program compiles at all — so it is tested here instead.

use std::path::PathBuf;
use std::process::Command;

fn manitc() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_manitc"))
}

fn temp_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("manitc_phase2_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("failed to create temp dir");
    dir
}

/// Run `manitc <sub> <file> <extra...>`; returns (ok, stdout+stderr).
fn run(sub: &str, name: &str, source: &str, extra: &[&str]) -> (bool, String) {
    let path = temp_dir().join(name);
    std::fs::write(&path, source).expect("failed to write test source");
    let mut args: Vec<String> = vec![sub.into(), path.display().to_string()];
    if sub == "compile" {
        args.push("-o".into());
        args.push(path.with_extension("out").display().to_string());
    }
    args.extend(extra.iter().map(|s| s.to_string()));
    let out = Command::new(manitc())
        .args(&args)
        .output()
        .expect("failed to run manitc");
    let mut blob = String::from_utf8_lossy(&out.stdout).to_string();
    blob.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.success(), blob)
}

const GUI: &str = r#"
extern "c" fn gui::set_color(r: int, g: int, b: int) -> void
    available(llvm);
fn paint() { gui::set_color(1, 2, 3); }
fn draw_frame() { paint(); }
fn main() { draw_frame(); }
"#;

#[test]
fn a2_an_unavailable_call_three_levels_down_fails_the_build() {
    // The whole point of A2: this is a static property of the call graph, and
    // before it the only way to find out was to compile, run, and diff against
    // the other backend.
    let (ok, t3) = run("compile", "a2_chain_t3.mt", GUI, &["--target", "t3"]);
    assert!(!ok, "t3 build must fail:\n{}", t3);
    assert!(
        t3.contains("main -> draw_frame -> paint -> gui::set_color"),
        "the diagnostic must name the CHAIN, not just the endpoint:\n{}",
        t3
    );
    assert!(t3.contains("declared available only on: llvm"), "{}", t3);
}

#[test]
fn a2_the_same_program_still_builds_for_the_backend_it_is_available_on() {
    let (ok, llvm) = run("compile", "a2_chain_llvm.mt", GUI, &["--target", "llvm"]);
    assert!(ok, "llvm build must succeed:\n{}", llvm);
}

#[test]
fn a2_check_stays_backend_agnostic() {
    // `manitc check` picks no backend, so it cannot have an availability
    // question to answer. Reporting one here would answer a question nobody
    // posed — the same rule A1 step 3 follows.
    let (ok, out) = run("check", "a2_agnostic.mt", GUI, &[]);
    assert!(ok, "{}", out);
    assert!(!out.contains("cannot be compiled"), "{}", out);
}

#[test]
fn a2_reports_the_outermost_offender_once_not_every_link() {
    // One unavailable extern makes every function above it unavailable too.
    // Reporting all of them means N copies of one fact; the outermost one's
    // chain already names every hop including the culprit.
    let (_, t3) = run("compile", "a2_once.mt", GUI, &["--target", "t3"]);
    let n = t3.matches("cannot be compiled for the t3 backend").count();
    assert_eq!(n, 1, "expected exactly one chain diagnostic, got {}:\n{}", n, t3);
}

#[test]
fn a2_availability_is_the_meet_over_a_recursive_cycle() {
    // The specification's one subtlety: recursion needs no separate SCC pass
    // because the fixpoint settles at the meet over the cycle. `ping` never
    // names the extern — only `pong` does — and both must come out unavailable.
    let src = r#"
extern "c" fn gui::flush() -> void available(llvm);
fn ping(n: int) { if n > 0 { pong(n - 1); } }
fn pong(n: int) { gui::flush(); if n > 0 { ping(n - 1); } }
fn main() { ping(3); }
"#;
    let (ok, t3) = run("compile", "a2_cycle.mt", src, &["--target", "t3"]);
    assert!(!ok, "a cycle must not hide the unavailability:\n{}", t3);
    assert!(t3.contains("ping"), "the chain must pass through the cycle:\n{}", t3);
    assert!(t3.contains("gui::flush"), "{}", t3);
}

#[test]
fn a2_ordinary_recursion_is_not_a_false_positive() {
    let src = r#"
use std::io;
fn fact(n: int) -> int { if n <= 1 { return 1; } return n * fact(n - 1); }
fn main() { io::println_int(fact(5)); }
"#;
    let (ok, t3) = run("compile", "a2_fact.mt", src, &["--target", "t3"]);
    assert!(ok, "a self-recursive function calls nothing unavailable:\n{}", t3);
}

#[test]
fn a2_a_written_clause_is_checked_against_the_body() {
    // The assertion half. `render` claims t3 and reaches an llvm-only extern,
    // so the claim is false — and it is false regardless of which backend is
    // being compiled for, so `check` reports it.
    let src = r#"
extern "c" fn gui::flush() -> void available(llvm);
fn render() available(llvm, t3) { gui::flush(); }
fn main() { render(); }
"#;
    let (ok, out) = run("check", "a2_assert_bad.mt", src, &[]);
    assert!(!ok, "a contradicted assertion must fail:\n{}", out);
    assert!(
        out.contains("declares `available(t3)` but cannot run there"),
        "{}",
        out
    );
}

#[test]
fn a2_a_written_clause_that_holds_is_silent() {
    let src = r#"
use std::io;
extern "c" fn gui::flush() -> void available(llvm);
fn render() available(llvm) { gui::flush(); }
fn safe() available(llvm, t3) { io::println("ok"); }
fn main() { safe(); }
"#;
    let (ok, out) = run("check", "a2_assert_ok.mt", src, &[]);
    assert!(ok, "an assertion the body satisfies must pass:\n{}", out);
}

#[test]
fn a2_a_written_clause_constrains_callers() {
    // A function declared llvm-only makes its callers llvm-only, with no
    // extern involved anywhere. This failed silently at first: the search for
    // a witness chain only accepted an `extern` as a terminator, so the
    // constraint was inferred and then dropped on the way to the diagnostic.
    let src = r#"
use std::io;
fn only_llvm() available(llvm) { io::println("x"); }
fn wrapper() { only_llvm(); }
fn main() { wrapper(); }
"#;
    let (ok, t3) = run("compile", "a2_written_chain.mt", src, &["--target", "t3"]);
    assert!(!ok, "{}", t3);
    assert!(
        t3.contains("main -> wrapper -> only_llvm"),
        "the chain must name the written clause as the cause:\n{}",
        t3
    );
    let (ok, llvm) = run("compile", "a2_written_chain_ok.mt", src, &["--target", "llvm"]);
    assert!(ok, "{}", llvm);
}

#[test]
fn a2_switching_the_lint_off_returns_the_diagnostic_it_replaced() {
    // Deny by default, but it is a lint, not a hard-wired error, so `-A`
    // silences it. What is left when it is silenced is the point of the whole
    // item: the build STILL fails, because the program still cannot run on t3
    // — but it fails as `Undefined label: gui::set_color`, with no source
    // location and no indication of which of the program's functions is
    // responsible. That is the error A2 exists to replace, and this test pins
    // the difference rather than asserting the build passes (it must not).
    let (ok, out) = run(
        "compile",
        "a2_allow.mt",
        GUI,
        &["--target", "t3", "--allow", "backend-unavailable-chain"],
    );
    assert!(!ok, "the program genuinely cannot run on t3:\n{}", out);
    assert!(
        !out.contains("cannot be compiled for the t3 backend"),
        "-A must silence A2's own diagnostic:\n{}",
        out
    );
    assert!(
        out.contains("Undefined label: gui::set_color"),
        "without A2 the failure is an assembler error with no span:\n{}",
        out
    );
}

#[test]
fn a2_available_is_still_usable_as_an_identifier() {
    // `available` is contextual, not a keyword. stdlib/sync.mt declares a
    // method called exactly that, so making it a keyword would have broken the
    // standard library.
    let src = r#"
use std::io;
fn available(n: int) -> int { return n; }
fn main() { io::println_int(available(3)); }
"#;
    let (ok, out) = run("check", "a2_ident.mt", src, &[]);
    assert!(ok, "`available` must remain an ordinary identifier:\n{}", out);
}

// ---------------------------------------------------------------------------
// The Trit* width audit
// ---------------------------------------------------------------------------
//
// The `Trit*` IR instructions are word-width on T3 and trit-width on LLVM: the
// LLVM backend types `TritMin`, `TritMax` and `TritNeg` as `i8` because their
// operand really is a trit, while T3's TMIN/TMAX/TNEG act on a whole register.
// Anything that reaches one of them with a WORD is therefore correct on T3 and
// silently truncated on LLVM.
//
// This bit three times before it was looked for systematically: C2's `tnotw`,
// the `sign` formula proposed for C7, and then the whole ternary-logic operator
// family, which `is_ternary()` let through for `tryte`/`t9`/`t27`/`t54`/`tfloat`.
// Measured on `let a: t27 = 9841; let b: t27 = 121`, seven of eight operators
// disagreed between the backends.
//
// The audit's conclusion is that no such route should exist at all, so these
// tests pin the absence rather than the agreement.

const WIDE_OPS: &[(&str, &str)] = &[
    ("tand", "let a: t27 = 9841; let b: t27 = 121; let c = a tand b;"),
    ("tor", "let a: t27 = 9841; let b: t27 = 121; let c = a tor b;"),
    ("txor", "let a: t27 = 9841; let b: t27 = 121; let c = a txor b;"),
    ("tcon", "let a: t27 = 9841; let b: t27 = 121; let c = a tcon b;"),
    ("tany", "let a: t27 = 9841; let b: t27 = 121; let c = a tany b;"),
    ("timp", "let a: t27 = 9841; let b: t27 = 121; let c = a timp b;"),
    ("teq", "let a: t27 = 9841; let b: t27 = 121; let c = a teq b;"),
    ("tnot", "let a: t27 = 9841; let c = tnot a;"),
    ("tposs", "let a: t27 = 9841; let c = tposs a;"),
    ("tnec", "let a: t27 = 9841; let c = tnec a;"),
    ("tryte", "let a: tryte = 9; let b: tryte = 4; let c = a tand b;"),
    ("t9", "let a: t9 = 500; let b: t9 = 40; let c = a timp b;"),
    ("t54", "let a: t54 = 99999; let b: t54 = 7; let c = a txor b;"),
];

#[test]
fn width_a_ternary_logic_operator_rejects_a_multi_trit_operand() {
    for (name, body) in WIDE_OPS {
        let src = format!("fn main() {{ {} }}", body);
        let (ok, out) = run("check", &format!("width_{}.mt", name), &src, &[]);
        assert!(
            !ok,
            "`{}` on a multi-trit operand must be rejected — it computes a \
             different value on each backend:\n{}",
            name, out
        );
        assert!(
            out.contains("three-valued logic operator"),
            "the diagnostic must explain WHY, not just refuse:\n{}",
            out
        );
    }
}

#[test]
fn width_the_diagnostic_points_at_the_lane_wise_form() {
    // A rejection that leaves the author with no way to say what they meant is
    // a worse bug than the one it fixes. `tandw`/`tnotw` ARE the well-defined
    // thing to do to a whole word, and they are word-width on both backends.
    let (_, out) = run(
        "check",
        "width_hint_bin.mt",
        "fn main() { let a: t27 = 9841; let b: t27 = 121; let c = a tand b; }",
        &[],
    );
    assert!(out.contains("tandw"), "{}", out);

    let (_, out) = run(
        "check",
        "width_hint_un.mt",
        "fn main() { let a: t27 = 9841; let c = tnot a; }",
        &[],
    );
    assert!(out.contains("tnotw"), "{}", out);
}

#[test]
fn width_single_trit_operands_are_untouched() {
    // The fix must not narrow the language the documentation describes:
    // "Łukasiewicz three-valued logic on `trit` and `bool3` values".
    let src = r#"
use std::io;
use std::ternary;
fn main() {
    let a: trit = +;
    let b: trit = 0;
    io::println_int(ternary::trit_to_int(a tand b));
    io::println_int(ternary::trit_to_int(a timp b));
    io::println_int(ternary::trit_to_int(tnot a));
    let p: bool3 = True;
    let q: bool3 = Unknown;
    io::println_int(ternary::trit_to_int((p tor q) as trit));
    if tposs a { io::println("ok"); }
    if a > b tand b < a { io::println("bool operands still work"); }
}
"#;
    let (ok, out) = run("check", "width_trit_ok.mt", src, &[]);
    assert!(ok, "trit and bool3 operands must still be accepted:\n{}", out);
}

#[test]
fn width_the_lane_wise_family_accepts_words_because_it_is_word_width() {
    let src = r#"
use std::io;
fn main() {
    let a: t27 = 9841;
    let b: t27 = 121;
    io::println_int(a tandw b);
    io::println_int(tnotw a);
}
"#;
    let (ok, out) = run("check", "width_lane_ok.mt", src, &[]);
    assert!(ok, "the lane-wise family is exactly what words are for:\n{}", out);
}
