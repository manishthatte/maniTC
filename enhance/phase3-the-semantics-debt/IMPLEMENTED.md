# Phase 3 — what landed

© Manish Jagdish Thatte
24 August 2026

Against `enhance/phase3-the-semantics-debt/README.md`: **A3, E2, C4 and N5 are
complete** — the whole of Phase 3 except the A3 increments listed under "Not
done". Working-tree changes, uncommitted, per the repo convention.

## Verification

| | before A3/E2 | after A3/E2 | after C4/N5 |
|---|---|---|---|
| `cargo test` | 473 pass, 0 fail | 508 pass, 0 fail | **535 pass, 0 fail** |
| examples on T3 | 17/17 | 17/17 | **17/17 v1, 17/17 v2** |
| examples on LLVM | 17/17 | 17/17 | **17/17 v1, 17/17 v2** |
| cross-backend output parity | 17/17 | 17/17 | **17/17 v1, 17/17 v2** |
| thatteos `tests/test_all.sh` | 61/61 | 61/61 | **61/61** |
| conformance tests (3 ways) | — | 33 | **41** |
| L1 instrument vs `14178d…` | 0 diffs | 0 diffs (268 files) | *not re-run — see below* |

## A3 — three parts

### 1. `docs/semantics.md` — the normative semantics

A small-step operational semantics for the core: the four scalar types,
arithmetic, comparison, short-circuit, the three-valued and lane-wise families,
casts, `if`/`tif`/`while`/`return`, functions, and output. 329 lines.

Scope is stated and narrow, per §11's own instruction ("do not attempt the whole
language"). Floats, strings, arrays, structs, traits, generics, `match`, `?`,
modules, concurrency and the heap are **out**, and a program using them is
outside the document rather than quietly under-specified.

Written from MEASURED behaviour, not from reading the compiler: every rule was
probed on both backends first. That ordering is why it found defects instead of
merely restating them.

### 2. `src/reference/` — a third implementation

A definitional interpreter with **its own lexer, its own AST and its own
parser** — 4 files, 1,133 lines. It implements `docs/semantics.md` and cites
section numbers on each rule.

The independence is the whole point and is **enforced, not merely stated**:
`conformance_tests.rs::the_reference_implementation_is_independent` fails the
build if anything under `src/reference/` imports from the rest of the crate. A
reference implementation sharing the compiler's front end cannot witness a
front-end bug, and front-end bugs are exactly what the two-backend oracle is
blind to — so without that rule every conformance test would still pass and
prove nothing.

### 3. `tests/conformance_tests.rs` — three-way agreement

26 tests. Every program runs three ways — reference, T3, LLVM — and all three
must produce the same observable behaviour (output trace + trap/no-trap). Cases
are organised by the section of the semantics they pin, plus three generated
suites (450 expressions total, fixed seed).

Programs stay inside the 27-trit range on purpose: §10.1 records that T3 traps
on overflow and LLVM does not, so a wider value would test that known
divergence instead of the semantics.

## What it found

Three defects, in the order they surfaced. **None was findable from the two
backends**, and that is the argument for the whole item.

| | defect | who was wrong |
|---|---|---|
| **P2** | `int as trit` did not clamp | T3 only |
| **P3** | `tposs`/`tnec`/`timp`/`teq` returned -1 as a `bool` | LLVM only |
| **P4** | block scoping lost in the IR lowerer | **both, identically** |

**P2** was found while writing §6.7 — within the hour. The language reference
had always said `as trit` clamps and LLVM had always done it; T3 emitted a bare
`MOV`, so `5 as trit` was 5 on the backend whose premise is that the carrier
set is the hardware's.

**P3** is the worst of the three by consequence. All four operators are typed
`bool` but computed their answer in the three-valued carrier and returned it
unnormalised, so false was -1. T3's `if` dispatches on sign and read that as
false; LLVM's tests nonzero and read it as true. **`tnec x` — "is x definitely
true?" — was true for every input on LLVM.** The modal operators are C1's
bridge out of three-valued logic into `if`, and on one backend they did not work
at all. Fixed with one instruction (`TritMax(v, 0)`), free in `tnec`'s case.

**P4 is the one that justifies A3 rather than merely benefiting from it.**
`IRLowerer::locals` is a flat map with no scope stack, so an inner `let`
overwrote the outer binding permanently:

```text
let x: int = 1;
{ let x: int = 2; io::println_int(x); }   // 2
io::println_int(x);                       // printed 2, want 1
```

The semantic analyser had it right all along — real scope stack, outer type
restored after the block, and a `shadowing` lint whose own text calls the inner
binding "a binding that hides an outer one". The lowering discarded information
the checker had. In a `while` body it was worse than a wrong binding: one probe
printed 6975333711 where 7 was expected.

**Both backends were wrong identically**, so the 17-example matrix, the
cross-backend parity check and every `cross_*` test agreed with each other and
with the bug. This is trap 10 in the plan's list — "the two-backend oracle
cannot see a defect upstream of the split" — and the third recorded instance,
after section 31 and section 51. The reference interpreter found it on its
first complete run.

## One bug of my own, worth recording

The reference parser's `use`-skipping loop was written over `eat`, which only
advances on a match, so it spun forever on the first token that was neither `;`
nor EOF — an infinite loop on every program. It presented as the whole
conformance suite hanging rather than failing, which is a worse symptom than a
wrong answer and took isolating a single trivial program to find. Noted because
"the test suite hangs" is not a diagnosis anyone should accept twice.

## E2 — the memory model

`docs/memory-model.md`, 162 lines. Measured first, then written — the same order
A3 used, and for the same reason.

**The answer is not the one the item anticipated.** E2 was phrased as
"`AtomicTrit` exists; there is no document saying what it guarantees." What it
guarantees is *nothing beyond a plain variable*, because:

> **ManiT has no concurrency today, on either backend.** `spawn { B }` lowers to
> `self.lower_block(B)` — one site — so it runs the block inline and to
> completion.

`manit_spawn`/`manit_join` in `runtime/sync.c` are real pthreads, declared in
the LLVM helpers and **never called** by generated code. T3's syscalls 80–82 are
explicit no-op stubs. The cooperative-scheduler `Task` struct is
`#[allow(dead_code)]`.

So the memory model is *sequential*, and that is the whole of it. Writing
ordering rules for machinery that never runs concurrently would have produced a
specification of nothing — and it would have been believed. §11 warns twice
about "a wrong number wearing a success label"; a guarantee wearing one is
worse.

### What that turned up — P5, four defects

| | |
|---|---|
| **P5.1** | `recv` on an empty channel returns **0 on T3** and **deadlocks on LLVM** |
| **P5.2** | `let t = spawn { 42 };` binds 0 on T3; on LLVM emits `load void`, which clang refuses |
| **P5.3** | `await t` on a spawned block does not type-check |
| **P5.4** | the language reference asserted `spawn` "returns `Task<T>`" — false on both backends |

P5.1 is the sharpest divergence recorded anywhere in this project: the same
program terminates with a wrong answer on one backend and hangs forever on the
other, printing nothing at all because stdout is never flushed before it blocks.

It was invisible until now for an instructive reason. `examples/concurrency.mt`
passes on both backends and is byte-identical between them — because `spawn`
being synchronous means every producer runs to completion before its consumer
starts, so no receive ever finds an empty queue. **That agreement is an artefact
of the bug, not evidence against it.**

P5.4 is corrected in `docs/language-reference.md`, with the old text quoted in
place so anyone who wrote code against it can tell.

### Left OPEN deliberately

P5 is documented, not fixed. Every candidate repair is a design decision:
blocking `recv` on T3 needs a scheduler to block onto; non-blocking `recv` on
LLVM changes channel semantics; a real `Task<T>` requires choosing between
cooperative and pre-emptive scheduling first — and that choice determines
whether `AtomicTrit` has any reason to exist at all. `docs/memory-model.md` §5
lays the four decisions out and notes that the T3 emulator's dead
`Task`/syscall stubs point at cooperative, which would give a much stronger
model than pthreads. Picking one casually is how this surface reached its
current state.

What IS guaranteed is now tested: `tests/phase3_tests.rs` pins that
`spawn { B }` is sequential, runs `B` in place, and shares the enclosing scope —
identically on both backends. Two tests. The defects are deliberately not
encoded as tests: a test asserting a bug's current output has to be edited
before the fix can land, and the next reader cannot tell whether the old
expectation was a promise or a symptom.

## A3, increment 2 — `Result`, `?` and `match`

