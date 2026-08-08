// Test: Vec, Map, Set, Deque — constructors, mutations, queries
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
// Vec
// ---------------------------------------------------------------------------

fn test_vec_push_pop() {
    let mut v: Vec<int> = Vec::new();
    check_int("vec: initial len=0", v.len(), 0);

    v.push(10);
    v.push(20);
    v.push(30);
    check_int("vec: after 3 pushes len=3", v.len(), 3);

    check_int("vec: get[0]=10", v.get(0), 10);
    check_int("vec: get[1]=20", v.get(1), 20);
    check_int("vec: get[2]=30", v.get(2), 30);

    let top = v.pop();
    check_int("vec: pop=30", top, 30);
    check_int("vec: after pop len=2", v.len(), 2);
}

fn test_vec_set() {
    let mut v: Vec<int> = Vec::new();
    v.push(1);
    v.push(2);
    v.push(3);
    v.set(1, 99);
    check_int("vec-set: v[1]=99", v.get(1), 99);
    check_int("vec-set: v[0]=1",  v.get(0), 1);
    check_int("vec-set: v[2]=3",  v.get(2), 3);
}

fn test_vec_contains() {
    let mut v: Vec<int> = Vec::new();
    v.push(5);
    v.push(10);
    v.push(15);
    check("vec-contains: 10", v.contains(10));
    check("vec-contains: !99", !v.contains(99));
    check("vec-contains: 5",   v.contains(5));
    check("vec-contains: 15",  v.contains(15));
}

fn test_vec_sort() {
    let mut v: Vec<int> = Vec::new();
    v.push(3);
    v.push(1);
    v.push(4);
    v.push(1);
    v.push(5);
    v.sort();
    check_int("vec-sort: [0]=1", v.get(0), 1);
    check_int("vec-sort: [1]=1", v.get(1), 1);
    check_int("vec-sort: [2]=3", v.get(2), 3);
    check_int("vec-sort: [3]=4", v.get(3), 4);
    check_int("vec-sort: [4]=5", v.get(4), 5);
}

fn test_vec_map() {
    let mut v: Vec<int> = Vec::new();
    v.push(1);
    v.push(2);
    v.push(3);
    let doubled = v.map(fn(x: int) => x * 2);
    check_int("vec-map: len=3",  doubled.len(), 3);
    check_int("vec-map: [0]=2",  doubled.get(0), 2);
    check_int("vec-map: [1]=4",  doubled.get(1), 4);
    check_int("vec-map: [2]=6",  doubled.get(2), 6);
}

fn test_vec_filter() {
    let mut v: Vec<int> = Vec::new();
    v.push(1); v.push(2); v.push(3); v.push(4); v.push(5);
    let evens = v.filter(fn(x: int) => x % 2 == 0);
    check_int("vec-filter: len=2",  evens.len(), 2);
    check_int("vec-filter: [0]=2",  evens.get(0), 2);
    check_int("vec-filter: [1]=4",  evens.get(1), 4);
}

fn test_vec_fold() {
    let mut v: Vec<int> = Vec::new();
    v.push(1); v.push(2); v.push(3); v.push(4); v.push(5);
    let sum = v.fold(0, fn(acc: int, x: int) => acc + x);
    check_int("vec-fold: sum=15", sum, 15);
    let product = v.fold(1, fn(acc: int, x: int) => acc * x);
    check_int("vec-fold: product=120", product, 120);
}

fn test_vec_reverse() {
    let mut v: Vec<int> = Vec::new();
    v.push(1); v.push(2); v.push(3);
    v.reverse();
    check_int("vec-rev: [0]=3", v.get(0), 3);
    check_int("vec-rev: [1]=2", v.get(1), 2);
    check_int("vec-rev: [2]=1", v.get(2), 1);
}

fn test_vec_remove() {
    let mut v: Vec<int> = Vec::new();
    v.push(10); v.push(20); v.push(30);
    v.remove(1);
    check_int("vec-remove: len=2",  v.len(), 2);
    check_int("vec-remove: [0]=10", v.get(0), 10);
    check_int("vec-remove: [1]=30", v.get(1), 30);
}

