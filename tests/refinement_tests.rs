//! B6 — refinement types over trit ranges, `where` on a parameter.
//!
//! © Manish Jagdish Thatte
//!
//! `enhance/phase5-type-system-second-half/README.md` item B6, specified in
//! `docs/language-reference.md` §26. A parameter may declare the interval its
//! value lies in, and a call that cannot satisfy it is refused at compile time.
//!
//! Four properties carry the design, and each is a row rather than a comment
//! because prose review does not catch this class (permanent rule 6):
//!
//! 1. **There are THREE verdicts and only one is an error.** Provably inside is
//!    silent, provably outside is an error, and neither is a lint defaulting to
//!    `allow`. A checker that collapsed the middle case into either of the
//!    others would pass a suite that only tested the extremes, so a row asserts
//!    all three on the same callee.
//! 2. **The checker must never prove something false.** `let mut` drops its
//!    interval, unknown is neither accepted nor refused, and interval
//!    arithmetic widens to unknown on `i64` overflow rather than wrapping. A
//!    false REJECTION is the one failure mode this feature cannot have.
//! 3. **Chained comparison is legal in a `where` and nowhere else.** The
//!    expression grammar refuses `a < b < c` deliberately; the item's own
//!    example is written that way. Both halves are rows.
//! 4. **A refinement emits nothing.** The compiled program is byte-identical to
//!    the one without it, asserted on the emitted LLVM rather than on output —
//!    which is exactly what a behavioural test cannot see.
//!
//! Three rows record a LIMIT rather than a capability and say so in their own
//! docstrings.

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
    let d = common::suite_root("b6").join(slot.to_string());
    std::fs::create_dir_all(&d).expect("temp dir");
    d
}

fn write(src: &str) -> PathBuf {
    let d = workdir();
    let path = d.join("p.mt");
    std::fs::write(&path, src).expect("write");
    path
}

/// `manitc check` with optional extra flags — (accepted?, combined output).
fn check_with(src: &str, extra: &[&str]) -> (bool, String) {
    let path = write(src);
    let mut args: Vec<String> = vec!["check".into()];
    args.extend(extra.iter().map(|s| s.to_string()));
    args.push(path.to_str().unwrap().to_string());
    let o = Command::new(manitc_bin()).args(&args).output().expect("check");
    let txt =
        String::from_utf8_lossy(&o.stdout).into_owned() + &String::from_utf8_lossy(&o.stderr);
    (o.status.success() && !txt.contains("error:"), txt)
}

fn check(src: &str) -> (bool, String) {
    check_with(src, &[])
}

fn run_t3(src: &str) -> String {
    let path = write(src);
    let base = path.with_extension("");
    let c = Command::new(manitc_bin())
        .args([
            "compile",
            path.to_str().unwrap(),
            "--target",
            "t3",
            "-o",
            base.to_str().unwrap(),
        ])
        .output()
        .expect("compile");
    assert!(
        c.status.success(),
        "T3 compile failed:\n{}{}",
        String::from_utf8_lossy(&c.stdout),
        String::from_utf8_lossy(&c.stderr)
    );
    let r = Command::new(manitc_bin())
        .args(["run-t3", base.with_extension("t3b").to_str().unwrap()])
        .output()
        .expect("run");
    String::from_utf8_lossy(&r.stdout)
        .lines()
        .filter(|l| !l.starts_with("[T3ISA]"))
        .map(|l| format!("{l}\n"))
        .collect()
}

/// `None` only when clang is absent — told apart by the ARTEFACT, never by the
/// shape of an error message (P47).
fn run_llvm(src: &str) -> Option<String> {
    let path = write(src);
    let bin = path.with_file_name("p.bin");
    Command::new(manitc_bin())
        .args([
            "compile",
            path.to_str().unwrap(),
            "--target",
            "llvm",
            "-o",
            bin.to_str().unwrap(),
        ])
        .output()
        .expect("compile");
    if !bin.exists() {
        return None;
    }
    let r = Command::new(&bin).output().expect("run");
    Some(String::from_utf8_lossy(&r.stdout).into_owned())
}

/// Both backends produce `want` — against a hand-derived string, never against
/// each other (P44/P58: all of this is upstream of the backend split).
fn both(src: &str, want: &str, what: &str) {
    assert_eq!(run_t3(src), want, "{what}: T3");
    if let Some(ll) = run_llvm(src) {
        assert_eq!(ll, want, "{what}: LLVM");
    }
}

