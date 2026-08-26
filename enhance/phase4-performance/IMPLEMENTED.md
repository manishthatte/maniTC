# Phase 4 — what landed

© Manish Jagdish Thatte
24–25 August 2026

Against `enhance/phase4-performance/README.md`: **F-1 and F-3 are implemented,
and F-1's premise is corrected. F-2 is in progress — three of its six existing
passes turned out to be doing nothing, and all three are now repaired; the
inliner is not written.** F-4 is not started.

Read in order: F-1 below (SSA and `mem2reg`), then **F-3** and **F-2** at the
end of this file. `report.txt` §10 (P11–P27) is the source of truth for every
defect any of it found.

## F-1 — verification as it stood on 24 August

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

## Why `mem2reg` was off by default — SUPERSEDED, it is ON now

Kept because it is the argument F-3 had to answer, and F-3 answered it: the
allocator was rewritten, the phi defects behind these numbers were fixed
(P11, P12, P14, P16), and promotion now runs 17/17 with 17/17 parity on both
language versions. `--no-mem2reg` is the way out.

Because the evidence said so on one backend and not the other.

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

## F-3 — the T3 register allocator, rewritten

The item above named it as blocking, and it was. `src/codegen_t3/regalloc.rs`
now gives **a temp ONE location for the whole function**; `rescue_reg`,
caller-save, canonical block state and the phi home were **deleted rather than
repaired**, because they were the heuristics that were the defect.

Four findings, all silently wrong answers, all in `report.txt` §10:

- **P15** — a struct or enum PARAMETER destroyed by the heap-alloca syscall
  that exists to hold it: the "is this parameter safe in R1?" guard listed
  calls but not syscalls, so every enum parameter arrived as a heap address,
  `match` fell through every arm and `==` was false against all variants.
- **P16** — phi copies into a frame SLOT scheduled as though slots could not
  interfere, storing to a slot before the copy that read it. `fib(10)` printed
  `512 512`.
- **P17** — every struct/tuple alloca got BOTH a frame region and a heap cell
  while only one is ever used. Dead weight rather than corruption.
- **P18** — a float PARAMETER destroyed by the instruction meant to read it,
  because the R1–R3 guard asked "live ACROSS this instruction" when the right
  question is "live INTO it". A regression F-3 itself introduced, and its own
  surviving comment described it.

With F-3 in, `mem2reg` went on by default: −51 % of the IR and −82 % of
load/store traffic on `ternary_sort`.

## F-2 — the optimiser passes

### Three of the six were doing nothing, and each for a different reason

`--pass-stats` and `--rounds` were built first, as the instrument. Across all
17 examples, before any repair:

| pass | effect |
|---|---|
| `mem2reg` | removes 9 – 3,747 — the dominant pass |
| `dead_code_eliminate` | removes 0 – 265, scales with `mem2reg` |
| `constant_fold` | rewrites 0 – 88 |
| `constant_propagate` | rewrites 0 – 68 |
| **`ternary_peephole`** | **fires twice, in one program** |
| **`common_subexpression_eliminate`** | **three times, across three programs** |
| **`strength_reduce`** | **zero times, in all seventeen** |

**Iteration was the first hypothesis and it is wrong.** `--rounds 5` removes 4
instructions of 41,306 — one hundredth of a per cent. The passes are not
mis-ordered and they are not starved of each other's output. They do not apply.

All three are now repaired, and the diagnosis differed every time.

### `strength_reduce` did no strength reduction

Four algebraic identities needing a literal constant nobody writes. **The
ternary one is the point**: T3ISA has `TSHI`/`TSHR` — multiply and divide by
three in ONE instruction — and the IR's shifts mapped to the BINARY
`BSHL`/`BSHR`. 203 of 1,708 multiplies and divides in the examples (11.9 %) are
by a power of three. `IRBinOp::TShl`/`TShr`/`TShlT27` now carry the shift
AMOUNT and lower to `TSHI`/`TSHR`; on LLVM they are rewritten back into the
`Mul`/`DivNear` they came from, so parity is by construction. **142 `TSHI`
where there were none.** `Div` is excluded — it truncates and `TSHR` rounds —
and `MulT27` takes the CHECKED partner, so `--lang v2` keeps N5's guard.

### `common_subexpression_eliminate` was mis-scoped

Not broken, not inapplicable: **confined to a scope that barely exists.**

| blocks | 14,857 |
|---|---|
| mean block length | **2.78 instructions** |
| holding 0 or 1 instruction | 9,769 — **65.8 %** |
| empty, terminator a plain `Jump` | 4,119 — 27.7 % |

Two-thirds of blocks cannot hold a redundancy at all. Scoped to **dominance**
the same key set finds 186; `GetPtr` — 21 % of the IR and not keyed at all —
takes it to 238, `BoundsCheck` to 308. `Load`, `Call` and `Alloca` stay
unkeyed, and each exclusion is a wrong answer avoided rather than a missed one.

**3 hits → 311 instructions removed across 15 of 17 examples.**

### `ternary_peephole` had `strength_reduce`'s disease

Five identities on literal trit operands nobody writes, on an operand class
that is 0.36 % of the IR. **And, again, the ternary one is the point.** T3ISA
has `TCMP Rd, Ra, R0` — sign in one instruction — and `TBRANCH`. The IR has had
both since C7. Nothing PRODUCED the pair from ordinary source, so the sign
trichotomy (`if x > 0 … else if x < 0 … else …`, which is how a
balanced-ternary standard library is written) became two two-way comparisons
and two two-way branches — and the T3 backend then computed the three-way sign,
**clamped it back to a boolean**, and branched three ways with two arms on one
label. Twelve instructions in two blocks where the machine needs two in one.

**53 sites collapsed across 12 of 17 examples.** Float is excluded and that is
P20 from the other side: a NaN is false to `<`, `>` and `==` alike, so it needs
a fourth arm that `TritBranch` does not have.

### What it is worth, statically and dynamically

Static instruction counts are the wrong measure and this is the finding.
Bisecting `run-t3 --max-steps` gives a program's exact dynamic count:

| | static | dynamic |
|---|---|---|
| CSE | −399 | **−6,420 (−1.51 %)** |
| trichotomy collapse | −644 | −430 (−0.10 %) |
| both | −1,043 (−0.67 %) | **−6,850 (−1.61 %)** |
| T3 code size | −9,192 bytes (−0.70 %) | |

**The static count understates CSE by a factor of sixteen and overstates the
collapse** — CSE takes work out of loops, and the collapsed trichotomies sit in
stdlib functions these examples call a bounded number of times. The ranking of
the two repairs is opposite in the two measures.

CSE also ADDS memory traffic on T3 (`LOAD` +117, `STORE` +168): a value
computed once and used twice must survive between the two, and on 27 registers
part of that surfaces as spill. That trade is real and it is still favourable.

### Verification

616 `cargo test` (610 + 6 new in `tests/phase4_tests.rs`), 17/17 both backends
with 17/17 parity under all four of {v1, v2} × {`mem2reg`, `--no-mem2reg`},
thatteOS 61/61, and the 1,147-file corpus sweep unchanged from its recorded
baseline on both language versions.

### F-2's inliner — and its premise is wrong too

