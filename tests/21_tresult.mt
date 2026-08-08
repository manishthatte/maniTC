// Test: Result three-armed handling — Ok/Unknown/Err matching, propagation
//        chains, conversion to T3Bool, nested result matching
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
fn check_str(label: str, got: str, want: str) {
    if got == want { pass(label) }
    else {
        io::print("FAIL "); io::print(label);
        io::print(" got=["); io::print(got);
        io::print("] want=["); io::print(want); io::println("]");
    }
}

// ---------------------------------------------------------------------------
// Helper functions returning Result<int, str> in all three arms
// ---------------------------------------------------------------------------

fn safe_divide(a: int, b: int) -> Result<int, str> {
    if b == 0 {
        Err("division by zero")
    } else {
        Ok(a / b)
    }
}

fn maybe_sqrt(n: int) -> Result<int, str> {
    if n < 0 {
        Err("negative")
    } elif n == 0 {
        Unknown("zero input")
    } else {
        let mut i: int = 0;
        while (i + 1) * (i + 1) <= n {
            i = i + 1;
        }
        Ok(i)
    }
}

fn pending_value() -> Result<int, str> {
    Unknown("not yet resolved")
}

// ---------------------------------------------------------------------------
// Basic match branching on Ok
// ---------------------------------------------------------------------------

fn test_tresult_ok() {
    let r = safe_divide(20, 4);
    let got = match r {
        Ok(v)      => v,
        Unknown(h) => -999,
        Err(e)     => -998,
    };
    check_int("match-ok: 20/4=5", got, 5);
}

// ---------------------------------------------------------------------------
// Basic match branching on Err
// ---------------------------------------------------------------------------

fn test_tresult_err() {
    let r = safe_divide(10, 0);
    let got = match r {
        Ok(v)      => "success",
        Unknown(h) => "unknown",
        Err(e)     => "error",
    };
    check_str("match-err: div-by-zero", got, "error");
}

// ---------------------------------------------------------------------------
// Basic match branching on Unknown
// ---------------------------------------------------------------------------

fn test_tresult_unknown() {
    let r = pending_value();
    let got = match r {
        Ok(v)      => 1,
        Unknown(h) => 0,
        Err(e)     => -1,
    };
    check_int("match-unk: pending=0", got, 0);
}

// ---------------------------------------------------------------------------
// Tresult with binding access inside arms
// ---------------------------------------------------------------------------

fn test_tresult_bindings() {
    let r1 = Ok(42);
    let v1 = match r1 {
        Ok(val)     => val * 2,
        Unknown(h)  => -1,
        Err(e)      => -2,
    };
    check_int("match-bind: Ok(42)*2=84", v1, 84);

    let r2: Result<int, str> = Err("oops");
    let v2 = match r2 {
        Ok(val)     => 100,
        Unknown(h)  => 200,
        Err(msg)    => 300,
    };
    check_int("match-bind: Err arm=300", v2, 300);

    let r3: Result<int, str> = Unknown("hmm");
    let v3 = match r3 {
        Ok(val)     => 100,
        Unknown(h)  => 200,
        Err(msg)    => 300,
    };
    check_int("match-bind: Unknown arm=200", v3, 200);
}

// ---------------------------------------------------------------------------
// Tresult with block bodies (multi-statement arms)
// ---------------------------------------------------------------------------

fn test_tresult_blocks() {
    let r = safe_divide(100, 10);
    let got = match r {
        Ok(v) => {
            let doubled = v * 2;
            doubled + 1
        },
        Unknown(h) => { -1 },
        Err(e)     => { -2 },
    };
    check_int("match-block: (100/10)*2+1=21", got, 21);
}

// ---------------------------------------------------------------------------
// Tresult used for propagation (chaining)
// ---------------------------------------------------------------------------

fn chain_divide(a: int, b: int, c: int) -> Result<int, str> {
    let step1 = safe_divide(a, b);
    match step1 {
        Ok(v1) => safe_divide(v1, c),
        Unknown(h) => Unknown(h),
        Err(e) => Err(e),
    }
}

fn test_tresult_chain() {
    // Ok chain: 100/5/4 = 5
    let r1 = chain_divide(100, 5, 4);
    let v1 = match r1 {
        Ok(v)      => v,
        Unknown(h) => -999,
        Err(e)     => -998,
    };
    check_int("match-chain: 100/5/4=5", v1, 5);

    // Err in first step
    let r2 = chain_divide(100, 0, 4);
    let v2 = match r2 {
        Ok(v)      => 1,
        Unknown(h) => 0,
        Err(e)     => -1,
    };
    check_int("match-chain: first-div-zero=-1", v2, -1);

    // Err in second step
    let r3 = chain_divide(100, 5, 0);
    let v3 = match r3 {
        Ok(v)      => 1,
        Unknown(h) => 0,
        Err(e)     => -1,
    };
    check_int("match-chain: second-div-zero=-1", v3, -1);
}

// ---------------------------------------------------------------------------
// Tresult on maybe_sqrt (exercises all three arms)
// ---------------------------------------------------------------------------

fn test_tresult_maybe_sqrt() {
    // Positive input: Ok
    let r1 = maybe_sqrt(16);
    let v1 = match r1 {
        Ok(v)      => v,
        Unknown(h) => -100,
        Err(e)     => -200,
    };
    check_int("match-sqrt: sqrt(16)=4", v1, 4);

    // Zero input: Unknown
    let r2 = maybe_sqrt(0);
    let v2 = match r2 {
        Ok(v)      => 1,
        Unknown(h) => 0,
        Err(e)     => -1,
    };
    check_int("match-sqrt: sqrt(0)=Unknown", v2, 0);

    // Negative input: Err
    let r3 = maybe_sqrt(-5);
    let v3 = match r3 {
        Ok(v)      => 1,
        Unknown(h) => 0,
        Err(e)     => -1,
    };
    check_int("match-sqrt: sqrt(-5)=Err", v3, -1);
}