fn emitted_ll(src: &str) -> String {
    let path = write(src);
    let ll = path.with_extension("ll");
    let o = Command::new(manitc_bin())
        .args([
            "compile",
            path.to_str().unwrap(),
            "--target",
            "llvm",
            "-o",
            ll.to_str().unwrap(),
        ])
        .output()
        .expect("compile");
    assert!(o.status.success(), "llvm compile failed");
    std::fs::read_to_string(&ll).expect("read .ll")
}

// ---------------------------------------------------------------------------
// 1. The item's own example
// ---------------------------------------------------------------------------

/// `fn scale(x: t27 where -100 <= x <= 100) -> t27`, verbatim from the
/// recommendation, compiled and run on both backends.
#[test]
fn b6_the_items_own_example_runs() {
    both(
        "use std::io;\n\
         fn scale(x: t27 where -100 <= x <= 100) -> t27 { return x * 3; }\n\
         fn main() { io::println_int(scale(50) as int); }\n",
        "150\n",
        "the item's own signature",
    );
}

/// A literal outside the interval is refused, and the message quotes the
/// predicate AS WRITTEN.
///
/// The quoting matters and has its own assertion: an earlier draft rendered
/// `-100` as `- 100` from its token stream, which is P124's defect one level
/// down — a message that claims to show the author's source and does not.
#[test]
fn b6_a_literal_outside_the_interval_is_refused() {
    let (ok, msg) = check(
        "fn scale(x: t27 where -100 <= x <= 100) -> t27 { return x; }\n\
         fn main() { let _ = scale(500); }\n",
    );
    assert!(!ok, "500 must be refused");
    assert!(
        msg.contains("cannot satisfy `where -100 <= x <= 100`"),
        "the predicate must be quoted as written, with no stray space: {msg}"
    );
    assert!(msg.contains("-100..100"), "and the interval named: {msg}");
}

/// Both ends, and the boundary is INSIDE.
///
/// Rule 8: both orderings of the pair. A checker with `<` where it needs `<=`
/// passes every row that only tests the middle of the range.
#[test]
fn b6_the_boundary_values_are_inside_the_interval() {
    both(
        "use std::io;\n\
         fn scale(x: t27 where -100 <= x <= 100) -> t27 { return x; }\n\
         fn main() { io::println_int(scale(100) as int); io::println_int(scale(-100) as int); }\n",
        "100\n-100\n",
        "the closed interval includes its ends",
    );
    let (ok, _) = check(
        "fn scale(x: t27 where -100 <= x <= 100) -> t27 { return x; }\n\
         fn main() { let _ = scale(-500); }\n",
    );
    assert!(!ok, "the negative end must be refused too");
}

// ---------------------------------------------------------------------------
// 2. The three verdicts
// ---------------------------------------------------------------------------

/// **The discriminating row.** One callee, three call sites, three verdicts.
///
/// `y` is `0..3`, so `y * 3` is `0..9` (proven), `y * 5` is `0..15` (neither),
/// and `y + 100` is `100..103` (refuted). A checker that collapsed the middle
/// case into either neighbour would pass a suite testing only the extremes.
#[test]
fn b6_proven_unproven_and_refuted_are_three_different_answers() {
    const INNER: &str = "fn inner(x: int where 0 <= x <= 10) -> int { return x; }\n";

    let (ok, msg) = check(&format!(
        "{INNER}fn a(y: int where 0 <= y <= 3) -> int {{ return inner(y * 3); }}\nfn main() {{ }}\n"
    ));
    assert!(ok, "0..9 is inside 0..10 and must be silent: {msg}");
    assert!(!msg.contains("unproven"), "and must not be listed: {msg}");

    let (ok, msg) = check(&format!(
        "{INNER}fn b(y: int where 0 <= y <= 3) -> int {{ return inner(y * 5); }}\nfn main() {{ }}\n"
    ));
    assert!(ok, "0..15 is not provably wrong, so it must compile: {msg}");

    let (ok, msg) = check(&format!(
        "{INNER}fn c(y: int where 0 <= y <= 3) -> int {{ return inner(y + 100); }}\nfn main() {{ }}\n"
    ));
    assert!(!ok, "100..103 cannot reach 0..10 and must be refused");
    assert!(msg.contains("100..103"), "naming the interval it computed: {msg}");
}

