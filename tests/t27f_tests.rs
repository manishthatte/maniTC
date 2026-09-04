//! C5 — `t27f` as a first-class type, with literal syntax.
//!
//! © Manish Jagdish Thatte
//!
//! `enhance/phase5-type-system-second-half/README.md` item C5, specified in
//! `docs/language-reference.md` §27. Also the home of four findings the item
//! walked straight into, exactly as its own rationale warned it would
//! ("Section 51 found a real wrong value in exactly this area … Pin values,
//! not just agreement"):
//!
//! * **P127** — `tfloat` was a PHANTOM: a keyword, a `ManiType` variant, and
//!   IEEE 754 double in every observable property while claiming to be the
//!   ternary format.
//! * **P128** — `t27f::add` answered **40** for 100 + 25, and `t27f::mul`
//!   answered **0** on LLVM and **trapped** on T3 for 100 × 25.
//! * **P129** — `as` was UNCHECKED, so a struct cast to a number yielded its
//!   allocation address.
//! * **P130** — `ManiType::display` rendered an array as `[int; Some(2)]`.
//!
//! Three of the four are silent wrong answers on BOTH backends, which is why
//! none of them was ever caught by the differential oracle: the two backends
//! agree, because the mistake is upstream of where they part.

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
    let d = common::suite_root("c5").join(slot.to_string());
    std::fs::create_dir_all(&d).expect("temp dir");
    d
}

fn write(src: &str) -> PathBuf {
    let d = workdir();
    let path = d.join("p.mt");
    std::fs::write(&path, src).expect("write");
    path
}

fn check(src: &str) -> (bool, String) {
    let path = write(src);
    let o = Command::new(manitc_bin())
        .args(["check", path.to_str().unwrap()])
        .output()
        .expect("check");
    let txt =
        String::from_utf8_lossy(&o.stdout).into_owned() + &String::from_utf8_lossy(&o.stderr);
    (o.status.success() && !txt.contains("error:"), txt)
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
    let out: String = String::from_utf8_lossy(&r.stdout)
        .lines()
        .filter(|l| !l.starts_with("[T3ISA]"))
        .map(|l| format!("{l}\n"))
        .collect();
    // A TRAP is not an answer. P128's `mul` trapped on T3 while returning 0 on
    // LLVM, so a row that only compared the two outputs would have compared a
    // trap against a wrong number and called them different for the wrong
    // reason.
    let err = String::from_utf8_lossy(&r.stderr);
    assert!(!err.contains("TRAP"), "T3 trapped: {err}");
    out
}

/// `None` only when clang is absent — told apart by the ARTEFACT (P47).
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
/// each other. P128 is the reason that rule is written down: the two backends
/// disagreed there, and each was wrong in its own way.
fn both(src: &str, want: &str, what: &str) {
    assert_eq!(run_t3(src), want, "{what}: T3");
    if let Some(ll) = run_llvm(src) {
        assert_eq!(ll, want, "{what}: LLVM");
    }
}

// ---------------------------------------------------------------------------
// 1. The literal
// ---------------------------------------------------------------------------

/// `3.5t27f` is a ternary floating-point literal, and it has the FORMAT's
/// precision rather than a double's.
///
/// 3.4999999883847144 is not an approximation of the test's making: it is what
/// an 18-trit mantissa holds, and the error is about 3^-18. A literal that
/// printed 3.5 exactly would mean the suffix had produced a `float`.
#[test]
fn c5_a_t27f_literal_carries_the_formats_precision() {
    both(
        "use std::io;\nuse std::t27f;\n\
         fn main() { let x = 3.5t27f; io::println_float(t27f::to_float(x)); }\n",
        "3.4999999883847144\n",
        "the literal is the ternary format, not a double",
    );
}

/// A literal whose decimal spelling is longer than its value's shortest one.
///
/// **This row exists because the first implementation got it wrong.** Adjacency
/// was tested in the PARSER by reconstructing the literal's width from its
/// value, and `format!("{}", 100.0)` is `"100"` — three characters for a
/// five-character literal — so `100.0t27f` was not recognised while `3.5t27f`
/// was. Recognising the suffix in the LEXER, where the source characters are,
/// is the repair.
#[test]
fn c5_a_literal_is_recognised_by_its_text_not_by_its_value() {
    both(
        "use std::io;\nuse std::t27f;\n\
         fn main() { io::println_int(t27f::to_int(100.0t27f)); }\n",
        "100\n",
        "100.0t27f",
    );
}

