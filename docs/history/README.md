# `docs/history/` — superseded documents, kept as record

© Manish Jagdish Thatte

Three documents merged here from `oss/unreleased/` on 29 August 2026. Each is
**byte-unchanged from its original** and carries a **dated notice** stating which
of its claims were re-measured and found false. The notice is prepended rather
than folded in, because a design document that has been quietly corrected stops
being a record of what was intended.

| File | What it is | Status |
|---|---|---|
| `DESIGN.md` | The original maniT language and compiler design session — Q&A record, primitive types, syntax, T3ISA specification, implementation plan. | Historical. Four claims measured false 29 Aug 2026: `int` is 27 trits and not unbounded; `await` does not parse; `spawn` runs in place; the `trit` build tool does not exist. |
| `audit-2026-08-02.md` | A full compiler audit — GOD files, five bugs, language gaps, type-system and optimiser issues. | Substantially superseded by the `report.txt` campaign. Re-probe before treating any item as open. |
| `trit-abi-2026-08-17.md` | `[trit]` has two incompatible runtime representations — a length-prefixed i64 form and a raw i8 form — and the same function body is correct or crashing depending on what its caller passed. | **Stale: the defect no longer reproduces.** Both of its cases now agree on both backends. Kept for the mechanism description. |

**The convention these follow** is the one `docs/language-reference.md` §14 and
§22 already use: when a documented claim stops being true, add a dated notice
saying so and leave the original sentence standing. report.txt records three
documentation defects (P51, P55, and §14's refuted address explanation) in which
**no sentence was false** — an absence, a word, and a mechanism that was true
when written. That is the argument for dating claims rather than editing them,
and for pinning the ones that matter with tests.
