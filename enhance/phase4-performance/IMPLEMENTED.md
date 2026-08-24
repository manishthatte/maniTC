# Phase 4 — what landed

© Manish Jagdish Thatte
24 August 2026

Against `enhance/phase4-performance/README.md`: **F-1 is implemented, and its
premise is corrected.** F-3, F-2 and F-4 are not started. Working-tree changes,
uncommitted, per the repo convention.

## Verification

| | before F-1 | after F-1 |
|---|---|---|
| `cargo test` | 535 pass, 0 fail | **585 pass, 0 fail** |
| examples, v1 (default path) | 17/17 T3, 17/17 LLVM, 17/17 parity | **unchanged** |
| examples, v2 (default path) | 17/17 T3, 17/17 LLVM, 17/17 parity | **unchanged** |
| thatteos `tests/test_all.sh` | 61/61 | **61/61** |
| examples with `--mem2reg`, LLVM | — | **17/17, byte-identical to the pre-F-1 compiler** |
| examples with `--mem2reg`, T3 | — | 16/17 run, 11/17 agree — see "why it is off" |

## F-1's premise is wrong, and that is the finding

The recommendation says "the current IR is not SSA (`src/ir/optimize.rs`
propagates constants through a HashMap keyed by temp name, which is a
workaround for not having SSA)".

The first thing built was the instrument to check that. `src/ir/ssa.rs`
verifies three properties separately — single assignment, dominance of every
use by its definition, well-formed phis — and reports which one failed and
where. Run over 18 programs (the 17 examples and thatteOS):

| | |
|---|---|
| function lowerings | 1,806 |
| blocks | 18,828 |
| instructions | 79,953 |
| **SSA violations, after lowering** | **0** |
| **SSA violations, after the optimiser** | **0** |

The IR **is** in SSA form. What it is not is SSA over **variables**: every local
lives in an `Alloca` reached by `Load` and `Store`, so no variable value ever
flows through a temp across a statement and there is nothing for an optimiser
to see. The HashMap is a workaround for values living in **memory**, not for
missing SSA — a different diagnosis with a different fix.

The size of the real problem, same corpus:

| | |
|---|---|
| allocas | 9,455 |
| of those, promotable | 8,660 (**91.5 %**) |
| loads + stores | 42,008 |
| share of all IR instructions | **52.5 %** |

More than half of the IR is moving locals to and from stack slots they never
needed to occupy.

## What was built

### `src/ir/ssa.rs` — the analysis, and the instrument

CFG, Cooper–Harvey–Kennedy dominators, dominance frontiers, `verify`,
`promotable_allocas`, `split_critical_edges`, and a `Stats` counter. 31 unit
tests, including the cases a naive implementation gets wrong: a phi operand is
checked at **its own edge**, not at the phi; a loop-carried phi whose back-edge
value is defined later in program order is legal; an unreachable block is not
checked at all.

Cooper–Harvey–Kennedy over Lengauer–Tarjan deliberately: it is a page of code,
it is fast enough at these block counts, and it is short enough to check by
eye. A dominance bug puts phi nodes in the wrong places, and a wrong phi is a
silently wrong answer.

`manitc compile --verify-ssa` reports twice — after lowering and after the
optimiser — because those are different questions: whether the lowerer
PRODUCES SSA, and whether the passes PRESERVE it.

### `src/ir/mem2reg.rs` — the pass

Cytron et al., in the standard two phases: phi placement at the iterated
dominance frontier of each variable's stores, then renaming down the dominator
tree with a stack of current values. 12 unit tests.

Two things it deliberately does not do:

- **An alloca whose address escapes is left alone** — passed to a call, indexed
  with `GetPtr`, or stored as a value rather than used as a pointer. That is
  the whole safety argument and it is tested on its own.
- **An uninitialised read becomes a typed zero** rather than whatever the stack
  slot held. That is a change, and an improvement: today such a read is a real
  cross-backend divergence, because the T3 emulator zeroes its memory and an
  LLVM `alloca` does not.

## What building it found

Five defects, one in the pass itself and four in code that predates it. All
four of the pre-existing ones were LATENT — correct-by-luck rather than
correct-by-design — and every one of them was found by comparing the two
backends, never by a test of the change.

- **The IR's `Load` and `Store` are not type-neutral.** They carry a `ty` and
  the backends coerce to it, so `store i8 %v` with `%v` an `i64` is a
  narrowing. Removing the memory operation removes the coercion with it.
  `promotable_allocas` now checks the stored value's own type, and
  `ssa::value_types` is what tells it. Nothing had documented this property of
  the IR.
