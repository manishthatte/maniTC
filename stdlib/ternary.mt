// stdlib/std/ternary.mt
// Low-level balanced ternary operations for maniT.
//
// This module exposes the primitive trit-manipulation builtins that underpin
// the language's native ternary arithmetic.  Most user code should work with
// the higher-level trit / tryte / t9 / t27 types directly; reach for this
// module when you need bitwise-style control over individual trit positions.
//
// Trit value encoding used throughout this module:
//   +  =>  +1  (the "positive" trit)
//    0 =>   0  (the "zero" trit)
//   -  =>  -1  (the "negative" trit)
//
// Usage:
//   use std::ternary;
//   let trits = ternary::int_to_trits(42, 5);

// ---------------------------------------------------------------------------
// Single-trit arithmetic
// ---------------------------------------------------------------------------

// Add two trits.  Returns (sum_trit, carry_trit).
// The full-add truth table follows balanced ternary carry rules.
// trit_add(+, +) -> (-, +)  because +1 + +1 = +2 = -1 + 1*3^1
// trit_add(+, -) -> (0, 0)  because +1 + -1 = 0
fn trit_add(a: trit, b: trit) -> (trit, trit) {
    // |a + b| can reach 2, which no single balanced trit holds, so the excess
    // becomes a carry: +2 = -1 + 1*3, -2 = +1 - 1*3.
    let s: int = trit_to_int(a) + trit_to_int(b);
    if s == 2 { return (-, +); }
    if s == -2 { return (+, -); }
    if s == 1 { return (+, 0); }
    if s == -1 { return (-, 0); }
    return (0, 0);
}

// Multiply two trits.  Result is always a single trit (no carry needed).
// trit_mul(+, +) -> +    (+1 * +1 = +1)
// trit_mul(+, -) -> -    (+1 * -1 = -1)
// trit_mul(-, -) -> +    (-1 * -1 = +1)
// trit_mul(0, _) -> 0    (0 times anything is 0)
fn trit_mul(a: trit, b: trit) -> trit {
    // The product of two trits is always a trit: |a*b| <= 1, so unlike
    // addition there is never a carry.
    let p: int = trit_to_int(a) * trit_to_int(b);
    if p > 0 { return +; }
    if p < 0 { return -; }
    return 0;
}

// Negate a trit (ternary NOT): + <-> -, 0 stays 0.
fn trit_neg(t: trit) -> trit {
    return tnot t;
}

// Ternary AND (Łukasiewicz / min): min(a, b) in {-1, 0, +1}.
fn trit_and(a: trit, b: trit) -> trit {
    return a tand b;
}

// Ternary OR (max): max(a, b) in {-1, 0, +1}.
fn trit_or(a: trit, b: trit) -> trit {
    return a tor b;
}

// Ternary XOR (addition mod 3, mapped to balanced representation).
// trit_xor(+, -) -> 0   (+1 + -1 mod 3 = 0)
// trit_xor(+, 0) -> +   (+1 + 0  mod 3 = +1)
// trit_xor(+, +) -> -   (+1 + +1 mod 3 = -1, i.e. 2 -> -1 in balanced)
fn trit_xor(a: trit, b: trit) -> trit {
    // Identical to the `txor` operator since 19 August 2026, when the operator
    // was corrected to the mod-3 addition this comment always described. They
    // disagreed in 6 of 9 cases before that; see ORACLE_FINDINGS.md Section 7.
    return a txor b;
}

// Consensus / median of three trits (used in fault-tolerant TMR logic).
// Returns the "majority vote" trit.
fn trit_median(a: trit, b: trit, c: trit) -> trit {
    // The middle value once sorted -- NOT the clamped sum, which differs:
    // median(-, 0, 0) is 0 while the sum is -1.
    //
    // Remove the smallest and the largest and the median is what remains, so
    // no sorting is needed.
    let x: int = trit_to_int(a);
    let y: int = trit_to_int(b);
    let z: int = trit_to_int(c);
    let mut lo: int = x;
    if y < lo { lo = y; }
    if z < lo { lo = z; }
    let mut hi: int = x;
    if y > hi { hi = y; }
    if z > hi { hi = z; }
    return int_to_trit(x + y + z - lo - hi);
}

