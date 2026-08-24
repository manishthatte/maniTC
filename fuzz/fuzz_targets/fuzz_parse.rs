//! Fuzz the lexer and parser together.  (F-8)
//!
//! © Manish Jagdish Thatte
//!
//! The contract is the same as the lexer's: any input, a Result, never a
//! panic. This is the target that would have found section 48's 13-parameter
//! emitter panic class, and the recursion-depth guard (MAX_PARSE_DEPTH) is
//! here to be tested rather than trusted — a fuzzer generates nesting far
//! faster than a person writes it.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(src) = std::str::from_utf8(data) else {
        return;
    };
    let Ok(tokens) = manitc::lexer::Lexer::with_file(src, "<fuzz>").tokenize() else {
        return;
    };
    let _ = manitc::parser::Parser::with_file(tokens, "<fuzz>").parse();
});
