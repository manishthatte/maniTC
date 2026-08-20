// Test: comprehensive compiler stress test — large structs, deep call chains,
//        large trit arrays, complex expressions, all-operator expressions,
//        mixed-type structs, multi-level function call chains
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

// ---------------------------------------------------------------------------
// Large struct with many trit fields
// ---------------------------------------------------------------------------

struct TritRegister {
    pub t0: trit,
    pub t1: trit,
    pub t2: trit,
    pub t3: trit,
    pub t4: trit,
    pub t5: trit,
    pub t6: trit,
    pub t7: trit,
    pub t8: trit,
    pub tag: int,
    pub valid: bool3,
}

impl TritRegister {
    fn new_zero() -> TritRegister {
        TritRegister {
            t0: 0, t1: 0, t2: 0, t3: 0, t4: 0,
            t5: 0, t6: 0, t7: 0, t8: 0,
            tag: 0, valid: true,
        }
    }

    fn new_pattern(sign: trit) -> TritRegister {
        TritRegister {
            t0: sign, t1: sign, t2: sign,
            t3: sign, t4: sign, t5: sign,
            t6: sign, t7: sign, t8: sign,
            tag: trit_val(sign), valid: true,
        }
    }

    fn sum(self) -> int {
        trit_val(self.t0) + trit_val(self.t1) + trit_val(self.t2) +
        trit_val(self.t3) + trit_val(self.t4) + trit_val(self.t5) +
        trit_val(self.t6) + trit_val(self.t7) + trit_val(self.t8)
    }
}

fn test_large_struct() {
    let zero_reg = TritRegister::new_zero();
    check_int("large-struct: zero sum=0", zero_reg.sum(), 0);
    check_int("large-struct: zero tag=0", zero_reg.tag, 0);

    let pos_reg = TritRegister::new_pattern(+);
    check_int("large-struct: all-pos sum=9", pos_reg.sum(), 9);
    check_int("large-struct: pos tag=1", pos_reg.tag, 1);

    let neg_reg = TritRegister::new_pattern(-);
    check_int("large-struct: all-neg sum=-9", neg_reg.sum(), -9);
    check_int("large-struct: neg tag=-1", neg_reg.tag, -1);

    tif pos_reg.valid {
        + => pass("large-struct: valid=true"),
        0 => fail("large-struct: valid"),
        - => fail("large-struct: valid"),
    }
}

// ---------------------------------------------------------------------------
// Deeply nested function calls (10+ levels)
// ---------------------------------------------------------------------------

fn f1(x: int) -> int { x + 1 }
fn f2(x: int) -> int { f1(x) + 1 }
fn f3(x: int) -> int { f2(x) + 1 }
fn f4(x: int) -> int { f3(x) + 1 }
fn f5(x: int) -> int { f4(x) + 1 }
fn f6(x: int) -> int { f5(x) + 1 }
fn f7(x: int) -> int { f6(x) + 1 }
fn f8(x: int) -> int { f7(x) + 1 }
fn f9(x: int) -> int { f8(x) + 1 }
fn f10(x: int) -> int { f9(x) + 1 }
fn f11(x: int) -> int { f10(x) + 1 }
fn f12(x: int) -> int { f11(x) + 1 }

fn test_deep_call_chain() {
    // f12(0) = 0 + 12 = 12
    check_int("deep-call: f12(0)=12",  f12(0),  12);
    // f12(10) = 10 + 12 = 22
    check_int("deep-call: f12(10)=22", f12(10), 22);
    // f12(-6) = -6 + 12 = 6
    check_int("deep-call: f12(-6)=6",  f12(-6), 6);
}

// ---------------------------------------------------------------------------
// Large array of trits (81 elements = 3^4)
// ---------------------------------------------------------------------------

