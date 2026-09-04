// Test: IR-layer regressions — uniform aggregate slot layout (trit fields),
//        struct value semantics on `let b = a`, for-loop variable shadowing,
//        string/float match patterns, float comparison, return inside an
//        if-arm feeding a value binding, or-pattern per-alternative binding
use std::io;

fn pass(label: str) { io::print("PASS "); io::println(label); }
fn fail(label: str) { io::print("FAIL "); io::println(label); }
fn check(label: str, cond: bool) { if cond { pass(label) } else { fail(label) } }
fn check_int(label: str, got: int, want: int) {
    if got == want { pass(label) }
    else {
        io::print("FAIL "); io::print(label);
        io::print(" got="); io::print_int(got);
        io::print(" want="); io::print_int(want);
        io::newline();
    }
}

fn trit_val(t: trit) -> int {
    if t > 0 { 1 } elif t == 0 { 0 } else { -1 }
}

// ---------------------------------------------------------------------------
// Struct with a trit field — storing the trit must not clobber the int
// field, and both fields must read back correctly (uniform 8-byte slots)
// ---------------------------------------------------------------------------

struct Probe { pub big: int, pub sign: trit }

fn test_struct_trit_field() {
    let n: trit = -;
    let p = Probe { big: 300, sign: n };
    check_int("struct-trit: int field intact",  p.big, 300);
    check_int("struct-trit: trit field reads",  trit_val(p.sign), -1);

    let mut q = Probe { big: 7, sign: 0 };
    q.sign = +;
    check_int("struct-trit: after field assign, int intact", q.big, 7);
    check_int("struct-trit: after field assign, trit reads", trit_val(q.sign), 1);
}

// ---------------------------------------------------------------------------
// Struct value semantics: `let b = a` must copy, not alias
// ---------------------------------------------------------------------------

struct Cell { pub x: int, pub y: int }

// `let b = a` inside the callee must copy the struct: without the copy, b
// would alias the caller's struct (passed as a pointer) and b.x = ... would
// silently mutate the caller's binding.
fn bump(c: Cell) -> int {
    let mut b = c;
    b.x = b.x + 100;
    b.x
}

fn test_struct_copy_semantics() {
    let a = Cell { x: 1, y: 2 };
    check_int("struct-copy: copy sees fields",  bump(a), 101);
    check_int("struct-copy: a.x unchanged",     a.x, 1);
    check_int("struct-copy: a.y unchanged",     a.y, 2);
}

// ---------------------------------------------------------------------------
// `for s in <array of structs>` must bind a copy of the element, not a
// pointer parked in a struct-typed slot. When it stored the pointer, every
// field access read the slot itself: field 0 came back as the struct's own
// address (printed as a string, arbitrary bytes) and field 1 as whatever
// followed it. Indexing the array directly was unaffected, so this survived
// every run that only checked for traps and exit codes.
// ---------------------------------------------------------------------------

struct Labelled { pub name: str, pub tag: bool3, pub n: int }

fn test_for_over_struct_array() {
    let items: [Labelled] = [
        Labelled { name: "alpha", tag: True,    n: 1 },
        Labelled { name: "beta",  tag: Unknown, n: 2 },
        Labelled { name: "gamma", tag: False,   n: 3 },
    ];

    let mut n_total = 0;
    let mut names_ok = true;
    let mut tags_ok = true;
    let mut i = 0;
    for it in items {
        n_total = n_total + it.n;
        // Each field must match what indexing the same element yields.
        if it.n != items[i].n { names_ok = false; }
        if it.tag != items[i].tag { tags_ok = false; }
        i = i + 1;
    }

    check_int("for-struct: int field summed",  n_total, 6);
    check("for-struct: fields match index",    names_ok);
    check("for-struct: bool3 field matches",   tags_ok);

    // A trailing field must not be read out of the neighbouring slot.
    let mut last_n = 0;
    for it in items { last_n = it.n; }
    check_int("for-struct: last element read", last_n, 3);
}

// ---------------------------------------------------------------------------
// For-loop variable must not clobber a same-named outer local
// ---------------------------------------------------------------------------