// Convert a trit to its integer equivalent: + -> +1, 0 -> 0, - -> -1.
fn trit_to_int(t: trit) -> int ;  // native

// Convert an integer to a trit by its SIGN: positive -> +, negative -> -,
// zero -> 0.  In particular int_to_trit(5) is +, not an error.
//
// The doc comment said "panics for other values" until 19 August 2026 and
// neither backend did that: the T3 emitter treated it as an identity register
// move, so int_to_trit(5) yielded 5 -- not a valid trit at all -- while LLVM
// clamped. That is why it was one of the four DIVERGENT functions. Clamping to
// the sign is what every caller in the tree already relied on.
fn int_to_trit(n: int) -> trit {
    if n > 0 { return +; }
    if n < 0 { return -; }
    return 0;
}

// Return the sign of a trit as a trit (identity function, included for
// symmetry with math::sign).
fn trit_sign(t: trit) -> trit {
    // A balanced trit already IS its own sign.
    return t;
}

// ---------------------------------------------------------------------------
// Multi-trit (word-level) arithmetic
// ---------------------------------------------------------------------------

// Left-shift a t27 value by `n` trit positions (multiply by 3^n).
// Trits shifted beyond position 26 are discarded (wrapping behaviour).
fn trit_shift_left(val: t27, n: int) -> t27 ;  // native

// Right-shift a t27 value by `n` trit positions (divide by 3^n, truncate).
fn trit_shift_right(val: t27, n: int) -> t27 ;  // native

// Rotate a t27 value left by `n` trit positions.
fn trit_rotate_left(val: t27, n: int) -> t27 {
    // A rotation WRAPS the trits it pushes out; a shift discards them. The T3
    // backend aliased this to trit_shift_left until 19 August 2026, so it
    // silently computed a shift there while failing to compile on LLVM.
    let k: int = ((n % 27) + 27) % 27;
    let ts: [trit; 27] = unpack_trits(val);
    let mut out: [trit; 27] = [0; 27];
    for i in 0..27 {
        out[(i + k) % 27] = ts[i];
    }
    return t27_from_trits(out);
}

// Rotate a t27 value right by `n` trit positions.
fn trit_rotate_right(val: t27, n: int) -> t27 {
    let k: int = ((n % 27) + 27) % 27;
    return trit_rotate_left(val, 27 - k);
}

// Bitwise (trit-wise) AND of two t27 values: min per position.
fn t27_and(a: t27, b: t27) -> t27 ;  // native

// Trit-wise OR of two t27 values: max per position.
fn t27_or(a: t27, b: t27) -> t27 ;  // native

// Trit-wise negation of a t27: flip sign of every trit.
fn t27_neg(a: t27) -> t27 ;  // native

// Trit-wise XOR (addition mod 3, balanced) of two t27 values.
fn t27_xor(a: t27, b: t27) -> t27 {
    // Trit-wise mod-3 addition. Unlike t27_and / t27_or there is no single
    // T3ISA instruction for this (the ISA's BXOR is binary), so it is a loop
    // on both backends.
    let ta: [trit; 27] = unpack_trits(a);
    let tb: [trit; 27] = unpack_trits(b);
    let mut out: [trit; 27] = [0; 27];
    for i in 0..27 {
        out[i] = ta[i] txor tb[i];
    }
    return t27_from_trits(out);
}

// ---------------------------------------------------------------------------
// Packing and unpacking
// ---------------------------------------------------------------------------

// Pack a slice of trits (LST-first, i.e. index 0 = least significant trit)
// into a t27.  If the slice has fewer than 27 elements the remaining
// positions are filled with 0.  Panics if the slice has more than 27 trits.
fn pack_trits(trits: [trit]) -> t27 ;  // native

// Unpack all 27 trits of a t27 into a fixed-size array, LST-first.
fn unpack_trits(val: t27) -> [trit; 27] {
    let mut out: [trit; 27] = [0; 27];
    let mut v: int = t27_to_int(val);
    for i in 0..27 {
        // Balanced digit: a residue of 2 is written -1 and carries, which is
        // what keeps the digit set {-1, 0, +1}.
        let mut d: int = v % 3;
        if d == 2 { d = -1; }
        if d == -2 { d = 1; }
        out[i] = int_to_trit(d);
        v = (v - d) / 3;
    }
    return out;
}

