//! Phase 1 — "close the boundary". A1, A5, B1 and F-8.
//!
//! Author: Manish Jagdish Thatte
//!
//! Each test pins one behaviour the phase introduced, plus — as importantly —
//! the behaviours it deliberately did NOT change. The second kind is not
//! padding: L1 is defined as "generations pass `manitc check`", so a change to
//! what the checker accepts invalidates every earlier measurement. The tests
//! named `..._is_unchanged` are the record of that promise.

use std::path::PathBuf;
use std::process::Command;

fn manitc() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_manitc"))
}

fn temp_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("manitc_phase1_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("failed to create temp dir");
    dir
}

fn write_source(name: &str, source: &str) -> PathBuf {
    let path = temp_dir().join(name);
    std::fs::write(&path, source).expect("failed to write test source");
    path
}

/// Run `manitc check <file> <extra...>`; returns (ok, stdout+stderr).
fn check(name: &str, source: &str, extra: &[&str]) -> (bool, String) {
    let path = write_source(name, source);
    let mut args: Vec<String> = vec!["check".into(), path.display().to_string()];
    args.extend(extra.iter().map(|s| s.to_string()));
    let out = Command::new(manitc())
        .args(&args)
        .output()
        .expect("failed to run manitc");
    let mut blob = String::from_utf8_lossy(&out.stdout).to_string();
    blob.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.success(), blob)
}

// ---------------------------------------------------------------------------
// A1 — extern declarations
// ---------------------------------------------------------------------------

#[test]
fn a1_an_extern_declaration_parses_with_both_clauses() {
    let (ok, out) = check(
        "a1_parse.mt",
        r#"
extern "c" fn io::println(s: str) -> void
    available(llvm, t3);
fn main() { io::println("hi"); }
"#,
        &[],
    );
    assert!(ok, "a declaration with a signature and available() must check:\n{}", out);
}

#[test]
fn a1_step2_a_declared_extern_checks_its_arguments() {
    // Section 53 exactly: `io::println_int(5 > 0)` was a silent coercion that
    // printed -1 on LLVM and 1 on T3. With a declaration it is a type error at
    // the call site, with a span.
    let (ok, out) = check(
        "a1_enforce.mt",
        r#"
extern "c" fn io::println_int(n: int) -> void available(llvm, t3);
fn main() { io::println_int(5 > 0); }
"#,
        &[],
    );
    assert!(!ok, "a bool argument to a declared int parameter must be rejected:\n{}", out);
    assert!(
        out.contains("expected `int`") && out.contains("found `bool`"),
        "the diagnostic must name both types:\n{}",
        out
    );
}

#[test]
fn a1_an_undeclared_native_is_unchanged() {
    // The SAME call, with no declaration, still passes — A1 step 2 narrows the
    // asymmetry only where a declaration exists. Anything else would change
    // what `manitc check` accepts for every program in the corpus.
    let (ok, out) = check(
        "a1_unchanged.mt",
        "fn main() { io::println_int(5 > 0); }\n",
        &[],
    );
    assert!(ok, "an undeclared native must behave exactly as before:\n{}", out);
}

#[test]
fn a1_the_migration_backlog_is_generated_on_demand_only() {
    let src = "fn main() { io::println(\"a\"); io::println_int(1); }\n";

    let (ok, quiet) = check("a1_backlog_off.mt", src, &[]);
    assert!(ok, "{}", quiet);
    assert!(
        !quiet.contains("migration backlog"),
        "the backlog must be silent by default — it is 413 natives:\n{}",
        quiet
    );

    let (ok, loud) = check("a1_backlog_on.mt", src, &["--warn", "undeclared-native"]);
    assert!(ok, "asking for the backlog must not fail the check:\n{}", loud);
    assert!(loud.contains("io::println"), "backlog must list the natives:\n{}", loud);
    assert!(loud.contains("migration backlog"), "{}", loud);
}

#[test]
fn a1_a_native_is_declared_once() {
    let (ok, out) = check(
        "a1_dup.mt",
        r#"
extern "c" fn io::println(s: str) -> void available(llvm);
extern "c" fn io::println(s: str) -> void available(t3);
fn main() { io::println("x"); }
"#,
        &[],
    );
    assert!(!ok, "a second declaration of one native must be refused:\n{}", out);
    assert!(out.contains("already declared"), "{}", out);
}

