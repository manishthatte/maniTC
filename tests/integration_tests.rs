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

mod common;

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
    // The suffix is part of the SUITE name rather than of the path below it,
    // because it is what makes two backends' outputs for one program distinct.
    // `suite_root` splits the pid off from the right, so a suite name with a
    // `-` in it is fine.
    let dir = common::suite_root(&format!("integration-{}", suffix));
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
    // Same candidates as runtime_link::find_clang — Debian installs
    // versioned names only (clang-19), possibly outside PATH.
    ["clang", "clang-19", "clang-18", "clang-17", "/usr/lib/llvm-19/bin/clang"]
        .iter()
        .any(|c| {
            Command::new(c)
                .arg("--version")
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        })
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
        #[ignore = "LLVM runtime: C-runtime thread/channel semantics hang this test"]
        fn $name() {
            run_llvm_test($file);
        }
    };
}

llvm_test!(test_01_control_flow_llvm, "01_control_flow.mt");
llvm_test!(test_02_operators_llvm, "02_operators.mt");
llvm_test!(test_03_functions_llvm, "03_functions.mt");
llvm_test!(test_04_structs_enums_llvm, "04_structs_enums.mt");
llvm_test!(test_05_ternary_types_llvm, "05_ternary_types.mt");
llvm_test!(test_06_collections_llvm, "06_collections.mt");
llvm_test!(test_07_error_handling_llvm, "07_error_handling.mt");
llvm_test!(test_08_pattern_matching_llvm, "08_pattern_matching.mt");
llvm_test!(test_09_type_casting_llvm, "09_type_casting.mt");
llvm_test!(test_10_ternary_trie_llvm, "10_ternary_trie.mt");
llvm_test!(test_11_string_operations_llvm, "11_string_operations.mt");
llvm_test!(test_12_advanced_closures_llvm, "12_advanced_closures.mt");
llvm_test!(test_13_numeric_edge_cases_llvm, "13_numeric_edge_cases.mt");
llvm_test!(test_14_ternary_logic_complete_llvm, "14_ternary_logic_complete.mt");
llvm_test!(test_15_generics_and_traits_llvm, "15_generics_and_traits.mt");
llvm_test!(test_16_concurrency_basic_llvm, "16_concurrency_basic.mt");

// ---------------------------------------------------------------------------
// LLVM example matrix (K1): every example that the LLVM backend supports must
// compile to a native binary, run to completion, and exit 0. Skipped when
// clang is unavailable. bridge_demo and float_demo are absent: their stdlib
// module function bodies (bridge::*, t27f::*) are not lowered into the
// program on either backend (T3 fails identically with undefined labels).
// concurrency, crypto_demo and patent_classify fail in the front-end /
// semantic phase on both backends.
// ---------------------------------------------------------------------------

fn run_llvm_example(example: &str, expect: &str) {
    if !has_clang() {
        eprintln!("Skipping LLVM example test (clang not found): {}", example);
        return;
    }

    let manitc = get_manitc();
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join(format!("{}.mt", example));
    let output_base = temp_output(&format!("{}.mt", example), "llvmex");

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
        "LLVM compilation failed for example {}:\nstdout: {}\nstderr: {}",
        example,
        String::from_utf8_lossy(&compile.stdout),
        String::from_utf8_lossy(&compile.stderr)
    );

    let run = Command::new(&output_base)
        .output()
        .expect("failed to run compiled example binary");
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert_eq!(
        run.status.code(),
        Some(0),
        "example {} (LLVM) must exit 0, got {:?}\nstderr: {}",
        example,
        run.status.code(),
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(
        stdout.contains(expect),
        "example {} (LLVM) output must contain {:?}, got:\n{}",
        example,
        expect,
        stdout
    );
}

macro_rules! llvm_example_test {
    ($name:ident, $example:expr, $expect:expr) => {
        #[test]
        fn $name() {
            run_llvm_example($example, $expect);
        }
    };
}

llvm_example_test!(example_capability_demo_llvm, "capability_demo", "Demo complete.");
llvm_example_test!(example_database_llvm, "database", "Demo complete.");
llvm_example_test!(example_data_structures_llvm, "data_structures", "All demos complete.");
llvm_example_test!(example_fibonacci_llvm, "fibonacci", "Done.");
llvm_example_test!(example_hello_llvm, "hello", "Done.");
llvm_example_test!(example_neural_net_llvm, "neural_net", "Demo complete.");
llvm_example_test!(example_oop_llvm, "oop", "All demos complete.");
llvm_example_test!(example_patent_classify_llvm, "patent_classify", "R1=-1 -> tryte=-1");
llvm_example_test!(example_stream_demo_llvm, "stream_demo", "=== Stream demo complete ===");
llvm_example_test!(example_ternary_calculator_llvm, "ternary_calculator", "Demo complete.");
llvm_example_test!(example_ternary_demo_llvm, "ternary_demo", "Demo complete.");
llvm_example_test!(example_ternary_sort_llvm, "ternary_sort", "Demo complete.");
llvm_example_test!(example_three_valued_logic_llvm, "three_valued_logic", "Demo complete.");

