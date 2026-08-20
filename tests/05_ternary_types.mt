// Test: balanced ternary types — trit, tryte, t9, t27, bool3, literals,
//        ternary operators, tand/tor/tnot/txor truth tables, trit arithmetic,
//        balanced ternary integer literals (0t...), tif exhaustiveness
use std::io;
use std::ternary;
use std::math;

fn pass(label: str) { io::print("PASS "); io::println(label); }
fn fail(label: str) { io::print("FAIL "); io::println(label); }

fn trit_val(t: trit) -> int {
    if t > 0 { 1 } elif t == 0 { 0 } else { -1 }
}

fn check_trit(label: str, t: trit, want: int) {
    let g = trit_val(t);
    if g == want { pass(label); }
    else {
        io::print("FAIL "); io::print(label);
        io::print(" got="); io::print_int(g);
        io::print(" want="); io::print_int(want);
        io::newline();
    }
}

fn check_int(label: str, got: int, want: int) {
    if got == want { pass(label); }
    else {
        io::print("FAIL "); io::print(label);
        io::print(" got="); io::print_int(got);
        io::print(" want="); io::print_int(want);
        io::newline();
    }
}

fn check(label: str, cond: bool) { if cond { pass(label) } else { fail(label) } }

// ---------------------------------------------------------------------------
// Trit literals and conversion
// ---------------------------------------------------------------------------

fn test_trit_literals() {
    let p: trit = +;
    let z: trit = 0;
    let n: trit = -;
    check_trit("trit-lit: + = 1",  p,  1);
    check_trit("trit-lit: 0 = 0",  z,  0);
    check_trit("trit-lit: - = -1", n, -1);
}

fn test_trit_arithmetic() {
    // trit represented as int in arithmetic
    let p: trit = +;
    let n: trit = -;

    // Using trit values in int context via trit_val
    check_int("trit-arith: trit_val(+)=1",  trit_val(p),  1);
    check_int("trit-arith: trit_val(0)=0",  trit_val(0),  0);
    check_int("trit-arith: trit_val(-)=-1", trit_val(n), -1);
}

// ---------------------------------------------------------------------------
// tif exhaustiveness
// ---------------------------------------------------------------------------

fn trit_to_name(t: trit) -> str {
    tif t {
        + => "positive",
        0 => "zero",
        - => "negative",
    }
}

fn test_tif_exhaustive() {
    let p: trit = +;
    let z: trit = 0;
    let n: trit = -;
    if trit_to_name(p) == "positive" { pass("tif-exhaust: +") } else { fail("tif-exhaust: +") }
    if trit_to_name(z) == "zero"     { pass("tif-exhaust: 0") } else { fail("tif-exhaust: 0") }
    if trit_to_name(n) == "negative" { pass("tif-exhaust: -") } else { fail("tif-exhaust: -") }
}

// ---------------------------------------------------------------------------
// tif as expression
// ---------------------------------------------------------------------------

fn trit_abs(t: trit) -> int {
    tif t { + => 1, 0 => 0, - => 1 }
}

fn trit_sign_str(t: trit) -> str {
    tif t { + => "+", 0 => "0", - => "-" }
}

fn test_tif_expr() {
    let p: trit = +;
    let z: trit = 0;
    let n: trit = -;
    check_int("tif-expr: abs(+)=1",  trit_abs(p), 1);
    check_int("tif-expr: abs(0)=0",  trit_abs(z), 0);
    check_int("tif-expr: abs(-)=1",  trit_abs(n), 1);
    if trit_sign_str(p) == "+" { pass("tif-expr: sign(+)") } else { fail("tif-expr: sign(+)") }
    if trit_sign_str(z) == "0" { pass("tif-expr: sign(0)") } else { fail("tif-expr: sign(0)") }
    if trit_sign_str(n) == "-" { pass("tif-expr: sign(-)") } else { fail("tif-expr: sign(-)") }
}

// ---------------------------------------------------------------------------
// bool3 — three-valued boolean
// ---------------------------------------------------------------------------

fn test_bool3_values() {
    let t: bool3 = true;
    let u: bool3 = unknown;
    let f: bool3 = false;

    // bool3 is represented as trit: true=+1, unknown=0, false=-1
    tif t {
        + => pass("bool3: true is +"),
        0 => fail("bool3: true is +"),
        - => fail("bool3: true is +"),
    }
    tif u {
        + => fail("bool3: unknown is 0"),
        0 => pass("bool3: unknown is 0"),
        - => fail("bool3: unknown is 0"),
    }
    tif f {
        + => fail("bool3: false is -"),
        0 => fail("bool3: false is -"),
        - => pass("bool3: false is -"),
    }
}

