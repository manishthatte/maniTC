//! B4 — `const fn` and compile-time evaluation.
//!
//! © Manish Jagdish Thatte
//!
//! `enhance/phase5-type-system-second-half/README.md` item B4, specified in
//! `docs/language-reference.md` §29. A `const fn` may be RUN at compile time,
//! and its result may be a trit width or an array length.
//!
//! **This item's reach was measured before it was built, and the measurement
//! is the interesting part.** The recommendations call `const fn` a
//! prerequisite for const generics (B3), for compile-time trit tables, and for
//! C6's pattern compilation. Three sessions measured otherwise: C6 needed none
//! of it, C3 needed none of it, and B3 needed it for exactly ONE construct —
//! `t<A + 1>`, an expression over a bound parameter. B3 left a row saying so,
//! and that row is now `b3_a_const_expression_over_a_parameter_now_evaluates`.
//!
//! Four properties carry the design:
//!
//! 1. **`const` adds a capability and removes none.** A `const fn` is an
//!    ordinary function too — checked, lowered, emitted, callable at run time.
//!    A row calls the same function both ways in one program.
//! 2. **Termination is bought, not assumed.** A `const fn` may loop, so
//!    evaluation carries a step budget and exhausting it is a diagnostic, not
//!    a hang. A row runs a non-terminating one.
//! 3. **A width's fragment stops below comparison**, which is what lets
//!    `t<A + 1>` close on its own `>` without Rust's braces.
//! 4. **There are two evaluators and they must agree** (permanent rule 5), so
//!    a row runs the same expressions through both.

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
    let d = common::suite_root("b4").join(slot.to_string());
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
    String::from_utf8_lossy(&r.stdout)
        .lines()
        .filter(|l| !l.starts_with("[T3ISA]"))
        .map(|l| format!("{l}\n"))
        .collect()
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

fn both(src: &str, want: &str, what: &str) {
    assert_eq!(run_t3(src), want, "{what}: T3");
    if let Some(ll) = run_llvm(src) {
        assert_eq!(ll, want, "{what}: LLVM");
    }
}

// ---------------------------------------------------------------------------
// 1. `const fn`
// ---------------------------------------------------------------------------

/// A `const fn` produces an array length and a trit width.
///
/// The array is indexed at its last element, so a length that came out wrong
/// would be a bounds error rather than a silent pass.
#[test]
fn b4_a_const_fn_produces_a_length_and_a_width() {
    both(
        "use std::io;\n\
         const fn dbl(x: int) -> int { return x * 2; }\n\
         fn main() { let a: [int; dbl(2)] = [1, 2, 3, 4]; io::println_int(a[3]); }\n",
        "4\n",
        "a const fn as an array length",
    );
    both(
        "use std::io;\n\
         const fn half(x: int) -> int { return x / 2; }\n\
         fn main() { let v: t<half(18)> = 9841; io::println_int(v as int); }\n",
        "9841\n",
        "a const fn as a trit width",
    );
}

/// **`const` ADDS a capability and removes none.**
///
/// The same function is evaluated at compile time to size an array and called
/// at run time in the same program. A `const fn` that could only be used one
/// way would pass every other row in this file.
#[test]
fn b4_a_const_fn_is_still_an_ordinary_function() {
    both(
        "use std::io;\n\
         const fn dbl(x: int) -> int { return x * 2; }\n\
         fn main() {\n\
         \x20 let a: [int; dbl(2)] = [1, 2, 3, 4];\n\
         \x20 io::println_int(a[3]);\n\
         \x20 io::println_int(dbl(21));\n\
         }\n",
        "4\n42\n",
        "one const fn, both ways",
    );
}

/// The restricted subset really is a subset of the LANGUAGE: `if`, `while`,
/// `let`, assignment and `return` all run.
///
/// `fact(3)` is 6 and `maxi(9, 18)` is 18, and both are asserted through a
/// construct that would fail loudly at any other value — the array's last
/// index, and a literal only the wider type can hold.
#[test]
fn b4_control_flow_runs_at_compile_time() {
    both(
        "use std::io;\n\
         const fn fact(n: int) -> int {\n\
         \x20 let mut r = 1; let mut i = 1;\n\
         \x20 while i <= n { r = r * i; i = i + 1; }\n\
         \x20 return r;\n\
         }\n\
         fn main() { let a: [int; fact(3)] = [1,2,3,4,5,6]; io::println_int(a[5]); }\n",
        "6\n",
        "a `while` loop in a const fn",
    );
    both(
        "use std::io;\n\
         const fn maxi(a: int, b: int) -> int { if a > b { return a; } return b; }\n\
         fn main() { let v: t<maxi(9, 18)> = 193710244; io::println_int(v as int); }\n",
        "193710244\n",
        "an `if` with a `return` — and 193710244 needs all 18 trits",
    );
}

/// **Termination is bought, not assumed.**
///
/// A `const fn` that does not terminate exhausts a step budget and is
/// reported. Without it the compiler would hang on a program, which is the one
/// failure mode a compile-time evaluator must not have.
#[test]
fn b4_a_non_terminating_const_fn_is_a_diagnostic_not_a_hang() {
    let (ok, msg) = check(
        "const fn spin(n: int) -> int { let mut i = 0; while i >= 0 { i = i + 1; } return i; }\n\
         fn main() { let _a: [int; spin(1)] = [1]; }\n",
    );
    assert!(!ok, "a non-terminating const fn must be refused");
    assert!(
        msg.contains("evaluation steps") && msg.contains("must terminate"),
        "and say why, so the remedy is obvious: {msg}"
    );
}

// ---------------------------------------------------------------------------
// 2. Constant expressions in type position
// ---------------------------------------------------------------------------

