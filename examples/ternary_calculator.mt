// examples/ternary_calculator.mt
// Interactive Balanced Ternary Calculator
//
// Balanced ternary arithmetic has properties that binary lacks:
//   - Negation is trit-flip (tnot).  No two's complement.  No bias.
//   - Symmetric range: t9 = -9841 to +9841 (vs unsigned 16-bit 0..65535)
//   - Rounding is trivial: truncate the least significant trits
//   - Carry propagation is simpler (carry is always -1, 0, or +1)
//   - Addition and subtraction use the SAME circuit (no separate subtractor)
//
// This example implements a balanced ternary calculator that:
//   - Converts between decimal and balanced ternary
//   - Performs add, subtract, multiply, divide
//   - Shows carry propagation in balanced ternary addition
//   - Demonstrates the elegance of trit-flip negation
//   - Displays results in both decimal and ternary notation
//
// Authored by: Manish Jagdish Thatte

use std::io;
use std::fmt;
use std::math;
use std::ternary;
use std::collections;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------
fn int_pow(base: int, exp: int) -> int {
    let mut result = 1;
    let mut i = 0;
    while i < exp {
        result = result * base;
        i = i + 1;
    }
    return result;
}

fn heading(title: str) {
    io::println("");
    io::println("================================================");
    io::print("  ");
    io::println(title);
    io::println("================================================");
}

// Print a number in both decimal and balanced ternary (inline).
fn print_dual_inline(n: int) {
    let trits = math::to_balanced_ternary(n);
    io::print_int(n);
    io::print(" (");
    io::print(ternary::trits_to_str(trits));
    io::print(")");
}

// Print a number in both notations with a label, followed by newline.
fn print_dual(label: str, n: int) {
    io::print(label);
    print_dual_inline(n);
    io::println("");
}

// Show a trit with sign character.
fn trit_char(t: trit) -> str {
    tif t {
        + => return "+",
        0 => return "0",
        - => return "-",
    }
}

// ---------------------------------------------------------------------------
// 1. Decimal to Balanced Ternary Conversion
// ---------------------------------------------------------------------------
// Algorithm:
//   1. Divide n by 3 to get quotient and remainder
//   2. If remainder is 2, record trit as - and add 1 to quotient (carry)
//   3. If remainder is -2, record trit as + and subtract 1 from quotient
//   4. Otherwise, record remainder as trit
//   5. Repeat until quotient is 0

fn demo_conversion() {
    heading("1. Decimal to Balanced Ternary Conversion");

    io::println("  Balanced ternary uses digits {-, 0, +} (values -1, 0, +1).");
    io::println("  Conversion from decimal: divide by 3, adjust remainders.");
    io::println("");

    // Step-by-step conversion of 42.
    let n = 42;
    io::print("  Converting ");
    io::print_int(n);
    io::println(" to balanced ternary:");
    io::println("");
    io::println("    Step  | n     | n%3  | trit | n/3 (adjusted)");
    io::println("    ------|-------|------|------|---------------");

    let mut rem_val = n;
    let mut step = 1;
    let digits: Vec<str> = Vec::new();

    while rem_val != 0 {
        let r = rem_val % 3;
        let mut trit_s = "";
        let mut new_rem = rem_val / 3;

        if r == 2 {
            trit_s = "-";
            new_rem = new_rem + 1;
        } elif r == -2 {
            trit_s = "+";
            new_rem = new_rem - 1;
        } elif r == 1 {
            trit_s = "+";
        } elif r == -1 {
            trit_s = "-";
        } else {
            trit_s = "0";
        }

        digits.push(trit_s);

        io::print("      ");
        io::print_int(step);
        io::print("   | ");
        io::print(fmt::align_right(fmt::show_int(rem_val), 5, ' '));
        io::print(" |  ");
        io::print(fmt::align_right(fmt::show_int(r), 2, ' '));
        io::print("  |  ");
        io::print(trit_s);
        io::print("   | ");
        io::println_int(new_rem);

        rem_val = new_rem;
        step = step + 1;
    }

    io::print("  Result (MST-first): ");
    let mut i = digits.len() - 1;
    while i >= 0 {
        io::print(digits.get(i));
        i = i - 1;
    }
    io::println("");

    io::println("");
    io::println("  Verification:");
    let trits = math::to_balanced_ternary(n);
    io::print("    math::to_balanced_ternary(42) = ");
    io::println(ternary::trits_to_str(trits));
    io::print("    math::from_balanced_ternary(result) = ");
    io::println_int(math::from_balanced_ternary(trits));
}

