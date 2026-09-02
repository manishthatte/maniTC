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

mod common;

static N: AtomicUsize = AtomicUsize::new(0);

fn manitc_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_manitc"))
}

fn workdir() -> PathBuf {
    let slot = N.fetch_add(1, Ordering::Relaxed);
    let d = common::suite_root("p95")
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

/// `manitc check` with extra command-line arguments.
fn check_with(src: &str, extra: &[&str]) -> (bool, String) {
    let d = workdir();
    let path = d.join("p.mt");
    std::fs::write(&path, src).expect("write");
    let mut args = vec!["check".to_string(), path.to_string_lossy().into_owned()];
    args.extend(extra.iter().map(|s| s.to_string()));
    let c = Command::new(manitc_bin()).args(&args).output().expect("check");
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

// ---------------------------------------------------------------------------
// P103 — a field name the struct does not have
// ---------------------------------------------------------------------------
//
// P95 one level in. That refuses a TYPE name nothing declares; this refuses a
// FIELD name a perfectly well declared struct does not have. Both end in
// `ManiType::Unknown`, and this one then reaches `field_slot_index`, which has
// no slot for it and reads SLOT 0 — so the program RUNS and returns a
// different field's value, on both backends, with `check` exiting 0.
//
// `field_slot_index` has carried a `debug_assert!` for this since P44, and it
// is DEBUG-ONLY while `thatteos/build.sh` resolves the compiler to
// `target/release/manitc`. Two thatteOS syscalls shipped reading the wrong
// field because of that gap (report.txt P102(b)).

#[test]
fn p103_a_field_the_struct_does_not_have_is_refused() {
    let (ok, msg) = check(
        "struct Desc { pub fd: int, pub ino: int, pub open: bool }\n\
         fn mk() -> Desc { return Desc { fd: 3, ino: 9, open: true }; }\n\
         fn main() { let d = mk(); if !d.valid { io::print_int(0); } }",
    );
    assert!(!ok, "P103: a field the struct does not have was accepted\n{msg}");
    assert!(msg.contains("has no field `valid`"), "P103: wrong reason:\n{msg}");
}

/// `lint allow(undeclared-field);` restores the previous compiler exactly —
/// asserted as a PAIR, same program, so "accepted" cannot be satisfied by a
/// compiler that accepts everything.
///
/// **The VALUE is deliberately not asserted here, and the reason is worth
/// recording.** The old behaviour was `!d.valid` reading SLOT 0 — `fd` — which
/// is the thatteOS shape exactly: with `fd: 0` the guard fires when it must
/// not, and with any other `fd` it never fires at all. But that value is only
/// observable through a RELEASE compiler: `field_slot_index` has carried a
/// `debug_assert!` since P44, this harness runs the debug binary, and the
/// assertion aborts before the program is built.
///
/// That abort is **pre-existing and unchanged** — a debug compiler refused
/// this program before P103 too — so the `allow` really is an exact
/// restoration in both build modes. It is also the whole of P102(b): the
/// assertion is debug-only and `thatteos/build.sh` resolves the compiler to
/// `target/release/manitc`, so nothing that ships ever ran it.
#[test]
fn p103_the_allow_restores_the_previous_compiler() {
    const BODY: &str = "struct Desc { pub fd: int, pub ino: int, pub open: bool }\n\
         fn mk() -> Desc { return Desc { fd: 0, ino: 9, open: true }; }\n\
         fn main() { let d = mk(); if d.valid { io::print_int(1); } }";
    let (refused, msg) = check(BODY);
    assert!(!refused, "nothing to allow — P103 is not firing at all\n{msg}");
    let (ok, msg2) = check(&format!("lint allow(undeclared-field);\n{BODY}"));
    assert!(ok, "P103: the allow did not restore acceptance:\n{msg2}");
    assert!(!msg2.contains("has no field"), "still reported under allow:\n{msg2}");
}

/// GUARD: a field that DOES exist is untouched, including on a generic struct
/// whose field TYPE is still unknown.
///
/// A generic struct's field NAMES do not depend on its type arguments, which is
/// why membership in the struct table is the right question even when P68's
/// argument-aware lookup returns `Unknown`. Without this row the refusal could
/// have been written to fire on any field whose type is unknown, which would
/// reject every generic struct in the language.
#[test]
fn p103_a_real_field_is_untouched_including_on_a_generic_struct() {
    let (ok, msg) = check(
        "struct Pair<T> { pub first: T, pub second: int }\n\
         fn main() {\n\
         \x20   let p = Pair { first: 5, second: 7 };\n\
         \x20   io::print_int(p.first); io::print_int(p.second);\n\
         }",
    );
    assert!(ok, "P103: a real field on a generic struct was refused\n{msg}");
}

/// GUARD: an UNRESOLVED receiver is P95's finding, not this one.
///
/// The two must not overlap: if the refusal fired on `ManiType::Unknown` it
/// would report a field problem for what is really a type problem, and the
/// message would send the reader to the wrong line.
#[test]
fn p103_an_unresolved_receiver_is_still_reported_as_a_type() {
    let (ok, msg) = check(
        "struct Holder { pub a: NoSuchType, pub n: int }\n\
         fn main() { io::print_int(1); }",
    );
    assert!(!ok, "{msg}");
    assert!(
        msg.contains("names no type"),
        "P103 must not shadow P95's diagnostic:\n{msg}"
    );
    assert!(
        !msg.contains("has no field"),
        "an unresolved TYPE was reported as a missing FIELD:\n{msg}"
    );
}

// ---------------------------------------------------------------------------
// P104 — a lint whose `allow` cannot be spelled
// ---------------------------------------------------------------------------

/// P104, FIXED — and this row is the previous one rewritten in place, because
/// its own assertion message said to.
///
/// It used to read `assert!(!ok, "P104 has been fixed — update this row")` and
/// pinned the LIMIT: `lint allow(unknown-type);` was a parse error, because the
/// lexer reads `unknown` as the three-valued literal and a `Token` carries only
/// a kind and a span. **2 of the 19 lint names were unwritable** —
/// `unknown-type` and `literal-out-of-word` — while their command-line forms
/// (`-A unknown-type`) worked, since those never reach the lexer. So exactly
/// one of the two control surfaces was unreachable, and *a lint whose `allow`
/// cannot be spelled is not an exact restoration of anything*.
///
/// Kept as a row rather than deleted because it is also why P103's lint is
/// named `undeclared-field`: that name was chosen to dodge a trap that no
/// longer exists. The name stays — it belongs beside `undeclared-type` and
/// `undeclared-native` on its own merits — but the reason is now historical,
/// and this is where that is written down.
#[test]
fn p104_a_keyword_named_lint_can_now_be_spelled() {
    let (ok, msg) = check("lint allow(unknown-type);\nfn main() { io::print_int(1); }");
    assert!(ok, "P104's fix regressed — `unknown` is a keyword again:\n{msg}");
    // The command-line surface, which always worked, still does.
    let (ok2, msg2) = check_with("fn main() { io::print_int(1); }", &["-A", "unknown-type"]);
    assert!(ok2, "the CLI surface regressed:\n{msg2}");
}


// ---------------------------------------------------------------------------
// P104's fix — every lint name can now be written in a directive
// ---------------------------------------------------------------------------

/// A registry that must agree with another registry gets a test, not a comment
/// (permanent rule 5).
///
/// `lexer::lint_word_lexeme` must carry a spelling for every keyword that
/// appears inside a lint name. This iterates `lint::LINTS` ITSELF rather than a
/// list — so the day someone adds a lint whose name collides with a keyword,
/// this row names the word instead of the lint quietly becoming unsilenceable.
///
/// Measured before the fix: **2 of the 19 lint names were unwritable** —
/// `unknown-type` and `literal-out-of-word`, `word` being the patent alias for
/// `t27`. Their command-line forms always worked, which is why nothing noticed.
#[test]
fn p104_every_lint_name_can_be_written_in_a_directive() {
    let mut unwritable = Vec::new();
    for (_, name, _) in manitc::lint::LINTS {
        let (ok, msg) = check(&format!(
            "lint allow({name});\nfn main() {{ io::print_int(1); }}"
        ));
        if !ok {
            unwritable.push(format!("{name}: {}", msg.lines().next().unwrap_or("")));
        }
    }
    assert!(
        unwritable.is_empty(),
        "these lint names cannot be spelled in a `lint` directive, so their \
         `allow` is unreachable and they are not an exact restoration of \
         anything (report.txt P104):\n  {}",
        unwritable.join("\n  ")
    );
}

/// ...and the two that used to fail are named explicitly, so the row above
/// cannot go green by the registry shrinking.
#[test]
fn p104_the_two_keyword_lint_names_parse() {
    for name in ["unknown-type", "literal-out-of-word"] {
        let (ok, msg) = check(&format!(
            "lint allow({name});\nfn main() {{ io::print_int(1); }}"
        ));
        assert!(ok, "`lint allow({name});` must parse (P104):\n{msg}");
    }
}

/// GUARD: widening the lint-name parser must not widen the grammar elsewhere.
/// `unknown` and `word` are still keywords in every other position.
#[test]
fn p104_the_keywords_are_still_keywords_outside_a_lint_directive() {
    let (ok, msg) = check("fn main() { let unknown = 1; io::print_int(unknown); }");
    assert!(!ok, "`unknown` stopped being a keyword outside a lint name:\n{msg}");
    let (ok2, msg2) = check("fn main() { let word = 1; io::print_int(word); }");
    assert!(!ok2, "`word` stopped being a keyword outside a lint name:\n{msg2}");
}