fn test_large_trit_array() {
    // Use Vec<int> to hold 81 trit values (as ints: +1, 0, -1)
    let mut arr: Vec<int> = Vec::new();

    // Fill 81 entries: position mod 3 determines trit value
    let mut i: int = 0;
    while i < 81 {
        if i % 3 == 0 { arr.push(1); }
        elif i % 3 == 1 { arr.push(0); }
        else { arr.push(-1); }
        i = i + 1;
    }

    check_int("big-arr: length=81", arr.len(), 81);

    // Verify first few
    check_int("big-arr: [0]=+",  arr[0],  1);
    check_int("big-arr: [1]=0",  arr[1],  0);
    check_int("big-arr: [2]=-",  arr[2], -1);
    check_int("big-arr: [3]=+",  arr[3],  1);

    // Verify last few
    check_int("big-arr: [78]=+", arr[78],  1);
    check_int("big-arr: [79]=0", arr[79],  0);
    check_int("big-arr: [80]=-", arr[80], -1);

    // Sum all values: 27 positives, 27 zeros, 27 negatives = 0
    let mut sum: int = 0;
    i = 0;
    while i < 81 {
        sum = sum + arr[i];
        i = i + 1;
    }
    check_int("big-arr: sum of 81 trits=0", sum, 0);
}

// ---------------------------------------------------------------------------
// Complex expression with many operators chained
// ---------------------------------------------------------------------------

fn test_complex_expression() {
    // ((3 + 4) * (9 - 2) + 27) / 3 - 1 = (7*7 + 27)/3 - 1 = 76/3 - 1 = 25 - 1 = 24
    let result = ((3 + 4) * (9 - 2) + 27) / 3 - 1;
    check_int("complex-expr: ((3+4)*(9-2)+27)/3-1=24", result, 24);

    // Nested arithmetic with modulo
    let r2 = (100 % 27) * 3 + (81 / 9) - (13 % 4);
    // 100 % 27 = 19, 19*3 = 57, 81/9 = 9, 13%4 = 1, 57+9-1 = 65
    check_int("complex-expr: (100%27)*3+(81/9)-(13%4)=65", r2, 65);

    // Mixed with unary negation
    let r3 = -(-3 * 3) + -(-(9));
    // -(-9) + -(-(9)) = 9 + 9 = 18
    check_int("complex-expr: -(-3*3)+-(-(9))=18", r3, 18);
}

// ---------------------------------------------------------------------------
// Function using ALL ternary operators in one expression
// ---------------------------------------------------------------------------

fn all_ternary_ops(a: trit, b: trit) -> int {
    // Use tand, tor, tnot, txor, tcon, tany all together
    let r_tand = a tand b;
    let r_tor  = a tor b;
    let r_tnot = tnot a;
    let r_txor = a txor b;
    let r_tcon = a tcon b;
    let r_tany = a tany b;

    // Sum all results as integers
    trit_val(r_tand) + trit_val(r_tor) + trit_val(r_tnot) +
    trit_val(r_txor) + trit_val(r_tcon) + trit_val(r_tany)
}

fn test_all_ops_together() {
    // txor is balanced (a + b) mod 3 as of 19 August 2026; the three sums
    // that involve it moved by exactly the change in that one term.

    // a=+, b=+:
    //   tand(+,+)=+1, tor(+,+)=+1, tnot(+)=-1,
    //   txor(+,+)=-1, tcon(+,+)=+1, tany(+,+)=+1
    //   sum = 1+1+(-1)+(-1)+1+1 = 2
    check_int("all-ops: (+,+) sum=2", all_ternary_ops(+, +), 2);

    // a=+, b=-:
    //   tand(+,-)=-1, tor(+,-)=+1, tnot(+)=-1,
    //   txor(+,-)=0, tcon(+,-)=0, tany(+,-)=+1
    //   sum = -1+1+(-1)+0+0+1 = 0
    check_int("all-ops: (+,-) sum=0", all_ternary_ops(+, -), 0);

    // a=0, b=0:
    //   tand(0,0)=0, tor(0,0)=0, tnot(0)=0,
    //   txor(0,0)=0, tcon(0,0)=0, tany(0,0)=0
    //   sum = 0                              (unchanged — 0 is the identity)
    check_int("all-ops: (0,0) sum=0", all_ternary_ops(0, 0), 0);

    // a=-, b=-:
    //   tand(-,-)=-1, tor(-,-)=-1, tnot(-)=+1,
    //   txor(-,-)=+1, tcon(-,-)=-1, tany(-,-)=-1
    //   sum = -1+(-1)+1+1+(-1)+(-1) = -2
    check_int("all-ops: (-,-) sum=-2", all_ternary_ops(-, -), -2);
}

