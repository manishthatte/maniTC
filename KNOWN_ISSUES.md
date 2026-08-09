# Known issues

Current, honest state of the two backends. Everything listed here is a real
gap, reproducible from a clean checkout. CI runs the working set on every
push, so anything that works today cannot silently regress; this file is the
list of what does not work yet.

Last measured: 9 August 2026, against the initial public release.

## Example programs

The T3ISA backend compiles and runs 11 of the 17 examples. The LLVM backend
compiles and runs 3.

| Example | T3ISA | LLVM |
|---|---|---|
| capability_demo | works | build fails |
| database | works | build fails |
| data_structures | works | build fails |
| fibonacci | works | works |
| hello | works | build fails |
| neural_net | works | build fails |
| stream_demo | works | works |
| ternary_calculator | works | build fails |
| ternary_demo | works | build fails |
| ternary_sort | works | build fails |
| three_valued_logic | works | build fails |
| oop | assembler error | works |
| bridge_demo | parse error | build fails |
| concurrency | build fails | build fails |
| crypto_demo | build fails | build fails |
| float_demo | build fails | build fails |
| patent_classify | build fails | build fails |

## Open defects behind those failures

**LLVM backend emits invalid IR for several constructs.** clang rejects the
generated module rather than the compiler reporting an error itself. Observed
so far:

- undefined runtime symbol `@io_println_bool3` is called but never declared
  (`examples/hello.mt`)
- a value typed `i64` used where `ptr` is expected (`examples/ternary_demo.mt`)
- a returned value that does not match the function's declared result type
  (`examples/three_valued_logic.mt`)

**Front-end gaps.**

- `std::t27f` is not resolvable as a standard library module, so balanced
  ternary floating point cannot be imported (`examples/float_demo.mt`)
- array literal syntax fails to parse in `examples/bridge_demo.mt:21`

**T3ISA assembler.**

- unresolved symbol `float_Point::zero_0` when a struct associated function is
  generic over a float type (`examples/oop.mt`)

**Result / pattern binding.** `Ok(v)`, `Err(e)` and `Unknown(m)` bindings in
`match` arms are reported as unknown identifiers by the semantic pass, and the
error payload prints as `(null)` at runtime (`examples/fibonacci.mt`,
`examples/hello.mt`).

**Exit status.** Several examples return a nonzero exit status after running
correctly to completion; `main`'s return value is not being translated into a
process status.

**SIGPIPE.** `manitc run-t3 | head` panics with "failed printing to stdout:
Broken pipe" when the reader closes early, instead of exiting quietly the way
a command line tool should.

## Runtime linking

The LLVM backend links compiled programs against the ManiT C runtime. With the
SDL2 and libcurl development packages installed, the full runtime is built;
without them the compiler probes pkg-config and falls back to the minimal
runtime (`-DMANIT_NO_GUI`), which drops the `gui` and `net` modules and keeps
everything else. `MANIT_NO_GUI=1` forces the minimal build.

Programs using `gui` or `net` therefore need SDL2 and libcurl present at
compile time. There is no diagnostic yet when they are used without it — the
link simply fails on undefined symbols.

---

© Manish Jagdish Thatte
