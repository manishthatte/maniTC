# T3ISA Reference

T3ISA (Ternary Three-address Instruction Set Architecture) is the virtual machine
used as the backend for maniT's balanced ternary compilation target. This document
specifies the architecture, instruction set, encoding, assembly syntax, and emulator
behaviour.

**Specification version 1.6** — tagged `t3isa-spec-v1.6` in this repository.
This document is the normative definition of T3ISA; independent implementations
should cite the tagged version they were written against. Where this document
and the maniTC emulator disagree, that is a specification bug — please report it.

Change since 1.5. Like 1.4 and 1.5 this one **alters the architecture**: it ADDS
INSTRUCTIONS, so an implementation written against 1.5 or earlier will decode a
conformant 1.6 program as invalid. A 1.6 implementation runs every 1.5 program
unchanged; the reverse does not hold.

- **§5 Two rounding instructions, opcodes 43–44** — `TDIVN`, `TMODN`. Same
  operand shape as `TDIV`/`TMOD`: three-address, with the third operand a
  register or an immediate.

  `TDIVN Rd, Ra, Rb` sets `Rd` to `Ra / Rb` **rounded to the nearest integer,
  ties away from zero**. `TMODN Rd, Ra, Rb` sets `Rd` to the remainder that
  pairs with it, `Ra − TDIVN(Ra, Rb) × Rb`. Both trap on a zero divisor,
  exactly as `TDIV` and `TMOD` do, and both check their result against the
  27-trit range under the 1.4 rule.

  `TDIV` and `TMOD` are unchanged and remain truncating. The two pairs are
  alternatives, not a replacement: `(TDIV, TMOD)` and `(TDIVN, TMODN)`, with
  `(a / b) × b + (a % b) = a` holding for each pair and for neither crossing.

  **Why the ISA has this at all.** Truncation is C's rule, imported from a
  representation this machine does not use. In balanced ternary, dropping low
  trits *is* rounding to nearest — `TSHR` has rounded correctly since 1.0 — so
  truncating is extra work done to imitate two's complement. A binary machine
  needs sixteen instructions to compute what `TDIVN` computes in one, and the
  maniTC LLVM backend emits exactly those sixteen; that ratio is the same
  argument the lane-wise group makes in 1.5, on a different operation.

  Ties go away from zero rather than to even, because the balanced range is
  symmetric about zero and `TDIVN(−a, b) = −TDIVN(a, b)` should hold. Nothing
  is asserted here about statistical bias: balanced ternary's unbiasedness is a
  property of the representation, not of a tie-break.

  Opcodes 43 and 44 previously decoded as invalid; 45 and above still do. The
  1.5 note reserving 43+ against a `TNOTW` still stands — these are not it, and
  no implementation may assign a lane-wise negation to any opcode.

Change in 1.5. Like the 1.4 change this one **alters the architecture**, and
more substantially: it ADDS INSTRUCTIONS, so an implementation written against
1.4 or earlier will decode a conformant 1.5 program as invalid. A 1.5
implementation runs every 1.4 program unchanged; the reverse does not hold.

- **§4, §5 Seven lane-wise instructions, opcodes 36–42** — `TANDW`, `TORW`,
  `TXORW`, `TIMPW`, `TCMPW`, `TPOPC`, `TSELW`. They read a word as 27
  independent trits rather than as one magnitude, so each performs 27
  three-valued operations in a single instruction. Opcodes 36 and above
  previously decoded as invalid; 43 and above still do.

  This is the architectural claim the ISA exists to make. A binary machine has
  no equivalent: 64 bits are 64 lanes of HALF a datum each — a bit cannot carry
  a three-valued answer — whereas 27 trits are 27 lanes of genuinely
  three-valued data. Emulating one `TANDW` on a binary machine costs 27
  extract-operate-insert cycles, each a division and a multiply by a power of
  three. That gap is measured, not assumed — see §5, which reports 2
  instructions against 3,034 for the same operation.

  (The 1.5 text said "43 and above still do" of invalid opcodes. 1.6 assigns 43
  and 44; 45 and above still decode as invalid.)

  No lane-wise instruction can trap. Every lane result is in {−1, 0, +1} by
  construction, so the reassembled word is in range by construction — a
  property of the balanced representation rather than a bound being enforced.
  This does not weaken the 1.4 overflow rule: nothing here silently clamps a
  result, because no result can leave the range in the first place.

