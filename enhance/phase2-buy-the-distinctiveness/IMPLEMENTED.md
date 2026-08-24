# Phase 2 — what landed

© Manish Jagdish Thatte
24 August 2026

Against `enhance/phase2-buy-the-distinctiveness/README.md`: **C1, C2, C7 and
A2 all complete — the phase is done.** Working-tree changes, uncommitted, per
the repo convention.

## Verification

| | before | after |
|---|---|---|
| `cargo test` | 451 pass, 0 fail | **473 pass, 0 fail** |
| examples on T3 | 17/17 | 17/17 |
| examples on LLVM | 17/17 | 17/17 |
| thatteos `tests/test_all.sh` | 61/61 | 61/61 |

**Twenty-two new tests** (451 → 473), no existing test replaced: four in
`tests/expected_output_tests.rs` (`expected_30_lanewise`, `cross_30_lanewise`,
`expected_31_trit_intrinsics`, `cross_31_trit_intrinsics`), one profiler
regression test in the emulator, fifteen in the new `tests/phase2_tests.rs`
(eleven for A2, four for the width audit), and two in the new
`tests/differential_tests.rs`.

One existing test changed rather than broke:
`a1_availability_is_reported_against_the_selected_backend` now passes
`-A backend-unavailable-chain`. It pins A1 step 3's `allow` default, and A2
makes the same program a hard error by a different route — the flag keeps the
two independently testable instead of letting one test assert the other's
behaviour.

## C1 — the Łukasiewicz family

`timp`, `teq`, `tposs`, `tnec`. Landed earlier in the session; truth tables
verified against the spec on both backends, byte-identical. `a timp a` gives
`+1` for unknown — the deduction theorem, which is what makes the logic L3 and
not Kleene's K3.

## C2 — lane-wise logic, T3ISA v1.5

The ISA/IR/emulator/runtime layer had already landed and was covered by 11
tests (`cargo test --lib v15`), but `IRInstr::TritLane` was **unreachable from
ManiT source**. This closes that.

### The language surface

Six operators, following the C1 commit shape exactly — lexer keyword →
`BinOpKind`/`UnOpKind` → `parse_ternary_logic_expr` → `binop_symbol` /
`binop_type` / operand hint → `lower_expr`:

| surface | lowers to | T3 |
|---|---|---|
| `a tandw b` | `IRLaneOp::And` | `TANDW` |
| `a torw b` | `IRLaneOp::Or` | `TORW` |
| `a txorw b` | `IRLaneOp::Xor` | `TXORW` |
| `a timpw b` | `IRLaneOp::Imp` | `TIMPW` |
| `a tcmpw b` | `IRLaneOp::Cmp` | `TCMPW` |
| `tnotw a` | `IRInstr::UnOp{Neg, I64}` | `TNEG` |

`tcmpw` was **not** in the handoff's list of five. It is included because the
ISA layer already published `TCMPW` and leaving it unreachable from source is
the exact defect this item exists to fix. `TPOPC` remains unreachable by
design — C7 claims it as `trit::count`. `TSELW` has no surface and no
`IRLaneOp` yet; it is reachable only from hand-written assembly.

Precedence: same level as the scalar family, left-associative. Binding them
differently would make `a tandw b torw c` and `a tand b tor c` parse into
different shapes for no reason a reader could predict.

Type rule: operands are **words**, not trits — any integer type, rejecting
`float`, `tfloat`, `bool` and `bool3`. `Tfloat` is excluded explicitly rather
than left to `is_numeric()` (which admits it) because its 27 trits are a
mantissa and an exponent, not lanes.

### Two defects found while doing it

**1. `tnotw` truncated to 8 bits on LLVM.** Lowering it to `IRInstr::TritNeg` —
which is what "lane-wise NOT *is* TNEG" suggests — was wrong at the IR level.
`TritNeg` is a *trit* instruction: the LLVM backend types it `i8` and emits
`sub i8 0, x`. `tnotw 9841` gave **-113 on LLVM and -9841 on T3** (9841 & 0xFF
= 113). Both backends looked self-consistent alone; only the differential test
saw it. Fixed by emitting `UnOp{Neg, I64}`, which still emits `TNEG` on T3, so
the ISA claim is intact — the claim is true about the *architecture* and was
false about the *IR*.

