// Test: closures and higher-order functions — map/filter/fold pipelines,
//        closures as arguments and return values, partial application patterns,
//        function pointers, repeated application, composition
use std::io;

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
// Higher-order helpers
// ---------------------------------------------------------------------------

fn apply(f: fn(int) -> int, x: int) -> int { f(x) }
fn apply2(f: fn(int, int) -> int, a: int, b: int) -> int { f(a, b) }
fn apply3(f: fn(int) -> int, g: fn(int) -> int, x: int) -> int { g(f(x)) }
fn twice(f: fn(int) -> int, x: int) -> int { f(f(x)) }
fn n_times(f: fn(int) -> int, n: int, x: int) -> int {
    let mut result = x;
    let mut i = 0;
    while i < n {
        result = f(result);
        i = i + 1;
    }
    result
}

// ---------------------------------------------------------------------------
// 1. Lambda literals
// ---------------------------------------------------------------------------

fn test_lambda_literals() {
    let inc    = fn(x: int) => x + 1;
    let double = fn(x: int) => x * 2;
    let neg    = fn(x: int) => -x;
    let square = fn(x: int) => x * x;
    let zero   = fn(x: int) => 0;

    check_int("lambda: inc(5)=6",     apply(inc, 5),    6);
    check_int("lambda: double(5)=10", apply(double, 5), 10);
    check_int("lambda: neg(5)=-5",    apply(neg, 5),    -5);
    check_int("lambda: square(4)=16", apply(square, 4), 16);
    check_int("lambda: zero(99)=0",   apply(zero, 99),  0);
}

// ---------------------------------------------------------------------------
// 2. Composition
// ---------------------------------------------------------------------------

fn test_composition() {
    let inc    = fn(x: int) => x + 1;
    let double = fn(x: int) => x * 2;
    let square = fn(x: int) => x * x;

    // (x+1)*2
    check_int("compose: inc then double: 3→8",  apply3(inc, double, 3), 8);
    // (x*2)+1
    check_int("compose: double then inc: 3→7",  apply3(double, inc, 3), 7);
    // (x+1)^2
    check_int("compose: inc then square: 4→25", apply3(inc, square, 4), 25);
    // (x^2)+1
    check_int("compose: square then inc: 4→17", apply3(square, inc, 4), 17);
}

// ---------------------------------------------------------------------------
// 3. Repeated application
// ---------------------------------------------------------------------------

fn test_twice() {
    let inc    = fn(x: int) => x + 1;
    let double = fn(x: int) => x * 2;

    check_int("twice: inc 5→7",    twice(inc, 5),    7);
    check_int("twice: double 3→12", twice(double, 3), 12);
}

fn test_n_times() {
    let inc = fn(x: int) => x + 1;
    // 0 applications → identity
    check_int("n-times: 0 inc(5)=5",  n_times(inc, 0, 5),  5);
    check_int("n-times: 3 inc(5)=8",  n_times(inc, 3, 5),  8);
    check_int("n-times: 5 inc(0)=5",  n_times(inc, 5, 0),  5);

    let double = fn(x: int) => x * 2;
    // 1*2^4 = 16
    check_int("n-times: 4 double(1)=16", n_times(double, 4, 1), 16);
}

// ---------------------------------------------------------------------------
// 4. Vec pipeline: map → filter → fold
// ---------------------------------------------------------------------------

fn test_pipeline() {
    let mut v: Vec<int> = Vec::new();
    v.push(1); v.push(2); v.push(3); v.push(4); v.push(5);

    // Map: double each element → [2,4,6,8,10]
    let doubled = v.map(fn(x: int) => x * 2);
    check_int("pipeline: doubled[0]=2",  doubled.get(0), 2);
    check_int("pipeline: doubled[4]=10", doubled.get(4), 10);

    // Filter: keep only those > 6 → [8, 10]
    let big = doubled.filter(fn(x: int) => x > 6);
    check_int("pipeline: big len=2",   big.len(), 2);
    check_int("pipeline: big[0]=8",    big.get(0), 8);
    check_int("pipeline: big[1]=10",   big.get(1), 10);

    // Fold: sum → 18
    let sum = big.fold(0, fn(acc: int, x: int) => acc + x);
    check_int("pipeline: sum=18", sum, 18);
}

// ---------------------------------------------------------------------------
// 5. Map with complex transformation
// ---------------------------------------------------------------------------

fn clamp_to_10(x: int) -> int {
    if x > 10 { 10 } elif x < -10 { -10 } else { x }
}

