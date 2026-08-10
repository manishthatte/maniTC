// Test: struct pointers that outlive the loop iteration that created them.
//
// A struct value is a pointer occupying one slot, so `xs[i] = f(...)` stores a
// pointer into a container that outlives the loop. On T3 struct allocations used
// to come off the stack, and the emitter pops back to the block's canonical
// depth on the back edge, so every iteration's allocation landed on the same
// address. Every element of the array then aliased one buffer and read back the
// last value written — or garbage, once that stack region was reused.
//
// It was invisible for a single call and only appeared once a loop was involved,
// which is why `call once` below passes on the old compiler and `call in loop`
// does not. thatteos scheduler.mt hit it through `pcbs[i] = age_tick(p)`: all
// nine process control blocks reported the same PID.
//
// Struct allocations are now heap-allocated (T3 syscall #218), matching what the
// LLVM backend already did with malloc.
use std::io;

fn pass(label: str) { io::print("PASS "); io::println(label); }
fn check(label: str, cond: bool) { if cond { pass(label) } else { io::print("FAIL "); io::println(label); } }

struct P { pub id: int, pub n: int }

fn bump(p: P) -> P { return P { id: p.id, n: p.n + 100 }; }
fn mk(i: int) -> P { return P { id: i, n: i * 10 }; }

// ---------------------------------------------------------------------------
// The original failure: a struct-returning call whose result is stored into an
// array element, inside a loop.
// ---------------------------------------------------------------------------
fn test_call_result_into_array() {
    let mut d: [P] = [P{id:1,n:10}, P{id:2,n:20}, P{id:3,n:30}];
    let mut j = 0;
    while j < 3 {
        d[j] = bump(d[j]);
        j = j + 1;
    }
    check("ids survive the loop", d[0].id == 1 && d[1].id == 2 && d[2].id == 3);
    check("values survive the loop", d[0].n == 110 && d[1].n == 120 && d[2].n == 130);
    check("elements do not alias", d[0].n != d[1].n && d[1].n != d[2].n);
}

// ---------------------------------------------------------------------------
// The same escape without a prior element read: every slot is freshly built.
// ---------------------------------------------------------------------------
fn test_fresh_structs_into_array() {
    let mut xs: [P] = [P{id:0,n:0}, P{id:0,n:0}, P{id:0,n:0}, P{id:0,n:0}];
    let mut i = 0;
    while i < 4 {
        xs[i] = mk(i + 1);
        i = i + 1;
    }
    let mut ok = true;
    let mut k = 0;
    while k < 4 {
        if xs[k].id != k + 1 { ok = false; }
        if xs[k].n != (k + 1) * 10 { ok = false; }
        k = k + 1;
    }
    check("each iteration allocates its own struct", ok);
}

// ---------------------------------------------------------------------------
// A struct literal built directly in the loop body, no call involved.
// ---------------------------------------------------------------------------
fn test_literal_into_array() {
    let mut xs: [P] = [P{id:0,n:0}, P{id:0,n:0}, P{id:0,n:0}];
    let mut i = 0;
    while i < 3 {
        xs[i] = P { id: i, n: i + 7 };
        i = i + 1;
    }
    check("loop-built literals stay distinct",
          xs[0].n == 7 && xs[1].n == 8 && xs[2].n == 9);
}

// ---------------------------------------------------------------------------
// Nested loops: the inner allocation must not disturb the outer one.
// ---------------------------------------------------------------------------
fn test_nested_loop_allocation() {
    let mut outer: [P] = [P{id:0,n:0}, P{id:0,n:0}, P{id:0,n:0}];
    let mut i = 0;
    while i < 3 {
        let held = mk(i + 1);
        let mut j = 0;
        let mut sum = 0;
        while j < 3 {
            let inner = mk(j + 1);
            sum = sum + inner.n;
            j = j + 1;
        }
        // `held` must still be intact after three inner allocations.
        outer[i] = P { id: held.id, n: sum };
        i = i + 1;
    }
    check("outer struct survives inner allocations",
          outer[0].id == 1 && outer[1].id == 2 && outer[2].id == 3);
    check("inner sums are correct",
          outer[0].n == 60 && outer[1].n == 60 && outer[2].n == 60);
}

// ---------------------------------------------------------------------------
// A struct that does NOT escape still works — this is the case the stack
// allocation handled correctly, and it must not regress.
// ---------------------------------------------------------------------------
fn test_non_escaping_struct() {
    let mut acc = 0;
    let mut i = 0;
    while i < 50 {
        let p = mk(i);
        acc = acc + p.id;
        i = i + 1;
    }
    check("non-escaping struct in a long loop", acc == 1225);
}

fn main() {
    io::println("-- call result into array --");
    test_call_result_into_array();

    io::println("-- fresh structs into array --");
    test_fresh_structs_into_array();

    io::println("-- literal into array --");
    test_literal_into_array();

    io::println("-- nested loop allocation --");
    test_nested_loop_allocation();

    io::println("-- non-escaping struct --");
    test_non_escaping_struct();

    io::println("all struct escape tests done");
}
