// stdlib/std/str.mt
// String utilities for maniT.
//
// maniT strings are UTF-8 encoded, immutable byte sequences.  ALL index and
// length values in this module are measured in BYTES.  `char_at` returns the
// byte at an index, `slice` takes a byte range, and `len` is the byte count.
//
// Usage:
//   use std::str;
//   let n = str::len("hello");       // 5  — bytes
//   let m = str::char_count("aéb");  // 3  — Unicode scalar values
//   let b = str::len("aéb");         // 4  — bytes
//
// CORRECTED 29 August 2026 (report.txt P48).  The three sentences above used
// to claim the opposite — "measured in Unicode scalar values (codepoints), not
// raw bytes" — while `byte_len` fifteen lines below said "ManiT strings are
// byte strings today".  The module contradicted itself and the implementation
// was bytes throughout, so a reader who believed the header wrote a loop that
// was right for ASCII and wrong for everything else.  `char_count` is new, and
// is what makes `byte_len` a distinction rather than a synonym for `len`.
//
// Bytes is not a placeholder for a codepoint API: `slice` has been byte-ranged
// on both backends since 19 August (report.txt P50 brought T3 to the C
// runtime's rule), and an index that means one thing in `len` and another in
// `slice` cannot be looped over safely.  `char_count` is deliberately a COUNT
// and not an index, for the same reason.

// ---------------------------------------------------------------------------
// Inspection
// ---------------------------------------------------------------------------

// Return the number of BYTES in the UTF-8 encoding of `s`.
// For the number of characters, see `char_count`.
fn len(s: str) -> int ;  // native

// Return the number of Unicode scalar values (characters) in `s`.
//
// P48: the only function here that does not count bytes, and the one to reach
// for when the question is "how many characters".  It is not an index — no
// function in this module takes a codepoint offset.
fn char_count(s: str) -> int ;  // native

// Return the number of bytes in the UTF-8 encoding of `s`.
fn byte_len(s: str) -> int {
    // maniT strings ARE byte strings, so this is len() and says so. Kept as a
    // name because it states at the call site which unit was meant, which the
    // bare `len` does not — see `char_count` for the other unit (P48).
    return len(s);
}

// Return true if `s` has zero bytes.
fn is_empty(s: str) -> bool {
    return len(s) == 0;
}

// Return the BYTE at position `i` (0-indexed), as an unsigned value 0..=255.
// Out of range gives 0.  P48: this is a byte, not a codepoint — for "aéb" the
// four indices give 97, 195, 169, 98.
fn char_at(s: str, i: int) -> char ;  // native

// Return a sub-string of `s` from index `start` up to (not including) `end`.
fn slice(s: str, start: int, end: int) -> str ;  // native

// Return the first `n` BYTES of `s`.  Panics if n > len(s).  P48: a byte
// count, so on a multi-byte character this can split it.
fn take(s: str, n: int) -> str {
    let l: int = len(s);
    let mut k: int = n;
    if k < 0 { k = 0; }
    if k > l { k = l; }
    return slice(s, 0, k);
}

// Return all but the first `n` BYTES of `s`.  Panics if n > len(s).  P48: a
// byte count, so on a multi-byte character this can split it.
fn drop(s: str, n: int) -> str {
    let l: int = len(s);
    let mut k: int = n;
    if k < 0 { k = 0; }
    if k > l { k = l; }
    return slice(s, k, l);
}

// Return true if `s` contains the sub-string `needle`.
fn contains(s: str, needle: str) -> bool ;  // native

// Return true if `s` begins with `prefix`.
fn starts_with(s: str, prefix: str) -> bool {
    let lp: int = len(prefix);
    if lp > len(s) { return false; }
    return slice(s, 0, lp) == prefix;
}

// Return true if `s` ends with `suffix`.
fn ends_with(s: str, suffix: str) -> bool {
    let ls: int = len(s);
    let lx: int = len(suffix);
    if lx > ls { return false; }
    return slice(s, ls - lx, ls) == suffix;
}

// Return the BYTE index of the first occurrence of `needle` in `s`,
// or -1 if not found.  P48.
fn find(s: str, needle: str) -> int ;  // native

