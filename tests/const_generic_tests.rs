//! B3 — const generics over trit width.
//!
//! © Manish Jagdish Thatte
//!
//! `enhance/phase5-type-system-second-half/README.md` item B3, specified in
//! `docs/language-reference.md` §25. A `const N: int` generic parameter binds
//! an INTEGER at instantiation, and can be written as a trit width (`t<N>`),
//! as an array length (`[trit; N]`), and as an ordinary value.
//!
//! This is the half of C3 that C3 recorded as not built. Its own limit row —
//! `c3_width_polymorphism_is_not_implemented_and_says_so` — was written to go
//! red the day B3 landed, and it was the single failing row in the suite when
//! this file was first compiled. That is the shape a stated limit should have.
//!
//! Four properties carry the design, and each is a row rather than a comment
//! because prose review does not catch this class (permanent rule 6):
//!
//! 1. **A const parameter binds a VALUE, not a type**, and the two are
//!    different kinds. A row asserts that one source instantiated at two
//!    widths gives two ANSWERS — `309` and `306` — because a family that
//!    erased the width would give one.
//! 2. **`const` is contextual.** Measured before it was added: the word occurs
//!    four times across both repositories and the 2,507-program corpus, three
//!    of them in comments, and `fn f<const>(x: const)` COMPILES on the
//!    previous compiler. The collision row uses `const` as a variable and as a
//!    parameter in one program, which is what would fight if it were reserved.
//! 3. **A failed instantiation is an ERROR here, where P65 makes it silent.**
//!    That is a deliberate divergence and rows 8 and 9 pin both halves of it:
//!    the erased body of a const generic is unreachable rather than merely
//!    unlikely, because reaching it printed a value the declared type could
//!    not hold.
//! 4. **B4 bound for const EXPRESSIONS and nothing else.** The item lists
//!    `const fn` as a prerequisite for the whole of B3; measured, a bound
//!    parameter is a literal and needs no evaluator, and only `t<A+1>` did. B4
//!    landed the same day and closed exactly that construct — the row that
//!    recorded the boundary now asserts the value the expression produces.
//!
//! Three rows record a LIMIT rather than a capability and say so in their own
//! docstrings, so a reader is not left inferring capability from a green run.

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
    let d = common::suite_root("b3").join(slot.to_string());
    std::fs::create_dir_all(&d).expect("temp dir");
    d
}

fn write(src: &str) -> PathBuf {
    let d = workdir();
    let path = d.join("p.mt");
    std::fs::write(&path, src).expect("write");
    path
}

/// `manitc check` — (accepted?, combined output).
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

/// `None` only when clang is absent.
///
/// P47: a skip condition that overlaps the failure condition is a silent pass.
/// The state is told apart by the ARTEFACT — did a binary appear? — and never
/// by the shape of an error message, and the caller asserts the T3 answer
/// unconditionally, so a row can never assert nothing.
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

/// Assert both backends produce `want` — against a hand-derived string, never
/// against each other (P44/P58: the parity matrix cannot see a mistake made
/// upstream of the backend split, and all of this is upstream of it).
fn both(src: &str, want: &str, what: &str) {
    assert_eq!(run_t3(src), want, "{what}: T3");
    if let Some(ll) = run_llvm(src) {
        assert_eq!(ll, want, "{what}: LLVM");
    }
}

// ---------------------------------------------------------------------------
// 1. The core: one source, two widths, two answers
// ---------------------------------------------------------------------------

/// **The discriminating row of the whole suite.**
///
/// One function body, instantiated at 9 trits and at 6, and the two answers
/// differ BY THE WIDTH: `300 + 9` and `300 + 6`. A compiler that parsed
/// `const A: int` and then erased it would print `300` twice and pass every
/// row that only checked the syntax.
///
/// 300 is inside a `tryte` (±364) on purpose, so that the difference between
/// the instantiations is the value of `A` and not a range refusal.
#[test]
fn b3_one_source_two_widths_two_answers() {
    both(
        "use std::io;\n\
         fn f<const A: int>(x: t<A>) -> int { let y: t<A> = 300; return y as int + A; }\n\
         fn main() { let a: t9 = 1; let b: tryte = 1;\n\
         io::println_int(f(a)); io::println_int(f(b)); }\n",
        "309\n306\n",
        "the width reaches the body as a value",
    );
}

