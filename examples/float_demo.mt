// float_demo.mt — T27F Balanced Ternary Floating-Point Demonstration
//
// Demonstrates the T27F format: 27-trit float with no sign bit,
// no dual zero, no NaN. Negation is exact trit-flip.
//
// Format: [9-trit exponent | 18-trit mantissa]
// Value = mantissa × 3^exponent
//
// Authored by: Manish Jagdish Thatte

use std::t27f;

fn main() {
    print("=== T27F Balanced Ternary Float Demo ===");
    print("");

    // Construction
    let hundred = t27f::from_parts(0, 100);
    print("100 = mantissa ", t27f::mantissa(hundred), " × 3^", t27f::exponent(hundred));

    let twenty_seven = t27f::from_parts(3, 1);  // 1 × 3^3 = 27
    print("27  = mantissa ", t27f::mantissa(twenty_seven), " × 3^", t27f::exponent(twenty_seven));
    print("");

    // Property 1: No sign bit — negation is exact trit-flip
    print("--- Property: Exact negation (trit-flip) ---");
    let pos = t27f::from_parts(0, 42);
    let neg_val = t27f::neg(pos);
    print("  +42 mantissa: ", t27f::mantissa(pos));
    print("  -42 mantissa: ", t27f::mantissa(neg_val));
    print("  neg(neg(42)): ", t27f::mantissa(t27f::neg(neg_val)));
    print("");

    // Property 2: No dual zero — exactly one zero
    print("--- Property: Single zero ---");
    let z1 = t27f::ZERO;
    let z2 = t27f::normalize(t27f::from_parts(5, 0));  // 0 × 3^5
    print("  ZERO is zero: ", t27f::is_zero(z1));
    print("  0×3^5 normalized is zero: ", t27f::is_zero(z2));
    print("");

    // Property 3: No NaN — every pattern is a valid number
    print("--- Property: No NaN ---");
    print("  Every 27-trit pattern is a valid T27F number.");
    print("  No special bit patterns, no NaN, no infinity.");
    print("");

    // Arithmetic
    print("--- Arithmetic ---");
    let a = t27f::from_parts(0, 100);
    let b = t27f::from_parts(0, 200);
    let sum = t27f::add(a, b);
    print("  100 + 200 = ", t27f::mantissa(sum), " × 3^", t27f::exponent(sum));

    let diff = t27f::sub(b, a);
    print("  200 - 100 = ", t27f::mantissa(diff), " × 3^", t27f::exponent(diff));

    let prod = t27f::mul(t27f::from_parts(0, 3), t27f::from_parts(0, 9));
    print("  3 × 9     = ", t27f::mantissa(prod), " × 3^", t27f::exponent(prod));
    print("");

    // Comparison
    print("--- Comparison ---");
    print("  200 > 100: ", t27f::compare(b, a));   // +1
    print("  100 < 200: ", t27f::compare(a, b));   // -1
    print("  100 = 100: ", t27f::compare(a, a));   //  0
    print("");

    print("=== T27F demo complete ===");
}