// ---------------------------------------------------------------------------
// 2. Balanced Ternary Arithmetic
// ---------------------------------------------------------------------------
fn demo_arithmetic() {
    heading("2. Balanced Ternary Arithmetic");

    io::println("  All four operations, shown in both decimal and ternary.");
    io::println("");

    // Addition.
    io::println("  --- Addition ---");
    let add_a: [int] = [42, -13, 100, 27];
    let add_b: [int] = [17, 8, -100, 27];
    let mut ai = 0;
    while ai < 4 {
        let a = add_a[ai];
        let b = add_b[ai];
        let result = a + b;
        io::print("    ");
        print_dual_inline(a);
        io::print(" + ");
        print_dual_inline(b);
        io::print(" = ");
        print_dual_inline(result);
        io::println("");
        ai = ai + 1;
    }

    io::println("");

    // Subtraction.
    // Key insight: in balanced ternary, subtraction is addition with negation.
    // Negation is trit-flip (tnot).  So subtraction = addition with tnot.
    // No separate subtraction circuit needed!
    io::println("  --- Subtraction (add with tnot — same circuit!) ---");
    let sub_a: [int] = [42, 100, 13, 0];
    let sub_b: [int] = [17, 99, 40, 1];
    let mut si = 0;
    while si < 4 {
        let a = sub_a[si];
        let b = sub_b[si];
        let result = a - b;
        io::print("    ");
        print_dual_inline(a);
        io::print(" - ");
        print_dual_inline(b);
        io::print(" = ");
        print_dual_inline(result);
        io::println("");
        si = si + 1;
    }

    io::println("");

    // Multiplication.
    io::println("  --- Multiplication ---");
    let mul_a: [int] = [7, 13, -9, 3];
    let mul_b: [int] = [6, -3, -9, 27];
    let mut mi = 0;
    while mi < 4 {
        let a = mul_a[mi];
        let b = mul_b[mi];
        let result = a * b;
        io::print("    ");
        print_dual_inline(a);
        io::print(" * ");
        print_dual_inline(b);
        io::print(" = ");
        print_dual_inline(result);
        io::println("");
        mi = mi + 1;
    }

    io::println("");

    // Division (integer).
    io::println("  --- Division (integer, truncates toward zero) ---");
    let div_a: [int] = [42, 100, -27, 81];
    let div_b: [int] = [7, 3, 9, -3];
    let mut di = 0;
    while di < 4 {
        let a = div_a[di];
        let b = div_b[di];
        let quotient = a / b;
        let remainder = a % b;
        io::print("    ");
        print_dual_inline(a);
        io::print(" / ");
        print_dual_inline(b);
        io::print(" = ");
        print_dual_inline(quotient);
        io::print("  remainder ");
        print_dual_inline(remainder);
        io::println("");
        di = di + 1;
    }
}

// ---------------------------------------------------------------------------
// 3. Carry Propagation in Balanced Ternary Addition
// ---------------------------------------------------------------------------
fn demo_carry_propagation() {
    heading("3. Carry Propagation — Trit-by-Trit Addition");

    io::println("  Balanced ternary addition truth table (single trit):");
    io::println("  a + b + carry_in -> (sum, carry_out)");
    io::println("");
    io::println("  When a + b + c_in gives:");
    io::println("    -3 => sum = 0, carry = -    (-3 = 0 + (-1)*3)");
    io::println("    -2 => sum = +, carry = -    (-2 = 1 + (-1)*3)");
    io::println("    -1 => sum = -, carry = 0    (-1 = -1 + 0*3)");
    io::println("     0 => sum = 0, carry = 0");
    io::println("    +1 => sum = +, carry = 0    (+1 = 1 + 0*3)");
    io::println("    +2 => sum = -, carry = +    (+2 = -1 + 1*3)");
    io::println("    +3 => sum = 0, carry = +    (+3 = 0 + 1*3)");
    io::println("");

    // Show a worked addition: 42 + 17 = 59
    let a = 42;
    let b = 17;
    let result = a + b;

    io::println("  Worked example: 42 + 17 = 59");
    io::println("");

    let ta = math::to_balanced_ternary(a);
    let tb = math::to_balanced_ternary(b);
    let tr = math::to_balanced_ternary(result);

    io::print("    ");
    io::print(fmt::align_right(fmt::show_int(a), 4, ' '));
    io::print("  =  ");
    io::println(ternary::trits_to_str(ta));

    io::print("  + ");
    io::print(fmt::align_right(fmt::show_int(b), 4, ' '));
    io::print("  =  ");
    io::println(ternary::trits_to_str(tb));

    io::println("          --------");

    io::print("    ");
    io::print(fmt::align_right(fmt::show_int(result), 4, ' '));
    io::print("  =  ");
    io::println(ternary::trits_to_str(tr));
    io::println("");
    io::println("  Note: carry is always {-, 0, +} — never larger than a single trit.");
    io::println("  This makes the carry chain simpler than binary (where carry is {0, 1}).");
}