**2. The profiler could not see opcodes 36–42.** `opcode_counts` was a
hard-coded `[usize; 36]` and the summary's name table stopped at `STORET`, so
every lane-wise instruction was counted in `total_instructions` and in
`ternary_native_ops` and then dropped from the per-opcode histogram. A
benchmark of `TANDW` showed the work happening and no `TANDW` in the
breakdown. Both are now sized from `isa::T3_OPCODE_COUNT`, with a
`debug_assert` tying the name table to it and a test
(`v15_profiler_sees_every_lane_opcode`) covering all seven.

This mattered beyond tidiness: **R4 requires the performance claim to be
measured before it is quoted, and the instrument could not see the
instruction.**

### R3 — the spec bump (done)

`docs/t3isa-reference.md` is now **v1.5**, announced the way v1.4 announced its
own architecture change, with the compatibility direction stated explicitly (a
1.5 machine runs 1.4 programs; the reverse does not hold). Opcodes 36–42 are in
the table and have a normative §5 section. The absence of `TNOTW` is itself
normative — implementations must not assign an opcode to one.

### R4 — the performance claim (measured)

Method: lane-wise AND of the same two words, 1000 calls, once as `TANDW` and
once as the extract-operate-insert loop a machine without it must write; both
compiled by maniTC and run on this emulator. A third run with the operation
removed establishes the loop-harness baseline, subtracted from both.

| | instructions per call, above baseline |
|---|---|
| `a tandw b` | **2** |
| the same thing written out, 27 lanes | **3,034** (112 per lane) |

**1,517× fewer instructions.** The plan's "~27×" was conservative and also
describes a different quantity: 27 is the LANE COUNT, not the instruction
ratio. The ratio is larger because each lane costs an extract (division,
remainder, rebalance) and an insert (multiply by a power of three, add), not
one instruction.

Caveat, stated in the spec too: 3,034 is compiler-generated code from ManiT
source, not hand-tuned assembly. A hand-written expansion would be tighter, so
the architectural floor is nearer 100–200× than 1,517×. Both are far above 27.

### Documentation

`docs/language-reference.md` gained the lane-wise section **and** the C1
family, which was undocumented — the reference described only `tand`, `tor`,
`tnot`, `txor` and its keyword list was missing six operators that already
existed in the language. Documenting `timpw` while `timp` was absent would have
been incoherent, since the lane-wise section explains itself in terms of it.
`tcon` and `tany` were undocumented too and are now included. Every code
example and the one error message in the new sections were executed and their
values pasted from the run, not written by hand.

The LSP hover and completion tables were likewise missing the whole C1 family;
they now carry C1 and C2 both.

## C7 — trit intrinsics

A `trit::` namespace, because `math::trit_count(x)` already exists and means
trit LENGTH while the intrinsic wanted counts LANES EQUAL TO k — the same
obvious name for a different question. New stdlib module `stdlib/trit.mt`
(registered in all three of the lists `analyzer/mod.rs` warns about).