- **`TNOTW` is deliberately absent, and its absence is normative.** Lane-wise
  negation is `TNEG` (opcode 6). Negating a balanced-ternary number flips the
  sign of every trit in it, so `TNEG` already negates all 27 lanes and has done
  since 1.0. Adding a `TNOTW` would have published a second encoding of an
  existing instruction. Implementations must not assign opcode 43+ to one.

Change in 1.4, which also **altered the architecture**, so implementations
written against 1.3 or earlier are not conformant to 1.4:

- **§1 Arithmetic now traps on overflow instead of saturating.** `TADD`, `TSUB`,
  `TMUL` and `TSHI` halt the machine and report the operation and the offending
  value when the true result falls outside ±3,812,798,742,493. Every earlier
  revision ran these results through `clamp27`, so a program that left the range
  silently received ±T3_MAX and carried on to exit 0 with a wrong answer. That was
  the wrong call and it cost a correct result: `examples/fibonacci.mt` guards on
  the 64-bit bound, so `fib_safe(70)` returned `Ok(3812798742493)` instead of
  `Ok(190392490709135)`. Nothing silently clamps any more.

Changes in 1.3, all corrections to sections that described the emulator
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
| Arithmetic | Balanced ternary; traps on overflow |
| Endianness | N/A (ternary, stored as little-endian i64 in binary) |

Arithmetic neither wraps nor saturates. `TADD`, `TSUB`, `TMUL` and `TSHI` trap
when the true result falls outside the 27-trit range
[−3,812,798,742,493, +3,812,798,742,493], halting the machine and reporting the
operation and the value the way division by zero already did. Overflow is a
property of the program being run, not of the implementation, and is reported
rather than absorbed. Revisions up to 1.3 clamped instead; see the changelog
above.

---

## 2. Register file

| Register | Role |
|----------|------|
| R0 | Always reads as 0; writes are discarded |
| R1–R3 | ABI: syscall arguments, syscall result, function return value. **Never allocated to a temp.** R1 is also argument 0 |
| R4–R20 | The allocatable pool — 17 registers. A parameter may also *arrive* in R4–R8, which is why the pool's low end overlaps argument passing |
| R21–R25 | Emission scratch. Never allocated; the emitter may clobber any of them at any point |
| R26 | Stack pointer (SP). Initial value 60,000 (§3); grows down |

> **Corrected 27 August 2026.** The table above previously read `R9–R23`
> general-purpose, `R24` *"dedicated return-value stash (callee must not
> clobber)"*, `R25` *"reserved (currently unused)"*. Four of the seven rows
> disagreed with the allocator. The authority is the written invariant in
> `src/codegen_t3/regalloc.rs`, which the allocator **enforces** (`POOL_LO = 4`,
> `POOL_HI = 20`, and the test `no_allocated_register_is_outside_the_pool`)
> rather than merely asserting. R24 in particular is not a stash and never was:
> its only appearance in the compiler is `const SCRATCH: usize = 24` in
> `emit_parallel_moves`, breaking cycles in parallel register copies — it is
> clobbered freely, which is the opposite of the retired claim. It appears zero
> times in every checked-in `.t3l` listing.

**FLAGS is the sign of the last data-processing result**, not a comparison
flag. `TCMP Rd, Rx, Ry` writes it as you would expect —

- +1 if `Rx > Ry`
- 0 if `Rx == Ry`
- −1 if `Rx < Ry`

— because `TCMP`'s *result* is that trit. But so does every other
data-processing opcode, with the sign of whatever it computed. Measured against
the emulator on 27 August 2026, **27 opcodes write FLAGS and 18 do not**:

| | opcodes |
|---|---|
| **write FLAGS** | `TADD` `TSUB` `TMUL` `TDIV` `TMOD` `TDIVN` `TMODN` `TNEG` `TAND` `TOR` `TNOT` `TANDW` `TORW` `TXORW` `TIMPW` `TCMPW` `TPOPC` `TSELW` `TSHI` `TSHR` `BAND` `BOR` `BXOR` `BSHL` `BSHR` `TCMP` `LOADT` |
| **leave it alone** | `NOP` `TMIN` `TMAX` `LOAD` `STORE` `TLIT` `MOV` `TBRANCH` `TBRPOS` `TBRZERO` `TBRNEG` `JUMP` `CALL` `CALLR` `RET` `HALT` `SYSCALL` `STORET` |