/// The item's own example: `fn widen<const A: int>(x: t<A>) -> t54`.
///
/// `enhance/…/README.md` writes it as `fn widen<const A: int, const B: int>(x:
/// t<A>) -> t<B> where B >= A`. The two-parameter form is refused, and row 10
/// says why: nothing at a call site can name `B`.
#[test]
fn b3_the_items_own_widen_runs() {
    both(
        "use std::io;\n\
         fn widen<const A: int>(x: t<A>) -> t54 { return x as t54; }\n\
         fn main() { let a: t9 = 5; let b: tryte = 4;\n\
         io::println_int(widen(a) as int); io::println_int(widen(b) as int); }\n",
        "5\n4\n",
        "widen at two widths",
    );
}

/// A const parameter in RETURN position, bound from an argument.
///
/// `-> t<A>` is what makes width polymorphism worth having: the result is as
/// wide as the input rather than as wide as the widest case.
#[test]
fn b3_the_return_type_follows_the_argument_width() {
    both(
        "use std::io;\n\
         fn twice<const A: int>(x: t<A>) -> t<A> { return x + x; }\n\
         fn main() { let a: t9 = 5; let b: tryte = 4;\n\
         io::println_int(twice(a) as int); io::println_int(twice(b) as int); }\n",
        "10\n8\n",
        "t<A> in return position",
    );
}

/// `t<1>` is `trit`, so a `trit` argument binds the parameter to ONE.
///
/// C3 made the surface spelling `t<1>` resolve to `trit` exactly, on the
/// ground that a width-1 balanced ternary number's three values ARE the three
/// logic values. The inverse has to hold or a round trip through the type
/// system would change the width, and only a row can say so.
#[test]
fn b3_a_trit_argument_binds_the_width_to_one() {
    both(
        "use std::io;\n\
         fn w<const A: int>(x: t<A>) -> int { return A; }\n\
         fn main() { let a: trit = 1; let b: t54 = 1;\n\
         io::println_int(w(a)); io::println_int(w(b)); }\n",
        "1\n54\n",
        "trit is width one at both ends",
    );
}

// ---------------------------------------------------------------------------
// 2. Array lengths
// ---------------------------------------------------------------------------

/// `[int; N]` binds `N` from the argument's length.
///
/// This is the second position a const parameter can be written in, and it is
/// the one that decided the representation: an array length is not bounded by
/// 54, so a const parameter could not be encoded as the width TYPE it
/// resolves to. Measured against the item's own `struct TVec<const N: int> {
/// data: [trit; N] }` before `MonoBinding` was written.
#[test]
fn b3_an_array_length_binds_the_parameter() {
    both(
        "use std::io;\n\
         fn len<const N: int>(a: [int; N]) -> int { return N; }\n\
         fn main() { let x: [int; 3] = [7, 8, 9]; let y: [int; 5] = [1, 2, 3, 4, 5];\n\
         io::println_int(len(x)); io::println_int(len(y)); }\n",
        "3\n5\n",
        "array length as a const argument",
    );
}

// ---------------------------------------------------------------------------
// 3. `const` is contextual, not reserved
// ---------------------------------------------------------------------------

/// `const` as a variable and as a parameter keyword IN ONE PROGRAM.
///
/// The measurement that forced this: `fn f<const>(x: const)` and `struct const
/// { .. }` both COMPILE on the previous compiler, so `const` is a legal name
/// in exactly the positions this feature wants. Reserving it would have
/// deleted them. Across both repositories and the 2,507-program corpus the
/// word occurs four times, three inside comments and one in a generated file
/// writing Rust — so the population that could break is zero, and being
/// contextual makes it zero for source not yet written too.
///
/// P104's lesson met before it bit, for the fifth time (`t` in C3, `move` in
/// D-2, `fs::move`).
#[test]
fn b3_const_is_contextual_and_still_a_legal_name() {
    both(
        "use std::io;\n\
         fn w<const A: int>(x: t<A>) -> int { return A; }\n\
         fn main() { let const = 7; let a: t9 = 1;\n\
         io::println_int(const); io::println_int(w(a)); }\n",
        "7\n9\n",
        "const as a variable and as a keyword at once",
    );
}

