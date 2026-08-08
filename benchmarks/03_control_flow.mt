// benchmarks/03_control_flow.mt
// Control flow intensive benchmark.
// Tests: nested loops, deep recursion, branching patterns.
// Highlights T3ISA's three-way branching advantage.

use std::io;

// Ackermann function (deeply recursive, heavy control flow)
fn ackermann(m: int, n: int) -> int {
    if m == 0 {
        return n + 1;
    } elif n == 0 {
        return ackermann(m - 1, 1);
    } else {
        return ackermann(m - 1, ackermann(m, n - 1));
    }
}

// Three-way comparison sort (optimal for ternary: one compare, three branches)
fn sort3(a: int, b: int, c: int) -> int {
    // Returns median (middle value) — uses three-way compare naturally
    if a <= b {
        if b <= c {
            return b;
        } elif a <= c {
            return c;
        } else {
            return a;
        }
    } else {
        if a <= c {
            return a;
        } elif b <= c {
            return c;
        } else {
            return b;
        }
    }
}

// Sieve-like prime counting without arrays (trial division)
fn is_prime(n: int) -> int {
    if n < 2 { return 0; }
    if n < 4 { return 1; }
    if n % 2 == 0 { return 0; }
    if n % 3 == 0 { return 0; }
    let mut i = 5;
    while i * i <= n {
        if n % i == 0 { return 0; }
        if n % (i + 2) == 0 { return 0; }
        i = i + 6;
    }
    return 1;
}

// Matrix multiply 4x4 (simulated with flat indexing)
// Represents 4x4 matrix as 16 sequential computations
fn mat_elem(row: int, col: int) -> int {
    // Generate a deterministic test matrix element
    return (row + 1) * 10 + col + 1;
}

fn mat_mul_elem(r: int, c: int) -> int {
    // (A * A)[r][c] = sum over k of A[r][k] * A[k][c]
    let mut sum = 0;
    let mut k = 0;
    while k < 4 {
        sum = sum + mat_elem(r, k) * mat_elem(k, c);
        k = k + 1;
    }
    return sum;
}

// Nested loop: compute sum of all i*j*k for i,j,k in 1..N
fn triple_product_sum(n: int) -> int {
    let mut sum = 0;
    let mut i = 1;
    while i <= n {
        let mut j = 1;
        while j <= n {
            let mut k = 1;
            while k <= n {
                sum = sum + i * j * k;
                k = k + 1;
            }
            j = j + 1;
        }
        i = i + 1;
    }
    return sum;
}

fn main() {
    io::println("=== Control Flow Benchmark ===");

    // Ackermann
    let a = ackermann(3, 4);
    io::print("ackermann(3,4) = ");
    io::println_int(a);

    // Three-way sort: find medians of many triples
    let mut median_sum = 0;
    let mut i = 1;
    while i <= 50 {
        let mut j = 1;
        while j <= 50 {
            let mut k = 1;
            while k <= 50 {
                median_sum = median_sum + sort3(i, j, k);
                k = k + 10;
            }
            j = j + 10;
        }
        i = i + 10;
    }
    io::print("Median sum (5x5x5 triples): ");
    io::println_int(median_sum);

    // Prime counting
    let mut prime_count = 0;
    let mut n = 2;
    while n <= 10000 {
        prime_count = prime_count + is_prime(n);
        n = n + 1;
    }
    io::print("Primes <= 10000: ");
    io::println_int(prime_count);

    // Matrix multiply trace
    let mut trace = 0;
    let mut r = 0;
    while r < 4 {
        trace = trace + mat_mul_elem(r, r);
        r = r + 1;
    }
    io::print("tr(A*A) [4x4]: ");
    io::println_int(trace);

    // Triple product sum
    let tps = triple_product_sum(20);
    io::print("Sum(i*j*k), i,j,k=1..20: ");
    io::println_int(tps);

    io::println("Done.");
}