fn test_vec_for_loop() {
    let mut v: Vec<int> = Vec::new();
    v.push(2); v.push(4); v.push(6);
    let mut sum: int = 0;
    for x in v {
        sum = sum + x;
    }
    check_int("vec-for: sum=12", sum, 12);
}

// ---------------------------------------------------------------------------
// Map
// ---------------------------------------------------------------------------

fn test_map_insert_get() {
    let mut m: Map<str, int> = Map::new();
    check_int("map: initial len=0", m.len(), 0);
    m.insert("a", 1);
    m.insert("b", 2);
    m.insert("c", 3);
    check_int("map: len=3",    m.len(), 3);
    check_int("map: get a=1",  m.get("a"), 1);
    check_int("map: get b=2",  m.get("b"), 2);
    check_int("map: get c=3",  m.get("c"), 3);
}

fn test_map_overwrite() {
    let mut m: Map<str, int> = Map::new();
    m.insert("k", 10);
    m.insert("k", 99);
    check_int("map-overwrite: k=99", m.get("k"), 99);
    check_int("map-overwrite: len=1", m.len(), 1);
}

fn test_map_contains_key() {
    let mut m: Map<str, int> = Map::new();
    m.insert("x", 1);
    check("map-ck: x", m.contains_key("x"));
    check("map-ck: !y", !m.contains_key("y"));
}

fn test_map_get_or() {
    let mut m: Map<str, int> = Map::new();
    m.insert("a", 42);
    check_int("map-get-or: existing=42",    m.get_or("a", 0), 42);
    check_int("map-get-or: missing=0",      m.get_or("z", 0), 0);
    check_int("map-get-or: missing=-1",     m.get_or("z", -1), -1);
}

fn test_map_remove() {
    let mut m: Map<str, int> = Map::new();
    m.insert("a", 1);
    m.insert("b", 2);
    m.remove("a");
    check_int("map-remove: len=1",  m.len(), 1);
    check("map-remove: !a", !m.contains_key("a"));
    check("map-remove: b",   m.contains_key("b"));
}

fn test_map_frequency_count() {
    // Word frequency count pattern
    let mut freq: Map<str, int> = Map::new();
    let words: Vec<str> = Vec::new();
    // We simulate by calling insert manually
    let w1 = "apple"; let w2 = "banana"; let w3 = "apple"; let w4 = "cherry";

    let c1 = if freq.contains_key(w1) { freq.get(w1) } else { 0 };
    freq.insert(w1, c1 + 1);
    let c2 = if freq.contains_key(w2) { freq.get(w2) } else { 0 };
    freq.insert(w2, c2 + 1);
    let c3 = if freq.contains_key(w3) { freq.get(w3) } else { 0 };
    freq.insert(w3, c3 + 1);
    let c4 = if freq.contains_key(w4) { freq.get(w4) } else { 0 };
    freq.insert(w4, c4 + 1);

    check_int("map-freq: apple=2",  freq.get("apple"),  2);
    check_int("map-freq: banana=1", freq.get("banana"), 1);
    check_int("map-freq: cherry=1", freq.get("cherry"), 1);
}

// ---------------------------------------------------------------------------
// Set
// ---------------------------------------------------------------------------

fn test_set_insert_contains() {
    let mut s: Set<int> = Set::new();
    check_int("set: initial len=0", s.len(), 0);
    s.insert(1);
    s.insert(2);
    s.insert(3);
    s.insert(2);   // duplicate
    check_int("set: len=3",      s.len(), 3);
    check("set: contains 1",     s.contains(1));
    check("set: contains 2",     s.contains(2));
    check("set: !contains 9",    !s.contains(9));
}

fn test_set_remove() {
    let mut s: Set<int> = Set::new();
    s.insert(10);
    s.insert(20);
    s.remove(10);
    check_int("set-remove: len=1",  s.len(), 1);
    check("set-remove: !10", !s.contains(10));
    check("set-remove: 20",   s.contains(20));
}

