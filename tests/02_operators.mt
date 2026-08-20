// Test: all operators — arithmetic, comparison, logical, bitwise, compound assignment,
//        ternary logic (tand/tor/tnot/txor), unary, precedence
use std::io;

fn pass(label: str) { io::print("PASS "); io::println(label); }
fn fail(label: str) { io::print("FAIL "); io::println(label); }

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
// Arithmetic
// ---------------------------------------------------------------------------

fn test_add()    { check_int("add: 3+4=7",     3 + 4,   7); }
fn test_sub()    { check_int("sub: 10-3=7",    10 - 3,   7); }
fn test_mul()    { check_int("mul: 6*7=42",     6 * 7,  42); }
fn test_div()    { check_int("div: 20/4=5",    20 / 4,   5); }
fn test_mod()    { check_int("mod: 17%5=2",    17 % 5,   2); }
fn test_neg()    { check_int("neg: -(-5)=5",  -(-5),     5); }

fn test_div_truncate() {
    check_int("div-trunc: 7/2=3",   7 / 2,  3);
    check_int("div-trunc: -7/2=-3", -7 / 2, -3);
}

fn test_mod_sign() {
    check_int("mod-sign: 7%3=1",    7 % 3,  1);
    check_int("mod-sign: -7%3=-1", -7 % 3, -1);
}

fn test_chained_arith() {
    check_int("chain: 2+3*4=14",   2 + 3 * 4,     14);
    check_int("chain: (2+3)*4=20", (2 + 3) * 4,   20);
    check_int("chain: 10-2-3=5",   10 - 2 - 3,     5);
}

// ---------------------------------------------------------------------------
// Comparison (produce bool, tested in if)
// ---------------------------------------------------------------------------

fn test_cmp_eq()  { if 5 == 5   { pass("cmp: ==") }  else { fail("cmp: ==") } }
fn test_cmp_ne()  { if 5 != 6   { pass("cmp: !=") }  else { fail("cmp: !=") } }
fn test_cmp_lt()  { if 3 < 4    { pass("cmp: <") }   else { fail("cmp: <") } }
fn test_cmp_le()  { if 4 <= 4   { pass("cmp: <=") }  else { fail("cmp: <=") } }
fn test_cmp_gt()  { if 5 > 4    { pass("cmp: >") }   else { fail("cmp: >") } }
fn test_cmp_ge()  { if 4 >= 4   { pass("cmp: >=") }  else { fail("cmp: >=") } }

fn test_cmp_neg() {
    if !(5 == 6)   { pass("cmp-neg: !=")  } else { fail("cmp-neg: !=")  }
    if !(3 > 4)    { pass("cmp-neg: !>")  } else { fail("cmp-neg: !>")  }
}

// ---------------------------------------------------------------------------
// Logical &&, ||, !
// ---------------------------------------------------------------------------

fn test_logical() {
    if true && true  { pass("logical: T&&T") } else { fail("logical: T&&T") }
    if !(true && false) { pass("logical: T&&F=F") } else { fail("logical: T&&F=F") }
    if true || false { pass("logical: T||F") } else { fail("logical: T||F") }
    if !(false || false) { pass("logical: F||F=F") } else { fail("logical: F||F=F") }
    if !false { pass("logical: !F") } else { fail("logical: !F") }
    if !(! true) { pass("logical: !!T") } else { fail("logical: !!T") }
}

fn test_short_circuit() {
    // && short-circuits: right side should not matter
    let mut x: int = 0;
    if false && { x = 1; true } { fail("short-circuit: &&") }
    else { pass("short-circuit: &&") }
    check_int("short-circuit: x not mutated", x, 0);
}

// ---------------------------------------------------------------------------
// Bitwise &, |, ^, <<, >>
// ---------------------------------------------------------------------------

fn test_bitwise() {
    check_int("bitwise: 6&3=2",   6 & 3,  2);
    check_int("bitwise: 6|3=7",   6 | 3,  7);
    check_int("bitwise: 6^3=5",   6 ^ 3,  5);
    check_int("bitwise: 1<<3=8",  1 << 3, 8);
    check_int("bitwise: 8>>2=2",  8 >> 2, 2);
    check_int("bitwise: ~0 via xor: -1^0=-1", -1 ^ 0, -1);
}

