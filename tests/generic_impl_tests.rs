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

mod common;

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
    let d = common::suite_root("oracle");
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

/// `manitc check`'s stderr for a source string, with the exit status.
///
/// P70 needs the diagnostic's LINE, not merely that one appeared: the whole
/// claim is that the message moved from the use to the declaration, and an
/// exit-status assertion is identical before and after for the ten names that
/// were already refused.
fn check_source_out(tag: &str, src: &str) -> (bool, String) {
    let p = tmp(&format!("{}.mt", tag));
    std::fs::write(&p, src).expect("write temp source");
    let o = Command::new(manitc())
        .args(["check", p.to_str().unwrap()])
        .output()
        .expect("check");
    (o.status.success(), String::from_utf8_lossy(&o.stderr).into_owned())
}

/// The 1-based line a diagnostic points at, or `None` if it named none.
fn diagnostic_line(stderr: &str) -> Option<u32> {
    stderr
        .lines()
        .find_map(|l| l.split(".mt:").nth(1))
        .and_then(|rest| rest.split(':').next())
        .and_then(|n| n.parse().ok())
}

fn assert_t3_value(name: &str) {
    let want = expected(name);
    match run_t3(name) {
        Ok(got) => assert_eq!(got, want, "{}: T3 printed the wrong value", name),
        Err(e) => panic!("{}: did not compile for T3:\n{}", name, e),
    }
}

/// **A payload variant can be CONSTRUCTED.** report.txt P43, closed 29 August
/// 2026.
///
/// The constructor emitted a call to a symbol nothing defines — `Undefined
/// label: Shape::Circle` on T3, `use of undefined value '@Shape_Circle'` on
/// LLVM — from a program `manitc check` exits 0 on. But the constructor was
/// only the site with NO implementation; the two that had one **disagreed with
/// each other**, the tag test reading the scrutinee as a bare integer and the
/// pattern binder reading the same value as a pointer to `[tag, payload]`.
#[test]
fn pe61_construct_payload() {
    assert_t3_value("pe61_construct_payload");
}

/// **The payload-enum representation, end to end.** report.txt P43.
///
/// `pe61_construct_payload` is the minimal repro; this is what the fix has to
/// get right beyond it, and **every line of it fails to COMPILE on the control
/// binary** — `Undefined label: Shape::Circle`.
///
///   * `Rect(w, h)` binds TWO fields. Both pattern binders read every field
///     from word 1, so `w * h` was `w * w` — a silent wrong answer that could
///     not be reached because nothing could construct a `Rect`. **A defect
///     behind an unreachable one is still a defect**, and it becomes reachable
///     the moment the outer one is fixed.
///   * `Dot` is a plain variant of an enum that HAS payload variants, so it is
///     a cell too. One representation per ENUM, not per variant: the tag test
///     runs before the variant is known, so a scrutinee whose shape depended on
///     its variant would be undecidable exactly where it is decided.
///   * the value crosses a function boundary, lives in a variable, and is
///     matched directly as a temporary.
#[test]
fn p43_payload_enum_end_to_end() {
    assert_t3_value("p43_payload_enum");
}

/// **P43's three error paths, none of which was checked before.**
///
/// The constructor reached the lowerer without the analyzer having looked at
/// it at all — it fell through to an empty parameter list, so arity and types
/// were unchecked. `Shape::Circle` named on its own is the dangerous one: it
/// used to build a cell nobody had written a payload into, and the `match`
/// then bound `r` to 0 on BOTH backends.
///
/// The last two rows are controls: the callee position must still ACCEPT the
/// name it rejects everywhere else, and a plain enum must be unaffected.
#[test]
fn p43_payload_variant_arity_and_types_are_checked() {
    let head = "use std::io;\nenum Shape { Circle(int), Rect(int, int), Dot }\n";
    let bad = |tag: &str, body: &str| {
        assert!(
            !check_source(tag, &format!("{}fn main() {{ {} }}\n", head, body)),
            "should be refused: {}",
            body
        );
    };
    bad("p43_named_bare", "let s = Shape::Circle; io::print_int(1);");
    bad("p43_too_many", "let s = Shape::Circle(1, 2); io::print_int(1);");
    bad("p43_wrong_type", "let s = Shape::Circle(\"x\"); io::print_int(1);");
    bad("p43_too_few", "let s = Shape::Rect(1); io::print_int(1);");
    assert!(
        check_source(
            "p43_ok_ctor",
            &format!("{}fn main() {{ let s = Shape::Circle(1); io::print_int(1); }}\n", head),
        ),
        "the callee position must still accept the name — otherwise the rule \
         that a payload variant cannot be named bare has eaten the one place \
         naming it is right"
    );
    assert!(
        check_source(
            "p43_plain_enum",
            "use std::io;\nenum D { N, S }\nfn main() { let d = D::N; io::print_int(1); }\n",
        ),
        "an enum with no payload variants is untouched"
    );
}

/// **A PATH-FORM call to a generic `impl<T>` method binds from the receiver.**
/// report.txt P73, which P69 recorded as a limit and closed 29 August 2026.
///
/// `Box2::bigger(b)` arrives at the free-function call site, where the binding
/// comes from the ARGUMENTS — and `self` is declared `Self`, which is not one
/// of the impl's generics, so nothing bound and the call kept the erased body.
/// The receiver IS the first argument, so P69's own mechanism applies; it was
/// only reached through a different syntax.
///
/// **THE NEGATIVE PAIR IS THE TEST, AND THREE OF THE FOUR LINES ARE CONTROLS.**
/// Positive doubles order the same way as their bit patterns, so `(1.5, 2.5)`
/// answers 2.5 whether the comparison is a float compare or an integer one —
/// P68's trap, one finding later. On the control binary this fixture prints
/// `2.5 / -2.5 / -1.5 / 7` and only the second line moves.
#[test]
fn p73_path_form_impl_call_is_instantiated() {
    assert_t3_value("p73_path_form_impl_call");
}

