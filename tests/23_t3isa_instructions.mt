// Test: T3ISA instruction-level tests — arithmetic, ternary logic ops on all 9
//        combinations, trit shifts, comparisons, load/store
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
// Arithmetic: tadd (integer addition)
// ---------------------------------------------------------------------------

fn test_tadd() {
    check_int("tadd: 0+0=0",       0 + 0,       0);
    check_int("tadd: 1+1=2",       1 + 1,       2);
    check_int("tadd: -1+1=0",      -1 + 1,      0);
    check_int("tadd: -1+(-1)=-2",  -1 + (-1),   -2);
    check_int("tadd: 13+13=26",    13 + 13,     26);
    check_int("tadd: -13+13=0",    -13 + 13,    0);
    check_int("tadd: 27+(-27)=0",  27 + (-27),  0);
    check_int("tadd: 9+3=12",      9 + 3,       12);
}

// ---------------------------------------------------------------------------
// Arithmetic: tsub (integer subtraction)
// ---------------------------------------------------------------------------

fn test_tsub() {
    check_int("tsub: 0-0=0",       0 - 0,       0);
    check_int("tsub: 1-1=0",       1 - 1,       0);
    check_int("tsub: 0-1=-1",      0 - 1,      -1);
    check_int("tsub: -1-(-1)=0",   -1 - (-1),   0);
    check_int("tsub: 27-9=18",     27 - 9,     18);
    check_int("tsub: 3-9=-6",      3 - 9,      -6);
}

// ---------------------------------------------------------------------------
// Arithmetic: tmul (integer multiplication)
// ---------------------------------------------------------------------------

fn test_tmul() {
    check_int("tmul: 0*1=0",       0 * 1,       0);
    check_int("tmul: 1*1=1",       1 * 1,       1);
    check_int("tmul: -1*1=-1",     -1 * 1,     -1);
    check_int("tmul: -1*(-1)=1",   -1 * (-1),   1);
    check_int("tmul: 3*9=27",      3 * 9,      27);
    check_int("tmul: -3*3=-9",     -3 * 3,     -9);
    check_int("tmul: 3*3*3=27",    3 * 3 * 3,  27);
}

// ---------------------------------------------------------------------------
// Arithmetic: tdiv (integer division, truncating)
// ---------------------------------------------------------------------------

fn test_tdiv() {
    check_int("tdiv: 27/3=9",      27 / 3,      9);
    check_int("tdiv: 27/9=3",      27 / 9,      3);
    check_int("tdiv: -27/3=-9",    -27 / 3,    -9);
    check_int("tdiv: 10/3=3",      10 / 3,      3);
    check_int("tdiv: -10/3=-3",    -10 / 3,    -3);
    check_int("tdiv: 1/1=1",       1 / 1,       1);
    check_int("tdiv: 0/5=0",       0 / 5,       0);
}

// ---------------------------------------------------------------------------
// Ternary ops: tand on all 9 combinations
// ---------------------------------------------------------------------------

fn test_tand_full() {
    let p: trit = +;
    let z: trit = 0;
    let n: trit = -;

    check_trit("t3-tand: +,+=+",  p tand p,  1);
    check_trit("t3-tand: +,0=0",  p tand z,  0);
    check_trit("t3-tand: +,-=-",  p tand n, -1);
    check_trit("t3-tand: 0,+=0",  z tand p,  0);
    check_trit("t3-tand: 0,0=0",  z tand z,  0);
    check_trit("t3-tand: 0,-=-",  z tand n, -1);
    check_trit("t3-tand: -,+=-",  n tand p, -1);
    check_trit("t3-tand: -,0=-",  n tand z, -1);
    check_trit("t3-tand: -,-=-",  n tand n, -1);
}

// ---------------------------------------------------------------------------
// Ternary ops: tor on all 9 combinations
// ---------------------------------------------------------------------------