// ---------------------------------------------------------------------------
// Struct with T3Bool, trit, tryte, t9, t27, word fields all together
// ---------------------------------------------------------------------------

struct FullRegister {
    pub flag: T3Bool,
    pub single: trit,
    pub triple: tryte,
    pub nine: t9,
    pub twentyseven: t27,
    pub label: int,
}

fn test_mixed_type_struct() {
    let t = ternary::tryte_from_trits(+, 0, -);
    let n = ternary::int_to_t9(42);
    let w = ternary::int_to_t27(729);

    let reg = FullRegister {
        flag: true,
        single: +,
        triple: t,
        nine: n,
        twentyseven: w,
        label: 99,
    };

    // Verify flag
    tif reg.flag {
        + => pass("mixed-struct: flag=true"),
        0 => fail("mixed-struct: flag"),
        - => fail("mixed-struct: flag"),
    }

    // Verify single trit
    check_trit("mixed-struct: single=+", reg.single, 1);

    // Verify tryte via int conversion
    let tv = ternary::tryte_to_int(reg.triple);
    // tryte(+, 0, -) = +1*9 + 0*3 + (-1)*1 = 8
    check_int("mixed-struct: tryte=8", tv, 8);

    // Verify t9 value
    check_int("mixed-struct: t9=42", ternary::t9_to_int(reg.nine), 42);

    // Verify t27 value
    check_int("mixed-struct: t27=729", ternary::t27_to_int(reg.twentyseven), 729);

    // Verify int field
    check_int("mixed-struct: label=99", reg.label, 99);
}

// ---------------------------------------------------------------------------
// Passing ternary values through 5+ function call chain
// ---------------------------------------------------------------------------

fn chain_a(t: trit) -> trit { t tand + }
fn chain_b(t: trit) -> trit { chain_a(t) tor 0 }
fn chain_c(t: trit) -> trit { tnot (chain_b(t)) }
fn chain_d(t: trit) -> trit { chain_c(t) txor + }
fn chain_e(t: trit) -> trit { chain_d(t) tcon chain_d(t) }
fn chain_f(t: trit) -> int  { trit_val(chain_e(t)) }

fn test_trit_call_chain() {
    // Trace for +: chain_a(+) = + tand + = +
    //              chain_b(+) = + tor 0 = +
    //              chain_c(+) = tnot(+) = -
    //              chain_d(+) = - txor + = 0 (mod-3: -1 + 1 = 0)
    //              chain_e(+) = 0 tcon 0 = 0
    //              chain_f(+) = 0
    // Was 1 until 19 August 2026, when txor became mod-3 addition: the old
    // clamped |a-b| made `- txor +` = + and the chain ended at + instead.
    check_int("trit-chain: f(+)=0", chain_f(+), 0);

    // Trace for 0: chain_a(0) = 0 tand + = 0
    //              chain_b(0) = 0 tor 0 = 0
    //              chain_c(0) = tnot(0) = 0
    //              chain_d(0) = 0 txor + = + (mod-3: 0 + 1 = +1)
    //              chain_e(0) = + tcon + = +
    //              chain_f(0) = 1
    check_int("trit-chain: f(0)=1", chain_f(0), 1);

    // Trace for -: chain_a(-) = - tand + = -
    //              chain_b(-) = - tor 0 = 0 (max(-1,0)=0)
    //              chain_c(-) = tnot(0) = 0
    //              chain_d(-) = 0 txor + = +
    //              chain_e(-) = + tcon + = +
    //              chain_f(-) = 1
    check_int("trit-chain: f(-)=1", chain_f(-), 1);
}

// ---------------------------------------------------------------------------
// Deep recursion stress: Fibonacci iterative with ternary state
// ---------------------------------------------------------------------------

fn ternary_fib(n: int) -> int {
    if n <= 0 { return 0; }
    if n == 1 { return 1; }

    let mut a: int = 0;
    let mut b: int = 1;
    let mut i: int = 2;
    while i <= n {
        let tmp = a + b;
        a = b;
        b = tmp;
        i = i + 1;
    }
    b
}