fn test_map_complex() {
    let mut v: Vec<int> = Vec::new();
    v.push(-15); v.push(-5); v.push(0); v.push(5); v.push(15);

    let clamped = v.map(fn(x: int) => clamp_to_10(x));
    check_int("map-complex: -15→-10",  clamped.get(0), -10);
    check_int("map-complex: -5→-5",    clamped.get(1), -5);
    check_int("map-complex: 0→0",      clamped.get(2), 0);
    check_int("map-complex: 5→5",      clamped.get(3), 5);
    check_int("map-complex: 15→10",    clamped.get(4), 10);
}

// ---------------------------------------------------------------------------
// 6. Filter with complex predicate
// ---------------------------------------------------------------------------

fn is_perfect_square(n: int) -> bool {
    if n < 0 { return false; }
    let mut i = 0;
    while i * i <= n {
        if i * i == n { return true; }
        i = i + 1;
    }
    false
}

fn test_filter_complex() {
    let mut v: Vec<int> = Vec::new();
    v.push(0); v.push(1); v.push(2); v.push(3);
    v.push(4); v.push(5); v.push(9); v.push(16);

    let squares = v.filter(fn(x: int) => is_perfect_square(x));
    // Squares: 0, 1, 4, 9, 16
    check_int("filter-complex: perfect squares len=5", squares.len(), 5);
    check_int("filter-complex: [0]=0",  squares.get(0), 0);
    check_int("filter-complex: [1]=1",  squares.get(1), 1);
    check_int("filter-complex: [2]=4",  squares.get(2), 4);
    check_int("filter-complex: [3]=9",  squares.get(3), 9);
    check_int("filter-complex: [4]=16", squares.get(4), 16);
}

// ---------------------------------------------------------------------------
// 7. Fold: more complex accumulators
// ---------------------------------------------------------------------------

fn test_fold_complex() {
    let mut v: Vec<int> = Vec::new();
    v.push(3); v.push(1); v.push(4); v.push(1); v.push(5);

    // Max via fold
    let max_val = v.fold(-99999, fn(acc: int, x: int) => if x > acc { x } else { acc });
    check_int("fold: max=5", max_val, 5);

    // Min via fold
    let min_val = v.fold(99999, fn(acc: int, x: int) => if x < acc { x } else { acc });
    check_int("fold: min=1", min_val, 1);

    // Count elements > 2
    let cnt = v.fold(0, fn(acc: int, x: int) => if x > 2 { acc + 1 } else { acc });
    check_int("fold: count>2=3", cnt, 3);

    // String construction via fold (int → sum as string length)
    let total = v.fold(0, fn(acc: int, x: int) => acc + x);
    check_int("fold: sum=14", total, 14);
}

// ---------------------------------------------------------------------------
// 8. Two-argument closures
// ---------------------------------------------------------------------------

fn test_two_arg_closures() {
    let add = fn(a: int, b: int) => a + b;
    let sub = fn(a: int, b: int) => a - b;
    let mul = fn(a: int, b: int) => a * b;
    let max2 = fn(a: int, b: int) => if a > b { a } else { b };

    check_int("2-arg: add(7,3)=10",   apply2(add, 7, 3),  10);
    check_int("2-arg: sub(7,3)=4",    apply2(sub, 7, 3),   4);
    check_int("2-arg: mul(7,3)=21",   apply2(mul, 7, 3),  21);
    check_int("2-arg: max2(7,3)=7",   apply2(max2, 7, 3),  7);
    check_int("2-arg: max2(3,7)=7",   apply2(max2, 3, 7),  7);
}

// ---------------------------------------------------------------------------
// 9. Named function as argument
// ---------------------------------------------------------------------------

fn increment(x: int) -> int { x + 1 }
fn decrement(x: int) -> int { x - 1 }
fn negate(x: int) -> int { -x }

fn test_named_fn_as_arg() {
    check_int("named-fn: increment(4)=5",    apply(increment, 4),  5);
    check_int("named-fn: decrement(4)=3",    apply(decrement, 4),  3);
    check_int("named-fn: negate(4)=-4",      apply(negate, 4),    -4);
    check_int("named-fn: twice-inc(5)=7",    twice(increment, 5),  7);
    check_int("named-fn: n-times 5 inc(0)=5", n_times(increment, 5, 0), 5);
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() {
    io::println("=== 12 Advanced Closures ===");

    io::println("-- lambda literals --");
    test_lambda_literals();

    io::println("-- composition --");
    test_composition();

    io::println("-- twice --");
    test_twice();

    io::println("-- n-times --");
    test_n_times();

    io::println("-- pipeline map→filter→fold --");
    test_pipeline();

    io::println("-- map complex --");
    test_map_complex();

    io::println("-- filter complex --");
    test_filter_complex();

    io::println("-- fold complex --");
    test_fold_complex();

    io::println("-- two-arg closures --");
    test_two_arg_closures();

    io::println("-- named fn as arg --");
    test_named_fn_as_arg();

    io::println("Done.");
}