| surface | lowering | T3 cost |
|---|---|---|
| `trit::sign(x)` | `IRInstr::TritSign` (new) | 1 instruction |
| `trit::abs(x)` | `TritSign` + word-width `Mul` | 2 |
| `trit::count(x, k)` | `IRLaneOp::Popcount` (C2's) | 1 |
| `trit::shift3(x, n)` | renamed onto `ternary::trit_shift_left` | 1 |
| `trit::leading_zeros(x)` | ManiT over `math::trit_count` | — |
| `trit::trailing_zeros(x)` | ManiT | — |

The natives are lowered in `ir/lower/lower_expr.rs`, NOT intercepted per-backend
in the two emitters. `math` took the other route and a census measured **3 of
its 52** functions working on both backends, because each intercept had to be
written twice and nothing forced the second one. There is now no second place to
forget.

### The plan's formula for `sign` was wrong, and quietly

The handoff specified `sign(x) = TritMax(TritMin(x, 1), -1)` — "two native
instructions, no branch". It type-checks, it is branchless, and it is **wrong on
LLVM for any word wider than 8 bits**: `TritMin`/`TritMax` are trit-width there
(both `record_temp_type(..., "i8")`), so the operand is truncated before it is
clamped and `sign(256)` returns **0** instead of `+1`.

This is the same defect class as C2's `tnotw`, found the same way, and it is
worth naming as a pattern: **the `Trit*` IR instructions are trit-width on LLVM
and word-width on T3.** Anything that reaches for one to operate on a word is
correct on T3 and silently truncating on LLVM. `tests/31_trit_intrinsics.mt`
carries 256 and -256 specifically to catch it.

So `sign` got its own instruction, `IRInstr::TritSign`, which is what the
recommendations asked for anyway: R0 always reads as zero, so `TCMP Rd, Ra, R0`
IS the sign, in **one** instruction. Measured against the hand-written
`if x > 0 { 1 } elif x < 0 { -1 } else { 0 }`: **2.0 instructions per call
against 11.5**, a 5.8x difference, branchless.

Adding an IR instruction was not free and the cost is worth recording:
`optimize.rs` has 14 catch-all match arms, so a new `IRInstr` variant falls into
several of them **silently** — including `collect_used_from_value`, where being
missed means dead-code elimination deletes the operand's definition. All eight
`TritNeg` sites plus both liveness sites were updated by hand. This is why
`sign` got an instruction and `abs` did not.

## A2 — backend availability, inferred

A function is available on backend B exactly when every function it calls is.
The call graph is built during checking (`SemanticAnalyzer::call_graph`), so it
cannot disagree with what the checker saw; availability is a backwards dataflow
over it, and the lattice is subsets of a two-element set, so a fixpoint
iteration converges in a few rounds and handles recursion with no separate SCC
pass — a cycle simply settles at the meet over itself.

```
error: demo.mt:8:19: 'main' cannot be compiled for the t3 backend:
       main -> draw_frame -> paint -> gui::set_color
       - and 'gui::set_color' is declared available only on: llvm
```

Three design decisions worth their reasons:

- **Report the OUTERMOST offender, not every link.** One unavailable extern
  makes everything above it unavailable, so the first version emitted three
  errors for one fact. Reporting the innermost instead would have duplicated A1
  step 3, which already fires at the call site. What A2 knows that A1 cannot is
  the transitive part, so the outermost function is the one to name — and its
  chain lists every hop including the culprit.
- **Deny by default**, unlike `backend-unavailable`'s `allow`. That lint is the
  A1 migration backlog, mostly annotations nobody has written. This one fires
  only when a clause someone WROTE is contradicted by a call chain that
  EXISTS, for the backend being compiled right now.
- **`manitc check` reports nothing.** It selects no backend and so has no
  availability question to answer. The written-assertion check is the exception
  and does fire there, because a false assertion is a statement about the
  program rather than about one invocation.

### Written availability as a checked assertion

`fn render() available(llvm, t3) { ... }` now parses — `available` stays
contextual, not a keyword, because `stdlib/sync.mt` declares a method called
exactly that. Inference decides; a written clause is checked against it, and
also constrains callers.

A bug worth recording, because it failed silently: the witness search
originally accepted only an `extern` as a chain terminator. A written clause on
an ordinary function therefore narrowed the lattice correctly, the search then
ran off the end of the graph, found no witness, and the diagnostic was dropped
— the constraint was inferred and then discarded on the way to the user.
`a2_a_written_clause_constrains_callers` pins it.

### What A2 does NOT yet do

**It is inert on the existing corpus.** Grepped: no `.mt` file in `manitc/` or
`thatteos/` writes an `available(...)` clause on anything, so there is nothing
for the inference to constrain and all 17 examples plus thatteOS compile exactly
as before. The mechanism is right and it currently has no data.

The obvious next step, and it is not small: the T3 emitter's intercept list IS
the set of natives T3 can supply, and the C runtime's symbols are the LLVM set.
Seeding availability from those two facts would make A2 bite immediately and
would find the real `editor.mt`-shaped problems the recommendations describe.
That is a separate item — done carelessly it would declare every undeclared
native T3-unavailable and break the world.

## The width audit — and finding P1

Run after the phase, because the same defect had by then bitten twice by
accident (`tnotw`, and the `sign` formula the plan specified). Auditing for the
CLASS found a third and much larger instance, now recorded as **report.txt
Section 10, P1**.

**The class.** The `Trit*` IR instructions are word-width on T3 and trit-width
on LLVM — the LLVM backend types `TritMin`/`TritMax`/`TritNeg` as `i8` because
their operand really is a trit, while T3's TMIN/TMAX/TNEG act on a whole
register. Anything reaching one with a WORD is correct on T3 and silently
truncated on LLVM. Each instance is invisible to single-backend testing and to
reading either backend alone, because each backend is internally consistent.

**The third instance: the whole ternary-logic operator family.** `binop_type`
gated on `is_ternary()`, which admits `tryte`/`t9`/`t27`/`t54`/`tfloat` —
ternary NUMBERS, not three-valued values. On `let a: t27 = 9841; let b: t27 =
121`, seven of eight operators disagreed between the backends. Worse than a
divergence: the result was typed `trit` and neither backend produced one, so
the operation was never defined at all.

Fixed by rejecting, not by making the backends agree — the reference documents
these as operating "on `trit` and `bool3` values", and C2's lane-wise family is
the defined thing to do to a word. The diagnostic names `tandw`/`tnotw` so a
rejection is not a dead end. **Safe, measured not assumed:** `binop_type` was
instrumented and all 268 shipped `.mt` files checked — zero sites.

**Audit result.** Every `TritMin`/`TritMax`/`TritNeg` emission site is in
`src/ir/lower/` and all now carry trit-width operands by construction;
`TritSign` and `TritLane` are word-width by construction. No route remains from
ManiT source to a trit-width IR instruction holding a word.

## The third implementation — `tests/differential_tests.rs`

N7 says not to trust agreement between the two backends as evidence, and the
project has twice been burned by both agreeing and both being wrong. This adds
a reference implementation written from the NORMATIVE TEXT of the T3ISA
reference §5 rather than from `codegen_t3::isa`, deliberately decomposing a word
by a different route (descending from the top power of three) so a mistake in
`trits27` cannot be reproduced and cancel out.

130 cases — edge cases plus 120 pseudo-random words from a fixed seed, biased
WIDE because the P1 defect is invisible below 8 bits — across 13 operations,
checked three ways: T3, LLVM, reference. Plus 500 rounds of algebraic laws
checked against the reference alone, which is what catches a reference that is
itself wrong.

**Mutation-checked.** Reverting the `tnotw` fix makes it report **127
disagreements over 130 cases**. A differential test that has never been shown
to fail is not evidence.

This is A3's first increment at a fraction of A3's cost.

## Instrument re-measurement

`manitc check` was measured against `14178d80b64116d9`, the strict L1
instrument, over the same 268 `.mt` files the Phase 1 measurement used:
**0 verdict differences**, both before and after the P1 strictening. L1 numbers
recorded with the old binary remain comparable. Tooling:
`manit-model/eval/instrument_diff.py`. Binaries archived as
`df97ca84b8704469` (phase 2) and `c90bafd93a090d42` (phase 2 + width fix).

Not a formality: A2's lint denies by default and is the first change in either
phase able to turn a passing compile into a failing one.

## Loose ends

- **`(closes finding F2)`** in this phase's README is a dangling reference.
  `report.txt`'s F2 is an unrelated struct-literal parsing issue
  (`src/parser/exprs.rs:561-581`), and `enhance/` defines no F-series of its
  own. No finding status was changed on the strength of it. Resolve the
  reference before quoting it as closed.
- **The "changes nothing" measurement has not been repeated, and this phase
  raises the stakes on it.** Phase 1 was verified to produce 0 verdict
  differences over all 268 `.mt` files against the previous binary. That has
  NOT been redone for C1/C2/C7/A2.

  Two halves of it have been checked by hand and are clean: **keyword
  collisions** (all 269 `.mt` files grepped for the six lane-wise keywords used
  as identifiers — none, and `available` was deliberately kept contextual for
  the same reason), and **A2's effect on the corpus** (no `.mt` file writes an
  `available(...)` clause, so the new deny-by-default lint fires nowhere).

  The verdict-text comparison over the corpus still needs a rerun. Note that
  A2 is the first change in either phase that can turn a *passing* compile into
  a *failing* one on evidence the previous binary did not have, so this is no
  longer a formality — do it before the next L1 run.
- **A2 is inert until availability is declared.** See its section above: the
  mechanism is complete and tested, and the corpus gives it nothing to
  constrain. Seeding from the T3 emitter's intercept list is the item that
  would make it bite.
