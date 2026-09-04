//! B7 D-5 — where the move check lives, and what it can see.
//!
//! © Manish Jagdish Thatte
//!
//! `enhance/phase5-type-system-second-half/B7_AFFINE_TYPES.md` D-5, specified
//! in `docs/language-reference.md` §22.
//!
//! **The decision is that the check stays in `src/borrow/mod.rs`** — it runs
//! over the `TypedProgram`, after the analyzer and before lowering, because an
//! affine check needs types. The consequence is the part that needed work:
//! the checker sees exactly the function bodies the analyzer managed to BUILD,
//! and for a generic function that is more than one body.
//!
//! A generic is checked erased, where a `T` is not a move type because nothing
//! says it is, and again per instantiation, where it may be. So `report.txt`
//! P65's rule — an instantiation whose body fails to check is DISCARDED and
//! the call keeps the erased path — silently decided a second question it was
//! never asked: whether the move checker ever saw that body.
//!
//! **That is P71's shape a second time.** P71 found one verdict gating two
//! questions: the NAME must wait on the body's verdict, the RETURN TYPE must
//! not, because a return type is a function of the declaration. The third
//! question at the same fork is coverage, and its honest answer when a body
//! was not built is "not checked, and here" rather than silence. Hence
//! `unchecked-instantiation` (§20), `warn` by default.
//!
//! **Measured before it was written.** The population of discarded
//! instantiations is **0 of 2,507 model-corpus files** and **4 of 366 files
//! across maniTC and thatteOS** — and all four of those are fixtures written
//! to exercise this very fallback. A lint with no backlog can afford `warn`;
//! that is what `undeclared-native`, with 413, could not.
//!
//! One row here is a LIMIT rather than a guarantee, and it is marked: a move
//! error inside a discarded instantiation is still not reported, because there
//! is no body to report it from. It goes red the day that is closed.

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

mod common;

static N: AtomicUsize = AtomicUsize::new(0);

fn manitc_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_manitc"))
}

fn write(src: &str) -> PathBuf {
    let slot = N.fetch_add(1, Ordering::Relaxed);
    let d = common::suite_root("d5").join(slot.to_string());
    std::fs::create_dir_all(&d).expect("temp dir");
    let path = d.join("p.mt");
    std::fs::write(&path, src).expect("write");
    path
}

/// `manitc check`, with any extra flags. Returns (accepted, all output).
fn check_with(src: &str, flags: &[&str]) -> (bool, String) {
    let path = write(src);
    let mut args: Vec<String> = vec!["check".into()];
    args.extend(flags.iter().map(|s| s.to_string()));
    args.push(path.to_str().unwrap().to_string());
    let o = Command::new(manitc_bin()).args(&args).output().expect("check");
    let txt =
        String::from_utf8_lossy(&o.stdout).into_owned() + &String::from_utf8_lossy(&o.stderr);
    (o.status.success() && !txt.contains("error:"), txt)
}

fn check(src: &str) -> (bool, String) {
    check_with(src, &[])
}

const P: &str = "struct P { pub x: int, pub y: int }\n";

/// A generic body whose move depends on `T`, instantiated at `AT`.
fn dup_at(at: &str) -> String {
    format!(
        "{P}fn dup<T>(a: T) -> int {{\n    let b = a;\n    let c = a;\n    return 1;\n}}\n\
         fn main() {{ io::print_int(dup({at})); }}\n"
    )
}

/// The same, with a line that does not type-check at `T = P`, so the
/// instantiation is discarded.
fn dup_discarded() -> String {
    format!(
        "{P}fn dup<T>(a: T) -> int {{\n    let b = a;\n    let c = a;\n    let q = a | 1;\n    \
         return 1;\n}}\n\
         fn main() {{ io::print_int(dup(P {{ x: 1, y: 2 }})); }}\n"
    )
}

// ---------------------------------------------------------------------------
// What the checker DOES see. These pin behaviour that already worked, so that
// the coverage rule below is stated against a known floor rather than a guess.
// ---------------------------------------------------------------------------

