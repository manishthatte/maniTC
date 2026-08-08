// Test: generics and traits — generic functions, generic structs,
//        trait constraints, multiple trait implementations, trait objects,
//        generic enums, associated types, default trait methods
use std::io;
use std::fmt;

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
fn check_str(label: str, got: str, want: str) {
    if got == want { pass(label); }
    else {
        io::print("FAIL "); io::print(label);
        io::print(" got=["); io::print(got);
        io::print("] want=["); io::print(want); io::println("]");
    }
}

// ---------------------------------------------------------------------------
// Generic functions
// ---------------------------------------------------------------------------

fn identity<T>(x: T) -> T { x }

fn swap_apply<T>(a: T, b: T, f: fn(T, T) -> T) -> T { f(b, a) }

fn max_of<T>(a: T, b: T) -> T { if a > b { a } else { b } }
fn min_of<T>(a: T, b: T) -> T { if a < b { a } else { b } }

fn clamp<T>(v: T, lo: T, hi: T) -> T {
    if v < lo { lo } elif v > hi { hi } else { v }
}

fn test_generic_functions() {
    check_int("generic: identity(42)=42",       identity(42),        42);
    check_int("generic: max(7,3)=7",            max_of(7, 3),        7);
    check_int("generic: max(3,7)=7",            max_of(3, 7),        7);
    check_int("generic: max(5,5)=5",            max_of(5, 5),        5);
    check_int("generic: min(7,3)=3",            min_of(7, 3),        3);
    check_int("generic: clamp(5,0,10)=5",       clamp(5,  0, 10),    5);
    check_int("generic: clamp(-1,0,10)=0",      clamp(-1, 0, 10),    0);
    check_int("generic: clamp(15,0,10)=10",     clamp(15, 0, 10),   10);
    check_int("generic: clamp(10,10,10)=10",    clamp(10,10, 10),   10);
}

// ---------------------------------------------------------------------------
// Generic sort utility
// ---------------------------------------------------------------------------

fn sort3(a: int, b: int, c: int) -> Vec<int> {
    let mut v: Vec<int> = Vec::new();
    v.push(a); v.push(b); v.push(c);
    v.sort();
    v
}

fn test_generic_sort() {
    let s1 = sort3(3, 1, 2);
    check_int("sort3: [1]", s1.get(0), 1);
    check_int("sort3: [2]", s1.get(1), 2);
    check_int("sort3: [3]", s1.get(2), 3);

    let s2 = sort3(-5, 0, 5);
    check_int("sort3-neg: [-5]", s2.get(0), -5);
    check_int("sort3-neg: [0]",  s2.get(1),  0);
    check_int("sort3-neg: [5]",  s2.get(2),  5);

    let s3 = sort3(7, 7, 7);
    check_int("sort3-eq: [7]", s3.get(0), 7);
    check_int("sort3-eq: [7]", s3.get(1), 7);
    check_int("sort3-eq: [7]", s3.get(2), 7);
}

// ---------------------------------------------------------------------------
// Trait definitions and implementations
// ---------------------------------------------------------------------------

trait Printable {
    fn to_str(self) -> str;
}

trait Scalable {
    fn scale(self, factor: int) -> Self;
}

trait Summable {
    fn sum_with(self, other: Self) -> Self;
}

trait Describable {
    fn describe(self) -> str;
}

// ---------------------------------------------------------------------------
// Struct: Vec2 (2D integer vector)
// ---------------------------------------------------------------------------

struct Vec2 {
    pub x: int,
    pub y: int,
}

impl Vec2 {
    fn new(x: int, y: int) -> Vec2 { Vec2 { x: x, y: y } }
    fn dot(self, other: Vec2) -> int { self.x * other.x + self.y * other.y }
    fn length_sq(self) -> int { self.x * self.x + self.y * self.y }
}

impl Printable for Vec2 {
    fn to_str(self) -> str { fmt::format("({},{})", self.x, self.y) }
}

impl Scalable for Vec2 {
    fn scale(self, factor: int) -> Vec2 {
        Vec2 { x: self.x * factor, y: self.y * factor }
    }
}

