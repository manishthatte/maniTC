# D1 and D2 — architecture notes

© Manish Jagdish Thatte
2 September 2026

**Scope, stated first because it is a constraint and not a preface.** This
document is written under §R6 of the phase plan and the public-repo discipline
in `CLAUDE.md`: it describes **architecture and purpose only**. No dimensions,
thresholds, wavelengths, SNR figures, simulation results or unpublished
application material appear here, and none should be added. Seven of the twelve
applications are unpublished and this repository is public. The plan itself
says to start *thinking* about this tier early and to keep the discussion at
the architectural level; that is exactly what this is.

It is also **not a decision document**. D2 in particular depends on almost
everything above it, and the plan is explicit that starting it early would be a
mistake. What follows is the shape of the two problems and how they constrain
each other, so that the decisions, when they are taken, are taken knowingly.

## D1 — absence is not zero

### The problem, in this language specifically

Every binary language conflates three things at some layer: the number zero,
the absence of a value, and falsehood. ManiT has already separated the third:
`bool3` is `True` / `Unknown` / `False`, and §6.4 gives `Unknown` its own
algebra rather than treating it as a degenerate false.

**D1 is the same separation one level down, and balanced ternary makes it
sharper rather than softer.** In an unsigned binary word, 0 sits at the edge of
the range, so borrowing it as a sentinel costs one value at the boundary. In
balanced ternary, **0 sits in the middle**: it is the value a trit takes
between −1 and +1, the identity of `+`, and the answer `tand` gives for the
genuinely undetermined case. Borrowing it as "absent" does not cost a boundary
value — it removes the centre.

The failure mode is therefore not "one lost value" but **an absence that is
indistinguishable from the most ordinary answer a computation can give**. This
codebase has already paid for that shape three times without the type system
being involved: P44 read slot 0 on a field miss, P70 read slot 0 for
`struct Self`, and P103 read slot 0 for a field the struct does not have. Each
time the wrong answer was *plausible*, and each time that is what made it
survive.

### What the type system would have to say

The question D1 has to answer is **where the distinction lives**:

1. **In the value.** A wider representation with a reserved encoding. Cheapest
   to check, most expensive in width, and it reintroduces a sentinel — the
   thing D1 exists to remove — one level down.
2. **In the type.** `T` and "possibly-absent `T`" are different types, and the
   compiler refuses to use one where the other is meant. This is what
   `Result<T, str>` already does for failure (§6.8), and D1 is the same move
   for *absence*, which is not the same thing as failure and should not borrow
   its vocabulary.
3. **In the clock.** A value that is absent *now* and present later is not
   absent at all; it is a value on a different clock. **That is D2**, and it is
   why these two are one document.

Reading 2 is the one consistent with everything above it: §6.8's `Result` is
already a three-variant closed type, `?` already propagates `Unknown`
distinctly, and §10.2 already refuses to let `false as trit` be `false`.

### What it would cost

**A pervasive strictness change**, and this repository has a measured method
for those: P95 and P103 both instrumented the site before writing a refusal and
swept 115 repo files and 2,507 corpus files with a positive control. D1 is
bigger than either, so the instrument comes first or the blast radius is a
guess. Expect the population to be larger than any reading of the code
predicts — that has been true of every finding in this campaign that bothered
to measure (P90 recorded four kinds and measured more, P94 recorded two
positions and measured seven, P95 recorded four and measured thirteen).

## D2 — clock-domain types and synchronous dataflow

### The problem

Give a value a **clock** in its type, and check clock compatibility the way
types are checked: combining two values on unrelated clocks is a type error
unless they are explicitly synchronised. The idea is not new — the synchronous
dataflow languages have it — and the reason it belongs here rather than being
borrowed wholesale is that this language's target is not a single synchronous
domain and its logic is not two-valued.

### Why it is the most ambitious item, and why it is last

The plan says D2 "depends on almost everything above, so starting it early
would be a mistake" and that it "also constrains D1 and C3". Both halves are
right, and the second is the load-bearing one:

- **It constrains D1**, per reading 3 above: absence and not-yet-on-this-clock
  are the same phenomenon seen from two places, and answering D1 without D2 in
  view means answering it twice.
- **It constrains C3** (width-polymorphic `t<N>`): a width and a clock are both
  properties a value carries that the type system must propagate through
  ordinary operations without the programmer restating them. Whichever is built
  first sets the mechanism the other inherits.

### What already waits on it

**Concurrency step 5** — three-valued `held / free / UNKNOWN` locking, the last
item of `CONCURRENCY_DECISION.md` §5. It is not blocked by implementation
effort; it is blocked by a question D2 answers. A lock whose state is genuinely
`UNKNOWN` is only meaningful if there is an account of **when the unknown
resolves**, and "when" is a clock. Without D2, `UNKNOWN` in a lock is a third
enum variant with no semantics — which is precisely what §2 of the decision
document says `AtomicTrit` was, and why it was deprecated.

That is the strongest available argument for D2 being real rather than
decorative: **a feature this project already wants is stalled on it, and stalled
for a reason that is about meaning rather than about work.**

## How these two should be approached

1. **Nothing here is decided.** Both items need a decision document of their
   own, in the shape `CONCURRENCY_DECISION.md` used, and D1's should come
   first because D2 constrains it and not the reverse.
2. **B7 comes before both.** Affinity is the smaller type-system change, it has
   an existing checker to measure against (`enhance/phase5-.../B7_AFFINE_TYPES.md`),
   and N3's reasoning — do not build on an unsound type system — applies to D1
   and D2 more than to anything else in the plan.
3. **The instrument comes before the refusal.** Twice this campaign the useful
   move was to instrument a site, sweep the corpus, and only then write the
   check. For a change of D1's reach that is not a preference, it is the
   difference between a measured strictness move and an unmeasured one.

---

Authored by **Manish Jagdish Thatte**

© Manish Jagdish Thatte, 2026
