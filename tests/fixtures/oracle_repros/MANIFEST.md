# maniTC repro drop — 26 August 2026

© Manish Jagdish Thatte

Programs behind ORACLE_FINDINGS §61, §62 and §63, handed over on request for
`manitc/tests/generic_impl_tests.rs`. Each `<name>.mt` has a `<name>.expected`
holding the stdout it SHOULD produce.

**Assert the VALUE, not the exit status.** Every FAILS row below except the two
marked FAILS-CHECK type-checks clean and exits 0 from `manitc check`; three of
them also compile and run and print a wrong answer. An exit-status assertion
passes on all of those.

**Controls are not padding.** Each PASSES row differs from a FAILS row by one
thing, and together they are what localises each defect — §62 is the
intersection of generic and boundary precisely because `impl` alone and
generics alone are both green here. Keep them in the suite: they are what will
tell a future reader that a regression is in the intersection and not in either
half.

Measured against the pinned 25 Aug release `96c6f5c71b3ad63d`, and re-checked
against a debug build of the post-Phase-4 tree at 1d1b5e7 — both binaries agree
on every row.

| program | finding | now | `#[ignore]` reason string | what it is |
|---|---|---|---|---|
| `pe61_construct_payload.mt` | §61 | **FAILS** | `§61: payload variant CONSTRUCTED — the defect` | payload variant CONSTRUCTED — the defect |
| `pe61_match_only.mt` | §61 | **PASSES** | — | payload enum declared AND matched, never constructed — control |
| `pe61_nopayload.mt` | §61 | **PASSES** | — | enum with no payload variants, constructed — control |
| `pe61_mixed_plain_variant.mt` | §61 | **PASSES** | — | mixed enum, only the PLAIN variant constructed — control |
| `gs62_impl_single_param.mt` | §62 | **PASSES** (P44 fixed, 26 Aug) | — | impl<T> Pair<T> swap — THE DOCUMENTED FORM; printed 2 2, now prints 2 1 |
| `gs62_impl_two_param.mt` | §62 | **PASSES** (P44 fixed, 26 Aug) | — | impl<A,B> Pair<A,B> swap, mixed int/str |
| `gs62_impl_noswap.mt` | §62 | **PASSES** (P44 fixed, 26 Aug) | — | method returning the UNSWAPPED Pair<A,B> — so it is not about swapping |
| `gs62_vec_of_generic.mt` | §62 | **PASSES** (P44 fixed, 26 Aug) | — | generic struct read back out of a Vec |
| `gs62_generic_freefn.mt` | §62 | **FAILS-CHECK** | `§62: generic struct into a generic free fn — TypeError, t` | generic struct into a generic free fn — TypeError, the honest failure |
| `gs62_fields_only.mt` | §62 | **PASSES** | — | generic struct, field access only — control |
| `gs62_swap_inline.mt` | §62 | **PASSES** | — | the same swap written INLINE at the call site — control |
| `gs62_impl_nongeneric.mt` | §62 | **PASSES** | — | impl on a NON-generic struct — control, so impl is not the culprit |
| `gs62_two_param_fn.mt` | §62 | **PASSES** | — | two type params on a FUNCTION — control, so generics are not the culprit |
| `ord63_str_via_bound.mt` | §63 | **PASSES** (P45 fixed, 26 Aug) | — | str through <T: Ord> — now REFUSED; the row asserts the refusal, not a value |
| `ord63_str_direct.mt` | §63 | **PASSES** | — | direct str comparison — correctly a TypeError. The front end KNOWS. |
| `ord63_float_via_bound.mt` | §63 | **FAILS** | `§63: float through <T: Ord> — the returned VALUE is corru` | float through <T: Ord> — the returned VALUE is corrupted, not just the choice |
| `ord63_float_controls.mt` | §63 | **PASSES** | — | print_float and direct float compare — controls ruling out the printer |
| `ord63_int_trit_via_bound.mt` | §63 | **PASSES** | — | int and trit through the bound — control, the bound works for these |
| `ord63_address_theory.mt` | §63 | **PASSES** (P45 fixed, 26 Aug) | — | REFUTES the reference's address explanation; now refused, so the row asserts the refusal |
| `s64_reverse_kills_t3.mt` | §64 | **PASSES** (P50 fixed, 26 Aug) | — | str::reverse on multi-byte crashed the compiler process; the row asserts it does not |
| `s64_len_equals_bytelen.mt` | §64 | **FAILS** | `§64: str::len counts BYTES; it and byte_len are synonyms` | str::len counts BYTES; it and byte_len are synonyms |
| `s64_char_as_int_sign.mt` | §64 | **FAILS** | `§64: `char as int` is UNSIGNED on T3 and SIGNED on LLVM f` | `char as int` is UNSIGNED on T3 and SIGNED on LLVM for bytes >= 128 |
| `s64_ascii_control.mt` | §64 | **PASSES** | — | the same three calls on ASCII — control, so this is not `str` being broken |
| `s64_print_multibyte_control.mt` | §64 | **PASSES** | — | printing a multi-byte literal untouched — control, so I/O is not the problem |