// ---------------------------------------------------------------------------
// 4. Negation — Trit-Flip (No Two's Complement!)
// ---------------------------------------------------------------------------
fn demo_negation() {
    heading("4. Negation — Trit-Flip Elegance");

    io::println("  Binary negation (two's complement):");
    io::println("    1. Flip all bits");
    io::println("    2. Add 1");
    io::println("    3. Watch for overflow (MIN_INT has no positive counterpart!)");
    io::println("    Binary 16-bit range: -32768 to +32767  (ASYMMETRIC)");
    io::println("");
    io::println("  Balanced ternary negation (trit-flip):");
    io::println("    1. Flip every trit: + <-> -, 0 stays 0");
    io::println("    That's it. No add-one step. No overflow. Always works.");
    io::println("    3-trit (tryte) range: -13 to +13  (SYMMETRIC)");
    io::println("");

    let values: [int] = [1, -1, 13, -13, 42, -42, 100, -100, 0];
    io::println("    Value   | Balanced Ternary | Negated (trit-flip) | Negated Value");
    io::println("    --------|------------------|---------------------|-------------");

    for v in values {
        let trits = math::to_balanced_ternary(v);
        let neg_v = 0 - v;
        let neg_trits = math::to_balanced_ternary(neg_v);

        io::print("    ");
        io::print(fmt::align_right(fmt::show_int(v), 7, ' '));
        io::print(" | ");
        io::print(fmt::align_right(ternary::trits_to_str(trits), 16, ' '));
        io::print(" | ");
        io::print(fmt::align_right(ternary::trits_to_str(neg_trits), 19, ' '));
        io::print(" | ");
        io::print(fmt::align_right(fmt::show_int(neg_v), 7, ' '));
        io::println("");
    }

    io::println("");
    io::println("  Every positive number has an exact negative counterpart.");
    io::println("  Zero negates to zero. The range is perfectly symmetric.");
    io::println("  In hardware: negation = tnot gate on each trit. One cycle.");
}

// ---------------------------------------------------------------------------
// 5. Symmetric Ranges
// ---------------------------------------------------------------------------
fn demo_ranges() {
    heading("5. Symmetric Ranges — No Bias, No Gotchas");

    io::println("  Balanced ternary word sizes and their ranges:");
    io::println("");
    io::println("    Type   | Trits | Values  | Range");
    io::println("    -------|-------|---------|----------------------------");
    io::println("    trit   |   1   |       3 | -1 to +1");
    io::println("    tryte  |   3   |      27 | -13 to +13");
    io::println("    t9     |   9   |  19,683 | -9,841 to +9,841");
    io::println("    t27    |  27   |  7.6e12 | -3,812,798,742,493 to +3,812,798,742,493");
    io::println("");
    io::println("  Contrast with binary:");
    io::println("    int8   |  8b   |     256 | -128 to +127        (OFF BY ONE!)");
    io::println("    int16  | 16b   |  65,536 | -32,768 to +32,767  (OFF BY ONE!)");
    io::println("    int32  | 32b   |  4.3e9  | -2,147,483,648 to +2,147,483,647");
    io::println("");
    io::println("  Binary's asymmetry causes real bugs:");
    io::println("    -(-128) overflows in int8 (there is no +128)");
    io::println("    abs(INT_MIN) is undefined behavior in C");
    io::println("");
    io::println("  Balanced ternary: -(-13) = +13.  Always safe.  Always symmetric.");

    io::println("");
    io::println("  Powers of 3 (the ternary word boundaries):");
    let mut n = 0;
    while n <= 9 {
        let pow_val = int_pow(3, n);
        let range_max = (pow_val - 1) / 2;
        io::print("    3^");
        io::print_int(n);
        io::print(" = ");
        io::print(fmt::align_right(fmt::show_int(pow_val), 8, ' '));
        io::print("  =>  ");
        io::print_int(n);
        io::print("-trit range: -");
        io::print_int(range_max);
        io::print(" to +");
        io::println_int(range_max);
        n = n + 1;
    }
}

