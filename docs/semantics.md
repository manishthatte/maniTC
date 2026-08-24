# ManiT — Normative Operational Semantics (Core)

© Manish Jagdish Thatte

**Status: version 0.3, 24 August 2026. Normative for the constructs it
covers, and silent about everything else.**

This document says what a ManiT program *means*, independently of how either
backend compiles it. Where this document and an implementation disagree, the
implementation is wrong — that is the point of writing it down, and it is the
same standing the T3ISA reference already has.

## 0. Why this exists

maniTC has two backends, which gives a differential oracle: compile a program
twice, run it twice, compare. That finds disagreements and is **structurally
blind to shared mistakes** — anything decided upstream of the split, in the
lexer, the parser, the analyser or the IR lowering. Both backends can agree and
both be wrong, and on this project they twice have been (negative module-level
constants; module-level `bool3`).

A third account of the language, derived from written rules rather than from
the same front end, breaks that symmetry. `src/reference/` implements exactly
this document and is forbidden to import from the rest of the compiler.

This paid for itself immediately. Writing §6.7 found `int as trit` failing to
clamp on T3 (report.txt P2) within the hour. The conformance suite's first
complete run found two more, and the second is the one that matters:

- **P3** — `tposs`, `tnec` and `timp`/`teq` on two `bool` operands returned -1
  as a `bool`. T3's `if` tests sign and read it as false; LLVM's tests nonzero
  and read it as true, so `tnec x` was TRUE FOR EVERY x on one backend.
- **P4** — block scoping was lost in the IR lowerer, on **both backends
  identically**, so a shadowed binding never came back. Every cross-backend
  test in the repository agreed, because they compare two consumers of one
  front end. This is the third recorded instance of that trap.

## 1. Scope

**In scope (the core):**

- types `trit`, `bool3`, `bool`, `int`
- integer and trit literals, `true`/`false`, `True`/`Unknown`/`False`
- `let`, `let mut`, assignment to a local
- arithmetic `+ - * / %`, unary `-`
- comparison `== != < > <= >=`
- short-circuit `&&`, `||`
- three-valued `tand tor txor tcon tany timp teq`, unary `tnot tposs tnec`
- lane-wise `tandw torw txorw timpw tcmpw`, unary `tnotw`
- casts between the four core types
- `if` / `elif` / `else`, `tif`, `while`, `return`
- function definition and call, including recursion
- output via `io::print_int`, `io::println_int`, `io::print`, `io::println`

- `Result<T, str>`, its three constructors and six accessors, `?`, and `match`

**Out of scope, deliberately:** floats, strings beyond literal output and
`Result` messages, arrays, structs, enums, traits, generics, closures, modules,
concurrency, the heap.

**Out of scope but wanted next: tail expressions.** `fn f() -> int { if c { 1 }
else { 2 } }` — a block whose last expression is its value, and `if` used as an
expression — is idiomatic ManiT and the reference interpreter does not yet
parse it, so conformance programs are written with explicit `return`. That is a
limitation of the reference, not of the language, and it is the next increment. These are not un-specifiable; they are simply not
specified *yet*. §11 of the recommendations is explicit: specify the core and
grow it, rather than attempting the language in one pass.

A program using anything out of scope is outside this document, and the
conformance suite does not run it.

### 1.1 Language versions

A ManiT program is written in a **language version**, selected by `--lang` and
defaulting to **v1**. Two versions exist:

| | |
|---|---|
| **v1** | The language as it shipped. The default. |
| **v2** | C4 — `/` and `%` round to nearest (§6.1.2). N5 — `Int` is a 27-trit word on every backend, so §10.1's divergence does not arise. |

Only §6.1 differs between them, and §10.1 stops applying under v2. Everything
else in this document is version-independent.

v1 remains the default because recommendation R2 holds that delay is preferable
to making a change of this kind casually, and moving the default in the same
release that introduces the behaviour would be making it casually. When the
default moves, it will move as its own change.

`--warn division-semantics` lists every `/` and `%` whose meaning depends on
the version. That list is the migration backlog, generated from the program
rather than maintained by hand.

## 2. Syntax of the core

```
program ::= fndef+
fndef   ::= "fn" ident "(" params ")" ("->" type)? block
params  ::= (ident ":" type ("," ident ":" type)*)?
type    ::= "trit" | "bool3" | "bool" | "int"

block   ::= "{" stmt* "}"
stmt    ::= "let" "mut"? ident (":" type)? "=" expr ";"
          | ident "=" expr ";"
          | "if" expr block ("elif" expr block)* ("else" block)?
          | "tif" expr "{" "+" "=>" arm "," "0" "=>" arm "," "-" "=>" arm ","? "}"
          | "while" expr block
          | "return" expr? ";"
          | expr ";"
arm     ::= expr | block

expr    ::= literal | ident | ident "(" args ")"
          | unop expr | expr binop expr | "(" expr ")" | expr "as" type
literal ::= INT | "+" | "0" | "-" | "true" | "false"
          | "True" | "Unknown" | "False"
```

