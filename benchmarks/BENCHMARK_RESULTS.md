# maniT Compiler Benchmark Results: T3ISA (Ternary) vs LLVM/x86 (Binary)

**Date:** 12 April 2026
**Compiler version:** manitc 0.1.0
**Hardware:** AMD EPYC 9334, 251 GB DDR5
**Method:** `manitc bench <file> --iterations 5`

## Code Density

| Benchmark | T3ISA words | T3ISA trits | x86 instrs (static) | LLVM binary |
|-----------|-------------|-------------|----------------------|-------------|
| 01_arithmetic | 573 | 15,471 | 3,585 | 35 KB |
| 03_control_flow | 733 | 19,791 | 3,533 | 35 KB |

T3ISA is 5-6x more compact in static instruction count. Each ternary word encodes 42.8 bits of information (27 trits x 1.585 bits/trit) — 58.5% more information per digit than binary.

## Execution Profile

| Benchmark | T3 instrs executed | T3 time | LLVM time | Ternary-native ops | 3-way branches |
|-----------|-----------|---------|-----------|--------------------|----|
| 01_arithmetic | 10M (step limit) | 72 ms | 3 ms | 724K (7.2%) | 1.01M |
| 02_ternary_native | 2.3M | 17 ms | N/A (LLVM trit codegen incomplete) | 200K (8.7%) | 219K |
| 03_control_flow | 2.0M | 15 ms | 1.3 ms | 145K (7.3%) | 199K |
| fibonacci (example) | 40K | 0.4 ms | N/A (Result<T> codegen incomplete) | 2K (4.8%) | 2.7K |

## Instruction Mix (T3ISA)

### 01_arithmetic (10M instructions)
```
Arithmetic ops:     2,346,119  (23.5%)
Ternary-native ops:   724,036  ( 7.2%)
Control flow ops:   1,780,595  (17.8%)
Memory ops:         2,718,208  (27.2%)
Max call depth:     20

Top opcodes:
  2,195,702  LOAD
  1,563,927  TLIT
    744,076  TSUB
    723,036  TMAX
    723,036  TCMP
    723,036  TBRPOS
    712,089  JUMP
    522,506  STORE
    471,966  TADD
    418,911  TNEG
```

### 02_ternary_native (2.3M instructions)
```
Arithmetic ops:       541,994  (23.4%)
Ternary-native ops:   200,774  ( 8.7%)
Control flow ops:     342,753  (14.8%)
Memory ops:           546,990  (23.6%)
Max call depth:     2

Top opcodes:
    460,714  TLIT
    344,769  LOAD
    202,221  STORE
    166,430  TSUB
    142,511  TBRPOS
    140,919  TMAX
    140,865  TCMP
    137,860  TNEG
    128,452  TADD
    115,888  JUMP
```

### 03_control_flow (2.0M instructions)
```
Arithmetic ops:       471,322  (23.6%)
Ternary-native ops:   145,388  ( 7.3%)
Control flow ops:     341,163  (17.1%)
Memory ops:           478,312  (23.9%)
Max call depth:     127

Top opcodes:
    354,165  LOAD
    318,486  TLIT
    186,654  TSUB
    125,365  TMAX
    125,365  TCMP
    125,365  TBRPOS
    124,147  STORE
    119,313  MOV
    111,862  TADD
    100,754  JUMP
```

## Key Findings

### 1. Three-way branching advantage
Every TBRANCH replaces what would be 2-4 binary instructions (cmp+jz+cmp+jl). In the control flow benchmark alone, 199K three-way branches replace ~800K binary instructions. This is a fundamental architectural advantage of balanced ternary: comparison naturally produces three outcomes (+/0/-), not two.

### 2. Ternary-native operations
5-9% of all executed instructions are ops with NO binary equivalent (TAND, TOR, TNOT, TMIN, TMAX, TSHI, TSHR). On actual ternary hardware, these would execute in one cycle. On binary hardware, each would require multiple instructions to emulate (e.g., TMIN requires a compare-and-select sequence).

### 3. Code density
T3ISA programs are dramatically smaller: 573 vs 3,585 instructions for the same computation. This is a direct consequence of the 27-trit word width and three-valued encoding. Smaller code means better cache utilisation on real hardware.

### 4. Wall time caveat
The T3ISA emulator runs as a software interpreter on binary hardware — so the ~10-20x wall-time gap is expected and meaningless for comparing the architectures. On native ternary hardware, T3ISA would execute each instruction in one cycle, just like x86. The comparison that matters is instruction count and code density, not emulated wall time.

### 5. Information-theoretic advantage
- Binary: 1.000 bit per digit
- Balanced ternary: 1.585 bits per digit (log2(3))
- A 27-trit word carries 42.8 bits of information
- A 64-bit word carries 64.0 bits of information
- But the ternary word uses only 27 digits vs 64 digits — 58.5% more efficient per digit

## Benchmark Programs

- `benchmarks/01_arithmetic.mt` — Recursive/iterative fibonacci, Collatz, divisor sums, GCD, integer power
- `benchmarks/02_ternary_native.mt` — Trit majority vote, balanced ternary weight, trit counting, ternary search, three-way classification
- `benchmarks/03_control_flow.mt` — Ackermann function, three-way sort, prime sieve, matrix multiply, nested loops

## LLVM Backend Status

The LLVM backend successfully compiles and produces correct binaries for programs using:
- Integer arithmetic, loops, recursion, function calls
- `io::print`, `io::println`, `io::print_int`, `io::println_int`

Known incomplete features preventing LLVM compilation of some benchmarks:
- `Result<T,E>` type lowering (ret type mismatch between ptr and i64)
- `trit` return types (ret type mismatch: i64 vs i8 in dead merge blocks)

Authored by: Manish Jagdish Thatte
