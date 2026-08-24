// tests/30_lanewise.mt — C2 / T3ISA v1.5: lane-wise ternary logic.
//
// © Manish Jagdish Thatte
//
// The lane-wise family reads a word as 27 independent trits rather than as a
// magnitude, so one T3 instruction does 27 three-valued operations. This file
// is registered as BOTH an expected-output test and a cross-target test: the
// second is the one that matters here, because the LLVM backend reaches these
// operators through C runtime calls (manit_lane_*) while T3 has real
// instructions for them, and nothing else in the suite exercises that path.
//
// It is not a formality. The cross-target check caught `tnotw 9841` returning
// -113 on LLVM and -9841 on T3: lane-wise NOT was lowered to `IRInstr::TritNeg`,
// which the LLVM backend types `i8` because a trit fits in one, so every word
// wider than 8 bits was truncated. Both backends looked self-consistent alone.

use std::io;

fn row(a: int, b: int) {
    io::print_int(a);
    io::print(" ");
    io::print_int(b);
    io::print(" | ");
    io::print_int(a tandw b);
    io::print(" ");
    io::print_int(a torw b);
    io::print(" ");
    io::print_int(a txorw b);
    io::print(" ");
    io::print_int(a timpw b);
    io::print(" ");
    io::print_int(a tcmpw b);
    io::print(" ");
    io::println_int(tnotw a);
}

fn main() {
    io::println("a b | tandw torw txorw timpw tcmpw tnotw(a)");
    row(5, -7);
    row(0, 0);
    row(1, -1);
    row(9841, -9841);
    row(121, 40);
    row(-3, 3);

    // The deduction theorem, 27 lanes at a time. `a timpw a` must be the
    // all-+1 word (3812798742493) for EVERY input, including lanes holding 0 —
    // under Kleene's max(-a, b) those lanes would give 0 and the word would
    // not be all-ones. This is the lane-wise witness that the logic is L3.
    io::println("");
    io::println("deduction theorem: a timpw a == 3812798742493");
    io::println_int(5 timpw 5);
    io::println_int(0 timpw 0);
    io::println_int(9841 timpw 9841);
    io::println_int(-9841 timpw -9841);
    io::println_int(121 timpw 121);

    // Width guard. Every one of these exceeds 8 bits, which is exactly the
    // case that was silently truncated on LLVM. 3812798742493 is the widest
    // word the 27 lanes can hold.
    io::println("");
    io::println("width guard: tnotw beyond 8 bits");
    io::println_int(tnotw 9841);
    io::println_int(tnotw 100000);
    io::println_int(tnotw 3812798742493);
    io::println_int(tnotw tnotw 121);

    // Algebraic identities that pin the lane semantics rather than the
    // encoding: the all-+1 word is the identity for lane-wise min, and the
    // all--1 word is the identity for lane-wise max.
    io::println("");
    io::println("identities: x tandw all(+1) == x, x torw all(-1) == x");
    io::println_int(5 tandw 3812798742493);
    io::println_int(-7 tandw 3812798742493);
    io::println_int(9841 tandw 3812798742493);
    io::println_int(5 torw -3812798742493);
    io::println_int(-100000 torw -3812798742493);

    // txorw is balanced sum mod 3, so it is NOT an involution: 3k = 0 (mod 3)
    // takes THREE applications to recover the original, not two. Applying the
    // same key twice gives a different word; a third application restores it.
    io::println("");
    io::println("txorw needs THREE applications to recover");
    io::println_int(5 txorw 121);
    io::println_int(5 txorw 121 txorw 121);
    io::println_int(5 txorw 121 txorw 121 txorw 121);
}
