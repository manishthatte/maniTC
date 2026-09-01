//! P94 — an array that outlives the frame that built it.
//!
//! © Manish Jagdish Thatte
//!
//! The T3 backend put every array alloca in the caller's frame and applied no
//! escape analysis, so returning one — bare, inside a struct, inside a tuple,
//! from a method, through a function pointer — handed back an address the next
//! `CALL` overwrote. `fn mkarr() -> [int]` printed
//! `-1 -2 -3 -4 -5 -6 -7 59990 8` where LLVM printed `-1 … -9`, and the number
//! of corrupt elements was the NEXT callee's frame size counted from the end,
//! independent of the array's length.
//!
//! **The mechanism is not a missing analysis, it is an existing repair that one
//! spelling of the type cannot reach.** `lower::lower_array_call` has always
//! copied an array-returning call's result into a caller-owned buffer, giving
//! arrays value semantics. It is reached when the return type is a SIZED
//! `IRType::Array`. An unsized `[T]` becomes `IRType::Ptr` in
//! `IRType::from_mani` — not an array at all — so it reaches neither that copy
//! nor any other array-shaped reasoning, and its caller gets the raw pointer.
//! The caller cannot copy what it has no length for, so for that spelling the
//! storage must genuinely outlive the frame: `regalloc::escaping_allocas`
//! decides which allocas those are and they go to the heap.
//!
//! **Every row asserts a VALUE, on both backends** (permanent rule 8). Asserting
//! that the program runs would pass over the defect: on T3 it ran and printed
//! plausible wrong numbers. The LLVM half is the oracle — it has mallocked
//! array allocas throughout, so it was never wrong here, which is exactly what
//! makes it the reference for what the answer should be.
//!
//! The pairing with the lowerer is deliberate and is pinned rather than
//! described (permanent rule 5): `escaping_allocas` is sound only while every
//! sized-array return is copied by its caller, and
//! `p94_value_semantics_hold_through_a_fn_pointer` is the row that fails if
//! either half moves.

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static N: AtomicUsize = AtomicUsize::new(0);

fn manitc_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_manitc"))
}

