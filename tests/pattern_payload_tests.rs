//! P90 — a literal in an enum pattern's payload.
//!
//! © Manish Jagdish Thatte
//!
//! `Err("closed")` matched every `Err`, on every enum, for every literal type,
//! and `manitc check` called the arm exhaustive. Two halves of one defect: the
//! lowerer discarded the payload sub-patterns (`Pattern::Enum(v, e, _, _)`) and
//! the exhaustiveness checker discarded them at the same time, so the wrong arm
//! ran AND no arm was reported missing. They are fixed together because either
//! alone is unsound — testing payloads without tightening exhaustiveness turns
//! a wrong answer into a match that falls off its end.
//!
//! **Every row runs on BOTH backends** unless its comment says why it cannot.
//! The pair matters here for the reason P89's rows give: a defect that lives in
//! the shared lowerer shows up identically on both, so a single-backend row
//! cannot tell "fixed" from "this backend never had it".

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static N: AtomicUsize = AtomicUsize::new(0);

fn manitc_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_manitc"))
}

fn workdir() -> PathBuf {
    let slot = N.fetch_add(1, Ordering::Relaxed);
    let d = std::env::temp_dir()
        .join(format!("manitc_p90_{}", std::process::id()))
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
// The literal is tested, for every literal type and every enum kind
// ---------------------------------------------------------------------------

#[test]
fn p90_str_literal_payload_selects_the_right_arm() {
    // The original report. `Err("empty")` must not take the `Err("closed")`
    // arm. Control: prints "MATCHED closed" on both backends.
    both(r#"
fn main() {
    let r: Result<int, str> = Err("empty");
    match r {
        Ok(v)         => io::println("ok"),
        Unknown(m)    => io::println("unknown"),
        Err("closed") => io::println("MATCHED closed"),
        Err(_)        => io::println("other"),
    }
}
"#, "other", "str literal payload");
}

#[test]
fn p90_int_literal_payload_selects_the_right_arm() {
    // The report named STRINGS. The defect is not string-specific: it is the
    // payload sub-pattern being discarded, whatever it holds.
    both(r#"
fn main() {
    let r: Result<int, int> = Err(7);
    match r {
        Ok(v)      => io::println("ok"),
        Unknown(m) => io::println("unknown"),
        Err(9)     => io::println("matched nine"),
        Err(_)     => io::println("other"),
    }
}
"#, "other", "int literal payload");
}

#[test]
fn p90_float_literal_payload_selects_the_right_arm() {
    // T3 ONLY, and the reason is a SEPARATE pre-existing defect rather than
    // this one: constructing a `Result` with a FLOAT payload emits
    // `store i64 0x3FF8000000000000`, and `0x...` is floating-point syntax in
    // LLVM IR, invalid for `i64`. clang rejects the module. It fails
    // identically on the control binary and with no literal pattern present
    // at all, so it is not P90 and not this fix.
    let out = run_t3(r#"
fn main() {
    let r: Result<float, str> = Ok(1.5);
    match r {
        Ok(2.5)    => io::println("matched two point five"),
        Ok(v)      => io::println("other"),
        Unknown(m) => io::println("unknown"),
        Err(e)     => io::println("err"),
    }
}
"#);
    assert!(out.contains("other"), "float literal payload: T3 gave {out:?}");
}

#[test]
fn p90_user_enum_literal_payload_selects_the_right_arm() {
    // Not just `Result`: the same discard sat on the user-enum path, which
    // reaches the payload through a BOXED cell (P43).
    both(r#"
enum Shape { Circle(int), Square(int) }
fn main() {
    let s: Shape = Shape::Circle(3);
    match s {
        Shape::Circle(9) => io::println("matched nine"),
        Shape::Circle(r) => io::println("other"),
        Shape::Square(x) => io::println("square"),
    }
}
"#, "other", "user enum literal payload");
}

#[test]
fn p90_tuple_literal_payload_selects_the_right_arm() {
    // `Pattern::Tuple` had NO lowering at all -- it fell to the `_ => true`
    // catch-all, so `Ok((9, 9))` matched `Ok((1, 2))`. That catch-all is now
    // gone, which is what makes this class closed rather than narrowed.
    both(r#"
fn main() {
    let r: Result<(int, int), str> = Ok((1, 2));
    match r {
        Ok((9, 9)) => io::println("matched nine nine"),
        Ok(p)      => io::println("other"),
        Unknown(m) => io::println("unknown"),
        Err(e)     => io::println("err"),
    }
}
"#, "other", "tuple literal payload");
}

// ---------------------------------------------------------------------------
// The row that separates this design from the obvious one
// ---------------------------------------------------------------------------

#[test]
fn p90_a_payload_test_never_runs_before_its_tag() {
    // THE design row. Testing the payload eagerly and ANDing the result is
    // shorter and it segfaults: this scrutinee is `Ok(5)`, so word 1 of the
    // cell is the INTEGER 5, and an eager `Err("closed")` test hands 5 to
    // `StrEq`, which dereferences it as a `char*`. The payload test is
    // guarded by the tag, so it never runs here.
    //
    // Passes on the control too -- deliberately. It is not evidence that the
    // fix works; it is the row that goes red if the fix is ever rewritten
    // eagerly, which is the mistake available at exactly this spot.
    both(r#"
fn main() {
    let r: Result<int, str> = Ok(5);
    match r {
        Err("closed") => io::println("err closed"),
        Ok(v)         => io::println("ok"),
        Unknown(m)    => io::println("unknown"),
        Err(e)        => io::println("err"),
    }
}
"#, "ok", "cross-variant payload guard");
}

// ---------------------------------------------------------------------------
// Exhaustiveness -- the half without which the fix is unsound
// ---------------------------------------------------------------------------

#[test]
fn p90_a_literal_payload_arm_does_not_cover_its_variant() {
    // Without this half the program below compiles and then matches NOTHING
    // at run time: a wrong answer would have become a silent fall-through.
    // The control accepts it and prints "closed".
    let (ok, msg) = check(r#"
fn main() {
    let r: Result<int, str> = Err("empty");
    match r {
        Ok(v)         => io::println("ok"),
        Unknown(m)    => io::println("unknown"),
        Err("closed") => io::println("closed"),
    }
}
"#);
    assert!(!ok, "a literal-payload arm must not cover its variant; check passed:\n{msg}");
    assert!(msg.contains("non-exhaustive match on `Result`") && msg.contains("Err"),
            "the diagnostic must name the variant still missing, got:\n{msg}");
}

#[test]
fn p90_an_irrefutable_payload_arm_still_covers_its_variant() {
    // The other direction, and it is the one over-tightening would break:
    // `Err(e)` and `Err(_)` accept every `Err` and must go on counting. Every
    // `match` on a `Result` in both repositories has this shape.
    let (ok, msg) = check(r#"
fn main() {
    let r: Result<int, str> = Err("empty");
    match r {
        Ok(v)      => io::println("ok"),
        Unknown(m) => io::println("unknown"),
        Err(e)     => io::println("err"),
    }
}
"#);
    assert!(ok, "an irrefutable payload arm must still cover its variant:\n{msg}");
}

#[test]
fn p90_a_user_enum_literal_payload_arm_does_not_cover_its_variant() {
    let (ok, msg) = check(r#"
enum Shape { Circle(int), Square(int) }
fn main() {
    let s: Shape = Shape::Circle(3);
    match s {
        Shape::Circle(9) => io::println("nine"),
        Shape::Square(x) => io::println("square"),
    }
}
"#);
    assert!(!ok, "a literal-payload arm must not cover a user enum variant; check passed:\n{msg}");
    assert!(msg.contains("Circle"),
            "the diagnostic must name the variant still missing, got:\n{msg}");
}

// ---------------------------------------------------------------------------
// Tuple binding -- `manitc check` said OK and both backends failed at link
// ---------------------------------------------------------------------------

#[test]
fn p90_a_tuple_pattern_binds_its_elements() {
    // `bind_pattern_locals` had no `Tuple` arm, so `Ok((a, b))` bound NEITHER
    // name and both resolved to globals. The failure surfaced past codegen --
    // `Cannot resolve: a` from the T3 assembler, `global variable reference
    // must have pointer type` from clang -- while `manitc check` exited 0.
    both(r#"
fn main() {
    let r: Result<(int, int), str> = Ok((4, 7));
    match r {
        Ok((a, b)) => io::println(str::from_int(a * 100 + b)),
        Unknown(m) => io::println("unknown"),
        Err(e)     => io::println("err"),
    }
}
"#, "407", "tuple element binding");
}

#[test]
fn p90_a_nested_payload_binds_from_the_payload_not_the_tag() {
    // The nested branch of the enum binder passed the CELL down instead of the
    // payload word, so a nested pattern read `[tag, payload]` and bound the
    // tag. Invisible while no nested pattern bound anything at all: there was
    // no Tuple arm, and a struct payload is not constructible. Two defects
    // hiding each other -- the answer here was 63100, not 407.
    let out = run_t3(r#"
fn main() {
    let r: Result<(int, int), str> = Ok((4, 7));
    match r {
        Ok((a, b)) => io::println(str::from_int(a * 100 + b)),
        Unknown(m) => io::println("unknown"),
        Err(e)     => io::println("err"),
    }
}
"#);
    assert!(out.contains("407") && !out.contains("63100"),
            "nested payload must be read from the payload word, got {out:?}");
}