`src/ir/inline.rs`. It splices a call to a small **single-block** callee into
its caller: the body goes where the `Call` stood, the caller's block is not
divided, no block is added and no jump. The counts that sized it:

| | |
|---|---|
| `Call` instructions across the 17 examples | 6,331 |
| to a callee whose body is in this module | 1,750 |
| non-recursive, callee ≤ 16 instructions | 1,101 |
| **callee is ONE block ending in `Return`** | **482 (44 %)** |

The other 56 % needs the caller's block split, the returns rewritten to jump to
a continuation, and a phi to join the values — the phi-on-its-edge surgery that
has cost this project three defects already (P11, P12, P14). Doing the easy
half first and measuring it is what says whether the hard half is worth the
risk. It also has a second reason: the multi-block case leaves three blocks
where there was one, and P26 says nothing merges them.

It runs **after `mem2reg` and before constant folding**, and both halves are
deliberate. After promotion, because a body still in allocas is twice the size
and mostly loads and stores, so a size limit measured against it measures
memory traffic. Before folding, because that is where it compounds: once a body
is spliced its parameters ARE the caller's arguments, so `scale(7, 3)` folds to
`22` and disappears entirely.

#### It shipped substituting nothing (P29)

The rename map was keyed on the bare parameter name; the lowerer spells a
body's parameters `param_<name>`. Every argument binding missed, silently, and
every spliced body arrived with its parameters still free. **13 of 17 examples
would not compile on LLVM** (`use of undefined value '%param_pad'`) and one
trapped on T3. One line. What makes it worth recording is that nothing in the
pass's shape objects to it — the splice is well-formed and the count of inlined
sites is exactly right — and that on T3 a free temp gets a register anyway, so
the arithmetic can come out plausible. LLVM rejecting the IR is what found it.

#### Then it made programs 5 % SLOWER, with byte-identical output (P30)

The first correct version was measured at **+5.11 % dynamic instructions** over
the 17 examples and **+159 % on `ternary_calculator`** — with 17/17 parity on
both backends, all 619 tests green, and the v1 corpus sweep on its recorded
baseline. Nothing that checks answers could see it.

The pass's most attractive candidate was `fmt::align_right`: 91 call sites, a
body of ONE instruction forwarding to `str::pad_left`. **It is a native.** The
`fmt`, `str`, `ternary`, `math`, `env`, `test` and `trit` modules are *mixed* —
`stdlib_expand` merges a ManiT body and each emitter also intercepts the call —
so on T3 `fmt::align_right` is `SYSCALL #15`, one instruction, and the merged
body is compiled and never reached. Splicing it is a correct transformation of
a body the backend had already replaced with something 188× cheaper.

An eleven-line repro, a twenty-iteration loop calling it, isolates it exactly:

| | dynamic instructions |
|---|---|
| `--inline-limit 0` | **564** |
| `--inline-limit 1` | **4,324** |

7.7×, from splicing one one-instruction body, output identical. And the limit
sweep is what proved it was not register pressure: **a limit of 1 already costs
the whole regression** and raising it to 24 slightly *recovers*.

The refusal is `backend_may_implement`, and it tests the MODULE, not a list of
intercepted names. The two emitters intercept different sets — `str::pad_left`
is a syscall on LLVM and compiled ManiT on T3 — the IR is shared, and a
name-level rule would be the union of two lists living in 64 match arms across
two backends. `SemanticAnalyzer::STDLIB_MODULES` is the boundary the rest of
the compiler already draws, it is a superset of what either emitter intercepts,
and it cannot go stale.

#### What it is worth

| | |
|---|---|
| call sites spliced, before the refusal | 318 |
| of those, stdlib — the whole of the regression | **240 (75 %)** |
| call sites spliced now | **78** |
| dynamic instructions, 17 examples | **−727 (−0.17 %)** |
| examples improved / unchanged / worse | **10 / 7 / 0** |

**So the recommendations' "biggest single win" is an order of magnitude smaller
than the CSE re-scoping that preceded it (−1.51 %).** That is the second time
in this phase that measuring a recommendation's premise has contradicted it,
after F-1's. The pass is kept because it is real and uniform — every example
neutral or better, none worse — and because it is the prerequisite for the
multi-block half. It is not kept because it was predicted to be large.

#### Verification

628 `cargo test` (619 + 9 new), 17/17 both backends with 17/17 parity under
{v1, v2} × {`mem2reg`, `--no-mem2reg`} and again under `--no-inline`, thatteOS
61/61, and the 1,147-file corpus sweep on both language versions **with the
pass on and off against the same binary** — the off runs being what separates
this pass from the rest of the session's uncommitted work:

| | compared | match | differ |
|---|---|---|---|
| v1, inline on | 574 | 561 | **13** |
| v1, inline off | 574 | 561 | **13** — the same 13 |
| v2, inline on | 550 | 548 | **2** |
| v2, inline off | 550 | 548 | **2** — identical |

v1 is exactly the recorded baseline. v2's recorded baseline is 559/17 of 576,
and the difference is the PREVIOUS session's v2 decision that an out-of-word
`int` literal is a hard `TypeError`: 26 corpus files now fail `check --lang v2`
on that error, which is precisely 576 − 550.

**One program in 1,147 changes its answer**, without moving a verdict:
`math_int/big3.mt`, already one of the 13, goes from printing a reshaped
out-of-word literal to trapping in the print path, because splicing changes
*where* a literal too wide for 27 trits gets reshaped. `literal-out-of-word`
names it under v1 and v2 refuses it outright, so it has no defined answer to
change. report.txt P30 has it.

Two of the new tests are about COST rather than output, because nothing about
P30 shows up in an answer: they run the repro under `run-t3 --max-steps 1200`,
which passes at 564 and fails at 4,324.

## P31 — the instrument, and the two defects it found in itself

The emulator has **always** collected a complete `ExecProfile`: total
instructions, a per-opcode histogram, maximum call depth, heap high-water.
`manitc bench` printed it; `run-t3` did not. So the dynamic instruction count
that P22 established as the right measure of an optimiser was obtained by
bisecting `run-t3 --max-steps` — about **forty emulator runs to read out a
number the emulator was already holding in a field**, with the histogram thrown
away at the end of it.

`run-t3 --profile` prints it: one prefixed line per fact, to stderr, the WHOLE
histogram rather than `summary()`'s top ten, `[T3ISA]`-prefixed so the sweep
scripts already filter it and stdout stays byte-identical.

**Within the hour it found two defects in the measurement path**, and neither
was findable with the bisection alone — because the bisection and the budget
shared the same wrong answer and agreed with each other perfectly.

### P32 — `--max-steps` was off by one

`run` left its loop on `!halted && steps < max_steps` and then asked
`steps >= max_steps`. The loop also exits when the program HALTS, so a program
halting on its last budgeted instruction was reported as cut off and `run-t3`
returned 71 instead of the program's own exit code. `run_debug` has always had
the same test in the right order.

Found by disagreement: `hello` bisects to 719 and its profile says 718.
**Every dynamic count ever taken by bisection was one too high** — deltas
unaffected, since both sides carried the same +1.

### P33 — the budget did not charge work done inside a callback

`call_fn_ptr` and `call_fn_ptr_2arg` are **re-entrant emulator loops**: a
syscall handed a maniT function pointer — `Vec::map`, `Vec::filter`,
`Vec::fold`, `for_each` — drives the callee itself rather than returning to the
main loop, and counted its own iterations against a private `1_000_000`.