/// A bare `<const>` is still a TYPE parameter named `const`, as it was.
///
/// The disambiguation is `const` followed by an identifier, so the previous
/// reading survives wherever the new one cannot apply. This row passes on the
/// compiler WITHOUT B3 and says so — it asserts that nothing moved, which is
/// the one thing a red-on-control row cannot express (rule 9).
#[test]
fn b3_a_bare_const_is_unchanged_and_this_row_passes_either_way() {
    let (ok, msg) = check("fn f<const>(x: const) -> int { return 1; }\nfn main() { }\n");
    assert!(ok, "`<const>` must still be a type parameter named const: {msg}");
}

// ---------------------------------------------------------------------------
// 4. Kinds: a value is not a type
// ---------------------------------------------------------------------------

/// A const parameter written where a type belongs names the KIND, and offers
/// all three positions it can legally appear in.
///
/// It is caught before `name_to_manitype`, because P95's remedy for an unknown
/// name is "did you mean" over the declared types and `A` is not a misspelling
/// of a type — it is a name that exists and is the wrong kind.
#[test]
fn b3_a_const_parameter_is_not_a_type() {
    let (ok, msg) = check(
        "fn f<const A: int>(x: t<A>) -> int { let y: A = 1; return y; }\nfn main() { }\n",
    );
    assert!(!ok, "`let y: A` must be refused");
    assert!(
        msg.contains("is a `const` generic parameter") && msg.contains("t<A>"),
        "the message must name the kind and the remedy: {msg}"
    );
}

/// A TYPE parameter written where a width belongs, in the other direction.
///
/// This is the row `tests/ternary_width_tests.rs` used to carry as C3's limit.
/// It stays refused, and what changed is the reason given: the kind, not the
/// token.
#[test]
fn b3_a_type_parameter_is_not_a_width() {
    let (ok, msg) = check("fn f<T>(x: t<T>) -> int { return 1; }\nfn main() { }\n");
    assert!(!ok, "`t<T>` over a type parameter must be refused");
    assert!(
        msg.contains("which is not a constant here"),
        "refused for the kind, not by a parse accident: {msg}"
    );
    // B4 generalised this message: a width is now any constant EXPRESSION, so
    // the refusal names what a width may be rather than only what `T` is not.
    assert!(
        msg.contains("`const` generic parameter") && msg.contains("const fn"),
        "and list what a width may be: {msg}"
    );
}

// ---------------------------------------------------------------------------
// 5. Declarations that have no reading
// ---------------------------------------------------------------------------

/// Const parameters come LAST, and the diagnostic writes the corrected list.
///
/// The restriction is real and is the price of keeping `generics` meaning "the
/// type parameters" for its fifty-odd existing readers: the two lists are
/// recombined positionally, so `<const N: int, T>` has no reading either can
/// represent. Measured before the diagnostic existed: it reached the type
/// resolver as an undeclared type `T`, so the reader was told to declare
/// something they had declared.
#[test]
fn b3_a_type_parameter_after_a_const_one_is_refused_with_the_fix() {
    let (ok, msg) = check(
        "fn f<const N: int, T>(x: t<N>, y: T) -> int { return N; }\nfn main() { }\n",
    );
    assert!(!ok, "`<const N: int, T>` must be refused");
    assert!(
        msg.contains("come LAST") && msg.contains("<T, const N: int>"),
        "the message must write the corrected list out: {msg}"
    );
}

/// `int` is the only type a const parameter may be declared with.
///
/// Refused by name rather than ignored: a `const N: str` that silently became
/// an integer parameter is the kind of quiet reinterpretation this compiler's
/// record is full of. Measured before the check existed — it type-checked, and
/// then bound `N` to a trit width.
#[test]
fn b3_a_const_parameter_must_be_an_int() {
    let (ok, msg) = check("fn f<const N: str>(x: t<N>) -> int { return N; }\nfn main() { }\n");
    assert!(!ok, "`const N: str` must be refused");
    assert!(
        msg.contains("`int` is the only type"),
        "and say what is allowed: {msg}"
    );
}

/// One name cannot be both a type and a value.
///
/// Measured before the check existed: `<N, const N: int>` was accepted and the
/// type parameter won every lookup, so `t<N>` resolved through the erased type
/// rather than the width.
#[test]
fn b3_a_name_cannot_be_both_kinds() {
    let (ok, msg) = check("fn f<N, const N: int>(x: t<N>) -> int { return N; }\nfn main() { }\n");
    assert!(!ok, "`<N, const N: int>` must be refused");
    assert!(
        msg.contains("declared twice") && msg.contains("both a type and a value"),
        "the message must name the collision of kinds: {msg}"
    );
}

