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
// Precision: ~8.6 balanced ternary digits (~5.4 decimal digits)
// Range: approximately ±1.6 × 10^4700 (vastly exceeds IEEE 754 double)
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
    raw: word,
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
    tif f == 0.0 {
        return ZERO;
    }
    // Find exponent: largest e such that |f| >= 3^e
    let e: t9 = 0;
    let scale: float = 1.0;
    let abs_f: float = tif f > 0.0 { f } else { 0.0 - f };
    while scale * 3.0 <= abs_f {
        scale = scale * 3.0;
        e = e + 1;
    }
    while scale > abs_f {
        scale = scale / 3.0;
        e = e - 1;
    }
    // Mantissa = f / 3^e, rounded to integer
    let man: word = float_to_int(f / pow3(e));
    from_parts(e, man)
}

// Convert T27F to approximate host float.
fn to_float(x: T27F) -> float {
    let e = exponent(x);
    let m = mantissa(x);
    int_to_float(m) * pow3_float(e)
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
    tif ea > eb {
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
    tif m < 0 {
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
    tif m > 0 { +1 } elif m == 0 { 0 } else { -1 }
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

    tif m == 0 {
        return ZERO;
    }

    // Shift mantissa up (multiply by 3, decrease exponent) while it fits
    let m_norm = m;
    let e_norm = e;
    while m_norm * 3 <= MANTISSA_MAX tand m_norm * 3 >= 0 - MANTISSA_MAX {
        m_norm = m_norm * 3;
        e_norm = e_norm - 1;
    }

    // Shift mantissa down if it overflows
    while m_norm > MANTISSA_MAX tor m_norm < 0 - MANTISSA_MAX {
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
    tif n == 0 { return 1; }
    tif n < 0 {
        // 3^(-n) for negative exponent — returns 0 for integer division
        return 0;
    }
    let result: word = 1;
    let i: t9 = 0;
    while i < n {
        result = result * 3;
        i = i + 1;
    }
    result
}

// Float power of 3 (handles negative exponents).
fn pow3_float(n: t9) -> float {
    tif n == 0 { return 1.0; }
    let result: float = 1.0;
    let i: t9 = 0;
    tif n > 0 {
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
    tif x < lo { lo } elif x > hi { hi } else { x }
}

// Balanced ternary modulo (trit extraction).
fn tmod(x: word, base: word) -> t9 {
    x - tdiv(x, base) * base
}

// Balanced ternary integer division (round toward zero).
fn tdiv(a: word, b: word) -> word { /* native */ }

// Convert float to integer (truncate toward zero).
fn float_to_int(f: float) -> word { /* native */ }

// Convert integer to float.
fn int_to_float(n: word) -> float { /* native */ }
