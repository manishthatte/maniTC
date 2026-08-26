// stdlib/std/bridge.mt
// Binary-ternary bridge for maniT (Claim 17).
//
// This module provides conversion functions between balanced ternary and
// binary representations, enabling interoperability between the native
// ternary data path and external binary systems.
//
// Encoding scheme — two bits per trit:
//
//   Trit value    Bit pair (b1, b0)
//   ----------    -----------------
//      +1             (1, 0)
//       0             (0, 0)
//      -1             (0, 1)
//      --             (1, 1)  ← INVALID, not a legal trit
//
// This two-bit encoding preserves the sign symmetry of balanced ternary:
// +1 and -1 each have exactly one bit set, while the zero trit has no
// bits set.  The unused (1,1) pattern serves as an error sentinel.
//
// Width mappings:
//   1 trit   →  2 binary bits
//   tryte (3 trits)  →  6 binary bits  (stored in a t9)
//   t27   (27 trits) →  54 binary bits (stored as [trit; 54])
//
// Usage:
//   use std::bridge;
//   let (b1, b0) = bridge::trit_to_bits(+);
//   let t = bridge::bits_to_trit(b1, b0);

// ---------------------------------------------------------------------------
// Single trit ↔ two-bit encoding
// ---------------------------------------------------------------------------

// Encode one trit as a two-bit pair.
//
// Mapping:
//   +1  → (1, 0)   — high bit set
//    0  → (0, 0)   — no bits set
//   -1  → (0, 1)   — low bit set
//
// The returned tuple (b1, b0) uses trit values 0 and +1 to represent
// binary 0 and 1 respectively, since maniT has no native bit type.
fn trit_to_bits(t: trit) -> (trit, trit) {
    let one: trit = +1;
    let zero: trit = 0;
    if t > 0 {
        return (one, zero);
    }
    if t < 0 {
        return (zero, one);
    }
    return (zero, zero);
}

// Decode a two-bit pair back to a single trit.
//
// Mapping:
//   (1, 0)  → +1
//   (0, 0)  →  0
//   (0, 1)  → -1
//   (1, 1)  →  0   (invalid pattern — returns zero as safe fallback)
//
// Use is_valid_encoding() to check for the invalid (1,1) pattern before
// calling this function if error detection is required.
fn bits_to_trit(b1: trit, b0: trit) -> trit {
    let zero: trit = 0;
    if b1 > 0 && b0 > 0 {
        return zero;
    }
    if b1 > 0 {
        let pos: trit = +1;
        return pos;
    }
    if b0 > 0 {
        let neg: trit = -1;
        return neg;
    }
    return zero;
}

// Check whether a two-bit pattern is a valid trit encoding.
//
// Returns true (+1) for valid patterns (0,0), (0,1), (1,0).
// Returns false (-1) for the invalid pattern (1,1).
//
// In the two-bit-per-trit scheme, (1,1) has no corresponding trit value.
// This function allows callers to detect corruption or encoding errors
// at the binary-ternary boundary before conversion.
fn is_valid_encoding(b1: trit, b0: trit) -> bool3 {
    if b1 > 0 && b0 > 0 {
        return false;
    }
    return true;
}

// ---------------------------------------------------------------------------
// Tryte (3 trits) ↔ binary encoding
// ---------------------------------------------------------------------------