The practical consequence: **FLAGS does not survive arithmetic.** A `TCMP`
followed by a `TADD` leaves FLAGS describing the addition. The emitted code for
`a < b` is `TCMP` then `TNEG` then `TMAX`, and after it FLAGS holds the sign
of the `TNEG`, not of the comparison — `TMAX` is one of the 18 and preserves it.
Branch on the comparison's *register*, which is what `TBRANCH` does, and treat
FLAGS as valid only in the instruction immediately after the one that set it.

> **Corrected 27 August 2026.** This section previously said only *"The FLAGS
> register is set by `TCMP Rd, Rx, Ry`"* and gave the three cases. No sentence
> was false; what was missing was that `TCMP` is one of twenty-seven, which is
> the difference between a comparison flag and a result sign — an absence
> rather than an untruth, which is the kind of documentation defect this
> project keeps finding and the argument for pinning a documented claim with a
> test rather than reviewing the prose.

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

The `opcode` field holds one of the following 43 values. Values 43 and above
are unassigned and decode as an invalid instruction. (The 9-trit opcode field
holds ±9,841, so the encoding has room; the limit is what this specification
assigns, not what the word can express.)

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
| 36 | `TANDW` | Rd, Ra, Rb\|#imm | Rd = lane-wise min(a_i, b_i), 27 lanes |
| 37 | `TORW` | Rd, Ra, Rb\|#imm | Rd = lane-wise max(a_i, b_i), 27 lanes |
| 38 | `TXORW` | Rd, Ra, Rb\|#imm | Rd = lane-wise balanced sum mod 3, 27 lanes |
| 39 | `TIMPW` | Rd, Ra, Rb\|#imm | Rd = lane-wise min(+1, 1 − a_i + b_i), 27 lanes |
| 40 | `TCMPW` | Rd, Ra, Rb\|#imm | Rd = lane-wise sign(a_i − b_i), 27 lanes |
| 41 | `TPOPC` | Rd, Ra, Rb\|#imm | Rd = count of lanes of Ra equal to trit k = clamp(rhs, −1, +1) |
| 42 | `TSELW` | Rd, Rs, Ra, Rb | per-lane select: s_i > 0 → a_i, s_i < 0 → b_i, s_i = 0 → 0 |
| 43 | `TDIVN` | Rd, Ra, Rb\|#imm | Rd = a / b rounded to nearest, ties away from zero |
| 44 | `TMODN` | Rd, Ra, Rb\|#imm | Rd = a − TDIVN(a, b) × b — the balanced remainder |

Opcodes 29–35 are the binary-interop and single-trit memory group: they let a
ternary program manipulate packed binary values without leaving the machine.

Opcodes 36–42 are the lane-wise group, new in 1.5. See §5 for their normative
definition.

Opcodes 43–44 are the rounding pair, new in 1.6. See §5.

### The effective right-hand operand

Every three-address ALU instruction resolves its right-hand side as

```
rhs = regs[r3] + imm
```

`R0` reads as zero, so `r3 = 0` with `imm = n` encodes an immediate, and
`imm = 0` with `r3 = n` encodes a register. Both forms are legal on `TADD`,
`TSUB`, `TMUL`, `TDIV`, `TMOD`, `TDIVN`, `TMODN`, `TSHI`, `TSHR`, `TMIN`,
`TMAX`, `TCMP`, `BAND`, `BOR`, `BXOR`, `BSHL` and `BSHR`. An implementation
must not assume the immediate form is the only one.

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
| `TADD` | Rd, Ra, Rb | Rd = Ra + Rb, trapping if the true result leaves the range |
| `TSUB` | Rd, Ra, Rb | Rd = Ra − Rb, trapping if the true result leaves the range |
| `TMUL` | Rd, Ra, Rb | Rd = Ra × Rb, trapping if the true result leaves the range |
| `TDIV` | Rd, Ra, Rb | Rd = Ra ÷ Rb (truncate toward zero; Rb = 0 traps) |
| `TMOD` | Rd, Ra, Rb | Rd = Ra rem Rb (truncating remainder, sign of the dividend; Rb = 0 traps) |
| `TDIVN` | Rd, Ra, Rb | Rd = Ra ÷ Rb rounded to nearest, ties away from zero (v1.6; Rb = 0 traps) |
| `TMODN` | Rd, Ra, Rb | Rd = Ra − TDIVN(Ra, Rb) × Rb — the balanced remainder (v1.6; Rb = 0 traps) |
| `TNEG` | Rd, Ra | Rd = −Ra |

