// benchmarks/01_arithmetic.mt
// Pure arithmetic benchmark — no strings, no Result.
// Tests: loops, recursion, function calls, integer arithmetic.

use std::io;

// Recursive fibonacci
fn fib(n: int) -> int {
    if n <= 1 {
        return n;
    }
    return fib(n - 1) + fib(n - 2);
}

// Iterative fibonacci
fn fib_iter(n: int) -> int {
    if n <= 1 { return n; }
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

// Collatz sequence length
fn collatz_len(n: int) -> int {
    let mut x = n;
    let mut count = 0;
    while x != 1 {
        if x % 2 == 0 {
            x = x / 2;
        } else {
            x = x * 3 + 1;
        }
        count = count + 1;
    }
    return count;
}

// Sum of divisors (for classifying perfect/abundant/deficient)
fn sum_divisors(n: int) -> int {
    let mut sum = 0;
    let mut i = 1;
    while i * i <= n {
        if n % i == 0 {
            sum = sum + i;
            if i != n / i {
                sum = sum + n / i;
            }
        }
        i = i + 1;
    }
    return sum - n;
}

// GCD via Euclidean algorithm
fn gcd(a: int, b: int) -> int {
    let mut x = a;
    let mut y = b;
    while y != 0 {
        let tmp = y;
        y = x % y;
        x = tmp;
    }
    return x;
}

// Integer power
fn ipow(base: int, exp: int) -> int {
    let mut result = 1;
    let mut b = base;
    let mut e = exp;
    while e > 0 {
        if e % 2 == 1 {
            result = result * b;
        }
        b = b * b;
        e = e / 2;
    }
    return result;
}

fn main() {
    io::println("=== Arithmetic Benchmark ===");

    // Recursive fib(20)
    let f20 = fib(20);
    io::print("fib(20) = ");
    io::println_int(f20);

    // Iterative fib(50)
    let f50 = fib_iter(50);
    io::print("fib_iter(50) = ");
    io::println_int(f50);

    // Collatz lengths for 1..100
    let mut max_collatz = 0;
    let mut max_n = 1;
    let mut n = 1;
    while n <= 1000 {
        let len = collatz_len(n);
        if len > max_collatz {
            max_collatz = len;
            max_n = n;
        }
        n = n + 1;
    }
    io::print("Longest Collatz (1..1000): n=");
    io::print_int(max_n);
    io::print(" len=");
    io::println_int(max_collatz);

    // Count perfect numbers up to 10000
    let mut perfect_count = 0;
    let mut i = 2;
    while i <= 10000 {
        if sum_divisors(i) == i {
            perfect_count = perfect_count + 1;
        }
        i = i + 1;
    }
    io::print("Perfect numbers <= 10000: ");
    io::println_int(perfect_count);

    // GCD chain
    let g = gcd(gcd(gcd(1071, 462), 1029), 1155);
    io::print("GCD(1071,462,1029,1155) = ");
    io::println_int(g);

    // Power
    let p = ipow(3, 12);
    io::print("3^12 = ");
    io::println_int(p);

    io::println("Done.");
}
