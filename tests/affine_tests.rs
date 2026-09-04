//! B7 D-1 — `affine struct`, a type that may be used once.
//!
//! © Manish Jagdish Thatte
//!
//! `enhance/phase5-type-system-second-half/B7_AFFINE_TYPES.md` D-1, specified
//! in `docs/language-reference.md` §22.
//!
//! **Affinity is OPTED INTO.** That is D-1's answer and the design document
//! argued for it from this codebase rather than from Rust: the set of types
//! where a copy is observable is small and already known, and every guess the
//! move checker made about the rest was invisible. So `affine` is a marker on
//! a declaration, not a rule about a category.
//!
//! **WHAT THE MARKER ACTUALLY CHANGES, measured rather than assumed — and the
//! first answer written here was wrong.** This file claimed that a fieldless
//! struct is Copy and that `affine` makes it a move type. It does not:
//! `is_move_type` answers `true` for `ManiType::Struct(_, _)` with no regard
//! for fields, so EVERY user struct is already affine at the binding level and
//! the marker adds nothing there. The row asserting otherwise passed on the
//! unmarked control and was removed; `the_binding_rule_is_unchanged_by_the_marker`
//! now states the true fact instead.
//!
//! So for a user-declared type the marker changes exactly one thing today:
//! **whether `spawn` may capture it** (D-4 part 3). The `aggregates` set that
//! governs captures IS keyed on having fields, so a fieldless struct is
//! capturable and an affine one is not. That pair is the marker's whole
//! observable content, and it is real.
//!
//! Its other intended content — inverting the Copy exemption that
//! `is_move_type` grants BY NAME to `AtomicTrit`, `Barrier`, `Semaphore`,
//! `MutexGuard`, `Mutex`, `Channel` and `Task` — is the motivating case, and
//! it is blocked by P132 rather than by anything here.
//!
//! **Measured before it was added** (P104's lesson, an eighth time): `affine`
//! occurs **5 times across both repositories and 0 times in the 2,507-program
//! corpus**, and all five are the SAME comment line in `stdlib/sync.mt` —
//! "Ownership follows maniT's affine type rules", written when there were
//! none. It is contextual, so `let affine = 7;` still compiles.
//!
//! One row is a LIMIT: the `MutexGuard` surface that B7 §3 names as the
//! motivating case cannot be reached by affinity, because `Mutex::lock()`
//! returns `Unknown` (report.txt P132). It goes red the day that resolves.

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

mod common;

static N: AtomicUsize = AtomicUsize::new(0);

fn manitc_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_manitc"))
}

fn check(src: &str) -> (bool, String) {
    let slot = N.fetch_add(1, Ordering::Relaxed);
    let d = common::suite_root("affine").join(slot.to_string());
    std::fs::create_dir_all(&d).expect("temp dir");
    let path = d.join("p.mt");
    std::fs::write(&path, src).expect("write");
    let o = Command::new(manitc_bin())
        .args(["check", path.to_str().unwrap()])
        .output()
        .expect("check");
    let txt =
        String::from_utf8_lossy(&o.stdout).into_owned() + &String::from_utf8_lossy(&o.stderr);
    (o.status.success() && !txt.contains("error:"), txt)
}

/// A fieldless struct, `affine` or not, bound twice.
fn bound_twice(marker: &str) -> String {
    format!(
        "{marker}struct Token {{ }}\n\
         fn mk() -> Token {{ return Token {{ }}; }}\n\
         fn main() {{\n    let t = mk();\n    let a = t;\n    let b = t;\n    \
         io::print_int(1);\n}}\n"
    )
}

// ---------------------------------------------------------------------------
// The marker, against its own control.
// ---------------------------------------------------------------------------

/// **The binding rule is the SAME with the marker and without it, and that is
/// the honest statement of what D-1 bought.** Every user struct is already a
/// move type — `is_move_type` answers `true` for `ManiType::Struct(_, _)`
/// regardless of fields — so an affine binding is refused for a reason that
/// predates the marker entirely.
///
/// Asserting the pair rather than one side is what makes this checkable: a
/// row that only asserted the marked case would pass on a compiler with no
/// marker at all, which is exactly how it was first written here.
#[test]
fn the_binding_rule_is_unchanged_by_the_marker() {
    let (marked, m1) = check(&bound_twice("affine "));
    let (plain, m2) = check(&bound_twice(""));
    assert!(
        !marked && m1.contains("use of moved value"),
        "an affine value bound twice must be refused; got:\n{m1}"
    );
    assert!(
        !plain && m2.contains("use of moved value"),
        "and so must the UNMARKED one — every struct is already a move type. \
         If this now passes, the shape rule changed and the marker has acquired \
         binding-level content it did not have on 4 September 2026:\n{m2}"
    );
}

