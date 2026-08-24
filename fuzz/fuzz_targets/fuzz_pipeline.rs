//! Fuzz the whole front end through IR lowering.  (F-8)
//!
//! © Manish Jagdish Thatte
//!
//! Stops at the IR rather than at codegen. Lowering is where a program that
//! passed every check still meets an assumption nobody wrote down, and it is
//! the last stage that is backend-independent — so a panic found here is a
//! defect in the language implementation rather than in one target.
//!
//! Codegen is deliberately NOT included: the T3 emitter and the LLVM emitter
//! are separate targets' worth of surface, and mixing them in would make a
//! crash report say "the compiler panicked" without saying which half.
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
    let Ok(typed) = analyzer.analyze(&program) else {
        return;
    };
    // Borrow checking first, as the driver does: lowering is entitled to
    // assume it ran.
    if manitc::borrow::check_borrows(&typed).is_err() {
        return;
    }
    let mut module = manitc::ir::IRLowerer::lower(&typed);
    manitc::ir::optimize::run_passes(&mut module);
});
