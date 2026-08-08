// Test: pattern matching — every pattern kind, guards, nested, wildcard,
//        or-patterns, struct patterns, enum with and without payload
use std::io;

fn pass(label: str) { io::print("PASS "); io::println(label); }
fn fail(label: str) { io::print("FAIL "); io::println(label); }
fn check(label: str, cond: bool) { if cond { pass(label) } else { fail(label) } }
fn check_int(label: str, got: int, want: int) {
    if got == want { pass(label) }
    else {
        io::print("FAIL "); io::print(label);
        io::print(" got="); io::print_int(got);
        io::print(" want="); io::print_int(want);
        io::newline();
    }
}
fn check_str(label: str, got: str, want: str) {
    if got == want { pass(label) }
    else {
        io::print("FAIL "); io::print(label);
        io::print(" got=["); io::print(got);
        io::print("] want=["); io::print(want); io::println("]");
    }
}

// ---------------------------------------------------------------------------
// Wildcard pattern
// ---------------------------------------------------------------------------

fn test_wildcard() {
    let x: int = 99;
    let r = match x {
        0 => "zero",
        _ => "other",
    };
    check_str("wildcard: 99→other", r, "other");

    let y: int = 0;
    let r2 = match y {
        0 => "zero",
        _ => "other",
    };
    check_str("wildcard: 0→zero", r2, "zero");
}

// ---------------------------------------------------------------------------
// Literal patterns — int, negative, bool
// ---------------------------------------------------------------------------

fn describe_num(n: int) -> str {
    match n {
        -1 => "minus one",
        0  => "zero",
        1  => "one",
        2  => "two",
        _  => "many",
    }
}

fn test_literal_patterns() {
    check_str("lit: -1", describe_num(-1), "minus one");
    check_str("lit:  0", describe_num(0),  "zero");
    check_str("lit:  1", describe_num(1),  "one");
    check_str("lit:  2", describe_num(2),  "two");
    check_str("lit: 99", describe_num(99), "many");
}

// ---------------------------------------------------------------------------
// Identifier (binding) patterns
// ---------------------------------------------------------------------------

fn double_if_pos(n: int) -> int {
    match n {
        v if v > 0 => v * 2,
        v          => v,
    }
}

fn test_binding() {
    check_int("binding: pos 5→10",   double_if_pos(5),  10);
    check_int("binding: neg -3→-3",  double_if_pos(-3), -3);
    check_int("binding: zero 0→0",   double_if_pos(0),  0);
}

// ---------------------------------------------------------------------------
// Guard patterns
// ---------------------------------------------------------------------------

fn classify_num(n: int) -> str {
    match n {
        v if v < -10 => "very negative",
        v if v < 0   => "negative",
        0             => "zero",
        v if v < 10  => "small positive",
        v if v < 100 => "medium positive",
        _             => "large positive",
    }
}

fn test_guards() {
    check_str("guard: -20", classify_num(-20), "very negative");
    check_str("guard:  -5", classify_num(-5),  "negative");
    check_str("guard:   0", classify_num(0),   "zero");
    check_str("guard:   5", classify_num(5),   "small positive");
    check_str("guard:  50", classify_num(50),  "medium positive");
    check_str("guard: 200", classify_num(200), "large positive");
}

// ---------------------------------------------------------------------------
// Or patterns
// ---------------------------------------------------------------------------

fn is_vowel_num(n: int) -> bool {
    match n {
        1 | 2 | 3 => true,
        _          => false,
    }
}

fn day_type(n: int) -> str {
    // 0=Mon..6=Sun
    match n {
        5 | 6 => "weekend",
        _     => "weekday",
    }
}

fn test_or_patterns() {
    check("or: 1",   is_vowel_num(1));
    check("or: 2",   is_vowel_num(2));
    check("or: 3",   is_vowel_num(3));
    check("or: !4",  !is_vowel_num(4));
    check_str("or: day 0=weekday", day_type(0), "weekday");
    check_str("or: day 5=weekend", day_type(5), "weekend");
    check_str("or: day 6=weekend", day_type(6), "weekend");
}

// ---------------------------------------------------------------------------
// Enum patterns
// ---------------------------------------------------------------------------

enum Season { Spring, Summer, Autumn, Winter }

fn season_temp(s: Season) -> str {
    match s {
        Season::Spring => "mild",
        Season::Summer => "hot",
        Season::Autumn => "cool",
        Season::Winter => "cold",
    }
}

fn is_warm(s: Season) -> bool {
    match s {
        Season::Summer | Season::Spring => true,
        _ => false,
    }
}

fn test_enum_patterns() {
    check_str("enum-pat: spring=mild",   season_temp(Season::Spring), "mild");
    check_str("enum-pat: summer=hot",    season_temp(Season::Summer), "hot");
    check_str("enum-pat: autumn=cool",   season_temp(Season::Autumn), "cool");
    check_str("enum-pat: winter=cold",   season_temp(Season::Winter), "cold");
    check("enum-or: summer is warm",     is_warm(Season::Summer));
    check("enum-or: spring is warm",     is_warm(Season::Spring));
    check("enum-or: winter is not warm", !is_warm(Season::Winter));
    check("enum-or: autumn is not warm", !is_warm(Season::Autumn));
}