// ---------------------------------------------------------------------------
// Conversion: Result arm -> T3Bool (trit mapping)
// ---------------------------------------------------------------------------

fn result_to_t3bool(r: Result<int, str>) -> T3Bool {
    match r {
        Ok(v)      => true,
        Unknown(h) => unknown,
        Err(e)     => false,
    }
}

fn test_tresult_to_t3bool() {
    let b1 = result_to_t3bool(Ok(42));
    tif b1 { + => pass("match-t3bool: Ok=true"), 0 => fail("match-t3bool: Ok"), - => fail("match-t3bool: Ok") }

    let b2 = result_to_t3bool(Unknown("pending"));
    tif b2 { + => fail("match-t3bool: Unk"), 0 => pass("match-t3bool: Unknown=unknown"), - => fail("match-t3bool: Unk") }

    let b3 = result_to_t3bool(Err("fail"));
    tif b3 { + => fail("match-t3bool: Err"), 0 => fail("match-t3bool: Err"), - => pass("match-t3bool: Err=false") }
}

// ---------------------------------------------------------------------------
// T3Bool trit logic on converted results
// ---------------------------------------------------------------------------

fn test_t3bool_logic_on_results() {
    let ok_bool = result_to_t3bool(Ok(1));
    let unk_bool = result_to_t3bool(Unknown("x"));
    let err_bool = result_to_t3bool(Err("y"));

    // true tand unknown = unknown (0)
    let a = ok_bool tand unk_bool;
    tif a { + => fail("t3bool-logic: tand"), 0 => pass("t3bool-logic: true tand unknown=unknown"), - => fail("t3bool-logic: tand") }

    // true tor false = true (+)
    let b = ok_bool tor err_bool;
    tif b { + => pass("t3bool-logic: true tor false=true"), 0 => fail("t3bool-logic: tor"), - => fail("t3bool-logic: tor") }

    // tnot true = false
    let c = tnot ok_bool;
    tif c { + => fail("t3bool-logic: tnot"), 0 => fail("t3bool-logic: tnot"), - => pass("t3bool-logic: tnot true=false") }
}

// ---------------------------------------------------------------------------
// Nested match
// ---------------------------------------------------------------------------

fn nested_operation(a: int, b: int, c: int) -> int {
    let r1 = safe_divide(a, b);
    match r1 {
        Ok(v1) => {
            let r2 = maybe_sqrt(v1);
            match r2 {
                Ok(v2)      => v2 + c,
                Unknown(h)  => c,
                Err(e)      => -1,
            }
        },
        Unknown(h) => -2,
        Err(e)     => -3,
    }
}

fn test_nested_match() {
    // 100/4 = 25, sqrt(25)=5, 5+10=15
    check_int("nested-match: 100/4 sqrt+10=15", nested_operation(100, 4, 10), 15);

    // 100/0 -> Err -> -3
    check_int("nested-match: div-zero=-3", nested_operation(100, 0, 10), -3);

    // 0/1 = 0, sqrt(0)=Unknown -> c=10
    check_int("nested-match: sqrt(0)=c=10", nested_operation(0, 1, 10), 10);
}

// ---------------------------------------------------------------------------
// Result predicate helpers via match
// ---------------------------------------------------------------------------

fn is_ok(r: Result<int, str>) -> bool {
    match r {
        Ok(v)      => true,
        Unknown(h) => false,
        Err(e)     => false,
    }
}

fn is_err(r: Result<int, str>) -> bool {
    match r {
        Ok(v)      => false,
        Unknown(h) => false,
        Err(e)     => true,
    }
}

fn is_unknown(r: Result<int, str>) -> bool {
    match r {
        Ok(v)      => false,
        Unknown(h) => true,
        Err(e)     => false,
    }
}

fn test_tresult_predicates() {
    check("pred: is_ok(Ok(1))",         is_ok(Ok(1)));
    check("pred: !is_ok(Err)",          !is_ok(Err("e")));
    check("pred: !is_ok(Unknown)",      !is_ok(Unknown("u")));
    check("pred: is_err(Err)",          is_err(Err("e")));
    check("pred: !is_err(Ok)",          !is_err(Ok(1)));
    check("pred: is_unknown(Unknown)",  is_unknown(Unknown("u")));
    check("pred: !is_unknown(Ok)",      !is_unknown(Ok(1)));
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() {
    io::println("=== 21 Tresult ===");

    io::println("-- match Ok --");
    test_tresult_ok();

    io::println("-- match Err --");
    test_tresult_err();

    io::println("-- match Unknown --");
    test_tresult_unknown();

    io::println("-- match bindings --");
    test_tresult_bindings();

    io::println("-- match blocks --");
    test_tresult_blocks();

    io::println("-- match chain --");
    test_tresult_chain();

    io::println("-- match maybe_sqrt --");
    test_tresult_maybe_sqrt();

    io::println("-- match to T3Bool --");
    test_tresult_to_t3bool();

    io::println("-- T3Bool logic on results --");
    test_t3bool_logic_on_results();

    io::println("-- nested match --");
    test_nested_match();

    io::println("-- match predicates --");
    test_tresult_predicates();

    io::println("Done.");
}
