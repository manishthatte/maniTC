// tests/generic_impl_tests.rs — the oracle probes' repro programs, as tests.
//
// report.txt P43 (§61, payload-enum constructors), P44 (§62, generic structs
// through a boundary) and P45 (§63, an `Ord` bound that does not bind). The
// programs came from the ManiTBench probe session that found the three; the
// fixtures in tests/fixtures/oracle_repros/ are theirs verbatim, each with the
// stdout it SHOULD produce.
//
// THREE THINGS ABOUT THE SHAPE OF THIS FILE, EACH OF WHICH IS A FINDING IN ITS
// OWN RIGHT.
//
//   * THEY ASSERT THE VALUE, NOT THE EXIT STATUS. Every broken row but two
//     type-checks clean and exits 0, and three of them compile, run and print
//     a WRONG ANSWER. An exit-status assertion is green on all of those. This
//     is §50's lesson one layer down: that finding was a PARSE bug, so the
//     tests written to close it assert that the code parses — and they keep
//     passing however wrongly it evaluates. A regression test inherits the
//     question its bug asked, and asserting a value is a separate act of
//     authorship.
//
//   * THE CONTROLS ARE NOT PADDING. Ten of the nineteen currently PASS, and
//     each differs from a broken row by exactly one thing. §62 is the
//     INTERSECTION of "generic" and "crosses a boundary" precisely because
//     `impl` alone and generics alone are both green; without the controls a
//     future reader sees a failing generic-impl test and cannot tell which
//     half moved.
//
//   * A SINGLE CASE FOR A COMPARISON IS HALF A TEST. The probe that first
//     covered §63 used `largest("ab", "zz")`, expected `"zz"` and got `"zz"` —
//     green, because `"ab" > "zz"` is genuinely false, so the correct answer
//     and the always-false answer are THE SAME VALUE. `("mm", "aa")` failed
//     instantly. Both orderings of every pair, or the test says nothing about
//     the operator.
//
// The broken rows are `#[ignore]`d with their finding id rather than deleted or
// left red, so `cargo test -- --ignored` is the standing list of known-broken
// idioms and the suite stays green. A red suite that everyone learns to skim is
// how a commit went out with a failing test in it (report.txt P41).
//
// © Manish Jagdish Thatte

use std::path::PathBuf;
use std::process::Command;

fn manitc() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_manitc"))
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/oracle_repros")
        .join(format!("{}.mt", name))
}

fn expected(name: &str) -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/oracle_repros")
        .join(format!("{}.expected", name));
    std::fs::read_to_string(p).expect("expected file")
}

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("manitc_oracle_{}", std::process::id()));
    std::fs::create_dir_all(&d).expect("temp dir");
    d.join(name)
}

/// Compile for T3 and run. Returns `Err` with the compiler's message when the
/// program does not compile at all — several of these do not, and that is the
/// finding rather than a reason to panic.
fn run_t3(name: &str) -> Result<String, String> {
    let out = tmp(name);
    let c = Command::new(manitc())
        .args([
            "compile",
            fixture(name).to_str().unwrap(),
            "--target",
            "t3",
            "-o",
            out.to_str().unwrap(),
        ])
        .output()
        .expect("compile");
    if !c.status.success() {
        return Err(String::from_utf8_lossy(&c.stderr).into_owned());
    }
    let r = Command::new(manitc())
        .args(["run-t3", out.with_extension("t3b").to_str().unwrap()])
        .output()
        .expect("run");
    Ok(String::from_utf8_lossy(&r.stdout)
        .lines()
        .filter(|l| !l.starts_with("[T3ISA]"))
        .map(|l| format!("{}\n", l))
        .collect())
}

/// `manitc check`'s exit status, and nothing else.
fn check_status(name: &str) -> bool {
    Command::new(manitc())
        .args(["check", fixture(name).to_str().unwrap()])
        .output()
        .expect("check")
        .status
        .success()
}

/// `manitc check`'s exit status for a source string written to a temp file.
/// P45's agreement test builds its programs rather than shipping 45 fixtures.
fn check_source(tag: &str, src: &str) -> bool {
    let p = tmp(&format!("{}.mt", tag));
    std::fs::write(&p, src).expect("write temp source");
    Command::new(manitc())
        .args(["check", p.to_str().unwrap()])
        .output()
        .expect("check")
        .status
        .success()
}

fn assert_t3_value(name: &str) {
    let want = expected(name);
    match run_t3(name) {
        Ok(got) => assert_eq!(got, want, "{}: T3 printed the wrong value", name),
        Err(e) => panic!("{}: did not compile for T3:\n{}", name, e),
    }
}

/// payload variant CONSTRUCTED — the defect
#[test]
#[ignore = "§61: payload variant CONSTRUCTED — the defect"]
fn pe61_construct_payload() {
    assert_t3_value("pe61_construct_payload");
}

/// payload enum declared AND matched, never constructed — control
#[test]
fn pe61_match_only() {
    assert_t3_value("pe61_match_only");
}

/// enum with no payload variants, constructed — control
#[test]
fn pe61_nopayload() {
    assert_t3_value("pe61_nopayload");
}

/// mixed enum, only the PLAIN variant constructed — control
#[test]
fn pe61_mixed_plain_variant() {
    assert_t3_value("pe61_mixed_plain_variant");
}

/// impl<T> Pair<T> swap — THE DOCUMENTED FORM. Both backends print 2 2.
#[test]
fn gs62_impl_single_param() {
    assert_t3_value("gs62_impl_single_param");
}

