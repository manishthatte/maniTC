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
| `gs62_impl_single_param.mt` | §62 | **FAILS** | `§62: impl<T> Pair<T> swap — THE DOCUMENTED FORM. Both bac` | impl<T> Pair<T> swap — THE DOCUMENTED FORM. Both backends print 2 2. |
| `gs62_impl_two_param.mt` | §62 | **FAILS** | `§62: impl<A,B> Pair<A,B> swap, mixed int/str — returns an` | impl<A,B> Pair<A,B> swap, mixed int/str — returns an address |
| `gs62_impl_noswap.mt` | §62 | **FAILS** | `§62: method returning the UNSWAPPED Pair<A,B> — so it is ` | method returning the UNSWAPPED Pair<A,B> — so it is not about swapping |
| `gs62_vec_of_generic.mt` | §62 | **FAILS** | `§62: generic struct read back out of a Vec` | generic struct read back out of a Vec |
| `gs62_generic_freefn.mt` | §62 | **FAILS-CHECK** | `§62: generic struct into a generic free fn — TypeError, t` | generic struct into a generic free fn — TypeError, the honest failure |
| `gs62_fields_only.mt` | §62 | **PASSES** | — | generic struct, field access only — control |
| `gs62_swap_inline.mt` | §62 | **PASSES** | — | the same swap written INLINE at the call site — control |
| `gs62_impl_nongeneric.mt` | §62 | **PASSES** | — | impl on a NON-generic struct — control, so impl is not the culprit |
| `gs62_two_param_fn.mt` | §62 | **PASSES** | — | two type params on a FUNCTION — control, so generics are not the culprit |
| `ord63_str_via_bound.mt` | §63 | **FAILS** | `§63: str through <T: Ord> — accepted, no diagnostic, comp` | str through <T: Ord> — accepted, no diagnostic, comparison always false |
| `ord63_str_direct.mt` | §63 | **PASSES** | — | direct str comparison — correctly a TypeError. The front end KNOWS. |
| `ord63_float_via_bound.mt` | §63 | **FAILS** | `§63: float through <T: Ord> — the returned VALUE is corru` | float through <T: Ord> — the returned VALUE is corrupted, not just the choice |
| `ord63_float_controls.mt` | §63 | **PASSES** | — | print_float and direct float compare — controls ruling out the printer |
| `ord63_int_trit_via_bound.mt` | §63 | **PASSES** | — | int and trit through the bound — control, the bound works for these |
| `ord63_address_theory.mt` | §63 | **FAILS** | `§63: REFUTES the reference's address explanation: swappin` | REFUTES the reference's address explanation: swapping DECLARATION order does not flip it |
| `s64_reverse_kills_t3.mt` | §64 | **FAILS** | `§64: str::reverse on multi-byte: T3 emits NOTHING, losing` | str::reverse on multi-byte: T3 emits NOTHING, losing all later output |
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

### `ord63_str_via_bound.mt` — §63

* expected stdout: `mm
mm
zz`
* T3 actual: `aa\naa\nab`
* LLVM actual: `aa\naa\nab`

### `ord63_float_via_bound.mt` — §63

* expected stdout: `2.5
-1.5`
* T3 actual: `0.000...001 (denormal) then NaN`
* LLVM actual: `same`

### `ord63_address_theory.mt` — §63

* expected stdout: `zzz
zzz`
* T3 actual: `aaa\naaa`
* LLVM actual: `aaa\naaa`

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

* 19 programs, 9 failing, 10 controls
* §61 payload-enum constructor · §62 generic struct across a boundary · §63 unbound `T: Ord`

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
