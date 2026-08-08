// Test: nested control flow — deeply nested tif, tif inside loops, tmatch
//        inside tif, tand/tor in loop conditions, break/continue with ternary
//        conditions, recursion with tif branching
use std::io;
use std::ternary;

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
fn check_str(label: str, got: str, want: str) {
    if got == want { pass(label) }
    else {
        io::print("FAIL "); io::print(label);
        io::print(" got=["); io::print(got);
        io::print("] want=["); io::print(want); io::println("]");
    }
}

fn trit_val(t: trit) -> int {
    if t > 0 { 1 } elif t == 0 { 0 } else { -1 }
}

// ---------------------------------------------------------------------------
// Deeply nested tif (3 levels)
// ---------------------------------------------------------------------------

fn classify_three(a: trit, b: trit, c: trit) -> str {
    tif a {
        + => tif b {
            + => tif c {
                + => "+++",
                0 => "++0",
                - => "++-",
            },
            0 => tif c {
                + => "+0+",
                0 => "+00",
                - => "+0-",
            },
            - => "+-*",
        },
        0 => tif b {
            + => "0+*",
            0 => "000",
            - => "0-*",
        },
        - => "-**",
    }
}

fn test_nested_tif_3_levels() {
    check_str("nest-tif3: +++",  classify_three(+, +, +),  "+++");
    check_str("nest-tif3: ++0",  classify_three(+, +, 0),  "++0");
    check_str("nest-tif3: ++-",  classify_three(+, +, -),  "++-");
    check_str("nest-tif3: +0+",  classify_three(+, 0, +),  "+0+");
    check_str("nest-tif3: +00",  classify_three(+, 0, 0),  "+00");
    check_str("nest-tif3: +0-",  classify_three(+, 0, -),  "+0-");
    check_str("nest-tif3: +-*",  classify_three(+, -, +),  "+-*");
    check_str("nest-tif3: 0+*",  classify_three(0, +, 0),  "0+*");
    check_str("nest-tif3: 000",  classify_three(0, 0, 0),  "000");
    check_str("nest-tif3: 0-*",  classify_three(0, -, -),  "0-*");
    check_str("nest-tif3: -**",  classify_three(-, +, +),  "-**");
}

// ---------------------------------------------------------------------------
// tif inside while loop
// ---------------------------------------------------------------------------

fn test_tif_in_while() {
    let trits: Vec<int> = Vec::new();
    trits.push(1); trits.push(0); trits.push(-1); trits.push(1); trits.push(-1);

    let mut pos_count: int = 0;
    let mut zero_count: int = 0;
    let mut neg_count: int = 0;
    let mut i: int = 0;

    while i < 5 {
        let t: trit = trits.get(i) as trit;
        tif t {
            + => { pos_count = pos_count + 1; },
            0 => { zero_count = zero_count + 1; },
            - => { neg_count = neg_count + 1; },
        }
        i = i + 1;
    }

    check_int("tif-while: pos count=2",  pos_count,  2);
    check_int("tif-while: zero count=1", zero_count, 1);
    check_int("tif-while: neg count=2",  neg_count,  2);
}

// ---------------------------------------------------------------------------
// tif inside for loop with accumulation
// ---------------------------------------------------------------------------

fn test_tif_in_for() {
    // Compute weighted sum: + contributes +1, 0 contributes 0, - contributes -1
    let mut sum: int = 0;
    let vals: Vec<int> = Vec::new();
    vals.push(1); vals.push(1); vals.push(0); vals.push(-1); vals.push(1);

    for vi in vals {
        let t: trit = vi as trit;
        let contrib = tif t {
            + => 1,
            0 => 0,
            - => -1,
        };
        sum = sum + contrib;
    }
    // 1+1+0+(-1)+1 = 2
    check_int("tif-for: weighted sum=2", sum, 2);
}

// ---------------------------------------------------------------------------
// match inside tif
// ---------------------------------------------------------------------------

