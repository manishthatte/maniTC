# manitc — the ManiT compiler

**ManiT** is a systems programming language in which balanced ternary is the
native number system: integers are signed by construction, logic is
three-valued (`True` / `Unknown` / `False`), and the fundamental control-flow
construct is a three-way branch. **manitc** is its compiler — written in Rust,
with two backends:

- **LLVM IR** — compile and run ManiT programs natively on your machine today
- **T3ISA** — a balanced ternary instruction set, with an assembler and a
  cycle-accurate emulator included, targeting photonic-ternary hardware
  (The THATTE Device)

This is not a binary language with ternary bolted on. `t27` words, trytes and
trits, Kleene three-valued logic, `tif` three-way branching, balanced ternary
floating point (T27F), and a ternary-native standard library (12 modules) are
the ground floor.

## Quick start

**New here? Read [GETTING_STARTED.md](GETTING_STARTED.md)** — the full
handbook from install to booting THATTEOS.

```sh
cargo build --release

# compile to T3ISA and run on the cycle-accurate ternary emulator
./target/release/manitc compile --target t3 examples/ternary_demo.mt -o demo.t3b
./target/release/manitc run-t3 demo.t3b

# native execution via the LLVM backend (needs clang):
./target/release/manitc compile examples/hello.mt -o hello.ll
clang-19 -O2 -c runtime/manit_runtime.c -o manit_runtime.o
clang-19 hello.ll manit_runtime.o -o hello -lm && ./hello

# or compare both backends on one program
./target/release/manitc bench examples/fibonacci.mt
```

Seventeen example programs live in `examples/` — from `fibonacci.mt` to a
ternary neural network. Start with [GETTING_STARTED.md](GETTING_STARTED.md), then the
[language reference](docs/language-reference.md), the
[stdlib reference](docs/stdlib-reference.md), and the
[T3ISA reference](docs/t3isa-reference.md).

The companion repository [thatteos](https://github.com/manishthatte/thatteos)
is a microkernel operating system written entirely in ManiT.

## License

- **AGPL-3.0** ([LICENSE](LICENSE)) with the
  **[ManiT Runtime Library Exception](COPYING.RUNTIME-EXCEPTION)**:
  programs you write in ManiT and compile with manitc are **yours, under any
  license** — the copyleft covers only the compiler, runtime, and stdlib
  themselves.
- **Commercial licenses** for proprietary derivatives of the compiler/runtime
  are available: manish@manitlab.org

Contributions require the one-line CLA — see [CONTRIBUTING.md](CONTRIBUTING.md).

## Patents

The photonic-ternary hardware this language ultimately targets is covered by
twelve patent applications filed with the Indian Patent Office (2026), sole
inventor Manish Jagdish Thatte — see [NOTICE](NOTICE). The AGPL's patent
grant (§11) applies to this software as released; hardware implementations
require a separate license.

---

Authored by **Manish Jagdish Thatte** · manish@manitlab.org · [manitlab.org](https://www.manitlab.org)

© Manish Jagdish Thatte, 2026
