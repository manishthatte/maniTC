//! Fuzz the semantic analyzer.  (F-8)
//!
//! © Manish Jagdish Thatte
//!
//! Only inputs that lex and parse reach here, so the fuzzer has to build a
//! syntactically valid program before it can test anything — which is exactly
//! why the corpus matters more for this target than for the other two. Seeded
//! from the examples, the stdlib and the regression tests, libFuzzer mutates
//! real programs rather than starting from noise.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(src) = std::str::from_utf8(data) else {
        return;
    };
    let Ok(tokens) = manitc::lexer::Lexer::with_file(src, "<fuzz>").tokenize() else {
        return;
    };
    let Ok(program) = manitc::parser::Parser::with_file(tokens, "<fuzz>").parse() else {
        return;
    };
    let mut analyzer = manitc::semantic::SemanticAnalyzer::with_file("<fuzz>");
    let _ = analyzer.analyze(&program);
});
