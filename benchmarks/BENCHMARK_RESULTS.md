# maniTC Benchmark Results: T3ISA (ternary) vs LLVM/x86 (binary)

**Date:** 11 August 2026
**Compiler version:** maniTC 0.1.0, at the commit that introduced arithmetic
overflow traps and bounded lp-string reads
**Hardware:** AMD EPYC 9334, 251 GB DDR5
**Method:** `manitc bench <file> --iterations 5`

> **This file replaces the 12 April 2026 results, which should not be cited.**
> That table published static x86 instruction counts (`3,585`) and LLVM wall
> times (`3 ms`) that the harness does not collect and that do not reproduce.
> It also carried a "LLVM trit codegen incomplete" caveat that is no longer
> true — all three benchmarks now compile and run on both backends. The code
> sizes have moved too, as a consequence of the August codegen fixes.

---

## What the harness does and does not measure

Stated up front, because the previous revision of this file quoted numbers it
could not produce.

| Measured | Not measured |
|---|---|
| T3ISA static code size, in words and trits | LLVM/x86 **executed** instruction counts |
| T3ISA executed instruction counts and opcode mix | LLVM/x86 wall time |
| Assembly lines emitted for **both** backends | Anything about real ternary hardware |
| T3ISA emulator wall time | |

`N/A` in the LLVM column below means "the harness does not collect this", not
"the backend failed".

---

## Static code size

Both backends emit assembly, so this is a like-for-like comparison and the
most meaningful number here.

| Benchmark | T3ISA asm lines | LLVM asm lines | Ratio | T3ISA words | T3ISA trits |
|---|---:|---:|---:|---:|---:|
| 01_arithmetic | 596 | 1,322 | 2.2× | 546 | 14,742 |
| 02_ternary_native | 1,004 | 1,592 | 1.6× | 930 | 25,110 |
| 03_control_flow | 745 | 1,357 | 1.8× | 696 | 18,792 |

T3ISA emits **1.6–2.2× fewer assembly lines** for the same program.

Information density, for context rather than as a performance claim:

- Binary: 1.000 bit per digit; balanced ternary: log₂3 = **1.585** bits per digit
- A 27-trit word carries **42.8 bits**; a 64-bit word carries 64.0 bits
- The ternary word uses 27 digits against 64 for roughly two-thirds of the
  capacity — denser per digit, smaller in total

---

## Execution profile (T3ISA emulator, average of 5 runs)

| Benchmark | Instructions executed | Wall time | Ternary-native | Cond. branches † | Max call depth |
|---|---:|---:|---:|---:|---:|
| 01_arithmetic | 10,000,000 ⚠ | 88.7 ms | 724,036 (7.2%) | 1,011,725 | 20 |
| 02_ternary_native | 2,315,774 | 20.5 ms | 200,739 (8.7%) | 219,607 | 2 |
| 03_control_flow | 1,999,366 | 17.5 ms | 145,388 (7.3%) | 199,472 | 127 |

† This column counts executed `TBR_POS` / `TBR_ZERO` / `TBR_NEG` instructions,
which is what `bench.rs` sums. It is **not** a count of three-way branches:
`TBRANCH` is a pseudo-instruction the assembler expands into two of these plus a
`JUMP`, so the number of three-way dispatches is roughly half the figure shown.
Earlier revisions of this file labelled it "3-way branches" and then multiplied
it by four to estimate a binary equivalent, which double-counted twice over.

⚠ **01_arithmetic does not run to completion.** It stops on `TRAP: step limit
exceeded` at the emulator's 10,000,000-instruction ceiling, so that figure is
the limit, not the program's work. The LLVM build of the same source finishes
normally. Quote this row only with the caveat attached.

### Opcode mix

