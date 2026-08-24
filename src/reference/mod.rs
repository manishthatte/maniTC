//! A3 — the reference interpreter: a third account of ManiT, derived from
//! `docs/semantics.md` rather than from this compiler's front end.
//!
//! © Manish Jagdish Thatte
//!
//! ## The independence rule
//!
//! **No module under `src/reference/` may import from anywhere else in this
//! crate.** It has its own lexer, its own AST and its own parser, and that is
//! the entire point: two backends give a differential oracle that finds
//! disagreements and is structurally blind to shared mistakes, because both
//! backends are fed by one front end. A reference implementation reusing that
//! front end would inherit its blindness and prove nothing.
//!
//! The rule is enforced, not merely stated — see
//! `tests/conformance_tests.rs::the_reference_implementation_is_independent`.
//!
//! ## Scope
//!
//! The core of docs/semantics.md §1. A program using anything outside it fails
//! to PARSE here, which is the desired outcome: better a loud refusal than a
//! quiet mis-evaluation of a construct nobody specified.

pub mod ast;
pub mod eval;
pub mod lex;
pub mod parse;

pub use eval::{Lang, Observation};

/// Lex, parse and evaluate a core ManiT program.
///
/// `Err` means the program is outside the specified core (or malformed); `Ok`
/// carries the observable behaviour of §4 — the output trace, and whether it
/// ended in a trap.
pub fn interpret(source: &str) -> Result<Observation, String> {
    interpret_with(source, Lang::default())
}

/// As [`interpret`], under an explicit language version (R2).
///
/// The only construct whose meaning differs is `/` and `%` on an integer —
/// C4 — because N5's other half was already true here: this account has always
/// held `int` to 27 trits, which is what made `docs/semantics.md` §10.1 a
/// divergence the LLVM backend had rather than a question the specification
/// had left open.
pub fn interpret_with(source: &str, lang: Lang) -> Result<Observation, String> {
    let toks = lex::lex(source)?;
    let program = parse::P::new(toks).program()?;
    eval::run_with(&program, lang)
}
