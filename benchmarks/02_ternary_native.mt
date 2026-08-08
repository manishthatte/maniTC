// benchmarks/02_ternary_native.mt
// Ternary-native operations benchmark.
// Tests: trit types, tand/tor/tnot/txor, tif branching, balanced ternary arithmetic.
// These operations are native to T3ISA and have NO binary equivalent.

use std::io;
use std::ternary;

// Ternary majority vote: returns the majority of three trits
fn majority(a: trit, b: trit, c: trit) -> trit {
    // majority = (a tand b) tor (b tand c) tor (a tand c)
    let ab: trit = a tand b;
    let bc: trit = b tand c;
    let ac: trit = a tand c;
    let tmp: trit = ab tor bc;
    return tmp tor ac;
}

// Ternary consensus: returns + only if all positive, - if all negative, else 0
fn consensus(a: trit, b: trit, c: trit) -> trit {
    let bc: trit = b tand c;
    return a tand bc;
}

// Count positive trits in a value (balanced ternary digit extraction)
fn count_positive_trits(val: int) -> int {
    let mut count = 0;
    let mut x = val;
    let mut i = 0;
    while i < 27 {
        let digit = x % 3;
        if digit == 1 {
            count = count + 1;
        }
        x = x / 3;
        i = i + 1;
    }
    return count;
}

// Balanced ternary addition using trit-level operations
fn bt_weight(val: int) -> int {
    // Sum of all trit values (+ counts as +1, - counts as -1, 0 counts as 0)
    let mut weight = 0;
    let mut x = val;
    let mut i = 0;
    while i < 27 {
        let rem = x % 3;
        if rem == 1 {
            weight = weight + 1;
        } elif rem == 2 {
            weight = weight - 1;
        }
        x = x / 3;
        i = i + 1;
    }
    return weight;
}

// Three-way classify: positive, zero, or negative
fn classify(x: int) -> trit {
    if x > 0 {
        return +;
    } elif x == 0 {
        return 0;
    } else {
        return -;
    }
}

// Ternary search: find target in sorted array using three-way comparison
// Returns index or -1 if not found
fn ternary_search(arr_start: int, arr_len: int, target: int) -> int {
    let mut lo = 0;
    let mut hi = arr_len - 1;

    while lo <= hi {
        let third = (hi - lo) / 3;
        let m1 = lo + third;
        let m2 = hi - third;

        // Three-way branch at each probe point
        let v1 = arr_start + m1 * m1;  // simulated sorted array: f(i) = start + i*i
        let v2 = arr_start + m2 * m2;

        tif classify(target - v1) {
            + => {
                tif classify(target - v2) {
                    + => { lo = m2 + 1; }
                    0 => { return m2; }
                    - => { lo = m1 + 1; hi = m2 - 1; }
                }
            }
            0 => { return m1; }
            - => { hi = m1 - 1; }
        }
    }
    return 0 - 1;
}

fn main() {
    io::println("=== Ternary-Native Benchmark ===");

    // Majority vote over many combinations
    let trits: [trit] = [+, 0, -];
    let mut pos_count = 0;
    let mut zero_count = 0;
    let mut neg_count = 0;

    for a in trits {
        for b in trits {
            for c in trits {
                let m = majority(a, b, c);
                tif m {
                    + => { pos_count = pos_count + 1; }
                    0 => { zero_count = zero_count + 1; }
                    - => { neg_count = neg_count + 1; }
                }
            }
        }
    }
    io::print("Majority votes (27 combos): +:");
    io::print_int(pos_count);
    io::print(" 0:");
    io::print_int(zero_count);
    io::print(" -:");
    io::println_int(neg_count);

    // Balanced ternary weight for range of values
    let mut total_weight = 0;
    let mut i = 0;
    while i < 1000 {
        total_weight = total_weight + bt_weight(i);
        i = i + 1;
    }
    io::print("Sum of BT weights (0..999): ");
    io::println_int(total_weight);

    // Positive trit counting
    let mut total_pos = 0;
    let mut j = 0;
    while j < 1000 {
        total_pos = total_pos + count_positive_trits(j);
        j = j + 1;
    }
    io::print("Total positive trits (0..999): ");
    io::println_int(total_pos);

    // Ternary search: find values in simulated sorted array
    let mut found = 0;
    let mut k = 0;
    while k < 100 {
        let target = k * k;  // search for perfect squares
        let idx = ternary_search(0, 100, target);
        if idx >= 0 {
            found = found + 1;
        }
        k = k + 1;
    }
    io::print("Ternary search hits: ");
    io::println_int(found);

    // Three-way classification benchmark
    let mut classify_sum = 0;
    let mut m = 0 - 500;
    while m <= 500 {
        let c = classify(m);
        tif c {
            + => { classify_sum = classify_sum + 1; }
            0 => {}
            - => { classify_sum = classify_sum - 1; }
        }
        m = m + 1;
    }
    io::print("Classify balance (-500..500): ");
    io::println_int(classify_sum);

    io::println("Done.");
}