#[test]
fn a1_an_unknown_backend_in_available_is_refused() {
    // A typo in `available(llmv)` would make step 3 refuse the call on every
    // backend, for a reason the source does not show.
    let (ok, out) = check(
        "a1_badbackend.mt",
        r#"
extern "c" fn io::println(s: str) -> void available(llmv);
fn main() { io::println("x"); }
"#,
        &[],
    );
    assert!(!ok, "an unknown backend name must be refused:\n{}", out);
    assert!(out.contains("unknown backend"), "{}", out);
}

#[test]
fn a1_deprecated_warns_at_the_call_site() {
    let (ok, out) = check(
        "a1_dep.mt",
        r#"
extern "c" fn io::println(s: str) -> void
    available(llvm, t3) deprecated("use fmt::print");
fn main() { io::println("x"); }
"#,
        &[],
    );
    assert!(ok, "a deprecation is a warning, not an error:\n{}", out);
    assert!(out.contains("is deprecated"), "{}", out);
    assert!(out.contains("use fmt::print"), "the message must be carried through:\n{}", out);
}

#[test]
fn a1_availability_is_reported_against_the_selected_backend() {
    // Section 52's shape, made visible at the CALL SITE with a span instead of
    // an undefined label out of the assembler. Step 3 does not enforce yet —
    // the lint defaults to `allow` — but the diagnostic exists, so the step-3
    // backlog is generated the same way the step-1 one is.
    //
    // A2 later made the SAME program a hard error, by a different route: it
    // infers over the call graph and denies `backend-unavailable-chain`. That
    // is A2's job to test, not this one's — this test pins A1's call-site
    // diagnostic and its `allow` default. So the chain lint is switched off
    // here, to keep the two independently testable. Without that, this test
    // would be asserting A2's default rather than A1's.
    let src = r#"
extern "c" fn gui::set_color(r: int, g: int, b: int) -> void
    available(llvm);
fn main() { gui::set_color(1, 2, 3); }
"#;

    let (ok, agnostic) = check("a1_avail_none.mt", src, &["--warn", "backend-unavailable"]);
    assert!(ok, "{}", agnostic);
    assert!(
        !agnostic.contains("not available"),
        "`check` is backend-agnostic unless asked — reporting here answers a \
         question nobody posed:\n{}",
        agnostic
    );

    let (ok, t3) = check(
        "a1_avail_t3.mt",
        src,
        &["--backend", "t3", "--warn", "backend-unavailable",
          "--allow", "backend-unavailable-chain"],
    );
    assert!(ok, "the lint is allow by default, so this must not fail:\n{}", t3);
    assert!(t3.contains("not available on the t3 backend"), "{}", t3);
    assert!(t3.contains("declared available on: llvm"), "{}", t3);

    let (ok, llvm) = check(
        "a1_avail_llvm.mt",
        src,
        &["--backend", "llvm", "--warn", "backend-unavailable"],
    );
    assert!(ok, "{}", llvm);
    assert!(!llvm.contains("not available"), "available means available:\n{}", llvm);
}

#[test]
fn a1_a_declaration_is_the_authority_on_what_is_native() {
    // `gui` is not in the analyzer's hardcoded STDLIB_MODULES list. Keying the
    // A1 diagnostics on that list meant a declared `gui::` extern was invisible
    // to all of them — the form could be written and then did nothing.
    let (ok, out) = check(
        "a1_authority.mt",
        r#"
extern "c" fn gui::set_color(r: int, g: int, b: int) -> void
    available(llvm, t3) deprecated("use gui::color");
fn main() { gui::set_color(1, 2, 3); }
"#,
        &[],
    );
    assert!(ok, "{}", out);
    assert!(
        out.contains("is deprecated"),
        "a declared extern outside the stdlib namespace must still be seen:\n{}",
        out
    );
}

// ---------------------------------------------------------------------------
// A5 — lint levels
// ---------------------------------------------------------------------------

const UNUSED: &str = "fn main() { let unused: int = 5; io::println(\"x\"); }\n";

#[test]
fn a5_a_lint_can_be_allowed_warned_and_denied() {
    let (ok, warn) = check("a5_warn.mt", UNUSED, &[]);
    assert!(ok, "warn must not fail the check:\n{}", warn);
    assert!(warn.contains("unused variable"), "{}", warn);

    let (ok, allow) = check("a5_allow.mt", UNUSED, &["--allow", "unused-variable"]);
    assert!(ok, "{}", allow);
    assert!(!allow.contains("unused variable"), "allow must silence it:\n{}", allow);

    let (ok, deny) = check("a5_deny.mt", UNUSED, &["--deny", "unused-variable"]);
    assert!(!deny.is_empty());
    assert!(!ok, "deny must fail the check:\n{}", deny);
}