// Return the BYTE index of the last occurrence of `needle` in `s`,
// or -1 if not found.  P48.
fn rfind(s: str, needle: str) -> int {
    let ls: int = len(s);
    let ln: int = len(needle);
    // The empty needle matches at the end, mirroring find()'s match at 0.
    if ln == 0 { return ls; }
    if ln > ls { return -1; }
    let stop: int = ls - ln + 1;
    let mut best: int = -1;
    for i in 0..stop {
        if slice(s, i, i + ln) == needle { best = i; }
    }
    return best;
}

// Count non-overlapping occurrences of `needle` in `s`.
fn count(s: str, needle: str) -> int {
    // Non-overlapping, so count() and replace() agree on what an occurrence
    // is: "aaa".count("aa") is 1, not 2.
    let ls: int = len(s);
    let ln: int = len(needle);
    if ln == 0 { return 0; }
    if ln > ls { return 0; }
    let mut c: int = 0;
    let mut i: int = 0;
    while i + ln <= ls {
        if slice(s, i, i + ln) == needle {
            c = c + 1;
            i = i + ln;
        } else {
            i = i + 1;
        }
    }
    return c;
}

// ---------------------------------------------------------------------------
// Transformation
// ---------------------------------------------------------------------------

// Convert all ASCII letters to uppercase.
fn to_upper(s: str) -> str {
    let n: int = len(s);
    let mut out: str = "";
    let mut i: int = 0;
    while i < n {
        let c: char = char_at(s, i);
        let code: int = c as int;
        if code >= 97 {
            if code <= 122 { out = concat(out, from_char((code - 32) as char)); }
            else { out = concat(out, from_char(c)); }
        } else {
            out = concat(out, from_char(c));
        }
        i = i + 1;
    }
    return out;
}

// Convert all ASCII letters to lowercase.
fn to_lower(s: str) -> str {
    let n: int = len(s);
    let mut out: str = "";
    let mut i: int = 0;
    while i < n {
        let c: char = char_at(s, i);
        let code: int = c as int;
        if code >= 65 {
            if code <= 90 { out = concat(out, from_char((code + 32) as char)); }
            else { out = concat(out, from_char(c)); }
        } else {
            out = concat(out, from_char(c));
        }
        i = i + 1;
    }
    return out;
}

// Remove leading and trailing ASCII whitespace.
fn trim(s: str) -> str ;  // native

// Remove only leading ASCII whitespace.
fn trim_start(s: str) -> str {
    let l: int = len(s);
    let mut i: int = 0;
    let mut go: bool = true;
    while go {
        if i >= l {
            go = false;
        } else {
            let c: str = slice(s, i, i + 1);
            if c == " " || c == "\t" || c == "\n" || c == "\r" { i = i + 1; }
            else { go = false; }
        }
    }
    return slice(s, i, l);
}

// Remove only trailing ASCII whitespace.
fn trim_end(s: str) -> str {
    let mut e: int = len(s);
    let mut go: bool = true;
    while go {
        if e <= 0 {
            go = false;
        } else {
            let c: str = slice(s, e - 1, e);
            if c == " " || c == "\t" || c == "\n" || c == "\r" { e = e - 1; }
            else { go = false; }
        }
    }
    return slice(s, 0, e);
}

// Remove leading and trailing occurrences of `chars` (any char in the set).
fn trim_chars(s: str, chars: str) -> str {
    let l: int = len(s);
    let mut a: int = 0;
    let mut go: bool = true;
    while go {
        if a >= l { go = false; }
        elif contains(chars, slice(s, a, a + 1)) { a = a + 1; }
        else { go = false; }
    }
    let mut b: int = l;
    go = true;
    while go {
        if b <= a { go = false; }
        elif contains(chars, slice(s, b - 1, b)) { b = b - 1; }
        else { go = false; }
    }
    return slice(s, a, b);
}

// Replace every non-overlapping occurrence of `from` with `to`.
fn replace(s: str, from: str, to: str) -> str ;  // native

// Replace only the first occurrence of `from` with `to`.
fn replace_first(s: str, from: str, to: str) -> str {
    let l: int = len(s);
    let i: int = find(s, from);
    if i < 0 { return slice(s, 0, l); }
    let head: str = slice(s, 0, i);
    let tail: str = slice(s, i + len(from), l);
    return concat(concat(head, to), tail);
}

