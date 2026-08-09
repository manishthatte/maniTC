//! Locating the ManiT C runtime, and working out how to link against it.
//!
//! The runtime aggregates seven C files and builds in one of two configurations:
//!
//!   full     — SDL2 and libcurl are present; the `gui` and `net` modules are
//!              compiled in, and the final link needs their libraries
//!   minimal  — `-DMANIT_NO_GUI`; everything else in the runtime still works
//!
//! Which one is used is decided by probing pkg-config, so a machine without
//! SDL2 installed still compiles and links ordinary programs instead of
//! failing at the link step with undefined SDL_* references.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The complete runtime, embedded. `manit_runtime.c` is only an aggregator —
/// it `#include`s the other six — so all of them have to be written out
/// together for a manitc binary distributed without its source tree to work.
const RUNTIME_FILES: [(&str, &str); 7] = [
    ("manit_runtime.c", include_str!("../runtime/manit_runtime.c")),
    ("core.c", include_str!("../runtime/core.c")),
    ("collections.c", include_str!("../runtime/collections.c")),
    ("sync.c", include_str!("../runtime/sync.c")),
    ("system.c", include_str!("../runtime/system.c")),
    ("net.c", include_str!("../runtime/net.c")),
    ("gui.c", include_str!("../runtime/gui.c")),
];

/// Extra flags for the runtime compile and the final link.
pub struct LinkFlags {
    /// Passed to `clang -c manit_runtime.c`
    pub cflags: Vec<String>,
    /// Passed to the final `clang <prog>.ll manit_runtime.o -o <prog>`
    pub libs: Vec<String>,
}

/// Locate `manit_runtime.c`, in order of preference:
///   1. next to the source file being compiled (`../runtime/`)
///   2. under the current working directory
///   3. next to the manitc executable (`../runtime/`) — this is the layout
///      the release tarball ships, `bin/manitc` alongside `runtime/`
///   4. failing all that, write the embedded copy to a temporary directory
pub fn resolve_source(source_file: Option<&Path>) -> PathBuf {
    let candidates = [
        source_file
            .and_then(|f| f.parent())
            .map(|p| p.join("../runtime/manit_runtime.c"))
            .unwrap_or_default(),
        PathBuf::from("runtime/manit_runtime.c"),
        std::env::current_exe()
            .ok()
            .and_then(|e| e.parent().map(|p| p.join("../runtime/manit_runtime.c")))
            .unwrap_or_default(),
    ];

    if let Some(found) = candidates.iter().find(|p| p.exists()) {
        return found.clone();
    }
    write_embedded()
}

/// Write the embedded runtime — all seven files — into a temporary directory
/// and return the path to the aggregator.
fn write_embedded() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("manit_runtime_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    for (name, contents) in RUNTIME_FILES {
        let _ = std::fs::write(dir.join(name), contents);
    }
    dir.join("manit_runtime.c")
}

/// Where to put the compiled runtime object. Keyed by process id so that
/// concurrent compilations — the test suite runs many at once — do not
/// overwrite one another's object file.
pub fn object_path(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("manit_runtime_{}_{}.o", tag, std::process::id()))
}

/// Decide between the full and minimal runtime configurations.
///
/// `MANIT_NO_GUI=1` in the environment forces minimal without probing, which
/// is useful for reproducible builds and for shell/TUI-only targets.
pub fn flags() -> LinkFlags {
    if std::env::var("MANIT_NO_GUI").is_ok() {
        return minimal();
    }
    match probe_pkg_config() {
        Some(f) => f,
        None => minimal(),
    }
}

fn minimal() -> LinkFlags {
    LinkFlags {
        cflags: vec!["-DMANIT_NO_GUI".to_string()],
        libs: Vec::new(),
    }
}

/// Ask pkg-config for SDL2, SDL2_ttf and libcurl together. All three are
/// needed for the full runtime; if any is missing we fall back to minimal.
fn probe_pkg_config() -> Option<LinkFlags> {
    let packages = ["sdl2", "SDL2_ttf", "libcurl"];

    let cflags = pkg_config(&["--cflags"], &packages)?;
    let libs = pkg_config(&["--libs"], &packages)?;

    Some(LinkFlags { cflags, libs })
}

fn pkg_config(mode: &[&str], packages: &[&str]) -> Option<Vec<String>> {
    let output = Command::new("pkg-config")
        .args(mode)
        .args(packages)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }
    Some(
        String::from_utf8_lossy(&output.stdout)
            .split_whitespace()
            .map(str::to_string)
            .collect(),
    )
}