/// impl<A,B> Pair<A,B> swap, mixed int/str — returns an address
#[test]
fn gs62_impl_two_param() {
    assert_t3_value("gs62_impl_two_param");
}

/// method returning the UNSWAPPED Pair<A,B> — so it is not about swapping
#[test]
fn gs62_impl_noswap() {
    assert_t3_value("gs62_impl_noswap");
}

/// generic struct read back out of a Vec
#[test]
fn gs62_vec_of_generic() {
    assert_t3_value("gs62_vec_of_generic");
}

/// generic struct into a generic free fn — and it was never about generics.
///
/// Recorded for a day as the third defect in P44's family: a struct literal's
/// bare type `Pair` never unifies with `Pair<T>`, so a generic struct could not
/// be passed to a generic free function at all. **It was about the NAME.**
/// `resolve_type` carried a hardcoded list of built-in generic constructors and
/// `Pair` was on it with nothing behind it, so a user's own `struct Pair<T>`
/// was shadowed: the ANNOTATION resolved to `Generic("Pair", [..])` and the
/// LITERAL to `Struct("Pair")`, and those do not unify. Renaming the struct to
/// `Duo` — one `sed`, same program — compiled and printed `2 1` on both
/// backends, which is what showed it (report.txt P67).
///
/// Kept spelled `Pair` on purpose. The name IS the test.
#[test]
fn gs62_generic_freefn() {
    assert_t3_value("gs62_generic_freefn");
}

/// P67: RENAMING A STRUCT MUST NOT CHANGE WHAT A PROGRAM MEANS — and this
/// records exactly which names still break that.
///
/// The test the defect deserved, rather than the one that found it. A program
/// is compiled once per struct NAME, identical otherwise, and the answers are
/// compared against a control name on no list.
///
/// TWO SETS, AND THE DISTINCTION IS THE FINDING. `resolve_type` carries a
/// hardcoded list of generic constructors, and a name on it shadows a user's
/// own `struct <name><T>`: the ANNOTATION resolves to `Generic(name, [..])`
/// while the LITERAL resolves to `Struct(name)`, and those do not unify.
///
///   * `Pair` was on that list with NOTHING BEHIND IT — no stdlib source, no
///     IR, no backend — so it shadowed a user struct for nothing. Removed
///     (report.txt P67), and it is asserted here to work.
///   * The other nine have real implementations, so shadowing them is a
///     genuine collision rather than a phantom, and "the built-in wins" is a
///     defensible rule. They are listed below as still-shadowed, so this test
///     ENCODES the current convention rather than imposing a new one — and it
///     fails if the set changes in either direction.
///
/// What is NOT defensible is that the collision is silent: the program is
/// refused with `expected Vec<<unknown>>, found Vec`, which names neither the
/// cause nor the remedy. That is recorded as P67's open half.
#[test]
fn p67_a_struct_name_must_not_change_the_program() {
    // Names with no built-in behind them: a user struct must work.
    const FREE: &[&str] = &["Duo", "Pair", "Task", "String"];
    // Names whose built-in has an implementation, which currently wins.
    const SHADOWED: &[&str] = &[
        "Vec", "Map", "Set", "Deque", "TernaryTrie", "Channel", "Mutex",
        "Result", "Range",
    ];
    let prog = |n: &str| {
        format!(
            "struct {n}<T> {{ pub first: T, pub second: T }}\n\
             fn swap<T>(p: {n}<T>) -> {n}<T> {{ {n} {{ first: p.second, second: p.first }} }}\n\
             fn main() {{ let p = {n} {{ first: 1, second: 2 }}; let q = swap(p);\n\
                 io::println_int(q.first); }}\n"
        )
    };

    let broken: Vec<&str> = FREE
        .iter()
        .copied()
        .filter(|n| !check_source(&format!("p67_free_{n}"), &prog(n)))
        .collect();
    assert!(
        broken.is_empty(),
        "declaring `struct {}<T>` and using it is refused, while the identical \
         program under another name compiles. A name with no built-in behind it \
         must not shadow a user's own declaration — report.txt P67. Refused: {:?}",
        broken.first().unwrap_or(&"?"),
        broken,
    );

    let no_longer_shadowed: Vec<&str> = SHADOWED
        .iter()
        .copied()
        .filter(|n| check_source(&format!("p67_sh_{n}"), &prog(n)))
        .collect();
    assert!(
        no_longer_shadowed.is_empty(),
        "these names no longer shadow a user struct: {:?}. That may well be an \
         improvement — if so, move them to FREE and say so in report.txt P67. \
         This half of the test exists to make the convention visible, not to \
         defend it.",
        no_longer_shadowed,
    );
}

/// generic struct, field access only — control
#[test]
fn gs62_fields_only() {
    assert_t3_value("gs62_fields_only");
}

/// the same swap written INLINE at the call site — control
#[test]
fn gs62_swap_inline() {
    assert_t3_value("gs62_swap_inline");
}

/// impl on a NON-generic struct — control, so impl is not the culprit
#[test]
fn gs62_impl_nongeneric() {
    assert_t3_value("gs62_impl_nongeneric");
}

/// two type params on a FUNCTION — control, so generics are not the culprit
#[test]
fn gs62_two_param_fn() {
    assert_t3_value("gs62_two_param_fn");
}

