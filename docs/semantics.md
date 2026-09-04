# ManiT — Normative Operational Semantics (Core)

© Manish Jagdish Thatte

**Status: version 0.9, 2 September 2026. Normative for the constructs it
covers, and silent about everything else.**

> **Correction, 2 September 2026.** This line read *"version 0.3, 24 August
> 2026"* until today, while §12 below has carried a **0.4** entry since 29
> August. Nothing in it was a false sentence when written; it simply was not
> reopened when §11 landed. That is the fourth shape in this repository's
> documentation-defect series — an absence, a word, a mechanism, and a count —
> here in the normative document itself, and it is now pinned by
> `tests/conformance_tests.rs` rather than described.


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

- `spawn`, `yield`, and a channel with `send`/`recv` — **§11, and §11 alone**.
  That section specifies a target and is ahead of every implementation,
  including the reference; §1.2 says what that means for the suite.

**Out of scope, deliberately:** floats, strings beyond literal output and
`Result` messages, arrays, structs, enums, traits, generics, closures, modules,
the heap. Concurrency was on this list until §11 was written; `Task<T>`,
`await`, `Mutex`, `Barrier`, `Semaphore` and bounded channels remain on it, and
§11.1 says why each.

**Tail expressions are IN SCOPE as of 5 September 2026 (§7.6).** This
paragraph used to read "out of scope but wanted next", and the reason it was
promoted is worth keeping: it was the one entry on that list where **the
reference was behind the LANGUAGE**, not ahead of it. The compiler already
accepted `fn f(c: bool) -> int { if c { 1 } else { 2 } }` and printed `1` and
`2`; the reference could not parse it, so conformance programs were written
with explicit `return` to work around a limitation of the third account. §1.2
requires a section that is ahead of the implementations to say so in its own
first line — a section that is BEHIND them is the case §1.2 does not cover, and
it is worse, because nothing announces it. These are not un-specifiable; they are simply not
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

### 1.2 Sections that are ahead of the implementations

Every section of this document except §11 describes behaviour that all three
implementations have. **§11 describes behaviour none of them has yet**, and it
is marked so in its own first line.

The distinction has to be carried into the suite rather than left to a reader,
because a conformance row is a claim about agreement and these rows are claims
about a *gap*. A row that runs a §11 program against a backend is asserting
that the backend does not implement §11 yet — which is a true and useful thing
to pin, since it is what will change when it does — and it must say so in its
name and its message. A row that merely fails, or is quietly skipped, turns the
specification's lead over the implementation into either a false alarm or
nothing at all.

This is the same rule §9 states for agreement, applied to disagreement:
**record it, give it an owner, and never let it be silent.**

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

This is the configuration of a program with one thread of control, which is
every program §1 admits except those using §11. §11.3 extends it to a task
pool and §11.7 shows the determinism survives.

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
`-> void` function.

### 7.6 Tail expressions

**Added 5 September 2026.** A block's last statement may be an expression
written **without a trailing `;`**. That expression is the block's **value**.

```
fn g() -> int { 7 }
fn f(c: bool) -> int { if c { 1 } else { 2 } }
```

Four rules, and each is checked by a conformance row:

1. **A tail expression must be last in its block.** An expression not followed
   by `;` anywhere else is a syntax error, not a statement.
2. **A function whose body ends in a tail expression returns that value.** This
   is why "falling off the end of a non-void function is rejected" no longer
   describes such a function: it does not fall off the end, it ends in a value.
3. **`if` is an expression when its arms end in tail expressions**, and then
   every arm must — including an `else`, because an `if` with no `else` has no
   value when the condition is false. `tif` behaves the same way, and it
   already requires all three arms for a separate reason (above).
4. **A `while` body is not a value position.** A loop runs any number of times,
   so there is no one value for it to produce; a tail expression inside a loop
   body is evaluated and discarded. **`return` is unaffected everywhere**: it
   ends the enclosing CALL, so a `return` inside an `if` used as an expression
   leaves the function rather than producing the block's value.

`return` and a tail value are therefore different control flow, not two
spellings of one thing, and the reference interpreter models them as two
distinct states for exactly that reason.

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

## 11. Interleaving

**Status, corrected again 2 September 2026 (0.9): §11.1–§11.8 are IMPLEMENTED
by all three; §11.9, §11.11 and §11.12 by BOTH BACKENDS only; §11.10 by all
three.** §1.2 makes this line an obligation rather than a courtesy, so it is
restated whenever it stops being true — and it has now been wrong twice, in
opposite directions, which is the argument for a row rather than a sentence.

> The previous version read *"§11.9 by both backends and §11.10 by both
> backends, with the A3 reference behind on those two"*. **§11.10 was wrong**:
> `src/reference/eval.rs` carries `closed: Vec<bool>` as "§11.10 𝒦, the closed
> set" and `recv` implements (RECV-CLOSED) — P105 put it in all three accounts
> when it wrote the section. §11.11 was not mentioned at all, and the reference
> has `caps` and `blocked_send`, so it has that too.

Steps 1–3 of `enhance/phase3-the-semantics-debt/CONCURRENCY_DECISION.md` §5
have landed: the A3 reference (step 1), the T3 emulator's scheduler and the
`spawn` lowering (step 2), and the C runtime's scheduler and outlining
(step 3). `--sched cooperative` compiles §11 on both backends and
`examples/concurrency.mt` is byte-identical between them. **§11.9 is step 4 and
is not implemented at the moment it is being read for the first time**, which
is the state §1.2 exists to make legible.

> The original text, kept because it is the record and because it was true for
> six days: *"this section specifies a TARGET, and is ahead of all three
> implementations. Everything above describes what ManiT does; this describes
> what it will do when `CONCURRENCY_DECISION.md` is implemented. Until then
> `docs/memory-model.md` §4 is the normative account of the language as it runs
> — execution is sequential and `spawn { B }` evaluates `B` in place — and
> report.txt P5.2/P5.3 are open against that, not against this."*

`docs/memory-model.md` §4 remains the account of the **default** mode, which is
still `--sched inline` and still evaluates `spawn { B }` in place; P5.2 and
P5.3 are open against that. What changed is that the scheduled mode is no
longer hypothetical.

Saying so is the point of writing it here rather than later. The decision was
taken on 24 August 2026 and its own §5 puts specification first, "because A3's
whole point is that the third implementation comes from the written rules, and
concurrency is where an unwritten rule does the most damage".

