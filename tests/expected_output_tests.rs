//! Expected-output tests for the ManiT compiler.
//!
//! For each test that has a `.expected` file in `tests/expected/`, compile via
//! T3ISA, run in the emulator, and verify the output matches exactly.
//!
//! Also tests cross-target consistency: for tests that pass on both T3 and LLVM
//! backends, verify both produce identical stdout.

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

mod common;

static TEMP_COUNTER: AtomicUsize = AtomicUsize::new(0);

fn get_manitc() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_manitc"))
}

fn test_source(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join(name)
}

fn expected_file(stem: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("expected")
        .join(format!("{}.expected", stem))
}

fn temp_output(stem: &str, suffix: &str) -> PathBuf {
    // A per-call counter so parallel tests never share an output path — but
    // NESTED under one directory per process, not one per call. Flat, this
    // helper alone left 6,170 top-level entries in the temp directory
    // (report.txt P28): a unique PATH is what the tests need, and a unique
    // top-level entry per test is what makes a temp directory of 100,000
    // slow to open.
    let slot = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = common::suite_root("expected")
        .join(format!("{}_{}", suffix, slot));
    std::fs::create_dir_all(&dir).expect("failed to create temp dir");
    dir.join(stem)
}

/// Get T3 output for a test file.
fn get_t3_output(test_file: &str) -> String {
    let manitc = get_manitc();
    let source = test_source(test_file);
    let stem = test_file.replace(".mt", "");
    let output_base = temp_output(&stem, "t3exp");

    let compile = Command::new(&manitc)
        .args(["compile", source.to_str().unwrap(), "--target", "t3", "-o", output_base.to_str().unwrap()])
        .output()
        .expect("failed to compile");
    assert!(compile.status.success(), "T3 compile failed for {}", test_file);

    let t3b = output_base.with_extension("t3b");
    let run = Command::new(&manitc)
        .args(["run-t3", t3b.to_str().unwrap()])
        .output()
        .expect("failed to run T3");

    // Strip the "[T3ISA] running ..." header line from stdout
    let raw = String::from_utf8_lossy(&run.stdout).to_string();
    raw.lines()
        .filter(|l| !l.starts_with("[T3ISA]"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Get LLVM output for a test file (returns None if clang not available).
fn get_llvm_output(test_file: &str) -> Option<String> {
    // Same candidates as runtime_link::find_clang — Debian installs
    // versioned names only (clang-19), possibly outside PATH.
    let has_clang = ["clang", "clang-19", "clang-18", "clang-17", "/usr/lib/llvm-19/bin/clang"]
        .iter()
        .any(|c| {
            Command::new(c)
                .arg("--version")
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        });
    if !has_clang {
        return None;
    }

    let manitc = get_manitc();
    let source = test_source(test_file);
    let stem = test_file.replace(".mt", "");
    let output_base = temp_output(&stem, "llvmexp");

    let compile = Command::new(&manitc)
        .args(["compile", source.to_str().unwrap(), "--target", "llvm", "-o", output_base.to_str().unwrap()])
        .output()
        .expect("failed to compile");

    if !compile.status.success() {
        return None;
    }

    let run = Command::new(&output_base)
        .output()
        .ok()?;

    Some(String::from_utf8_lossy(&run.stdout).to_string())
}

/// Test that T3 output matches the expected file.
fn run_expected_test(test_file: &str) {
    let stem = test_file.replace(".mt", "");
    let expected_path = expected_file(&stem);

    if !expected_path.exists() {
        panic!("No expected file: {}", expected_path.display());
    }

    let expected = std::fs::read_to_string(&expected_path).unwrap();
    let actual = get_t3_output(test_file);

    assert_eq!(
        actual.trim(), expected.trim(),
        "Output mismatch for {} (T3 backend).\n\n--- Expected ---\n{}\n--- Actual ---\n{}",
        test_file, expected.trim(), actual.trim()
    );
}

/// Test that T3 and LLVM produce identical output.
fn run_cross_target_test(test_file: &str) {
    let t3_output = get_t3_output(test_file);
    if let Some(llvm_output) = get_llvm_output(test_file) {
        assert_eq!(
            t3_output.trim(), llvm_output.trim(),
            "Cross-target mismatch for {}.\n\n--- T3 ---\n{}\n--- LLVM ---\n{}",
            test_file, t3_output.trim(), llvm_output.trim()
        );
    }
}

macro_rules! expected_test {
    ($name:ident, $file:expr) => {
        #[test]
        fn $name() {
            run_expected_test($file);
        }
    };
}

macro_rules! cross_target_test {
    ($name:ident, $file:expr) => {
        #[test]
        fn $name() {
            run_cross_target_test($file);
        }
    };
}

// Expected-output tests (T3 output vs .expected file)
expected_test!(expected_01_control_flow, "01_control_flow.mt");
expected_test!(expected_02_operators, "02_operators.mt");
expected_test!(expected_03_functions, "03_functions.mt");
expected_test!(expected_06_collections, "06_collections.mt");
expected_test!(expected_09_type_casting, "09_type_casting.mt");
expected_test!(expected_11_string_operations, "11_string_operations.mt");
expected_test!(expected_12_advanced_closures, "12_advanced_closures.mt");
expected_test!(expected_13_numeric_edge_cases, "13_numeric_edge_cases.mt");
expected_test!(expected_14_ternary_logic_complete, "14_ternary_logic_complete.mt");
expected_test!(expected_17_tcon_tany, "17_tcon_tany.mt");
expected_test!(expected_27_ir_regressions, "27_ir_regressions.mt");
expected_test!(expected_28_regalloc, "28_regalloc.mt");
expected_test!(expected_29_struct_escape, "29_struct_escape.mt");
expected_test!(expected_30_lanewise, "30_lanewise.mt");
expected_test!(expected_31_trit_intrinsics, "31_trit_intrinsics.mt");

// Cross-target consistency (T3 vs LLVM — tests that pass on both)
// 05_ternary_types is excluded: T3 and LLVM print a char-typed value
// differently (emulator vs C runtime %c), a known backend divergence.
// 16_concurrency_basic is excluded: the LLVM binary hangs on the C
// runtime's channel semantics.
cross_target_test!(cross_01_control_flow, "01_control_flow.mt");
cross_target_test!(cross_02_operators, "02_operators.mt");
cross_target_test!(cross_03_functions, "03_functions.mt");
cross_target_test!(cross_04_structs_enums, "04_structs_enums.mt");
cross_target_test!(cross_06_collections, "06_collections.mt");
cross_target_test!(cross_07_error_handling, "07_error_handling.mt");
cross_target_test!(cross_08_pattern_matching, "08_pattern_matching.mt");
cross_target_test!(cross_10_ternary_trie, "10_ternary_trie.mt");
cross_target_test!(cross_09_type_casting, "09_type_casting.mt");
cross_target_test!(cross_11_string_operations, "11_string_operations.mt");
cross_target_test!(cross_12_advanced_closures, "12_advanced_closures.mt");
cross_target_test!(cross_13_numeric_edge_cases, "13_numeric_edge_cases.mt");
cross_target_test!(cross_14_ternary_logic_complete, "14_ternary_logic_complete.mt");
cross_target_test!(cross_15_generics_and_traits, "15_generics_and_traits.mt");
cross_target_test!(cross_17_tcon_tany, "17_tcon_tany.mt");
cross_target_test!(cross_27_ir_regressions, "27_ir_regressions.mt");
cross_target_test!(cross_28_regalloc, "28_regalloc.mt");
cross_target_test!(cross_29_struct_escape, "29_struct_escape.mt");
// C2: the lane-wise family. This one is load-bearing rather than routine —
// LLVM reaches these operators through C runtime calls (manit_lane_*) and T3
// through real instructions, so it is the only test that checks the two
// implementations against each other. It caught an i8 truncation in `tnotw`
// that neither backend revealed alone.
cross_target_test!(cross_30_lanewise, "30_lanewise.mt");
// C7: `trit::sign`/`trit::abs` lower to `IRInstr::TritSign`, which is one TCMP
// on T3 and two comparisons plus a subtract on LLVM — two unrelated
// implementations of one operation, checked against each other here.
cross_target_test!(cross_31_trit_intrinsics, "31_trit_intrinsics.mt");