/// str through <T: Ord> — now REFUSED, which is P45's fix.
///
/// THIS ROW CHANGED THE QUESTION IT ASKS, AND THAT IS THE FINDING. It used to
/// assert that `largest("mm", "aa")` prints `mm`, which is what the probe
/// session expected to see once the defect was gone. It is not what should
/// happen: `str` has no ordering in maniT at all — `"mm" > "aa"` is a clean
/// TypeError — so a `<T: Ord>` bound instantiated at `str` is exactly the
/// program the bound exists to reject. Making it PRINT the right answer would
/// have meant giving `str` an ordering, a language change nobody asked for,
/// arrived at by taking a fixture's expectation as a specification.
#[test]
fn ord63_str_via_bound() {
    assert!(
        !check_status("ord63_str_via_bound"),
        "ord63_str_via_bound: `largest<T: Ord>(\"mm\", \"aa\")` compiles again. \
         The bound does not bind, and the program returns its SECOND argument \
         every time — report.txt P45."
    );
}

/// direct str comparison — correctly a TypeError. The front end KNOWS.
///
/// The only row whose expected outcome is a REFUSAL. It is here because it is
/// the other half of §63: a direct `"mm" > "aa"` is a clean TypeError, so the
/// front end already knows the answer that the same comparison reaches through
/// `<T: Ord>` without a murmur. Same compiler, one syntactic form away,
/// opposite verdicts.
#[test]
fn ord63_str_direct() {
    assert!(
        !check_status("ord63_str_direct"),
        "ord63_str_direct: a direct str comparison should still be a TypeError — \
         if this now passes, the front end lost the check that makes §63 a defect"
    );
}

/// float through a generic — the returned VALUE was corrupted, not just the
/// choice. FIXED by P65's monomorphisation.
///
/// NOT P45 and never about the bound: `float` IS ordered, so the bound was
/// correctly satisfied and the call correctly accepted. The value was
/// destroyed by type erasure — the argument numerically CAST into an `i64`
/// parameter, the comparison done on integers, and the result read back as
/// float BITS. `largest(1.5, 2.5)` printed 1e-323 (the integer 2 as a bit
/// pattern) and `largest(-1.5, -2.5)` printed NaN (the integer -1, all ones).
#[test]
fn ord63_float_via_bound() {
    assert_t3_value("ord63_float_via_bound");
}

/// print_float and direct float compare — controls ruling out the printer
#[test]
fn ord63_float_controls() {
    assert_t3_value("ord63_float_controls");
}

/// int and trit through the bound — control, the bound works for these
#[test]
fn ord63_int_trit_via_bound() {
    assert_t3_value("ord63_int_trit_via_bound");
}

/// P45: `Ord` ADMITS EXACTLY THE TYPES `<` AND `>` ADMIT, CHECKED BY ASKING
/// BOTH AND COMPARING — not by restating a list.
///
/// The defect was two places deciding one question and disagreeing.
/// `binop_type` asks `ManiType::is_comparable`; `check_generic_bounds` asked
/// "is it a primitive", which is a question about what a type is NOT, and
/// `str`, `[int; 2]`, `(int, int)`, `Vec<T>` and `Result<T, E>` all answered
/// yes. So `largest<T: Ord>("mm", "aa")` compiled and returned `"aa"` every
/// time while `"mm" > "aa"` was a clean TypeError one syntactic form away.
///
/// THIS TEST CROSSES AN ORIGIN BOUNDARY, WHICH IS WHY IT IS WORTH MORE THAN
/// THE TABLE IT REPLACED. Asserting "these eleven types satisfy `Ord`" would
/// be me checking a list against the list I had just written — P64's
/// common-origin trap, where agreement carries no information. Asking the
/// OPERATOR and asking the BOUND and requiring the same answer tests two
/// independently-written code paths against each other, and it keeps its
/// meaning if the language later makes `str` ordered: both verdicts move
/// together and the test still passes without being edited.
///
/// EACH CASE CARRIES A CONTROL, AND THE CONTROL IS NOT CEREMONY. A malformed
/// program fails BOTH ways and therefore AGREES — a vacuous pass, and the
/// failure mode this exact test shape invites. The control compiles the
/// declarations and the two bindings with no comparison at all, so a case
/// that stops meaning anything says so instead of going quietly green.
#[test]
fn ord63_bound_agrees_with_the_operator() {
    // (label, extra declarations, type, literal a, literal b)
    const CASES: &[(&str, &str, &str, &str, &str)] = &[
        ("int", "", "int", "1", "2"),
        ("float", "", "float", "1.5", "2.5"),
        ("trit", "", "trit", "+", "-"),
        ("bool", "", "bool", "true", "false"),
        ("bool3", "", "bool3", "True", "False"),
        ("char", "", "char", "'a'", "'b'"),
        ("tryte", "", "tryte", "1 as tryte", "2 as tryte"),
        ("t9", "", "t9", "1 as t9", "2 as t9"),
        ("t27", "", "t27", "1 as t27", "2 as t27"),
        ("t54", "", "t54", "1 as t54", "2 as t54"),
        ("tfloat", "", "tfloat", "1.5 as tfloat", "2.5 as tfloat"),
        ("str", "", "str", "\"aa\"", "\"bb\""),
        ("array", "", "[int; 2]", "[1, 2]", "[3, 4]"),
        ("tuple", "", "(int, int)", "(1, 2)", "(3, 4)"),
        ("struct", "struct P { pub v: int }", "P", "P { v: 1 }", "P { v: 2 }"),
        ("vec", "use std::collections;", "Vec<int>", "Vec::new()", "Vec::new()"),
        ("result", "", "Result<int, str>", "Ok(1)", "Ok(2)"),
    ];

    let mut disagreed = Vec::new();
    for (label, decls, ty, a, b) in CASES {
        let bind = format!("let x: {ty} = {a}; let y: {ty} = {b};");
        let control = format!("{decls}\nfn main() {{ {bind} }}");
        let direct = format!("{decls}\nfn main() {{ {bind} if x > y {{ }} }}");
        let via_bound = format!(
            "{decls}\nfn gt2<T: Ord>(p: T, q: T) -> bool {{ p > q }}\n\
             fn main() {{ {bind} if gt2(x, y) {{ }} }}"
        );

        assert!(
            check_source(&format!("ord63_ctl_{label}"), &control),
            "ord63 agreement: the CONTROL for `{label}` does not compile, so this \
             row proves nothing — both verdicts below would be `reject` and would \
             agree vacuously.\n{control}"
        );

        let d = check_source(&format!("ord63_dir_{label}"), &direct);
        let v = check_source(&format!("ord63_bnd_{label}"), &via_bound);
        if d != v {
            disagreed.push(format!(
                "  {label}: `x > y` {} but `gt2<T: Ord>(x, y)` {}",
                if d { "compiles" } else { "is rejected" },
                if v { "compiles" } else { "is rejected" },
            ));
        }
    }

    assert!(
        disagreed.is_empty(),
        "the `Ord` bound and the `>` operator disagree about {} type(s) — \
         each such type is a program that type-checks through the bound and \
         computes a wrong answer, which is report.txt P45:\n{}",
        disagreed.len(),
        disagreed.join("\n"),
    );
}