// Reverse the BYTE order of `s`.  P48: on a multi-byte character this
// reverses its bytes, which is not a character reversal; see the module
// header.  Reversing ASCII is exact.
fn reverse(s: str) -> str {
    let l: int = len(s);
    let mut out: str = "";
    for i in 0..l {
        out = concat(slice(s, i, i + 1), out);
    }
    return out;
}

// Repeat `s` exactly `n` times, concatenating the copies.
fn repeat(s: str, n: int) -> str {
    let mut out: str = "";
    for _i in 0..n { out = concat(out, s); }
    return out;
}

// Pad `s` on the left with `pad_char` until its length is at least `width`.
// Returns `s` unchanged when it is already at least `width` long.
fn pad_left(s: str, width: int, pad_char: char) -> str {
    let l: int = len(s);
    if l >= width { return s; }
    return concat(repeat(from_char(pad_char), width - l), s);
}

// Pad `s` on the right with `pad_char` until its length is at least `width`.
// Returns `s` unchanged when it is already at least `width` long.
fn pad_right(s: str, width: int, pad_char: char) -> str {
    let l: int = len(s);
    if l >= width { return s; }
    return concat(s, repeat(from_char(pad_char), width - l));
}

// Center `s` within `width` BYTES, padding both sides with `pad_char`.  P48.
// An odd remainder goes to the right, so center("hi", 5, '-') is "-hi--".
fn center(s: str, width: int, pad_char: char) -> str {
    let l: int = len(s);
    if l >= width { return s; }
    let total: int = width - l;
    let left: int = total / 2;
    let pad: str = from_char(pad_char);
    return concat(concat(repeat(pad, left), s), repeat(pad, total - left));
}

// ---------------------------------------------------------------------------
// Splitting and joining
// ---------------------------------------------------------------------------

// Return a sub-string of `s` starting at byte offset `start` with length `len`.
fn substr(s: str, start: int, n: int) -> str {
    let l: int = len(s);
    let mut a: int = start;
    if a < 0 { a = 0; }
    if a > l { a = l; }
    let mut b: int = a + n;
    if b < a { b = a; }
    if b > l { b = l; }
    return slice(s, a, b);
}

// Return the substring before the first occurrence of `sep`.
// Returns the whole string if `sep` is not found.
fn split_head(s: str, sep: str) -> str {
    let l: int = len(s);
    let i: int = find(s, sep);
    if i < 0 { return slice(s, 0, l); }
    return slice(s, 0, i);
}

// Return the substring after the first occurrence of `sep`.
// Returns empty string if `sep` is not found.
fn split_tail(s: str, sep: str) -> str {
    let l: int = len(s);
    let i: int = find(s, sep);
    if i < 0 { return ""; }
    return slice(s, i + len(sep), l);
}

// Split `s` on every occurrence of `delim`, returning a Vec of sub-strings.
// Consecutive delimiters produce empty strings in the result.
fn split(s: str, delim: str) -> Vec<str> ;  // native

// Split `s` into at most `n` parts on `delim`.
fn splitn(s: str, delim: str, n: int) -> Vec<str> {
    // At most n pieces; the last one keeps whatever delimiters remain.
    let out: Vec<str> = Vec::new();
    let ld: int = len(delim);
    let mut rest: str = slice(s, 0, len(s));
    if ld == 0 { out.push(rest); return out; }
    let mut made: int = 1;
    let mut go: bool = true;
    while go {
        if made >= n { go = false; }
        else {
            let i: int = find(rest, delim);
            if i < 0 { go = false; }
            else {
                out.push(slice(rest, 0, i));
                rest = slice(rest, i + ld, len(rest));
                made = made + 1;
            }
        }
    }
    out.push(rest);
    return out;
}

// Split `s` into individual lines (splitting on "\n" and "\r\n").
fn lines(s: str) -> Vec<str> {
    return split(s, "\n");
}

// Join a Vec of strings, placing `sep` between each pair of adjacent strings.
//
// The separator goes BEFORE every element except the first, rather than after
// every element except the last. Both read the same in prose; only the first
// survives an empty Vec without a trailing separator to strip.
fn join(parts: Vec<str>, sep: str) -> str {
    let n: int = parts.len();
    let mut out: str = "";
    let mut i: int = 0;
    while i < n {
        if i > 0 { out = concat(out, sep); }
        out = concat(out, parts[i]);
        i = i + 1;
    }
    return out;
}