/// Adjacency is required, in both directions.
///
/// `3.5 t27f` is two tokens and stays two; `3.5t27foo` is a float and an
/// identifier, not a literal and a suffix. Without the trailing check the
/// second would lex as `3.5t27f` followed by `oo`.
///
/// **This row PASSES on the compiler without C5, by construction** (rule 9):
/// there, nothing is a `t27f` literal, so both spellings are errors for a
/// duller reason. It is one of this suite's three inertness rows — it
/// discriminates the change that would have swallowed an identifier after any
/// float, not a change that swallowed none.
#[test]
fn c5_the_suffix_must_be_adjacent_and_complete() {
    for src in [
        "fn main() { let _x = 3.5 t27f; }\n",
        "fn main() { let _x = 3.5t27foo; }\n",
    ] {
        let (ok, _) = check(src);
        assert!(!ok, "must not lex as a t27f literal: {src}");
    }
    // And an ordinary float is untouched.
    both(
        "use std::io;\nfn main() { io::println_float(3.5); }\n",
        "3.5\n",
        "a plain float literal is unaffected",
    );
}

// ---------------------------------------------------------------------------
// 2. The type
// ---------------------------------------------------------------------------

/// `t27f` is a type, and mentioning it is enough to pull its module in.
///
/// No `use std::t27f;` anywhere. Two mechanisms make that work and both are
/// asserted here: the literal desugars to `t27f::from_float`, which is an
/// expression reference the expander already follows, and a TYPE mention in a
/// signature is followed too — which it was not until C5, so
/// `fn f(x: t27f) -> t27f` resolved to a struct that was never merged.
#[test]
fn c5_t27f_is_a_type_and_needs_no_use() {
    both(
        "use std::io;\n\
         fn twice(x: t27f) -> t27f { return t27f::add(x, x); }\n\
         fn main() { io::println_int(t27f::to_int(twice(21.0t27f))); }\n",
        "42\n",
        "t27f in a signature, with no `use`",
    );
}

/// **LIMIT.** The operators are not overloaded, and they say so.
///
/// `+` and `>` on a `t27f` are refused. This language's arithmetic and
/// comparison operators are built in and never dispatch to a user impl — the
/// unsatisfied-bound diagnostic says so in as many words — so promoting the
/// type gave it a spelling and a literal, not an operator set. Recorded rather
/// than implied, and the row goes red the day dispatch lands.
#[test]
fn c5_the_operators_are_not_overloaded_and_say_so() {
    for op in ["+", ">"] {
        let (ok, msg) = check(&format!(
            "fn main() {{ let x = 3.5t27f; let y = 1.5t27f; let _z = x {op} y; }}\n"
        ));
        assert!(!ok, "`{op}` on a t27f must be refused");
        assert!(
            msg.contains("invalid operands") && msg.contains("T27F"),
            "and name the type: {msg}"
        );
    }
}

// ---------------------------------------------------------------------------
// 3. P127 — the phantom
// ---------------------------------------------------------------------------

/// `tfloat` is refused, and the message names BOTH honest alternatives.
///
/// It was IEEE 754 double lowered to `IRType::F64`, identical to `Float`, while
/// its name and `stdlib/t27f.mt` claimed a format with no NaN, one zero and a
/// range to about 4.3e4703. A reader who believed the name got a double.
#[test]
fn p127_tfloat_is_refused_and_names_both_alternatives() {
    let (ok, msg) = check("fn main() { let _x: tfloat = 1.5; }\n");
    assert!(!ok, "`tfloat` must be refused");
    assert!(
        msg.contains("`float`") && msg.contains("`t27f`"),
        "the message must offer the binary AND the ternary answer: {msg}"
    );
    assert!(
        msg.contains("NaN"),
        "and say what the ternary format actually promises: {msg}"
    );
}

/// The properties that made it a phantom, asserted on `float` itself.
///
/// This row does not test `tfloat` — it cannot, it is gone. It pins the
/// MEASUREMENT that condemned it: `float` has NaN and overflows to infinity,
/// and `tfloat` did both identically. If a future `tfloat` returns, these are
/// the two answers it must not give.
#[test]
fn p127_the_measurements_that_condemned_it() {
    both(
        "use std::io;\n\
         fn main() { let z: float = 0.0; io::println_bool((z / z) != (z / z)); }\n",
        "true\n",
        "float has NaN — and `tfloat` gave the same answer",
    );
}

// ---------------------------------------------------------------------------
// 4. P128 — the library was wrong for its primary operations
// ---------------------------------------------------------------------------

/// **The values, not the agreement.** The three operations at DIFFERING
/// exponents, which is the case that was broken.
///
/// `from_float(100.0)` has exponent -13 and `from_float(25.0)` has -14, so the
/// aligned sum exceeds the 18-trit mantissa — and `from_parts` clamped it to
/// MANTISSA_MAX before `normalize` could ever see it. The measured answers
/// before the fix were 40, 40, and (0 on LLVM / a TRAP on T3).
///
/// With EQUAL exponents and small mantissas nothing overflows and the old code
/// was right, which is why the module's own usage example never showed it —
/// so both cases are here.
#[test]
fn p128_add_sub_and_mul_are_right_at_differing_exponents() {
    both(
        "use std::io;\nuse std::t27f;\n\
         fn main() {\n\
         \x20 let a = 100.0t27f; let b = 25.0t27f;\n\
         \x20 io::println_int(t27f::to_int(t27f::add(a, b)));\n\
         \x20 io::println_int(t27f::to_int(t27f::sub(a, b)));\n\
         \x20 io::println_int(t27f::to_int(t27f::mul(a, b)));\n\
         }\n",
        "125\n75\n2500\n",
        "100 and 25 through add, sub and mul",
    );
    // The case that always worked, kept so a repair cannot trade one for the
    // other: equal exponents, mantissas that fit.
    both(
        "use std::io;\nuse std::t27f;\n\
         fn main() {\n\
         \x20 let c = t27f::from_parts(0, 100); let d = t27f::from_parts(0, 25);\n\
         \x20 io::println_int(t27f::to_int(t27f::add(c, d)));\n\
         \x20 io::println_int(t27f::to_int(t27f::mul(c, d)));\n\
         }\n",
        "125\n2500\n",
        "equal exponents still work",
    );
}