/// P45: an `impl Ord` does NOT satisfy an ordering bound, because `>` never
/// dispatches to it — and the old diagnostic told the user to write one.
///
/// Following that advice made the program compile and print `aa` again, which
/// is the original wrong answer restored by the fix's own suggestion. The
/// remedy a diagnostic names has to be a remedy.
#[test]
fn ord63_user_impl_ord_is_not_a_workaround() {
    let src = "use std::io;\n\
               trait Ord { fn cmp(self, other: str) -> int; }\n\
               impl Ord for str { fn cmp(self, other: str) -> int { 0 } }\n\
               fn largest<T: Ord>(a: T, b: T) -> T { if a > b { a } else { b } }\n\
               fn main() { io::println(largest(\"mm\", \"aa\")); }\n";
    assert!(
        !check_source("ord63_user_impl", src),
        "`impl Ord for str` satisfied the bound again. It cannot make `>` work — \
         the comparison operators are lowered by the compiler and read no trait \
         table — so accepting it returns the program to printing `aa` for \
         `largest(\"mm\", \"aa\")`."
    );
}

/// REFUTES the reference's address explanation — and is now refused outright.
///
/// The program declares `hi` before `lo` and then `lo2` before `hi2`, so if
/// the comparison really compared ADDRESSES, as `language-reference.md` §14
/// claimed until 26 August 2026, the two lines would differ. They did not:
/// both printed `aaa`, because the comparison was simply always false. That
/// measurement is what retired the explanation, and it is recorded in §14's
/// dated notice, because it can no longer be re-derived by running this
/// program — with P45 fixed the program does not compile at all.
///
/// The row is kept, asserting the refusal, since the source is the record of
/// what the refutation was performed ON.
#[test]
fn ord63_address_theory() {
    assert!(
        !check_status("ord63_address_theory"),
        "ord63_address_theory: a `<T: Ord>` bound instantiated at `str` compiles \
         again — see report.txt P45."
    );
}

// ---------------------------------------------------------------------------
// report.txt P65 — a generic function is type-erased to a machine word
// ---------------------------------------------------------------------------

/// The RETURN type of a generic call, which is the half that is not about
/// floats at all.
///
/// `fn id<T>(x: T) -> T` gave its call the type `Unknown`, so `q.first` and
/// `q.second` were both field lookups on `<unknown>`, which is not a key in
/// the struct table, so both resolved to slot 0. It printed `1 1`. A one-field
/// struct would have been right by luck, which is why the fixture has two.
#[test]
fn p65_generic_return_field() {
    assert_t3_value("p65_generic_return_field");
}

/// ONE generic, FOUR calls, TWO types — the shape a single erased body cannot
/// serve, whatever its representation.
///
/// This is the test that would still mean something if the fix were replaced
/// by a different one. A single body has to pick a comparison and a width; the
/// int calls need integer comparison and the float calls need float
/// comparison, and `-1.5 > -2.5` is the case that separates them from a
/// bit-pattern compare, since IEEE-754 negatives order the opposite way to
/// their bit patterns as integers. Interleaving int and float calls also
/// checks that instantiations do not overwrite one another.
#[test]
fn p65_two_instantiations() {
    assert_t3_value("p65_two_instantiations");
}

/// `docs/language-reference.md` §14's OWN example, pinned.
///
/// The section claims a generic function is compiled once per distinct
/// combination of concrete argument types, and illustrates it with exactly
/// this `max`. Documentation defects in this file have three times now been
/// true-sounding sentences nobody ran (P51, P55, §14's address explanation),
/// so the claim is pinned rather than reviewed. Note the bound is ABSENT here
/// on purpose: it is the reference's example verbatim, and it also shows the
/// fix does not depend on `T: Ord`.
#[test]
fn p65_reference_example() {
    assert_t3_value("p65_reference_example");
}

/// `docs/language-reference.md` §14's generic-struct example, pinned.
///
/// The section's example declared its fields WITHOUT `pub` and never read one,
/// so it could be read and not used: copying it and adding `p.first` gives
/// "field 'first' of type 'Pair' is private". A fourth documentation defect of
/// the shape this file keeps finding — no false sentence, an example that
/// stops one line before the line that fails. It now shows the field read and
/// the float case, and this pins both.
#[test]
fn p68_reference_generic_struct() {
    assert_t3_value("p68_reference_generic_struct");
}

