//! P95 — a type name that is declared nowhere, and P98 — one declared later.
//!
//! © Manish Jagdish Thatte
//!
//! `struct Holder { pub a: NoSuchType, pub b: int }` type-checked, `manitc
//! check` exited 0, and both backends ran the program. The name resolved to
//! `ManiType::Unknown`, which is compatible with everything, so nothing
//! downstream objected either and the field simply held whatever it was given.
//! The instance that motivated it was five weeks old and shipped: a `pub buf:
//! Buffer` field naming a type defined nowhere in its repository.
//!
//! **THIRTEEN POSITIONS, not the four reported.** The rows below iterate them:
//! a struct field, a parameter, a return type, a `let` annotation, an enum
//! variant payload, an array element, a generic argument, a tuple element, a
//! function type's parameter, an `impl` target, a cast target, a global's
//! annotation, and a generic struct's argument. Six of them resolve only while
//! a BODY is checked, which is why the compiler runs the report twice.
//!
//! **The refusal could not go in the resolver's fallback**, and that is the
//! finding rather than an implementation note: `register_native_module_sigs`
//! resolves every native stdlib signature against tables holding only the
//! user's declarations, which is 3,850 arrivals at that fallback per
//! compilation. Tightening it would reject the standard library. The names the
//! user actually wrote are told apart by `Span::module`, which is what P80
//! bought — and P95 had to close P80's SECOND site first, because that scan
//! lexed through `Lexer::with_file`, which sets the file name and leaves the
//! module `None`.
//!
//! **P98 is the same `Unknown`, reached by declaration ORDER.** `struct A { pub
//! b: B }` before `struct B` resolved `B` against a table that did not hold it
//! yet, stored `Unknown`, exited `check` 0 and then PANICKED the compiler in
//! `field_slot_index` (P44's assertion) or read slot 0 in release. No shipped
//! `.mt` file forward-references a struct, which is why it was never seen.
//!
//! Rows assert a VALUE or a specific diagnostic, never a bare exit status
//! (permanent rule 8), and the `allow` rows assert both directions.

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
        .join(format!("manitc_p95_{}", std::process::id()))
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
// The thirteen positions
// ---------------------------------------------------------------------------

/// Each entry is a complete program naming `NoSuchType` in one type position.
const POSITIONS: &[(&str, &str)] = &[
    ("struct field",
     "struct H { pub a: NoSuchType, pub b: int }\nfn main() { let h = H { a: 1, b: 2 }; io::print_int(h.b); }"),
    ("parameter",
     "fn f(x: NoSuchType) -> int { return 1; }\nfn main() { io::print_int(f(5)); }"),
    ("return type",
     "fn f() -> NoSuchType { return 1; }\nfn main() { let v = f(); }"),
    ("let annotation",
     "fn main() { let x: NoSuchType = 5; io::print_int(1); }"),
    ("enum variant payload",
     "enum E { A(NoSuchType), B }\nfn main() { let e = E::B; }"),
    ("array element",
     "fn main() { let a: [NoSuchType; 3] = [1,2,3]; io::print_int(1); }"),
    ("generic argument",
     "fn main() { let v: Vec<NoSuchType> = Vec::new(); io::print_int(1); }"),
    ("tuple element",
     "fn main() { let t: (NoSuchType, int) = (1, 2); io::print_int(1); }"),
    ("function type parameter",
     "fn g(x: int) -> int { return x; }\nfn main() { let f: fn(NoSuchType) -> int = g; }"),
    ("impl target",
     "impl NoSuchType { fn m(self) -> int { return 1; } }\nfn main() { io::print_int(1); }"),
    ("cast target",
     "fn main() { let x: int = 5; let y = x as NoSuchType; io::print_int(1); }"),
    ("global annotation",
     "let G: NoSuchType = 5;\nfn main() { io::print_int(1); }"),
    ("generic struct argument",
     "struct Box2<T> { pub v: T }\nfn main() { let b: Box2<NoSuchType> = Box2 { v: 1 }; }"),
];

#[test]
fn p95_an_undeclared_type_is_refused_in_every_type_position() {
    // Derived from the table rather than written out thirteen times, so a new
    // position cannot be added to the list without being asserted.
    for (what, src) in POSITIONS {
        let (ok, msg) = check(src);
        assert!(!ok, "P95: {what}: `manitc check` accepted an undeclared type\n{msg}");
        assert!(
            msg.contains("`NoSuchType` names no type"),
            "P95: {what}: refused, but not for this reason:\n{msg}"
        );
    }
}

