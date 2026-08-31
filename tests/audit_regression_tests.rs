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
    // A constant negative index folds at compile time. Since 21 August 2026
    // that includes computed ones (`a[0 - 1]`, `a[2 * 5]`) — see
    // s24_the_static_index_check_uses_the_same_folder. Only a genuinely
    // dynamic index reaches the runtime guard; see
    // a2_runtime_negative_index_faults_on_both_backends, which builds its
    // index from a mutable variable for exactly that reason.
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

// ---------------------------------------------------------------------------
// LP1/LP2 — the two in-memory layouts of `[trit]`.
//
// maniTC carries two different layouts behind the single type `[trit]`:
//
//   FLAT  an unsized array *parameter* — element i at slot i, with the length
//         passed alongside in a hidden `__len_` argument. Indexing and `for`
//         iteration assume this, and tests/27_ir_regressions.mt pins it.
//
//   LP    everything else — mem[0] is the length, trits occupy mem[1..=len],
//         least-significant first. Documented on INTERNAL_RUNTIME_HELPERS in
//         codegen_llvm/helpers.rs and mirrored by T3 syscalls 8/10/11/12/13.
//         Runtime producers (math::to_balanced_ternary) emit this, and the
//         native trit consumers read it.
//
// LP_FUNCS in ir/lower/lower_expr.rs bridges FLAT -> LP. Two holes were
// measured on 18 Aug 2026, both silently wrong on T3 and a segfault on LLVM:
//
//   LP1  a flat `[trit]` parameter reaching pack_trits/trits_to_str went
//        unconverted, because the bridge could only build the prefixed buffer
//        with a compile-time length (IRInstr::Alloca is statically sized), so
//        the callee read the first trit as the length. Fixed by the
//        __lp_from_flat helper — @__lp_from_flat in codegen_llvm/helpers.rs
//        and T3 syscall #203 — which mallocs the copy at run time.
//   LP2  `from_balanced_ternary` was listed as `ternary::` but is declared in
//        stdlib/math.mt, so the name matched nothing and even a *sized* array
//        went unconverted. Now spelled `math::`.
// ---------------------------------------------------------------------------

/// LP1, the case that motivated __lp_from_flat: a flat `[trit]` parameter
/// handed to each of the three length-prefixed callees. Before the helper,
/// T3 printed -1 for a value of -137 (reading the leading `+` as a length of
/// 1) and LLVM segfaulted.
#[test]
fn lp1_flat_trit_param_reaches_lp_callees_on_both_backends() {
    let src = r#"
fn pack(a: [trit]) -> int { return ternary::pack_trits(a); }
fn show(a: [trit]) -> str { return ternary::trits_to_str(a); }
fn recover(a: [trit]) -> int { return math::from_balanced_ternary(a); }
fn main() {
    // LST-first: 1 - 3 + 0 + 27 + 81 - 243 = -137
    let s: [trit; 6] = [+, -, 0, +, +, -];
    io::println(fmt::format("pack={} show={} recover={}", [
        fmt::show_int(pack(s)), show(s), fmt::show_int(recover(s))]));
}
"#;
    let ((t3_code, t3_out), (ll_code, ll_out)) = run_both_backends("lp1_param.mt", src);
    assert_eq!(t3_code, 0, "t3 should exit cleanly, got:\n{}", t3_out);
    assert_eq!(ll_code, 0, "llvm should exit cleanly, got:\n{}", ll_out);
    let want = "pack=-137 show=+-0++- recover=-137";
    assert!(t3_out.contains(want), "t3 wrong:\n{}", t3_out);
    assert!(ll_out.contains(want), "llvm wrong:\n{}", ll_out);
}

/// A zero-length flat parameter must produce an empty prefixed buffer, not a
/// read of whatever the pointer happens to address.
#[test]
fn lp1_empty_flat_trit_param_is_zero() {
    let src = r#"
fn pack(a: [trit]) -> int { return ternary::pack_trits(a); }
fn main() {
    let z: [trit; 0] = [];
    io::println(fmt::format("empty={}", [fmt::show_int(pack(z))]));
}
"#;
    let ((t3_code, t3_out), (ll_code, ll_out)) = run_both_backends("lp1_empty.mt", src);
    assert_eq!(t3_code, 0, "t3 should exit cleanly, got:\n{}", t3_out);
    assert_eq!(ll_code, 0, "llvm should exit cleanly, got:\n{}", ll_out);
    assert!(t3_out.contains("empty=0"), "t3 wrong:\n{}", t3_out);
    assert!(ll_out.contains("empty=0"), "llvm wrong:\n{}", ll_out);
}

/// The false-positive guard, and the reason the bridge keys on the hidden
/// `#len:` local rather than on the type: a runtime-produced `[trit]` is also
/// `Array(Trit, None)` but is ALREADY prefixed. Wrapping it in __lp_from_flat
/// would prefix it twice. It must keep compiling and keep its value.
#[test]
fn lp1_runtime_trit_slice_is_not_double_prefixed() {
    let src = r#"
fn main() {
    let r: [trit] = math::to_balanced_ternary(-137);
    io::println(fmt::format("v={} s={}", [
        fmt::show_int(math::from_balanced_ternary(r)),
        ternary::trits_to_str(r)]));
}
"#;
    let ((t3_code, t3_out), (ll_code, ll_out)) = run_both_backends("lp1_runtime.mt", src);
    assert_eq!(t3_code, 0, "t3 should exit cleanly, got:\n{}", t3_out);
    assert_eq!(ll_code, 0, "llvm should exit cleanly, got:\n{}", ll_out);
    assert!(t3_out.contains("v=-137 s=+-0++-"), "t3 wrong:\n{}", t3_out);
    assert!(ll_out.contains("v=-137 s=+-0++-"), "llvm wrong:\n{}", ll_out);
}

/// The type-check-level guard for the same case: rejecting a runtime-produced
/// `[trit]` at an LP callee once broke examples/ternary_demo.mt.
#[test]
fn lp1_runtime_trit_slice_is_still_accepted() {
    assert_checks(
        "lp1_runtime_ok.mt",
        r#"
fn main() {
    let bt: [trit] = math::to_balanced_ternary(42);
    io::println(ternary::trits_to_str(bt));
    let lit: [trit] = [+, -, 0, +, +, -];
    io::print_int(ternary::t27_to_int(ternary::pack_trits(lit)));
}
"#,
    );
}

/// The other guard: the bridge must not disturb the FLAT layout itself.
/// Iterating and indexing an unsized `[trit]` parameter still assume element
/// i at slot i, and must keep working on both backends.
#[test]
fn lp1_flat_trit_param_still_iterates_on_both_backends() {
    let src = r#"
fn count_pos(ts: [trit]) -> int {
    let mut c = 0;
    for t in ts { if t > 0 { c = c + 1; } }
    return c;
}
fn first(ts: [trit]) -> int { return ts[0] as int; }
fn main() {
    let a: [trit; 5] = [+, -, +, 0, +];
    io::println(fmt::format("pos={} first={}", [
        fmt::show_int(count_pos(a)), fmt::show_int(first(a))]));
}
"#;
    let ((t3_code, t3_out), (ll_code, ll_out)) = run_both_backends("lp1_iter.mt", src);
    assert_eq!(t3_code, 0, "t3 should exit cleanly, got:\n{}", t3_out);
    assert_eq!(ll_code, 0, "llvm should exit cleanly, got:\n{}", ll_out);
    assert!(t3_out.contains("pos=3 first=1"), "t3 wrong:\n{}", t3_out);
    assert!(ll_out.contains("pos=3 first=1"), "llvm wrong:\n{}", ll_out);
}

#[test]
fn lp2_from_balanced_ternary_converts_a_sized_array() {
    // LST-first [+, 0, -] = 1*1 + 0*3 + (-1)*9 = -8. Before the module
    // qualifier was corrected: T3 returned 0, LLVM segfaulted.
    let src = r#"
fn main() {
    let t: [trit; 3] = [+, 0, -];
    let rt: [trit] = math::to_balanced_ternary(-8);
    io::println(fmt::format("sized={} runtime={}", [
        fmt::show_int(math::from_balanced_ternary(t)),
        fmt::show_int(math::from_balanced_ternary(rt))]));
}
"#;
    let ((t3_code, t3_out), (ll_code, ll_out)) = run_both_backends("lp2_fbt.mt", src);
    assert_eq!(t3_code, 0, "t3 should exit cleanly, got:\n{}", t3_out);
    assert_eq!(ll_code, 0, "llvm should exit cleanly, got:\n{}", ll_out);
    assert!(t3_out.contains("sized=-8 runtime=-8"), "t3 wrong:\n{}", t3_out);
    assert!(ll_out.contains("sized=-8 runtime=-8"), "llvm wrong:\n{}", ll_out);
}

// ---------------------------------------------------------------------------
// TX — `txor` is balanced (a + b) mod 3.
//
// Changed 19 August 2026 from clamped |a - b|. The old operator was a
// difference detector wearing ternary clothes: it could never return `-`, so a
// third of the digit set was unreachable in its range, and it was not a
// bijection — for any fixed b, two of the three inputs mapped to `+` — so
// `x txor k` could not be undone. tests/22_crypto.mt had to hand-roll mod-3 as
// its own `txor_trit` for exactly that reason.
//
// Note which recovery property actually holds. Binary XOR undoes itself after
// TWO applications only because 2 = 0 (mod 2); that is an accident of base 2.
// In base 3 it takes THREE, because 3k = 0 (mod 3). Assuming self-inverse here
// is the binary habit this project exists to avoid.
// ---------------------------------------------------------------------------

#[test]
fn tx_txor_is_balanced_mod3_on_both_backends() {
    let src = r#"
fn main() {
    let ts: [trit; 3] = [-, 0, +];
    let mut row: str = "";
    for i in 0..3 {
        for j in 0..3 {
            row = fmt::format("{}{}", [row,
                ternary::trits_to_str([ts[i] txor ts[j]])]);
        }
    }
    io::println(fmt::format("table={}", [row]));
}
"#;
    let ((t3_code, t3_out), (ll_code, ll_out)) = run_both_backends("tx_table.mt", src);
    assert_eq!(t3_code, 0, "t3 should exit cleanly, got:\n{}", t3_out);
    assert_eq!(ll_code, 0, "llvm should exit cleanly, got:\n{}", ll_out);
    // Rows a = -, 0, +; columns b = -, 0, + within each row.
    //   -,-=+  -,0=-  -,+=0 | 0,-=-  0,0=0  0,+=+ | +,-=0  +,0=+  +,+=-
    let want = "table=+-0-0+0+-";
    assert!(t3_out.contains(want), "t3 wrong:\n{}", t3_out);
    assert!(ll_out.contains(want), "llvm wrong:\n{}", ll_out);
}

#[test]
fn tx_txor_recovers_after_three_applications_not_two() {
    let src = r#"
fn main() {
    let ts: [trit; 3] = [-, 0, +];
    let mut twice: bool = true;
    let mut thrice: bool = true;
    let mut bijection: bool = true;
    for j in 0..3 {
        let k: trit = ts[j];
        for i in 0..3 {
            let x: trit = ts[i];
            if ((x txor k) txor k) != x { twice = false; }
            if (((x txor k) txor k) txor k) != x { thrice = false; }
        }
        let i0: trit = ts[0] txor k;
        let i1: trit = ts[1] txor k;
        let i2: trit = ts[2] txor k;
        if i0 == i1 { bijection = false; }
        if i1 == i2 { bijection = false; }
        if i0 == i2 { bijection = false; }
    }
    let mut a: str = "no";
    if twice { a = "yes"; }
    let mut b: str = "no";
    if thrice { b = "yes"; }
    let mut c: str = "no";
    if bijection { c = "yes"; }
    io::println(fmt::format("self_inverse={} three={} bijection={}", [a, b, c]));
}
"#;
    let ((t3_code, t3_out), (ll_code, ll_out)) = run_both_backends("tx_inv.mt", src);
    assert_eq!(t3_code, 0, "t3 should exit cleanly, got:\n{}", t3_out);
    assert_eq!(ll_code, 0, "llvm should exit cleanly, got:\n{}", ll_out);
    // self_inverse=no is the POINT of this test, not an oversight.
    let want = "self_inverse=no three=yes bijection=yes";
    assert!(t3_out.contains(want), "t3 wrong:\n{}", t3_out);
    assert!(ll_out.contains(want), "llvm wrong:\n{}", ll_out);
}

// ---------------------------------------------------------------------------
// S10 — tuples: allocation size, and the width a destructured name is bound at.
//
// ORACLE_FINDINGS.md Section 10 was filed as "LLVM loses a trit argument when
// two call results feed a tuple-returning call in a loop". It was actually TWO
// independent memory bugs, neither specific to trits, loops, or arity two:
//
//   1. HEAP. Every tuple mapped to the single IR type name "<tuple>", which is
//      in no struct-size table, so the LLVM backend's lookup fell through to
//      its `unwrap_or(1)` default and malloc'd 8 bytes for a tuple of ANY
//      arity. A 2-tuple overran its allocation by 8 bytes on every
//      construction. Fixed by encoding the arity as "<tuple:N>".
//
//   2. STACK. Destructuring bound every name as i64. Tuple slots really are
//      8 bytes wide, so the load was right, but the binding then claimed i64
//      for a `trit`, and assigning it back to a trit variable emitted
//          store i64 %v, ptr %carry     ; %carry = alloca i8
//      an 8-byte write into a 1-byte slot, silently overwriting the
//      neighbouring allocas. Fixed by binding each name at its element type.
//
// The reported symptom needed both, which is why the finding recorded it as
// "not fully isolated" and suspected register allocation. T3 was correct
// throughout — its slots are uniformly one word.
// ---------------------------------------------------------------------------

#[test]
fn s10_destructured_trit_assigned_back_does_not_clobber_neighbours() {
    // The original reproducer, reduced. `carry = c` is the 8-byte store; the
    // neighbour it used to destroy was `tb`, which read back as 0 from the
    // second iteration on.
    let src = r#"
fn trit_at(n: int, pos: int) -> trit {
    let mut v: int = n;
    for _k in 0..pos {
        let mut d: int = v % 3;
        if d == 2 { d = -1; }
        if d == -2 { d = 1; }
        v = (v - d) / 3;
    }
    let mut d: int = v % 3;
    if d == 2 { d = -1; }
    if d == -2 { d = 1; }
    return ternary::int_to_trit(d);
}
fn add3(a: trit, b: trit, cin: trit) -> (trit, trit) {
    let s: int = ternary::trit_to_int(a) + ternary::trit_to_int(b)
               + ternary::trit_to_int(cin);
    if s == 3 { return (0, +); }
    if s == 2 { return (-, +); }
    if s == 1 { return (+, 0); }
    if s == 0 { return (0, 0); }
    if s == -1 { return (-, 0); }
    if s == -2 { return (+, -); }
    return (0, -);
}
fn main() {
    let mut carry: trit = 0;
    let mut row: str = "";
    for i in 0..4 {
        let ta: trit = trit_at(40, i);
        let tb: trit = trit_at(13, i);
        let (s, c) = add3(ta, tb, carry);
        row = fmt::format("{}{}", [row, ternary::trits_to_str([tb])]);
        carry = c;
    }
    io::println(fmt::format("tb={}", [row]));
}
"#;
    let ((t3_code, t3_out), (ll_code, ll_out)) = run_both_backends("s10_carry.mt", src);
    assert_eq!(t3_code, 0, "t3 should exit cleanly, got:\n{}", t3_out);
    assert_eq!(ll_code, 0, "llvm should exit cleanly, got:\n{}", ll_out);
    // 13 is `+++` least-significant first, so positions 0..3 are +, +, +, 0.
    // LLVM used to print `+000`.
    assert!(t3_out.contains("tb=+++0"), "t3 wrong:\n{}", t3_out);
    assert!(ll_out.contains("tb=+++0"), "llvm wrong:\n{}", ll_out);
}

#[test]
fn s10_tuple_elements_keep_their_own_widths() {
    // Mixed widths in one tuple: a trit is one byte, an int is eight. Binding
    // both as i64 is what produced the oversized store.
    let src = r#"
fn mix(t: trit, n: int) -> (trit, int, trit) {
    return (t, n * 2, tnot t);
}
fn main() {
    let mut acc: trit = 0;
    for i in 0..3 {
        let (a, b, c) = mix(+, i);
        acc = c;
        io::println(fmt::format("a={} b={} c={} acc={}", [
            ternary::trits_to_str([a]), fmt::show_int(b),
            ternary::trits_to_str([c]), ternary::trits_to_str([acc])]));
    }
}
"#;
    let ((t3_code, t3_out), (ll_code, ll_out)) = run_both_backends("s10_mixed.mt", src);
    assert_eq!(t3_code, 0, "t3 should exit cleanly, got:\n{}", t3_out);
    assert_eq!(ll_code, 0, "llvm should exit cleanly, got:\n{}", ll_out);
    assert_eq!(t3_out, ll_out, "backends disagree:\nT3:\n{}\nLLVM:\n{}", t3_out, ll_out);
    assert!(t3_out.contains("a=+ b=0 c=- acc=-"), "wrong:\n{}", t3_out);
    assert!(t3_out.contains("a=+ b=4 c=- acc=-"), "wrong:\n{}", t3_out);
}

#[test]
fn s10_wide_tuples_are_allocated_at_full_size() {
    // Arity beyond two: the old 8-byte allocation overran by 8 bytes per extra
    // element, so a 5-tuple wrote 32 bytes past its buffer.
    let src = r#"
fn five() -> (int, int, int, int, int) {
    return (11, 22, 33, 44, 55);
}
fn main() {
    let (a, b, c, d, e) = five();
    io::println(fmt::format("{} {} {} {} {}", [
        fmt::show_int(a), fmt::show_int(b), fmt::show_int(c),
        fmt::show_int(d), fmt::show_int(e)]));
}
"#;
    let ((t3_code, t3_out), (ll_code, ll_out)) = run_both_backends("s10_wide.mt", src);
    assert_eq!(t3_code, 0, "t3 should exit cleanly, got:\n{}", t3_out);
    assert_eq!(ll_code, 0, "llvm should exit cleanly, got:\n{}", ll_out);
    assert!(t3_out.contains("11 22 33 44 55"), "t3 wrong:\n{}", t3_out);
    assert!(ll_out.contains("11 22 33 44 55"), "llvm wrong:\n{}", ll_out);
}

// ---------------------------------------------------------------------------
// §9 tail — the six char-dependent str:: functions
//
// Closed 19 August 2026. Before this, all six were `// native` declarations:
// char_at/to_upper/to_lower existed only in the C runtime (so they worked on
// LLVM and failed to assemble on T3), and pad_left/pad_right/center existed
// nowhere at all — `center` did not even have an LLVM declaration, so it
// failed at link time with "use of undefined value '@str_center'".
//
// The fix adds exactly TWO primitives on both backends — str_char_at (133) and
// str_from_char (134) — and writes the other four in ManiT on top of them, so
// there is one body per function and the backends cannot drift apart the way
// §8's aliases did.
// ---------------------------------------------------------------------------

#[test]
fn s9_char_dependent_str_functions_agree_on_both_backends() {
    let src = r#"
fn main() {
    io::println(fmt::format("up={} low={}", [
        str::to_upper("Hello, World 42!"),
        str::to_lower("Hello, World 42!")]));
    io::println(fmt::format("padl={} padr={} ceven={} codd={}", [
        str::pad_left("7", 4, '0'),
        str::pad_right("7", 4, '.'),
        str::center("hi", 6, '-'),
        str::center("hi", 5, '-')]));
    // Width already met: all three must return the string untouched rather
    // than truncating it.
    io::println(fmt::format("noop={}{}{}", [
        str::pad_left("hello", 3, '0'),
        str::pad_right("hello", 3, '0'),
        str::center("hello", 2, '-')]));
}
"#;
    let ((t3_code, t3_out), (ll_code, ll_out)) = run_both_backends("s9_char.mt", src);
    assert_eq!(t3_code, 0, "t3 should exit cleanly, got:\n{}", t3_out);
    assert_eq!(ll_code, 0, "llvm should exit cleanly, got:\n{}", ll_out);
    for want in [
        "up=HELLO, WORLD 42! low=hello, world 42!",
        // center's odd remainder goes right: "-hi--", not "--hi-".
        "padl=0007 padr=7... ceven=--hi-- codd=-hi--",
        "noop=hellohellohello",
    ] {
        assert!(t3_out.contains(want), "t3 missing {:?}:\n{}", want, t3_out);
        assert!(ll_out.contains(want), "llvm missing {:?}:\n{}", want, ll_out);
    }
}

#[test]
fn s9_printing_a_char_prints_the_character_not_the_codepoint() {
    // `char` is documented as a Unicode scalar value, but print() grouped it
    // with the integer types, so `print(str::char_at("hello", 1))` emitted
    // "101" instead of "e" — which made char_at useless for display and
    // indistinguishable from an int. Nothing in the tree printed a char at the
    // time, so the fix was free.
    let src = r#"
fn main() {
    let c: char = str::char_at("hello", 1);
    io::println(fmt::format("c={} first={} made={}", [
        str::from_char(c),
        str::from_char(str::char_at("hello", 0)),
        str::from_char('Z')]));
    print(c);
}
"#;
    let ((t3_code, t3_out), (ll_code, ll_out)) = run_both_backends("s9_char_print.mt", src);
    assert_eq!(t3_code, 0, "t3 should exit cleanly, got:\n{}", t3_out);
    assert_eq!(ll_code, 0, "llvm should exit cleanly, got:\n{}", ll_out);
    assert!(t3_out.contains("c=e first=h made=Z"), "t3 wrong:\n{}", t3_out);
    assert!(ll_out.contains("c=e first=h made=Z"), "llvm wrong:\n{}", ll_out);
    // The bare print(c) line: the character, never the codepoint.
    assert!(!t3_out.contains("101"), "t3 printed the codepoint:\n{}", t3_out);
    assert!(!ll_out.contains("101"), "llvm printed the codepoint:\n{}", ll_out);
}

#[test]
fn s9_char_at_out_of_range_is_zero_on_both_backends() {
    // The C runtime returns 0 for a negative or past-the-end index; the T3
    // emulator handler must agree rather than trapping or reading out of
    // bounds. Pinned because the two implementations are necessarily separate.
    let src = r#"
fn main() {
    io::println(fmt::format("neg={} past={} last={}", [
        fmt::show_int(str::char_at("abc", -1) as int),
        fmt::show_int(str::char_at("abc", 3) as int),
        fmt::show_int(str::char_at("abc", 2) as int)]));
}
"#;
    let ((t3_code, t3_out), (ll_code, ll_out)) = run_both_backends("s9_char_oob.mt", src);
    assert_eq!(t3_code, 0, "t3 should exit cleanly, got:\n{}", t3_out);
    assert_eq!(ll_code, 0, "llvm should exit cleanly, got:\n{}", ll_out);
    // 'c' is 99.
    assert!(t3_out.contains("neg=0 past=0 last=99"), "t3 wrong:\n{}", t3_out);
    assert!(ll_out.contains("neg=0 past=0 last=99"), "llvm wrong:\n{}", ll_out);
}

