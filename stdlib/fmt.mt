// stdlib/std/fmt.mt
// String formatting utilities for maniT.
//
// Provides a format! macro-equivalent function, display traits, and
// ternary-specific number formatters.  The primary entry point for most code
// is fmt::format (or its shorthand fmt!) which works like Python's str.format
// or Rust's format! macro.
//
// Format string syntax:
//   {}        — default display
//   {:d}      — decimal integer
//   {:f}      — floating point (default precision)
//   {:f.2}    — float with 2 decimal places
//   {:t}      — balanced ternary ("+0-+")
//   {:t9}     — 9-trit padded ternary
//   {:t27}    — 27-trit padded ternary
//   {:b}      — bool3 ("True"/"Unknown"/"False")
//   {:x}      — lowercase hexadecimal
//   {:X}      — uppercase hexadecimal
//   {:o}      — octal
//   {:e}      — scientific notation
//   {:>10}    — right-align in a field of width 10
//   {:<10}    — left-align in a field of width 10
//   {:^10}    — center in a field of width 10
//   {:0>5}    — zero-pad to width 5 (right-align with '0')
//
// Usage:
//   use std::fmt;
//   let s = fmt::format("x={}, t={:t}", [fmt::show_int(x), fmt::show_t27(v)]);
//
// ---------------------------------------------------------------------------
// Implementation note — 20 August 2026
// ---------------------------------------------------------------------------
//
// Until this date only SIX of the 31 functions below had an implementation on
// either backend: format, show_int, show_float, show_bool, align_left and
// align_right. The other 25 were signatures with nothing behind them, so a
// program that followed this module header hit `Undefined label` on T3 or an
// undefined symbol at link on LLVM. show_trit was worse than either — it
// linked on LLVM and did not exist on T3, so the same source built on one
// backend and not the other.
//
// They are now implemented IN ManiT, on top of the str:: and ternary::
// primitives, rather than as new C functions plus new T3 syscalls. That is the
// anti-divergence rule this stdlib is built on: one body, compiled for both
// targets, so the backends cannot disagree. The rule is not a preference —
// fmt::align_left was C on LLVM and a syscall on T3, and for months it silently
// dropped its pad character on the LLVM side while T3 honoured it. Nothing
// caught it, because native call arguments are never type-checked. align_left
// and align_right are therefore ALSO reimplemented here now, over
// str::pad_right/pad_left, which closes that class of defect rather than
// patching one instance of it.
//
// Only four functions remain native, because each is genuinely primitive —
// there is nothing in ManiT to write them in terms of:
//
//   format       the template engine
//   show_int     int   -> str
//   show_float   float -> str
//   show_bool    bool  -> str

use std::str;
use std::ternary;

// ---------------------------------------------------------------------------
// Display trait (interface)
// ---------------------------------------------------------------------------

// Types that implement Display can be embedded in format strings automatically.
// Implement this for your own structs to get fmt::format support.
trait Display {
    fn display(self) -> str;
}

// Debug variant — intended for programmer-readable output (may include
// type information, raw values, etc.).
trait Debug {
    fn debug(self) -> str;
}

// ---------------------------------------------------------------------------
// Core format function
// ---------------------------------------------------------------------------

// Format `template` by replacing `{}` placeholders with values from `args`.
// Each arg must be a str produced by one of the show_* helpers below, or by
// a type's Display implementation.
//
// Panics if the number of placeholders does not match len(args).
fn format(template: str, args: [str]) -> str ;  // native

// Convenience: format with a single argument.
fn format1(template: str, a: str) -> str {
    return format(template, [a]);
}

// Convenience: format with two arguments.
fn format2(template: str, a: str, b: str) -> str {
    return format(template, [a, b]);
}

// Convenience: format with three arguments.
fn format3(template: str, a: str, b: str, c: str) -> str {
    return format(template, [a, b, c]);
}

// ---------------------------------------------------------------------------
// Type → str conversion ("show" functions)
// ---------------------------------------------------------------------------

// Convert an int to a decimal string.
fn show_int(n: int) -> str ;  // native

// Convert a float to a decimal string (default precision: 6 sig figs).
fn show_float(f: float) -> str ;  // native

