# Phase 3 — the semantics debt

© Manish Jagdish Thatte

Horizon: **months.** Source: `MANITC_FEATURE_RECOMMENDATIONS.txt` §11, Phase 3.

## Items

- **A3 — a normative operational semantics, and a conformance suite both
  backends are checked against.** A written small-step semantics for the core
  language, plus the suite. Prerequisite for the two breaking changes below.
- **E2 — a memory model, written down.** `AtomicTrit` exists; there is no
  document saying what it guarantees.
- **C4 — round-to-nearest division as the default, with truncation explicit.**
  *Versioned.* Breaking.
- **N5 — `int` becomes `t27` everywhere.** *Versioned.* Breaking: today `int`
  is i64 on LLVM and 27 trits on T3.

## Rationale (verbatim, §11)

> The two breaking changes should land together, once, behind one version
> bump, with the conformance suite already in place to catch what they
> disturb.

## Constraints

- **R2.** C4 and N5 — and the implied N1 operator repointing — each change the
  value a program computes. Every one needs a version bump, a migration lint,
  both behaviours available during transition, and A3 in place **first**. This
  project has twice been bitten by "a wrong number wearing a success label";
  delay is preferable to doing these casually.
- **N7.** Do not trust agreement between the two backends as evidence of
  correctness — that is what A3's normative semantics is for.
