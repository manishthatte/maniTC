// stdlib/std/t27f.mt
// T27F — Balanced Ternary Floating-Point Representation
//
// A 27-trit floating-point format with no sign bit, no dual zero, and no NaN.
// This is the native floating-point type for photonic-ternary hardware.
//
// Format:  [9-trit exponent | 18-trit mantissa]
//   - Exponent: balanced ternary, range -9841 to +9841 (base-3 exponent)
//   - Mantissa: balanced ternary, range -(3^18-1)/2 to +(3^18-1)/2
//   - Value = mantissa × 3^exponent
//
// Properties (advantages over IEEE 754):
//   - No sign bit: sign is inherent in balanced ternary (negative mantissa = negative number)
//   - No dual zero: exactly one zero representation (all trits = 0)
//   - No NaN: every 27-trit pattern is a valid number
//   - No denormals: uniform precision across the range
//   - Negation is exact trit-flip (no special cases)
//   - Rounding by truncation is unbiased (balanced ternary truncation rounds to nearest)
//
// Precision: 18 balanced ternary digits of mantissa (~8.6 decimal digits).
//            Sits between IEEE 754 single (~7.2) and double (~15.9).
// Range:     up to ~4.3 × 10^4703 (IEEE 754 double tops out near 1.8 × 10^308).
//
// Usage:
//   use std::t27f;
//   let x = t27f::from_parts(0, 100);  // 100 × 3^0 = 100
//   let y = t27f::from_float(3.14159);
//   let z = t27f::add(x, y);

// ---------------------------------------------------------------------------
// T27F representation
// ---------------------------------------------------------------------------

// A balanced ternary floating-point number stored as a 27-trit word.
// Trits 0-8 (low 9): exponent
// Trits 9-26 (high 18): mantissa
struct T27F {
    pub raw: word,
}

// Maximum mantissa value: (3^18 - 1) / 2 = 193,710,244
let MANTISSA_MAX: word = 193710244;

// Maximum exponent value: (3^9 - 1) / 2 = 9841
let EXPONENT_MAX: word = 9841;

// The T27F representation of zero.
let ZERO: T27F = T27F { raw: 0 };

// The T27F representation of one (mantissa=1, exponent=0).
let ONE: T27F = T27F { raw: 1 * 19683 };  // 1 << 9 trits = 3^9

// ---------------------------------------------------------------------------
// Construction and decomposition
// ---------------------------------------------------------------------------

// Construct a T27F from exponent and mantissa parts.
//   value = mantissa × 3^exponent
fn from_parts(exponent: t9, mantissa: word) -> T27F {
    // Pack: low 9 trits = exponent, high 18 trits = mantissa
    let exp_clamped = tclamp(exponent, 0 - EXPONENT_MAX, EXPONENT_MAX);
    let man_clamped = tclamp(mantissa, 0 - MANTISSA_MAX, MANTISSA_MAX);
    T27F { raw: exp_clamped + man_clamped * 19683 }
}

// Extract the exponent (low 9 trits) from a T27F.
fn exponent(x: T27F) -> t9 {
    tmod(x.raw, 19683)  // x.raw mod 3^9
}

// Extract the mantissa (high 18 trits) from a T27F.
fn mantissa(x: T27F) -> word {
    tdiv(x.raw, 19683)  // x.raw / 3^9
}

