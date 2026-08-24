# enhance/ — the maniT / maniTC enhancement phases

© Manish Jagdish Thatte

Working area for the enhancement programme laid out in
`../../MANITC_FEATURE_RECOMMENDATIONS.txt` (24 August 2026).
One subdirectory per phase, following section 11 (SEQUENCING) of that
document; the phase ordering there is normative for this tree.

| Phase | Directory | Horizon | Contents |
|-------|-----------|---------|----------|
| 1 | `phase1-close-the-boundary/`      | weeks  | A1, A5, B1, F-8 |
| 2 | `phase2-buy-the-distinctiveness/` | weeks  | C1, C7, C2, A2 |
| 3 | `phase3-the-semantics-debt/`      | months | A3, E2, C4, N5 |
| 4 | `phase4-performance/`             | months | F-1, F-3, F-2, F-4 |
| 5 | `phase5-type-system-second-half/` | months | B3, B4, C3, B6, B7, C5, C6 |
| 6 | `phase6-the-hardware-language/`   | research | D1, D2, D5, D3 |

Each phase directory carries a `README.md` naming its items verbatim from
the recommendations document. Item identifiers (A/B/C/D/E/F-/N) are the
document's own and are the stable way to refer to a recommendation.

Standing constraints on everything in this tree, from section 12 of the
document:

- **R2 — breaking changes.** C4 (division rounding), N5 (`int` width) and
  the implied N1 (operator repointing) each change the value a program
  computes. Each needs a version bump, a migration lint, both behaviours
  available during transition, and the A3 conformance suite in place first.
- **R3 — C2 is an architecture change.** Lane-wise logic makes T3ISA v1.5,
  announced the way v1.4 announced its own architectural change.
- **R5 — the L1 metric.** L1 is defined as "generations pass `manitc check`",
  so any change to what the checker accepts invalidates every earlier L1
  number. Land checker-strictness changes in deliberate, recorded steps,
  never incidentally alongside a feature and never while an L1 run is live;
  record the binary's sha256 beside any result it scored.
- **R6 — disclosure.** Five of the twelve patent applications are published
  and seven are not. Phase 6 touches the device: keep its discussion at the
  architectural level in anything that leaves this machine, and route any
  concrete hardware-directed feature through the disclosure rule before it
  appears in a public commit message, design document, or submission.

Note that `../../MANITC_FEATURE_RECOMMENDATIONS.txt` is a design document,
not a task list, and section 12 R1 declines to treat it as a commitment:
phases 1 and 2 are the two it defends as clearly worth doing.