/// P68: a generic struct's field holds the value it was given.
///
/// `Box2 { a: 1.5 }` used to hold the integer 1, and `p.a` read back 5e-324.
/// Two lines of the struct-literal lowering disagreed: the value was coerced
/// to the DECLARED field type — `Unknown` for a field declared `T`, which
/// means `i64`, which truncates — and then STORED with the value's own type,
/// `F64`. A generic struct could not carry a float at all, on either backend,
/// and no test noticed because every generic struct in every corpus holds
/// integers.
///
/// Both orderings, on purpose. Positive doubles order the same way as their
/// bit patterns read as integers, so the `(1.5, 2.5)` line alone passes under
/// an integer comparison; `(-1.5, -2.5)` is what distinguishes them. The `int`
/// line is the control: whatever changes here must leave the case that always
/// worked alone.
#[test]
fn p68_generic_struct_float_field() {
    assert_t3_value("p68_generic_struct_float_field");
}

/// The SAME defect through a METHOD — P65's last piece, closed by P69, and
/// this row is the one that proves it rather than a pair that passes anyway.
///
/// P65 instantiated generic FREE FUNCTIONS. A method in an `impl<T>` block was
/// not, so its body was checked once with `T` erased and its comparison was an
/// INTEGER comparison of two float bit patterns. P69 instantiates it, binding
/// the impl's parameters from the RECEIVER's type arguments.
///
/// **IT USED TO TEST ONLY `(1.5, 2.5)`, AND P68 MADE THAT PAIR PASS WHILE THE
/// DEFECT STOOD.** Positive IEEE-754 doubles order the same way as their bit
/// patterns read as integers, so an integer comparison gets the right answer
/// on them; once P68 stopped the field store truncating the value, the row
/// printed `2.5` and would have been un-`#[ignore]`d as fixed. The negative
/// pair is what separates the two — negatives order the OPPOSITE way — and
/// `(-1.5, -2.5)` answered `-2.5` for another whole session behind it.
///
/// This is the file's own rule turned on its author: **a single case for a
/// comparison is half a test**, and the half that was missing is the one that
/// distinguishes the fix from the accident. Keep both pairs.
#[test]
fn p65_impl_method_instantiated() {
    assert_t3_value("p65_impl_method_still_erased");
}

/// TWO instantiations of ONE method in ONE program, and both orderings of each.
///
/// The single-instantiation row cannot tell a monomorphised method from a
/// method that happens to be lowered for the type the one call site used. This
/// one needs `Box2::bigger$float` and `Box2::bigger$int` to coexist and to
/// disagree about what `>` means, which is the property the mangled name is
/// for. The `int` rows are the control: `int` is unaffected by erasure
/// *because its representation IS the erasure*, so they were green throughout
/// and a change that broke them would be a change to the ordinary path.
#[test]
fn p69_impl_method_two_instantiations() {
    assert_t3_value("p69_impl_method_two_instantiations");
}

/// The receiver reached through TWO generic free functions.
///
/// Instantiating at the top level is not enough, and this is the row that says
/// so: `mid<T>(x: B<T>)` binds `T` from a parameter whose declared type is
/// `B<T>` and whose actual type is `Struct("B", [float])` — and `bind_generics`
/// matched only `ManiType::Generic` there, because `ManiType::Struct` did not
/// carry arguments until P68. So `mid` stayed erased, `x` inside it was a
/// `B<unknown>`, and the method call had nothing to bind from. Monomorphisation
/// stopped at the first boundary it crossed, silently.
///
/// Both orderings, so the row distinguishes the fix from `a` always winning.
#[test]
fn p69_impl_method_through_generic_fn() {
    assert_t3_value("p69_impl_method_through_generic_fn");
}

/// TWO type parameters, with the float in each slot in turn.
///
/// The impl's parameters are mapped to the receiver's arguments POSITIONALLY —
/// that is the only reading `ast::ImplBlock` supports, since it reduces
/// `impl<A, B> Two<A, B>` to a base name plus an ordered list. A mapping that
/// silently reversed, or that bound every parameter to the first argument,
/// gets the `Two<float, int>` rows right by luck; the `Two<int, float>` rows
/// are what catch it.
///
/// **THE FIRST VERSION OF THIS ROW WAS HOLLOW AND THE CONTROL BINARY SAID SO.**
/// It was `fn geta(self) -> A { self.a }` — a field read and nothing else — so
/// erasure could not change the answer, and it passed on the pre-P69 compiler
/// unchanged. A test for a type-erasure defect has to make the program DO
/// something the erased type gets wrong, and for `T` that means a COMPARISON:
/// negative doubles order the opposite way from their bit patterns read as
/// integers. Written that way it answers `-2.5 / -7 / -7 / -2.5` on the
/// control and `-1.5 / -7 / -7 / -1.5` here.
#[test]
fn p69_impl_method_two_type_params() {
    assert_t3_value("p69_impl_method_two_type_params");
}

/// `docs/language-reference.md` §14's generic-method example, verbatim.
///
/// The reference now states an OUTPUT for this program (`-1.5`), and a stated
/// output is a claim that can quietly stop being true. Three documentation
/// defects in that file (P51, P55, §14's refuted address explanation) had no
/// false sentence in them — an absence, a word, and a mechanism that was true
/// when written — which is the argument for pinning a documented claim with a
/// test rather than re-reading the prose.
#[test]
fn p69_reference_impl_method() {
    assert_t3_value("p69_reference_impl_method");
}

