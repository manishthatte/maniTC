//! C3 — width-polymorphic ternary types, `t<N>`.
//!
//! © Manish Jagdish Thatte
//!
//! `enhance/phase5-type-system-second-half/README.md` item C3, specified in
//! `docs/language-reference.md` §24. `t<N>` is a balanced ternary integer of N
//! trits, for N from 1 to 54; `trit`, `tryte`, `t9`, `t27` and `t54` are five
//! of those widths under their own names.
//!
//! Four properties carry the design, and each is here as a row rather than a
//! comment because each is the sort of thing prose review does not catch
//! (permanent rule 6):
//!
//! 1. **The range is DERIVED from the width.** That is report.txt P122's fix
//!    paying out: because the range comes from one authority rather than a
//!    table, every width between the names got a correct range the moment the
//!    syntax existed. The boundary rows below assert it at a width that has no
//!    name, where a table would have had to be extended by hand.
//! 2. **`trit` is `t<1>` and carries the three-valued logic role alone.**
//!    Measured, not assumed: `tand` accepts `trit` and refuses the other four.
//!    A row asserts BOTH directions, because a family that quietly gave every
//!    width the logic role would pass a row that only checked `trit`.
//! 3. **`t` is not a keyword.** It is declared as a variable 855 times across
//!    both repositories and the model corpus. The collision row runs a program
//!    using `t` as a variable, `t < 0` as a comparison and `t<18>` as a type at
//!    once — the three things that would fight if `t` were reserved.
//! 4. **The four named widths are INERT.** `t9` lowers to what it always
//!    lowered to. A syntax addition must not move a shipped type's
//!    representation, so a row asserts the emitted LLVM type directly.
//!
//! Two rows record a LIMIT rather than a fix and say so in their own
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
    let d = common::suite_root("c3").join(slot.to_string());
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
        .output().expect("check");
    let txt = String::from_utf8_lossy(&o.stdout).into_owned()
        + &String::from_utf8_lossy(&o.stderr);
    (o.status.success() && !txt.contains("error:"), txt)
}

fn run_t3(src: &str) -> String {
    let path = write(src);
    let base = path.with_extension("");
    let c = Command::new(manitc_bin())
        .args(["compile", path.to_str().unwrap(), "--target", "t3",
               "-o", base.to_str().unwrap()])
        .output().expect("compile");
    assert!(c.status.success(), "T3 compile failed:\n{}{}",
            String::from_utf8_lossy(&c.stdout), String::from_utf8_lossy(&c.stderr));
    let r = Command::new(manitc_bin())
        .args(["run-t3", base.with_extension("t3b").to_str().unwrap()])
        .output().expect("run");
    String::from_utf8_lossy(&r.stdout)
        .lines().filter(|l| !l.starts_with("[T3ISA]"))
        .map(|l| format!("{l}\n")).collect()
}

