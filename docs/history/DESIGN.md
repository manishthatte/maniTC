<!-- MERGED FROM oss/unreleased/manitc ON 29 AUGUST 2026. The document below
     is unchanged from the original; everything above this line is the notice. -->

> **This is a HISTORICAL design record, dated by its content to the original
> maniT design session. It is kept because it is the record of what was
> intended, not because it describes maniTC as built.** Four of its claims were
> re-measured against `target/release/manitc` (`0c6133b887765551`, HEAD
> `79cf045`) on 29 August 2026 and are false today. They are listed here rather
> than edited out, because a design document that has been quietly corrected
> stops being a record.
>
> 1. **§3 says `int` is "Arbitrary precision ternary int / Unbounded". It is 27
>    trits.** `let a: int = 3812798742493; let b = a + 1;` traps at run time with
>    `int addition overflow: result 3812798742494 is outside the 27-trit range
>    [-3812798742493, 3812798742493]`. See `docs/semantics.md` §10.1 and
>    report.txt N5 / P79.
>
> 2. **§7.2's `async` / `await` is not implemented.** `async fn f(...)` parses
>    and type-checks; `await f(1)` is a parse error — `unexpected token in
>    expression: Await`. The declaration half of the design exists and the use
>    half does not.
>
> 3. **§7.1's `spawn` and channels DO work** — `let ch = channel<int>(); spawn
>    { ch.send(42); } ch.recv()` prints 42 on both backends — **but `spawn` runs
>    its block IN PLACE**, synchronously, rather than starting a task. See
>    report.txt P5 and P81.
>
> 4. **§12's `trit` build tool and `trit.toml` manifest do not exist.** There is
>    no such binary. `manitc` has seven subcommands: `compile`, `check`, `lex`,
>    `parse`, `run-t3`, `bench`, `lsp`.
>
> © Manish Jagdish Thatte

# maniT Language & Compiler — Design Document

> A balanced ternary, multi-paradigm programming language targeting both
> binary (x86-64) and ternary (Setun-style) hardware.

---

## 1. Design Session — Q&A Record

| # | Question | User Answer | Recommendation Chosen |
|---|----------|-------------|----------------------|
| 1 | Paradigm & logic values | Mix (most efficient); logic: -1, 0, +1 | Multi-paradigm (imperative + functional + OOP elements); ternary logic native |
| 2 | Syntax style | Most efficient | Rust-inspired: static, expressive, braces, type inference |
| 3 | Language name | **maniT** | maniT |
| 4 | Native ternary types | Recommend | Ternary-native primitives: `trit`, `tryte`, `word` + standard types |
| 5 | Trit notation | -1, 0, +1 | Display as `-`, `0`, `+` in code; stored as -1/0/+1 internally |
| 6 | Type system | Recommend | **Static typing with full inference** (like Rust/Go) |
| 7 | 3-way branching | Recommend | `tif / tunknown / telse` — native 3-way branch |
| 8 | Error handling | Recommend | `Result<T>` with 3 states: `Ok(T)`, `Unknown(hint)`, `Err(E)` |
| 9 | Concurrency | Yes, definitely | CSP channels + `async/await` + `spawn` (Go + Rust hybrid model) |
| 10 | Binary target | x86-64 (recommend) | x86-64 via **LLVM IR** backend (also unlocks ARM, WASM for free) |
| 11 | Ternary target | Setun-style (recommend) | Custom modernized **T3ISA** (27-trit word, 27 registers) |
| 12 | Output format | Recommend | Native binary (Option 1) or Ternary assembly/machine code (Option 2) |
| 13 | Standard library | Rich | Full stdlib: I/O, math, collections, net, sync, fs, time, fmt, ternary |
| 14 | Primary use case | Full balanced ternary ecosystem | Systems + applications; ternary-native efficiency throughout |
| 15 | Documentation | Document everything | This file + inline compiler docs |

---

## 2. Language Philosophy

Balanced ternary is mathematically superior to binary in several ways:
- **No two's complement**: negatives are natural (just negate each trit)
- **Efficient rounding**: the middle value (0) is equidistant from extremes
- **Three-valued logic**: `true / unknown / false` maps perfectly to +1/0/-1
- **Radix economy**: base 3 is the most efficient integer base (closest to `e`)

maniT is designed to expose and exploit these properties natively while
remaining practical for real-world use.

---

## 3. Primitive Types

| Type | Description | Range / Size |
|------|-------------|--------------|
| `trit` | Single balanced ternary digit | {-1, 0, +1} |
| `tryte` | 6 trits | -364 to +364 (729 values) |
| `t9` | 9 trits | -9841 to +9841 |
| `t27` | 27 trits (native word) | (3^27−1)/2 ≈ ±3.8×10^12 |
| `t54` | 54 trits (double word) | ±~3.4×10^25 |
| `int` | Arbitrary precision ternary int | Unbounded |
| `float` | Ternary floating point (27-trit) | High precision |
| `bool3` | Three-valued boolean | False(-1), Unknown(0), True(+1) |
| `char` | Unicode scalar value | U+0000 to U+10FFFF |
| `str` | UTF-8 string | Dynamic |
| `void` | No value | — |

### Trit Literals
```
let a: trit = -;    // -1
let b: trit = 0;    // 0
let c: trit = +;    // +1