// Convert a float with exactly `dp` decimal places, rounding half away from
// zero.  show_float_dp(2.345, 2) is "2.35"; show_float_dp(-2.345, 2) is
// "-2.35"; show_float_dp(7.0, 0) is "7".
//
// Scales by 10^dp into an int, so it is exact only while |f| * 10^dp stays
// inside the int range.  Use show_float or show_float_sci for magnitudes
// beyond that.
fn show_float_dp(f: float, dp: int) -> str {
    let mut x: float = f;
    let mut neg: bool = false;
    if x < 0.0 { neg = true; x = 0.0 - x; }

    let mut scale: int = 1;
    for _i in 0..dp { scale = scale * 10; }

    // + 0.5 then truncate is round-half-up, and the sign was already taken
    // off, so on the original value it is round-half-away-from-zero.
    let scaled: int = (x * (scale as float) + 0.5) as int;
    let whole: int = scaled / scale;

    let mut out: str = show_int(whole);
    if dp > 0 {
        let frac: int = scaled % scale;
        out = str::concat(out, str::concat(".", str::pad_left(show_int(frac), dp, '0')));
    }
    if neg { return str::concat("-", out); }
    return out;
}

// Convert a float in scientific notation with six decimal places and a
// two-digit signed exponent, matching C's "%e": show_float_sci(12345.0) is
// "1.234500e+04".  Zero is "0.000000e+00".
fn show_float_sci(f: float) -> str {
    if f == 0.0 { return "0.000000e+00"; }

    let mut x: float = f;
    let mut neg: bool = false;
    if x < 0.0 { neg = true; x = 0.0 - x; }

    // Normalise the mantissa into [1, 10).
    let mut e: int = 0;
    while x >= 10.0 { x = x / 10.0; e = e + 1; }
    while x < 1.0 { x = x * 10.0; e = e - 1; }

    let mut mant: str = show_float_dp(x, 6);
    // Rounding to six places can carry the mantissa out of range — 9.9999999
    // becomes "10.000000". Renormalise once; a second carry is impossible.
    if str::len(mant) > 1 {
        if str::slice(mant, 0, 2) == "10" {
            e = e + 1;
            mant = show_float_dp(x / 10.0, 6);
        }
    }

    let mut sign: str = "+";
    let mut mag: int = e;
    if e < 0 { sign = "-"; mag = 0 - e; }
    let out: str = format("{}e{}{}", [mant, sign, str::pad_left(show_int(mag), 2, '0')]);
    if neg { return str::concat("-", out); }
    return out;
}

// Convert a bool to "true" or "false".
fn show_bool(b: bool) -> str ;  // native

// Convert a bool3 to "True", "Unknown", or "False".
fn show_bool3(b: bool3) -> str {
    return tif b { + => "True", 0 => "Unknown", - => "False" };
}

// Convert a trit to "+", "0", or "-".
fn show_trit(t: trit) -> str {
    return tif t { + => "+", 0 => "0", - => "-" };
}

// Convert a t27 to its balanced ternary string (no leading zeros).
fn show_t27(n: t27) -> str {
    return ternary::t27_to_str(n);
}

// Convert a t27 to a zero-padded balanced ternary string of exactly `width`
// trit positions.  Truncates on the left if the value needs more trits.
fn show_t27_padded(n: t27, width: int) -> str {
    let full: str = ternary::t27_to_str_padded(n);  // exactly 27 glyphs, MST-first
    let l: int = str::len(full);
    if width >= l { return str::pad_left(full, width, '0'); }
    // Keep the `width` LEAST significant trits: truncation drops from the left.
    return str::slice(full, l - width, l);
}

// Convert a t9 to balanced ternary (no leading zeros).
fn show_t9(n: t9) -> str {
    return show_t27(ternary::int_to_t27(ternary::t9_to_int(n)));
}

// Convert a tryte to balanced ternary (always 3 characters).
fn show_tryte(n: tryte) -> str {
    return show_t27_padded(ternary::int_to_t27(ternary::tryte_to_int(n)), 3);
}

// Render `n` in `base` using `digits` as the digit alphabet, most significant
// first, with a leading "-" for negatives.
//
// One helper behind show_hex, show_hex_upper, show_octal and show_binary,
// rather than four separate conversions that can each be wrong in their own
// way.  `base` must be at least 2 and no more than str::len(digits).
fn to_radix(n: int, base: int, digits: str) -> str {
    if n == 0 { return "0"; }
    let mut v: int = n;
    let mut neg: bool = false;
    if v < 0 { neg = true; v = 0 - v; }
    let mut out: str = "";
    while v > 0 {
        let d: int = v % base;
        out = str::concat(str::slice(digits, d, d + 1), out);
        v = v / base;
    }
    if neg { return str::concat("-", out); }
    return out;
}

// Convert an int to hexadecimal ("0x1a2b").  Negatives keep their sign in
// front of the prefix: show_hex(-26) is "-0x1a".
fn show_hex(n: int) -> str {
    if n < 0 { return str::concat("-0x", to_radix(0 - n, 16, "0123456789abcdef")); }
    return str::concat("0x", to_radix(n, 16, "0123456789abcdef"));
}