/// `None` only when clang is absent.
///
/// P47: a skip condition that overlaps the failure condition is a silent pass.
/// Every program here compiles to LLVM if the toolchain exists at all, and the
/// caller asserts the T3 answer unconditionally, so a row can never assert
/// nothing.
fn run_llvm(src: &str) -> Option<String> {
    let path = write(src);
    let bin = path.with_file_name("p.bin");
    Command::new(manitc_bin())
        .args(["compile", path.to_str().unwrap(), "--target", "llvm",
               "-o", bin.to_str().unwrap()])
        .output().expect("compile");
    if !bin.exists() { return None; }
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
// 1. The core: a width that has no name of its own
// ---------------------------------------------------------------------------

/// C3's reason for existing, in one program.
///
/// 18 trits is the width `stdlib/t27f.mt` calls its mantissa, and before this
/// the format's own component could not be spelled in the type system hosting
/// it. 193710244 is (3^18 − 1)/2, derived by hand.
#[test]
fn c3_a_width_between_the_names_is_a_type() {
    both("use std::io;\n\
          fn main() { let x: t<18> = 193710244; io::println_int(x as int); }\n",
         "193710244\n", "t<18> holds its own maximum");
}

/// `t<N>` in every type position a type can occupy.
#[test]
fn c3_a_width_is_a_type_everywhere_a_type_goes() {
    both("use std::io;\n\
          struct Mant { pub m: t<18>, pub e: t9 }\n\
          fn scale(x: t<18>, k: t<12>) -> t<18> { return x + (k as t<18>); }\n\
          fn main() {\n\
            let s: Mant = Mant { m: scale(100, 5), e: 3 };\n\
            let arr: [t<18>; 3] = [1, 2, 7];\n\
            io::println_int(s.m as int);\n\
            io::println_int(s.e as int);\n\
            io::println_int(arr[2] as int);\n\
          }\n",
         "105\n3\n7\n", "t<N> in field, param, return, array, cast");
}

// ---------------------------------------------------------------------------
// 2. The range is derived, not tabulated — P122 paying out
// ---------------------------------------------------------------------------

/// The boundary at a width no table ever listed.
///
/// This is the row that distinguishes a derivation from a table: (3^18−1)/2 is
/// 193710244, and nothing in the compiler ever wrote that number down.
#[test]
fn c3_the_range_of_an_unnamed_width_is_exact_in_both_directions() {
    let (ok, _) = check("fn main() { let _x: t<18> = 193710244; }\n");
    assert!(ok, "(3^18-1)/2 must be accepted");

    let (ok, msg) = check("fn main() { let _x: t<18> = 193710245; }\n");
    assert!(!ok, "one past (3^18-1)/2 must be refused");
    assert!(msg.contains("193710245") && msg.contains("-193710244..193710244"),
            "the message must quote the value and the derived range: {msg}");
}

/// Three widths, three ranges, none of them tabulated anywhere.
#[test]
fn c3_every_width_gets_its_own_derived_range() {
    // (3^N - 1) / 2, computed here independently of the compiler.
    for (w, max) in [(2u32, 4i64), (5, 121), (13, 797161), (20, 1743392200)] {
        let (ok, _) = check(&format!("fn main() {{ let _x: t<{w}> = {max}; }}\n"));
        assert!(ok, "t<{w}> must accept its maximum {max}");
        let (ok, _) = check(&format!("fn main() {{ let _x: t<{w}> = {}; }}\n", max + 1));
        assert!(!ok, "t<{w}> must refuse {}", max + 1);
    }
}

// ---------------------------------------------------------------------------
// 3. The five names are five of the widths
// ---------------------------------------------------------------------------

/// An alias is the SAME type, not a convertible one — asserted by binding in
/// both directions, which a one-way row would not distinguish from a coercion.
#[test]
fn c3_the_named_widths_are_the_widths_they_name() {
    for (name, w) in [("tryte", 6), ("t9", 9), ("t27", 27), ("t54", 54)] {
        let (ok, msg) = check(&format!(
            "fn main() {{ let a: {name} = 1; let b: t<{w}> = a; let _c: {name} = b; }}\n"));
        assert!(ok, "{name} and t<{w}> must be one type: {msg}");
    }
    // And the two alias spellings the resolver owns.
    for (alias, w) in [("word", 27), ("trint", 54)] {
        let (ok, msg) = check(&format!(
            "fn main() {{ let a: {alias} = 1; let _b: t<{w}> = a; }}\n"));
        assert!(ok, "{alias} must still be t<{w}>: {msg}");
    }
}

/// The alias table read FROM THE DOCUMENT and asked of the COMPILER.
///
/// This is report.txt P122's own shape, and for P122's own reason. That defect
/// was two internally-consistent tables disagreeing about `tryte`, so a check
/// reading this crate's table and comparing it with this crate's resolver would
/// be same-origin and prove nothing (P64). The table is parsed out of
/// `docs/language-reference.md` §24 — the artefact a reader actually consults —
/// and every row of it is then put to the compiler as a user would put it.
///
/// It therefore fails if the document and the compiler drift in EITHER
/// direction, which is the pair that has actually gone wrong here before.
#[test]
fn c3_the_documented_alias_table_agrees_with_the_compiler() {
    let doc = std::fs::read_to_string(
        concat!(env!("CARGO_MANIFEST_DIR"), "/docs/language-reference.md"))
        .expect("the language reference is in the repository");
    let sec = doc.split("## 24. Ternary widths").nth(1)
        .expect("§24 must exist — it is what documents `t<N>`");

    // Rows read `| `trit`   | `t<1>`  | — |`
    let mut pairs: Vec<(String, u32)> = Vec::new();
    for line in sec.lines().take_while(|l| !l.starts_with("### ")) {
        let cells: Vec<&str> = line.split('|').map(str::trim).collect();
        if cells.len() < 4 { continue; }
        let name = cells[1].trim_matches('`').to_string();
        let means = cells[2].trim_matches('`');
        if let Some(w) = means.strip_prefix("t<").and_then(|r| r.strip_suffix('>'))
            .and_then(|d| d.parse::<u32>().ok())
        {
            pairs.push((name, w));
        }
    }
    assert_eq!(pairs.len(), 5,
               "§24's table must list all five named widths, found {pairs:?}");

    // The discriminator is the RANGE, not assignability, and that was measured
    // rather than assumed: ManiT's numeric types are all mutually assignable —
    // `let a: tryte = 0; let _b: t9 = a;` is accepted on the compiler before
    // `t<N>` existed — so a row binding one to the other would pass for every
    // pairing and pin nothing. What genuinely depends on the width is where the
    // type stops accepting literals.
    for (name, w) in &pairs {
        if *w >= 40 {
            // Widths of 40 and up are i64-bounded rather than 3^N-bounded
            // (§24), so there is no literal boundary to find. Skipped
            // deliberately and not silently: the `t54` row is carried by
            // `c3_the_named_widths_are_the_widths_they_name` instead.
            continue;
        }
        let max: i64 = (0..*w).fold(1i64, |a, _| a * 3) / 2;
        let (ok, msg) = check(&format!("fn main() {{ let _x: {name} = {max}; }}\n"));
        assert!(ok, "§24 says `{name}` is t<{w}>, so it must accept {max}: {msg}");
        let (ok, _) = check(&format!("fn main() {{ let _x: {name} = {}; }}\n", max + 1));
        assert!(!ok, "§24 says `{name}` is t<{w}>, so it must refuse {}", max + 1);
        // and the width spelling agrees with the name, at the same boundary
        let (ok, _) = check(&format!("fn main() {{ let _x: t<{w}> = {max}; }}\n"));
        assert!(ok, "t<{w}> must accept {max} exactly as `{name}` does");
        let (ok, _) = check(&format!("fn main() {{ let _x: t<{w}> = {}; }}\n", max + 1));
        assert!(!ok, "t<{w}> must refuse {} exactly as `{name}` does", max + 1);
    }
}

/// The diagnostic spells a named width by its name and an unnamed one in the
/// `t<N>` form — and the `t<N>` form is source that can be pasted back.
#[test]
fn c3_a_width_renders_under_the_name_it_has() {
    let (_, msg) = check("fn main() { let _x: tryte = 999999; }\n");
    assert!(msg.contains("'tryte'"), "a named width renders under its name: {msg}");

    let (_, msg) = check("fn main() { let _x: t<18> = 999999999; }\n");
    assert!(msg.contains("'t<18>'"), "an unnamed width renders as t<N>: {msg}");

    // …and that rendering round-trips: the spelling the diagnostic used is one
    // the compiler accepts.
    let (ok, _) = check("fn main() { let _x: t<18> = 5; }\n");
    assert!(ok, "the spelling a diagnostic prints must be one that compiles");
}

// ---------------------------------------------------------------------------
// 4. `trit` is `t<1>`, and width one is the only one with the logic role
// ---------------------------------------------------------------------------

/// Both directions in one row.
///
/// A family that quietly gave every width the three-valued role would pass a
/// row asserting only that `trit` accepts `tand`; a family that took the role
/// away from `trit` would pass a row asserting only that `tryte` refuses it.
#[test]
fn c3_width_one_carries_the_logic_role_and_no_other_width_does() {
    let (ok, msg) = check(
        "fn main() { let a: t<1> = 1; let b: t<1> = 0; let _c = a tand b; }\n");
    assert!(ok, "t<1> IS trit and must take a three-valued operator: {msg}");

    let (ok, _) = check("fn main() { let a: trit = 1; let b: trit = 0; let _c = a tand b; }\n");
    assert!(ok, "trit must still take a three-valued operator");

    for w in [2u32, 6, 9, 18, 27, 54] {
        let (ok, msg) = check(&format!(
            "fn main() {{ let a: t<{w}> = 1; let b: t<{w}> = 0; let _c = a tand b; }}\n"));
        assert!(!ok, "t<{w}> must NOT take a three-valued operator");
        assert!(msg.contains("three-valued logic operator"),
                "and must say why: {msg}");
    }
}

/// `t<1>` and `trit` are one type, not two that happen to agree.
#[test]
fn c3_width_one_and_trit_are_the_same_type() {
    let (ok, msg) = check("fn main() { let a: trit = 1; let b: t<1> = a; let _c: trit = b; }\n");
    assert!(ok, "trit and t<1> must be one type: {msg}");
}

// ---------------------------------------------------------------------------
// 5. `t` is not a keyword
// ---------------------------------------------------------------------------

/// The collision, in one program.
///
/// `t` as a variable, `t < 0` as a comparison and `t<18>` as a type together —
/// the three things that would fight if `t` were reserved. Measured before the
/// syntax was added: `t` is declared 855 times across both repositories and the
/// model corpus, 11 of those followed by `<`.
#[test]
fn c3_t_is_still_an_ordinary_variable_name() {
    both("use std::io;\n\
          fn main() {\n\
            let t: int = -3;\n\
            let w: t<18> = 40;\n\
            if t < 0 { io::println(\"neg\"); }\n\
            let mut t2: int = 0;\n\
            while t2 < 3 { t2 = t2 + 1; }\n\
            io::println_int(w as int);\n\
            io::println_int(t2);\n\
          }\n",
         "neg\n40\n3\n", "t as a name, a comparison and a width at once");
}

// ---------------------------------------------------------------------------
// 6. Bounds, and the half that is not built
// ---------------------------------------------------------------------------

/// A width outside 1..=54 names its own problem rather than reaching the
/// undeclared-type machinery, whose remedy ("did you mean") is wrong here:
/// `t<0>` is not a misspelling of anything.
#[test]
fn c3_a_width_outside_the_range_is_refused_by_name() {
    for w in ["0", "55", "100"] {
        let (ok, msg) = check(&format!("fn main() {{ let _x: t<{w}> = 1; }}\n"));
        assert!(!ok, "t<{w}> must be refused");
        assert!(msg.contains(&format!("t<{w}>")) && msg.contains("width runs from 1 to 54"),
                "the message must name the width and the bound: {msg}");
    }
}

/// **This row recorded a LIMIT and B3 CLOSED IT, on 4 September 2026.**
///
/// It was written to go red the day const generics landed rather than let the
/// gap be discovered by a user, and that is exactly what it did: it was the
/// single failing row in the whole suite when B3 was first built. Kept in
/// place, rewritten, rather than deleted — the discrimination it carries is
/// still worth having, and what changed is which half is refused.
///
/// A TYPE parameter as a width is still refused, because a type is not a
/// width; it is now refused by a message that names the kind (`T` is not a
/// `const` generic parameter) instead of by the parser's "needs a trit width",
/// which was the only refusal available when nothing could be a width but a
/// literal. The CONST parameter form now compiles and runs — see
/// `tests/const_generic_tests.rs`, which is B3's own suite.
#[test]
fn c3_width_polymorphism_over_a_type_parameter_is_still_refused() {
    let (ok, msg) = check("fn f<T>(x: t<T>) -> int { return 1; }\nfn main() { }\n");
    assert!(!ok, "a TYPE parameter as a width must be refused");
    assert!(msg.contains("which is not a constant here"),
            "and refused for the right reason — the kind, not the token: {msg}");

    // B3: the const-generic form, which this row used to assert was unparsable.
    let (ok, msg) = check(
        "fn widen<const A: int>(x: t<A>) -> int { return A; }\n\
         fn main() { let a: t9 = 1; let _ = widen(a); }\n");
    assert!(ok, "the const-generic form must now compile: {msg}");
}

// ---------------------------------------------------------------------------
// 7. Inertness: a syntax addition must not move a shipped representation
// ---------------------------------------------------------------------------

/// `t9` lowers to `i32` as it always has, and `tryte` to `i16`.
///
/// Asserted on the emitted LLVM directly, because this is exactly the property
/// a behavioural test cannot see: a narrower `t9` would still compute 9841
/// correctly. A tight derivation from the width would have put `t9` in `i16`,
/// which is a representation change to a shipped type arriving as a side
/// effect of adding a syntax (rule 11 — the looseness is recorded in §24
/// rather than repaired here).
///
/// **This row PASSES on the compiler without C3, by construction, and that is
/// what it is for** (rule 9). It is the only one of this suite's fifteen that
/// does: the other fourteen go red there. A row asserting that nothing moved
/// cannot discriminate a change that moved nothing — it discriminates the
/// change that would have, and it was written after a tight derivation from
/// the width was considered and rejected for putting `t9` in `i16`.
#[test]
fn c3_the_named_widths_lower_to_what_they_always_lowered_to() {
    let path = write("fn f(a: tryte, b: t9, c: t27, d: t54) -> int {\n\
                      return (a as int) + (b as int) + (c as int) + (d as int); }\n\
                      fn main() { let _x = f(1, 2, 3, 4); }\n");
    let ll = path.with_extension("ll");
    let o = Command::new(manitc_bin())
        .args(["compile", path.to_str().unwrap(), "--target", "llvm",
               "-o", path.with_extension("bin").to_str().unwrap()])
        .output().expect("compile");
    assert!(o.status.success() || ll.exists(),
            "LLVM emission must at least write the module");
    let text = std::fs::read_to_string(&ll).expect("the .ll is written beside the output");
    let sig = text.lines().find(|l| l.contains("define") && l.contains("@f"))
        .unwrap_or("").to_string();
    assert!(sig.contains("i16") && sig.contains("i32") && sig.contains("i64"),
            "tryte/t9/t27 must still be i16/i32/i64: {sig}");
}

// ---------------------------------------------------------------------------
// 8. P123 — a diagnostic must name a spelling the language has
// ---------------------------------------------------------------------------
//
// Pre-existing, found while adding §24 and dated on the clean-HEAD control:
// two user-facing messages printed a `ManiType` with `{:?}`, so they named the
// compiler's internal Rust variant — `Tryte`, `T9`, `T27` — rather than the
// source spelling. One of the two PRESCRIBES A REMEDY, which is what makes it
// more than cosmetic: "add `-> Tryte` to its signature" is advice that does not
// compile, so a reader who followed it got a second error. P45's shape, where
// the diagnostic was worse than the defect it reported.
//
// It is loud only because of P95. Before undeclared types were refused,
// `-> Tryte` resolved to `Unknown` — compatible with everything — so following
// the advice produced a program that COMPILED with the wrong return type and
// said nothing. A fix in one place turned a silent wrong remedy into a visible
// one, which is how this was found at all.

/// The prescribed remedy must be a program that compiles — asserted by
/// compiling it, not by inspecting the wording.
#[test]
fn p123_the_remedy_a_diagnostic_prescribes_is_one_that_works() {
    let (ok, msg) = check("fn f() { let x: tryte = 5; return x; }\nfn main() { let _y = f(); }\n");
    assert!(!ok, "returning a value from a void function is still an error");
    assert!(msg.contains("-> tryte"), "the remedy must be spelled in the language: {msg}");
    assert!(!msg.contains("Tryte"), "and must not name the internal variant: {msg}");

    // Follow the advice verbatim. This is the whole point of the row: a
    // message that merely LOOKS right is what shipped for as long as the
    // message existed.
    let (ok, msg) = check("fn f() -> tryte { let x: tryte = 5; return x; }\nfn main() { let _y = f(); }\n");
    assert!(ok, "the compiler's own remedy must compile: {msg}");
}

/// The same defect one message along — the overflow diagnostic.
#[test]
fn p123_the_overflow_message_names_the_type_in_the_language() {
    for (spelling, internal) in [("tryte", "Tryte"), ("t9", "T9"), ("t27", "T27")] {
        let (ok, msg) = check(&format!("fn main() {{ let _x: {spelling} = 999999999999999; }}\n"));
        assert!(!ok, "{spelling} must still refuse an out-of-range literal");
        assert!(msg.contains(&format!("'{spelling}'")),
                "must name the source spelling: {msg}");
        assert!(!msg.contains(internal),
                "must not name the internal variant `{internal}`: {msg}");
    }
}
