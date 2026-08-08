// stdlib/std/str.mt
// String utilities for maniT.
//
// maniT strings are UTF-8 encoded, immutable byte sequences.  All index and
// length values are measured in Unicode scalar values (codepoints), not raw
// bytes, unless the function name explicitly mentions "bytes".
//
// Usage:
//   use std::str;
//   let n = str::len("hello");  // 5

// ---------------------------------------------------------------------------
// Inspection
// ---------------------------------------------------------------------------

// Return the number of Unicode codepoints in `s`.
fn len(s: str) -> int { /* native */ }

// Return the number of bytes in the UTF-8 encoding of `s`.
fn byte_len(s: str) -> int { /* native */ }

// Return true if `s` has zero codepoints.
fn is_empty(s: str) -> bool { /* native */ }

// Return the codepoint at position `i` (0-indexed).  Panics on out-of-bounds.
fn char_at(s: str, i: int) -> char { /* native */ }

// Return a sub-string of `s` from index `start` up to (not including) `end`.
fn slice(s: str, start: int, end: int) -> str { /* native */ }

// Return the first `n` codepoints of `s`.  Panics if n > len(s).
fn take(s: str, n: int) -> str { /* native */ }

// Return all but the first `n` codepoints of `s`.  Panics if n > len(s).
fn drop(s: str, n: int) -> str { /* native */ }

// Return true if `s` contains the sub-string `needle`.
fn contains(s: str, needle: str) -> bool { /* native */ }

// Return true if `s` begins with `prefix`.
fn starts_with(s: str, prefix: str) -> bool { /* native */ }

// Return true if `s` ends with `suffix`.
fn ends_with(s: str, suffix: str) -> bool { /* native */ }

// Return the codepoint index of the first occurrence of `needle` in `s`,
// or -1 if not found.
fn find(s: str, needle: str) -> int { /* native */ }

// Return the codepoint index of the last occurrence of `needle` in `s`,
// or -1 if not found.
fn rfind(s: str, needle: str) -> int { /* native */ }

// Count non-overlapping occurrences of `needle` in `s`.
fn count(s: str, needle: str) -> int { /* native */ }

// ---------------------------------------------------------------------------
// Transformation
// ---------------------------------------------------------------------------

// Convert all ASCII letters to uppercase.
fn to_upper(s: str) -> str { /* native */ }

// Convert all ASCII letters to lowercase.
fn to_lower(s: str) -> str { /* native */ }

// Remove leading and trailing ASCII whitespace.
fn trim(s: str) -> str { /* native */ }

// Remove only leading ASCII whitespace.
fn trim_start(s: str) -> str { /* native */ }

// Remove only trailing ASCII whitespace.
fn trim_end(s: str) -> str { /* native */ }

// Remove leading and trailing occurrences of `chars` (any char in the set).
fn trim_chars(s: str, chars: str) -> str { /* native */ }

// Replace every non-overlapping occurrence of `from` with `to`.
fn replace(s: str, from: str, to: str) -> str { /* native */ }

// Replace only the first occurrence of `from` with `to`.
fn replace_first(s: str, from: str, to: str) -> str { /* native */ }

// Reverse the codepoint order of `s`.
fn reverse(s: str) -> str { /* native */ }

// Repeat `s` exactly `n` times, concatenating the copies.
fn repeat(s: str, n: int) -> str { /* native */ }

// Pad `s` on the left with `pad_char` until its length is at least `width`.
fn pad_left(s: str, width: int, pad_char: char) -> str { /* native */ }

// Pad `s` on the right with `pad_char` until its length is at least `width`.
fn pad_right(s: str, width: int, pad_char: char) -> str { /* native */ }

// Center `s` within `width` codepoints, padding both sides with `pad_char`.
fn center(s: str, width: int, pad_char: char) -> str { /* native */ }

// ---------------------------------------------------------------------------
// Splitting and joining
// ---------------------------------------------------------------------------

// Return a sub-string of `s` starting at byte offset `start` with length `len`.
fn substr(s: str, start: int, len: int) -> str { /* native */ }

// Return the substring before the first occurrence of `sep`.
// Returns the whole string if `sep` is not found.
fn split_head(s: str, sep: str) -> str { /* native */ }

// Return the substring after the first occurrence of `sep`.
// Returns empty string if `sep` is not found.
fn split_tail(s: str, sep: str) -> str { /* native */ }

// Split `s` on every occurrence of `delim`, returning a Vec of sub-strings.
// Consecutive delimiters produce empty strings in the result.
fn split(s: str, delim: str) -> Vec<str> { /* native */ }

// Split `s` into at most `n` parts on `delim`.
fn splitn(s: str, delim: str, n: int) -> Vec<str> { /* native */ }

// Split `s` into individual lines (splitting on "\n" and "\r\n").
fn lines(s: str) -> Vec<str> { /* native */ }

// Join a Vec of strings, placing `sep` between each pair of adjacent strings.
fn join(parts: Vec<str>, sep: str) -> str { /* native */ }

// Concatenate two strings.
fn concat(a: str, b: str) -> str { /* native */ }

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

// Parse `s` as a decimal integer.  Panics on failure.
fn parse_int(s: str) -> int { /* native */ }

// Parse `s` as a floating-point number.  Panics on failure.
fn parse_float(s: str) -> float { /* native */ }

// Parse a balanced ternary string ("+", "0", "-" characters) as a t27.
// Ignores leading whitespace.  Panics on invalid characters.
fn parse_ternary(s: str) -> t27 { /* native */ }

// Try to parse `s` as an int; return Ok(n) on success, Err(msg) on failure.
fn try_parse_int(s: str) -> Result<int, str> { /* native */ }

// Try to parse `s` as a float.
fn try_parse_float(s: str) -> Result<float, str> { /* native */ }

// ---------------------------------------------------------------------------
// Formatting helpers
// ---------------------------------------------------------------------------

// Convert an int to its decimal string representation.
fn from_int(n: int) -> str { /* native */ }

// Convert a float to a decimal string with default precision.
fn from_float(f: float) -> str { /* native */ }

// Convert a bool to "true" or "false".
fn from_bool(b: bool) -> str { /* native */ }

// Convert a trit to "+", "0", or "-".
fn from_trit(t: trit) -> str { /* native */ }

// Convert a bool3 to "True", "Unknown", or "False".
fn from_bool3(b: bool3) -> str { /* native */ }

// Convert a t27 to its balanced ternary string representation.
fn from_ternary(n: t27) -> str { /* native */ }

// Produce a formatted string using a template.
// Placeholders are `{}` for default formatting, `{:t}` for ternary.
// This is a thin wrapper around std::fmt::format.
fn format(template: str, args: [str]) -> str { /* native */ }

// ---------------------------------------------------------------------------
// Character classification helpers
// ---------------------------------------------------------------------------

// Return true if every codepoint in `s` is an ASCII digit.
fn is_numeric(s: str) -> bool { /* native */ }

// Return true if every codepoint in `s` is an ASCII letter.
fn is_alpha(s: str) -> bool { /* native */ }

// Return true if every codepoint in `s` is an ASCII letter or digit.
fn is_alphanumeric(s: str) -> bool { /* native */ }

// Return true if `s` is empty or contains only ASCII whitespace.
fn is_blank(s: str) -> bool { /* native */ }