```
examples/concurrency  --max-steps 26699  →  ran 30,299 instructions,
                                            printed all 71 lines, exited 0
```

So `--max-steps` was not a bound on a runaway program if the runaway was in a
callback, and bisecting it **under-reported exactly the programs that use
callbacks** — `concurrency` by 3,600, `data_structures` by 584 — silently. All
three loops now charge `profile.total_instructions`, and the two instruments
agree program-for-program where before they differed on two.

## P26 — block merging, and its premise is wrong as well

`src/ir/merge_blocks.rs`. Merges a block into its single predecessor, to a
fixpoint.

**The design question it was parked on dissolves.** P26 was left open because
`split_critical_edges` deliberately inserts empty blocks, so merging them looked
like it would undo phi placement. It cannot, and the reason is structural rather
than a matter of ordering: the merge condition is *A ends in a plain `Jump` to
B, and A is B's only predecessor*, while splitting inserts B on an edge whose
predecessor **branches** — so that pair never matches, and the far end does not
either, because the successor has several predecessors, which is what made the
edge critical. The two passes act on disjoint shapes. Merging also preserves the
no-critical-edge property: a successor's predecessor *count* is unchanged, since
B is replaced by A in the set rather than added to it.

### What it is worth

Every number measured over the 17 examples with `--no-merge-blocks` as the
control on the same binary:

| | |
|---|---|
| merges performed | 1,477 |
| blocks in the emitted `.t3s` | 14,754 → 13,327 (**−1,427**) |
| static `JUMP` | 7,730 → 6,303 (**−1,427**) |
| T3 binary bytes | 810,144 → 802,520 (**−0.94 %**) |
| dynamic instructions | 422,822 → 421,739 (**−1,083, −0.26 %**) |
| improved / unchanged / worse | **10 / 7 / 0** |
| **downstream passes changed on** | **0 of 17** |

One block removed is exactly one jump removed, and the two counts agree to the
unit. The dynamic saving is **entirely `JUMP`**: on `ternary_sort` the executed
histogram moves `JUMP 1081 → 813` and *no other opcode changes at all* — which
is also the sharpest available statement that the pass only relocates
instructions and never adds, drops or reorders one.

**And the rationale was wrong.** P26 was recorded as "the thing that would give
every other block-scoped pass something to look at". It changes no downstream
pass's output on any of the seventeen examples, because the blocks it merges are
**empty** — the predecessor gains no instructions, so nothing new comes into
view. That expectation was reasoning about block *count* when those passes need
block *contents*.

Note the measurement cannot be static, and not for P22's reason: a `Jump` is a
**terminator**, and `count_func` counts instructions, so `--pass-stats` reports
this pass as `+0` and always will. Only the emitted code and the execution
profile can see it.

## F-2's inliner, the multi-block half — and a third premise contradicted

`src/ir/inline.rs`, the CFG path. The 56 % the single-block path declined. The
caller's block is split around the call, the callee's blocks are copied between
the halves, every `Return` becomes a jump to the continuation, and a phi in the
continuation joins the returned values.

### Two defects, and neither instrument caught both

**P34 — a terminator's OPERANDS were never renamed, only its labels.**
`BinBranch` and `TritBranch` each carry a COND as well as target labels;
`retarget_term` put every label through the renaming function and cloned the
condition verbatim. The spliced branch therefore tested a temp only the
ORIGINAL callee defines, the copy that defined it under its new name was used
by nobody, and dead-code elimination removed it.

It is P29's failure mode exactly: the splice is well-formed, the site count is
right, and on T3 the free temp gets a register and the program prints something
plausible — `pick(-4)` printed 4, correct by luck, and `pick(7)` printed −7,
because both copies kept the same free name and the second call inherited the
first call's decision.

**`--verify-ssa` reports it in one command, and has since F-1**: 65 violations
across 7 of the 17 examples with the pass on, 0 with `--no-inline` on the same
binary, 0 with the fix. The instrument existed throughout and was not run. That
is P31's lesson arriving from a second direction, and the rule it yields is:
**an optimiser pass that rewrites control flow is checked against the SSA
verifier before it is checked against any program's output.**

**P35 — `IRValue::Void` reached a phi arm.** A value-returning function reaches
`Return(Some(Void))` more often than it sounds: the lowerer gives an exhaustive
`match` a trailing `match_nextN` block no arm matched, and every `Result` match
in the standard library has one. On a `ret` both backends coerce it; a phi arm
gets no coercion, and `Void` renders as the EMPTY STRING —
`phi ptr [ %t8, %arm1 ], …, [ , %next8 ]` — which is not parseable.

| instrument | what it saw |
|---|---|
| T3 backend | accepted it, ran, printed the right answer |
| `--verify-ssa` | **0 violations** — `Void` is not a temp, and the arm is present rather than missing |
| LLVM backend | refused the module at parse |

The fix is one line: `lower::helpers::sanitize_phi_incoming`, which the lowerer
has used since it started emitting phis. The inliner is the compiler's only
OTHER producer of phis and was not using it.

### P36 — it was a pessimisation, and the reason is in the T3 backend

Correct and SSA-clean, the CFG path made the 17 examples **423,782 dynamic
instructions against 421,739 with it off — +2,043, +0.48 %.** The third
Phase-4 premise contradicted by measuring it.

**Sweeping `--inline-limit` said it was not a size effect.** The whole
regression appears at 12 and the curve is flat on either side:

| limit | 4 | 8 | 10 | 12 | 16 | 24 |
|---|---|---|---|---|---|---|
| total | 421,139 | 420,679 | 421,896 | 423,782 | 423,782 | 423,782 |

One callee in each of two programs crosses at 12 — `fibonacci`'s
`fib_iterative` at 5 sites, `neural_net`'s `print_weights` / `print_vec` /
`neuron` — and each contains a `while` loop.

**The opcode histogram named the mechanism, and register pressure was the
obvious hypothesis and wrong for the second time in this phase.** On an
eleven-line repro — a callee that loops 60 times, called three times with
CONSTANT arguments — splicing costs +165 (+7.3 %) and the executed histogram
moves `TLIT +180` against 183 iterations, with every other opcode moving by
exactly the three call frames saved (`CALL −3, RET −3, MOV −3, TMAX −3,
TCMP −3, TSUB −3`).

**The emitted loop shows it directly**, which is better than inferring it from
the histogram. The condition block of the same `while`, un-spliced and spliced:

```
accum_while_cond4:              main_il0_while_cond4:
                                  TLIT  R8, #60     <- EVERY ITERATION
  TCMP  R6, R4, R1                TCMP  R7, R5, R8
  TNEG  R6, R6                    TNEG  R7, R7
  TMAX  R6, R6, R0                TMAX  R7, R7, R0
  TLIT  R7, #1                    TLIT  R9, #1
  TMIN  R6, R6, R7                TMIN  R7, R7, R9
  TBRANCH ...                     TBRANCH ...
```

Un-spliced, the bound is `R1` — the PARAMETER, already in a register. Spliced,
it is the literal 60, and nothing hoists it out of the loop.

**The decisive experiment is the same callee with an OPAQUE argument:**