/// The erased body is checked, called or not: `str` is a move type whatever
/// `T` turns out to be.
#[test]
fn an_erased_generic_body_is_move_checked() {
    for (what, main) in [
        ("called", "fn main() { io::print_int(dup(1, \"hi\")); }\n"),
        ("uncalled", "fn main() { io::print_int(1); }\n"),
    ] {
        let src = format!(
            "fn dup<T>(a: T, s: str) -> int {{\n    let b = s;\n    let c = s;\n    return 1;\n}}\n{main}"
        );
        let (ok, msg) = check(&src);
        assert!(
            !ok && msg.contains("use of moved value"),
            "the erased body must be move-checked ({what}); got:\n{msg}"
        );
    }
}

/// And the instantiation is checked at the type it was called at — which is
/// the only place a move on a `T` can be seen at all.
#[test]
fn a_generic_body_is_move_checked_at_the_type_it_is_called_at() {
    let (ok, msg) = check(&dup_at("P { x: 1, y: 2 }"));
    assert!(
        !ok && msg.contains("use of moved value"),
        "`dup<P>` moves `a` twice and must be refused; got:\n{msg}"
    );
}

/// The other half of the same pair (permanent rule 8): at a Copy type the
/// identical body is not a move, and refusing it would be the false positive
/// a conservative erased rule would produce.
#[test]
fn the_same_generic_body_at_a_copy_type_is_not_a_move() {
    let (ok, msg) = check(&dup_at("1"));
    assert!(ok, "`dup<int>` copies and must be accepted; got:\n{msg}");
}

// ---------------------------------------------------------------------------
// What it cannot see, and now says so.
// ---------------------------------------------------------------------------

/// The free-function fork.
#[test]
fn a_discarded_instantiation_is_reported_at_the_call_site() {
    let (ok, msg) = check(&dup_discarded());
    assert!(ok, "P65 stands: a failed instantiation is not an error; got:\n{msg}");
    assert!(
        msg.contains("[unchecked-instantiation]"),
        "the discard must be reported, not silent; got:\n{msg}"
    );
    // At the CALL, not at the declaration: the binding is the call's property.
    assert!(
        msg.contains(":8:"),
        "the diagnostic belongs at the call site (line 8); got:\n{msg}"
    );
}

/// And it carries the reason, which is what `mono_failure` was kept for.
#[test]
fn the_report_names_the_reason_the_body_failed() {
    let (_, msg) = check(&dup_discarded());
    assert!(
        msg.contains("operator `|` cannot be applied"),
        "the report must name why the body failed; got:\n{msg}"
    );
    assert!(
        msg.contains("the move checker"),
        "the report must say what went unchecked; got:\n{msg}"
    );
}

/// The `impl<T>` method fork. **Both sites, one shape** — the hazard note in
/// `CLAUDE.md` is that a fix at the reported site leaves the others, and this
/// fork is reached through a method rather than a free function.
#[test]
fn the_impl_method_fork_reports_too() {
    let src = format!(
        "{P}struct Box2<T> {{ pub a: T, pub b: T }}\n\
         impl<T> Box2<T> {{ fn first(self) -> T {{ if self.a > self.b {{ self.a }} else {{ self.a }} }} }}\n\
         fn main() {{\n    let b = Box2 {{ a: P {{ x: 1, y: 2 }}, b: P {{ x: 3, y: 4 }} }};\n    \
         let q = b.first();\n    io::print_int(q.x);\n}}\n"
    );
    let (ok, msg) = check(&src);
    assert!(ok, "P65 stands here too; got:\n{msg}");
    assert!(
        msg.contains("[unchecked-instantiation]") && msg.contains("Box2::first"),
        "the method fork must report, naming the method; got:\n{msg}"
    );
}

/// A healthy instantiation is not reported. The false-positive guard, and it
/// is not vacuous: the flag itself only parses on a compiler that has the
/// lint, so this row is red on the compiler without it.
#[test]
fn an_instantiation_that_checks_is_not_reported() {
    let (ok, msg) = check_with(&dup_at("1"), &["-D", "unchecked-instantiation"]);
    assert!(
        ok && !msg.contains("unchecked-instantiation:"),
        "a body that checks must not be reported, even at deny; got:\n{msg}"
    );
}