// ---------------------------------------------------------------------------
// 6. The divergence from P65: a failed instantiation is an error here
// ---------------------------------------------------------------------------

/// **The row that exists because this increment produced a wrong answer.**
///
/// `let y: t<A> = 365` inside `f<const A: int>`, called at A = 6, is out of
/// range: a `tryte` holds ±364. Under P65's rule the instantiation is
/// DISCARDED and the call keeps the erased body, where `t<A>` is `Unknown` —
/// and the program printed **365**. Measured on this increment, before the
/// call site learned to refuse.
///
/// The diagnostic carries the instantiation (`A = 6`) and the real reason with
/// its own line, rather than "instantiation failed", because the failure is
/// three lines away from the call that caused it.
#[test]
fn b3_a_failed_const_instantiation_is_refused_not_silently_erased() {
    let (ok, msg) = check(
        "fn f<const A: int>(x: t<A>) -> int { let y: t<A> = 365; return y as int; }\n\
         fn main() { let a: tryte = 1; let _ = f(a); }\n",
    );
    assert!(!ok, "an out-of-range instantiation must be refused, not erased");
    assert!(
        msg.contains("A = 6") && msg.contains("365") && msg.contains("tryte"),
        "the message must name the instantiation AND the real reason: {msg}"
    );
}

/// The other half of the same rule: the SAME body at a width that fits.
///
/// Both halves are asserted because a compiler that refused every const
/// instantiation would pass the row above. 365 is inside a `t9` (±9,841).
#[test]
fn b3_the_same_body_is_accepted_at_a_width_that_holds_it() {
    both(
        "use std::io;\n\
         fn f<const A: int>(x: t<A>) -> int { let y: t<A> = 365; return y as int; }\n\
         fn main() { let a: t9 = 1; io::println_int(f(a)); }\n",
        "365\n",
        "365 fits a t9",
    );
}

/// A call that pins down no width is refused, and the advice is the right one.
///
/// `width(n)` with an `int` argument: `int` is not `t<A>` for any A, so
/// nothing binds `A`. Measured before the check existed — it reached the
/// erased body, substituted 0, and printed **0**.
#[test]
fn b3_an_unbindable_const_argument_is_refused() {
    let (ok, msg) = check(
        "fn width<const A: int>(x: t<A>) -> int { return A; }\n\
         fn main() { let n: int = 5; let _ = width(n); }\n",
    );
    assert!(!ok, "a call binding no width must be refused");
    assert!(
        msg.contains("does not pin down") && msg.contains("bound from the ARGUMENTS"),
        "and point at the arguments: {msg}"
    );
}

/// A parameter that appears ONLY in the return type gets its own advice.
///
/// `fn make<const B: int>() -> t<B>` is half of the item's two-parameter
/// `widen`, and it cannot work: this language has no turbofish, so nothing at
/// the call site can name `B`. The two failures share a check and must not
/// share a message — the remedies are different.
#[test]
fn b3_a_return_position_only_const_parameter_is_refused_by_name() {
    let (ok, msg) = check(
        "fn make<const B: int>() -> t<B> { return 0; }\n\
         fn main() { let _x: t9 = make(); }\n",
    );
    assert!(!ok, "a return-position-only const parameter must be refused");
    assert!(
        msg.contains("only in the return type") && msg.contains("turbofish"),
        "with the advice that fits THIS failure: {msg}"
    );
}

// ---------------------------------------------------------------------------
// 7. The limits, stated
// ---------------------------------------------------------------------------

/// **LIMIT.** A const generic ARGUMENT cannot be supplied explicitly.
///
/// `struct TVec<const N: int>` declares and its fields resolve, but
/// `TVec<27>` is refused: a const argument would have to live inside
/// `ManiType::Struct`'s argument list, which is a `Vec<ManiType>`, and a value
/// is not a `ManiType`. Recorded rather than half-built. The row goes red the
/// day struct instantiation lands, which is the point of it.
#[test]
fn b3_a_struct_declares_a_const_parameter_but_cannot_be_instantiated_at_one() {
    let (ok, msg) = check("struct TVec<const N: int> { data: [trit; N] }\nfn main() { }\n");
    assert!(ok, "the DECLARATION must resolve: {msg}");

    let (ok, msg) = check(
        "struct TVec<const N: int> { data: [trit; N] }\n\
         fn main() { let _v: TVec<27> = TVec { data: [0] }; }\n",
    );
    assert!(!ok, "`TVec<27>` must be refused");
    assert!(
        msg.contains("const generic ARGUMENT") && msg.contains("not implemented"),
        "refused by name, not by `expected type, found Int(27)`: {msg}"
    );
}

