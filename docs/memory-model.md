# ManiT — Memory and Concurrency Model

© Manish Jagdish Thatte

**Status: version 0.1, 24 August 2026. Normative.**

This is E2 of Phase 3: "`AtomicTrit` exists; there is no document saying what
it guarantees." This document says what it guarantees, and the answer is not the
one the item anticipated.

## 0. The headline

**ManiT has no concurrency today, on either backend.** `spawn { … }` lowers to
the block, run inline and to completion, at the point it appears
(`ir/lower/lower_expr.rs`, `TypedExprKind::Spawn` → `self.lower_block(block)`).
There is one lowering site and it constructs no task, no thread and no
continuation.

Therefore:

> **The memory model is sequential. Every operation happens in program order,
> there is exactly one thread of execution, and no reordering is observable
> because there is no second observer.**

That is a complete and honest model, and it is the only one this implementation
can currently support. Everything below is either evidence for it, or a record
of what the language *advertises* and does not deliver.

This is not a small distinction. A memory model invented for machinery that
does not run concurrently would be a specification of nothing, and it would be
believed. §11 of the recommendations warns twice about "a wrong number wearing a
success label"; a guarantee wearing one is worse.

## 1. What each primitive actually is

> **Correction, 2 September 2026.** The table below is the measurement of
> **24 August 2026** and is kept as the record; it is no longer the state of
> the compiler. Steps 1–4 of `CONCURRENCY_DECISION.md` §5 have landed, and
> under **`--sched cooperative`** the rows now read:
>
> | primitive | LLVM backend | T3 backend |
> |---|---|---|
> | `spawn { … }` | body OUTLINED, a real task (P99b) | a FORK on syscall 80 (P89) |
> | `yield`, `task_exit` | `__task_yield` / `__task_main_done` | syscalls 81/82, real |
> | `Mutex<T>` | a one-slot channel carrying the value; `lock` blocks | the same, in the emulator |
> | `Barrier`, `Semaphore` | scheduler wait queues; `wait`/`acquire` block | the same |
> | `channel<T>` | `recv` blocks onto the run queue | the same |
> | `AtomicTrit` | **DEPRECATED** — §2 below, and now it warns | as LLVM |
>
> The **default is still `--sched inline`**, where every row of the original
> table below still holds and `spawn { B }` still evaluates `B` in place. §4 of
> this document remains the account of that mode. `docs/semantics.md` §11 is
> normative for the scheduled one, and its §11.9 specifies the three waiting
> primitives as derived forms.
>
> One row of the original was measured wrong in a way worth keeping: *"`Barrier`,
> `Semaphore` | pthread primitives"* was true of `runtime/sync.c` and false of
> what LLVM actually ran, because the backend EMITTED its own counting barrier
> into every module and that definition shadowed the runtime's. The C code the
> row describes was unreachable.

Measured, 24 August 2026, on both backends.

| primitive | LLVM backend | T3 backend |
|---|---|---|
| `spawn { … }` | block inlined, synchronous | block inlined, synchronous |
| `manit_spawn` / `manit_join` | real `pthread_create`/`pthread_join` in `runtime/sync.c`, **declared but never called** | syscall 80, an explicit no-op stub returning 0 |
| `yield`, `task_exit` | — | syscalls 81/82, no-op stubs |
| `Mutex<T>` | `pthread_mutex_t` around a value | a plain heap object, no lock |
| `AtomicTrit` | `volatile int8_t` guarded by a `pthread_mutex_t` | a plain heap object, no lock |
| `Barrier`, `Semaphore` | pthread primitives | plain heap counters |
| `channel<T>` | mutex + condition variable, **`recv` blocks** | a heap queue, `recv` returns 0 when empty |

The `Task` struct for a cooperative scheduler exists in
`src/codegen_t3/emulator/profiler.rs` and is marked `#[allow(dead_code)]`.

## 2. What `AtomicTrit` guarantees

**Today: nothing beyond what a plain variable guarantees**, because there is no
second thread to be atomic with respect to.

On the LLVM backend each `get`/`set`/`swap` takes and releases a mutex, so *if*
threads existed, each individual operation would be indivisible and would
publish its write before releasing. That is a per-operation guarantee only:
there is no ordering promised between two different `AtomicTrit`s, no fence, no
acquire/release distinction, and no `compare_and_swap`. On the T3 backend there
is no lock at all, which is correct for a single-threaded machine and would be
wrong the moment one existed.

So the honest statement is: **`AtomicTrit` is an interface without a
requirement.** It cannot be relied on for anything today, and it does not yet
promise enough to be relied on later. Fixing that is design work, not
documentation work, and §5 says what it depends on.

## 3. Measured divergences in the concurrency surface

Each of these is a defect, recorded in report.txt Section 10 as P5.

