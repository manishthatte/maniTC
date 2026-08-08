// examples/data_structures.mt
// Standard and ternary-native data structures in maniT.
//
// Demonstrates:
//   - Vec<T>: push, pop, map, filter, fold, sort
//   - Map<K,V>: insert, get, iteration
//   - Set<T>: insert, contains, intersection/union
//   - Deque<T>: push_front/back, pop_front/back
//   - TernaryTrie<V>: insert/get by trit key (maniT-native!)
//     * keys derived from balanced ternary decomposition of integers
//     * prefix queries
//     * frequency counting over a ternary domain

use std::io;
use std::fmt;
use std::math;
use std::ternary;
use std::collections;

// ---------------------------------------------------------------------------
// Helper: section heading
// ---------------------------------------------------------------------------
fn section(title: str) {
    io::println("");
    io::println("--------------------------------------------");
    io::print("  ");
    io::println(title);
    io::println("--------------------------------------------");
}

// ---------------------------------------------------------------------------
// 1. Vec<T>
// ---------------------------------------------------------------------------
fn demo_vec() {
    section("1. Vec<int>");

    let v: Vec<int> = Vec::new();
    let data: [int] = [5, 3, 8, 1, 9, 2, 7, 4, 6];
    for d in data {
        v.push(d);
    }

    io::print("  Original: ");
    v.for_each(fn(x: int) { io::print_int(x); io::print(" "); });
    io::println("");

    // Sort in place.
    v.sort();
    io::print("  Sorted:   ");
    v.for_each(fn(x: int) { io::print_int(x); io::print(" "); });
    io::println("");

    // map: square each element.
    let squares = v.map(fn(x: int) -> int => x * x);
    io::print("  Squared:  ");
    squares.for_each(fn(x: int) { io::print_int(x); io::print(" "); });
    io::println("");

    // filter: keep only even numbers.
    let evens = v.filter(fn(x: int) -> bool => x % 2 == 0);
    io::print("  Evens:    ");
    evens.for_each(fn(x: int) { io::print_int(x); io::print(" "); });
    io::println("");

    // fold: sum all elements.
    let total = v.fold(0, fn(acc: int, x: int) -> int => acc + x);
    io::print("  Sum:      ");
    io::println_int(total);

    // pop last element.
    let last = v.pop();
    io::print("  Popped:   ");
    io::println_int(last);
    io::print("  Length after pop: ");
    io::println_int(v.len());

    // slice.
    let middle = v.slice(2, 5);
    io::print("  Slice[2..5]: ");
    middle.for_each(fn(x: int) { io::print_int(x); io::print(" "); });
    io::println("");
}

// ---------------------------------------------------------------------------
// 2. Map<K, V>
// ---------------------------------------------------------------------------
fn demo_map() {
    section("2. Map<str, int>  (word frequency count)");

    let words: [str] = [
        "ternary", "binary", "ternary", "trit", "bit",
        "trit", "ternary", "logic", "bit", "trit",
    ];

    let freq: Map<str, int> = Map::new();
    for w in words {
        if freq.contains_key(w) {
            freq.insert(w, freq.get(w) + 1);
        } else {
            freq.insert(w, 1);
        }
    }

    io::println("  Word frequencies:");
    let keys = freq.keys();
    keys.sort();  // alphabetical order
    let mut ki = 0;
    let klen = keys.len();
    while ki < klen {
        let k = keys.get(ki);
        io::print("    ");
        io::print(fmt::align_left(k, 10, ' '));
        io::print(": ");
        io::println_int(freq.get(k));
        ki = ki + 1;
    }

    io::print("  Unique words: ");
    io::println_int(freq.len());

    io::print("  Contains 'ternary': ");
    io::println_bool(freq.contains_key("ternary"));
    io::print("  Contains 'binary':  ");
    io::println_bool(freq.contains_key("binary"));

    // get_or for a missing key.
    io::print("  Freq of 'decimal' (default 0): ");
    io::println_int(freq.get_or("decimal", 0));
}