#[test]
fn s8_int_to_trits_had_no_implementation_on_either_backend() {
    // ternary::int_to_trits is the worked example in stdlib/ternary.mt's own
    // module header, and until 19 Aug 2026 it existed only as a `// native`
    // declaration: LLVM emitted a call to an undefined @ternary_int_to_trits
    // and T3 could not assemble the label. It was previously recorded as
    // needing "unsized array returns, a language gap" — that was wrong.
    // math::to_balanced_ternary already returns an unsized [trit] on both
    // backends, so the feature was there and only this function was missing.
    let src = r#"
fn main() {
    io::println(fmt::format("a={} b={} c={}", [
        ternary::trits_to_str(ternary::int_to_trits(11, 4)),
        ternary::trits_to_str(ternary::int_to_trits(42, 5)),
        ternary::trits_to_str(ternary::int_to_trits(0, 3))]));
    io::println(fmt::format("neg={} trunc={} zero_width={}", [
        ternary::trits_to_str(ternary::int_to_trits(-4, 3)),
        ternary::trits_to_str(ternary::int_to_trits(11, 2)),
        ternary::trits_to_str(ternary::int_to_trits(11, 0))]));
}
"#;
    let ((t3_code, t3_out), (ll_code, ll_out)) = run_both_backends("s8_i2t.mt", src);
    assert_eq!(t3_code, 0, "t3 should exit cleanly, got:\n{}", t3_out);
    assert_eq!(ll_code, 0, "llvm should exit cleanly, got:\n{}", ll_out);
    // LST-first, so read each right-to-left to check the value:
    //   -++0 = -1 + 3 + 9      = 11
    //   0---+ = -3 -9 -27 + 81 = 42
    //   --0  = -1 + -3         = -4
    //   -+   = -1 + 3          = 2   (higher trits discarded, as documented)
    //   empty renders "0"
    for want in ["a=-++0 b=0---+ c=000", "neg=--0 trunc=-+ zero_width=0"] {
        assert!(t3_out.contains(want), "t3 missing {:?}:\n{}", want, t3_out);
        assert!(ll_out.contains(want), "llvm missing {:?}:\n{}", want, ll_out);
    }
}

#[test]
fn s8_to_balanced_ternary_matches_its_documented_examples() {
    // stdlib/math.mt carried two WRONG worked examples until 19 Aug 2026: one
    // was a draft left mid-correction ("[-, +] ... is wrong; actually"), and
    // to_balanced_ternary(-4) was documented as [-, +, -], which is -7. The
    // implementation was right both times; only the comment lied. Pinned here
    // so the doc and the code cannot drift apart again.
    let src = r#"
fn main() {
    io::println(fmt::format("five={} zero={} negfour={}", [
        ternary::trits_to_str(math::to_balanced_ternary(5)),
        ternary::trits_to_str(math::to_balanced_ternary(0)),
        ternary::trits_to_str(math::to_balanced_ternary(-4))]));
    io::println(fmt::format("rt5={} rtm4={}", [
        fmt::show_int(math::from_balanced_ternary(math::to_balanced_ternary(5))),
        fmt::show_int(math::from_balanced_ternary(math::to_balanced_ternary(-4)))]));
}
"#;
    let ((t3_code, t3_out), (ll_code, ll_out)) = run_both_backends("s8_bt.mt", src);
    assert_eq!(t3_code, 0, "t3 should exit cleanly, got:\n{}", t3_out);
    assert_eq!(ll_code, 0, "llvm should exit cleanly, got:\n{}", ll_out);
    // five = --+ : -1 + -3 + 9 = 5.   negfour = -- : -1 + -3 = -4.
    for want in ["five=--+ zero=0 negfour=--", "rt5=5 rtm4=-4"] {
        assert!(t3_out.contains(want), "t3 missing {:?}:\n{}", want, t3_out);
        assert!(ll_out.contains(want), "llvm missing {:?}:\n{}", want, ll_out);
    }
}

#[test]
fn s14_fmt_align_honours_the_pad_char_on_both_backends() {
    // stdlib/fmt.mt declares align_left/align_right with three parameters and
    // the lowerer has always emitted a 3-argument call, but the LLVM declares
    // named only two. clang accepts a 3-argument call to a 2-parameter
    // function, so the pad char was dropped and the C hardcoded a space:
    // align_left("ab", 5, '.') printed "ab..." on T3 and "ab   " on LLVM.
    // Nothing caught it because native call arguments are never type-checked.
    let src = r#"
fn main() {
    io::println(fmt::align_left("ab", 5, '.'));
    io::println(fmt::align_right("ab", 5, '.'));
    // A space pad must still work — it is what the broken version always did,
    // so testing only this case would have passed against the bug.
    io::println(fmt::align_left("ab", 5, ' '));
    // Width already met: return the string untouched, never truncated.
    io::println(fmt::align_left("toolong", 3, '.'));
}
"#;
    let ((t3_code, t3_out), (ll_code, ll_out)) = run_both_backends("s14_align.mt", src);
    assert_eq!(t3_code, 0, "t3 should exit cleanly, got:\n{}", t3_out);
    assert_eq!(ll_code, 0, "llvm should exit cleanly, got:\n{}", ll_out);
    for want in ["ab...", "...ab", "ab   ", "toolong"] {
        assert!(t3_out.contains(want), "t3 missing {:?}:\n{}", want, t3_out);
        assert!(ll_out.contains(want), "llvm missing {:?}:\n{}", want, ll_out);
    }
}

#[test]
fn s16_tuple_field_access_reads_the_right_element() {
    // `p.1` loaded element 0. Two independent holes lined up: the analyzer's
    // resolve_field_type had no tuple arm and returned ManiType::Unknown, and
    // the IR lowerer looked the field name up in the struct table, missed, and
    // fell through `unwrap_or(0)`. Both backends agreed on the wrong value and
    // nothing warned. Destructuring — `let (a, b) = ...` — has its own path and
    // was always correct, which is why the whole stdlib never tripped over it.
    //
    // The heterogeneous tuple matters: with `(int, int)` the missing type
    // resolution is invisible, so a same-type test would have passed against
    // half the bug.
    let src = r#"
fn main() {
    let a: (int, int, int) = (7, 8, 9);
    io::println(fmt::format("{} {} {}", [
        fmt::show_int(a.0), fmt::show_int(a.1), fmt::show_int(a.2)]));

    let b: (int, str) = (42, "hi");
    io::println(fmt::format("{} {}", [fmt::show_int(b.0), b.1]));

    // Through a container, and through a tuple-returning stdlib call.
    let v: Vec<(str, str)> = Vec::new();
    v.push(("k0", "v0"));
    v.push(("k1", "v1"));
    let e: (str, str) = v.get(1);
    io::println(fmt::format("{}={}", [e.0, e.1]));

    let sc: (trit, trit) = ternary::trit_add(+, +);
    io::println(fmt::format("sum={} carry={}", [
        fmt::show_trit(sc.0), fmt::show_trit(sc.1)]));
}
"#;
    let ((t3_code, t3_out), (ll_code, ll_out)) = run_both_backends("s16_tuple.mt", src);
    assert_eq!(t3_code, 0, "t3 should exit cleanly, got:\n{}", t3_out);
    assert_eq!(ll_code, 0, "llvm should exit cleanly, got:\n{}", ll_out);
    // 1 + 1 = 3 - 1, so the sum trit is - and the carry is +.
    for want in ["7 8 9", "42 hi", "k1=v1", "sum=- carry=+"] {
        assert!(t3_out.contains(want), "t3 missing {:?}:\n{}", want, t3_out);
        assert!(ll_out.contains(want), "llvm missing {:?}:\n{}", want, ll_out);
    }
}

#[test]
fn s17_the_whole_fmt_surface_runs_on_both_backends() {
    // 25 of fmt::'s 31 functions were signatures with no implementation on
    // either backend, and show_trit was worse — it linked on LLVM and had no T3
    // intercept, so the same source built on one target and not the other. They
    // are ManiT source now, over str:: and ternary::, so there is one body per
    // function and the backends cannot disagree.
    //
    // This exercises every function the module exports. The point is coverage,
    // not depth: a link error or a missing T3 label is what regression looks
    // like here, and it shows up as a non-zero exit long before any assert.
    let src = r#"
fn min_op(a: trit, b: trit) -> trit { return ternary::trit_and(a, b); }

fn main() {
    io::println(fmt::format1("f1={}", "a"));
    io::println(fmt::format2("f2={},{}", "a", "b"));
    io::println(fmt::format3("f3={},{},{}", "a", "b", "c"));

    io::println(fmt::format("trit={} bool3={} dual={}", [
        fmt::show_trit(-), fmt::show_bool3(True), fmt::show_dual(5)]));

    io::println(fmt::format("t27={} pad={} t9={}", [
        fmt::show_t27(ternary::int_to_t27(5)),
        fmt::show_t27_padded(ternary::int_to_t27(5), 6),
        fmt::show_t9(ternary::int_to_t9(40))]));
    io::println(fmt::format("tryte={}", [fmt::show_tryte(ternary::int_to_tryte(4))]));

    io::println(fmt::format("hex={} HEX={} neg={}", [
        fmt::show_hex(6699), fmt::show_hex_upper(6699), fmt::show_hex(-26)]));
    io::println(fmt::format2("oct={} bin={}", fmt::show_octal(493), fmt::show_binary(10)));

    io::println(fmt::format("[{}][{}][{}]", [
        fmt::align_left("ab", 5, '.'),
        fmt::align_right("ab", 5, '.'),
        fmt::align_center("hi", 6, '-')]));
    io::println(fmt::format2("zp={} zpneg={}", fmt::zero_pad("42", 5), fmt::zero_pad("-42", 5)));

    io::println(fmt::format2("dp={} sci={}", fmt::show_float_dp(2.345, 2),
                             fmt::show_float_sci(12345.0)));

    io::println(fmt::format1("slice={}", fmt::show_trit_slice([-, -, +])));
    io::print(fmt::show_trit_table(5));
    // Colour is ANSI: check it is longer than the plain form rather than
    // embedding escape bytes in this file.
    io::println(fmt::format1("colour_len={}", fmt::show_int(
        str::len(fmt::show_t27_colour(ternary::int_to_t27(5))))));

    fmt::print_separator(4, '-');
    fmt::print_section("S");
    let rows: Vec<(str, str)> = Vec::new();
    rows.push(("k", "v"));
    rows.push(("long", "w"));
    fmt::print_table(rows);
    fmt::print_truth_table("min", min_op);
}
"#;
    let ((t3_code, t3_out), (ll_code, ll_out)) = run_both_backends("s17_fmt.mt", src);
    assert_eq!(t3_code, 0, "t3 should exit cleanly, got:\n{}", t3_out);
    assert_eq!(ll_code, 0, "llvm should exit cleanly, got:\n{}", ll_out);
    assert_eq!(t3_out, ll_out, "the two backends must produce identical output");

    for want in [
        "f1=a", "f2=a,b", "f3=a,b,c",
        "trit=- bool3=True dual=5 (+--)",
        "t27=+-- pad=000+-- t9=++++",
        "tryte=0++",
        "hex=0x1a2b HEX=0x1A2B neg=-0x1a",
        "oct=0o755 bin=0b1010",
        "[ab...][...ab][--hi--]",
        "zp=00042 zpneg=-0042",
        "dp=2.35 sci=1.234500e+04",
        // Slices are least-significant-first, so [-, -, +] is -1 + -3 + 9 = 5,
        // and MST-first that reads +--.
        "slice=+--",
        // 3 glyphs x (5-byte SGR + glyph + 4-byte reset).
        "colour_len=30",
        "----",
        "  S  ",
        "k   : v",
        "long: w",
        "min  |  +  0  -",
        "   - |  -  -  -",
    ] {
        assert!(t3_out.contains(want), "t3 missing {:?}:\n{}", want, t3_out);
    }
    // The decomposition table, checked as a block so column widths are pinned.
    for want in [
        "pos       3^pos  trit   contribution",
        "  2           9     +              9",
        "  1           3     -             -3",
        "  0           1     -             -1",
        "total                              5",
    ] {
        assert!(t3_out.contains(want), "t3 missing table row {:?}:\n{}", want, t3_out);
    }
}

#[test]
fn s18_the_whole_str_surface_runs_on_both_backends() {
    // str:: went from 33 of 43 measurable functions to all of them on 20 August
    // 2026. Ten were declared and defined NOWHERE — no ManiT body, no C body,
    // and for six of them not even an LLVM `declare` — so they failed at link on
    // LLVM and at assembly on T3. The module header documented an API that did
    // not exist.
    //
    // The five from_* converters delegate to fmt:: rather than carrying their
    // own bodies: they ARE the same conversion under a second name, and a second
    // implementation is exactly how `align_left` came to mean two different
    // things at once (section 14a).
    let src = r#"
use std::io;
use std::str;
use std::fmt;

fn shows(r: Result<int, str>) -> str {
    match r { Ok(v) => { return fmt::show_int(v); }, Unknown(m) => { return str::concat("Unknown:", m); }, Err(e) => { return str::concat("Err:", e); } }
}
fn showf(r: Result<float, str>) -> str {
    match r { Ok(v) => { return fmt::show_float(v); }, Unknown(m) => { return str::concat("Unknown:", m); }, Err(e) => { return str::concat("Err:", e); } }
}

fn main() {
    io::println(str::concat("conv=", str::from_float(3.5)));
    io::println(str::concat("bool=", str::from_bool(false)));
    io::println(str::concat("trit=", str::from_trit(-)));
    io::println(str::concat("b3=", str::from_bool3(Unknown)));
    io::println(str::concat("t27=", str::from_ternary(5 as t27)));
    // parse_ternary and from_ternary must be exact inverses.
    io::println(str::concat("rt=", str::from_ternary(str::parse_ternary("-++"))));

    io::println(str::concat("pf1=", fmt::show_float(str::parse_float("-12.25"))));
    io::println(str::concat("pf2=", fmt::show_float(str::parse_float("2500e-3"))));
    io::println(str::concat("pf3=", fmt::show_float(str::parse_float("-1.5E-2"))));

    io::println(str::concat("num=", fmt::show_bool(str::is_numeric("12345"))));
    io::println(str::concat("num0=", fmt::show_bool(str::is_numeric(""))));
    io::println(str::concat("alpha=", fmt::show_bool(str::is_alpha("abc1"))));
    io::println(str::concat("alnum=", fmt::show_bool(str::is_alphanumeric("a1B2"))));
    io::println(str::concat("blank=", fmt::show_bool(str::is_blank(" \t\n\r"))));
    // is_blank(s) and is_empty(trim(s)) must never disagree.
    io::println(str::concat("agree=", fmt::show_bool(
        str::is_blank(" \t ") == str::is_empty(str::trim(" \t ")))));

    let v: Vec<str> = Vec::new();
    v.push("x"); v.push("y"); v.push("z");
    io::println(str::concat("join=", str::join(v, "-")));
    let e: Vec<str> = Vec::new();
    io::println(str::concat("joinempty=[", str::concat(str::join(e, "-"), "]")));
    io::println(str::concat("split_join=", str::join(str::split("a,b,c", ","), ",")));

    io::println(str::concat("ti=", shows(str::try_parse_int("  13  "))));
    io::println(str::concat("ti_bad=", shows(str::try_parse_int("12a"))));
    io::println(str::concat("tf=", showf(str::try_parse_float("-1.5e2"))));
    io::println(str::concat("tf_bad=", showf(str::try_parse_float("1.2.3"))));
}
"#;
    let ((t3_code, t3_out), (ll_code, ll_out)) = run_both_backends("s18_str.mt", src);
    assert_eq!(t3_code, 0, "t3 should exit cleanly, got:\n{}", t3_out);
    assert_eq!(ll_code, 0, "llvm should exit cleanly, got:\n{}", ll_out);
    assert_eq!(t3_out, ll_out, "the two backends must produce identical output");

    for want in [
        "conv=3.5", "bool=false", "trit=-", "b3=Unknown", "t27=+--", "rt=-++",
        "pf1=-12.25", "pf2=2.5", "pf3=-0.015",
        // Empty is FALSE for the three validators and TRUE for is_blank; that
        // asymmetry is deliberate and is what this pins.
        "num=true", "num0=false", "alpha=false", "alnum=true", "blank=true",
        "agree=true",
        "join=x-y-z", "joinempty=[]", "split_join=a,b,c",
        "ti=13", "ti_bad=Err:not an integer",
        "tf=-150", "tf_bad=Err:not a float",
    ] {
        assert!(t3_out.contains(want), "t3 missing {:?}:\n{}", want, t3_out);
    }
}

#[test]
fn s19_result_carries_a_float_payload_on_both_backends() {
    // `Ok(1.5)` did not assemble on LLVM: the Result box is [tag, i64] and the
    // `@Ok(i64)` constructor took its payload as i64, so handing it a double was
    // "defined with type 'double' but expected 'i64'". T3, being word-oriented,
    // had always worked — a backend divergence hidden behind a type nobody had
    // instantiated with a float until str::try_parse_float needed it.
    //
    // The fix reinterprets the bits at the call boundary rather than converting
    // the value. That is only sound because the slot is TYPE-ERASED; a genuine
    // numeric conversion is lowered as an explicit cast well before codegen.
    let src = r#"
use std::io;
use std::fmt;

fn mkf() -> Result<float, str> { return Ok(1.5); }
fn mki() -> Result<int, str> { return Ok(7); }
fn mke() -> Result<float, str> { return Err("bad"); }

fn main() {
    match mkf() { Ok(v) => { io::println(fmt::show_float(v)); }, Unknown(m) => { io::println(m); }, Err(e) => { io::println(e); } }
    match mki() { Ok(v) => { io::println(fmt::show_int(v)); }, Unknown(m) => { io::println(m); }, Err(e) => { io::println(e); } }
    match mke() { Ok(v) => { io::println(fmt::show_float(v)); }, Unknown(m) => { io::println(m); }, Err(e) => { io::println(e); } }
}
"#;
    let ((t3_code, t3_out), (ll_code, ll_out)) = run_both_backends("s19_okfloat.mt", src);
    assert_eq!(t3_code, 0, "t3 should exit cleanly, got:\n{}", t3_out);
    assert_eq!(ll_code, 0, "llvm should exit cleanly, got:\n{}", ll_out);
    assert_eq!(t3_out, ll_out, "the two backends must produce identical output");
    // The float must survive the round trip through the i64 payload slot intact.
    assert!(t3_out.contains("1.5"), "float payload lost:\n{}", t3_out);
    assert!(t3_out.contains("7"), "int payload lost:\n{}", t3_out);
    assert!(t3_out.contains("bad"), "err payload lost:\n{}", t3_out);
}

#[test]
fn s20_t3_does_not_rescue_a_destination_that_is_not_defined_yet() {
    // `(n as float) * 2.0` returned `n as float` on T3 and the product on LLVM.
    //
    // The T3 allocator hands out the destination register BEFORE emitting the
    // operand setup, so between `dst_reg()` and the syscall that writes it, the
    // destination temp is mapped to a register whose contents belong to something
    // else. `rescue_reg` then found that mapping, saw the temp was live past this
    // instruction, and "rescued" it — copying a stale value to R5 and, worse,
    // REBINDING the temp to R5. The multiply syscall duly wrote its product to R1
    // and the return read R5. The wrong answer was silent: no crash, no warning,
    // just the first operand where the product should have been.
    //
    // `holds_value()` is the guard: a temp whose defining instruction has not been
    // emitted yet holds nothing, so there is nothing to rescue. Function
    // parameters have no defining instruction and are live from entry, which is
    // why absence from `first_def` reads as defined rather than as undefined.
    //
    // Each case below drives a different rescue site. `scale` is the original
    // report (rescue_reg, via float-literal loading); `chain` keeps a second float
    // live across the syscall so a genuine rescue must still happen; `twoval`
    // forces rescue_reg_inclusive by consuming both operands in the same
    // instruction.
    let src = r#"
use std::io;
use std::fmt;

fn scale(n: int) -> float { return (n as float) * 2.0; }

fn chain(n: int) -> float {
    let a: float = n as float;
    let b: float = a * 3.0;
    return a + b;
}

fn twoval(x: float, y: float) -> float { return (x * y) + 1.0; }

fn main() {
    io::println(fmt::show_float(scale(3)));
    io::println(fmt::show_float(chain(2)));
    io::println(fmt::show_float(twoval(2.5, 4.0)));
}
"#;
    let ((t3_code, t3_out), (ll_code, ll_out)) = run_both_backends("s20_rescue.mt", src);
    assert_eq!(t3_code, 0, "t3 should exit cleanly, got:\n{}", t3_out);
    assert_eq!(ll_code, 0, "llvm should exit cleanly, got:\n{}", ll_out);
    assert_eq!(t3_out, ll_out, "the two backends must produce identical output");
    // Pinned absolutely, not just cross-backend: agreeing on the wrong number is
    // the failure mode a pure differential check cannot see.
    for want in ["6", "8", "11"] {
        assert!(t3_out.contains(want), "expected {:?} in output:\n{}", want, t3_out);
    }
}

#[test]
fn s21_float_rendering_is_identical_on_both_backends() {
    // The LLVM runtime printed floats with "%g" and the T3 emulator with Rust's
    // `format!("{}", f)`. Those are different numbers on screen, not two
    // spellings of one:
    //
    //     3.14159265358979  ->  "3.14159"      vs  "3.14159265358979"
    //     1234567.0         ->  "1.23457e+06"  vs  "1234567"
    //     2.0/3.0           ->  "0.666667"     vs  "0.6666666666666666"
    //
    // "%g" gives six significant figures and switches to scientific notation, so
    // eleven digits of a double were dropped on one backend and kept on the
    // other. No float-valued program could be cross-checked — which is the one
    // thing the two-backend oracle exists to do.
    //
    // T3 was the correct side, so the C runtime moved to match it: shortest
    // round-tripping digits, rendered positionally. Two traps were paid for on
    // the way, and both are now pinned by the cases below.
    //
    //  * "%.0f" is NOT the answer for large values. It renders the exact binary
    //    value. `big` below is 10.0 multiplied thirty times, which lands on the
    //    double 9.999999999999999e29 — whose EXACT value is
    //    999999999999999879147136483328 but whose shortest round-trip rendering
    //    is 9999999999999999 followed by fourteen zeros. The two part company at
    //    digit 17, so this case fails loudly under "%.0f" and under "%g" alike.
    //  * glibc breaks a decimal tie to EVEN and Rust breaks it AWAY FROM ZERO.
    //    1059438285926254.25 is ...254.2 from one and ...254.3 from the other,
    //    and both read back as the same double. About 1 double in 4,000 is
    //    affected. `tie` pins one; a 2,000,012-value differential sweep of the
    //    C renderer against Rust's found zero disagreements after the fix.
    let src = r#"
use std::io;
use std::fmt;
fn show(f: float) { io::println(fmt::show_float(f)); }
fn main() {
    show(3.14159265358979);
    show(1234567.0);
    show(0.0000001);
    show(0.0);
    show(-0.0);
    show(-12.25);
    let third: float = 2.0 / 3.0;
    show(third);
    let big: float = 1.0;
    let mut b: float = big;
    let mut i: int = 0;
    while i < 30 { b = b * 10.0; i = i + 1; }
    show(b);
    let tie: float = 1059438285926254.25;
    show(tie);
}
"#;
    let ((t3_code, t3_out), (ll_code, ll_out)) = run_both_backends("s21_floatfmt.mt", src);
    assert_eq!(t3_code, 0, "t3 should exit cleanly, got:\n{}", t3_out);
    assert_eq!(ll_code, 0, "llvm should exit cleanly, got:\n{}", ll_out);
    assert!(!t3_out.trim().is_empty(), "t3 produced no output — an empty-vs-empty comparison is not a pass");
    assert_eq!(t3_out, ll_out, "the two backends must render floats identically");

    // Pinned absolutely too: agreeing on the wrong string is a failure a pure
    // differential check cannot see.
    for want in [
        "3.14159265358979",     // full precision, not %g's 3.14159
        "1234567",              // positional, not 1.23457e+06
        "0.0000001",            // positional, not 1e-07
        "0.6666666666666666",   // 16 digits, not 0.666667
        "-12.25",
        "1059438285926254.3",   // Rust's tie-break, not glibc's ...254.2
    ] {
        assert!(t3_out.contains(want), "expected {:?} in output:\n{}", want, t3_out);
    }
    assert!(!t3_out.contains('e') && !t3_out.contains('E'),
            "no float may render in scientific notation:\n{}", t3_out);
    // Shortest round-trip digits, zero-padded to position — NOT the exact binary
    // value 999999999999999879147136483328, and not scientific notation.
    assert!(t3_out.contains(&format!("9999999999999999{}", "0".repeat(14))),
            "large float must render from shortest round-trip digits:\n{}", t3_out);
    assert!(!t3_out.contains("999999999999999879147136483328"),
            "large float must NOT render its exact binary value:\n{}", t3_out);
}

