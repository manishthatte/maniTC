//! P53 and P54 — a bare function name evaluated to its RETURN type.
//!
//! © Manish Jagdish Thatte
//!
//! Two findings recorded separately, six costumes, one mechanism. The `Ident`
//! arm of `check_expr` answered a function name with the function's RETURN
//! type unless the context supplied a `fn`-typed hint; its own comment called
//! that "the legacy view".
//!
//! **P53** — `let f = dbl;` has no hint, so `f` was typed `int`, and the
//! lowerer emitted a call to the BINDING's name: `Undefined label: f` on T3 and
//! `use of undefined value '@f'` on LLVM, from a program `manitc check`
//! accepted. `let f: fn(int) -> int = dbl;` worked because **the annotation IS
//! the hint**, which is exactly why annotating was a complete workaround. A
//! lambda worked by COINCIDENCE — it is emitted under the name it is bound to,
//! so `@f` happened to resolve — and the pair below states that as an
//! experiment: `let dbl2 = dbl;` failed while `let dbl = dbl;` ran.
//!
//! **P54** — the CALLEE of `pick()` is checked with no hint, so `pick` was
//! typed as its return type `fn(int) -> int`, and the call checker's first arm
//! read THAT type's parameters as `pick`'s own: "function 'pick' expects 1
//! argument(s), found 0", where the `int` belongs to the return type. With a
//! zero-argument return type there was nothing to absorb, so arity passed and
//! the RESULT was mistyped instead — `fn pick() -> fn() -> int` checks clean
//! and dies in the assembler. That half is not in the report.
//!
//! A function whose return type is not itself a function is unaffected either
//! way, which is why this survived: the call checker's first arm only matches
//! when the answer is already a `ManiType::Fn`.
//!
//! **The fix moved a defect one layer down before it fixed anything**, and that
//! is the thing worth keeping. Teaching a function name to carry its function
//! type left every reader of that type newly incomplete: `lower_expr`'s
//! `is_indirect` inferred "this callee is a variable" from the type being
//! `ManiType::Fn`, which was only ever true for variables. Every direct call
//! became an indirect one, silently dropping the flat-array parameter
//! expansion — `fn pack(a: [trit])` is emitted as `@pack(ptr, i64)` and the
//! call site passed one argument — and eight existing rows went red. It now
//! asks whether the name is a LOCAL, which is the question it meant.
//!
//! Every row asserts a VALUE on both backends (permanent rule 8).

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

mod common;

static N: AtomicUsize = AtomicUsize::new(0);

fn manitc_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_manitc"))
}

fn workdir() -> PathBuf {
    let slot = N.fetch_add(1, Ordering::Relaxed);
    let d = common::suite_root("p53")
        .join(slot.to_string());
    std::fs::create_dir_all(&d).expect("temp dir");
    d
}

/// Compile and run on T3. Panics with the compiler's own output on failure.
fn run_t3(src: &str) -> String {
    let d = workdir();
    let path = d.join("p.mt");
    std::fs::write(&path, src).expect("write");
    let base = path.with_extension("");

    let c = Command::new(manitc_bin())
        .args(["compile", path.to_str().unwrap(), "--target", "t3",
               "-o", base.to_str().unwrap()])
        .output()
        .expect("compile");
    assert!(c.status.success(), "T3 compile failed:\n{}\n{}",
            String::from_utf8_lossy(&c.stdout), String::from_utf8_lossy(&c.stderr));

    let r = Command::new(manitc_bin())
        .args(["run-t3", base.with_extension("t3b").to_str().unwrap()])
        .output()
        .expect("run");
    String::from_utf8_lossy(&r.stdout)
        .lines()
        .filter(|l| !l.starts_with("[T3ISA]"))
        .map(|l| format!("{}\n", l))
        .collect()
}

/// Compile and run on LLVM. Returns `None` when clang is absent, so the row
/// degrades to T3-only rather than failing for an environment reason --
/// `conformance_tests.rs` documents why a clang MENTION is not a clang
/// ABSENCE, so this checks for the compiler's own "not found" line.
fn run_llvm(src: &str) -> Option<String> {
    let d = workdir();
    let path = d.join("p.mt");
    std::fs::write(&path, src).expect("write");
    let bin = d.join("p.bin");

    let c = Command::new(manitc_bin())
        .args(["compile", path.to_str().unwrap(), "--target", "llvm",
               "-o", bin.to_str().unwrap()])
        .output()
        .expect("compile");
    let msg = format!("{}{}", String::from_utf8_lossy(&c.stdout),
                      String::from_utf8_lossy(&c.stderr));
    if msg.contains("clang not found") {
        return None;
    }
    assert!(c.status.success(), "LLVM compile failed:\n{}", msg);

    let r = Command::new(&bin).output().expect("run llvm binary");
    Some(String::from_utf8_lossy(&r.stdout).into_owned())
}

