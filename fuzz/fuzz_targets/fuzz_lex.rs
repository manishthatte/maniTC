//! Fuzz the lexer.  (F-8)
//!
//! © Manish Jagdish Thatte
//!
//! The contract: for ANY input, `tokenize()` returns `Ok` or `Err`. It never
//! panics, never aborts, and never runs away. A lexer that can be made to
//! panic can be made to panic by a user, and section 49 (three stdlib modules
//! do not parse) is what a front end looks like when nothing has ever fed it
//! anything but well-formed code.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(src) = std::str::from_utf8(data) else {
        return;
    };
    let _ = manitc::lexer::Lexer::with_file(src, "<fuzz>").tokenize();
});
