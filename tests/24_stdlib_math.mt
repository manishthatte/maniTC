// Test: Math and ternary operations — balanced ternary conversion round-trips,
//        trit arithmetic tables, powers of 3, abs/min/max/clamp,
//        trit_count, trit_weight — uses std::math where available,
//        pure ManiT implementations otherwise
use std::io;
use std::math;

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
// Balanced ternary conversion round-trip: int -> trits -> int
// ---------------------------------------------------------------------------

fn test_bt_roundtrip() {
    // Powers of 3
    check_int("bt-rt: 0", math::from_balanced_ternary(math::to_balanced_ternary(0)), 0);
    check_int("bt-rt: 1", math::from_balanced_ternary(math::to_balanced_ternary(1)), 1);
    check_int("bt-rt: -1", math::from_balanced_ternary(math::to_balanced_ternary(-1)), -1);
    check_int("bt-rt: 3", math::from_balanced_ternary(math::to_balanced_ternary(3)), 3);
    check_int("bt-rt: -3", math::from_balanced_ternary(math::to_balanced_ternary(-3)), -3);
    check_int("bt-rt: 9", math::from_balanced_ternary(math::to_balanced_ternary(9)), 9);
    check_int("bt-rt: 27", math::from_balanced_ternary(math::to_balanced_ternary(27)), 27);

    // Non-power values
    check_int("bt-rt: 5", math::from_balanced_ternary(math::to_balanced_ternary(5)), 5);
    check_int("bt-rt: -5", math::from_balanced_ternary(math::to_balanced_ternary(-5)), -5);
    check_int("bt-rt: 8", math::from_balanced_ternary(math::to_balanced_ternary(8)), 8);
    check_int("bt-rt: 13", math::from_balanced_ternary(math::to_balanced_ternary(13)), 13);
    check_int("bt-rt: -13", math::from_balanced_ternary(math::to_balanced_ternary(-13)), -13);
    check_int("bt-rt: 42", math::from_balanced_ternary(math::to_balanced_ternary(42)), 42);
}

// ---------------------------------------------------------------------------
// trit_add: full truth table (all 9 input combinations with carry)
// Pure ManiT implementation since ternary::trit_add returns a tuple
// ---------------------------------------------------------------------------

// Compute balanced ternary single-trit addition.
// Returns sum as first value, carry as second (via two separate functions).
fn trit_add_sum(a: int, b: int) -> int {
    let s = a + b;
    if s > 1 { s - 3 }      // 2 -> -1 (carry +1)
    elif s < -1 { s + 3 }   // -2 -> +1 (carry -1)
    else { s }
}

fn trit_add_carry(a: int, b: int) -> int {
    let s = a + b;
    if s > 1 { 1 }
    elif s < -1 { -1 }
    else { 0 }
}

