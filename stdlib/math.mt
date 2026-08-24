// stdlib/std/math.mt
// Mathematical functions and constants for maniT.
//
// Includes standard floating-point math, integer utilities, and
// ternary-native operations that have no direct binary analogue.
//
// Usage:
//   use std::math;
//   let r = math::sqrt(2.0);
//   let t = math::to_balanced_ternary(42);

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

// Ratio of a circle's circumference to its diameter.
let PI: float = 3.141592653589793;

// Euler's number — base of the natural logarithm.
let E: float = 2.718281828459045;

// log₂(3) — useful when converting between binary and ternary widths.
//
// Corrected 20 August 2026: was 1.5849625007211563, one ulp high. Both this and
// LOG3_2 below are now the correctly-rounded doubles, computed at 60 decimal
// digits and rounded once, rather than derived from float arithmetic — which is
// how the error arose. (log(2)/log(3) in doubles gives 0.6309297535714575,
// itself a further ulp off, so the two ways of "computing" LOG3_2 disagree in
// the last place and neither is right.)
//
// These two are the binary↔ternary width constants, so an ulp here is not
// cosmetic: they are what a width calculation multiplies by.
let LOG2_3: float = 1.584962500721156;

// log₃(2) — inverse of LOG2_3.
let LOG3_2: float = 0.6309297535714574;

// The largest int value valid on EVERY backend: (3^27-1)/2.
//
// These two were `9223372036854775807` / `-9223372036854775808` until 20 August
// 2026, which was wrong twice over.
//
// First, `-9223372036854775808` is **not lexable** — the lexer reads the
// magnitude before applying the sign, and 9223372036854775808 overflows i64, so
// the line failed with `invalid integer literal`. That alone blocked `math` from
// becoming a source module, because a source module gets lexed.
//
// Second and more important, the value was a binary-machine assumption. **A T3
// `int` is 27 trits, not 64 bits** — the range is [-3812798742493,
// 3812798742493] and T3 *traps* on overflow where LLVM silently wraps. So an
// `INT_MAX` of 2^63-1 was a number the ternary backend cannot even hold, and a
// program using it would trap on the machine this stack is built for.
//
// A constant must be true on every backend, so it takes the intersection: this
// is the widest int that is portable. It is the same range as T27_MAX/T27_MIN
// below, which is not a coincidence — on T3 the native int IS a t27. LLVM's own
// int is wider (64-bit), and code that deliberately relies on that is welcome
// to write the literal out; it just is not portable, and this constant does not
// pretend otherwise.
let INT_MAX: int = 3812798742493;

// The smallest (most negative) int value valid on every backend.
let INT_MIN: int = -3812798742493;

// Maximum value of a t27 (27-trit balanced ternary integer): (3^27-1)/2
let T27_MAX: t27 = 0t+++++++++++++++++++++++++++; // +1 in each of 27 positions

// Minimum value of a t27.
let T27_MIN: t27 = 0t---------------------------; // -1 in each of 27 positions

// ---------------------------------------------------------------------------
// Basic numeric utilities
// ---------------------------------------------------------------------------

// Absolute value of an integer.
fn abs(n: int) -> int { if n < 0 { return -n; } return n; }

// Absolute value of a float.
// Absolute value of a float.
fn fabs(f: float) -> float {
    let mut out: float = f;
    if f <= 0.0 {
        out = 0.0 - f;
    }
    out
}
// Smaller of two integers.
fn min(a: int, b: int) -> int { if a < b { return a; } return b; }

// Larger of two integers.
fn max(a: int, b: int) -> int { if a > b { return a; } return b; }

// Smaller of two floats.
// Smaller of two floats.
fn fmin(a: float, b: float) -> float {
    let mut out: float = b;
    if a < b {
        out = a;
    }
    out
}
// Larger of two floats.
// Larger of two floats.
fn fmax(a: float, b: float) -> float {
    let mut out: float = b;
    if a > b {
        out = a;
    }
    out
}
// Clamp n to the inclusive range [lo, hi].
fn clamp(n: int, lo: int, hi: int) -> int {
    if n < lo { return lo; }
    if n > hi { return hi; }
    return n;
}

// Clamp f to the inclusive range [lo, hi].
// Clamp f to the inclusive range [lo, hi].
fn fclamp(f: float, lo: float, hi: float) -> float {
    let mut out: float = f;
    if out < lo {
        out = lo;
    }
    if out > hi {
        out = hi;
    }
    out
}
// Sign of n: returns +1, 0, or -1 as an int.
// Note: the trit equivalent is trit_sign in std::ternary.
fn sign(n: int) -> int {
    if n > 0 { return 1; }
    if n < 0 { return -1; }
    return 0;
}

// ---------------------------------------------------------------------------
// Division, named rather than assumed  (recommendation C4)
// ---------------------------------------------------------------------------
//
// `/` and `%` mean different things in the two language versions: they
// truncate under v1 and round to nearest, ties away from zero, under v2. These
// four name the two behaviours explicitly, and all four mean the same thing in
// BOTH versions. Code written over them says which division it wants and keeps
// saying it across the version boundary; `--warn division-semantics` lists the
// `/` and `%` sites that have not yet been made to say.
//
// The two modes are PAIRS, and mixing them across a pair breaks the identity
// `(a / b) * b + (a % b) == a`, which holds for `(div_trunc, rem_trunc)` and
// for `(div_near, rem_near)` and for neither crossing. Pair them.
//
// All four are lowered in the IR lowerer (ir/lower/lower_expr.rs) to the IR
// operations the surface operators use, so they cost exactly what the operator
// costs — no call, no dispatch — and they cannot drift from it. That is the
// `trit::` route rather than the per-backend-intercept route `math` took
// elsewhere in this file; see stdlib/trit.mt for what the difference measured.

// Truncating division — the quotient rounded towards zero. v1's `/`.
fn div_trunc(a: int, b: int) -> int ;  // native

// The remainder pairing with div_trunc. Takes the sign of `a`. v1's `%`.
fn rem_trunc(a: int, b: int) -> int ;  // native

// Division rounded to the nearest integer, ties away from zero. v2's `/`.
//
// Ties go away from zero rather than to even because the balanced system is
// symmetric about zero and `div_near(-a, b) == -div_near(a, b)` is the
// property worth keeping; the unbiasedness balanced ternary claims comes from
// the representation, not from the tie-break.
fn div_near(a: int, b: int) -> int ;  // native

// The balanced remainder pairing with div_near — `a - div_near(a, b) * b`.
// Lies in [-|b|/2, +|b|/2], so unlike rem_trunc it can be negative for a
// positive `a`: `div_near(7, 2)` is 4 and `rem_near(7, 2)` is -1. v2's `%`.
fn rem_near(a: int, b: int) -> int ;  // native

// ---------------------------------------------------------------------------
// Powers and roots
// ---------------------------------------------------------------------------

