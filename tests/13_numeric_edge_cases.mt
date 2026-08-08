// Test: numeric edge cases — integer boundaries, division/mod behavior,
//        balanced ternary extremes, large numbers, signed arithmetic,
//        zero-related operations, overflow-safe patterns
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
// Integer identity
// ---------------------------------------------------------------------------

fn test_identity_ops() {
    check_int("id: n+0=n",     42 + 0,     42);
    check_int("id: n-0=n",     42 - 0,     42);
    check_int("id: n*1=n",     42 * 1,     42);
    check_int("id: n/1=n",     42 / 1,     42);
    check_int("id: n*0=0",     42 * 0,     0);
    check_int("id: 0/n=0",     0 / 42,     0);
    check_int("id: 0+0=0",     0 + 0,     0);
    check_int("id: 0-0=0",     0 - 0,     0);
    check_int("id: 0*0=0",     0 * 0,     0);
}

// ---------------------------------------------------------------------------
// Negation and double negation
// ---------------------------------------------------------------------------

fn test_negation() {
    check_int("neg: -0=0",          -0,       0);
    check_int("neg: --5=5",         -(-5),    5);
    check_int("neg: --0=0",         -(-0),    0);
    check_int("neg: -1=-1",         -1,      -1);
    check_int("neg: -(-1)=1",       -(-1),    1);
    check_int("neg: -(a+b)=-a-b",   -(3 + 4), -7);
}

// ---------------------------------------------------------------------------
// Division truncation toward zero
// ---------------------------------------------------------------------------

fn test_div_truncation() {
    // Positive / positive: truncate down
    check_int("div-trunc: 7/2=3",     7 / 2,    3);
    check_int("div-trunc: 1/2=0",     1 / 2,    0);
    check_int("div-trunc: 9/3=3",     9 / 3,    3);
    check_int("div-trunc: 10/3=3",    10 / 3,   3);

    // Negative / positive: truncate toward zero (not floor)
    check_int("div-trunc: -7/2=-3",  -7 / 2,   -3);
    check_int("div-trunc: -1/2=0",   -1 / 2,    0);
    check_int("div-trunc: -9/3=-3",  -9 / 3,   -3);

    // Positive / negative
    check_int("div-trunc: 7/-2=-3",   7 / -2,  -3);
    check_int("div-trunc: 1/-2=0",    1 / -2,   0);

    // Negative / negative: positive
    check_int("div-trunc: -7/-2=3",  -7 / -2,  3);
    check_int("div-trunc: -9/-3=3",  -9 / -3,  3);
}

// ---------------------------------------------------------------------------
// Modulo: sign matches dividend (truncated division)
// ---------------------------------------------------------------------------

fn test_modulo() {
    check_int("mod: 7%3=1",     7 % 3,    1);
    check_int("mod: 9%3=0",     9 % 3,    0);
    check_int("mod: 10%3=1",    10 % 3,   1);
    check_int("mod: 1%1=0",     1 % 1,    0);

    // Negative dividend: result sign matches dividend
    check_int("mod: -7%3=-1",  -7 % 3,   -1);
    check_int("mod: -9%3=0",   -9 % 3,    0);
    check_int("mod: -10%3=-1", -10 % 3,  -1);

    // Negative divisor
    check_int("mod: 7%-3=1",    7 % -3,   1);
    check_int("mod: -7%-3=-1", -7 % -3,  -1);

    // n%1 = 0 for any n
    check_int("mod: 42%1=0",  42 % 1,   0);
    check_int("mod: -5%1=0", -5 % 1,   0);
    check_int("mod: 0%1=0",   0 % 1,   0);
}

// ---------------------------------------------------------------------------
// Div/mod invariant: (a/b)*b + a%b == a
// ---------------------------------------------------------------------------

fn check_divmod_invariant(label: str, a: int, b: int) {
    let q = a / b;
    let r = a % b;
    let check_val = q * b + r;
    check_int(label, check_val, a);
}

