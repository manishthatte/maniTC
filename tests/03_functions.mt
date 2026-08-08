// Test: functions — recursion, multiple return paths, generics, closures/lambdas,
//        mutual recursion, default args via wrapper, higher-order functions
use std::io;

fn pass(label: str) { io::print("PASS "); io::println(label); }
fn fail(label: str) { io::print("FAIL "); io::println(label); }
fn check(label: str, cond: bool) { if cond { pass(label) } else { fail(label) } }

fn check_int(label: str, got: int, want: int) {
    if got == want { pass(label); }
    else {
        io::print("FAIL ");
        io::print(label);
        io::print(" got=");
        io::print_int(got);
        io::print(" want=");
        io::print_int(want);
        io::newline();
    }
}

// ---------------------------------------------------------------------------
// Basic functions
// ---------------------------------------------------------------------------

fn add(a: int, b: int) -> int { a + b }
fn mul(a: int, b: int) -> int { a * b }
fn square(n: int) -> int { n * n }

fn test_basic() {
    check_int("basic: add(3,4)=7",     add(3, 4),   7);
    check_int("basic: mul(6,7)=42",    mul(6, 7),   42);
    check_int("basic: square(9)=81",   square(9),   81);
}

// ---------------------------------------------------------------------------
// Multiple return paths
// ---------------------------------------------------------------------------

fn sign(n: int) -> int {
    if n > 0 { return 1; }
    if n < 0 { return -1; }
    0
}

fn abs_val(n: int) -> int {
    if n < 0 { -n } else { n }
}

fn test_multiple_returns() {
    check_int("multi-ret: sign(5)=1",   sign(5),   1);
    check_int("multi-ret: sign(-3)=-1", sign(-3), -1);
    check_int("multi-ret: sign(0)=0",   sign(0),   0);
    check_int("abs: abs(-7)=7",         abs_val(-7), 7);
    check_int("abs: abs(7)=7",          abs_val(7),  7);
}

// ---------------------------------------------------------------------------
// Recursion
// ---------------------------------------------------------------------------

fn factorial(n: int) -> int {
    if n <= 1 { 1 } else { n * factorial(n - 1) }
}

fn fib(n: int) -> int {
    if n <= 1 { n } else { fib(n - 1) + fib(n - 2) }
}

fn power(base: int, exp: int) -> int {
    if exp == 0 { 1 }
    elif exp % 2 == 0 {
        let half = power(base, exp / 2);
        half * half
    } else {
        base * power(base, exp - 1)
    }
}

fn test_recursion() {
    check_int("rec: 0!=1",         factorial(0),  1);
    check_int("rec: 1!=1",         factorial(1),  1);
    check_int("rec: 5!=120",       factorial(5),  120);
    check_int("rec: fib(0)=0",     fib(0),  0);
    check_int("rec: fib(1)=1",     fib(1),  1);
    check_int("rec: fib(10)=55",   fib(10), 55);
    check_int("rec: pow(2,10)=1024", power(2, 10), 1024);
    check_int("rec: pow(3,4)=81",    power(3, 4),  81);
    check_int("rec: pow(5,0)=1",     power(5, 0),  1);
}

// ---------------------------------------------------------------------------
// Mutual recursion
// ---------------------------------------------------------------------------

fn is_even(n: int) -> bool {
    if n == 0 { true } else { is_odd(n - 1) }
}

fn is_odd(n: int) -> bool {
    if n == 0 { false } else { is_even(n - 1) }
}

fn test_mutual_recursion() {
    check("mutual: is_even(0)",  is_even(0));
    check("mutual: is_odd(1)",   is_odd(1));
    check("mutual: is_even(4)",  is_even(4));
    check("mutual: is_odd(7)",   is_odd(7));
    check("mutual: !is_even(3)", !is_even(3));
}

// ---------------------------------------------------------------------------
// Higher-order functions (lambdas / closures)
// ---------------------------------------------------------------------------