## Observed behaviour of the failing rows

### `pe61_construct_payload.mt` — §61

* expected stdout: `2`
* T3 actual: `CodegenError: Undefined label: Shape::Circle (exit 1)`
* LLVM actual: `use of undefined value '@Shape_Circle'; LLVM still EXITS 0`

### `gs62_impl_single_param.mt` — §62

* expected stdout: `2 1`
* T3 actual: `2 2`
* LLVM actual: `2 2`

### `gs62_impl_two_param.mt` — §62

* expected stdout: `x
1`
* T3 actual: `x\n1081`
* LLVM actual: `x\n94693544710838`

### `gs62_impl_noswap.mt` — §62

* expected stdout: `1x`
* T3 actual: `raw memory bytes`
* LLVM actual: `empty output`

### `gs62_vec_of_generic.mt` — §62

* expected stdout: `1 a
2 b`
* T3 actual: `raw memory bytes`
* LLVM actual: `empty output`

### `gs62_generic_freefn.mt` — §62

* expected stdout: `2 1`
* T3 actual: `check: expected `Pair<<unknown>>`, found `Pair``
* LLVM actual: `same`

### `ord63_str_via_bound.mt` — §63 — **RESOLVED 26 August 2026 (P45)**

* was expected stdout: `mm
mm
zz`
* was T3 actual: `aa\naa\nab`
* was LLVM actual: `aa\naa\nab`
* **now**: `check` REFUSES it — `` `str` does not satisfy the bound `T: Ord` ``.
  The expectation itself was wrong: `str` has no ordering in maniT (`"mm" > "aa"`
  is a TypeError), so printing `mm` would have required giving `str` an ordering.
  A fixture's expectation is a report of what someone wanted to see, not a
  specification.

### `ord63_float_via_bound.mt` — §63

* expected stdout: `2.5
-1.5`
* T3 actual: `0.000...001 (denormal) then NaN`
* LLVM actual: `same`

### `ord63_address_theory.mt` — §63 — **RESOLVED 26 August 2026 (P45)**

* was expected stdout: `zzz
zzz`
* was T3 actual: `aaa\naaa`
* was LLVM actual: `aaa\naaa`
* **now**: refused, same diagnostic. The `aaa\naaa` above IS the refutation of
  §14's address explanation — declaration order does not flip it — and it can no
  longer be re-derived by running the program, so it now lives in the dated
  notice in `docs/language-reference.md` §14.

### `s64_reverse_kills_t3.mt` — §64

* expected stdout: `béa
done`
* T3 actual: `EMPTY — not even `done`. stdout ends at the call.`
* LLVM actual: `b<?><?>a then `done` — mojibake, but execution continues`

### `s64_len_equals_bytelen.mt` — §64

* expected stdout: `3
4`
* T3 actual: `4 then 4 — len is byte_len`
* LLVM actual: `4 then 4 — identical`

### `s64_char_as_int_sign.mt` — §64

* expected stdout: `195
65`
* T3 actual: `195 then 65`
* LLVM actual: `-61 then 65 — 195 read as signed 8-bit`

## Counts

