# Getting Started with ManiT and thatteOS

*The handbook for your first hour with the balanced ternary stack.*

ManiT is a systems language where **three** is the native radix: trits
(`+`, `0`, `-`) instead of bits, three-valued logic instead of Boolean,
three-way branches instead of if/else. This guide takes you from nothing to
running programs on both backends — your own machine (LLVM) and the
cycle-accurate ternary emulator (T3ISA) — and then to booting the thatteOS
shell.

## 0. Prerequisites

- **Rust 1.70+** with Cargo — `curl https://sh.rustup.rs -sSf | sh`
- **clang** (any recent version; scripts default to `clang-19`) — only
  needed for native execution and thatteOS
- Linux (the reference platform)

## 1. Build the compiler

```sh
git clone https://github.com/manishthatte/maniTC
cd maniTC
cargo build --release
export PATH="$PATH:$(pwd)/target/release"
```

You now have `manitc`. Check it:

```sh
manitc --help
```

## 2. Your first program — on ternary hardware (emulated)

Create `hello.mt`:

```manit
use std::io;

fn main() {
    io::println("Hello from the ternary world!");
}
```

Compile to **T3ISA** — the balanced ternary instruction set — and run it on
the cycle-accurate emulator:

```sh
manitc compile --target t3 hello.mt
manitc run-t3 a.t3b
```

That output was produced by a program whose every integer, address, and
branch was balanced ternary. Add `--debug` to `run-t3` for the interactive
debugger (stepping, breakpoints, register inspection).

## 3. The same program, natively

```sh
manitc compile --target llvm hello.mt -o hello.ll
clang-19 -O2 -c runtime/manit_runtime.c -o manit_runtime.o
clang-19 hello.ll manit_runtime.o -o hello -lm
./hello
```

One language, two worlds. `manitc bench examples/fibonacci.mt` compiles a
program for both backends and compares them.

## 4. Meet the trits

```manit
use std::io;

fn main() {
    let a: trit = +;              // a trit literal: +, 0, or -

    tif a {                       // the three-way branch
        + => io::println("positive"),
        0 => io::println("zero"),
        - => io::println("negative"),
    }

    let sensor: bool3 = unknown;  // three-valued logic: true / unknown / false
    tif sensor {
        + => io::println("sensor OK"),
        0 => io::println("uncertain"),
        - => io::println("fault"),
    }

    let n = 0t+0-;                // balanced ternary literal: (+1)*9 + 0*3 + (-1)*1 = 8
    io::print_int(n);
    io::newline();
}
```

Three ideas to absorb:

1. **Negation is free.** Balanced ternary numbers are symmetric — negating
   flips each trit. There is no two's complement, no sign bit, no unsigned.
2. **`unknown` is a value, not an error.** `bool3` follows Kleene's
   three-valued logic; `Result` has `Ok` / `Err` / `Unknown` arms.
3. **`tif` compiles to ONE instruction** on T3ISA. Three-way comparison is
   what the hardware does natively.

## 5. Explore from here

| Want to… | Go to |
|----------|-------|
| Learn the language properly | [docs/language-reference.md](docs/language-reference.md) |
| See what the stdlib offers (18 modules) | [docs/stdlib-reference.md](docs/stdlib-reference.md) |
| Read 17 worked examples | [examples/](examples/) + [docs/examples.md](docs/examples.md) |
| Understand the instruction set | [docs/t3isa-reference.md](docs/t3isa-reference.md) |
| Hack on the compiler itself | [docs/compiler-internals.md](docs/compiler-internals.md) + [docs/howto/](docs/howto/) |
| Editor support (LSP) | `manitc lsp` — a Language Server over stdio |

Good example progression: `hello.mt` → `ternary_demo.mt` →
`three_valued_logic.mt` → `fibonacci.mt` → `data_structures.mt` →
`neural_net.mt` (a ternary neural network).

## 6. Boot thatteOS

The companion repository is a microkernel OS written entirely in ManiT:

```sh
cd ..
git clone https://github.com/manishthatte/thatteOS
cd thatteOS
bash build.sh        # finds ../maniTC automatically
./thatteos           # the thatteOS interactive shell
```

Continue in
[thatteos/GETTING_STARTED.md](https://github.com/manishthatte/thatteOS/blob/main/GETTING_STARTED.md).

## 7. Questions?

Open a [GitHub Discussion](https://github.com/manishthatte/maniTC/discussions).
Contributions welcome — read [CONTRIBUTING.md](CONTRIBUTING.md) (one-line CLA
required).

---

Authored by **Manish Jagdish Thatte** · manish@manitlab.org · [manitlab.org](https://www.manitlab.org)

© Manish Jagdish Thatte, 2026
