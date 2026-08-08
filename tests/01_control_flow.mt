// Test: control flow — if/elif/else, tif, match, for, while, loop, break, continue, return
use std::io;

fn pass(label: str) {
    io::print("PASS ");
    io::println(label);
}

fn fail(label: str) {
    io::print("FAIL ");
    io::println(label);
}

// ---------------------------------------------------------------------------
// if / elif / else
// ---------------------------------------------------------------------------

fn test_if_basic() {
    let x: int = 7;
    if x > 5 { pass("if: x>5") } else { fail("if: x>5") }
    if x == 7 { pass("if: x==7") } else { fail("if: x==7") }
    if x < 0 { fail("if: negative") } else { pass("if: not-negative") }
}

fn test_elif() {
    let x: int = 3;
    if x == 1 {
        fail("elif: branch1")
    } elif x == 2 {
        fail("elif: branch2")
    } elif x == 3 {
        pass("elif: branch3")
    } else {
        fail("elif: else")
    }
}

fn test_if_expression() {
    let x: int = 10;
    let label = if x > 5 { "big" } else { "small" };
    if label == "big" { pass("if-expr: big") } else { fail("if-expr: big") }

    let label2 = if x == 10 { "ten" } elif x == 9 { "nine" } else { "other" };
    if label2 == "ten" { pass("if-expr: elif") } else { fail("if-expr: elif") }
}

fn test_nested_if() {
    let a: int = 1;
    let b: int = 2;
    if a < b {
        if b < 3 {
            pass("nested-if: both")
        } else {
            fail("nested-if: inner")
        }
    } else {
        fail("nested-if: outer")
    }
}

// ---------------------------------------------------------------------------
// tif — three-way branch on trit
// ---------------------------------------------------------------------------

fn test_tif() {
    let pos: trit = +;
    let zer: trit = 0;
    let neg: trit = -;

    tif pos {
        + => pass("tif: pos arm"),
        0 => fail("tif: pos arm"),
        - => fail("tif: pos arm"),
    }
    tif zer {
        + => fail("tif: zer arm"),
        0 => pass("tif: zer arm"),
        - => fail("tif: zer arm"),
    }
    tif neg {
        + => fail("tif: neg arm"),
        0 => fail("tif: neg arm"),
        - => pass("tif: neg arm"),
    }
}

fn test_tif_bool3() {
    let t: bool3 = true;
    let u: bool3 = unknown;
    let f: bool3 = false;

    tif t { + => pass("tif-bool3: true"), 0 => fail("tif-bool3: true"), - => fail("tif-bool3: true") }
    tif u { + => fail("tif-bool3: unknown"), 0 => pass("tif-bool3: unknown"), - => fail("tif-bool3: unknown") }
    tif f { + => fail("tif-bool3: false"), 0 => fail("tif-bool3: false"), - => pass("tif-bool3: false") }
}

// ---------------------------------------------------------------------------
// match — literal patterns
// ---------------------------------------------------------------------------

fn test_match_int() {
    let x: int = 42;
    match x {
        1  => fail("match-int: 1"),
        42 => pass("match-int: 42"),
        _  => fail("match-int: wildcard"),
    }
}

fn test_match_wildcard() {
    let x: int = 999;
    match x {
        1 => fail("match-wild: 1"),
        _ => pass("match-wild: wildcard"),
    }
}

fn test_match_guard() {
    let x: int = 15;
    match x {
        n if n > 100 => fail("match-guard: big"),
        n if n > 10  => pass("match-guard: medium"),
        _            => fail("match-guard: small"),
    }
}

fn test_match_or_pattern() {
    let x: int = 3;
    match x {
        1 | 2 | 3 => pass("match-or: 1|2|3"),
        _         => fail("match-or: wildcard"),
    }
    let y: int = 7;
    match y {
        1 | 2 | 3 => fail("match-or: no match"),
        _         => pass("match-or: fallthrough"),
    }
}

fn classify(n: int) -> str {
    match n {
        0         => "zero",
        1         => "one",
        n if n < 0 => "negative",
        _         => "other",
    }
}

fn test_match_as_expr() {
    let a = classify(0);
    if a == "zero"     { pass("match-expr: zero")    } else { fail("match-expr: zero")    }
    let b = classify(1);
    if b == "one"      { pass("match-expr: one")     } else { fail("match-expr: one")     }
    let c = classify(-5);
    if c == "negative" { pass("match-expr: negative") } else { fail("match-expr: negative") }
    let d = classify(100);
    if d == "other"    { pass("match-expr: other")   } else { fail("match-expr: other")   }
}

