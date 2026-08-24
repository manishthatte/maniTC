# The concurrency decision — cooperative, deterministic, both backends

© Manish Jagdish Thatte
24 August 2026

Resolves the four open questions in `docs/memory-model.md` §5. This is a
decision document, not a survey: P5 reached its current state because the
surface was built without one, and leaving the choice open a second time would
be the same mistake wearing a different hat.

**Decision: ManiT gets COOPERATIVE, DETERMINISTIC concurrency, with the same
scheduling semantics on both backends. `AtomicTrit` is deprecated. `recv` on a
channel that no runnable task can fill is a TRAP, not a hang.**

---

## 1. Cooperative, not pre-emptive

### The evidence

| | |
|---|---|
| T3 emulator | has a `Task { pc, regs, stack }` struct and three reserved syscalls (80 `spawn`, 81 `yield`, 82 `task_exit`), all no-op stubs today |
| T3 machine | one register file, one PC. Pre-emption would have to be invented, not exposed |
| LLVM runtime | real pthreads in `sync.c`, **declared but never called** by generated code |
| LLVM runtime | `channel_recv` waits on a `pthread_cond_t` that, with `spawn` inlined, nothing can ever signal |

The T3 side already committed to cooperative and stopped. The LLVM side has
pthreads that nothing reaches. Neither is load-bearing, so neither constrains
the choice — which is a rare and temporary luxury.

### Why cooperative wins

**Determinism is the project's currency.** This campaign uses measured wall time
as a resource model, `noise_floor_diff.py` exists because reproducibility is
treated as a measurable property, and ORACLE_FINDINGS §58 spent 2.5 GPU-hours
establishing the instrument's own spread. Pre-emptive concurrency makes program
output non-reproducible by construction. Adopting it would mean building a
third source of noise into a project that just finished measuring the other two.

**Data races become unreachable rather than undefined.** With interleaving only
at explicit yield points, there is no such thing as a torn read. That is a
strictly stronger memory model than C11 or Java can offer, it is expressible in
a page, and it can be specified in `docs/semantics.md` as reduction rules over
an interleaving of tasks — which a pre-emptive model cannot be, at any length.

**It is the honest fit for the target.** The hardware ManiT targets is not a
multi-socket cache-coherent machine, and adopting a memory model designed for
one would be importing constraints from a machine this language is not for.

### What it costs, stated plainly

The LLVM backend must **emulate** cooperative scheduling rather than use
pthreads: `ucontext`, an explicit state-machine transform, or a stack-switching
trampoline. None of that exists today (`grep ucontext|setjmp runtime/*.c` is
empty). This is the real price of the decision and it is not small — it is more
work than calling `pthread_create`, and it forfeits multi-core parallelism.

That forfeit is the point rather than a regret. ManiT's claim is not "fast by
parallelism", it is that ternary operations are cheap — C2 measured a single
`TANDW` against 3,034 instructions for the same work. Multi-core throughput is
the wrong axis to compete on and the wrong thing to spend the memory model on.

---

## 2. `AtomicTrit` is deprecated

It exists to make an operation indivisible against a pre-emptive scheduler.
Under §1 there is no pre-emption, so between two yield points every sequence is
already indivisible and `AtomicTrit` guarantees nothing a plain `trit` does not.

Keeping it would be worse than removing it: a primitive named "atomic" that
provides no ordering is an invitation to write code that assumes one. It should
be deprecated with a message naming the plain binding as its replacement — the
A1 `deprecated("...")` clause already exists for exactly this.

`Mutex`, `Barrier` and `Semaphore` survive, with different jobs: not mutual
exclusion against pre-emption, but **structured waiting** — a task that cannot
proceed yields until it can. Their pthread implementations are replaced by
scheduler operations.

---

## 3. `recv` on an unfillable channel TRAPS

Today it returns 0 on T3 (silent data loss) and hangs forever on LLVM (P5.1).
Both are wrong and neither is a design.

Under cooperative scheduling the scheduler knows the whole runnable set, so it
can answer a question a pthread runtime cannot: *is any task able to fill this
channel?* If no task is runnable and none is waiting to send, the program cannot
make progress, and that is a **deadlock the scheduler can detect** rather than a
hang the user must diagnose with a debugger.

    TRAP: deadlock — recv on an empty channel with no runnable sender

Exit status 70, like every other trap, with the output trace retained.

This is the strongest single argument for cooperative scheduling and it is worth
stating separately: **detectable deadlock is a property pre-emptive runtimes
cannot offer.** A pthread program that deadlocks just stops.

---

## 4. Three-valued synchronisation — the genuinely novel part

A trit-valued lock has three states, and `held / free / unknown` is a
distinction binary synchronisation cannot express. "Unknown" is the honest state
of a lock whose owner is on another clock domain, or whose acquisition is in
flight — a case binary code must model as either held (over-conservative) or
free (wrong).

**Not now.** It depends on D2 (clock-domain types), it is research rather than
engineering, and the first three decisions do not depend on it. Recorded so that
§1's scheduler design leaves room for a three-state lock rather than baking in
two.

---

## 5. Sequencing

1. **Specify first.** `docs/semantics.md` gains an interleaving section, and the
   reference interpreter gets a scheduler. A3's whole point is that the third
   implementation comes from the written rules, and concurrency is where an
   unwritten rule does the most damage.
2. **T3 second.** The `Task` struct and syscalls 80–82 already exist as stubs;
   the emulator is single-threaded, so the scheduler is a loop over saved
   register files. This is the cheap backend and it validates the design.
3. **LLVM third**, emulating the same semantics. Hardest, and doing it last
   means the semantics are settled before the difficult implementation starts.
4. **Deprecate `AtomicTrit`** and re-point `Mutex`/`Barrier`/`Semaphore` at the
   scheduler.
5. **Then** revisit §4.

Until step 1 lands, `docs/memory-model.md` remains normative: ManiT is
sequential, `spawn` runs its block in place, and P5 stays OPEN.

---

## 6. What would change this decision

Recorded so the reasoning can be attacked rather than merely disagreed with:

- **If ManiT ever targets a real multi-core ternary machine**, forfeiting
  parallelism stops being free and §1 needs re-arguing. Nothing on the roadmap
  implies one.
- **If the LLVM emulation proves unworkable**, the fallback is pre-emptive on
  LLVM and cooperative on T3 — two models, which is what P5 already is and what
  this decision exists to avoid. Prefer dropping concurrency entirely over
  shipping two models.
- **If a use case needs true parallelism**, it belongs outside the language, as
  processes over channels, not inside the memory model.
