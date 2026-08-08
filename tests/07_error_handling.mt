// Test: Result<T,E> type — Ok/Unknown/Err construction, match, chaining
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
// Basic Result construction and matching
// ---------------------------------------------------------------------------

fn safe_divide(a: int, b: int) -> Result<int, str> {
    if b == 0 {
        Err("division by zero")
    } else {
        Ok(a / b)
    }
}

fn safe_sqrt(n: int) -> Result<int, str> {
    if n < 0 {
        Err("negative input")
    } else {
        // Integer square root (floor)
        let mut i: int = 0;
        while (i + 1) * (i + 1) <= n {
            i = i + 1;
        }
        Ok(i)
    }
}

fn pending_computation() -> Result<int, str> {
    Unknown("not yet computed")
}

fn test_result_ok() {
    let r = safe_divide(10, 2);
    match r {
        Ok(v)       => check_int("result-ok: 10/2=5",      v, 5),
        Unknown(m)  => fail("result-ok: got Unknown"),
        Err(e)      => fail("result-ok: got Err"),
    }
}

fn test_result_err() {
    let r = safe_divide(10, 0);
    match r {
        Ok(v)       => fail("result-err: got Ok"),
        Unknown(m)  => fail("result-err: got Unknown"),
        Err(e)      => pass("result-err: div-by-zero"),
    }
}

fn test_result_unknown() {
    let r = pending_computation();
    match r {
        Ok(v)       => fail("result-unk: got Ok"),
        Unknown(m)  => pass("result-unk: pending"),
        Err(e)      => fail("result-unk: got Err"),
    }
}

// ---------------------------------------------------------------------------
// Match on Result value
// ---------------------------------------------------------------------------

fn result_or_default(r: Result<int, str>, default: int) -> int {
    match r {
        Ok(v)      => v,
        Unknown(_) => default,
        Err(_)     => default,
    }
}

fn test_result_or_default() {
    check_int("result-or: Ok(42)",     result_or_default(Ok(42), 0),       42);
    check_int("result-or: Unknown→0",  result_or_default(Unknown("x"), 0), 0);
    check_int("result-or: Err→-1",     result_or_default(Err("e"), -1),   -1);
}

// ---------------------------------------------------------------------------
// Chaining / cascaded operations
// ---------------------------------------------------------------------------

fn add_results(a: Result<int, str>, b: Result<int, str>) -> Result<int, str> {
    match a {
        Ok(va) => match b {
            Ok(vb) => Ok(va + vb),
            Unknown(m) => Unknown(m),
            Err(e) => Err(e),
        },
        Unknown(m) => Unknown(m),
        Err(e) => Err(e),
    }
}

fn test_result_chain() {
    let a = Ok(3);
    let b = Ok(4);
    let c = add_results(a, b);
    match c {
        Ok(v) => check_int("chain: Ok(3)+Ok(4)=Ok(7)", v, 7),
        _     => fail("chain: Ok+Ok"),
    }

    let d = Ok(5);
    let e: Result<int, str> = Err("boom");
    let f = add_results(d, e);
    match f {
        Err(_) => pass("chain: Ok+Err=Err"),
        _      => fail("chain: Ok+Err"),
    }

    let g: Result<int, str> = Unknown("pending");
    let h = Ok(1);
    let i = add_results(g, h);
    match i {
        Unknown(_) => pass("chain: Unk+Ok=Unk"),
        _          => fail("chain: Unk+Ok"),
    }
}

// ---------------------------------------------------------------------------
// Safe sqrt chain
// ---------------------------------------------------------------------------

fn test_safe_sqrt() {
    match safe_sqrt(9) {
        Ok(v)      => check_int("sqrt: sqrt(9)=3", v, 3),
        Unknown(_) => fail("sqrt: unknown"),
        Err(_)     => fail("sqrt: err"),
    }
    match safe_sqrt(0) {
        Ok(v)      => check_int("sqrt: sqrt(0)=0", v, 0),
        Unknown(_) => fail("sqrt: unknown"),
        Err(_)     => fail("sqrt: err"),
    }
    match safe_sqrt(-1) {
        Ok(_)      => fail("sqrt: neg should be err"),
        Unknown(_) => fail("sqrt: neg should be err"),
        Err(e)     => pass("sqrt: sqrt(-1)=Err"),
    }
}

