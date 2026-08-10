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
fn trit_add(a: trit, b: trit) -> (trit, trit) ;  // native

// Multiply two trits.  Result is always a single trit (no carry needed).
// trit_mul(+, +) -> +    (+1 * +1 = +1)
// trit_mul(+, -) -> -    (+1 * -1 = -1)
// trit_mul(-, -) -> +    (-1 * -1 = +1)
// trit_mul(0, _) -> 0    (0 times anything is 0)
fn trit_mul(a: trit, b: trit) -> trit ;  // native

// Negate a trit (ternary NOT): + <-> -, 0 stays 0.
fn trit_neg(t: trit) -> trit ;  // native

// Ternary AND (Łukasiewicz / min): min(a, b) in {-1, 0, +1}.
fn trit_and(a: trit, b: trit) -> trit ;  // native

// Ternary OR (max): max(a, b) in {-1, 0, +1}.
fn trit_or(a: trit, b: trit) -> trit ;  // native

// Ternary XOR (addition mod 3, mapped to balanced representation).
// trit_xor(+, -) -> 0   (+1 + -1 mod 3 = 0)
// trit_xor(+, 0) -> +   (+1 + 0  mod 3 = +1)
// trit_xor(+, +) -> -   (+1 + +1 mod 3 = -1, i.e. 2 -> -1 in balanced)
fn trit_xor(a: trit, b: trit) -> trit ;  // native

// Consensus / median of three trits (used in fault-tolerant TMR logic).
// Returns the "majority vote" trit.
fn trit_median(a: trit, b: trit, c: trit) -> trit ;  // native

// Convert a trit to its integer equivalent: + -> +1, 0 -> 0, - -> -1.
fn trit_to_int(t: trit) -> int ;  // native

// Convert an integer in {-1, 0, +1} to a trit.  Panics for other values.
fn int_to_trit(n: int) -> trit ;  // native

// Return the sign of a trit as a trit (identity function, included for
// symmetry with math::sign).
fn trit_sign(t: trit) -> trit ;  // native

// ---------------------------------------------------------------------------
// Multi-trit (word-level) arithmetic
// ---------------------------------------------------------------------------

// Left-shift a t27 value by `n` trit positions (multiply by 3^n).
// Trits shifted beyond position 26 are discarded (wrapping behaviour).
fn trit_shift_left(val: t27, n: int) -> t27 ;  // native

// Right-shift a t27 value by `n` trit positions (divide by 3^n, truncate).
fn trit_shift_right(val: t27, n: int) -> t27 ;  // native

// Rotate a t27 value left by `n` trit positions.
fn trit_rotate_left(val: t27, n: int) -> t27 ;  // native

// Rotate a t27 value right by `n` trit positions.
fn trit_rotate_right(val: t27, n: int) -> t27 ;  // native

// Bitwise (trit-wise) AND of two t27 values: min per position.
fn t27_and(a: t27, b: t27) -> t27 ;  // native

// Trit-wise OR of two t27 values: max per position.
fn t27_or(a: t27, b: t27) -> t27 ;  // native

// Trit-wise negation of a t27: flip sign of every trit.
fn t27_neg(a: t27) -> t27 ;  // native

// Trit-wise XOR (addition mod 3, balanced) of two t27 values.
fn t27_xor(a: t27, b: t27) -> t27 ;  // native

// ---------------------------------------------------------------------------
// Packing and unpacking
// ---------------------------------------------------------------------------

// Pack a slice of trits (LST-first, i.e. index 0 = least significant trit)
// into a t27.  If the slice has fewer than 27 elements the remaining
// positions are filled with 0.  Panics if the slice has more than 27 trits.
fn pack_trits(trits: [trit]) -> t27 ;  // native

// Unpack all 27 trits of a t27 into a fixed-size array, LST-first.
fn unpack_trits(val: t27) -> [trit; 27] ;  // native

// Pack up to 9 trits into a t9.
fn pack_t9(trits: [trit]) -> t9 ;  // native

// Unpack all 9 trits of a t9.
fn unpack_t9(val: t9) -> [trit; 9] ;  // native

// Pack exactly 3 trits into a tryte.
fn tryte_from_trits(t2: trit, t1: trit, t0: trit) -> tryte ;  // native

// Unpack a tryte into its three constituent trits (MST-first order).
fn tryte_to_trits(t: tryte) -> (trit, trit, trit) ;  // native

// Build a t27 from three t9 segments (high, mid, low).
fn t27_from_t9(hi: t9, mid: t9, lo: t9) -> t27 ;  // native

// Build a t27 directly from a 27-element trit array (LST-first).
fn t27_from_trits(trits: [trit; 27]) -> t27 ;  // native

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
fn t27_from_str(s: str) -> t27 ;  // native

// Format a trit slice as a string (MST-first for readability).
fn trits_to_str(trits: [trit]) -> str ;  // native

// Format a t27 as a fixed-width 27-character string (with leading zeros).
fn t27_to_str_padded(val: t27) -> str ;  // native

// Display a t27 in a human-readable table showing each trit position and
// its contribution to the total value.  Writes to stdout.
fn t27_explain(val: t27) ;  // native

// ---------------------------------------------------------------------------
// Ternary-specific predicates
// ---------------------------------------------------------------------------

// Return true if the value is negative in balanced ternary sense (MST is -).
fn is_negative_trit(val: t27) -> bool ;  // native

// Return the most significant non-zero trit position (0-indexed from LST).
// Returns -1 for the value zero.
fn highest_trit_pos(val: t27) -> int ;  // native

// Count the number of non-zero trits in val.
fn count_nonzero_trits(val: t27) -> int ;  // native

// Return a t27 whose value is 3^n (the "ternary unit" at position n).
fn trit_unit(n: int) -> t27 ;  // native