// Pack up to 9 trits into a t9.
fn pack_t9(trits: [trit]) -> t9 {
    // Iterated rather than indexed: an unsized `[trit]` parameter is flat and
    // `.len()` on one does not currently compile, but `for` finds the hidden
    // length. Positions past 9 are ignored.
    let mut acc: int = 0;
    let mut place: int = 1;
    let mut i: int = 0;
    for t in trits {
        if i < 9 {
            acc = acc + trit_to_int(t) * place;
            place = place * 3;
        }
        i = i + 1;
    }
    return int_to_t9(acc);
}

// Unpack all 9 trits of a t9.
fn unpack_t9(val: t9) -> [trit; 9] {
    let mut out: [trit; 9] = [0; 9];
    let mut v: int = t9_to_int(val);
    for i in 0..9 {
        let mut d: int = v % 3;
        if d == 2 { d = -1; }
        if d == -2 { d = 1; }
        out[i] = int_to_trit(d);
        v = (v - d) / 3;
    }
    return out;
}

// Pack exactly 3 trits into a tryte.
fn tryte_from_trits(t2: trit, t1: trit, t0: trit) -> tryte ;  // native

// Unpack a tryte into its three constituent trits (MST-first order).
fn tryte_to_trits(t: tryte) -> (trit, trit, trit) {
    // MST-first, so the result feeds straight back into tryte_from_trits.
    let mut v: int = tryte_to_int(t);
    let mut d0: int = v % 3;
    if d0 == 2 { d0 = -1; }
    if d0 == -2 { d0 = 1; }
    v = (v - d0) / 3;
    let mut d1: int = v % 3;
    if d1 == 2 { d1 = -1; }
    if d1 == -2 { d1 = 1; }
    v = (v - d1) / 3;
    let mut d2: int = v % 3;
    if d2 == 2 { d2 = -1; }
    if d2 == -2 { d2 = 1; }
    return (int_to_trit(d2), int_to_trit(d1), int_to_trit(d0));
}

// Build a t27 from three t9 segments (high, mid, low).
fn t27_from_t9(hi: t9, mid: t9, lo: t9) -> t27 {
    // 3^9 and 3^18. Written out rather than computed so no intermediate ever
    // approaches 3^27, which is outside the 27-trit range and traps on T3.
    let p9: int = 19683;
    let p18: int = 387420489;
    return int_to_t27(t9_to_int(hi) * p18 + t9_to_int(mid) * p9 + t9_to_int(lo));
}

// Build a t27 directly from a 27-element trit array (LST-first).
fn t27_from_trits(trits: [trit; 27]) -> t27 {
    let mut acc: int = 0;
    let mut place: int = 1;
    for i in 0..27 {
        acc = acc + trit_to_int(trits[i]) * place;
        // Stop at 3^26: forming 3^27 would trap, and there is no position 27.
        if i < 26 { place = place * 3; }
    }
    return int_to_t27(acc);
}

// ---------------------------------------------------------------------------
// Conversion between integer and ternary word types
// ---------------------------------------------------------------------------

// Convert a plain int to t27, wrapping if the value is out of range.
fn int_to_t27(n: int) -> t27 ;  // native

// Convert t27 to int (sign-extended; preserves negative values).
fn t27_to_int(val: t27) -> int ;  // native

// Convert an int to a trit slice of exactly `width` trits (LST-first).
// Trits beyond `width` are discarded; missing positions are zero-padded.
fn int_to_trits(n: int, width: int) -> [trit] ;  // native

// Convert a t9 to int.
fn t9_to_int(val: t9) -> int ;  // native

// Convert int to t9, wrapping on overflow (range: -9841..9841).
fn int_to_t9(n: int) -> t9 ;  // native

// Convert a tryte to int (range: -13..13).
fn tryte_to_int(t: tryte) -> int ;  // native

// Convert int to tryte, panicking if out of range.
fn int_to_tryte(n: int) -> tryte ;  // native

// ---------------------------------------------------------------------------
// Ternary display / formatting
// ---------------------------------------------------------------------------