// ---------------------------------------------------------------------------
// Result in conditional
// ---------------------------------------------------------------------------

fn is_ok(r: Result<int, str>) -> bool {
    match r {
        Ok(_)      => true,
        Unknown(_) => false,
        Err(_)     => false,
    }
}

fn is_err(r: Result<int, str>) -> bool {
    match r {
        Ok(_)      => false,
        Unknown(_) => false,
        Err(_)     => true,
    }
}

fn is_unknown(r: Result<int, str>) -> bool {
    match r {
        Ok(_)      => false,
        Unknown(_) => true,
        Err(_)     => false,
    }
}

fn test_result_predicates() {
    check("is_ok(Ok(1))",           is_ok(Ok(1)));
    check("!is_ok(Err)",            !is_ok(Err("e")));
    check("!is_ok(Unknown)",        !is_ok(Unknown("u")));
    check("is_err(Err)",            is_err(Err("e")));
    check("!is_err(Ok)",            !is_err(Ok(1)));
    check("is_unknown(Unknown)",    is_unknown(Unknown("u")));
    check("!is_unknown(Ok)",        !is_unknown(Ok(1)));
}

// ---------------------------------------------------------------------------
// Multiple error types / messages
// ---------------------------------------------------------------------------

fn parse_positive(s: str) -> Result<int, str> {
    // Simulate: if s is "42" return Ok(42), else error
    if s == "42"  { Ok(42) }
    elif s == "0" { Ok(0) }
    elif s == ""  { Err("empty input") }
    else          { Err("not a positive integer") }
}

fn test_parse() {
    match parse_positive("42") {
        Ok(v)  => check_int("parse: 42=Ok(42)", v, 42),
        _      => fail("parse: 42"),
    }
    match parse_positive("0") {
        Ok(v)  => check_int("parse: 0=Ok(0)", v, 0),
        _      => fail("parse: 0"),
    }
    match parse_positive("") {
        Err(e) => pass("parse: empty=Err"),
        _      => fail("parse: empty"),
    }
    match parse_positive("abc") {
        Err(e) => pass("parse: abc=Err"),
        _      => fail("parse: abc"),
    }
}

// ---------------------------------------------------------------------------
// Nested Result matching
// ---------------------------------------------------------------------------

fn compute(a: int, b: int, c: int) -> Result<int, str> {
    match safe_divide(a, b) {
        Ok(ab) => match safe_divide(ab, c) {
            Ok(abc) => Ok(abc),
            Err(e)  => Err(e),
            Unknown(m) => Unknown(m),
        },
        Err(e)  => Err(e),
        Unknown(m) => Unknown(m),
    }
}

fn test_nested_result() {
    match compute(100, 5, 4) {
        Ok(v) => check_int("nested: 100/5/4=5", v, 5),
        _     => fail("nested: 100/5/4"),
    }
    match compute(100, 0, 4) {
        Err(_) => pass("nested: div-by-zero"),
        _      => fail("nested: div-by-zero"),
    }
    match compute(100, 5, 0) {
        Err(_) => pass("nested: second div-by-zero"),
        _      => fail("nested: second div-by-zero"),
    }
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() {
    io::println("=== 07 Error Handling ===");

    io::println("-- basic Result --");
    test_result_ok();
    test_result_err();
    test_result_unknown();

    io::println("-- result_or_default --");
    test_result_or_default();

    io::println("-- chaining --");
    test_result_chain();

    io::println("-- safe_sqrt --");
    test_safe_sqrt();

    io::println("-- predicates --");
    test_result_predicates();

    io::println("-- parse --");
    test_parse();

    io::println("-- nested --");
    test_nested_result();

    io::println("Done.");
}