fn test_trit_add_table() {
    // +1 + +1 = +2 = (sum=-1, carry=+1) since 2 = 3*1 + (-1)
    check_int("trit-add: (+,+) sum=-",    trit_add_sum(1, 1),   -1);
    check_int("trit-add: (+,+) carry=+",  trit_add_carry(1, 1),  1);

    // +1 + 0 = +1 = (sum=+1, carry=0)
    check_int("trit-add: (+,0) sum=+",    trit_add_sum(1, 0),    1);
    check_int("trit-add: (+,0) carry=0",  trit_add_carry(1, 0),  0);

    // +1 + -1 = 0 = (sum=0, carry=0)
    check_int("trit-add: (+,-) sum=0",    trit_add_sum(1, -1),   0);
    check_int("trit-add: (+,-) carry=0",  trit_add_carry(1, -1), 0);

    // 0 + +1 = +1 = (sum=+1, carry=0)
    check_int("trit-add: (0,+) sum=+",    trit_add_sum(0, 1),    1);
    check_int("trit-add: (0,+) carry=0",  trit_add_carry(0, 1),  0);

    // 0 + 0 = 0 = (sum=0, carry=0)
    check_int("trit-add: (0,0) sum=0",    trit_add_sum(0, 0),    0);
    check_int("trit-add: (0,0) carry=0",  trit_add_carry(0, 0),  0);

    // 0 + -1 = -1 = (sum=-1, carry=0)
    check_int("trit-add: (0,-) sum=-",    trit_add_sum(0, -1),  -1);
    check_int("trit-add: (0,-) carry=0",  trit_add_carry(0, -1), 0);

    // -1 + +1 = 0 = (sum=0, carry=0)
    check_int("trit-add: (-,+) sum=0",    trit_add_sum(-1, 1),   0);
    check_int("trit-add: (-,+) carry=0",  trit_add_carry(-1, 1), 0);

    // -1 + 0 = -1 = (sum=-1, carry=0)
    check_int("trit-add: (-,0) sum=-",    trit_add_sum(-1, 0),  -1);
    check_int("trit-add: (-,0) carry=0",  trit_add_carry(-1, 0), 0);

    // -1 + -1 = -2 = (sum=+1, carry=-1) since -2 = 3*(-1) + 1
    check_int("trit-add: (-,-) sum=+",    trit_add_sum(-1, -1),   1);
    check_int("trit-add: (-,-) carry=-",  trit_add_carry(-1, -1), -1);
}

// ---------------------------------------------------------------------------
// trit_mul: full truth table (all 9 combinations)
// ---------------------------------------------------------------------------

// Pure ManiT trit multiplication
fn local_trit_mul(a: int, b: int) -> int {
    let r = a * b;
    if r > 0 { 1 } elif r == 0 { 0 } else { -1 }
}

fn test_trit_mul_table() {
    // Sign rules: positive*positive=positive, positive*negative=negative,
    //             negative*negative=positive, anything*zero=zero
    check_int("trit-mul: (+)(+)=+", local_trit_mul(1, 1),    1);
    check_int("trit-mul: (+)(0)=0", local_trit_mul(1, 0),    0);
    check_int("trit-mul: (+)(-)=-", local_trit_mul(1, -1),  -1);
    check_int("trit-mul: (0)(+)=0", local_trit_mul(0, 1),    0);
    check_int("trit-mul: (0)(0)=0", local_trit_mul(0, 0),    0);
    check_int("trit-mul: (0)(-)=0", local_trit_mul(0, -1),   0);
    check_int("trit-mul: (-)(+)=-", local_trit_mul(-1, 1),  -1);
    check_int("trit-mul: (-)(0)=0", local_trit_mul(-1, 0),   0);
    check_int("trit-mul: (-)(-)=+", local_trit_mul(-1, -1),  1);
}

// ---------------------------------------------------------------------------
// Power of 3 calculations
// ---------------------------------------------------------------------------

// Pure ManiT pow3
fn local_pow3(n: int) -> int {
    let mut r: int = 1;
    let mut i: int = 0;
    while i < n {
        r = r * 3;
        i = i + 1;
    }
    r
}

fn test_pow3() {
    check_int("pow3: 3^0=1",    local_pow3(0),   1);
    check_int("pow3: 3^1=3",    local_pow3(1),   3);
    check_int("pow3: 3^2=9",    local_pow3(2),   9);
    check_int("pow3: 3^3=27",   local_pow3(3),  27);
    check_int("pow3: 3^4=81",   local_pow3(4),  81);
    check_int("pow3: 3^5=243",  local_pow3(5), 243);
    check_int("pow3: 3^6=729",  local_pow3(6), 729);
}

// Pure ManiT is_pow3
fn local_is_pow3(n: int) -> bool {
    if n <= 0 { return false; }
    let mut v: int = n;
    while v > 1 {
        if v % 3 != 0 { return false; }
        v = v / 3;
    }
    v == 1
}

