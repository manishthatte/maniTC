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

> **The count is dated, 4 September 2026: it is now 1,953 lines.** The 860 was
> measured on 2 September, before D-1, D-2, D-3 and D-4 were built into it, and
> it is left standing because the sentence around it is the point — the file
> was already large and already rejecting programs before B7 began.
> `documented_line_counts_match_the_source_files` cannot hold this figure: it
> reads `**File:**` lines in `docs/`, and this is prose in a plan.

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

> **TAKEN, 3 September 2026 — and reading the two together first found a
> defect that made the question's premise false (report.txt P118).** The
> premise was that a spawned task gets a copy. Measured, it does not: for an
> AGGREGATE, T3 shares the heap cell (a write inside the task escapes to the
> spawner) and LLVM binds the captured ADDRESS as though it were the value (a
> task that merely reads the capture sees garbage). Neither is a copy, and the
> two are not even the same wrong answer.
>
> So D-4 is decided in three parts, and only the first is a rule about
> affinity:
>
> 1. **§11.2 stands. A capture is a copy, and where the implementations cannot
>    make one, the capture is REFUSED** rather than given a third meaning. The
>    refusal is in `borrow/mod.rs` and names §11.2. Population measured first:
>    0 of 34 `spawn` sites in both repositories and the corpus.
> 2. **A move inside the task is the task's own**, because the store is a copy.
>    This is a LOOSENING and it is sound only because of (1) — what remains
>    capturable is scalars, strings and handles, all of which the backends
>    really do copy.
> 3. **An affine value may not be captured**, for the reason the question
>    gives: a capture is a copy and an affine value copied exists twice. Under
>    D-1 affinity is opt-in, so this rule is unreachable until the first affine
>    type exists — stated now, in the shape §1.2 requires of a rule that is
>    ahead of its implementation, and it falls out of (1) rather than being a
>    new mechanism: the same site asks the same question of the capture's type.
>
> **`spawn` does NOT consume what it captures.** That was the other candidate
> and it is wrong for the same reason D-2's blanket version was: it would
> refuse ordinary code — `spawn { print(s) }` followed by `print(s)` — for a
> value that is genuinely copied. Consumption is for affine types, which are
> the ones for which copying is the thing being forbidden.
>
> What is still owed is the copy itself: a deep copy at the spawn site, which
> is F-4's regions (P63's heap is 2,536 words with no free). Until then the
> refusal is what stands between a user and two different wrong answers.

**D-5. Where does the check live?** `borrow/mod.rs` runs after the analyzer and
before lowering. An affine check needs types, so it stays there — but note that
P65 DISCARDS a failed generic instantiation, so a move error inside one would
vanish. That is the same shape as P71 and wants the same split.

> **TAKEN, 4 September 2026 — and the question's premise held, which is the
> first time in this document that it did.** D-4's premise was false when
> measured. This one reproduced exactly as written, and the reproduction is
> `report.txt` P131.
>
> **Measured first.** A generic function is move-checked TWICE: once erased,
> where a `T` is not a move type because nothing says it is, and once per
> instantiation, where it may be. So the three states are distinguishable and
> were probed:
>
> ```text
> fn dup<T>(a: T) { let b = a; let c = a; }
>   dup(1)                  accepted   — int is Copy, and refusing would be wrong
>   dup(P { .. })           REFUSED    — the instantiation is checked
>   ...with `let q = a | 1` added, which does not check at T = P:
>   dup(P { .. })           ACCEPTED   — the instantiation is discarded
> ```
>
> The move error is reported or not according to whether an UNRELATED line in
> the same body type-checks under the same binding. That is not a rule anyone
> would choose in either direction.
>
> **Population, before deciding anything:** discarded instantiations occur in
> **0 of 2,507 model-corpus programs** and **4 of 366 files across maniTC and
> thatteOS** — and all four of the four are fixtures written to exercise this
> fallback (`p71_failed_inst_freefn`, `p71_failed_inst_impl_method`,
> `ord63_address_theory`, `ord63_str_via_bound`). Real code has none.
>
> So D-5 is decided in three parts:
>
> 1. **The check stays in `borrow/mod.rs`**, over the `TypedProgram`, after the
>    analyzer and before lowering. An affine check needs types and that is
>    where the types are. Nothing moves.
> 2. **Its coverage is therefore exactly the bodies the analyzer BUILT**, and
>    that is now a stated property rather than an accident — `docs/
>    language-reference.md` §22, "What the checker can see: generic bodies".
> 3. **A body it could not see is REPORTED, not passed over in silence.**
>    `unchecked-instantiation` (§20, `warn`) fires at the call site, names the
>    instantiation and the reason its body failed. `warn` because the
>    population is zero, so there is no backlog to bury a reader under;
>    `allow` restores the previous compiler exactly.
>
> **This is P71's split with a third question at the same fork.** P71 found one
> verdict answering two: the NAME must wait on the body's verdict, the RETURN
> TYPE must not, because a return type is a function of the declaration. The
> third is coverage, and it is the borrow checker's. **P65's rule stands
> untouched** — a failed instantiation is still not an error, the call still
> keeps the erased path — because denying here would reject programs that
> compile today, and the four fixtures are the proof.
>
> **What is NOT closed, stated as a limit with a row that goes red the day it
> is:** the move error itself is still unreported, and the reason is
> mechanical rather than a preference. `check_fn` returned `Err`, so no typed
> body was ever produced; and `consume_if_move` reads the type on each USE-SITE
> expression, which in the erased body is `Unknown` for every `T`. Substituting
> after the fact means retyping a whole body — which is precisely the work that
> just failed. Closing it wants either partial typing or B3's answer for const
> parameters (make the failure an error), and that is a language decision with
> its own measured step. The row is
> `tests/move_coverage_tests.rs::limit_a_move_inside_a_discarded_instantiation_is_still_unreported`.
>
> **What was already right, and is now pinned rather than assumed:** a
> parameter declared `move` consumes its argument whether or not the callee's
> body checks at that binding. That is D-2's answer being a DECLARATION, and it
> is why D-2's shape was the correct one — an inferred rule would have had this
> hole too.

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