// Approximate conversion from host float to T27F.
fn from_float(f: float) -> T27F {
    if f == 0.0 {
        return ZERO;
    }
    // Find exponent: largest e such that |f| >= 3^e
    let mut e: t9 = 0;
    let mut scale: float = 1.0;
    let abs_f: float = if f > 0.0 { f } else { 0.0 - f };
    while scale * 3.0 <= abs_f {
        scale = scale * 3.0;
        e = e + 1;
    }
    while scale > abs_f {
        scale = scale / 3.0;
        e = e - 1;
    }
    // report.txt P49. THIS NORMALISED THE WRONG WAY ROUND, and `normalize`
    // twenty lines below says which way is right: "shift mantissa up
    // (multiply by 3, decrease exponent) while it fits", exactly as IEEE
    // keeps an implicit leading 1, so a normalized exponent is NEGATIVE.
    //
    // The loops above maximise the EXPONENT: they leave the largest e with
    // 3^e <= |f|, which forces |f| / 3^e into [1,3), and `float_to_int` then
    // truncates it to 1 or 2. Seventeen of the eighteen mantissa trits were
    // always zero and MANTISSA_MAX was unreachable by construction, so
    // `from_float(4.0)` came back as 3 and `from_float(0.5)` as one third —
    // errors of a quarter to a third, identical on both backends, which is
    // why cross-backend parity could not see it either.
    //
    // Normalising AFTER the fact does not help and it is worth saying why:
    // by then the precision is gone, and multiplying a mantissa of 1 by three
    // seventeen times only reproduces 3. The exponent has to be chosen BEFORE
    // the mantissa is taken, which is what this loop does — the smallest e
    // for which |f| / 3^e still fits. It runs at most ~18 times, since each
    // step multiplies the quotient by three and MANTISSA_MAX is about 3^17.6.
    let man_max: float = int_to_float(MANTISSA_MAX);
    while abs_f / pow3_float(e - 1) <= man_max {
        e = e - 1;
    }
    // Mantissa = f / 3^e, ROUNDED to integer. pow3_float also handles
    // negative exponents (integer pow3 would return 0 and divide by zero).
    //
    // The rounding is the second half of P49 and it is worth a line. Once the
    // exponent is chosen to fill the mantissa, the quotient is a large number
    // reached by dividing in binary, so it lands just under the integer it
    // should be: 12.0 / 3^-15 computes as 172186883.99999997, and
    // `float_to_int` truncates toward zero — giving 172186883 and a round trip
    // of 11.999999930. One half added before the truncation makes it
    // 172186884, and 172186884 / 3^15 is 12 exactly. `float_to_int` itself is
    // left alone: it is documented as truncating and has other callers.
    let scaled: float = f / pow3_float(e);
    let man: word = if scaled >= 0.0 {
        float_to_int(scaled + 0.5)
    } else {
        float_to_int(scaled - 0.5)
    };
    from_parts(e, man)
}

// Convert T27F to approximate host float.
fn to_float(x: T27F) -> float {
    let e = exponent(x);
    let m = mantissa(x);
    int_to_float(m) * pow3_float(e)
}

// WHY THIS EXISTS (23 August 2026). `to_float` is lossy in a way that is easy
// to miss: it reconstructs through binary `float`, and a normalized T27F has a
// negative exponent, so it divides by 3.0 repeatedly. One third is not
// representable in binary, and 100 + 200 comes back as 300.00000000000006 on
// BOTH backends. Every exact comparison against a T27F result therefore fails,
// which is not a rounding subtlety anyone should have to rediscover.
//
// The obvious hand-rolled alternative is worse. Writing
//
//     mantissa(x) * pow3(exponent(x))
//
// looks right and is silently, totally wrong for every normalized value:
// `normalize` maximises mantissa precision (it shifts up while the mantissa
// fits, exactly as IEEE keeps an implicit leading 1), so a normalized
// exponent is NEGATIVE -- and integer `pow3` returns 0 for a negative
// argument. The product is 0, not the value. maniTC's own test
// 20_t27f_float.mt was written that way and asserted `== 300` against 0.
//
// Reconstruct the EXACT integer value of `x`, for values that are integers:
// multiply the mantissa when the exponent is non-negative, divide when it is
// negative. The division is exact whenever `x` really does hold an integer,
// because normalize scaled the mantissa up by that same power of three.
fn to_int(x: T27F) -> word {
    let e = exponent(x);
    let m = mantissa(x);
    if e >= 0 {
        return m * pow3(e);
    }
    tdiv(m, pow3(0 - e))
}

// ---------------------------------------------------------------------------
// Arithmetic operations
// ---------------------------------------------------------------------------

// Add two T27F values.
// Aligns exponents, adds mantissas, normalizes result.
fn add(a: T27F, b: T27F) -> T27F {
    let ea = exponent(a);
    let eb = exponent(b);
    let ma = mantissa(a);
    let mb = mantissa(b);

    // Align to the smaller exponent
    if ea > eb {
        // Scale a's mantissa up by 3^(ea-eb)
        let shift = ea - eb;
        let ma_scaled = ma * pow3(shift);
        let sum = ma_scaled + mb;
        normalize(from_parts(eb, sum))
    } elif ea < eb {
        let shift = eb - ea;
        let mb_scaled = mb * pow3(shift);
        let sum = ma + mb_scaled;
        normalize(from_parts(ea, sum))
    } else {
        normalize(from_parts(ea, ma + mb))
    }
}

