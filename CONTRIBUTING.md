# Contributing

Thank you for your interest in the ManiT stack. Contributions are welcome —
bug reports, fixes, stdlib modules, examples, docs, ports.

## Ground rules

1. **CLA required.** Every contribution needs the one-line CLA agreement —
   see [CLA.md](CLA.md). Add this line to your PR description:

       I have read and agree to CLA.md (ManiT Individual CLA v1.0)

   PRs without it cannot be merged, however good the code. (This is what
   keeps the project's dual licensing — free AGPL + commercial — legally
   sound.)

2. **License.** The compiler and runtime are AGPL-3.0; the runtime and
   stdlib additionally carry the
   [ManiT Runtime Library Exception](COPYING.RUNTIME-EXCEPTION), so
   programs *you compile* are yours under any license.

3. **Discussions first for big changes.** Open a GitHub Discussion or
   issue before large features — balanced ternary has design principles
   (three-valued logic, signed arithmetic, no binary idioms bolted on) and
   early alignment saves everyone time.

4. **Style.** Match the surrounding code. Rust code: `cargo fmt` clean.
   ManiT code: follow the stdlib's conventions.

5. **Tests.** New behavior needs a test under `tests/`. Run the suite
   before submitting.

## Getting started

    cargo build --release
    ./target/release/manitc compile --target llvm examples/hello.mt -o hello.ll

See `docs/` for the language reference, compiler internals, and the T3ISA
reference.

© Manish Jagdish Thatte, 2026
