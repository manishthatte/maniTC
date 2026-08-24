// stdlib/std/trit.mt
// Trit intrinsics — the operations balanced ternary does cheaply and binary
// does not, named so a programmer can reach for them.
//
// Usage:
//   use std::trit;
//   let s = trit::sign(x);
//
// WHY THIS MODULE EXISTS SEPARATELY FROM `math`. `math::trit_count(x)` already
// exists and means the trit LENGTH of x — how many balanced-ternary digits it
// occupies. The intrinsic wanted here counts LANES EQUAL TO k, which is a
// different question with the same obvious name. Rather than overload the name
// or invent an uglier one, the new family lives in its own namespace, so
// `math::trit_count(x)` and `trit::count(x, k)` read as the different
// operations they are.
//
// WHY THESE ARE NATIVE. Every one of them is a small number of T3 instructions
// with no branch, and that is the entire point — the reason to have `sign` in
// the language is that on this machine it is not a branch. They are lowered in
// the IR lowerer (ir/lower/lower_expr.rs), NOT intercepted per-backend in the
// two emitters. That distinction is deliberate: `math` was intercepted in the
// T3 emitter and a census measured 3 of its 52 functions working on both
// backends, because each intercept had to be written twice and nothing forced
// the second one. Lowering to existing IR means both backends inherit these
// from instructions they already implement, and there is no second place to
// forget.

// ---------------------------------------------------------------------------
// Native intrinsics
// ---------------------------------------------------------------------------

// Count the lanes of `x` equal to the trit `k`.
//
// One instruction on T3 (TPOPC, added in T3ISA v1.5); a runtime call on LLVM.
// `k` is clamped into {-1, 0, +1}: a count of lanes equal to 7 has no meaning,
// and silently answering zero would hide the mistake rather than report it.
// The result is a count in 0..=27, not a word read lane-wise.
fn count(x: int, k: trit) -> int ;  // native

// The sign of `x`, as a trit: -1, 0 or +1.
//
// This is the intrinsic the recommendations single out. In two's complement
// sign is a branch or a shift-and-or; here it is two native instructions and
// no branch at all:
//
//     sign(x) = TritMax(TritMin(x, +1), -1)
//
// The clamp is exact because it is a NUMERIC min and max, so any positive
// magnitude collapses to +1 and any negative to -1 in one step each. Compare
// what a programmer writes without it -- `if x > 0 { + } elif x < 0 { - }
// else { 0 }` -- which is a three-way branch and several times the cost.
//
// The ISA can do better still: TCMP against R0 computes this in ONE
// instruction, because R0 always reads as zero. The compiler does not emit
// that yet; see the note in the language reference.
fn sign(x: int) -> trit ;  // native

// The absolute value of `x`, exact for every input.
//
//     abs(x) = TritMax(x, -x)
//
// Two instructions, no branch, and — unlike two's complement — no asymmetric
// minimum to special-case. The 27-trit range is symmetric (±3,812,798,742,493),
// so there is no value whose negation overflows. In two's complement
// `abs(INT_MIN)` is undefined or wraps to itself; here the question does not
// arise. That is a property of the representation, not a bound being checked.
fn abs(x: int) -> int ;  // native

// `x * 3^n` — the machine's native shift, named as such.
//
// One instruction on T3 (TSHI). This is the shift that matters on a ternary
// machine; `<<` is the binary one and multiplies by 2^n.
fn shift3(x: int, n: int) -> int ;  // native

// ---------------------------------------------------------------------------
// Derived — ordinary ManiT, because these are not single instructions
// ---------------------------------------------------------------------------

// The number of leading zero-trits in the 27-trit representation of `x`.
//
// Written over `math::trit_count`, which is the trit LENGTH of x and already
// works identically on both backends, so this needs no new primitive.
// `trit_count(0)` is 1 rather than 0 — zero still occupies one digit — so the
// zero case is handled separately: the all-zero word has 27 leading zeros, not
// 26.
fn leading_zeros(x: int) -> int {
    if x == 0 { return 27; }
    return 27 - math::trit_count(x);
}

// The number of trailing zero-trits in `x` — the count of low trits that are
// zero before the first non-zero one.
//
// A trit at position 0 is zero exactly when x is divisible by 3, and when it
// is, the division is exact, so no rounding convention is involved and the
// loop is the same on both backends.
fn trailing_zeros(x: int) -> int {
    if x == 0 { return 27; }
    let mut n: int = 0;
    let mut v: int = x;
    while v % 3 == 0 {
        v = v / 3;
        n = n + 1;
    }
    return n;
}