```
constant argument   2,274 → 2,439   +165
opaque argument     5,238 → 5,229     −9
```

So the cost is THE CONSTANT, not the loop and not the frame. Splicing binds
each parameter to the argument VALUE — the whole reason the pass runs before
constant folding, since a constant argument makes the body foldable. Inside a
loop it inverts: a bound that was a PARAMETER lives in a register and is
compared once per iteration, and substituted it is a literal the backend has
nowhere to keep, because there is no loop-invariant code motion to hoist it.

A branch-only callee called 180 times FROM a loop is **−13.6 %** on the
matching repro, so the refusal is about a loop in the CALLEE and says nothing
about the caller.

### What it is worth

`collect` refuses a callee whose CFG has a back edge, using the dominator graph
`ssa.rs` already builds.

| | dynamic instructions over the 17 |
|---|---|
| no inlining at all | 422,466 |
| single-block path only | 421,739 |
| **both paths, with the loop refusal** | **420,668 (−1,071, −0.25 %)** |
| improved / unchanged / worse | **7 / 10 / 0** |

The multi-block half is worth slightly MORE than the single-block half (−727),
and the two together take the pass to −1,798, −0.43 %.

**That the refusal addresses the mechanism rather than fitting a constant to
seventeen programs is visible in the flatness it produces**: the spread across
limits 8 to 24 collapses from 3,103 instructions to 15, and the size limit
stops being a performance knob.

| limit | 8 | 16 | 24 |
|---|---|---|---|
| with the refusal | 420,658 | 420,668 | 420,673 |

**P36 stays OPEN, and it is in the backend rather than the inliner.** `TLIT` is
loop-invariant and nothing hoists it; there is no rematerialisation and no LICM
pass at all. Fixing that would pay for far more than this refusal gives up
(about 3 instructions per site on the opaque-argument case) — it is a property
of every loop the compiler emits, not a property of inlining.

## P37 — the peephole absorbed a comparison something else still read

`ternary_peephole`'s trichotomy collapse, from this phase's own P22 repair.
**The one finding in the phase that made a shipped program compute a wrong
answer, and no test caught it.**

The pass finds two chained sign tests on one value, replaces the first block's
terminator with `TritSign` + `TritBranch`, and lets the second block — already
required to hold NOTHING BUT the comparison — become unreachable. Those are
different questions. *The block holds no other work* is not *the block's VALUE
has no other reader*, and the pass never asked the second.

**The shape is ordinary code**: a sign tested once to normalise a value and
again at the end to put the sign back.

```
fn to_balanced_ternary(n: int) -> str {
    if n == 0 { return "0"; }
    let mut val = n;
    let neg = val < 0;             // ONE definition
    if neg { val = 0 - val; }      // absorbed into the TritBranch
    ...
    if neg { ...flip the trits... }   // still reads it
    return result;
}
```

That function is copied verbatim into **five** thatteOS files.

| | T3, working tree | correct |
|---|---|---|
| `to_balanced_ternary(5)` | `+--` | `+--` |
| `to_balanced_ternary(-5)` | `+--` | **`-++`** |
| `to_balanced_ternary(4)` | `++` | `++` |
| `to_balanced_ternary(-4)` | `++` | **`--`** |

**Every negative input returned the positive number's representation**,
silently, because on T3 a free temp gets a register like any other. On LLVM the
module did not link at all — `use of undefined value '%t7'` — which is the only
reason it was not silent everywhere.

### How it was found, and why nothing else could have

By running `--verify-ssa` over every `.mt` file in thatteOS, which nothing had
ever done: five files, one `Undefined` violation each, all naming the same
function.

**The flag bisection MISLED.** The defect clears under `--no-mem2reg` and
survives `--no-inline`, which points squarely at promotion. Diffing the IR
against the pre-campaign `target/release` binary named the real pass in one
look: two `BinBranch`es on `t2` and `t7` became one `TritSign` + `TritBranch`,
and `t7`'s definition went with the absorbed block.

`f2_a_side_effect_between_the_two_comparisons_is_not_absorbed` already existed,
so the pass HAD been thought about — but it asks whether there is other WORK
between the comparisons, not whether there is another READER of the comparison.

### The procedural half

**`thatteos/build.sh` prefers `manitc/target/release/manitc`.** That binary was
last built on 25 August at 02:47 and predates the whole uncommitted campaign,
so **every "thatteOS 61/61" recorded during this campaign was measured against
a compiler from before the changes it was supposed to validate.** Built with
`MANITC=<working-tree binary> ./build.sh` it is 61/61 with this fix, and does
not link at all without it.

### The fix, and what it costs

A third condition: refuse the collapse if the second comparison's temp is read
anywhere but the terminator being absorbed — counting phi operands, since a phi
in a target block could read it across the very edge the pass retargets. The
FIRST comparison needs no such test, because its block KEEPS its instructions
and dead-code elimination then removes the temp exactly when it is genuinely
unused.

**Cost: none, and measured directly rather than inferred.** The pass performs
**55 collapses across the 17 examples with the new condition and 55 without
it** — it declines not one of them. The four example configurations agree to
the digit, 422,466 / 420,668 / 420,658 / 420,673. So every collapse the
examples contain is a case where the comparison had no other reader, and the
refusal is exactly as narrow as it should be. (P22 recorded 53; it is 55 now
because the multi-block splice creates two more collapsible shapes.)

## P38 — the image never checked against the stack, and the pass never bounded

Two findings that only meet at a threshold, and **the corpus sweep's
`--no-inline` control is the only thing that separated them.**

### The image can run into the stack, and nothing said so

The emulator's memory map puts code at 0 growing UP, string literals at
`code_size + 1024`, then floats — while the stack starts at 60,000 and grows
DOWN. No pass, emitter or assembler compared the two. A program whose image
reaches 60,000 words overlaps its own stack: the first `CALL` writes a return
address over an instruction, and execution eventually fetches a stack word:

```
TRAP: unknown opcode 11865895301 at PC=59383
```

which names the SYMPTOM — a word that is not an instruction — and says nothing
about size.

Measured by lengthening one program's `main` and compiling with `--no-inline`,
so that nothing but source length changed:

| | |
|---|---|
| 59,991 words | runs correctly |
| 60,004 words | **TRAP** |

Nothing in between, and no diagnostic anywhere. **This is not an inliner
defect** — it is reachable by writing a long enough program, and it predates
this campaign entirely.

`assemble()` now computes the image top and refuses `>= STACK_BASE`, which is
`pub` in the emulator because the emulator owns the map.

**What the check costs, measured over the 1,147-file corpus: exactly ONE file
is refused, and the same one with inlining and without** — `math_log/tbig.mt`,
90,904 words under `--no-inline` and 99,828 with. It had been compiling and
producing a cross-backend divergence, which is what silent corruption looks
like from outside. The check turns one wrong answer into one honest error and
touches nothing else. The bound is the HARD
OVERLAP rather than a headroom estimate: how much stack a program needs is
dynamic, so `>= STACK_BASE` is the one line certainly wrong for every program
rather than arguably wrong for some.

### `SIZE_LIMIT` bounds one splice, not the pass

A body of twelve instructions spliced at 597 sites is 597 legal splices. On
`oracle/census/math_agent_work/math_log/sweep0.mt`, `relerr18` is exactly that:

| | |
|---|---|
| IR instructions | 4,084 → 12,978 (**+218 %**) |
| emitted T3 words | 26,245 → 94,473 (**+260 %**) |

and 94,473 words does not fit below 60,000. **Fourteen programs of the
1,147-file corpus stopped working.**

**The control is what diagnosed it.** The v1 sweep with inlining ON gave 27
divergences against a recorded baseline of 13; `--no-inline` **on the same
binary** gave exactly 13. That turned "something regressed" into "the inliner,
and by fourteen" in one run.

Fixed with a module-wide growth budget, charged per splice in the callee's
instruction count and shared by both paths: `max(64, 20 % of the module's
pre-inline size)`.

**The measurement sets the numbers, and one example sets the FLOOR.** Across
the seventeen examples the pass adds 0.0 %–2.8 % — except `patent_classify`,
whose entire module is 21 instructions and which grows 71 % by gaining 15. A
percentage alone would refuse that; a floor of 64 admits it.

| | |
|---|---|
| largest absolute example growth | +111 (`ternary_calculator`), budget 800 |
| examples on which the budget binds | **none** |
| all four example configurations | **identical to the digit** before and after |
| sweep0 | +8,894 → **+654**, image 20,343 words, output matches `--no-inline` |

**Still open: 60,000 words is a small ceiling for a compiled program**, and it
is the emulator's choice rather than the ISA's — the ISA fixes a 65,536-word
space and the map spends 5,536 on globals, scratch and heap. Raising it means
moving the stack, the globals window and the heap together, which is a
memory-map change wanting its own measurement. The check turns silent
corruption into a clear error; it does not make big programs work.

## Verification of the whole of it

| | |
|---|---|
| `cargo test --no-fail-fast` | **647 passing, 0 failing**, 0 warnings |
| examples, both backends, 6 flag combinations | 17/17, parity 17/17 |
| thatteOS with the **working-tree** compiler | 61/61 |
| `--verify-ssa` — examples, thatteOS, corpus | 0 / 0 / 0 violations |
| R5: `manitc check` verdicts vs the pre-campaign binary | **0 differences** over 271 repo files and 1,147 corpus files |

Cross-backend corpus sweep, against the pinned binary `manitc-v3`
(`a9ef42fc…`) over the list pinned at 02:26:

| configuration | compared | match | **diff** |
|---|---|---|---|
| v1, inline ON | 574 | 562 | **12** |
| v1, inline OFF | 574 | 562 | **12** |
| v2, inline ON | 550 | 549 | **1** |
| v2, inline OFF | *interrupted by a planned reboot* | | |

**The v1 pair is the result that matters.** Identical counts and, checked
directly, identical FILE SETS — the only difference in 1,147 programs is
`math_int/big3.mt`'s exit code (`rc 70 vs 0` with inlining, `rc 0 vs 0`
without), the P21-cluster-1 file P30 already recorded, which diverges either
way. **So the multi-block path contributes no divergence**, as the
single-block path also measured.

Against the recorded baselines — v1 561/13 of 574, v2 548/2 of 550 — one file
left each set, and neither started agreeing: `math_log/tbig.mt` is 90,904 words
even without inlining and is now refused by the P38 image check, and
`bench_T1-12__main.mt` no longer compiles under v2.

## Not done

- **F-4, heap management / regions.** Untouched, and now the only Phase 4 item
  left. N2 forbids a garbage collector; regions plus affine types (B7,
  Phase 5) are the intended fit.
- **P38's address space.** OPEN. 60,000 words of code is the emulator's memory
  map, not the ISA's 65,536-word limit. Moving the stack, globals and heap up
  together is the change; the assembler now refuses an over-large image rather
  than corrupting it.
- **P36 — loop-invariant code motion in the T3 backend.** STILL OPEN, but it
  is no longer the largest thing here and **its premise turned out to be
  wrong** — see P40 below, which is what looking at it produced. LICM would
  not have removed the TLITs P36 points at, because a constant is an OPERAND
  in this IR and not an INSTRUCTION, so there is nothing in the loop for a
  code-motion pass to lift. What remains genuinely open is the narrow case P36
  measured: a constant substituted into a loop BOUND by the inliner, which is
  why the inliner still refuses callees with a back edge.
- **P26 is DONE** — `src/ir/merge_blocks.rs`, above. The entry that stood here
  called it "a design question rather than a patch"; the design question
  dissolved structurally.
- **P13's real fix** landed with F-1's follow-up; the backend now truncates at
  the call's DEFINITION rather than at each use.

## P40 — the immediate operand the emitter never used

**Found by going to look at P36, and it contradicts P36's premise. It is the
largest single performance finding of the campaign: 420,668 → 376,172 dynamic
instructions over the seventeen examples, −44,496, −10.58 %.**

Every data-processing opcode on T3ISA takes its third operand as either a
register or a balanced 3-trit immediate. `assembler.rs::reg_or_imm_pair`
encodes `#k` as register R0 — always zero — with `k` in the imm field, and
`execute.rs` computes `rhs_eff = regs[sr3] + imm`. **`TADD Rd, Ra, #5` and
`TADD Rd, Ra, Rb` are the same instruction with a different third register.**
The emitter used the form for the shifts and the frame push/pop and nowhere
else: `val_reg` on a constant took a scratch register and emitted a TLIT.

**TLIT was the most-executed opcode in the language** — 78,293 of 420,668,
18.6 % of everything the examples ran — and a scan of the emitted assembly put
9,553 of 26,191 static TLIT sites (36 %) in the directly collapsible shape.

| stage | dynamic | Δ |
|---|---|---|
| baseline | 420,668 | |
| (1) binary operators | 398,936 | −21,732 (−5.17 %) |
| (2) `GetPtr` + (3) the dead clamp | **376,172** | **−44,496 (−10.58 %)** |

Tranche (3) is a deletion rather than a substitution: `<` and `>` ended with
`TLIT o, #1` / `TMIN d, d, o`, and `TCMP` writes `sign_i64` ∈ {−1,0,+1}, which
`TMAX d, d, R0` leaves in {0,1} — so the clamp was the identity. Two
instructions on every `<` and `>` in the language.

**The signature is what makes it believable.** Exactly two opcodes move and
their deltas sum to the total: TLIT 78,293 → 43,274 (−35,019) and TMIN
13,531 → 4,054 (−9,477). Every other opcode is identical to the digit.
**15 improved, 2 unchanged, 0 worse**; images 162,044 → 151,309 words (−6.6 %).

For scale against the rest of the phase, measured the same way: CSE re-scoping
−1.51 %, the inliner (both halves) −0.43 %, block merging −0.26 %. **This is
more than all of Phase 4's optimiser work put together, and it is not an
optimiser pass — it is instruction selection that was never written.**

**The test went hollow first.** `f2_a_small_constant_operand_is_spent_as_an_immediate`
first asserted `asm.contains(", #13")`, which `TLIT R6, #13` satisfies exactly
as well as `TADD R4, R5, #13`; it passed with the change reverted. It now
parses the line and checks the operand slot. Reintroducing the defect is what
caught that, and both new tests were re-checked that way.

## P41, P42 — two defects found by running things that were already there