// ---------------------------------------------------------------------------
// 6. Multiplication by 3 — Just Shift Left
// ---------------------------------------------------------------------------
fn demo_multiply_by_3() {
    heading("6. Multiply/Divide by 3 — Trivial Shift");

    io::println("  In binary, multiply by 2 = shift left by 1 bit.  Trivial.");
    io::println("  In balanced ternary, multiply by 3 = shift left by 1 trit.  Trivial.");
    io::println("");
    io::println("  This is a FREE operation — no multiplier circuit needed.");
    io::println("");

    let values: [int] = [1, 4, 13, -7, 42];
    for v in values {
        let tripled = v * 3;
        let divided = v / 3;
        let v_trits = math::to_balanced_ternary(v);
        let tripled_trits = math::to_balanced_ternary(tripled);

        io::print("    ");
        io::print(fmt::align_right(fmt::show_int(v), 4, ' '));
        io::print("  (");
        io::print(ternary::trits_to_str(v_trits));
        io::print(")  * 3 = ");
        io::print(fmt::align_right(fmt::show_int(tripled), 5, ' '));
        io::print("  (");
        io::print(ternary::trits_to_str(tripled_trits));
        io::println(")  [shift left 1 trit]");
    }

    io::println("");
    io::println("  Similarly, divide by 3 = shift right by 1 trit.");
    io::println("  Powers of 3 are what powers of 2 are to binary — the native scale.");
}

// ---------------------------------------------------------------------------
// 7. Running Calculations
// ---------------------------------------------------------------------------
fn demo_calculations() {
    heading("7. Sample Calculations");

    io::println("  A series of calculations displayed in dual notation:");
    io::println("");

    // Expression 1: (13 + 27) * 3
    let e1 = (13 + 27) * 3;
    io::println("  (13 + 27) * 3:");
    print_dual("    13 + 27 = ", 13 + 27);
    print_dual("    40 * 3  = ", e1);
    io::println("");

    // Expression 2: 81 - 27 - 9 - 3 - 1 (all powers of 3)
    let e2 = 81 - 27 - 9 - 3 - 1;
    io::println("  81 - 27 - 9 - 3 - 1  (subtracting powers of 3):");
    print_dual("    result = ", e2);
    io::println("");

    // Expression 3: show that 3^4 - 3^3 - 3^2 - 3^1 - 3^0 = 41
    let e3 = int_pow(3, 4) - int_pow(3, 3) - int_pow(3, 2) - int_pow(3, 1) - int_pow(3, 0);
    io::println("  3^4 - 3^3 - 3^2 - 3^1 - 3^0:");
    print_dual("    result = ", e3);
    io::print("    This is +---- in balanced ternary (MST=+, rest=-)");
    io::println("");
    io::println("");

    // Expression 4: perfect ternary representations
    io::println("  Numbers that are powers of 3 (single non-zero trit):");
    let mut p = 0;
    while p <= 6 {
        let val = int_pow(3, p);
        io::print("    3^");
        io::print_int(p);
        io::print(" = ");
        print_dual_inline(val);
        io::println("");
        p = p + 1;
    }

    io::println("");

    // Expression 5: verify negation symmetry with arithmetic
    io::println("  Negation symmetry: a + (-a) = 0 for every value:");
    let test_vals: [int] = [1, 13, 42, 100, 9841];
    for v in test_vals {
        let neg = 0 - v;
        let sum = v + neg;
        io::print("    ");
        io::print_int(v);
        io::print(" + (");
        io::print_int(neg);
        io::print(") = ");
        io::print_int(sum);
        if sum == 0 {
            io::println("  [verified]");
        } else {
            io::println("  [ERROR]");
        }
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------
fn main() {
    io::println("maniT Balanced Ternary Calculator");
    io::println("=================================");
    io::println("Authored by: Manish Jagdish Thatte");

    demo_conversion();
    demo_arithmetic();
    demo_carry_propagation();
    demo_negation();
    demo_ranges();
    demo_multiply_by_3();
    demo_calculations();

    io::println("");
    io::println("Demo complete.");
}
