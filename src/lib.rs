pub mod ast;
pub mod error;
pub mod lang;
pub mod lint;
pub mod lexer;
pub mod parser;
pub mod semantic;
pub mod ir;
pub mod borrow;
pub mod codegen_llvm;
pub mod codegen_t3;
pub mod lsp;
pub mod runtime_link;
/// A3: the reference interpreter. Structurally independent of everything above
/// it — see src/reference/mod.rs for the rule and why it matters.
pub mod reference;

// ---------------------------------------------------------------------------
// Stack reservation (A3, F-8)
// ---------------------------------------------------------------------------

/// Stack reserved for a compiler pass.
///
/// Every pass after the lexer — parser, semantic analyzer, IR lowering, both
/// emitters — recurses over the expression tree. `MAX_PARSE_DEPTH` refuses
/// input nested past 256, but that limit is only ENFORCEABLE if the stack is
/// deep enough to reach it: on a default thread stack the process aborts at a
/// shallower depth, before the guard ever runs. This is a virtual reservation;
/// only pages actually touched are committed.
pub const COMPILER_STACK_BYTES: usize = 256 * 1024 * 1024;

/// Run a compiler pass on a thread with enough stack to reach the parser's
/// depth limit.
///
/// The reservation used to live in `main`, which meant the guarantee belonged
/// to the BINARY and not to the library: any other embedder — the language
/// server on a tokio worker, a test harness, a fuzz target — got the default
/// stack and therefore a process abort instead of the diagnostic the guard
/// exists to produce. Found by the F-8 corpus harness, which is a library
/// consumer and aborted on 2048-deep nesting that the CLI rejects cleanly.
///
/// Panics inside `f` propagate, so this is transparent to callers that catch
/// or report them.
pub fn with_compiler_stack<T, F>(f: F) -> T
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    std::thread::Builder::new()
        .name("manitc-pass".to_string())
        .stack_size(COMPILER_STACK_BYTES)
        .spawn(f)
        .expect("failed to spawn the compiler thread")
        .join()
        .unwrap_or_else(|e| std::panic::resume_unwind(e))
}