### 11.1 What is in this core

`spawn { … }`, `yield`, and a channel with `send` and `recv`. That is all,
and the smallness is deliberate: §1's rule is to specify the core and grow it.

**Deliberately not specified yet**, each because it needs a decision this
document should not take casually:

- ~~**`Task<T>` and `await`**~~ — **specified in §11.12 as of 0.8 and
  IMPLEMENTED BY BOTH BACKENDS as of 0.9, both 2 September 2026.** It takes the
  three decisions this bullet named: awaiting a finished task returns
  immediately, awaiting twice is a TRAP, and a handle may outlive its task. The
  **A3 reference does not** implement it and cannot as written — §2's core
  grammar gives a block no value — which §11.12 now says in its own first line
  instead of the reverse, which is what it said when written.
- ~~**`Mutex`, `Barrier`, `Semaphore`**~~ — **specified in §11.9 as of 0.5,
  2 September 2026.** The decision document §2 keeps them as *structured
  waiting* rather than mutual exclusion. They are expressible in terms of
  §11.4's yield points, and they need none of their own — a claim this bullet
  made before it had been checked, and which §11.9 discharges by giving the
  expression. It is exactly true: no new rule, no new yield point, no new
  configuration component.
- ~~**Bounded channels**~~ — **specified in §11.11 as of 0.7, 2 September
  2026.** This bullet read "so `send` never blocks (§11.4)", and on the LLVM
  backend it was **false the whole time**: `channel_new` allocated a 256-slot
  ring and a 257th send blocked on a condition variable nothing could signal,
  so the program printed nothing at all while T3 grew its queue and answered
  (report.txt P107). The clause that justified §11.4's list of three was the
  one the implementation contradicted.
- ~~**Closing a channel**~~ — **specified in §11.10 as of 0.6, 2 September
  2026**, which is how a receiver learns no more will come. Without it the only
  way a `recv` ends is a value or the deadlock of §11.6. This bullet was the
  reverse of §11.9's: **both backends had implemented `close` since before §11
  was written**, and `examples/concurrency.mt` depended on it, so the document
  was the thing that lagged. Writing the rules down found two defects.
- **Three-valued synchronisation** — `held / free / unknown`, the genuinely
  novel question. It depends on D2 (clock-domain types) and is research.

### 11.2 Tasks do not share a store

A spawned task gets a **copy** of the spawning task's store at the moment of
the spawn, and its writes are its own. Channels are the only way one task can
affect another.

This is not a simplification made for the reference's convenience; it is the
strongest form of the decision document's claim. Cooperative scheduling makes
data races *unreachable rather than undefined* by confining interleaving to
yield points — and with no shared mutable state at all, there is nothing to
race over even at one. It is also the shape §6 of that document endorses for
anything wanting true parallelism later: "processes over channels, not inside
the memory model".

The core has no heap (§1), so this costs nothing here. A version of this
section that admits references will have to say what a spawn captures, and
that is where the yield-point rule starts doing real work.

> **Notice, 3 September 2026 (report.txt P118) — the surface language does not
> honour this clause for an AGGREGATE, and the two backends fail in opposite
> directions.** Measured with the task's write ordered before the spawner's
> read, so the answer is about sharing and not about scheduling:
>
> ```
> struct P { pub x: int, pub y: int }
> let mut p = P { x: 1, y: 2 };   spawn { p.x = 99; }   yield;   print(p.x)
>
>   T3    99   the task and the spawner hold the same heap cell
>   LLVM   1   but for the wrong reason: the outlined body binds the captured
>              ADDRESS as though it were the value, so a task that merely READS
>              the capture sees `x=94299331632576 y=0`
> ```
>
> The paragraph above predicted exactly this — "a version of this section that
> admits references will have to say what a spawn captures" — and the surface
> language admits references while the core does not.
>
> **Until the copy exists, capturing an aggregate in a `spawn` is REFUSED** by
> the move checker, naming this clause. The population was measured before the
> refusal was written: 0 of the 34 `spawn` sites across both repositories and
> the 2,507-file corpus capture one; every one captures a channel, a
> `Mutex`/atomic handle, or a scalar, all of which are one word and copy
> correctly. A field-less struct — `AtomicTrit`, `Barrier`, `Semaphore` — is a
> handle by that same test and is not refused.
>
> **The other direction of this clause is now implemented rather than merely
> stated**: because the task's store is a copy, a move INSIDE a spawned block
> consumes the task's binding and not the spawner's. The move checker used to
> treat `spawn { B }` as a plain block, and its own comment said so.
>
> Making the copy real is B7's D-4 with F-4's regions behind it: a deep copy at
> the spawn site is what this clause asks for, and P63's 2,536-word heap with
> no free is why it cannot be written today. **The A3 reference is unaffected
> and always was** — it gives each task its own store by construction, which is
> what §1.2 asks a normative section to be able to say.

### 11.3 Configurations

§4's `⟨ s , σ , ω ⟩` becomes `⟨ R , B , 𝒞 , ω ⟩`:

- `R` — the **run queue**, a finite sequence of tasks. A task is a pair
  `⟨s, σ⟩` of a statement sequence and its own store. The **head of `R` is the
  running task**; there is never more than one.
- `B` — the **blocked map**, from channel to the finite sequence of tasks
  waiting to receive on it, longest-waiting first.
- `𝒞` — the **channel store**, from channel to the finite sequence of values
  sent and not yet received.
- `ω` — the output trace, exactly as in §4. It is one trace for the whole
  program, not one per task: output is a program-level observable.

A program starts as `⟨ ⟨main's body, ∅⟩ , ∅ , ∅ , ε ⟩`. Observable behaviour
is still the pair `(ω, outcome)` of §4, and nothing else — in particular **not
which task produced which part of `ω`**, and not how many steps a task ran
before yielding.

### 11.4 Yield points — the complete list

A running task keeps running until it reaches one of exactly three things:

1. **`yield`** — explicit.
2. **`recv` on a channel whose queue is empty.**
3. **its own termination.**

