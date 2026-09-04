# Phase 5 — the type system's second half

© Manish Jagdish Thatte

Horizon: **months.** Source: `MANITC_FEATURE_RECOMMENDATIONS.txt` §11, Phase 5.

## State, 4 September 2026

**Six of the seven items are done: B3, B4, B6, C3, C5, C6.** B7 is PARTIAL —
**all five** of its decisions are now taken (this line read "three" and then
"four" on 4 September 2026; both notices are in its entry), and it is still not
done, because no type can be declared affine. Its entry below says which. Every
completed item carries a dated note on what was built against what was
proposed, because in all six the proposal turned out to be wrong about
something, and the difference is the useful part.

## Items

- **B3 — const generics over trit width.** `struct TVec<const N: int> { ... }`
  **DONE for functions, 4 September 2026** — `docs/language-reference.md` §25,
  `tests/const_generic_tests.rs`, `report.txt` B3.

  Four notes on what was built against what was proposed.

  **It closed C3's recorded limit, and C3's own row said when.** C3 left
  `c3_width_polymorphism_is_not_implemented_and_says_so`, written to go red the
  day B3 landed. It was the single failing row in a 967-row suite when this was
  first compiled — the finish line was pinned a day before the work started,
  which is what a stated limit is for.

  **It is a property of FUNCTIONS, not of structs, and the reason is a
  representation.** A struct's const argument would have to live in
  `ManiType::Struct`'s argument list, which holds `ManiType`s, and a value is
  not one. Adding a variant for it is P68's and P72's hazard — every
  `matches!(t, A | B)` stays valid and stops being true — so a struct DECLARES
  a const parameter and its fields resolve, and `TVec<27>` is refused by name.
  The item's headline example is therefore half-delivered and says so, in the
  diagnostic and in §25.

  **The item's stated prerequisite binds for one named construct and nothing
  else.** B4 (`const fn`) is listed as required for the whole of B3; measured, a
  BOUND const parameter is already a literal by the time anything reads it, so
  no evaluator is involved in widths, lengths, values, inference or
  monomorphisation. Only `t<A+1>` — a const EXPRESSION — needs B4. That is the
  third item in a row whose stated dependency was contradicted by measuring it
  (C6 about B4, C3 about B3 and B4), and the first where it turned out real for
  a *named part* rather than the whole.

  **`fn widen<const A: int, const B: int>(x: t<A>) -> t<B> where B >= A` is not
  writable, and the obstacle is not the `where`.** `B` appears only in the
  return type, and ManiT has no turbofish, so nothing at a call site can name
  it. The `where B >= A` clause was never reached: a predicate over two
  parameters is moot when one of them cannot be supplied. Refused by name, with
  a message that distinguishes this failure from an argument that simply
  carries no width — the remedies differ.
- **B4 — `const fn` and compile-time evaluation.**
  `src/semantic/const_fold.rs` exists and folds literals; this is the real
  thing.
  **DONE, 4 September 2026** — `docs/language-reference.md` §29,
  `tests/const_eval_tests.rs`, `src/semantic/const_eval.rs`, `report.txt` B4.

  Three notes on what was built against what was proposed.

  **Its claimed reach was measured three times before it was built, and was
  too large every time.** This item is listed as a prerequisite for B3, for
  C6's pattern compilation, and for compile-time trit tables. C6 recorded that
  it needed none of it; C3 recorded the same; B3 measured that it needed B4 for
  exactly ONE construct — `t<A + 1>`, an expression *over* a bound parameter,
  because a bound parameter is already a literal by the time anything reads it.
  That is four items in a row with a stated prerequisite contradicted by
  measuring it, and the cause is the same each time: the document reasons from
  what a feature RESEMBLES in another language, and measuring asks what this
  compiler has to compute, and when.

  **The `>` ambiguity is solved by a precedence floor, not by braces.** Rust
  spells the same construct `t<{A + 1}>` because its constant fragment includes
  comparison. Here the fragment inside a width stops below comparison — a width
  is a number and never a bool — so the first `>` can only be the bracket.

  **There are two evaluators, and a row makes them agree** (permanent rule 5).
  `const_fold` folds a checked expression; `const_eval` folds an AST expression
  in type position, where nothing has been type-checked because a type is not
  an expression. They cannot be one function, so the same eight expressions run
  down both paths and the numbers are compared.