#[test]
fn s22_t3_spill_reads_are_measured_against_the_right_stack_depth() {
    // A function with more live float constants than the register pool holds
    // returned a WRONG SUM on T3 and the right one on LLVM — off by exactly one
    // coefficient, at every size, silently.
    //
    // It is an ordering bug, not an arithmetic one. The rescue paths bump
    // `sp_depth` the moment they decide to spill, but stage their `TSUB R26`
    // into `cur_instr`. Spill READS are pushed into `lines`, which the flush
    // appends BEFORE `cur_instr`. So the LOAD executes while R26 still holds its
    // pre-rescue value, while its offset had been computed against the
    // post-rescue depth:
    //
    //     ; reload spill t19 (offset 1)
    //     LOAD  R21, [R26+#1]                 <- reads one slot too high
    //     TSUB  R26, R26, #1  ; rescue-spill  <- the push it was measured against
    //     STORE R2,  [R26+#0]
    //
    // `rescue_pushes_this_instr` counts the difference so reads are measured
    // against the depth R26 will actually hold when they run.
    //
    // This is the bug behind the "house rule" that every float constant in the
    // designed math:: bodies is bound on the statement that consumes it: a
    // 20-constant header exhausted the pool and the function silently returned
    // its own argument. The rule is no longer needed.
    //
    // 16 is the threshold (the pool is R1-R20); 40 is well past it.
    let mut src = String::from("use std::io;\nuse std::fmt;\n\n");
    for n in [16usize, 40] {
        src.push_str(&format!("fn upfront{}(t: float) -> float {{\n", n));
        for i in 0..n { src.push_str(&format!("    let c{}: float = {}.0;\n", i, i + 1)); }
        src.push_str("    let mut s: float = 0.0;\n");
        for i in 0..n { src.push_str(&format!("    s = s + c{} * t;\n", i)); }
        src.push_str("    return s;\n}\n\n");
    }
    src.push_str("fn main() {\n");
    src.push_str("    io::println(fmt::show_float(upfront16(2.0)));\n");
    src.push_str("    io::println(fmt::show_float(upfront40(2.0)));\n");
    src.push_str("}\n");

    let ((t3_code, t3_out), (ll_code, ll_out)) = run_both_backends("s22_spill.mt", &src);
    assert_eq!(t3_code, 0, "t3 should exit cleanly, got:\n{}", t3_out);
    assert_eq!(ll_code, 0, "llvm should exit cleanly, got:\n{}", ll_out);
    assert!(!t3_out.trim().is_empty(), "t3 produced no output");
    assert_eq!(t3_out, ll_out, "the two backends must agree under register pressure");
    // sum(1..=16)*2 = 272 and sum(1..=40)*2 = 1640. Pinned absolutely: the old
    // T3 answers were 270 and 1638, and two backends agreeing on 270 would pass
    // a purely differential check.
    assert!(t3_out.contains("272"), "16-constant sum wrong:\n{}", t3_out);
    assert!(t3_out.contains("1640"), "40-constant sum wrong:\n{}", t3_out);
}

#[test]
fn s23_the_whole_math_float_surface_runs_on_both_backends() {
    // All 34 float functions in math:: were NATIVE declarations, and the census
    // that measured them found the worst possible split:
    //
    //   * on T3, ALL 34 were undefined labels. The T3 emulator has no float-math
    //     syscalls at all — only arithmetic (212-215), comparison (216),
    //     conversion (210-211) and load (219). `math::sqrt` simply did not exist.
    //   * on LLVM, 9 of the 34 worked (sqrt, log, log2, log3, floor, ceil, round,
    //     sin, cos) and the other 25 were declared but never defined, so any
    //     program touching them failed at link time.
    //
    // Nine working on the backend most people build with, and none on the target
    // the language exists for, is worse than none working anywhere: it looks fine
    // until it is ported.
    //
    // All 34 are now ManiT bodies shared by both backends, and the nine C
    // definitions are deleted — keeping them would shadow the shared body and
    // reintroduce the divergence. No libm is involved on either side.
    //
    // Accuracy was measured against libm over 780 sampled points: worst 3 ulp
    // (cbrt), everything else <= 2, and log/log2/atan 96-97% bit-exact. This test
    // pins exactness where it is achievable and cross-backend equality always.
    let src = r#"
use std::io;
use std::fmt;
use std::math;

fn show(tag: str, f: float) { io::print(tag); io::print("="); io::println(fmt::show_float(f)); }

fn main() {
    show("fabs", math::fabs(-3.5));
    show("fmin", math::fmin(2.0, 7.0));
    show("fmax", math::fmax(2.0, 7.0));
    show("fclamp", math::fclamp(9.0, 0.0, 4.0));
    show("fpow", math::fpow(2.0, 10.0));
    show("sqrt", math::sqrt(16.0));
    show("cbrt", math::cbrt(27.0));
    show("hypot", math::hypot(3.0, 4.0));
    show("floor", math::floor(-2.5));
    show("ceil", math::ceil(-2.5));
    show("round", math::round(2.5));
    show("trunc", math::trunc(-2.7));
    show("fract", math::fract(2.75));
    show("brnd", math::balanced_round(7.1, 1));
    show("log", math::log(1.0));
    show("log2", math::log2(8.0));
    show("log10", math::log10(1000.0));
    show("log3", math::log3(9.0));
    show("logn", math::logn(81.0, 3.0));
    show("exp", math::exp(0.0));
    show("exp2", math::exp2(10.0));
    show("exp3", math::exp3(4.0));
    show("sin", math::sin(0.0));
    show("cos", math::cos(0.0));
    show("tan", math::tan(0.0));
    show("asin", math::asin(0.0));
    show("acos", math::acos(1.0));
    show("atan", math::atan(0.0));
    show("atan2", math::atan2(0.0, 1.0));
    show("sinh", math::sinh(0.0));
    show("cosh", math::cosh(0.0));
    show("tanh", math::tanh(0.0));
    show("torad", math::to_radians(180.0));
    show("todeg", math::to_degrees(3.141592653589793));
}
"#;
    let ((t3_code, t3_out), (ll_code, ll_out)) = run_both_backends("s23_mathfloat.mt", src);
    assert_eq!(t3_code, 0, "t3 should exit cleanly, got:\n{}", t3_out);
    assert_eq!(ll_code, 0, "llvm should exit cleanly, got:\n{}", ll_out);
    assert!(!t3_out.trim().is_empty(), "t3 produced no output");
    assert_eq!(t3_out, ll_out, "every math:: float function must agree across backends");

    // Arguments were chosen so the true result is exactly representable: an
    // approximation good to 3 ulp still has to be EXACT here, so these catch a
    // body that is merely self-consistent.
    for want in [
        "fabs=3.5", "fmin=2", "fmax=7", "fclamp=4", "fpow=1024",
        "sqrt=4", "cbrt=3", "hypot=5",
        "floor=-3", "ceil=-2", "round=3", "trunc=-2", "fract=0.75",
        "brnd=6",                       // balanced_round(7.1, 1) -> 6, not 9
        "log=0", "log2=3", "log10=3", "log3=2", "logn=4",
        "exp=1", "exp2=1024", "exp3=81",
        "sin=0", "cos=1", "tan=0", "asin=0", "acos=0", "atan=0", "atan2=0",
        "sinh=0", "cosh=1", "tanh=0",
        "torad=3.141592653589793", "todeg=180",
    ] {
        assert!(t3_out.contains(want), "expected {:?} in output:\n{}", want, t3_out);
    }
}

// ---------------------------------------------------------------------------
// S24 (ORACLE_FINDINGS Section 31) — module-level constants
// ---------------------------------------------------------------------------
// Three faults in one place, and the worst of them was invisible to the
// differential method.
//
//   31.1  `lower_expr_to_const` matched a bare `Lit` and nothing else, with a
//         wildcard returning `IRConst::Null`. `-42` parses as
//         `UnOp(Neg, Lit(42))`, so every negative module-level constant read
//         as 0 — identically on BOTH backends, because the fault sat upstream
//         of the split. Cross-checking t3 against llvm cannot see this; only
//         the absolute assertions below can.
//   31.2  A float global's bits were fed to the TLIT/TMUL immediate builder,
//         which trapped before main: "TMUL overflow: result 4609434218000".
//   31.3  A `str` global stored 0 on T3 (irconst_to_i64's Str arm) while LLVM
//         emitted `global ptr @str0`, so printing one dumped emulator memory.
//
// Every value below is pinned absolutely as well as cross-backend.

#[test]
fn s24_module_level_constants_are_folded_not_zeroed() {
    let src = r#"
let NEG: int   = -42;
let SUB: int   = 0 - 42;
let PROD: int  = 6 * 7;
let POS: int   = 42;
let ONE: int   = -1;
let FLOOR: int = -3812798742493;
let NEST: int  = -(20 + 1) * 2;
let PI2: float = -2.25;
let HALF: float = 1.5;
let SUM: float = 0.5 - 2.75;
let TXT: str   = "hello";
let YES: bool  = true;
let TN: trit   = -;
let T27N: t27  = 0t---------------------------;

fn main() {
    io::print("NEG="); io::println_int(NEG);
    io::print("SUB="); io::println_int(SUB);
    io::print("PROD="); io::println_int(PROD);
    io::print("POS="); io::println_int(POS);
    io::print("ONE="); io::println_int(ONE);
    io::print("FLOOR="); io::println_int(FLOOR);
    io::print("NEST="); io::println_int(NEST);
    io::print("PI2="); io::println_float(PI2);
    io::print("HALF="); io::println_float(HALF);
    io::print("SUM="); io::println_float(SUM);
    io::print("TXT="); io::println(TXT);
    io::print("YES="); io::println_bool(YES);
    io::print("TN="); io::println_trit(TN);
    io::print("T27N="); io::println_int(T27N as int);
}
"#;
    let ((t3_code, t3_out), (ll_code, ll_out)) =
        run_both_backends("s24_globals.mt", src);
    assert_eq!(t3_code, 0, "t3 should exit cleanly, got:\n{}", t3_out);
    assert_eq!(ll_code, 0, "llvm should exit cleanly, got:\n{}", ll_out);
    assert_eq!(t3_out, ll_out, "global initialisers must agree across backends");

    // The absolute half. Before the fix the first seven of these read "=0" on
    // both backends at once, so `t3_out == ll_out` above passed throughout.
    for want in [
        "NEG=-42", "SUB=-42", "PROD=42", "POS=42", "ONE=-1",
        "FLOOR=-3812798742493", "NEST=-42",
        "PI2=-2.25", "HALF=1.5", "SUM=-2.25",
        "TXT=hello", "YES=true", "TN=-", "T27N=-3812798742493",
    ] {
        assert!(t3_out.contains(want), "expected {:?} in output:\n{}", want, t3_out);
    }
}

#[test]
fn s24_an_uncomputable_global_initialiser_is_a_compile_error() {
    // The point of the fix is not only that `-42` now folds, but that anything
    // which does NOT fold is reported instead of becoming a plausible zero.
    assert_check_error(
        "s24_call.mt",
        "fn f() -> int { 7 }\nlet X: int = f();\nfn main() { io::println_int(X); }\n",
        "is not a compile-time constant",
    );
    // An aggregate cannot live in a global's single word. This used to compile
    // to a null pointer and fault on first use.
    assert_check_error(
        "s24_arr.mt",
        "let T: [int; 3] = [1, 2, 3];\nfn main() { io::println_int(T[0]); }\n",
        "is not a compile-time constant",
    );
    // A constant expression that IS evaluable but faults gets its own message,
    // at the line that wrote it rather than at run time.
    assert_check_error(
        "s24_dz.mt",
        "let X: int = 10 / (2 - 2);\nfn main() { io::println_int(X); }\n",
        "divides by zero",
    );
    assert_check_error(
        "s24_ovf.mt",
        "let X: int = 9223372036854775807 + 1;\nfn main() { io::println_int(X); }\n",
        "overflows an int",
    );
}

#[test]
fn s24_the_static_index_check_uses_the_same_folder() {
    // A2's bounds check carried its own literal-only fold, the same shape as
    // the one that made 31.1 possible. Sharing the folder means a computed
    // constant index is caught too — `a[0 - 1]` was previously left to the
    // runtime guard.
    assert_check_error(
        "s24_ix_neg.mt",
        "fn main() { let a = [10, 20, 30]; io::println_int(a[0 - 1]); }\n",
        "out of bounds",
    );
    assert_check_error(
        "s24_ix_big.mt",
        "fn main() { let a = [10, 20, 30]; io::println_int(a[2 * 5]); }\n",
        "out of bounds",
    );
    // ...and an in-range computed index must still be accepted.
    assert_checks(
        "s24_ix_ok.mt",
        "fn main() { let a = [10, 20, 30]; io::println_int(a[1 + 1]); }\n",
    );
}

// ---------------------------------------------------------------------------
// S25 (ORACLE_FINDINGS Section 18) — Result methods, and Result's lifetime
// ---------------------------------------------------------------------------
// `Result` was usable only through `match`: every method passed semantic
// analysis and was emitted by neither backend, so `.unwrap()` failed at link
// (`Undefined label: Result::unwrap` / `undefined value '@Result_unwrap'`).
// The methods now lower to the same loads, compares and branches `match`
// already uses — one body in ir/lower/lower_result.rs, nothing new in either
// backend except the tag guard.
//
// Fixing that exposed a larger fault underneath: T3 built the Result box on the
// STACK and returned R26, while LLVM had always malloc'd. A Result therefore
// died at the `RET` of the function that produced it, and a program that passed
// one to another function printed nothing at all on T3 and was silent about it.

#[test]
fn s25_every_result_method_agrees_across_backends() {
    let src = r#"
fn ok() -> Result<int,str>   { Ok(7) }
fn er() -> Result<int,str>   { Err("boom") }
fn un() -> Result<int,str>   { Unknown("dunno") }
fn fl() -> Result<float,str> { Ok(2.5) }
fn st() -> Result<str,str>   { Ok("hi") }

fn show(name: str, r: Result<int,str>) {
    io::print(name); io::print(" is_ok=");      io::println_bool(r.is_ok());
    io::print(name); io::print(" is_unknown="); io::println_bool(r.is_unknown());
    io::print(name); io::print(" is_err=");     io::println_bool(r.is_err());
    io::print(name); io::print(" or99=");       io::println_int(r.unwrap_or(99));
}

fn main() {
    show("Ok", ok());
    show("Un", un());
    show("Er", er());
    io::print("uw_int = "); io::println_int(ok().unwrap());
    io::print("uw_flt = "); io::println_float(fl().unwrap());
    io::print("uw_str = "); io::println(st().unwrap());
    io::print("tag_ok = "); io::println_trit(ok().tag());
    io::print("tag_un = "); io::println_trit(un().tag());
    io::print("tag_er = "); io::println_trit(er().tag());
}
"#;
    let ((t3_code, t3_out), (ll_code, ll_out)) =
        run_both_backends("s25_methods.mt", src);
    assert_eq!(t3_code, 0, "t3 should exit cleanly, got:\n{}", t3_out);
    assert_eq!(ll_code, 0, "llvm should exit cleanly, got:\n{}", ll_out);
    assert_eq!(t3_out, ll_out, "Result methods must agree across backends");

    for want in [
        "Ok is_ok=true", "Ok is_unknown=false", "Ok is_err=false", "Ok or99=7",
        "Un is_ok=false", "Un is_unknown=true", "Un is_err=false", "Un or99=99",
        "Er is_ok=false", "Er is_unknown=false", "Er is_err=true", "Er or99=99",
        "uw_int = 7", "uw_flt = 2.5", "uw_str = hi",
        // The tag IS a trit, which is the point: one value carries all three
        // outcomes, where is_ok/is_err/is_unknown are its binary decomposition.
        "tag_ok = +", "tag_un = 0", "tag_er = -",
    ] {
        assert!(t3_out.contains(want), "expected {:?} in output:\n{}", want, t3_out);
    }
}

#[test]
fn s25_a_result_outlives_the_function_that_built_it() {
    // T3 allocated the box on the callee's stack and returned R26, so the
    // caller's frame grew straight over it: the tag read as garbage, no arm
    // matched, and the program stopped mid-line with no diagnostic. LLVM had
    // always malloc'd, so this was a representation divergence, not a tuning
    // difference.
    //
    // The producer here is deliberately TINY. A larger one (`if b == 0 { … }
    // Ok(a / b)`) leaves the box deeper in a frame the callee's prologue does
    // not immediately reach, and survives by luck — measured, and it does pass
    // at the previous commit. `fn ok() -> Result<int,str> { Ok(7) }` puts the
    // box at the very bottom of the frame, where the next call's first alloca
    // lands on it.
    let src = r#"
fn ok() -> Result<int,str> { Ok(7) }
fn er() -> Result<int,str> { Err("div by zero") }
fn rewrap(r: Result<int,str>) -> Result<int,str> {
    match r {
        Ok(v)      => Ok(v),
        Err(e)     => Err(e),
        Unknown(m) => Unknown(m),
    }
}
fn consume(name: str, r: Result<int,str>) {
    io::print(name);
    match r {
        Ok(v)      => io::println_int(v),
        Err(e)     => io::println(e),
        Unknown(m) => io::println(m),
    }
}
fn main() {
    consume("passed-ok:  ", ok());
    consume("passed-err: ", er());
    consume("rewrap-ok:  ", rewrap(ok()));
    consume("rewrap-err: ", rewrap(er()));
    io::print("method-across-call: ");
    io::println_int(rewrap(ok()).unwrap_or(0));
    io::println("done");
}
"#;
    let ((t3_code, t3_out), (ll_code, ll_out)) =
        run_both_backends("s25_lifetime.mt", src);
    assert_eq!(t3_code, 0, "t3 should exit cleanly, got:\n{}", t3_out);
    assert_eq!(ll_code, 0, "llvm should exit cleanly, got:\n{}", ll_out);
    assert_eq!(t3_out, ll_out, "a Result must mean the same thing on both backends");

    for want in [
        "passed-ok:  7", "passed-err: div by zero",
        "rewrap-ok:  7", "rewrap-err: div by zero",
        "method-across-call: 7",
        // The truncation was silent, so the last line is the load-bearing one:
        // at the previous commit this program stopped after "passed-ok:  ".
        "done",
    ] {
        assert!(t3_out.contains(want), "expected {:?} in output:\n{}", want, t3_out);
    }
}

#[test]
fn s25_unwrap_on_a_non_ok_result_faults_identically() {
    // A Result has three outcomes and `unwrap` names one of them. The other
    // two fault, with the same message and the same exit status on both
    // backends — the C guard and SYSCALL #561 are the one hand-written pair in
    // the whole Result implementation, so this test is what watches them.
    for (ctor, want) in [
        ("Err(\"boom\")",     "TRAP: unwrap on a Result that is Err"),
        ("Unknown(\"dunno\")", "TRAP: unwrap on a Result that is Unknown"),
    ] {
        let src = format!(
            "fn g() -> Result<int,str> {{ {} }}\n\
             fn main() {{ io::println(\"before\"); io::println_int(g().unwrap()); io::println(\"after\"); }}\n",
            ctor,
        );
        let name = format!("s25_trap_{}.mt", if ctor.starts_with("Err") { "err" } else { "unk" });
        let ((t3_code, t3_out), (ll_code, ll_out)) = run_both_backends(&name, &src);
        assert_eq!(t3_code, 70, "t3 should fault with 70 for {}, got:\n{}", ctor, t3_out);
        assert_eq!(ll_code, 70, "llvm should fault with 70 for {}, got:\n{}", ctor, ll_out);
        for out in [&t3_out, &ll_out] {
            assert!(out.contains("before"), "the program must run up to the unwrap:\n{}", out);
            assert!(out.contains(want), "expected {:?} in:\n{}", want, out);
            assert!(!out.contains("after"), "execution must stop at the unwrap:\n{}", out);
        }
    }
}

#[test]
fn s25_option_is_refused_and_points_at_result() {
    // `Option<T>` was half-declared surface: resolvable as a type, typed for
    // `.unwrap()`, and constructible on neither backend, so `Some(7)` died at
    // assembly with "Undefined label: Some". It is refused rather than
    // implemented because `Result` already carries the third outcome `Option`
    // cannot express — `Unknown` IS `None`.
    assert_check_error(
        "s25_opt_ty.mt",
        "fn f(x: Option<int>) -> int { 0 }\nfn main() { io::println_int(1); }\n",
        "there is no `Option<T>` in ManiT",
    );
    assert_check_error(
        "s25_opt_some.mt",
        "fn main() { let x = Some(7); io::println_int(1); }\n",
        "`Some` is not a ManiT constructor",
    );
    assert_check_error(
        "s25_opt_none.mt",
        "fn main() { let x = None; io::println_int(1); }\n",
        "`None` is not a ManiT constructor",
    );
    // The three real constructors must of course still work.
    assert_checks(
        "s25_opt_ok.mt",
        "fn g() -> Result<int,str> { Ok(1) }\n\
         fn h() -> Result<int,str> { Unknown(\"m\") }\n\
         fn i() -> Result<int,str> { Err(\"e\") }\n\
         fn main() { io::println_int(g().unwrap()); }\n",
    );
}

#[test]
fn s25_a_result_method_with_no_body_is_a_compile_error() {
    // `.map()` was typed by resolve_method_type and emitted by neither backend
    // — the identical silence that made `.unwrap()` a link failure. The
    // semantic pass now checks the method against the list the lowering
    // actually implements, so the two cannot drift apart.
    assert_check_error(
        "s25_map.mt",
        "fn g() -> Result<int,str> { Ok(7) }\nfn main() { let x = g().map(); io::println_int(1); }\n",
        "`Result` has no method `map`",
    );
}

// ---------------------------------------------------------------------------
// S26 (ORACLE_FINDINGS Section 15) — a branch's type comes from all its arms
// ---------------------------------------------------------------------------
// `tif` took its FIRST arm's type and called that the answer; `if` took its
// `else`. Either is arbitrary whenever the arms are compatible but not
// identical, and in a ternary language they very often are — a bare `0` is a
// valid `int` AND a valid `trit`, so which one it is depended on which arm
// happened to be written first:
//
//     tif i { + => +, 0 => +, - => 0 }   typed trit
//     tif i { + => 0, 0 => -, - => - }   typed INT
//
// Two spellings of the same three-valued function. Nested inside a trit-valued
// tif, the second fed an i64 into an i8 phi and clang rejected the module:
// "'%t8' defined with type 'i64' but expected 'i8'". T3 compiled the same
// source correctly, so this one the oracle DID see — as a build failure.