> **Amended 2 September 2026 (0.7): there are now FOUR**, and this section
> predicted the amendment — see the `send` note below, which named a full
> `send` as the fourth and said "this list is what has to change". §11.11 makes
> that change. Point 2 also gained "and which is not closed" in 0.6 (§11.10).
> The current list is:
>
> 1. `yield` — explicit.
> 2. `recv` on a channel whose queue is empty **and which is not closed**.
> 3. **`send` on a channel that is full** — bounded channels only.
> 4. its own termination.
>
> **An unbounded channel is never full, so a program that does not ask for a
> capacity cannot reach point 3**, and for such programs the original three
> are still exactly the yield points. That is why the addition moves nothing.

**That is the whole list, and its completeness is the specification.** A
conforming implementation may not switch tasks anywhere else — not at a call,
not at a return, not on a loop back-edge, not at a print, and not on a timer.

Two consequences worth stating separately, because both are choices:

- **`spawn` does not yield.** The spawning task continues; the new task is
  appended at the back. So a task's own code reads sequentially, which is what
  makes `spawn` usable without reasoning about the scheduler.
- **`send` does not yield**, because §11.1 leaves channels unbounded, so a send
  can always proceed. If bounded channels are ever added, a full `send` becomes
  a fourth yield point, and this list is what has to change. **They were added
  in 0.7 and it did** — §11.11. An UNBOUNDED `send` still does not yield, which
  is what keeps this bullet true for every program that does not ask for a
  capacity.

### 11.5 The rules

Rule (LOCAL) is what makes every rule in §5–§8 apply unchanged to the running
task: an ordinary step of the head task is a step of the configuration.

```
                    ⟨s, σ, ω⟩ → ⟨s', σ', ω'⟩
        ─────────────────────────────────────────────────────      (LOCAL)
        ⟨ ⟨s,σ⟩·R , B, 𝒞, ω ⟩ → ⟨ ⟨s',σ'⟩·R , B, 𝒞, ω' ⟩


   ⟨ ⟨spawn{b}; s, σ⟩·R , B, 𝒞, ω ⟩ → ⟨ ⟨s,σ⟩·R·⟨b,σ⟩ , B, 𝒞, ω ⟩  (SPAWN)

        ⟨ ⟨yield; s, σ⟩·R , B, 𝒞, ω ⟩ → ⟨ R·⟨s,σ⟩ , B, 𝒞, ω ⟩      (YIELD)
```

`send` appends, and wakes at most one waiter — the longest-waiting, appended
to the back of the run queue:

```
                        B(c) = ε
        ─────────────────────────────────────────────────────      (SEND)
        ⟨ ⟨c.send(v); s, σ⟩·R , B, 𝒞, ω ⟩
                    → ⟨ ⟨s,σ⟩·R , B, 𝒞[c ↦ 𝒞(c)·v], ω ⟩

                     B(c) = ⟨t⟩·W
        ─────────────────────────────────────────────────────      (SEND-WAKE)
        ⟨ ⟨c.send(v); s, σ⟩·R , B, 𝒞, ω ⟩
                    → ⟨ ⟨s,σ⟩·R·t , B[c ↦ W], 𝒞[c ↦ 𝒞(c)·v], ω ⟩
```

`recv` takes the head of the queue, or blocks:

```
                      𝒞(c) = v·Q
        ─────────────────────────────────────────────────────      (RECV)
        ⟨ ⟨let x = c.recv(); s, σ⟩·R , B, 𝒞, ω ⟩
                    → ⟨ ⟨s, σ[x ↦ v]⟩·R , B, 𝒞[c ↦ Q], ω ⟩

                        𝒞(c) = ε
        ─────────────────────────────────────────────────────      (RECV-BLOCK)
        ⟨ ⟨t, σ⟩·R , B, 𝒞, ω ⟩ → ⟨ R , B[c ↦ B(c)·⟨t,σ⟩], 𝒞, ω ⟩
                    where t is `let x = c.recv(); s`
```

A task whose statement sequence is exhausted is removed:

```
            ⟨ ⟨ε,σ⟩·R , B, 𝒞, ω ⟩ → ⟨ R , B, 𝒞, ω ⟩               (DONE)
```

### 11.6 Termination, and deadlock as a trap

```
    R = ε   and   B = ∅          the program ends, outcome = Normal
    R = ε   and   B ≠ ∅          TRAP (§8), outcome = Trap
```

*As of 0.6, §11.10's (CLOSE) empties `B(c)` for the channel it closes, so a
task waiting on a channel that someone closes is never among the `B ≠ ∅` that
trap here. Before that rule existed it was — and the trap fired on programs
whose receivers had in fact been told there was nothing more to wait for.*

The second is the decision document's §3, and it is the strongest single
argument for cooperative scheduling: **the scheduler knows the whole runnable
set, so it can detect a deadlock a pthread runtime can only suffer.** The
message names the situation rather than the symptom:

```
TRAP: deadlock — every task is blocked on a channel that no runnable task can fill
```

The trace produced up to that point is retained and observable, exactly as §8
requires. That is what P5.1 could not do: on LLVM the same program blocked in
`pthread_cond_wait` with stdout unflushed and printed **nothing at all**, so
the trace was lost along with the answer.

**`main` returning does not end the program.** `main` is a task like any other
and simply terminates; the remaining tasks run. This is the compatible choice
rather than the obvious one, and the reason is P5.4: because `spawn { B }`
runs `B` inline today, every spawned block in every existing program has
already completed by the time `main` returns. Ending the program at `main`
would silently discard work those programs currently do.

### 11.7 Determinism

**Proposition.** `→` is a partial function: for every configuration at most
one rule applies. Therefore a core program has exactly one observable
behaviour, and §4's determinism survives concurrency intact.

*Argument.* Every rule's left-hand side is keyed on the **head** of `R`, which
is unique, and on the head task's next statement, and the statement forms
`spawn`/`yield`/`send`/`recv`/other are disjoint. (RECV) and (RECV-BLOCK) are
separated by whether `𝒞(c)` is empty, and (SEND) and (SEND-WAKE) by whether
`B(c)` is. The two termination cases of §11.6 apply only when `R` is empty,
where no other rule does.

This is why the scheduler is specified as a queue with stated insertion points
rather than left to the implementation. **An implementation is free to be slow
here and not free to be clever**: reordering the run queue, running a task
"until it blocks" past a `yield`, or waking all waiters instead of one all
produce configurations these rules do not.

