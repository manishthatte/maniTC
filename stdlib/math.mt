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
let LOG2_3: float = 1.5849625007211563;

// log₃(2) — inverse of LOG2_3.
let LOG3_2: float = 0.6309297535714573;

// The largest finite int value representable on this platform.
let INT_MAX: int = 9223372036854775807;

// The smallest (most negative) finite int value.
let INT_MIN: int = -9223372036854775808;

// Maximum value of a t27 (27-trit balanced ternary integer): (3^27-1)/2
let T27_MAX: t27 = 0t+++++++++++++++++++++++++++; // +1 in each of 27 positions

// Minimum value of a t27.
let T27_MIN: t27 = 0t---------------------------; // -1 in each of 27 positions

// ---------------------------------------------------------------------------
// Basic numeric utilities
// ---------------------------------------------------------------------------

// Absolute value of an integer.
fn abs(n: int) -> int ;  // native

// Absolute value of a float.
fn fabs(f: float) -> float ;  // native

// Smaller of two integers.
fn min(a: int, b: int) -> int ;  // native

// Larger of two integers.
fn max(a: int, b: int) -> int ;  // native

// Smaller of two floats.
fn fmin(a: float, b: float) -> float ;  // native

// Larger of two floats.
fn fmax(a: float, b: float) -> float ;  // native

// Clamp n to the inclusive range [lo, hi].
fn clamp(n: int, lo: int, hi: int) -> int ;  // native

// Clamp f to the inclusive range [lo, hi].
fn fclamp(f: float, lo: float, hi: float) -> float ;  // native

// Sign of n: returns +1, 0, or -1 as an int.
// Note: the trit equivalent is trit_sign in std::ternary.
fn sign(n: int) -> int ;  // native

// ---------------------------------------------------------------------------
// Powers and roots
// ---------------------------------------------------------------------------

// Raise base to integer exponent (exact, no floating-point rounding).
fn pow(base: int, exp: int) -> int ;  // native

// Raise base to floating-point exponent.
fn fpow(base: float, exp: float) -> float ;  // native

// Raise 3 to integer exponent — trivially cheap in balanced ternary.
// pow3(n) == 3^n.
fn pow3(n: int) -> int ;  // native

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
//          balanced_round(7.1, 1) -> 9.0   (nearest multiple of 3¹ = 9 > 6)
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
fn gcd(a: int, b: int) -> int ;  // native

// Least common multiple.
fn lcm(a: int, b: int) -> int ;  // native

// Return true if n is a perfect power of 3.
fn is_pow3(n: int) -> bool ;  // native

// Integer square root: largest k such that k*k <= n.
fn isqrt(n: int) -> int ;  // native

// Factorial n!  Panics for n < 0 or n > 20 (overflows int).
fn factorial(n: int) -> int ;  // native

// ---------------------------------------------------------------------------
// Ternary-specific numeric utilities
// ---------------------------------------------------------------------------

// Number of trits required to represent n in balanced ternary.
// trit_count(0) == 1, trit_count(1) == 1, trit_count(13) == 3.
fn trit_count(n: int) -> int ;  // native

// Convert a decimal integer to its balanced ternary trit array.
// The returned array is ordered least-significant trit first.
// to_balanced_ternary(5)  -> [-, +]  because 5 = 1*3 + (-1)*1 is wrong;
//   actually 5 = +1*9 + (-1)*3 + (-1)*1 = [-, -, +] in LST-first order.
// to_balanced_ternary(0)  -> [0]
// to_balanced_ternary(-4) -> [-, +, -]  (sign is handled automatically)
fn to_balanced_ternary(n: int) -> [trit] ;  // native

// Reconstruct an integer from a balanced ternary trit array (LST-first).
fn from_balanced_ternary(trits: [trit]) -> int ;  // native

// Return the ternary digit sum: sum of the absolute values of each trit.
// Equivalent to the Hamming weight for balanced ternary.
fn trit_weight(n: int) -> int ;  // native

// Ternary integer division that rounds toward zero (same as `/` for int).
fn tdiv(a: int, b: int) -> int ;  // native

// Ternary modulo consistent with tdiv (a == b * tdiv(a,b) + tmod(a,b)).
fn tmod(a: int, b: int) -> int ;  // native