// Value pins for LLVM example behavior fixed in this campaign:
// Err payloads must print their message, not (null) (Result layout fix),
// and pack/unpack round-trips must survive (internal ternary helpers).
#[test]
fn example_fibonacci_llvm_err_payload_prints() {
    if !has_clang() {
        return;
    }
    let manitc = get_manitc();
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("fibonacci.mt");
    let output_base = temp_output("fibonacci_err.mt", "llvmex");
    let compile = Command::new(&manitc)
        .args([
            "compile", source.to_str().unwrap(),
            "--target", "llvm",
            "-o", output_base.to_str().unwrap(),
        ])
        .output()
        .expect("failed to compile fibonacci (LLVM)");
    assert!(compile.status.success());
    let run = Command::new(&output_base).output().expect("failed to run");
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(
        stdout.contains("Err(\"overflow: fib(n) exceeds the 27-trit word range for n > 61\")"),
        "Err payload must print its message (was (null) before the Result \
         layout fix), got:\n{}",
        stdout
    );
    assert!(!stdout.contains("(null)"), "no Result payload may print as (null):\n{}", stdout);
}

// ---------------------------------------------------------------------------
// K8: using gui/net with a minimal runtime (-DMANIT_NO_GUI) must produce a
// clear diagnostic naming the missing packages, not raw undefined symbols.
// The check runs before clang is even invoked, so no clang is needed.
// ---------------------------------------------------------------------------

#[test]
fn minimal_runtime_gui_net_use_gets_clear_diagnostic() {
    let src = write_temp_mt(
        "net_user.mt",
        "fn main() {\n    let body = net_http_get(\"http://example.com/\");\n    io::println(body);\n}\n",
    );
    let output_base = temp_output("net_user.mt", "k8");
    let run = Command::new(get_manitc())
        .env("MANIT_NO_GUI", "1")
        .args([
            "compile", src.to_str().unwrap(),
            "--target", "llvm",
            "-o", output_base.to_str().unwrap(),
        ])
        .output()
        .expect("failed to run manitc compile");
    assert!(
        !run.status.success(),
        "compiling a net-using program against the minimal runtime must fail"
    );
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.contains("libcurl") && stderr.contains("net"),
        "diagnostic must name the missing package and module, got:\n{}",
        stderr
    );
}

fn write_temp_mt(name: &str, source: &str) -> PathBuf {
    let dir = common::suite_root("runt3");
    std::fs::create_dir_all(&dir).expect("failed to create temp dir");
    let path = dir.join(name);
    std::fs::write(&path, source).expect("failed to write temp source");
    path
}

#[test]
fn run_t3_on_mt_source_auto_compiles_and_runs() {
    let src = write_temp_mt("auto_compile.mt", "fn main() { io::println(\"auto-compiled ok\"); }\n");
    let run = Command::new(get_manitc())
        .args(["run-t3", src.to_str().unwrap()])
        .output()
        .expect("failed to run manitc run-t3");
    let stdout = String::from_utf8_lossy(&run.stdout);
    // The old behavior executed the raw source bytes and silently exited 0
    // with no output — the program's actual output must appear now.
    assert!(
        stdout.contains("auto-compiled ok"),
        "run-t3 on a .mt source must auto-compile and run it, got stdout:\n{}\nstderr:\n{}",
        stdout,
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(run.status.success());
}

#[test]
fn run_t3_on_non_binary_errors_clearly() {
    let junk = write_temp_mt("not_a_binary.txt", "this is not a T3 binary\n");
    let run = Command::new(get_manitc())
        .args(["run-t3", junk.to_str().unwrap()])
        .output()
        .expect("failed to run manitc run-t3");
    assert!(
        !run.status.success(),
        "run-t3 on a non-binary must exit nonzero (silent exit 0 was B22)"
    );
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.contains("not a T3ISA binary"),
        "expected a clear diagnostic, got:\n{}", stderr
    );
}