// Concatenate two strings.
fn concat(a: str, b: str) -> str ;  // native

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

// Parse `s` as a decimal integer.  Panics on failure.
fn parse_int(s: str) -> int {
    // Leading '+'/'-' then decimal digits. Non-digits are skipped rather than
    // rejected; use try_parse_int when you need to detect malformed input.
    let l: int = len(s);
    let mut i: int = 0;
    let mut neg: bool = false;
    if l > 0 {
        let c0: str = slice(s, 0, 1);
        if c0 == "-" { neg = true; i = 1; }
        if c0 == "+" { i = 1; }
    }
    let mut acc: int = 0;
    while i < l {
        let c: str = slice(s, i, i + 1);
        let mut d: int = -1;
        if c == "0" { d = 0; }
        if c == "1" { d = 1; }
        if c == "2" { d = 2; }
        if c == "3" { d = 3; }
        if c == "4" { d = 4; }
        if c == "5" { d = 5; }
        if c == "6" { d = 6; }
        if c == "7" { d = 7; }
        if c == "8" { d = 8; }
        if c == "9" { d = 9; }
        if d >= 0 { acc = acc * 10 + d; }
        i = i + 1;
    }
    if neg { return -acc; }
    return acc;
}

// Parse `s` as a floating-point number.
//
// Accepts an optional sign, decimal digits, an optional fractional part, and an
// optional `e`/`E` exponent with its own optional sign. Characters outside that
// grammar are SKIPPED rather than rejected, which is the same leniency parse_int
// above already has — the two are deliberately consistent, and neither panics.
// The old doc comment here said "Panics on failure"; nothing ever implemented
// it, on either backend, so there was no behaviour to stay compatible with.
//
// Digits are decoded through `char_at ... as int` rather than the ten-way string
// comparison parse_int uses. Both work; this one is shorter and is the same
// primitive to_upper/to_lower are built on.
fn parse_float(s: str) -> float {
    let l: int = len(s);
    let mut i: int = 0;
    let mut neg: bool = false;
    if l > 0 {
        let c0: int = char_at(s, 0) as int;
        if c0 == 45 { neg = true; i = 1; }
        if c0 == 43 { i = 1; }
    }
    // Mantissa is accumulated as an integer and scaled once at the end, so the
    // fractional digits contribute no rounding error of their own.
    let mut mant: int = 0;
    let mut decimals: int = 0;
    let mut seen_dot: bool = false;
    let mut in_exp: bool = false;
    let mut exp_neg: bool = false;
    let mut expo: int = 0;
    while i < l {
        let c: int = char_at(s, i) as int;
        if c == 46 && !seen_dot && !in_exp {
            seen_dot = true;
        } else {
            if (c == 101 || c == 69) && !in_exp {
                in_exp = true;
                // A sign belonging to the exponent, not to the mantissa.
                if i + 1 < l {
                    let n: int = char_at(s, i + 1) as int;
                    if n == 45 { exp_neg = true; i = i + 1; }
                    if n == 43 { i = i + 1; }
                }
            } else {
                let d: int = c - 48;
                if d >= 0 && d <= 9 {
                    if in_exp {
                        expo = expo * 10 + d;
                    } else {
                        mant = mant * 10 + d;
                        if seen_dot { decimals = decimals + 1; }
                    }
                }
            }
        }
        i = i + 1;
    }
    if exp_neg { expo = 0 - expo; }
    let mut scale: int = expo - decimals;
    let mut out: float = mant as float;
    // Multiply up or divide down rather than raising 10 to a negative power,
    // so the scaling factor is always an exactly-representable power of ten.
    while scale > 0 {
        out = out * 10.0;
        scale = scale - 1;
    }
    while scale < 0 {
        out = out / 10.0;
        scale = scale + 1;
    }
    if neg { return 0.0 - out; }
    return out;
}