* 19 programs, **3 still failing**, 16 passing. Nine were failing when this
  manifest was written on 26 August; P44 resolved four (`gs62_impl_*`,
  `gs62_vec_of_generic`) and P45 two (`ord63_str_via_bound`,
  `ord63_address_theory`) the same day. What remains is
  `pe61_construct_payload` (P43), `gs62_generic_freefn` (P44's third,
  honest defect: a struct literal's bare type never unifies with `Pair<T>`)
  and `ord63_float_via_bound` (the type-erasure defect, report.txt P65).
* §61 payload-enum constructor · §62 generic struct across a boundary · §63 unbound `T: Ord`

**THIS TABLE IS A SNAPSHOT AND THE TEST FILE IS THE AUTHORITY.** A count
written here has to be re-checked by hand every time a row moves, and it has
already gone stale once within a day. `grep '#\[ignore' ../../generic_impl_tests.rs`
is the live list; each entry carries its finding id.

Note `ord63_str_direct.mt` is listed PASSES but its *expected* result is a
`check` FAILURE — it exists to show the front end already rejects the
comparison that §63 lets through a generic. Wrap it as an assertion that
`check` exits non-zero, not as a run.

---

## §64 addendum — 26 August, second drop

Five programs added for §64 (`s64_*`), three failing and two controls.

**`s64_reverse_kills_t3.mt` is the one to look at first.** T3 does not mangle
the string — it stops producing output entirely, and the `io::println("done")`
on the next line never appears. A trap would at least announce itself. Whatever
wrapper you put this in, assert on the presence of `done`, not on the reversed
string: the string is the symptom and the missing `done` is the finding.

The two controls exist to close off the two readings that look obvious and are
wrong. `s64_ascii_control` runs the same three calls on ASCII and passes, so
`str` is not broken generally. `s64_print_multibyte_control` prints the same
multi-byte literal untouched and passes, so the I/O path handles multi-byte
fine. What remains is the byte-level manipulation of a UTF-8 sequence.

`s64_char_as_int_sign.mt` is a cross-backend divergence rather than a wrong
answer as such — 195 against -61 for the same byte. Its `.expected` names the
unsigned reading because that is T3's, but the finding is that the two disagree
at all; either convention consistently applied would be defensible.

`§65` and `§66` are deliberately NOT here — both are fixed in the maniTC tree
with regression tests as of 26 August (P46, P49).


---

## P65 addendum — 26 August, evening

Three programs added for report.txt P65, the type-erasure defect that P45's fix
separated out. These did not come from a probe session; they were written here.

| file | finding | status | ignore reason | what it is |
|---|---|---|---|---|
| `p65_generic_return_field.mt` | P65 | **PASSES** | — | a generic call's RETURN type was `Unknown`, so both field reads resolved to slot 0 and it printed `1 1` |
| `p65_two_instantiations.mt` | P65 | **PASSES** | — | ONE generic, FOUR calls, TWO types, interleaved — the shape a single erased body cannot serve |
| `p65_reference_example.mt` | P65 | **PASSES** | — | `docs/language-reference.md` §14's own `max` example, pinned — unbounded `<T>` called at `int` and at `float` in one program |
| `p65_impl_method_still_erased.mt` | P65 (open half) | **FAILS** | `P65 (open half): an impl<T> method is still type-erased — the NEGATIVE pair is what shows it` | the same defect through a METHOD. **Strengthened on 27 August**: it tested only `(1.5, 2.5)`, and P68 made that pair pass while the defect stood, because positive doubles order the same way as their bit patterns. `(-1.5, -2.5)` is the half that still fails. |

`p65_generic_return_field` has TWO fields on purpose: with one, slot 0 is the
right answer by luck, and the row would pass while the defect stood.

`p65_two_instantiations` includes `largest(-1.5, -2.5)` on purpose: IEEE-754
negatives order the OPPOSITE way to their bit patterns read as integers, so
that call separates a real float comparison from a bit-pattern one. The
positive pair alone does not.


---

## P67 / P68 addendum — 27 August

| file | finding | status | what it is |
|---|---|---|---|
| `p68_generic_struct_float_field.mt` | P68 | **PASSES** | a generic struct's field holds the value it was given; both orderings, plus an `int` control |
| `p68_reference_generic_struct.mt` | P68 | **PASSES** | `docs/language-reference.md` §14's generic-struct example, which declared private fields and never read one |

`gs62_generic_freefn.mt` moved to **PASSES** on 27 August and is kept spelled
`Pair` on purpose: **the name IS the test** (report.txt P67). Renaming it would
delete the defect it exists to catch.

Note also that P44's four `gs62_impl_*` rows now pass for a different reason
than the one recorded when they were un-`#[ignore]`d — P67 removed the cause,
and P44's own fix is defence in depth. The rows are unchanged; only the
explanation moved.

---

## P69 addendum — 27 August

| file | finding | status | what it is |
|---|---|---|---|
| `p65_impl_method_still_erased.mt` | P65/P69 | **PASSES** | the open half of P65, closed. Kept under its original name so the finding it names stays findable; both pairs, and the NEGATIVE one is the one that shows the fix |
| `p69_impl_method_two_instantiations.mt` | P69 | **PASSES** | two instantiations of ONE method in ONE program, `float` and `int`, both orderings of each. The `int` rows are the control — `int` is unaffected by erasure *because its representation IS the erasure* |
| `p69_impl_method_through_generic_fn.mt` | P69 | **PASSES** | the receiver reached through TWO generic free functions. Monomorphisation used to stop at the first boundary, because `bind_generics` matched only `ManiType::Generic` and a user struct's arguments live in `ManiType::Struct` |
| `p69_impl_method_two_type_params.mt` | P69 | **PASSES** | `impl<A, B>` with the float in each slot in turn, so a reversed or first-argument-for-everything mapping is caught |
| `p69_reference_impl_method.mt` | P69 | **PASSES** | `docs/language-reference.md` §14's generic-method example, which now states an output |

**`p69_impl_method_two_type_params` was HOLLOW as first written and the pinned
control binary is what said so.** It was `fn geta(self) -> A { self.a }` — a
field read and nothing else — and it passed on the PRE-CHANGE compiler
unchanged, asserting nothing about the change it was written for. A test for a
type-erasure defect has to make the program DO something the erased type gets
wrong, and for a type parameter that means a COMPARISON. **Run every new row
against the previous pinned binary before believing it**: that is P40's
"reintroduce the defect" for the price of one command.

---

## P71 addendum — 29 August

| file | finding | status | what it is |
|---|---|---|---|
| `p71_failed_inst_freefn.mt` | P71 | **PASSES** | a generic free function whose instantiation is DISCARDED still has a declared return type. `1 1` before, `1 2` now |
| `p71_failed_inst_impl_method.mt` | P71 | **PASSES** | the same through an `impl<T>` method. P69's §6 recorded only this half; the free-function site carried it too |

**Both arms of `pick`/`first` return the SAME operand on purpose.** The
comparison that makes the instantiation fail is `>` on a struct, which is A4's
open address comparison and picks a different struct on each backend — T3 and
LLVM print `4` and `2` for the same program if the arms differ. Returning
`self.a` from both makes the VALUE determinate while leaving the failure that
the fixture exists to trigger in place, so the row tests the field slot and not
A4. The first draft of this fixture did not do that and was cross-backend
divergent for a reason that had nothing to do with P71.

---

## P72 addendum — 29 August, closing P48

| file | finding | status | what it is |
|---|---|---|---|
| `p48_char_is_an_unsigned_byte.mt` | P48/P72 | **PASSES** | one line per divergence family. **All five lines answered differently on the two backends before the fix and all five agree after it** — that is how the count was established rather than asserted |
| `s64_char_as_int_sign.mt` | P48/P72 | **PASSES** (was `#[ignore]`d) | the one divergence P48 recorded |
| `s64_len_equals_bytelen.mt` | P48/P72 | **PASSES** (was `#[ignore]`d) | **REWRITTEN.** It expected `len` to be 3 — codepoints — which contradicts `s64_char_as_int_sign` expecting `char_at("é", 0)` to be 195: a codepoint `len` sharing its index with a byte `char_at` cannot be looped over. **Two ignored rows, each holding half of a design nobody had settled, and neither looking wrong alone.** Rewritten under P45's rule that a fixture's expected output is a report of what someone wanted, not a specification; it now asserts 4/4/3 and gets the codepoint count from the new `str::char_count` |

**The `p48_*` fixture asserts the VALUE and AGREEMENT together.** Agreement
alone is satisfiable by making both backends wrong at once (P44); the value
alone was green on T3 for three of the five lines throughout, because T3 was
the backend that already had the signedness right.
