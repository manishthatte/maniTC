// examples/ternary_demo.mt
// Comprehensive demonstration of maniT's balanced ternary type system.
//
// Covers:
//   - trit / tryte / t9 / t27 types and literals
//   - Balanced ternary arithmetic and display
//   - tif three-way branching on trit expressions
//   - Ternary operators: tand, tor, tnot, txor
//   - std::ternary low-level operations
//   - std::math ternary conversion helpers
//   - Comparison of ternary vs binary storage efficiency

use std::io;
use std::fmt;
use std::math;
use std::ternary;

// ---------------------------------------------------------------------------
// Helper: print a section heading
// ---------------------------------------------------------------------------
fn heading(title: str) {
    io::println("");
    io::println("=================================================");
    io::print("  ");
    io::println(title);
    io::println("=================================================");
}

// ---------------------------------------------------------------------------
// 1. Trit type and literals
// ---------------------------------------------------------------------------
fn demo_trit_basics() {
    heading("1. Trit Basics");

    let pos: trit = +;
    let zer: trit = 0;
    let neg: trit = -;

    io::println("Trit literals:");
    io::print("  +  -> int value: "); io::println_int(ternary::trit_to_int(pos));
    io::print("  0  -> int value: "); io::println_int(ternary::trit_to_int(zer));
    io::print("  -  -> int value: "); io::println_int(ternary::trit_to_int(neg));

    io::println("");
    io::println("tif branching on trit values:");
    let trits: [trit] = [+, 0, -];
    for t in trits {
        io::print("  tif ");
        io::print_trit(t);
        io::print(" => ");
        tif t {
            + => io::println("positive branch"),
            0 => io::println("zero branch"),
            - => io::println("negative branch"),
        }
    }
}

// ---------------------------------------------------------------------------
// 2. Tryte (3-trit), t9 (9-trit), t27 (27-trit)
// ---------------------------------------------------------------------------
fn demo_word_types() {
    heading("2. Ternary Word Types");

    // tryte: range -13 to +13  (3^3 = 27 values)
    // The literal syntax uses 0t followed by + / 0 / - digits, MST first.
    let a: tryte = 0t++-;   // (+1)*9 + (+1)*3 + (-1)*1 = 9 + 3 - 1 = 11
    let b: tryte = 0t--+;   // (-1)*9 + (-1)*3 + (+1)*1 = -9 - 3 + 1 = -11
    let c: tryte = 0t+0-;   // (+1)*9 + (0)*3 + (-1)*1  = 9 + 0 - 1 = 8

    io::println("tryte examples (3 trits, range -13..+13):");
    io::print("  0t++- = "); io::print_tryte(a); io::print(" = "); io::println_int(11);
    io::print("  0t--+ = "); io::print_tryte(b); io::print(" = "); io::println_int(-11);
    io::print("  0t+0- = "); io::print_tryte(c); io::print(" = "); io::println_int(8);

    // t9: range -9841 to +9841  (3^9 = 19683 values)
    let d: t9 = 0t+00000000;  // = 9^4... wait, t9 has 9 trits so 3^9 / 2 max
    // value: +1 * 3^8 = 6561
    io::println("");
    io::println("t9 examples (9 trits, range -9841..+9841):");
    io::print("  0t+00000000 = "); io::println_int(6561);
    io::print("  0t+-------- = "); io::println_int(6561 - 3280);  // 3281

    // t27: range -(3^27-1)/2 to +(3^27-1)/2
    let e: t27 = 0t+0-+0-+0-;   // mixed pattern
    io::println("");
    io::println("t27 examples (27 trits):");
    io::print("  0t+0-+0-+0- ... = "); io::println_ternary(e);
    io::print("  as integer     = "); io::println_int(ternary::t27_to_int(e));
}