fn test_for_shadowing() {
    let i = 10;
    let mut total = 0;
    for i in 0..3 {
        total = total + i;
    }
    check_int("for-shadow: range loop ran",     total, 3);
    check_int("for-shadow: outer i restored",   i, 10);

    let v = 100;
    let arr = [1, 2, 3];
    let mut sum = 0;
    for v in arr {
        sum = sum + v;
    }
    check_int("for-shadow: array loop ran",     sum, 6);
    check_int("for-shadow: outer v restored",   v, 100);

    let e = 55;
    let mut vec: Vec<int> = Vec::new();
    vec.push(4);
    vec.push(5);
    let mut vsum = 0;
    for e in vec {
        vsum = vsum + e;
    }
    check_int("for-shadow: vec loop ran",       vsum, 9);
    check_int("for-shadow: outer e restored",   e, 55);
}

// ---------------------------------------------------------------------------
// String literal patterns must match runtime-built strings
// ---------------------------------------------------------------------------

fn classify(s: str) -> int {
    match s {
        "foobar" => 1,
        "foobaz" => 2,
        _        => 0,
    }
}

fn test_string_match() {
    check_int("match-str: concat hits arm 1",  classify("foo".concat("bar")), 1);
    check_int("match-str: concat hits arm 2",  classify("foo".concat("baz")), 2);
    check_int("match-str: wildcard still ok",  classify("other"), 0);
}

// ---------------------------------------------------------------------------
// float comparisons must use float compares, not integer-on-bits
// ---------------------------------------------------------------------------

fn test_float_compare() {
    let a: float = -2.5;
    let b: float = 1.5;
    check("float: -2.5 < 1.5",  a < b);
    check("float: 1.5 > -2.5",  b > a);
    check("float: -2.5 <= -2.5", a <= a);
}

// ---------------------------------------------------------------------------
// return inside an if-arm feeding a value binding
// ---------------------------------------------------------------------------

fn pick(flag: bool) -> int {
    let x = if flag { return 99; } else { 42 };
    x + 1
}

fn test_if_return_arm() {
    check_int("if-ret: else path",   pick(false), 43);
    check_int("if-ret: return path", pick(true),  99);
}

// ---------------------------------------------------------------------------
// Or-patterns bind variables from each alternative's own positions
// ---------------------------------------------------------------------------

fn pair_key(p: Cell) -> int {
    match p {
        Cell { x: 0, y: v } | Cell { x: v, y: _ } => v,
    }
}

fn test_or_pattern_binding() {
    check_int("or-bind: first alt uses y",  pair_key(Cell { x: 0, y: 4 }), 4);
    check_int("or-bind: second alt uses x", pair_key(Cell { x: 3, y: 4 }), 3);
}

// ---------------------------------------------------------------------------
// Unsized array params carry a hidden length: iteration must visit the
// elements (previously the pointer value was used as the loop bound and
// the body saw the index instead of the element)
// ---------------------------------------------------------------------------

fn sum_all(xs: [int]) -> int {
    let mut s = 0;
    for x in xs { s = s + x; }
    s
}

fn forward_sum(xs: [int]) -> int {
    // Forwarding an unsized param must forward its length too.
    sum_all(xs)
}

fn count_pos(ts: [trit]) -> int {
    let mut c = 0;
    for t in ts {
        if t > 0 { c = c + 1; }
    }
    c
}

fn test_unsized_array_params() {
    check_int("unsized: sum of 4 ints",   sum_all([1, 2, 3, 4]), 10);
    check_int("unsized: sum of 2 ints",   sum_all([10, 20]), 30);
    check_int("unsized: forwarded param", forward_sum([5, 6, 7]), 18);
    check_int("unsized: trit elements",   count_pos([+, -, +, 0, +]), 3);

    let nested: [[int]] = [[3, 2, 1, 1], [1, 1, 3, 2]];
    let mut total = 0;
    for row in nested {
        for v in row { total = total + v; }
    }
    check_int("unsized: nested rows", total, 14);
}

fn main() {
    io::println("-- struct trit field --");
    test_struct_trit_field();

    io::println("-- struct copy semantics --");
    test_struct_copy_semantics();

    io::println("-- for over struct array --");
    test_for_over_struct_array();

    io::println("-- for-loop shadowing --");
    test_for_shadowing();

    io::println("-- string match --");
    test_string_match();

    io::println("-- float compare --");
    test_float_compare();

    io::println("-- if with return arm --");
    test_if_return_arm();

    io::println("-- or-pattern binding --");
    test_or_pattern_binding();

    io::println("-- unsized array params --");
    test_unsized_array_params();

    io::println("all ir regression tests done");
}
