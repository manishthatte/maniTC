//! Shared scratch-directory handling for the test suites.
//!
//! © Manish Jagdish Thatte
//!
//! **P115.** Every suite used to build its own root directly under
//! `std::env::temp_dir()` — `manitc_audit_regr_<pid>`, `manitc_sw_<pid>`,
//! thirty distinct prefixes — and **nothing removed any of them, ever**. One
//! `cargo test` leaves roughly a hundred and twenty inodes per suite behind,
//! which is invisible until it is not: measured on 3 September 2026,
//! **6,792 such directories held 844,623 inodes** and `/var/scratch` refused
//! to write with 926 GB of space free and fourteen inodes left. A volume that
//! reports 60 % free and cannot create a file is a confusing way to fail, and
//! it cost a session's measurements.
//!
//! Two changes, and the second is the one that bounds it:
//!
//! **One parent.** Every root now lives under `manitc-tests/`, so the whole of
//! it is one path to sweep, to inspect, or to delete by hand — rather than
//! thirty prefixes nobody can enumerate from memory.
//!
//! **A sweep keyed on PROCESS LIVENESS, not on age.** The first call in a
//! process removes every sibling root whose pid is no longer running. That is
//! what makes the steady state one run's worth instead of every run ever: a
//! finished `cargo test` leaves roots whose processes are gone, and the next
//! run — seconds later or a week later — removes them. An age threshold was
//! the obvious alternative and is worse, because it has to be longer than the
//! slowest suite and so cannot clean up after a run that just finished.
//!
//! **The conservative direction is deliberate.** Liveness is read from
//! `/proc`, and where it cannot be read *every* pid is reported live, so
//! nothing is swept. A pid that has been REUSED likewise reports live and its
//! root is left alone. Both errors leak; neither deletes a directory a
//! running suite is using. A missed cleanup is a disk that fills eventually,
//! a wrongful one is another suite failing now, and those are not the same
//! size of mistake.
//!
//! Removal is deliberately NOT tied to a test finishing. Several suites
//! return a path that lives inside the scratch directory — `compile_llvm`
//! hands back the binary it just built — so a guard dropped at the end of the
//! helper would delete the artefact its caller is about to run. Sweeping at
//! the start of the NEXT process cannot have that problem by construction.
//!
//! `tests/common/mod.rs` is not a test target: cargo builds only the direct
//! `.rs` children of `tests/`.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::Once;

/// The single directory under `temp_dir()` that all suite scratch lives in.
///
/// `audit_regression_tests::every_suite_uses_the_shared_scratch_root` asserts
/// that no suite bypasses it, because a registry that must agree with another
/// gets a test rather than a comment (permanent rule 5).
pub const PARENT: &str = "manitc-tests";

static SWEEP: Once = Once::new();

/// The directory every suite root lives under.
///
/// Exists so that nothing outside this module needs `std::env::temp_dir()` —
/// which is what `every_suite_uses_the_shared_scratch_root` checks for, and
/// the row found its own first draft reaching past the module to build this
/// path by hand.
pub fn parent_dir() -> PathBuf {
    std::env::temp_dir().join(PARENT)
}

/// This process's scratch root for `suite`, created.
///
/// `suite` is a short name — `sw`, `p94`, `audit_regr` — and appears in the
/// directory name, so a leftover root still says which suite produced it.
pub fn suite_root(suite: &str) -> PathBuf {
    let root = parent_dir().join(format!("{}-{}", suite, std::process::id()));

    SWEEP.call_once(|| {
        sweep_dead_roots();
        // Our own pid may be a REUSED one, in which case anything already
        // under this name belongs to a process that is gone and its files
        // would be read as ours. The sweep above cannot remove it — it asks
        // whether the pid is live, and ours is.
        let _ = std::fs::remove_dir_all(&root);
    });

    std::fs::create_dir_all(&root).expect("failed to create scratch dir");
    root
}

/// Remove the roots of test processes that are no longer running.
///
/// `pub` so `audit_regression_tests` can assert what it does, rather than
/// describing it: the sweep is the half of P115 that bounds the growth, and a
/// comment claiming a directory is removed is exactly the shape this
/// repository keeps finding to be false.
pub fn sweep_dead_roots() {
    let parent = parent_dir();
    let entries = match std::fs::read_dir(&parent) {
        Ok(e) => e,
        Err(_) => return, // first run on this machine, or not ours to read
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = match name.to_str() {
            Some(n) => n,
            None => continue,
        };
        // `<suite>-<pid>`; a suite name may itself contain `-`, so split from
        // the right.
        let pid = match name.rsplit_once('-').and_then(|(_, p)| p.parse::<u32>().ok()) {
            Some(p) => p,
            None => continue, // not a name this module wrote — leave it
        };
        if pid_is_live(pid) {
            continue;
        }
        let _ = std::fs::remove_dir_all(entry.path());
    }
}

/// Whether a process with this pid is running.
///
/// Answers **true** when it cannot tell — no `/proc` — so a platform this was
/// not written for sweeps nothing rather than sweeping a live suite's work.
fn pid_is_live(pid: u32) -> bool {
    if !Path::new("/proc").is_dir() {
        return true;
    }
    Path::new(&format!("/proc/{}", pid)).exists()
}