#[test]
fn a5_the_diagnostic_names_its_own_lint() {
    // A diagnostic that does not name the string you have to type into
    // --allow is a diagnostic you cannot act on.
    let (_, out) = check("a5_named.mt", UNUSED, &[]);
    assert!(out.contains("[unused-variable]"), "{}", out);
}

#[test]
fn a5_a_denied_lint_is_reported_once_not_twice() {
    let (_, out) = check("a5_once.mt", UNUSED, &["--deny", "unused-variable"]);
    let n = out.matches("unused variable `unused`").count();
    assert_eq!(n, 1, "the same span must not be printed as warning AND error:\n{}", out);
    assert!(out.contains("aborting: 1 denied lint"), "{}", out);
}

#[test]
fn a5_an_unknown_lint_name_is_an_error_not_a_no_op() {
    let (ok, out) = check("a5_typo.mt", UNUSED, &["--deny", "unusd-variable"]);
    assert!(!ok, "a typo must not be silently ignored:\n{}", out);
    assert!(out.contains("unknown lint"), "{}", out);
    assert!(out.contains("unused-variable"), "it must list the real names:\n{}", out);
}

#[test]
fn a5_a_module_can_set_its_own_level() {
    let (ok, out) = check(
        "a5_item.mt",
        "lint allow(unused-variable);\nfn main() { let unused: int = 5; io::println(\"x\"); }\n",
        &[],
    );
    assert!(ok, "{}", out);
    assert!(!out.contains("unused variable"), "the module's own level must apply:\n{}", out);
}

#[test]
fn a5_forbid_cannot_be_lowered_by_a_module() {
    let (ok, out) = check(
        "a5_forbid.mt",
        "lint allow(unused-variable);\nfn main() { let unused: int = 5; io::println(\"x\"); }\n",
        &["--forbid", "unused-variable"],
    );
    assert!(!ok, "forbid must survive a module trying to lower it:\n{}", out);
}

#[test]
fn a5_the_lint_manifest_is_recorded_in_the_t3_artifact() {
    let src = write_source("a5_manifest.mt", "fn main() { io::println(\"x\"); }\n");
    let out_base = temp_dir().join("a5_manifest_out");
    let st = Command::new(manitc())
        .args([
            "compile",
            "--target",
            "t3",
            src.to_str().unwrap(),
            "-o",
            out_base.to_str().unwrap(),
            "--deny",
            "shadowing",
        ])
        .output()
        .expect("failed to run manitc");
    assert!(st.status.success(), "{}", String::from_utf8_lossy(&st.stderr));

    let sidecar = out_base.with_extension("t3l");
    let text = std::fs::read_to_string(&sidecar)
        .unwrap_or_else(|e| panic!("no lint manifest at {}: {}", sidecar.display(), e));
    assert!(text.starts_with("manitc-lints v1"), "{}", text);
    assert!(text.contains("shadowing=deny"), "the requested level must be recorded:\n{}", text);
    // Every lint, not only the changed one: a manifest that records deltas is
    // unreadable without knowing the defaults of the compiler that wrote it.
    assert!(text.contains("unused-variable="), "{}", text);
    assert!(text.contains("unsatisfied-bound="), "{}", text);
}

#[test]
fn a5_warn_as_error_still_means_all() {
    let (ok, out) = check("a5_wae.mt", UNUSED, &["--warn-as-error"]);
    assert!(!ok, "the flag section 54's strict binary was built with must still work:\n{}", out);
}

// ---------------------------------------------------------------------------
// B1 / A4 — trait bounds
// ---------------------------------------------------------------------------

#[test]
fn b1_a_bound_is_satisfied_by_a_primitive() {
    let (ok, out) = check(
        "b1_ok.mt",
        "fn max2<T: Ord>(a: T, b: T) -> T { if a > b { a } else { b } }\n\
         fn main() { io::println_int(max2(3, 7)); }\n",
        &[],
    );
    assert!(ok, "int must satisfy Ord without an impl:\n{}", out);
}