// ---------------------------------------------------------------------------
// 3. Set<T>
// ---------------------------------------------------------------------------
fn demo_set() {
    section("3. Set<int>  (prime sieve intersection)");

    // Small primes.
    let primes: Set<int> = Set::new();
    let prime_list: [int] = [2, 3, 5, 7, 11, 13, 17, 19, 23, 29];
    for p in prime_list {
        primes.insert(p);
    }

    // Fibonacci numbers (first several).
    let fibs: Set<int> = Set::new();
    let fib_list: [int] = [1, 2, 3, 5, 8, 13, 21, 34];
    for f in fib_list {
        fibs.insert(f);
    }

    // Intersection: primes that are also Fibonacci numbers.
    let prime_fibs = primes.intersection(fibs);
    io::print("  Primes ∩ Fibonacci: ");
    let pf_vec: Vec<int> = Vec::new();
    for p in prime_list {
        if prime_fibs.contains(p) {
            pf_vec.push(p);
        }
    }
    pf_vec.sort();
    pf_vec.for_each(fn(x: int) { io::print_int(x); io::print(" "); });
    io::println("");

    // Union.
    let all = primes.union(fibs);
    io::print("  |primes ∪ fibs| = ");
    io::println_int(all.len());

    // Difference: primes not in fibs.
    let only_primes = primes.difference(fibs);
    io::print("  Primes only (not Fibonacci): ");
    let op_vec: Vec<int> = Vec::new();
    for p in prime_list {
        if only_primes.contains(p) {
            op_vec.push(p);
        }
    }
    op_vec.sort();
    op_vec.for_each(fn(x: int) { io::print_int(x); io::print(" "); });
    io::println("");

    io::print("  fibs ⊆ all: ");
    io::println_bool(fibs.is_subset(all));
}

// ---------------------------------------------------------------------------
// 4. Deque<T>
// ---------------------------------------------------------------------------
fn demo_deque() {
    section("4. Deque<str>  (sliding window)");

    let dq: Deque<str> = Deque::new();
    let items: [str] = ["alpha", "beta", "gamma", "delta", "epsilon", "zeta"];

    // Fill the deque.
    for item in items {
        dq.push_back(item);
        io::print("  pushed back: ");
        io::println(item);
    }

    io::print("  deque size: ");
    io::println_int(dq.len());

    // Front/back peek.
    io::print("  front: ");
    io::println(dq.front());
    io::print("  back:  ");
    io::println(dq.back());

    // Drain from front while non-empty.
    io::print("  drain front: ");
    loop {
        if dq.is_empty() { break; }
        io::print(dq.pop_front());
        if !dq.is_empty() { io::print(" -> "); }
    }
    io::println("");

    // Demonstrate push_front for stack-like use.
    let stack: Deque<int> = Deque::new();
    let nums: [int] = [1, 2, 3, 4, 5];
    for n in nums {
        stack.push_front(n);   // push_front makes latest item the front
    }
    io::print("  LIFO (push_front, pop_front): ");
    loop {
        if stack.is_empty() { break; }
        io::print_int(stack.pop_front());
        if !stack.is_empty() { io::print(" "); }
    }
    io::println("");
}

// ---------------------------------------------------------------------------
// 5. TernaryTrie<V> — trit-indexed prefix tree (maniT-native!)
// ---------------------------------------------------------------------------

// Utility: convert an int to a fixed-width Vec<trit> key (MST-first).
// Uses direct integer arithmetic to avoid dependency on to_balanced_ternary.
fn bt_key(n: int, width: int) -> Vec<trit> {
    // Compute balanced ternary digits LST-first into a Vec
    let digits: Vec<trit> = Vec::new();
    let mut rem = n;
    let mut i = 0;
    while i < width {
        let d3 = rem % 3;
        if d3 == 2 {
            digits.push(-1);
            rem = (rem + 1) / 3;
        } elif d3 == -2 {
            digits.push(1);
            rem = (rem - 1) / 3;
        } else {
            digits.push(d3);
            rem = rem / 3;
        }
        i = i + 1;
    }
    // Reverse to get MST-first order
    digits.reverse();
    digits
}