fn test_tor_full() {
    let p: trit = +;
    let z: trit = 0;
    let n: trit = -;

    check_trit("t3-tor: +,+=+",  p tor p,  1);
    check_trit("t3-tor: +,0=+",  p tor z,  1);
    check_trit("t3-tor: +,-=+",  p tor n,  1);
    check_trit("t3-tor: 0,+=+",  z tor p,  1);
    check_trit("t3-tor: 0,0=0",  z tor z,  0);
    check_trit("t3-tor: 0,-=0",  z tor n,  0);
    check_trit("t3-tor: -,+=+",  n tor p,  1);
    check_trit("t3-tor: -,0=0",  n tor z,  0);
    check_trit("t3-tor: -,-=-",  n tor n, -1);
}

// ---------------------------------------------------------------------------
// Ternary ops: tnot on all 3 values
// ---------------------------------------------------------------------------

fn test_tnot_full() {
    let p: trit = +;
    let z: trit = 0;
    let n: trit = -;

    check_trit("t3-tnot: tnot(+)=-",  tnot p, -1);
    check_trit("t3-tnot: tnot(0)=0",  tnot z,  0);
    check_trit("t3-tnot: tnot(-)=+",  tnot n,  1);

    // Double negation identity
    check_trit("t3-tnot: tnot(tnot(+))=+",  tnot (tnot p),  1);
    check_trit("t3-tnot: tnot(tnot(0))=0",  tnot (tnot z),  0);
    check_trit("t3-tnot: tnot(tnot(-))=-",  tnot (tnot n), -1);
}

// ---------------------------------------------------------------------------
// Ternary ops: txor on all 9 combinations
// ---------------------------------------------------------------------------

fn test_txor_full() {
    let p: trit = +;
    let z: trit = 0;
    let n: trit = -;

    // Balanced (a + b) mod 3 — see tests/14_ternary_logic_complete.mt.
    check_trit("t3-txor: +,+=-",  p txor p, -1);
    check_trit("t3-txor: +,0=+",  p txor z,  1);
    check_trit("t3-txor: +,-=0",  p txor n,  0);
    check_trit("t3-txor: 0,+=+",  z txor p,  1);
    check_trit("t3-txor: 0,0=0",  z txor z,  0);
    check_trit("t3-txor: 0,-=-",  z txor n, -1);
    check_trit("t3-txor: -,+=0",  n txor p,  0);
    check_trit("t3-txor: -,0=-",  n txor z, -1);
    check_trit("t3-txor: -,-=+",  n txor n,  1);
}

// ---------------------------------------------------------------------------
// Ternary ops: tcon on all 9 combinations
// ---------------------------------------------------------------------------

fn test_tcon_full() {
    let p: trit = +;
    let z: trit = 0;
    let n: trit = -;

    check_trit("t3-tcon: +,+=+",  p tcon p,  1);
    check_trit("t3-tcon: +,0=0",  p tcon z,  0);
    check_trit("t3-tcon: +,-=0",  p tcon n,  0);
    check_trit("t3-tcon: 0,+=0",  z tcon p,  0);
    check_trit("t3-tcon: 0,0=0",  z tcon z,  0);
    check_trit("t3-tcon: 0,-=0",  z tcon n,  0);
    check_trit("t3-tcon: -,+=0",  n tcon p,  0);
    check_trit("t3-tcon: -,0=0",  n tcon z,  0);
    check_trit("t3-tcon: -,-=-",  n tcon n, -1);
}

// ---------------------------------------------------------------------------
// Ternary ops: tany on all 9 combinations
// ---------------------------------------------------------------------------