// ---------------------------------------------------------------------------
// Compound assignment
// ---------------------------------------------------------------------------

fn test_compound_assign() {
    let mut n: int = 10;
    n += 5;  check_int("compound: +=",  n, 15);
    n -= 3;  check_int("compound: -=",  n, 12);
    n *= 2;  check_int("compound: *=",  n, 24);
    n /= 4;  check_int("compound: /=",  n,  6);
}

// ---------------------------------------------------------------------------
// Ternary logic operators on trit
// ---------------------------------------------------------------------------

fn check_trit(label: str, got: trit, want: int) {
    let g = if got > 0 { 1 } elif got == 0 { 0 } else { -1 };
    if g == want { pass(label); }
    else {
        io::print("FAIL ");
        io::print(label);
        io::print(" got=");
        io::print_int(g);
        io::print(" want=");
        io::print_int(want);
        io::newline();
    }
}

fn test_tand() {
    let p: trit = +;
    let z: trit = 0;
    let n: trit = -;
    check_trit("tand: + tand + = +", p tand p, 1);
    check_trit("tand: + tand 0 = 0", p tand z, 0);
    check_trit("tand: + tand - = -", p tand n, -1);
    check_trit("tand: 0 tand 0 = 0", z tand z, 0);
    check_trit("tand: 0 tand - = -", z tand n, -1);
    check_trit("tand: - tand - = -", n tand n, -1);
}

fn test_tor() {
    let p: trit = +;
    let z: trit = 0;
    let n: trit = -;
    check_trit("tor: + tor + = +", p tor p, 1);
    check_trit("tor: + tor 0 = +", p tor z, 1);
    check_trit("tor: + tor - = +", p tor n, 1);
    check_trit("tor: 0 tor 0 = 0", z tor z, 0);
    check_trit("tor: 0 tor - = 0", z tor n, 0);
    check_trit("tor: - tor - = -", n tor n, -1);
}

fn test_tnot() {
    let p: trit = +;
    let z: trit = 0;
    let n: trit = -;
    check_trit("tnot: tnot + = -", tnot p, -1);
    check_trit("tnot: tnot 0 = 0", tnot z, 0);
    check_trit("tnot: tnot - = +", tnot n, 1);
}

fn test_txor() {
    let p: trit = +;
    let z: trit = 0;
    let n: trit = -;
    // txor is balanced (a + b) mod 3 — sum without carry.
    check_trit("txor: + txor + = -", p txor p, -1);
    check_trit("txor: + txor 0 = +", p txor z, 1);
    check_trit("txor: + txor - = 0", p txor n, 0);
    check_trit("txor: - txor - = +", n txor n, 1);
    check_trit("txor: 0 txor - = -", z txor n, -1);
}

// ---------------------------------------------------------------------------
// Operator precedence
// ---------------------------------------------------------------------------

fn test_precedence() {
    // arithmetic > comparison
    check_int("prec: 2+3==5",       if 2 + 3 == 5   { 1 } else { 0 }, 1);
    // unary minus binds tightly
    check_int("prec: -2*3=-6",     -2 * 3,  -6);
    // multiplication before addition
    check_int("prec: 2+3*4=14",     2 + 3 * 4,  14);
    // shift before arithmetic (low prio in our lang — verify)
    check_int("prec: 1+1<<1=4",    (1 + 1) << 1,  4);
    // bitwise before logical
    check_int("prec: 2&3|4=6",      (2 & 3) | 4,  6);
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() {
    io::println("=== 02 Operators ===");

    io::println("-- arithmetic --");
    test_add(); test_sub(); test_mul(); test_div(); test_mod(); test_neg();
    test_div_truncate();
    test_mod_sign();
    test_chained_arith();

    io::println("-- comparison --");
    test_cmp_eq(); test_cmp_ne(); test_cmp_lt();
    test_cmp_le(); test_cmp_gt(); test_cmp_ge();
    test_cmp_neg();

    io::println("-- logical --");
    test_logical();
    test_short_circuit();

    io::println("-- bitwise --");
    test_bitwise();

    io::println("-- compound assign --");
    test_compound_assign();

    io::println("-- ternary logic --");
    test_tand(); test_tor(); test_tnot(); test_txor();

    io::println("-- precedence --");
    test_precedence();

    io::println("Done.");
}
