# Phase 4 — performance

© Manish Jagdish Thatte

Horizon: **months.** Source: `MANITC_FEATURE_RECOMMENDATIONS.txt` §11, Phase 4.

## Items, in order

1. **F-1 — SSA form in the IR.** Prerequisite for everything else in the tier;
   the current IR is not SSA.
2. **F-3 — a real register allocator.** `KNOWN_ISSUES` issue 2 documents two
   register-allocation defects.
3. **F-2 — the missing optimiser passes**, in the document's order: function
   inlining first (with a size heuristic — the biggest single win), ternary
   strength reduction third.
4. **F-4 — heap management / regions.** `KNOWN_ISSUES` issue 6: "No
   free/destroy API — leak by design".

## Constraints

- **N2.** Do not add a garbage collector. Regions plus affine types (B7,
  Phase 5) are the intended fit.
