// Test: TernaryTrie<V> — insert, get, contains_key, len, keys, keys_with_prefix
//       This is the maniT-native data structure indexed by Vec<trit> keys.
use std::io;

fn pass(label: str) { io::print("PASS "); io::println(label); }
fn fail(label: str) { io::print("FAIL "); io::println(label); }
fn check(label: str, cond: bool) { if cond { pass(label) } else { fail(label) } }
fn check_int(label: str, got: int, want: int) {
    if got == want { pass(label); }
    else {
        io::print("FAIL "); io::print(label);
        io::print(" got="); io::print_int(got);
        io::print(" want="); io::print_int(want);
        io::newline();
    }
}

// ---------------------------------------------------------------------------
// Helper: build a Vec<int> (trit sequence) for a given integer value.
// Decomposes n into balanced ternary digits (LST-first order).
// Each digit is -1, 0, or +1.
// ---------------------------------------------------------------------------

fn int_to_trit_key(n: int) -> Vec<int> {
    let key: Vec<int> = Vec::new();
    if n == 0 {
        key.push(0);
        return key;
    }
    let mut m: int = n;
    while m != 0 {
        let r: int = m % 3;
        let d: int = if r > 1 { r - 3 } elif r < -1 { r + 3 } else { r };
        key.push(d);
        m = (m - d) / 3;
    }
    key
}

// ---------------------------------------------------------------------------
// 1. Basic insert and get
// ---------------------------------------------------------------------------

fn test_basic_insert_get() {
    let mut trie: TernaryTrie<int> = TernaryTrie::new();
    check_int("trie: initial len=0", trie.len(), 0);

    // Insert three key-value pairs
    let k1: Vec<int> = Vec::new(); k1.push(1);                    // key [+]
    let k2: Vec<int> = Vec::new(); k2.push(1); k2.push(-1);      // key [+,-]
    let k3: Vec<int> = Vec::new(); k3.push(0);                    // key [0]

    trie.insert(k1, 100);
    trie.insert(k2, 200);
    trie.insert(k3, 300);

    check_int("trie: len=3 after inserts", trie.len(), 3);

    // Retrieve values
    let r1: Vec<int> = Vec::new(); r1.push(1);
    let r2: Vec<int> = Vec::new(); r2.push(1); r2.push(-1);
    let r3: Vec<int> = Vec::new(); r3.push(0);

    check_int("trie: get [+]=100",    trie.get(r1), 100);
    check_int("trie: get [+,-]=200",  trie.get(r2), 200);
    check_int("trie: get [0]=300",    trie.get(r3), 300);
}

// ---------------------------------------------------------------------------
// 2. contains_key
// ---------------------------------------------------------------------------

fn test_contains_key() {
    let mut trie: TernaryTrie<int> = TernaryTrie::new();

    let k1: Vec<int> = Vec::new(); k1.push(1); k1.push(0);      // [+, 0]
    let k2: Vec<int> = Vec::new(); k2.push(-1);                   // [-]
    trie.insert(k1, 42);
    trie.insert(k2, 99);

    let ck1: Vec<int> = Vec::new(); ck1.push(1); ck1.push(0);
    let ck2: Vec<int> = Vec::new(); ck2.push(-1);
    let ck_miss: Vec<int> = Vec::new(); ck_miss.push(1); ck_miss.push(1);

    check("trie-ck: [+,0] exists",     trie.contains_key(ck1));
    check("trie-ck: [-] exists",       trie.contains_key(ck2));
    check("trie-ck: [+,+] not exists", !trie.contains_key(ck_miss));
}

// ---------------------------------------------------------------------------
// 3. Overwrite existing key
// ---------------------------------------------------------------------------

fn test_overwrite() {
    let mut trie: TernaryTrie<int> = TernaryTrie::new();

    let k: Vec<int> = Vec::new(); k.push(1);
    trie.insert(k, 10);
    let k2: Vec<int> = Vec::new(); k2.push(1);
    trie.insert(k2, 20);   // overwrite

    let rk: Vec<int> = Vec::new(); rk.push(1);
    check_int("trie-overwrite: value updated",  trie.get(rk), 20);
    check_int("trie-overwrite: len still 1",    trie.len(), 1);
}

// ---------------------------------------------------------------------------
// 4. Empty key (root)
// ---------------------------------------------------------------------------

fn test_empty_key() {
    let mut trie: TernaryTrie<int> = TernaryTrie::new();

    let empty_key: Vec<int> = Vec::new();   // empty trit sequence = root
    trie.insert(empty_key, 777);

    let lookup: Vec<int> = Vec::new();
    check_int("trie-empty-key: get root=777", trie.get(lookup), 777);
    check_int("trie-empty-key: len=1",        trie.len(), 1);
}

// ---------------------------------------------------------------------------
// 5. keys_with_prefix
// ---------------------------------------------------------------------------

fn test_keys_with_prefix() {
    let mut trie: TernaryTrie<int> = TernaryTrie::new();

    // Insert several keys
    let k_pp:  Vec<int> = Vec::new(); k_pp.push(1);  k_pp.push(1);              // [+,+]
    let k_pz:  Vec<int> = Vec::new(); k_pz.push(1);  k_pz.push(0);              // [+,0]
    let k_pn:  Vec<int> = Vec::new(); k_pn.push(1);  k_pn.push(-1);             // [+,-]
    let k_nn:  Vec<int> = Vec::new(); k_nn.push(-1); k_nn.push(-1);             // [-,-]

    trie.insert(k_pp, 11);
    trie.insert(k_pz, 10);
    trie.insert(k_pn, 1);
    trie.insert(k_nn, -11);

    // All keys with prefix [+] should return 3 results
    let prefix_p: Vec<int> = Vec::new(); prefix_p.push(1);
    let matches = trie.keys_with_prefix(prefix_p);
    check_int("trie-prefix: [+] matches 3", matches.len(), 3);

    // All keys with prefix [-] should return 1 result
    let prefix_n: Vec<int> = Vec::new(); prefix_n.push(-1);
    let matches2 = trie.keys_with_prefix(prefix_n);
    check_int("trie-prefix: [-] matches 1", matches2.len(), 1);

    // Empty prefix = all keys
    let empty_prefix: Vec<int> = Vec::new();
    let all = trie.keys_with_prefix(empty_prefix);
    check_int("trie-prefix: empty=all 4", all.len(), 4);
}

