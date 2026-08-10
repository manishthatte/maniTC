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
fn format1(template: str, a: str) -> str ;  // native

// Convenience: format with two arguments.
fn format2(template: str, a: str, b: str) -> str ;  // native

// Convenience: format with three arguments.
fn format3(template: str, a: str, b: str, c: str) -> str ;  // native

// ---------------------------------------------------------------------------
// Type → str conversion ("show" functions)
// ---------------------------------------------------------------------------

// Convert an int to a decimal string.
fn show_int(n: int) -> str ;  // native

// Convert a float to a decimal string (default precision: 6 sig figs).
fn show_float(f: float) -> str ;  // native

// Convert a float with exactly `dp` decimal places.
fn show_float_dp(f: float, dp: int) -> str ;  // native

// Convert a float in scientific notation ("1.23e+04").
fn show_float_sci(f: float) -> str ;  // native

// Convert a bool to "true" or "false".
fn show_bool(b: bool) -> str ;  // native

// Convert a bool3 to "True", "Unknown", or "False".
fn show_bool3(b: bool3) -> str ;  // native

// Convert a trit to "+", "0", or "-".
fn show_trit(t: trit) -> str ;  // native

// Convert a t27 to its balanced ternary string (no leading zeros).
fn show_t27(n: t27) -> str ;  // native

// Convert a t27 to a zero-padded balanced ternary string of exactly `width`
// trit positions.  Truncates on the left if the value needs more trits.
fn show_t27_padded(n: t27, width: int) -> str ;  // native

// Convert a t9 to balanced ternary (no leading zeros).
fn show_t9(n: t9) -> str ;  // native

// Convert a tryte to balanced ternary (always 3 characters).
fn show_tryte(n: tryte) -> str ;  // native

// Convert an int to hexadecimal ("0x1a2b").
fn show_hex(n: int) -> str ;  // native

// Convert an int to uppercase hexadecimal ("0x1A2B").
fn show_hex_upper(n: int) -> str ;  // native

// Convert an int to octal ("0o755").
fn show_octal(n: int) -> str ;  // native

// Convert an int to binary ("0b1010").
fn show_binary(n: int) -> str ;  // native

// ---------------------------------------------------------------------------
// Alignment and padding helpers
// ---------------------------------------------------------------------------

// Right-align `s` in a field of `width` characters, padding with `pad`.
fn align_right(s: str, width: int, pad: char) -> str ;  // native

// Left-align `s` in a field of `width` characters.
fn align_left(s: str, width: int, pad: char) -> str ;  // native

// Center `s` in a field of `width` characters.
fn align_center(s: str, width: int, pad: char) -> str ;  // native

// Zero-pad an integer string to `width` digits.
fn zero_pad(s: str, width: int) -> str ;  // native

// ---------------------------------------------------------------------------
// Ternary-specific display utilities
// ---------------------------------------------------------------------------

// Format `n` as both decimal and balanced ternary side-by-side.
// Example: show_dual(5) -> "5 (+--)"
fn show_dual(n: int) -> str ;  // native

// Produce a multi-line table showing the trit decomposition of `n`.
// Each row shows the trit position, 3^position, the trit value, and
// the contribution to the total.
fn show_trit_table(n: int) -> str ;  // native

// Format a trit slice as a compact string (MST-first).
fn show_trit_slice(trits: [trit]) -> str ;  // native

// Format a t27 as a colour-coded ANSI string (+ green, - red, 0 white).
// Degrades gracefully if the terminal does not support ANSI codes.
fn show_t27_colour(n: t27) -> str ;  // native

// ---------------------------------------------------------------------------
// bool3 truth-table formatter
// ---------------------------------------------------------------------------

// Print a two-input truth table for the given ternary binary operator.
// `op_name` is the label (e.g., "tand").
// `op` is a function trit × trit → trit.
fn print_truth_table(op_name: str, op: fn(trit, trit) -> trit) ;  // native

// ---------------------------------------------------------------------------
// Structured output
// ---------------------------------------------------------------------------

// Print `label: value` pairs as an aligned two-column table to stdout.
fn print_table(rows: Vec<(str, str)>) ;  // native

// Print a horizontal separator line of `width` characters.
fn print_separator(width: int, ch: char) ;  // native

// Print `title` centered and surrounded by separator lines.
fn print_section(title: str) ;  // native
