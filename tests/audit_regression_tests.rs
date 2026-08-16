//! Second-pass audit regression tests (report.txt section 9, findings A1-A18).
//!
//! Section 9 attacked a different axis from the earlier campaign: what the
//! compiler does with ill-formed, adversarial or merely unusual input. Every
//! test here pins one such fix, so the crash/unsafety classes stay closed.
//!
//! Author: Manish Jagdish Thatte

use std::path::PathBuf;
use std::process::Command;

fn get_manitc() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_manitc"))
}

fn temp_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("manitc_audit_regr_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("failed to create temp dir");
    dir
}

/// Write `source` to a temp .mt file and return its path.
fn write_source(name: &str, source: &str) -> PathBuf {
    let path = temp_dir().join(name);
    std::fs::write(&path, source).expect("failed to write test source");
    path
}

/// Run `manitc <args...>` and return (success, stdout, stderr).
fn run_manitc(args: &[&str]) -> (bool, String, String) {
    let output = Command::new(get_manitc())
        .args(args)
        .output()
        .expect("failed to run manitc");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Assert `manitc check <file>` fails with an error mentioning `expect_substr`.
fn assert_check_error(name: &str, source: &str, expect_substr: &str) {
    let path = write_source(name, source);
    let (ok, stdout, stderr) = run_manitc(&["check", path.to_str().unwrap()]);
    assert!(!ok, "{} should FAIL to check, but succeeded:\n{}", name, stdout);
    let all = format!("{}{}", stdout, stderr);
    assert!(
        all.contains(expect_substr),
        "{}: expected an error containing '{}', got:\n{}",
        name, expect_substr, all,
    );
}

/// Assert `manitc check <file>` succeeds.
fn assert_checks(name: &str, source: &str) -> String {
    let path = write_source(name, source);
    let (ok, stdout, stderr) = run_manitc(&["check", path.to_str().unwrap()]);
    assert!(ok, "{} should check cleanly, but failed:\n{}{}", name, stdout, stderr);
    format!("{}{}", stdout, stderr)
}

// ---------------------------------------------------------------------------
// A1 — a non-void function must supply a value on every path
// ---------------------------------------------------------------------------
// Falling off the end left the return slot uninitialised: `-> int` silently
// produced 0, `-> str` produced an uninitialised pointer that the print path
// dereferenced (T3 printed raw memory bytes, LLVM printed "(null)").

#[test]
fn a1_missing_return_on_int_path_is_an_error() {
    assert_check_error(
        "a1_int.mt",
        "fn nofinal(x: int) -> int { if x > 0 { return 111; } }\nfn main() {}\n",
        "can finish without returning a value",
    );
}

#[test]
fn a1_missing_return_on_str_path_is_an_error() {
    // The memory-unsafe case: an uninitialised pointer reaches io::println.
    assert_check_error(
        "a1_str.mt",
        "fn nostr(x: int) -> str { if x > 0 { return \"yes\"; } }\nfn main() {}\n",
        "can finish without returning a value",
    );
}

#[test]
fn a1_empty_body_with_return_type_is_an_error() {
    assert_check_error(
        "a1_empty.mt",
        "fn f() -> int { }\nfn main() {}\n",
        "can finish without returning a value",
    );
}

#[test]
fn a1_while_loop_does_not_count_as_returning() {
    // A `while` may execute zero times, so it guarantees nothing.
    assert_check_error(
        "a1_while.mt",
        "fn f(n: int) -> int { while n > 0 { return 1; } }\nfn main() {}\n",
        "can finish without returning a value",
    );
}

#[test]
fn a1_if_without_else_does_not_count_as_returning() {
    assert_check_error(
        "a1_noelse.mt",
        "fn f(n: int) -> int { if n > 0 { return 1; } elif n < 0 { return 2; } }\nfn main() {}\n",
        "can finish without returning a value",
    );
}

/// Every shape that genuinely does supply a value must keep compiling. These
/// are the false-positive guards: the check is worthless if it rejects real
/// code, and all 17 examples plus 55 thatteos sources rely on these forms.
#[test]
fn a1_all_returning_shapes_are_accepted() {
    assert_checks(
        "a1_ok.mt",
        r#"
fn tail(x: int) -> int { x + 1 }
fn both(x: int) -> int { if x > 0 { return 1; } else { return 2; } }
fn tail_if(x: int) -> int { if x > 0 { 1 } else { 2 } }
fn via_tif(t: trit) -> int { tif t { + => return 1, 0 => return 0, - => return -1, } }
fn forever() -> int { loop { io::println("x"); } }
fn exits(x: int) -> int { if x > 0 { return 1; } env::exit(2); }
fn matched(x: int) -> str { match x { 0 => "zero", _ => "other", } }
fn elif_chain(x: int) -> int {
    if x > 0 { return 1; } elif x < 0 { return 2; } else { return 3; }
}
fn nested(x: int) -> int {
    if x > 0 { if x > 5 { return 2; } else { return 1; } } else { return 0; }
}
fn void_needs_nothing(x: int) { if x > 0 { io::println("hi"); } }
fn main() {
    io::println_int(tail(1) + both(1) + tail_if(1) + via_tif(+) + elif_chain(1) + nested(9));
    io::println(matched(0));
    void_needs_nothing(1);
}
"#,
    );
}

// ---------------------------------------------------------------------------
// A2 / A7 — out-of-bounds indexing and division by zero must fault cleanly,
// identically on both backends, instead of segfaulting (LLVM) or silently
// reading adjacent memory (T3).
// ---------------------------------------------------------------------------

/// Compile for both backends, run, and return (exit_code, output) per backend.
fn run_both_backends(name: &str, source: &str) -> ((i32, String), (i32, String)) {
    let path = write_source(name, source);
    let stem = temp_dir().join(name.trim_end_matches(".mt"));

    // T3 — always via a compiled .t3b, never run-t3 on source (see B22).
    let t3_out = stem.with_extension("t3out");
    let (ok, so, se) = run_manitc(&[
        "compile", "--target", "t3", path.to_str().unwrap(), "-o", t3_out.to_str().unwrap(),
    ]);
    assert!(ok, "{}: t3 compile failed:\n{}{}", name, so, se);
    let t3b = t3_out.with_extension("t3b");
    let t3 = Command::new(get_manitc())
        .args(["run-t3", t3b.to_str().unwrap()])
        .output()
        .expect("run-t3");
    // Drop the "[T3ISA] running ..." banner line.
    let t3_text: String = String::from_utf8_lossy(&t3.stdout)
        .lines()
        .skip(1)
        .map(|l| format!("{}\n", l))
        .collect();

    let bin = stem.with_extension("bin");
    let (ok, so, se) = run_manitc(&[
        "compile", "--target", "llvm", path.to_str().unwrap(), "-o", bin.to_str().unwrap(),
    ]);
    assert!(ok, "{}: llvm compile failed:\n{}{}", name, so, se);
    let ll = Command::new(&bin).output().expect("run llvm binary");
    let ll_text = format!(
        "{}{}",
        String::from_utf8_lossy(&ll.stdout),
        String::from_utf8_lossy(&ll.stderr),
    );

    (
        (t3.status.code().unwrap_or(-1), t3_text),
        (ll.status.code().unwrap_or(-1), ll_text),
    )
}

#[test]
fn a2_static_out_of_bounds_index_is_a_compile_error() {
    assert_check_error(
        "a2_static.mt",
        "fn main() { let a = [10, 20, 30]; io::println_int(a[9]); }\n",
        "out of bounds",
    );
}

#[test]
fn a2_static_negative_index_is_a_compile_error() {
    // A literal negative index folds at compile time. A computed one
    // (`a[0 - 1]`) is not folded by the semantic pass and is caught by the
    // runtime guard instead — see a2_runtime_negative_index_faults.
    assert_check_error(
        "a2_neg.mt",
        "fn main() { let a = [10, 20, 30]; io::println_int(a[-1]); }\n",
        "out of bounds",
    );
}

#[test]
fn a2_runtime_negative_index_faults_on_both_backends() {
    // On T3 this was the worst case: a[-1] returned 3, the array's own length
    // header stored just below the data, so a negative index was a direct read
    // of object metadata.
    let src = r#"
fn main() {
    let a = [10, 20, 30];
    let mut i = 0;
    i = i - 1;
    io::println("before");
    io::println_int(a[i]);
    io::println("survived");
}
"#;
    let ((t3_code, t3_out), (ll_code, ll_out)) = run_both_backends("a2_negrt.mt", src);
    for (label, code, out) in [("t3", t3_code, &t3_out), ("llvm", ll_code, &ll_out)] {
        assert_eq!(code, 70, "{}: expected trap exit 70, got {}:\n{}", label, code, out);
        assert!(
            out.contains("index -1 is out of bounds"),
            "{}: expected a negative-index trap, got:\n{}", label, out,
        );
        assert!(!out.contains("survived"), "{}: continued past the fault", label);
        // Must never surface the length header (3) as if it were an element.
        assert!(!out.contains("\n3\n"), "{}: leaked the length header:\n{}", label, out);
    }
    assert_eq!(t3_out, ll_out, "backends must report the same text");
}

#[test]
fn a2_runtime_out_of_bounds_faults_the_same_on_both_backends() {
    let src = r#"
fn main() {
    let a = [10, 20, 30];
    let mut i = 0;
    while i < 3 { io::println_int(a[i]); i = i + 1; }
    io::println("before");
    io::println_int(a[i + 6]);
    io::println("survived");
}
"#;
    let ((t3_code, t3_out), (ll_code, ll_out)) = run_both_backends("a2_rt.mt", src);

    for (label, code, out) in [("t3", t3_code, &t3_out), ("llvm", ll_code, &ll_out)] {
        assert_eq!(code, 70, "{}: expected trap exit 70, got {} — output:\n{}", label, code, out);
        assert!(
            out.contains("out of bounds"),
            "{}: expected an out-of-bounds trap message, got:\n{}", label, out,
        );
        assert!(!out.contains("survived"), "{}: execution continued past the fault", label);
        // The output printed before the fault must survive — stdout is block
        // buffered when redirected, and a signal death used to discard it.
        assert!(out.contains("before"), "{}: output before the fault was lost", label);
    }
    assert_eq!(t3_out, ll_out, "backends must report the same text");
}

#[test]
fn a7_literal_zero_divisor_is_a_compile_error() {
    assert_check_error(
        "a7_lit.mt",
        "fn main() { io::println_int(7 / 0); }\n",
        "division by zero",
    );
}

#[test]
fn a7_runtime_division_by_zero_faults_the_same_on_both_backends() {
    let src = r#"
fn main() {
    io::println("before");
    let z = 0;
    io::println_int(7 / z);
    io::println("survived");
}
"#;
    let ((t3_code, t3_out), (ll_code, ll_out)) = run_both_backends("a7_rt.mt", src);

    for (label, code, out) in [("t3", t3_code, &t3_out), ("llvm", ll_code, &ll_out)] {
        assert_eq!(code, 70, "{}: expected trap exit 70, got {} — output:\n{}", label, code, out);
        assert!(
            out.contains("division by zero"),
            "{}: expected a division-by-zero trap, got:\n{}", label, out,
        );
        assert!(!out.contains("survived"), "{}: execution continued past the fault", label);
        assert!(out.contains("before"), "{}: output before the fault was lost", label);
    }
    assert_eq!(t3_out, ll_out, "backends must report the same text");
}

#[test]
fn a2_in_bounds_indexing_is_unaffected() {
    let src = r#"
fn main() {
    let a = [10, 20, 30];
    let mut i = 0;
    let mut sum = 0;
    while i < 3 { sum = sum + a[i]; i = i + 1; }
    io::println_int(sum);
}
"#;
    let ((t3_code, t3_out), (ll_code, ll_out)) = run_both_backends("a2_ok.mt", src);
    assert_eq!(t3_code, 0, "t3 should exit 0, output:\n{}", t3_out);
    assert_eq!(ll_code, 0, "llvm should exit 0, output:\n{}", ll_out);
    assert!(t3_out.contains("60"), "t3 sum wrong:\n{}", t3_out);
    assert!(ll_out.contains("60"), "llvm sum wrong:\n{}", ll_out);
}

// ---------------------------------------------------------------------------
// A3 — pathological nesting is a parse error, not a stack-overflow abort
// ---------------------------------------------------------------------------

#[test]
fn a3_deeply_nested_parens_are_a_clean_error() {
    let src = format!("fn main() {{ let x = {}1{}; }}", "(".repeat(2000), ")".repeat(2000));
    assert_check_error("a3_parens.mt", &src, "nested too deeply");
}

#[test]
fn a3_deeply_nested_blocks_are_a_clean_error() {
    let src = format!("fn main() {}{}", "{".repeat(600), "}".repeat(600));
    assert_check_error("a3_blocks.mt", &src, "nested too deeply");
}

#[test]
fn a3_deeply_nested_types_are_a_clean_error() {
    let src = format!(
        "fn f(x: {}int{}) {{}}\nfn main() {{}}\n",
        "Vec<".repeat(600), ">".repeat(600),
    );
    assert_check_error("a3_types.mt", &src, "nested too deeply");
}

#[test]
fn a3_moderate_nesting_still_compiles_and_runs() {
    // 150 used to abort the process; a realistic chain at that depth must
    // survive the whole pipeline, not just the parser.
    let depth = 150;
    let src = format!(
        "fn main() {{ let x = {}1{}; io::println_int(x); }}",
        "1 + (".repeat(depth), ")".repeat(depth),
    );
    let path = write_source("a3_ok.mt", &src);
    let out = temp_dir().join("a3_ok");
    let (ok, stdout, stderr) = run_manitc(&[
        "compile", "--target", "t3", path.to_str().unwrap(), "-o", out.to_str().unwrap(),
    ]);
    assert!(ok, "nesting depth {} should compile:\n{}{}", depth, stdout, stderr);

    let t3b = out.with_extension("t3b");
    let (ok, stdout, stderr) = run_manitc(&["run-t3", t3b.to_str().unwrap()]);
    assert!(ok, "compiled program should run:\n{}{}", stdout, stderr);
    assert!(
        stdout.contains(&(depth + 1).to_string()),
        "expected the sum {} in output, got:\n{}", depth + 1, stdout,
    );
}

// ---------------------------------------------------------------------------
// A4 — `[value; N]` must bound N instead of allocating unboundedly
// ---------------------------------------------------------------------------

#[test]
fn a4_huge_array_repeat_count_is_rejected() {
    assert_check_error(
        "a4_big.mt",
        "fn main() { let a = [1; 300000000]; io::println(\"hi\"); }\n",
        "exceeds the maximum",
    );
}

#[test]
fn a4_realistic_array_repeat_count_still_works() {
    // 54 is the largest repeat count used across the examples and thatteos.
    assert_checks(
        "a4_ok.mt",
        "fn main() { let a = [7; 54]; io::println_int(a[53]); }\n",
    );
}

// ---------------------------------------------------------------------------
// A18 — `-o X.ll` must leave valid LLVM IR at X.ll, not an ELF binary
// ---------------------------------------------------------------------------
// `output.with_extension("ll")` is a no-op when -o already ends in .ll, so the
// IR path and the link output path were the same file and the linker
// overwrote the IR. This broke thatteos/build.sh, whose whole flow is
// "emit .ll → patch it → link it ourselves".

#[test]
fn a18_dash_o_ll_emits_ir_and_does_not_link_over_it() {
    let src = write_source("a18_ir.mt", "fn main() { io::println(\"hi\"); }\n");
    let out = temp_dir().join("a18_out.ll");
    let _ = std::fs::remove_file(&out);

    let (ok, stdout, stderr) = run_manitc(&[
        "compile", "--target", "llvm",
        src.to_str().unwrap(),
        "-o", out.to_str().unwrap(),
    ]);
    assert!(ok, "compile to a .ll path should succeed:\n{}{}", stdout, stderr);

    let bytes = std::fs::read(&out).expect("the .ll file should still exist");
    assert!(
        !bytes.starts_with(b"\x7fELF"),
        "A18: -o X.ll was overwritten with an ELF binary; IR was destroyed",
    );
    let text = String::from_utf8_lossy(&bytes);
    assert!(
        text.contains("define") && text.contains("@main"),
        "A18: expected LLVM IR defining @main at the .ll path, got:\n{}",
        &text.chars().take(200).collect::<String>(),
    );
}

#[test]
fn a18_dash_o_without_ll_extension_still_links_a_binary() {
    // The complementary direction: a non-.ll -o must still produce an
    // executable, with the IR written alongside it.
    let src = write_source("a18_bin.mt", "fn main() { io::println(\"hi\"); }\n");
    let out = temp_dir().join("a18_out_bin");
    let _ = std::fs::remove_file(&out);

    let (ok, stdout, stderr) = run_manitc(&[
        "compile", "--target", "llvm",
        src.to_str().unwrap(),
        "-o", out.to_str().unwrap(),
    ]);
    assert!(ok, "compile should succeed:\n{}{}", stdout, stderr);

    // clang may be unavailable in a minimal environment; only assert the
    // linked-binary property when the compiler reported that it linked one.
    if stdout.contains("[LLVM] binary:") {
        let bytes = std::fs::read(&out).expect("binary should exist");
        assert!(bytes.starts_with(b"\x7fELF"), "expected an ELF executable at -o path");
    }
    let ll = out.with_extension("ll");
    let ir = std::fs::read_to_string(&ll).expect("IR should be written alongside");
    assert!(ir.contains("@main"), "expected IR next to the binary");
}

// ---------------------------------------------------------------------------
// V1 — `v[i]` on a Vec must index the elements, not the header.
//
// A `Vec` value is a pointer to a {data, len, cap} header. Index lowering
// treated it as a flat array, so GetPtr+Load read the header fields as though
// they were elements 0, 1 and 2. The semantic pass types Vec indexing (it
// yields the element type), so this compiled and ran on both backends and
// silently produced wrong values:
//
//     v = [3, 1, 4]        T3: v[0..2] = 0, 0, 0
//                        LLVM: v[0..2] = <heap ptr>, 3 (the len), 8 (the cap)
//
// Both now lower to the same native Vec::get / Vec::set the methods use.
// ---------------------------------------------------------------------------

#[test]
fn v1_vec_index_reads_elements_on_both_backends() {
    let src = r#"
fn main() {
    let mut v: Vec<int> = Vec::new();
    v.push(3); v.push(1); v.push(4);
    io::println(fmt::format("{} {} {}", [
        fmt::show_int(v[0]), fmt::show_int(v[1]), fmt::show_int(v[2])]));
}
"#;
    let ((t3_code, t3_out), (ll_code, ll_out)) = run_both_backends("v1_read.mt", src);
    assert_eq!(t3_code, 0, "t3 should exit cleanly, got:\n{}", t3_out);
    assert_eq!(ll_code, 0, "llvm should exit cleanly, got:\n{}", ll_out);
    assert!(t3_out.contains("3 1 4"), "t3 must read elements, got:\n{}", t3_out);
    assert!(ll_out.contains("3 1 4"), "llvm must read elements, got:\n{}", ll_out);
}

#[test]
fn v1_vec_index_assignment_writes_elements_on_both_backends() {
    // Plain and compound assignment through the index operator, cross-checked
    // against .get() so a regression cannot pass by breaking both alike.
    let src = r#"
fn main() {
    let mut v: Vec<int> = Vec::new();
    v.push(3); v.push(1); v.push(4);
    v[1] = 99;
    v[2] += 10;
    io::println(fmt::format("{} {} {} len={}", [
        fmt::show_int(v[0]), fmt::show_int(v.get(1)), fmt::show_int(v[2]),
        fmt::show_int(v.len())]));
}
"#;
    let ((t3_code, t3_out), (ll_code, ll_out)) = run_both_backends("v1_write.mt", src);
    assert_eq!(t3_code, 0, "t3 should exit cleanly, got:\n{}", t3_out);
    assert_eq!(ll_code, 0, "llvm should exit cleanly, got:\n{}", ll_out);
    // len=3 is the tell: writing through v[1] must not clobber the header.
    assert!(t3_out.contains("3 99 14 len=3"), "t3 wrong:\n{}", t3_out);
    assert!(ll_out.contains("3 99 14 len=3"), "llvm wrong:\n{}", ll_out);
}