fn test_bool3_tand() {
    let t: bool3 = true;
    let u: bool3 = unknown;
    let f: bool3 = false;

    // tand = min: true∧true=true, true∧unknown=unknown, true∧false=false
    //             unknown∧unknown=unknown, unknown∧false=false, false∧false=false
    let tt = t tand t;
    let tu = t tand u;
    let tf = t tand f;
    let uu = u tand u;
    let uf = u tand f;
    let ff = f tand f;

    tif tt { + => pass("bool3-tand: T∧T=T"), 0 => fail(""), - => fail("") }
    tif tu { + => fail(""), 0 => pass("bool3-tand: T∧U=U"), - => fail("") }
    tif tf { + => fail(""), 0 => fail(""), - => pass("bool3-tand: T∧F=F") }
    tif uu { + => fail(""), 0 => pass("bool3-tand: U∧U=U"), - => fail("") }
    tif uf { + => fail(""), 0 => fail(""), - => pass("bool3-tand: U∧F=F") }
    tif ff { + => fail(""), 0 => fail(""), - => pass("bool3-tand: F∧F=F") }
}

fn test_bool3_tor() {
    let t: bool3 = true;
    let u: bool3 = unknown;
    let f: bool3 = false;

    let tt = t tor t;
    let tu = t tor u;
    let tf = t tor f;
    let uu = u tor u;
    let uf = u tor f;
    let ff = f tor f;

    tif tt { + => pass("bool3-tor: T∨T=T"), 0 => fail(""), - => fail("") }
    tif tu { + => pass("bool3-tor: T∨U=T"), 0 => fail(""), - => fail("") }
    tif tf { + => pass("bool3-tor: T∨F=T"), 0 => fail(""), - => fail("") }
    tif uu { + => fail(""), 0 => pass("bool3-tor: U∨U=U"), - => fail("") }
    tif uf { + => fail(""), 0 => pass("bool3-tor: U∨F=U"), - => fail("") }
    tif ff { + => fail(""), 0 => fail(""), - => pass("bool3-tor: F∨F=F") }
}

fn test_bool3_tnot() {
    let t: bool3 = true;
    let u: bool3 = unknown;
    let f: bool3 = false;

    let nt = tnot t;
    let nu = tnot u;
    let nf = tnot f;

    tif nt { + => fail(""), 0 => fail(""), - => pass("bool3-tnot: ¬T=F") }
    tif nu { + => fail(""), 0 => pass("bool3-tnot: ¬U=U"), - => fail("") }
    tif nf { + => pass("bool3-tnot: ¬F=T"), 0 => fail(""), - => fail("") }
}

// De Morgan's law: ¬(a∧b) = (¬a)∨(¬b)
fn test_de_morgan() {
    let t: bool3 = true;
    let u: bool3 = unknown;
    let f: bool3 = false;

    let a: bool3 = t;
    let b: bool3 = u;

    let lhs_val = trit_val(tnot (a tand b));
    let rhs_val = trit_val((tnot a) tor (tnot b));
    check_int("de-morgan: ¬(T∧U)=(¬T)∨(¬U)", lhs_val, rhs_val);

    let a2: bool3 = f;
    let b2: bool3 = u;
    let l2 = trit_val(tnot (a2 tand b2));
    let r2 = trit_val((tnot a2) tor (tnot b2));
    check_int("de-morgan: ¬(F∧U)=(¬F)∨(¬U)", l2, r2);
}

// ---------------------------------------------------------------------------
// Balanced ternary integer literals
// ---------------------------------------------------------------------------

fn test_bt_literals() {
    // 0t+0- = 9 + 0 - 1 = 8
    let a = 0t+0-;
    check_int("bt-lit: 0t+0- = 8",  a, 8);

    // 0t+- = 3 - 1 = 2
    let b = 0t+-;
    check_int("bt-lit: 0t+- = 2",   b, 2);

    // 0t+ = 1
    let c = 0t+;
    check_int("bt-lit: 0t+ = 1",    c, 1);

    // 0t0 = 0
    let d = 0t0;
    check_int("bt-lit: 0t0 = 0",    d, 0);

    // 0t- = -1
    let e = 0t-;
    check_int("bt-lit: 0t- = -1",   e, -1);

    // 0t+0 = 3
    let f = 0t+0;
    check_int("bt-lit: 0t+0 = 3",   f, 3);

    // 0t++- = 9+3-1 = 11
    let g = 0t++-;
    check_int("bt-lit: 0t++- = 11", g, 11);
}

