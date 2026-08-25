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

/// The temporary files one compilation needs, removed when it ends.
///
/// EVERYTHING THIS GUARD HOLDS USED TO BE LEFT BEHIND. Nothing removed the
/// compiled runtime object or the extracted runtime sources, so every manitc
/// process left a ~92 KB `.o` in the temp directory and every LLVM compile
/// that could not find the runtime on disk left a directory of seven C files
/// beside it. A 1,147-file corpus sweep leaves 1,147 of each; measured on this
/// machine after a few of them, 80,919 stale objects and 1,819 stale
/// directories — about 7 GB, and 100,000 entries in one directory.
///
/// A GUARD rather than a call at the end of the link, because the link path
/// has several early returns and the leak that only happens when the build
/// FAILS is the one nobody notices.
///
/// Only paths this compiler CREATED are ever registered — `resolve_source`
/// adds the extracted directory and not the repository's own `runtime/`,
/// which it may equally well return and which deleting would be a disaster.
/// That is why registration lives inside `resolve_source` rather than at the
/// call site: a caller cannot tell the two apart from the path alone.
#[derive(Default)]
pub struct Scratch {
    paths: Vec<PathBuf>,
}

impl Scratch {
    pub fn new() -> Scratch {
        Scratch::default()
    }

    /// Remove `path` — a file or a directory — when this guard is dropped.
    /// Returns it, so a call site can wrap the path it was going to use.
    pub fn add(&mut self, path: PathBuf) -> PathBuf {
        self.paths.push(path.clone());
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        for p in &self.paths {
            // Best effort, deliberately: a compilation that cannot tidy up
            // after itself has still compiled, and reporting a failure to
            // remove a temporary file over the top of a successful build
            // would be worse than the leak.
            let _ = if p.is_dir() {
                std::fs::remove_dir_all(p)
            } else {
                std::fs::remove_file(p)
            };
        }
    }
}

/// Locate `manit_runtime.c`, in order of preference:
///   1. next to the source file being compiled (`../runtime/`)
///   2. under the current working directory
///   3. next to the manitc executable (`../runtime/`) — this is the layout
///      the release tarball ships, `bin/manitc` alongside `runtime/`
///   4. failing all that, write the embedded copy to a temporary directory
///
/// Only case 4 is registered with `scratch`: the first three are real source
/// trees that happen to be where the runtime lives.
pub fn resolve_source(
    source_file: Option<&Path>,
    scratch: &mut Scratch,
) -> std::io::Result<PathBuf> {
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
        return Ok(found.clone());
    }
    let written = write_embedded()?;
    if let Some(dir) = written.parent() {
        scratch.add(dir.to_path_buf());
    }
    Ok(written)
}

/// Write the embedded runtime — all seven files — into a temporary directory
/// and return the path to the aggregator. Write errors propagate to the
/// caller: silently returning a path to files that were never written would
/// surface later as a baffling clang error.
///
/// The directory name carries a monotonic nanosecond timestamp in addition to
/// the PID: a recycled PID must not silently reuse a stale directory left by
/// an earlier process (whose runtime files may be from a different manitc
/// version).
fn write_embedded() -> std::io::Result<PathBuf> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!(
        "manit_runtime_{}_{}",
        std::process::id(),
        nanos
    ));
    std::fs::create_dir_all(&dir)?;
    for (name, contents) in RUNTIME_FILES {
        std::fs::write(dir.join(name), contents)?;
    }
    Ok(dir.join("manit_runtime.c"))
}

/// Locate a usable clang executable. `clang` on PATH is preferred; Debian
/// installs versioned names only (`clang-19`), and an LLVM prefix install may
/// not be on PATH at all.
pub fn find_clang() -> Option<String> {
    let candidates = [
        "clang",
        "clang-19",
        "clang-18",
        "clang-17",
        "/usr/lib/llvm-19/bin/clang",
    ];
    candidates
        .iter()
        .find(|c| {
            Command::new(c)
                .arg("--version")
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        })
        .map(|c| c.to_string())
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