- **P41.** Syscall #218 bumped `heap_ptr` twice — once inside P39's
  `heap_reserve`, once in a line the extraction left in the caller — so every
  struct allocation charged 2n words for n. **Its unit test was already red at
  HEAD `1d1b5e7`**, which was gated on `cargo check --all-targets`; that builds
  test targets and never runs them, and the commit message repeats a "647
  passing" table copied rather than measured.
- **P42.** `thatteos/userspace/build.sh` still did the three hand-link steps
  `../build.sh` deleted on 23 August, so **userspace had not built since 10
  August** and `tests/test_all.sh` was running sixteen-day-old binaries. Worse:
  every userspace test is guarded on the binary existing, so with `bin/` empty
  the suite runs 27 tests, passes 27 and prints **"ALL TESTS PASSED"** — 34 of
  61 assertions vanish and the summary still reads green. Fixed by the same
  delegation. thatteOS is now 61/61 with **both halves** built by the
  working-tree compiler, which is the first time that has been true.

## P43, P44, P45 — three defects handed over by the oracle probes

Recorded in `report.txt` and pinned as `tests/generic_impl_tests.rs`: nineteen
programs, ten controls that pass and nine `#[ignore]`d rows carrying their
finding id. Payload-enum constructors emit an undefined symbol (and **LLVM
writes the module and exits 0**); a generic struct crossing a method, a `Vec`
or a function boundary reads the wrong field **on both backends**; and a
`<T: Ord>` bound is never checked against the argument type, so the fix the
language reference prescribes for that exact bug does not bind.

**P44 is the one that bears on this phase's own evidence.** The cross-backend
parity matrix has carried most of the correctness argument here, and it reports
17/17 on a program that prints `2 2` where it should print `2 1` — because both
backends print it. **Parity is evidence about the parts of the two backends
that DIFFER; a shared lowering shares its bugs.**


## Verification of P40, P41 and P42 together

| | |
|---|---|
| `cargo test --no-fail-fast` | **659 passing, 0 failing, 9 ignored**, 0 warnings |
| `cargo test -- --ignored` | **9 failing, deliberately** — P43/P44/P45's standing list |
| 17 examples × 6 flag combinations × 2 backends | **17/17, parity 17/17** (output AND exit code) |
| 17 examples, output vs the pre-change binary | **0 of 17 differ**, all exit codes identical |
| dynamic instructions | **420,668 → 376,172, −10.58 %**; 15 better, 2 unchanged, 0 worse |
| `--verify-ssa`, 17 examples (both stages) and all 55 thatteOS sources | **0 violations** |
| thatteOS, **kernel and userspace both** built by the working-tree compiler | **61/61** |
| R5: `manitc check` verdicts vs the pre-change binary, 271 repo files | **0 differences** |
| corpus sweep, v1 inline ON, 1,147 files | **13 diffs of 574 — and the control on the pre-change binary gives 13 with IDENTICAL diff and trap file sets** |

The six flag combinations are `{none, --no-inline, --no-mem2reg,
--no-merge-blocks}` plus `{--lang v2, --lang v2 --no-inline}`.

**R5 holds structurally as well as by measurement**: `manitc check` runs neither
the assembler nor the emitter, so nothing in P40 can reach it. Measured anyway.

**The sweep needed a CONTROL, not a baseline.** Against the previous handoff's
recorded 12 diffs the change looks like +1. The extra file is
`bench_T1-13__shortest_len.mt` and the cause is P39's heap bound check, which
landed after `manitc-v3` was pinned: under that binary the program printed 84
lines past an exhausted heap instead of trapping. Running the pre-change binary
over the same list gives the same 13, file for file.