- **P11 (FIXED).** T3 emitted phi copies on one edge SEQUENTIALLY. `a, b = b,
  a + b` — the iterative Fibonacci loop — is two phis whose homes are each
  other's sources, and copying them in order gave `fib(4) = 4`. Fixed by
  emitting a copy only when nothing pending reads its destination, breaking
  cycles through R21 — the same technique the register reconciliation twenty
  lines below already used.
- **P12 (FIXED, and measured latent).** T3 emitted phi copies only in its
  `Jump` arm, so a phi reached from a conditional branch got nothing on that
  edge. Measured first: 616 phis in the corpus, **zero** such edges, so it has
  never fired in a shipped program. Recorded because the margin is luck rather
  than design. Fixed at the IR level by `split_critical_edges`, which is
  backend-agnostic and leaves LLVM unaffected.
- **P13 (OPEN, worked around).** A call's IR return type and its emitted type
  differ: `TernaryTrie::get` is `ret_ty: Trit` in the IR and `call i64` in the
  LLVM. The backend narrows at each use, which works for everything except a
  phi — whose type comes from the IR. Promotion declines to touch a slot fed by
  a narrow-returning call until the backend emits the narrowing at the
  definition instead.
- **P14 (FIXED).** A phi OPERAND's live range ended at the phi rather than at
  the end of its incoming block, so on a back edge the operand looked dead the
  instant it was computed and its register was handed to the next temp. This is
  the same subtlety the verifier had to get right from the other side — **a phi
  operand is used on its EDGE** — and getting it wrong in the allocator is a
  silently wrong answer rather than a failed check. `a = a + b; b = b + a` in a
  loop printed 1 and 95 instead of 144 and 233.

## Why `mem2reg` is off by default

Because the evidence says so on one backend and not the other.

**LLVM:** all 17 examples compile, run, and produce output and exit status
**byte-identical** to the pre-F-1 compiler. That is the strongest statement
available that the pass is semantics-preserving.

**T3:** 16 of 17 run and 11 of 17 agree with LLVM, after the three fixes above.
The three moved it there in steps, which is worth recording because it says
what kind of problem each was:

| | runs | agree |
|---|---|---|
| `mem2reg` alone | 14/17 | 9/17 |
| + P11, parallel phi copies | 15/17 | 10/17 |
| + P12, critical-edge splitting | 15/17 | 10/17 |
| + P14, phi operand liveness | **16/17** | **11/17** |

**What remains is not a fourth defect of the same kind.** It is the RESCUE and
RECONCILIATION machinery — the per-register heuristics that move a value out of
a syscall-clobbered register and try to put it back at a jump. Ten lines
reproduce it:

```manit
fn tbl(op: fn(trit, trit) -> trit) {
    let vals: [trit] = [+, 0, -];
    for a in vals { io::print_trit(op(a, a)); }
}
```

`param_op` is rescued out of R1 for the syscall into R7; the jump's
reconciliation declines to move it back because R1 is "occupied" by the print
argument; the next iteration reads R1 as the function pointer and calls
whatever is there. Correct without `--mem2reg`, because the induction variable
was in memory and the pressure was lower.

That is a global allocation problem being decided one register at a time, which
is exactly what F-3 replaces. `KNOWN_ISSUES` issue 2 already documents two
defects of this family that "produced SILENTLY WRONG ANSWERS and needed enough
register pressure that minimal reproductions passed". Adding another heuristic
to the component whose heuristics are the defect is not a fix.

Turning the pass on by default would trade 52 % fewer IR instructions for wrong
answers on the backend whose whole purpose is to be the reference semantics.
`--mem2reg` makes it available for measurement and for working on F-3.

## Not done

- **F-3, a real register allocator.** Now the blocking item rather than a
  future one: `--mem2reg` is the workload that shows why the current allocator
  needs replacing, and it is a ready-made test for the replacement. The
  acceptance criterion is not "it compiles" — it is
  `matrix3.sh --mem2reg` reaching 17/17 on T3 and 17/17 parity, with the LLVM
  side still byte-identical to the pre-F-1 reference. The ten-line reproduction
  above is where to start.
- **F-2 optimiser passes, F-4 regions.** Untouched. Note that the optimiser
  already has more than the three passes F4 of the review describes — constant
  folding and propagation, ternary peephole, CSE, strength reduction, dead code
  and dead block elimination — so that count is stale too.
- **P13's real fix**, in the LLVM backend's call emission.