fn test_set_intersection() {
    let mut a: Set<int> = Set::new();
    a.insert(1); a.insert(2); a.insert(3);
    let mut b: Set<int> = Set::new();
    b.insert(2); b.insert(3); b.insert(4);
    let inter = a.intersection(b);
    check_int("set-inter: len=2",  inter.len(), 2);
    check("set-inter: 2",  inter.contains(2));
    check("set-inter: 3",  inter.contains(3));
    check("set-inter: !1", !inter.contains(1));
    check("set-inter: !4", !inter.contains(4));
}

fn test_set_union() {
    let mut a: Set<int> = Set::new();
    a.insert(1); a.insert(2);
    let mut b: Set<int> = Set::new();
    b.insert(2); b.insert(3);
    let u = a.union(b);
    check_int("set-union: len=3", u.len(), 3);
    check("set-union: 1", u.contains(1));
    check("set-union: 2", u.contains(2));
    check("set-union: 3", u.contains(3));
}

fn test_set_difference() {
    let mut a: Set<int> = Set::new();
    a.insert(1); a.insert(2); a.insert(3);
    let mut b: Set<int> = Set::new();
    b.insert(2);
    let diff = a.difference(b);
    check_int("set-diff: len=2", diff.len(), 2);
    check("set-diff: 1",  diff.contains(1));
    check("set-diff: 3",  diff.contains(3));
    check("set-diff: !2", !diff.contains(2));
}

fn test_set_subset() {
    let mut small: Set<int> = Set::new();
    small.insert(1); small.insert(2);
    let mut big: Set<int> = Set::new();
    big.insert(1); big.insert(2); big.insert(3);
    check("set-subset: small⊆big",  small.is_subset(big));
    check("set-subset: !big⊆small", !big.is_subset(small));
}

// ---------------------------------------------------------------------------
// Deque
// ---------------------------------------------------------------------------

fn test_deque_stack() {
    let mut d: Deque<int> = Deque::new();
    d.push_back(1);
    d.push_back(2);
    d.push_back(3);
    check_int("deque-stack: pop=3", d.pop_back(), 3);
    check_int("deque-stack: pop=2", d.pop_back(), 2);
    check_int("deque-stack: pop=1", d.pop_back(), 1);
    check_int("deque-stack: len=0", d.len(), 0);
}

fn test_deque_queue() {
    let mut d: Deque<int> = Deque::new();
    d.push_back(10);
    d.push_back(20);
    d.push_back(30);
    check_int("deque-queue: front=10", d.pop_front(), 10);
    check_int("deque-queue: front=20", d.pop_front(), 20);
    check_int("deque-queue: front=30", d.pop_front(), 30);
}

fn test_deque_front_back() {
    let mut d: Deque<int> = Deque::new();
    d.push_front(5);
    d.push_front(3);
    d.push_back(7);
    // order: 3, 5, 7
    check_int("deque-mixed: front=3", d.front(), 3);
    check_int("deque-mixed: back=7",  d.back(), 7);
    check_int("deque-mixed: len=3",   d.len(), 3);
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() {
    io::println("=== 06 Collections ===");

    io::println("-- Vec --");
    test_vec_push_pop();
    test_vec_set();
    test_vec_contains();
    test_vec_sort();
    test_vec_map();
    test_vec_filter();
    test_vec_fold();
    test_vec_reverse();
    test_vec_remove();
    test_vec_for_loop();

    io::println("-- Map --");
    test_map_insert_get();
    test_map_overwrite();
    test_map_contains_key();
    test_map_get_or();
    test_map_remove();
    test_map_frequency_count();

    io::println("-- Set --");
    test_set_insert_contains();
    test_set_remove();
    test_set_intersection();
    test_set_union();
    test_set_difference();
    test_set_subset();

    io::println("-- Deque --");
    test_deque_stack();
    test_deque_queue();
    test_deque_front_back();

    io::println("Done.");
}
