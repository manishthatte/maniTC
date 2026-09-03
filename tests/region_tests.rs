//! F-4 — allocation regions.
//!
//! © Manish Jagdish Thatte
//!
//! `KNOWN_ISSUES` issue 6 has read "no free/destroy API — leak by design"
//! since the initial release, and `MANITC_FEATURE_RECOMMENDATIONS.txt` F-4
//! answered it with REGIONS rather than a collector, for a reason specific to
//! this machine: the T3 heap **is** a bump pointer, so releasing a region
//! costs one assignment.
//!
//! `region { B }` lowers to two ordinary calls around an ordinary block —
//! **no new IR instruction, no new terminator and not one pass touched**,
//! which is P89's shape and for P89's reason. The rows below assert the two
//! things that makes true: the answer does not change, and the memory does.
//!
//! Every row runs BOTH backends where it can. The two reclaim by different
//! mechanisms — T3 resets a bump pointer, LLVM frees a recorded list — and
//! that is what makes their agreement evidence rather than a shared lowering
//! agreeing with itself.

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
    let d = common::suite_root("f4").join(slot.to_string());
    std::fs::create_dir_all(&d).expect("temp dir");
    d
}

/// Compile and run on T3; returns (output, peak heap words).
fn run_t3(src: &str) -> (String, i64) {
    let d = workdir();
    let path = d.join("p.mt");
    std::fs::write(&path, src).expect("write");
    let base = path.with_extension("");
    let c = Command::new(manitc_bin())
        .args(["compile", path.to_str().unwrap(), "--target", "t3",
               "-o", base.to_str().unwrap()])
        .output()
        .expect("compile");
    assert!(c.status.success(), "T3 compile failed:\n{}{}",
            String::from_utf8_lossy(&c.stdout), String::from_utf8_lossy(&c.stderr));
    let r = Command::new(manitc_bin())
        .args(["run-t3", base.with_extension("t3b").to_str().unwrap(), "--profile"])
        .output()
        .expect("run");
    let all = format!("{}{}", String::from_utf8_lossy(&r.stdout),
                      String::from_utf8_lossy(&r.stderr));
    // The emulator prints `max-heap-words`, with a HYPHEN. A sweep that greps
    // for the underscore sums zero and reports agreement.
    let heap = all
        .lines()
        .find(|l| l.contains("max-heap-words"))
        .and_then(|l| l.split_whitespace().last())
        .and_then(|n| n.parse::<i64>().ok())
        .unwrap_or(-1);
    let out: String = String::from_utf8_lossy(&r.stdout)
        .lines()
        .filter(|l| !l.starts_with("[T3ISA]"))
        .map(|l| format!("{}\n", l))
        .collect();
    (out, heap)
}

/// Compile and run on LLVM. `None` when no binary appeared, which is an
/// environment fact rather than a result — told apart by the ARTEFACT and
/// never by the shape of a message (P47).
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
    if !bin.exists() {
        return None;
    }
    assert!(c.status.success(), "LLVM compile failed:\n{}{}",
            String::from_utf8_lossy(&c.stdout), String::from_utf8_lossy(&c.stderr));
    let r = Command::new(&bin).output().expect("run");
    Some(String::from_utf8_lossy(&r.stdout).into_owned())
}

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

const LOOP_WITH_REGION: &str = r#"
use std::io;
struct P { pub a: int, pub b: int }
fn main() {
    let mut i: int = 0;
    while i < 200 {
        region {
            let p: P = P { a: i, b: i * 2 };
            let q: P = P { a: p.b, b: p.a };
            if i == 0 { io::print("first q.a="); io::println_int(q.a); }
        }
        i = i + 1;
    }
    io::println("done");
}
"#;

const LOOP_WITHOUT_REGION: &str = r#"
use std::io;
struct P { pub a: int, pub b: int }
fn main() {
    let mut i: int = 0;
    while i < 200 {
        let p: P = P { a: i, b: i * 2 };
        let q: P = P { a: p.b, b: p.a };
        if i == 0 { io::print("first q.a="); io::println_int(q.a); }
        i = i + 1;
    }
    io::println("done");
}
"#;