let n: tryte = 0t+0-+0;   // balanced ternary literal prefix: 0t
let m: t27   = 0t++-00-+10;
```

---

## 4. Syntax

### 4.1 Variables & Inference
```manit
let x = 42;           // inferred as int
let y: tryte = 0t+0-; // explicit ternary type
mut z = 3.14;         // mutable, inferred float
```

### 4.2 Functions
```manit
fn add(a: t27, b: t27) -> t27 {
    return a + b;
}

// Short form
fn square(x: int) -> int => x * x;
```

### 4.3 Three-Way Branching (`tif`)
```manit
tif signal {
    + => { /* true / positive branch  */ }
    0 => { /* unknown / null branch   */ }
    - => { /* false / negative branch */ }
}

// Nested
tif sensor.read() {
    + => activate(),
    0 => wait(),
    - => shutdown(),
}
```

### 4.4 Standard `if` (binary-compatible)
```manit
if x > 0 {
    // ...
} elif x == 0 {
    // ...
} else {
    // ...
}
```

### 4.5 Pattern Matching
```manit
match value {
    0t+++ => "max tryte",
    0t--- => "min tryte",
    0t000 => "zero",
    _     => "other",
}
```

### 4.6 Loops
```manit
for i in 0..27 { }
while condition { }
loop { break; }           // infinite loop
for item in collection { }
```

### 4.7 Structs & Methods
```manit
struct Point {
    x: float,
    y: float,
}

impl Point {
    fn new(x: float, y: float) -> Point => Point { x, y };
    fn magnitude(self) -> float => sqrt(self.x*self.x + self.y*self.y);
}
```

### 4.8 Enums
```manit
enum Signal {
    Positive,
    Zero,
    Negative,
}
```

### 4.9 Traits (Interfaces)
```manit
trait Ternary {
    fn to_trits(self) -> [trit];
    fn from_trits(trits: [trit]) -> Self;
}
```

### 4.10 Generics
```manit
fn max<T: Ord>(a: T, b: T) -> T {
    if a > b { a } else { b }
}
```

---

## 5. Ternary Logic Operators

| Operator | Symbol | Description |
|----------|--------|-------------|
| ternary NOT | `tnot` / `~` | Negate: +→-, -→+, 0→0 |
| ternary AND | `tand` / `&` | min(a, b) |
| ternary OR | `tor` / `\|` | max(a, b) |
| ternary XOR | `txor` | Lukasiewicz t-norm |
| consensus | `tcon` | +1 if both +1, -1 if both -1, else 0 |
| any | `tany` | +1 if either +1, -1 if either -1, else 0 |

```manit
let a: trit = +;
let b: trit = -;
let c = a tand b;  // -
let d = a tor  b;  // +
let e = ~a;        // -
```

---

## 6. Error Handling

`Result<T>` has three variants — mapping naturally to ternary:

```manit
enum Result<T, E> {
    Ok(T),          // +1: success
    Unknown(str),   //  0: indeterminate / partial
    Err(E),         // -1: failure
}
```

```manit
fn read_sensor() -> Result<float, SensorError> {
    if !sensor.ready() { return Unknown("sensor warming up"); }
    // ...
}

// Propagation operator
let val = read_sensor()?;  // propagates Unknown or Err up

// Full handling
tresult read_sensor() {
    Ok(v)  => process(v),
    Unknown(hint) => log(hint),
    Err(e) => panic(e),
}
```

---

## 7. Concurrency

### 7.1 Spawn & Channels (CSP)
```manit
let ch = channel<int>();

spawn {
    ch.send(42);
}