#[test]
fn p95_the_allow_restores_the_previous_compiler() {
    // Both directions, on one program: `deny` refuses it and `allow` accepts
    // it AND it still runs. An escape hatch that merely silenced the
    // diagnostic while changing what the program does would be worse than the
    // defect.
    let src = "struct H { pub a: NoSuchType, pub b: int }\n\
               fn main() { let h = H { a: 7, b: 42 }; io::print(\"b=\"); io::println_int(h.b); }";
    let (ok, msg) = check(src);
    assert!(!ok, "P95: expected a refusal without the allow\n{msg}");

    let allowed = format!("lint allow(undeclared-type);\n{src}");
    let (ok, msg) = check(&allowed);
    assert!(ok, "P95: `lint allow(undeclared-type);` did not restore acceptance\n{msg}");
    both(&allowed, "b=42", "allow restores the previous behaviour");
}

#[test]
fn p95_the_diagnostic_names_the_type_and_suggests_a_neighbour() {
    // Asserting the MESSAGE, not the exit status: the whole defect was that
    // the compiler said nothing, so "it failed" is not the property under test.
    let (ok, msg) = check(
        "struct Point { pub x: int, pub y: int }\n\
         fn main() { let p: Pointt = Point { x: 1, y: 2 }; }",
    );
    assert!(!ok, "P95: expected a refusal\n{msg}");
    assert!(msg.contains("`Pointt` names no type"), "P95: not named:\n{msg}");
    assert!(msg.contains("did you mean 'Point'"), "P95: no suggestion:\n{msg}");
    assert!(
        msg.contains("lint allow(undeclared-type)"),
        "P95: the diagnostic does not say how to restore the old behaviour:\n{msg}"
    );
}

// ---------------------------------------------------------------------------
// What must STILL be accepted — the half that decides whether this is usable
// ---------------------------------------------------------------------------

/// Every spelling the compiler answers for without a declaration.
///
/// Asked of the COMPILER rather than asserted against the constant it is
/// derived from, which is the point (P64: agreement between two things with a
/// common origin carries no information). If a name is dropped from
/// `BUILTIN_TYPE_NAMES` this row fails, because the compiler will then refuse a
/// primitive.
const BUILTIN_SPELLINGS: &[&str] = &[
    "int", "i64", "float", "f64", "bool", "bool3", "tribool", "T3Bool",
    "trit", "tryte", "t9", "t27", "word", "t54", "trint", "tfloat",
    "str", "String", "char",
];

/// A GUARD row: it passes on the pre-fix compiler, which refused nothing at
/// all. Its value is entirely in the other direction — this is the row that
/// fails if the accepted set is ever narrowed and a primitive starts being
/// reported as undeclared.
#[test]
fn p95_every_builtin_spelling_is_still_a_type() {
    for name in BUILTIN_SPELLINGS {
        let src = format!("fn f(x: {name}) -> int {{ return 1; }}\nfn main() {{ io::print_int(1); }}");
        let (_, msg) = check(&src);
        assert!(
            !msg.contains("names no type"),
            "P95: the builtin spelling `{name}` was reported as undeclared:\n{msg}"
        );
    }
}

