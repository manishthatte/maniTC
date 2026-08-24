//! Replay the fuzzing corpus and every recorded crash, on stable.  (F-8)
//!
//! © Manish Jagdish Thatte
//!
//! Coverage-guided fuzzing needs a nightly toolchain and libFuzzer. This does
//! not: it walks `fuzz/corpus/` and `fuzz/artifacts/` and pushes every file
//! through the same pipeline the fuzz targets use, asserting only that the
//! compiler returns rather than panics.
//!
//! The division of labour matters. The fuzzer FINDS crashes; this file is what
//! stops a found crash coming back. Once cargo-fuzz writes a reproducer into
//! `fuzz/artifacts/`, that file is committed and every subsequent `cargo test`
//! replays it — so the finding survives the toolchain that found it, survives
//! CI not having nightly, and survives nobody remembering to re-run the
//! fuzzer. A fuzz finding that is not replayable is a finding you get to make
//! twice.

use std::path::{Path, PathBuf};

/// Every file in a directory under `fuzz/`, if it exists.
fn corpus_files(subdir: &str) -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("fuzz").join(subdir);
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(&root) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let Ok(inner) = std::fs::read_dir(&path) else { continue };
            for f in inner.flatten() {
                if f.path().is_file() {
                    out.push(f.path());
                }
            }
        } else if path.is_file() {
            out.push(path);
        }
    }
    out.sort();
    out
}

/// The pipeline the fuzz targets drive, to the same depth, on a stack big
/// enough for the parser's depth guard to be reachable.
///
/// Every stage is allowed to return an error; none is allowed to panic. The
/// function deliberately swallows `Err` and returns `()` — the assertion is
/// "we got here", and a compiler that rejects malformed input is behaving.
///
/// `with_compiler_stack` is not incidental. Without it this harness aborts on
/// 2048-deep nesting that `manitc check` rejects cleanly, because the depth
/// limit is only enforceable on a stack deep enough to reach it — which is the
/// first thing this corpus found, and the reason the reservation moved out of
/// `main` and into the library.
fn drive_front_end(src: &str) {
    let owned = src.to_string();
    manitc::with_compiler_stack(move || drive_front_end_inner(&owned));
}

/// Drive many inputs on ONE reserved stack.
///
/// `with_compiler_stack` spawns a thread, and a thread per input costs more
/// than the compilation does — the corpus is thousands of files after a
/// campaign. One thread for the whole batch is the same guarantee at a
/// fraction of the price.
fn drive_all(sources: impl IntoIterator<Item = String>) {
    let batch: Vec<String> = sources.into_iter().collect();
    manitc::with_compiler_stack(move || {
        for src in &batch {
            drive_front_end_inner(src);
        }
    });
}

fn drive_front_end_inner(src: &str) {
    let Ok(tokens) = manitc::lexer::Lexer::with_file(src, "<corpus>").tokenize() else {
        return;
    };
    let Ok(program) = manitc::parser::Parser::with_file(tokens, "<corpus>").parse() else {
        return;
    };
    let mut analyzer = manitc::semantic::SemanticAnalyzer::with_file("<corpus>");
    let Ok(typed) = analyzer.analyze(&program) else {
        return;
    };
    if manitc::borrow::check_borrows(&typed).is_err() {
        return;
    }
    let mut module = manitc::ir::IRLowerer::lower(&typed);
    manitc::ir::optimize::run_passes(&mut module);
}

/// Every shipped `.mt` source, which is what `fuzz/seed_corpus.sh` seeds from.
///
/// Read directly rather than through `fuzz/corpus/` so this test has no
/// ordering dependency on the seed script and no dependency on a working
/// corpus being committed — `fuzz/corpus/` is gitignored, because a short
/// campaign fills it with thousands of mutated files.
fn shipped_sources() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut out = Vec::new();
    for sub in ["examples", "stdlib"] {
        let Ok(entries) = std::fs::read_dir(root.join(sub)) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "mt") {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

#[test]
fn the_seed_corpus_does_not_panic_the_front_end() {
    // Shipped sources only. `fuzz/corpus/` is the FUZZER's memory — after a
    // campaign it holds thousands of mutated inputs, it is gitignored, and
    // replaying all of it turned a 6-second test into a 60-second one for no
    // extra regression cover: the inputs that matter are the ones that
    // crashed, and those live in `artifacts/` and are replayed separately.
    let files = shipped_sources();
    assert!(
        !files.is_empty(),
        "no sources found — examples/ and stdlib/ should always be present"
    );
    drive_all(files.into_iter().filter_map(|p| {
        std::fs::read(&p).ok().and_then(|b| String::from_utf8(b).ok())
    }));
}

/// Every reproducer cargo-fuzz has ever written, replayed.
///
/// Passes vacuously while `fuzz/artifacts/` is empty, which is the correct
/// behaviour for a regression file with no regressions yet — and it means the
/// first crash found needs no new test written, only the artifact committed.
#[test]
fn recorded_fuzz_crashes_stay_fixed() {
    drive_all(corpus_files("artifacts").into_iter().filter_map(|p| {
        std::fs::read(&p).ok().and_then(|b| String::from_utf8(b).ok())
    }));
}

/// Structured inputs that have historically broken the front end, kept inline
/// so they run even with no corpus on disk.
///
/// Both entries are section 48's shape — a wide parameter list, and deep
/// nesting — because they are the two the recorded defect history names and
/// the two a fuzzer reaches first.
#[test]
fn pathological_shapes_are_rejected_not_fatal() {
    // Section 48: the T3 emitter panicked on a 13-parameter function. The
    // front end must survive far more than 13.
    let params: Vec<String> = (0..64).map(|i| format!("p{}: int", i)).collect();
    drive_front_end(&format!("fn wide({}) {{ }}\nfn main() {{ }}\n", params.join(", ")));

    // Deep nesting, past MAX_PARSE_DEPTH: a diagnostic, never a stack
    // overflow. The guard exists; this is what checks it still works.
    let deep = format!(
        "fn main() {{ let x: int = {}1{}; }}\n",
        "(".repeat(2048),
        ")".repeat(2048)
    );
    drive_front_end(&deep);

    // Unterminated and truncated forms, which is what a mutation of a real
    // file most often produces.
    for src in [
        "fn main() {",
        "fn main() { let x: int = ",
        "extern \"c\" fn io::println(s: str)",
        "lint deny(",
        "fn f<T: ",
        "struct S { pub x:",
        "\u{0}\u{1}\u{2}",
    ] {
        drive_front_end(src);
    }
}