/// Assert both backends agree on `want`.
fn both(src: &str, want: &str, what: &str) {
    let t3 = run_t3(src);
    assert!(t3.contains(want), "{what}: T3 gave {t3:?}, wanted {want:?}");
    if let Some(ll) = run_llvm(src) {
        assert!(ll.contains(want), "{what}: LLVM gave {ll:?}, wanted {want:?}");
    }
}


// ---------------------------------------------------------------------------
// P53 — binding an existing function
// ---------------------------------------------------------------------------

const DBL: &str = "fn dbl(x: int) -> int { return x * 2; }\n";

#[test]
fn p53_a_function_bound_without_an_annotation_is_callable() {
    // The reported case. Before: `Undefined label: f` on T3 and
    // `use of undefined value '@f'` on LLVM, from a program check accepted.
    both(&format!("{DBL}fn main() {{ let f = dbl; io::println_int(f(5)); }}"),
         "10", "function bound without an annotation");
}

#[test]
fn p53_the_annotated_form_still_works() {
    // The documented workaround, which must keep working: it is what the
    // language reference tells a reader to write, and it is the form the whole
    // higher-order surface already used.
    both(&format!("{DBL}fn main() {{ let f: fn(int) -> int = dbl; io::println_int(f(5)); }}"),
         "10", "function bound with an annotation");
}

#[test]
fn p53_the_coincidence_is_stated_as_an_experiment() {
    // WHY it survived, as a pair of programs one variable apart. A lambda is
    // emitted under the name it is bound to, so `@f` resolved by coincidence
    // rather than by design, and every lambda in the corpus is bound exactly
    // once at its definition. Bind an EXISTING function to a DIFFERENT name and
    // the coincidence breaks; bind it to its own name and it holds.
    //
    // On the pre-fix compiler the first fails and the second runs. Both must
    // now run, and a fix that only made the first work would leave the second
    // as the accident it was.
    both(&format!("{DBL}fn main() {{ let dbl2 = dbl; io::println_int(dbl2(5)); }}"),
         "10", "bound to a different name");
    both(&format!("{DBL}fn main() {{ let dbl = dbl; io::println_int(dbl(5)); }}"),
         "10", "bound to its own name");
}