#[test]
fn s26_a_tif_of_tifs_returning_trit_compiles_and_is_correct() {
    // Kleene-style consensus. Every inner tif is a different shape, and the
    // third one — first arm `0` — is the one that used to be typed `int`.
    let src = r#"
fn step(s: trit, i: trit) -> trit {
    return tif s {
        + => tif i { + => +, 0 => +, - => 0 },
        0 => tif i { + => +, 0 => 0, - => - },
        - => tif i { + => 0, 0 => -, - => - }
    };
}
fn main() {
    io::println_trit(step(+, +));
    io::println_trit(step(+, 0));
    io::println_trit(step(+, -));
    io::println_trit(step(0, +));
    io::println_trit(step(0, 0));
    io::println_trit(step(0, -));
    io::println_trit(step(-, +));
    io::println_trit(step(-, 0));
    io::println_trit(step(-, -));
}
"#;
    let ((t3_code, t3_out), (ll_code, ll_out)) = run_both_backends("s26_tif.mt", src);
    assert_eq!(t3_code, 0, "t3 should exit cleanly, got:\n{}", t3_out);
    assert_eq!(ll_code, 0, "llvm should exit cleanly, got:\n{}", ll_out);
    assert_eq!(t3_out, ll_out, "a tif of tifs must agree across backends");
    // All nine cells, read straight off the source.
    assert_eq!(t3_out, "+\n+\n0\n+\n0\n-\n0\n-\n-\n");
}

#[test]
fn s26_arm_order_does_not_change_a_branch_type() {
    // A false-positive guard, not a test of the fix: measured, all three of
    // these pass without it. An `int`-typed tif whose value flows straight into
    // a `trit` return is coerced at the boundary and gives the right answer —
    // which is exactly why the mistyping stayed invisible until one was nested
    // inside another (the test above). This pins that widening the rule did not
    // break the plain cases.
    let src = r#"
fn zero_first(t: trit) -> trit { tif t { + => 0, 0 => -, - => - } }
fn trit_first(t: trit) -> trit { tif t { + => +, 0 => 0, - => - } }
fn via_if(b: bool, t: trit) -> trit { if b { 0 } else { t } }
fn main() {
    io::println_trit(zero_first(+));
    io::println_trit(zero_first(0));
    io::println_trit(zero_first(-));
    io::println_trit(trit_first(+));
    io::println_trit(trit_first(0));
    io::println_trit(trit_first(-));
    io::println_trit(via_if(true, -));
    io::println_trit(via_if(false, -));
}
"#;
    let ((t3_code, t3_out), (ll_code, ll_out)) = run_both_backends("s26_order.mt", src);
    assert_eq!(t3_code, 0, "t3 should exit cleanly, got:\n{}", t3_out);
    assert_eq!(ll_code, 0, "llvm should exit cleanly, got:\n{}", ll_out);
    assert_eq!(t3_out, ll_out, "arm order must not change the result");
    assert_eq!(t3_out, "0\n-\n-\n+\n0\n-\n0\n-\n");
}

#[test]
fn s26_a_computed_int_arm_keeps_the_branch_an_int() {
    // The guard on the rule. Only a bare literal in -1..=1 lets a ternary arm
    // pull the whole expression to `trit`; an `int` arm that is COMPUTED keeps
    // it an `int`, because narrowing a computed integer would turn a build
    // failure into a wrong answer. 1000 must survive.
    let src = r#"
fn pick(b: bool, t: trit) -> int {
    if b { 500 + 500 } else { t as int }
}
fn main() {
    io::println_int(pick(true, +));
    io::println_int(pick(false, -));
}
"#;
    let ((t3_code, t3_out), (ll_code, ll_out)) = run_both_backends("s26_wide.mt", src);
    assert_eq!(t3_code, 0, "t3 should exit cleanly, got:\n{}", t3_out);
    assert_eq!(ll_code, 0, "llvm should exit cleanly, got:\n{}", ll_out);
    assert_eq!(t3_out, ll_out, "a computed int arm must agree across backends");
    assert_eq!(t3_out, "1000\n-1\n");
}

// ---------------------------------------------------------------------------
// S27 — the stdlib reference must match the stdlib
// ---------------------------------------------------------------------------
// docs/stdlib-reference.md was hand-written on 8 April 2026 and never revisited.
// By 21 August it covered roughly a third of the surface and said nothing about
// whether the entries it listed existed — which is the declared-vs-defined
// problem in prose form, the same one that let `fmt::` document twenty-five
// functions defined nowhere.
//
// It is generated now, and generated by MEASUREMENT: tools/stdlib_census.py
// reads every declaration out of stdlib/*.mt and then calls each one on both
// backends. This test is what stops it going stale again — a doc nothing checks
// is a doc that drifts, and four months is how long that took last time.

#[test]
// WHAT THIS GUARANTEES, AND WHAT IT DOES NOT (noted 26 Aug 2026). `--check`
// re-derives the doc from the live stdlib sources and diffs it against the
// stored file, so it catches a doc that has fallen behind the source — which
// is what it is for, and it caught exactly that twice today when stdlib
// bodies were edited.
//
// It does NOT establish that the doc is CORRECT. A bug in the scanner produces
// a consistent wrong doc and this passes, because the generator and the
// checker derive from the same code. That is the day's recurring shape — two
// things compared that share the fault, so their agreement carries no
// information (report.txt P44, P59, P62) — and it is stated here rather than
// repaired because freshness is the property actually wanted from a generated
// file. Do not read a green result as "the reference is right".
fn s27_the_stdlib_reference_is_current() {
    let out = Command::new("python3")
        .args([
            concat!(env!("CARGO_MANIFEST_DIR"), "/tools/stdlib_census.py"),
            "--check",
            "--manitc",
        ])
        .arg(get_manitc())
        .output()
        .expect("failed to run tools/stdlib_census.py");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    assert!(
        out.status.success(),
        "docs/stdlib-reference.md is out of date. Run:\n    \
         python3 tools/stdlib_census.py\n\n{}",
        text,
    );
}

// ---------------------------------------------------------------------------
// S28 (ORACLE_FINDINGS Section 33.1) — ternary logic on bool operands
// ---------------------------------------------------------------------------
// `tand` refused two `bool`s, which made `a > b tand c > d` unwritable: a
// comparison produces `bool` and nothing produces a trit. Four real sources did
// not compile, two of them in thatteOS.
//
// The refusal was not baseless — `tand` lowers to TritBranch and TritMin, which
// read their operand as -1/0/+1, so a raw `bool` would make `false` mean
// UNKNOWN. But the fix is to convert, not to refuse, and the `bool` to `bool3`
// coercion was already blessed by the language and already emitted by
// `coerce_value`.
//
// And for three of the five operators the conversion is not even needed: with
// false as -1 and true as +1, min, max and "either +1 wins" are CLOSED on
// {-1, +1}, so `tand`/`tor`/`tany` on two bools ARE `&&`/`||`/`||` and give a
// `bool` that an `if` will take. `txor` and `tcon` are not closed — both reach
// `unknown` from two-valued inputs — so they stay three-valued.

#[test]
fn s28_ternary_logic_accepts_bool_operands() {
    let src = r#"
fn tt(a: bool, b: bool) -> bool  { a tand b }
fn to(a: bool, b: bool) -> bool  { a tor  b }
fn ty(a: bool, b: bool) -> bool  { a tany b }
fn tx(a: bool, b: bool) -> bool3 { a txor b }
fn tc(a: bool, b: bool) -> bool3 { a tcon b }
fn mixed(a: bool, t: bool3) -> bool3 { a tand t }
fn main() {
    io::print("tand "); io::println_bool(tt(true, true));
    io::print("tand "); io::println_bool(tt(true, false));
    io::print("tand "); io::println_bool(tt(false, false));
    io::print("tor  "); io::println_bool(to(true, false));
    io::print("tor  "); io::println_bool(to(false, false));
    io::print("tany "); io::println_bool(ty(true, false));
    io::print("tany "); io::println_bool(ty(false, false));
    io::print("txor "); io::println_bool3(tx(true, true));
    io::print("txor "); io::println_bool3(tx(true, false));
    io::print("txor "); io::println_bool3(tx(false, false));
    io::print("tcon "); io::println_bool3(tc(true, false));
    io::print("mixd "); io::println_bool3(mixed(true, unknown));
    io::print("mixd "); io::println_bool3(mixed(false, unknown));
    let a = 5;
    let b = 3;
    if a > b tand a > 0 { io::println("if-tand works"); }
    if a < b tor a > 0 { io::println("if-tor works"); }
}
"#;
    let ((t3_code, t3_out), (ll_code, ll_out)) = run_both_backends("s28_tand.mt", src);
    assert_eq!(t3_code, 0, "t3 should exit cleanly, got:\n{}", t3_out);
    assert_eq!(ll_code, 0, "llvm should exit cleanly, got:\n{}", ll_out);
    assert_eq!(t3_out, ll_out, "ternary logic on bools must agree across backends");
    assert_eq!(
        t3_out,
        // tand/tor/tany reduce to boolean and/or exactly. txor is mod-3
        // addition, so true txor true is FALSE and true txor false is UNKNOWN —
        // it is not boolean xor, and this pins that it is not quietly made into
        // one. tcon of two disagreeing bools is unknown by definition.
        "tand true\ntand false\ntand false\n\
         tor  true\ntor  false\n\
         tany true\ntany false\n\
         txor false\ntxor unknown\ntxor true\n\
         tcon unknown\n\
         mixd unknown\nmixd false\n\
         if-tand works\nif-tor works\n",
    );
}

#[test]
fn s28_the_two_open_operators_still_need_a_tif() {
    // `txor` and `tcon` on two bools yield `bool3`, so an `if` must reject
    // them. That is the guard on the rule above: only the three operators that
    // are provably closed on {-1, +1} collapse to `bool`, and it would be a
    // silent loss of the third state to let the other two through.
    assert_check_error(
        "s28_txor_if.mt",
        "fn main() { if true txor false { io::println(\"x\"); } }\n",
        "if condition must be `bool`",
    );
    assert_check_error(
        "s28_tcon_if.mt",
        "fn main() { if true tcon false { io::println(\"x\"); } }\n",
        "if condition must be `bool`",
    );
    // Non-ternary, non-bool operands are still refused outright.
    assert_check_error(
        "s28_str.mt",
        "fn main() { let x = \"a\" tand \"b\"; io::println(\"x\"); }\n",
        "cannot be applied to",
    );
}

// ---------------------------------------------------------------------------
// S29 — a block's register state must come from a PREDECESSOR, not from
// whichever block happened to be emitted just before it (ORACLE_FINDINGS §38).
// ---------------------------------------------------------------------------

#[test]
fn s29_a_loop_variable_survives_a_call_in_one_branch_arm() {
    // The last of the 17 examples to diverge. `examples/data_structures.mt`
    // counted five distinct words on LLVM and two on T3, attributing nine of
    // the ten occurrences to the empty string.
    //
    // The array base pointer lived in R3. The `then` arm rescued it out of R3
    // before a syscall clobbered R3 as an argument register; the `else` arm,
    // emitted immediately after, INHERITED the then-arm's allocator state,
    // concluded R3 was already free, and clobbered it without rescuing. The
    // merge then restored R3 from the register only the then-arm had written.
    //
    // So: on any iteration that took the else arm, the array base became the
    // `contains_key` result — 0 — and every later iteration read its key from
    // address zero. The first word survives, which is what made the output
    // look like a hash collision rather than a wild pointer.
    let src = r#"
use std::io;
use std::collections;
fn main() {
    let words: [str] = ["aa", "bb", "aa", "cc"];
    let freq: Map<str, int> = Map::new();
    for w in words {
        if freq.contains_key(w) { freq.insert(w, freq.get(w) + 1); } else { freq.insert(w, 1); }
        io::print("["); io::print(w); io::println("]");
    }
    let ks = freq.keys();
    ks.sort();
    let mut i = 0;
    while i < ks.len() {
        io::print(ks.get(i)); io::print("="); io::println_int(freq.get(ks.get(i)));
        i = i + 1;
    }
}
"#;
    let ((t3_code, t3_out), (ll_code, ll_out)) =
        run_both_backends("s29_loopvar_branch.mt", src);
    assert_eq!(t3_code, 0, "t3 should exit cleanly, got:\n{}", t3_out);
    assert_eq!(ll_code, 0, "llvm should exit cleanly, got:\n{}", ll_out);
    assert_eq!(t3_out, ll_out, "both backends must see the same keys");

    // Absolute values, not just agreement: the oracle's blind spot is a fault
    // upstream of both backends, which would make them agree on nonsense.
    for want in ["[aa]", "[bb]", "[cc]", "aa=2", "bb=1", "cc=1"] {
        assert!(t3_out.contains(want), "expected {:?} in output:\n{}", want, t3_out);
    }
    assert!(!t3_out.contains("[]"), "a loop variable went empty:\n{}", t3_out);
}

#[test]
fn s29_every_arm_of_a_match_writes_its_phi_to_the_same_place() {
    // The other half of the same fix, and the reason the first half could not
    // be applied alone. Once each arm is restored to a common entry state, each
    // arm allocates independently — so three arms rebuilding one value picked
    // three DIFFERENT destination registers for the same phi, and the merge
    // read whichever the first arm chose. Arms two and three lost their value.
    //
    // A phi destination is the one binding that is a contract between blocks
    // rather than a fact about one path, so it is pinned once and reused.
    //
    // Measured, so the record is straight: this test PASSES at the previous
    // commit, because there the arms shared state by accident and landed on one
    // register anyway. It fails only when the entry-state restore above is in
    // place and the phi pin is taken out — which is what it guards. It is a
    // regression test for the fix, not a reproduction of the original bug.
    let src = r#"
use std::io;
fn classify(t: trit) -> str {
    let s: str = tif t { + => "pos", 0 => "zero", - => "neg" };
    return s;
}
fn rewrap(r: Result<int,str>) -> Result<int,str> {
    match r {
        Ok(v)      => Ok(v),
        Err(e)     => Err(e),
        Unknown(m) => Unknown(m),
    }
}
fn show(name: str, r: Result<int,str>) {
    io::print(name);
    match r {
        Ok(v)      => io::println_int(v),
        Err(e)     => io::println(e),
        Unknown(m) => io::println(m),
    }
}
fn main() {
    io::println(classify(+));
    io::println(classify(0));
    io::println(classify(-));
    show("ok:  ", rewrap(Ok(7)));
    show("err: ", rewrap(Err("boom")));
    show("unk: ", rewrap(Unknown("dunno")));
}
"#;
    let ((t3_code, t3_out), (ll_code, ll_out)) =
        run_both_backends("s29_phi_home.mt", src);
    assert_eq!(t3_code, 0, "t3 should exit cleanly, got:\n{}", t3_out);
    assert_eq!(ll_code, 0, "llvm should exit cleanly, got:\n{}", ll_out);
    assert_eq!(t3_out, ll_out, "every arm must deliver its own value");

    for want in ["pos", "zero", "neg", "ok:  7", "err: boom", "unk: dunno"] {
        assert!(t3_out.contains(want), "expected {:?} in output:\n{}", want, t3_out);
    }
}

// ---------------------------------------------------------------------------
// S30 — Map and Set iterate in INSERTION order, on both backends
// (ORACLE_FINDINGS §39).
// ---------------------------------------------------------------------------

#[test]
fn s30_map_and_set_iterate_in_insertion_order() {
    // Iteration order used to be whatever the storage structure gave. Inserting
    // 50, 10, 30, 20, 40:
    //
    //     LLVM  40 50 30 20 10     open-addressed hash table, slots 0..cap
    //     T3    10 20 30 40 50     BTreeMap/BTreeSet, key order
    //
    // Neither is a property of the program, so the same source printed two
    // different things. Insertion order is a property of the program, and it is
    // the only order the two backends can agree on without knowing the key
    // type: keys reach the runtime type-erased as i64, and a string key is a
    // pointer on LLVM but an intern id on T3.
    //
    // The literal sequence below is pinned deliberately. Asserting only that
    // the backends agree would pass if both regressed to the same wrong order.
    let src = r#"
use std::io;
use std::collections;
fn main() {
    let xs: [int] = [50, 10, 30, 20, 40];
    let s: Set<int> = Set::new();
    for x in xs { s.insert(x); }
    io::print("set: ");
    s.for_each(fn(x: int) { io::print_int(x); io::print(" "); });
    io::println("");

    let m: Map<int, int> = Map::new();
    for x in xs { m.insert(x, x * 2); }
    let ks = m.keys();
    let vs = m.values();
    io::print("map: ");
    let mut i = 0;
    while i < ks.len() {
        io::print_int(ks.get(i)); io::print("->"); io::print_int(vs.get(i)); io::print(" ");
        i = i + 1;
    }
    io::println("");

    // A re-inserted key keeps its original position: it was already present,
    // so its insertion has already happened. Only the value moves.
    m.insert(50, 999);
    io::print("re:  ");
    let k2 = m.keys();
    let v2 = m.values();
    let mut j = 0;
    while j < k2.len() {
        io::print_int(k2.get(j)); io::print("->"); io::print_int(v2.get(j)); io::print(" ");
        j = j + 1;
    }
    io::println("");

    // Removal takes the key out of the sequence, leaving the rest in order.
    m.remove(30);
    io::print("rm:  ");
    let k3 = m.keys();
    let mut n = 0;
    while n < k3.len() { io::print_int(k3.get(n)); io::print(" "); n = n + 1; }
    io::println("");

    // Set algebra takes its order from the operands: self's order first, and
    // for union, whatever the other side adds, in the other side's order.
    let a: Set<int> = Set::new();
    for x in xs { a.insert(x); }
    let b: Set<int> = Set::new();
    b.insert(30); b.insert(70); b.insert(50);
    io::print("int: ");
    a.intersection(b).for_each(fn(x: int) { io::print_int(x); io::print(" "); });
    io::println("");
    io::print("uni: ");
    a.union(b).for_each(fn(x: int) { io::print_int(x); io::print(" "); });
    io::println("");
    io::print("dif: ");
    a.difference(b).for_each(fn(x: int) { io::print_int(x); io::print(" "); });
    io::println("");
}
"#;
    let ((t3_code, t3_out), (ll_code, ll_out)) =
        run_both_backends("s30_insertion_order.mt", src);
    assert_eq!(t3_code, 0, "t3 should exit cleanly, got:\n{}", t3_out);
    assert_eq!(ll_code, 0, "llvm should exit cleanly, got:\n{}", ll_out);
    assert_eq!(t3_out, ll_out, "iteration order is part of the language");

    for want in [
        "set: 50 10 30 20 40",
        "map: 50->100 10->20 30->60 20->40 40->80",
        "re:  50->999 10->20 30->60 20->40 40->80",
        "rm:  50 10 20 40",
        "int: 50 30",
        "uni: 50 10 30 20 40 70",
        "dif: 10 20 40",
    ] {
        assert!(t3_out.contains(want), "expected {:?} in output:\n{}", want, t3_out);
    }
}

// ---------------------------------------------------------------------------
// S31 — a `str` inside a collection is compared as TEXT, not as an address
// (ORACLE_FINDINGS §40).
// ---------------------------------------------------------------------------

#[test]
fn s31_collections_of_str_compare_text_not_identity() {
    // `str` is a value for `==` but was an IDENTITY once inside a collection:
    // every element reaches the runtime type-erased to i64, which is a pointer
    // natively and an intern id on T3. Measured, against a key built at run
    // time carrying the same text as a literal already in the collection:
    //
    //     vec.contains        LLVM false   T3 false    <- both wrong, agreeing
    //     vec.index_of        LLVM -1      T3 -1       <- both wrong, agreeing
    //     map.contains_key    LLVM false   T3 true     <- diverge, LLVM wrong
    //     set.contains        LLVM false   T3 true     <- diverge, LLVM wrong
    //     insert same text 2x LLVM len 2   T3 len 1    <- diverge, LLVM wrong
    //     sort()              unsorted     unsorted    <- both wrong, agreeing
    //
    // The three that AGREE are the reason this test pins absolute answers.
    // Comparing the backends could never have found them: the fault is upstream
    // of both, so both produce the same wrong result. Only asking what the
    // answer should BE finds those.
    let src = r#"
use std::io;
use std::str;
use std::collections;
fn yn(b: bool) -> str { if b { return "yes"; } return "no"; }
fn main() {
    // Same text as the literal "ab", but assembled at run time.
    let built = str::concat("a", "b");

    let v: Vec<str> = Vec::new();
    v.push("ab"); v.push("cd");
    io::print("vec.contains  = "); io::println(yn(v.contains(built)));
    io::print("vec.index_of  = "); io::println_int(v.index_of(built));

    let m: Map<str,int> = Map::new();
    m.insert("ab", 7);
    io::print("map.contains  = "); io::println(yn(m.contains_key(built)));
    io::print("map.get       = "); io::println_int(m.get(built));
    io::print("map.get_or    = "); io::println_int(m.get_or(built, 99));
    m.insert(built, 8);
    io::print("map.len       = "); io::println_int(m.len());
    m.remove(built);
    io::print("map.len rm    = "); io::println_int(m.len());

    let s: Set<str> = Set::new();
    s.insert("ab");
    io::print("set.contains  = "); io::println(yn(s.contains(built)));
    s.insert(built);
    io::print("set.len       = "); io::println_int(s.len());

    // Set algebra needs no str-aware form of its own: once every stored
    // element is canonical, comparing stored entries is already correct.
    let t: Set<str> = Set::new();
    t.insert(str::concat("a", "b"));
    io::print("intersection  = ");
    s.intersection(t).for_each(fn(x: str) { io::print(x); io::print(" "); });
    io::println("");

    let w: Vec<str> = Vec::new();
    w.push("pear"); w.push("apple"); w.push("fig");
    w.sort();
    io::print("sorted        = ");
    w.for_each(fn(x: str) { io::print(x); io::print(" "); });
    io::println("");
}
"#;
    let ((t3_code, t3_out), (ll_code, ll_out)) =
        run_both_backends("s31_str_identity.mt", src);
    assert_eq!(t3_code, 0, "t3 should exit cleanly, got:\n{}", t3_out);
    assert_eq!(ll_code, 0, "llvm should exit cleanly, got:\n{}", ll_out);
    assert_eq!(t3_out, ll_out, "a str key must mean the same on both backends");

    for want in [
        "vec.contains  = yes",
        "vec.index_of  = 0",
        "map.contains  = yes",
        "map.get       = 7",
        "map.get_or    = 7",
        "map.len       = 1",   // re-inserting the same TEXT is not a new key
        "map.len rm    = 0",   // and removing by computed text finds it
        "set.contains  = yes",
        "set.len       = 1",
        "intersection  = ab",
        "sorted        = apple fig pear",
    ] {
        assert!(t3_out.contains(want), "expected {:?} in output:\n{}", want, t3_out);
    }
}

// ---------------------------------------------------------------------------
// A narrow-typed native must not store past the end of its stack slot.
//
// `tryte` lowers to i16 and `t9` to i32, but the LLVM helpers
// @ternary_tryte_from_trits, @ternary_int_to_tryte and @ternary_int_to_t9 all
// return i64. The Store emitter reasoned only about the value being NARROWER
// than the slot (sext); a WIDER value fell through to storing at the value's
// own width, so
//     %t0 = alloca i16, align 2
//     store i64 %t1, ptr %t0, align 2
// wrote six bytes past a two-byte allocation, through the return address.
//
// What makes this class hard to see: the stored VALUE is correct and the
// function returns the right answer. `tests/23_t3isa_instructions.mt` printed
// all 132 of its PASS lines and then segfaulted on the way out — so the failure
// was invisible to anything reading stdout for correctness, and the two-backend
// oracle recorded it as "llvm did not run" rather than as a wrong answer.
// ---------------------------------------------------------------------------