`docs/semantics.md` 0.2 (§6.8–§6.10), the reference interpreter extended with a
`Result` value form, and 7 more conformance tests.

**The flagship claim is real.** Measured before anything was written: the three
constructors, all six accessors, `unwrap`'s two distinct trap messages, and `?`
propagating `Unknown` distinctly from `Err` with the message intact through two
levels of call — all correct, on both backends, and now agreeing with a third
implementation. Unlike `spawn` (P5), this feature is what the documentation says
it is.

**One defect: P6.** `match` on a `Result` was not checked for exhaustiveness.
`check_exhaustiveness` handled user enums, `trit`, `bool3` and `bool`; `Result`
is a `Generic` and fell past the match. With no `Unknown` arm and an `Unknown`
value, T3 halted at exit status 24 losing the rest of the program and LLVM fell
through and carried on — the exact failure the type exists to prevent, in the
construct most used to consume it.

A measurement lesson came with it: instrumenting the checker over every `.mt`
file found ZERO cases, and that was incomplete — three more lived as ManiT
programs inside Rust string literals in `audit_regression_tests.rs`. **A corpus
scan of `.mt` files cannot see embedded programs.** The test suite caught what
the scan missed.

### Known limitation, recorded rather than hidden

The reference interpreter does not parse **tail expressions** —
`fn f() -> int { if c { 1 } else { 2 } }`, a block whose last expression is its
value, and `if` used as an expression. That is idiomatic ManiT, so conformance
programs are written with explicit `return`. It limits how much real code the
suite can run and it is the next A3 increment.

## P5 — the concurrency decision (item 4)

`CONCURRENCY_DECISION.md` in this directory. A decision, not a survey: P5
reached its current state because the surface was built without one.

**Cooperative, deterministic, same semantics on both backends.** The evidence
was one-sided once collected — T3 already has a `Task` struct and three
reserved syscalls and is single-threaded by construction, while LLVM's pthreads
are declared and never called, so neither side is load-bearing and neither
constrains the choice.