**But a wrong configuration is not the same as a wrong observation, and one of
these three is much harder to catch than it looks.** Waking *all* waiters
instead of one survives every obvious test, because a spuriously woken receiver
re-executes its `recv`, finds nothing, and blocks again **while printing
nothing** — so the extra wake leaves no trace. It is observable only where it
changes the ORDER of `B`, which needs a program that wakes a receiver, takes
the value out from under it, and lets it go back to the end of the queue before
the next send chooses between it and another. `tests/interleaving_tests.rs`
carries that program, and the row above it — the obvious one, counting how many
waiters woke — does not fail against a wake-all at all.

Recorded here rather than only in the test because it generalises past this
rule: **a specification clause that selects one of several waiting things is
testable only once the choice changes what is PRINTED**, and something that
goes straight back to waiting prints nothing.

### 11.8 What implementing this will cost

Recorded here because the specification cannot be read as cost-free, and
because the cost falls on step 2 rather than on this section.

**It moves the output of existing programs.** `spawn { B }` runs `B` before
the statement after it today; under (SPAWN) it runs later. Every program that
spawns and prints has its trace reordered — `examples/concurrency.mt`
included, which P5.4 records as passing on both backends and being
byte-identical between them *because* every producer runs to completion before
the consumer starts.

**This section cannot move anything.** Concurrency was outside §1's core until
now, so no conformance program uses it and none could. The reference
interpreter can therefore implement §11 in full while both backends remain
sequential, and the resulting disagreement is the specification being ahead of
the implementations rather than a regression — provided the conformance suite
says so explicitly. See §9: a row that compares the reference against a backend
on a concurrency program is asserting the gap, and must be labelled as
asserting it.

### 11.9 Structured waiting — `Mutex`, `Semaphore`, `Barrier`

*Added in 0.5, 2 September 2026. §11.1 listed these three under "deliberately
not specified yet" and said they were "expressible in terms of §11.4's yield
points, and they need none of their own". This section discharges that
sentence: it gives the expression, and the claim turns out to be exactly true.*

**They are DERIVED forms, not primitives.** §11.5 gains no rule, §11.3 gains no
component, and §11.4's list of three yield points is unchanged and still
complete. Each of the three desugars into channel operations already specified,
so every property proved of the core — determinism (§11.7), the longest-waiting
wake order (SEND-WAKE), deadlock as a detected trap (§11.6) — holds of these by
construction rather than by a second argument.

#### The desugaring

Written in the core of §2 plus §11's `channel()`, `send` and `recv`, which is
the whole vocabulary it needs.

A **`Semaphore`** is a channel pre-loaded with one token per permit. Acquiring
takes a token; releasing puts one back. A task that finds the channel empty
blocks at §11.4's *second* yield point, which is the one that already exists.

```
    sem_new(n)      ≡   let s = channel();
                        mut i = 0; while i < n { s.send(1); i = i + 1; }  s
    s.acquire()     ≡   s.recv();          // blocks while no permit is free
    s.release()     ≡   s.send(1);         // wakes AT MOST ONE waiter
    s.available()   ≡   |𝒞(s)|
```

A **`Mutex<T>`** is a one-slot channel **carrying the protected value**, and
this is the part worth stating separately, because it answers a question §11.2
otherwise leaves open. §11.2 gives a spawned task a *copy* of its spawner's
store, so shared mutable state cannot live in a store at all; channels are the
only contact between tasks. A mutex therefore cannot be a flag beside a value —
**the value has to be the token**:

```
    mutex_new(v)    ≡   let m = channel();  m.send(v);  m
    m.lock()        ≡   m.recv()           // blocks while another task holds it
    g.get()         ≡   the value lock() returned
    g.set(v)        ≡   changes what unlock() will send back
    g.unlock()      ≡   m.send(v)          // hands it to the LONGEST-WAITING task
```

**Mutual exclusion is the token's absence from the channel**, not a lock bit,
and the exclusion is therefore the same mechanism as `recv` blocking — not an
analogue of it. `Mutex<T>` is `Semaphore(1)` whose permit carries `T`.

A **`Barrier(n)`** needs an arrival count that outlives any one task, so the
count is itself a channel, held exclusively while it is being read and written
— a mutex by the line above — and a second channel gates the release:

```
    bar_new(n)      ≡   let count = channel(); count.send(0);
                        let gate  = channel();  (count, gate)

    b.wait()        ≡   mut c = count.recv();       // take the counter
                        c = c + 1;
                        if c == n {
                            count.send(0);          // reset: the barrier is reusable
                            mut i = 1;
                            while i < n { gate.send(1); i = i + 1; }
                            true                    // the LAST to arrive is the leader
                        } else {
                            count.send(c);          // release the counter FIRST
                            gate.recv();            // then block until released
                            false
                        }
```

**The order of those last two lines is the specification, not an accident of
how it was written.** Releasing the counter *after* blocking would leave the
counter held by a task that is not running, so no later task could arrive to
release it: the barrier would deadlock at n = 2 for every program. It is
mechanical to write the two the wrong way round, and §11.6's trap is what
catches it.

#### The observable contract

An implementation is conforming when a program cannot observe a difference from
the desugaring. Stated directly, because these are the properties the tests
assert and the ones the implementations got wrong:

1. **At most one task holds a `Mutex`.** If task *A* holds it, a `lock()` in
   task *B* does not return until *A* unlocks.
2. **A `Semaphore(n)` admits at most n holders.** The (n+1)-th `acquire`
   blocks until a `release`.
3. **A `Barrier(n)` releases nobody until n have arrived**, then releases all
   n, and returns `true` in exactly one of them — the last to arrive.
4. **A release wakes at most one waiter**, the longest-waiting, appended to the
   back of `R`. This is (SEND-WAKE) unchanged, including §11.7's warning that
   waking all of them is nearly invisible to a test.
5. **Blocking here is not a new yield point.** It is yield point 2, reached
   through the desugaring. An implementation may not switch tasks on a `lock`
   that succeeds, or on a `release`.

#### What this section makes wrong, measured

Recorded here rather than left for the reader to discover, because §11.8 set
the precedent that a specification states its own cost, and because in this
case the cost is already paid by programs that exist.

Measured on `d841305` under `--sched cooperative`, two tasks contending:

| | T3 | LLVM |
|---|---|---|
| `Mutex` | **both tasks hold it at once** | hangs, printing nothing at all |
| `Semaphore(1)` | **admits two holders** | hangs, printing nothing at all |
| `Barrier(2)` | **one party passes alone** | one party passes alone |