// ---------------------------------------------------------------------------
// 6. Integer-keyed trie (using balanced ternary decomposition)
// ---------------------------------------------------------------------------

fn test_int_keyed_trie() {
    let mut trie: TernaryTrie<str> = TernaryTrie::new();

    // Map integers to their English names via their ternary keys
    let k1 = int_to_trit_key(1);    // "+" → [1]
    let k2 = int_to_trit_key(2);    // "+-" → [1, -1]
    let k4 = int_to_trit_key(4);    // "+0-" → [1, 0, -1]  (no: 4 = 0t+0- = 9-1=8? no 4=0t+-=3-1=2? let me recalc
                                     // 4 in bal.tern: 4 = 9/2=ok just use values without caring about exact digits)
    let k8 = int_to_trit_key(8);    // 8 = 9-1 = 0t+0-

    trie.insert(k1, "one");
    trie.insert(k2, "two");
    trie.insert(k4, "four");
    trie.insert(k8, "eight");

    check_int("trie-int: len=4", trie.len(), 4);

    // Retrieve by re-computing the key
    let rk1 = int_to_trit_key(1);
    let rk8 = int_to_trit_key(8);
    let v1 = trie.get(rk1);
    let v8 = trie.get(rk8);

    check("trie-int: get(1)=one",   v1 == "one");
    check("trie-int: get(8)=eight", v8 == "eight");
}

// ---------------------------------------------------------------------------
// 7. Deep keys (longer trit sequences)
// ---------------------------------------------------------------------------

fn test_deep_keys() {
    let mut trie: TernaryTrie<int> = TernaryTrie::new();

    // 6-trit key [+,+,+,0,0,-]
    let deep: Vec<int> = Vec::new();
    deep.push(1); deep.push(1); deep.push(1);
    deep.push(0); deep.push(0); deep.push(-1);
    trie.insert(deep, 12345);

    let r: Vec<int> = Vec::new();
    r.push(1); r.push(1); r.push(1);
    r.push(0); r.push(0); r.push(-1);
    check_int("trie-deep: 6-trit key", trie.get(r), 12345);

    // A shorter prefix of the deep key is a different key (not found)
    let short: Vec<int> = Vec::new();
    short.push(1); short.push(1); short.push(1);
    check("trie-deep: shorter prefix not found", !trie.contains_key(short));
}

// ---------------------------------------------------------------------------
// 8. Multiple values, frequency counting
// ---------------------------------------------------------------------------

fn test_frequency_count() {
    let mut freq: TernaryTrie<int> = TernaryTrie::new();

    // Count occurrences of balanced ternary numbers
    let nums: Vec<int> = Vec::new();
    nums.push(1); nums.push(2); nums.push(1); nums.push(3);
    nums.push(2); nums.push(1); nums.push(0); nums.push(0);

    for n in nums {
        let key = int_to_trit_key(n);
        let key2 = int_to_trit_key(n);
        let cur = if freq.contains_key(key) { freq.get(key2) } else { 0 };
        let key3 = int_to_trit_key(n);
        freq.insert(key3, cur + 1);
    }

    let k0 = int_to_trit_key(0);
    let k1 = int_to_trit_key(1);
    let k2 = int_to_trit_key(2);
    let k3 = int_to_trit_key(3);

    check_int("trie-freq: 0 appears 2", freq.get(k0), 2);
    check_int("trie-freq: 1 appears 3", freq.get(k1), 3);
    check_int("trie-freq: 2 appears 2", freq.get(k2), 2);
    check_int("trie-freq: 3 appears 1", freq.get(k3), 1);
}

// ---------------------------------------------------------------------------
// 9. keys() returns all keys
// ---------------------------------------------------------------------------

fn test_keys_all() {
    let mut trie: TernaryTrie<int> = TernaryTrie::new();

    let k1: Vec<int> = Vec::new(); k1.push(1);
    let k2: Vec<int> = Vec::new(); k2.push(-1);
    let k3: Vec<int> = Vec::new(); k3.push(0);

    trie.insert(k1, 1);
    trie.insert(k2, 2);
    trie.insert(k3, 3);

    let all_keys = trie.keys();
    check_int("trie-keys: 3 keys returned", all_keys.len(), 3);
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() {
    io::println("=== 10 TernaryTrie ===");

    io::println("-- basic insert/get --");
    test_basic_insert_get();

    io::println("-- contains_key --");
    test_contains_key();

    io::println("-- overwrite --");
    test_overwrite();

    io::println("-- empty key (root) --");
    test_empty_key();

    io::println("-- keys_with_prefix --");
    test_keys_with_prefix();

    io::println("-- int-keyed trie --");
    test_int_keyed_trie();

    io::println("-- deep keys --");
    test_deep_keys();

    io::println("-- frequency count --");
    test_frequency_count();

    io::println("-- keys() all --");
    test_keys_all();

    io::println("Done.");
}
