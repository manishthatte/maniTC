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
fn fabs(f: float) -> float ;  // native

// Smaller of two integers.
fn min(a: int, b: int) -> int { if a < b { return a; } return b; }

// Larger of two integers.
fn max(a: int, b: int) -> int { if a > b { return a; } return b; }

// Smaller of two floats.
fn fmin(a: float, b: float) -> float ;  // native

// Larger of two floats.
fn fmax(a: float, b: float) -> float ;  // native

// Clamp n to the inclusive range [lo, hi].
fn clamp(n: int, lo: int, hi: int) -> int {
    if n < lo { return lo; }
    if n > hi { return hi; }
    return n;
}

// Clamp f to the inclusive range [lo, hi].
fn fclamp(f: float, lo: float, hi: float) -> float ;  // native

// Sign of n: returns +1, 0, or -1 as an int.
// Note: the trit equivalent is trit_sign in std::ternary.
fn sign(n: int) -> int {
    if n > 0 { return 1; }
    if n < 0 { return -1; }
    return 0;
}

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
fn fpow(base: float, exp: float) -> float ;  // native

// Raise 3 to integer exponent — trivially cheap in balanced ternary.
// pow3(n) == 3^n.
fn pow3(n: int) -> int { return pow(3, n); }

// Square root.
fn sqrt(f: float) -> float ;  // native

// Cube root (cbrt).  Exact for perfect cubes when input is a whole number.
fn cbrt(f: float) -> float ;  // native

// Hypotenuse: sqrt(a² + b²) without intermediate overflow.
fn hypot(a: float, b: float) -> float ;  // native

// ---------------------------------------------------------------------------
// Rounding
// ---------------------------------------------------------------------------

// Floor — largest integer not greater than f.
fn floor(f: float) -> float ;  // native

// Ceiling — smallest integer not less than f.
fn ceil(f: float) -> float ;  // native

// Round to nearest integer, half-away-from-zero.
fn round(f: float) -> float ;  // native

// Truncate toward zero.
fn trunc(f: float) -> float ;  // native

// Fractional part of f (f - trunc(f)).
fn fract(f: float) -> float ;  // native

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
fn balanced_round(f: float, trit_position: int) -> float ;  // native

// ---------------------------------------------------------------------------
// Logarithms and exponentials
// ---------------------------------------------------------------------------

// Natural logarithm (base e).
fn log(f: float) -> float ;  // native

// Binary logarithm (base 2).
fn log2(f: float) -> float ;  // native

// Base-10 logarithm.
fn log10(f: float) -> float ;  // native

// Ternary logarithm (base 3) — native to maniT.
// log3(27.0) == 3.0 exactly.
fn log3(f: float) -> float ;  // native

// Logarithm to an arbitrary base.
fn logn(f: float, base: float) -> float ;  // native

// e^f.
fn exp(f: float) -> float ;  // native

// 2^f.
fn exp2(f: float) -> float ;  // native

// 3^f — ternary exponential, exact for integer f.
fn exp3(f: float) -> float ;  // native

// ---------------------------------------------------------------------------
// Trigonometry (arguments in radians unless noted)
// ---------------------------------------------------------------------------

fn sin(f: float) -> float ;  // native
fn cos(f: float) -> float ;  // native
fn tan(f: float) -> float ;  // native
fn asin(f: float) -> float ;  // native
fn acos(f: float) -> float ;  // native
fn atan(f: float) -> float ;  // native
fn atan2(y: float, x: float) -> float ;  // native
fn sinh(f: float) -> float ;  // native
fn cosh(f: float) -> float ;  // native
fn tanh(f: float) -> float ;  // native

// Convert degrees to radians.
fn to_radians(deg: float) -> float ;  // native

// Convert radians to degrees.
fn to_degrees(rad: float) -> float ;  // native

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