Precedence, loosest to tightest: `||`; `&&`; the three-valued and lane-wise
operators (one level, left-associative); comparison (non-associative — `a < b <
c` is a syntax error); `+ -`; `* / %`; unary; `as`; call; parentheses.

The trit literals `+`, `0`, `-` are ambiguous with the arithmetic operators.
The rule is positional: in operand position they are literals, in operator
position they are operators. `0` is always the integer zero and is also the
trit zero — the two are the same value at different types (§3).

## 3. Values

```
Trit   = {-1, 0, +1}          written -, 0, +
Bool3  = {-1, 0, +1}          written False, Unknown, True
Bool   = {0, 1}               written false, true
Int    = [-T3_MAX, +T3_MAX]   T3_MAX = (3^27 - 1)/2 = 3,812,798,742,493
```

`Trit` and `Bool3` share a carrier and differ only in type. `Bool` **does
not**: `false` is `0`, and `0` read as a trit is *unknown*, not *false*. This
is a real hazard and it is specified rather than smoothed over — see §6.4.

`Int` is defined **only** on the 27-trit range. An operation whose true result
leaves that range traps (§8). Implementations currently disagree here and the
disagreement is known: see §10.1.

## 4. Configurations

A configuration is `⟨ s , σ , ω ⟩`:

- `s` — the statement or expression being reduced
- `σ` — the store, a finite map from identifier to value
- `ω` — the output trace, a finite sequence of bytes

Reduction is written `⟨ s, σ, ω ⟩ → ⟨ s', σ', ω' ⟩`. A program's **observable
behaviour** is the pair `(ω, outcome)` where `outcome ∈ {Normal, Trap}`. Two
implementations conform iff for every core program they produce the same
observable behaviour. Nothing else — not timing, not instruction counts, not
register allocation — is observable.

Evaluation is deterministic. Every rule below has a disjoint left-hand side.

## 5. Evaluation order

Sub-expressions reduce **left to right**, fully, before the operator applies:

```
        ⟨e1, σ, ω⟩ → ⟨e1', σ', ω'⟩
    ───────────────────────────────────────
    ⟨e1 ⊕ e2, σ, ω⟩ → ⟨e1' ⊕ e2, σ', ω'⟩

        ⟨e2, σ, ω⟩ → ⟨e2', σ', ω'⟩
    ───────────────────────────────────────      (v1 a value)
    ⟨v1 ⊕ e2, σ, ω⟩ → ⟨v1 ⊕ e2', σ', ω'⟩
```

with the two exceptions in §6.3. Function arguments likewise reduce left to
right before the call.

This matters because expressions can print: `f(g(), h())` runs `g` before `h`,
and the output trace records it.

## 6. Primitive operations

### 6.1 Arithmetic

`+`, `-`, `*` are the ordinary integer operations on `Int`, trapping if the
true result leaves the range (§8).

`/` and `%` are the one place where this document is **parameterised by the
language version** (§1.1). Everything else in it means the same thing under
both.

#### 6.1.1 Division under v1

**Division truncates toward zero, and `%` takes the sign of the dividend.**

```
    7 /  2 =  3        7 %  2 =  1
   -7 /  2 = -3       -7 %  2 = -1
    7 / -2 = -3        7 % -2 =  1
   -7 / -2 =  3       -7 % -2 = -1
```

#### 6.1.2 Division under v2 — C4

**Division rounds to the nearest integer, ties away from zero, and `%` is the
remainder that pairs with it.**

```
    7 /  2 =  4        7 %  2 = -1
   -7 /  2 = -4       -7 %  2 =  1
    1 /  3 =  0        1 %  3 =  1
    2 /  3 =  1        2 %  3 = -1
   -2 /  3 = -1       -2 %  3 =  1
    5 /  3 =  2        5 %  3 = -1
    4 /  3 =  1        4 %  3 =  1
```

`%` is **defined from** `/` — `a % b = a − (a / b) × b` — and is not given a
rule of its own. Stating it separately would state the identity below twice and
invite the two statements to disagree.

Ties go away from zero rather than to even. Round-half-to-even is the
statistically unbiased tie-break and was the alternative considered; it was
rejected because balanced ternary's unbiasedness comes from the
**representation**, not from the tie-break, and the property worth preserving
is the symmetry the representation already has:

```
    (-a) / b  =  -(a / b)        for every a and every b ≠ 0
```

which half-to-even does not have.

The balanced remainder lies in `[-|b|/2, +|b|/2]`, so unlike v1's it can be
**negative for a positive dividend**. `x % 2 == 0` is still an evenness test;
`x % 2 == 1` is no longer an oddness test.

#### 6.1.3 What holds in both

The identity `(a / b) * b + (a % b) = a` holds for every `a` and every `b ≠ 0`,
under **both** versions. That requirement is why C4 changes `%` as well as `/`:
rounding one while truncating the other would break it.

`b = 0` traps, for both `/` and `%`, under both versions.

`math::div_trunc`, `math::rem_trunc`, `math::div_near` and `math::rem_near`
name the two behaviours explicitly and mean the same thing under both versions.
They are the migration path: code written over them says which division it
wants and goes on saying it across the version boundary.

> **Both versions are pinned, deliberately.** C4 is a change to the value a
> program computes, and it can only be *detected* as a change against a written
> statement of what the value was before. §6.1.1 is that statement, and the
> conformance suite checks both — three ways, under each version.

### 6.2 Comparison

`== != < > <= >=` compare two values of the same core type numerically, on the
carriers of §3, and produce `Bool`. Comparison does not chain.

### 6.3 Short-circuit

`&&` and `||` are the two exceptions to §5. `e1 && e2` reduces `e1`; if it is
`0` the result is `0` and **`e2` is not reduced at all** — in particular its
output is not produced. `e1 || e2` likewise short-circuits on `1`.

### 6.4 Three-valued operators

On `Trit`/`Bool3` carriers, with `a` and `b` in {-1, 0, +1}:

| op | definition |
|---|---|
| `a tand b` | `min(a, b)` |
| `a tor b` | `max(a, b)` |
| `tnot a` | `-a` |
| `a txor b` | balanced sum mod 3 (§6.5) |
| `a tcon b` | `+1` if `a = b = +1`; `-1` if `a = b = -1`; else `0` |
| `a tany b` | `+1` if either is `+1`; else `-1` if either is `-1`; else `0` |
| `a timp b` | `min(+1, 1 - a + b)` |
| `a teq b` | `(a timp b) tand (b timp a)` |
| `tposs a` | `1` if `a ≥ 0` else `0` — result type `Bool` |
| `tnec a` | `1` if `a = +1` else `0` — result type `Bool` |

`timp` is **Łukasiewicz, not Kleene**: the `a = b = 0` cell is `+1`, where
Kleene's `max(-a, b)` gives `0`. Consequently `a timp a = +1` for every `a`,
including unknown — the deduction theorem. An implementation giving `0` there
implements a different logic and does not conform.

`tany` is not the dual of `tcon` under negation: `+1` wins over `-1` wherever
both appear, so `- tany + = +`.

**Operand types.** These operators take `Trit`, `Bool3` or `Bool` only. A
`Bool` operand is converted to the `Bool3` carrier by `b ↦ 2b − 1`, so `false`
becomes `-1` and `true` becomes `+1`. Applying them to a multi-trit ternary
number (`tryte`, `t9`, `t27`, `t54`, `tfloat`) is **not defined** and must be
rejected; see report.txt P1 for what happened when it was not.

`tand`, `tor`, `tany`, `timp` and `teq` are closed on `{-1, +1}`, so with two
`Bool` operands the result is `Bool`. `txor` and `tcon` are not closed —
`true txor false` is unknown — and their result is `Bool3`.

> **The `Bool` hazard, stated rather than hidden.** `false` is `0` in the
> `Bool` carrier and `-1` in the `Bool3` carrier. The conversion above applies
> in three-valued operator positions. It does **not** apply to `as trit`
> (§6.7), so `false as trit` is `0`, i.e. *unknown*. That is what the language
> does today; it is a wart, and specifying it is the first step to changing it.

### 6.5 Balanced sum mod 3

`txor` and `txorw` use, per trit:

|  | `b = -` | `b = 0` | `b = +` |
|---|---|---|---|
| **`a = -`** | `+` | `-` | `0` |
| **`a = 0`** | `-` | `0` | `+` |
| **`a = +`** | `0` | `+` | `-` |

It is **not** an involution. `x txor k txor k ≠ x`; three applications recover
the original, because `3k ≡ 0 (mod 3)`. Binary XOR undoes itself after two only
because `2 ≡ 0 (mod 2)`, which is an accident of base 2.

### 6.6 Lane-wise operators