/// **A stack overflow names itself.** report.txt P76.
///
/// The stack grows DOWN from 60,000 and the code grows UP from 0, and nothing
/// stopped them meeting: `STORE` had an upper bound and no lower one, so a deep
/// enough recursion overwrote its own instructions and the emulator then
/// executed them — `TRAP: register index 43 out of range (0..=26)`, which names
/// the symptom and not the cause.
///
/// **The call-depth guard cannot catch it and that is the point: it counts
/// FRAMES.** A 45-word frame overflows at depth ~1,300 while the guard waits
/// for 10,000. P38 checked that the IMAGE fits below the stack; this is the
/// same collision from the other side.
///
/// Built rather than shipped as a fixture, because the threshold is a function
/// of the frame size and the image size and a hand-written file would pin
/// neither. The assertion is on the MESSAGE: the trap already happened before
/// this change, it just said something else.
#[test]
fn p76_a_stack_overflow_says_so() {
    let mut src = String::from("use std::io;\nfn rec(n: int) -> int {\n    if n <= 0 { return 0; }\n");
    for i in 0..60 {
        src.push_str(&format!("    let v{}: int = n + {};\n", i, i));
    }
    src.push_str("    let s: int = ");
    src.push_str(&(0..60).map(|i| format!("v{}", i)).collect::<Vec<_>>().join(" + "));
    src.push_str(";\n    return 1 + rec(n - 1) + (s - s);\n}\n");
    let run = |depth: u32| -> String {
        let p = tmp(&format!("p76_{}.mt", depth));
        std::fs::write(&p, format!("{}fn main() {{ io::print_int(rec({})); io::newline(); }}\n", src, depth))
            .expect("write");
        let out = tmp(&format!("p76_{}", depth));
        let c = Command::new(manitc())
            .args(["compile", p.to_str().unwrap(), "--target", "t3", "-o", out.to_str().unwrap()])
            .output().expect("compile");
        assert!(c.status.success(), "must compile");
        let r = Command::new(manitc())
            .args(["run-t3", out.with_extension("t3b").to_str().unwrap()])
            .output().expect("run");
        format!("{}{}", String::from_utf8_lossy(&r.stdout), String::from_utf8_lossy(&r.stderr))
    };
    // Shallow enough to fit: the value, not a trap.
    let shallow = run(200);
    assert!(shallow.contains("200"), "depth 200 must still run: {}", shallow);
    assert!(!shallow.contains("TRAP"), "depth 200 must not trap: {}", shallow);
    // Deep enough to collide: a trap that NAMES the collision.
    let deep = run(4000);
    assert!(
        deep.contains("stack overflow") && deep.contains("inside the program image"),
        "a stack/code collision must say so, not report a bogus register index: {}",
        deep
    );
}

/// **A diagnostic inside merged stdlib source names the stdlib file.**
/// report.txt P8, closed 29 August 2026.
///
/// `stdlib_expand` parses each module with its OWN line numbering and appends
/// the items to the user's program. `Span` carried a line and a column and
/// nothing else, so every diagnostic was reported under the file the compiler
/// was invoked on: a warning inside `fmt::to_radix` came out as
/// `hello.mt:230:22` for a `hello.mt` FIVE LINES LONG. Right line, wrong file,
/// on every diagnostic rather than one lint.
///
/// **AND FIXING THE NAME MADE THE SNIPPET WRONG — the mirror of the same
/// defect.** The renderer cuts its source line out of the file the compiler was
/// invoked on, indexed by the diagnostic's line number. With the name corrected
/// and a long enough user file, `stdlib/fmt.mt:232` printed the user's
/// `fn pad229()`. Both halves are asserted here, because either alone is a
/// diagnostic that lies about where it points.
#[test]
fn p8_a_stdlib_diagnostic_names_the_stdlib_file_and_shows_its_line() {
    // A user file long enough to HAVE a line 232, so a wrong snippet is
    // available to be printed. The five-line version cannot tell the two
    // fixes apart: it prints no snippet either way.
    let mut src = String::from("use std::io;\nuse std::fmt;\n");
    for i in 0..300 {
        src.push_str(&format!("fn pad{}() -> int {{ return {}; }}\n", i, i));
    }
    src.push_str("fn main() { io::println(fmt::show_hex(255)); }\n");
    let p = tmp("p8_long.mt");
    std::fs::write(&p, &src).expect("write");
    let o = Command::new(manitc())
        .args(["check", "--warn", "division-semantics", p.to_str().unwrap()])
        .output().expect("check");
    let text = format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));

    assert!(
        text.contains("stdlib/fmt.mt:232"),
        "a diagnostic inside merged stdlib source must name the stdlib file:\n{}",
        text
    );
    assert!(
        !text.contains("p8_long.mt:232"),
        "it must not be attributed to the user's file:\n{}",
        text
    );
    // The snippet must be the NAMED file's line, not the invoked file's.
    assert!(
        text.contains("v = v / base;"),
        "the snippet must come from stdlib/fmt.mt, which is what the header \
         names:\n{}",
        text
    );
    assert!(
        !text.contains("fn pad229()"),
        "the snippet must NOT be the user's line 232 under a stdlib header:\n{}",
        text
    );
    // Control: a diagnostic in the USER's own code still names the user's file
    // and shows the user's line, so this is not "everything says stdlib now".
    let up = tmp("p8_user.mt");
    std::fs::write(&up, "use std::io;\nfn main() {\n    let a: int = 7;\n    let b: int = a / 2;\n    io::print_int(b);\n}\n")
        .expect("write");
    let uo = Command::new(manitc())
        .args(["check", "--warn", "division-semantics", up.to_str().unwrap()])
        .output().expect("check");
    let utext = format!("{}{}", String::from_utf8_lossy(&uo.stdout), String::from_utf8_lossy(&uo.stderr));
    assert!(utext.contains("p8_user.mt:4"), "user diagnostics keep the user's file:\n{}", utext);
    assert!(utext.contains("let b: int = a / 2;"), "and the user's line:\n{}", utext);
}