// Raise base to integer exponent (exact, no floating-point rounding).
//
// Negative exponents were unspecified and are now defined: the only integer
// results are for base 1 (always 1) and base -1 (alternating); everything else
// truncates to 0. 0^0 is 1.
//
// The `if e > 0` guard on the squaring is load-bearing, not an optimisation. A
// T3 int is 27 trits and TRAPS on overflow, so squaring `b` one last time after
// the final multiply — which the textbook loop does — can trap on an input
// whose answer is perfectly representable. The guard keeps |b| <= |result| at
// every step, so pow overflows only when its own result does.
fn pow(base: int, exp: int) -> int {
    if exp < 0 {
        if base == 1 { return 1; }
        if base == -1 {
            if exp % 2 == 0 { return 1; }
            return -1;
        }
        return 0;
    }
    let mut result: int = 1;
    let mut b: int = base;
    let mut e: int = exp;
    while e > 0 {
        if e % 2 == 1 { result = result * b; }
        e = e / 2;
        if e > 0 { b = b * b; }
    }
    return result;
}

// Raise base to floating-point exponent.
// Raise base to floating-point exponent.
fn fpow(base: float, exp: float) -> float {
    let mut out: float = 1.0;
    let e: float = trunc(exp);
    if exp != 0.0 {
        if base <= 0.0 && exp != e {
            let z: float = base - base;
            out = z / z;
            if base == 0.0 {
                out = 0.0;
                if exp < 0.0 {
                    out = 1.0 / z;
                }
            }
        } else {
            if fabs(e) >= 1.0e18 {
                let z: float = base - base;
                let ab: float = fabs(base);
                out = 0.0;
                if ab > 1.0 && exp > 0.0 {
                    out = 1.0 / z;
                }
                if ab < 1.0 && exp < 0.0 {
                    out = 1.0 / z;
                }
                if ab == 1.0 {
                    out = 1.0;
                }
            } else {
                let mut n: int = e as int;
                let mut invert: int = 0;
                if n < 0 {
                    n = 0 - n;
                    invert = 1;
                }
                let n0: int = n;
                let mut acc: float = 1.0;
                let mut p: float = base;
                while n > 0 {
                    if n % 2 == 1 {
                        acc = acc * p;
                    }
                    n = n / 2;
                    if n > 0 {
                        p = p * p;
                    }
                }
                if invert == 1 {
                    acc = 1.0 / acc;
                    if acc == 0.0 && base != 0.0 {
                        let mut n2: int = n0;
                        let mut q: float = 1.0 / base;
                        acc = 1.0;
                        while n2 > 0 {
                            if n2 % 2 == 1 {
                                acc = acc * q;
                            }
                            n2 = n2 / 2;
                            if n2 > 0 {
                                q = q * q;
                            }
                        }
                    }
                }
                out = acc;
                if exp != e {
                    let fr: float = exp - e;
                    if fr == 0.5 {
                        out = acc * sqrt(base);
                    } else {
                        if fr == 0.0 - 0.5 {
                            out = acc / sqrt(base);
                        } else {
                            out = acc * exp2(fr * log2(base));
                        }
                    }
                }
            }
        }
    }
    out
}
// Raise 3 to integer exponent — trivially cheap in balanced ternary.
// pow3(n) == 3^n.
fn pow3(n: int) -> int { return pow(3, n); }

// Square root.
// Square root.
fn sqrt(f: float) -> float {
    let mut out: float = f;
    if f < 0.0 {
        let z: float = f - f;
        out = z / z;
    }
    if f > 0.0 && f <= 1.7976931348623157e308 {
        let mut m: float = f;
        let mut k: int = 0;
        while m >= 4.0 {
            m = m / 4.0;
            k = k + 1;
        }
        while m < 1.0 {
            m = m * 4.0;
            k = k - 1;
        }
        let mut x: float = 0.5 * (m + 1.0);
        let mut i: int = 0;
        while i < 6 {
            x = 0.5 * (x + m / x);
            i = i + 1;
        }
        let mut j: int = 0;
        while j < k {
            x = x * 2.0;
            j = j + 1;
        }
        while j > k {
            x = x / 2.0;
            j = j - 1;
        }
        out = x;
    }
    out
}
// Cube root (cbrt).  Exact for perfect cubes when input is a whole number.
// Cube root (cbrt).  Exact for perfect cubes when input is a whole number.
fn cbrt(f: float) -> float {
    let mut out: float = f;
    let a: float = fabs(f);
    if a > 0.0 && a <= 1.7976931348623157e308 {
        let mut m: float = a;
        let mut k: int = 0;
        while m >= 8.0 {
            m = m / 8.0;
            k = k + 1;
        }
        while m < 1.0 {
            m = m * 8.0;
            k = k - 1;
        }
        let mut x: float = 1.0 + (m - 1.0) * 0.14285714285714285;
        let mut i: int = 0;
        while i < 7 {
            x = (2.0 * x + m / (x * x)) / 3.0;
            i = i + 1;
        }
        let mut j: int = 0;
        while j < k {
            x = x * 2.0;
            j = j + 1;
        }
        while j > k {
            x = x / 2.0;
            j = j - 1;
        }
        let r: float = round(x);
        if r * r * r == a {
            x = r;
        }
        out = x;
        if f < 0.0 {
            out = 0.0 - x;
        }
    }
    out
}
// Hypotenuse: sqrt(a² + b²) without intermediate overflow.
// Hypotenuse: sqrt(a² + b²) without intermediate overflow.
fn hypot(a: float, b: float) -> float {
    let x: float = fabs(a);
    let y: float = fabs(b);
    let mut hi: float = x;
    let mut lo: float = y;
    if y > x {
        hi = y;
        lo = x;
    }
    let mut out: float = hi;
    if hi > 0.0 && hi <= 1.7976931348623157e308 {
        let r: float = lo / hi;
        out = hi * sqrt(1.0 + r * r);
    }
    out
}
// ---------------------------------------------------------------------------
// Rounding
// ---------------------------------------------------------------------------

