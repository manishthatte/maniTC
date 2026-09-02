//! A21 — `i64::MIN` could not be written.
//!
//! © Manish Jagdish Thatte
//!
//! The lexer saw `-9223372036854775808` as a unary minus applied to
//! `9223372036854775808`, which is `i64::MAX + 1` and rejected before the
//! parser ever reached the minus:
//! `invalid integer literal: 9223372036854775808`. `i64::MAX` itself was fine,
//! which is the asymmetry that identifies the cause.
//!
//! Half of the original report is stale and stays recorded: it said
//! `stdlib/math.mt` "had never passed check" because it declared `INT_MIN`
//! that way. Under N5 an `int` IS 27 trits, so `INT_MIN` is now the 27-trit
//! minimum and that declaration was wrong independently of the lexer. What
//! remained is narrow and real: a `t54` or `--lang v1` context still could not
//! spell the most negative machine word.
//!
//! **The fold is attempted only when it is the difference between a value and
//! an error** — when the unsigned parse overflows `i64` and the signed one
//! succeeds, which is true of exactly one magnitude, `2^63`, and that magnitude
//! is an error today. So the blast radius is one literal that no program can
//! currently contain. Every other negative literal keeps its `Minus` token,
//! which matters: `parse_unary_expr` reads `-` followed by a delimiter as the
//! trit literal `-1`, and folding eagerly would take that away.
//!
//! **Prefix position is decided from the token BEFORE the minus**, so
//! `x - 9223372036854775808` keeps its error instead of quietly becoming
//! `x - (-9223372036854775808)`. The rows below assert both directions of that,
//! because a fold that fires in infix position would change what a program
//! MEANS rather than what it accepts.
//!
//! **It also made a latent panic reachable**, which is recorded here rather
//! than only in `report.txt`: the ternary range check in
//! `semantic/analyzer/stmts.rs` negated a literal with `-*v`, unreachable while
//! no literal could be `i64::MIN`, and `-(-9223372036854775808)` crashed the
//! compiler with "attempt to negate with overflow". *A defect behind an
//! unreachable one is still a defect, and it becomes reachable the moment the
//! outer one is fixed.*

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
    let d = common::suite_root("a21")
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

/// `manitc check` only: returns the diagnostics, and whether it exited 0.
fn check(src: &str) -> (bool, String) {
    let d = workdir();
    let path = d.join("p.mt");
    std::fs::write(&path, src).expect("write");
    let c = Command::new(manitc_bin())
        .args(["check", path.to_str().unwrap()])
        .output()
        .expect("check");
    (c.status.success(),
     format!("{}{}", String::from_utf8_lossy(&c.stdout),
             String::from_utf8_lossy(&c.stderr)))
}

/// Assert both backends agree on `want`.
fn both(src: &str, want: &str, what: &str) {
    let t3 = run_t3(src);
    assert!(t3.contains(want), "{what}: T3 gave {t3:?}, wanted {want:?}");
    if let Some(ll) = run_llvm(src) {
        assert!(ll.contains(want), "{what}: LLVM gave {ll:?}, wanted {want:?}");
    }
}


// ---------------------------------------------------------------------------
// The value that could not be written
// ---------------------------------------------------------------------------

/// LLVM only, and the reason is not a limitation of this fix.
///
/// A T3 machine word is 27 trits, about ±3.8e12, so `i64::MIN` is not a value
/// that backend can hold or print — `i64::MAX` traps there too, on the
/// pre-fix compiler as well. `both()` would be asserting something about the
/// word size rather than about the lexer.
fn llvm_only(src: &str, want: &str, what: &str) {
    if let Some(out) = run_llvm(src) {
        assert!(out.contains(want), "{what}: LLVM gave {out:?}, wanted {want:?}");
    }
}

#[test]
fn a21_the_most_negative_machine_word_can_be_written() {
    llvm_only(
        "fn main() { let x: t54 = -9223372036854775808; io::println_int(x as int); }",
        "-9223372036854775808",
        "i64::MIN as a literal",
    );
}

#[test]
fn a21_the_most_positive_machine_word_still_can() {
    // The control for the row above: `i64::MAX` was always accepted, and the
    // asymmetry between the two is what identified the cause. It must not
    // become a casualty of making the other one work.
    llvm_only(
        "fn main() { let x: t54 = 9223372036854775807; io::println_int(x as int); }",
        "9223372036854775807",
        "i64::MAX as a literal",
    );
}