- **C3 — width-polymorphic ternary types, `t<N>`.** One family replacing
  trit / tryte / t9 / t27 / t54. **DONE, 3 September 2026** —
  `docs/language-reference.md` §24, `tests/ternary_width_tests.rs`,
  `report.txt` C3.

  Three notes on what was built against what was proposed.

  **It is not a replacement, and a number decided that before anything was
  written.** `trit` occurs 2,051 times across both repositories against 381 for
  the five named widths combined, so replacing the spellings would have been
  2,400 mechanical edits for no gain. `t<N>` is additive and the five names are
  aliases — which is what the item's own "keep the existing names as aliases so
  nothing breaks" asks for, and which its headline sentence contradicts.

  **The item's premise is wrong about one of the five.** "The current five are
  not a design, they are five points sampled from a continuum" — measured,
  `trit` accepts `tand` and the other four are refused, so `trit` carries the
  three-valued logic role and they do not. `t<1>` therefore resolves to `trit`
  exactly, and that is principled rather than a carve-out: a width-1 balanced
  ternary number's three values ARE the three logic values.

  **It did not turn out to need B3 or B4**, which the item lists as
  prerequisites. A literal width is known at parse time. This is the second
  item in a row whose stated dependency did not bind — C6 recorded the same
  about B4 — and it is recorded rather than assumed, because four Phase-4
  premises were contradicted by measuring them. What genuinely waits on B3 is
  width POLYMORPHISM, `fn widen<const A: int>(x: t<A>)`, which is not built and
  is refused by name.
- **B6 — refinement types over trit ranges.**
  `fn scale(x: t27 where -100 <= x <= 100) -> t27`
  **DONE, 4 September 2026** — `docs/language-reference.md` §26,
  `tests/refinement_tests.rs`, `src/semantic/interval.rs`, `report.txt` B6.

  Three notes on what was built against what was proposed.

  **The item's own example syntax is refused by the expression grammar.**
  `-100 <= x <= 100` gives "comparison operators cannot be chained" — a
  deliberate refusal, because C's reading of `a < b < c` is a bug magnet. So
  the refinement parser is its own, and the chained form is legal inside a
  `where` and nowhere else: there the middle term must be the parameter, so
  there is exactly one reading. Measured before anything was written.

  **There are three verdicts, not two.** Provably inside is silent, provably
  outside for every value is an error, and NEITHER is a lint defaulting to
  `allow` — the pattern `literal-out-of-word` and `division-semantics`
  established. `y * 5` where `y` is `0..3` is right for some inputs and wrong
  for others, and a checker that made that an error would refuse working
  programs while one that made it silent would find almost nothing.

  **"The linear-arithmetic fragment" is not what was built.** What is built is
  interval arithmetic over integer literals and refined parameters, which
  covers the item's example and the array-index idiom
  (`fn get<const N: int>(a: [int; N], i: int where 0 <= i < N)` — B6 composed
  with B3, and neither could say it alone). `x < y` between two parameters is
  not expressible, and there is no `where` on a return type: a refinement is a
  precondition, not a postcondition. Both are stated in §26 rather than
  implied.