// Encode a 3-trit tryte as 6 binary bits, packed into a t9.
//
// Each of the three trits in the tryte is encoded as two bits using
// the trit_to_bits() scheme, producing 6 bits total.  The bits are
// packed into the lowest 6 trit positions of the returned t9:
//
//   t9 positions [5..4] = bits for trit 2 (MST of tryte)
//   t9 positions [3..2] = bits for trit 1
//   t9 positions [1..0] = bits for trit 0 (LST of tryte)
//
// Remaining t9 positions [8..6] are zero-filled.
//
// Within each pair, the lower position holds b0 and the upper holds b1
// (so position 2k = b0 of trit k, position 2k+1 = b1 of trit k).
//
// "BYTE" IS A MISNOMER AND THE RANGE IS 0..=273, NOT 0..=255 (report.txt P57).
// Six TERNARY positions is 3^6 = 729 values, not 2^6 = 64, and the encoding
// actually uses 0..=273 — trytes +11, +12 and +13 encode to 271, 270 and 273.
// The return type is `t9` and this header has always said "packed into a t9",
// so nothing is lost; but a caller who reads the NAME and stores the result in
// eight bits truncates exactly those three values.
//
// The pair IS lossless in the direction that matters: all 27 trytes survive
// `tryte_to_byte` then `byte_to_tryte`, measured on both backends. Scanning
// all 256 bytes through `byte_to_tryte` then `tryte_to_byte` and finding 232
// that do not round-trip is NOT evidence of a defect — 229 of them are not
// valid encodings at all (27 values cannot have 256 distinct encodings) and
// the other 3 are the canonical encodings above 255.
fn tryte_to_byte(ty: tryte) -> t9 {
    let mut n: int = ty as int;
    let mut packed: int = 0;
    let mut place: int = 1;
    let mut k: int = 0;
    while k < 3 {
        // Peel off the least significant balanced-ternary digit.
        let mut d: int = n % 3;
        if d == 2 {
            d = -1;
        }
        if d == -2 {
            d = 1;
        }
        n = (n - d) / 3;
        // Encode: +1 → b1 set (position 2k+1), -1 → b0 set (position 2k).
        if d == 1 {
            packed = packed + place * 3;
        }
        if d == -1 {
            packed = packed + place;
        }
        place = place * 9;
        k = k + 1;
    }
    return packed as t9;
}

// Decode 6 binary bits (stored in a t9) back to a 3-trit tryte.
//
// Reads the lowest 6 trit positions of the t9, interpreting each
// consecutive pair as a trit encoding per bits_to_trit().
// Positions [8..6] are ignored.
fn byte_to_tryte(b: t9) -> tryte {
    let mut n: int = b as int;
    let mut value: int = 0;
    let mut place: int = 1;
    let mut k: int = 0;
    while k < 3 {
        // Positions hold only 0/+1 digits, so plain base-3 peeling works.
        let b0: int = n % 3;
        n = n / 3;
        let b1: int = n % 3;
        n = n / 3;
        // (1,0) → +1, (0,1) → -1, (0,0) and invalid (1,1) → 0.
        value = value + (b1 - b0) * place;
        place = place * 3;
        k = k + 1;
    }
    return value as tryte;
}

// ---------------------------------------------------------------------------
// Word (27 trits) ↔ 54-bit binary encoding
// ---------------------------------------------------------------------------

// Encode a 27-trit word (t27) as 54 binary bits.
//
// Each of the 27 trits is encoded as a two-bit pair, producing a
// 54-element array of trit values (each either 0 or +1, representing
// binary 0 and 1).
//
// Layout:
//   bits[0..1]   = encoding of trit 0  (LST of word)
//   bits[2..3]   = encoding of trit 1
//   ...
//   bits[52..53] = encoding of trit 26 (MST of word)
//
// This is the format expected by binary DMA, PCIe, or USB bridges
// described in Claim 17.
fn word_to_binary(w: t27) -> [trit; 54] {
    let bits: [trit; 54] = [0; 54];
    let one: trit = +1;
    let mut n: int = w as int;
    let mut k: int = 0;
    while k < 27 {
        let mut d: int = n % 3;
        if d == 2 {
            d = -1;
        }
        if d == -2 {
            d = 1;
        }
        n = (n - d) / 3;
        if d == 1 {
            bits[2 * k + 1] = one;
        }
        if d == -1 {
            bits[2 * k] = one;
        }
        k = k + 1;
    }
    return bits;
}

