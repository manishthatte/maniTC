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
  `tif`.

## Constraints

- **N3.** Do not add macros until the type system is sound. Macros over an
  unsound type system compound the unsoundness.
- **N4.** Do not chase Rust parity. Every item here is proposed because
  ternary or the target hardware makes it valuable.