fn test_iterative_fib() {
    check_int("fib-iter: fib(0)=0",   ternary_fib(0),   0);
    check_int("fib-iter: fib(1)=1",   ternary_fib(1),   1);
    check_int("fib-iter: fib(5)=5",   ternary_fib(5),   5);
    check_int("fib-iter: fib(10)=55", ternary_fib(10),  55);
    check_int("fib-iter: fib(20)=6765", ternary_fib(20), 6765);
}

// ---------------------------------------------------------------------------
// Stress: Vec of structs with ternary fields
// ---------------------------------------------------------------------------

struct TritPair {
    pub a: trit,
    pub b: trit,
    pub product: trit,
}

// Local trit multiply for the vec-of-structs test
fn local_tmul_trit(a: trit, b: trit) -> trit {
    let va = trit_val(a);
    let vb = trit_val(b);
    let r = va * vb;
    if r > 0 { + } elif r == 0 { 0 } else { - }
}

fn test_vec_of_structs() {
    let mut pairs: Vec<TritPair> = Vec::new();

    // Build all 9 trit pairs with their products
    let vals: Vec<int> = Vec::new();
    vals.push(1); vals.push(0); vals.push(-1);

    for ai in vals {
        for bi in vals {
            let at: trit = ai as trit;
            let bt: trit = bi as trit;
            let pt = local_tmul_trit(at, bt);
            pairs.push(TritPair { a: at, b: bt, product: pt });
        }
    }

    check_int("vec-struct: 9 pairs", pairs.len(), 9);

    // Verify first pair: (+)(+) = +
    check_trit("vec-struct: pair[0].product=+", pairs.get(0).product, 1);

    // Verify middle pair: (0)(0) = 0
    check_trit("vec-struct: pair[4].product=0", pairs.get(4).product, 0);

    // Verify last pair: (-)(-)=+
    check_trit("vec-struct: pair[8].product=+", pairs.get(8).product, 1);
}

// ---------------------------------------------------------------------------
// Stress: t27 arithmetic through conversion chain
// ---------------------------------------------------------------------------

fn test_t27_arithmetic_chain() {
    let a = ternary::int_to_t27(13);   // 13 = +++ in balanced ternary
    let b = ternary::int_to_t27(-4);   // -4 = -+- in balanced ternary

    // Convert to int, do arithmetic, convert back
    let va = ternary::t27_to_int(a);
    let vb = ternary::t27_to_int(b);
    let sum = ternary::int_to_t27(va + vb);
    let diff = ternary::int_to_t27(va - vb);

    check_int("t27-chain: 13+(-4)=9",  ternary::t27_to_int(sum),  9);
    check_int("t27-chain: 13-(-4)=17", ternary::t27_to_int(diff), 17);
}

// ---------------------------------------------------------------------------
// Stress: enum dispatch in tight loop
// ---------------------------------------------------------------------------

enum Op { Add, Sub, Mul }

fn apply_op(op: Op, a: int, b: int) -> int {
    match op {
        Op::Add => a + b,
        Op::Sub => a - b,
        Op::Mul => a * b,
    }
}

fn test_enum_dispatch_loop() {
    let ops: Vec<Op> = Vec::new();
    ops.push(Op::Add); ops.push(Op::Mul); ops.push(Op::Sub);
    ops.push(Op::Add); ops.push(Op::Mul);

    let mut result: int = 1;
    for op in ops {
        result = apply_op(op, result, 3);
    }
    // 1+3=4, 4*3=12, 12-3=9, 9+3=12, 12*3=36
    check_int("enum-dispatch: chain result=36", result, 36);
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() {
    io::println("=== 26 Stress Test ===");

    io::println("-- large struct --");
    test_large_struct();

    io::println("-- deep call chain --");
    test_deep_call_chain();

    io::println("-- large trit array (81) --");
    test_large_trit_array();

    io::println("-- complex expression --");
    test_complex_expression();

    io::println("-- all ternary ops --");
    test_all_ops_together();

    io::println("-- mixed-type struct --");
    test_mixed_type_struct();

    io::println("-- trit call chain --");
    test_trit_call_chain();

    io::println("-- iterative fibonacci --");
    test_iterative_fib();

    io::println("-- vec of structs --");
    test_vec_of_structs();

    io::println("-- t27 arithmetic chain --");
    test_t27_arithmetic_chain();

    io::println("-- enum dispatch loop --");
    test_enum_dispatch_loop();

    io::println("Done.");
}