// ---------------------------------------------------------------------------
// report.txt P48 / P50 — §64, `str` is bytes wearing the vocabulary of scalars
// ---------------------------------------------------------------------------
//
// These four are shaped differently from the rows above because §64 is three
// defects wearing one name, and each needs a different assertion:
//
//   * P50 — a HOST PANIC, now fixed. Asserted on continued execution.
//   * the byte/scalar confusion in `len`, still open. Asserted on the value.
//   * a CROSS-BACKEND DIVERGENCE in `char as int`, still open. Asserted on
//     agreement between the backends, since T3 alone gives the right answer
//     and a value assertion would pass while the defect stands.

/// Run for T3 and LLVM and return both outputs.
fn both(name: &str) -> (String, String) {
    let out = tmp(name);
    let c = Command::new(manitc())
        .args(["compile", fixture(name).to_str().unwrap(), "--target", "t3",
               "-o", out.to_str().unwrap()])
        .output().expect("compile t3");
    assert!(c.status.success(), "{}: T3 compile failed", name);
    let r = Command::new(manitc())
        .args(["run-t3", out.with_extension("t3b").to_str().unwrap()])
        .output().expect("run t3");
    let t3: String = String::from_utf8_lossy(&r.stdout)
        .lines().filter(|l| !l.starts_with("[T3ISA]"))
        .map(|l| format!("{}\n", l)).collect();

    let binp = out.with_extension("bin");
    let c2 = Command::new(manitc())
        .args(["compile", fixture(name).to_str().unwrap(), "--target", "llvm",
               "-o", binp.to_str().unwrap()])
        .output().expect("compile llvm");
    assert!(c2.status.success(), "{}: LLVM compile failed", name);
    let r2 = Command::new(&binp).output().expect("run llvm");
    (t3, String::from_utf8_lossy(&r2.stdout).into_owned())
}

/// **`str::reverse` on a multi-byte string must not kill the emulator.**
///
/// report.txt P50, and it was worse than the report said. `str::reverse` is
/// ManiT source that walks a string one index at a time, and the T3 emulator's
/// `str_slice` sliced a Rust `String` by byte index — which PANICS when the
/// index splits a character, taking the whole emulator process down with a
/// Rust backtrace. Not a wrong answer and not a T3 trap: a crash of the
/// toolchain, reachable from any program containing a non-ASCII literal.
///
/// **The assertion is on `done`, not on the reversed string.** The mangled
/// string is the symptom of §64's byte/scalar confusion, which is still open;
/// the missing `done` was the finding. What this pins is that execution
/// CONTINUES past the call.
#[test]
fn s64_reverse_does_not_kill_the_emulator() {
    let (t3, ll) = both("s64_reverse_kills_t3");
    assert!(
        t3.contains("done"),
        "T3 stopped producing output at the reverse — everything after it was \
         lost. Got {:?}",
        t3
    );
    assert!(ll.contains("done"), "LLVM lost its output too: {:?}", ll);
}

/// **`str::len` counts BYTES, so it and `byte_len` are synonyms.** Open: the
/// two functions assert a distinction their values do not have.
#[test]
#[ignore = "§64/P48: str::len counts BYTES; it and byte_len are synonyms"]
fn s64_len_counts_characters_not_bytes() {
    let (t3, _) = both("s64_len_equals_bytelen");
    assert_eq!(t3, expected("s64_len_equals_bytelen"), "str::len should count characters");
}

/// **`char as int` is UNSIGNED on T3 and SIGNED on LLVM for any byte >= 128.**
///
/// Asserted on AGREEMENT rather than on a value, and that is the point: T3
/// alone gives 195, which is what `.expected` holds, so a value assertion
/// against T3 would pass while the divergence stands. ASCII agrees on both,
/// which is why nothing caught this — see the control below.
#[test]
#[ignore = "§64/P48: char as int is UNSIGNED on T3 (195) and SIGNED on LLVM (-61)"]
fn s64_char_as_int_agrees_across_backends() {
    let (t3, ll) = both("s64_char_as_int_sign");
    assert_eq!(t3, ll, "backends disagree on `char as int` for a byte >= 128");
}

/// The same three calls on ASCII — control, so this is not `str` being broken
/// generally, and it is why five weeks of corpora never saw §64.
#[test]
fn s64_ascii_control() {
    let (t3, ll) = both("s64_ascii_control");
    assert_eq!(t3, expected("s64_ascii_control"), "ASCII must be correct");
    assert_eq!(t3, ll, "ASCII must agree across backends");
}

/// Printing a multi-byte literal untouched — control, so the I/O path handles
/// multi-byte fine and what is left is byte-level manipulation of a UTF-8
/// sequence.
#[test]
fn s64_print_multibyte_control() {
    let (t3, ll) = both("s64_print_multibyte_control");
    assert_eq!(t3, expected("s64_print_multibyte_control"), "printing must be correct");
    assert_eq!(t3, ll, "printing must agree across backends");
}

// ---------------------------------------------------------------------------
// report.txt P51 — the move rule, as documented in language-reference.md §22
// ---------------------------------------------------------------------------
//
// Section 22 was written on 26 August 2026 because the checker had existed
// without any documentation at all. This test exists so the section cannot
// quietly stop being true — which is not hypothetical: §14 of the same
// document still explained an unrelated defect by a mechanism (address
// comparison) that P45 measured and refuted. A documented rule with no test
// is a claim nobody re-checks.
//
// Each row is one line of §22's move/no-move table. If the compiler changes,
// this goes red and the section has to be rewritten with it.