The T3 emulator's five sites say so in their own comments — `mutex_lock` is
*"no-op in sequential model"*, and so are `mutex_unlock`, `semaphore_acquire`
and `semaphore_release`. **Those comments were true when they were written.**
In the sequential model of `docs/memory-model.md` §4 there is never more than
one task, so a lock that does nothing is not an approximation of mutual
exclusion, it *is* mutual exclusion. Steps 2 and 3 of `CONCURRENCY_DECISION.md`
§5 made tasks real and did not reach these five lines.

The LLVM half is the sharper of the two only in appearance. A hang is loud; T3
answers wrongly and exits 0, which by this project's own ranking is worse.
Both are the same defect, and the Barrier row is worse still: **both backends
agree**, so the parity matrix reports no divergence at all.

### 11.10 Closing a channel

*Added in 0.6, 2 September 2026. §11.1 listed this under "deliberately not
specified yet" while **both backends had implemented it since before §11 was
written** and `examples/concurrency.mt` depended on it. That is the reverse of
§11.9's situation — there the document led and the implementations lagged; here
the document lagged, and writing the rules down found two defects.*

`c.close()` states that no further value will be sent. It is how a receiver
learns that a drain is finished, and without it §11.6's deadlock trap is the
only way a `recv` can end other than with a value.

#### The rules

§11.3's configuration gains **`𝒦`**, the set of closed channels. A program
starts with `𝒦 = ∅`.

```
        ⟨ ⟨c.close(); s, σ⟩·R , B, 𝒞, 𝒦, ω ⟩
                    → ⟨ ⟨s,σ⟩·R·B(c) , B[c ↦ ε], 𝒞, 𝒦 ∪ {c}, ω ⟩   (CLOSE)
```

**(CLOSE) wakes EVERY task waiting on `c`, and that is the one place in §11
where all of them are woken rather than one.** It is not an inconsistency with
(SEND-WAKE); it follows from what the two operations make true. A `send`
produces exactly one value, so exactly one waiter can proceed and waking a
second would have it find nothing and block again — §11.7's invisible bug. A
`close` produces no value but makes a permanent fact true of the channel, and
**every** waiter's `recv` can now complete. Leaving any of them on `B(c)` would
strand it forever, because after a close no `send` will ever wake it.

They are appended to `R` in `B(c)`'s own order, longest-waiting first, so
(CLOSE) is as deterministic as the rest of §11.5.

Closing is **idempotent**: `c ∈ 𝒦` already makes `B(c) = ε` by the rule above,
so a second close moves nothing and adds nothing.

A closed channel still **drains** — (RECV) is unchanged and takes values sent
before the close:

```
                 𝒞(c) = ε   and   c ∈ 𝒦
        ─────────────────────────────────────────────────────  (RECV-CLOSED)
        ⟨ ⟨let x = c.recv(); s, σ⟩·R , B, 𝒞, 𝒦, ω ⟩
                    → ⟨ ⟨s, σ[x ↦ 0]⟩·R , B, 𝒞, 𝒦, ω ⟩
```

(RECV-CLOSED) takes precedence over (RECV-BLOCK), which now requires
`c ∉ 𝒦`. So a receive on a drained, closed channel **completes with the zero
value rather than blocking**, and §11.7's determinism argument still holds:
the three receive rules are separated by whether `𝒞(c)` is empty and whether
`c ∈ 𝒦`, which are disjoint conditions.

> **The zero is a real value and cannot be told from a sent one.** That is the
> cost of this rule and the reason `try_recv` exists: it answers
> `Err("closed")` where `recv` answers `0`, so a receiver that must
> distinguish "the producer sent 0" from "the producer is finished" has to use
> it. `examples/concurrency.mt` does exactly that, and its consumer loop is
> the idiom.

#### `send` on a closed channel is a trap

```
                        c ∈ 𝒦
        ─────────────────────────────────────────────────────  (SEND-CLOSED)
        ⟨ ⟨c.send(v); s, σ⟩·R , B, 𝒞, 𝒦, ω ⟩   →   TRAP

    TRAP: send on a closed channel — the value cannot be received
```

The value has nowhere to go: no receiver will ever see it, because
(RECV-CLOSED) drains what is already queued and then yields zeroes forever.
Silently discarding it is data loss with no diagnostic, which is the shape
P5.1 and P81 already rejected once for `recv`.

This is the decision this section takes, and it is taken for the same reason as
that one: **when a program cannot make progress in the way it asked to, the
implementation should say so rather than continue plausibly.**

#### What this section found

Both defects were present on **both** backends, which is why the parity matrix
reported nothing about either.

1. **`close` did not wake blocked receivers.** A task blocked in `recv` when
   another task closed the channel was never woken, and the program hit
   §11.6's deadlock trap — reporting that no runnable task could fill the
   channel, which was true and useless, because the close had already
   established that none ever would. Measured identically on T3 and LLVM.

2. **`send` on a closed channel diverged.** T3 dropped the value in silence;
   LLVM dropped it and wrote `manit: send on closed channel` to stderr. Both
   lost the value, and they disagreed about whether a program could tell.

### 11.11 Bounded channels, and the fourth yield point

*Added in 0.7, 2 September 2026. §11.1 listed this under "deliberately not
specified yet" and §11.4 named the consequence in advance: "If bounded channels
are ever added, a full `send` becomes a fourth yield point, and this list is
what has to change." This is that change, made where it was predicted.*

`channel<T>(n)` creates a channel that holds at most `n` values. `channel<T>()`
is unbounded, as §11.1 has always said.

#### `n` is a bound, not a hint

A capacity below 1 is a **trap**, not a clamp:

```
TRAP: a channel capacity must be at least 1
```

A zero-capacity channel can never hold a value, so every `send` on it blocks
forever and the program's first send is a guaranteed deadlock. Rounding it up
to 1 would turn a program that cannot work into one that quietly does something
else.

#### The rules

§11.3's configuration gains **`S`**, the *send*-blocked map — from channel to
the finite sequence of tasks waiting to send on it, longest-waiting first. It
is a second map and not an extension of `B`, and that is the point: a `recv`
must wake a **sender**, a `send` must wake a **receiver**, and one queue holding
both would let either wake the wrong kind, which is (SEND-WAKE)'s bug wearing a
new hat.