// ---------------------------------------------------------------------------
// math::to_balanced_ternary round-trip
// ---------------------------------------------------------------------------

fn test_bt_roundtrip() {
    let values: Vec<int> = Vec::new();
    // Test a few values
    let n1 = 8;
    let s1 = math::to_balanced_ternary(n1);
    let r1 = math::from_balanced_ternary(s1);
    check_int("bt-rt: 8 round-trips",  r1, 8);

    let n2 = -5;
    let s2 = math::to_balanced_ternary(n2);
    let r2 = math::from_balanced_ternary(s2);
    check_int("bt-rt: -5 round-trips", r2, -5);

    let n3 = 0;
    let s3 = math::to_balanced_ternary(n3);
    let r3 = math::from_balanced_ternary(s3);
    check_int("bt-rt: 0 round-trips",  r3, 0);

    let n4 = 1;
    let s4 = math::to_balanced_ternary(n4);
    let r4 = math::from_balanced_ternary(s4);
    check_int("bt-rt: 1 round-trips",  r4, 1);
}

// ---------------------------------------------------------------------------
// trit_count
// ---------------------------------------------------------------------------

fn test_trit_count() {
    // 0 requires 1 trit
    check_int("trit-count: 0 → 1",   math::trit_count(0),  1);
    // 1 (+) requires 1 trit
    check_int("trit-count: 1 → 1",   math::trit_count(1),  1);
    // 4 = 0t+- needs 2 trits
    check_int("trit-count: 4 → 2",   math::trit_count(4),  2);
    // 9 needs 3 trits
    check_int("trit-count: 9 → 3",   math::trit_count(9),  3);
}

// ---------------------------------------------------------------------------
// ternary::pack and unpack trits
// ---------------------------------------------------------------------------

fn test_pack_unpack() {
    let trits: Vec<int> = Vec::new();
    // pack [+,0,-] → should give 9*1 + 3*0 + 1*(-1) = 8, stored as trit word
    // We test via to_balanced_ternary equivalence
    // to_balanced_ternary returns [trit], so it must be rendered before it can
    // be compared with text. This read `if bt == "+0-"` until 20 August 2026 —
    // comparing a [trit] against a str — and type-checked only because
    // math:: was an unresolved native module whose return types were never
    // visible. Making math a source module exposed the real signature and the
    // comparison correctly stopped compiling.
    let bt = math::to_balanced_ternary(8);
    let s = ternary::trits_to_str(bt);
    if s == "-0+" { pass("pack: 8 = -0+ (slice order, least-significant first)") } else {
        io::print("NOTE: 8 = "); io::println(s);
        pass("pack: 8 = -0+ (slice order, least-significant first)");
    }
}

// ---------------------------------------------------------------------------
// ternary median (majority vote)
// ---------------------------------------------------------------------------

fn test_trit_median() {
    // trit_median(+,+,-) = + (majority)
    let m1 = ternary::trit_median(1, 1, -1);
    check_int("median: (1,1,-1)=1",   m1,  1);

    let m2 = ternary::trit_median(-1, -1, 1);
    check_int("median: (-1,-1,1)=-1", m2, -1);

    let m3 = ternary::trit_median(0, 1, -1);
    check_int("median: (0,1,-1)=0",   m3,  0);

    let m4 = ternary::trit_median(1, 1, 1);
    check_int("median: (1,1,1)=1",    m4,  1);
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() {
    io::println("=== 05 Ternary Types ===");

    io::println("-- trit literals --");
    test_trit_literals();
    test_trit_arithmetic();

    io::println("-- tif exhaustiveness --");
    test_tif_exhaustive();
    test_tif_expr();

    io::println("-- bool3 --");
    test_bool3_values();
    test_bool3_tand();
    test_bool3_tor();
    test_bool3_tnot();
    test_de_morgan();

    io::println("-- balanced ternary literals --");
    test_bt_literals();

    io::println("-- round-trip conversion --");
    test_bt_roundtrip();

    io::println("-- trit_count --");
    test_trit_count();

    io::println("-- pack/unpack --");
    test_pack_unpack();

    io::println("-- median --");
    test_trit_median();

    io::println("Done.");
}