fn test_divmod_invariant() {
    check_divmod_invariant("divmod: 17,5",   17,  5);
    check_divmod_invariant("divmod: -17,5", -17,  5);
    check_divmod_invariant("divmod: 17,-5",  17, -5);
    check_divmod_invariant("divmod: -17,-5",-17, -5);
    check_divmod_invariant("divmod: 100,7",  100,  7);
    check_divmod_invariant("divmod: 1,3",      1,  3);
    check_divmod_invariant("divmod: 0,7",      0,  7);
}

// ---------------------------------------------------------------------------
// Large integer arithmetic
// ---------------------------------------------------------------------------

fn test_large_numbers() {
    let big: int = 1000000000;   // 10^9
    let big2 = big * 1000;       // 10^12
    check_int("large: 10^9 * 1000 / 1000 = 10^9", big2 / 1000, big);

    let sum = big + big;
    check_int("large: 10^9 + 10^9 = 2*10^9", sum / big, 2);

    let diff = big2 - big;
    check_int("large: 10^12 - 10^9 = 999*10^9", diff / 1000000000, 999);
}

// ---------------------------------------------------------------------------
// Power function correctness
// ---------------------------------------------------------------------------

fn power(base: int, exp: int) -> int {
    if exp == 0 { return 1; }
    let mut result: int = 1;
    let mut e = exp;
    while e > 0 {
        result = result * base;
        e = e - 1;
    }
    result
}

fn test_power() {
    check_int("pow: 2^0=1",   power(2, 0),  1);
    check_int("pow: 2^1=2",   power(2, 1),  2);
    check_int("pow: 2^10=1024", power(2, 10), 1024);
    check_int("pow: 3^5=243", power(3, 5),  243);
    check_int("pow: 10^6=10^6", power(10, 6), 1000000);
    check_int("pow: 1^100=1",  power(1, 100), 1);
    check_int("pow: -1^1=-1",  power(-1, 1), -1);
    check_int("pow: -1^2=1",   power(-1, 2),  1);
    check_int("pow: -1^3=-1",  power(-1, 3), -1);
}

// ---------------------------------------------------------------------------
// GCD and LCM
// ---------------------------------------------------------------------------

fn gcd(a: int, b: int) -> int {
    let mut x = if a < 0 { -a } else { a };
    let mut y = if b < 0 { -b } else { b };
    while y != 0 {
        let tmp = y;
        y = x % y;
        x = tmp;
    }
    x
}

fn lcm(a: int, b: int) -> int {
    if a == 0 || b == 0 { return 0; }
    let g = gcd(a, b);
    (a / g) * b
}

fn test_gcd_lcm() {
    check_int("gcd: gcd(12,8)=4",    gcd(12, 8),    4);
    check_int("gcd: gcd(9,6)=3",     gcd(9, 6),     3);
    check_int("gcd: gcd(17,13)=1",   gcd(17, 13),   1);
    check_int("gcd: gcd(0,5)=5",     gcd(0, 5),     5);
    check_int("gcd: gcd(5,0)=5",     gcd(5, 0),     5);
    check_int("gcd: gcd(7,7)=7",     gcd(7, 7),     7);

    check_int("lcm: lcm(4,6)=12",    lcm(4, 6),     12);
    check_int("lcm: lcm(3,5)=15",    lcm(3, 5),     15);
    check_int("lcm: lcm(0,5)=0",     lcm(0, 5),     0);
    check_int("lcm: lcm(7,7)=7",     lcm(7, 7),     7);
}

// ---------------------------------------------------------------------------
// Balanced ternary literal extremes and conversions
// ---------------------------------------------------------------------------