#[test]
fn narrow_native_return_does_not_overrun_its_slot() {
    let src = r#"
use std::io;
use std::ternary;
fn main() {
    let ty = ternary::tryte_from_trits(+, 0, -);
    io::println_int(ternary::tryte_to_int(ty));
    let t2 = ternary::int_to_tryte(8);
    io::println_int(ternary::tryte_to_int(t2));
    let n9 = ternary::int_to_t9(100);
    io::println_int(ternary::t9_to_int(n9));
    io::println("returned cleanly");
}
"#;
    let ((t3_code, t3_out), (ll_code, ll_out)) =
        run_both_backends("narrow_slot.mt", src);

    // The crash was on RETURN, after every value had been printed, so exit
    // status is the assertion that matters — not stdout.
    assert_eq!(ll_code, 0, "llvm must exit cleanly (139 = the stack smash):\n{}", ll_out);
    assert_eq!(t3_code, 0, "t3 must exit cleanly:\n{}", t3_out);
    assert_eq!(t3_out, ll_out, "backends must agree:\nT3:\n{}\nLLVM:\n{}", t3_out, ll_out);

    // And the values must be right, so a future "fix" cannot buy the exit code
    // by truncating something real.
    for want in ["8\n", "8\n", "100\n", "returned cleanly"] {
        assert!(ll_out.contains(want), "expected {:?} in:\n{}", want, ll_out);
    }
}

#[test]
fn every_stdlib_test_program_survives_its_own_return() {
    // 23_t3isa_instructions.mt is the program that exposed the above. It is run
    // by the suite already, but nothing checked its EXIT STATUS — it printed
    // 132 PASS lines and died, and passed. Pin the whole file end to end.
    let src = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/23_t3isa_instructions.mt"),
    )
    .expect("23_t3isa_instructions.mt");
    let ((t3_code, t3_out), (ll_code, ll_out)) =
        run_both_backends("t3isa_full.mt", &src);

    assert_eq!(ll_code, 0, "llvm must exit cleanly:\n{}", ll_out);
    assert_eq!(t3_code, 0, "t3 must exit cleanly:\n{}", t3_out);
    assert_eq!(t3_out, ll_out, "backends must agree on all 132 lines");
    assert!(!ll_out.contains("FAIL"), "a check failed:\n{}", ll_out);
    assert!(ll_out.trim_end().ends_with("Done."), "program did not finish:\n{}", ll_out);
}

// ---------------------------------------------------------------------------
// `x = <struct>` on an existing struct variable must copy, not alias.
//
// A struct local's storage IS its pointer, so the generic assignment path
// emitted `store ptr %new, ptr %x` — writing the new struct's ADDRESS into x's
// FIRST FIELD. `x.a` came back as a pointer (moving between runs with ASLR on
// LLVM, a T3 address on T3) and `x.b` kept its old value forever, because the
// freshly built struct was discarded and nothing ever read it.
//
// Both fields wrong, on both backends, for the plainest assignment in the
// language. It survived because the two-backend oracle sees agreement, not
// correctness, and here the backends disagreed only in WHICH wrong number they
// printed — so it was filed as a divergence in ten thatteOS programs rather
// than recognised as one miscompile.
// ---------------------------------------------------------------------------

#[test]
fn assigning_a_struct_to_an_existing_variable_copies_it() {
    let src = r#"
use std::io;
struct P { pub a: int, pub b: int }
fn bump(p: P) -> P { return P { a: p.a, b: p.b + 1 }; }
fn main() {
    let mut x = P { a: 42, b: 7 };
    io::print("a="); io::print_int(x.a); io::print(" b="); io::println_int(x.b);
    x = bump(x);
    io::print("a="); io::print_int(x.a); io::print(" b="); io::println_int(x.b);
    x = bump(x);
    io::print("a="); io::print_int(x.a); io::print(" b="); io::println_int(x.b);
}
"#;
    let ((t3_code, t3_out), (ll_code, ll_out)) = run_both_backends("struct_assign.mt", src);
    assert_eq!(ll_code, 0, "llvm:\n{}", ll_out);
    assert_eq!(t3_code, 0, "t3:\n{}", t3_out);

    // Absolute values, not just agreement: the two backends AGREED on nothing
    // here, but a fix that made them agree on the wrong number would still be
    // wrong. `a` must never move, and `b` must actually advance.
    let want = "a=42 b=7\na=42 b=8\na=42 b=9\n";
    assert_eq!(ll_out, want, "llvm output");
    assert_eq!(t3_out, want, "t3 output");
}

#[test]
fn assigning_to_a_struct_typed_field_still_works() {
    // The sibling case, deliberately left on the generic path: a struct-typed
    // FIELD slot really does hold a pointer, so storing one there is correct.
    // Pinned so the fix above is not later "generalised" onto it.
    let src = r#"
use std::io;
struct In  { pub p: int, pub q: int }
struct Out { pub tag: int, pub inner: In }
fn mk(p: int) -> In { return In { p: p, q: p * 10 }; }
fn main() {
    let mut o = Out { tag: 99, inner: mk(1) };
    io::print("tag="); io::print_int(o.tag);
    io::print(" p="); io::print_int(o.inner.p);
    io::print(" q="); io::println_int(o.inner.q);
    o.inner = mk(5);
    io::print("tag="); io::print_int(o.tag);
    io::print(" p="); io::print_int(o.inner.p);
    io::print(" q="); io::println_int(o.inner.q);
}
"#;
    let ((_, t3_out), (_, ll_out)) = run_both_backends("struct_field_assign.mt", src);
    let want = "tag=99 p=1 q=10\ntag=99 p=5 q=50\n";
    assert_eq!(ll_out, want, "llvm output");
    assert_eq!(t3_out, want, "t3 output");
}

// ---------------------------------------------------------------------------
// S45 (ORACLE_FINDINGS Section 45) — a bool argument reaching a bool3 parameter
// of a NATIVE stdlib declaration
// ---------------------------------------------------------------------------
//
// `IRLowerer` coerces call arguments to the callee's declared parameter type,
// and it learned those types from `TypedProgram::functions`. A native stdlib
// function — `io::println_bool3`, whose body is in the backends and not in
// `.mt` source — has no body, so it never appears there, and a MISSING entry
// was indistinguishable from a function with NO parameters: the coercion was
// skipped and a raw `bool` went through unconverted.
//
// bool3 is -1/0/+1 and bool is 0/1, so `false` (0) landed on bool3's UNKNOWN.
// That is why these tests assert the ABSOLUTE strings and not merely that the
// backends agree: on `false` the two backends agreed with each other and were
// both wrong, which is exactly the failure mode a differential oracle cannot
// see. Only the accidental disagreement on `true` made any of it visible.

#[test]
fn s45_bool_reaches_a_native_bool3_parameter_as_bool3() {
    let src = r#"
use std::io;
fn no() -> bool { return false; }
fn main() {
    io::println_bool3(true);
    io::println_bool3(false);
    io::println_bool3(no());
    io::println_bool3(1 == 2);
    io::print_bool3(false);
    io::newline();
}
"#;
    let ((t3_code, t3_out), (ll_code, ll_out)) = run_both_backends("s45_native_bool3.mt", src);
    assert_eq!(ll_code, 0, "llvm:\n{}", ll_out);
    assert_eq!(t3_code, 0, "t3:\n{}", t3_out);
    let want = "true\nfalse\nfalse\nfalse\nfalse\n";
    assert_eq!(ll_out, want, "llvm output");
    assert_eq!(t3_out, want, "t3 output");
}

#[test]
fn s45_a_real_bool3_argument_is_not_converted_twice() {
    // The other half of the fix: coercion must fire on bool->bool3 and on
    // nothing else. A value that is ALREADY bool3 must arrive untouched — a
    // second `2*b - 1` would send unknown(0) to false(-1) and false(-1) to -3.
    let src = r#"
use std::io;
fn main() {
    let a: bool3 = unknown;
    let b: bool3 = true;
    let c: bool3 = false;
    io::println_bool3(a);
    io::println_bool3(b);
    io::println_bool3(c);
    io::println_bool(false);
    io::println_bool(true);
}
"#;
    let ((_, t3_out), (_, ll_out)) = run_both_backends("s45_bool3_passthrough.mt", src);
    let want = "unknown\ntrue\nfalse\nfalse\ntrue\n";
    assert_eq!(ll_out, want, "llvm output");
    assert_eq!(t3_out, want, "t3 output");
}

#[test]
fn s45_source_written_stdlib_bool3_parameters_still_work() {
    // `fmt::show_bool3` and `str::from_bool3` are ManiT source, so they reached
    // `TypedProgram::functions` and always had their parameter type. Pinned so
    // that seeding the native declarations first — which is what makes the
    // tests above pass — cannot later shadow a real definition of the same name.
    let src = r#"
use std::io;
use std::fmt;
use std::str;
fn main() {
    io::println(fmt::show_bool3(false));
    io::println(fmt::show_bool3(true));
    io::println(str::from_bool3(false));
}
"#;
    let ((_, t3_out), (_, ll_out)) = run_both_backends("s45_source_bool3.mt", src);
    let want = "False\nTrue\nFalse\n";
    assert_eq!(ll_out, want, "llvm output");
    assert_eq!(t3_out, want, "t3 output");
}

// ---------------------------------------------------------------------------
// S44 (ORACLE_FINDINGS Section 44) — a syscall argument overwritten by the next
// ---------------------------------------------------------------------------
//
// `fmt::format` is variadic: `lower_expr` splats the substitution array, so the
// call reaches `emit_syscall_2arg_ret` with a template plus one argument per
// `{}`. That helper used to resolve every argument with `val_reg` and collect
// the results before moving any of them. `val_reg` does not merely report where
// a value lives — for a SPILLED temp it emits a reload into a scratch register,
// and there are three of those (R21, R22, then R25 for every reload after the
// second). Four spilled arguments therefore reloaded the third and fourth into
// the same register, and two arguments arrived holding one value.
//
// The same defect had already been found and fixed for the general call path;
// the syscall helpers were simply never migrated to it. They share
// `emit_call_operands` now.

#[test]
fn s44_a_wide_format_call_does_not_reuse_an_argument() {
    // Needs BOTH the guarded reassignments and the loop: they are what push
    // enough temps into spill slots for the third and fourth reload to collide.
    let src = r#"
use std::io;
use std::fmt;
use std::ternary;
fn main() {
    let ts: [trit; 3] = [-, 0, +];
    for i in 0..3 {
        for j in 0..3 {
            let a: trit = ts[i];
            let b: trit = ts[j];
            let ai: int = ternary::trit_to_int(a);
            let bi: int = ternary::trit_to_int(b);
            let mut lo: int = ai;
            if bi < lo { lo = bi; }
            let mut hi: int = ai;
            if bi > hi { hi = bi; }
            let mut m1: str = "no";
            if ternary::trit_to_int(a tand b) == lo { m1 = "yes"; }
            let mut m2: str = "no";
            if ternary::trit_to_int(a tor b) == hi { m2 = "yes"; }
            io::println(fmt::format("{} {} tand={} tor={} txor={} min={} max={}", [
                ternary::trits_to_str([a]), ternary::trits_to_str([b]),
                ternary::trits_to_str([a tand b]),
                ternary::trits_to_str([a tor b]),
                ternary::trits_to_str([a txor b]), m1, m2]));
        }
    }
}
"#;
    let ((t3_code, t3_out), (ll_code, ll_out)) = run_both_backends("s44_wide_format.mt", src);
    assert_eq!(ll_code, 0, "llvm:\n{}", ll_out);
    assert_eq!(t3_code, 0, "t3:\n{}", t3_out);

    // Absolute, not agreement — and not merely "the two columns differ" either.
    // `tand` is min and `tor` is max in balanced ternary, so the whole 3x3 table
    // is checked against the arithmetic rather than against the other backend.
    let want = "\
- - tand=- tor=- txor=+ min=yes max=yes
- 0 tand=- tor=0 txor=- min=yes max=yes
- + tand=- tor=+ txor=0 min=yes max=yes
0 - tand=- tor=0 txor=- min=yes max=yes
0 0 tand=0 tor=0 txor=0 min=yes max=yes
0 + tand=0 tor=+ txor=+ min=yes max=yes
+ - tand=- tor=+ txor=0 min=yes max=yes
+ 0 tand=0 tor=+ txor=+ min=yes max=yes
+ + tand=+ tor=+ txor=- min=yes max=yes
";
    assert_eq!(ll_out, want, "llvm output");
    assert_eq!(t3_out, want, "t3 output");
}

#[test]
fn s44_binding_the_arguments_first_does_not_move_the_corruption() {
    // The same call with every argument bound to a local. This shape was ALSO
    // wrong before the fix, and differently: the collision landed on argument 5
    // instead of argument 3, so `txor` printed `m1`'s "yes". Pinned because it
    // is what showed the fault was positional pressure on the scratch registers
    // and not anything about `tand`.
    let src = r#"
use std::io;
use std::fmt;
use std::ternary;
fn main() {
    let ts: [trit; 3] = [-, 0, +];
    for i in 0..3 {
        for j in 0..3 {
            let a: trit = ts[i];
            let b: trit = ts[j];
            let ai: int = ternary::trit_to_int(a);
            let bi: int = ternary::trit_to_int(b);
            let mut lo: int = ai;
            if bi < lo { lo = bi; }
            let mut hi: int = ai;
            if bi > hi { hi = bi; }
            let mut m1: str = "no";
            if ternary::trit_to_int(a tand b) == lo { m1 = "yes"; }
            let mut m2: str = "no";
            if ternary::trit_to_int(a tor b) == hi { m2 = "yes"; }
            let sa: str = ternary::trits_to_str([a]);
            let sb: str = ternary::trits_to_str([b]);
            let sand: str = ternary::trits_to_str([a tand b]);
            let sor: str = ternary::trits_to_str([a tor b]);
            let sxor: str = ternary::trits_to_str([a txor b]);
            io::println(fmt::format("{} {} tand={} tor={} txor={} min={} max={}", [
                sa, sb, sand, sor, sxor, m1, m2]));
        }
    }
}
"#;
    let ((_, t3_out), (_, ll_out)) = run_both_backends("s44_bound_format.mt", src);
    let want = "\
- - tand=- tor=- txor=+ min=yes max=yes
- 0 tand=- tor=0 txor=- min=yes max=yes
- + tand=- tor=+ txor=0 min=yes max=yes
0 - tand=- tor=0 txor=- min=yes max=yes
0 0 tand=0 tor=0 txor=0 min=yes max=yes
0 + tand=0 tor=+ txor=+ min=yes max=yes
+ - tand=- tor=+ txor=0 min=yes max=yes
+ 0 tand=0 tor=+ txor=+ min=yes max=yes
+ + tand=+ tor=+ txor=- min=yes max=yes
";
    assert_eq!(ll_out, want, "llvm output");
    assert_eq!(t3_out, want, "t3 output");
}

#[test]
fn s44_more_than_eight_parameters_is_refused_on_t3() {
    // The sibling limit the S44 investigation surfaced. `emit_function` reserves
    // parameter registers with `(i + 1).min(8)`, which is a CLAMP: a ninth
    // parameter shares R8 with the eighth. There is no stack argument area, so
    // the function cannot be represented at all and must be refused rather than
    // miscompiled. Before this check the only thing stopping it was an assertion
    // about a staging register, which is why `thatteos/src/kernel/context.mt`
    // (a 15-parameter `context_switch`) failed with an internal error.
    let src = r#"
use std::io;
fn wide(a: int, b: int, c: int, d: int, e: int, f: int, g: int, h: int, i: int) -> int {
    return a + b + c + d + e + f + g + h + i;
}
fn main() { io::println_int(wide(1, 2, 3, 4, 5, 6, 7, 8, 9)); }
"#;
    let path = write_source("s44_nine_params.mt", src);
    let out = temp_dir().join("s44_nine_params");
    let (ok, so, se) = run_manitc(&[
        "compile", "--target", "t3", path.to_str().unwrap(), "-o", out.to_str().unwrap(),
    ]);
    let all = format!("{}{}", so, se);
    assert!(!ok, "a 9-parameter function should be refused on T3, but compiled:\n{}", all);
    assert!(
        all.contains("R1-R8") && all.contains("9 parameters"),
        "the diagnostic should name the limit and the count, got:\n{}",
        all,
    );

    // LLVM has no such limit and must still compile and run it.
    let bin = temp_dir().join("s44_nine_params_ll");
    let (ok, so, se) = run_manitc(&[
        "compile", "--target", "llvm", path.to_str().unwrap(), "-o", bin.to_str().unwrap(),
    ]);
    assert!(ok, "llvm should compile a 9-parameter function:\n{}{}", so, se);
    let run = Command::new(&bin).output().expect("run llvm binary");
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "45");
}

// ---------------------------------------------------------------------------
// §49 — generic impl blocks, and keywords as method names
// ---------------------------------------------------------------------------
//
// `impl<T> Vec<T> { … }` did not parse. The parser expected a bare identifier
// straight after `impl`, so it stopped at the `<` with "expected identifier,
// found Lt" — and that is why stdlib/collections.mt, stdlib/async.mt and
// stdlib/sync.mt, the language's own Vec, Future and Mutex, did not parse.
// 159 functions and 18 structs of the standard library were invisible to the
// compiler that ships them.
//
// tests/15_generics_and_traits.mt existed the whole time and passed. It covers
// generic FUNCTIONS (`fn identity<T>`) and NON-generic impls (`impl Vec2`) and
// contains not one `impl<`. A test named for both halves never touched their
// intersection, which is the shape this pair of tests exists to prevent.

#[test]
fn s49_generic_impl_blocks_run_on_both_backends() {
    let src = "\
struct Box<T> { value: T }

impl<T> Box<T> {
    fn get(self) -> T { self.value }
}

struct Pair<A, B> { a: A, b: B }

impl<A, B> Pair<A, B> {
    fn first(self) -> A { self.a }
    fn second(self) -> B { self.b }
}

fn main() {
    let b = Box { value: 42 };
    io::println(fmt::show_int(b.get()));
    let p = Pair { a: 7, b: 9 };
    io::println(fmt::show_int(p.first()));
    io::println(fmt::show_int(p.second()));
}
";
    let ((t3_code, t3), (ll_code, ll)) = run_both_backends("s49_generic_impl.mt", src);
    assert_eq!(t3_code, 0, "t3 exited {}: {}", t3_code, t3);
    assert_eq!(ll_code, 0, "llvm exited {}: {}", ll_code, ll);
    assert_eq!(t3.trim(), "42\n7\n9", "t3 output wrong: {:?}", t3);
    assert_eq!(t3.trim(), ll.trim(),
               "backends disagree on generic impl:\n t3: {:?}\n ll: {:?}", t3, ll);
}

// `spawn` is a keyword (the `spawn { }` statement), and stdlib/async.mt declares
// `fn spawn<T>(self, fut: Future<T>) -> Task<T>` — so the module could not
// declare its own method, and `runtime.spawn(f)` could not have called it.
// maniT had already decided this question for module paths: `use async::spawn`
// works because expect_path_segment maps keyword tokens back to their spelling.
// The same now applies after `fn` and after `.`, the two other positions where
// only a name can appear. This test pins the general rule, not just `spawn`.
#[test]
fn s49_keywords_are_legal_names_after_fn_and_dot() {
    let src = "\
struct R { n: int }

impl R {
    fn spawn(self) -> int { self.n }
    fn channel(self) -> int { self.n + 1 }
}

fn main() {
    let r = R { n: 41 };
    io::println(fmt::show_int(r.spawn()));
    io::println(fmt::show_int(r.channel()));
}
";
    let ((t3_code, t3), (ll_code, ll)) = run_both_backends("s49_kw_names.mt", src);
    assert_eq!(t3_code, 0, "t3 exited {}: {}", t3_code, t3);
    assert_eq!(ll_code, 0, "llvm exited {}: {}", ll_code, ll);
    assert_eq!(t3.trim(), ll.trim(),
               "backends disagree:\n t3: {:?}\n ll: {:?}", t3, ll);
    assert!(t3.contains("41") && t3.contains("42"), "wrong output: {:?}", t3);
}

// The three stdlib modules themselves must keep type-checking. This is the
// direct regression: if `impl<T>` breaks again, these fail immediately and name
// the module, rather than the failure resurfacing as a mysterious gap in a
// training corpus months later.
#[test]
fn s49_the_generic_stdlib_modules_type_check() {
    for m in ["async", "collections", "sync"] {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/stdlib/");
        let full = format!("{}{}.mt", path, m);
        let (ok, so, se) = run_manitc(&["check", &full]);
        assert!(ok, "stdlib/{}.mt no longer type-checks:\n{}{}", m, so, se);
    }
}

// ---------------------------------------------------------------------------
// §48 — too many parameters is a DIAGNOSTIC, not a panic
// ---------------------------------------------------------------------------
//
// The T3 calling convention passes arguments in R1-R8 with no stack argument
// area, so a function of more than eight parameters cannot be emitted; the
// register assignment clamps at R8 and several parameters would silently share
// it. Refusing is correct. Refusing by `assert!` was not: a panic prints a Rust
// backtrace notice rather than an `error:` line, cannot be caught by anything
// driving the compiler, and makes a careful compiler look like a crashed one.
//
// This test pins BOTH halves — that it is refused, and that it is refused
// through the error channel. `thatteos/src/kernel/context.mt` is the real file
// that hits it, with a 13-parameter `context_save` and a 15-parameter
// `context_switch`.
#[test]
fn s48_more_than_eight_params_is_a_t3_error_not_a_panic() {
    let src = "\
fn nine(a: int, b: int, c: int, d: int, e: int, f: int, g: int, h: int, i: int) -> int {
    a + b + c + d + e + f + g + h + i
}

fn main() { io::println(fmt::show_int(nine(1,2,3,4,5,6,7,8,9))); }
";
    let path = write_source("s48_nine_params.mt", src);
    let out = temp_dir().join("s48_nine_params.t3b");
    let (ok, so, se) = run_manitc(&[
        "compile", "--target", "t3", path.to_str().unwrap(), "-o", out.to_str().unwrap(),
    ]);
    let blob = format!("{}{}", so, se);
    assert!(!ok, "a 9-parameter function must be refused on T3, but it compiled");
    assert!(
        !blob.contains("panicked"),
        "refused by PANIC rather than by diagnostic — that is §48:\n{}",
        blob
    );
    assert!(
        blob.contains("error:") && blob.contains("R1-R8"),
        "expected a diagnostic naming the calling convention, got:\n{}",
        blob
    );
    assert!(
        blob.contains("nine"),
        "the diagnostic must name the offending function, got:\n{}",
        blob
    );
}

// Eight is the boundary and must still WORK. A check written as `>= 8` instead
// of `> 8` would pass the test above and quietly reject every legal
// eight-argument function, so the limit is pinned from both sides.
#[test]
fn s48_exactly_eight_params_still_compiles_on_both_backends() {
    let src = "\
fn eight(a: int, b: int, c: int, d: int, e: int, f: int, g: int, h: int) -> int {
    a + b + c + d + e + f + g + h
}

fn main() { io::println(fmt::show_int(eight(1,2,3,4,5,6,7,8))); }
";
    let ((t3_code, t3), (ll_code, ll)) = run_both_backends("s48_eight_params.mt", src);
    assert_eq!(t3_code, 0, "t3 exited {}: {}", t3_code, t3);
    assert_eq!(ll_code, 0, "llvm exited {}: {}", ll_code, ll);
    assert_eq!(t3.trim(), "36", "t3 got {:?}", t3);
    assert_eq!(t3.trim(), ll.trim(), "backends disagree: {:?} vs {:?}", t3, ll);
}