| | 01_arithmetic | 02_ternary_native | 03_control_flow |
|---|---:|---:|---:|
| Arithmetic | 23.5% | 23.4% | 23.6% |
| Ternary-native | 7.2% | 8.7% | 7.3% |
| Control flow | 17.8% | 14.8% | 17.1% |
| Memory | 27.2% | 23.6% | 23.9% |

Top opcodes are consistently `LOAD`, `TLIT`, `TSUB`, `TMAX`, `TCMP`, `TBRPOS`
and `JUMP` across all three.

---

## Cross-backend output equivalence

The strongest correctness result available without hardware: compile the same
source to both targets, run both, compare output byte for byte.

**18 of 20 programs (17 examples + 3 benchmarks) are byte-identical.**

The two that are not:

| Program | Divergence | Cause |
|---|---|---|
| 01_arithmetic | 5 lines | T3 hits the 10M step limit and traps; LLVM completes. A resource limit, not a semantic difference — and the trap is the correct behaviour, since the alternative is silent truncation |
| data_structures | 11 lines | `Map<str,int>` section only. `[str]` array elements do not survive a loop iteration whose body allocates on T3 — arrays are stack-allocated where structs were moved to the heap. `KNOWN_ISSUES.md` issue 5 |

This comparison is what found the two defects fixed on 11 August, neither of
which any existing test caught:

- **Arithmetic saturated silently.** `fib_safe(70)` returned
  `Ok(3812798742493)` — T3_MAX — on T3, against `Ok(190392490709135)` on LLVM.
  The golden file had recorded the wrong answer *as the expected output*, which
  is why the suite was green over it. Overflow now traps.
- **`read_lp_string` fabricated data.** An untrusted length word turned one bad
  address into **7.7 GB** of NUL bytes, and the run still exited 0. Bounded and
  validated; that program's output is now 2,184 bytes.

`cargo test`: 313 passed, 0 failed.

---

## Findings

**1. Three-way branching.** Comparison naturally produces three outcomes in
balanced ternary, so `TBRANCH` matches the question being asked, and the
three-armed structure survives from source to assembly without being decomposed
into two-way tests and imperfectly reconstructed. What it does *not* yet buy is
a cycle: `TBRANCH` is a pseudo-instruction that the assembler expands into three
machine words, against the two a binary compare-and-branch pair needs. The
saving is one line of assembly and an intact control-flow shape, not fewer
executed instructions. No binary-equivalent instruction count is estimated here;
the previous revision's `×4` figure was arithmetic on a mislabelled column.

**2. Ternary-native operations.** 7.2–8.7% of executed instructions are ops
with no binary equivalent — `TAND`, `TOR`, `TNOT`, `TMIN`, `TMAX`, `TSHI`,
`TSHR`. On ternary hardware each is a single cycle; on binary hardware each
needs a compare-and-select sequence to emulate.

**3. Code density.** 1.6–2.2× fewer assembly lines, a direct consequence of the
27-trit word and three-valued encoding.

**4. Wall time here means nothing.** The T3ISA "machine" is a software
interpreter running on binary hardware. Its wall times measure interpretation
overhead, not architecture. **They are not a ternary-versus-binary result and
must never be quoted as one.** The comparable quantities are static code size
and executed instruction counts.

---

## Benchmark programs

- `01_arithmetic.mt` — recursive and iterative Fibonacci, Collatz, divisor sums, GCD, integer power
- `02_ternary_native.mt` — trit majority vote, balanced ternary weight, trit counting, ternary search, three-way classification
- `03_control_flow.mt` — Ackermann, three-way sort, prime sieve, matrix multiply, nested loops

## Reproducing

```sh
cargo build --release
./target/release/manitc bench benchmarks/01_arithmetic.mt --iterations 5
```

Cross-backend comparison, per program:

```sh
manitc compile --target t3   -o out.t3b prog.mt && manitc run-t3 out.t3b | grep -v '^\[T3ISA\]'
manitc compile --target llvm -o out.bin prog.mt && ./out.bin
```

Authored by: Manish Jagdish Thatte