#[test]
fn p53_an_unannotated_binding_can_be_passed_on() {
    // Not in the report, which recorded only the CALL. The binding was typed
    // `int`, so passing it failed in `check` with an argument-type error —
    // a different diagnostic from a different pass, same cause.
    both(&format!("{DBL}\
fn apply(g: fn(int) -> int, v: int) -> int {{ return g(v); }}
fn main() {{ let f = dbl; io::println_int(apply(f, 5)); }}"),
         "10", "unannotated binding passed as an argument");
}

#[test]
fn p53_a_function_may_be_a_tuple_element() {
    // Not in the report. A tuple literal's element got no fn-typed hint, so
    // `(dbl, 1)` against `(fn(int) -> int, int)` was a type mismatch.
    both(&format!("{DBL}\
fn main() {{ let t: (fn(int) -> int, int) = (dbl, 1); io::println_int((t.0)(5)); }}"),
         "10", "function as a tuple element");
}

#[test]
fn p53_a_function_may_be_a_struct_field() {
    // A GUARD row: this worked on the pre-fix compiler, because a struct
    // literal's field DOES carry the declared type as a hint. It is here to
    // pin the boundary — the hint mechanism was never wrong, it was only ever
    // absent in the positions above.
    both(&format!("{DBL}\
struct Ops {{ pub f: fn(int) -> int, pub n: int }}
fn main() {{ let o = Ops {{ f: dbl, n: 1 }}; io::println_int((o.f)(5)); }}"),
         "10", "function as a struct field");
}

// ---------------------------------------------------------------------------
// P54 — a function type in return position
// ---------------------------------------------------------------------------

#[test]
fn p54_a_function_may_be_returned() {
    // Before: "function 'pick' expects 1 argument(s), found 0" — and `pick`
    // takes none. The `int` it was said to expect is the parameter of its
    // RETURN type.
    both(&format!("{DBL}\
fn pick() -> fn(int) -> int {{ return dbl; }}
fn main() {{ let g = pick(); io::println_int(g(5)); }}"),
         "10", "function returned from a function");
}

#[test]
fn p54_a_two_parameter_function_may_be_returned() {
    // Two absorbed parameters rather than one, so the arity error moved with
    // the return type's arity — which is what identifies WHOSE parameters they
    // were.
    both("fn add(a: int, b: int) -> int { return a + b; }
fn pick() -> fn(int, int) -> int { return add; }
fn main() { let g = pick(); io::println_int(g(2, 3)); }",
         "5", "two-parameter function returned");
}

#[test]
fn p54_a_zero_parameter_return_type_is_the_half_not_reported() {
    // THE HALF THE REPORT DOES NOT HAVE. With nothing to absorb, arity passed
    // and `manitc check` exited 0 — so this looked fixed from the checker's
    // side. What was wrong instead was the RESULT type: `g` came out `int`,
    // and the program died in the assembler. A row that asserted only the exit
    // status of `check` would have called this working.
    both("fn one() -> int { return 1; }
fn pick() -> fn() -> int { return one; }
fn main() { let g = pick(); io::println_int(g()); }",
         "1", "zero-parameter function returned");
}

// ---------------------------------------------------------------------------
// What the fix nearly broke — the readers of the type it changed
// ---------------------------------------------------------------------------

#[test]
fn p53_a_flat_unsized_array_parameter_still_reaches_its_callee() {
    // `fn pack(a: [trit])` is emitted as `@pack(ptr, i64)`: an unsized array
    // parameter travels flat, as a pointer and a length. `lower_expr` decided
    // direct-versus-indirect by testing whether the callee's TYPE was
    // `ManiType::Fn`, which was true only for variables while a function name
    // typed as its return type. Making the name carry its function type turned
    // every direct call indirect and dropped the length argument, so the call
    // site passed one argument to a two-parameter definition.
    //
    // Eight existing rows caught it, and this one states the property directly
    // so the next change to `is_indirect` fails HERE rather than in a struct
    // test that reads as unrelated. It is therefore GREEN on the pre-fix
    // compiler — it guards against a regression this work could have shipped,
    // not against the defect it fixes.
    both("fn pack(a: [trit]) -> int { return ternary::pack_trits(a); }
fn main() {
    let z: [trit; 0] = [];
    io::println(fmt::format(\"empty={}\", [fmt::show_int(pack(z))]));
}",
         "empty=0", "flat unsized array parameter survives a direct call");
}

#[test]
fn p53_a_function_typed_parameter_is_still_called_indirectly() {
    // The other side of the same decision: a PARAMETER of function type is a
    // local, so it must stay an indirect call. This is the whole higher-order
    // stdlib surface (`Vec::map`, `Vec::filter`, `Vec::fold`), which is why it
    // was unaffected by P53 and must remain so.
    both(&format!("{DBL}\
fn apply(g: fn(int) -> int, v: int) -> int {{ return g(v); }}
fn main() {{ io::println_int(apply(dbl, 5)); }}"),
         "10", "function-typed parameter called indirectly");
}

#[test]
fn p53_a_lambda_is_still_callable_through_its_binding() {
    // A lambda bound to a name is a local holding a hoisted function, so it is
    // an indirect call too. It worked before the fix by the coincidence this
    // suite records above, and it must keep working for the ordinary reason.
    both("fn main() { let f = fn(x: int) => x * 2; io::println_int(f(5)); }",
         "10", "lambda called through its binding");
}

/// A GUARD row: green on the pre-fix compiler, because the annotated form was
/// never broken. It is here for the VALUE it asserts — 15 rather than 10 — so a
/// future change that resolved a call from the binding's NAME again would be
/// caught by an answer rather than by a link failure.
#[test]
fn p53_a_reassigned_function_variable_calls_the_new_target() {
    // Asserts the VALUE and not merely that it runs: with the binding's NAME
    // driving the emitted symbol, a reassignment could not be observed at all.
    // 15 rather than 10 is the whole point.
    both("fn dbl(x: int) -> int { return x * 2; }
fn trp(x: int) -> int { return x * 3; }
fn main() { let mut f: fn(int) -> int = dbl; f = trp; io::println_int(f(5)); }",
         "15", "reassigned function variable");
}
