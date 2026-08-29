//! Golden-output tests for the shipped examples.
//!
//! `tests/test_all.sh`-style checks and the integration suite only assert that
//! an example compiles, does not TRAP, and exits 0. That is not enough: three
//! examples once printed byte soup for a struct field through every green run,
//! because nothing compared what they actually wrote. Each example here is
//! compiled with the T3ISA backend and its stdout matched against a stored
//! `tests/expected/examples/<name>.expected`.
//!
//! To refresh a golden file after an intentional output change:
//!     ./target/release/manitc compile examples/<name>.mt --target t3 -o /tmp/<name>
//!     ./target/release/manitc run-t3 /tmp/<name>.t3b | grep -v '^\[T3ISA\]' \
//!         > tests/expected/examples/<name>.expected

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static TEMP_COUNTER: AtomicUsize = AtomicUsize::new(0);

fn get_manitc() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_manitc"))
}

fn example_source(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join(format!("{}.mt", name))
}

fn expected_file(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("expected")
        .join("examples")
        .join(format!("{}.expected", name))
}

fn temp_output(name: &str) -> PathBuf {
    // Unique per call, nested under one directory per process — see the note
    // in `expected_output_tests::temp_output` (report.txt P28).
    let slot = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir()
        .join(format!("manitc_example_{}", std::process::id()))
        .join(slot.to_string());
    std::fs::create_dir_all(&dir).expect("failed to create temp dir");
    dir.join(name)
}

fn t3_output(name: &str) -> String {
    let manitc = get_manitc();
    let source = example_source(name);
    let output_base = temp_output(name);

    let compile = Command::new(&manitc)
        .args([
            "compile",
            source.to_str().unwrap(),
            "--target",
            "t3",
            "-o",
            output_base.to_str().unwrap(),
        ])
        .output()
        .expect("failed to run manitc compile");
    assert!(
        compile.status.success(),
        "T3 compile failed for examples/{}.mt:\n{}",
        name,
        String::from_utf8_lossy(&compile.stderr)
    );

    let t3b = output_base.with_extension("t3b");
    assert!(
        t3b.exists(),
        "no .t3b produced for examples/{}.mt (assembler error?)",
        name
    );

    let run = Command::new(&manitc)
        .args(["run-t3", t3b.to_str().unwrap()])
        .output()
        .expect("failed to run the T3 emulator");

    let stdout = String::from_utf8_lossy(&run.stdout).to_string();
    assert!(
        run.status.success(),
        "examples/{}.mt exited {:?}:\nstdout:\n{}\nstderr:\n{}",
        name,
        run.status.code(),
        stdout,
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(
        !stdout.contains("TRAP:"),
        "examples/{}.mt trapped:\n{}",
        name,
        stdout
    );

    stdout
        .lines()
        .filter(|l| !l.starts_with("[T3ISA]"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn run_example_golden(name: &str) {
    let expected_path = expected_file(name);
    let expected = std::fs::read_to_string(&expected_path).unwrap_or_else(|_| {
        panic!(
            "missing golden file {} — see the header of this file to generate it",
            expected_path.display()
        )
    });
    let actual = t3_output(name);

    assert_eq!(
        actual.trim_end(),
        expected.trim_end(),
        "examples/{}.mt output changed.\n\n--- Expected ---\n{}\n\n--- Actual ---\n{}",
        name,
        expected.trim_end(),
        actual.trim_end()
    );
}

macro_rules! example_golden {
    ($test_name:ident, $example:expr) => {
        #[test]
        fn $test_name() {
            run_example_golden($example);
        }
    };
}

example_golden!(example_bridge_demo, "bridge_demo");
example_golden!(example_capability_demo, "capability_demo");
example_golden!(example_concurrency, "concurrency");
example_golden!(example_crypto_demo, "crypto_demo");
example_golden!(example_database, "database");
// data_structures has NO golden entry on purpose, and should get one as soon as
// the bug below is fixed.
//
// Its `Map<str, int>` word-frequency section does not produce stable output: the
// same source compiled to two different -o paths runs to two different results,
// one of them emitting long runs of NUL bytes where the map keys belong. The
// keys are runtime-constructed strings, so this reads like a string address
// escaping into uninitialised memory rather than anything about the map itself.
// A golden file would encode whichever run happened to generate it.
//
// It is also the slowest example by a wide margin — roughly four minutes under
// the debug emulator `cargo test` builds, against about a second in release.
example_golden!(example_fibonacci, "fibonacci");
example_golden!(example_float_demo, "float_demo");
example_golden!(example_hello, "hello");
example_golden!(example_neural_net, "neural_net");
example_golden!(example_oop, "oop");
example_golden!(example_patent_classify, "patent_classify");
example_golden!(example_stream_demo, "stream_demo");
example_golden!(example_ternary_calculator, "ternary_calculator");
example_golden!(example_ternary_demo, "ternary_demo");
example_golden!(example_ternary_sort, "ternary_sort");
example_golden!(example_three_valued_logic, "three_valued_logic");