// ---------------------------------------------------------------------------
// 3. Balanced ternary arithmetic
// ---------------------------------------------------------------------------
fn demo_arithmetic() {
    heading("3. Balanced Ternary Arithmetic");

    let x: int = 42;
    let y: int = -17;

    io::print("x = "); io::println_int(x);
    io::print("y = "); io::println_int(y);
    io::println("");

    // Conversion to balanced ternary.
    let tx: [trit] = math::to_balanced_ternary(x);
    let ty: [trit] = math::to_balanced_ternary(y);

    io::print("x in balanced ternary (LST-first): ");
    io::println(ternary::trits_to_str(tx));
    io::print("y in balanced ternary (LST-first): ");
    io::println(ternary::trits_to_str(ty));

    io::println("");

    // Ternary word arithmetic using t27.
    let a = ternary::int_to_t27(42);
    let b = ternary::int_to_t27(-17);

    io::println("t27 arithmetic (values as balanced ternary strings):");
    io::print("  a (42)  = "); io::println(ternary::t27_to_str(a));
    io::print("  b (-17) = "); io::println(ternary::t27_to_str(b));

    // Shift operations.
    let a_shifted = ternary::trit_shift_left(a, 1);   // multiply by 3
    io::print("  a << 1 (x3) = "); io::print(ternary::t27_to_str(a_shifted));
    io::print(" = "); io::println_int(ternary::t27_to_int(a_shifted));

    let a_rshift = ternary::trit_shift_right(a, 1);   // divide by 3, truncate
    io::print("  a >> 1 (/3) = "); io::print(ternary::t27_to_str(a_rshift));
    io::print(" = "); io::println_int(ternary::t27_to_int(a_rshift));

    // Trit-wise negation.
    let a_neg = ternary::t27_neg(a);
    io::print("  -a (tnot)   = "); io::print(ternary::t27_to_str(a_neg));
    io::print(" = "); io::println_int(ternary::t27_to_int(a_neg));
}

// ---------------------------------------------------------------------------
// 4. Ternary operators: tand, tor, tnot, txor
// ---------------------------------------------------------------------------
fn demo_ternary_operators() {
    heading("4. Ternary Operators (tand / tor / tnot / txor)");

    let p: trit = +;
    let z: trit = 0;
    let n: trit = -;

    io::println("Single-trit operations:");
    io::print("  tnot + = "); io::println_trit(tnot p);
    io::print("  tnot 0 = "); io::println_trit(tnot z);
    io::print("  tnot - = "); io::println_trit(tnot n);

    io::println("");
    io::println("tand (min) truth table:");
    let row_labels: [str] = ["+", "0", "-"];
    let row_vals:   [trit] = [+, 0, -];
    io::println("       +    0    -");
    let mut ri = 0;
    for a in row_vals {
        io::print("  ");
        io::print(row_labels[ri]);
        io::print("  ");
        let mut ci = 0;
        for b in row_vals {
            io::print("  ");
            io::print_trit(a tand b);
            io::print("  ");
            ci = ci + 1;
        }
        io::println("");
        ri = ri + 1;
    }

    io::println("");
    io::println("tor (max) truth table:");
    io::println("       +    0    -");
    let mut ri2 = 0;
    for a in row_vals {
        io::print("  ");
        io::print(row_labels[ri2]);
        io::print("  ");
        for b in row_vals {
            io::print("  ");
            io::print_trit(a tor b);
            io::print("  ");
        }
        io::println("");
        ri2 = ri2 + 1;
    }

    io::println("");
    io::println("txor (mod-3 add, balanced) truth table:");
    io::println("       +    0    -");
    let mut ri3 = 0;
    for a in row_vals {
        io::print("  ");
        io::print(row_labels[ri3]);
        io::print("  ");
        for b in row_vals {
            io::print("  ");
            io::print_trit(a txor b);
            io::print("  ");
        }
        io::println("");
        ri3 = ri3 + 1;
    }

    io::println("");
    io::println("Practical example: signal combining with tand / tor");
    let enable: trit = +;
    let fault:  trit = -;
    let status: trit = enable tand fault;
    io::print("  enable(+) tand fault(-) = ");
    io::println_trit(status);   // min(+1,-1) = -1 -> fault wins
}

// ---------------------------------------------------------------------------
// 5. Packing / unpacking trits
// ---------------------------------------------------------------------------
fn demo_pack_unpack() {
    heading("5. Packing and Unpacking Trits");

    // Build a t27 from an array of trits (LST-first).
    let trits: [trit] = [+, -, 0, +, +, -];  // 6-trit value
    let packed: t27 = ternary::pack_trits(trits);

    io::print("Packed [+,-,0,+,+,-] (LST-first) = ");
    io::print(ternary::t27_to_str(packed));
    io::print("  (int: ");
    io::print_int(ternary::t27_to_int(packed));
    io::println(")");

    // Unpack back.
    let unpacked: [trit; 27] = ternary::unpack_trits(packed);
    io::print("Unpacked first 6 trits (LST-first): ");
    let mut k = 0;
    for t in unpacked {
        if k < 6 {
            io::print_trit(t);
            io::print(" ");
        }
        k = k + 1;
    }
    io::println("");

    // tryte_from_trits.
    let tr: tryte = ternary::tryte_from_trits(+, 0, -);
    io::print("tryte_from_trits(+, 0, -) = ");
    io::print_tryte(tr);
    io::print("  = ");
    io::println_int(ternary::tryte_to_int(tr));
}