/// **LIMIT.** An `impl` block cannot declare a const parameter.
///
/// The impl target is a bare `String` on the AST — the `<N>` of `impl<const N:
/// int> TVec<N>` is not parsed as an argument list at all — so a value could
/// only arrive from a struct instantiation, which the row above records as not
/// built. The diagnostic names the working alternative rather than only the
/// refusal.
#[test]
fn b3_an_impl_block_cannot_declare_a_const_parameter() {
    let (ok, msg) = check(
        "struct S { v: int }\n\
         impl<const N: int> S { fn g(self) -> int { return N; } }\n\
         fn main() { }\n",
    );
    assert!(!ok, "`impl<const N: int>` must be refused");
    assert!(
        msg.contains("Declare the parameter on the METHOD"),
        "and name where it does work: {msg}"
    );
}

/// **THIS ROW RECORDED THE ONE PLACE B4 GENUINELY BOUND, AND B4 CLOSED IT ON
/// 4 SEPTEMBER 2026.**
///
/// It was written to refuse `t<A+1>` and to name B4 as the owner of the gap.
/// That was the whole finding: a BOUND const parameter is a literal by the time
/// anything reads it, so no evaluator is involved in any other row of this
/// suite — an EXPRESSION over one was the single construct that needed it.
///
/// Rewritten rather than deleted, because the claim is still worth pinning and
/// what changed is its sign. `t<A + 1>` now compiles and evaluates, and the row
/// asserts the VALUE it produces rather than that it parses: at A = 9 the type
/// is `t<10>`, which holds 29,524, and `t<9>` does not.
#[test]
fn b3_a_const_expression_over_a_parameter_now_evaluates() {
    let (ok, msg) = check(
        "fn f<const A: int>(x: t<A>) -> int { let y: t<A + 1> = 1; return y as int; }\n\
         fn main() { let a: t9 = 1; let _ = f(a); }\n",
    );
    assert!(ok, "`t<A + 1>` must now compile: {msg}");

    // A + 1 is 10 at this instantiation, and (3^10 - 1)/2 is 29,524 — which a
    // `t<9>` (max 9,841) could not hold. The width is really the expression's
    // value, not the parameter's.
    let (ok, msg) = check(
        "fn f<const A: int>(x: t<A>) -> int { let y: t<A + 1> = 29524; return y as int; }\n\
         fn main() { let a: t9 = 1; let _ = f(a); }\n",
    );
    assert!(ok, "t<10> must hold 29524: {msg}");
    let (ok, _) = check(
        "fn f<const A: int>(x: t<A>) -> int { let y: t<A> = 29524; return y as int; }\n\
         fn main() { let a: t9 = 1; let _ = f(a); }\n",
    );
    assert!(!ok, "and t<9> must not — otherwise the `+ 1` did nothing");
}

// ---------------------------------------------------------------------------
// 8. Inertness: an ordinary generic must not have moved
// ---------------------------------------------------------------------------

/// A generic with no const parameters mangles exactly as it did.
///
/// `mono_name` gained a second loop for const arguments, and a mangled name
/// that changed would rename every instantiated symbol on both backends. The
/// row asserts the SYMBOL in the emitted LLVM, not the answer, because that is
/// precisely what a behavioural test cannot see: `id$int` and `id$int$` both
/// compute 3.
///
/// **This row passes on the compiler without B3, by construction, and that is
/// what it is for** (rule 9). It discriminates the change that would have
/// moved a shipped symbol, not a change that moved nothing.
#[test]
fn b3_an_ordinary_generic_still_mangles_as_before() {
    let path = write(
        "use std::io;\n\
         fn id<T>(x: T) -> T { return x; }\n\
         fn main() { io::println_int(id(3)); }\n",
    );
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
    let ir = std::fs::read_to_string(&ll).expect("read .ll");
    assert!(
        ir.contains("@id$int"),
        "the instantiation must still be `id$int`, with no empty const suffix"
    );
    assert!(
        !ir.contains("@id$int$"),
        "a trailing `$` would mean the const loop ran on an empty list"
    );
}