fn test_bt_extremes() {
    // 0t+ = 1
    let one = 0t+;
    check_int("bt-extreme: 0t+=1",   one, 1);

    // 0t- = -1
    let neg_one = 0t-;
    check_int("bt-extreme: 0t-=-1",  neg_one, -1);

    // 0t+++ = 9+3+1 = 13  (max tryte)
    let max_tryte = 0t+++;
    check_int("bt-extreme: 0t+++=13", max_tryte, 13);

    // 0t--- = -(9+3+1) = -13  (min tryte)
    let min_tryte = 0t---;
    check_int("bt-extreme: 0t---= -13", min_tryte, -13);

    // Verify negation: -(0t+++) == 0t---
    check_int("bt-extreme: -(max)=min", -max_tryte, min_tryte);
    check_int("bt-extreme: -(min)=max", -min_tryte, max_tryte);

    // 0t+0- = 9+0-1 = 8
    check_int("bt-extreme: 0t+0-=8", 0t+0-, 8);

    // 0t-0+ = -9+0+1 = -8
    check_int("bt-extreme: 0t-0+=-8", 0t-0+, -8);
}

// ---------------------------------------------------------------------------
// math::to_balanced_ternary / from_balanced_ternary round-trips
// ---------------------------------------------------------------------------

fn test_bt_roundtrip_many() {
    let vals: Vec<int> = Vec::new();
    vals.push(0); vals.push(1); vals.push(-1);
    vals.push(13); vals.push(-13); vals.push(100); vals.push(-100);
    vals.push(364); vals.push(-364); vals.push(1000); vals.push(-1000);

    for v in vals {
        let s = math::to_balanced_ternary(v);
        let r = math::from_balanced_ternary(s);
        if r == v {
            pass("bt-roundtrip: value");
        } else {
            io::print("FAIL bt-roundtrip: ");
            io::print_int(v);
            io::print(" got=");
            io::print_int(r);
            io::newline();
        }
    }
}

// ---------------------------------------------------------------------------
// Absolute value
// ---------------------------------------------------------------------------

fn abs(n: int) -> int { if n < 0 { -n } else { n } }

fn test_abs() {
    check_int("abs: abs(5)=5",   abs(5),   5);
    check_int("abs: abs(-5)=5",  abs(-5),  5);
    check_int("abs: abs(0)=0",   abs(0),   0);
    check_int("abs: abs(-1)=1",  abs(-1),  1);
    check_int("abs: abs(100)=100", abs(100), 100);
}

// ---------------------------------------------------------------------------
// Comparison boundary cases
// ---------------------------------------------------------------------------

fn test_comparison_boundary() {
    check("cmp-bnd: 0 < 1",     0 < 1);
    check("cmp-bnd: -1 < 0",   -1 < 0);
    check("cmp-bnd: -1 < 1",   -1 < 1);
    check("cmp-bnd: 0 == 0",    0 == 0);
    check("cmp-bnd: -1 == -1", -1 == -1);
    check("cmp-bnd: 1 > 0",     1 > 0);
    check("cmp-bnd: 0 >= 0",    0 >= 0);
    check("cmp-bnd: 1 >= 0",    1 >= 0);
    check("cmp-bnd: 0 <= 0",    0 <= 0);
    check("cmp-bnd: 0 <= 1",    0 <= 1);
    check("cmp-bnd: !(0 < 0)", !(0 < 0));
    check("cmp-bnd: !(1 < 1)", !(1 < 1));
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() {
    io::println("=== 13 Numeric Edge Cases ===");

    io::println("-- identity ops --");
    test_identity_ops();

    io::println("-- negation --");
    test_negation();

    io::println("-- div truncation --");
    test_div_truncation();

    io::println("-- modulo --");
    test_modulo();

    io::println("-- divmod invariant --");
    test_divmod_invariant();

    io::println("-- large numbers --");
    test_large_numbers();

    io::println("-- power --");
    test_power();

    io::println("-- gcd/lcm --");
    test_gcd_lcm();

    io::println("-- bt extremes --");
    test_bt_extremes();

    io::println("-- bt roundtrip --");
    test_bt_roundtrip_many();

    io::println("-- abs --");
    test_abs();

    io::println("-- comparison boundary --");
    test_comparison_boundary();

    io::println("Done.");
}