// ---------------------------------------------------------------------------
// 6. Ternary vs binary efficiency
// ---------------------------------------------------------------------------
fn demo_efficiency() {
    heading("6. Ternary vs Binary Storage Efficiency");

    // A t27 (27 trits) can represent (3^27 - 1)/2 = 3,812,798,742,493 values.
    // A 27-bit binary integer can represent only 2^27 - 1 = 134,217,727 values.
    // Ratio: log2(3) ≈ 1.585 — each trit carries ~1.585 bits of information.

    io::println("Capacity comparison:");
    io::println("  Format      | Digits | Distinct values        | Bits equivalent");
    io::println("  ------------|--------|------------------------|----------------");
    io::println("  8-bit int   |   8    | 256                    | 8.0");
    io::println("  1-tryte     |   3    | 27                     | 4.75");
    io::println("  t9          |   9    | 19,683                 | 14.3");
    io::println("  16-bit int  |  16    | 65,536                 | 16.0");
    io::println("  t27         |  27    | 7,625,597,484,987      | 42.8");
    io::println("  32-bit int  |  32    | 4,294,967,296          | 32.0");
    io::println("  t54         |  54    | ~5.8e25                | 85.6");
    io::println("  64-bit int  |  64    | 1.8e19                 | 64.0");
    io::println("");
    io::println("Information per digit:");
    io::println("  Binary trit (bit): log2(2) = 1.000 bit");
    io::println("  Balanced trit:     log2(3) = 1.585 bits  (+58.5% efficiency)");
    io::println("");
    io::println("A t27 encodes as much information as a 42.8-bit binary integer,");
    io::println("using the same number of 'digits'.");

    io::println("");
    io::println("Ternary logarithm examples (base 3):");
    let vals: [int] = [1, 3, 9, 27, 81, 243, 729];
    for v in vals {
        io::print("  log3(");
        io::print_int(v);
        io::print(") = ");
        // log3(3^k) = k exactly
        let k = math::to_balanced_ternary(v);
        io::println_int(math::trit_count(v) - 1);
    }
}

// ---------------------------------------------------------------------------
// 7. trit_count and to_balanced_ternary
// ---------------------------------------------------------------------------
fn demo_conversions() {
    heading("7. Integer <-> Balanced Ternary Conversion");

    let samples: [int] = [0, 1, -1, 5, -5, 13, -13, 42, 100, -100, 255, 1000];
    io::println("n       | trit_count | balanced ternary (LST-first)");
    io::println("--------|------------|-----------------------------");
    for n in samples {
        let tc = math::trit_count(n);
        let bt = math::to_balanced_ternary(n);

        io::print("  ");
        // Right-align n in a 6-char field (manual).
        let ns = fmt::show_int(n);
        io::print(fmt::align_right(ns, 6, ' '));
        io::print("  |     ");
        io::print_int(tc);
        io::print("      | ");
        io::println(ternary::trits_to_str(bt));
    }

    io::println("");
    io::println("Round-trip check (int -> ternary -> int):");
    let check_vals: [int] = [42, -99, 1234, -5678];
    for v in check_vals {
        let trits = math::to_balanced_ternary(v);
        let back  = math::from_balanced_ternary(trits);
        io::print("  ");
        io::print_int(v);
        io::print(" -> ");
        io::print(ternary::trits_to_str(trits));
        io::print(" -> ");
        io::print_int(back);
        io::print("  [");
        if v == back {
            io::print("OK");
        } else {
            io::print("FAIL");
        }
        io::println("]");
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------
fn main() {
    io::println("maniT Balanced Ternary Demo");
    io::println("===========================");

    demo_trit_basics();
    demo_word_types();
    demo_arithmetic();
    demo_ternary_operators();
    demo_pack_unpack();
    demo_efficiency();
    demo_conversions();

    io::println("");
    io::println("Demo complete.");
}