/// Signs, zero, and a value large enough to need renormalising.
///
/// Rule 8: both orderings of every pair. Balanced ternary is symmetric, so a
/// sign error would not show on the positive cases alone.
#[test]
fn p128_the_arithmetic_holds_across_signs_and_zero() {
    both(
        "use std::io;\nuse std::t27f;\n\
         fn main() {\n\
         \x20 io::println_int(t27f::to_int(t27f::add(t27f::neg(7.0t27f), 3.0t27f)));\n\
         \x20 io::println_int(t27f::to_int(t27f::add(3.0t27f, t27f::neg(7.0t27f))));\n\
         \x20 io::println_int(t27f::to_int(t27f::mul(t27f::neg(6.0t27f), 7.0t27f)));\n\
         \x20 io::println_int(t27f::to_int(t27f::mul(7.0t27f, t27f::neg(6.0t27f))));\n\
         \x20 io::println_int(t27f::to_int(t27f::add(0.0t27f, 5.0t27f)));\n\
         \x20 io::println_int(t27f::to_int(t27f::add(12345.0t27f, 1.0t27f)));\n\
         }\n",
        "-4\n-4\n-42\n-42\n5\n12346\n",
        "signs, zero and renormalisation",
    );
}

// ---------------------------------------------------------------------------
// 5. P129 — `as` was unchecked
// ---------------------------------------------------------------------------

/// A struct cast to a number used to yield its ALLOCATION ADDRESS.
///
/// `p as int` answered 63000 — `HEAP_BASE` — and `p as float` the same address
/// reinterpreted as a double. A4's defect in a second syntax: there `>`
/// compared allocation addresses, here `as` returns one.
#[test]
fn p129_an_aggregate_cannot_be_cast_to_a_number() {
    for (decls, expr) in [
        ("struct P { pub v: int }\n", "let p = P { v: 7 }; let _n = p as int;"),
        ("struct P { pub v: int }\n", "let p = P { v: 7 }; let _n = p as float;"),
        ("", "let a: [int; 2] = [1, 2]; let _n = a as int;"),
        ("", "let t = (1, 2); let _n = t as int;"),
    ] {
        let (ok, msg) = check(&format!("{decls}fn main() {{ {expr} }}\n"));
        assert!(!ok, "must be refused: {expr}");
        assert!(
            msg.contains("cannot be cast to") && msg.contains("ALLOCATION ADDRESS"),
            "and say what it used to do: {msg}"
        );
    }
}

/// The rule is narrow: numbers still cast, an aggregate still casts to itself,
/// and an ERASED generic is never refused.
///
/// The last is the one that matters. A generic parameter is `Unknown`, and
/// refusing there would reject a body that is correct at every instantiation —
/// which is the false rejection this kind of check must not have.
#[test]
fn p129_the_cast_rule_refuses_nothing_else() {
    for src in [
        "use std::io;\nfn main() { let x: int = 7; io::println_float(x as float); }\n",
        "use std::io;\nfn main() { io::println_int(3.9 as int); }\n",
        "struct P { pub v: int }\nfn main() { let p = P { v: 1 }; let _q = p as P; }\n",
        "fn f<T>(x: T) -> int { return x as int; }\nfn main() { }\n",
    ] {
        let (ok, msg) = check(src);
        assert!(ok, "must still be accepted: {src}\n{msg}");
    }
}

// ---------------------------------------------------------------------------
// 6. P130 — display leaked Rust's Option
// ---------------------------------------------------------------------------

/// An array type renders in SURFACE syntax, not as `[int; Some(2)]`.
///
/// Reachable from any array type mismatch, on the compiler before this — and
/// `display` is the function P123 had just finished routing two other messages
/// to, whose contract is that a reader can paste its output back as source.
#[test]
fn p130_an_array_type_renders_in_surface_syntax() {
    let (ok, msg) = check("fn f(a: [int; 2]) -> int { return 1; }\nfn main() { f(7); }\n");
    assert!(!ok, "the mismatch must still be reported");
    assert!(msg.contains("[int; 2]"), "in surface syntax: {msg}");
    assert!(
        !msg.contains("Some(") && !msg.contains("None"),
        "with no Rust `Option` in it: {msg}"
    );
}