// Subtract: a - b.  Negation is exact trit-flip.
fn sub(a: T27F, b: T27F) -> T27F {
    add(a, neg(b))
}

// Multiply two T27F values.
// Mantissas multiply, exponents add.
fn mul(a: T27F, b: T27F) -> T27F {
    let ea = exponent(a);
    let eb = exponent(b);
    let ma = mantissa(a);
    let mb = mantissa(b);
    normalize(from_parts(ea + eb, ma * mb))
}

// Negate a T27F.  Exact: just negate the mantissa (trit-flip).
fn neg(x: T27F) -> T27F {
    let e = exponent(x);
    let m = mantissa(x);
    from_parts(e, 0 - m)
}

// Absolute value.
fn abs(x: T27F) -> T27F {
    let m = mantissa(x);
    if m < 0 {
        neg(x)
    } else {
        x
    }
}

// Compare two T27F values.
// Returns: +1 if a > b, 0 if a == b, -1 if a < b.
fn compare(a: T27F, b: T27F) -> trit {
    let diff = sub(a, b);
    let m = mantissa(diff);
    if m > 0 { +1 } elif m == 0 { 0 } else { -1 }
}

// Check if a T27F is zero.
fn is_zero(x: T27F) -> T3Bool {
    mantissa(x) == 0
}

// ---------------------------------------------------------------------------
// Normalization
// ---------------------------------------------------------------------------

// Normalize: shift mantissa so it uses maximum precision.
// In balanced ternary, normalization means the most significant trit
// of the mantissa is nonzero (unless the value is zero).
fn normalize(x: T27F) -> T27F {
    let e = exponent(x);
    let m = mantissa(x);

    if m == 0 {
        return ZERO;
    }

    // Shift mantissa up (multiply by 3, decrease exponent) while it fits
    let mut m_norm = m;
    let mut e_norm = e;
    while m_norm * 3 <= MANTISSA_MAX && m_norm * 3 >= 0 - MANTISSA_MAX {
        m_norm = m_norm * 3;
        e_norm = e_norm - 1;
    }

    // Shift mantissa down if it overflows
    while m_norm > MANTISSA_MAX || m_norm < 0 - MANTISSA_MAX {
        m_norm = tdiv(m_norm, 3);
        e_norm = e_norm + 1;
    }

    from_parts(e_norm, m_norm)
}

// ---------------------------------------------------------------------------
// Utility helpers
// ---------------------------------------------------------------------------

// Integer power of 3.
fn pow3(n: t9) -> word {
    if n == 0 { return 1; }
    if n < 0 {
        // 3^(-n) for negative exponent — returns 0 for integer division
        return 0;
    }
    let mut result: word = 1;
    let mut i: t9 = 0;
    while i < n {
        result = result * 3;
        i = i + 1;
    }
    result
}

// Float power of 3 (handles negative exponents).
fn pow3_float(n: t9) -> float {
    if n == 0 { return 1.0; }
    let mut result: float = 1.0;
    let mut i: t9 = 0;
    if n > 0 {
        while i < n {
            result = result * 3.0;
            i = i + 1;
        }
    } else {
        let abs_n = 0 - n;
        while i < abs_n {
            result = result / 3.0;
            i = i + 1;
        }
    }
    result
}

// Clamp a value to a range.
fn tclamp(x: word, lo: word, hi: word) -> word {
    if x < lo { lo } elif x > hi { hi } else { x }
}

// Balanced ternary modulo (trit extraction).
fn tmod(x: word, base: word) -> t9 {
    x - tdiv(x, base) * base
}

// Balanced ternary integer division (round to nearest; ties cannot occur
// for the odd divisors used in this module). Dropping low trits of a
// balanced ternary number IS round-to-nearest division, and the
// from_parts/exponent/mantissa packing relies on exactly that.
// Requires b > 0 (this module divides only by 3 and 19683).
fn tdiv(a: word, b: word) -> word {
    let ai: int = a as int;
    let bi: int = b as int;
    let mut q: int = ai / bi;
    let r: int = ai - q * bi;
    let half: int = (bi - 1) / 2;
    if r > half {
        q = q + 1;
    }
    if 0 - r > half {
        q = q - 1;
    }
    return q as word;
}

// Convert float to integer (truncate toward zero).
fn float_to_int(f: float) -> word {
    return (f as int) as word;
}

// Convert integer to float.
fn int_to_float(n: word) -> float {
    return (n as int) as float;
}