let val = ch.recv();
```

### 7.2 Async / Await
```manit
async fn fetch(url: str) -> Result<str, NetError> {
    let resp = await http.get(url)?;
    Ok(resp.body)
}
```

### 7.3 Shared State
```manit
let counter = Mutex<int>::new(0);
spawn {
    counter.lock().deref_mut() += 1;
}
```

---

## 8. Standard Library Modules

| Module | Contents |
|--------|----------|
| `std::io` | stdin, stdout, stderr, print, println, read_line |
| `std::math` | Ternary-native arithmetic, trig, log, sqrt |
| `std::ternary` | Low-level trit ops, trit packing/unpacking, shift |
| `std::collections` | Vec, Map, Set, TernaryTrie, Deque, Heap |
| `std::str` | String slicing, formatting, search, regex |
| `std::net` | TCP/UDP sockets, HTTP client/server |
| `std::sync` | Mutex, RwLock, Channel, Barrier, Semaphore |
| `std::async` | Runtime, Task, Future, Executor |
| `std::fs` | File, Dir, Path, read, write, watch |
| `std::time` | Instant, Duration, sleep, timer |
| `std::fmt` | Formatters, display traits, ternary number formatting |
| `std::env` | Args, environment variables, OS info |
| `std::test` | Unit test framework, benchmarks |
| `std::ffi` | C interop (for binary target) |

---

## 9. Compiler Architecture

```
Source (.mt)
    │
    ▼
┌─────────┐
│  Lexer  │  → Tokens
└─────────┘
    │
    ▼
┌─────────┐
│ Parser  │  → AST (Abstract Syntax Tree)
└─────────┘
    │
    ▼
┌───────────────────┐
│ Semantic Analysis │  Type checking, name resolution, borrow check
└───────────────────┘
    │
    ▼
┌───────────────────┐
│  HIR (High IR)    │  Desugared, typed AST
└───────────────────┘
    │
    ▼
┌───────────────────┐
│  MIR (Mid IR)     │  Ternary-aware optimizations, SSA form
└───────────────────┘
    │
    ├──────────────────────────────────────────┐
    ▼                                          ▼
┌───────────────┐                   ┌────────────────────┐
│  LLVM Backend │                   │  T3ISA Backend     │
│  (Option 1)   │                   │  (Option 2)        │
└───────────────┘                   └────────────────────┘
    │                                          │
    ▼                                          ▼
x86-64 / ARM / WASM binary          Ternary Assembly (.t3s)
                                               │
                                               ▼
                                    T3ISA Machine Code (.t3b)
```

### Compiler Stages Detail

| Stage | Input | Output | Key Tasks |
|-------|-------|--------|-----------|
| Lexer | Source text | Token stream | Keywords, literals, operators |
| Parser | Tokens | AST | Grammar rules, syntax errors |
| Semantic | AST | Typed AST | Types, scopes, borrow checking |
| HIR Lower | Typed AST | HIR | Desugar syntax, explicit types |
| MIR Lower | HIR | MIR/SSA | Ternary opts, dead code elim |
| LLVM Codegen | MIR | LLVM IR | Binary emission |
| T3ISA Codegen | MIR | T3 Assembly | Ternary native emission |

---

## 10. T3ISA — Balanced Ternary ISA Specification

### Design Principles
- 27-trit word (natural 3^3 grouping)
- 27 general-purpose registers (R0–R26), R0 always 0
- Ternary-native arithmetic (no two's complement needed)
- Three-way branch as first-class instruction
- Inspired by Setun; modernized for pipelines

### Registers
```
R0       — always zero (reads 0, writes ignored)
R1–R25   — general purpose
R26      — stack pointer (SP)
PC       — program counter (not directly addressable)
FLAGS    — trit-flags register: {+, 0, -} per condition
```

### Instruction Encoding
All instructions are 27 trits wide (one word):
```
[op: 9 trits][dst: 5 trits][src1: 5 trits][src2: 5 trits][imm: 3 trits]
```

### Instruction Set

**Arithmetic**
| Mnemonic | Operation |
|----------|-----------|
| `TADD Rd, Rs1, Rs2` | Rd = Rs1 + Rs2 |
| `TSUB Rd, Rs1, Rs2` | Rd = Rs1 - Rs2 |
| `TMUL Rd, Rs1, Rs2` | Rd = Rs1 × Rs2 |
| `TDIV Rd, Rs1, Rs2` | Rd = Rs1 ÷ Rs2 |
| `TMOD Rd, Rs1, Rs2` | Rd = Rs1 mod Rs2 |
| `TNEG Rd, Rs` | Rd = -Rs (flip all trits) |

**Ternary Logic**
| Mnemonic | Operation |
|----------|-----------|
| `TAND Rd, Rs1, Rs2` | Rd = min(Rs1, Rs2) per-trit |
| `TOR  Rd, Rs1, Rs2` | Rd = max(Rs1, Rs2) per-trit |
| `TNOT Rd, Rs` | Rd = ~Rs (negate per-trit) |
| `TSHI Rd, Rs, n` | Shift Rs by n trit positions |
| `TCON Rd, Rs1, Rs2` | Consensus |
| `TANY Rd, Rs1, Rs2` | Any |

**Memory**
| Mnemonic | Operation |
|----------|-----------|
| `LOAD  Rd, [Rs+imm]` | Load word from memory |
| `STORE Rs, [Rd+imm]` | Store word to memory |
| `LOADT Rd, [Rs+imm]` | Load single trit |
| `STORET Rs, [Rd+imm]` | Store single trit |

**Control Flow**
| Mnemonic | Operation |
|----------|-----------|
| `TBRANCH Rs, L+, L0, L-` | Jump to L+/L0/L- based on sign of Rs |
| `JUMP label` | Unconditional jump |
| `CALL label` | Call subroutine (push PC) |
| `RET` | Return from subroutine |
| `HALT` | Stop execution |

**Special**
| Mnemonic | Operation |
|----------|-----------|
| `SYSCALL n` | System call n |
| `TLIT Rd, imm` | Load ternary immediate |
| `MOV Rd, Rs` | Register copy |
| `NOP` | No operation |

### Assembly Example
```t3asm
; Absolute value of R1 → R2
    TBRANCH R1, .positive, .zero, .negative