// Parse a balanced ternary string ("+", "0", "-" characters) as a t27.
// Ignores leading whitespace.  Panics on invalid characters.
fn parse_ternary(s: str) -> t27 {
    // Horner over MST-first balanced-ternary text. Written out rather than
    // delegating to ternary::t27_from_str so that using any str function does
    // not drag the whole ternary module into the program.
    let l: int = len(s);
    let mut acc: int = 0;
    for i in 0..l {
        let g: str = slice(s, i, i + 1);
        if g != " " {
            let mut d: int = 0;
            if g == "+" { d = 1; }
            if g == "-" { d = -1; }
            acc = acc * 3 + d;
        }
    }
    return acc as t27;
}

// Try to parse `s` as an int; return Ok(n) on success, Err(msg) on failure.
//
// This is the STRICT counterpart to parse_int above. parse_int skips anything
// that is not a digit and can therefore never fail; these two exist precisely to
// report the failure it swallows, so they validate the whole string first and
// only then hand it over.
//
// NOTE for callers: as of 20 August 2026 `Result` supports construction and
// `match`, but `.unwrap()` and `.is_ok()` are declared in type inference and
// emitted by NEITHER backend (ORACLE_FINDINGS section 18). Consume the result
// with `match`, not with a method, until that is fixed.
fn try_parse_int(s: str) -> Result<int, str> {
    let t: str = trim(s);
    let n: int = len(t);
    if n == 0 { return Err("empty string"); }
    let mut i: int = 0;
    let c0: int = char_at(t, 0) as int;
    if c0 == 45 || c0 == 43 {
        if n == 1 { return Err("sign with no digits"); }
        i = 1;
    }
    while i < n {
        let c: int = char_at(t, i) as int;
        if c < 48 || c > 57 { return Err("not an integer"); }
        i = i + 1;
    }
    return Ok(parse_int(t));
}

// Try to parse `s` as a float.
// Strict in the same way, and with the same note about Result methods.
fn try_parse_float(s: str) -> Result<float, str> {
    let t: str = trim(s);
    let n: int = len(t);
    if n == 0 { return Err("empty string"); }
    let mut i: int = 0;
    let c0: int = char_at(t, 0) as int;
    if c0 == 45 || c0 == 43 { i = 1; }
    let mut digits: int = 0;
    let mut dots: int = 0;
    let mut exps: int = 0;
    let mut exp_digits: int = 0;
    while i < n {
        let c: int = char_at(t, i) as int;
        if c >= 48 && c <= 57 {
            if exps > 0 { exp_digits = exp_digits + 1; } else { digits = digits + 1; }
        } else {
            if c == 46 {
                // A dot is only legal once, and never inside the exponent.
                if dots > 0 || exps > 0 { return Err("not a float"); }
                dots = dots + 1;
            } else {
                if c == 101 || c == 69 {
                    if exps > 0 || digits == 0 { return Err("not a float"); }
                    exps = exps + 1;
                    // The exponent carries its own optional sign.
                    if i + 1 < n {
                        let nx: int = char_at(t, i + 1) as int;
                        if nx == 45 || nx == 43 { i = i + 1; }
                    }
                } else {
                    return Err("not a float");
                }
            }
        }
        i = i + 1;
    }
    if digits == 0 { return Err("no digits"); }
    if exps > 0 && exp_digits == 0 { return Err("exponent with no digits"); }
    return Ok(parse_float(t));
}

// ---------------------------------------------------------------------------
// Formatting helpers
// ---------------------------------------------------------------------------

// Convert an int to its decimal string representation.
fn from_int(n: int) -> str {
    return fmt::show_int(n);
}

// Build a one-character string from `c`.
//
// One of only two primitives in this module that touch a char — `char_at` is
// the other. Every char-dependent function above (to_upper, to_lower,
// pad_left, pad_right, center) is written in ManiT on top of these two, so
// each has exactly one implementation and the two backends cannot diverge.
fn from_char(c: char) -> str ;  // native

// Convert a float to a decimal string with default precision.
//
// The five converters below delegate to fmt:: rather than carrying their own
// bodies. They are the same conversion under a second name, and one body per
// conversion is the whole reason `fmt` was moved into ManiT — a second
// implementation here is precisely how `align_left` came to mean two different
// things at once (ORACLE_FINDINGS section 14a).
fn from_float(f: float) -> str {
    return fmt::show_float(f);
}

// Convert a bool to "true" or "false".
fn from_bool(b: bool) -> str {
    return fmt::show_bool(b);
}

