use std::io;
use std::str;
// P48. `len` and `byte_len` ARE synonyms and both count bytes — that is the
// decision, not a defect, and `str::slice` has been byte-ranged on both
// backends since 19 August (P50). What was missing was any way to ask the
// other question, so `char_count` is new here and is what makes `byte_len` a
// distinction rather than a second spelling of `len`.
//
// This file used to expect 3 then 4 — `len` counting codepoints. That
// expectation was a report of what someone wanted, not a specification
// (report.txt P45), and it contradicted `s64_char_as_int_sign`, which expects
// `char_at("é", 0)` to be 195: a codepoint `len` sharing an index with a byte
// `char_at` cannot be looped over.
fn main() {
    io::print_int(str::len("aéb")); io::newline();
    io::print_int(str::byte_len("aéb")); io::newline();
    io::print_int(str::char_count("aéb")); io::newline();
}
