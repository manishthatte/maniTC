// Test: string operations — equality, length, concat, contains, find,
//        split, trim, replace, int↔str conversion, format
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
// String equality and inequality
// ---------------------------------------------------------------------------

fn test_str_equality() {
    let a: str = "hello";
    let b: str = "hello";
    let c: str = "world";

    check("str-eq: hello==hello",  a == b);
    check("str-eq: hello!=world",  a != c);
    check("str-eq: empty==empty",  "" == "");
    check("str-eq: empty!=space",  " " != "");
}

// ---------------------------------------------------------------------------
// String length
// ---------------------------------------------------------------------------

fn test_str_len() {
    check_int("str-len: empty=0",      "".len(), 0);
    check_int("str-len: a=1",          "a".len(), 1);
    check_int("str-len: hello=5",      "hello".len(), 5);
    check_int("str-len: two words=9",  "two words".len(), 9);
}

// ---------------------------------------------------------------------------
// String concatenation
// ---------------------------------------------------------------------------

fn test_str_concat() {
    let a: str = "foo";
    let b: str = "bar";
    let c = a.concat(b);
    check_str("str-concat: foo+bar=foobar",  c, "foobar");

    let empty1 = "".concat("abc");
    check_str("str-concat: empty+abc",       empty1, "abc");

    let empty2 = "abc".concat("");
    check_str("str-concat: abc+empty",       empty2, "abc");

    let both_empty = "".concat("");
    check_str("str-concat: empty+empty",     both_empty, "");
}

// ---------------------------------------------------------------------------
// String contains
// ---------------------------------------------------------------------------

fn test_str_contains() {
    let s: str = "hello world";

    check("str-contains: 'hello'",   s.contains("hello"));
    check("str-contains: 'world'",   s.contains("world"));
    check("str-contains: ' '",       s.contains(" "));
    check("str-contains: !xyz",      !s.contains("xyz"));
    check("str-contains: empty",     s.contains(""));
    check("str-contains: self",      s.contains("hello world"));
}

// ---------------------------------------------------------------------------
// String find (index)
// ---------------------------------------------------------------------------

fn test_str_find() {
    let s: str = "abcabc";

    // 'a' first occurrence at 0
    check_int("str-find: 'a' at 0",       s.find("a"), 0);
    // 'b' at 1
    check_int("str-find: 'b' at 1",       s.find("b"), 1);
    // 'abc' at 0
    check_int("str-find: 'abc' at 0",     s.find("abc"), 0);
    // 'z' not found → -1
    check_int("str-find: 'z' not found",  s.find("z"), -1);
    // empty string found at 0
    check_int("str-find: '' at 0",        s.find(""), 0);
}

// ---------------------------------------------------------------------------
// String trim
// ---------------------------------------------------------------------------

fn test_str_trim() {
    let s1: str = "  hello  ";
    let t1 = s1.trim();
    check_str("str-trim: spaces",   t1, "hello");

    let s2: str = "nospaces";
    let t2 = s2.trim();
    check_str("str-trim: noop",     t2, "nospaces");

    let s3: str = "   ";
    let t3 = s3.trim();
    check_str("str-trim: all-spaces→empty", t3, "");
}

// ---------------------------------------------------------------------------
// String replace
// ---------------------------------------------------------------------------

fn test_str_replace() {
    let s: str = "aabbcc";

    let r1 = s.replace("bb", "XX");
    check_str("str-replace: bb→XX", r1, "aaXXcc");

    let r2 = s.replace("zz", "XX");
    check_str("str-replace: no-match stays same", r2, "aabbcc");

    let r3 = s.replace("a", "");
    check_str("str-replace: delete a", r3, "bbcc");
}

// ---------------------------------------------------------------------------
// String slice
// ---------------------------------------------------------------------------

fn test_str_slice() {
    let s: str = "hello";

    let sub1 = s.slice(0, 3);
    check_str("str-slice: 0..3 = 'hel'",  sub1, "hel");

    let sub2 = s.slice(1, 4);
    check_str("str-slice: 1..4 = 'ell'",  sub2, "ell");

    let sub3 = s.slice(0, 5);
    check_str("str-slice: whole",          sub3, "hello");

    let sub4 = s.slice(3, 3);
    check_str("str-slice: empty range",    sub4, "");
}

// ---------------------------------------------------------------------------
// Int ↔ str conversion
// ---------------------------------------------------------------------------

fn test_int_str_conversion() {
    let n: int = 42;
    let s = n.to_str();
    check_str("int→str: 42", s, "42");

    let neg_s = (-7).to_str();
    check_str("int→str: -7", neg_s, "-7");

    let zero_s = 0.to_str();
    check_str("int→str: 0", zero_s, "0");

    // str → int
    let back = "42".to_int();
    check_int("str→int: '42'=42", back, 42);

    let neg_back = "-7".to_int();
    check_int("str→int: '-7'=-7", neg_back, -7);

    let zero_back = "0".to_int();
    check_int("str→int: '0'=0", zero_back, 0);
}

// ---------------------------------------------------------------------------
// fmt::format string interpolation
// ---------------------------------------------------------------------------

fn test_fmt_format() {
    // Single int argument
    let s1 = fmt::format("x={}", 42);
    check_str("fmt: x={}", s1, "x=42");

    // Two arguments
    let s2 = fmt::format("{} + {} = {}", 3, 4, 7);
    check_str("fmt: 3+4=7", s2, "3 + 4 = 7");

    // No arguments (just a literal string)
    let s3 = fmt::format("hello world", 0);
    // format with no {} placeholders just returns the template
    check_str("fmt: no-args", s3, "hello world");
}

// ---------------------------------------------------------------------------
// String in match / conditional
// ---------------------------------------------------------------------------

fn describe_greeting(s: str) -> str {
    if s == "hello" { "friendly greeting" }
    elif s == "bye" { "farewell" }
    else { "unknown" }
}

fn test_str_in_cond() {
    check_str("str-cond: hello",  describe_greeting("hello"), "friendly greeting");
    check_str("str-cond: bye",    describe_greeting("bye"),   "farewell");
    check_str("str-cond: other",  describe_greeting("hi"),    "unknown");
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() {
    io::println("=== 11 String Operations ===");

    io::println("-- equality --");
    test_str_equality();

    io::println("-- length --");
    test_str_len();

    io::println("-- concat --");
    test_str_concat();

    io::println("-- contains --");
    test_str_contains();

    io::println("-- find --");
    test_str_find();

    io::println("-- trim --");
    test_str_trim();

    io::println("-- replace --");
    test_str_replace();

    io::println("-- slice --");
    test_str_slice();

    io::println("-- int↔str --");
    test_int_str_conversion();

    io::println("-- fmt::format --");
    test_fmt_format();

    io::println("-- str in conditional --");
    test_str_in_cond();

    io::println("Done.");
}
