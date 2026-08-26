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
#[ignore = "§62: impl<T> Pair<T> swap — THE DOCUMENTED FORM. Both bac"]
fn gs62_impl_single_param() {
    assert_t3_value("gs62_impl_single_param");
}

/// impl<A,B> Pair<A,B> swap, mixed int/str — returns an address
#[test]
#[ignore = "§62: impl<A,B> Pair<A,B> swap, mixed int/str — returns an"]
fn gs62_impl_two_param() {
    assert_t3_value("gs62_impl_two_param");
}

/// method returning the UNSWAPPED Pair<A,B> — so it is not about swapping
#[test]
#[ignore = "§62: method returning the UNSWAPPED Pair<A,B> — so it is"]
fn gs62_impl_noswap() {
    assert_t3_value("gs62_impl_noswap");
}

/// generic struct read back out of a Vec
#[test]
#[ignore = "§62: generic struct read back out of a Vec"]
fn gs62_vec_of_generic() {
    assert_t3_value("gs62_vec_of_generic");
}

/// generic struct into a generic free fn — TypeError, the honest failure
#[test]
#[ignore = "§62: generic struct into a generic free fn — TypeError, t"]
fn gs62_generic_freefn() {
    assert_t3_value("gs62_generic_freefn");
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

/// str through <T: Ord> — accepted, no diagnostic, comparison always false
#[test]
#[ignore = "§63: str through <T: Ord> — accepted, no diagnostic, comp"]
fn ord63_str_via_bound() {
    assert_t3_value("ord63_str_via_bound");
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

/// float through <T: Ord> — the returned VALUE is corrupted, not just the choice
#[test]
#[ignore = "§63: float through <T: Ord> — the returned VALUE is corru"]
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

/// REFUTES the reference's address explanation: swapping DECLARATION order does not flip it
#[test]
#[ignore = "§63: REFUTES the reference's address explanation: swappin"]
fn ord63_address_theory() {
    assert_t3_value("ord63_address_theory");
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