- **B7 — linear / affine types, properly.** `src/borrow/mod.rs` is 858 lines
  and the negative tests already gesture at this.
  **PARTIAL — 3 of its 5 decisions have landed. NOT done.**
  **Superseded 4 September 2026: all five decisions are now TAKEN, and the
  item is still not done. See the two notices below, in order.**

  `B7_AFFINE_TYPES.md` names five decisions. **D-1** (an aggregate is a
  reference), **D-2** (`move` on a parameter, not a rule about every call) and
  **D-3** (the array-literal asymmetry) were decided by Manish and implemented
  on 2–3 September 2026. **D-4** (the interaction with `spawn`) and **D-5**
  (where the check lives) are open, and F-4, §11.12's await-twice trap and the
  `MutexGuard` surface all still wait on them.

  > **CORRECTED, 4 September 2026 — the sentence above overshot, and it is the
  > same defect from the other side.** Checked against the tree rather than
  > quoted: **D-4 is not open.** `B7_AFFINE_TYPES.md` carries a
  > *TAKEN, 3 September 2026* block on it, deciding it in three parts — and
  > recording that reading `docs/semantics.md` §11.2 and `borrow/mod.rs`
  > together falsified the question's own premise (`report.txt` P118). Parts 1
  > and 2 are IMPLEMENTED, in `src/borrow/mod.rs`'s `TypedExprKind::Spawn` arm:
  > an aggregate capture is refused naming §11.2, and a move inside the task is
  > the task's own. Both are PINNED, by
  > `tests/scheduling_tests.rs::p118_a_spawn_may_not_capture_an_aggregate` and
  > `::p118_a_move_inside_a_task_does_not_consume_the_spawners_binding`. Part 3
  > (an affine value may not be captured) is stated ahead of its implementation
  > in the shape `docs/semantics.md` §1.2 requires, and is unreachable until
  > the first affine type exists.
  >
  > **So four of the five decisions have landed, and D-5 alone is open.** The
  > sentence it corrects is kept rather than replaced, because the useful part
  > is that a STATUS went stale in both directions inside three days: "done"
  > when three had landed, "two open" when four had. Neither was checked
  > against the tree; both were quoted from the previous handoff.
  >
  > **`src/borrow/mod.rs` is 1,953 lines, not 858** — measured the same day.
  > The 858 was true on 2 September, before D-1, D-2, D-3 and D-4 were built
  > into it. `documented_line_counts_match_the_source_files` cannot see this
  > one: it reads `**File:**` lines in the shipped `docs/`, and this is a prose
  > figure in a plan.

  **Recorded here because "B7 is done" was written into two handoffs and was
  not true.** The 2 September session index says plainly "design document
  written, implementation not started"; the three decisions landed after it,
  and a later handoff compressed that into "done", which the next one copied.
  That is `report.txt` P93's defect applied to a STATUS rather than a count —
  correct-ish when written, never reopened — and the remedy is the same: state
  which parts, and let the next reader check them rather than quote them.

  > **D-5 TAKEN AND IMPLEMENTED, 4 September 2026.** The check stays in
  > `src/borrow/mod.rs`; its coverage is exactly the bodies the analyzer built,
  > which `docs/language-reference.md` §22 now states; and a body it could not
  > see is reported rather than passed over — `unchecked-instantiation` (§20,
  > `warn`). P71's split with a third question at the same fork, and **P65's
  > rule is untouched**: a failed instantiation is still not an error.
  > `tests/move_coverage_tests.rs` (11 rows, **7 red on the control**), the
  > reproduction is `report.txt` P131, and the decision with its measurements
  > is `B7_AFFINE_TYPES.md` D-5.
  >
  > **So all five decisions are taken — and B7 is STILL NOT DONE, for a reason
  > that is not a decision.** There is no way to declare a type affine. Checked
  > rather than assumed, on the day of writing: `affine` appears nowhere in
  > `src/` — not as a keyword, not as a marker, not as a type property. D-1 was
  > decided as *an aggregate is a reference*, which is a rule about what the
  > existing checker moves; the design document's D-1 asked the wider question
  > (*is affinity opted into?*) and that half is unbuilt. D-4's own part 3 says
  > so in as many words: the rule that an affine value may not be captured "is
  > unreachable until the first affine type exists".
  >
  > **D-1's MARKER LANDED LATER THE SAME DAY, and B7 is STILL not finished —
  > for a third reason, measured rather than guessed.** `affine struct` is
  > parsed, recorded and enforced (`tests/affine_tests.rs`, 5 rows;
  > `docs/language-reference.md` §22), and it closed **D-4 part 3** the moment
  > it existed: `spawn` now refuses to capture an affine value, which was
  > decided on 3 September and was unimplementable that day.
  >
  > **What the marker changes is narrower than the item wants, and the first
  > version of its test suite claimed otherwise.** A row asserting that
  > `affine` makes a fieldless struct move-checked PASSED ON THE UNMARKED
  > CONTROL: `is_move_type` answers `true` for every `ManiType::Struct`, so
  > every user struct was already affine at the binding level. The marker's
  > whole observable content for a user type is the `spawn` rule. Corrected in
  > the suite, which now asserts the PAIR.
  >
  > **And the motivating case cannot be reached at all — `report.txt` P132.**
  > B7 §3 names the `MutexGuard` surface: "a guard that can be copied is a
  > guard that can unlock twice." `MutexGuard` is now declared `affine` and it
  > changes nothing, because `Mutex::lock()` returns `Unknown` — the guard's
  > type never arrives, and affinity is keyed on a type name. P103's field
  > check does not fire on it either, which is the independent evidence that
  > the missing thing is the TYPE. So what B7 waits on now is not a decision
  > and not a marker: it is typing the concurrency surface.
  >
  > **This is the third status written for B7 in three days, so it is written
  > as a claim a reader can check**, not as a word. Five decisions taken;
  > `MutexGuard`, §11.12's await-twice trap and F-4 wait on the affine MARKER
  > rather than on any remaining decision. That is the next step, and it is the
  > one the document's §4 warns against starting with — "do not add syntax
  > first" — which is now satisfied, because the four measurable decisions
  > underneath it have all been taken and measured.