// Floor — largest integer not greater than f.
// Floor — largest integer not greater than f.
fn floor(f: float) -> float {
    let mut out: float = trunc(f);
    if out > f {
        out = out - 1.0;
    }
    out
}
// Ceiling — smallest integer not less than f.
// Ceiling — smallest integer not less than f.
fn ceil(f: float) -> float {
    let mut out: float = trunc(f);
    if out < f {
        out = out + 1.0;
    }
    out
}
// Round to nearest integer, half-away-from-zero.
// Round to nearest integer, half-away-from-zero.
fn round(f: float) -> float {
    let mut out: float = trunc(f);
    let d: float = f - out;
    if d >= 0.5 {
        out = out + 1.0;
    }
    if d <= 0.0 - 0.5 {
        out = out - 1.0;
    }
    out
}
// Truncate toward zero.
// Truncate toward zero.
fn trunc(f: float) -> float {
    let mut out: float = f;
    if f < 4503599627370496.0 && f > 0.0 - 4503599627370496.0 {
        out = (f as int) as float;
    }
    out
}
// Fractional part of f (f - trunc(f)).
// Fractional part of f (f - trunc(f)).
fn fract(f: float) -> float {
    let mut out: float = 0.0;
    out = f - trunc(f);
    out
}
// Balanced-ternary round: round f to the nearest representable trit boundary.
// In balanced ternary the "digits" are -1, 0, +1 so this rounds f to the
// nearest integer whose balanced-ternary representation ends in a zero trit
// (i.e. the nearest multiple of the given power of 3).
//
// Example: balanced_round(3.7, 1) -> 3.0   (nearest multiple of 3¹ = 3)
//          balanced_round(7.1, 1) -> 6.0   (6 is nearer to 7.1 than 9 is)
//
// The second example read `-> 9.0   (nearest multiple of 3¹ = 9 > 6)` until
// 20 August 2026, and it was simply wrong arithmetic: 7.1 lies between the
// multiples 6 and 9, at distance 1.1 and 1.9 respectively, so the nearest is 6.
// The prose and the first example were right; only the second disagreed, and no
// reading of the spec satisfies both. Same class as to_balanced_ternary, whose
// two worked examples were also wrong until 19 August 2026 — worked examples in
// this file have a track record and should be recomputed, not trusted.
// Balanced-ternary round: round f to the nearest representable trit boundary.
// In balanced ternary the "digits" are -1, 0, +1 so this rounds f to the
// nearest integer whose balanced-ternary representation ends in a zero trit
// (i.e. the nearest multiple of the given power of 3).
//
// Example: balanced_round(3.7, 1) -> 3.0   (nearest multiple of 3¹ = 3)
//          balanced_round(7.1, 1) -> 9.0   (nearest multiple of 3¹ = 9 > 6)
fn balanced_round(f: float, trit_position: int) -> float {
    let mut s: float = 1.0;
    let mut i: int = 0;
    while i < trit_position {
        s = s * 3.0;
        i = i + 1;
    }
    while i > trit_position {
        s = s / 3.0;
        i = i - 1;
    }
    let mut out: float = 0.0;
    out = round(f / s) * s;
    out
}
// ---------------------------------------------------------------------------
// Logarithms and exponentials
// ---------------------------------------------------------------------------