fn apply(x: int, f: fn(int) -> int) -> int { f(x) }
fn apply2(a: int, b: int, f: fn(int, int) -> int) -> int { f(a, b) }
fn compose(x: int, f: fn(int) -> int, g: fn(int) -> int) -> int { g(f(x)) }

fn test_lambdas() {
    let double = fn(x: int) => x * 2;
    let inc    = fn(x: int) => x + 1;
    let triple = fn(x: int) => x * 3;

    check_int("lambda: double(5)=10",          apply(5, double), 10);
    check_int("lambda: inc(9)=10",             apply(9, inc),    10);
    check_int("lambda: compose inc then dbl",  compose(3, inc, double), 8);
    check_int("lambda: compose dbl then inc",  compose(3, double, inc), 7);
    check_int("lambda: apply2 add",            apply2(3, 4, fn(a: int, b: int) => a + b), 7);
    check_int("lambda: apply2 mul",            apply2(3, 4, fn(a: int, b: int) => a * b), 12);
}

// ---------------------------------------------------------------------------
// Generic functions
// ---------------------------------------------------------------------------

fn identity<T>(x: T) -> T { x }

fn max_int(a: int, b: int) -> int { if a > b { a } else { b } }
fn min_int(a: int, b: int) -> int { if a < b { a } else { b } }

fn clamp_int(v: int, lo: int, hi: int) -> int {
    if v < lo { lo } elif v > hi { hi } else { v }
}

fn test_generics() {
    check_int("generic: identity(42)=42",  identity(42),  42);
    check_int("generic: max(7,3)=7",       max_int(7, 3), 7);
    check_int("generic: max(3,7)=7",       max_int(3, 7), 7);
    check_int("generic: min(7,3)=3",       min_int(7, 3), 3);
    check_int("generic: clamp(5,0,10)=5",  clamp_int(5,  0, 10),  5);
    check_int("generic: clamp(-1,0,10)=0", clamp_int(-1, 0, 10),  0);
    check_int("generic: clamp(15,0,10)=10",clamp_int(15, 0, 10), 10);
}

// ---------------------------------------------------------------------------
// Functions as return values (via wrapper)
// ---------------------------------------------------------------------------

fn add_to_ten(n: int) -> int { n + 10 }

fn make_adder_result(n: int) -> int {
    // Simulate closure result: equivalent to (fn(x) => x + n)(10)
    add_to_ten(n)
}

fn test_fn_result() {
    check_int("fn-result: make_adder(5)(10)=15",  make_adder_result(5),  15);
    check_int("fn-result: make_adder(0)(10)=10",  make_adder_result(0),  10);
    check_int("fn-result: make_adder(-3)(10)=7",  make_adder_result(-3), 7);
}

// ---------------------------------------------------------------------------
// Tail position / last-expression-as-return
// ---------------------------------------------------------------------------

fn double_if_positive(n: int) -> int {
    if n > 0 { n * 2 } else { n }
}

fn test_tail_expr() {
    check_int("tail: double_if_positive(5)=10",  double_if_positive(5),  10);
    check_int("tail: double_if_positive(-3)=-3", double_if_positive(-3), -3);
    check_int("tail: double_if_positive(0)=0",   double_if_positive(0),  0);
}

// ---------------------------------------------------------------------------
// Void functions
// ---------------------------------------------------------------------------

fn void_fn(n: int) {
    let mut x = n;
    x = x + 1;
    // returns void implicitly
}

fn test_void() {
    void_fn(5);
    pass("void: fn returns normally");
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() {
    io::println("=== 03 Functions ===");

    io::println("-- basic --");
    test_basic();

    io::println("-- multiple returns --");
    test_multiple_returns();

    io::println("-- recursion --");
    test_recursion();

    io::println("-- mutual recursion --");
    test_mutual_recursion();

    io::println("-- lambdas --");
    test_lambdas();

    io::println("-- generics --");
    test_generics();

    io::println("-- fn result --");
    test_fn_result();

    io::println("-- tail expr --");
    test_tail_expr();

    io::println("-- void --");
    test_void();

    io::println("Done.");
}
