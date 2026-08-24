# Phase 2 — buy the distinctiveness

© Manish Jagdish Thatte

Horizon: **weeks; highest visible payoff.**
Source: `MANITC_FEATURE_RECOMMENDATIONS.txt` §11, Phase 2.

## Items

- **C1 — `timp`, Lukasiewicz implication** *(closes finding F1)*, and the modal
  family alongside it. The cheapest distinctiveness in the document: without
  an implication connective, the operator set is equally describable as
  Kleene's K3, and the reference's claim that the logic is Lukasiewicz is not
  decidable from the operators.
- **C7 — trit intrinsics.** `trit_count(x, k)` and family.
- **C2 — tritwise (lane-wise) logic, as T3ISA v1.5** *(closes finding F2)*.
  27-way SIMD that is already paid for; the biggest performance win here.
- **A2 — backend availability as a property the type system carries, not a
  comment.** Generalises A1's `available(...)` from externs to ordinary
  functions.

## Rationale (verbatim, §11)

> C1 and C7 are small and make the language visibly ternary. C2 is the item
> that makes it FAST in a way no binary language can copy.

## Constraints

- **R3.** C2 changes the architecture, not just the compiler. T3ISA is
  published as a normative specification with a version tag and an invitation
  to independent implementers; adding instructions is a v1.5, announced the
  way v1.4 announced its own architectural change.
- **R4.** The ~27x instruction-count win claimed for C2 on lane-parallel inner
  loops is argued, not measured. Measure it before it is quoted.