fn test_tany_full() {
    let p: trit = +;
    let z: trit = 0;
    let n: trit = -;

    check_trit("t3-tany: +,+=+",  p tany p,  1);
    check_trit("t3-tany: +,0=+",  p tany z,  1);
    check_trit("t3-tany: +,-=+",  p tany n,  1);
    check_trit("t3-tany: 0,+=+",  z tany p,  1);
    check_trit("t3-tany: 0,0=0",  z tany z,  0);
    check_trit("t3-tany: 0,-=-",  z tany n, -1);
    check_trit("t3-tany: -,+=+",  n tany p,  1);
    check_trit("t3-tany: -,0=-",  n tany z, -1);
    check_trit("t3-tany: -,-=-",  n tany n, -1);
}

// ---------------------------------------------------------------------------
// Trit shift left (multiply by 3^n)
// ---------------------------------------------------------------------------

fn test_shift_left() {
    let shifted1 = ternary::t27_shift_left(1, 1);
    check_int("tshift-L: 1<<1=3", ternary::t27_to_int(shifted1), 3);

    let shifted2 = ternary::t27_shift_left(1, 2);
    check_int("tshift-L: 1<<2=9", ternary::t27_to_int(shifted2), 9);

    let shifted3 = ternary::t27_shift_left(1, 3);
    check_int("tshift-L: 1<<3=27", ternary::t27_to_int(shifted3), 27);

    let shifted4 = ternary::t27_shift_left(4, 1);
    check_int("tshift-L: 4<<1=12", ternary::t27_to_int(shifted4), 12);

    // Shift by 0 is identity
    let shifted0 = ternary::t27_shift_left(5, 0);
    check_int("tshift-L: 5<<0=5", ternary::t27_to_int(shifted0), 5);
}

// ---------------------------------------------------------------------------
// Trit shift right (divide by 3^n, truncate)
// ---------------------------------------------------------------------------

fn test_shift_right() {
    let rsh1 = ternary::t27_shift_right(27, 1);
    check_int("tshift-R: 27>>1=9", ternary::t27_to_int(rsh1), 9);

    let rsh2 = ternary::t27_shift_right(27, 3);
    check_int("tshift-R: 27>>3=1", ternary::t27_to_int(rsh2), 1);

    let rsh3 = ternary::t27_shift_right(9, 2);
    check_int("tshift-R: 9>>2=1", ternary::t27_to_int(rsh3), 1);

    // Shift by 0 is identity
    let rsh0 = ternary::t27_shift_right(13, 0);
    check_int("tshift-R: 13>>0=13", ternary::t27_to_int(rsh0), 13);
}

// ---------------------------------------------------------------------------
// Shift left then right is identity
// ---------------------------------------------------------------------------

fn test_shift_roundtrip() {
    let orig = 7;
    let up = ternary::t27_shift_left(orig, 2);
    let back = ternary::t27_shift_right(up, 2);
    check_int("tshift-rt: left-then-right 7", ternary::t27_to_int(back), orig);

    let orig2 = -4;
    let up2 = ternary::t27_shift_left(orig2, 1);
    let back2 = ternary::t27_shift_right(up2, 1);
    check_int("tshift-rt: left-then-right -4", ternary::t27_to_int(back2), orig2);
}

// ---------------------------------------------------------------------------
// Comparison and branching
// ---------------------------------------------------------------------------

fn test_comparisons() {
    // Integer comparisons feed trit conditions
    let a: int = 13;
    let b: int = 4;

    check("cmp: 13>4",    a > b);
    check("cmp: 4<13",    b < a);
    check("cmp: 13>=13",  a >= a);
    check("cmp: 4<=4",    b <= b);
    check("cmp: 13!=4",   a != b);
    check("cmp: 13==13",  a == a);
}

fn test_comparison_branching() {
    let x: int = 0;
    let label = if x > 0 { "pos" } elif x < 0 { "neg" } else { "zero" };
    check("cmp-branch: 0 is zero", label == "zero");

    let y: int = -5;
    let label2 = if y > 0 { "pos" } elif y < 0 { "neg" } else { "zero" };
    check("cmp-branch: -5 is neg", label2 == "neg");
}