// Natural logarithm (base e).
// Natural logarithm (base e).
fn log(f: float) -> float {
    let zero: float = 0.0;
    if f < 0.0  { return zero / zero; }            // NaN
    if f == 0.0 { return (0.0 - 1.0) / zero; }     // -inf
    if f > 1.7976931348623157e308 { return f; }    // +inf passes through

    // Range-reduce f = m * 2^k with m in [sqrt(2)/2, sqrt(2)).  Scaling by two
    // is exact in binary floating point, and centring the interval on sqrt(2)
    // means f near 1 gives k = 0, so k*ln2 and the series never cancel.
    // The `k` bounds are unreachable for any real argument (|k| <= 1074) and
    // exist only so the loop cannot spin forever: `>=` and `<` return TRUE for
    // a NaN operand on the T3 backend (measured), so an unbounded float-driven
    // loop is not safe there.
    let mut m: float = f;
    let mut k: int = 0;
    while m >= 1.4142135623730951 && k < 1100 { m = m * 0.5; k = k + 1; }
    while m < 0.7071067811865476 && k > -1100 { m = m * 2.0; k = k - 1; }

    // ln(m) = 2*atanh(z) with z = (m-1)/(m+1).  |z| <= 0.17157 over that
    // interval, where the series has converged by the 12th term; 13 are taken.
    let z: float = (m - 1.0) / (m + 1.0);
    let z2: float = z * z;
    let mut term: float = z;
    let mut sum: float = z;
    let mut i: int = 1;
    while i < 13 {
        term = term * z2;
        let d: int = 2 * i + 1;
        sum = sum + term / (d as float);
        i = i + 1;
    }
    return (k as float) * 0.6931471805599453 + 2.0 * sum;
}
// Binary logarithm (base 2).
// Binary logarithm (base 2).
fn log2(f: float) -> float {
    let zero: float = 0.0;
    if f < 0.0  { return zero / zero; }
    if f == 0.0 { return (0.0 - 1.0) / zero; }
    if f > 1.7976931348623157e308 { return f; }
    // Reducing in base two before calling log makes log2(2^n) exactly n for
    // every representable power of two: the reduction lands on m == 1.0.
    let mut m: float = f;
    let mut k: int = 0;
    while m >= 1.4142135623730951 && k < 1100 { m = m * 0.5; k = k + 1; }
    while m < 0.7071067811865476 && k > -1100 { m = m * 2.0; k = k - 1; }
    return (k as float) + log(m) * 1.4426950408889634;   // 1/ln2
}
// Base-10 logarithm.
// Base-10 logarithm.
fn log10(f: float) -> float {
    let zero: float = 0.0;
    if f < 0.0  { return zero / zero; }
    if f == 0.0 { return (0.0 - 1.0) / zero; }
    if f > 1.7976931348623157e308 { return f; }
    // Same idea in base ten, centred on sqrt(10) so that f near 1 keeps k = 0.
    // Centring is what makes this safe: reducing to [1,10) instead would leave
    // log10(0.99999999) computed as -1 + 0.99999999..., losing eight digits.
    let mut m: float = f;
    let mut k: int = 0;
    while m >= 3.1622776601683795 && k < 1100 { m = m / 10.0; k = k + 1; }
    while m < 0.31622776601683794 && k > -1100 { m = m * 10.0; k = k - 1; }
    return (k as float) + log(m) * 0.43429448190325176;  // 1/ln10
}
// Ternary logarithm (base 3) — native to maniT.
// log3(27.0) == 3.0 exactly.
// Ternary logarithm (base 3) - native to maniT.
// log3(27.0) == 3.0 exactly.
fn log3(f: float) -> float {
    let zero: float = 0.0;
    if f < 0.0  { return zero / zero; }
    if f == 0.0 { return (0.0 - 1.0) / zero; }
    if f > 1.7976931348623157e308 { return f; }
    // The base-3 reduction is what makes log3 exact on powers of three: for
    // f = 3^n the loop divides down to m == 1.0 exactly and log(1.0) is 0.0,
    // so the answer is the integer k with nothing added to it.  Deriving log3
    // from log instead (log(f) * 1/ln3) loses that -- measured: it misses 896
    // of the 1285 representable powers of three.
    let mut m: float = f;
    let mut k: int = 0;
    while m >= 1.7320508075688772 && k < 1100 { m = m / 3.0; k = k + 1; }
    while m < 0.5773502691896258 && k > -1100 { m = m * 3.0; k = k - 1; }
    return (k as float) + log(m) * 0.9102392266268373;   // 1/ln3
}
// Logarithm to an arbitrary base.
// Logarithm to an arbitrary base.
fn logn(f: float, base: float) -> float {
    // Dispatching the three bases that have their own reduction keeps
    // logn(f, 3.0) == log3(f) exactly, rather than one ulp away from it.
    if base == 3.0  { return log3(f); }
    if base == 2.0  { return log2(f); }
    if base == 10.0 { return log10(f); }
    return log(f) / log(base);
}
// e^f.
// e^f.
fn exp(f: float) -> float {
    let zero: float = 0.0;
    if f > 710.0  { return 1.0 / zero; }   // overflows to +inf
    if f < -746.0 { return 0.0; }          // underflows to 0
    // Both guards are outside the true overflow/underflow points
    // (709.7827 and -745.2); they exist so the int cast below cannot
    // overflow on a huge argument, and the loop reaches inf / 0 by itself.

    // f = n*ln2 + r with |r| <= ln2/2, so exp(f) = 2^n * exp(r).
    // ln2 is carried in two pieces: LN2_HI has its low mantissa bits clear so
    // n*LN2_HI is exact for every reachable n, and HI+LO reproduces ln2 to
    // about 1e-21.  A single-double ln2 leaves 2.3e-17 of absolute error in
    // the constant which, times n ~ 1000, is 2.4e-14 of RELATIVE error in the
    // result -- measured at 4.98e-14, versus 2.2e-16 for the split.
    let q: float = f * 1.4426950408889634;
    let mut n: int = 0;
    if q >= 0.0 { n = (q + 0.5) as int; } else { n = (q - 0.5) as int; }
    // Clamp before scaling.  |n| never exceeds 1077 for a real argument, so
    // this cannot clip a legitimate value; it is here because `NaN as int` is
    // undefined and the LLVM backend yields INT_MIN, which turned the scaling
    // loop below into a 9.2e18-iteration hang.  Measured, not hypothetical.
    if n > 1100  { n = 1100; }
    if n < -1100 { n = -1100; }
    let nf: float = n as float;
    let r: float = (f - nf * 6.93147180369123816490e-01)
                       - nf * 1.90821492927058770002e-10;

    // exp(r) - 1 by Horner: r/1*(1 + r/2*(1 + r/3*(1 + ...))).  Horner folds
    // the smallest terms in first and measures three times better than the
    // term-by-term Taylor sum.
    let mut t: float = 0.0;
    let mut i: int = 16;
    while i >= 1 {
        t = r * (1.0 + t) / (i as float);
        i = i - 1;
    }
    let mut res: float = 1.0 + t;

    // Scale by 2^n.  Doubling and halving are exact, and stepping one power at
    // a time keeps gradual underflow correct down to the smallest subnormal.
    let mut e: int = n;
    while e > 0 { res = res * 2.0; e = e - 1; }
    while e < 0 { res = res * 0.5; e = e + 1; }
    return res;
}
// 2^f.
// 2^f.
fn exp2(f: float) -> float {
    let zero: float = 0.0;
    if f > 1025.0  { return 1.0 / zero; }
    if f < -1080.0 { return 0.0; }
    let mut n: int = 0;
    if f >= 0.0 { n = (f + 0.5) as int; } else { n = (f - 0.5) as int; }
    // Clamp before scaling.  |n| never exceeds 1077 for a real argument, so
    // this cannot clip a legitimate value; it is here because `NaN as int` is
    // undefined and the LLVM backend yields INT_MIN, which turned the scaling
    // loop below into a 9.2e18-iteration hang.  Measured, not hypothetical.
    if n > 1100  { n = 1100; }
    if n < -1100 { n = -1100; }
    let r: float = f - (n as float);        // exact; zero for integer f
    let mut res: float = exp(r * 0.6931471805599453);
    let mut e: int = n;
    while e > 0 { res = res * 2.0; e = e - 1; }
    while e < 0 { res = res * 0.5; e = e + 1; }
    return res;
}
// 3^f — ternary exponential, exact for integer f.
// 3^f - ternary exponential, exact for integer f.
fn exp3(f: float) -> float {
    let zero: float = 0.0;
    if f > 647.0  { return 1.0 / zero; }    // 3^f overflows above 646.06
    if f < -700.0 { return 0.0; }           // 3^f underflows below -678.2
    let mut n: int = 0;
    if f >= 0.0 { n = (f + 0.5) as int; } else { n = (f - 0.5) as int; }
    // Clamp before scaling.  |n| never exceeds 1077 for a real argument, so
    // this cannot clip a legitimate value; it is here because `NaN as int` is
    // undefined and the LLVM backend yields INT_MIN, which turned the scaling
    // loop below into a 9.2e18-iteration hang.  Measured, not hypothetical.
    if n > 1100  { n = 1100; }
    if n < -1100 { n = -1100; }
    let r: float = f - (n as float);        // exact; zero for integer f
    let mut res: float = exp(r * 1.0986122886681098);
    let mut e: int = n;
    // 3^33 = 5559060566555523 is the largest power of three that is exactly
    // representable as a double, so stepping in chunks of 33 costs one
    // rounding per 33 powers instead of one per power: for 3^600 that is 20
    // roundings rather than 600, and the measured error drops 4x.
    while e >= 33  { res = res * 5559060566555523.0; e = e - 33; }
    while e <= -33 { res = res / 5559060566555523.0; e = e + 33; }
    while e > 0 { res = res * 3.0; e = e - 1; }
    while e < 0 { res = res / 3.0; e = e + 1; }
    return res;
}
// ---------------------------------------------------------------------------
// Private helpers for the trigonometric bodies below. Not public surface.
//
// Argument reduction is the whole game here: sin/cos/tan reduce |f| modulo pi/2
// and restore the sign afterwards, so these never see a negative argument.
// ---------------------------------------------------------------------------

// Quadrant count n = nearest integer to x * 2/pi, for 0 <= x < 2^21.
// sin/cos/tan reduce |f| and restore the sign afterwards, so this never sees a
// negative argument and no sign branch is needed.
fn _trig_n(x: float) -> float {
    let INVPIO2: float = 0.6366197723675814;
    let scaled: float = x * INVPIO2;
    let half: float = 0.5;
    let bumped: float = scaled + half;
    let ni: int = bumped as int;
    return ni as float;
}

// Cody-Waite reduction: r = x - n*(pi/2), landing in [-pi/4, pi/4].
//
// pi/2 is carried as PIO2_1 + PIO2_2 + PIO2_3 + PIO2_3T, which reproduces it
// to 160 bits.  PIO2_1/2/3 each have at most 32 significant bits, so nf*PIO2_k
// is an EXACT product while |nf| < 2^21, and x - nf*PIO2_1 is then exact by
// Sterbenz.  That exactness is the whole reason the reduction survives large
// arguments; a single-constant `x - n*PI2` loses one digit per decade of x.
fn _trig_r(x: float, nf: float) -> float {
    let PIO2_1: float = 1.5707963267341256;
    let w1: float = nf * PIO2_1;
    let r1: float = x - w1;
    let PIO2_2: float = 6.077100506303966e-11;
    let w2: float = nf * PIO2_2;
    let r2: float = r1 - w2;
    let PIO2_3: float = 2.0222662487111665e-21;
    let w3: float = nf * PIO2_3;
    let r3: float = r2 - w3;
    let back: float = r2 - r3;
    let resid: float = back - w3;
    let PIO2_3T: float = 8.4784276603689e-32;
    let w4: float = nf * PIO2_3T;
    let corr: float = w4 - resid;
    return r3 - corr;
}

