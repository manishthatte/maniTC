//! Negative tests for the ManiT compiler.
//!
//! Each test compiles a `.mt` source file from `tests/negative/` and asserts
//! that compilation FAILS. The first line of each file is a comment of the form:
//!   // EXPECT_ERROR: <substring>
//! The test verifies that the error output contains the expected substring.

use std::path::PathBuf;
use std::process::Command;

fn get_manitc() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_manitc"))
}

fn negative_test_source(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("negative")
        .join(name)
}

/// Run a negative test: compile the file and assert it fails with the expected error.
fn run_negative_test(test_file: &str) {
    let manitc = get_manitc();
    let source_path = negative_test_source(test_file);

    // Read the first line to extract EXPECT_ERROR
    let source = std::fs::read_to_string(&source_path)
        .unwrap_or_else(|e| panic!("cannot read {}: {}", test_file, e));
    let first_line = source.lines().next().unwrap_or("");
    let expected_substr = first_line
        .strip_prefix("// EXPECT_ERROR: ")
        .unwrap_or_else(|| panic!(
            "{}: first line must be '// EXPECT_ERROR: <substring>', got: {}",
            test_file, first_line
        ));

    // Run manitc check
    let output = Command::new(&manitc)
        .args(["check", source_path.to_str().unwrap()])
        .output()
        .expect("failed to run manitc check");

    // Must fail
    assert!(
        !output.status.success(),
        "Expected {} to FAIL compilation, but it succeeded.\nstdout: {}\nstderr: {}",
        test_file,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    // Error output must contain expected substring
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(expected_substr),
        "Expected error output for {} to contain '{}', but got:\n{}",
        test_file, expected_substr, stderr,
    );
}

macro_rules! negative_test {
    ($name:ident, $file:expr) => {
        #[test]
        fn $name() {
            run_negative_test($file);
        }
    };
}

negative_test!(neg_parse_unexpected_token, "err_parse_unexpected_token.mt");
negative_test!(neg_parse_missing_brace, "err_parse_missing_brace.mt");
negative_test!(neg_use_after_move, "err_use_after_move.mt");
negative_test!(neg_double_move, "err_double_move.mt");
negative_test!(neg_non_exhaustive_match_enum, "err_non_exhaustive_match_enum.mt");
negative_test!(neg_non_exhaustive_match_trit, "err_non_exhaustive_match_trit.mt");