fn test_is_pow3() {
    check("is_pow3: 1",   local_is_pow3(1));
    check("is_pow3: 3",   local_is_pow3(3));
    check("is_pow3: 9",   local_is_pow3(9));
    check("is_pow3: 27",  local_is_pow3(27));
    check("is_pow3: 81",  local_is_pow3(81));
    check("is_pow3: !2",  !local_is_pow3(2));
    check("is_pow3: !4",  !local_is_pow3(4));
    check("is_pow3: !10", !local_is_pow3(10));
}

// ---------------------------------------------------------------------------
// abs, min, max, clamp
// ---------------------------------------------------------------------------

// Pure ManiT implementations
fn local_abs(n: int) -> int { if n < 0 { -n } else { n } }
fn local_min(a: int, b: int) -> int { if a < b { a } else { b } }
fn local_max(a: int, b: int) -> int { if a > b { a } else { b } }
fn local_clamp(n: int, lo: int, hi: int) -> int {
    if n < lo { lo } elif n > hi { hi } else { n }
}

fn test_abs() {
    check_int("abs: abs(5)=5",     local_abs(5),    5);
    check_int("abs: abs(-7)=7",    local_abs(-7),   7);
    check_int("abs: abs(0)=0",     local_abs(0),    0);
    check_int("abs: abs(-1)=1",    local_abs(-1),   1);
    check_int("abs: abs(13)=13",   local_abs(13),  13);
}

fn test_min_max() {
    check_int("min: min(3,7)=3",     local_min(3, 7),     3);
    check_int("min: min(7,3)=3",     local_min(7, 3),     3);
    check_int("min: min(-1,1)=-1",   local_min(-1, 1),   -1);
    check_int("min: min(0,0)=0",     local_min(0, 0),     0);

    check_int("max: max(3,7)=7",     local_max(3, 7),     7);
    check_int("max: max(7,3)=7",     local_max(7, 3),     7);
    check_int("max: max(-1,1)=1",    local_max(-1, 1),    1);
    check_int("max: max(0,0)=0",     local_max(0, 0),     0);
}

fn test_clamp() {
    check_int("clamp: 5 in [0,10]=5",    local_clamp(5, 0, 10),     5);
    check_int("clamp: -5 in [0,10]=0",   local_clamp(-5, 0, 10),    0);
    check_int("clamp: 15 in [0,10]=10",  local_clamp(15, 0, 10),   10);
    check_int("clamp: 0 in [0,10]=0",    local_clamp(0, 0, 10),     0);
    check_int("clamp: 10 in [0,10]=10",  local_clamp(10, 0, 10),   10);
    check_int("clamp: -1 in [-1,1]=-1",  local_clamp(-1, -1, 1),   -1);
    check_int("clamp: 1 in [-1,1]=1",    local_clamp(1, -1, 1),     1);
}

// ---------------------------------------------------------------------------
// Integer log base 3 (pure ManiT — no float needed)
// ---------------------------------------------------------------------------

// Integer log3: largest k such that 3^k <= n (for n >= 1)
fn local_ilog3(n: int) -> int {
    if n <= 0 { return -1; }
    let mut k: int = 0;
    let mut p: int = 1;
    while p * 3 <= n {
        p = p * 3;
        k = k + 1;
    }
    k
}

fn test_log3() {
    check_int("ilog3: ilog3(1)=0",  local_ilog3(1),   0);
    check_int("ilog3: ilog3(3)=1",  local_ilog3(3),   1);
    check_int("ilog3: ilog3(9)=2",  local_ilog3(9),   2);
    check_int("ilog3: ilog3(27)=3", local_ilog3(27),  3);
    check_int("ilog3: ilog3(81)=4", local_ilog3(81),  4);
    check_int("ilog3: ilog3(8)=1",  local_ilog3(8),   1);
    check_int("ilog3: ilog3(26)=2", local_ilog3(26),  2);
}