// Exact x - n*(pi/2), where n is supplied already split into three chunks:
// n = c2*2^42 + c1*2^21 + c0, every chunk below 2^21 in magnitude.
//
// The split is what makes this work.  A plain Cody-Waite `x - nf*PIO2_1`
// needs that product to be exact, which holds only while nf carries about 21
// significant bits; beyond that the rounding of the product IS the answer.
// With 21-bit chunks each chunk*piece product is exact by construction (21
// bits times a 32-bit constant is 53), the scaling by 2^42 / 2^21 is a pure
// exponent change, and the twelve exact terms are subtracted in a
// double-double accumulator by TwoSum, which is also exact.
fn _pio2_sub(x: float, c2: float, c1: float, c0: float) -> float {
    let T42: float = 4398046511104.0;
    let T21: float = 2097152.0;
    let one: float = 1.0;
    let C: [float] = [c2, c1, c0];
    let SC: [float] = [T42, T21, one];
    let P: [float] = [1.5707963267341256, 6.077100506303966e-11,
                      2.0222662487111665e-21, 8.4784276603689e-32];
    let mut ah: float = x;
    let mut al: float = 0.0;
    for i in 0..3 {
        for j in 0..4 {
            let cc: float = C[i];
            let pp: float = P[j];
            let prod: float = cc * pp;
            let ss: float = SC[i];
            let term: float = prod * ss;
            let nt: float = 0.0 - term;
            let s: float = ah + nt;
            let bb: float = s - ah;
            let w1: float = s - bb;
            let w2: float = ah - w1;
            let w3: float = nt - bb;
            let err: float = w2 + w3;
            ah = s;
            al = al + err;
        }
    }
    return ah + al;
}

// Reduce x (>= 0, < 2^52) modulo pi/2.  Returns [r, q]: r in [-pi/4, pi/4],
// q the quadrant 0..3 as a float.
fn _rem_pio2_ext(x: float) -> [float] {
    let INVPIO2: float = 0.6366197723675814;
    let scaled: float = x * INVPIO2;
    let TWO52: float = 4503599627370496.0;
    let bumped: float = scaled + TWO52;
    let nf: float = bumped - TWO52;

    let I42: float = 2.2737367544323206e-13;
    let u2: float = nf * I42;
    let i2: int = u2 as int;
    let c2: float = i2 as float;
    let T42: float = 4398046511104.0;
    let p2: float = c2 * T42;
    let v2: float = nf - p2;

    let I21: float = 4.76837158203125e-07;
    let u1: float = v2 * I21;
    let i1: int = u1 as int;
    let c1: float = i1 as float;
    let T21: float = 2097152.0;
    let p1: float = c1 * T21;
    let c0: float = v2 - p1;

    // First pass.  INVPIO2 is only a 53-bit approximation of 2/pi, so for
    // large x the nf above can miss the true nearest integer by one or two.
    // That is not fatal — r and q stay consistent — but it would push |r|
    // past pi/4 and out of the Taylor polynomials' range, so correct it.
    let r0: float = _pio2_sub(x, c2, c1, c0);
    let t0: float = r0 * INVPIO2;
    // Round t0 to the nearest integer.  The add-then-subtract-2^52 trick is
    // only valid for a NON-NEGATIVE operand: for a negative one the sum lands
    // in [2^51, 2^52) where the ulp is 0.5, and the "integer" comes back as a
    // half.  r0 is signed, so the sign has to be split out.
    let mut k: float = 0.0;
    if t0 >= 0.0 {
        let b2: float = t0 + TWO52;
        k = b2 - TWO52;
    } else {
        let b2: float = t0 - TWO52;
        k = b2 + TWO52;
    }
    let c0b: float = c0 + k;

    let r: float = _pio2_sub(x, c2, c1, c0b);
    // n mod 4 == c0b mod 4, because 2^21 and 2^42 are multiples of 4.
    // |c0b| stays under 2^21, so this cast is inside T3's 27-trit word.
    let ic: int = c0b as int;
    let mut q: int = ic % 4;
    if q < 0 { q = q + 4; }
    let qf: float = q as float;
    return [r, qf];
}

// sin(r) for |r| <= pi/4 — Taylor through r^15; the first dropped term is
// below 7e-17 relative.
fn _sin_poly(r: float) -> float {
    let z: float = r * r;
    let mut p: float = -7.647163731819816e-13;
    let c6: float = 1.6059043836821613e-10;
    p = c6 + z * p;
    let c5: float = -2.505210838544172e-08;
    p = c5 + z * p;
    let c4: float = 2.7557319223985893e-06;
    p = c4 + z * p;
    let c3: float = -0.0001984126984126984;
    p = c3 + z * p;
    let c2: float = 0.008333333333333333;
    p = c2 + z * p;
    let c1: float = -0.16666666666666666;
    p = c1 + z * p;
    let zr: float = z * r;
    let corr: float = zr * p;
    return r + corr;
}

// cos(r) for |r| <= pi/4 — Taylor through r^18.
fn _cos_poly(r: float) -> float {
    let z: float = r * r;
    let mut p: float = -1.5619206968586225e-16;
    let d8: float = 4.779477332387385e-14;
    p = d8 + z * p;
    let d7: float = -1.1470745597729725e-11;
    p = d7 + z * p;
    let d6: float = 2.08767569878681e-09;
    p = d6 + z * p;
    let d5: float = -2.755731922398589e-07;
    p = d5 + z * p;
    let d4: float = 2.48015873015873e-05;
    p = d4 + z * p;
    let d3: float = -0.001388888888888889;
    p = d3 + z * p;
    let d2: float = 0.041666666666666664;
    p = d2 + z * p;
    let d1: float = -0.5;
    p = d1 + z * p;
    let corr: float = z * p;
    let one: float = 1.0;
    return one + corr;
}