### 3.1 `recv` on an empty channel: 0 on T3, deadlock on LLVM

```
let ch = channel<int>();
let v = ch.recv();        // nothing was ever sent
```

- **T3** returns `0` and continues.
- **LLVM** blocks forever (killed at 10 s), and prints nothing at all — not even
  output produced before the `recv`, because stdout was never flushed.

The same program terminates with a wrong answer on one backend and hangs on the
other. This is the sharpest divergence found anywhere in the project so far.

### 3.2 `spawn` does not return what the reference says

`docs/language-reference.md` §"Spawn" says: *"Creates a cooperative task.
Returns `Task<T>` where `T` is the block's type."* None of that is true.

```
let t = spawn { 42 };
```

- **T3** binds `0`. The checker accepts it.
- **LLVM** emits `%t2 = load void, ptr %t0` and clang refuses the module —
  the program does not build at all.

### 3.3 `await` does not type-check

`let v = await t;` on a spawned block is a type error, so the documented pair
`spawn`/`await` cannot be used together as documented.

### 3.4 The working case works by being sequential

`spawn { … }` used for effect, with no value and no await, behaves identically
on both backends — because it is a block. `examples/concurrency.mt` passes on
both backends and is byte-identical between them for exactly this reason: every
"producer" runs to completion before the "consumer" starts. Its channel
receives never find an empty queue, which is why 3.1 was not already known.

## 4. What is normative here

> **Scoped, 2 September 2026.** This section is normative for
> **`--sched inline`**, which is still the default and still what every program
> compiled without the flag runs. Under `--sched cooperative` the normative
> document is `docs/semantics.md` §11, where point 2 below is exactly the rule
> that changes: (SPAWN) appends a task and the spawner continues. Point 1 stays
> true even there in the sense that matters — one task runs at a time — and
> point 3 becomes stronger rather than weaker, because §11.2 gives a spawned
> task a copy of the store, so there is nothing to order.

1. Execution is single-threaded and sequential. Program order is the only
   order.
2. `spawn { B }` is defined as: evaluate `B` to completion, in place, and
   produce no value. An implementation that runs `B` concurrently, defers it,
   or produces a task handle does not conform to *this* document — which is a
   statement about today, and the version of this document that admits
   concurrency will say otherwise.
3. No primitive in §1 provides any inter-thread ordering guarantee, because
   there are no threads. Programs must not be written as though one does.
4. The behaviours in §3 are defects, not semantics. Do not depend on them, in
   either direction.

## 5. What a real model will have to decide

> **Decided, 24 August 2026.** The four questions below are answered in
> `enhance/phase3-the-semantics-debt/CONCURRENCY_DECISION.md`: **cooperative,
> deterministic, same semantics on both backends; `AtomicTrit` deprecated;
> `recv` on an unfillable channel traps rather than hangs.** The reasoning and
> the costs are there. This section is kept as the statement of the problem the
> decision answers; it is not itself out of date, because nothing has been
> implemented yet and §4 remains normative until it is.

Recorded now because the decisions constrain each other, and because the
cooperative-scheduler stub in the T3 emulator shows the intended direction.

- **Cooperative or pre-emptive.** T3 is an emulator with a `Task` struct and
  three reserved syscalls, which points at cooperative scheduling with explicit
  yield points. That would give a far stronger model than pthreads —
  interleaving only at yields, so data races become unreachable rather than
  merely undefined — and it is the model a deterministic instrument wants. The
  LLVM backend would then have to *emulate* cooperative scheduling rather than
  using pthreads directly, which is a real cost and the central trade.
- **What `AtomicTrit` is for.** Under cooperative scheduling with no
  pre-emption, it is unnecessary. Its existence only makes sense under
  pre-emption, so item 1 decides whether it survives.
- **Whether `recv` blocks.** Blocking requires a scheduler to block *onto*.
  Until there is one, 3.1's LLVM behaviour is a deadlock rather than a feature,
  and the T3 behaviour is silent data loss. Neither is a design.
- **Three-valued synchronisation.** The genuinely novel question, and the one
  worth the effort: a trit-valued lock has three states, and "held / free /
  *unknown*" is a distinction binary synchronisation cannot express. Nothing in
  the current implementation explores it. It belongs with D2 (clock-domain
  types) rather than with a retrofit of pthreads.

## 6. Relationship to `docs/semantics.md`

The core semantics (A3) explicitly excludes concurrency from its scope. That
exclusion is now justified rather than provisional: there is no concurrency to
specify, and when there is, it will need its own reduction rules for
interleaving, which the single-configuration model of §4 there cannot express.

## 7. Changes

- **0.1** (24 Aug 2026) — first version. Written as E2, Phase 3. Established
  by measurement that the language has no concurrency, and found the four
  defects in §3.