fn classify_with_detail(t: trit, magnitude: int) -> str {
    tif t {
        + => match magnitude {
            0         => "pos-zero",
            n if n < 5 => "pos-small",
            _         => "pos-large",
        },
        0 => "neutral",
        - => match magnitude {
            0         => "neg-zero",
            n if n < 5 => "neg-small",
            _         => "neg-large",
        },
    }
}

fn test_match_inside_tif() {
    check_str("match-tif: pos-zero",   classify_with_detail(+, 0),  "pos-zero");
    check_str("match-tif: pos-small",  classify_with_detail(+, 3),  "pos-small");
    check_str("match-tif: pos-large",  classify_with_detail(+, 10), "pos-large");
    check_str("match-tif: neutral",    classify_with_detail(0, 5),  "neutral");
    check_str("match-tif: neg-zero",   classify_with_detail(-, 0),  "neg-zero");
    check_str("match-tif: neg-small",  classify_with_detail(-, 2),  "neg-small");
    check_str("match-tif: neg-large",  classify_with_detail(-, 99), "neg-large");
}

// ---------------------------------------------------------------------------
// Loop with tand/tor in condition
// ---------------------------------------------------------------------------

fn test_tand_in_loop_condition() {
    // Iterate while both conditions are "positive" in ternary sense
    let mut i: int = 0;
    let mut count: int = 0;
    while i < 10 {
        let a: trit = if i < 5 { + } else { - };
        let b: trit = if i % 2 == 0 { + } else { 0 };
        let combined = a tand b;
        // combined is + only when both a=+ and b=+
        // that happens at i=0, i=2, i=4 (a=+, b=+ for even i<5)
        if trit_val(combined) == 1 {
            count = count + 1;
        }
        i = i + 1;
    }
    check_int("tand-loop: positive count=3", count, 3);
}

fn test_tor_in_loop_condition() {
    // Count how many iterations have either signal positive
    let mut i: int = 0;
    let mut count: int = 0;
    while i < 9 {
        let a: trit = if i < 3 { + } elif i < 6 { 0 } else { - };
        let b: trit = if i % 3 == 0 { + } elif i % 3 == 1 { 0 } else { - };
        let combined = a tor b;
        if trit_val(combined) == 1 {
            count = count + 1;
        }
        i = i + 1;
    }
    // i=0: +tor+=+, i=1: +tor0=+, i=2: +tor-=+, i=3: 0tor+=+, i=4: 0tor0=0
    // i=5: 0tor-=0, i=6: -tor+=+, i=7: -tor0=0, i=8: -tor-=-
    // positive: i=0,1,2,3,6 => count=5
    check_int("tor-loop: positive count=5", count, 5);
}

// ---------------------------------------------------------------------------
// break with ternary condition
// ---------------------------------------------------------------------------

fn test_break_on_negative_trit() {
    let vals: Vec<int> = Vec::new();
    vals.push(1); vals.push(0); vals.push(1); vals.push(-1); vals.push(1);

    let mut last_index: int = -1;
    let mut i: int = 0;
    while i < 5 {
        let t: trit = vals.get(i) as trit;
        tif t {
            + => { last_index = i; },
            0 => { last_index = i; },
            - => { break; },
        }
        i = i + 1;
    }
    // Should break at index 3 (first negative), last_index was set to 2
    check_int("break-trit: stopped at index 2", last_index, 2);
}

// ---------------------------------------------------------------------------
// continue with ternary condition (skip zero trits)
// ---------------------------------------------------------------------------

fn test_continue_on_zero_trit() {
    let vals: Vec<int> = Vec::new();
    vals.push(1); vals.push(0); vals.push(-1); vals.push(0); vals.push(1);

    let mut sum: int = 0;
    for vi in vals {
        let t: trit = vi as trit;
        if trit_val(t) == 0 {
            continue;
        }
        sum = sum + trit_val(t);
    }
    // 1 + (-1) + 1 = 1 (skipping zeros)
    check_int("continue-trit: sum skipping zeros=1", sum, 1);
}

