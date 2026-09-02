# Phase 5 — the type system's second half

© Manish Jagdish Thatte

Horizon: **months.** Source: `MANITC_FEATURE_RECOMMENDATIONS.txt` §11, Phase 5.

## Items

- **B3 — const generics over trit width.** `struct TVec<const N: int> { ... }`
- **B4 — `const fn` and compile-time evaluation.**
  `src/semantic/const_fold.rs` exists and folds literals; this is the real
  thing.
- **C3 — width-polymorphic ternary types, `t<N>`.** One family replacing
  trit / tryte / t9 / t27 / t54.
- **B6 — refinement types over trit ranges.**
  `fn scale(x: t27 where -100 <= x <= 100) -> t27`
- **B7 — linear / affine types, properly.** `src/borrow/mod.rs` is 858 lines
  and the negative tests already gesture at this.
- **C5 — `t27f` as a first-class type with literal syntax.** Promotes it from
  a stdlib struct to a language type.
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