#[test]
fn run_t3_propagates_main_exit_status() {
    let src = write_temp_mt("exit_status.mt", "fn main() -> int { return 42; }\n");
    let run = Command::new(get_manitc())
        .args(["run-t3", src.to_str().unwrap()])
        .output()
        .expect("failed to run manitc run-t3");
    assert_eq!(
        run.status.code(),
        Some(42),
        "main's return value must become the process exit status (K6)"
    );

    let src0 = write_temp_mt("exit_status0.mt", "fn main() { io::println(\"bye\"); }\n");
    let run0 = Command::new(get_manitc())
        .args(["run-t3", src0.to_str().unwrap()])
        .output()
        .expect("failed to run manitc run-t3");
    assert_eq!(run0.status.code(), Some(0), "void main must exit 0");
}

#[test]
fn run_t3_survives_closed_stdout_pipe() {
    // `manitc run-t3 <bin> | head -1` must exit quietly on SIGPIPE (K7).
    let src = write_temp_mt(
        "pipey.mt",
        "fn main() { let mut i = 0; while i < 500 { io::println(\"line\"); i = i + 1; } }\n",
    );
    let sh = Command::new("sh")
        .arg("-c")
        .arg(format!(
            "{} run-t3 {} | head -1",
            get_manitc().display(),
            src.display()
        ))
        .output()
        .expect("failed to run shell pipeline");
    let stderr = String::from_utf8_lossy(&sh.stderr);
    assert!(
        !stderr.contains("panicked"),
        "run-t3 must not panic on a closed stdout pipe, stderr:\n{}", stderr
    );
    assert!(sh.status.success(), "pipeline `run-t3 | head -1` must exit 0");
}

// ---------------------------------------------------------------------------
// oop.mt on T3 (K4): the monomorphized float labels (float_Point::zero_0)
// must assemble, and the example must run to completion.
// ---------------------------------------------------------------------------

#[test]
fn example_oop_compiles_and_runs_on_t3() {
    let manitc = get_manitc();
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples").join("oop.mt");
    let output_base = temp_output("oop_example.mt", "t3oop");

    let compile = Command::new(&manitc)
        .args([
            "compile", source.to_str().unwrap(),
            "--target", "t3",
            "-o", output_base.to_str().unwrap(),
        ])
        .output()
        .expect("failed to run manitc compile");
    let compile_err = String::from_utf8_lossy(&compile.stderr);
    assert!(
        compile.status.success() && !compile_err.contains("assembler error"),
        "oop.mt must compile for T3 without assembler errors (K4), stderr:\n{}", compile_err
    );

    let t3b = output_base.with_extension("t3b");
    let run = Command::new(&manitc)
        .args(["run-t3", t3b.to_str().unwrap()])
        .output()
        .expect("failed to run manitc run-t3");
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(
        stdout.contains("All demos complete."),
        "oop.mt must run to completion on T3, got:\n{}", stdout
    );
    assert!(!stdout.contains("TRAP"), "oop.mt run hit a TRAP:\n{}", stdout);
    // Pin a couple of stable computed values (robust to unrelated changes).
    assert!(stdout.contains("area = 12"), "expected `area = 12` in:\n{}", stdout);
    assert!(stdout.contains("perimeter = 14"), "expected `perimeter = 14` in:\n{}", stdout);
}

// ---------------------------------------------------------------------------
// Large literals (B4/B5): the full emit_lit split path, end to end.
// ---------------------------------------------------------------------------

#[test]
fn large_literals_roundtrip_through_t3() {
    // 796_162 and 797_161 sit at the TLIT wide-imm boundary; the larger
    // values force the (recursive) hi*1000+lo split.
    let src = write_temp_mt(
        "biglits.mt",
        "fn main() {\n\
           io::println_int(796162);\n\
           io::println_int(797161);\n\
           io::println_int(-797161);\n\
           io::println_int(797162);\n\
           io::println_int(800000000);\n\
           io::println_int(-800000000);\n\
           io::println_int(800000000000);\n\
           io::println_int(3812798742493);\n\
         }\n",
    );
    let run = Command::new(get_manitc())
        .args(["run-t3", src.to_str().unwrap()])
        .output()
        .expect("failed to run manitc run-t3");
    let stdout = String::from_utf8_lossy(&run.stdout);
    for expect in [
        "796162", "797161", "-797161", "797162",
        "800000000", "-800000000", "800000000000", "3812798742493",
    ] {
        assert!(
            stdout.lines().any(|l| l.trim() == expect),
            "expected literal {} in run-t3 output:\n{}", expect, stdout
        );
    }
}