`tandw torw txorw timpw tcmpw tnotw` read an `Int` as 27 independent trits and
apply the corresponding connective to each. Lane `i` of `w` is the digit `d_i`
in `w = Σ d_i·3^i`, `d_i ∈ {-1, 0, +1}`; the decomposition is unique. Per-lane
definitions are those of §6.4, plus `a tcmpw b` = `sign(a_i − b_i)` per lane.
`tnotw a = -a`, because negating a balanced-ternary number negates every trit.

Normative detail is in `docs/t3isa-reference.md` §5, which this section does
not duplicate. No lane-wise operation can trap: every lane result is in
{-1, 0, +1}, so the reassembled word is in range by construction.

### 6.7 Casts

| from → to | meaning |
|---|---|
| `Int → Trit` | **clamp** to {-1, 0, +1} |
| `Int → Bool` | `0 ↦ 0`, anything else `↦ 1` |
| `Trit → Int` | identity on the carrier |
| `Trit ↔ Bool3` | identity on the carrier |
| `Bool → Int` | identity on the carrier (`false ↦ 0`, `true ↦ 1`) |
| `Bool → Trit` | identity on the carrier — **not** the `2b−1` conversion |

The clamp is not a truncation: `5 as trit` is `+1`, not the low trit of 5.
T3 did not clamp until 24 August 2026 (report.txt P2).

### 6.8 `Result<T, str>`

The core's one compound type, and the language's strongest claim. A `Result` has
**three** outcomes, carried by a trit tag:

| constructor | tag | carries |
|---|---|---|
| `Ok(v)` | `+1` | the value `v` |
| `Unknown(m)` | `0` | a message saying why the answer is not known |
| `Err(e)` | `-1` | a message saying what failed |

**`Unknown` is not a kind of `Err`.** `Err` means it failed; `Unknown` means we
do not know, which is not a failure. An implementation that merges them does
not conform. This is the distinction binary languages must encode as either an
error or a sentinel, and both are lies.

The error type is fixed to `str` in the core, which is what the language
reference recommends in general (`Result<T, str>`, with `Unknown(msg)` serving
where another language would write `None`). There is no `Option<T>`.

Accessors:

| | |
|---|---|
| `r.tag()` | the trit: `+` Ok, `0` Unknown, `-` Err |
| `r.is_ok()`, `r.is_unknown()`, `r.is_err()` | the same question, one yes-or-no at a time |
| `r.unwrap()` | the payload; **traps** on the other two, with different messages |
| `r.unwrap_or(d)` | the payload, or `d`. `d` is evaluated either way |

`tag()` is the primitive: it yields a trit, so `tif r.tag()` is one three-way
dispatch rather than a chain of tests. `unwrap` names one of three outcomes, so
the other two trap (§8) — and the two trap messages differ, because "it failed"
and "we do not know" are different facts and one message would hide which.

### 6.9 `?`

`e?` evaluates `e` to a `Result`. On `Ok(v)` the expression is `v`. On
`Unknown` or `Err`, **the whole Result is returned from the enclosing
function, unchanged.**

Unchanged is the operative word: an `Unknown("no data")` propagated through
three calls arrives at the top still `Unknown` and still saying `no data`. `?`
is a return, not an error path, which is exactly why the third state survives
it.

```
fn chain(k: int) -> Result<int, str> {
    let v = mk(k)?;          // Unknown here returns Unknown from chain
    return Ok(v + 1);
}
```

`?` is not a trap and does not end the program. If it reaches the top of `main`
the program stops normally.

### 6.10 `match` on a `Result`

```
match r {
    Ok(v)      => …,   // v is the payload
    Unknown(m) => …,   // m is the message
    Err(e)     => …,   // e is the message
}
```

**All three arms are required**, or a wildcard `_`. A `Result` is a closed
three-variant type and is treated exactly as an enum, a trit and a `bool3`
already are.

> Until 24 August 2026 it was not. `match r { Ok(v) => .., Err(e) => .. }`
> compiled, and when `r` was `Unknown` the third state vanished — T3 halted with
> exit status 24 and lost the rest of the program, LLVM fell through the match
> and carried on, and neither said anything. That is report.txt P6: the exact
> failure this type exists to prevent, in the construct most used to consume it.

`tresult` is the same dispatch as a dedicated form and has always required all
three arms.

## 7. Statements

`let` binds; `let mut` binds mutably; assignment to a non-`mut` binding is
rejected before evaluation. Bindings are lexically scoped to the enclosing
block and shadowing is permitted.

`if` evaluates its condition to `Bool` and takes the first branch whose
condition is `1`.