// Decode 54 binary bits back to a 27-trit word (t27).
//
// Reads consecutive two-bit pairs from the input array and converts
// each to a trit using the bits_to_trit() decoding.
//
// If any two-bit pair is the invalid (1,1) pattern, the corresponding
// trit position is set to 0 (safe fallback).  For strict validation,
// check each pair with is_valid_encoding() before calling this function.
fn binary_to_word(bits: [trit; 54]) -> t27 {
    let mut value: int = 0;
    let mut place: int = 1;
    let mut k: int = 0;
    while k < 27 {
        let mut b0: int = 0;
        let mut b1: int = 0;
        if bits[2 * k] > 0 {
            b0 = 1;
        }
        if bits[2 * k + 1] > 0 {
            b1 = 1;
        }
        value = value + (b1 - b0) * place;
        // report.txt P56. The advance is GUARDED because the last one is dead
        // and fatal. On k = 26 `place` is already 3^26; multiplying once more
        // on the way out of the loop produces 3^27 = 7625597484987, which is
        // roughly twice the largest `int` and traps T3:
        //
        //   TRAP: int multiplication overflow: result 7625597484987 is
        //   outside the 27-trit range
        //
        // The result is COMPLETE before that multiplication — it is dead, and
        // it killed the module's headline operation on the native backend.
        // LLVM computes it in i64, never notices, and returns the right
        // answer, so this was a divergence in which T3 WAS THE HONEST ONE:
        // it reported a real overflow in code that had no business performing
        // the multiplication at all.
        if k < 26 {
            place = place * 3;
        }
        k = k + 1;
    }
    return value as t27;
}

// ---------------------------------------------------------------------------
// Bulk validation
// ---------------------------------------------------------------------------

// Validate an entire 54-bit binary buffer for encoding correctness.
//
// Returns true if every two-bit pair in the buffer is a valid trit
// encoding (i.e., no (1,1) patterns are present).  Returns false if
// any invalid pair is found.
//
// This is the fast-path check for incoming binary data at the
// binary-ternary bridge boundary.
fn validate_binary_buffer(bits: [trit; 54]) -> bool3 {
    let mut k: int = 0;
    while k < 27 {
        if bits[2 * k] > 0 && bits[2 * k + 1] > 0 {
            return false;
        }
        k = k + 1;
    }
    return true;
}

// ---------------------------------------------------------------------------
// Convenience: integer ↔ binary round-trip
// ---------------------------------------------------------------------------

// Convert a t27 value to its binary integer equivalent and back.
// These are thin wrappers combining the word/binary conversions with
// standard packing, useful for FFI and binary system interfacing.

// Pack a 54-bit binary array into a pair of t27 values (high 27 bits, low 27 bits).
// This enables storing a binary-encoded word in two native ternary registers.
// Each bit occupies one trit position (digit 0 or +1) of its t27.
fn binary_to_t27_pair(bits: [trit; 54]) -> (t27, t27) {
    let mut lo: int = 0;
    let mut hi: int = 0;
    let mut place: int = 1;
    let mut k: int = 0;
    while k < 27 {
        if bits[k] > 0 {
            lo = lo + place;
        }
        if bits[k + 27] > 0 {
            hi = hi + place;
        }
        // Guarded for the same reason as `binary_to_word` above (report.txt
        // P56): on k = 26 the advance would form 3^27 and trap T3 after both
        // accumulators are already complete. THIS WAS THE SECOND SITE of the
        // same one-line defect, and the pattern — accumulate against a place
        // value, advance unconditionally, loop to the width of the type — is
        // what to grep for. `stdlib/ternary.mt` had already carried this exact
        // guard at two sites, with a comment naming 3^27; the knowledge was in
        // the codebase and had simply not been propagated here.
        if k < 26 {
            place = place * 3;
        }
        k = k + 1;
    }
    return (hi as t27, lo as t27);
}

// Unpack a pair of t27 values (high, low) back into a 54-bit binary array.
fn t27_pair_to_binary(hi: t27, lo: t27) -> [trit; 54] {
    let bits: [trit; 54] = [0; 54];
    let one: trit = +1;
    let mut lo_n: int = lo as int;
    let mut hi_n: int = hi as int;
    let mut k: int = 0;
    while k < 27 {
        if lo_n % 3 == 1 {
            bits[k] = one;
        }
        if hi_n % 3 == 1 {
            bits[k + 27] = one;
        }
        lo_n = lo_n / 3;
        hi_n = hi_n / 3;
        k = k + 1;
    }
    return bits;
}
