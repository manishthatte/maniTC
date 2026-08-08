# maniT Documentation

maniT is a balanced ternary programming language with a complete compiler toolchain,
targeting both native x86-64 (via LLVM) and a simulated balanced ternary processor
(T3ISA). The language is designed around three-valued logic: every value can be
positive (+1), zero (0), or negative (−1) — not just true or false.

## Documents in this directory

| File | What it covers |
|------|---------------|
| [language-reference.md](language-reference.md) | Complete language specification: types, expressions, statements, control flow, traits, generics |
| [compiler-internals.md](compiler-internals.md) | Every source file, module, and data structure in the compiler |
| [t3isa-reference.md](t3isa-reference.md) | T3ISA architecture, instruction set, encoding, and emulator |
| [stdlib-reference.md](stdlib-reference.md) | Standard library modules and their functions |
| [examples.md](examples.md) | Annotated walkthrough of all seven example programs |
| [howto/getting-started.md](howto/getting-started.md) | Build the toolchain from source, write and run your first program |
| [howto/trit-tool.md](howto/trit-tool.md) | trit build tool: all commands, trit.toml format, CI integration |
| [howto/adding-a-feature.md](howto/adding-a-feature.md) | Step-by-step guide for extending the compiler |
| [howto/debugging.md](howto/debugging.md) | Diagnose and fix compile errors, assembler failures, emulator issues |

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
   ↓  ┌── LLVM backend (src/codegen_llvm.rs) → a.ll → clang → a.out
      └── T3 backend   (src/codegen_t3/)    → a.t3s → assembler → a.t3b → emulator
```

## Building the project

```bash
cargo build                          # build compiler + trit tool
./target/debug/manitc compile --target t3 examples/hello.mt
./target/debug/manitc run-t3 a.t3b
```