// Convert an int to uppercase hexadecimal ("0x1A2B").
fn show_hex_upper(n: int) -> str {
    if n < 0 { return str::concat("-0x", to_radix(0 - n, 16, "0123456789ABCDEF")); }
    return str::concat("0x", to_radix(n, 16, "0123456789ABCDEF"));
}

// Convert an int to octal ("0o755").
fn show_octal(n: int) -> str {
    if n < 0 { return str::concat("-0o", to_radix(0 - n, 8, "01234567")); }
    return str::concat("0o", to_radix(n, 8, "01234567"));
}

// Convert an int to binary ("0b1010").
//
// Present for interoperability with binary hardware and file formats.  Base 3
// is the native radix of this language — reach for show_t27 or show_dual
// unless you are talking to something binary.
fn show_binary(n: int) -> str {
    if n < 0 { return str::concat("-0b", to_radix(0 - n, 2, "01")); }
    return str::concat("0b", to_radix(n, 2, "01"));
}

// ---------------------------------------------------------------------------
// Alignment and padding helpers
// ---------------------------------------------------------------------------

// Right-align `s` in a field of `width` characters, padding with `pad`.
// Returns `s` unchanged when it is already at least `width` long — it is never
// truncated.
fn align_right(s: str, width: int, pad: char) -> str {
    return str::pad_left(s, width, pad);
}

// Left-align `s` in a field of `width` characters.
fn align_left(s: str, width: int, pad: char) -> str {
    return str::pad_right(s, width, pad);
}

// Center `s` in a field of `width` characters.  An odd remainder goes to the
// right, so align_center("hi", 5, '-') is "-hi--".
fn align_center(s: str, width: int, pad: char) -> str {
    return str::center(s, width, pad);
}

// Zero-pad an integer string to `width` characters.
//
// A leading "-" stays in front of the zeros, which is the whole reason this is
// not just align_right(s, width, '0'): zero_pad("-42", 5) is "-0042", never
// "00-42".
fn zero_pad(s: str, width: int) -> str {
    let l: int = str::len(s);
    if l >= width { return s; }
    if l > 0 {
        if str::slice(s, 0, 1) == "-" {
            return str::concat("-", str::pad_left(str::slice(s, 1, l), width - 1, '0'));
        }
    }
    return str::pad_left(s, width, '0');
}

// ---------------------------------------------------------------------------
// Ternary-specific display utilities
// ---------------------------------------------------------------------------

// 3 raised to `k`, for k >= 0.
//
// A local helper because math::pow3 does not resolve on the T3 backend; see
// ORACLE_FINDINGS section 17. Keeping it here means this module has no
// dependency on math:: at all.
fn pow3_int(k: int) -> int {
    let mut p: int = 1;
    for _i in 0..k { p = p * 3; }
    return p;
}

// Balanced ternary digits of `n`, least significant first.
//
// Derived here rather than through ternary::unpack_trits so the callers below
// work for any int without a t27 round-trip, and so the digit count follows
// the value instead of being fixed at 27.
//
// ManiT's % truncates toward zero, so it yields 2 and -2 where balanced
// ternary needs -1 and +1; the fix-up is the two `if`s, and the carry is
// folded into (v - d) / 3 which stays exact for either sign.
fn balanced_digits(n: int) -> Vec<int> {
    // No `mut` on a Vec that is pushed to: the compiler enforces `mut` for
    // assignment but not for method receivers (ORACLE_FINDINGS section 14c),
    // so `let mut` here would emit a "does not need to be mutable" warning
    // into every program that touches fmt::.
    let out: Vec<int> = Vec::new();
    if n == 0 { out.push(0); return out; }
    let mut v: int = n;
    while v != 0 {
        let mut d: int = v % 3;
        if d == 2 { d = -1; }
        if d == -2 { d = 1; }
        out.push(d);
        v = (v - d) / 3;
    }
    return out;
}

// Glyph for a balanced ternary digit given as an int in {-1, 0, +1}.
fn digit_glyph(d: int) -> str {
    if d > 0 { return "+"; }
    if d < 0 { return "-"; }
    return "0";
}

// Format `n` as both decimal and balanced ternary side-by-side.
// Example: show_dual(5) -> "5 (+--)"
fn show_dual(n: int) -> str {
    return format("{} ({})", [show_int(n), show_t27(ternary::int_to_t27(n))]);
}

// Produce a multi-line table showing the trit decomposition of `n`.
// Each row shows the trit position, 3^position, the trit value, and
// the contribution to the total.  Rows run most significant first and the
// last line is the total.  Every line, including the last, ends in "\n".
//
//   pos       3^pos  trit   contribution
//     2           9     +              9
//     1           3     -             -3
//     0           1     -             -1
//   total                              5
fn show_trit_table(n: int) -> str {
    let ds: Vec<int> = balanced_digits(n);
    let mut out: str = "pos       3^pos  trit   contribution\n";
    let mut i: int = ds.len() - 1;
    while i >= 0 {
        let d: int = ds.get(i);
        let place: int = pow3_int(i);
        out = str::concat(out, format("{}{}{}{}\n", [
            align_right(show_int(i), 3, ' '),
            align_right(show_int(place), 12, ' '),
            align_right(digit_glyph(d), 6, ' '),
            align_right(show_int(d * place), 15, ' ')]));
        i = i - 1;
    }
    return str::concat(out, format("total{}\n", [align_right(show_int(n), 31, ' ')]));
}

