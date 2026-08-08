// Test: type casting — `as` operator, int↔float, trit↔int, bool↔int, tryte/t9/t27
use std::io;
use std::math;

fn pass(label: str) { io::print("PASS "); io::println(label); }
fn fail(label: str) { io::print("FAIL "); io::println(label); }
fn check(label: str, cond: bool) { if cond { pass(label) } else { fail(label) } }
fn check_int(label: str, got: int, want: int) {
    if got == want { pass(label); }
    else {
        io::print("FAIL "); io::print(label);
        io::print(" got="); io::print_int(got);
        io::print(" want="); io::print_int(want);
        io::newline();
    }
}

// ---------------------------------------------------------------------------
// int → float (as float) — use math functions to verify
// ---------------------------------------------------------------------------

fn test_int_to_float() {
    let n: int = 5;
    let f: float = n as float;
    // Verify via float arithmetic: 5.0 * 2.0 = 10.0
    let doubled = f * 2.0;
    let back: int = doubled as int;
    check_int("cast: int→float→*2→int: 5*2=10", back, 10);

    let zero: int = 0;
    let fz: float = zero as float;
    let bz: int = fz as int;
    check_int("cast: 0 as float as int = 0", bz, 0);

    let neg: int = -7;
    let fn_: float = neg as float;
    let bn: int = fn_ as int;
    check_int("cast: -7 as float as int = -7", bn, -7);
}

// ---------------------------------------------------------------------------
// float → int truncation
// ---------------------------------------------------------------------------

fn test_float_to_int() {
    let f1: float = 3.9;
    let i1: int = f1 as int;
    check_int("cast: 3.9→int = 3 (truncate)", i1, 3);

    let f2: float = -2.7;
    let i2: int = f2 as int;
    check_int("cast: -2.7→int = -2 (truncate toward zero)", i2, -2);

    let f3: float = 1.0;
    let i3: int = f3 as int;
    check_int("cast: 1.0→int = 1", i3, 1);

    let f4: float = 0.0;
    let i4: int = f4 as int;
    check_int("cast: 0.0→int = 0", i4, 0);
}

// ---------------------------------------------------------------------------
// trit ↔ int
// ---------------------------------------------------------------------------

fn test_trit_to_int() {
    let p: trit = +;
    let z: trit = 0;
    let n: trit = -;

    let pi: int = p as int;
    let zi: int = z as int;
    let ni: int = n as int;

    check_int("cast: + as int = 1",  pi,  1);
    check_int("cast: 0 as int = 0",  zi,  0);
    check_int("cast: - as int = -1", ni, -1);
}

fn test_int_to_trit() {
    // int 1, 0, -1 → trit +, 0, -
    let p: trit = 1 as trit;
    let z: trit = 0 as trit;
    let n: trit = -1 as trit;

    tif p { + => pass("cast: 1 as trit = +"), 0 => fail("cast"), - => fail("cast") }
    tif z { + => fail("cast"), 0 => pass("cast: 0 as trit = 0"), - => fail("cast") }
    tif n { + => fail("cast"), 0 => fail("cast"), - => pass("cast: -1 as trit = -") }
}

// ---------------------------------------------------------------------------
// bool → int
// ---------------------------------------------------------------------------

fn test_bool_to_int() {
    let t: bool = true;
    let f: bool = false;

    let ti: int = t as int;
    let fi: int = f as int;

    check_int("cast: true as int = 1",  ti, 1);
    check_int("cast: false as int = 0", fi, 0);
}

// ---------------------------------------------------------------------------
// int → bool
// ---------------------------------------------------------------------------

fn test_int_to_bool() {
    let b1: bool = 1 as bool;
    let b0: bool = 0 as bool;

    check("cast: 1 as bool = true",  b1);
    check("cast: 0 as bool = false", !b0);
}

// ---------------------------------------------------------------------------
// tryte, t9, t27 narrow/widen casts
// ---------------------------------------------------------------------------

fn test_narrow_wide_casts() {
    // int → tryte: should preserve value in range
    let n: int = 8;
    let t: tryte = n as tryte;
    let back: int = t as int;
    check_int("cast: int→tryte→int: 8", back, 8);

    // int → t9
    let m: int = 100;
    let n9: t9 = m as t9;
    let back9: int = n9 as int;
    check_int("cast: int→t9→int: 100", back9, 100);

    // int → t27 (native word)
    let large: int = 1000000;
    let n27: t27 = large as t27;
    let back27: int = n27 as int;
    check_int("cast: int→t27→int: 1000000", back27, 1000000);

    // Negative values
    let neg: int = -42;
    let nt: tryte = neg as tryte;
    let nback: int = nt as int;
    check_int("cast: -42→tryte→int", nback, -42);
}

// ---------------------------------------------------------------------------
// float arithmetic via cast
// ---------------------------------------------------------------------------

fn test_float_arithmetic() {
    let a: int = 10;
    let b: int = 3;
    // Integer division loses fractional part
    let idiv: int = a / b;
    check_int("cast: 10/3=3 (int div)", idiv, 3);

    // Float division preserves it
    let fdiv: float = (a as float) / (b as float);
    // 10.0/3.0 ≈ 3.333... → truncate to 3
    let truncated: int = fdiv as int;
    check_int("cast: (10.0/3.0) as int = 3", truncated, 3);

    // Ceiling-like: check that float value is > 3.0
    let floor_val: int = fdiv as int;
    let diff: float = fdiv - (floor_val as float);
    // diff should be > 0 for non-exact
    check("cast: 10.0/3.0 has fractional part", diff > 0.0);
}

// ---------------------------------------------------------------------------
// Chained casts
// ---------------------------------------------------------------------------

fn test_chained_casts() {
    let n: int = 5;
    // int → float → int
    let r1 = (n as float * 1.5) as int;
    check_int("cast-chain: 5*1.5 as int = 7", r1, 7);

    // int → trit → int (round-trip for valid trit values)
    let p: trit = +;
    let t_as_i: int = p as int;
    let i_as_t: trit = t_as_i as trit;
    tif i_as_t {
        + => pass("cast-chain: + → int → trit → +"),
        0 => fail("cast-chain: trit round-trip"),
        - => fail("cast-chain: trit round-trip"),
    }
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() {
    io::println("=== 09 Type Casting ===");

    io::println("-- int→float --");
    test_int_to_float();

    io::println("-- float→int --");
    test_float_to_int();

    io::println("-- trit→int --");
    test_trit_to_int();

    io::println("-- int→trit --");
    test_int_to_trit();

    io::println("-- bool→int --");
    test_bool_to_int();

    io::println("-- int→bool --");
    test_int_to_bool();

    io::println("-- narrow/wide casts --");
    test_narrow_wide_casts();

    io::println("-- float arithmetic --");
    test_float_arithmetic();

    io::println("-- chained casts --");
    test_chained_casts();

    io::println("Done.");
}