`tif` evaluates its scrutinee to a `Trit`/`Bool3` and takes the `+`, `0` or
`-` arm according to its value. **All three arms are required.** There is no
fall-through and no default: a three-valued scrutinee has exactly three
outcomes, and allowing two arms would let one silently vanish.

`while` re-evaluates its condition before each iteration.

`return` ends the enclosing call with the given value, or with no value from a
`-> void` function. Falling off the end of a non-void function is rejected
before evaluation.

## 8. Traps

A trap ends the program immediately with `outcome = Trap`. The output trace
produced up to that point is retained and is observable.

Trapping operations:

- integer `+ - *` whose true result leaves `[-T3_MAX, T3_MAX]`
- `/` or `%` with divisor `0`

Nothing silently clamps. A saturating result wearing a success label is the
failure mode this rule exists to prevent, and it has cost this project a
correct answer before (T3ISA v1.4, `fib_safe(70)`).

## 9. Conformance

An implementation conforms iff, for every program in the core (§1), it produces
the observable behaviour (§4) these rules assign. `tests/conformance_tests.rs`
checks the three implementations — the reference interpreter, the T3 backend,
and the LLVM backend — against each other and against this document.

Agreement between any two is **not** evidence. Three-way agreement, with one
party derived from this text rather than from the shared front end, is.

## 10. Known divergences

Recorded rather than smoothed over. Each is a defect in an implementation or a
gap in this document, and each has an owner.

### 10.1 `Int` width beyond the 27-trit range — N5. **Closed under v2.**

§3 defines `Int` on `[-T3_MAX, T3_MAX]` and §8 says leaving it traps. Under
**v1**, T3 traps and **LLVM does not** — it continues into 64-bit arithmetic:

```
let m: int = 3812798742493;   // T3_MAX
m + 1                          // v1:  T3: TRAP    LLVM: 3812798742494
                               // v2:  T3: TRAP    LLVM: TRAP
```

Under **v2** the LLVM backend range-checks `int` addition, subtraction and
multiplication and both backends trap. The check is on the operands and the
true result is computed in `__int128`, so a product that overflows the machine
word is caught on its real value rather than on a wrapped one.

The divergence therefore remains only under v1, where it is the behaviour v1
had and is left alone on purpose. Under v1 this document is normative only on
the defined range, and the conformance suite generates no value outside it
except in the tests explicitly marked v2.

What v2 does **not** cover, stated so that it is not mistaken for covered:
`int` literals, casts, `<<`, and values returned by natives are not
range-checked on LLVM. Only the three arithmetic operators are. N5's claim is
about arithmetic, and widening it further is a separate decision.

### 10.2 `false as trit` is unknown, not false — §6.4

Both backends agree, so the two-backend oracle cannot see it. Specified in
§6.7 as it currently behaves. Changing it is a breaking change and belongs
with C4/N5 behind one version bump, not on its own.

### 10.3 `Trint` beyond the machine's word

`trint` is the wider type v2 offers to code that wants the machine word, and it
is deliberately **not** range-checked (an opt-in with no escape hatch is not an
opt-in). But a T3 register *is* 27 trits, so a `trint` cannot hold more than
that on T3 and the machine traps:

```
let w: trint = 3812798742493;
w + 1                          // T3: TRAP    LLVM: 3812798742494
```

This is a pre-existing property of `trint` on T3, not something N5 introduces;
`trint` has mapped to `IRType::I64` since before either version existed. Once
v2 closes §10.1, it is the **only** remaining place where the two backends
disagree about integer width. Recorded as report.txt P9.

## 11. Changes

- **0.1** (24 Aug 2026) — first version. Core as listed in §1. Written as A3,
  Phase 3. Found report.txt P2 while being written, and P3 and P4 on the
  conformance suite's first complete run.
- **0.2** (24 Aug 2026) — added `Result`, `?` and `match` (§6.8–§6.10). Found
  P6: `match` on a `Result` was not checked for exhaustiveness, so the third
  state could vanish, differently on each backend. The rest of the `Result`
  surface was measured and is correct — `?` really does propagate `Unknown`
  distinctly, on both backends, with the message intact.
- **0.3** (24 Aug 2026) — C4 and N5, behind R2's version machinery. Added §1.1
  (language versions), split §6.1 into the v1 and v2 division rules, closed
  §10.1 under v2 and added §10.2's neighbour §10.3 (`trint` on T3). The
  document is now parameterised by version in exactly one place.

  Two defects were found by landing it, both by the conformance and parity
  machinery rather than by either backend alone: report.txt **P7**, a latent
  return-value ABI mismatch in the C runtime that C4's longer instruction
  sequence turned into a wrong answer, and **P8**, spans in merged stdlib
  source being attributed to the user's file.
