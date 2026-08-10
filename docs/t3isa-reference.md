# T3ISA Reference

T3ISA (Ternary Three-address Instruction Set Architecture) is the virtual machine
used as the backend for maniT's balanced ternary compilation target. This document
specifies the architecture, instruction set, encoding, assembly syntax, and emulator
behaviour.

**Specification version 1.3** — tagged `t3isa-spec-v1.3` in this repository.
This document is the normative definition of T3ISA; independent implementations
should cite the tagged version they were written against. Where this document
and the manitc emulator disagree, that is a specification bug — please report it.

Changes since 1.2, all corrections to sections that described the emulator
inaccurately rather than changes to the architecture:

- **§3 Memory model** now gives the actual layout. The initial SP is 60,000, not
  65,535, and the heap base is 63,000, not "~50000".
- **§8 String / fmt syscalls** were listed at 140–143. No handler has ever
  existed at those numbers. The real ones are 14, 15, 127, 129, 130 and 132, and
  only 9 of the 31 declared `fmt` natives are implemented on T3 at all.
- **§8** documents syscall 218, `heap_alloc_words`, which struct allocations now
  use.

---

## Table of contents

1. [Architecture overview](#1-architecture-overview)
2. [Register file](#2-register-file)
3. [Memory model](#3-memory-model)
4. [Word encoding](#4-word-encoding)
5. [Instruction set](#5-instruction-set)
6. [Assembly syntax](#6-assembly-syntax)
7. [Calling convention](#7-calling-convention)
8. [Syscall table](#8-syscall-table)
9. [Binary file format](#9-binary-file-format)
10. [Emulator behaviour](#10-emulator-behaviour)

---

## 1. Architecture overview

T3ISA is a load/store register machine with a 27-trit word size.

| Property | Value |
|----------|-------|
| Word width | 27 trits (stored as `i64`) |
| Word range | −3,812,798,742,493 … +3,812,798,742,493 |
| Registers | 27 (R0–R26) |
| FLAGS | 1 trit: −1, 0, +1 |
| Address space | 65,536 word-addressable cells |
| Arithmetic | Balanced ternary (saturating at ±T3_MAX) |
| Endianness | N/A (ternary, stored as little-endian i64 in binary) |

All arithmetic saturates at the 27-trit boundary (values are clamped to
[−3,812,798,742,493, +3,812,798,742,493]) rather than overflowing/wrapping.

---

## 2. Register file

| Register | Role |
|----------|------|
| R0 | Always reads as 0; writes are discarded |
| R1 | Function argument 0 / return value |
| R2–R8 | Function arguments 1–7 |
| R9–R23 | General-purpose temporaries |
| R24 | Dedicated return-value stash (callee must not clobber) |
| R25 | Reserved (currently unused) |
| R26 | Stack pointer (SP). Starts at top of memory, grows down |

The FLAGS register is set by `TCMP Rd, Rx, Ry`:
- +1 if `Rx > Ry`
- 0 if `Rx == Ry`
- −1 if `Rx < Ry`

---

## 3. Memory model

Memory is word-addressable. Each address holds one 27-trit word (i64 in storage).
The emulator implements 65,536 words.

Only the 65,536-word address space is architectural. The division below is the
reference emulator's own layout, not a requirement on an implementation — but the
compiler emits absolute addresses for globals, so an implementation that reuses
this toolchain's output has to leave those windows alone.

| Base | Region |
|------|--------|
| 0 | Code, followed by string-literal addresses (`code_size + 1024 + i`) |
| 60,000 | Initial `R26` (SP); the stack grows **downward** from here |
| 61,000 | Module globals, one word each |
| 62,000 | Emulator scratch (`RESULT_AREA`, `TUPLE_AREA`) |
| 63,000 | Heap, grows **upward** to the top of memory |

**Stack:** `R26` (SP) points to the most recently pushed word. Push =
`TSUB R26, R26, #1; STORE Rv, [R26+#0]`. Pop = `LOAD Rv, [R26+#0]; TADD R26, R26, #1`.
Note the initial SP is 60,000, not 65,535 as earlier revisions of this document
stated.

**Heap:** A bump allocator, exposed to compiled code through syscall #218 and used
internally for strings, arrays and struct allocations. There is no free; objects
persist for the life of the program. Collection objects (Vec, Map, Set, Deque,
Channel) are separate: they live outside addressable memory and are referenced by
integer handles at or above `0x8000_0000`.

**String data:** String literal addresses are placed past the code section
(`code_size + 1024 + i`). Strings are not in addressable memory; they are held in
a sidecar table (`.t3d` file) indexed by address.

---

## 4. Word encoding

Every instruction is one 27-trit word (stored as a signed 64-bit integer).

### Field layout

```
Bit position (trit position)  [26..18]  [17..13]  [12..8]  [7..3]  [2..0]
Field                          opcode     r1        r2       r3      imm
Width (trits)                    9          5         5        5       3
Power of 3                      3^18      3^13      3^8      3^3     3^0
```

### Standard encoding

```
word = opcode × 3^18 + r1 × 3^13 + r2 × 3^8 + r3 × 3^3 + imm
```

Used by: `TADD`, `TSUB`, `TMUL`, `TDIV`, `TMOD`, `TAND`, `TOR`, `TNOT`,
`TSHI`, `TSHR`, `TMIN`, `TMAX`, `TCMP`, `LOAD`, `STORE`, `MOV`, `TNEG`,
`TBR_POS`, `TBR_ZERO`, `TBR_NEG`, `CALLR`, `RET`, `HALT`, `NOP`.

### Wide-immediate encoding

```
word = opcode × 3^18 + r1 × 3^13 + wide_imm rem_euclid 3^13
```

Used by: `TLIT`, `JUMP`, `CALL`, `SYSCALL`, `TBRANCH` (partial).

Wide immediate fits ±(3^13−1)/2 = ±797,161.

### Opcode values

The `opcode` field holds one of the following 36 values. Values 36 and above
are unassigned and decode as an invalid instruction.

| # | Mnemonic | Operands | Operation |
|---|----------|----------|-----------|
| 0 | `NOP` | — | no operation |
| 1 | `TADD` | Rd, Ra, Rb | Rd = clamp27(Ra + Rb) |
| 2 | `TSUB` | Rd, Ra, Rb | Rd = clamp27(Ra − Rb) |
| 3 | `TMUL` | Rd, Ra, Rb | Rd = clamp27(Ra × Rb) |
| 4 | `TDIV` | Rd, Ra, Rb | Rd = Ra ÷ Rb, truncated toward zero; Rb = 0 traps |
| 5 | `TMOD` | Rd, Ra, Rb | Rd = Ra rem Rb, truncating; Rb = 0 traps |
| 6 | `TNEG` | Rd, Ra | Rd = −Ra |
| 7 | `TAND` | Rd, Ra, Rb | Rd = min(Ra, Rb) — Łukasiewicz conjunction |
| 8 | `TOR` | Rd, Ra, Rb | Rd = max(Ra, Rb) — Łukasiewicz disjunction |
| 9 | `TNOT` | Rd, Ra | Rd = −Ra — Łukasiewicz negation |
| 10 | `TSHI` | Rd, Ra, Rb\|#imm | Rd = Ra × 3^n, n = rhs clamped to 0..26 |
| 11 | `TSHR` | Rd, Ra, Rb\|#imm | Rd = Ra ÷ 3^n, n = rhs clamped to 0..26 |
| 12 | `TMIN` | Rd, Ra, Rb | Rd = min(Ra, Rb) |
| 13 | `TMAX` | Rd, Ra, Rb | Rd = max(Ra, Rb) |
| 14 | `TCMP` | Rd, Ra, Rb | Rd = FLAGS = sign(Ra − Rb) ∈ {−1, 0, +1} |
| 15 | `LOAD` | Rd, [Ra+#imm] | Rd = memory[Ra + imm] |
| 16 | `STORE` | Ra, [Rb+#imm] | memory[Rb + imm] = Ra |
| 17 | `TLIT` | Rd, #imm | Rd = imm (wide immediate, signed) |
| 18 | `MOV` | Rd, Ra | Rd = Ra |
| 19 | `TBRANCH` | Rc, addr_pos, addr_zero | three-way branch; see below |
| 20 | `JUMP` | addr | PC = addr (unconditional) |
| 21 | `CALL` | addr | push return address, PC = addr |
| 22 | `RET` | — | pop return address into PC |
| 23 | `HALT` | — | stop execution |
| 24 | `SYSCALL` | #num | invoke host service `num` (section 8) |
| 25 | `TBR_POS` | Rc, addr | PC = addr if Rc > 0 |
| 26 | `TBR_ZERO` | Rc, addr | PC = addr if Rc = 0 |
| 27 | `TBR_NEG` | Rc, addr | PC = addr if Rc < 0 |
| 28 | `CALLR` | Rx | push return address, PC = Rx |
| 29 | `BAND` | Rd, Ra, Rb\|#imm | Rd = clamp27(Ra & rhs) — binary bitwise AND |
| 30 | `BOR` | Rd, Ra, Rb\|#imm | Rd = clamp27(Ra \| rhs) — binary bitwise OR |
| 31 | `BXOR` | Rd, Ra, Rb\|#imm | Rd = clamp27(Ra ^ rhs) — binary bitwise XOR |
| 32 | `BSHL` | Rd, Ra, Rb\|#imm | Rd = clamp27(Ra << n), n = rhs clamped to 0..63 |
| 33 | `BSHR` | Rd, Ra, Rb\|#imm | Rd = clamp27(Ra >> n), n = rhs clamped to 0..63, arithmetic |
| 34 | `LOADT` | Rd, [Ra+#imm] | Rd = clamp(memory[Ra + imm], −1, +1) — single trit |
| 35 | `STORET` | Rs, [Ra+#imm] | memory[Ra + imm] = clamp(Rs, −1, +1) — single trit |

Opcodes 29–35 are the binary-interop and single-trit memory group: they let a
ternary program manipulate packed binary values without leaving the machine.

### The effective right-hand operand

Every three-address ALU instruction resolves its right-hand side as

```
rhs = regs[r3] + imm
```

`R0` reads as zero, so `r3 = 0` with `imm = n` encodes an immediate, and
`imm = 0` with `r3 = n` encodes a register. Both forms are legal on `TADD`,
`TSUB`, `TMUL`, `TDIV`, `TMOD`, `TSHI`, `TSHR`, `TMIN`, `TMAX`, `TCMP`,
`BAND`, `BOR`, `BXOR`, `BSHL` and `BSHR`. An implementation must not assume
the immediate form is the only one.

### TBRANCH encoding

`TBRANCH` is a pseudo-instruction that the assembler expands to three words:

1. `TBR_POS Rcond, addr_pos` — jump to `addr_pos` if FLAGS > 0
2. `TBR_ZERO Rcond, addr_zero` — jump to `addr_zero` if FLAGS == 0
3. `JUMP addr_neg` — jump to `addr_neg` unconditionally (covers FLAGS < 0)

---

## 5. Instruction set

### Arithmetic

| Mnemonic | Operands | Operation |
|----------|----------|-----------|
| `TADD` | Rd, Ra, Rb | Rd = clamp27(Ra + Rb) |
| `TSUB` | Rd, Ra, Rb | Rd = clamp27(Ra − Rb) |
| `TMUL` | Rd, Ra, Rb | Rd = clamp27(Ra × Rb) |
| `TDIV` | Rd, Ra, Rb | Rd = Ra ÷ Rb (truncate toward zero; Rb = 0 traps) |
| `TMOD` | Rd, Ra, Rb | Rd = Ra rem Rb (truncating remainder, sign of the dividend; Rb = 0 traps) |
| `TNEG` | Rd, Ra | Rd = −Ra |

### Logic

| Mnemonic | Operands | Operation |
|----------|----------|-----------|
| `TAND` | Rd, Ra, Rb | Rd = min(Ra, Rb) — Łukasiewicz conjunction |
| `TOR` | Rd, Ra, Rb | Rd = max(Ra, Rb) — Łukasiewicz disjunction |
| `TNOT` | Rd, Ra | Rd = −Ra — Łukasiewicz negation |
| `TSHI` | Rd, Ra, Rb | Rd = Ra × 3^Rb (ternary shift left) |
| `TSHR` | Rd, Ra, Rb | Rd = Ra ÷ 3^Rb (ternary shift right, round to nearest — drops the low Rb trits) |
| `TMIN` | Rd, Ra, Rb | Rd = min(Ra, Rb) |
| `TMAX` | Rd, Ra, Rb | Rd = max(Ra, Rb) |

### Comparison

| Mnemonic | Operands | Operation |
|----------|----------|-----------|
| `TCMP` | Rd, Ra, Rb | Rd = FLAGS = sign(Ra − Rb) ∈ {−1, 0, +1} |

`TCMP` writes the sign into `Rd` **and** sets FLAGS; subsequent branch
instructions read FLAGS. The three-address form is the only one the reference
compiler emits.

### Memory

| Mnemonic | Operands | Operation |
|----------|----------|-----------|
| `TLIT` | Rd, #imm | Rd = imm (wide immediate, signed) |
| `LOAD` | Rd, [Ra+#imm] | Rd = memory[Ra + imm] |
| `STORE` | Ra, [Rb+#imm] | memory[Rb + imm] = Ra |
| `MOV` | Rd, Ra | Rd = Ra |

### Branching

| Mnemonic | Operands | Operation |
|----------|----------|-----------|
| `JUMP` | addr | PC = addr (unconditional) |
| `TBR_POS` | Rcond, addr | if Rcond > 0: PC = addr |
| `TBR_ZERO` | Rcond, addr | if Rcond == 0: PC = addr |
| `TBR_NEG` | Rcond, addr | if Rcond < 0: PC = addr |
| `TBRANCH` | Rcond, Lpos, Lzero, Lneg | Three-way: jump to label depending on sign(Rcond) |

`TBRANCH` is expanded by the assembler. It does **not** use FLAGS; it reads
the register value directly.

### Functions

| Mnemonic | Operands | Operation |
|----------|----------|-----------|
| `CALL` | addr | push (PC+1); PC = addr |
| `CALLR` | Ra | push (PC+1); PC = Ra (indirect call) |
| `RET` | — | PC = pop() |

### System

| Mnemonic | Operands | Operation |
|----------|----------|-----------|
| `SYSCALL` | #n | Execute system call n (see [Syscall table](#8-syscall-table)) |
| `HALT` | — | Stop execution |
| `NOP` | — | No operation |

---

## 6. Assembly syntax

### Line format

Each line is one of:
- Empty / comment (`;` introduces a comment)
- A label on its own line: `labelname:`
- A label followed by an instruction: `labelname: INSTR operands`
- An instruction: `INSTR operands`
- A data section declaration: `.data:` or `.globals:`
- A string declaration (inside `.data:` section): `label: .string "content"`

Label names may contain alphanumeric characters, underscores, dots, and `::`.

```asm
; A simple function
my_fn:
  my_fn_entry:
    TSUB  R26, R26, #2        ; alloca 2 locals
    TLIT  R1, #10             ; R1 = 10
    STORE R1, [R26+#0]        ; store to local[0]
    TLIT  R2, #20             ; R2 = 20
    STORE R2, [R26+#1]        ; store to local[1]
    LOAD  R1, [R26+#0]        ; load local[0]
    LOAD  R2, [R26+#1]        ; load local[1]
    TADD  R1, R1, R2          ; R1 = R1 + R2
    TADD  R26, R26, #2        ; dealloc
    RET
```

### Qualified labels

Labels may contain `::` for qualified names (used by method names in impl
blocks):

```asm
Point::to_str:
  Point::to_str_entry:
    ...
    RET

Direction::opposite:
  Direction::opposite_entry:
    ...
    RET
```

### Immediates

Immediates are written as `#value` or `#label`. Signed integers are allowed:

```asm
TLIT R1, #-5      ; R1 = -5
JUMP main         ; jump to label 'main'
```

### Data section

```asm
.data:
  str0: .string "Hello, world!\n"
  str1: .string "done"
.globals:
  ; code here
```

String labels are assigned addresses past the code section. `print_str` syscall
takes a string address in R1 and looks it up in the string sidecar table.

---

## 7. Calling convention

### Argument passing

Arguments are passed in registers R1–R8 (up to 8 arguments). If more than 8
arguments are needed, the excess must be stack-allocated by the caller.

```
R1 = first argument  (also the return value register)
R2 = second argument
...
R8 = eighth argument
```

### Return value

A single return value is placed in R1 before `RET`.

### Caller-save vs callee-save

The current emitter is **all caller-save**: the caller saves any live values
to the stack before a `CALL` and restores them afterward. The callee is not
obligated to preserve any registers.

R24 is used as a dedicated "stash" register for return values that must be
preserved across a subsequent call:

```asm
CALL  foo
MOV   R24, R1   ; stash foo's result
CALL  bar       ; R1 is free again
MOV   R9, R24   ; recover stashed value
```

### Stack frame

The stack grows downward from address 65535. `R26` always points to the most
recently allocated word. Function prologue:

```asm
TSUB R26, R26, #N    ; allocate N words for locals
```

Function epilogue:

```asm
TADD R26, R26, #N    ; free N words
RET
```

Local variables are accessed as `[R26+#offset]` where offset 0 = most recently
allocated slot.

---

## 8. Syscall table

The emulator handles `SYSCALL #n` by inspecting R1 for the primary operand.

### I/O syscalls

| # | Name | R1 | Effect |
|---|------|----|--------|
| 1 | `print_int` | integer | Print as decimal |
| 2 | `print_float` | float bits | Print as decimal float |
| 3 | `print_str` | string addr | Print string from sidecar |
| 4 | `print_newline` | — | Print `\n` |
| 5 | `print_trit` | trit value | Print `+`, `0`, or `-` |
| 6 | `print_bool3` | bool3 value | Print `true`, `unknown`, or `false` |
| 7 | `print_char` | char code | Print single character |

### Vec syscalls

| # | Name | R1 | R2 | Result (R1) |
|---|------|----|----|------------|
| 20 | `Vec::new` | — | — | handle |
| 21 | `Vec::push` | handle | value | — |
| 22 | `Vec::pop` | handle | — | value |
| 23 | `Vec::len` | handle | — | length |
| 24 | `Vec::get` | handle | index | value |
| 25 | `Vec::set` | handle | index (R2), value (R3) | — |
| 26 | `Vec::remove` | handle | index | — |
| 27 | `Vec::contains` | handle | value | 0/1 |
| 28 | `Vec::sort` | handle | — | — |
| 29 | `Vec::reverse` | handle | — | — |

### Map syscalls (50–59)

| # | Name | R1 | R2 | R3 | Result |
|---|------|----|----|----|----|
| 50 | `Map::new` | — | — | — | handle |
| 51 | `Map::insert` | handle | key | value | — |
| 52 | `Map::get` | handle | key | — | value |
| 53 | `Map::contains_key` | handle | key | — | 0/1 |
| 54 | `Map::remove` | handle | key | — | — |
| 55 | `Map::len` | handle | — | — | length |
| 56 | `Map::keys` | handle | — | — | Vec handle |
| 57 | `Map::values` | handle | — | — | Vec handle |

### Set syscalls (80–89)

| # | Name | R1 | R2 | Result |
|---|------|----|----|--------|
| 80 | `Set::new` | — | — | handle |
| 81 | `Set::insert` | handle | value | — |
| 82 | `Set::contains` | handle | value | 0/1 |
| 83 | `Set::remove` | handle | value | — |
| 84 | `Set::len` | handle | — | length |
| 85 | `Set::to_vec` | handle | — | Vec handle |

### Mutex / channel / concurrency (100+)

| # | Name | Notes |
|---|------|-------|
| 100 | `Mutex::new` | R1 = initial value; returns handle |
| 101 | `Mutex::lock` | R1 = handle; sets lock |
| 102 | `Mutex::unlock` | R1 = handle; clears lock |
| 103 | `channel()` | returns handle |
| 104 | `Channel::send` | R1 = handle, R2 = value |
| 105 | `Channel::recv` | R1 = handle; returns value |
| 109 | `AtomicTrit::new` | R1 = initial; returns handle |
| 110 | `AtomicTrit::get` | R1 = handle; returns trit |
| 111 | `AtomicTrit::set` | R1 = handle, R2 = trit |
| 115 | `Barrier::new` | R1 = count; returns handle |
| 116 | `Barrier::wait` | R1 = handle |
| 120 | `Semaphore::new` | R1 = count; returns handle |
| 121 | `Semaphore::acquire` | R1 = handle |
| 122 | `Semaphore::release` | R1 = handle |

### Async (130+)

| # | Name | Notes |
|---|------|-------|
| 130 | `async::yield_now` | Yield to scheduler |
| 131 | `async::sleep` | R1 = ms; yield for duration |

### String / fmt

Earlier revisions of this document listed these at 140–143. **That was wrong** —
no handler has ever existed at those numbers, and calling them traps with
`unknown syscall`. The real numbers are below. Anything not listed here is not
implemented on T3 yet: of the 31 `fmt` natives declared in `stdlib/fmt.mt`, the
emitter maps 9. The rest — `fmt::concat`, `fmt::show_trit`, `fmt::show_bool3`,
`fmt::format2` among them — fail at **assemble** time with `Undefined label`,
not at run time. The LLVM backend implements all of them.

| # | Name | Notes |
|---|------|-------|
| 14 | `fmt::show_int`, `fmt::int_to_str` | R1 = int; returns string addr |
| 15 | `fmt::align_right`, `fmt::pad_left` | R1 = addr, R2 = width |
| 127 | `fmt::format` | R1, R2 = str addrs; returns formatted addr |
| 129 | `fmt::show_float` | R1 = float bits; returns string addr |
| 130 | `fmt::show_bool` | R1 = bool; returns string addr |
| 132 | `fmt::align_left`, `fmt::pad_right` | R1 = addr, R2 = width |

### Memory (218)

| # | Name | R1 | Result (R1) |
|---|------|----|-------------|
| 218 | `heap_alloc_words` | word count | Base address of a zeroed block |

Struct allocations use this rather than the stack. A stack slot is scoped to its
loop iteration — the emitter pops back to the block's canonical depth on the back
edge — so a struct pointer that outlives the iteration would alias the next
iteration's allocation. Allocating past the top of memory traps rather than
silently dropping the writes.

---

## 9. Binary file format

### `.t3b` — code binary

A sequence of 64-bit signed little-endian integers, one per instruction word.
There is no header; the file length divided by 8 gives the word count.

Reading:
```rust
let mut words = Vec::new();
let mut buf = [0u8; 8];
while file.read_exact(&mut buf).is_ok() {
    words.push(i64::from_le_bytes(buf));
}
```

### `.t3d` — string sidecar

A newline-separated text file where each line is `addr:content`. Embedded
newlines in content are escaped as `\n`. The emulator re-expands `\n` to actual
newlines when serving `print_str` syscalls.

```
1024:Hello, world!\n
1025:done\n
1026:error: something went wrong
```

Address values correspond to entries in `label_map` for string labels.

Only `\n` is escaped. A double quote and a backslash are written raw here,
while the `.string` literal in the `.t3s` listing escapes both as `\"` and
`\\`. An implementation reading both artifacts must unescape them differently.

String literals are assigned addresses `code_size + 1024 + i`, where `i` is
the literal's position in **declaration order** — the order its label appears
in the `.data` section of the `.t3s` listing. Float literals follow the same
rule from `code_size + 1024 + <number of strings>`.

---

## 10. Emulator behaviour

### Execution model

The emulator runs a fetch-decode-execute loop:

1. Fetch `memory[pc]`
2. Decode into `(opcode, r1, r2, r3, imm)` or `(opcode, r1, wide_imm)`
3. Execute
4. Advance `pc` by 1 (unless the instruction modified `pc`)
5. Repeat until `halted`

`R0` always reads as 0 regardless of writes. All writes to R0 are silently
discarded.

### Arithmetic semantics

All arithmetic uses signed 64-bit integers internally. Results are clamped to
the 27-trit range [−3,812,798,742,493, +3,812,798,742,493] by `clamp27()`.

Division or remainder by zero is a fault. `TDIV` and `TMOD` with a zero
right-hand operand raise `TRAP: division by zero` and `TRAP: modulo by zero`
respectively, and the machine halts. Every trap halts; a trapped program
exits with status 70 rather than the value in R1.

### Stack overflow

If `R26` goes below address 0 or above address 65535, the emulator does not
currently raise an error — it wraps or accesses out-of-bounds memory. Programs
with deep recursion should be tested with care.

### Cooperative tasks

Tasks are stored in `Vec<Task>`. Each `Task` holds a snapshot of `(pc, regs[0..27],
call_stack, flags)`. When `async::yield_now` or `async::sleep` is called,
the emulator:

1. Saves current task state
2. Advances `current_task = (current_task + 1) % tasks.len()`
3. Restores the next task's state
4. Resumes execution

All tasks run on a single thread. There is no preemption — a task that never
yields will starve all other tasks.

### Heap objects

Heap objects are referenced by integer handles. `heap_alloc_obj(obj)` inserts
into `heap_objs: HashMap<usize, HeapObj>` at the current `heap_ptr` and increments
it. Handles are passed in registers just like integer values.

The emulator does not garbage-collect heap objects. All objects persist until
the program halts.
