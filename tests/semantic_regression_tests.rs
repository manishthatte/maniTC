//! Semantic-analysis & borrow-checker regression tests.
//!
//! Each test pins the fix for a specific defect (S1-S18, K2, K5 from the
//! 2026-08-09 review). Sources are written to a per-process temp directory and
//! run through `manitc check` (must succeed / fail with the expected message)
//! or `manitc run-t3` (behavioral pins).

use std::path::PathBuf;
use std::process::Command;

mod common;

fn get_manitc() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_manitc"))
}

/// Write `source` to a temp .mt file and return its path.
fn write_source(name: &str, source: &str) -> PathBuf {
    let dir = common::suite_root("semantic_regr");
    std::fs::create_dir_all(&dir).expect("failed to create temp dir");
    let path = dir.join(name);
    std::fs::write(&path, source).expect("failed to write test source");
    path
}

/// Run `manitc <cmd> <file>` and return (success, combined stdout+stderr).
fn run_manitc(cmd: &str, path: &PathBuf) -> (bool, String) {
    let output = Command::new(get_manitc())
        .args([cmd, path.to_str().unwrap()])
        .output()
        .expect("failed to run manitc");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    (output.status.success(), combined)
}

/// Assert that the source passes `manitc check`.
fn assert_checks(name: &str, source: &str) -> String {
    let path = write_source(name, source);
    let (ok, out) = run_manitc("check", &path);
    assert!(ok, "{} should type-check, but failed:\n{}", name, out);
    out
}

/// Assert that `manitc check` fails and the output contains `expected`.
fn assert_check_error(name: &str, source: &str, expected: &str) {
    let path = write_source(name, source);
    let (ok, out) = run_manitc("check", &path);
    assert!(!ok, "{} should FAIL type-checking, but succeeded:\n{}", name, out);
    assert!(
        out.contains(expected),
        "{}: expected error containing '{}', got:\n{}",
        name, expected, out
    );
}

// ===========================================================================
// S1 — struct literal field order / validation
// ===========================================================================