```
              |𝒞(c)| = cap(c)
        ─────────────────────────────────────────────────────  (SEND-BLOCK)
        ⟨ ⟨t, σ⟩·R , B, S, 𝒞, 𝒦, ω ⟩ → ⟨ R , B, S[c ↦ S(c)·⟨t,σ⟩], 𝒞, 𝒦, ω ⟩
                    where t is `c.send(v); s`
```

As with (RECV-BLOCK), the task is stored **with its `send` still in front of
it**, so being woken means re-executing the send rather than being credited
with it. A third task may take the space in between, and this is what makes
that behave the way the rules say.

```
                 𝒞(c) = v·Q   and   S(c) = ⟨t⟩·W
        ─────────────────────────────────────────────────────  (RECV-WAKE)
        ⟨ ⟨let x = c.recv(); s, σ⟩·R , B, S, 𝒞, 𝒦, ω ⟩
                    → ⟨ ⟨s, σ[x ↦ v]⟩·R·t , B, S[c ↦ W], 𝒞[c ↦ Q], 𝒦, ω ⟩
```

(RECV-WAKE) wakes **at most one** sender, the longest-waiting, appended to the
back of `R` — the same shape as (SEND-WAKE) and for the same reason: a receive
frees exactly one slot, so exactly one sender can proceed.

(CLOSE) wakes the senders too:

```
        ⟨ ⟨c.close(); s, σ⟩·R , B, S, 𝒞, 𝒦, ω ⟩
             → ⟨ ⟨s,σ⟩·R·B(c)·S(c) , B[c ↦ ε], S[c ↦ ε], 𝒞, 𝒦 ∪ {c}, ω ⟩
```

A woken sender re-executes its `send`, finds `c ∈ 𝒦` and traps by §11.10's
(SEND-CLOSED). **That is the right outcome and not an accident**: its value has
nowhere to go, and the alternative is a task parked forever on a channel
nothing will ever drain.

#### §11.4 now lists FOUR yield points

**Corrected here rather than rewritten in place, because §11.4 says its own
completeness is the specification.** The list is:

1. `yield` — explicit.
2. `recv` on a channel whose queue is empty **and which is not closed**
   (§11.10).
3. **`send` on a channel that is full** — bounded channels only, and new in
   0.7.
4. its own termination.

An unbounded channel is never full, so **a program that does not ask for a
capacity cannot reach point 3** and §11.4's original three remain exactly its
yield points. That is why this addition moves no existing program.

#### §11.6 counts both maps

```
    R = ε   and   B = ∅   and   S = ∅       the program ends, outcome = Normal
    R = ε   and   (B ≠ ∅  or   S ≠ ∅)       TRAP (§8), outcome = Trap
```

A task blocked on a full channel that nothing will drain is as deadlocked as
one blocked on an empty channel nothing will fill, and the scheduler can see
both. The message names which:

```
TRAP: deadlock — every task is blocked on a channel that no runnable task can drain
```

#### Determinism survives

§11.7's argument is unchanged in form. (SEND) and (SEND-BLOCK) are separated by
whether `|𝒞(c)| = cap(c)`; (SEND-CLOSED) by `c ∈ 𝒦`, which is tested first;
(RECV) and (RECV-WAKE) by whether `S(c)` is empty, and both by `𝒞(c)` against
(RECV-CLOSED) and (RECV-BLOCK). Every rule still keys on the head of `R` and on
one statement form, so `→` is still a partial function.

**And the wake-all hazard §11.7 records applies to (RECV-WAKE) exactly as it
does to (SEND-WAKE)**: a spuriously woken sender re-executes its send, finds
the channel still full, and blocks again *while printing nothing*. It is
observable only where the choice changes what is printed.

### 11.12 `Task<T>` and `await`

**Status, corrected 2 September 2026 (0.9): BOTH BACKENDS IMPLEMENT THIS
SECTION; the A3 reference does not.** §1.2 makes saying so an obligation, and
this line is the second thing in this section that had to be corrected rather
than merely updated.

> **The original status line was FALSE ON THE DAY IT WAS WRITTEN.** It read:
> *"this section is AHEAD of both backends. §1.2 makes saying so an obligation.
> The A3 reference implements it; T3 and LLVM do not"*. It had the two halves
> exactly the wrong way round about the reference — `src/reference/ast.rs`
> carries `Stmt::Spawn(Vec<Stmt>)`, a STATEMENT, with a comment saying that
> making it an expression *"would be the first half of the `Task<T>` decision
> §11.1 declines to take"*, and the core lexer has no `await` at all. Nothing
> checked the claim, because §1.2's own discipline had been applied only to the
> backends: the conformance row asserted the gap through the SURFACE compiler
> and said so in its comment.
>
> **And the reference cannot implement this section as written.** `spawn { B }
> : Task<T>` is specified "where `T` is the value of block `B`", and §2's core
> grammar gives a block no value — it is `"{" stmt* "}"` and nothing more. So
> §11.12 needs a grammar extension it does not supply before a third account of
> it can exist. That is recorded here rather than papered over, and
> `interleaving_tests::s11_12_the_reference_does_not_have_task_and_await`
> asserts the gap in both directions with the instruction to delete itself.

`structured_waiting_tests::s11_12_*` runs the behaviour on both backends, in
**both scheduling modes**, and the two reach it by different mechanisms — T3
FORKS, so the parent's `__task_fork` result is the handle and the child exits
carrying the block's value; LLVM OUTLINES, and the trampoline completes the
handle from the body's return. That difference is what makes their agreement
evidence rather than a shared lowering agreeing with itself.

*Added in 0.8, 2 September 2026, closing report.txt P5.2 and P5.3. Both
`docs/examples.md` and `docs/language-reference.md` have shown `let t = spawn
{ … }` followed by `await t` since before §11 existed. **The first half parses
today and binds `void`; the second does not parse at all.** So this is the
third section running where the documentation was ahead of the language rather
than behind it — and the first where it was ahead of BOTH.*

#### `spawn` becomes an expression

```
    spawn { B }  :  Task<T>       where T is the value of block B
```

Used as a statement its value is discarded, exactly as any expression
statement's is. **Every existing program is therefore unmoved**, which is the
whole reason the handle is a return value rather than a change to the form.

#### `await` is not a new yield point