/// A GUARD row in the same sense: green on the pre-fix compiler by default.
/// It is what turned nineteen false positives into zero — each of the four
/// scopes below was a real refusal of correct code during development, and
/// the R5 sweep is what found them.
#[test]
fn p95_generic_parameters_are_not_undeclared_types() {
    // The four scopes a type parameter can come from, and every one of them
    // reached the resolver's fallback before this work: a generic struct's own
    // parameter while its FIELDS are resolved, a free function's, an `impl<T>`
    // block's while its methods are CHECKED (`check_fn` opens with
    // `std::mem::take(&mut self.type_params)`, so binding them around the loop
    // does nothing and they must arrive ON the method), and a method's own
    // while a BODY-LESS native signature is registered.
    for (what, src) in [
        ("generic struct field",
         "struct Pair<A, B> { pub first: A, pub second: B }\n\
          fn main() { let p = Pair { first: 1, second: 2 }; io::print_int(p.first); }"),
        ("generic free function",
         "fn id<T>(x: T) -> T { return x; }\nfn main() { io::print_int(id(7)); }"),
        ("impl block parameter",
         "struct Pair<T> { pub first: T, pub second: T }\n\
          impl<T> Pair<T> { fn swap(self) -> Pair<T> { Pair { first: self.second, second: self.first } } }\n\
          fn main() { let p = Pair { first: 1, second: 2 }; let q = p.swap(); io::print_int(q.first); }"),
        ("method's own parameter",
         "struct Holder<T> { pub v: T }\n\
          impl<T> Holder<T> { fn map<U>(self, f: fn(T) -> U) -> U { return f(self.v); } }\n\
          fn main() { io::print_int(1); }"),
    ] {
        let (_, msg) = check(src);
        assert!(
            !msg.contains("names no type"),
            "P95: {what}: a type parameter was reported as undeclared:\n{msg}"
        );
    }
}

/// A GUARD row: green on the pre-fix compiler.
#[test]
fn p95_a_stdlib_type_is_not_undeclared() {
    // A name a stdlib module declares but the compiled program did not: the
    // accepted set is read from the embedded library rather than listed, so a
    // module gaining a type cannot silently fall out of step.
    let (_, msg) = check(
        "fn f(d: Duration) -> int { return 1; }\nfn main() { io::print_int(1); }",
    );
    assert!(
        !msg.contains("names no type"),
        "P95: `Duration`, declared in stdlib/time.mt, was reported as undeclared:\n{msg}"
    );
}

// ---------------------------------------------------------------------------
// P98 — the same `Unknown`, reached by declaration order
// ---------------------------------------------------------------------------

#[test]
fn p98_a_struct_may_refer_to_one_declared_later() {
    // Asserts the VALUE. Before this, `check` exited 0 and the compiler then
    // panicked in `field_slot_index: no slot for field 'x' on '<unknown>'` —
    // P44's assertion — or read slot 0 in a release build. It must not merely
    // be REFUSED now: a forward reference is a declared type, so refusing it
    // would be P95 firing where it should not.
    both(
        "struct A { pub b: B, pub n: int }\n\
         struct B { pub x: int }\n\
         fn main() {\n\
         \x20   let bb = B { x: 5 };\n\
         \x20   let aa = A { b: bb, n: 1 };\n\
         \x20   io::print(\"x=\"); io::println_int(aa.b.x);\n\
         }",
        "x=5",
        "a struct field may name a struct declared later",
    );
}

/// THIS ROW PASSES ON THE PRE-FIX COMPILER, and it is here to record the
/// boundary rather than the fix (permanent rule 9 — a row that passes on the
/// compiler without the fix is not evidence for it, and saying so is cheaper
/// than letting a green suite imply otherwise).
///
/// Measured: a forward-declared ENUM already worked, `match` included. P98 is
/// struct-specific, because what breaks is `field_slot_index` on the field
/// whose TYPE resolved to `Unknown` — an enum value is a tagged cell and its
/// arm selection never asks the struct table. The row exists so that stays
/// true.
#[test]
fn p98_an_enum_declared_later_is_reachable_too() {
    both(
        "struct Wrap { pub e: Colour, pub n: int }\n\
         enum Colour { Red, Green }\n\
         fn main() {\n\
         \x20   let w = Wrap { e: Colour::Green, n: 3 };\n\
         \x20   io::print(\"n=\"); io::println_int(w.n);\n\
         }",
        "n=3",
        "a struct field may name an enum declared later",
    );
}

#[test]
fn p98_a_genuinely_undeclared_name_is_still_refused_after_the_prepass() {
    // The boundary between P95 and P98: pre-registering names must not turn
    // "declared nowhere" into "declared later". Without this row the P98 fix
    // could have been written as "accept anything" and both P98 rows above
    // would still pass.
    let (ok, msg) = check(
        "struct A { pub b: NoSuchType, pub n: int }\nfn main() { io::print_int(1); }",
    );
    assert!(!ok, "P95/P98: a name declared nowhere was accepted\n{msg}");
    assert!(msg.contains("`NoSuchType` names no type"), "P95/P98: wrong reason:\n{msg}");
}