.positive:
    MOV R2, R1
    JUMP .done
.zero:
    TLIT R2, 0
    JUMP .done
.negative:
    TNEG R2, R1
.done:
    RET
```

---

## 11. File Extensions

| Extension | Purpose |
|-----------|---------|
| `.mt` | maniT source file |
| `.mti` | maniT interface / header |
| `.t3s` | T3ISA assembly source |
| `.t3b` | T3ISA binary (ternary machine code) |
| `.mtpkg` | maniT package archive |

---

## 12. Build Tool — `trit` (package manager / build system)

```bash
trit new myproject      # scaffold new project
trit build              # compile (auto-detects target)
trit build --target x86 # binary target
trit build --target t3  # ternary target
trit run                # build + run
trit test               # run tests
trit bench              # run benchmarks
trit add <package>      # add dependency
trit fmt                # format source
trit doc                # generate docs
```

### Project Layout
```
myproject/
├── trit.toml           # project manifest & dependencies
├── src/
│   ├── main.mt         # entry point
│   └── lib.mt          # library root
├── tests/
│   └── integration.mt
├── benches/
│   └── perf.mt
└── docs/
```

---

## 13. Hello World

```manit
use std::io;

fn main() {
    io::println("Hello from maniT!");

    let t: trit = +;
    tif t {
        + => io::println("Positive!"),
        0 => io::println("Unknown."),
        - => io::println("Negative!"),
    }
}
```

---

## 14. Implementation Plan

### Phase 1 — Foundation
- [ ] Lexer (tokenizer for maniT syntax)
- [ ] Parser (AST generation)
- [ ] Basic type system
- [ ] HIR lowering

### Phase 2 — Semantics
- [ ] Full type inference
- [ ] Borrow checker
- [ ] Error handling (`Result<T>`)
- [ ] Trait system

### Phase 3 — Codegen Option 1 (Binary)
- [ ] MIR lowering
- [ ] LLVM IR emission
- [ ] x86-64 binary output

### Phase 4 — Codegen Option 2 (Ternary)
- [ ] T3ISA assembler
- [ ] T3ISA emulator / simulator
- [ ] MIR → T3ISA codegen

### Phase 5 — Standard Library
- [ ] std::io, std::math, std::ternary
- [ ] std::collections
- [ ] std::net, std::sync, std::async
- [ ] std::fs, std::time, std::fmt

### Phase 6 — Tooling
- [ ] `trit` build tool
- [ ] LSP language server
- [ ] Formatter
- [ ] Documentation generator

---

## 15. Efficiency Rationale

Why balanced ternary is more efficient:

| Property | Binary | Balanced Ternary |
|----------|--------|-----------------|
| Radix economy | 2 × ln2 ≈ 1.386 | 3 × (1/ln3) ≈ 1.366 (optimal) |
| Sign representation | Two's complement (hack) | Native (negate trits) |
| Rounding | Biased (round half-up) | Natural (round to nearest) |
| Three-state logic | Requires extra bits | Native trit |
| Division remainder | Always non-negative | Symmetric around 0 |

---

*Document version: 0.1.0 — Created during initial design session.*
*Language: maniT | Compiler: manitc | Build tool: trit*