// Format a t27 as a balanced ternary string using "+", "0", "-" characters.
// Leading zero trits are suppressed (except for the value zero itself).
// Example: t27_to_str(5t) -> "+--"  (since 9-3-1 = 5)
fn t27_to_str(val: t27) -> str ;  // native

// Parse a balanced ternary string ("+", "0", "-" chars) into a t27.
// Ignores leading whitespace.  Panics on invalid characters.
fn t27_from_str(s: str) -> t27 {
    // Horner over MST-first text: each glyph shifts the accumulator by one
    // ternary place and adds its own value. Whitespace is skipped; any other
    // character contributes 0.
    let mut acc: int = 0;
    let n: int = s.len();
    for i in 0..n {
        let g: str = s.slice(i, i + 1);
        if g != " " {
            let mut d: int = 0;
            if g == "+" { d = 1; }
            if g == "-" { d = -1; }
            acc = acc * 3 + d;
        }
    }
    return int_to_t27(acc);
}

// Format a trit slice as a string (MST-first for readability).
fn trits_to_str(trits: [trit]) -> str ;  // native

// Format a t27 as a fixed-width 27-character string (with leading zeros).
fn t27_to_str_padded(val: t27) -> str {
    // Exactly 27 glyphs, MST-first, leading zeros kept. Aliased to the
    // unpadded t27_to_str on T3 until 19 August 2026, so it produced a
    // variable-width string there and failed to compile on LLVM.
    let ts: [trit; 27] = unpack_trits(val);
    let mut out: str = "";
    for i in 0..27 {
        // unpack_trits is LST-first, so prepending builds MST-first.
        let g: str = trits_to_str([ts[i]]);
        out = fmt::format("{}{}", [g, out]);
    }
    return out;
}

// Display a t27 in a human-readable table showing each trit position and
// its contribution to the total value.  Writes to stdout.
fn t27_explain(val: t27) {
    // Aliased to t27_to_str on T3 until 19 August 2026 — which returns a
    // string and prints nothing, so the call did the opposite of its purpose.
    let ts: [trit; 27] = unpack_trits(val);
    io::println(fmt::format("t27 {} = {}", [
        t27_to_str(val), fmt::show_int(t27_to_int(val))]));
    let mut place: int = 1;
    for i in 0..27 {
        let d: int = trit_to_int(ts[i]);
        if d != 0 {
            io::println(fmt::format("  3^{} x {} = {}", [
                fmt::show_int(i), fmt::show_int(d), fmt::show_int(d * place)]));
        }
        if i < 26 { place = place * 3; }
    }
}

// ---------------------------------------------------------------------------
// Ternary-specific predicates
// ---------------------------------------------------------------------------

// Return true if the value is negative in balanced ternary sense (MST is -).
fn is_negative_trit(val: t27) -> bool {
    // In balanced ternary the sign of the value IS the sign of its most
    // significant non-zero trit — there is no sign bit and no negative zero —
    // so this needs no trit inspection at all.
    return t27_to_int(val) < 0;
}

// Return the most significant non-zero trit position (0-indexed from LST).
// Returns -1 for the value zero.
fn highest_trit_pos(val: t27) -> int {
    let ts: [trit; 27] = unpack_trits(val);
    let mut pos: int = -1;
    for i in 0..27 {
        if trit_to_int(ts[i]) != 0 { pos = i; }
    }
    return pos;
}

// Count the number of non-zero trits in val.
fn count_nonzero_trits(val: t27) -> int {
    let ts: [trit; 27] = unpack_trits(val);
    let mut c: int = 0;
    for i in 0..27 {
        if trit_to_int(ts[i]) != 0 { c = c + 1; }
    }
    return c;
}

// Return a t27 whose value is 3^n (the "ternary unit" at position n).
// Positions outside 0..26 return 0: 3^27 is outside the 27-trit range and
// forming it would trap on T3.
fn trit_unit(n: int) -> t27 {
    if n < 0 { return int_to_t27(0); }
    if n > 26 { return int_to_t27(0); }
    let mut p: int = 1;
    for _i in 0..n { p = p * 3; }
    return int_to_t27(p);
}