/// `affine` is contextual, exactly as `const fn` is (B4) and `const N` is (B3).
/// A bare `affine` is still an ordinary identifier, and both meanings coexist
/// in one program.
#[test]
fn affine_is_contextual_and_both_meanings_coexist() {
    let src = "affine struct Token { }\n\
               fn mk() -> Token { return Token { }; }\n\
               fn main() {\n    let affine = 7;\n    let t = mk();\n    let a = t;\n    \
               io::print_int(affine);\n}\n";
    let (ok, msg) = check(src);
    assert!(ok, "`affine` must remain an ordinary identifier; got:\n{msg}");
}

// ---------------------------------------------------------------------------
// D-4 part 3, which this marker is what made reachable.
// ---------------------------------------------------------------------------

/// **B7 D-4 part 3.** Decided 3 September 2026 and unimplementable that day,
/// because it is a rule about affine values and no type could be declared
/// affine. A capture is a COPY (§11.2), and an affine value copied exists
/// twice — which is what affinity forbids.
#[test]
fn spawn_may_not_capture_an_affine_value() {
    let src = "affine struct Token { }\n\
               fn mk() -> Token { return Token { }; }\n\
               fn main() {\n    let t = mk();\n    spawn { let u = t; }\n    \
               io::print_int(1);\n}\n";
    let (ok, msg) = check(src);
    assert!(
        !ok && msg.contains("declared `affine`"),
        "an affine capture must be refused, naming affinity as the reason; got:\n{msg}"
    );
}

/// The pair. Without the marker the same capture is allowed, because a
/// fieldless struct is not an aggregate — so the refusal above is affinity's
/// and not P118's aggregate rule wearing a different message.
#[test]
fn spawn_may_capture_the_same_struct_unmarked() {
    let src = "struct Token { }\n\
               fn mk() -> Token { return Token { }; }\n\
               fn main() {\n    let t = mk();\n    spawn { let u = t; }\n    \
               io::print_int(1);\n}\n";
    let (ok, msg) = check(src);
    assert!(ok, "the unmarked control must still be accepted; got:\n{msg}");
}

// ---------------------------------------------------------------------------
// The limit.
// ---------------------------------------------------------------------------

/// **A STATED LIMIT, 4 September 2026. This row goes red the day it closes.**
///
/// `B7_AFFINE_TYPES.md` §3 names the `MutexGuard` surface as one of the three
/// things waiting on affine types — "a guard that can be copied is a guard
/// that can unlock twice". `stdlib/sync.mt` now declares `MutexGuard` affine,
/// and it changes nothing, because **the guard's type never arrives**:
/// `Mutex::lock()` is a native method on a generic `impl<T>` and its call
/// returns `Unknown`. Affinity is keyed on the type name, and `Unknown` has
/// none.
///
/// Measured the same way P103's field check would be: a nonsense field on the
/// guard is accepted too, which is the independent evidence that the type —
/// and not the marker — is what is missing. Recorded as `report.txt` P132.
#[test]
fn limit_the_mutex_guard_surface_is_type_erased_so_affinity_cannot_reach_it() {
    let src = "use std::sync;\n\
               fn main() {\n    let m = sync::Mutex::new(1);\n    let g = m.lock();\n    \
               let h = g;\n    g.unlock();\n    h.unlock();\n}\n";
    let (ok, msg) = check(src);
    assert!(
        ok,
        "LIMIT CLOSED: the guard's type now resolves and affinity reaches it. \
         `MutexGuard` is already marked `affine` in stdlib/sync.mt, so this is \
         the win B7 §3 was waiting for — update §22, this row, and P132.\n{msg}"
    );
    // The independent evidence that it is the TYPE that is missing: P103's
    // check does not fire either.
    let (ok2, _) = check(
        "use std::sync;\n\
         fn main() {\n    let m = sync::Mutex::new(1);\n    let g = m.lock();\n    \
         io::print_int(g.no_such_field_at_all);\n}\n",
    );
    assert!(
        ok2,
        "if a nonsense field on the guard is now refused, the guard's type \
         resolves and this limit's diagnosis has changed — re-measure P132"
    );
}