/// Arithmetic, a module-level constant, and a `const fn`, in both positions.
#[test]
fn b4_the_fragment_reaches_both_type_positions() {
    both(
        "use std::io;\n\
         let N: int = 3;\n\
         fn main() {\n\
         \x20 let a: [int; 2 + 1] = [1, 2, 3];\n\
         \x20 let b: [int; N] = [4, 5, 6];\n\
         \x20 let c: [int; N * 2] = [1, 2, 3, 4, 5, 6];\n\
         \x20 io::println_int(a[2]); io::println_int(b[2]); io::println_int(c[5]);\n\
         }\n",
        "3\n6\n6\n",
        "arithmetic, a constant, and a product as lengths",
    );
}

/// **A width's fragment stops below comparison, and that is what removes the
/// ambiguity.**
///
/// `t<A + 1>` closes on its own `>` because `<` and `>` cannot be operators
/// inside a width — a width is a number, never a bool. Rust reaches for braces
/// (`t<{A + 1}>`) to solve the same problem. A row asserts the refusal names
/// this rather than reporting a stray token.
#[test]
fn b4_a_comparison_inside_a_width_is_the_bracket_not_an_operator() {
    let (ok, msg) = check("fn main() { let _x: t<1 < 2> = 1; }\n");
    assert!(!ok, "`t<1 < 2>` must be refused");
    assert!(
        msg.contains("stops below comparison") || msg.contains("not a valid trit width"),
        "and explain which `>` closed it: {msg}"
    );
}

/// A constant expression that is broken is reported where it is written.
#[test]
fn b4_a_broken_constant_expression_names_its_fault() {
    for (src, want) in [
        ("fn main() { let _a: [int; 1 / 0] = [1]; }\n", "divides by zero"),
        ("fn main() { let _a: [int; 0 - 3] = [1]; }\n", "cannot be negative"),
        ("fn main() { let _x: t<0 + 99> = 1; }\n", "width runs from 1 to 54"),
        (
            "fn f(x: int) -> int { return x; }\nfn main() { let _a: [int; f(1)] = [1]; }\n",
            "is not declared `const fn`",
        ),
    ] {
        let (ok, msg) = check(src);
        assert!(!ok, "must be refused: {src}");
        assert!(msg.contains(want), "wanted {want:?}, got: {msg}");
    }
}

// ---------------------------------------------------------------------------
// 3. Two evaluators, one answer
// ---------------------------------------------------------------------------

/// **Permanent rule 5.** `const_fold` folds a checked expression for globals
/// and array bounds; `const_eval` folds an AST expression in type position.
/// Two things that must agree get a test rather than a comment.
///
/// The same arithmetic is put through both — as a module-level `let`
/// initialiser, which is `const_fold`'s path, and as an array length, which is
/// `const_eval`'s — and the program prints both. A disagreement shows as two
/// different numbers on one line.
#[test]
fn b4_the_two_evaluators_agree() {
    // Each row is (expression, its value). Every operator both modules
    // implement is here, including the ones where a sign or a rounding rule
    // could differ.
    const CASES: &[(&str, i64)] = &[
        ("2 + 3", 5),
        ("10 - 4", 6),
        ("6 * 7", 42),
        ("20 / 6", 3),
        ("20 % 6", 2),
        ("2 + 3 * 4", 14),
        ("(2 + 3) * 4", 20),
        ("0 - 5 + 12", 7),
    ];
    for (expr, want) in CASES {
        // `const_fold`'s path: a module-level `let`, folded and emitted.
        let via_fold = run_t3(&format!(
            "use std::io;\nlet K: int = {expr};\nfn main() {{ io::println_int(K); }}\n"
        ));
        assert_eq!(via_fold, format!("{want}\n"), "const_fold on `{expr}`");

        // `const_eval`'s path: the same expression as an array length, read
        // back through the array's own size.
        let elems = (1..=*want).map(|i| i.to_string()).collect::<Vec<_>>().join(", ");
        let via_eval = run_t3(&format!(
            "use std::io;\nfn main() {{ let a: [int; {expr}] = [{elems}]; \
             io::println_int(a[{}]); }}\n",
            want - 1
        ));
        assert_eq!(via_eval, format!("{want}\n"), "const_eval on `{expr}`");
    }
}

// ---------------------------------------------------------------------------
// 4. What is not here
// ---------------------------------------------------------------------------

/// **LIMIT.** An expression over a parameter cannot be INFERRED from.
///
/// `fn f<const A: int>(x: t<A + 1>)` called with a `t9` would need the compiler
/// to invert `A + 1`, and it does not. Only a bare parameter binds. The row
/// records it and goes red the day inversion lands.
#[test]
fn b4_an_expression_width_is_checked_never_inferred_from() {
    let (ok, msg) = check(
        "fn f<const A: int>(x: t<A + 1>) -> int { return A; }\n\
         fn main() { let a: t9 = 1; let _ = f(a); }\n",
    );
    assert!(!ok, "nothing binds `A` through `A + 1`");
    assert!(
        msg.contains("does not pin down"),
        "and it is the const-argument check that says so: {msg}"
    );
}

/// **LIMIT.** `const` is contextual, so a bare `const` is still a name.
///
/// **This row passes on the compiler without B4, by construction** (rule 9):
/// it asserts that nothing moved. `const fn` occurs zero times across both
/// repositories and the 2,507-program corpus, so making the pair meaningful
/// could break nothing — and this row is what says the single word still
/// cannot.
#[test]
fn b4_const_is_still_a_name_on_its_own() {
    let (ok, msg) = check(
        "fn main() { let const = 7; let _y = const + 1; }\n",
    );
    assert!(ok, "`const` alone must still be an identifier: {msg}");
}