#[test]
fn f4_a_region_releases_what_its_body_allocated() {
    // The pair is the claim: the same program with and without the region.
    // Asserting a heap number alone would pass on a compiler that had simply
    // stopped allocating, and asserting the output alone would pass on one
    // where `region` parsed and did nothing.
    let (out_r, heap_r) = run_t3(LOOP_WITH_REGION);
    let (out_n, heap_n) = run_t3(LOOP_WITHOUT_REGION);
    assert_eq!(out_r, out_n, "F-4: a region must not change the answer");
    assert_eq!(out_r, "first q.a=0\ndone\n");
    assert!(
        heap_n >= 400,
        "F-4: the control should allocate per iteration; it peaked at {heap_n}"
    );
    assert!(
        heap_r < 20,
        "F-4: the region should release per iteration; it peaked at {heap_r} \
         against the control's {heap_n}"
    );
}

#[test]
fn f4_both_backends_give_the_same_answer_through_a_region() {
    // T3 resets a bump pointer; LLVM frees a recorded list. Two mechanisms,
    // one observable — which is what makes this row evidence rather than a
    // shared lowering agreeing with itself.
    let (t3, _) = run_t3(LOOP_WITH_REGION);
    if let Some(ll) = run_llvm(LOOP_WITH_REGION) {
        assert_eq!(t3, ll, "F-4: the backends disagree through a region");
    }
}

#[test]
fn f4_a_scalar_may_leave_a_region_and_that_is_how_it_returns_an_answer() {
    let src = r#"
use std::io;
fn main() {
    let mut n: int = 0;
    region {
        let s: str = str::concat("a", "b");
        n = str::len(s);
    }
    io::print("n="); io::println_int(n);
}
"#;
    let (ok, msg) = check(src);
    assert!(ok, "F-4: an int may leave a region:\n{msg}");
    let (t3, _) = run_t3(src);
    assert_eq!(t3, "n=2\n");
    if let Some(ll) = run_llvm(src) {
        assert_eq!(ll, "n=2\n");
    }
}