// ---------------------------------------------------------------------------
// §51 — a module-level `let` of struct type
// ---------------------------------------------------------------------------
//
// `let ZERO: T27F = T27F { raw: 0 };` did not type-check: const-folding
// admitted literals and arithmetic but not struct literals, so `stdlib/t27f.mt`
// failed a standalone `manitc check` and only worked because `stdlib_expand`
// inlines module constants at their use sites, where the initialiser sits
// inside a function. A struct VALUE is a pointer to its fields, and a global
// is one word, so the word now holds the address of a static payload — the
// same shape a `str` global has always had.

#[test]
fn s51_struct_constant_global_works_on_both_backends() {
    let src = "\
struct Inner {
    pub a: int,
    pub b: float,
}

struct Outer {
    pub tag: str,
    pub inner: Inner,
    pub flag: bool3,
    pub t: trit,
    pub n: int,
}

let I: Inner = Inner { a: 7, b: 2.5 };
let O: Outer = Outer { tag: \"nested\", inner: Inner { a: -1, b: 0.5 }, flag: false, t: -1, n: 3 * 4 };

fn main() {
    io::println_int(I.a);
    io::println_float(I.b);
    io::println(O.tag);
    io::println_int(O.inner.a);
    io::println_float(O.inner.b);
    io::println_int(O.flag as int);
    io::println_int(O.t as int);
    io::println_int(O.n);
}
";
    let ((t3_code, t3), (ll_code, ll)) = run_both_backends("s51_struct_global.mt", src);
    assert_eq!(t3_code, 0, "t3 exited {}: {}", t3_code, t3);
    assert_eq!(ll_code, 0, "llvm exited {}: {}", ll_code, ll);
    // Every field kind a payload slot can hold: int, float (64-bit bits, which
    // the T3 word cannot carry and which travel through the .float section),
    // str (an address), a NESTED struct (another address), bool3 and trit.
    assert_eq!(
        t3.trim(),
        "7\n2.5\nnested\n-1\n0.5\n-1\n-1\n12",
        "t3 got {:?}",
        t3
    );
    assert_eq!(t3.trim(), ll.trim(), "backends disagree: {:?} vs {:?}", t3, ll);
}

// The module this defect was found in. It is checked as a FILE rather than
// re-inlined here, so the test fails if `t27f.mt` regains an unfoldable
// initialiser — which is the thing that was actually broken.
#[test]
fn s51_t27f_type_checks_standalone() {
    let t27f = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("stdlib/t27f.mt");
    let (ok, so, se) = run_manitc(&["check", t27f.to_str().unwrap()]);
    assert!(ok, "stdlib/t27f.mt must check standalone:\n{}{}", so, se);
}

// A module-level `bool3` took its representation from the FOLD, which knows
// only `Bool`, instead of from the declared type. `false` was stored as 0 —
// bool3's UNKNOWN — while the same literal inside a function was -1. Both
// backends were wrong identically, so the differential oracle could not see
// it; this pins the value, not the agreement.
#[test]
fn s51_module_level_bool3_false_is_minus_one() {
    let src = "\
let B: bool3 = false;
let T: bool3 = true;

fn main() {
    io::println_int(B as int);
    io::println_int(T as int);
    let local: bool3 = false;
    io::println_int(local as int);
}
";
    let ((t3_code, t3), (ll_code, ll)) = run_both_backends("s51_global_bool3.mt", src);
    assert_eq!(t3_code, 0, "t3 exited {}: {}", t3_code, t3);
    assert_eq!(ll_code, 0, "llvm exited {}: {}", ll_code, ll);
    assert_eq!(
        t3.trim(),
        "-1\n1\n-1",
        "a module-level bool3 must agree with a local one; t3 got {:?}",
        t3
    );
    assert_eq!(t3.trim(), ll.trim(), "backends disagree: {:?} vs {:?}", t3, ll);
}

// ---------------------------------------------------------------------------
// §52 — the command-line argument trio worked on neither backend
// ---------------------------------------------------------------------------
//
// `thatteos/userspace/editor.mt` was the one shipped program that failed on
// BOTH backends, and the reason was three separate defects wearing one name:
//
//   env::arg   the maniT spelling is `arg`, so the LLVM backend emits
//              `@env_arg` — but runtime/system.c defined `env_argv`, a symbol
//              no maniT program could name. Neither backend had it.
//   env::argc  implemented on LLVM, no T3 syscall at all.
//   env::args  no `env_args` symbol on LLVM; on T3 a syscall that returned an
//              EMPTY VEC to every caller, which the stdlib census scored as
//              "T3 only" — a stub counted as an implementation.
//
// `args` is now maniT source over the two scalar natives, so there is one
// implementation instead of two that could drift, and `run-t3` takes the
// program's arguments after the binary.
//
// These tests pin the VALUES on both backends, not merely that the backends
// agree: `args` agreed with nothing (LLVM could not link) and the T3 stub
// would have passed any test that only compared the two.