fn workdir() -> PathBuf {
    let slot = N.fetch_add(1, Ordering::Relaxed);
    let d = std::env::temp_dir()
        .join(format!("manitc_p94_{}", std::process::id()))
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
// A driver whose frame is guaranteed to cover the corpse.
//
// `deep` recurses with a local array, so the popped region below the returned
// address is written over rather than left intact by luck. Several of these
// rows pass on the BROKEN compiler without it, which is the whole reason it is
// here: an escaping pointer that nothing has overwritten yet reads correctly.
// ---------------------------------------------------------------------------
const DEEP: &str = "
fn deep(n: int) -> int {
    if n <= 0 { return 0; }
    let b: [int; 9] = [n,n,n,n,n,n,n,n,n];
    return b[0] + deep(n - 1);
}
";

fn show9() -> &'static str {
    "
fn show9(a: [int], label: str) {
    io::print(label); io::print(\": \");
    let mut i: int = 0;
    while i < 9 { io::print_int(a[i]); io::print(\" \"); i = i + 1; }
    io::println(\"\");
}
"
}

// ---------------------------------------------------------------------------
// The five costumes of an escaping array
// ---------------------------------------------------------------------------

#[test]
fn p94_an_array_returned_bare_survives_the_return() {
    // The reported case. On the pre-fix compiler the tail reads as the next
    // callee's frame: `-1 -2 -3 -4 -5 -6 -7 59990 8`.
    both(&format!("{}{}
fn mkarr() -> [int] {{
    let a: [int] = [-1,-2,-3,-4,-5,-6,-7,-8,-9];
    return a;
}}
fn main() {{
    let b: [int] = mkarr();
    deep(7);
    show9(b, \"bare\");
}}
", DEEP, show9()),
    "bare: -1 -2 -3 -4 -5 -6 -7 -8 -9", "array returned bare");
}

#[test]
fn p94_an_array_in_a_returned_struct_survives() {
    // The field is UNSIZED. A SIZED field is safe even on the pre-fix
    // compiler, because `lower_struct_call` deep-copies sized array fields at
    // the call boundary — see the row below, which pins that the two spellings
    // are genuinely different and that only one of them was broken.
    both(&format!("{}{}
struct Holder {{ pub a: [int], pub tag: int }}
fn mk() -> Holder {{
    let a: [int] = [11,12,13,14,15,16,17,18,19];
    return Holder {{ a: a, tag: 7 }};
}}
fn main() {{
    let h: Holder = mk();
    deep(7);
    show9(h.a, \"struct\");
}}
", DEEP, show9()),
    "struct: 11 12 13 14 15 16 17 18 19", "array in a returned struct");
}

#[test]
fn p94_an_array_in_a_returned_tuple_survives() {
    both(&format!("{}{}
fn mk() -> ([int], int) {{
    let a: [int] = [21,22,23,24,25,26,27,28,29];
    return (a, 3);
}}
fn main() {{
    let t: ([int], int) = mk();
    deep(7);
    show9(t.0, \"tuple\");
}}
", DEEP, show9()),
    "tuple: 21 22 23 24 25 26 27 28 29", "array in a returned tuple");
}

#[test]
fn p94_an_array_returned_from_a_method_survives() {
    // Not in the original report, which was written from the free-function
    // costume. A write-up inherits the incidental features of the site where
    // the finding was seen (P71).
    both(&format!("{}{}
struct Maker {{ pub seed: int }}
impl Maker {{
    fn build(self) -> [int] {{
        let a: [int] = [61,62,63,64,65,66,67,68,69];
        return a;
    }}
}}
fn main() {{
    let m = Maker {{ seed: 1 }};
    let v: [int] = m.build();
    deep(7);
    show9(v, \"method\");
}}
", DEEP, show9()),
    "method: 61 62 63 64 65 66 67 68 69", "array returned from a method");
}

#[test]
fn p94_a_nested_array_of_arrays_survives() {
    // The inner arrays escape by being STORED into the outer one, which is
    // itself returned. Both pointers in the outer array were clobbered to the
    // same dead address, so the two rows printed identical junk.
    both(&format!("{}
fn mk() -> [[int]] {{
    let r0: [int] = [31,32,33];
    let r1: [int] = [41,42,43];
    let g: [[int]] = [r0, r1];
    return g;
}}
fn main() {{
    let g: [[int]] = mk();
    deep(12);
    let r0: [int] = g[0];
    let r1: [int] = g[1];
    io::print(\"nested: \");
    io::print_int(r0[0]); io::print(\" \"); io::print_int(r0[1]); io::print(\" \"); io::print_int(r0[2]);
    io::print(\" | \");
    io::print_int(r1[0]); io::print(\" \"); io::print_int(r1[1]); io::print(\" \"); io::print_int(r1[2]);
    io::println(\"\");
}}
", DEEP),
    "nested: 31 32 33 | 41 42 43", "array of arrays");
}

// ---------------------------------------------------------------------------
// The third sink: a CALL ARGUMENT
// ---------------------------------------------------------------------------

#[test]
fn p94_an_array_passed_to_a_callee_that_keeps_it_survives() {
    // `caller` never RETURNS the array and never STORES it — it only passes
    // it. The callee is what puts it in a heap cell that outlives the caller's
    // frame. An escape analysis with only the Return and Store sinks reports
    // this array as frame-safe and it prints `71 12 12 12 …`, so this row is
    // what makes the call-argument sink load-bearing rather than merely
    // conservative.
    both(&format!("{}
struct Keeper {{ pub a: [int], pub tag: int }}
fn keep(a: [int]) -> Keeper {{ return Keeper {{ a: a, tag: 1 }}; }}
fn caller() -> Keeper {{
    let a: [int] = [71,72,73,74,75,76,77,78,79];
    return keep(a);
}}
fn main() {{
    let k: Keeper = caller();
    deep(12);
    io::print(\"callesc: \");
    let mut i: int = 0;
    while i < 9 {{ io::print_int(k.a[i]); io::print(\" \"); i = i + 1; }}
    io::println(\"\");
}}
", DEEP),
    "callesc: 71 72 73 74 75 76 77 78 79", "array kept by a callee");
}

// ---------------------------------------------------------------------------
// The lowerer's half, and the coupling it creates
// ---------------------------------------------------------------------------

#[test]
fn p94_value_semantics_hold_through_a_fn_pointer() {
    // `escaping_allocas` treats a SIZED array return as non-escaping BECAUSE
    // the caller copies it (`lower_array_call`). The indirect path emitted a
    // bare `CallIndirect` and did no copy, so the premise was false for it:
    // this printed `8 8 8 8 8 8`, which is literally `deep(8)`'s locals read
    // through a dangling pointer.
    //
    // THIS ROW IS THE PIN BETWEEN THE TWO HALVES. If the copy is ever removed
    // from either call path while the register allocator still assumes it, the
    // sized case silently returns frame garbage again and this is what says so.
    both(&format!("{}
fn mk() -> [int; 6] {{ let a: [int; 6] = [81,82,83,84,85,86]; return a; }}
fn main() {{
    let f: fn() -> [int; 6] = mk;
    let v: [int; 6] = f();
    deep(8);
    io::print(\"via-fnptr: \");
    let mut i: int = 0;
    while i < 6 {{ io::print_int(v[i]); io::print(\" \"); i = i + 1; }}
    io::println(\"\");
}}
", DEEP),
    "via-fnptr: 81 82 83 84 85 86", "sized array through a fn pointer");
}

#[test]
fn p94_a_sized_array_field_was_already_safe_and_still_is() {
    // Stated rather than left implicit: this row passes on the compiler
    // WITHOUT the fix, and it is here to pin the boundary the fix is drawn
    // along. `lower_struct_call` deep-copies SIZED array fields at the call
    // boundary, which is why `[int; 6]` survived while the `[int]` beside it
    // did not. A row that passes on the control is not evidence for the fix
    // (permanent rule 9) — it is evidence about where the defect stopped.
    both(&format!("{}
struct SBox {{ pub a: [int; 6], pub tag: int }}
fn mk() -> SBox {{ let a: [int; 6] = [91,92,93,94,95,96]; return SBox {{ a: a, tag: 1 }}; }}
fn main() {{
    let s: SBox = mk();
    deep(7);
    io::print(\"sized-field: \");
    let mut i: int = 0;
    while i < 6 {{ io::print_int(s.a[i]); io::print(\" \"); i = i + 1; }}
    io::println(\"\");
}}
", DEEP),
    "sized-field: 91 92 93 94 95 96", "sized array field");
}

// ---------------------------------------------------------------------------
// The cost, pinned in the direction that can regress
// ---------------------------------------------------------------------------

#[test]
fn p94_a_non_escaping_array_stays_in_the_frame() {
    // The heap here is 2,536 words and the allocator has no free, so promoting
    // every array alloca — which is correct, and is what the LLVM backend
    // does — is not affordable: this program allocates 8 words on each of 400
    // calls and dies with `TRAP: heap exhausted`. Measured, not reasoned; it
    // is why the storage class is decided by escape and not by type.
    //
    // The row asserts the ANSWER rather than the absence of a trap, so it
    // fails the same way whether the array is misplaced or the arithmetic is.
    both("
fn work(n: int) -> int {
    let a: [int] = [n, n+1, n+2, n+3, n+4, n+5, n+6, n+7];
    return a[0] + a[7];
}
fn main() {
    let mut i: int = 0;
    let mut acc: int = 0;
    while i < 400 { acc = acc + work(i); i = i + 1; }
    io::print(\"acc=\"); io::println_int(acc);
}
", "acc=162400", "non-escaping array stays in the frame");
}