/// The middle verdict is a BACKLOG, and `--warn` is what prints it.
///
/// Default `allow`, for the reason `literal-out-of-word` and
/// `division-semantics` have it: an unproven call is not a wrong one, and
/// warning by default would report working programs.
#[test]
fn b6_the_unproven_case_is_a_backlog_the_lint_generates_on_demand() {
    let src = "fn inner(x: int where 0 <= x <= 10) -> int { return x; }\n\
               fn b(y: int where 0 <= y <= 3) -> int { return inner(y * 5); }\n\
               fn main() { }\n";

    let (ok, msg) = check(src);
    assert!(ok && !msg.contains("unproven-refinement"),
            "silent by default: {msg}");

    let (ok, msg) = check_with(src, &["-W", "unproven-refinement"]);
    assert!(ok, "the lint must not make it an error: {msg}");
    assert!(
        msg.contains("unproven-refinement") && msg.contains("0..15"),
        "and must name the interval it could not prove: {msg}"
    );
}

// ---------------------------------------------------------------------------
// 3. What the checker knows, and what it must not claim
// ---------------------------------------------------------------------------

/// An interval crosses a call boundary: a refined parameter passed to another.
#[test]
fn b6_an_interval_reaches_across_a_call() {
    let (ok, msg) = check(
        "fn inner(x: int where 0 <= x <= 10) -> int { return x; }\n\
         fn outer(y: int where 20 <= y <= 30) -> int { return inner(y); }\n\
         fn main() { }\n",
    );
    assert!(!ok, "20..30 cannot reach 0..10");
    assert!(msg.contains("20..30"), "{msg}");
}

/// An interval survives an immutable `let` — and a `let mut` drops it.
///
/// **Both halves, and the second is the one that matters.** A `let mut` can be
/// assigned anything later and this pass does not follow assignments, so
/// keeping its interval would let the checker refuse a correct program. A false
/// rejection is the one failure mode this feature cannot have.
#[test]
fn b6_an_immutable_binding_keeps_its_interval_and_a_mutable_one_does_not() {
    let (ok, msg) = check(
        "fn inner(x: int where 0 <= x <= 10) -> int { return x; }\n\
         fn c(y: int where 0 <= y <= 3) -> int { let z = y + 100; return inner(z); }\n\
         fn main() { }\n",
    );
    assert!(!ok, "the interval must survive an immutable binding: {msg}");

    let (ok, msg) = check(
        "fn inner(x: int where 0 <= x <= 10) -> int { return x; }\n\
         fn c(y: int where 0 <= y <= 3) -> int { let mut z = y + 100; z = 5; return inner(z); }\n\
         fn main() { }\n",
    );
    assert!(ok, "a `let mut` must NOT be refused on a stale interval: {msg}");
}

/// An argument the fragment knows nothing about is accepted.
///
/// A refuter is silent where it cannot decide. Asserted with a run, not just a
/// verdict, so that "accepted" also means "computes what it always did".
#[test]
fn b6_an_unknown_argument_is_accepted() {
    both(
        "use std::io;\n\
         fn scale(x: t27 where -100 <= x <= 100) -> t27 { return x; }\n\
         fn opaque() -> t27 { return 7; }\n\
         fn main() { io::println_int(scale(opaque()) as int); }\n",
        "7\n",
        "an unknown argument is not refused",
    );
}

// ---------------------------------------------------------------------------
// 4. Chained comparison: legal here, refused there
// ---------------------------------------------------------------------------

/// The two positions, in one row, because the claim is about the DIFFERENCE.
///
/// `-100 <= x <= 100` is the item's own notation and is accepted in a `where`.
/// The same shape in an expression is still refused by the message that exists
/// because C's reading of it is a bug magnet — B6 must not have relaxed that.
#[test]
fn b6_chaining_is_legal_in_a_where_and_still_refused_in_an_expression() {
    let (ok, msg) = check(
        "fn scale(x: t27 where -100 <= x <= 100) -> t27 { return x; }\nfn main() { }\n",
    );
    assert!(ok, "chained bounds must be accepted in a `where`: {msg}");

    let (ok, msg) = check("fn main() { let x: int = 5; let _b = -100 <= x <= 100; }\n");
    assert!(!ok, "chaining must still be refused in an expression");
    assert!(
        msg.contains("cannot be chained"),
        "by the message that has always refused it: {msg}"
    );
}