// atan(t) for |t| <= tan(pi/8) = 0.4142 — Taylor through t^47.
// Successive terms shrink by t^2 <= 0.1716, so the tail after 24 terms is
// under 9e-21 and no term ever cancels against its neighbour.
fn _atan_poly(t: float) -> float {
    let z: float = t * t;
    let mut p: float = -0.02127659574468085;
    let a22: float = 0.022222222222222223;
    p = a22 + z * p;
    let a21: float = -0.023255813953488372;
    p = a21 + z * p;
    let a20: float = 0.024390243902439025;
    p = a20 + z * p;
    let a19: float = -0.02564102564102564;
    p = a19 + z * p;
    let a18: float = 0.02702702702702703;
    p = a18 + z * p;
    let a17: float = -0.02857142857142857;
    p = a17 + z * p;
    let a16: float = 0.030303030303030304;
    p = a16 + z * p;
    let a15: float = -0.03225806451612903;
    p = a15 + z * p;
    let a14: float = 0.034482758620689655;
    p = a14 + z * p;
    let a13: float = -0.037037037037037035;
    p = a13 + z * p;
    let a12: float = 0.04;
    p = a12 + z * p;
    let a11: float = -0.043478260869565216;
    p = a11 + z * p;
    let a10: float = 0.047619047619047616;
    p = a10 + z * p;
    let a9: float = -0.05263157894736842;
    p = a9 + z * p;
    let a8: float = 0.058823529411764705;
    p = a8 + z * p;
    let a7: float = -0.06666666666666667;
    p = a7 + z * p;
    let a6: float = 0.07692307692307693;
    p = a6 + z * p;
    let a5: float = -0.09090909090909091;
    p = a5 + z * p;
    let a4: float = 0.1111111111111111;
    p = a4 + z * p;
    let a3: float = -0.14285714285714285;
    p = a3 + z * p;
    let a2: float = 0.2;
    p = a2 + z * p;
    let a1: float = -0.3333333333333333;
    p = a1 + z * p;
    let one: float = 1.0;
    p = one + z * p;
    return t * p;
}

// sinh(x) for |x| <= 1 — Taylor through x^19.  Every term is positive, so
// there is no cancellation anywhere; this is what keeps sinh accurate near
// zero, where (e^x - e^-x)/2 throws away every digit x has.
fn _sinh_poly(x: float) -> float {
    let z: float = x * x;
    let mut p: float = 8.22063524662433e-18;
    let s8: float = 2.8114572543455206e-15;
    p = s8 + z * p;
    let s7: float = 7.647163731819816e-13;
    p = s7 + z * p;
    let s6: float = 1.6059043836821613e-10;
    p = s6 + z * p;
    let s5: float = 2.505210838544172e-08;
    p = s5 + z * p;
    let s4: float = 2.7557319223985893e-06;
    p = s4 + z * p;
    let s3: float = 0.0001984126984126984;
    p = s3 + z * p;
    let s2: float = 0.008333333333333333;
    p = s2 + z * p;
    let s1: float = 0.16666666666666666;
    p = s1 + z * p;
    let one: float = 1.0;
    p = one + z * p;
    return x * p;
}

// cosh(x) for |x| <= 1 — Taylor through x^18.
fn _cosh_poly(x: float) -> float {
    let z: float = x * x;
    let mut p: float = 1.5619206968586225e-16;
    let h8: float = 4.779477332387385e-14;
    p = h8 + z * p;
    let h7: float = 1.1470745597729725e-11;
    p = h7 + z * p;
    let h6: float = 2.08767569878681e-09;
    p = h6 + z * p;
    let h5: float = 2.755731922398589e-07;
    p = h5 + z * p;
    let h4: float = 2.48015873015873e-05;
    p = h4 + z * p;
    let h3: float = 0.001388888888888889;
    p = h3 + z * p;
    let h2: float = 0.041666666666666664;
    p = h2 + z * p;
    let h1: float = 0.5;
    p = h1 + z * p;
    let one: float = 1.0;
    return one + z * p;
}

// ---------------------------------------------------------------------------
// Trigonometry (arguments in radians unless noted)
// ---------------------------------------------------------------------------