fn checks(name: &str, body: &str) -> bool {
    let src = tmp(&format!("borrow_{}.mt", name));
    std::fs::write(
        &src,
        format!(
            "use std::io;\n\
             fn take(x: str) -> int {{ return str::len(x); }}\n\
             struct Point {{ pub x: str }}\n\
             fn main() {{\n{}\n}}\n",
            body
        ),
    )
    .expect("write");
    Command::new(manitc())
        .args(["check", src.to_str().unwrap()])
        .output()
        .expect("check")
        .status
        .success()
}

/// **§22: assignment MOVES.** All four rows of the "moves" half of the table.
#[test]
fn p51_the_documented_move_table_holds_for_moves() {
    assert!(!checks("let_then_use", "    let s: str = \"ab\"; let t: str = s; io::println(s);"),
        "§22 says `let t = s` moves — a later use of `s` must be refused");
    assert!(!checks("let_twice", "    let s: str = \"ab\"; let t: str = s; let u: str = s; io::println(t);"),
        "§22 says a second `let` from a moved binding is refused");
    assert!(!checks("tuple_elem", "    let s: str = \"ab\"; let t: (str, int) = (s, 1); io::println(s);"),
        "§22 says a TUPLE literal moves its elements");
    assert!(!checks("struct_field", "    let s: str = \"ab\"; let p: Point = Point { x: s }; io::println(s);"),
        "§22 says a struct literal moves its fields");
}

/// **§22: passing to a function does NOT move**, which is the half a reader
/// arriving from Rust gets backwards.
#[test]
fn p51_the_documented_move_table_holds_for_borrows() {
    assert!(checks("call_twice", "    let s: str = \"ab\"; take(s); take(s); io::println(s);"),
        "§22 says a call never consumes its argument");
    assert!(checks("call_then_let", "    let s: str = \"ab\"; take(s); let t: str = s; io::println(t);"),
        "§22 says a call leaves the binding live for a later move");
    assert!(checks("call_in_loop", "    let s: str = \"ab\"; for i in 0..3 { io::print_int(take(s)); }"),
        "§22 says calling in a loop is fine, since a call does not move");
    assert!(checks("method_arg", "    let s: str = \"ab\"; let v: Vec<str> = Vec::new(); v.push(s); io::println(s);"),
        "§22 says a method argument does not move either");
}

/// **§22: rebinding clears a move, shadowing is per-binding, and moving a
/// non-local inside a loop is refused.**
#[test]
fn p51_the_documented_move_table_holds_for_scopes() {
    assert!(checks("rebind", "    let mut s: str = \"ab\"; let t: str = s; s = \"cd\"; io::println(s); io::println(t);"),
        "§22 says assigning a fresh value makes a moved binding usable again");
    assert!(checks("shadow", "    let s: str = \"ab\"; if true { let s: str = \"cd\"; let t: str = s; io::println(t); } io::println(s);"),
        "§22 says moving an inner shadow does not poison the outer binding");
    assert!(!checks("move_in_loop", "    let s: str = \"ab\"; for i in 0..3 { let t: str = s; io::println(t); }"),
        "§22 says moving a non-local inside a loop body is refused");
    assert!(checks("int_is_copy", "    let n: int = 1; let m: int = n; io::print_int(n); io::print_int(m);"),
        "§22 lists `int` as Copy, so binding it twice must be fine");
}

/// **The array/tuple asymmetry, pinned as the open sub-finding it is.**
///
/// §22 records the array row with a warning not to rely on it. This test holds
/// the CURRENT behaviour so that changing it is a deliberate act that also
/// updates the section — not a silent drift in either direction.
#[test]
fn p51_an_array_literal_does_not_move_its_elements_yet() {
    assert!(
        checks("array_elem", "    let s: str = \"ab\"; let a: [str; 2] = [s, s]; io::println(s); io::println(a[0]);"),
        "an array literal moving its elements would be a CHANGE — correct, \
         probably, and it must update language-reference.md §22's table and \
         its warning note at the same time"
    );
}

// ---------------------------------------------------------------------------
// report.txt P46 (third element type), P53, P54, P55 — function values
// ---------------------------------------------------------------------------

fn runs_and_prints(name: &str, body: &str, want: &str) {
    let src = tmp(&format!("fnv_{}.mt", name));
    std::fs::write(&src, body).expect("write");
    let out = tmp(&format!("fnv_{}", name));
    let c = Command::new(manitc())
        .args(["compile", src.to_str().unwrap(), "--target", "t3",
               "-o", out.to_str().unwrap()])
        .output().expect("compile");
    assert!(c.status.success(), "{}: T3 compile failed:\n{}",
            name, String::from_utf8_lossy(&c.stderr));
    let r = Command::new(manitc())
        .args(["run-t3", out.with_extension("t3b").to_str().unwrap()])
        .output().expect("run");
    let got: String = String::from_utf8_lossy(&r.stdout)
        .lines().filter(|l| !l.starts_with("[T3ISA]"))
        .map(|l| format!("{}\n", l)).collect();
    assert_eq!(got, want, "{}: T3 output", name);

    let bin = out.with_extension("bin");
    let c2 = Command::new(manitc())
        .args(["compile", src.to_str().unwrap(), "--target", "llvm",
               "-o", bin.to_str().unwrap()])
        .output().expect("compile llvm");
    assert!(c2.status.success(), "{}: LLVM compile failed:\n{}",
            name, String::from_utf8_lossy(&c2.stderr));
    if bin.exists() {
        let r2 = Command::new(&bin).output().expect("run llvm");
        assert_eq!(String::from_utf8_lossy(&r2.stdout), want, "{}: LLVM output", name);
    }
}

