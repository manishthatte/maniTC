# maniT Documentation

maniT is a balanced ternary programming language with a complete compiler toolchain,
targeting both native x86-64 (via LLVM) and a simulated balanced ternary processor
(T3ISA). The language is designed around three-valued logic: every value can be
positive (+1), zero (0), or negative (−1) — not just true or false.

## Documents in this directory

| File | What it covers |
|------|---------------|
| [semantics.md](semantics.md) | **Normative** operational semantics of the core language. Where this and an implementation disagree, the implementation is wrong |
| [language-reference.md](language-reference.md) | Complete language specification: types, expressions, statements, control flow, traits, generics |
| [compiler-internals.md](compiler-internals.md) | The compiler stage by stage: the pipeline, its data structures, and the principal module of each stage |
| [t3isa-reference.md](t3isa-reference.md) | T3ISA architecture, instruction set, encoding, and emulator |
| [stdlib-reference.md](stdlib-reference.md) | Standard library modules and their functions |
| [memory-model.md](memory-model.md) | Memory and concurrency model; §5 carries the dated scheduling decision |
| [examples.md](examples.md) | Annotated walkthrough of seven of the seventeen example programs |
| [howto/getting-started.md](howto/getting-started.md) | Build the toolchain from source, write and run your first program |
| [howto/trit-tool.md](howto/trit-tool.md) | trit build tool: all commands, trit.toml format, CI integration |
| [howto/adding-a-feature.md](howto/adding-a-feature.md) | Step-by-step guide for extending the compiler |
| [howto/debugging.md](howto/debugging.md) | Diagnose and fix compile errors, assembler failures, emulator issues |
| [history/README.md](history/README.md) | Superseded documents, kept as record — each with a dated notice naming the claims since measured false |

> **Corrected 1 September 2026.** The table above is headed *"Documents in
> this directory"* and listed nine of the fifteen `.md` files under `docs/`.
> The omissions included **`semantics.md`, the normative specification** — the
> document that, by the repository's own rule, wins against any implementation
> disagreeing with it — and `memory-model.md`, which carries the dated
> scheduling decision. A heading that asserts *this directory* is a claim about
> the directory, and it is now pinned by
> `tests/audit_regression_tests.rs::the_documentation_index_lists_every_document`.
> The pipeline diagram below also named `src/codegen_llvm.rs`, a directory
> since the initial public release. The `compiler-internals.md` row also read
> *"Every source file, module, and data structure in the compiler"*: that
> document names sixteen `.rs` files in its headings (twenty-five entries in all,
> counting directories) against sixty-six `.rs` files under `src/`, so the
> row is now worded for what it delivers. **This is the same defect as the
> "all seven" corrected in the `examples.md` row on 30 August** — an index
> billing a document as complete when it covers a subset — found at a second
> site only because the whole table was measured rather than the one row that
> had been reported.

## Quick orientation

```
source.mt
   ↓  Lexer           (src/lexer.rs)
tokens
   ↓  Parser          (src/parser/)
AST (Program)
   ↓  SemanticAnalyzer (src/semantic/)
TypedProgram
   ↓  IRLowerer        (src/ir/)
IRModule
   ↓  ┌── LLVM backend (src/codegen_llvm/)   → a.ll → clang → a.out
      └── T3 backend   (src/codegen_t3/)    → a.t3s → assembler → a.t3b → emulator
```

## Building the project

```bash
cargo build                          # build compiler + trit tool
./target/debug/manitc compile --target t3 examples/hello.mt
./target/debug/manitc run-t3 a.t3b
```
