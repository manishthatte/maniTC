//! C6 — trit-pattern matching with wildcards and captures.
//!
//! © Manish Jagdish Thatte
//!
//! `enhance/phase5-type-system-second-half/README.md` item C6, specified in
//! `docs/language-reference.md` §13. A trit pattern matches the individual
//! trits of a balanced-ternary word: `+`/`0`/`-` fix a trit, `?` is one
//! wildcard trit, a leading `*` leaves the high trits free, and `@name` binds
//! the run before it.
//!
//! Three properties carry the design and each has rows here rather than a
//! comment, because all three are the sort of thing prose review does not
//! catch (permanent rule 6):
//!
//! 1. **Zero extension.** The trits above the pattern must be zero unless it
//!    opens with `*`, which is what makes a wildcard-free trit pattern mean
//!    exactly the literal it spells. It needs no sign-extension rule because
//!    balanced ternary has none to need — `-1` is `-` with zeros above it.
//! 2. **`*` is leftmost-only, and that is portability.** A `*` in the middle
//!    could only be placed by knowing the word's trit width, which
//!    `docs/semantics.md` §10.1 records as differing between the backends
//!    under v1.
//! 3. **A radix-3 decision compiles to a radix-3 branch.** That is C6's whole
//!    point, so it is asserted on the emitted assembly and not merely on the
//!    answer — a row that checks only the value would pass over a lowering
//!    that threw the payoff away (rule 8's shape: a test inherits the question
//!    its bug asked).
//!
//! Every expected value below was derived by hand from the balanced-ternary
//! representation before it was run.

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
    let d = common::suite_root("c6")
        .join(slot.to_string());
    std::fs::create_dir_all(&d).expect("temp dir");
    d
}

fn run_t3(src: &str) -> String {
    let d = workdir();
    let path = d.join("p.mt");
    std::fs::write(&path, src).expect("write");
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
    let out: String = String::from_utf8_lossy(&r.stdout)
        .lines().filter(|l| !l.starts_with("[T3ISA]"))
        .map(|l| format!("{l}\n")).collect();
    out + &String::from_utf8_lossy(&r.stderr)
        .lines().filter(|l| l.starts_with("TRAP"))
        .map(|l| format!("{l}\n")).collect::<String>()
}

fn compile_llvm(src: &str) -> Option<PathBuf> {
    let d = workdir();
    let path = d.join("p.mt");
    std::fs::write(&path, src).expect("write");
    let bin = d.join("p.bin");
    Command::new(manitc_bin())
        .args(["compile", path.to_str().unwrap(), "--target", "llvm",
               "-o", bin.to_str().unwrap()])
        .output().expect("compile");
    bin.exists().then_some(bin)
}

/// `None` only when clang is absent.
///
/// P47's defect is that a skip condition overlapping the failure condition is
/// a silent pass, and it has bitten this repository four times. Every program
/// here compiles to LLVM if the toolchain exists at all, so a missing binary
/// IS the environment answer and there is no second state to confuse it with —
/// but the caller still asserts the T3 answer unconditionally, so a row can
/// never assert nothing.
fn run_llvm(src: &str) -> Option<String> {
    let bin = compile_llvm(src)?;
    let r = Command::new(&bin).output().expect("run");
    Some(String::from_utf8_lossy(&r.stdout).into_owned()
         + &String::from_utf8_lossy(&r.stderr))
}

/// Assert both backends produce `want`.
///
/// Against the SAME hand-derived string, never against each other: the parity
/// matrix cannot see a mistake made upstream of the split, and this lowering
/// is entirely upstream of it (P44/P58).
fn both(src: &str, want: &str, what: &str) {
    assert_eq!(run_t3(src), want, "{what}: T3");
    if let Some(ll) = run_llvm(src) {
        assert_eq!(ll, want, "{what}: LLVM");
    }
}

/// `manitc check` on a source: (accepted, combined output).
fn check(src: &str) -> (bool, String) {
    let d = workdir();
    let path = d.join("p.mt");
    std::fs::write(&path, src).expect("write");
    let o = Command::new(manitc_bin())
        .args(["check", path.to_str().unwrap()])
        .output().expect("check");
    (o.status.success(),
     String::from_utf8_lossy(&o.stdout).into_owned()
     + &String::from_utf8_lossy(&o.stderr))
}

