//! Integration tests for the ManiT compiler.
//!
//! Each test compiles a `.mt` source file from `tests/` using the T3ISA backend,
//! runs the resulting `.t3b` binary in the built-in emulator, and asserts that:
//!   - The output is non-empty.
//!   - No output line contains "FAIL".
//!
//! A parallel set of tests does the same via the LLVM backend (compile to native
//! binary, then execute it directly). These are skipped when `clang` is not found
//! on the system PATH.

use std::path::PathBuf;
use std::process::Command;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn get_manitc() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_manitc"))
}

fn test_source(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join(name)
}

/// Unique temp directory per test to allow parallel execution.
fn temp_output(test_file: &str, suffix: &str) -> PathBuf {
    let stem = test_file.replace(".mt", "");
    let dir = std::env::temp_dir().join(format!("manitc_test_{}_{}", suffix, std::process::id()));
    std::fs::create_dir_all(&dir).expect("failed to create temp dir");
    dir.join(stem)
}

// ---------------------------------------------------------------------------
// T3ISA test runner
// ---------------------------------------------------------------------------

fn run_t3_test(test_file: &str) {
    let manitc = get_manitc();
    let source = test_source(test_file);
    let output_base = temp_output(test_file, "t3");

    // Compile to T3ISA
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
        "T3 compilation failed for {}:\nstdout: {}\nstderr: {}",
        test_file,
        String::from_utf8_lossy(&compile.stdout),
        String::from_utf8_lossy(&compile.stderr)
    );

    // Run in emulator
    let t3b_path = output_base.with_extension("t3b");
    let run = Command::new(&manitc)
        .args(["run-t3", t3b_path.to_str().unwrap()])
        .output()
        .expect("failed to run manitc run-t3");

    let stdout = String::from_utf8_lossy(&run.stdout);

    assert!(
        !stdout.is_empty(),
        "No output from {} (exit code: {:?})\nstderr: {}",
        test_file,
        run.status.code(),
        String::from_utf8_lossy(&run.stderr)
    );

    for (i, line) in stdout.lines().enumerate() {
        assert!(
            !line.contains("FAIL"),
            "Test {} FAILED at line {}: {}",
            test_file,
            i + 1,
            line
        );
    }
}

// ---------------------------------------------------------------------------
// LLVM test runner
// ---------------------------------------------------------------------------

fn has_clang() -> bool {
    Command::new("clang")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn run_llvm_test(test_file: &str) {
    if !has_clang() {
        eprintln!("Skipping LLVM test (clang not found): {}", test_file);
        return;
    }

    let manitc = get_manitc();
    let source = test_source(test_file);
    let output_base = temp_output(test_file, "llvm");

    // Compile to native binary via LLVM
    let compile = Command::new(&manitc)
        .args([
            "compile",
            source.to_str().unwrap(),
            "--target",
            "llvm",
            "-o",
            output_base.to_str().unwrap(),
        ])
        .output()
        .expect("failed to run manitc compile (LLVM)");

    assert!(
        compile.status.success(),
        "LLVM compilation failed for {}:\nstdout: {}\nstderr: {}",
        test_file,
        String::from_utf8_lossy(&compile.stdout),
        String::from_utf8_lossy(&compile.stderr)
    );

    // Run the native binary
    let run = Command::new(&output_base)
        .output()
        .expect(&format!("failed to run compiled binary for {}", test_file));

    let stdout = String::from_utf8_lossy(&run.stdout);

    assert!(
        !stdout.is_empty(),
        "No output from {} (LLVM, exit code: {:?})\nstderr: {}",
        test_file,
        run.status.code(),
        String::from_utf8_lossy(&run.stderr)
    );

    for (i, line) in stdout.lines().enumerate() {
        assert!(
            !line.contains("FAIL"),
            "Test {} (LLVM) FAILED at line {}: {}",
            test_file,
            i + 1,
            line
        );
    }
}

// ---------------------------------------------------------------------------
// T3ISA tests
// ---------------------------------------------------------------------------

macro_rules! t3_test {
    ($name:ident, $file:expr) => {
        #[test]
        fn $name() {
            run_t3_test($file);
        }
    };
}

t3_test!(test_01_control_flow_t3, "01_control_flow.mt");
t3_test!(test_02_operators_t3, "02_operators.mt");
t3_test!(test_03_functions_t3, "03_functions.mt");
t3_test!(test_04_structs_enums_t3, "04_structs_enums.mt");
t3_test!(test_05_ternary_types_t3, "05_ternary_types.mt");
t3_test!(test_06_collections_t3, "06_collections.mt");
t3_test!(test_07_error_handling_t3, "07_error_handling.mt");
t3_test!(test_08_pattern_matching_t3, "08_pattern_matching.mt");
t3_test!(test_09_type_casting_t3, "09_type_casting.mt");
t3_test!(test_10_ternary_trie_t3, "10_ternary_trie.mt");
t3_test!(test_11_string_operations_t3, "11_string_operations.mt");
t3_test!(test_12_advanced_closures_t3, "12_advanced_closures.mt");
t3_test!(test_13_numeric_edge_cases_t3, "13_numeric_edge_cases.mt");
t3_test!(test_14_ternary_logic_complete_t3, "14_ternary_logic_complete.mt");
t3_test!(test_15_generics_and_traits_t3, "15_generics_and_traits.mt");
t3_test!(test_16_concurrency_basic_t3, "16_concurrency_basic.mt");

// ---------------------------------------------------------------------------
// LLVM tests (skipped if clang is unavailable)
// ---------------------------------------------------------------------------

macro_rules! llvm_test {
    ($name:ident, $file:expr) => {
        #[test]
        fn $name() {
            run_llvm_test($file);
        }
    };
    ($name:ident, $file:expr, ignore) => {
        #[test]
        #[ignore = "LLVM runtime: struct/enum/collection print not yet functional"]
        fn $name() {
            run_llvm_test($file);
        }
    };
}

llvm_test!(test_01_control_flow_llvm, "01_control_flow.mt");
llvm_test!(test_02_operators_llvm, "02_operators.mt");
llvm_test!(test_03_functions_llvm, "03_functions.mt");
llvm_test!(test_04_structs_enums_llvm, "04_structs_enums.mt", ignore);
llvm_test!(test_05_ternary_types_llvm, "05_ternary_types.mt", ignore);
llvm_test!(test_06_collections_llvm, "06_collections.mt");
llvm_test!(test_07_error_handling_llvm, "07_error_handling.mt", ignore);
llvm_test!(test_08_pattern_matching_llvm, "08_pattern_matching.mt", ignore);
llvm_test!(test_09_type_casting_llvm, "09_type_casting.mt");
llvm_test!(test_10_ternary_trie_llvm, "10_ternary_trie.mt", ignore);
llvm_test!(test_11_string_operations_llvm, "11_string_operations.mt");
llvm_test!(test_12_advanced_closures_llvm, "12_advanced_closures.mt");
llvm_test!(test_13_numeric_edge_cases_llvm, "13_numeric_edge_cases.mt");
llvm_test!(test_14_ternary_logic_complete_llvm, "14_ternary_logic_complete.mt");
llvm_test!(test_15_generics_and_traits_llvm, "15_generics_and_traits.mt", ignore);
llvm_test!(test_16_concurrency_basic_llvm, "16_concurrency_basic.mt", ignore);