The first three rows said `clamp27(...)` until 1.6. That was left over from 1.3
and contradicted 1.4's own change note in this document: since 1.4 these trap
rather than clamp, and nothing in T3ISA silently substitutes ±T3_MAX for a
result it cannot represent. Corrected, not changed — the emulator has trapped
since 1.4.

### Rounding division (v1.6)

`TDIVN` and `TMODN` are the round-to-nearest pair. They exist because
truncation is C's rule and this is not a two's-complement machine: dropping low
trits *is* rounding to nearest here, which is why `TSHR` has always rounded,
and `TDIV` spends work to imitate a representation the machine does not use.

Normative, not commentary:

- **The two move together.** `TMODN` is defined from `TDIVN`, so
  `(a ÷ b) × b + (a rem b) = a` holds for `(TDIVN, TMODN)` exactly as it does
  for `(TDIV, TMOD)`. An implementation that rounds one and truncates the other
  is not conformant.
- **Ties go away from zero**, so `TDIVN(−a, b) = −TDIVN(a, b)` for every `a`
  and every `b ≠ 0`. Round-half-to-even is *not* conformant here, and the
  reason for the choice is symmetry rather than statistical bias — the latter
  is a property of the representation, not of the tie-break.
- **The balanced remainder can be negative for a positive dividend.** `TMODN`
  yields a value in [−|b|/2, +|b|/2]; `TMODN Rd, 7, 2` is −1, where
  `TMOD Rd, 7, 2` is +1.
- **`TDIV` and `TMOD` are unchanged.** 1.6 adds instructions; it retires none.

Worked cases, which double as the smallest conformance test for the pair:

```
    TDIVN(7, 2)  =  4      TMODN(7, 2)  = -1
    TDIVN(-7, 2) = -4      TMODN(-7, 2) =  1
    TDIVN(1, 3)  =  0      TMODN(1, 3)  =  1
    TDIVN(2, 3)  =  1      TMODN(2, 3)  = -1
    TDIVN(5, 3)  =  2      TMODN(5, 3)  = -1
    TDIVN(4, 3)  =  1      TMODN(4, 3)  =  1
```

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

### Lane-wise logic (v1.5)

Every instruction above treats a word as one number. These treat the same word
as **27 independent trit lanes** and operate on all of them at once.

Lanes are numbered from the least significant trit. Lane *i* of a word *w* is
the balanced-ternary digit d_i in w = Σ d_i · 3^i with each d_i ∈ {−1, 0, +1}.
The decomposition is unique, so "lane *i*" is well defined without reference to
any storage format.

| Mnemonic | Operands | Operation (for every lane i, 0 ≤ i < 27) |
|----------|----------|------------------------------------------|
| `TANDW` | Rd, Ra, Rb\|#imm | d_i = min(a_i, b_i) |
| `TORW` | Rd, Ra, Rb\|#imm | d_i = max(a_i, b_i) |
| `TXORW` | Rd, Ra, Rb\|#imm | d_i = balanced sum mod 3 of a_i and b_i |
| `TIMPW` | Rd, Ra, Rb\|#imm | d_i = min(+1, 1 − a_i + b_i) |
| `TCMPW` | Rd, Ra, Rb\|#imm | d_i = sign(a_i − b_i) |
| `TPOPC` | Rd, Ra, Rb\|#imm | Rd = #{ i : a_i = k }, k = clamp(rhs, −1, +1) |
| `TSELW` | Rd, Rs, Ra, Rb | d_i = a_i if s_i > 0; b_i if s_i < 0; 0 if s_i = 0 |

Notes that are normative, not commentary:

- **`TXORW` is not an involution.** The lane operation is addition mod 3 on a
  balanced digit set, and 3k ≡ 0 (mod 3), so recovering the original word takes
  **three** applications of the same key, not two. An implementation that makes
  it self-inverse is not conformant.
- **`TIMPW` is Łukasiewicz, not Kleene.** The a_i = b_i = 0 lane yields +1.
  Kleene's max(−a, b) yields 0 there. The consequence is checkable in one line:
  `TIMPW Rd, Ra, Ra` must produce the all-+1 word (+3,812,798,742,493) for
  **every** Ra, including words with zero lanes. This is the deduction theorem
  holding lane-wise, and it is the cheapest conformance test in this section.