fn demo_ternary_trie() {
    section("5. TernaryTrie<str>  (map over ternary key space)");

    let trie: TernaryTrie<str> = TernaryTrie::new();

    // Insert entries keyed by 4-trit balanced ternary representations.
    trie.insert(bt_key(0,   4), "zero");
    trie.insert(bt_key(1,   4), "one");
    trie.insert(bt_key(-1,  4), "minus-one");
    trie.insert(bt_key(4,   4), "four");
    trie.insert(bt_key(-4,  4), "minus-four");
    trie.insert(bt_key(13,  4), "thirteen");
    trie.insert(bt_key(-13, 4), "minus-thirteen");
    trie.insert(bt_key(27,  4), "twenty-seven");
    trie.insert(bt_key(40,  4), "forty");

    io::print("  Trie size: ");
    io::println_int(trie.len());
    io::println("");

    // Direct lookups.
    io::println("  Direct lookups:");
    let k0 = bt_key(0, 4);
    if trie.contains_key(k0) {
        io::print("    n=0   -> ");
        io::println(trie.get(k0));
    }
    let k1 = bt_key(1, 4);
    if trie.contains_key(k1) {
        io::print("    n=1   -> ");
        io::println(trie.get(k1));
    }
    let km1 = bt_key(-1, 4);
    if trie.contains_key(km1) {
        io::print("    n=-1  -> ");
        io::println(trie.get(km1));
    }
    let k13 = bt_key(13, 4);
    if trie.contains_key(k13) {
        io::print("    n=13  -> ");
        io::println(trie.get(k13));
    }
    let km13 = bt_key(-13, 4);
    if trie.contains_key(km13) {
        io::print("    n=-13 -> ");
        io::println(trie.get(km13));
    }

    // Prefix query: all keys starting with [+] (positive values, MST=+1).
    io::println("");
    io::println("  Keys with prefix [+]  (positive numbers):");
    let prefix_pos: Vec<trit> = Vec::new();
    prefix_pos.push(+);
    let pos_keys = trie.keys_with_prefix(prefix_pos);
    let mut pki = 0;
    let pklen = pos_keys.len();
    while pki < pklen {
        let k = pos_keys.get(pki);
        if trie.contains_key(k) {
            io::print("    -> ");
            io::println(trie.get(k));
        }
        pki = pki + 1;
    }

    // Prefix query: all keys starting with [-] (negative values).
    io::println("");
    io::println("  Keys with prefix [-]  (negative numbers):");
    let prefix_neg: Vec<trit> = Vec::new();
    prefix_neg.push(-);
    let neg_keys = trie.keys_with_prefix(prefix_neg);
    let mut nki = 0;
    let nklen = neg_keys.len();
    while nki < nklen {
        let k = neg_keys.get(nki);
        if trie.contains_key(k) {
            io::print("    -> ");
            io::println(trie.get(k));
        }
        nki = nki + 1;
    }
}

// ---------------------------------------------------------------------------
// 6. TernaryTrie as a sparse ternary state space
// ---------------------------------------------------------------------------
fn demo_trie_state_space() {
    section("6. TernaryTrie — 3x3 ternary grid");

    // Key: [row_trit, col_trit] — 9 cells in a 3x3 grid.
    let grid: TernaryTrie<trit> = TernaryTrie::new();

    let row_trits: [trit] = [-, 0, +];
    let col_trits: [trit] = [-, 0, +];

    let mut r = 0;
    for rt in row_trits {
        let mut c = 0;
        for ct in col_trits {
            let val: trit = if r == c {
                +
            } elif r < c {
                -
            } else {
                0
            };
            let key: Vec<trit> = Vec::new();
            key.push(rt);
            key.push(ct);
            grid.insert(key, val);
            c = c + 1;
        }
        r = r + 1;
    }

    io::println("  3x3 ternary grid (rows: -, 0, +; cols: -, 0, +):");
    io::println("         col- col0 col+");
    for rt in row_trits {
        io::print("  row");
        io::print_trit(rt);
        io::print(":  ");
        for ct in col_trits {
            let key: Vec<trit> = Vec::new();
            key.push(rt);
            key.push(ct);
            let cell_val = grid.get(key);
            io::print("  ");
            io::print_trit(cell_val);
            io::print("   ");
        }
        io::println("");
    }

    // Prefix query: all cells in row 0 (row_trit = 0).
    io::println("");
    io::print("  Row 0 (prefix [0]): ");
    let pfx0: Vec<trit> = Vec::new();
    pfx0.push(0);
    let row0_keys = grid.keys_with_prefix(pfx0);
    let mut rki = 0;
    let rklen = row0_keys.len();
    while rki < rklen {
        let k = row0_keys.get(rki);
        io::print_trit(grid.get(k));
        io::print(" ");
        rki = rki + 1;
    }
    io::println("");
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------
fn main() {
    io::println("maniT Data Structures Demo");
    io::println("==========================");

    demo_vec();
    demo_map();
    demo_set();
    demo_deque();
    demo_ternary_trie();
    demo_trie_state_space();

    io::println("");
    io::println("All demos complete.");
}