// ---------------------------------------------------------------------------
// trit_count: number of trits needed to represent a value
// ---------------------------------------------------------------------------

fn test_trit_count() {
    check_int("trit-cnt: 0 -> 1",    math::trit_count(0),    1);
    check_int("trit-cnt: 1 -> 1",    math::trit_count(1),    1);
    check_int("trit-cnt: -1 -> 1",   math::trit_count(-1),   1);
    check_int("trit-cnt: 2 -> 2",    math::trit_count(2),    2);
    check_int("trit-cnt: -2 -> 2",   math::trit_count(-2),   2);
    check_int("trit-cnt: 4 -> 2",    math::trit_count(4),    2);
    check_int("trit-cnt: 9 -> 3",    math::trit_count(9),    3);
    check_int("trit-cnt: 13 -> 3",   math::trit_count(13),   3);
    check_int("trit-cnt: -13 -> 3",  math::trit_count(-13),  3);
    check_int("trit-cnt: 14 -> 4",   math::trit_count(14),   4);
    check_int("trit-cnt: 27 -> 4",   math::trit_count(27),   4);
}

// ---------------------------------------------------------------------------
// trit_weight: ternary Hamming weight (sum of |trit_i|)
// ---------------------------------------------------------------------------

// Pure ManiT trit_weight: sum of absolute values of balanced ternary digits
fn local_trit_weight(n: int) -> int {
    let mut v: int = local_abs(n);
    if v == 0 { return 0; }
    let mut w: int = 0;
    while v > 0 {
        let rem = v % 3;
        if rem == 0 {
            // trit is 0, no weight
        } elif rem == 1 {
            w = w + 1;  // trit is +1
        } else {
            // rem == 2 means this trit is -1 (carry 1 to next)
            w = w + 1;
            v = v + 1;
        }
        v = v / 3;
    }
    w
}

fn test_trit_weight() {
    // 0 has weight 0 (single zero trit)
    check_int("trit-wt: 0 -> 0",    local_trit_weight(0),   0);
    // 1 = + has weight 1
    check_int("trit-wt: 1 -> 1",    local_trit_weight(1),   1);
    // -1 = - has weight 1
    check_int("trit-wt: -1 -> 1",   local_trit_weight(-1),  1);
    // 8 = +0- has weight 2 (one + and one -)
    check_int("trit-wt: 8 -> 2",    local_trit_weight(8),   2);
    // 13 = +++ has weight 3
    check_int("trit-wt: 13 -> 3",   local_trit_weight(13),  3);
}

// ---------------------------------------------------------------------------
// sign function
// ---------------------------------------------------------------------------

// Pure ManiT sign
fn local_sign(n: int) -> int {
    if n > 0 { 1 } elif n == 0 { 0 } else { -1 }
}

fn test_sign() {
    check_int("sign: sign(5)=1",    local_sign(5),    1);
    check_int("sign: sign(-3)=-1",  local_sign(-3),  -1);
    check_int("sign: sign(0)=0",    local_sign(0),    0);
    check_int("sign: sign(100)=1",  local_sign(100),  1);
    check_int("sign: sign(-1)=-1",  local_sign(-1),  -1);
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() {
    io::println("=== 24 Stdlib Math ===");

    io::println("-- balanced ternary round-trip --");
    test_bt_roundtrip();

    io::println("-- trit_add truth table --");
    test_trit_add_table();

    io::println("-- trit_mul truth table --");
    test_trit_mul_table();

    io::println("-- power of 3 --");
    test_pow3();
    test_is_pow3();

    io::println("-- abs --");
    test_abs();

    io::println("-- min/max --");
    test_min_max();

    io::println("-- clamp --");
    test_clamp();

    io::println("-- log3 --");
    test_log3();

    io::println("-- trit_count --");
    test_trit_count();

    io::println("-- trit_weight --");
    test_trit_weight();

    io::println("-- sign --");
    test_sign();

    io::println("Done.");
}