**Edge cases checked directly rather than argued**: ±13 (the field's edge) is
spent as an operand and ±14 falls back to TLIT; negative immediates; `trit` and
`bool` constants; truncating `/` and `%` with negative operands; and division
by a literal zero, which both binaries refuse at compile time identically.


## P46, P47 — a default-build failure, and the harness that hid it

**P46 belongs to this phase specifically**: it became reachable when
`--mem2reg` was made the default on 24 August. `Vec::get` is declared `i64`
while the IR knows the element type, so on a `Vec<str>` the promoted phi is
`ptr` with an `i64` arm and clang refuses the module. `--no-mem2reg` compiled
it throughout.

It is **P13's shape with the conversion pointing the other way**, and P13's own
comment already stated the principle — reconcile at the DEFINITION, because a
phi is the one construct whose type does not come from its operands. P13
implemented it for "declared integer is wider" only. `inttoptr` is one more arm.

The symmetric `ptrtoint` case was written, and is wrong: a `Result` handle
comes back from `@Ok` as a declared `ptr` that the backend then uses as an
address. Three tests red, and a corpus sweep that compiled 493 files of 1,147
against the correct version's 574.

**P47 is why the suite was green.** `run_llvm` skipped whenever a failed
compile mentioned clang, calling it "no toolchain in this environment" — but an
absent toolchain makes the compile SUCCEED, so the only way into that branch is
clang rejecting the module. 34 call sites, each `if let Some(..)`, each
vacuous on exactly the defects they exist to catch.

**`--verify-ssa` is correct to report 0 on P46 and cannot be blamed**: it
checks single assignment, dominance and phi edges, not operand types. That is
the extension to write next, and it is the natural successor to `VoidPhiArm`.


## P44, P45 — the two generic-type defects, fixed

**P44 was the only kind of defect this campaign ranks first: a silent wrong
answer on both backends, from a program `check` exits 0 on.**
`impl<T> Pair<T> { fn swap(self) -> Pair<T> }` printed `2 2` where it should
print `2 1`.

`field_slot_index` keys `self.structs` on the DECLARATION name. A struct
LITERAL has bare type `Pair`, which is that name — so a field read straight off
a literal was always right, and only a value that had crossed a method, a `Vec`
or a function boundary carried the declared `Pair<T>`, whose `ManiType` is
`Generic("Pair", [..])` and whose `display()` is `Pair<int>`. Never a key. The
lookup fell to `unwrap_or(0)` and every field read slot 0.

One match arm. It is exact rather than approximate, and the reason is worth
stating: **a struct's layout does not depend on its type arguments** — every
field is one machine word and there is one `structs` entry per declaration — so
the base name IS the right key and no monomorphised key is wanted. (P65 is the
case where that argument does NOT hold.)

**The `unwrap_or(0)` is now a `debug_assert!`, and that is the half that
generalises.** The same silent fallback swallowed the 20 August tuple defect one
type constructor earlier. Measured before being made an assertion, with a
temporary per-lookup trace so the sweep had a POSITIVE CONTROL — **8,169
lookups over 1,442 files, zero misses**, T3 and LLVM agreeing to the unit
because the function runs before the backends split. That agreement is also why
the parity matrix was 17/17 throughout: a shared lowering shares its bugs.

**P45 was two registries deciding one question and disagreeing** — P60's shape.
`binop_type` decides `<` and `>` by `ManiType::is_comparable`. `type_satisfies`
decided `Ord` by "is it a primitive", written as
`!matches!(ty, Struct | Enum | Unknown | Fn)`, and `str`, `[int; 2]`,
`(int, int)`, `Vec<T>` and `Result<T, E>` are none of those four. All five
satisfied `Ord` while the operator rejected every one. Tying the trait to
`is_comparable` — the same call the operator makes — fixed all five; naming
`str` would have fixed one.

**The second instance was in the diagnostic and was worse than the defect.** It
said "add `impl Ord for str`". Doing so made the program compile and print the
wrong answer again, because `>` dispatches to nothing: `trait_impls` is read at
exactly two sites and neither is in the lowering of a binary operator. So the
ordering rule had to be decided BEFORE the user-impl escape hatch rather than
after it — **an ORDERING change, not a predicate change** — and that also closes
**A4's own struct case**, which had been open the whole time through the remedy
A4 itself prescribed.

The test crosses an origin boundary: for fifteen types it compiles `x > y` and
the same comparison through `gt2<T: Ord>` and requires the two VERDICTS to
agree, never stating which types are ordered. **Each row carries a control with
no comparison in it**, because a malformed program is rejected both ways and
therefore AGREES — the vacuous pass is the failure this test shape invites.

## P65 — the fix P44 and P45 both stopped short of

Type erasure. A generic function is lowered ONCE with its type parameters bound
to `Unknown`, which lowers as `I64`. The body's `>` is an INTEGER compare
whatever `T` is, and the call site coerces the argument with a **value-changing**
`Cast { from_ty: F64, to_ty: I64 }`.

`largest(1.5, 2.5)` truncates to 1 and 2, compares as integers, returns the
integer 2, and `io::print_float` reads it as a BIT PATTERN: 1e-323, printed to
323 places. `largest(-1.5, -2.5)` returns -1, all ones, which is a quiet NaN.
**Both lines were predicted before being measured and both match to the digit**,
which is what says the mechanism is the whole mechanism.

`int` and `trit` are unaffected **because their representation IS the erasure**.
A defect in a type-erasing lowering is invisible for exactly the types the
erasure happens to pick.

The second costume was found by P44's new assertion within the hour: the RETURN
type is not substituted either, so `fn id<T>(x: T) -> T` followed by
`id(p).second` looks up a field on `<unknown>` and reads slot 0. **The shipped
release binary prints `1 1` where it should print `1 2`, on both backends.**
The fsi sweep covered 1,442 files and found zero misses; this program is in
none of them and took an hour of ordinary probing to write. **Zero misses over a
corpus is a statement about the corpus, not about the language.**

## P66 — and the last sentence of the previous section is wrong

The section above this one ends: "That is the extension to write next, and it
is the natural successor to `VoidPhiArm`." **It was written, measured and
withdrawn.**

Built as a `PhiArmType` violation — a phi arm whose defining instruction states
a type other than the phi's, which is exactly P46's shape. On the 17 examples,
the same 17 that pass 17/17 on both backends across six flag combinations, it
reported **1,107 mismatches in 15 of the 17 files, in 31 distinct type pairs,
334 of them mixing float with an integer type**. 740 more across thatteOS.
Nothing about those files is broken.

**The IR has no phi type invariant to verify.** `codegen_llvm/emit_instr.rs`'s
`Phi` arm computes the LLVM type from the ARMS — starting from `llvm_type(ty)`,
then widening the whole phi to `ptr` if any incoming value is a pointer — and
records that as the temp's type. The IR's `ty` on a phi is advisory.

**Why `VoidPhiArm` generalised and this does not**, which is the distinction to
keep: `IRValue::Void` in a phi is UNREPRESENTABLE — there is no token to emit,
so the module will not parse — and that is a property of the IR alone. A type
mismatch is representable and a later stage RESOLVES it, so it is not a property
of the IR alone, and no narrowing turns it into one. **Ask whether the property
you are about to verify is one the IR owns, or one a later stage decides.**

P46's blind spot is therefore not closed, and its real shape is now named: the
IR is untyped in practice, and closing it means giving the IR a typing
discipline both backends respect — work on the scale of monomorphisation, not a
verifier patch.

## Verification of P44 and P45 together

| | |
|---|---|
| `cargo test --no-fail-fast` | **689 passing, 0 failing, 7 ignored**, 0 warnings |
| 17 examples, both backends, 6 flag combinations | **17/17, parity 17/17** |
| thatteOS, both halves built by the working-tree compiler | **61/61**, 10 userspace binaries |
| `--verify-ssa`, 17 examples + all 55 thatteOS sources | **0 violations over 72 files, 0 files missing the denominator line** |
| R5, repos | **2 differences over 295** — both the P45 fixtures the change is about |
| R5, model corpus | **0 differences over 1,147** |
| `field_slot_index` misses | **0 over 8,169 lookups in 1,442 files** |
| binary | `c19387169f5ff7e3` (`/var/scratch/tmp/manitc-p45-final`) |

Both fixes were checked by REINTRODUCING the defect. P44: exactly the four
`gs62_*` rows go red and the 22 controls stay green, with the assertion naming
the finding by id rather than printing a plausible wrong number. P45: the
agreement test names all five disagreeing types and what each does.


## P65 — monomorphisation of generic free functions

Done, for free functions. The `impl<T>` method half is open and is blocked on
something else (below).

**The type work is one line, because the machinery was already there.**
`resolve_type` consults `self.type_params` before any other rule, and
`check_fn` fills it with `Unknown` for each of the function's generics. Bind it
to the CONCRETE types and check the same AST again, and the result is a fully
concrete `TypedFnDef` — `a > b` becomes a float comparison, `-> T` becomes
`-> float` — with no substitution pass over the AST at all. A
`TypedProgram`-level transform was considered and is the wrong shape: by then
`T` has been erased to `Unknown` and is indistinguishable from every other
`Unknown`.

**Two halves at the call site.** The NAME is rewritten to the instantiation,
which makes the body compile with real types. The RETURN TYPE is recomputed
under the binding, which stops the result arriving as `Unknown` — the half
responsible for `id(p).second` reading slot 0. Fixing only the name fixes only
the float row.

**A failed instantiation is discarded, not reported, and that is the shape of
the increment.** Checking a body with its real types finds errors the erased
copy could not. The first version reported them and four tests went red — two
of which state a deliberate design in as many words:
`b1_an_unbounded_generic_is_unchanged` says "Bounds are opt-in. A bare `<T>`
must still compile exactly as before … because inferring one would reject
programs that check today." So an instantiation that does not check is thrown
away and the call keeps the erased path. **The change can only fix a program,
never break one**, which is why it needs no R2 version bump and is not an R5
event. Making a failed instantiation a hard error — which closes A4 for good —
is a separate language decision and is deliberately not taken here.

**Re-entrancy exposed a latent defect in `check_fn`.** Instantiation happens
from inside expression checking, so `check_fn` re-enters; its body check is
`self.check_block(block)?`, which on error returns without popping the scope it
pushed or restoring `current_fn`/`current_fn_ret`. Invisible for its whole life
because a failing analysis aborts. `ensure_mono` snapshots and restores all
four, and `SymbolTable` grew `depth`/`truncate_to`.

### Measured

| | |
|---|---|
| `cargo test --no-fail-fast` | **693 passing, 0 failing, 7 ignored**, 0 warnings |
| 17 examples, both backends, 6 flag combinations | **17/17, parity 17/17** |
| thatteOS, both halves | **61/61**, 10 userspace binaries |
| `--verify-ssa`, 72 files | **0 violations**, 0 files missing the denominator line |
| R5, repos / corpus | **0 differences over 295** and **0 over 1,147** |
| dynamic instructions, 17 examples | **376,172 → 376,172, identical to the digit** |
| image size, 17 examples | **+1 word in total** (in `oop`) |
| reach | **16 instantiations across 10 files**; thatteOS generates none |
| behaviour diff, repos | **1 of 187 programs**, and it is `ord63_float_via_bound` |
| behaviour diff, corpus | **0 of 583 programs**, none compiled by only one binary |

Reintroducing it turns exactly three rows red — `ord63_float_via_bound`,
`p65_generic_return_field`, `p65_two_instantiations` — and nothing else.

`p65_two_instantiations` is the row that would still mean something under a
different fix: ONE generic, FOUR calls, TWO types, interleaved. A single body
has to pick a comparison and a width, and `-1.5 > -2.5` separates a float
comparison from a bit-pattern one, because IEEE-754 negatives order the
opposite way to their bit patterns read as integers.

### The open half, and why it is not "the same change for methods"

An `impl<T>` method is still compiled once with `T` erased, and still prints
1e-323 for a `Box2` of floats — measured, and pinned as
`p65_impl_method_still_erased`. A method's binding would have to come from the
RECEIVER's type, and **a generic struct literal does not carry its type
arguments**: `Box2 { a: 1.5, b: 2.5 }` has bare type `Box2`, not `Box2<float>`.
That is the same missing inference that keeps `gs62_generic_freefn` broken —
P44's third, honest defect, where a bare `Pair` never unifies with `Pair<T>`.
One fix serves both, and it is type inference on struct literals rather than
more monomorphisation.


## P67 — the name was the defect

`resolve_type`'s `Type::Generic(name, args)` arm matched a hardcoded list of
built-in generic constructors BEFORE asking whether the user had declared a
struct of that name, and **`Pair` was on the list with nothing behind it**.
Measured: outside that one line the name appears in a warning-suppression list
and nowhere else — no stdlib source, no IR, no backend. Every other entry is
real.

So a program declaring `struct Pair<T>` had its ANNOTATIONS resolve to
`Generic("Pair", [..])` while its LITERALS resolved to `Struct("Pair")`, and
those do not unify. `fn swap<T>(p: Pair<T>)` could not be called with a `Pair`.

**The experiment is one `sed`.** Rename `Pair` to `Duo` throughout the same
program: it compiles and prints `2 1` on both backends.

**And it is P44's cause.** P44 was recorded as "a generic struct is correct as
data and wrong through every boundary" and fixed with a `Generic` arm in
`field_slot_index`. That fixed the symptom; the reason a user struct's declared
type ever arrived there as a `Generic` was this phantom. Measured: with P44's
arm DELETED and P67 fixed, all 36 rows of `generic_impl_tests` pass and the
miss assertion fires on none of the 295 `.mt` files in both repos. The arm is
kept as defence in depth for the nine names that DO have implementations, and
its comment now says so.

**Why the original generalisation was wrong**, which is the transferable part:
the probe session put §62 at "the INTERSECTION of generic and crosses a
boundary" on the strength of ten one-variable-apart programs. **All ten spelled
the struct `Pair`.** A one-variable-apart family is evidence only about the
variables it varies, and a type's NAME reads like notation rather than like a
variable — which is exactly why nobody varied it. **When a family of probes
agrees, ask what every member HOLDS FIXED.**

Open: the nine names with real implementations (`Vec`, `Map`, `Set`, `Deque`,
`TernaryTrie`, `Channel`, `Mutex`, `Result`, `Range`) still shadow a user
struct, and do it silently — the program is refused with
`expected Vec<<unknown>>, found Vec`, naming neither cause nor remedy.
`p67_a_struct_name_must_not_change_the_program` compiles one program under
thirteen names and asserts BOTH halves, so it encodes the convention rather
than imposing one.

## P68 — a generic struct could not hold a float

`Box2 { a: 1.5, b: 2.5 }` stored the integer 1; `p.a` read back 5e-324. A
non-generic `struct B { pub a: float }` was correct, which localised it.

**Two lines of one loop, four statements apart, disagreed.** The struct-literal
lowering COERCES each field value to the DECLARED field type and STORES it with
the VALUE's own type. A generic struct's fields are registered `Unknown` — a
struct's type parameters are not in scope when it is registered — so the
coercion targeted `i64` and truncated, while the store said `F64`. The IR shows
a `Cast { from_ty: F64, to_ty: I64 }` immediately before a `Store { ty: F64 }`,
and the non-generic control has no `Cast` at all.

**Coercing into an unknown type is P65's shape for the third time**, and the
fix is the same: `if fmt.is_known()`.

The other half: **`ManiType::Struct` now carries the type arguments the literal
was built at**, and a field read resolves the declaration under them — without
which `p.a` stays `Unknown` and `p.a > p.b` is an integer comparison of bit
patterns, right for positive doubles and wrong for negative ones.

**Why the existing `Generic(name, args)` was not reused**: every question the
compiler asks about a struct is asked by matching `Struct`, at 27 sites, and a
second spelling means auditing all 27 and getting one wrong — `from_mani` alone
would have mapped a generic struct to `Ptr(I8)`. Carrying the arguments inside
the existing variant keeps the type nominally a struct everywhere, and made the
change safe: every site was a pattern the compiler forced a visit to. **The
arguments are carried, not compared** — `types_compatible` still looks only at
the name — so no verdict moves.

**It nearly made an open defect look fixed.** `p65_impl_method_still_erased`
tested `(1.5, 2.5)` and nothing else; positive doubles order the same way as
their bit patterns, so the moment the store stopped truncating, the row printed
`2.5` and would have been un-`#[ignore]`d while the method body was still
erased. `(-1.5, -2.5)` still answers `-2.5`. **A single case for a comparison
is half a test, and the missing half is the one that tells a fix from an
accident** — P45's own rule, catching the author who wrote it down.

### Measured (P67 and P68 together)

| | |
|---|---|
| `cargo test --no-fail-fast` | **697 passing, 0 failing, 6 ignored**, 0 warnings |
| 17 examples, both backends, 6 flag combinations | **17/17, parity 17/17** |
| thatteOS, both halves | **61/61**, 10 userspace binaries |
| `--verify-ssa`, 72 files | **0 violations** |
| R5, repos / corpus | **1 of 295** (P67's fixture, rejected → accepted) and **0 of 1,147** |
| behaviour diff, repos | **187 of 187 identical, 0 changed**; 1 file newly compiling |
| behaviour diff, corpus | **583 of 583 identical, 0 changed**; none compiled by only one binary |

Nothing in either repo uses a generic struct with a float field, which is why a
defect this total was invisible: **every generic struct in every corpus holds
integers.**

### Still open

`impl<T>` methods are not instantiated. The design is unblocked: the receiver
now carries its type arguments, and `ast::ImplBlock` reduces `impl<T> Box2<T>`
to a base name plus a positional `generics` list, so the mapping is positional
by construction. Three pieces remain — record the generic impl methods' ASTs;
instantiate through `ensure_mono` with the impl's generics and `Self` bound;
and redirect the call, which is the piece with no precedent, because the
lowerer derives a method's callee name from the RECEIVER's type rather than
from the typed expression.