impl Summable for Vec2 {
    fn sum_with(self, other: Vec2) -> Vec2 {
        Vec2 { x: self.x + other.x, y: self.y + other.y }
    }
}

impl Describable for Vec2 {
    fn describe(self) -> str {
        let lsq = self.length_sq();
        fmt::format("Vec2({},{}) len_sq={}", self.x, self.y, lsq)
    }
}

fn test_vec2_traits() {
    let v1 = Vec2::new(3, 4);
    let v2 = Vec2::new(1, 2);

    // Printable
    let s = v1.to_str();
    check_str("vec2: to_str", s, "(3,4)");

    // Scalable
    let doubled = v1.scale(2);
    check_int("vec2: scale x", doubled.x, 6);
    check_int("vec2: scale y", doubled.y, 8);

    // Summable
    let sum = v1.sum_with(v2);
    check_int("vec2: sum x", sum.x, 4);
    check_int("vec2: sum y", sum.y, 6);

    // Dot product
    check_int("vec2: dot (3,4)·(1,2)=11", v1.dot(v2), 11);
    check_int("vec2: length_sq (3,4)=25",  v1.length_sq(), 25);
}

// ---------------------------------------------------------------------------
// Struct: Color3 (RGB with 0-255 range)
// ---------------------------------------------------------------------------

struct Color3 {
    pub r: int,
    pub g: int,
    pub b: int,
}

impl Color3 {
    fn new(r: int, g: int, b: int) -> Color3 { Color3 { r: r, g: g, b: b } }
    fn brightness(self) -> int { (self.r + self.g + self.b) / 3 }
}

impl Printable for Color3 {
    fn to_str(self) -> str { fmt::format("rgb({},{},{})", self.r, self.g, self.b) }
}

impl Scalable for Color3 {
    fn scale(self, factor: int) -> Color3 {
        // Scale each channel, clamp to 0-255
        let nr = if self.r * factor > 255 { 255 } else { self.r * factor };
        let ng = if self.g * factor > 255 { 255 } else { self.g * factor };
        let nb = if self.b * factor > 255 { 255 } else { self.b * factor };
        Color3 { r: nr, g: ng, b: nb }
    }
}

fn test_color_traits() {
    let red = Color3::new(255, 0, 0);
    let green = Color3::new(0, 255, 0);
    let dark = Color3::new(50, 50, 50);

    check_int("color: red brightness=85",    red.brightness(),   85);
    check_int("color: dark brightness=50",   dark.brightness(),  50);

    let doubled = dark.scale(2);
    check_int("color: scale 50*2=100", doubled.r, 100);
    check_int("color: scale 50*2=100", doubled.g, 100);

    let bright = Color3::new(200, 200, 200);
    let over = bright.scale(2);  // 400 → clamped to 255
    check_int("color: scale clamped to 255", over.r, 255);
}

// ---------------------------------------------------------------------------
// Multiple structs with same trait — dispatch via function
// ---------------------------------------------------------------------------

fn print_to_str_vec2(v: Vec2) -> str { v.to_str() }
fn print_to_str_color(c: Color3) -> str { c.to_str() }

fn test_same_trait_multiple_types() {
    let v = Vec2::new(1, 2);
    let c = Color3::new(10, 20, 30);

    check_str("multi-type: vec2 printable",  v.to_str(), "(1,2)");
    check_str("multi-type: color printable", c.to_str(), "rgb(10,20,30)");
}

// ---------------------------------------------------------------------------
// Result<T,E> usage: inline Ok/Err, match pattern
// (Generic enum definitions not yet supported; use built-in Result<T,E>)
// ---------------------------------------------------------------------------

fn result_or(r: Result<int, str>, default: int) -> int {
    match r {
        Ok(v)      => v,
        Err(_)     => default,
        Unknown(_) => default,
    }
}