/// Executed instruction count, from the emulator's own profile.
///
/// P31: the emulator has always collected this and `--profile` prints it;
/// bisecting `--max-steps` to find the same number is ~40 runs to read out a
/// value the emulator was already holding.
fn t3_dynamic(src: &str) -> u64 {
    let d = workdir();
    let path = d.join("p.mt");
    std::fs::write(&path, src).expect("write");
    let base = path.with_extension("");
    let c = Command::new(manitc_bin())
        .args(["compile", path.to_str().unwrap(), "--target", "t3",
               "-o", base.to_str().unwrap()])
        .output().expect("compile");
    assert!(c.status.success(), "T3 compile failed:\n{}",
            String::from_utf8_lossy(&c.stderr));
    let r = Command::new(manitc_bin())
        .args(["run-t3", base.with_extension("t3b").to_str().unwrap(), "--profile"])
        .output().expect("run");
    let text = String::from_utf8_lossy(&r.stdout).into_owned()
        + &String::from_utf8_lossy(&r.stderr);
    text.lines()
        .find(|l| l.contains("total-instructions"))
        .and_then(|l| l.split_whitespace().last())
        .and_then(|n| n.parse().ok())
        .unwrap_or_else(|| panic!("no total-instructions in profile:\n{text}"))
}

/// The emitted T3 assembly for a source.
fn t3_asm(src: &str) -> String {
    let d = workdir();
    let path = d.join("p.mt");
    std::fs::write(&path, src).expect("write");
    let base = path.with_extension("");
    let c = Command::new(manitc_bin())
        .args(["compile", path.to_str().unwrap(), "--target", "t3",
               "-o", base.to_str().unwrap()])
        .output().expect("compile");
    assert!(c.status.success(), "T3 compile failed:\n{}",
            String::from_utf8_lossy(&c.stderr));
    std::fs::read_to_string(base.with_extension("t3s")).expect("read .t3s")
}