#[test]
fn b1_closes_a4_an_unordered_type_is_refused() {
    // A4's open question, answered by measurement: before bounds existed this
    // compiled clean, checked clean, and returned the WRONG value on BOTH
    // backends — `max2(P{9}, P{1}).x` gave 1, because the comparison was on the
    // two allocation addresses. Both backends agreed, so the differential
    // oracle was structurally blind to it.
    let (ok, out) = check(
        "b1_a4.mt",
        "struct P { pub x: int }\n\
         fn max2<T: Ord>(a: T, b: T) -> T { if a > b { a } else { b } }\n\
         fn main() {\n\
             let p: P = P { x: 9 };\n\
             let q: P = P { x: 1 };\n\
             io::println_int(max2(p, q).x);\n\
         }\n",
        &[],
    );
    assert!(!ok, "a struct with no ordering must not satisfy `T: Ord`:\n{}", out);
    assert!(out.contains("does not satisfy the bound"), "{}", out);
    assert!(out.contains("impl Ord for P"), "the fix must be named:\n{}", out);
}

#[test]
fn b1_a_where_clause_binds_the_same_as_angle_brackets() {
    let (ok, out) = check(
        "b1_where.mt",
        "struct P { pub x: int }\n\
         fn show_it<T>(v: T) -> int where T: Display { 1 }\n\
         fn main() { let p: P = P { x: 1 }; io::println_int(show_it(p)); }\n",
        &[],
    );
    assert!(!ok, "a where-clause bound must be checked too:\n{}", out);
    assert!(out.contains("T: Display"), "{}", out);
}

#[test]
fn b1_a_user_impl_satisfies_the_bound() {
    let (ok, out) = check(
        "b1_impl.mt",
        "trait Ord { fn cmp(self, other: P) -> int; }\n\
         struct P { pub x: int }\n\
         impl Ord for P { fn cmp(self, other: P) -> int { 0 } }\n\
         fn pick<T: Ord>(a: T, b: T) -> T { a }\n\
         fn main() { let p: P = P { x: 1 }; let q: P = P { x: 2 };\n\
             io::println_int(pick(p, q).x); }\n",
        &[],
    );
    assert!(ok, "an explicit impl must satisfy the bound:\n{}", out);
}

#[test]
fn b1_an_unbounded_generic_is_unchanged() {
    // Bounds are opt-in. A bare `<T>` must still compile exactly as before —
    // the soundness hole is closed by WRITING the bound, not by inferring one,
    // because inferring one would reject programs that check today.
    let (ok, out) = check(
        "b1_unbounded.mt",
        "struct P { pub x: int }\n\
         fn max2<T>(a: T, b: T) -> T { if a > b { a } else { b } }\n\
         fn main() {\n\
             let p: P = P { x: 9 };\n\
             let q: P = P { x: 1 };\n\
             io::println_int(max2(p, q).x);\n\
         }\n",
        &[],
    );
    assert!(ok, "an unbounded generic must behave exactly as before:\n{}", out);
}

#[test]
fn b1_the_bound_lint_can_be_downgraded_for_migration() {
    let (ok, out) = check(
        "b1_allow.mt",
        "struct P { pub x: int }\n\
         fn max2<T: Ord>(a: T, b: T) -> T { if a > b { a } else { b } }\n\
         fn main() { let p: P = P { x: 9 }; let q: P = P { x: 1 };\n\
             io::println_int(max2(p, q).x); }\n",
        &["--allow", "unsatisfied-bound"],
    );
    assert!(ok, "--allow must downgrade it, for a staged migration:\n{}", out);
}

// ---------------------------------------------------------------------------
// F-8 — the depth guard is the library's, not the binary's
// ---------------------------------------------------------------------------

#[test]
fn f8_deep_nesting_is_a_diagnostic_for_a_library_caller_too() {
    // The parser refuses past MAX_PARSE_DEPTH, but that limit is only
    // ENFORCEABLE on a stack deep enough to reach it. The reservation used to
    // live in `main`, so every other embedder — the language server on a tokio
    // worker, this test — got the default stack and a process abort instead of
    // the diagnostic. Found by the F-8 corpus harness.
    let src = format!(
        "fn main() {{ let x: int = {}1{}; }}\n",
        "(".repeat(2048),
        ")".repeat(2048)
    );
    let owned = src.clone();
    let refused = manitc::with_compiler_stack(move || {
        let tokens = manitc::lexer::Lexer::with_file(&owned, "<deep>")
            .tokenize()
            .expect("the lexer has no depth to exceed");
        manitc::parser::Parser::with_file(tokens, "<deep>").parse().is_err()
    });
    assert!(refused, "deep nesting must be refused, not fatal");

    // And the same input through the binary, which reserves its own stack.
    let (ok, out) = check("f8_deep.mt", &src, &[]);
    assert!(!ok, "{}", out);
    assert!(out.contains("nested too deeply"), "{}", out);
}