- **C5 — `t27f` as a first-class type with literal syntax.** Promotes it from
  a stdlib struct to a language type.
  **DONE, 4 September 2026** — `docs/language-reference.md` §27 and §28,
  `tests/t27f_tests.rs`, `report.txt` C5.

  Three notes on what was built against what was proposed.

  **The item's premise was half stale and half worse than it knew.** "Today the
  native format is a library and the foreign format is the keyword" — measured,
  the native format HAD a keyword, `tfloat`, and it was IEEE 754 double,
  lowered to exactly what `float` lowers to, while its name and the stdlib
  module promised no NaN, one zero and a range to 4.3e4703. The keyword was not
  missing; it was lying (P127).

  **Its own caution was right four times over.** "Section 51 found a real wrong
  value in exactly this area … Pin values, not just agreement." The item found
  four — a phantom type (P127), a standard library that answered **40** for
  100 + 25 and disagreed between backends on 100 × 25 (P128), an unchecked
  `as` that handed back allocation addresses (P129), and a type renderer
  leaking Rust's `Option` into a generated assembler label (P130). Three of the
  four are silent wrong answers on BOTH backends, which is exactly why the
  differential oracle had never seen them.

  **The operators are NOT overloaded, and that is measured rather than
  deferred.** The item asks for "arithmetic operators"; this language's
  arithmetic and comparison operators are built in and never dispatch to a user
  `impl` — its own unsatisfied-bound diagnostic says so in as many words — so
  `t27f` gained a spelling, a literal (`3.5t27f`) and a documented conversion
  lattice, and its arithmetic remains `t27f::add`. A row records the refusal
  and goes red the day dispatch lands.
- **C6 — trit-pattern matching with wildcards and captures**, in `match` and
  `tif`. **DONE, 3 September 2026** — `docs/language-reference.md` §13,
  `tests/trit_pattern_tests.rs`, `report.txt` C6.

  Two notes on what was built against what was proposed. The item names
  `tif` beside `match`, and **`tif` is already the single-trit case**: its
  three arms are a trit pattern of width one, and what C6 adds is that a
  `match` written that way now compiles to the same `TBRANCH` — measured at
  exactly `tif`'s instruction count, not merely close to it. No new `tif`
  syntax was invented, because the item does not ask for one and there is
  nothing it could not already say.

  The rationale also says the good compilation is "where B4's const
  evaluation pays". **It did not turn out to need B4.** A trit pattern's
  fixed trits are known at parse time, so the decision structure is
  available without a constant evaluator; what B4 would buy is the NEXT
  step, a multi-level decision tree over patterns that discriminate on
  different positions, which is not built. Recorded rather than assumed —
  four Phase-4 premises were contradicted by measuring them.

## Constraints

- **N3.** Do not add macros until the type system is sound. Macros over an
  unsound type system compound the unsoundness.
- **N4.** Do not chase Rust parity. Every item here is proposed because
  ternary or the target hardware makes it valuable.