/// The body of one function in the emitted assembly, label line excluded.
fn asm_fn(asm: &str, name: &str) -> String {
    let start = format!("\n{}:\n", name);
    let i = asm.find(&start).unwrap_or_else(|| panic!("no function `{name}` in:\n{asm}"))
        + start.len();
    let rest = &asm[i..];
    // Block labels inside a function are indented; the next FUNCTION label is
    // the next line starting at column 0 and ending in `:`.
    let mut out = String::new();
    for line in rest.lines() {
        if !line.starts_with(char::is_whitespace) && line.ends_with(':') {
            break;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

// ---------------------------------------------------------------------------
// 1. Zero extension: the property the whole syntax rests on
// ---------------------------------------------------------------------------

/// A wildcard-free trit pattern means EXACTLY the literal it spells.
///
/// `0t++0` is 12. The row asserts both directions on the same program: 12
/// matches, and 12 + 81 (the same low trits with a `+` at position 4) does
/// not — which is the half that fails if the high trits are not required to
/// be zero.
#[test]
fn c6_a_wildcard_free_trit_pattern_is_exactly_its_literal() {
    let src = r#"
use std::io;
fn f(x: int) -> int { match x { 0t++0 => 1, _ => 0 } }
fn g(x: int) -> int { match x { 0t++? => 1, _ => 0 } }
fn main() {
    io::println_int(f(12));
    io::println_int(f(93));
    io::println_int(f(11));
    io::println_int(g(12));
    io::println_int(g(93));
    io::println_int(g(11));
}
"#;
    // 12 = ++0. 93 = +0++0 (81 + 12), the same low three trits with more above.
    // 11 = ++-.
    //
    // `g` is the same pattern with the low trit freed, and it is here because
    // WITHOUT it this row would pass on the pre-C6 compiler: a wildcard-free
    // `0t++0` contains no wildcard, so it lexes as the `TernaryInt` it always
    // did and never reaches C6's code at all. The pair is the claim — the two
    // agree wherever the freed trit is `0`, and differ only there — and it is
    // also what makes the row discriminate.
    both(src, "1\n0\n0\n1\n0\n1\n", "zero extension");
}

/// Balanced ternary needs no sign extension, so the same rule serves negatives.
///
/// `-1` is `-`, `-4` is `--`, and each has ZEROS above it — not a run of `-`
/// the way two's complement would have a run of `1`. So `0t-` matches `-1` and
/// nothing wider, with no width in the rule anywhere.
#[test]
fn c6_a_negative_value_needs_no_sign_extension() {
    let src = r#"
use std::io;
fn f(x: int) -> int { match x { 0t- => 1, 0t-- => 2, _ => 0 } }
fn g(x: int) -> int { match x { 0t-? => 1, _ => 0 } }
fn main() {
    io::println_int(f(-1));
    io::println_int(f(-4));
    io::println_int(f(-13));
    io::println_int(g(-4));
    io::println_int(g(-2));
    io::println_int(g(-13));
}
"#;
    // -1 = `-`; -4 = `--`; -13 = `---`, which neither `f` pattern accepts.
    // `g` frees the low trit: -4 = `--` and -2 = `-+` both have trit 1 = `-`
    // and nothing above, so both match; -13 = `---` has a third trit and does
    // not. The wildcard form is here for the same reason as in the row above —
    // without it nothing in this program reaches C6's code.
    both(src, "1\n2\n0\n1\n1\n0\n", "no sign extension");
}

// ---------------------------------------------------------------------------
// 2. The wildcards
// ---------------------------------------------------------------------------

/// `?` is one trit of any value; the trits above are still required to be zero.
#[test]
fn c6_single_trit_wildcards_match_any_trit() {
    let src = r#"
use std::io;
fn f(x: int) -> str {
    match x {
        0t++?? => "high pair set",
        0t--?? => "high pair clear",
        0t?0?? => "third trit zero",
        _      => "other",
    }
}
fn main() {
    io::println(f(40));
    io::println(f(-40));
    io::println(f(9));
    io::println(f(0));
    io::println(f(1000));
}
"#;
    // 40 = ++++ ; -40 = ---- ; 9 = +00, so trit 2 is `+` and no arm takes it;
    // 0 is all zeros, which `0t?0??` accepts; 1000 = ++0+00+, seven trits wide,
    // so the zero-extension requirement rejects every four-trit pattern.
    both(src,
         "high pair set\nhigh pair clear\nother\nthird trit zero\nother\n",
         "single-trit wildcards");
}

/// A leading `*` is the only thing that frees the high trits.
#[test]
fn c6_the_run_wildcard_frees_the_high_trits() {
    let src = r#"
use std::io;
fn f(x: int) -> int { match x { 0t*++ => 1, _ => 0 } }
fn g(x: int) -> int { match x { 0t++  => 1, _ => 0 } }
fn main() {
    io::println_int(f(4));
    io::println_int(f(85));
    io::println_int(g(4));
    io::println_int(g(85));
}
"#;
    // 4 = ++ ; 85 = +0+++? -- 85 = 81 + 3 + 1 = +0 0 + +, low two trits `++`.
    // The open pattern takes both; the closed one takes only 4.
    both(src, "1\n1\n1\n0\n", "run wildcard");
}

// ---------------------------------------------------------------------------
// 3. Captures
// ---------------------------------------------------------------------------

/// `@name` binds the wildcard run immediately before it, as an `int`.
#[test]
fn c6_a_capture_binds_the_run_before_it() {
    let src = r#"
use std::io;
fn main() {
    match 40 { 0t++??@lo => io::println_int(lo), _ => io::println_int(-99) }
    match 1000 { 0t*@hi+00+ => io::println_int(hi), _ => io::println_int(-99) }
    match 40 {
        0t?@a?@b?? => { io::print_int(a); io::print(" "); io::println_int(b); }
        _ => io::println_int(-99),
    }
}
"#;
    // 40 = ++++ so the low two trits are `++` = 4.
    // 1000 = ++0+00+ ; the low four are `+00+`, leaving `++0` = 12 above.
    // 40's top two elements are trit 3 and trit 2, both `+`.
    both(src, "4\n12\n1 1\n", "captures");
}

/// A bare `*` with a name binds the whole word and matches everything.
#[test]
fn c6_a_bare_star_matches_everything_and_can_be_named() {
    let src = r#"
use std::io;
fn main() { match 123456 { 0t*@all => io::println_int(all) } }
"#;
    both(src, "123456\n", "bare star");
}

/// An or-pattern's alternatives write their captures into the same slot.
#[test]
fn c6_an_or_pattern_binds_a_capture_from_whichever_arm_matched() {
    let src = r#"
use std::io;
fn f(x: int) -> int {
    match x { 0t*+?@n | 0t*-?@n => n, _ => -99 }
}
fn main() {
    io::println_int(f(4));
    io::println_int(f(-4));
    io::println_int(f(0));
}
"#;
    // 4 = ++ : trit 1 is `+`, so the first alternative takes it and n = trit 0 = 1.
    // -4 = -- : trit 1 is `-`, the second takes it, n = trit 0 = -1.
    // 0 has trit 1 = 0, so neither alternative matches.
    both(src, "1\n-1\n-99\n", "or-pattern capture");
}

// ---------------------------------------------------------------------------
// 4. Exhaustiveness, and the three-way branch
// ---------------------------------------------------------------------------

/// Three arms fixing one position to `+`, `0` and `-` cover the word, so no
/// `_` arm is required.
#[test]
fn c6_a_one_trit_trichotomy_is_exhaustive() {
    let src = r#"
use std::io;
fn name(t: trit) -> str { match t { 0t*+ => "pos", 0t*0 => "zero", 0t*- => "neg" } }
fn main() { io::println(name(+)); io::println(name(0)); io::println(name(-)); }
"#;
    both(src, "pos\nzero\nneg\n", "trichotomy");
}

/// …and it is compiled as ONE three-way branch, which is C6's whole point.
///
/// Asserted on the emitted assembly, because the answer alone cannot tell a
/// `TBRANCH` from three equality tests that happen to agree with it. The
/// counts are exact: `TCMP` is what an equality test needs and a three-way
/// branch needs none.
#[test]
fn c6_a_trichotomy_compiles_to_a_single_three_way_branch() {
    let src = r#"
use std::io;
fn name(t: trit) -> str { match t { 0t*+ => "pos", 0t*0 => "zero", 0t*- => "neg" } }
fn main() { io::println(name(+)); }
"#;
    let body = asm_fn(&t3_asm(src), "name");
    let tbranch = body.matches("TBRANCH").count();
    let tcmp = body.matches("TCMP").count();
    assert_eq!(tbranch, 1, "expected exactly one TBRANCH in `name`, got {tbranch}:\n{body}");
    assert_eq!(tcmp, 0,
        "a three-way branch needs no comparison; {tcmp} TCMP means the match was \
         lowered as a chain of equality tests:\n{body}");
}

/// A trichotomy on a `trit` costs exactly what `tif` costs.
///
/// Not "about the same": the two programs execute the SAME NUMBER of
/// instructions, because on a `trit` scrutinee at position 0 the trit
/// extraction is the identity and is skipped, leaving both with one `TBRANCH`
/// and nothing else. An equality over a measurement is stronger evidence than
/// two implementations agreeing (P58), which is why the row asserts it rather
/// than a threshold.
///
/// For scale, the same function written the way it must be written without
/// C6 — `match t { + => …, 0 => …, - => … }` — runs 87,016 against these
/// 54,016 over the same 3,000 iterations.
#[test]
fn c6_a_trichotomy_on_a_trit_costs_exactly_what_tif_costs() {
    let body = |arm: &str| format!(r#"
use std::io;
fn name(t: trit) -> int {{ {arm} }}
fn main() {{
    let mut i: int = 0; let mut s: int = 0;
    while i < 3000 {{ s = s + name(((i % 3) - 1) as trit); i = i + 1; }}
    io::println_int(s);
}}
"#);
    let c6 = body("match t { 0t*+ => 1, 0t*0 => 2, 0t*- => 3 }");
    let tif = body("tif t { + => 1, 0 => 2, - => 3 }");

    // Same answer first: a cheaper program that computes something else is
    // not a faster program (rule 8 — assert the VALUE).
    assert_eq!(run_t3(&c6), "6000\n", "the trichotomy must compute the answer");
    assert_eq!(run_t3(&tif), "6000\n", "and so must `tif`");

    let a = t3_dynamic(&c6);
    let b = t3_dynamic(&tif);
    assert_eq!(a, b,
        "a trit-pattern trichotomy must cost exactly what `tif` costs: \
         match={a}, tif={b}");
}

/// Two of the three values is not a cover.
#[test]
fn c6_two_arms_of_a_trichotomy_are_not_exhaustive() {
    let src = r#"
use std::io;
fn name(t: trit) -> str { match t { 0t*+ => "pos", 0t*0 => "zero" } }
fn main() { io::println(name(+)); }
"#;
    let (ok, out) = check(src);
    assert!(!ok, "two of three arms must not be accepted as exhaustive:\n{out}");
    assert!(out.contains("non-exhaustive"), "expected a non-exhaustive message:\n{out}");
}

/// **A boundary row, and it passes on the pre-C6 compiler too.**
///
/// Without the `*` each arm ALSO requires every trit above position 0 to be
/// zero, so the three cover `-1`, `0` and `+1` and not a word. That is
/// exhaustive over a `trit` and not over an `int`, which is a claim about the
/// scrutinee's WIDTH — and the whole point of the leftmost-only rule is that a
/// trit pattern never makes one. So the compiler declines to call it a
/// trichotomy, and this row records that it declines.
#[test]
fn c6_without_the_star_three_arms_are_not_a_trichotomy() {
    let src = r#"
use std::io;
fn name(t: trit) -> str { match t { 0t+ => "pos", 0t0 => "zero", 0t- => "neg" } }
fn main() { io::println(name(+)); }
"#;
    let (ok, out) = check(src);
    assert!(!ok, "the no-star form must not be accepted as a trichotomy:\n{out}");
    assert!(out.contains("non-exhaustive"), "expected a non-exhaustive message:\n{out}");
}

// ---------------------------------------------------------------------------
// 5. Refusals — each names its own rule
// ---------------------------------------------------------------------------

#[test]
fn c6_the_run_wildcard_must_be_leftmost() {
    let src = r#"
use std::io;
fn f(x: int) -> int { match x { 0t+*? => 1, _ => 0 } }
fn main() { io::println_int(f(1)); }
"#;
    let (ok, out) = check(src);
    assert!(!ok, "a `*` after the first element must be refused:\n{out}");
    assert!(out.contains("only as the first element"),
            "the message must name the rule, not merely fail:\n{out}");
    assert!(out.contains("trit width"),
            "and must say WHY — a mid-pattern `*` would need the word's width:\n{out}");
}

#[test]
fn c6_the_scrutinee_must_be_a_balanced_ternary_word() {
    for (ty, val) in [("str", "\"a\""), ("float", "1.0"), ("bool", "true")] {
        let src = format!(r#"
use std::io;
fn f(x: {ty}) -> int {{ match x {{ 0t+? => 1, _ => 0 }} }}
fn main() {{ io::println_int(f({val})); }}
"#);
        let (ok, out) = check(&src);
        assert!(!ok, "a trit pattern on `{ty}` must be refused:\n{out}");
        assert!(out.contains("balanced-ternary word"),
                "the message must say what a trit pattern needs (`{ty}`):\n{out}");
    }
}

/// …including one level in, on a TUPLE ELEMENT.
///
/// The check walks sub-patterns rather than looking only at the scrutinee,
/// because `(0t+?, _)` on a `(str, int)` is the same mistake with a tuple
/// around it — and before it walked, that program was ACCEPTED and the
/// lowering divided a string pointer by three.
#[test]
fn c6_a_tuple_element_is_checked_too() {
    let src = r#"
use std::io;
fn f(a: str, b: int) -> int { match (a, b) { (0t+?, _) => 1, _ => 0 } }
fn main() { io::println_int(f("x", 1)); }
"#;
    let (ok, out) = check(src);
    assert!(!ok, "a trit pattern on a `str` tuple element must be refused:\n{out}");
    assert!(out.contains("balanced-ternary word"), "message:\n{out}");
    assert!(out.contains("`str`"),
            "and must name the ELEMENT's type, not the tuple's:\n{out}");
}

/// A trit pattern nested in a tuple pattern matches and captures normally.
///
/// The other direction of the row above: closing the hole must not close the
/// construct. `lo` is captured from the second element while the first is
/// tested, and the values discriminate — an earlier draft of this probe had
/// the answer collide with the fallback arm's, which would have passed over a
/// pattern that never matched at all (rule 8).
#[test]
fn c6_a_trit_pattern_nests_inside_a_tuple_pattern() {
    let src = r#"
use std::io;
fn f(a: int, b: int) -> int {
    match (a, b) { (0t+?, 0t-?@lo) => lo, (0t+?, _) => 100, _ => -9 }
}
fn main() {
    io::println_int(f(4, -2));
    io::println_int(f(4, 9));
    io::println_int(f(9, 9));
}
"#;
    // 4 = ++, so trit 1 is `+` and the first element matches both arms.
    // -2 = -+ : trit 1 is `-`, so the first arm takes it and lo = trit 0 = +1.
    // 9 = +00 : trit 1 is 0, so the second arm takes it.
    // 9 as the FIRST element has trit 1 = 0, so neither arm's first element
    // matches and the catch-all takes it.
    both(src, "1\n100\n-9\n", "nested trit pattern");
}

#[test]
fn c6_a_capture_must_follow_a_wildcard_run() {
    let src = r#"
use std::io;
fn f(x: int) -> int { match x { 0t+@a? => 1, _ => 0 } }
fn main() { io::println_int(f(1)); }
"#;
    let (ok, out) = check(src);
    assert!(!ok, "`@` on a fixed trit must be refused:\n{out}");
    assert!(out.contains("must follow the wildcard run"), "message:\n{out}");
}

#[test]
fn c6_a_capture_name_is_required_and_unique() {
    let (ok, out) = check(r#"
use std::io;
fn f(x: int) -> int { match x { 0t?@ => 1, _ => 0 } }
fn main() { io::println_int(f(1)); }
"#);
    assert!(!ok, "an empty capture name must be refused:\n{out}");
    assert!(out.contains("expected a capture name"), "message:\n{out}");

    let (ok, out) = check(r#"
use std::io;
fn f(x: int) -> int { match x { 0t?@a?@a => 1, _ => 0 } }
fn main() { io::println_int(f(1)); }
"#);
    assert!(!ok, "a repeated capture name must be refused:\n{out}");
    assert!(out.contains("binds `a` twice"), "message:\n{out}");
}

/// The width cap is arithmetic, not taste: the lowering needs 3^width as a
/// machine word and 3^40 is not one. The row asserts the boundary in both
/// directions, one trit either side.
#[test]
fn c6_a_trit_pattern_spans_at_most_thirty_nine_trits() {
    let mk = |n: usize| format!(r#"
use std::io;
fn f(x: int) -> int {{ match x {{ 0t{} => 1, _ => 0 }} }}
fn main() {{ io::println_int(f(1)); }}
"#, "?".repeat(n));

    let (ok, out) = check(&mk(39));
    assert!(ok, "39 trits is inside the cap and must be accepted:\n{out}");

    let (ok, out) = check(&mk(40));
    assert!(!ok, "40 trits is outside the cap and must be refused:\n{out}");
    assert!(out.contains("at most 39 trits"), "the message must name the cap:\n{out}");
    assert!(out.contains("does not fit"), "and the reason:\n{out}");
}

// ---------------------------------------------------------------------------
// 6. The lexical claim: no existing program changes meaning
// ---------------------------------------------------------------------------

/// `*` after a `0t` literal is still multiplication.
///
/// **This row passes on the pre-C6 compiler, and that is the whole point.**
/// It asserts that nothing moved, so a version of it that went red would be
/// reporting the change this row exists to rule out.
///
/// Extending the `0t` run to absorb `?` and `*` is a lexical change, and the
/// claim made for it is that no program's meaning moves. `*` is the only one
/// of the two that can legally follow an integer literal, so it is the one
/// that needs pinning: it becomes a run wildcard only in the leftmost
/// position, or when the character after it could not begin an operand. `3`
/// and `+` both can.
#[test]
fn c6_multiplication_after_a_ternary_literal_is_unchanged() {
    let src = r#"
use std::io;
fn main() {
    io::println_int(0t+*3);
    io::println_int(0t+*+);
    io::println_int(0t+-*3);
}
"#;
    // 0t+ is 1, so 1*3 = 3 and 1*(+1) = 1. 0t+- is 2, so 2*3 = 6.
    both(src, "3\n1\n6\n", "multiplication is unchanged");
}

/// A trit pattern means the same thing under both language versions.
///
/// This is the claim the leftmost-only `*` rule exists to make true, so it is
/// pinned rather than argued. `docs/semantics.md` §10.1 records `int` as a
/// 27-trit word on T3 and 64 bits on LLVM under v1, and N5 closes that gap
/// under v2 — a pattern whose meaning depended on the width would therefore
/// differ across FOUR combinations, and a row that ran only one of them would
/// not notice.
#[test]
fn c6_a_trit_pattern_is_width_independent() {
    let src = r#"
use std::io;
fn f(x: int) -> str {
    match x { 0t++?? => "a", 0t*++ => "b", 0t?0?? => "c", _ => "d" }
}
fn main() {
    io::println(f(40));
    io::println(f(85));
    io::println(f(0));
    io::println(f(1000));
}
"#;
    // 40 = ++++ takes the first arm. 85 = +00++ has trits above the fourth, so
    // the first arm's zero-extension rejects it and the open `0t*++` takes it.
    // 0 is all zeros, which reaches `0t?0??`. 1000 = ++0+00+ matches none.
    let want = "a\nb\nc\nd\n";
    for lang in ["v1", "v2"] {
        let d = workdir();
        let path = d.join("p.mt");
        std::fs::write(&path, src).expect("write");
        let base = path.with_extension("");
        let c = Command::new(manitc_bin())
            .args(["compile", path.to_str().unwrap(), "--target", "t3",
                   "--lang", lang, "-o", base.to_str().unwrap()])
            .output().expect("compile");
        assert!(c.status.success(), "T3 compile failed under {lang}:\n{}",
                String::from_utf8_lossy(&c.stderr));
        let r = Command::new(manitc_bin())
            .args(["run-t3", base.with_extension("t3b").to_str().unwrap()])
            .output().expect("run");
        let got: String = String::from_utf8_lossy(&r.stdout)
            .lines().filter(|l| !l.starts_with("[T3ISA]"))
            .map(|l| format!("{l}\n")).collect();
        assert_eq!(got, want, "T3 under --lang {lang}");

        let bin = d.join("p.bin");
        Command::new(manitc_bin())
            .args(["compile", path.to_str().unwrap(), "--target", "llvm",
                   "--lang", lang, "-o", bin.to_str().unwrap()])
            .output().expect("compile");
        if bin.exists() {
            let r = Command::new(&bin).output().expect("run");
            assert_eq!(String::from_utf8_lossy(&r.stdout), want,
                       "LLVM under --lang {lang}");
        }
    }
}

// ---------------------------------------------------------------------------
// 7. P113 — a match that matches nothing
// ---------------------------------------------------------------------------

/// A `match` no arm of which accepts the value TRAPS, identically on both
/// backends.
///
/// Pre-existing and reachable from ordinary literal patterns; C6 is what made
/// it easy to reach, because a wildcard-free trit pattern is narrow by
/// construction. The lowerer's own comment said this case "traps"; T3 emitted
/// a bare `HALT`, so the program stopped with status 0 and produced no further
/// output, and LLVM emitted an `unreachable`, which is undefined behaviour.
/// The row asserts the OUTPUT — a row on the exit status alone would have
/// called T3's silent stop a pass.
#[test]
fn p113_a_match_with_no_matching_arm_traps_on_both_backends() {
    let src = r#"
use std::io;
fn f(x: int) -> int { match x { 0t+ => 1, 0t0 => 0, 0t- => 2 } }
fn main() { io::println("before"); io::println_int(f(50)); io::println("after"); }
"#;
    both(src,
         "before\nTRAP: unreachable code reached — commonly a `match` with no arm \
          for this value\n",
         "P113");
}

/// The same defect through a plain integer literal pattern, which is what
/// dates it as pre-existing rather than C6's.
#[test]
fn p113_the_trap_is_not_specific_to_trit_patterns() {
    let src = r#"
use std::io;
fn f(x: int) -> int { match x { 1 => 1, 2 => 2 } }
fn main() { io::println("before"); io::println_int(f(50)); io::println("after"); }
"#;
    both(src,
         "before\nTRAP: unreachable code reached — commonly a `match` with no arm \
          for this value\n",
         "P113 via literal patterns");
}