/// `allow` restores the previous compiler exactly: the instantiation is still
/// discarded, the call still keeps the erased path, only the report goes.
#[test]
fn allow_restores_the_previous_compiler_exactly() {
    let src = format!("lint allow(unchecked-instantiation);\n{}", dup_discarded());
    let (ok, msg) = check(&src);
    assert!(ok, "`allow` must leave the program accepted; got:\n{msg}");
    assert!(
        !msg.contains("unchecked-instantiation"),
        "`allow` must silence it completely; got:\n{msg}"
    );
}

/// ...and the level is settable in the other direction too.
///
/// **This row was HOLLOW when first written, and permanent rule 9 is what
/// found it.** `!ok && msg.contains("unchecked-instantiation")` passed on the
/// compiler WITHOUT the lint, because there the flag is rejected — "unknown
/// lint 'unchecked-instantiation'" is also a failure and also contains the
/// name. A row that a wrong compiler passes for its own reason states
/// nothing. It now asserts the shape only the real diagnostic has.
#[test]
fn deny_turns_the_discard_into_a_failure() {
    let (ok, msg) = check_with(&dup_discarded(), &["-D", "unchecked-instantiation"]);
    assert!(!ok, "at deny the discard must fail the compilation; got:\n{msg}");
    assert!(
        !msg.contains("unknown lint"),
        "the flag must be ACCEPTED and the lint must fire — this failure is \
         the flag being rejected, which is a different thing:\n{msg}"
    );
    assert!(
        msg.contains("[unchecked-instantiation]")
            && msg.contains("1 denied lint (unchecked-instantiation)"),
        "at deny the lint itself must be what aborts; got:\n{msg}"
    );
}

// ---------------------------------------------------------------------------
// P71's half, and the limit.
// ---------------------------------------------------------------------------

/// **What is DECLARED survives the discard.** A `move` parameter consumes its
/// argument whether or not the callee's body checks at that binding, because
/// that is a fact about the signature rather than about the body — P71's split
/// exactly, and it already held. Pinned so that closing the limit below cannot
/// quietly cost it.
#[test]
fn a_declared_move_survives_a_discarded_instantiation() {
    let src = format!(
        "{P}fn eat<T>(a: move T) -> int {{ let q = a | 1; return 1; }}\n\
         fn main() {{\n    let p = P {{ x: 1, y: 2 }};\n    let n = eat(p);\n    \
         io::print_int(p.x + n);\n}}\n"
    );
    let (ok, msg) = check(&src);
    assert!(
        !ok && msg.contains("use of moved value"),
        "a declared `move` must consume even when the instantiation is \
         discarded; got:\n{msg}"
    );
}

/// **A STATED LIMIT, 4 September 2026. This row goes red the day it closes.**
///
/// A move error that only the discarded binding would have revealed is still
/// unreported, and there is no body to report it from: `check_fn` returned
/// `Err`, so no typed body was ever produced, and `consume_if_move` reads the
/// type on each use-site expression — which in the erased body is `Unknown`
/// for every `T`. Substituting after the fact would mean retyping a whole
/// body, which is the work `check_fn` under a `mono_binding` already does and
/// which is exactly what failed.
///
/// So the compiler reports the HOLE instead of the move. When a later
/// increment closes this — by partial typing, or by making a failed type
/// instantiation an error as B3 already did for const parameters — the first
/// assertion here fails and this row is the notice.
#[test]
fn limit_a_move_inside_a_discarded_instantiation_is_still_unreported() {
    let (ok, msg) = check(&dup_discarded());
    assert!(
        ok && !msg.contains("use of moved value"),
        "LIMIT CLOSED: a move inside a discarded instantiation is now \
         reported. Update `docs/language-reference.md` §22 and this row — the \
         limit it states is no longer true.\n{msg}"
    );
    assert!(
        msg.contains("[unchecked-instantiation]"),
        "while the limit stands, the hole must at least be named; got:\n{msg}"
    );
}