- **`TPOPC` returns a count, not a word.** Its result is in 0..=27 and is an
  ordinary magnitude; it is the one member of the group whose output is not
  read lane-wise.
- **`TSELW` takes four registers in a three-register encoding.** Rb rides in the
  3-trit immediate field read as UNSIGNED. That field holds 3^3 = 27 values and
  the register file is R0..R26 — exactly 27 — so no new instruction format is
  needed. The coincidence is not one: both numbers are "what three trits
  address". `TSELW` is genuinely three-way — the zero lane selects zero rather
  than choosing between two arms, which is a case a binary select does not have.
- **None of these can trap.** Every lane result is in {−1, 0, +1} by
  construction, so the reassembled word is in range by construction.
- **`TSELW` is not emitted by the reference compiler.** It is assembled,
  implemented and unit-tested like the rest of the group, and maniT has no
  surface syntax that lowers to it, so on this implementation it is reachable
  only from hand-written assembly. That is stated because it bears on
  conformance: the other six instructions are exercised end-to-end by compiled
  ManiT programs on both backends, and `TSELW`'s only coverage is the
  emulator's own tests. An independent implementer should treat it as the
  least-exercised part of v1.5 and test it accordingly.

  The same was true of `TPOPC` until `trit::count(x, k)` landed alongside this
  revision; it now has an end-to-end path.
- **Operands are read as exactly 27 lanes.** A conformant machine cannot present
  a wider value: §1 arithmetic traps on overflow, so a register never holds one.

#### The measured cost of not having them

R4 of the C2 plan requires this figure to be measured before it is quoted, so
it is measured here rather than argued. Method: lane-wise AND of the same two
words, 1000 calls, once as `TANDW` and once as the extract-operate-insert loop
a machine without it must write, both compiled by maniTC and run on this
emulator; a third run with the operation removed establishes the loop-harness
baseline, which is subtracted from both.

| | instructions per call, above baseline |
|---|---|
| `a tandw b` | **2** |
| the same thing written out, 27 lanes | **3,034** (112 per lane) |

That is **1,517× fewer instructions**, and the shape of the number is worth
stating precisely because it is easy to quote wrongly. The 27 in "27-way SIMD"
is the LANE COUNT — the parallelism — not the instruction ratio. The ratio is
larger than 27 because each lane costs an extract (a division, a remainder and
a rebalance) and an insert (a multiply by a power of three and an add), not one
instruction.

The honest caveat: 3,034 is compiler-generated code from maniT source, not
hand-tuned assembly. A hand-written expansion would be tighter, so the
architectural floor is nearer 100–200× than 1,517×. Both bounds are far above
27, which is the point: the claim in the plan was conservative.

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

Arguments are passed in registers R1–R8 (up to 8 arguments; `PARAM_MAX = 8` in
`regalloc.rs`). If more than 8 arguments are needed, the excess must be
stack-allocated by the caller. Note that R4–R8 do double duty: they carry
arguments 3–7 *and* are part of the allocatable pool (§2), so a parameter
arriving in one of them reserves it.

```
R1 = first argument  (also the return value register)
R2 = second argument
...
R8 = eighth argument
```

### Return value

A single return value is placed in R1 before `RET`.

### Caller-save vs callee-save

**A `CALL` may destroy every register.** The callee allocates from the same
pool, so the rule is not "caller-save" in the usual sense — it is stronger:
*nothing live across a call is in a register at all.* The allocator's
`must_spill` enforces it, which is what lets the call sequence use the whole
machine without saving anything. A value needed after a call is stored to the
frame once and reloaded once.

> **Corrected 27 August 2026.** This section previously described R24 as a
> *"dedicated stash register for return values that must be preserved across a
> subsequent call"* and showed `MOV R24, R1 ; stash foo's result`. **The
> compiler has never emitted that.** R24 is emission scratch (§2), and a value
> live across a call is spilled to the frame, not parked in a register. The
> retired example describes a convention this backend deliberately replaced —
> see the register invariant at the top of `src/codegen_t3/regalloc.rs`.

### Stack frame

The stack grows downward from the initial `R26` of **60,000** — see §3's
memory map, which is authoritative for every address in this document. `R26`
always points to the most recently allocated word. Function prologue:

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