fn sin(f: float) -> float {
    if f != f { return f; }
    let mut ax: float = f;
    let mut neg: bool = false;
    if ax < 0.0 {
        neg = true;
        ax = 0.0 - ax;
    }
    // At 2^52 a double's ulp reaches 1.0: neighbouring representable
    // arguments are then a whole radian apart and sin of one of them says
    // nothing about sin of the next.  Rather than return a confident wrong
    // number there, stop.  See the report for the accuracy ladder below it.
    let lim: float = 4503599627370496.0;
    if ax >= lim { return 0.0; }

    let mut r: float = 0.0;
    let mut q: int = 0;
    let fast: float = 2097152.0;
    if ax < fast {
        let nf: float = _trig_n(ax);
        r = _trig_r(ax, nf);
        let ni: int = nf as int;
        q = ni % 4;
    } else {
        let pr: [float] = _rem_pio2_ext(ax);
        r = pr[0];
        let qf: float = pr[1];
        q = qf as int;
    }

    let mut v: float = 0.0;
    if q == 0 { v = _sin_poly(r); }
    if q == 1 { v = _cos_poly(r); }
    if q == 2 {
        let s: float = _sin_poly(r);
        v = 0.0 - s;
    }
    if q == 3 {
        let c: float = _cos_poly(r);
        v = 0.0 - c;
    }
    if neg { return 0.0 - v; }
    return v;
}
fn cos(f: float) -> float {
    if f != f { return f; }
    let mut ax: float = f;
    if ax < 0.0 { ax = 0.0 - ax; }
    let lim: float = 4503599627370496.0;
    if ax >= lim { return 1.0; }

    let mut r: float = 0.0;
    let mut q: int = 0;
    let fast: float = 2097152.0;
    if ax < fast {
        let nf: float = _trig_n(ax);
        r = _trig_r(ax, nf);
        let ni: int = nf as int;
        q = ni % 4;
    } else {
        let pr: [float] = _rem_pio2_ext(ax);
        r = pr[0];
        let qf: float = pr[1];
        q = qf as int;
    }

    if q == 0 { return _cos_poly(r); }
    if q == 1 {
        let s: float = _sin_poly(r);
        return 0.0 - s;
    }
    if q == 2 {
        let c: float = _cos_poly(r);
        return 0.0 - c;
    }
    return _sin_poly(r);
}
fn tan(f: float) -> float {
    if f != f { return f; }
    let mut ax: float = f;
    let mut neg: bool = false;
    if ax < 0.0 {
        neg = true;
        ax = 0.0 - ax;
    }
    let lim: float = 4503599627370496.0;
    if ax >= lim { return 0.0; }

    let mut r: float = 0.0;
    let mut q: int = 0;
    let fast: float = 2097152.0;
    if ax < fast {
        let nf: float = _trig_n(ax);
        r = _trig_r(ax, nf);
        let ni: int = nf as int;
        q = ni % 2;
    } else {
        let pr: [float] = _rem_pio2_ext(ax);
        r = pr[0];
        let qf: float = pr[1];
        let q4: int = qf as int;
        q = q4 % 2;
    }

    let s: float = _sin_poly(r);
    let c: float = _cos_poly(r);
    // No pole test is needed or wanted.  pi/2 is not representable, so cos(r)
    // is never exactly zero for a double argument; at the double nearest pi/2
    // it is 6.1e-17 and tan comes out 1.633e16, which is the right answer for
    // that input.  A hand-rolled "near a pole" branch could only return a
    // wrong one.
    let mut v: float = 0.0;
    if q == 0 {
        v = s / c;
    } else {
        let t: float = c / s;
        v = 0.0 - t;
    }
    if neg { return 0.0 - v; }
    return v;
}
fn asin(f: float) -> float {
    if f != f { return f; }
    let mut x: float = f;
    let mut neg: bool = false;
    if x < 0.0 {
        neg = true;
        x = 0.0 - x;
    }
    let one: float = 1.0;
    // Out of domain clamps rather than returning NaN: T3 has no NaN to return.
    if x >= one {
        let PI2_HI: float = 1.5707963267948966;
        if neg { return 0.0 - PI2_HI; }
        return PI2_HI;
    }

    // asin(x) = atan(x / sqrt(1 - x^2)), with 1 - x^2 factored as
    // (1-x)(1+x).  That factoring is the whole trick: 1 - x*x has a relative
    // error of about x^2*eps/(1-x^2), which blows up as x approaches 1 and is
    // already 2 ulp at x = 0.9.  In the factored form 1-x is EXACT for every
    // x in [0,1] (Sterbenz) and 1+x costs half an ulp, so the product stays
    // within an ulp all the way to the domain edge and no branch is needed.
    let lo: float = one - x;
    let hi: float = one + x;
    let d: float = lo * hi;
    let q: float = sqrt(d);
    let t: float = x / q;
    let r: float = atan(t);

    if neg { return 0.0 - r; }
    return r;
}
fn acos(f: float) -> float {
    if f != f { return f; }
    let one: float = 1.0;
    let mut x: float = f;
    if x > one { x = one; }
    let none: float = 0.0 - one;
    if x < none { x = none; }

    let mut neg: bool = false;
    if x < 0.0 {
        neg = true;
        x = 0.0 - x;
    }

    let half: float = 0.5;
    if x <= half {
        let z: float = x * x;
        let d: float = one - z;
        let q: float = sqrt(d);
        let t: float = x / q;
        let a: float = atan(t);
        let PI2_HI: float = 1.5707963267948966;
        let PI2_LO: float = 6.123233995736766e-17;
        if neg {
            // acos(-u) = pi/2 + asin(u)
            let adjn: float = a + PI2_LO;
            return PI2_HI + adjn;
        }
        let adj: float = a - PI2_LO;
        return PI2_HI - adj;
    }

    // acos(x) = 2*asin(sqrt((1-x)/2)) for x in (1/2, 1].  Exact identity, no
    // constant involved, so acos(1.0) comes out exactly 0.0.
    let w: float = (one - x) * half;
    let s: float = sqrt(w);
    let u: float = one - w;
    let v: float = sqrt(u);
    let t: float = s / v;
    let a: float = atan(t);
    let two: float = 2.0;
    let two_a: float = two * a;
    if neg {
        let PI_LO: float = 1.2246467991473532e-16;
        let adj: float = two_a - PI_LO;
        let PI_HI: float = 3.141592653589793;
        return PI_HI - adj;
    }
    return two_a;
}
fn atan(f: float) -> float {
    if f != f { return f; }
    let mut x: float = f;
    let mut neg: bool = false;
    if x < 0.0 {
        neg = true;
        x = 0.0 - x;
    }

    let mut r: float = 0.0;
    let T8: float = 0.41421356237309503;
    if x <= T8 {
        r = _atan_poly(x);
    } else {
        let T38: float = 2.414213562373095;
        if x <= T38 {
            // atan(x) = pi/4 + atan((x-1)/(x+1)); for x in (tan(pi/8),
            // tan(3pi/8)] the fold lands back inside [-tan(pi/8), tan(pi/8)].
            let one: float = 1.0;
            let num: float = x - one;
            let den: float = x + one;
            let t: float = num / den;
            let p: float = _atan_poly(t);
            let PI4_LO: float = 3.061616997868383e-17;
            let adj: float = p + PI4_LO;
            let PI4_HI: float = 0.7853981633974483;
            r = PI4_HI + adj;
        } else {
            // atan(x) = pi/2 - atan(1/x), and 1/x < tan(pi/8) here.  Adding the
            // low half of pi/2 into the small term recovers the constant's own
            // rounding error instead of stamping it on the result.
            let one: float = 1.0;
            let t: float = one / x;
            let p: float = _atan_poly(t);
            let PI2_LO: float = 6.123233995736766e-17;
            let adj: float = p - PI2_LO;
            let PI2_HI: float = 1.5707963267948966;
            r = PI2_HI - adj;
        }
    }

    if neg { return 0.0 - r; }
    return r;
}
fn atan2(y: float, x: float) -> float {
    if y != y { return y; }
    if x != x { return x; }

    if x == 0.0 {
        let PI2_HI: float = 1.5707963267948966;
        if y > 0.0 { return PI2_HI; }
        if y < 0.0 { return 0.0 - PI2_HI; }
        // Both zero.  ManiT has no way to tell +0.0 from -0.0 that works on
        // both backends, so the four C results (+0, -0, +pi, -pi) collapse to
        // one; see the report.
        return 0.0;
    }
    if y == 0.0 {
        if x > 0.0 { return 0.0; }
        return 3.141592653589793;
    }

    let mut ay: float = y;
    if ay < 0.0 { ay = 0.0 - ay; }
    let mut ax: float = x;
    if ax < 0.0 { ax = 0.0 - ax; }

    // Divide the smaller magnitude by the larger so the quotient never
    // overflows and never has to be handed an infinity.
    if ax >= ay {
        let ratio: float = y / x;
        let a: float = atan(ratio);
        if x > 0.0 { return a; }
        let PI_HI: float = 3.141592653589793;
        if y > 0.0 { return a + PI_HI; }
        return a - PI_HI;
    }

    let ratio: float = x / y;
    let a: float = atan(ratio);
    let PI2_HI: float = 1.5707963267948966;
    // atan2(y,x) = sign(y)*pi/2 - atan(x/y) covers all four quadrants at once.
    if y > 0.0 { return PI2_HI - a; }
    let t: float = PI2_HI + a;
    return 0.0 - t;
}
fn sinh(f: float) -> float {
    if f != f { return f; }
    let mut x: float = f;
    let mut neg: bool = false;
    if x < 0.0 {
        neg = true;
        x = 0.0 - x;
    }

    let mut r: float = 0.0;
    let one: float = 1.0;
    if x <= one {
        r = _sinh_poly(x);
    } else {
        // e^x >= e here, so e^x - e^-x discards at most a tenth of a bit.
        let ex: float = exp(x);
        let inv: float = one / ex;
        let d: float = ex - inv;
        let half: float = 0.5;
        r = d * half;
    }

    if neg { return 0.0 - r; }
    return r;
}
fn cosh(f: float) -> float {
    if f != f { return f; }
    let mut x: float = f;
    if x < 0.0 { x = 0.0 - x; }
    let one: float = 1.0;
    if x <= one { return _cosh_poly(x); }
    let ex: float = exp(x);
    let inv: float = one / ex;
    let s: float = ex + inv;
    let half: float = 0.5;
    return s * half;
}
fn tanh(f: float) -> float {
    if f != f { return f; }
    let mut x: float = f;
    let mut neg: bool = false;
    if x < 0.0 {
        neg = true;
        x = 0.0 - x;
    }

    let mut r: float = 0.0;
    let one: float = 1.0;
    let big: float = 20.0;
    if x >= big {
        // 2/(e^40 + 1) is 8e-18, below half an ulp of 1.0, so 1.0 is the
        // correctly rounded answer and e^40 never has to be formed.
        r = one;
    } else {
        if x <= one {
            // Near zero, 1 - 2/(e^2x+1) cancels catastrophically: the quotient
            // approaches 1 and the subtraction destroys every digit x had.
            // Two all-positive Taylor series keep full relative accuracy.
            let s: float = _sinh_poly(x);
            let c: float = _cosh_poly(x);
            r = s / c;
        } else {
            // e^2x >= e^2 here, so 2/(e^2x+1) <= 0.24 and subtracting from 1
            // costs well under a bit.  This form cannot overflow.
            let two: float = 2.0;
            let t: float = two * x;
            let e: float = exp(t);
            let d: float = e + one;
            let q: float = two / d;
            r = one - q;
        }
    }

    if neg { return 0.0 - r; }
    return r;
}
// Convert degrees to radians.
// Convert degrees to radians.
fn to_radians(deg: float) -> float {
    return deg * 0.017453292519943295;      // pi/180, correctly rounded
}
// Convert radians to degrees.
// Convert radians to degrees.
fn to_degrees(rad: float) -> float {
    return rad * 57.29577951308232;         // 180/pi, correctly rounded
}
// ---------------------------------------------------------------------------
// Integer / number-theory helpers
// ---------------------------------------------------------------------------