// ---------------------------------------------------------------------------
// Load/store of trits, trytes, words via type conversions
// ---------------------------------------------------------------------------

fn test_trit_load_store() {
    // Single trit values
    let t1: trit = +;
    let t2: trit = 0;
    let t3: trit = -;
    check_trit("load-trit: +",  t1,  1);
    check_trit("load-trit: 0",  t2,  0);
    check_trit("load-trit: -",  t3, -1);
}

fn test_tryte_load_store() {
    // Tryte: 3 trits packed, verify via int round-trip
    let ty = ternary::tryte_from_trits(+, 0, -);
    let tv = ternary::tryte_to_int(ty);
    // +*9 + 0*3 + (-1)*1 = 9 - 1 = 8
    check_int("load-tryte: tryte(+,0,-)=8", tv, 8);

    let ty2 = ternary::tryte_from_trits(-, +, 0);
    let tv2 = ternary::tryte_to_int(ty2);
    // (-1)*9 + (+1)*3 + 0*1 = -9 + 3 = -6
    check_int("load-tryte: tryte(-,+,0)=-6", tv2, -6);
}

fn test_word_conversions() {
    // t27 from int and back
    let w = ternary::int_to_t27(42);
    let v = ternary::t27_to_int(w);
    check_int("word-rt: 42 round-trips", v, 42);

    let w2 = ternary::int_to_t27(-13);
    let v2 = ternary::t27_to_int(w2);
    check_int("word-rt: -13 round-trips", v2, -13);

    let w3 = ternary::int_to_t27(0);
    let v3 = ternary::t27_to_int(w3);
    check_int("word-rt: 0 round-trips", v3, 0);
}

// ---------------------------------------------------------------------------
// Overflow/underflow: clamp behavior
// ---------------------------------------------------------------------------

// Pure ManiT clamp (no std::math dependency)
fn clamp(n: int, lo: int, hi: int) -> int {
    if n < lo { lo } elif n > hi { hi } else { n }
}

fn test_clamp_behavior() {
    check_int("clamp: 5 in [0,10]=5",     clamp(5, 0, 10),     5);
    check_int("clamp: -5 in [0,10]=0",    clamp(-5, 0, 10),    0);
    check_int("clamp: 15 in [0,10]=10",   clamp(15, 0, 10),   10);
    check_int("clamp: 0 in [-1,1]=0",     clamp(0, -1, 1),     0);
    check_int("clamp: 99 in [-1,1]=1",    clamp(99, -1, 1),    1);
    check_int("clamp: -99 in [-1,1]=-1",  clamp(-99, -1, 1),  -1);
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() {
    io::println("=== 23 T3ISA Instructions ===");

    io::println("-- tadd --");
    test_tadd();

    io::println("-- tsub --");
    test_tsub();

    io::println("-- tmul --");
    test_tmul();

    io::println("-- tdiv --");
    test_tdiv();

    io::println("-- tand (9 combos) --");
    test_tand_full();

    io::println("-- tor (9 combos) --");
    test_tor_full();

    io::println("-- tnot (3 + double neg) --");
    test_tnot_full();

    io::println("-- txor (9 combos) --");
    test_txor_full();

    io::println("-- tcon (9 combos) --");
    test_tcon_full();

    io::println("-- tany (9 combos) --");
    test_tany_full();

    io::println("-- trit shift left --");
    test_shift_left();

    io::println("-- trit shift right --");
    test_shift_right();

    io::println("-- shift roundtrip --");
    test_shift_roundtrip();

    io::println("-- comparisons --");
    test_comparisons();
    test_comparison_branching();

    io::println("-- trit load/store --");
    test_trit_load_store();

    io::println("-- tryte load/store --");
    test_tryte_load_store();

    io::println("-- word conversions --");
    test_word_conversions();

    io::println("-- clamp behavior --");
    test_clamp_behavior();

    io::println("Done.");
}