// Format a trit slice as a compact string (MST-first).
//
// Trit slices are stored least-significant-first, and ternary::trits_to_str
// emits them in that stored order, so this reverses it. Leading zeros are
// dropped — that is what "compact" means here — but a value of zero still
// renders as "0". An empty slice gives an empty string.
fn show_trit_slice(trits: [trit]) -> str {
    let mst: str = str::reverse(ternary::trits_to_str(trits));
    let l: int = str::len(mst);
    if l == 0 { return ""; }
    let mut i: int = 0;
    while i < l - 1 {
        if str::slice(mst, i, i + 1) != "0" { break; }
        i = i + 1;
    }
    return str::slice(mst, i, l);
}

// Format a t27 as a colour-coded ANSI string (+ green, - red, 0 white).
//
// The caller decides whether colour is wanted: this always emits the escape
// sequences, so use show_t27 when writing to a file or a terminal that cannot
// render them.  (The signature promised graceful degradation, which a pure
// string function has no way to detect.)
//
// The ESC byte is built with `27 as char` because ManiT string literals have
// no \x or \u escape — the lexer accepts only \n \t \r \\ \" and \0.
fn show_t27_colour(n: t27) -> str {
    let s: str = show_t27(n);
    let l: int = str::len(s);
    let mut out: str = "";
    for i in 0..l {
        let g: str = str::slice(s, i, i + 1);
        // The SGR code is chosen as a fresh literal each iteration rather than
        // read from a variable hoisted out of the loop: `str` is a moved value,
        // so `let col: str = green;` inside a loop is rejected — the value would
        // be moved on every pass.
        let mut code: str = "[37m";
        if g == "+" { code = "[32m"; }
        if g == "-" { code = "[31m"; }
        out = str::concat(out, ansi_wrap(code, g));
    }
    return out;
}

// Wrap `body` in the ANSI SGR sequence `code`, then reset.
//
// Each ESC is built by its own str::from_char call because a `str` local can
// only be moved once, and this needs two of them.
fn ansi_wrap(code: str, body: str) -> str {
    let open: str = str::concat(str::from_char(27 as char), code);
    let close: str = str::concat(str::from_char(27 as char), "[0m");
    return str::concat(open, str::concat(body, close));
}

// ---------------------------------------------------------------------------
// bool3 truth-table formatter
// ---------------------------------------------------------------------------

// Print a two-input truth table for the given ternary binary operator.
// `op_name` is the label (e.g., "tand").
// `op` is a function trit × trit → trit.
//
//   tand |  +  0  -
//   -----+---------
//      + |  +  0  -
//      0 |  0  0  -
//      - |  -  -  -
fn print_truth_table(op_name: str, op: fn(trit, trit) -> trit) {
    let vals: [trit; 3] = [+, 0, -];
    io::println(format("{} |  +  0  -", [align_left(op_name, 4, ' ')]));
    io::println("-----+---------");
    for i in 0..3 {
        let mut row: str = format("{} |", [align_right(show_trit(vals[i]), 4, ' ')]);
        for j in 0..3 {
            row = str::concat(row, format("  {}", [show_trit(op(vals[i], vals[j]))]));
        }
        io::println(row);
    }
}

// ---------------------------------------------------------------------------
// Structured output
// ---------------------------------------------------------------------------

// Print `label: value` pairs as an aligned two-column table to stdout.
// The label column is as wide as the longest label.
fn print_table(rows: Vec<(str, str)>) {
    let n: int = rows.len();
    let mut w: int = 0;
    for i in 0..n {
        let r: (str, str) = rows.get(i);
        let l: int = str::len(r.0);
        if l > w { w = l; }
    }
    for i in 0..n {
        let r: (str, str) = rows.get(i);
        io::println(format("{}: {}", [align_left(r.0, w, ' '), r.1]));
    }
}

// Print a horizontal separator line of `width` characters.
fn print_separator(width: int, ch: char) {
    io::println(str::repeat(str::from_char(ch), width));
}

// Print `title` centered and surrounded by separator lines.
fn print_section(title: str) {
    let w: int = str::len(title) + 4;
    print_separator(w, '=');
    io::println(str::center(title, w, ' '));
    print_separator(w, '=');
}