// Greatest common divisor (always non-negative).
fn gcd(a: int, b: int) -> int {
    let mut x: int = a;
    let mut y: int = b;
    if x < 0 { x = -x; }
    if y < 0 { y = -y; }
    while y != 0 {
        let r: int = x % y;
        x = y;
        y = r;
    }
    return x;
}

// Least common multiple.
//
// Divides BEFORE multiplying. The textbook `abs(a * b) / gcd(a, b)` forms an
// intermediate larger than the answer, and on T3 that traps: lcm(2000000,
// 3000000) is 6000000, comfortably in range, while a*b is 6e12 and is not.
fn lcm(a: int, b: int) -> int {
    if a == 0 { return 0; }
    if b == 0 { return 0; }
    let g: int = gcd(a, b);
    let mut r: int = (a / g) * b;
    if r < 0 { r = -r; }
    return r;
}

// Return true if n is a perfect power of 3.
fn is_pow3(n: int) -> bool {
    if n <= 0 { return false; }
    let mut v: int = n;
    while v % 3 == 0 { v = v / 3; }
    return v == 1;
}

// Integer square root: largest k such that k*k <= n.
//
// The comparison is `mid <= n / mid`, not `mid * mid <= n`. They decide
// identically, but the squaring form builds products far larger than the result
// and TRAPS on T3 for inputs whose answers are perfectly representable.
fn isqrt(n: int) -> int {
    if n <= 0 { return 0; }
    let mut lo: int = 1;
    let mut hi: int = n;
    if hi > 3037000499 { hi = 3037000499; }
    while lo < hi {
        let mid: int = lo + (hi - lo + 1) / 2;
        if mid <= n / mid { lo = mid; } else { hi = mid - 1; }
    }
    return lo;
}

// Factorial n!  Returns -1 for n < 0 or n > 20 (which overflows int).
//
// The doc promised a panic. ManiT has no working `panic` — it is a hard
// assembler error on T3 and a deferred link failure on LLVM — so the promise
// could not be kept, and the sentinel is what is actually implemented.
//
// Reach differs by backend and cannot be made not to: 20! needs 64 bits, so on
// T3 (27 trits) this is exact only to n = 15.
fn factorial(n: int) -> int {
    if n < 0 { return -1; }
    if n > 20 { return -1; }
    let mut r: int = 1;
    let mut i: int = 2;
    while i <= n {
        r = r * i;
        i = i + 1;
    }
    return r;
}

// ---------------------------------------------------------------------------
// Ternary-specific numeric utilities
// ---------------------------------------------------------------------------

// Number of trits required to represent n in balanced ternary.
// trit_count(0) == 1, trit_count(1) == 1, trit_count(13) == 3.
fn trit_count(n: int) -> int ;  // native

// Convert a decimal integer to its balanced ternary trit array.
// The returned array is ordered least-significant trit first, with no leading
// zero trits. Sign is handled automatically — there is no separate sign trit,
// because balanced ternary carries it in the digits.
//
//   to_balanced_ternary(5)  -> [-, -, +]   -1 + -3 + 9 = 5
//   to_balanced_ternary(0)  -> [0]
//   to_balanced_ternary(-4) -> [-, -]      -1 + -3 = -4
//
// (Both worked examples here were wrong until 19 August 2026: the first was a
// draft left mid-correction in the comment, and the second gave [-, +, -],
// which is -1 + 3 - 9 = -7. Verified against both backends.)
fn to_balanced_ternary(n: int) -> [trit] ;  // native

// Reconstruct an integer from a balanced ternary trit array (LST-first).
fn from_balanced_ternary(trits: [trit]) -> int ;  // native

// Return the ternary digit sum: sum of the absolute values of each trit.
// Equivalent to the Hamming weight for balanced ternary.
fn trit_weight(n: int) -> int {
    // Arithmetic rather than a walk over to_balanced_ternary's result, because
    // `.len()` on a `[trit]` slice emits a garbage symbol on both backends, so
    // there is no way to ask that result how long it is.
    let mut v: int = n;
    if v < 0 { v = -v; }
    let mut w: int = 0;
    while v != 0 {
        let r: int = v % 3;
        if r != 0 { w = w + 1; }
        // A remainder of 2 is a balanced-ternary -1 with a carry.
        if r == 2 { v = v + 1; }
        v = v / 3;
    }
    return w;
}

// Ternary integer division that rounds toward zero (same as `/` for int).
//
// Checked deliberately rather than assumed: a symmetric digit set invites
// round-to-nearest, but the doc says truncation and tests/23_t3isa_instructions.mt
// independently pins the T3 ISA's `/` to it (-10 / 3 == -3). Round-to-nearest
// would be a different function, not this one.
fn tdiv(a: int, b: int) -> int { return a / b; }

// Ternary modulo consistent with tdiv (a == b * tdiv(a,b) + tmod(a,b)).
//
// Written as the identity itself so it holds by construction rather than by
// coincidence. (It equals `a % b`; that was verified, not assumed.)
fn tmod(a: int, b: int) -> int { return a - b * tdiv(a, b); }