// ---------------------------------------------------------------------------
// 5. Declarations that have no reading
// ---------------------------------------------------------------------------

/// An empty interval is refused where it is WRITTEN, not at each call.
#[test]
fn b6_an_empty_interval_is_refused_at_the_declaration() {
    let (ok, msg) = check("fn f(x: int where 10 <= x <= 5) -> int { return x; }\nfn main() { }\n");
    assert!(!ok, "`10 <= x <= 5` must be refused");
    assert!(
        msg.contains("is empty") && msg.contains("no call"),
        "and say why it can never be satisfied: {msg}"
    );
}

/// A bound outside what the TYPE can hold is refused — for a named width.
///
/// And `int` is exempt, deliberately: it is 27 trits on T3 and 64 bits on
/// LLVM, so a bound past 27 trits is legal source for one backend and not the
/// other. Both halves are asserted, because the exemption is a decision and
/// not an omission.
#[test]
fn b6_a_bound_outside_the_type_is_refused_but_int_is_exempt() {
    let (ok, msg) = check(
        "fn g(x: tryte where -1000 <= x <= 1000) -> tryte { return x; }\nfn main() { }\n",
    );
    assert!(!ok, "a `tryte` holds -364..364");
    assert!(
        msg.contains("-364..364") && msg.contains("reaches outside"),
        "and the message must name what the type holds: {msg}"
    );

    let (ok, msg) = check(
        "fn h(x: int where -9000000000000 <= x <= 9000000000000) -> int { return x; }\n\
         fn main() { }\n",
    );
    assert!(ok, "`int` must be exempt — it is 64 bits on LLVM: {msg}");
}

/// A bound must be a literal or a `const` generic parameter, and equality is
/// not a comparison.
#[test]
fn b6_the_fragment_says_where_it_ends() {
    for (src, want) in [
        (
            "fn f(x: int where x == 3) -> int { return x; }\nfn main() { }\n",
            "needs a comparison",
        ),
        (
            "fn f(x: int where y <= 3) -> int { return x; }\nfn main() { }\n",
            "must compare `x` itself",
        ),
        (
            "fn f(x: int where x <= foo()) -> int { return x; }\nfn main() { }\n",
            "integer literal or a `const` generic parameter",
        ),
    ] {
        let (ok, msg) = check(src);
        assert!(!ok, "must be refused: {src}");
        assert!(msg.contains(want), "wanted {want:?}, got: {msg}");
    }
}

// ---------------------------------------------------------------------------
// 6. B6 composed with B3
// ---------------------------------------------------------------------------

/// A `const` generic parameter as a refinement bound — the array-index idiom.
///
/// `fn get<const N: int>(a: [int; N], i: int where 0 <= i < N)` states the
/// relationship between an array and its index in the signature, which neither
/// feature could express alone. It runs, and the strict upper bound is what
/// makes it the right statement: a valid index is `< N`, not `<= N`.
#[test]
fn b6_a_const_generic_parameter_can_be_a_bound() {
    both(
        "use std::io;\n\
         fn get<const N: int>(a: [int; N], i: int where 0 <= i < N) -> int { return a[i]; }\n\
         fn main() { let v: [int; 3] = [7, 8, 9]; io::println_int(get(v, 1)); }\n",
        "8\n",
        "an index refined by the array's own length",
    );
}

// ---------------------------------------------------------------------------
// 7. Inertness: a refinement emits nothing
// ---------------------------------------------------------------------------

/// The same program with and without a `where` emits byte-identical LLVM.
///
/// Asserted on the IR rather than on output, because that is precisely what a
/// behavioural test cannot see: a refinement that inserted a runtime check
/// would still print 150.
#[test]
fn b6_a_refinement_emits_no_code() {
    let with = emitted_ll(
        "use std::io;\n\
         fn scale(x: t27 where -100 <= x <= 100) -> t27 { return x * 3; }\n\
         fn main() { io::println_int(scale(50) as int); }\n",
    );
    let without = emitted_ll(
        "use std::io;\n\
         fn scale(x: t27) -> t27 { return x * 3; }\n\
         fn main() { io::println_int(scale(50) as int); }\n",
    );
    assert_eq!(with, without, "a `where` must not change the emitted program");
}

