# Phase 1 — close the boundary

© Manish Jagdish Thatte

Horizon: **weeks.** Source: `MANITC_FEATURE_RECOMMENDATIONS.txt` §11, Phase 1.

## Items

- **A1 — an `extern` declaration form with mandatory signature and backend
  set.** *(steps 1 and 2)* Replace the current implicit native registration
  with an explicit declaration carrying an `available(...)` backend set.
- **A5 — a `--deny` family and a lint level system, with levels recorded in
  the build output.** Per-lint allow/warn/deny/forbid, settable per
  compilation and per module, and the levels in force recorded in the
  artifact.
- **B1 — trait bounds and where-clauses.** (Also filed as A4, because the
  defect history demands it, not merely the type system.)
- **F-8 — coverage-guided fuzzing of the front end, in CI.**

## Rationale (verbatim, §11)

> Five of the last eight recorded findings are FFI-boundary defects. Nothing
> else in this document pays back as fast.

## Constraints

- **R5.** A1 and A5 both change what `manitc check` accepts, and L1 is defined
  as "generations pass `manitc check`". Land them as deliberate, recorded
  steps — never incidentally, never while an L1 run is live — and record the
  binary's sha256 beside any result it scored. Both *improve* the checker and
  therefore the metric; this is a reason to sequence and announce them, not to
  avoid them.
- **N6.** Do not let the standard library grow before A1 lands.
