//! Front-end regression tests.
//!
//! Each test pins the fix for a specific lexer/parser defect. Sources are
//! written to a per-process temp directory and run through `manitc parse`
//! (parse must succeed / fail with the expected message) or `manitc lex`.

use std::path::PathBuf;
use std::process::Command;

mod common;

fn get_manitc() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_manitc"))
}

/// Write `source` to a temp .mt file and return its path.
fn write_source(name: &str, source: &str) -> PathBuf {
    let dir = common::suite_root("frontend_regr");
    std::fs::create_dir_all(&dir).expect("failed to create temp dir");
    let path = dir.join(name);
    std::fs::write(&path, source).expect("failed to write test source");
    path
}

/// Run `manitc <cmd> <file>` and return (success, stdout, stderr).
fn run_manitc(cmd: &str, path: &PathBuf) -> (bool, String, String) {
    let output = Command::new(get_manitc())
        .args([cmd, path.to_str().unwrap()])
        .output()
        .expect("failed to run manitc");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Assert that the source parses successfully.
fn assert_parses(name: &str, source: &str) -> String {
    let path = write_source(name, source);
    let (ok, stdout, stderr) = run_manitc("parse", &path);
    assert!(ok, "{} should parse, but failed:\n{}", name, stderr);
    stdout
}

/// Assert that parsing fails and the error mentions `expect_substr`.
fn assert_parse_error(name: &str, source: &str, expect_substr: &str) {
    let path = write_source(name, source);
    let (ok, stdout, stderr) = run_manitc("parse", &path);
    assert!(!ok, "{} should FAIL to parse, but succeeded:\n{}", name, stdout);
    assert!(
        stderr.contains(expect_substr),
        "{}: expected error containing '{}', got:\n{}",
        name, expect_substr, stderr,
    );
}

// ---------------------------------------------------------------------------
// F1 — balanced ternary literal overflow is a lexer error, not a panic
// ---------------------------------------------------------------------------

#[test]
fn f1_ternary_literal_overflow_is_error() {
    let literal = format!("0t{}0", "+".repeat(40));
    assert_parse_error(
        "f1_overflow.mt",
        &format!("fn main() {{ let x: int = {}; }}\n", literal),
        "out of range",
    );
}

// ---------------------------------------------------------------------------
// F2 — no struct literal in condition/iterator/scrutinee position
// ---------------------------------------------------------------------------

#[test]
fn f2_control_flow_with_empty_blocks() {
    assert_parses(
        "f2_empty_blocks.mt",
        "fn main() {\n\
         \x20   let x: bool = true;\n\
         \x20   while x { }\n\
         \x20   for i in 0..3 {}\n\
         \x20   if x {} else { }\n\
         \x20   let xs: [int; 2] = [1, 2];\n\
         \x20   for i in xs {}\n\
         \x20   match x { _ => 0 }\n\
         }\n",
    );
}

#[test]
fn f2_struct_literal_still_allowed_in_parens_and_args() {
    assert_parses(
        "f2_struct_lit_ok.mt",
        "struct Point { x: int }\n\
         fn f(p: Point) -> bool { true }\n\
         fn main() {\n\
         \x20   if (Point { x: 1 }).x > 0 { }\n\
         \x20   while f(Point { x: 2 }) { }\n\
         }\n",
    );
}

// ---------------------------------------------------------------------------
// F3 — ternary literal lexing does not swallow binary +/- operators
// ---------------------------------------------------------------------------

#[test]
fn f3_ternary_literal_minus_binds_as_operator() {
    // `0t+0-1` must lex as TernaryInt(3), Minus, Int(1) — i.e. 3 - 1 = 2.
    let path = write_source(
        "f3_lex.mt",
        "fn main() { let x: int = 0t+0-1; }\n",
    );
    let (ok, stdout, stderr) = run_manitc("lex", &path);
    assert!(ok, "f3 lex failed:\n{}", stderr);
    assert!(stdout.contains("TernaryInt(3)"), "expected TernaryInt(3) in:\n{}", stdout);
    assert!(stdout.contains("Minus"), "expected Minus token in:\n{}", stdout);
    assert!(stdout.contains("Int(1)"), "expected Int(1) token in:\n{}", stdout);
}

#[test]
fn f3_multi_trit_literals_still_lex_whole() {
    let path = write_source("f3_lex_whole.mt", "fn main() { let x: int = 0t+-0+; }\n");
    let (ok, stdout, stderr) = run_manitc("lex", &path);
    assert!(ok, "f3 lex failed:\n{}", stderr);
    assert!(stdout.contains("TernaryInt(19)"), "expected TernaryInt(19) in:\n{}", stdout);
}

// ---------------------------------------------------------------------------
// F4 — unary + parses its operand (numeric identity)
// ---------------------------------------------------------------------------

#[test]
fn f4_unary_plus_keeps_operand() {
    let stdout = assert_parses("f4_unary_plus.mt", "fn main() { let x: int = +5; }\n");
    assert!(stdout.contains("Int(5)"), "expected operand Int(5) in AST:\n{}", stdout);
    assert!(!stdout.contains("Trit(1)"), "`+5` must not parse as Trit(1):\n{}", stdout);
}

#[test]
fn f4_bare_plus_is_still_trit_literal() {
    let stdout = assert_parses(
        "f4_trit_plus.mt",
        "fn main() { let t: trit = +; let u: trit = -; }\n",
    );
    assert!(stdout.contains("Trit(1)"), "bare `+` must stay Trit(1):\n{}", stdout);
    assert!(stdout.contains("Trit(-1)"), "bare `-` must stay Trit(-1):\n{}", stdout);
}

// ---------------------------------------------------------------------------
// F5 — split `>>` stays in sync between peek() and advance()
// ---------------------------------------------------------------------------

#[test]
fn f5_nested_generics_and_channel() {
    assert_parses(
        "f5_split_gt.mt",
        "fn main() {\n\
         \x20   let c = channel<Vec<int>>();\n\
         \x20   let v: Vec<Vec<int>> = Vec::new();\n\
         }\n",
    );
}

// ---------------------------------------------------------------------------
// F6 — tuple indexing t.0
// ---------------------------------------------------------------------------

#[test]
fn f6_tuple_indexing() {
    let stdout = assert_parses(
        "f6_tuple_index.mt",
        "fn main() { let t = (1, 2); let x = t.0; let y = t.1; }\n",
    );
    assert!(stdout.contains("Field"), "expected Field access in AST:\n{}", stdout);
}

// ---------------------------------------------------------------------------
// F7 — char and ternary-int literal patterns
// ---------------------------------------------------------------------------

#[test]
fn f7_char_and_ternary_patterns() {
    assert_parses(
        "f7_patterns.mt",
        "fn main() {\n\
         \x20   let c: char = 'a';\n\
         \x20   let x = match c { 'a' => 1, _ => 2 };\n\
         \x20   let n: int = 0t+-;\n\
         \x20   let y = match n { 0t+- => 1, _ => 2 };\n\
         }\n",
    );
}

// ---------------------------------------------------------------------------
// F8 — statements may not silently merge
// ---------------------------------------------------------------------------

#[test]
fn f8_missing_semicolon_between_statements_is_error() {
    assert_parse_error(
        "f8_merge_let.mt",
        "fn main() { let x: int = 1 2; }\n",
        "expected `;`",
    );
    assert_parse_error(
        "f8_merge_exprs.mt",
        "fn main() { foo() bar(); }\n",
        "expected `;`",
    );
}

#[test]
fn f8_tail_expression_without_semicolon_still_allowed() {
    assert_parses(
        "f8_tail_expr.mt",
        "fn add(a: int, b: int) -> int {\n\
         \x20   a + b\n\
         }\n\
         fn main() { let x = add(1, 2); }\n",
    );
}

// ---------------------------------------------------------------------------
// F9 — negative float / negative ternary-int patterns
// ---------------------------------------------------------------------------

#[test]
fn f9_negative_literal_patterns() {
    let stdout = assert_parses(
        "f9_neg_patterns.mt",
        "fn main() {\n\
         \x20   let f: float = -1.5;\n\
         \x20   let x = match f { -1.5 => 1, _ => 2 };\n\
         \x20   let n: int = 0t+-;\n\
         \x20   let y = match n { -0t+- => 1, _ => 2 };\n\
         }\n",
    );
    assert!(stdout.contains("Float(-1.5)"), "expected Float(-1.5) pattern in:\n{}", stdout);
}

// ---------------------------------------------------------------------------
// F10 — unterminated block comment
// ---------------------------------------------------------------------------

#[test]
fn f10_unterminated_block_comment_is_error() {
    assert_parse_error(
        "f10_comment.mt",
        "fn main() { }\n/* never closed\n",
        "unterminated block comment",
    );
}

// ---------------------------------------------------------------------------
// F11 — malformed char literals
// ---------------------------------------------------------------------------

#[test]
fn f11_empty_and_triple_quote_char_literals_are_errors() {
    assert_parse_error(
        "f11_triple_quote.mt",
        "fn main() { let c: char = '''; }\n",
        "empty character literal",
    );
    assert_parse_error(
        "f11_empty.mt",
        "fn main() { let c: char = ''; }\n",
        "empty character literal",
    );
}

// ---------------------------------------------------------------------------
// F13 — tif / tresult arms are required and may not repeat
// ---------------------------------------------------------------------------

#[test]
fn f13_tif_missing_and_duplicate_arms() {
    assert_parse_error(
        "f13_tif_missing.mt",
        "fn main() { let t: trit = +; tif t { + => 1, 0 => 2 } }\n",
        "missing `-`",
    );
    assert_parse_error(
        "f13_tif_duplicate.mt",
        "fn main() { let t: trit = +; tif t { + => 1, + => 2, 0 => 3, - => 4 } }\n",
        "duplicate `+` arm",
    );
}

#[test]
fn f13_tresult_missing_arm() {
    assert_parse_error(
        "f13_tresult_missing.mt",
        "fn main() { let r = Ok(1); tresult r { Ok(v) => 1, Err(e) => 2 } }\n",
        "missing `Unknown`",
    );
}

// ---------------------------------------------------------------------------
// F14 — keyword-prefixed paths with multiple segments
// ---------------------------------------------------------------------------

#[test]
fn f14_multi_segment_keyword_path() {
    let stdout = assert_parses(
        "f14_async_path.mt",
        "fn main() { async::task::spawn(); }\n",
    );
    assert!(
        stdout.contains("async::task::spawn"),
        "expected full path in AST:\n{}",
        stdout
    );
}

// ---------------------------------------------------------------------------
// F16 — `mod` gets a targeted diagnostic
// ---------------------------------------------------------------------------

#[test]
fn f16_mod_block_has_specific_error() {
    assert_parse_error(
        "f16_mod.mt",
        "mod foo { }\n",
        "module blocks are not supported",
    );
}

// ---------------------------------------------------------------------------
// F17 — chained comparison is a parse error
// ---------------------------------------------------------------------------

#[test]
fn f17_chained_comparison_is_error() {
    assert_parse_error(
        "f17_chained_cmp.mt",
        "fn main() { let x = 1 < 2 < 3; }\n",
        "cannot be chained",
    );
    // A single comparison, and separate parenthesized ones, still parse.
    assert_parses(
        "f17_cmp_ok.mt",
        "fn main() { let a = 1 < 2; let b = (1 < 2) == true; }\n",
    );
}

// ---------------------------------------------------------------------------
// bridge_demo — array literals with signed elements and repeat form
// ---------------------------------------------------------------------------

#[test]
fn bridge_demo_array_literals_parse() {
    assert_parses(
        "bridge_arrays.mt",
        "fn main() {\n\
         \x20   let value: [trit; 5] = [+1, -1, 0, +1, -1];\n\
         \x20   let decoded: [trit; 5] = [0; 5];\n\
         }\n",
    );
}

#[test]
fn bridge_demo_example_parses() {
    let example = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("bridge_demo.mt");
    let (ok, _stdout, stderr) = run_manitc("parse", &example);
    assert!(ok, "examples/bridge_demo.mt should parse, but failed:\n{}", stderr);
}
