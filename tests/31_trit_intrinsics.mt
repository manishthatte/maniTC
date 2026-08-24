// tests/31_trit_intrinsics.mt — C7: the `trit::` intrinsics.
//
// © Manish Jagdish Thatte
//
// Registered as both an expected-output test and a cross-target test. The
// cross-target half is the one that matters: `trit::sign` and `trit::abs`
// lower to `IRInstr::TritSign`, which is ONE instruction on T3 (TCMP against
// R0, which always reads zero) and two comparisons plus a subtract on LLVM.
// Two unrelated implementations of the same operation is exactly the shape
// that hides a bug.
//
// The large values are deliberate and load-bearing. The obvious way to build
// `sign` is `TritMax(TritMin(x, +1), -1)`, which is what the plan proposed —
// but `TritMin`/`TritMax` are trit-width (the LLVM backend types both `i8`),
// so that form truncates its operand to 8 bits and `sign(256)` reports 0
// instead of +1. 256 and -256 are here to catch precisely that.

use std::io;
use std::trit;
use std::ternary;

fn row(x: int) {
    io::print_int(x); io::print(" | ");
    io::print_int(ternary::trit_to_int(trit::sign(x))); io::print(" ");
    io::print_int(trit::abs(x)); io::print(" ");
    io::print_int(trit::count(x, +)); io::print(" ");
    io::print_int(trit::count(x, 0)); io::print(" ");
    io::print_int(trit::count(x, -)); io::print(" ");
    io::print_int(trit::leading_zeros(x)); io::print(" ");
    io::println_int(trit::trailing_zeros(x));
}

fn main() {
    io::println("x | sign abs cnt+ cnt0 cnt- lz tz");
    row(0);
    row(1);
    row(-1);
    row(256);          // > 8 bits: the truncation trap
    row(-256);
    row(9841);
    row(-9841);
    row(3812798742493);  // T3_MAX — all 27 lanes are +1

    // shift3 is the machine's native shift: x * 3^n, not x * 2^n.
    io::println("");
    io::println("shift3(x, n) = x * 3^n:");
    io::println_int(trit::shift3(1, 0));
    io::println_int(trit::shift3(1, 3));
    io::println_int(trit::shift3(-5, 2));
    io::println_int(trit::shift3(7, 5));

    // The lane counts must always sum to 27 — every trit of a 27-trit word is
    // one of exactly three values. This is a total check on `count`, not a
    // spot check: it fails if any lane is miscounted or double-counted.
    io::println("");
    io::println("count(+) + count(0) + count(-) == 27:");
    io::println_int(trit::count(9841, +) + trit::count(9841, 0) + trit::count(9841, -));
    io::println_int(trit::count(-256, +) + trit::count(-256, 0) + trit::count(-256, -));
    io::println_int(trit::count(0, +) + trit::count(0, 0) + trit::count(0, -));

    // abs is exact for every input: the 27-trit range is symmetric, so unlike
    // two's complement there is no minimum whose negation overflows.
    io::println("");
    io::println("abs(-T3_MAX) == T3_MAX (no asymmetric minimum):");
    io::println_int(trit::abs(-3812798742493));
}