// ---------------------------------------------------------------------------
// 8. P125 — the word check sees what the compiler computed
// ---------------------------------------------------------------------------

/// **The out-of-word check must see a FOLDED constant, not only a written one.**
///
/// `3812798742493 + 3812798742493` writes no literal, so under `--lang v2` —
/// where N5 says an `int` IS a 27-trit word and the value therefore does not
/// exist — the program COMPILED. Measured before the fix: the written form is
/// a hard error and the folded form is accepted, which is one value getting two
/// answers.
#[test]
fn p125_a_folded_constant_is_checked_against_the_word() {
    const FOLDED: &str =
        "use std::io;\nfn main() { io::println_int(3812798742493 + 3812798742493); }\n";
    const WRITTEN: &str = "use std::io;\nfn main() { io::println_int(7625597484986); }\n";

    // v2: `int` IS 27 trits, so both spellings must be refused, and alike.
    for (src, what) in [(WRITTEN, "written"), (FOLDED, "folded")] {
        let (ok, msg) = check_with(src, &["--lang", "v2"]);
        assert!(!ok, "v2 must reject the {what} form");
        assert!(
            msg.contains("does not fit `int`") && msg.contains("7625597484986"),
            "naming the true value, not a fragment of it: {msg}"
        );
    }

    // v1: the migration backlog must list both.
    for (src, what) in [(WRITTEN, "written"), (FOLDED, "folded")] {
        let (ok, msg) = check_with(src, &["-W", "literal-out-of-word"]);
        assert!(ok, "v1 must still accept the {what} form: {msg}");
        assert!(
            msg.contains("literal-out-of-word") && msg.contains("7625597484986"),
            "and list it: {msg}"
        );
    }
}

/// **A LIMIT, and the specification is why.** The runtime behaviour is
/// unchanged: T3 still compiles the program and traps.
///
/// `docs/semantics.md` §10.1 specifies that `int` is 27 trits on T3 and 64 bits
/// on LLVM, and P79 and P116 pin that T3 COMPILES such a program and traps at
/// run time. Refusing it at compile time was tried and reverted: it moved three
/// pinned rows and contradicted a normative section (permanent rule 3). What P125
/// fixes is which values the CHECK sees, not what the backend does with them.
///
/// **This row PASSES on the compiler without B6, and that is what it is for**
/// (rule 9). It is the only one of this suite's seventeen that does: a row
/// asserting nothing moved cannot discriminate a change that moved nothing —
/// it discriminates the change that WOULD have, and that change was written,
/// measured against three pinned rows, and reverted.
#[test]
fn p125_the_specified_runtime_divergence_is_unchanged() {
    const FOLDED: &str =
        "use std::io;\nfn main() { io::println_int(3812798742493 + 3812798742493); }\n";

    let (ok, msg) = check(FOLDED);
    assert!(ok, "v1 default must still accept it silently: {msg}");

    let out = run_t3(FOLDED);
    assert!(
        out.contains("TRAP") && out.contains("27-trit range"),
        "T3 must still compile it and trap: {out:?}"
    );

    if let Some(ll) = run_llvm(FOLDED) {
        assert_eq!(
            ll, "7625597484986\n",
            "and LLVM must still print the 64-bit answer — the divergence is specified"
        );
    }
}

/// The lint's own sentence was wrong about the compiler.
///
/// It said such a value "is reshaped on T3". Clamping was REMOVED when trapping
/// replaced saturation — `clamp27` "silently substituted ±T3_MAX for the real
/// value", and that is exactly the wrong answer the trap exists to prevent. So
/// the message described behaviour the compiler had stopped having.
#[test]
fn p125_the_lint_no_longer_claims_the_value_is_reshaped() {
    let (_, msg) = check_with(
        "use std::io;\nfn main() { io::println_int(7625597484986); }\n",
        &["-W", "literal-out-of-word"],
    );
    assert!(msg.contains("TRAPS at run time"), "it must say what happens: {msg}");
    assert!(!msg.contains("reshaped"), "and not what stopped happening: {msg}");
}