// ---------------------------------------------------------------------------
// Nested match
// ---------------------------------------------------------------------------

fn nested_classify(a: int, b: int) -> str {
    match a {
        0 => match b {
            0 => "origin",
            _ => "on y-axis",
        },
        _ => match b {
            0 => "on x-axis",
            _ => "general",
        },
    }
}

fn test_nested_match() {
    check_str("nested: (0,0)=origin",   nested_classify(0, 0), "origin");
    check_str("nested: (0,1)=y-axis",   nested_classify(0, 1), "on y-axis");
    check_str("nested: (1,0)=x-axis",   nested_classify(1, 0), "on x-axis");
    check_str("nested: (1,1)=general",  nested_classify(1, 1), "general");
}

// ---------------------------------------------------------------------------
// Match as expression assigns to variable
// ---------------------------------------------------------------------------

fn test_match_expr_assign() {
    let x: int = 7;
    let category = match x {
        n if n < 0 => "neg",
        0          => "zero",
        n if n < 5 => "small",
        _          => "big",
    };
    check_str("match-assign: 7=big", category, "big");

    let y: int = 3;
    let cat2 = match y {
        n if n < 5 => "small",
        _          => "big",
    };
    check_str("match-assign: 3=small", cat2, "small");
}

// ---------------------------------------------------------------------------
// Result pattern matching (three-state)
// ---------------------------------------------------------------------------

fn extract_ok(r: Result<int, str>) -> int {
    match r {
        Ok(v)      => v,
        Unknown(_) => -999,
        Err(_)     => -998,
    }
}

fn test_result_patterns() {
    check_int("result-pat: Ok(7)=7",       extract_ok(Ok(7)),         7);
    check_int("result-pat: Unknown→-999",  extract_ok(Unknown("x")), -999);
    check_int("result-pat: Err→-998",      extract_ok(Err("e")),     -998);
}

// ---------------------------------------------------------------------------
// Match with struct pattern (struct fields in pattern)
// ---------------------------------------------------------------------------

struct Point { pub x: int, pub y: int }

fn quadrant(p: Point) -> str {
    match p {
        Point { x: 0, y: 0 } => "origin",
        Point { x: px, y: py } => {
            if px > 0 && py > 0 { "Q1" }
            elif px < 0 && py > 0 { "Q2" }
            elif px < 0 && py < 0 { "Q3" }
            else { "Q4" }
        },
    }
}

fn test_struct_pattern() {
    check_str("struct-pat: origin",   quadrant(Point { x: 0, y: 0 }), "origin");
    check_str("struct-pat: Q1",       quadrant(Point { x: 1, y: 1 }), "Q1");
    check_str("struct-pat: Q2",       quadrant(Point { x:-1, y: 1 }), "Q2");
    check_str("struct-pat: Q3",       quadrant(Point { x:-1, y:-1 }), "Q3");
    check_str("struct-pat: Q4",       quadrant(Point { x: 1, y:-1 }), "Q4");
}

// ---------------------------------------------------------------------------
// Trit and bool3 patterns in tif
// ---------------------------------------------------------------------------

fn encode_trit(t: trit) -> int {
    tif t { + => 1, 0 => 0, - => -1 }
}

fn bool3_label(b: bool3) -> str {
    tif b { + => "TRUE", 0 => "UNKNOWN", - => "FALSE" }
}

fn test_trit_patterns() {
    check_int("trit-pat: +=1",   encode_trit(+), 1);
    check_int("trit-pat: 0=0",   encode_trit(0), 0);
    check_int("trit-pat: -=-1",  encode_trit(-), -1);
    check_str("bool3-pat: true",    bool3_label(true),    "TRUE");
    check_str("bool3-pat: unknown", bool3_label(unknown), "UNKNOWN");
    check_str("bool3-pat: false",   bool3_label(false),   "FALSE");
}

// ---------------------------------------------------------------------------
// Sequential match arms — first matching wins
// ---------------------------------------------------------------------------

fn first_match(n: int) -> str {
    match n {
        1 => "one",
        1 => "one-again",   // unreachable but should not break
        _ => "other",
    }
}

fn test_first_match() {
    check_str("first-match: 1=one",   first_match(1), "one");
    check_str("first-match: 2=other", first_match(2), "other");
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() {
    io::println("=== 08 Pattern Matching ===");

    io::println("-- wildcard --");
    test_wildcard();

    io::println("-- literal --");
    test_literal_patterns();

    io::println("-- binding --");
    test_binding();

    io::println("-- guards --");
    test_guards();

    io::println("-- or-patterns --");
    test_or_patterns();

    io::println("-- enum --");
    test_enum_patterns();

    io::println("-- nested match --");
    test_nested_match();

    io::println("-- match as expr --");
    test_match_expr_assign();

    io::println("-- Result patterns --");
    test_result_patterns();

    io::println("-- struct pattern --");
    test_struct_pattern();

    io::println("-- trit/bool3 --");
    test_trit_patterns();

    io::println("-- first-match wins --");
    test_first_match();

    io::println("Done.");
}