/// A GUARD row: green on the pre-fix compiler, which refused this and
/// everything near it. Its value is in the other direction — it is what fails
/// if the fold is ever widened into "accept anything that fits with a sign".
#[test]
fn a21_one_past_the_minimum_is_still_refused() {
    // The fold must not become "accept anything that fits with a sign on it".
    // `-9223372036854775809` has no i64 value either way.
    let (ok, msg) = check("fn main() { let x: t54 = -9223372036854775809; }");
    assert!(!ok, "A21: a literal below i64::MIN was accepted\n{msg}");
    assert!(
        msg.contains("invalid integer literal"),
        "A21: refused, but not as an invalid literal:\n{msg}"
    );
}

/// A GUARD row: green on the pre-fix compiler.
#[test]
fn a21_an_unsigned_out_of_range_literal_is_still_refused() {
    // Without a minus in front of it, `2^63` is exactly as invalid as it was.
    let (ok, msg) = check("fn main() { let x: t54 = 9223372036854775808; }");
    assert!(!ok, "A21: a bare out-of-range literal was accepted\n{msg}");
    assert!(msg.contains("invalid integer literal"), "A21: wrong reason:\n{msg}");
}

// ---------------------------------------------------------------------------
// Prefix versus infix — the half that decides whether this is safe
// ---------------------------------------------------------------------------

#[test]
fn a21_the_fold_fires_only_in_prefix_position() {
    // A minus is prefix or infix depending on the token BEFORE it. Every
    // position below is one or the other, and getting this wrong in the infix
    // direction would not change what the compiler ACCEPTS — it would change
    // what an accepted program MEANS, turning `a - 2^63` into `a + 2^63`.
    for (what, src, want_ok) in [
        ("after `=`",       "fn main() { let x: t54 = -9223372036854775808; }", true),
        ("after `(`",       "fn main() { let x: t54 = (-9223372036854775808); }", true),
        ("after `[`",       "fn main() { let a: [t54; 1] = [-9223372036854775808]; }", true),
        ("after `,`",       "fn f(a: t54, b: t54) {} fn main() { f(1, -9223372036854775808); }", true),
        ("after `return`",  "fn g() -> t54 { return -9223372036854775808; } fn main() { let z = g(); }", true),
        ("after `=>`",      "fn main() { let f = fn(x: t54) => -9223372036854775808; }", true),
        // infix: the minus belongs to the expression, not to the literal
        ("after an ident",  "fn main() { let a: t54 = 1; let x: t54 = a -9223372036854775808; }", false),
        ("after `)`",       "fn g() -> t54 { return 1; } fn main() { let x: t54 = g() -9223372036854775808; }", false),
    ] {
        let (ok, msg) = check(src);
        assert_eq!(
            ok, want_ok,
            "A21: {what}: expected accepted={want_ok}\n{msg}"
        );
    }
}

/// A GUARD row: green on the pre-fix compiler. It pins the blast radius —
/// these are the forms an eager fold would have changed.
#[test]
fn a21_ordinary_negative_literals_are_untouched() {
    // The fold leaves every other literal alone, and these are the forms that
    // would break if it did not. The trit literal is the sharp one: the parser
    // reads `-` followed by a delimiter as `Trit(-1)`, so folding eagerly would
    // remove the token it keys on.
    both("fn main() { let x: int = -5; io::println_int(x); }", "-5", "negative int literal");
    both("fn main() { let t: trit = -1; io::print_trit(t); io::println(\"\"); }",
         "-", "the trit literal -1");
    both("fn main() { let a: int = 10; io::println_int(a - 3); }", "7", "ordinary subtraction");
}

// ---------------------------------------------------------------------------
// The latent panic this made reachable
// ---------------------------------------------------------------------------

#[test]
fn a21_negating_the_minimum_does_not_crash_the_compiler() {
    // `semantic/analyzer/stmts.rs` computed `-*v` for the ternary range check.
    // Unreachable while no literal could be `i64::MIN`; the moment one could,
    // `-(-9223372036854775808)` panicked the compiler with "attempt to negate
    // with overflow" — worse than the lex error it replaced.
    //
    // The row asserts the compiler SURVIVES and the program means what a `t54`
    // overflow means everywhere else. `t54` is the unchecked machine word by
    // design, and `-(i64::MIN)` wraps to `i64::MIN` exactly as
    // `i64::MAX + 1` already did.
    for src in [
        "fn main() { let x: t54 = --9223372036854775808; }",
        "fn main() { let x: t54 = -(-9223372036854775808); }",
    ] {
        let (_, msg) = check(src);
        assert!(
            !msg.contains("panicked") && !msg.contains("negate with overflow"),
            "A21: the compiler crashed on a negated minimum:\n{msg}"
        );
    }
    llvm_only(
        "fn main() { let x: t54 = -(-9223372036854775808); io::println_int(x as int); }",
        "-9223372036854775808",
        "negating the minimum wraps, as every other t54 overflow does",
    );
}