/// **`recv` on an empty open channel: 0 on T3, DEADLOCK on LLVM.**
/// report.txt P5.1 — "the sharpest divergence recorded in this file" — closed
/// 29 August 2026.
///
/// One program, a wrong answer on one backend and SILENCE on the other: LLVM
/// blocked on a condition variable nothing could signal, with stdout unflushed,
/// so it printed nothing at all — including the line it had already produced
/// before the recv.
///
/// **This is not the design decision P5 is parked on.** Making `recv` block
/// needs a scheduler to block onto, and that choice is still open. Under the
/// contract that exists — `spawn { B }` runs B in place and to completion, which
/// `phase3_tests` pins — an OPEN empty channel has no possible sender, so the
/// receive cannot be satisfied and both backends say so. It stops being
/// reachable the day `spawn` starts a real task.
///
/// The CLOSED empty channel is deliberately untouched: that is the drain case
/// `examples/concurrency.mt` relies on, and it still yields 0 on both.
#[test]
fn p5_1_recv_on_an_open_empty_channel_faults_on_both_backends() {
    let src = "use std::io;\nfn main() {\n    io::println(\"before\");\n    \
               let ch = channel<int>();\n    let v: int = ch.recv();\n    \
               io::print_int(v);\n}\n";
    let p = tmp("p5_1_recv.mt");
    std::fs::write(&p, src).expect("write");

    let out = tmp("p5_1_recv");
    let c = Command::new(manitc())
        .args(["compile", p.to_str().unwrap(), "--target", "t3", "-o", out.to_str().unwrap()])
        .output().expect("compile");
    assert!(c.status.success(), "must still compile");
    let r = Command::new(manitc())
        .args(["run-t3", out.with_extension("t3b").to_str().unwrap()])
        .output().expect("run");
    let t3 = format!("{}{}", String::from_utf8_lossy(&r.stdout), String::from_utf8_lossy(&r.stderr));
    assert!(t3.contains("before"), "output before the recv must survive: {:?}", t3);
    assert!(
        t3.contains("recv on an empty channel that is still open"),
        "T3 must fault rather than answer 0: {:?}",
        t3
    );
    assert!(!t3.contains("got 0"), "T3 must not silently answer 0: {:?}", t3);
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
////// The test the defect deserved, rather than the one that found it. A program
/// is compiled once per struct NAME, identical otherwise, and the answers are
/// compared against a control name on no list.
///
/// TWO SETS, AND THE DISTINCTION IS THE FINDING. `resolve_type` carries a
/// hardcoded list of generic constructors, and a name on it shadowed a user's
/// own `struct <name><T>`: the ANNOTATION resolved to `Generic(name, [..])`
/// while the LITERAL resolved to `Struct(name)`, and those do not unify.
///
///   * `Pair` was on that list with NOTHING BEHIND IT — no stdlib source, no
///     IR, no backend — so it shadowed a user struct for nothing. Removed
///     (report.txt P67), and it is asserted here to work.
///   * The other nine have real implementations, so shadowing them is a
///     genuine collision rather than a phantom, and "the built-in wins" is a
///     defensible rule. What was NOT defensible was that the collision was
///     silent, and that is now P70's declaration-site diagnostic: they are
///     asserted here to be refused, and `p70_*` below asserts WHERE.
///
/// THIS TEST WAS ITSELF AN INSTANCE OF THE RULE IT RECORDS, WHICH IS P70. Its
/// original `FREE` list held `String`, on the strength of the program below
/// compiling — and every program below is GENERIC. `struct String<T>` is
/// genuinely free; `struct String` is shadowed by `str`, and no member of this
/// family could see that, because `<T>` is what all thirteen of them hold
/// fixed. P67's own rule — when a family of probes agrees, ask what every
/// member HOLDS FIXED — applied one level up, to the test written to record
/// it. `p70_*` therefore probes both spellings of every name.
#[test]
fn p67_a_struct_name_must_not_change_the_program() {
    // Names with no built-in behind them: a user struct must work.
    // `String` is NOT here — see the note above; it is reserved in the plain
    // spelling and this program is the generic one.
    const FREE: &[&str] = &["Duo", "Pair", "Task"];
    // Names whose built-in has an implementation, which wins — now with a
    // diagnostic that says so.
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

/// P70 — the reserved set is DERIVED from the compiler's own table, in both
/// spellings, and the diagnostic must point at the DECLARATION.
///
/// P60's remedy applied to a third registry. The list is not copied here: the
/// test iterates `SemanticAnalyzer::RESERVED_TYPE_NAMES` itself, so a name
/// added to the table without a diagnostic, or a diagnostic without a table
/// entry, fails — the two cannot drift the way `STDLIB_MODULES` and
/// `SOURCE_MODULES` did.
///
/// THE ASSERTION IS THE LINE, NOT THE EXIT STATUS. Ten of these fifteen names
/// were ALREADY refused before P70, so a status assertion is green either way
/// and says nothing about the change. The claim is that the message moved from
/// the use to the declaration, and only the line number can carry it.
#[test]
fn p70_a_reserved_name_is_refused_at_the_declaration_in_both_spellings() {
    use manitc::semantic::analyzer::SemanticAnalyzer;

    // Line 1 is the declaration; line 3 is the use. Kept to three lines so the
    // number means something.
    let generic = |n: &str| {
        format!(
            "struct {n}<T> {{ pub first: T, pub second: T }}\n\
             fn swap<T>(p: {n}<T>) -> {n}<T> {{ {n} {{ first: p.second, second: p.first }} }}\n\
             fn main() {{ let p = {n} {{ first: 1, second: 2 }}; io::println_int(swap(p).first); }}\n"
        )
    };
    let plain = |n: &str| {
        format!(
            "struct {n} {{ pub first: int, pub second: int }}\n\
             fn swap(p: {n}) -> {n} {{ {n} {{ first: p.second, second: p.first }} }}\n\
             fn main() {{ let p = {n} {{ first: 1, second: 2 }}; io::println_int(swap(p).first); }}\n"
        )
    };

    let mut wrong = Vec::new();
    for (name, _, _) in SemanticAnalyzer::RESERVED_TYPE_NAMES {
        for (form, src) in [("g", generic(name)), ("p", plain(name))] {
            let (ok, err) = check_source_out(&format!("p70_{form}_{name}"), &src);
            if ok {
                wrong.push(format!("{name} ({form}): accepted"));
            } else if diagnostic_line(&err) != Some(1) {
                wrong.push(format!(
                    "{name} ({form}): refused at line {:?}, not the declaration",
                    diagnostic_line(&err)
                ));
            }
        }
    }
    assert!(
        wrong.is_empty(),
        "a reserved type name must be refused where it is DECLARED (line 1), \
         in both the generic and the plain spelling — report.txt P70. \
         Offending: {:#?}",
        wrong,
    );

    // The other direction, and it is the half that keeps the table honest.
    //
    // `AtomicTrit`, `Barrier`, `Semaphore` and `MutexGuard` are answered early
    // by `name_to_manitype` too — but they answer `Struct(name, [])`, which is
    // exactly what the struct table would have given, so nothing shadows and
    // they are deliberately ABSENT from the table. If someone adds them for
    // symmetry, this fails and says why.
    const FREE: &[&str] = &[
        "Pair", "Duo", "Task", "Widget",
        "AtomicTrit", "Barrier", "Semaphore", "MutexGuard", "RwLock", "Future",
    ];
    let mut refused = Vec::new();
    for n in FREE {
        for (form, src) in [("g", generic(n)), ("p", plain(n))] {
            if !check_source(&format!("p70_free_{form}_{n}"), &src) {
                refused.push(format!("{n} ({form})"));
            }
        }
    }
    assert!(
        refused.is_empty(),
        "these names are refused but are not in RESERVED_TYPE_NAMES, so the \
         diagnostic is firing on something the table does not claim, or a \
         resolver arm has grown without one — report.txt P70. Refused: {:?}",
        refused,
    );
}

/// P70 — `struct Self` was the only member of the set that did not end in a
/// refusal, and it is the reason this is a defect rather than a diagnostic
/// improvement.
///
/// `name_to_manitype("Self")` answers from `current_impl_type`, which is `None`
/// at top level, so the annotation resolved to `Unknown` — compatible with
/// everything. The program therefore TYPE-CHECKED, `manitc check` exited 0,
/// and `p.second` read slot 0: measured, `1` on BOTH backends where it should
/// print `2`. A debug compiler panics P44's assertion instead, which is how it
/// surfaced.
///
/// Both halves are asserted: the refusal, and — since a refusal is cheap to
/// get wrong — that it is the DECLARATION that is named.
#[test]
fn p70_struct_self_printed_the_wrong_field_and_is_now_refused() {
    let src = "struct Self { pub first: int, pub second: int }\n\
               fn swap(p: Self) -> int { p.second }\n\
               fn main() { let p = Self { first: 1, second: 2 }; io::println_int(swap(p)); }\n";
    let (ok, err) = check_source_out("p70_self", src);
    assert!(
        !ok,
        "`struct Self` type-checks. It did before P70, and then printed 1 \
         instead of 2 on both backends — report.txt P70."
    );
    assert_eq!(
        diagnostic_line(&err),
        Some(1),
        "`struct Self` is refused, but not at its declaration:\n{}",
        err
    );
    assert!(
        err.contains("reserved type name"),
        "`struct Self` is refused for some OTHER reason, which would make this \
         row pass without covering P70:\n{}",
        err
    );
}

/// P70 — `lint allow(reserved-type-name);` is an exact restoration, not a
/// softening.
///
/// The escape hatch matters because the two stdlib modules that declare `Vec`,
/// `Map`, `Mutex` and the rest ARE the built-ins, and they use it. What `allow`
/// must give back is the PREVIOUS compiler: the plain spelling compiled then
/// and must compile now, and the generic spelling failed then — at the use —
/// and must still fail there. Asserting only the first half would pass for a
/// version that silently accepted the broken program too.
#[test]
fn p70_lint_allow_restores_the_previous_behaviour_exactly() {
    let plain = "lint allow(reserved-type-name);\n\
                 struct Vec { pub first: int, pub second: int }\n\
                 fn swap(p: Vec) -> Vec { Vec { first: p.second, second: p.first } }\n\
                 fn main() { let p = Vec { first: 1, second: 2 }; io::println_int(swap(p).first); }\n";
    assert!(
        check_source("p70_allow_plain", plain),
        "`lint allow(reserved-type-name);` must restore the previous compiler, \
         in which `struct Vec` (plain) compiled — report.txt P70."
    );

    let generic = "lint allow(reserved-type-name);\n\
                   struct Vec<T> { pub first: T, pub second: T }\n\
                   fn swap<T>(p: Vec<T>) -> Vec<T> { Vec { first: p.second, second: p.first } }\n\
                   fn main() { let p = Vec { first: 1, second: 2 }; io::println_int(swap(p).first); }\n";
    let (ok, err) = check_source_out("p70_allow_generic", generic);
    assert!(
        !ok,
        "`allow` restored more than the previous behaviour: the generic \
         spelling was refused before P70 and must still be. `allow` silences \
         the declaration diagnostic; it does not make the shadowing go away."
    );
    assert_eq!(
        diagnostic_line(&err),
        Some(4),
        "under `allow` the generic spelling must fail where it always did — at \
         the USE, line 4 — not at the declaration:\n{}",
        err
    );
}

/// P70 — the stdlib's own `lint allow(reserved-type-name);` must not reach a
/// program that uses the module.
///
/// P62's shape, and the reason it is pinned rather than reasoned about:
/// `collections` and `sync` are not in `SOURCE_MODULES`, so they are never
/// merged into a host program and their lint item cannot travel. That is an
/// argument from a registry, and a registry can change — `tritfs` was moved
/// INTO `SOURCE_MODULES` by P60 for exactly the reasons that would apply to
/// these two. If either is ever expanded, a user's `struct Vec<T>` would
/// silently stop being reported and this row is what says so.
///
/// A COMPOSITION FAILURE HAS NO PAIR TO COMPARE — both halves are correct on
/// their own — so it has to be asserted as behaviour.
#[test]
fn p70_the_stdlib_lint_allow_does_not_travel_to_a_user_program() {
    let src = "use std::collections;\n\
               struct Vec<T> { pub first: T, pub second: T }\n\
               fn main() { let v: Vec<int> = Vec { first: 1, second: 2 }; io::println_int(v.first); }\n";
    let (ok, err) = check_source_out("p70_leak", src);
    assert!(
        !ok,
        "a program that declares `struct Vec<T>` is accepted when it also says \
         `use std::collections;` — the module's own `lint allow` has leaked \
         into the host program. report.txt P70."
    );
    assert!(
        err.contains("reserved type name"),
        "refused, but not by the reserved-name diagnostic:\n{}",
        err
    );

    // The control: using the module normally is untouched. Without this the
    // row above passes for a version that broke `use std::collections;`
    // outright.
    let control = "use std::sync;\n\
                   use std::collections;\n\
                   fn main() { let v: Vec<int> = Vec::new(); io::println_int(Vec::len(v)); }\n";
    assert!(
        check_source("p70_leak_control", control),
        "an ordinary user of std::collections and std::sync no longer compiles"
    );
}

/// P70 — the reference's two tables must agree with the compiler's.
///
/// `docs/language-reference.md` §14 lists the reserved type names and §20
/// lists the lints with their defaults. Both are registries describing another
/// registry, which is P60's shape and the reason it is checked rather than
/// proof-read: §14 carried a paragraph saying reserved names "say so badly"
/// for as long as that was true and would have carried it afterwards, and §20's
/// table was silently missing three lints when this was written —
/// `literal-out-of-word`, `backend-unavailable-chain` and `reserved-type-name`
/// itself.
///
/// A DOCUMENTATION DEFECT IN THIS CODEBASE HAS NEVER BEEN A FALSE SENTENCE
/// (P51, P55, §14's refuted address explanation, and now this): it is an
/// absence, a stale word, or a mechanism that was true when written. Prose
/// review does not find those. An assertion does.
#[test]
fn p70_the_reference_tables_agree_with_the_compiler() {
    use manitc::semantic::analyzer::SemanticAnalyzer;

    let doc = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs/language-reference.md"),
    )
    .expect("language reference");

    let s14 = {
        let i = doc.find("### Reserved type names").expect("§14 reserved-names section");
        let j = doc[i..].find("## 15.").map(|k| i + k).unwrap_or(doc.len());
        &doc[i..j]
    };
    let undocumented: Vec<&str> = SemanticAnalyzer::RESERVED_TYPE_NAMES
        .iter()
        .map(|(n, _, _)| *n)
        .filter(|n| !s14.contains(&format!("`{n}`")))
        .collect();
    assert!(
        undocumented.is_empty(),
        "these names are reserved by the compiler and absent from \
         language-reference.md §14's table, so a reader has no way to learn \
         they are taken: {:?}",
        undocumented,
    );

    let s20 = {
        let i = doc.find("### The lints").expect("§20 lint table");
        let j = doc[i..]
            .find("`--warn-as-error` still means")
            .map(|k| i + k)
            .unwrap_or(doc.len());
        &doc[i..j]
    };
    let mut wrong = Vec::new();
    for (kind, name, level) in manitc::lint::LINTS {
        let _ = kind;
        let row = s20
            .lines()
            .find(|l| l.contains(&format!("`{name}`")) && l.starts_with('|'));
        match row {
            None => wrong.push(format!("{name}: absent from the table")),
            Some(l) => {
                let want = level.as_str();
                if !l.split('|').nth(2).is_some_and(|c| c.trim() == want) {
                    wrong.push(format!("{name}: default is `{want}`, table says `{}`",
                        l.split('|').nth(2).unwrap_or("?").trim()));
                }
            }
        }
    }
    assert!(
        wrong.is_empty(),
        "language-reference.md §20's lint table disagrees with `lint::LINTS`: \
         {:#?}",
        wrong,
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
/// P82: both backends' stdout as RAW BYTES.
///
/// `both` decodes with `from_utf8_lossy`, which is right for every test that
/// compares text and BLIND to the one property P82 is about: a maniT string is
/// a byte string, and a harness that decodes before comparing cannot tell a
/// byte the program emitted from the U+FFFD its own decoder produced. The first
/// version of the reverse assertion failed against `both` while `od -c` showed
/// the program's bytes were already correct — the test was measuring the
/// harness. Same blindness the emulator had, one layer out.
///
/// The `[T3ISA]` banner is stripped by line, on bytes, for the same reason
/// `behav.sh` strips it: it names a per-run output path.
fn both_bytes(name: &str) -> (Vec<u8>, Vec<u8>) {
    let out = tmp(&format!("{}_bytes", name));
    let c = Command::new(manitc())
        .args(["compile", fixture(name).to_str().unwrap(), "--target", "t3",
               "-o", out.to_str().unwrap()])
        .output().expect("compile t3");
    assert!(c.status.success(), "{}: T3 compile failed", name);
    let r = Command::new(manitc())
        .args(["run-t3", out.with_extension("t3b").to_str().unwrap()])
        .output().expect("run t3");
    let mut t3: Vec<u8> = Vec::new();
    for line in r.stdout.split(|&b| b == b'\n') {
        if line.starts_with(b"[T3ISA]") || line.is_empty() {
            continue;
        }
        t3.extend_from_slice(line);
        t3.push(b'\n');
    }
    let binp = out.with_extension("bin");
    let c2 = Command::new(manitc())
        .args(["compile", fixture(name).to_str().unwrap(), "--target", "llvm",
               "-o", binp.to_str().unwrap()])
        .output().expect("compile llvm");
    assert!(c2.status.success(), "{}: LLVM compile failed", name);
    let r2 = Command::new(&binp).output().expect("run llvm");
    (t3, r2.stdout)
}

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
/// **The assertion was on `done`, not on the reversed string — and P82 removes
/// the reason for that.** P50's note read: "the mangled string is the symptom
/// of §64's byte/scalar confusion, which is still open". It is closed. The
/// emulator holds `Vec<u8>`, so a slice landing inside a character keeps the
/// raw bytes instead of yielding U+FFFD, and the two backends now agree BYTE
/// FOR BYTE on a program that reverses a multi-byte string.
///
/// `str::reverse` is the sharpest case because it is ManiT source walking a
/// string one index at a time: on `"aéb"` it slices INSIDE the `é` twice.
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
    // P82: the STRING, byte for byte, through a byte-level harness — `both`
    // decodes lossily and cannot see this.
    let (t3b, llb) = both_bytes("s64_reverse_kills_t3");
    assert_eq!(
        t3b, llb,
        "the two backends must agree byte for byte on a reversed multi-byte \
         string; T3 used to yield U+FFFD where LLVM kept the raw bytes.\n\
         T3:   {:?}\nLLVM: {:?}",
        t3b, llb
    );
    assert!(
        !t3b.windows(3).any(|w| w == [0xEF, 0xBF, 0xBD]),
        "T3 must not substitute U+FFFD (EF BF BD) for a byte it cannot \
         decode: {:?}",
        t3b
    );
}

/// **`str::len` counts BYTES, and `str::char_count` counts characters.**
/// Closed by P48 (29 August 2026).
///
/// The `len`/`byte_len` synonymy is the DECISION — the whole `str` surface is
/// byte-indexed, `slice` included (P50) — and what was actually missing was any
/// way to ask the other question. This row used to be `#[ignore]`d expecting
/// `len` to be 3, which contradicted `s64_char_as_int_sign`'s expectation that
/// `char_at("é", 0)` is 195: a codepoint `len` sharing its index with a byte
/// `char_at` cannot be looped over. Two ignored rows, each recording half of a
/// design nobody had settled.
///
/// Asserted on both backends, because `char_count` is a new native and a new
/// native is exactly where the two implementations can disagree.
#[test]
fn s64_len_is_bytes_and_char_count_is_characters() {
    let (t3, ll) = both("s64_len_equals_bytelen");
    assert_eq!(t3, expected("s64_len_equals_bytelen"), "len/byte_len are bytes; char_count is characters");
    assert_eq!(t3, ll, "the two backends must agree about both units");
}

/// **`char as int` was UNSIGNED on T3 and SIGNED on LLVM for any byte >= 128.**
/// Closed by P48 (29 August 2026).
///
/// Asserted on AGREEMENT **and** on the value, and the pair is the point.
/// Agreement alone is satisfiable by making both backends wrong together —
/// P44's lesson about the parity matrix — and a value assertion alone was
/// green on T3 throughout, because `.expected` holds T3's answer. ASCII agrees
/// on both and always did, which is why nothing caught this; see the control
/// below.
#[test]
fn s64_char_as_int_agrees_across_backends() {
    let (t3, ll) = both("s64_char_as_int_sign");
    assert_eq!(t3, ll, "backends disagree on `char as int` for a byte >= 128");
    assert_eq!(
        t3,
        expected("s64_char_as_int_sign"),
        "a char is an UNSIGNED byte: 0xC3 is 195, not -61"
    );
}

/// The same three calls on ASCII — control, so this is not `str` being broken
/// generally, and it is why five weeks of corpora never saw §64.
#[test]
fn s64_ascii_control() {
    let (t3, ll) = both("s64_ascii_control");
    assert_eq!(t3, expected("s64_ascii_control"), "ASCII must be correct");
    assert_eq!(t3, ll, "ASCII must agree across backends");
}

/// **A `char` is an UNSIGNED BYTE, and every operation on one must say so.**
/// report.txt P48, closed 29 August 2026.
///
/// P48 recorded exactly one divergence — `char as int`. This fixture is one
/// line per family, and on the pinned control `manitc-p71` **six of its seven
/// lines answer differently on the two backends**; all seven agree after the
/// fix. That is how the count was established rather than asserted.
///
/// **THE SEVENTH LINE IS THE ONE PARITY COULD NOT SEE, AND IT IS WHY THIS ROW
/// ASSERTS A VALUE.** `trit(-1) as char` was **-1 on BOTH backends** — they
/// agreed, on a value outside the type's own range, so a cross-backend check
/// reports it as fine. It is 0 now, by the clamp rule. P44/P58: a shared
/// lowering shares its bugs, and agreement between two implementations is weak
/// evidence about a design they both got from the same place.
///
/// The four P48 did not record:
///   * ORDERING. `c > 'a'` was 1 on T3 and 0 on LLVM, so every `str::`
///     function that compares characters answered differently on non-ASCII.
///   * `int as char` did not narrow AT ALL on T3 — `300 as char` stayed 300 —
///     while LLVM truncated to 44 and `255 as char as int` came back -1.
///   * `float as char` was not a conversion on T3: it handed back the raw
///     IEEE-754 bit pattern.
///   * the value crossing a call boundary or an array slot.
///
/// Line 6 and 7 are every OTHER cast that touches a char, and they are here
/// because giving `char` its own IR type silently removed it from five
/// or-patterns that listed `I8` — `is_scalar` (so no char local was promoted),
/// `is_int` (so no char/float coercion), T3's int→trit clamp, T3's int→char
/// clamp, and the byte clamp's treatment of `bool`. **The compiler reported
/// none of them**, because every pattern stayed valid without the variant
/// (report.txt P68's shape). `'Q' as trit` came out 81; `true as char` came out
/// 0. `'Q' as float` is a different case again: it was ALREADY wrong on T3
/// before any of this, and only probing the whole family found it.
///
/// Asserted on the VALUE and on AGREEMENT together. Agreement alone is
/// satisfiable by making both backends wrong at once (P44); the value alone
/// was green on T3 for three of the five lines throughout.
#[test]
fn p48_char_is_an_unsigned_byte_on_both_backends() {
    let (t3, ll) = both("p48_char_is_an_unsigned_byte");
    assert_eq!(
        t3,
        expected("p48_char_is_an_unsigned_byte"),
        "T3: a char is an unsigned byte 0..=255, and `as char` clamps"
    );
    assert_eq!(t3, ll, "the backends must agree about every char operation");
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

/// **The array/tuple asymmetry — RESOLVED 2 September 2026 by B7's D-3, and
/// this row is rewritten IN PLACE rather than replaced.**
///
/// It used to pin the opposite behaviour and read: *"an array literal moving
/// its elements would be a CHANGE — correct, probably, and it must update
/// language-reference.md §22's table and its warning note at the same time"*.
/// That instruction is what this rewrite is; the handoff worked, and it is
/// kept here because a row that says what to do when it goes red is the
/// cheapest handoff there is (P104).
///
/// **The resolution is not the one §22's old note anticipated.** It asked
/// which of the tuple row and the array row was wrong. Measured, an array
/// literal is TWO constructs wearing one syntax, and both rows are right for
/// their own construct.
#[test]
fn p51_an_array_literal_moves_its_elements_only_as_a_container() {
    // CONTAINER — bound to a name, so it outlives the expression and holds a
    // second name for `s`. Consumes, exactly as the tuple and struct rows do.
    assert!(
        !checks("array_container", "    let s: str = \"ab\"; let a: [str; 2] = [s, s]; io::println(s); io::println(a[0]);"),
        "§22 says an array literal BOUND TO A NAME moves its elements, like \
         a tuple and a struct literal"
    );
    // VARARGS — the argument list of a call, which does not consume (the row
    // above it in the same table). 1,120 of 1,120 array-literal sites in the
    // standard library are this, so the other reading would refuse
    // `fmt::format` itself.
    assert!(
        checks("array_vararg", "    let s: str = \"ab\"; io::println(fmt::format(\"{} {}\", [s, s])); io::println(s);"),
        "§22 says an array literal in ARGUMENT position is a varargs list and \
         does not consume — a call never consumes its argument"
    );
    // The rule fires only on a plain variable, which is what keeps its blast
    // radius at zero over 2,873 measured files.
    assert!(
        checks("array_literals", "    let a: [str; 2] = [\"To:\", \"Sub:\"]; io::println(a[0]);"),
        "an array of literals has no move site at all"
    );
}

/// As `checks`, with a `move`-annotated function in scope.
///
/// **Separate from `checks` on purpose.** Putting `fn eat(x: move str)` in the
/// shared preamble made every row that uses `checks` fail on the pre-D-2
/// compiler — including two §22 rows that have nothing to do with D-2 — so the
/// control was red for an environment reason rather than because the rows
/// discriminate. A control that fails for the wrong reason is worth as little
/// as one that passes for the wrong reason.
fn checks_d2(name: &str, body: &str) -> bool {
    let src = tmp(&format!("d2_{}.mt", name));
    std::fs::write(
        &src,
        format!(
            "use std::io;\n\
             fn take(x: str) -> int {{ return str::len(x); }}\n\
             fn eat(x: move str) -> int {{ return str::len(x); }}\n\
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

/// **B7's D-2 — `fn consume(x: move str)`.**
///
/// The sweep is why this is a per-parameter annotation and not a change to what
/// all calls do: making every call argument consume refuses **24.7 % of 1,545
/// corpus programs, 36.4 % of distinct repository programs and fifty
/// standard-library functions**, because ManiT has no reference types and a
/// call is therefore the only way to read a value twice. `str::take` calls
/// `len(s)` and then `slice(s, …)`; so does most correct code.
///
/// Annotating the few sites that genuinely consume has a blast radius of
/// **zero by construction** — the map of consuming positions is empty for a
/// program that writes no `move`.
#[test]
fn d2_a_move_parameter_consumes_its_argument() {
    assert!(
        !checks_d2("moved", "    let s: str = \"ab\"; io::print_int(eat(s)); io::println(s);"),
        "a `move` parameter must consume: using `s` afterwards is a use of a \
         moved value"
    );
    assert!(
        checks_d2("once", "    let s: str = \"ab\"; io::print_int(eat(s));"),
        "consuming and not using it again is the whole point — this must be \
         accepted"
    );
    assert!(
        !checks_d2("twice", "    let s: str = \"ab\"; io::print_int(eat(s)); io::print_int(eat(s));"),
        "the second call is a use of a moved value"
    );
}

#[test]
fn d2_a_plain_parameter_still_does_not_consume() {
    // Passes on the pre-D-2 compiler too — it pins the BOUNDARY the change is
    // drawn along, which is §22's call row, unmoved — which is the half that makes D-2 safe to add.
    assert!(
        checks("d2_plain", "    let s: str = \"ab\"; take(s); take(s); io::println(s);"),
        "an UNannotated parameter must borrow exactly as it always has"
    );
}

/// **`move` is a CONTEXTUAL keyword, and `stdlib/fs.mt` is why.**
///
/// That module declares `fn move(src: str, dst: str) -> int;`, so reserving the
/// word would delete a shipped standard-library function — P104's lesson,
/// which cost a lint a name it could not spell. The annotation is told from a
/// type by requiring something to follow it: in `x: move` the word is the
/// TYPE, in `x: move str` it is the annotation.
#[test]
fn d2_move_is_contextual_and_still_a_usable_name() {
    // Passes on the pre-D-2 compiler too, and that is the point: it records
    // that adding the annotation took nothing away (permanent rule 9's honest
    // half).
    assert!(
        checks("d2_fs_move", "    io::print_int(fs::move(\"/tmp/a\", \"/tmp/b\"));"),
        "`fs::move` is a shipped stdlib function and must keep compiling"
    );
}

/// A call through a function POINTER consumes nothing, because there is no
/// name to look the signature up under. Stated rather than left implicit.
///
/// **This row is RED on the pre-D-2 compiler for a weaker reason than the
/// others**, and saying so is the honest half of permanent rule 9: its
/// preamble declares `fn eat(x: move str)`, which that compiler cannot parse,
/// so the whole program fails to check. The redness shows the syntax is new,
/// not that the row discriminates the RULE. What it really pins is a LIMIT.
#[test]
fn d2_an_indirect_call_consumes_nothing() {
    assert!(
        checks_d2("indirect",
               "    let s: str = \"ab\"; let f: fn(str) -> int = eat; \
                io::print_int(f(s)); io::println(s);"),
        "an indirect call has no signature in hand, so it cannot consume — \
         this is a LIMIT and the row records it"
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

// ---------------------------------------------------------------------------
// P71 — a DISCARDED instantiation still has a declared return type
//
// P65's design is that an instantiation whose BODY does not check is discarded
// and the call keeps the erased path, which is what makes monomorphisation
// unable to break a program. The defect is that the same verdict also gated the
// RETURN TYPE, which is a function of the DECLARATION and not of the body: the
// reader wrote `-> T` and it means `P` whether or not the body compiles at
// `T = P`. Left `<unknown>`, a field read on the result takes slot 0.
//
// Both call sites carried it — free function and `impl<T>` method — and P69's
// §6 recorded only the second.
// ---------------------------------------------------------------------------

/// P71: free-function half. `1 1` on the pre-P71 release compiler, an assertion
/// panic on the pre-P71 debug compiler, `1 2` here.
#[test]
fn p71_failed_instantiation_still_types_the_return_freefn() {
    assert_t3_value("p71_failed_inst_freefn");
}

/// P71: `impl<T>` method half, reached through the receiver rather than the
/// arguments. Same value, a separate gate in the compiler.
#[test]
fn p71_failed_instantiation_still_types_the_return_impl_method() {
    assert_t3_value("p71_failed_inst_impl_method");
}

/// **P71 IS A STRICTNESS CHANGE, AND THIS IS THE PROGRAM IT NEWLY REFUSES.**
///
/// `<unknown>` is compatible with everything, so binding a discarded
/// instantiation's result to a mismatched annotation used to be accepted — and
/// the program it accepted was assigning a struct address to an `int`. Pinned
/// in both directions: the mismatched form must be refused, the matching form
/// must still be accepted, so the row cannot go green by refusing everything.
#[test]
fn p71_a_failed_instantiations_result_is_no_longer_compatible_with_everything() {
    let head = "use std::io;\n\
                struct P { pub x: int }\n\
                fn pick<T>(a: T, b: T) -> T { if a > b { a } else { a } }\n";
    assert!(
        !check_source(
            "p71_strict_bad",
            &format!("{}fn main() {{ let n: int = pick(P {{ x: 1 }}, P {{ x: 2 }}); io::print_int(n); }}\n", head),
        ),
        "binding a `P`-returning call to an `int` must be refused now that the \
         return type is substituted"
    );
    assert!(
        check_source(
            "p71_strict_good",
            &format!("{}fn main() {{ let n: P = pick(P {{ x: 1 }}, P {{ x: 2 }}); io::print_int(n.x); }}\n", head),
        ),
        "the correctly-annotated form must still be accepted — otherwise this \
         row is green because nothing compiles"
    );
}

/// **P71's LIMIT, recorded so it is not mistaken for fixed.**
///
/// Typing the return correctly is SUFFICIENT for a struct, whose erased
/// representation — an address — is already its real one. It is only NECESSARY
/// for a float: the discarded body still computed with integer semantics, so
/// the caller reinterprets those bits and P65's denormal comes back. Measured
/// byte-identical before and after P71, which is the argument that substituting
/// the type onto an erased body is neutral rather than harmful.
///
/// `a | 1` is the body because it checks under the erasure and not at `float`;
/// most float bodies instantiate fine, and one that does is no test of this.
#[test]
fn p71_a_failed_float_instantiation_still_returns_the_bit_pattern() {
    let src = "use std::io;\n\
               fn g<T>(a: T) -> T { let q = a | 1; a }\n\
               fn main() { io::print_float(g(1.5)); io::newline(); }\n";
    let p = tmp("p71_float_limit.mt");
    std::fs::write(&p, src).expect("write");
    let out = tmp("p71_float_limit");
    let c = Command::new(manitc())
        .args(["compile", p.to_str().unwrap(), "--target", "t3", "-o", out.to_str().unwrap()])
        .output().expect("compile");
    assert!(c.status.success(), "the erased path must still compile");
    // The instantiation is DISCARDED — that is the precondition, and asserting
    // it is what stops this row quietly becoming a test of a working case.
    let ll = tmp("p71_float_limit_ll");
    let _ = Command::new(manitc())
        .args(["compile", p.to_str().unwrap(), "--target", "llvm", "-o", ll.to_str().unwrap()])
        .output().expect("compile llvm");
    if let Ok(text) = std::fs::read_to_string(ll.with_extension("ll")) {
        assert!(
            !text.contains("@g$float"),
            "precondition: this body must FAIL to instantiate at `float`; if it \
             now succeeds, the row is testing nothing and needs a new body"
        );
    }
    let r = Command::new(manitc())
        .args(["run-t3", out.with_extension("t3b").to_str().unwrap()])
        .output().expect("run");
    let got: String = String::from_utf8_lossy(&r.stdout)
        .lines().filter(|l| !l.starts_with("[T3ISA]")).collect::<Vec<_>>().join("");
    assert!(
        got.starts_with("0.0000") && got.ends_with("5"),
        "P65's denormal is the documented remaining behaviour here; got {:?}",
        got
    );
}