#[test]
fn s1_struct_literal_source_order_does_not_leak() {
    // IR assigns fields by position: the semantic pass must reorder
    // `Point { y: 2, x: 1 }` into declaration order.
    let path = write_source(
        "s1_order.mt",
        r#"
use std::io;
struct Point { pub x: int, pub y: int }
fn main() {
    let p = Point { y: 2, x: 1 };
    io::println_int(p.x);
    io::println_int(p.y);
}
"#,
    );
    let out_base = path.with_extension("");
    let compile = Command::new(get_manitc())
        .args([
            "compile", path.to_str().unwrap(),
            "--target", "t3",
            "-o", out_base.to_str().unwrap(),
        ])
        .output()
        .expect("failed to run manitc compile");
    assert!(
        compile.status.success(),
        "s1_order should compile:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let t3b = out_base.with_extension("t3b");
    let (ok, out) = run_manitc("run-t3", &t3b);
    assert!(ok, "s1_order should run, got:\n{}", out);
    let nums: Vec<&str> = out
        .lines()
        .map(|l| l.trim())
        .filter(|l| *l == "1" || *l == "2")
        .collect();
    assert_eq!(nums, vec!["1", "2"], "p.x must be 1 and p.y must be 2, got:\n{}", out);
}

#[test]
fn s1_missing_field_is_error() {
    assert_check_error(
        "s1_missing.mt",
        "struct Point { x: int, y: int }\nfn main() { let p = Point { x: 1 }; }\n",
        "missing field",
    );
}

#[test]
fn s1_unknown_field_is_error() {
    assert_check_error(
        "s1_unknown_field.mt",
        "struct Point { x: int, y: int }\nfn main() { let p = Point { x: 1, y: 2, z: 3 }; }\n",
        "has no field 'z'",
    );
}

#[test]
fn s1_unknown_struct_is_error() {
    assert_check_error(
        "s1_unknown_struct.mt",
        "struct Point { x: int, y: int }\nfn main() { let p = Pointt { x: 1 }; }\n",
        "unknown struct 'Pointt'",
    );
}

#[test]
fn s1_duplicate_field_is_error() {
    assert_check_error(
        "s1_dup_field.mt",
        "struct Point { x: int, y: int }\nfn main() { let p = Point { x: 1, x: 2, y: 3 }; }\n",
        "more than once",
    );
}

// ===========================================================================
// S2 — lambda capture detection
// ===========================================================================

#[test]
fn s2_capture_after_prior_read_is_detected() {
    // `a` is read BEFORE the lambda; the old read_vars heuristic missed this.
    assert_check_error(
        "s2_capture.mt",
        r#"
fn main() {
    let a: int = 5;
    let b = a + 1;
    let f = fn(x: int) -> int => x + a;
    let c = f(b);
}
"#,
        "captures outer variable 'a'",
    );
}

#[test]
fn s2_module_global_is_not_a_capture() {
    assert_checks(
        "s2_global.mt",
        r#"
use std::io;
let G: int = 10;
fn main() {
    let f = fn(x: int) -> int => x + G;
    io::println_int(f(1));
}
"#,
    );
}

// ===========================================================================
// S3 — binary operator operand checking
// ===========================================================================

#[test]
fn s3_string_minus_int_is_error() {
    assert_check_error(
        "s3_str_minus.mt",
        "fn main() { let x = \"a\" - 1; }\n",
        "cannot be applied",
    );
}

#[test]
fn s3_bool_times_bool_is_error() {
    assert_check_error(
        "s3_bool_mul.mt",
        "fn main() { let x = true * false; }\n",
        "cannot be applied",
    );
}

#[test]
fn s3_string_less_than_int_is_error() {
    assert_check_error(
        "s3_str_lt.mt",
        "fn main() { let x = \"x\" < 5; }\n",
        "cannot be applied",
    );
}

#[test]
fn s3_logical_and_on_ints_is_error() {
    assert_check_error(
        "s3_and_int.mt",
        "fn main() { let x = 1 && 2; }\n",
        "cannot be applied",
    );
}

#[test]
fn s3_tand_on_str_is_error() {
    assert_check_error(
        "s3_tand_str.mt",
        "fn main() { let x = \"a\" tand \"b\"; }\n",
        "cannot be applied",
    );
}

// ===========================================================================
// S4 — let annotation vs initialiser
// ===========================================================================

#[test]
fn s4_annotation_mismatch_is_error() {
    assert_check_error(
        "s4_mismatch.mt",
        "fn main() { let x: int = \"hello\"; }\n",
        "type mismatch",
    );
}

#[test]
fn s4_int_literal_into_trit_is_ok() {
    // Docs-blessed coercion: int literals flow into ternary-typed contexts.
    assert_checks("s4_trit.mt", "fn main() { let t: trit = 1; let w: t27 = 100; }\n");
}

// ===========================================================================
// S5 — assignment: mutability, types, lvalues
// ===========================================================================

#[test]
fn s5_assign_to_immutable_is_error() {
    assert_check_error(
        "s5_immutable.mt",
        "fn main() { let x = 1; x = 2; }\n",
        "cannot assign to immutable variable 'x'",
    );
}

#[test]
fn s5_assign_to_mut_is_ok() {
    assert_checks("s5_mut.mt", "fn main() { let mut x = 1; x = 2; }\n");
}

#[test]
fn s5_assign_to_param_is_error() {
    assert_check_error(
        "s5_param.mt",
        "fn f(n: int) -> int { n = 5; n }\nfn main() { let x = f(1); }\n",
        "cannot assign to immutable variable 'n'",
    );
}

#[test]
fn s5_assign_type_mismatch_is_error() {
    assert_check_error(
        "s5_type.mt",
        "fn main() { let mut x = 1; x = \"s\"; }\n",
        "type mismatch",
    );
}

#[test]
fn s5_uninitialised_let_accepts_first_assignment() {
    // Docs: "left uninitialised until first assignment".
    assert_checks("s5_uninit.mt", "fn main() { let x: int; x = 5; }\n");
}

// ===========================================================================
// S6 — call arity and argument types
// ===========================================================================

#[test]
fn s6_user_fn_arity_is_checked() {
    assert_check_error(
        "s6_arity.mt",
        "fn add(a: int, b: int) -> int { a + b }\nfn main() { let x = add(1); }\n",
        "expects 2 argument(s), found 1",
    );
}

#[test]
fn s6_user_fn_arg_type_is_checked() {
    assert_check_error(
        "s6_argty.mt",
        "fn add(a: int, b: int) -> int { a + b }\nfn main() { let x = add(1, \"s\"); }\n",
        "argument 2 to 'add'",
    );
}

#[test]
fn s6_builtin_abs_arity_is_checked() {
    assert_check_error(
        "s6_abs.mt",
        "fn main() { let x = abs(); }\n",
        "expects 1 argument(s), found 0",
    );
}

#[test]
fn s6_builtin_sqrt_arg_type_is_checked() {
    assert_check_error(
        "s6_sqrt.mt",
        "fn main() { let x = sqrt(\"hi\"); }\n",
        "argument 1 to 'sqrt'",
    );
}

#[test]
fn s6_variadic_builtins_stay_permissive() {
    // println's registry entry is a placeholder — must not be enforced.
    assert_checks("s6_println.mt", "fn main() { println(\"a\", 1, 2); }\n");
}

// ===========================================================================
// S7 — branch type agreement (if / match / tif as values)
// ===========================================================================

#[test]
fn s7_if_branch_mismatch_is_error() {
    assert_check_error(
        "s7_if.mt",
        "fn main() { let x = if 1 < 2 { 1 } else { \"s\" }; }\n",
        "incompatible types",
    );
}

#[test]
fn s7_match_arm_mismatch_is_error() {
    assert_check_error(
        "s7_match.mt",
        "fn main() { let x = match 1 { 1 => 1, _ => \"s\" }; }\n",
        "incompatible types",
    );
}

#[test]
fn s7_tif_arm_mismatch_is_error() {
    assert_check_error(
        "s7_tif.mt",
        "fn main() { let t: trit = 1; let x = tif t { + => 1, 0 => \"s\", - => 2 }; }\n",
        "incompatible types",
    );
}

#[test]
fn s7_statement_if_with_void_branches_is_ok() {
    assert_checks(
        "s7_stmt_if.mt",
        r#"
use std::io;
fn main() {
    if 1 < 2 { io::println("a"); } else { let x = 5; }
}
"#,
    );
}

// ===========================================================================
// S8 — conditions must be bool
// ===========================================================================

#[test]
fn s8_while_int_condition_is_error() {
    assert_check_error(
        "s8_while.mt",
        "fn main() { while 5 { let a = 1; } }\n",
        "while condition must be `bool`",
    );
}

#[test]
fn s8_if_int_condition_is_error() {
    assert_check_error(
        "s8_if.mt",
        "fn main() { if 5 { let a = 1; } }\n",
        "if condition must be `bool`",
    );
}

// ===========================================================================
// S9 — guarded arms never count toward exhaustiveness
// ===========================================================================

#[test]
fn s9_guarded_arm_is_not_a_catch_all() {
    assert_check_error(
        "s9_guard.mt",
        r#"
fn f(t: trit) -> int {
    match t {
        x if x > 0 => 1,
        0 => 2,
        - => 3,
    }
}
fn main() { let x = f(1); }
"#,
        "non-exhaustive match on trit",
    );
}

#[test]
fn s9_guarded_plus_full_coverage_is_ok() {
    assert_checks(
        "s9_full.mt",
        r#"
fn f(t: trit) -> int {
    match t {
        x if x > 0 => 9,
        + => 1,
        0 => 2,
        - => 3,
    }
}
fn main() { let x = f(1); }
"#,
    );
}

// ===========================================================================
// S10 — unknown methods on generic receivers warn (typed Unknown, not int)
// ===========================================================================

#[test]
fn s10_unknown_method_on_vec_warns() {
    let out = assert_checks(
        "s10_psuh.mt",
        r#"
fn main() {
    let v: Vec<int> = Vec::new();
    v.psuh(1);
}
"#,
    );
    assert!(
        out.contains("unknown method 'psuh'"),
        "expected a warning about method 'psuh', got:\n{}",
        out
    );
}

// ===========================================================================
// S11 — module privacy and transitive imports
// ===========================================================================

#[test]
fn s11_private_module_item_is_error() {
    write_source(
        "privmod.mt",
        "pub fn visible() -> int { 1 }\nfn hidden() -> int { 2 }\n",
    );
    assert_check_error(
        "s11_priv_main.mt",
        "use privmod;\nfn main() { let x = privmod::hidden(); }\n",
        "private",
    );
}

#[test]
fn s11_public_module_item_is_ok() {
    write_source(
        "pubmod.mt",
        "pub fn visible() -> int { 1 }\nfn hidden() -> int { 2 }\n",
    );
    let out = assert_checks(
        "s11_pub_main.mt",
        "use pubmod;\nfn main() { let x = pubmod::visible(); }\n",
    );
    assert!(
        !out.contains("has no item"),
        "no unknown-item warning expected, got:\n{}",
        out
    );
}

#[test]
fn s11_transitive_use_is_registered() {
    write_source("basemod.mt", "pub fn base_fn() -> int { 7 }\n");
    write_source(
        "midmod.mt",
        "use basemod;\npub fn mid_fn() -> int { basemod::base_fn() }\n",
    );
    let out = assert_checks(
        "s11_trans_main.mt",
        r#"
use midmod;
fn main() {
    let a = midmod::mid_fn();
    let b = basemod::base_fn();
}
"#,
    );
    assert!(
        !out.contains("unknown module"),
        "transitively imported module must resolve, got:\n{}",
        out
    );
}

// ===========================================================================
// S12 — unknown `::` paths produce diagnostics
// ===========================================================================

#[test]
fn s12_unknown_std_item_is_an_error() {
    // This was a WARNING until 21 Aug 2026, and the warning was worse than
    // useless: it typed the path `Unknown` and let it through to codegen,
    // where the build died against a mangled symbol the programmer never
    // wrote — `@io_print_bool`, `@_get`, `Undefined label:` — with no line
    // number and nothing tying it back to the call. Three debugging sessions
    // were spent walking those symbols back by hand.
    let path = write_source(
        "s12_sqrtt.mt",
        "use std::math;\nfn main() { let x = math::sqrtt(4.0); }\n",
    );
    let (ok, out) = run_manitc("check", &path);
    assert!(!ok, "math::sqrtt must FAIL to check, got:\n{}", out);
    assert!(
        out.contains("has no item 'sqrtt'"),
        "expected the item named, got:\n{}",
        out
    );
    // The point of the change is the location, so pin it: the old link error
    // had none.
    assert!(
        out.contains("s12_sqrtt.mt:2:"),
        "the error must carry file:line:col, got:\n{}",
        out
    );
    assert!(
        out.contains("did you mean 'sqrt'"),
        "expected a suggestion, got:\n{}",
        out
    );
}

#[test]
fn s12_suggestion_is_the_same_on_every_run() {
    // `env::argv` sits at distance 1 from BOTH `arg` and `args`. The tie used
    // to be broken by whichever candidate the HashSet iterator yielded first,
    // and Rust seeds that order randomly PER PROCESS — so this same file
    // produced "did you mean 'arg'" on one run and "did you mean 'args'" on
    // the next. Each run is a fresh process, which is exactly what varies.
    let path = write_source(
        "s12_tie.mt",
        "use std::env;\nfn main() { let a = env::argv(); }\n",
    );
    let first = run_manitc("check", &path).1;
    for run in 2..=8 {
        let out = run_manitc("check", &path).1;
        assert_eq!(
            first, out,
            "run {} disagreed with run 1 on the same input:\n--- 1 ---\n{}\n--- {} ---\n{}",
            run, first, run, out
        );
    }
    assert!(
        first.contains("did you mean 'arg'"),
        "expected the lexicographically-first of the tied candidates, got:\n{}",
        first
    );
}

#[test]
fn s12_async_natives_are_not_rejected_by_the_hardened_check() {
    // The guard that makes the error above safe. `async.mt` declares its six
    // natives as `async fn`, which `scan_module_members` did not recognise
    // until 50a6f4a — so the member list did not have them and this hardened
    // path would have rejected three functions that work on BOTH backends.
    // The unit test `every_registered_builtin_is_in_its_module_member_list`
    // proves the invariant; this pins the user-visible consequence.
    assert_checks(
        "s12_async_ok.mt",
        "use std::async;\nfn main() { async::sleep(1); }\n",
    );
}

#[test]
fn s12_user_module_shadowing_a_stdlib_name_still_only_warns() {
    // A user module may legitimately be called `math`. Its module-scope
    // globals are NOT registered by `load_user_module` (a known gap), so they
    // reach this same code path — and they must not be measured against the
    // STDLIB math member list, or a missing feature becomes a hard rejection
    // of correct code.
    write_source("math.mt", "pub let SHADOW_ANSWER: int = 42;\n");
    let out = assert_checks(
        "s12_shadow.mt",
        "use math;\nfn main() { let x = math::SHADOW_ANSWER; }\n",
    );
    assert!(
        out.contains("has no item 'SHADOW_ANSWER'"),
        "expected a warning, not silence, got:\n{}",
        out
    );
}

#[test]
fn s12_unknown_module_warns() {
    let out = assert_checks(
        "s12_maths.mt",
        "fn main() { let x = maths::sqrt(4.0); }\n",
    );
    assert!(
        out.contains("unknown module or type 'maths'"),
        "expected a warning about module 'maths', got:\n{}",
        out
    );
}

#[test]
fn s12_known_std_item_is_silent() {
    let out = assert_checks(
        "s12_ok.mt",
        "use std::io;\nfn main() { io::println_int(1); }\n",
    );
    assert!(
        !out.contains("has no item"),
        "io::println_int must not warn, got:\n{}",
        out
    );
}

// ===========================================================================
// S13 — rebinding a moved variable clears the move
// ===========================================================================

#[test]
fn s13_reassign_after_move_is_ok() {
    assert_checks(
        "s13_rebind.mt",
        r#"
fn main() {
    let mut s = "a";
    let t = s;
    s = "b";
    let u = s;
}
"#,
    );
}

// ===========================================================================
// S14 — moved-set is keyed by binding (shadowing works both ways)
// ===========================================================================

#[test]
fn s14_inner_shadow_move_does_not_poison_outer() {
    assert_checks(
        "s14_inner.mt",
        r#"
fn main() {
    let s = "outer";
    if 1 < 2 {
        let s = "inner";
        let t = s;
    }
    let u = s;
}
"#,
    );
}

#[test]
fn s14_inner_let_does_not_launder_outer_move() {
    assert_check_error(
        "s14_launder.mt",
        r#"
fn main() {
    let s = "outer";
    let t = s;
    if 1 < 2 {
        let s = "inner";
        let x = s;
    }
    let u = s;
}
"#,
        "use of moved value: 's'",
    );
}

// ===========================================================================
// S15 — moves of loop-local variables are not "moves in a loop"
// ===========================================================================

#[test]
fn s15_loop_local_move_is_ok() {
    assert_checks(
        "s15_local.mt",
        r#"
fn main() {
    let mut i = 0;
    while i < 3 {
        let a = "x";
        let b = a;
        i = i + 1;
    }
}
"#,
    );
}

#[test]
fn s15_outer_move_in_loop_is_still_error() {
    assert_check_error(
        "s15_outer.mt",
        r#"
fn main() {
    let s = "a";
    let mut i = 0;
    while i < 3 {
        let t = s;
        i = i + 1;
    }
}
"#,
        "cannot move 's' in a loop",
    );
}

// ===========================================================================
// S16 — tresult arms fork the moved-set like if/match
// ===========================================================================
// NOTE: the `tresult` keyword's `Unknown` arm currently cannot be written in
// source (the lexer tokenises `Unknown` as a keyword and the parser only
// accepts identifier arm labels — a front-end limitation outside this
// suite's scope), so S16 is pinned by a unit test on the typed AST:
// `borrow::tests::test_tresult_arms_fork_moved_set`.

// ===========================================================================
// S17 — tuple destructuring must not retype the first element
// ===========================================================================

#[test]
fn s17_tuple_destructure_elements_are_copy() {
    assert_checks(
        "s17_tuple.mt",
        r#"
fn main() {
    let (a, b) = (1, 2);
    let x = a + a;
    let y = b + a;
}
"#,
    );
}

// ===========================================================================
// S18 — tif on Unknown-typed condition is permitted
// ===========================================================================

#[test]
fn s18_tif_on_generic_value_is_ok() {
    assert_checks(
        "s18_generic.mt",
        r#"
fn pick<T>(t: T) -> int {
    tif t { + => 1, 0 => 0, - => 2 }
}
fn main() { let x = pick(1); }
"#,
    );
}

#[test]
fn s18_tif_on_int_is_still_error() {
    // (An int VARIABLE, not a literal — bare int literals coerce to trit
    // under the condition's hint.)
    assert_check_error(
        "s18_int.mt",
        "fn main() { let n: int = 5; let x = tif n { + => 1, 0 => 0, - => 2 }; }\n",
        "tif condition must be `trit` or `bool3`",
    );
}

// ===========================================================================
// K2 — std::t27f resolves as a stdlib module
// ===========================================================================

#[test]
fn k2_std_t27f_import_resolves() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("float_demo.mt");
    let (ok, out) = run_manitc("check", &path);
    assert!(ok, "float_demo.mt must get past import resolution, got:\n{}", out);
    assert!(
        !out.contains("unknown standard library module"),
        "std::t27f must be a known module, got:\n{}",
        out
    );
}