A `Task<T>` is a **one-shot channel of capacity one** that the task sends to
when it terminates, and `await` is `recv`. So §11.4's list does not grow: a
task that has not finished is an empty queue, which is point 2.

§11.3's configuration gains **`𝒯`**, mapping a task handle to one of three
states — `running`, `done(v)`, and `taken`. The third is the only thing the
channel model does not already give, and §11.6 needs it.

```
        ⟨ ⟨let x = spawn{B}; s, σ⟩·R , … ⟩
              → ⟨ ⟨s, σ[x ↦ h]⟩·R·⟨B,σ,h⟩ , … , 𝒯[h ↦ running] ⟩   (SPAWN-T)

              𝒯(h) = done(v)
        ─────────────────────────────────────────────────────      (AWAIT)
        ⟨ ⟨let x = await h; s, σ⟩·R , … , 𝒯 ⟩
                    → ⟨ ⟨s, σ[x ↦ v]⟩·R , … , 𝒯[h ↦ taken] ⟩

              𝒯(h) = running
        ─────────────────────────────────────────────────────      (AWAIT-BLOCK)
        ⟨ ⟨t, σ⟩·R , B, … ⟩ → ⟨ R , B[h ↦ B(h)·⟨t,σ⟩], … ⟩
                    where t is `let x = await h; s`

        ⟨ ⟨ε,σ,h⟩·R , B, … , 𝒯 ⟩
              → ⟨ R·B(h) , B[h ↦ ε], … , 𝒯[h ↦ done(v)] ⟩          (DONE-T)
```

(DONE-T) replaces §11.5's (DONE) for a task with a handle, and it **wakes every
waiter**, for §11.10 (CLOSE)'s reason exactly: termination is a permanent fact,
so every awaiting task can now proceed, and one left queued would be stranded
forever.

#### The three decisions

**1. Awaiting a finished task returns immediately.** `𝒯(h) = done(v)` is
(AWAIT), which does not touch `R`. A handle whose task finished long ago is not
a special case; it is the ordinary one.

**2. Awaiting the same task twice is a TRAP.**

```
TRAP: await on a task whose value has already been taken
```

The alternative — returning the value again — is available and is rejected,
because a program that awaits one handle twice has almost certainly confused
two handles, and this document's standing preference is a **detected error over
a plausible continuation** (§11.6, §11.10's (SEND-CLOSED), §11.11's capacity
rule). It is also why `𝒯` has a `taken` state at all: the one-shot channel
alone would answer the second `await` with (RECV-CLOSED)'s zero, silently.

*This is the clause a later affine type system should subsume.* When B7 lands,
`await` consuming its handle makes the second one a COMPILE error and this trap
becomes unreachable — which is strictly better and is the reason the rule is
stated as a trap on the VALUE rather than a restriction on the handle.

**3. A handle may outlive its task, and an un-awaited value is discarded.** The
program may end with handles in `done(v)`; those values are dropped, exactly as
a spawned block's value is dropped today. Nothing waits for a task nobody
awaits, and §11.6 is unchanged: `𝒯` never keeps `R` alive.

#### §11.6 with tasks

A task blocked in `await` is in `B`, so §11.6 already covers it: if `R` empties
while somebody is awaiting, that is a deadlock, and it is reachable only when
the awaited task is itself blocked. The message says so:

```
TRAP: deadlock — every task is blocked awaiting a task that cannot finish
```

#### What this costs

**Nothing, for programs that exist.** `spawn` used as a statement is unchanged,
`await` was unparseable, and `Task<T>` named a type nothing produced. The
section can therefore be implemented backend by backend without a flag, unlike
§11's own arrival, which had to be gated because it moved output.

*Measured when it landed, 2 September 2026: `manitc check` reaches the same
verdict on all 366 `.mt` files in the two repositories and all 2,507 in the
model corpus, the 17 examples are byte-identical on both backends, and the
parity matrix is unmoved. The one live `await` in either repository is
`fetch_data(id).await` in `examples/concurrency.mt`, applied to an `async fn`
result — a surface this section does not specify, and which keeps the identity
typing it has always had for that reason.*

#### Two implementation notes that are consequences of the rules

**The value travels as a bit pattern.** A handle carries one machine word, so
`spawn { 1.5 }` must arrive at its `await` as 1.5 and not as 1. The first
implementation produced the word by storing the value at its own type into an
`alloca` and loading it back at `i64`; that is the design `§11.2`'s captured
store uses, and it does not work here, because `codegen_llvm` re-types a load
to whatever was stored — deliberately, so a store/load pair cannot disagree —
and the outlined body emitted `ret i64` for a `double`. The word is the value
itself, reinterpreted at the call boundary where an `i64`/`double` bitcast has
existed since `Ok(1.5)` needed one.

**§11.6's message names the await.** A task blocked in `await` is in `B`, so
§11.6 already covers it — but reporting a CHANNEL nobody is waiting on sends
the reader looking for a missing sender. This is §11.11's "fill" and "drain"
one construct along, and both backends had the channel message until a row
asked for the other one.

## 12. Changes

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
- **0.4** (29 Aug 2026) — §11, interleaving. The first section of this document
  that is **ahead of every implementation**, written as step 1 of
  `CONCURRENCY_DECISION.md` §5 ("specify first"), and §1.2 says what that
  obliges the conformance suite to do about it. Three yield points and no
  others; tasks that share no store, so a data race is unreachable rather than
  undefined; a deterministic run queue; and deadlock as a **detected trap**
  rather than a hang, which is the property a pthread runtime cannot offer and
  the one P5.1 measured the absence of. Found report.txt **P83** while being
  written: the decision this section implements had been taken five days
  earlier and `report.txt` still told the reader to go and take it.
- **0.5** (2 September 2026) — §11.9, structured waiting. `Mutex`, `Semaphore`
  and `Barrier` move out of §11.1's "deliberately not specified yet" list and
  into the core as **derived** forms, with the desugaring into channel
  operations that §11.1 asserted existed. The assertion holds exactly: no new
  rule, no new yield point, no new component of §11.3, and therefore no second
  determinism argument. The one result worth naming is that **a `Mutex<T>` must
  carry the protected value in the channel rather than beside it** — §11.2
  gives a spawned task a copy of the store, so shared mutable state cannot live
  in a store at all, and mutual exclusion is the token's absence rather than a
  lock bit.

  Written as step 4 of `CONCURRENCY_DECISION.md` §5, and it found what step 4
  was for: measured on `d841305`, a contended `Mutex` lets **both** tasks hold
  it on T3 and hangs printing nothing on LLVM, a `Semaphore(1)` admits two
  holders, and a `Barrier(2)` lets one party through alone **on both backends**,
  where the parity matrix cannot see it. The T3 emulator's own comments named
  the cause — *"no-op in sequential model"*, true when written and untouched by
  the two steps that made tasks real.

  Also corrected the **status line at the top of this document**, which still
  read 0.3 after 0.4 landed. A version in prose goes stale exactly as fast as a
  count in prose does, and this one sat in the normative specification.