/// **P46 reaches a THIRD element type: a `Vec` of function pointers.**
///
/// P46 was found on `Vec<str>` and fixed by reconciling a native's declared
/// return type with the IR's at the definition. `Vec<fn(int)->int>` is the
/// same defect through a different element type — and the one that would have
/// bitten `Vec::map`-style code held in a collection. Before the fix, LLVM
/// refused the module with P46's exact signature (`'%tN' defined with type
/// 'i64' but expected 'ptr'`) while T3 was correct all along, which is the
/// same asymmetry the original case had.
#[test]
fn p46_a_vec_of_function_pointers_keeps_its_element_type() {
    runs_and_prints(
        "vec_of_fnptr",
        "use std::io;\n\
         fn a(x: int) -> int { return x + 1; }\n\
         fn b(x: int) -> int { return x * 2; }\n\
         fn main() {\n\
         \x20   let fs: Vec<fn(int)->int> = Vec::new();\n\
         \x20   fs.push(a); fs.push(b);\n\
         \x20   for i in 0..fs.len() { let g: fn(int)->int = fs.get(i); io::print_int(g(6)); io::newline(); }\n\
         }\n",
        "7\n12\n",
    );
}

/// **P53's WORKAROUND, which is the half worth pinning.**
///
/// `let f: fn(int) -> int = dbl;` works on both backends. `let f = dbl;` does
/// not — the callee symbol is taken from the BINDING, so T3 reports
/// `Undefined label: f` and LLVM `use of undefined value '@f'`. The annotation
/// is a complete workaround, and it is what `docs/language-reference.md` now
/// tells a reader to write, so it is the thing that must not regress.
#[test]
fn p53_an_annotated_function_binding_works() {
    runs_and_prints(
        "bind_named_annotated",
        "use std::io;\n\
         fn dbl(x: int) -> int { return x * 2; }\n\
         fn main() { let f: fn(int)->int = dbl; io::print_int(f(5)); io::newline(); }\n",
        "10\n",
    );
}

/// A lambda bound without an annotation works — and this is WHY P53 survived.
/// The lambda is emitted under the name it is bound to, so `@f` resolves by
/// coincidence. Every lambda in the corpus is bound once, at its definition.
#[test]
fn p53_a_lambda_binding_works_without_an_annotation() {
    runs_and_prints(
        "bind_lambda",
        "use std::io;\n\
         fn main() { let f = fn(x: int) => x * 2; io::print_int(f(5)); io::newline(); }\n",
        "10\n",
    );
}

/// A function-typed PARAMETER works, which is why the whole higher-order
/// stdlib surface (`Vec::map`, `Vec::filter`, `Vec::fold`) is unaffected by
/// P53. Control.
#[test]
fn p53_a_function_typed_parameter_works() {
    runs_and_prints(
        "fn_as_param",
        "use std::io;\n\
         fn dbl(x: int) -> int { return x * 2; }\n\
         fn apply(g: fn(int)->int, v: int) -> int { return g(v); }\n\
         fn main() { io::print_int(apply(dbl, 5)); io::newline(); }\n",
        "10\n",
    );
}

/// **P53 itself.** Open: binding an existing function with no annotation emits
/// a call to the binding's name.
#[test]
#[ignore = "P53: `let f = dbl;` emits a call to `f`, a symbol nothing defines"]
fn p53_an_unannotated_function_binding_works() {
    runs_and_prints(
        "bind_named_unannotated",
        "use std::io;\n\
         fn dbl(x: int) -> int { return x * 2; }\n\
         fn main() { let f = dbl; io::print_int(f(5)); io::newline(); }\n",
        "10\n",
    );
}

/// **P54.** Open: a function type in RETURN position has its parameters
/// absorbed into the enclosing signature, so `pick()` is reported as expecting
/// one argument. Annotating cannot help — there is nowhere to put it — and it
/// fails in `check` rather than in codegen, which is what separates it from
/// P53.
#[test]
#[ignore = "P54: `fn pick() -> fn(int)->int` makes `pick` expect 1 argument"]
fn p54_a_function_type_in_return_position_parses() {
    runs_and_prints(
        "ret_fntype",
        "use std::io;\n\
         fn dbl(x: int) -> int { return x * 2; }\n\
         fn pick() -> fn(int)->int { return dbl; }\n\
         fn main() { let g: fn(int)->int = pick(); io::print_int(g(5)); io::newline(); }\n",
        "10\n",
    );
}

/// **P55: a lambda cannot capture, and the reference now says so.**
///
/// The diagnostic is the documented one, so this pins the TEXT a reader is
/// told to expect as well as the refusal.
#[test]
fn p55_a_capturing_lambda_is_refused_with_a_useful_message() {
    let src = tmp("fnv_capture.mt");
    std::fs::write(
        &src,
        "use std::io;\nfn main() { let k: int = 3; let f = fn(x: int) => x * k; io::print_int(f(5)); }\n",
    ).expect("write");
    let o = Command::new(manitc())
        .args(["check", src.to_str().unwrap()])
        .output().expect("check");
    assert!(!o.status.success(), "a capturing lambda must be refused");
    let text = format!("{}{}",
        String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
    assert!(
        text.contains("closures are not yet supported"),
        "the message language-reference.md quotes must still be the one emitted:\n{}",
        text
    );
    assert!(text.contains("'k'"), "the message must name the captured variable:\n{}", text);
}
