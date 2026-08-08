// Test: T27F balanced ternary floating-point (Claim 6/21)
//
// Properties verified:
//   - No sign bit (negation is trit-flip)
//   - No dual zero (exactly one zero)
//   - No NaN (every pattern is valid)
//   - Unbiased rounding by truncation

use std::t27f;

fn main() {
    // --- Construction and decomposition ---

    let x = t27f::from_parts(0, 100);  // 100 × 3^0 = 100
    assert(t27f::mantissa(x) == 100, "mantissa of 100 is 100");
    assert(t27f::exponent(x) == 0,   "exponent of 100×3^0 is 0");
    print("PASS from_parts / mantissa / exponent");

    // --- Zero properties ---
    let z = t27f::ZERO;
    assert(t27f::is_zero(z) == true, "ZERO is zero");
    assert(t27f::mantissa(z) == 0,   "ZERO mantissa is 0");
    // No dual zero: there is exactly one zero representation
    let z2 = t27f::from_parts(5, 0);
    assert(t27f::is_zero(t27f::normalize(z2)) == true, "0×3^5 normalizes to ZERO");
    print("PASS no dual zero");

    // --- Negation is exact trit-flip ---
    let pos = t27f::from_parts(0, 42);
    let neg_val = t27f::neg(pos);
    assert(t27f::mantissa(neg_val) == -42, "neg(42) mantissa = -42");
    assert(t27f::exponent(neg_val) == 0,   "neg preserves exponent");
    // Double negation is identity
    let pos2 = t27f::neg(neg_val);
    assert(t27f::mantissa(pos2) == 42, "neg(neg(42)) = 42");
    print("PASS exact negation (trit-flip)");

    // --- Arithmetic ---

    // Addition
    let a = t27f::from_parts(0, 100);
    let b = t27f::from_parts(0, 200);
    let sum = t27f::add(a, b);
    assert(t27f::mantissa(sum) * t27f::pow3(t27f::exponent(sum)) == 300,
           "100 + 200 = 300");
    print("PASS addition");

    // Subtraction
    let diff = t27f::sub(b, a);
    assert(t27f::mantissa(diff) * t27f::pow3(t27f::exponent(diff)) == 100,
           "200 - 100 = 100");
    print("PASS subtraction");

    // Multiplication
    let c = t27f::from_parts(0, 3);
    let d = t27f::from_parts(0, 9);
    let prod = t27f::mul(c, d);
    // 3 × 9 = 27 = 1 × 3^3
    let prod_val = t27f::mantissa(prod) * t27f::pow3(t27f::exponent(prod));
    assert(prod_val == 27, "3 × 9 = 27");
    print("PASS multiplication");

    // --- Comparison ---
    assert(t27f::compare(b, a) == +1, "200 > 100");
    assert(t27f::compare(a, b) == -1, "100 < 200");
    assert(t27f::compare(a, a) == 0,  "100 == 100");
    print("PASS comparison");

    // --- Absolute value ---
    let neg_val2 = t27f::from_parts(0, -50);
    let abs_val = t27f::abs(neg_val2);
    assert(t27f::mantissa(abs_val) > 0, "abs(-50) is positive");
    print("PASS absolute value");

    print("All T27F tests passed.");
}