- **0.6** (2 September 2026) — §11.10, closing a channel. The reverse of
  §11.9's situation: `close` had been implemented on **both backends since
  before §11 was written**, and `examples/concurrency.mt` depended on it, so
  the specification was the thing that lagged. Writing the rules down found two
  defects, both present on both backends and therefore invisible to the parity
  matrix: **`close` did not wake blocked receivers**, so closing a channel a
  task was waiting on deadlocked the program with a message that was true and
  useless; and **`send` on a closed channel diverged**, T3 dropping the value
  in silence while LLVM dropped it and wrote to stderr.

  The rule worth naming is **(CLOSE) wakes EVERY waiter**, the one place in
  §11 where all of them are woken rather than one. That is not an inconsistency
  with (SEND-WAKE): a `send` produces one value so only one waiter can proceed,
  while a `close` produces no value but makes a permanent fact true, and every
  waiter's `recv` can now complete. Leaving any on `B(c)` would strand it
  forever, because after a close no `send` will ever wake it.

  `send` on a closed channel is now a **TRAP**, on P81's precedent: the value
  has nowhere to go, and silently discarding it is data loss with no
  diagnostic.
- **0.7** (2 September 2026) — §11.11, bounded channels, and with them the
  **fourth yield point** §11.4 predicted in its own text. `S`, the send-blocked
  map, joins §11.3's configuration — a second map and not an extension of `B`,
  because a `recv` must wake a SENDER and a `send` must wake a RECEIVER, and
  one queue holding both lets either wake the wrong kind.

  Writing it found report.txt **P107**, which is the clause that justified the
  old list being false in the implementation: §11.1 said channels are unbounded
  and §11.4 gave that as the reason `send` is not a yield point, while on LLVM
  `channel_new` allocated a **256-slot ring** and a 257th send blocked on a
  condition variable nothing could signal. The program printed **nothing at
  all**; T3 grew its queue and answered. That is P5.1's signature for the third
  distinct time in this document's history, after `recv` on an open empty
  channel (P81) and the waiting primitives (P100).

  A capacity below 1 is a **trap** rather than a clamp: a zero-capacity channel
  can never hold a value, so every send on it blocks forever, and rounding it
  up to 1 turns a program that cannot work into one that quietly does something
  else.
- **0.8** (2 September 2026) — §11.12, `Task<T>` and `await`, closing
  report.txt P5.2 and P5.3. **The third section running where the documentation
  was ahead of the language rather than behind it, and the first where it was
  ahead of BOTH backends**: `docs/examples.md` and `docs/language-reference.md`
  have shown `let t = spawn { … }` with `await t` since before §11 existed —
  the first half parses today and binds `void`, and the second does not parse
  at all.

  Three decisions, and the second is the one with a future: **awaiting a
  finished task returns immediately**; **awaiting the same task twice is a
  TRAP** rather than returning the value again, because a program that does it
  has almost certainly confused two handles and this document prefers a
  detected error to a plausible continuation; and **a handle may outlive its
  task**, with an un-awaited value discarded exactly as a spawned block's value
  is discarded today.

  The trap is stated on the VALUE rather than as a restriction on the handle,
  deliberately: **when B7's affine types land, `await` consuming its handle
  makes the second one a COMPILE error and this trap becomes unreachable.**
  That is strictly better, and stating it this way is what lets the two coexist
  rather than conflict.

  `await` is **not** a fifth yield point. A `Task<T>` is a one-shot channel of
  capacity one that the task sends to on termination, so awaiting an unfinished
  task is §11.4's point 2 — an empty queue. The only thing the channel model
  does not supply is the `taken` state, which exists so that the second `await`
  traps instead of answering with (RECV-CLOSED)'s silent zero.

  **This section is AHEAD of both backends and says so in its own first line**,
  which §1.2 requires. The A3 reference implements it and a conformance row
  asserts the gap in both directions — the same shape §11 itself took in 0.4.

  > **Corrected in 0.9: the last sentence was false when written.** The A3
  > reference does not implement §11.12 and cannot as written — see 0.9.
- **0.9** (2 September 2026) — §11.12 **implemented on both backends**, and two
  status lines corrected. `spawn { B }` now has type `Task<T>`, `await h`
  parses in prefix position, and the six `structured_waiting_tests::s11_12_*`
  rows run the behaviour on T3 and LLVM in **both scheduling modes**.

  **Two claims in 0.8 were wrong, in opposite directions, and both were about
  which implementations had a section.** 0.8 said the A3 reference implemented
  §11.12: it does not, and `src/reference/ast.rs` says so in a comment —
  `spawn` is a STATEMENT there and `await` is not a token. §11's own status
  line said the reference was behind on §11.10: it is not, and has carried
  `closed: Vec<𝔹>` as "§11.10 𝒦" since P105 wrote that section into all three.
  **§1.2's discipline had been applied to the backends and not to the
  reference**, so the one account nothing checked is the one both claims got
  wrong. The rows now assert the reference's gap directly.

  **§11.12 cannot be given a third account as written**, and that is recorded
  rather than smoothed over: it specifies `spawn { B } : Task<T>` "where `T` is
  the value of block `B`", and §2's core grammar gives a block no value. A
  future version that wants the reference to have this section has to extend §2
  first.

  Landing it found no defect in either backend and moved no verdict: `manitc
  check` agrees on all 366 repository files and all 2,507 corpus files, the 17
  examples are byte-identical on both backends, and the parity matrix is
  unmoved. What it did find is that **the two backends disagreed about §11.6's
  message** — T3 named the await and LLVM named a channel nobody was waiting
  on — which is §11.11's "fill"/"drain" lesson one construct along and is what
  the deadlock row was written to catch.