// ===========================================================================
// K5 — Ok(v)/Err(e)/Unknown(m) match bindings are defined with payload types
// ===========================================================================

#[test]
fn k5_result_match_bindings_are_typed() {
    let out = assert_checks(
        "k5_result.mt",
        r#"
use std::io;
fn get(n: int) -> Result<int, str> {
    if n > 0 { return Ok(n); }
    Err("neg")
}
fn main() {
    let x: int = match get(3) {
        Ok(v) => { v + 1 }
        Unknown(m) => { io::println(m); 0 }
        Err(e) => { io::println(e); 0 }
    };
    io::println_int(x);
}
"#,
    );
    assert!(
        !out.contains("unknown identifier"),
        "match bindings must be defined, got:\n{}",
        out
    );
}

#[test]
fn k5_binding_payload_type_is_enforced() {
    // `v` is bound as int from Result<int, str>: using it as a string
    // argument to a fully-typed user function must fail.
    assert_check_error(
        "k5_typed.mt",
        r#"
fn takes_str(s: str) -> int { 1 }
fn get(n: int) -> Result<int, str> { Ok(n) }
fn main() {
    let x = match get(3) {
        Ok(v) => { takes_str(v) }
        Unknown(m) => { 0 }
        Err(e) => { 0 }
    };
}
"#,
        "argument 1 to 'takes_str'",
    );
}

#[test]
fn k5_user_enum_payload_bindings_are_typed() {
    assert_checks(
        "k5_enum.mt",
        r#"
use std::io;
enum Shape {
    Circle(float),
    Rectangle(int, int),
}
fn area(s: Shape) -> int {
    match s {
        Circle(r) => { 3 }
        Rectangle(w, h) => { w * h }
    }
}
fn main() { io::println_int(area(Shape::Rectangle(2, 3))); }
"#,
    );
}