#[test]
fn f4_the_three_rules_each_refuse_and_each_says_which() {
    // Three rules, three directions, asserted separately — a first failure
    // short-circuits the rest, so one probe with three shapes would prove only
    // that ONE of them bites (P93's control lesson).
    let cases = [
        ("return", r#"
use std::io;
fn f() -> int { region { return 1; } return 0; }
fn main() { io::println_int(f()); }
"#, "return"),
        ("storage escapes", r#"
use std::io;
fn main() {
    let mut s: str = "outer";
    region { s = str::concat("in", "side"); }
    io::println(s);
}
"#, "outlives"),
        ("break out", r#"
use std::io;
fn main() {
    let mut i: int = 0;
    while i < 3 { region { if i == 1 { break; } } i = i + 1; }
    io::println("done");
}
"#, "break"),
    ];
    for (what, src, needle) in cases {
        let (ok, msg) = check(src);
        assert!(!ok, "F-4: `{what}` inside a region must be refused");
        assert!(
            msg.contains("region") && msg.contains(needle),
            "F-4: the `{what}` refusal must say which rule it is, and said:\n{msg}"
        );
    }
}

#[test]
fn f4_a_loop_inside_a_region_is_ordinary() {
    // The boundary the `break` rule is really about. A row that only checked
    // the refusal would be satisfied by a compiler that refused every `break`
    // in the language.
    let src = r#"
use std::io;
fn main() {
    region {
        let mut i: int = 0;
        while i < 3 { if i == 1 { i = i + 1; continue; } i = i + 1; }
    }
    io::println("ok");
}
"#;
    let (ok, msg) = check(src);
    assert!(ok, "F-4: a loop written inside a region is ordinary:\n{msg}");
    let (t3, _) = run_t3(src);
    assert_eq!(t3, "ok\n");
}

#[test]
fn f4_a_program_without_regions_is_unchanged() {
    // A GUARD, and it says so: it passes on the pre-F-4 compiler too. It is
    // here because `manit_alloc` now stands where `malloc` did on LLVM and a
    // syscall number was added on T3, and the claim attached to both is that
    // a program that never writes `region` is the program it always was.
    let (out, heap) = run_t3(LOOP_WITHOUT_REGION);
    assert_eq!(out, "first q.a=0\ndone\n");
    assert!(heap >= 400, "the control's allocation behaviour moved: {heap}");
}

#[test]
fn f4_nested_regions_release_innermost_first() {
    // The mark is a STACK, not a variable. With a single saved mark the inner
    // release would reset to the outer one and the outer release would then
    // free memory a second time — which on T3 is silent and on LLVM is a
    // double free.
    let src = r#"
use std::io;
struct P { pub a: int }
fn main() {
    let mut n: int = 0;
    region {
        let outer: P = P { a: 1 };
        region {
            let inner: P = P { a: 2 };
            n = outer.a + inner.a;
        }
        n = n + outer.a;
    }
    io::print("n="); io::println_int(n);
}
"#;
    let (t3, _) = run_t3(src);
    assert_eq!(t3, "n=4\n", "F-4: the outer region's cell must survive the inner release");
    if let Some(ll) = run_llvm(src) {
        assert_eq!(ll, "n=4\n");
    }
}

// ---------------------------------------------------------------------------
// P119 — rule 3 asked about the wrong thing, and three spellings walked past it
// ---------------------------------------------------------------------------
//
// F-4 as first committed asked rule 3 of a plain-identifier assignment target.
// Three routes to the same escape were not targets of that shape:
//
//     v.push(s)      an outer Vec, given a cell allocated inside
//     b.f = s        a Field target, not an Ident
//     a[0] = s       an Index target, not an Ident
//
// Measured on the compiler that shipped without the fix, the first one:
// **T3 printed `v[0]=` — nothing — while LLVM printed `hello`.** A silent
// wrong answer on one backend and a divergence between them, in code F-4
// accepted. The cell was released with the region and the Vec kept its
// address; on LLVM the string is the collections library's own allocation,
// which the region does not hold, so it survived by not being reclaimed.
//
// The rule now asks about the ROOT of the target, and about the ARGUMENTS of a
// method call whose receiver is rooted outside the region. The receiver's own
// type is deliberately not the question: a `Vec` handle may leave a region;
// what may not is the cell it would be left holding.

#[test]
fn p119_storage_may_not_reach_an_outer_holder_by_any_route() {
    let cases = [
        ("method call on an outer handle", r#"
use std::io;
fn main() {
    let v: Vec<str> = Vec::new();
    region { let s: str = str::concat("hel", "lo"); v.push(s); }
    io::println(v.get(0));
}
"#),
        ("field of an outer struct", r#"
use std::io;
struct Box2 { pub s: str }
fn main() {
    let mut b: Box2 = Box2 { s: "outer" };
    region { b.s = str::concat("in", "ner"); }
    io::println(b.s);
}
"#),
        ("element of an outer array", r#"
use std::io;
fn main() {
    let a: [str; 2] = ["x", "y"];
    region { a[0] = str::concat("in", "ner"); }
    io::println(a[0]);
}
"#),
    ];
    for (what, src) in cases {
        let (ok, msg) = check(src);
        assert!(!ok, "P119: storage escaping by {what} must be refused");
        assert!(
            msg.contains("outlives this `region`"),
            "P119: the {what} refusal must name the rule, and said:\n{msg}"
        );
    }
}

#[test]
fn p119_the_rule_is_about_the_cell_and_not_about_the_receiver() {
    // Both directions of the discriminator, because getting it wrong the other
    // way refuses ordinary code: a handle built INSIDE the region may be
    // filled freely, and a scalar may be pushed onto an outer handle. A row
    // that only checked the refusals would be satisfied by a compiler that
    // refused every method call inside a region.
    let cases = [
        ("a handle built inside the region", r#"
use std::io;
fn main() {
    let mut n: int = 0;
    region {
        let v: Vec<str> = Vec::new();
        v.push(str::concat("a", "b"));
        n = v.len();
    }
    io::print("n="); io::println_int(n);
}
"#, "n=1\n"),
        ("a scalar into an outer handle", r#"
use std::io;
fn main() {
    let v: Vec<int> = Vec::new();
    region { v.push(7); }
    io::print("v0="); io::println_int(v.get(0));
}
"#, "v0=7\n"),
    ];
    for (what, src, want) in cases {
        let (ok, msg) = check(src);
        assert!(ok, "P119 must not refuse {what}:\n{msg}");
        let (t3, _) = run_t3(src);
        assert_eq!(t3, want, "P119: {what} on T3");
        if let Some(ll) = run_llvm(src) {
            assert_eq!(ll, want, "P119: {what} on LLVM");
        }
    }
}

// ---------------------------------------------------------------------------
// P120 — a handle may leave a region, but not while it is holding a cell
// ---------------------------------------------------------------------------
//
// F-4's split was storage-versus-handle, and P119 fixed the ROUTES a cell could
// take out. This is the third hole and it is neither: every step is permitted
// and the composition is unsound.
//
//     let mut keep: Vec<str> = Vec::new();
//     region {
//         let inner: Vec<str> = Vec::new();
//         inner.push(str::concat("hel", "lo"));  // legal: inner is inside
//         keep = inner;                          // legal: a handle may leave
//     }
//     keep.get(0)     // T3 printed NOTHING; LLVM printed "hello"
//
// So the test is on what the type CONTAINS and not on its head: `Vec<int>` may
// leave a region, `Vec<str>` may not.
//
// **Two of the three answers here came from probing rather than reasoning.**
// A `Generic` with an EMPTY argument list has to count — `any()` over nothing
// is vacuously false — and, measured, an unannotated `Vec::new()` does not
// type as `Vec` at all: it binds `Unknown`, which `let x: int = inner;`
// accepts on the same compiler (P95's family). A rule that enumerated the
// container types would have walked past the commonest way to write one.

#[test]
fn p120_a_container_of_cells_may_not_leave_a_region() {
    let cases = [
        ("an annotated Vec<str>", r#"
use std::io;
fn main() {
    let mut keep: Vec<str> = Vec::new();
    region {
        let inner: Vec<str> = Vec::new();
        inner.push(str::concat("hel", "lo"));
        keep = inner;
    }
    io::println(keep.get(0));
}
"#),
        ("an unannotated one, which types as Unknown", r#"
use std::io;
fn main() {
    let mut keep: Vec<str> = Vec::new();
    region { let inner = Vec::new(); inner.push(str::concat("a", "b")); keep = inner; }
    io::println(keep.get(0));
}
"#),
    ];
    for (what, src) in cases {
        let (ok, msg) = check(src);
        assert!(!ok, "P120: {what} must not leave a region");
        assert!(
            msg.contains("outlives this `region`"),
            "P120: the {what} refusal must name the rule, and said:\n{msg}"
        );
    }
}

#[test]
fn p120_a_container_of_words_still_may() {
    // The discriminator's other side, and the one that would hurt: refusing
    // every container would make a region useless for anything that computes
    // a collection. Asserted on the VALUE and on both backends.
    let src = r#"
use std::io;
fn main() {
    let mut keep: Vec<int> = Vec::new();
    region {
        let inner: Vec<int> = Vec::new();
        inner.push(7);
        keep = inner;
    }
    io::print("keep0="); io::println_int(keep.get(0));
}
"#;
    let (ok, msg) = check(src);
    assert!(ok, "P120 must not refuse a Vec<int> leaving a region:\n{msg}");
    let (t3, _) = run_t3(src);
    assert_eq!(t3, "keep0=7\n");
    if let Some(ll) = run_llvm(src) {
        assert_eq!(ll, "keep0=7\n");
    }
}