Cooperative wins on the project's own terms: determinism is this campaign's
currency (§58 spent 2.5 GPU-hours measuring the instrument's spread), and
pre-emption would build a third source of noise into a project that just
finished measuring the other two. It also makes data races *unreachable* rather
than undefined, which is a stronger memory model than C11 offers and one that
fits in a page of reduction rules.

The cost is stated plainly: the LLVM backend must **emulate** cooperative
scheduling rather than call `pthread_create`, and it forfeits multi-core
parallelism. That forfeit is the point — ManiT's claim is that ternary
operations are cheap (C2: one `TANDW` against 3,034 instructions), not that it
is fast by parallelism.

Two consequences fall out. **`AtomicTrit` is deprecated**: with no pre-emption
it guarantees nothing a plain `trit` does not, and a primitive named "atomic"
that provides no ordering invites code that assumes one. And **`recv` on an
unfillable channel traps** — the scheduler knows the whole runnable set, so it
can detect deadlock rather than hang, which is a property a pthread runtime
cannot offer.

Nothing is implemented. `docs/memory-model.md` §4 stays normative and P5 stays
OPEN until the sequencing in §5 of the decision is worked through — specify
first, T3 second, LLVM third.

## R2 — the version machinery

C4 and N5 both change the value a program computes, so R2 requires four things
before either lands: a version bump, both behaviours available during the
transition, a migration lint, and the A3 conformance suite already in place.
All four now exist.

**`src/lang.rs` — `LangVersion { V1, V2 }`, V1 the default.** `--lang` on
`compile`, `check`, `run-t3` and `bench`. An unrecognised version is an error
rather than a fallback: a typo that quietly selected v1 would compile the
program under arithmetic its author did not ask for.

V1 stays the default deliberately. R2 says delay is preferable to doing this
casually, and moving the default in the same change that introduces the
behaviour would be doing it casually.

The version rides on **`IRModule.lang`**, set by the lowerer and read by
`codegen_llvm`. `IRLowerer::lower()` is kept as the V1 entry point so its four
callers did not have to change; `lower_with(program, lang)` is the new one.

**Feature predicates, not comparisons.** `lang.division_rounds_to_nearest()`
and `lang.int_is_27_trits()` are methods rather than `== V2` at each use site,
so a third version can turn one off again without every site being wrong.

## C4 — round-to-nearest division

`/` and `%` round to nearest with **ties away from zero** under v2 and are
unchanged under v1.

**`%` moves with `/`, which is not in the recommendation and is the first thing
to notice.** `(a / b) * b + (a % b) == a` holds today and would break if `/`
rounded while `%` truncated. The two modes are therefore pairs —
`(div_nearest, rem_balanced)` and `(div_trunc, rem_trunc)` — and the identity
holds in both. The balanced remainder is defined *from* the quotient rather
than given a rule of its own, in all four implementations, so the identity
cannot be stated twice and disagree with itself.

**Ties away from zero, not half-to-even.** Half-to-even is the statistically
unbiased tie-break and was the alternative considered. It was rejected because
balanced ternary's unbiasedness comes from the REPRESENTATION, not from the
tie-break, and what is worth preserving is the symmetry the representation
already has: `div(-a, b) == -div(a, b)`, which half-to-even does not have.

**One rule, four implementations, deliberately two shapes.**

| where | form |
|---|---|
| `src/lang.rs::div_nearest` | negative magnitudes: `-|r| <= nb - nr` |
| `IRBinOp::DivNear` const-folding | calls `lang::div_nearest` |
| T3 emulator `Opcode::Tdivn` | calls `lang::div_nearest` |
| LLVM emitted sequence | the same negative-magnitude form, inline |
| `src/reference/eval.rs::div_nearest_ref` | the obvious i128 `2|r| >= |b|` |

The reference deliberately uses the *other* formulation. If the two disagree
anywhere, the conformance suite says so — and it can only say so while they are
written differently. `lang.rs` also carries a test comparing them across
`i64::MIN`, `±T27_MAX` and 8,000 small pairs.

Why negative magnitudes: `-|x|` is representable for every `i64` and `|x|` is
not (`i64::MIN`), so the test `2|r| >= |b|` rewritten as `-|r| <= -(|b| - |r|)`
is total with no widening and no special case. That matters because it is the
form both backends emit.

**T3ISA v1.6 — `TDIVN` and `TMODN`, opcodes 43–44.** One instruction each. R3's
rule for an architecture change was followed: the spec is bumped, the change
note says what an older implementation will do with a newer program, and the
existing `TDIV`/`TMOD` are untouched.

**Sixteen instructions on LLVM against one on T3.** Branchless on both — the
LLVM emitter produces one straight-line sequence per IR instruction and cannot
open a basic block in the middle of one. That ratio is C4's own argument, on a
different operation from C2's: dropping low trits already rounds to nearest in
this representation, so the machine gets for free what a binary one must build.

**`IRBinOp::Div` and `Rem` stay truncating.** The compiler's own lowerings
divide by powers of three to reach a lane, and a lane index that rounded would
be the wrong lane.

**`math::div_trunc`, `math::rem_trunc`, `math::div_near`, `math::rem_near`.**
The migration path: all four mean the same thing under both versions, on both
backends. Intercepted in `lower_expr` (C7's route) rather than per-backend —
`math` took the per-backend route elsewhere and a census measured 3 of its 52
functions working on both sides.

**The `division-semantics` lint**, `allow` by default, A1's `undeclared-native`
pattern exactly. `--warn division-semantics` generates the backlog on demand.
It names the enclosing function as well as the span, because the span alone
cannot locate a site inside merged stdlib source (P8).

Measured over the 17 examples and `thatteos.mt`: **39 distinct functions**
contain a version-dependent `/` or `%` — 27 in merged stdlib modules, 12 in
user code.

## N5 — `int` is 27 trits on both backends

`docs/semantics.md` §10.1, closed under v2. `let m: int = 3812798742493; m + 1`
trapped on T3 and answered 3812798742494 on LLVM; under v2 both trap.

**`IRBinOp::AddT27` / `SubT27` / `MulT27`.** `int`, `t27` and `trint` all lower
to `IRType::I64` and only the first two are 27 trits wide, so the distinction
has to survive to the backends. Three extra operations is the cheapest carrier:
an `IRType::T27` would touch every match on `IRType` in both backends, and a
flag on `IRInstr::BinOp` would touch every construction site. On T3 they emit
`TADD`/`TSUB`/`TMUL` unchanged — the machine's word already is 27 trits — so
N5 costs that backend nothing.

**The guard is called on the OPERANDS, before the arithmetic**, like the
existing divisor guard. That is what makes it exact: `manit_check_t27_mul`
computes the product in `__int128`, so a multiplication that overflows the
machine word is caught on its true value. Checked afterwards in `i64` it would
have missed exactly the products that overflow hardest — `4000000000 *
4000000000` wraps to −2446744073709551616, which is out of range but by the
wrong magnitude, and other pairs wrap back *into* range and would pass.

**The cost, stated plainly:** a call — not a compare-and-branch — before every
`int` add, subtract and multiply on LLVM. It is the cost the divisor guard has
always paid on every integer division, and it is paid only by code compiled
`--lang v2`.

**Not covered, stated so it is not mistaken for covered:** `int` literals,
casts, `<<`, and values returned by natives are not range-checked. N5's claim
is about the three arithmetic operators.

**`trint` is the escape hatch** and is not checked. It remains wider than a T3
register, which is P9 and predates all of this.

## What v2 costs, measured

Compiled all 17 examples both ways and compared.

| | v1 | v2 | change |
|---|---|---|---|
| LLVM IR, all 17 examples | 131,763 lines | 140,141 lines | **+6.3 %** |
| T3 binaries, all 17 | 1,635,024 bytes | 1,635,024 bytes | **0.0 %** |

**T3 pays nothing.** Not "little" — nothing. Every binary is the same size to
the byte, because `TDIVN`/`TMODN` replace `TDIV`/`TMOD` one word for one and
the `*T27` operations emit the same `TADD`/`TSUB`/`TMUL` they always did. Two
of the seventeen (`patent_classify`, `stream_demo`) are byte-identical, because
they contain no integer division and no `int` arithmetic at all.

The LLVM growth is 1,920 `manit_check_t27_*` calls and 400 rounding sequences,
against zero of each under v1. Per example it ranges from 0 % to 10.7 %.

That asymmetry is the whole argument, in one table: what the balanced-ternary
machine does for free, the binary one has to build.

**A cross-check worth recording.** `--warn division-semantics` reports 400
sites across the same 17 examples, and the LLVM backend emits 400 rounding
sequences for them. The migration backlog and the code generator agree on
exactly which sites the version change moves — which is the one thing a
migration list must not get wrong, and is why the lint takes its type from the
same place `binop_to_ir` does.

## What landing it found

- **P7 (FIXED).** `Vec::filter` returned every element on LLVM: the C runtime
  declared the predicate `int` while the compiler emits `i1`, whose upper bits
  the psABI leaves undefined. Latent for as long as the generated code happened
  to leave those bits clear; C4's sequence leaves −1 there instead. Found by the
  cross-backend parity matrix — one example of seventeen differed — not by any
  test of the change itself.
- **P8 (OPEN, mitigated).** Merged stdlib spans are attributed to the user's
  file. Pre-existing and affecting every lint; `division-semantics` is simply
  the first diagnostic to fire inside merged stdlib source.
- **P9 (OPEN, by design).** `trint` is wider than a T3 register, and after v2
  closes §10.1 it is the only remaining integer-width divergence.
- **P10 (OPEN, pre-existing).** A compound assignment types its load/op/store
  triple from the ASSIGNED VALUE rather than the target, so `f /= n` with
  `f: float` and `n: int` loads the double as an i64. Confirmed on the archived
  pre-C4 release binary, so C4 neither caused it nor worsens it. C4 does make
  the same mismatch able to select an operator that is not an instruction, so
  the lowerer now derives the operator from the same `ManiType` as `ty` and
  both emitters `debug_assert!` that a version-dependent integer op never
  arrives with a float type. Repairing P10 itself is a semantic-pass
  strictness question, and R5 forbids landing one alongside a feature.

## Not done, and why

- **The L1 instrument has not been re-run.** `--lang v1` is the default and
  every V1 measurement above is unchanged, but the release binary in
  `manit-model/runs/checkers/` is now older than these sources and
  `tools/stdlib_census.py` defaults to it — running the census without
  `--manitc` marks the four new `math::` functions as working on NEITHER
  backend, which is false. Archive, rebuild, re-measure as one step.
- **The default has not moved to v2**, and should not move in this change.
  See R2 above.

- **A3 beyond the core.** The core is a first increment, not the finished item.
  The obvious next constructs are `match`, `Result`/`?` (whose three-state
  behaviour is the language's strongest claim and is specified nowhere), and
  arrays. Each extends the same three files.