// ---------------------------------------------------------------------------
// for loop
// ---------------------------------------------------------------------------

fn test_for_range() {
    let mut sum: int = 0;
    for i in 1..5 {
        sum = sum + i;
    }
    // 1+2+3+4 = 10
    if sum == 10 { pass("for: range sum") } else { fail("for: range sum") }
}

fn test_for_inclusive_range() {
    let mut sum: int = 0;
    for i in 1..=5 {
        sum = sum + i;
    }
    // 1+2+3+4+5 = 15
    if sum == 15 { pass("for: inclusive range") } else { fail("for: inclusive range") }
}

fn test_for_zero_range() {
    let mut count: int = 0;
    for i in 5..5 {
        count = count + 1;
    }
    if count == 0 { pass("for: empty range") } else { fail("for: empty range") }
}

fn test_for_nested() {
    let mut total: int = 0;
    for i in 0..3 {
        for j in 0..3 {
            total = total + 1;
        }
    }
    if total == 9 { pass("for: nested 3x3=9") } else { fail("for: nested 3x3=9") }
}

// ---------------------------------------------------------------------------
// while loop
// ---------------------------------------------------------------------------

fn test_while() {
    let mut n: int = 1;
    let mut product: int = 1;
    while n <= 5 {
        product = product * n;
        n = n + 1;
    }
    // 5! = 120
    if product == 120 { pass("while: 5!=120") } else { fail("while: 5!=120") }
}

fn test_while_no_execute() {
    let mut count: int = 0;
    while count > 0 {
        count = count + 1;
    }
    if count == 0 { pass("while: no-execute") } else { fail("while: no-execute") }
}

// ---------------------------------------------------------------------------
// loop / break / continue
// ---------------------------------------------------------------------------

fn test_loop_break() {
    let mut i: int = 0;
    loop {
        i = i + 1;
        if i == 5 { break; }
    }
    if i == 5 { pass("loop: break at 5") } else { fail("loop: break at 5") }
}

fn test_continue() {
    let mut sum: int = 0;
    for i in 0..10 {
        if i % 2 == 0 { continue; }
        sum = sum + i;
    }
    // 1+3+5+7+9 = 25
    if sum == 25 { pass("continue: odd sum") } else { fail("continue: odd sum") }
}

fn test_break_in_for() {
    let mut last: int = 0;
    for i in 0..100 {
        if i == 7 { break; }
        last = i;
    }
    if last == 6 { pass("break: in for at 7") } else { fail("break: in for at 7") }
}

// ---------------------------------------------------------------------------
// return
// ---------------------------------------------------------------------------

fn first_positive(vals: int, count: int) -> int {
    // vals and count encode a small array via stack trick; simplified here
    return -1;
}

fn early_return(x: int) -> str {
    if x < 0 { return "negative"; }
    if x == 0 { return "zero"; }
    return "positive";
}

fn test_return() {
    if early_return(-1) == "negative" { pass("return: negative") } else { fail("return: negative") }
    if early_return(0) == "zero"      { pass("return: zero")     } else { fail("return: zero")     }
    if early_return(1) == "positive"  { pass("return: positive") } else { fail("return: positive") }
}

// ---------------------------------------------------------------------------
// block expressions
// ---------------------------------------------------------------------------

fn test_block_expr() {
    let v = {
        let a = 3;
        let b = 4;
        a * a + b * b
    };
    if v == 25 { pass("block-expr: 3^2+4^2=25") } else { fail("block-expr: 3^2+4^2=25") }
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() {
    io::println("=== 01 Control Flow ===");

    io::println("-- if/elif/else --");
    test_if_basic();
    test_elif();
    test_if_expression();
    test_nested_if();

    io::println("-- tif --");
    test_tif();
    test_tif_bool3();

    io::println("-- match --");
    test_match_int();
    test_match_wildcard();
    test_match_guard();
    test_match_or_pattern();
    test_match_as_expr();

    io::println("-- for --");
    test_for_range();
    test_for_inclusive_range();
    test_for_zero_range();
    test_for_nested();

    io::println("-- while --");
    test_while();
    test_while_no_execute();

    io::println("-- loop/break/continue --");
    test_loop_break();
    test_continue();
    test_break_in_for();

    io::println("-- return --");
    test_return();

    io::println("-- block expr --");
    test_block_expr();

    io::println("Done.");
}
