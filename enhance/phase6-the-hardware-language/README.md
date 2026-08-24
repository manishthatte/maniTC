# Phase 6 — the hardware language

© Manish Jagdish Thatte

Horizon: **research; start the thinking now.**
Source: `MANITC_FEATURE_RECOMMENDATIONS.txt` §11, Phase 6.

## Items

- **D1 — absence is not zero, and the type system should know.** Distinguish,
  in types, between the VALUE zero and the ABSENCE of a value.
- **D2 — clock-domain types and synchronous dataflow.** Give values a clock in
  their type and check clock compatibility.
- **D5 — an energy / timing cost model in the compiler.** Per-operation cost
  annotated on the IR.
- **D3 — first-class support for multi-state devices beyond three.**

## Rationale (verbatim, §11)

> D2 is the most ambitious idea here and the one most likely to be publishable
> as research in its own right. It also depends on almost everything above, so
> starting it early would be a mistake — but starting to THINK about it early
> is not, because it constrains D1 and C3.

## Constraints — read before writing anything down

- **R6 — DISCLOSURE.** Five of the twelve patent applications are published
  and seven are not. This tier touches the device. Keep the discussion at the
  **architectural level** in anything that leaves this machine, and route any
  concrete hardware-directed feature through the disclosure rule (Permanent
  Rule 8) before it appears in a public commit message, a design document, or
  a conference submission. Describing that maniT targets a three-input,
  multi-clock ternary device is fine. **Naming the specifics is not** — no
  dimensions, thresholds, wavelengths, phase designations, code-point
  assignments or simulation figures. The architecture may be described; the
  specifics may not.