/// As [`run_both_backends`], with arguments handed to the program itself.
fn run_both_backends_with_args(
    name: &str,
    source: &str,
    args: &[&str],
) -> ((i32, String), (i32, String)) {
    let path = write_source(name, source);
    let stem = temp_dir().join(name.trim_end_matches(".mt"));

    let t3_out = stem.with_extension("t3out");
    let (ok, so, se) = run_manitc(&[
        "compile", "--target", "t3", path.to_str().unwrap(), "-o", t3_out.to_str().unwrap(),
    ]);
    assert!(ok, "{}: t3 compile failed:\n{}{}", name, so, se);
    let t3b = t3_out.with_extension("t3b");
    let t3 = Command::new(get_manitc())
        .arg("run-t3")
        .arg(t3b.to_str().unwrap())
        .args(args)
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
    let ll = Command::new(&bin).args(args).output().expect("run llvm binary");
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

// The program prints, in order: argc, arg(1) or "-", arg(2) or "-", the
// out-of-range arg(99), args().len(), and whether arg(0) is non-empty.
//
// arg(0)'s CONTENT is deliberately not pinned and must not be: on LLVM it is
// the binary's path out of /proc/self/cmdline and on T3 it is the .t3b handed
// to the emulator. Those are different files by construction. What has to
// match is that both have one — argv[0] exists, so `argc() > 1` means the same
// thing to a program on either backend, which is precisely what editor.mt asks.
const ARGV_PROBE: &str = "\
use std::env;

fn main() {
    io::println_int(env::argc());
    if env::argc() > 1 { io::println(env::arg(1)); } else { io::println(\"-\"); }
    if env::argc() > 2 { io::println(env::arg(2)); } else { io::println(\"-\"); }
    io::println(env::arg(99));
    let a = env::args();
    io::println_int(a.len());
    if str::len(env::arg(0)) > 0 { io::println(\"argv0\"); } else { io::println(\"EMPTY\"); }
}
";

#[test]
fn s52_the_argument_trio_agrees_on_both_backends_with_no_args() {
    let ((t3_code, t3), (ll_code, ll)) =
        run_both_backends_with_args("s52_argv_none.mt", ARGV_PROBE, &[]);
    assert_eq!(t3_code, 0, "t3 exited {}: {}", t3_code, t3);
    assert_eq!(ll_code, 0, "llvm exited {}: {}", ll_code, ll);
    // argc is 1 (argv[0] alone), both optional args print "-", arg(99) is the
    // empty line, args() has one element, and arg(0) is non-empty.
    assert_eq!(t3.trim_end(), "1\n-\n-\n\n1\nargv0", "t3 got {:?}", t3);
    assert_eq!(t3.trim_end(), ll.trim_end(), "backends disagree: {:?} vs {:?}", t3, ll);
}

#[test]
fn s52_the_argument_trio_carries_real_arguments() {
    let ((t3_code, t3), (ll_code, ll)) =
        run_both_backends_with_args("s52_argv_two.mt", ARGV_PROBE, &["hello", "world"]);
    assert_eq!(t3_code, 0, "t3 exited {}: {}", t3_code, t3);
    assert_eq!(ll_code, 0, "llvm exited {}: {}", ll_code, ll);
    // This is the assertion the old T3 stub could never have passed: it
    // answered "no arguments" whatever it was given, so args().len() was 0.
    assert_eq!(t3.trim_end(), "3\nhello\nworld\n\n3\nargv0", "t3 got {:?}", t3);
    assert_eq!(t3.trim_end(), ll.trim_end(), "backends disagree: {:?} vs {:?}", t3, ll);
}

// An out-of-range index returns "" on both, and does NOT trap. The two
// implementations — runtime/system.c's env_arg and emulator syscall 553 — are
// the hand-written pair for this call, exactly like manit_check_result_ok and
// syscall 561, so the behaviour they share is pinned rather than assumed.
#[test]
fn s52_an_out_of_range_argument_is_empty_not_a_trap() {
    let src = "\
use std::env;

fn main() {
    io::println_int(str::len(env::arg(5)));
    io::println_int(str::len(env::arg(0 - 1)));
    io::println(\"survived\");
}
";
    let ((t3_code, t3), (ll_code, ll)) = run_both_backends("s52_argv_oob.mt", src);
    assert_eq!(t3_code, 0, "t3 exited {} (70 is a TRAP): {}", t3_code, t3);
    assert_eq!(ll_code, 0, "llvm exited {}: {}", ll_code, ll);
    assert_eq!(t3.trim_end(), "0\n0\nsurvived", "t3 got {:?}", t3);
    assert_eq!(t3.trim_end(), ll.trim_end(), "backends disagree: {:?} vs {:?}", t3, ll);
}

// The caller that started this. It compiled on NEITHER backend before §52;
// it must now at least get through the type checker, which is where
// `env::argv` stopped it. It still does not build on T3, for an unrelated
// reason recorded in §52 — a flat terminal layer (io_move_cursor,
// terminal_set_raw, …) that lives only in the C runtime.
#[test]
fn s52_the_editor_type_checks() {
    let editor = PathBuf::from("/home/manish/oss/thatteos/userspace/editor.mt");
    if !editor.exists() {
        return; // thatteOS is a separate repo; skip when it is not checked out.
    }
    let (ok, so, se) = run_manitc(&["check", editor.to_str().unwrap()]);
    assert!(ok, "thatteos/userspace/editor.mt must check:\n{}{}", so, se);
}

// ---------------------------------------------------------------------------
// §53 — `sext i1 true` is -1, and four widening sites did it
// ---------------------------------------------------------------------------
//
// `io::println_int(5 > 0)` printed **-1** on LLVM and **1** on T3. The
// comparison yields an `i1`, the declared parameter is `int`, and the LLVM
// backend's argument-coercion path SIGN-extended it: `sext i1 true to i64` is
// -1, not 1.
//
// The rule was already written out correctly at three of this backend's seven
// widening sites — including the vararg branch eleven lines above the broken
// one — and at the typed `pick_cast_op`, which is why an EXPLICIT `(5 > 0) as
// int` was always right. It is now `helpers::widen_op`, called by all of them.
//
// Why it survived: `sext i1 false` is 0, which is the correct answer. Only
// `true` was wrong, so every test that happened to check a false condition
// passed. This pins both polarities, on both backends, in every position a
// widening can happen: argument, return, store and comparison.

#[test]
fn s53_an_i1_widens_by_zero_extension_not_sign_extension() {
    let src = "\
fn ret_bool(n: int) -> int {
    return n > 0;
}

fn main() {
    // Argument position — the site that was found broken.
    io::println_int(5 > 0);
    io::println_int(1 > 5);
    io::println_int(true);
    io::println_int(false);
    // Return position: an i1 returned through an `int` slot.
    io::println_int(ret_bool(7));
    io::println_int(ret_bool(0 - 7));
    // Store position is NOT exercised here and cannot be: a let of an int
    // from a bool is a type error, and correctly so. That asymmetry is itself
    // the finding -- see s53_a_bool_is_rejected_by_let_and_accepted_by_a_native.
    // Comparison position is likewise unreachable from source: (9 > 2) == 1 is
    // rejected, == cannot be applied to a bool and an int. The icmp widening
    // sites are fixed anyway, defensively -- an i1 can still reach them
    // through lowering.
    // The explicit cast, which was never wrong, must not have changed.
    io::println_int((5 > 0) as int);
}
";
    let ((t3_code, t3), (ll_code, ll)) = run_both_backends("s53_i1_widen.mt", src);
    assert_eq!(t3_code, 0, "t3 exited {}: {}", t3_code, t3);
    assert_eq!(ll_code, 0, "llvm exited {}: {}", ll_code, ll);
    assert_eq!(
        t3.trim_end(),
        "1\n0\n1\n0\n1\n0\n1",
        "true must widen to 1, never -1; t3 got {:?}",
        t3
    );
    assert_eq!(t3.trim_end(), ll.trim_end(), "backends disagree: {:?} vs {:?}", t3, ll);
}

// The backend must not reason the rule out a fourth time. Any NEW widening
// site written as a bare `if aw < dw { "sext" }` reintroduces the defect, so
// the source is checked for the literal pattern rather than the behaviour.
#[test]
fn s53_every_widening_site_goes_through_widen_op() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/codegen_llvm");
    let mut offenders = Vec::new();
    for entry in std::fs::read_dir(&root).expect("read codegen_llvm") {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("read source");
        for (i, line) in text.lines().enumerate() {
            // A widening that names "sext" directly, rather than asking
            // widen_op, is only safe where i1 provably cannot arrive — and the
            // one such site (main's return, which is i64/i16/i8 by match) does
            // not use this shape.
            // The exact broken idiom: a WIDTH-driven widening whose widening
            // arm names "sext" directly. `widen_op`'s own definition tests
            // `== "i1"` and main's return-value coercion tests `== "i64"`, so
            // neither matches — and neither should, one being the rule itself
            // and the other a site i1 cannot reach (its match arms are
            // i64/i16/i8).
            let width_driven = line.contains("aw < dw") || line.contains("aw < tw");
            if width_driven && line.contains("\"sext\"") {
                offenders.push(format!("{}:{}: {}", path.display(), i + 1, line.trim()));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "these widening sites bypass helpers::widen_op, so `true` will widen to -1:\n{}",
        offenders.join("\n"),
    );
}

// The asymmetry §53 ran into, recorded rather than changed.
//
// `let s: int = 5 > 0;` is a type error. `io::println_int(5 > 0)` is not, and
// `manitc check` reports it clean with no warning — the same bool arriving in
// the same `int` slot, rejected in one position and silently coerced in the
// other. The coercion is deliberate (`native_param_manitys` exists to feed
// declared parameter types into lowering for natives, which have no body), but
// it happens WITHOUT a type check, so a native's declared signature does not
// constrain its callers the way a maniT function's does.
//
// This is left as-is on purpose: tightening it is §3.2's job — the checker-hole
// triage — not a change to smuggle in beside a codegen fix, and 771 known
// mutations already turn on how that leniency is narrowed. What §53 guarantees
// is only that while the coercion is permitted, it produces the RIGHT VALUE.
// The test pins the asymmetry so that closing it is a deliberate act that
// updates this test, rather than a silent change of behaviour.
#[test]
fn s53_a_bool_is_rejected_by_let_and_accepted_by_a_native() {
    assert_check_error(
        "s53_bool_let.mt",
        "fn main() { let s: int = 5 > 0; io::println_int(s); }\n",
        "type mismatch",
    );
    assert_checks(
        "s53_bool_native_arg.mt",
        "fn main() { io::println_int(5 > 0); }\n",
    );
}

// ---------------------------------------------------------------------------
// §54 — the checker SAW them and said `warning`
// ---------------------------------------------------------------------------
// Triage of the 771 mutations `manitc check` failed to catch (ORACLE_FINDINGS
// §54, HANDOFF §3.2). Two of the six classes needed no new analysis at all —
// the checker already detected them, named them, and even suggested the right
// spelling. It just emitted a warning and exited 0:
//
//     undefined_name  357   "unknown identifier 'totl' — did you mean 'total'?"
//     wrong_type      123   "value 42 overflows type 'Trit' (range -1..1)"
//     trit_range       50   same diagnostic, different mutation
//                     ---
//                     530   of 771, closed by changing a severity
//
// Why that mattered beyond the compiler: L1 — the metric the Phase A training
// gate turns on — is DEFINED as "generations pass `manitc check`". Every typo
// waved through was scored as a model success, so the gate was measuring the
// checker's blindness as the model's skill.
//
// The flip was rejected once, on 22 Aug, on the belief that `gui_set_color`
// (runtime/gui.c, no stdlib/*.mt declaration) depended on the leniency. It does
// not — it is in the native table with a full signature, and calling it wrongly
// already produced a hard arity error. Measured instead of assumed across all
// 128 shipped .mt files: exactly ONE name in ONE file depended on it, and it
// depended on it to call a C symbol that had simply never been registered.

/// Every position an unknown name can appear in, because the fix must not be
/// positional. Before §54 all five of these exited 0.
#[test]
fn s54_an_unknown_identifier_is_an_error_in_every_position() {
    let cases: &[(&str, &str)] = &[
        ("s54_undef_native_arg.mt",
         "fn main() { let total: int = 5; io::println_int(totl); }\n"),
        ("s54_undef_let_init.mt",
         "fn main() { let total: int = 5; let y: int = totl; io::println_int(y); }\n"),
        ("s54_undef_return.mt",
         "fn f() -> int { let total: int = 5; return totl; }\n\
          fn main() { io::println_int(f()); }\n"),
        ("s54_undef_binop.mt",
         "fn main() { let total: int = 5; let y: int = total + totl; io::println_int(y); }\n"),
        ("s54_undef_user_fn_arg.mt",
         "fn g(a: int) -> int { return a; }\n\
          fn main() { let total: int = 5; io::println_int(g(totl)); }\n"),
    ];
    for (name, src) in cases {
        assert_check_error(name, src, "unknown identifier");
    }
}

/// The diagnostic must keep naming the intended spelling. The suggestion is the
/// whole reason this class is cheap to fix in a training corpus: the compiler
/// hands the model the answer.
#[test]
fn s54_the_unknown_identifier_error_still_suggests_the_right_name() {
    assert_check_error(
        "s54_undef_hint.mt",
        "fn main() { let total: int = 5; io::println_int(totl); }\n",
        "did you mean 'total'?",
    );
}

/// Nothing legitimate is lost, because the escape hatch the leniency stood in
/// for already exists and is strictly better: a TYPED declaration of a runtime
/// symbol rather than an untyped hole.
#[test]
fn s54_a_declared_native_is_the_escape_hatch() {
    assert_checks(
        "s54_user_native.mt",
        "fn fs_remove_file(path: str) ;  // native\n\
         fn main() { io::println(\"ok\"); }\n",
    );
}

/// Functions are collected in a pre-pass, so a call to one defined later in the
/// file never reached the unknown-identifier branch. Pinned because if that
/// ever became single-pass, this change would turn a working idiom into a hard
/// error across the whole corpus.
#[test]
fn s54_forward_references_are_unaffected() {
    assert_checks(
        "s54_forward_ref.mt",
        "fn main() { io::println_int(later(3)); }\n\
         fn later(a: int) -> int { return a * 2; }\n",
    );
}

/// `fs_remove_file` was the ONLY name in 128 shipped files still relying on the
/// leniency — it is in runtime/system.c and in the LLVM emitter, but had never
/// been added to the native table. Registering it is what made the flip safe,
/// so removing it again would silently re-break thatteos/userspace/gui_fm.mt.
#[test]
fn s54_fs_remove_file_is_a_registered_native() {
    assert_checks(
        "s54_fs_remove_file.mt",
        "fn main() { fs_remove_file(\"/tmp/s54_nonexistent\"); }\n",
    );
}

/// The other severity class. This one is worse than a missed diagnostic: the
/// program RUNS, and both backends print 42 for a value that has three legal
/// states. They AGREE, so the differential oracle cannot see it — the same
/// blind spot as §51's module-level `bool3`.
#[test]
fn s54_a_literal_outside_a_ternary_range_is_an_error() {
    assert_check_error(
        "s54_trit_42.mt",
        "fn main() { let n: trit = 42; io::println_int(n); }\n",
        "overflows type",
    );
    // 2 is the mutation gen_repair actually applies — one step outside the set.
    assert_check_error(
        "s54_trit_2.mt",
        "fn main() { let t: trit = 2; io::println_int(t); }\n",
        "overflows type",
    );
    // Ranges are derived from the trit count, not conventional: 6 trits.
    assert_check_error(
        "s54_tryte_out.mt",
        "fn main() { let t: tryte = 365; io::println_int(t); }\n",
        "overflows type",
    );
    // BOTH polarities. `-17` is not `Lit::Int(-17)` — it parses as a negation
    // of `17` — so the first version of this check caught `17` and waved
    // through `-17`. Found by re-running the mutation corpus against the fix
    // itself. §53 is the same lesson: a rule tested on one polarity is correct
    // on one polarity.
    assert_check_error(
        "s54_trit_neg.mt",
        "fn main() { let y: trit = -17; io::println_int(y); }\n",
        "value -17 overflows",
    );
    assert_check_error(
        "s54_tryte_neg.mt",
        "fn main() { let t: tryte = -365; io::println_int(t); }\n",
        "overflows type",
    );
}

/// The complement, so the check cannot be "reject every ternary literal".
/// All three legal trits, and a tryte at each end of its exact range.
#[test]
fn s54_ternary_literals_inside_their_range_still_check() {
    assert_checks(
        "s54_trit_legal.mt",
        "fn main() {\n\
         \x20   let a: trit = -1;\n\
         \x20   let b: trit = 0;\n\
         \x20   let c: trit = 1;\n\
         \x20   let d: tryte = 364;\n\
         \x20   let e: tryte = -364;\n\
         \x20   io::println_int(a + b + c + d + e);\n\
         }\n",
    );
}

/// The third class, and the only one of the six where the two backends give
/// DIFFERENT answers rather than the same wrong one — so it is a wrong result,
/// not merely an unreported mutation:
///
///     fn f(n: int) { return n + 1; }   io::println_int(f(41))
///         LLVM   clang link failure
///         T3     prints 0, silently, for an expected 42
///
/// §A1 already enforced the other direction (a non-void function must supply a
/// value on every path). This one was missing because `current_fn_ret` was
/// consulted only as an inference HINT and never compared against.
#[test]
fn s54_a_function_with_no_return_type_may_not_return_a_value() {
    assert_check_error(
        "s54_dropped_ret_ty.mt",
        "fn f(n: int){ return n + 1; }\nfn main() { io::println_int(f(41)); }\n",
        "no declared return type",
    );
}

/// The complement: a genuinely void function must still be able to say
/// `return;` for an early exit, and a declared return type must still work.
/// Without this the check above could be "reject every return".
#[test]
fn s54_void_and_typed_returns_both_still_check() {
    assert_checks(
        "s54_void_return_ok.mt",
        "fn shout(m: str) { io::println(m); return; }\n\
         fn twice(n: int) -> int { return n * 2; }\n\
         fn main() { shout(\"hi\"); io::println_int(twice(21)); }\n",
    );
}

/// The fourth class. The asymmetry is what makes it a defect rather than a
/// missing feature: calling a FREE function with the wrong number of arguments
/// has always been a hard error, while the identical mistake through a receiver
/// was not checked at all. `v.slice(2)` for a two-argument slice, and a lambda
/// losing the parameter its body still refers to, both checked clean.
#[test]
fn s54_a_user_method_called_with_the_wrong_arity_is_an_error() {
    assert_check_error(
        "s54_method_arity.mt",
        "struct Point { x: int, y: int }\n\
         impl Point {\n\
         \x20   fn shifted(self, dx: int, dy: int) -> int { return self.x + dx + dy; }\n\
         }\n\
         fn main() {\n\
         \x20   let p: Point = Point { x: 1, y: 2 };\n\
         \x20   io::println_int(p.shifted(3));\n\
         }\n",
        "expects 2 argument(s), found 1",
    );
}

/// The receiver must not be counted as an argument, and a zero-argument method
/// must still be callable. `fn shifted(self, dx, dy)` is three PARAMETERS and
/// two ARGUMENTS — off by one here would reject every correct call in the
/// corpus, which is exactly why the arity map is built from the AST (where
/// `self` is visible by name) rather than from `functions` (which stores types
/// only).
#[test]
fn s54_correct_method_arity_still_checks() {
    assert_checks(
        "s54_method_arity_ok.mt",
        "struct Point { x: int, y: int }\n\
         impl Point {\n\
         \x20   fn shifted(self, dx: int, dy: int) -> int { return self.x + dx + dy; }\n\
         \x20   fn norm(self) -> int { return self.x + self.y; }\n\
         }\n\
         fn main() {\n\
         \x20   let p: Point = Point { x: 1, y: 2 };\n\
         \x20   io::println_int(p.shifted(3, 4));\n\
         \x20   io::println_int(p.norm());\n\
         }\n",
    );
}

/// Builtin methods are deliberately NOT covered: the analyzer has no signature
/// table for Vec/Map/Set/str, and inventing a partial one would reject correct
/// programs — a far worse failure than accepting wrong ones. This test records
/// that boundary as a decision rather than an oversight, so that building the
/// table later is a deliberate act that updates it.
#[test]
fn s54_builtin_method_arity_is_knowingly_unchecked() {
    assert_checks(
        "s54_builtin_arity_gap.mt",
        "fn main() {\n\
         \x20   let v: Vec<int> = Vec::new();\n\
         \x20   v.push(1);\n\
         \x20   let s: Vec<int> = v.slice(2);\n\
         \x20   io::println_int(s.len());\n\
         }\n",
    );
}

/// ManiT has TWO return paths and the first version of the dropped-return-type
/// check only covered one. A `return` inside a `tif` arm is an EXPRESSION, not
/// a statement, so 24 `drop_return_type` mutations walked straight past it —
/// every one of them a function returning through `tif`, which is the idiomatic
/// ternary dispatch and therefore common in exactly the code that matters.
///
/// §53's lesson a third time: the rule now lives once, in
/// `check_return_value_allowed`, and both sites call it.
#[test]
fn s54_a_tif_arm_return_obeys_the_same_return_type_rule() {
    assert_check_error(
        "s54_tif_return_no_ty.mt",
        "fn perm_str(p: trit) {\n\
         \x20   tif p {\n\
         \x20       + => return \"GRANT\",\n\
         \x20       0 => return \"CHECK\",\n\
         \x20       - => return \"DENY \",\n\
         \x20   }\n\
         }\n\
         fn main() { io::println(perm_str(1)); }\n",
        "no declared return type",
    );
    // The same function with its type restored must still check, or the rule
    // would be "reject tif dispatch" rather than "reject an undeclared return".
    assert_checks(
        "s54_tif_return_ok.mt",
        "fn perm_str(p: trit) -> str {\n\
         \x20   tif p {\n\
         \x20       + => return \"GRANT\",\n\
         \x20       0 => return \"CHECK\",\n\
         \x20       - => return \"DENY \",\n\
         \x20   }\n\
         }\n\
         fn main() { io::println(perm_str(1)); }\n",
    );
}

/// report.txt P24 — a non-pointer passed where a native declares `str`.
///
/// `str` is a POINTER and the LLVM backend dereferences whatever it is handed,
/// so `io::print(' ')` passed the char's integer value and SEGFAULTED — from a
/// program `manitc check` accepted. T3 read whatever that address held and
/// printed byte soup. One of the five LLVM crashes the corpus sweep found.
///
/// The check is deliberately NARROW: a declared `str` parameter and nothing
/// else. Widening it to every declared native parameter type breaks two
/// INTENTIONAL behaviours this file already pins — `fmt::format` is variadic
/// behind a `[str]` placeholder, and S53 documents that a native accepts a
/// `bool` where an `int` is declared. Both were measured, not guessed: the
/// broad version failed 13 tests.
#[test]
fn p24_a_non_pointer_where_a_native_declares_str_is_rejected() {
    assert_check_error(
        "p24_print_char",
        "fn main() { io::print(' '); }",
        "expected `str`, found `char`",
    );
    assert_check_error(
        "p24_print_int",
        "fn main() { io::print(42); }",
        "expected `str`, found `int`",
    );

    // The intentional permissiveness is untouched.
    assert_checks("p24_ok_str", "fn main() { io::println(\"ok\"); }");
    assert_checks("p24_ok_bool_native", "fn main() { io::println_int(true); }");
    assert_checks("p24_ok_trit", "fn main() { io::print_trit(+); }");
}

// report.txt P49 — `t27f::from_float` normalised the wrong way round
// ---------------------------------------------------------------------------
//
// `from_float` picked the LARGEST e with 3^e <= |f|, forcing |f| / 3^e into
// [1,3) so that `float_to_int` truncated it to 1 or 2. Seventeen of the
// eighteen mantissa trits were always zero and `MANTISSA_MAX` was unreachable
// by construction. `normalize`, in the same file, documents the opposite and
// correct discipline: shift the mantissa UP while it fits, so a normalized
// exponent is negative.
//
// The test asserts ACCURACY rather than an exact value. `to_float`
// reconstructs through binary and the file already documents that as lossy —
// 100 + 200 comes back as 300.00000000000006 — so pinning a literal would be
// pinning that lossiness. What the defect changed is the ORDER OF MAGNITUDE
// of the error: 25-33 %, against a representation good to 3^-18 ≈ 2.6e-9.
//
// Both backends were wrong IDENTICALLY, so the parity matrix could not see
// this and neither could the differential oracle. It is pinned by value.
#[test]
fn p49_from_float_fills_the_mantissa_instead_of_the_exponent() {
    let src = "\
use std::io;

// Relative error of a round trip, as a percentage, so the assertion reads in
// units the defect was measured in.
fn err_pct(x: float) -> float {
    let back: float = t27f::to_float(t27f::from_float(x));
    let d: float = if back > x { back - x } else { x - back };
    // The DENOMINATOR must be |x|. Dividing by x itself makes the percentage
    // NEGATIVE for a negative input, and every `< tolerance` check then passes
    // for free — which is exactly what the negative-value row did when the
    // defect was reintroduced to check this test.
    let ax: float = if x > 0.0 { x } else { 0.0 - x };
    return (d / ax) * 100.0;
}

fn main() {
    // Exactly representable in balanced ternary: 4 = 3+1, 12, 100, 729.
    // Before the fix these came back 25 %, 25 %, 19 % and 27 % low.
    io::println_int(if err_pct(4.0)    < 0.000001 { 1 } else { 0 });
    io::println_int(if err_pct(12.0)   < 0.000001 { 1 } else { 0 });
    io::println_int(if err_pct(100.0)  < 0.000001 { 1 } else { 0 });
    io::println_int(if err_pct(1000.0) < 0.000001 { 1 } else { 0 });
    // NOT representable in ternary — 1/2 is 0.111... in base 3 — so the
    // achievable error is the 18-trit limit, not zero. It was 33 % low.
    io::println_int(if err_pct(0.5) < 0.0001 { 1 } else { 0 });
    // The mantissa must actually be large now. Before, it was 1 or 2 for
    // every input; MANTISSA_MAX is 193710244 and this is well past 2.
    io::println_int(if t27f::mantissa(t27f::from_float(12.0)) > 1000000 { 1 } else { 0 });
    // And the sign must survive the new normalisation loop.
    io::println_int(if err_pct(0.0 - 12.0) < 0.000001 { 1 } else { 0 });
}
";
    let ((t3_code, t3), (ll_code, ll)) = run_both_backends("p49_from_float.mt", src);
    assert_eq!(t3_code, 0, "t3 exited {}: {}", t3_code, t3);
    assert_eq!(ll_code, 0, "llvm exited {}: {}", ll_code, ll);
    assert_eq!(
        t3.trim(),
        "1\n1\n1\n1\n1\n1\n1",
        "t3: a 0 is a round trip still losing precision — got {:?}",
        t3
    );
    assert_eq!(t3.trim(), ll.trim(), "backends disagree: {:?} vs {:?}", t3, ll);
}


// report.txt P56 / P57 — stdlib/bridge.mt
// ---------------------------------------------------------------------------
//
// P56: `binary_to_word` advanced its place value once more than it used, and
// the extra advance produced 3^27 — about twice the largest `int` — which
// traps T3 AFTER the result is already complete. LLVM computed it in i64 and
// never noticed, so this was a divergence in which T3 was the honest one.
//
// The test asserts the EXTREMES, not a convenient middling value: +/-
// 3812798742493 is the 27-trit range, so it exercises every trit including the
// most significant, which is the one the guarded advance is about. A test on
// small values passes with the guard placed one iteration wrong.
#[test]
fn p56_binary_to_word_round_trips_the_whole_27_trit_range() {
    let src = "\
use std::io;

fn main() {
    let mut bad: int = 0;
    let mut v: int = -13;
    while v <= 13 {
        let bits: [trit; 54] = bridge::word_to_binary(v as t27);
        if bridge::binary_to_word(bits) as int != v { bad = bad + 1; }
        v = v + 1;
    }
    io::print_int(bad); io::newline();
    let hi: [trit; 54] = bridge::word_to_binary(3812798742493 as t27);
    io::print_int(bridge::binary_to_word(hi) as int); io::newline();
    let lo: [trit; 54] = bridge::word_to_binary((0 - 3812798742493) as t27);
    io::print_int(bridge::binary_to_word(lo) as int); io::newline();

    // THE SECOND SITE. `binary_to_t27_pair` carried the identical unguarded
    // advance and was missed by fixing only the function that was reported.
    let (h, l) = bridge::binary_to_t27_pair(hi);
    io::print_int(bridge::binary_to_word(bridge::t27_pair_to_binary(h, l)) as int);
    io::newline();
}
";
    let ((t3_code, t3), (ll_code, ll)) = run_both_backends("p56_bridge_word.mt", src);
    assert_eq!(t3_code, 0, "t3 exited {}: {}", t3_code, t3);
    assert_eq!(ll_code, 0, "llvm exited {}: {}", ll_code, ll);
    assert_eq!(
        t3.trim(),
        "0\n3812798742493\n-3812798742493\n3812798742493",
        "t3: a trap here is the dead multiplication returning — got {:?}",
        t3
    );
    assert_eq!(t3.trim(), ll.trim(), "backends disagree: {:?} vs {:?}", t3, ll);
}

/// **P57: the byte/tryte pair is LOSSLESS, and its range is 0..=273.**
///
/// This one pins a NON-defect, deliberately. A scan of all 256 bytes through
/// `byte_to_tryte` then `tryte_to_byte` finds 232 that do not round-trip, and
/// that reads like breakage — it cost another session five parked tasks. It is
/// arithmetic: 27 values cannot have 256 distinct encodings, so 229 bytes are
/// not valid encodings at all, and the remaining 3 are canonical encodings
/// above 255 (trytes 11, 12, 13 encode to 271, 270, 273).
///
/// Six TERNARY positions is 3^6 = 729, not 2^6 = 64. "byte" is a misnomer; the
/// return type `t9` is correct and always was.
#[test]
fn p57_the_byte_tryte_pair_is_lossless_and_exceeds_255() {
    let src = "\
use std::io;

fn main() {
    let mut bad: int = 0;
    let mut hi: int = -100000;
    let mut over: int = 0;
    let mut t: int = -13;
    while t <= 13 {
        let b: int = bridge::tryte_to_byte(t as tryte) as int;
        if bridge::byte_to_tryte(b as t9) as int != t { bad = bad + 1; }
        if b > hi { hi = b; }
        if b > 255 { over = over + 1; }
        t = t + 1;
    }
    io::print_int(bad); io::newline();
    io::print_int(hi); io::newline();
    io::print_int(over); io::newline();
}
";
    let ((t3_code, t3), (ll_code, ll)) = run_both_backends("p57_bridge_tryte.mt", src);
    assert_eq!(t3_code, 0, "t3 exited {}: {}", t3_code, t3);
    assert_eq!(ll_code, 0, "llvm exited {}: {}", ll_code, ll);
    assert_eq!(
        t3.trim(),
        "0\n273\n3",
        "t3: expected 0 round-trip failures, a maximum of 273, and 3 \
         encodings above 255 — got {:?}",
        t3
    );
    assert_eq!(t3.trim(), ll.trim(), "backends disagree: {:?} vs {:?}", t3, ll);
}

/// **report.txt P59: `Vec::remove` returns the removed element.**
///
/// It used to return nothing, while `Vec<T>::remove` is typed `T`. All four
/// layers agreed with each other and none with the type: `void Vec_remove` in
/// the C runtime, `declare void` in the LLVM backend, a discarded result in
/// the T3 emulator, and the no-result syscall helper in the T3 emitter —
/// which allocates the destination register and then drops it.
///
/// **The last part is why it looked intermittent.** T3 returned whatever the
/// PRECEDING operation had left in that register: `3` after a `len() >= 3`
/// comparison, `30` after a `pop()`. The defect was neither intermittent nor
/// data-dependent, and the FIFO drain below is the shape that found it —
/// `remove(0)` in a loop is the one idiom that needs the value.
///
/// It survived because `v.remove(i)` as a STATEMENT is correct and is the
/// common use: nothing in manitc's 76 `.mt` files or thatteOS's 55 reads the
/// result.
#[test]
fn p59_vec_remove_returns_the_element_it_removed() {
    let src = "\
use std::io;

fn main() {
    let v: Vec<int> = Vec::new();
    v.push(10); v.push(20); v.push(30);
    io::print_int(v.remove(1)); io::newline();          // 20, from the middle
    io::print_int(v.len()); io::newline();              // 2
    io::print_int(v.get(0)); io::print_int(v.get(1)); io::newline();  // 1030

    // The two orderings that made it look data-dependent: whatever ran just
    // before must not reach the result.
    let w: Vec<int> = Vec::new();
    w.push(10); w.push(20); w.push(30);
    if w.len() >= 3 { io::print_int(w.remove(0)); io::newline(); }    // 10, not 3

    let x: Vec<int> = Vec::new();
    x.push(10); x.push(20); x.push(30);
    let p: int = x.pop();
    io::print_int(p); io::print_int(x.remove(0)); io::newline();      // 3010, not 3030

    // Out of range is 0 and leaves the vector alone, on both backends.
    let y: Vec<int> = Vec::new();
    y.push(7);
    io::print_int(y.remove(5)); io::print_int(y.len()); io::newline(); // 01

    // The FIFO drain this was found by.
    let q: Vec<int> = Vec::new();
    q.push(1); q.push(2); q.push(3);
    while q.len() > 0 { io::print_int(q.remove(0)); }
    io::newline();                                                    // 123
}
";
    let ((t3_code, t3), (ll_code, ll)) = run_both_backends("p59_vec_remove.mt", src);
    assert_eq!(t3_code, 0, "t3 exited {}: {}", t3_code, t3);
    assert_eq!(ll_code, 0, "llvm exited {}: {}", ll_code, ll);
    assert_eq!(
        t3.trim(),
        "20\n2\n1030\n10\n3010\n01\n123",
        "t3 got {:?}",
        t3
    );
    assert_eq!(t3.trim(), ll.trim(), "backends disagree: {:?} vs {:?}", t3, ll);
}

/// **report.txt P62: a program's own `impl` wins over a stdlib module's.**
///
/// A source module is pulled in by REFERENCE, not only by `use` — "referencing
/// a module is intent enough to expand it", and a bare method name counts. So
/// a program that defines its own type with a method whose name a source
/// module also implements drags that module in. Harmless bloat until impl
/// blocks began expanding (P61); after it, the module's `impl` arrived
/// alongside the program's own and the analyser refused the duplicate.
///
/// **Caught by R5** — `manitc check` verdicts against the pre-change binary —
/// which moved from 0 differences to 5, all of them `stdlib/tritfs_test.mt`
/// and its four fuzz-corpus copies. That file inlines its own copy of TritFS
/// deliberately and says so in a comment.
///
/// The third case is the one that makes the narrow repair necessary:
/// suppressing the pull-in instead was tried, and a program defining its own
/// `reverse` while also calling `s.reverse()` on a `str` then lost the `str`
/// body and failed at LINK rather than at check — visible bloat traded for a
/// silent failure.
#[test]
fn p62_a_programs_own_impl_is_not_duplicated_by_an_expanded_module() {
    // 1. Own type whose method name a source module also implements.
    let own = "\
use std::io;

struct Doc { pub x: int }

impl Doc {
    fn make() -> Doc { return Doc { x: 5 }; }
    fn reverse(self) -> int { return 0 - self.x; }
}

fn main() { let d: Doc = Doc::make(); io::print_int(d.reverse()); io::newline(); }
";
    let ((c1, o1), (c2, o2)) = run_both_backends("p62_own_impl.mt", own);
    assert_eq!(c1, 0, "t3 exited {}: {}", c1, o1);
    assert_eq!(c2, 0, "llvm exited {}: {}", c2, o2);
    assert_eq!(o1.trim(), "-5", "t3 got {:?}", o1);
    assert_eq!(o1.trim(), o2.trim(), "backends disagree");

    // 2. The mixed case: its OWN `reverse` and the stdlib's, in one program.
    //    Both must resolve — this is what the over-broad first fix broke.
    let mixed = "\
use std::io;

struct Doc { pub x: int }

impl Doc { fn reverse(self) -> int { return 0 - self.x; } }

fn main() {
    let d: Doc = Doc { x: 5 };
    io::print_int(d.reverse());
    io::print(\" \");
    let s: str = \"abc\";
    io::println(s.reverse());
}
";
    let ((c3, o3), (c4, o4)) = run_both_backends("p62_mixed_impl.mt", mixed);
    assert_eq!(c3, 0, "t3 exited {}: {}", c3, o3);
    assert_eq!(c4, 0, "llvm exited {}: {}", c4, o4);
    assert_eq!(
        o3.trim(),
        "-5 cba",
        "the program's own `reverse` AND `str::reverse` must both resolve — \
         got {:?}",
        o3
    );
    assert_eq!(o3.trim(), o4.trim(), "backends disagree");
}

/// **report.txt P63 / P41: the heap is 2,536 words, and P41 doubled it.**
///
/// `HEAP_BASE` is 63,000 and `memory.len()` is 65,536, so an allocating
/// program has 2,536 words — 634 four-word structs. This pins the boundary
/// because it is the arithmetic that makes P41 measurable rather than merely
/// argued: with the double bump the same program managed 317.
///
/// A second, INDEPENDENT ceiling from P38's 60,000-word code image: one is
/// reached by writing a long program, the other by allocating data.
#[test]
fn p63_the_heap_holds_exactly_what_the_memory_map_says() {
    let prog = |n: i32| {
        format!(
            "use std::io;\n\
             struct P {{ pub a: int, pub b: int, pub c: int, pub d: int }}\n\
             fn main() {{\n\
             \x20   let v: Vec<int> = Vec::new();\n\
             \x20   let mut i: int = 0;\n\
             \x20   while i < {} {{ let p: P = P {{ a: i, b: i, c: i, d: i }}; v.push(p.a); i = i + 1; }}\n\
             \x20   io::println(\"ok\");\n\
             }}\n",
            n
        )
    };

    // 2536 / 4 = 634 four-word structs fit; the next one does not.
    let ((c1, o1), _) = run_both_backends("p63_heap_fits.mt", &prog(634));
    assert_eq!(c1, 0, "634 four-word structs must fit in a 2,536-word heap: {}", o1);
    assert_eq!(o1.trim(), "ok", "got {:?}", o1);

    let ((_, o2), _) = run_both_backends("p63_heap_over.mt", &prog(635));
    assert!(
        o2.contains("heap exhausted"),
        "635 four-word structs must EXHAUST a 2,536-word heap and say so — a \
         silent success here means P39's bound check is gone and the program \
         is writing past the end of memory again. Got {:?}",
        o2
    );
}


// ---------------------------------------------------------------------------
// P91 — a sub-word STRUCT FIELD in an if/tif EXPRESSION emitted a phi whose
// operands were wider than itself, and clang refused the module.
//
// A struct field is a machine-word SLOT (`lower::helpers::slot_access_ty`), so
// reading a `trit` field yields an i64 temp; the phi is typed from the source
// EXPRESSION, which is `trit` — i8. Nothing else noticed: `check` passed,
// `--verify-ssa` reported 0 violations twice (it verifies SSA FORM, not operand
// TYPE agreement), and T3 compiled the same program and printed the right
// answer. Two live thatteOS kernel modules were unbuildable on LLVM because of
// it, and neither was in any build script, so nobody was told.
//
// Every row runs BOTH backends and requires them to agree. A row that only
// asserted "LLVM compiles" could not tell a fix from a change of answer, which
// matters here because the repair widens a phi and a widened phi that dropped a
// sign would still compile.
// ---------------------------------------------------------------------------

/// The reported case: a `trit` struct field in an `if`-EXPRESSION.
#[test]
fn p91_a_trit_struct_field_in_an_if_expression_compiles_on_both_backends() {
    let ((c3, o3), (cl, ol)) = run_both_backends(
        "p91_trit_field_if.mt",
        "use std::io;\n\
         struct S { pub sel: int, pub a: trit, pub b: trit }\n\
         fn main() {\n\
         \x20   let s = S { sel: 0, a: +, b: - };\n\
         \x20   let p = if s.sel == 0 { s.a } else { s.b };\n\
         \x20   io::print_trit(p); io::println(\"\");\n\
         }\n",
    );
    assert_eq!(cl, 0, "llvm must build and run: {}", ol);
    assert_eq!(c3, 0, "t3 must build and run: {}", o3);
    assert_eq!(ol.trim(), "+", "llvm printed {:?}", ol);
    assert_eq!(o3.trim(), "+", "t3 printed {:?}", o3);
}

/// `bool` is the OTHER sub-word type and takes the same slot, so it fails the
/// same way. Probing only the reported spelling would have fixed half of it —
/// report.txt P70's rule, and this is the second time it has paid.
#[test]
fn p91_a_bool_struct_field_in_an_if_expression_compiles_on_both_backends() {
    let ((c3, o3), (cl, ol)) = run_both_backends(
        "p91_bool_field_if.mt",
        "use std::io;\n\
         struct S { pub sel: int, pub a: bool, pub b: bool }\n\
         fn main() {\n\
         \x20   let s = S { sel: 0, a: true, b: false };\n\
         \x20   let p = if s.sel == 0 { s.a } else { s.b };\n\
         \x20   io::println(if p { \"T\" } else { \"F\" });\n\
         }\n",
    );
    assert_eq!(cl, 0, "llvm must build and run: {}", ol);
    assert_eq!(c3, 0, "t3 must build and run: {}", o3);
    assert_eq!(ol.trim(), "T", "llvm printed {:?}", ol);
    assert_eq!(o3.trim(), "T", "t3 printed {:?}", o3);
}

/// `tif` reaches the same phi by a three-armed route.
#[test]
fn p91_a_trit_struct_field_in_a_tif_expression_compiles_on_both_backends() {
    let ((c3, o3), (cl, ol)) = run_both_backends(
        "p91_trit_field_tif.mt",
        "use std::io;\n\
         struct S { pub a: trit, pub b: trit }\n\
         fn main() {\n\
         \x20   let s = S { a: +, b: - };\n\
         \x20   let p = tif s.a { + => s.a, 0 => s.b, - => s.b };\n\
         \x20   io::print_trit(p); io::println(\"\");\n\
         }\n",
    );
    assert_eq!(cl, 0, "llvm must build and run: {}", ol);
    assert_eq!(c3, 0, "t3 must build and run: {}", o3);
    assert_eq!(ol.trim(), "+", "llvm printed {:?}", ol);
    assert_eq!(o3.trim(), "+", "t3 printed {:?}", o3);
}

/// The MIXED arm shape, and it is the one that actually occurs in the field:
/// a field-load arm beside CONSTANT arms. It is what `attenuate` in thatteOS's
/// `security/capability.mt` lowers to, and it is the row that ruled out the
/// first two attempts at this repair.
///
/// Both of those tried to reconcile the types AT THE PHI, by widening it to
/// its operands. The mixed case is why that cannot work. Widening changes how
/// a CONSTANT arm must be spelled — LLVM writes an i1 as `true`/`false`, legal
/// at that width and nowhere else, so `phi i64 [ true, ... ]` is rejected as
/// loudly as the mismatch being repaired. And when one arm is a wide field
/// load while another is a genuinely narrow temp, no single phi type satisfies
/// both, because neither operand can carry a conversion. Reconciling at the
/// DEFINITION has none of these cases: there is exactly one value to convert
/// and one place to put the conversion.
#[test]
fn p91_a_field_load_beside_constant_arms_compiles_on_both_backends() {
    let ((c3, o3), (cl, ol)) = run_both_backends(
        "p91_mixed_arms.mt",
        "use std::io;\n\
         struct S { pub p: trit }\n\
         fn attenuate(s: S, req: trit) -> trit {\n\
         \x20   return tif req { + => s.p, 0 => -, - => - };\n\
         }\n\
         struct B { pub f: bool }\n\
         fn pick(b: B, sel: int) -> bool {\n\
         \x20   return if sel == 0 { b.f } else { true };\n\
         }\n\
         fn main() {\n\
         \x20   io::print_trit(attenuate(S { p: + }, +));\n\
         \x20   io::print_trit(attenuate(S { p: + }, 0));\n\
         \x20   io::println(\"\");\n\
         \x20   io::println(if pick(B { f: true }, 0) { \"T\" } else { \"F\" });\n\
         }\n",
    );
    assert_eq!(cl, 0, "llvm must build and run: {}", ol);
    assert_eq!(c3, 0, "t3 must build and run: {}", o3);
    assert_eq!(ol.trim(), o3.trim(), "backends disagree: llvm {:?} t3 {:?}", ol, o3);
    assert_eq!(ol.trim(), "+-\nT", "got {:?}", ol);
}

/// The NEGATIVE control. An array element, a plain local and a multi-`return`
/// function all compiled before the repair and must still. Without this row the
/// suite cannot tell "the widening is correctly scoped" from "the widening
/// happens to fire everywhere and nothing has noticed yet".
#[test]
fn p91_the_shapes_that_already_worked_still_do() {
    let ((c3, o3), (cl, ol)) = run_both_backends(
        "p91_negative_control.mt",
        "use std::io;\n\
         struct S { pub sel: int, pub a: trit, pub b: trit }\n\
         fn pick(s: S) -> trit { if s.sel == 0 { return s.a; } else { return s.b; } }\n\
         fn main() {\n\
         \x20   let arr: [trit; 3] = [+, 0, -];\n\
         \x20   let i = 0;\n\
         \x20   io::print_trit(if i == 0 { arr[0] } else { arr[1] });\n\
         \x20   let a: trit = +; let b: trit = -;\n\
         \x20   io::print_trit(if i == 0 { a } else { b });\n\
         \x20   io::print_trit(pick(S { sel: 0, a: +, b: - }));\n\
         \x20   io::println(\"\");\n\
         }\n",
    );
    assert_eq!(cl, 0, "llvm must build and run: {}", ol);
    assert_eq!(c3, 0, "t3 must build and run: {}", o3);
    assert_eq!(ol.trim(), "+++", "llvm printed {:?}", ol);
    assert_eq!(o3.trim(), "+++", "t3 printed {:?}", o3);
}

/// A LOOP carrying a narrow value across its back edge. This is the row that
/// exists because the first repair broke it: a phi on a back edge references a
/// temp defined AFTER it, `actual_type_of` answers for an unrecorded temp with
/// a GUESS of i64, and taking the guess as fact widened the phi against a
/// definition that turned out to be i8 — "instruction forward referenced with
/// type i64". The repair therefore requires a RECORDED type, not an answer.
#[test]
fn p91_a_narrow_value_carried_across_a_loop_back_edge_still_compiles() {
    let ((c3, o3), (cl, ol)) = run_both_backends(
        "p91_loop_back_edge.mt",
        "use std::io;\n\
         struct S { pub a: trit }\n\
         fn main() {\n\
         \x20   let s = S { a: + };\n\
         \x20   let mut carry: trit = -;\n\
         \x20   let mut i = 0;\n\
         \x20   while i < 3 {\n\
         \x20       carry = if i == 1 { s.a } else { carry };\n\
         \x20       i = i + 1;\n\
         \x20   }\n\
         \x20   io::print_trit(carry); io::println(\"\");\n\
         }\n",
    );
    assert_eq!(cl, 0, "llvm must build and run: {}", ol);
    assert_eq!(c3, 0, "t3 must build and run: {}", o3);
    assert_eq!(ol.trim(), o3.trim(), "backends disagree: llvm {:?} t3 {:?}", ol, o3);
    assert_eq!(ol.trim(), "+", "got {:?}", ol);
}

// ---------------------------------------------------------------------------
// The examples walkthrough must agree with the examples directory
// ---------------------------------------------------------------------------
// `docs/examples.md` opened with "The `examples/` directory contains seven
// programs" and `docs/index.md` billed it as a walkthrough of "all seven
// example programs". The directory holds seventeen, and has since long before
// either sentence was read again. Both were true when written.
//
// That is the class permanent rule 6 exists for: three documentation defects
// have been fixed in this repository and NONE was a false sentence — an
// absence, a single word, and a mechanism that was true when written. A count
// is the fourth shape, and it is the one prose review is worst at, because
// nothing about the sentence looks wrong. The obvious repair is wrong too:
// changing `index.md`'s "seven" to "seventeen" turns a stale claim into a
// false one, since the walkthrough really does cover seven.
//
// So this test pins CONSISTENCY, not completeness. Walking through a subset is
// a choice and stays available; what may not happen again is a document
// asserting a count that the directory has moved past, or walking through a
// program that is not shipped.

/// English for the small counts these two documents spell out. Deliberately
/// narrow: an out-of-range count should fail loudly rather than be papered
/// over, because it means the directory has grown past what the prose form
/// here can express and a human should choose the new wording.
fn number_word(n: usize) -> String {
    const WORDS: [&str; 21] = [
        "zero", "one", "two", "three", "four", "five", "six", "seven", "eight",
        "nine", "ten", "eleven", "twelve", "thirteen", "fourteen", "fifteen",
        "sixteen", "seventeen", "eighteen", "nineteen", "twenty",
    ];
    assert!(
        n < WORDS.len(),
        "examples/ now holds {} programs, past the range this test can spell. \
         Update number_word() and the prose in docs/examples.md and docs/index.md.",
        n,
    );
    WORDS[n].to_string()
}

#[test]
fn examples_walkthrough_matches_the_examples_directory() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    // The set that actually ships, read from disk — not from any list.
    let mut on_disk: Vec<String> = std::fs::read_dir(root.join("examples"))
        .expect("examples/ must exist")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter_map(|f| f.strip_suffix(".mt").map(str::to_string))
        .collect();
    on_disk.sort();
    assert!(
        !on_disk.is_empty(),
        "no .mt programs found in examples/ — the test is measuring nothing"
    );

    let walkthrough = std::fs::read_to_string(root.join("docs/examples.md"))
        .expect("docs/examples.md must exist");

    // Every "## <name>.mt" section heading is one program walked through.
    let mut documented: Vec<String> = walkthrough
        .lines()
        .filter_map(|l| l.strip_prefix("## "))
        .filter_map(|h| h.split_whitespace().next())
        .filter_map(|w| w.strip_suffix(".mt"))
        .map(str::to_string)
        .collect();
    documented.sort();
    documented.dedup();
    assert!(
        !documented.is_empty(),
        "no '## <name>.mt' sections found in docs/examples.md — the heading \
         format changed and this test is now blind. Fix the extraction, do not \
         delete the test."
    );

    // (1) The walkthrough may cover a subset, but every program it covers must
    //     be one that ships. A section for a deleted example documents nothing.
    let phantom: Vec<&String> = documented.iter().filter(|d| !on_disk.contains(d)).collect();
    assert!(
        phantom.is_empty(),
        "docs/examples.md walks through {:?}, which are not in examples/",
        phantom,
    );

    // (2) The count the prose states must be the count on disk. This is the
    //     half that went stale: seventeen programs described as seven.
    let total = number_word(on_disk.len());
    assert!(
        walkthrough.contains(&format!("contains **{}** programs", total)),
        "docs/examples.md must say it contains **{}** programs — examples/ holds \
         {} ({:?}). Update the opening paragraph.",
        total,
        on_disk.len(),
        on_disk,
    );

    // (3) And the number it claims to walk through must be the number it does.
    let covered = number_word(documented.len());
    assert!(
        walkthrough.contains(&format!("through {} of them", covered)),
        "docs/examples.md walks through {} programs ({:?}) but does not say \
         'through {} of them'.",
        documented.len(),
        documented,
        covered,
    );

    // (4) docs/index.md carried the same count and additionally claimed the
    //     walkthrough was of ALL of them. Both halves are pinned here, because
    //     the index is where a reader decides whether to open the file at all.
    let index = std::fs::read_to_string(root.join("docs/index.md"))
        .expect("docs/index.md must exist");
    let row = index
        .lines()
        .find(|l| l.contains("[examples.md](examples.md)"))
        .expect("docs/index.md must have a row for examples.md");
    assert!(
        row.contains(&format!("{} of the {} example programs", covered, total)),
        "docs/index.md's examples.md row must read '{} of the {} example \
         programs'; it reads:\n  {}",
        covered,
        total,
        row,
    );
    assert!(
        !row.contains("all "),
        "docs/index.md must not bill the walkthrough as covering ALL the \
         examples — it covers {} of {}. Row:\n  {}",
        documented.len(),
        on_disk.len(),
        row,
    );
}

// ---------------------------------------------------------------------------
// The documentation must agree with the repository it describes
// ---------------------------------------------------------------------------
// The walkthrough count above was one instance of a wider shape. Sweeping the
// same question across every tracked document found, in seven more files:
// thirty-two references to source paths that do not exist, three to a function
// that does not exist, seven wrong line counts, two wrong module counts, one
// wrong file count, and an index omitting six of the documents it claims to
// list. The sweep also separated two defects that look identical in prose:
//
//   * README.md's "(12 modules)" was EXACTLY RIGHT when it was written.
//     `STDLIB_MODULES` held twelve at the initial public release `94b46b8`;
//     six modules were added afterwards and the sentence was never reopened.
//     That is `examples.md`'s stale count again.
//
//   * `docs/compiler-internals.md` — billed by the index as covering "every
//     source file" — has NEVER been accurate in this repository's public
//     history. All five of its `**File:**` line counts were already wrong at
//     `94b46b8` (main.rs claimed 303, measured 456, measures 1,181 today), and
//     all five paths it names as `.rs` files were already DIRECTORIES then.
//     Not stale: wrong from the first public commit.
//
// Both read correctly in isolation, which is the whole difficulty, and only
// measurement tells them apart. These tests take every number and every path
// from disk, never from a list, so neither class can recur silently.
//
// Dated notices are exempt BY CONSTRUCTION: rule 7 keeps the retired wording in
// place, so a blockquote line is allowed to quote a path that no longer exists.
// That is why the extractors skip lines beginning with '>'. `docs/history/` is
// skipped for the same reason — its documents are byte-unchanged records.

/// Every `.md` this repository maintains as current documentation.
fn current_docs(root: &std::path::Path) -> Vec<PathBuf> {
    fn walk(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for e in entries.filter_map(|e| e.ok()) {
            let p = e.path();
            if p.is_dir() {
                // Superseded records, kept byte-unchanged on purpose.
                if p.file_name().is_some_and(|n| n == "history") {
                    continue;
                }
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "md") {
                out.push(p);
            }
        }
    }
    let mut out = Vec::new();
    walk(&root.join("docs"), &mut out);
    out.push(root.join("README.md"));
    out.push(root.join("GETTING_STARTED.md"));
    out.sort();
    out
}

/// Path-shaped tokens in a markdown HEADER, which carries no backticks.
///
/// This helper exists because the first version of
/// `documented_source_paths_exist` read backticked tokens only — and the
/// control PASSED with `### ir/lower.rs` reintroduced, because four of the five
/// original stale paths were section headings. A test that cannot see the form
/// the defect actually took is hollow; rule 9 is what caught it.
fn header_tokens(line: &str) -> Vec<String> {
    let t = line.trim_start();
    if !t.starts_with('#') || t.starts_with(">") {
        return Vec::new();
    }
    t.trim_start_matches('#')
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| c == ',' || c == ':'))
        .filter(|w| w.ends_with(".rs") || w.ends_with('/'))
        .map(str::to_string)
        .collect()
}

/// Backtick-quoted tokens on a line, ignoring dated-notice blockquotes.
fn backtick_tokens(line: &str) -> Vec<String> {
    if line.trim_start().starts_with('>') {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut rest = line;
    while let Some(a) = rest.find('`') {
        let after = &rest[a + 1..];
        match after.find('`') {
            Some(b) => {
                out.push(after[..b].to_string());
                rest = &after[b + 1..];
            }
            None => break,
        }
    }
    out
}

#[test]
fn documented_source_paths_exist() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    // Output directories a build tool creates; not source paths.
    const NOT_SOURCE: [&str; 1] = ["build/"];

    let mut missing: Vec<String> = Vec::new();
    let mut checked = 0usize;

    for doc in current_docs(&root) {
        let text = std::fs::read_to_string(&doc).expect("doc must be readable");
        let rel = doc.strip_prefix(&root).unwrap_or(&doc).display().to_string();

        for (n, line) in text.lines().enumerate() {
            let mut toks = backtick_tokens(line);
            toks.extend(header_tokens(line));
            for tok in toks {
                let is_path = tok.ends_with(".rs") || tok.ends_with('/');
                let has_name = tok.chars().any(|c| c.is_ascii_alphanumeric());
                if !is_path || !has_name || tok.contains(' ') || NOT_SOURCE.contains(&tok.as_str())
                {
                    continue;
                }
                checked += 1;

                let t = tok.strip_prefix("maniTC/").unwrap_or(&tok);
                // Qualified: resolve from the repo root or from src/.
                if root.join(t).exists() || root.join("src").join(t).exists() {
                    continue;
                }
                // Bare `foo.rs` in prose: accept it if exactly that file exists
                // somewhere under src/.
                if !t.contains('/') {
                    let mut hits = Vec::new();
                    fn find(dir: &std::path::Path, name: &str, out: &mut Vec<PathBuf>) {
                        if let Ok(rd) = std::fs::read_dir(dir) {
                            for e in rd.filter_map(|e| e.ok()) {
                                let p = e.path();
                                if p.is_dir() {
                                    find(&p, name, out);
                                } else if p.file_name().is_some_and(|f| f == name) {
                                    out.push(p);
                                }
                            }
                        }
                    }
                    find(&root.join("src"), t, &mut hits);
                    if !hits.is_empty() {
                        continue;
                    }
                }
                missing.push(format!("{}:{}  `{}`", rel, n + 1, tok));
            }
        }
    }

    assert!(
        checked > 40,
        "only {} source paths extracted from the documentation — the extractor \
         has gone blind. Fix it, do not delete the test.",
        checked,
    );
    assert!(
        missing.is_empty(),
        "{} documented source path(s) do not exist. Five module paths were \
         named as `.rs` files while being DIRECTORIES since the initial public \
         release, at thirty-two sites across four documents; this is that \
         class. Name the file or directory that actually holds the code:\n  {}",
        missing.len(),
        missing.join("\n  "),
    );
}

#[test]
fn documented_line_counts_match_the_source_files() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut wrong: Vec<String> = Vec::new();
    let mut checked = 0usize;

    let count_lines = |p: &std::path::Path| -> Option<usize> {
        if p.is_file() {
            return Some(std::fs::read_to_string(p).ok()?.lines().count());
        }
        if p.is_dir() {
            fn total(d: &std::path::Path, ext: &str) -> usize {
                let mut n = 0;
                if let Ok(rd) = std::fs::read_dir(d) {
                    for e in rd.filter_map(|e| e.ok()) {
                        let p = e.path();
                        if p.is_dir() {
                            n += total(&p, ext);
                        } else if p.extension().is_some_and(|x| x == ext) {
                            n += std::fs::read_to_string(&p).map(|s| s.lines().count()).unwrap_or(0);
                        }
                    }
                }
                n
            }
            return Some(total(p, "rs"));
        }
        None
    };

    for doc in current_docs(&root) {
        let text = std::fs::read_to_string(&doc).expect("doc must be readable");
        let rel = doc.strip_prefix(&root).unwrap_or(&doc).display().to_string();

        for (n, line) in text.lines().enumerate() {
            if line.trim_start().starts_with('>') || !line.starts_with("**File:**") {
                continue;
            }
            let toks = backtick_tokens(line);
            let Some(path_tok) = toks.first() else { continue };

            // "... (1,181 lines)" or "... — 4 files, 4,622 lines"
            let Some(pre) = line.rsplit_once(" lines") else { continue };
            let claimed: String = pre
                .0
                .chars()
                .rev()
                .take_while(|c| c.is_ascii_digit() || *c == ',')
                .filter(|c| c.is_ascii_digit())
                .collect();
            let Ok(claimed) = claimed.chars().rev().collect::<String>().parse::<usize>() else {
                continue;
            };
            checked += 1;

            let t = path_tok.strip_prefix("maniTC/").unwrap_or(path_tok);
            let target = if root.join(t).exists() {
                root.join(t)
            } else {
                root.join("src").join(t)
            };
            match count_lines(&target) {
                Some(actual) if actual == claimed => {}
                Some(actual) => wrong.push(format!(
                    "{}:{}  `{}` claims {} lines, measures {}",
                    rel,
                    n + 1,
                    path_tok,
                    claimed,
                    actual
                )),
                None => wrong.push(format!(
                    "{}:{}  `{}` does not exist",
                    rel,
                    n + 1,
                    path_tok
                )),
            }
        }
    }

    assert!(
        checked >= 12,
        "only {} '**File:** ... (N lines)' claims found — the format changed and \
         this test is now blind. Fix the extraction, do not delete the test.",
        checked,
    );
    assert!(
        wrong.is_empty(),
        "{} documented line count(s) disagree with the source. Every one of \
         compiler-internals.md's five was wrong at the initial public release \
         and none has ever been accurate, so these go stale silently. Update \
         the number in the document:\n  {}",
        wrong.len(),
        wrong.join("\n  "),
    );
}

#[test]
fn documented_stdlib_module_counts_match_the_registry() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    // The authoritative list: what the analyzer will accept a call into.
    let analyzer = std::fs::read_to_string(root.join("src/semantic/analyzer/mod.rs"))
        .expect("src/semantic/analyzer/mod.rs must exist");
    let start = analyzer
        .find("const STDLIB_MODULES:")
        .expect("STDLIB_MODULES must exist — if it was renamed, update this test");
    let body = &analyzer[start..];
    let end = body.find("];").expect("STDLIB_MODULES must be a closed slice");
    let registry: Vec<&str> = body[..end]
        .match_indices('"')
        .map(|(i, _)| i)
        .collect::<Vec<_>>()
        .chunks(2)
        .filter(|c| c.len() == 2)
        .map(|c| &body[c[0] + 1..c[1]])
        .collect();
    assert!(
        registry.len() > 5,
        "parsed only {} modules out of STDLIB_MODULES — the extraction broke",
        registry.len(),
    );

    // Every document that states the size of the standard library.
    let expected = format!("({} modules)", registry.len());
    for rel in ["README.md", "GETTING_STARTED.md"] {
        let text = std::fs::read_to_string(root.join(rel)).expect("document must exist");
        let stated: Vec<&str> = text
            .lines()
            .filter(|l| !l.trim_start().starts_with('>'))
            .filter(|l| l.contains(" modules)"))
            .collect();
        assert!(
            !stated.is_empty(),
            "{} no longer states a stdlib module count; if the sentence was \
             removed, remove it from this test's list too",
            rel,
        );
        for line in stated {
            assert!(
                line.contains(&expected),
                "{} must say '{}' — STDLIB_MODULES holds {} ({:?}). It reads:\n  {}",
                rel,
                expected,
                registry.len(),
                registry,
                line.trim(),
            );
        }
    }
}

#[test]
fn the_documentation_index_lists_every_document() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let index_path = root.join("docs/index.md");
    let index = std::fs::read_to_string(&index_path).expect("docs/index.md must exist");

    // Every markdown link target in the index, as written.
    let mut linked: Vec<String> = Vec::new();
    let mut rest = index.as_str();
    while let Some(a) = rest.find("](") {
        let after = &rest[a + 2..];
        match after.find(')') {
            Some(b) => {
                linked.push(after[..b].to_string());
                rest = &after[b + 1..];
            }
            None => break,
        }
    }

    let docs_dir = root.join("docs");
    let mut unlisted: Vec<String> = Vec::new();
    let mut total = 0usize;

    for doc in current_docs(&root) {
        if !doc.starts_with(&docs_dir) || doc == index_path {
            continue;
        }
        let rel = doc.strip_prefix(&docs_dir).unwrap().display().to_string();
        total += 1;
        // Listed directly, or reachable through its directory's own README.
        let via_readme = doc.parent().map(|d| d.join("README.md")).is_some_and(|r| {
            r.exists()
                && r != doc
                && linked.contains(
                    &r.strip_prefix(&docs_dir).unwrap().display().to_string(),
                )
        });
        if !linked.contains(&rel) && !via_readme {
            unlisted.push(rel);
        }
    }

    assert!(
        total > 8,
        "only {} documents found under docs/ — the walk is measuring nothing",
        total,
    );
    assert!(
        unlisted.is_empty(),
        "docs/index.md is headed \"Documents in this directory\" but does not \
         list {} of the {} documents under docs/: {:?}. It once omitted \
         semantics.md, the NORMATIVE specification. Add a row, or link the \
         directory's own README.",
        unlisted.len(),
        total,
        unlisted,
    );

    // A row may describe a subset — that is a choice. What it may not do is
    // bill a subset as everything. This is P93's "all seven" one site along:
    // the compiler-internals row read "Every source file ... in the compiler"
    // while the document names 25 of 66.
    let row = index
        .lines()
        .find(|l| l.contains("[compiler-internals.md](compiler-internals.md)"))
        .expect("docs/index.md must have a row for compiler-internals.md");
    if row.contains("Every source file") {
        let doc = std::fs::read_to_string(docs_dir.join("compiler-internals.md"))
            .expect("docs/compiler-internals.md must exist");
        let named = doc
            .lines()
            .filter_map(|l| l.strip_prefix("#"))
            .flat_map(|h| h.split_whitespace())
            .filter(|w| w.ends_with(".rs"))
            .count();
        let mut on_disk = 0usize;
        fn count_rs(d: &std::path::Path, n: &mut usize) {
            if let Ok(rd) = std::fs::read_dir(d) {
                for e in rd.filter_map(|e| e.ok()) {
                    let p = e.path();
                    if p.is_dir() {
                        count_rs(&p, n);
                    } else if p.extension().is_some_and(|x| x == "rs") {
                        *n += 1;
                    }
                }
            }
        }
        count_rs(&root.join("src"), &mut on_disk);
        panic!(
            "docs/index.md bills compiler-internals.md as covering EVERY source \
             file. It names {} in headings; src/ holds {}. Either document them \
             all, or word the row for what it delivers.",
            named, on_disk,
        );
    }
}