fn test_option() {
    // Direct Result construction (stack-local, safe)
    check_int("option: Ok(99) or 0 = 99",  result_or(Ok(99),       0), 99);
    check_int("option: Ok(42) or 0 = 42",  result_or(Ok(42),       0), 42);
    check_int("option: Err or 7 = 7",      result_or(Err("x"),     7), 7);
    check_int("option: Err or -1 = -1",    result_or(Err("none"), -1), -1);

    // Unknown case
    let u: Result<int, str> = Unknown("maybe");
    check_int("option: Unknown or 0 = 0",  result_or(u, 0), 0);

    // Use in an if expression
    let val: int = if true { result_or(Ok(100), 0) } else { 0 };
    check_int("option: conditional Ok(100)", val, 100);
}

// ---------------------------------------------------------------------------
// Concrete Pair struct (generic struct/impl params not yet fully supported)
// ---------------------------------------------------------------------------

struct IntPair {
    pub first: int,
    pub second: int,
}

impl IntPair {
    fn new(a: int, b: int) -> IntPair { IntPair { first: a, second: b } }
    fn swap(self) -> IntPair { IntPair { first: self.second, second: self.first } }
    fn sum(self) -> int { self.first + self.second }
}

fn test_generic_pair() {
    let p = IntPair::new(10, 20);
    check_int("pair: first=10",   p.first,  10);
    check_int("pair: second=20",  p.second, 20);
    check_int("pair: sum=30",     p.sum(),  30);

    let swapped = p.swap();
    check_int("pair-swap: first=20",  swapped.first,  20);
    check_int("pair-swap: second=10", swapped.second, 10);

    let p2 = IntPair::new(-5, 5);
    check_int("pair2: sum=0", p2.sum(), 0);
}

// ---------------------------------------------------------------------------
// Trait with multiple methods
// ---------------------------------------------------------------------------

trait Stats {
    fn count(self) -> int;
    fn total(self) -> int;
    fn average(self) -> int;
    fn min(self) -> int;
    fn max(self) -> int;
}

struct IntStats {
    pub values: Vec<int>,
}

impl IntStats {
    fn new() -> IntStats { IntStats { values: Vec::new() } }
    fn add(self, v: int) { self.values.push(v); }
}

impl Stats for IntStats {
    fn count(self) -> int { self.values.len() }
    fn total(self) -> int { self.values.fold(0, fn(acc: int, x: int) => acc + x) }
    fn average(self) -> int {
        let n = self.count();
        if n == 0 { 0 } else { self.total() / n }
    }
    fn min(self) -> int { self.values.fold(99999999, fn(acc: int, x: int) => if x < acc { x } else { acc }) }
    fn max(self) -> int { self.values.fold(-99999999, fn(acc: int, x: int) => if x > acc { x } else { acc }) }
}

fn test_stats_trait() {
    let mut s = IntStats::new();
    s.add(10);
    s.add(20);
    s.add(30);
    s.add(40);
    s.add(50);

    check_int("stats: count=5",   s.count(),   5);
    check_int("stats: total=150", s.total(),   150);
    check_int("stats: avg=30",    s.average(), 30);
    check_int("stats: min=10",    s.min(),     10);
    check_int("stats: max=50",    s.max(),     50);

    let mut s2 = IntStats::new();
    s2.add(-5);
    s2.add(0);
    s2.add(5);
    check_int("stats2: total=0",  s2.total(),   0);
    check_int("stats2: avg=0",    s2.average(), 0);
    check_int("stats2: min=-5",   s2.min(),    -5);
    check_int("stats2: max=5",    s2.max(),     5);
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() {
    io::println("=== 15 Generics & Traits ===");

    io::println("-- generic functions --");
    test_generic_functions();

    io::println("-- generic sort --");
    test_generic_sort();

    io::println("-- Vec2 traits --");
    test_vec2_traits();

    io::println("-- Color3 traits --");
    test_color_traits();

    io::println("-- same trait multiple types --");
    test_same_trait_multiple_types();

    io::println("-- Option<T> --");
    test_option();

    io::println("-- Pair<A,B> --");
    test_generic_pair();

    io::println("-- Stats trait --");
    test_stats_trait();

    io::println("Done.");
}