// ---------------------------------------------------------------------------
// Recursion with tif branching
// ---------------------------------------------------------------------------

fn ternary_factorial(n: int, sign: trit) -> int {
    // Recursive: if sign is +, compute n!, if -, return -n!, if 0, return 0
    tif sign {
        + => {
            if n <= 1 { 1 } else { n * ternary_factorial(n - 1, +) }
        },
        0 => 0,
        - => {
            if n <= 1 { -1 } else { -(n * ternary_factorial(n - 1, +)) }
        },
    }
}

fn test_ternary_recursion() {
    check_int("trec: 5! with +=120",   ternary_factorial(5, +),  120);
    check_int("trec: 5! with 0=0",     ternary_factorial(5, 0),  0);
    check_int("trec: 5! with -=-120",  ternary_factorial(5, -), -120);
    check_int("trec: 1! with +=1",     ternary_factorial(1, +),  1);
    check_int("trec: 0! with +=1",     ternary_factorial(0, +),  1);
}

// ---------------------------------------------------------------------------
// Recursive ternary search (trit-directed)
// ---------------------------------------------------------------------------

fn trit_search(vals: Vec<int>, target: int, lo: int, hi: int) -> int {
    if lo > hi { return -1; }
    let mid = lo + (hi - lo) / 2;
    let v = vals.get(mid);
    if v == target { return mid; }
    if v < target { trit_search(vals, target, mid + 1, hi) }
    else { trit_search(vals, target, lo, mid - 1) }
}

fn test_recursive_search() {
    let mut sorted: Vec<int> = Vec::new();
    sorted.push(-9); sorted.push(-3); sorted.push(0);
    sorted.push(3); sorted.push(9); sorted.push(27);

    check_int("tsearch: find 0 at idx 2",     trit_search(sorted, 0, 0, 5),  2);
    check_int("tsearch: find 27 at idx 5",    trit_search(sorted, 27, 0, 5), 5);
    check_int("tsearch: find -9 at idx 0",    trit_search(sorted, -9, 0, 5), 0);
    check_int("tsearch: find 99 = not found", trit_search(sorted, 99, 0, 5), -1);
}

// ---------------------------------------------------------------------------
// Nested loop with tif exit condition
// ---------------------------------------------------------------------------

fn test_nested_loop_tif_exit() {
    let mut found: int = 0;
    for i in 0..3 {
        for j in 0..3 {
            let t: trit = if i == 1 && j == 2 { - } else { + };
            tif t {
                + => { found = found + 1; },
                0 => {},
                - => { found = found * 10; },
            }
        }
    }
    // 9 iterations total. At i=1,j=2 we multiply by 10 instead of adding.
    // Sequence: +1,+1,+1,+1,+1 (5 adds = 5), then *10 at iteration 6 = 50
    // then +1,+1,+1 (3 more) = 53
    check_int("nested-loop-tif: found=53", found, 53);
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() {
    io::println("=== 25 Nested Control Flow ===");

    io::println("-- nested tif 3 levels --");
    test_nested_tif_3_levels();

    io::println("-- tif in while --");
    test_tif_in_while();

    io::println("-- tif in for --");
    test_tif_in_for();

    io::println("-- match inside tif --");
    test_match_inside_tif();

    io::println("-- tand in loop --");
    test_tand_in_loop_condition();

    io::println("-- tor in loop --");
    test_tor_in_loop_condition();

    io::println("-- break on negative trit --");
    test_break_on_negative_trit();

    io::println("-- continue on zero trit --");
    test_continue_on_zero_trit();

    io::println("-- ternary recursion --");
    test_ternary_recursion();

    io::println("-- recursive search --");
    test_recursive_search();

    io::println("-- nested loop tif exit --");
    test_nested_loop_tif_exit();

    io::println("Done.");
}
