# B7 — affine types, properly

© Manish Jagdish Thatte
2 September 2026

A design document, not an implementation. Written in the shape
`CONCURRENCY_DECISION.md` established: **measure what exists first, then take
the decisions, then say what depends on them.** That order is what found P83,
P105 and P107, and B7 is the item most likely to be designed from habit rather
than from this codebase, because every reader arrives with Rust in their head.

## 1. What exists, measured

`src/borrow/mod.rs` is **860 lines and it rejects programs**. This is not a
greenfield feature; it is an existing checker with an undocumented rule set,
and B7 is the job of making that rule set principled rather than of inventing
one.

`consume_if_move` is called at exactly **four** sites, and re-probing them on
2 September 2026 reproduces P51's finding to the letter:

| position | moves? |
|---|---|
| `let b = a;` — a `let` initialiser | **yes** |
| `b = a;` — an assignment value | **yes** |
| `let t = (a, 1);` — a tuple element | **yes** |
| `let s = S { x: a };` — a struct-literal field | **yes** |
| `f(a);` — **a call argument** | **no** |
| `let v = [a, a];` — **an array element** | **no** |

**That is backwards from Rust in both directions**, which is P51's point and
the reason it matters more than tidiness: a reader with Rust habits is
over-cautious about calls and careless about assignment, and the corpus a model
learns from is written by such readers.

Two asymmetries are live and neither has a stated reason:

- **A tuple literal moves its elements and an array literal does not.**
  Recorded as P51's open sub-finding; nothing in the code explains it.
- **A call argument does not move**, so the checker cannot express a function
  that consumes.

## 2. The decisions B7 has to take

These are the questions, with what this codebase pushes toward. They are
**not** taken here — taking them is B7's first step, and taking them by
accident is what §1 is warning against.

**D-1. What is the default: affine or copy?** Rust's answer (move by default,
`Copy` for scalars) is one option. This language has a different starting
point: a `trit` is one machine word and `str` is bytes, so the set of types
where a move is observable is small and already known. **The cheap, honest
answer is to make affinity a property a type OPTS INTO**, because the four
sites above show the checker has been guessing and every guess it made about
scalars was invisible.

**D-2. Does a call argument move?** It must, or `fn consume(x: T)` is
inexpressible and F-4 cannot be built on it. This is a **strictness change** and
therefore needs P70's treatment: measure the blast radius with R5 over the
corpus before writing it, and put it behind a lint whose `allow` restores the
previous compiler exactly.

**D-3. What does the tuple/array asymmetry become?** One of them is wrong.
Deciding which is a one-line change and an R5 sweep; leaving it is another
`documented-but-unenforced` waiting to be found by somebody else.

**D-4. What is the interaction with `spawn`?** §11.2 gives a spawned task a
**copy** of the store. An affine value copied into a task exists twice, which
is exactly what affinity forbids. Either `spawn` consumes what it captures, or
captured affine values are refused, or §11.2 gains an exception. **This is the
decision most likely to be missed**, because §11.2 and `borrow/mod.rs` were
written eighteen days apart and have never been read together.

**D-5. Where does the check live?** `borrow/mod.rs` runs after the analyzer and
before lowering. An affine check needs types, so it stays there — but note that
P65 DISCARDS a failed generic instantiation, so a move error inside one would
vanish. That is the same shape as P71 and wants the same split.

## 3. What is waiting on it

- **F-4, heap management and regions.** The only untouched Phase 4 item.
  `report.txt` records its own recommendation as "regions plus affine types",
  so it cannot start first. It is also what would remove P63's 2,536-word heap
  ceiling properly rather than by moving the memory map, which P77 measured and
  declined.
- **`docs/semantics.md` §11.12's second decision.** Awaiting the same task
  twice is a TRAP today. Under B7, `await` consuming its handle makes the
  second one a **compile error** and the trap unreachable — strictly better,
  and §11.12 states the rule on the VALUE rather than on the handle precisely
  so the two can coexist until then.
- **The `MutexGuard` surface.** `stdlib/sync.mt` declares it and §11.9 makes a
  `Mutex<T>` a one-slot channel carrying the value. A guard that can be copied
  is a guard that can unlock twice.

## 4. How to start, and how not to

**Do not start with the type system.** Start with **D-2 and D-3**, which are
two small strictness changes to a checker that already exists, each measurable
with R5 over 115 repo files and 2,507 corpus files, each behind a lint. That
gives the affine rule an honest floor — *what does this codebase already do,
and what does changing it cost?* — before any syntax is invented.

**Do not add syntax first.** N3 in the phase plan says no macros until the type
system is sound, and the same reasoning applies inside the type system: a
marker like `affine T` written before D-1 is answered is a commitment made by
typing rather than by deciding.

**The instrument to build first is a sweep that reports every move site in the
corpus**, the way P103's field-miss instrument was built before its refusal.
Without it, D-2's blast radius is a guess.

---

Authored by **Manish Jagdish Thatte**

© Manish Jagdish Thatte, 2026
