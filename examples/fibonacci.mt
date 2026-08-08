// examples/fibonacci.mt
// Fibonacci sequence in maniT.
//
// Demonstrates:
//   - Recursive and iterative function definitions
//   - Result<T,E> for overflow detection
//   - match on Result variants
//   - while loops with let mut

use std::io;

// ---------------------------------------------------------------------------
// Recursive Fibonacci (int)
// ---------------------------------------------------------------------------
fn fib_recursive(n: int) -> int {
    if n <= 0 {
        return 0;
    } elif n == 1 {
        return 1;
    } else {
        return fib_recursive(n - 1) + fib_recursive(n - 2);
    }
}

// ---------------------------------------------------------------------------
// Iterative Fibonacci (int)
// ---------------------------------------------------------------------------
fn fib_iterative(n: int) -> int {
    if n <= 0 { return 0; }
    if n == 1 { return 1; }

    let mut a = 0;
    let mut b = 1;
    let mut i = 2;
    while i <= n {
        let tmp = a + b;
        a = b;
        b = tmp;
        i = i + 1;
    }
    return b;
}

// ---------------------------------------------------------------------------
// Fibonacci with overflow check -> Result<int, str>
// ---------------------------------------------------------------------------
fn fib_safe(n: int) -> Result<int, str> {
    if n < 0 {
        return Err("fib is not defined for negative indices");
    }
    if n > 92 {
        return Err("overflow: exceeds 64-bit int range");
    }
    return Ok(fib_iterative(n));
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------
fn print_fib_row(n: int, val: int) {
    io::print("  fib(");
    io::print_int(n);
    io::print(") = ");
    io::println_int(val);
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------
fn main() {
    io::println("Fibonacci in maniT");
    io::println("==================");

    // --- Recursive (small values only) ------------------------------------
    io::println("");
    io::println("Recursive Fibonacci (n = 0..10):");
    let mut i = 0;
    while i <= 10 {
        print_fib_row(i, fib_recursive(i));
        i = i + 1;
    }

    // --- Iterative -----------------------------------------------------------
    io::println("");
    io::println("Iterative Fibonacci (n = 0..20):");
    let mut j = 0;
    while j <= 20 {
        print_fib_row(j, fib_iterative(j));
        j = j + 1;
    }

    // --- Safe (Result-based) -------------------------------------------------
    io::println("");
    io::println("Safe Fibonacci with overflow detection:");
    let test_indices: [int] = [30, 50, 70, 90, 92, 93, 100];
    for idx in test_indices {
        io::print("  fib_safe(");
        io::print_int(idx);
        io::print(") -> ");
        match fib_safe(idx) {
            Ok(v)      => { io::print("Ok("); io::print_int(v); io::println(")"); }
            Unknown(m) => { io::print("Unknown("); io::print(m); io::println(")"); }
            Err(e)     => { io::print("Err(\""); io::print(e); io::println("\")"); }
        }
    }

    // --- Properties ----------------------------------------------------------
    io::println("");
    io::println("F(n) mod 3 for n=0..15 (Pisano period 8):");
    io::print("  ");
    let mut q = 0;
    while q <= 15 {
        io::print_int(fib_iterative(q) % 3);
        if q < 15 { io::print(", "); }
        q = q + 1;
    }
    io::println("");

    io::println("");
    io::println("F(n+1)/F(n) converging to golden ratio (phi ~ 1.618...):");
    let phi_indices: [int] = [5, 10, 15, 20, 25, 30];
    for idx in phi_indices {
        let fn_    = fib_iterative(idx);
        let fn_1   = fib_iterative(idx + 1);
        let ratio_int  = fn_1 / fn_;
        let ratio_frac = (fn_1 * 1000000 / fn_) % 1000000;
        io::print("  F(");
        io::print_int(idx + 1);
        io::print(")/F(");
        io::print_int(idx);
        io::print(") = ");
        io::print_int(ratio_int);
        io::print(".");
        io::println_int(ratio_frac);
    }

    io::println("");
    io::println("Done.");
}