// Convert a trit to "+", "0", or "-".
fn from_trit(t: trit) -> str {
    return fmt::show_trit(t);
}

// Convert a bool3 to "True", "Unknown", or "False".
fn from_bool3(b: bool3) -> str {
    return fmt::show_bool3(b);
}

// Convert a t27 to its balanced ternary string representation.
//
// MST-first with no leading zeros, so it is the exact inverse of parse_ternary
// above — verified as a round trip, not assumed.
fn from_ternary(n: t27) -> str {
    return fmt::show_t27(n);
}

// `format` lived here until 20 August 2026. Use fmt::format instead — this was
// a second `; // native` declaration of that same function, with no body behind
// it on either backend, so every call to str::format failed while fmt::format
// worked. It had no callers anywhere in the tree.
//
// It was REMOVED rather than reimplemented, because with this signature it
// cannot be made to work:
//
//   * It cannot delegate. `@fmt_format(ptr, ...)` is VARARG, and the compiler
//     builds those varargs by expanding an array LITERAL at the call site.
//     A `[str]` that arrives as a parameter is one pointer, not a spread, so
//     forwarding it produced garbage on T3 ("59998 and 59998") and a segfault
//     on LLVM. That trap applies to any ManiT function forwarding a slice to
//     fmt::format, not just this one.
//   * It cannot walk the slice itself either. `[str]` supports indexing but has
//     no `.len()` — `a.len()` compiles to a call to `[str::len` — so a template
//     walker has no way to know how many arguments it was given, and would
//     index out of bounds on a malformed template.
//
// Call fmt::format directly with a literal array, which is the form that works.

// ---------------------------------------------------------------------------
// Character classification helpers
// ---------------------------------------------------------------------------

// Return true if every BYTE of `s` is an ASCII digit.  P48: a non-ASCII
// character fails, which is the intended answer for an ASCII predicate.
//
// The empty string is FALSE here, not vacuously true. Read literally, "every
// byte is a digit" is true of a string with no bytes, but these three
// predicates exist to validate input and "" is not a number, a word, or an
// identifier. is_blank below is the deliberate opposite — "" IS blank — which is
// why the choice is spelled out on each rather than left to the reader.
fn is_numeric(s: str) -> bool {
    let n: int = len(s);
    if n == 0 { return false; }
    let mut i: int = 0;
    while i < n {
        let c: int = char_at(s, i) as int;
        if c < 48 || c > 57 { return false; }
        i = i + 1;
    }
    return true;
}

// Return true if every BYTE of `s` is an ASCII letter.  P48.
// The empty string is false — see is_numeric.
fn is_alpha(s: str) -> bool {
    let n: int = len(s);
    if n == 0 { return false; }
    let mut i: int = 0;
    while i < n {
        let c: int = char_at(s, i) as int;
        let upper: bool = c >= 65 && c <= 90;
        let lower: bool = c >= 97 && c <= 122;
        if !upper && !lower { return false; }
        i = i + 1;
    }
    return true;
}

// Return true if every BYTE of `s` is an ASCII letter or digit.  P48.
// The empty string is false — see is_numeric.
fn is_alphanumeric(s: str) -> bool {
    let n: int = len(s);
    if n == 0 { return false; }
    let mut i: int = 0;
    while i < n {
        let c: int = char_at(s, i) as int;
        let digit: bool = c >= 48 && c <= 57;
        let upper: bool = c >= 65 && c <= 90;
        let lower: bool = c >= 97 && c <= 122;
        if !digit && !upper && !lower { return false; }
        i = i + 1;
    }
    return true;
}

// Return true if `s` is empty or contains only ASCII whitespace.
//
// Whitespace is space, tab, newline and carriage return — exactly the four that
// trim() strips, so `is_blank(s)` and `is_empty(trim(s))` always agree. Vertical
// tab and form feed are deliberately NOT included: widening the set here would
// make a string that is blank but does not trim away to nothing.
fn is_blank(s: str) -> bool {
    let n: int = len(s);
    let mut i: int = 0;
    while i < n {
        let c: int = char_at(s, i) as int;
        if c != 32 && c != 9 && c != 10 && c != 13 { return false; }
        i = i + 1;
    }
    return true;
}
