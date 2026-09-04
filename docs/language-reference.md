# maniT Language Reference

maniT is a statically typed, expression-oriented language built around balanced ternary
arithmetic and three-valued logic. This document is a complete specification of every
construct the compiler currently accepts.

---

## Table of contents

1. [Lexical structure](#1-lexical-structure)
2. [Types](#2-types)
3. [Literals](#3-literals)
4. [Variables and bindings](#4-variables-and-bindings)
5. [Expressions](#5-expressions)
6. [Statements](#6-statements)
7. [Control flow](#7-control-flow)
8. [Functions](#8-functions)
9. [Structs](#9-structs)
10. [Enums](#10-enums)
11. [Impl blocks and methods](#11-impl-blocks-and-methods)
12. [Traits](#12-traits)
13. [Pattern matching](#13-pattern-matching)
14. [Generics](#14-generics)
15. [Concurrency](#15-concurrency)
16. [Use declarations](#16-use-declarations)
17. [Global variables](#17-global-variables)
18. [Operators quick reference](#18-operators-quick-reference)
19. [External declarations](#19-external-declarations)
20. [Lint levels](#20-lint-levels)
21. [Language versions](#21-language-versions)
22. [Ownership and moves](#22-ownership-and-moves)

---

## 1. Lexical structure

### Comments

```
// single-line comment
/// doc comment — extracted by `trit doc`
```

There are no block comments.

### Keywords

```
let  mut  fn  pub  async  return  break  continue
if  elif  else  tif  match  for  in  while  loop
struct  enum  impl  trait  use  self
spawn  await
true  false  unknown
tand  tor  tnot  txor  tcon  tany
timp  teq  tposs  tnec
tandw  torw  txorw  timpw  tcmpw  tnotw
as
```

### Identifiers

An identifier is `[a-zA-Z_][a-zA-Z0-9_]*`. Identifiers may not shadow keywords.

### Integer bases

| Prefix | Base | Example |
|--------|------|---------|
| (none) | 10 decimal | `42`, `-7` |
| `0x` | 16 hex | `0xFF` |
| `0b` | 2 binary | `0b1010` |
| `0o` | 8 octal | `0o17` |
| `0t` | balanced ternary | `0t+0-` |

### Balanced ternary literals (`0t...`)

Each character after `0t` is a trit: `+` = +1, `0` = 0, `-` = −1. The most
significant trit comes first.

```
0t+0-   // (+1)×9 + (0)×3 + (−1)×1 = 9 + 0 − 1 = 8
0t+-    // (+1)×3 + (−1)×1 = 2
```

---

## 2. Types

### Primitive types

| Type | Description | Range |
|------|-------------|-------|
| `int` | Target's native machine word — **width differs by backend** | LLVM: −2⁶³ … 2⁶³−1 · T3ISA: −3,812,798,742,493 … +3,812,798,742,493 (27 trits) |
| `float` | 64-bit floating point | IEEE 754 double |
| `bool` | Boolean | `true`, `false` |
| `char` | Unicode scalar value | U+0000 … U+10FFFF |
| `str` | String (UTF-8) | — |
| `void` | Unit / no value | — |

> **`int` is not a fixed width, and portable code must account for that.** It
> lowers to `i64` on the LLVM backend and to a 27-trit machine word on T3ISA.
> A value between 3,812,798,742,493 and 2⁶³−1 is representable on one target and
> not the other, so a program that computes in that band will behave differently
> depending on how it was compiled.
>
> On T3ISA, arithmetic that leaves the 27-trit range **traps** — it does not
> saturate and it does not wrap. Earlier revisions clamped silently, which let a
> program produce a wrong answer and still report success; `examples/fibonacci.mt`
> documents the case that exposed it.
>
> Use `t27` when you want the ternary width explicitly on both targets, or guard
> against the narrower bound as `fib_safe` does.

### Ternary types

| Type | Trits | Range |
|------|-------|-------|
| `trit` | 1 | −1, 0, +1 |
| `tryte` | 6 | −364 … +364 |
| `t9` | 9 | −9841 … +9841 |
| `t27` | 27 | −3,812,798,742,493 … +3,812,798,742,493 |
| `t54` | 54 | −9,223,372,036,854,775,807 … +9,223,372,036,854,775,807 |
| `t<N>` | N | −(3ᴺ−1)/2 … +(3ᴺ−1)/2, for N from 1 to 54 |
| `bool3` | — | `true` (+1), `unknown` (0), `false` (−1) |

> **Correction, 3 September 2026 (report.txt P122).** Two of these rows were
> false, and the compiler has always enforced the corrected numbers.
>
> **`tryte` read "3 | −13 … +13".** The checker accepts up to ±364 and refuses
> ±365, which is (3⁶−1)/2 — six trits, not three. **The standard library
> settles which is right**: `bridge::byte_to_tryte(b: t9) -> tryte` maps a byte
> into a `tryte`, and a three-trit tryte holds ±13, which cannot hold a byte at
> all. The compiler said one thing, this table said another, and a *third*
> place — the diagnostic for a three-valued operator on a wide number — sided
> with the table and told the reader "a 3-trit number". All three now derive
> the width from one function.
>
> **`t54` read "≈ ±1.46×10²⁵".** A 54-trit range is (3⁵⁴−1)/2 ≈ 2.9×10²⁵, so
> the figure was wrong even for the width it claimed — and neither number is
> what binds: a `t54` is **i64-bounded**, which is what the type's own
> definition says and what the checker enforces (`i64::MAX` accepted,
> `i64::MAX + 1` refused). The table now states the bound that exists rather
> than the one the arithmetic would give.
>
> Both rows are pinned by
> `audit_regression_tests::documented_ternary_ranges_match_the_compiler`, which
> reads this table from disk and asks the compiler about each bound and the
> value one past it — because a table that must agree with the compiler gets a
> test rather than a proofread.

### Compound types

```
[T]          // slice / dynamic array
[T; N]       // fixed-size array of N elements
(T, U)       // tuple
fn(T, U) -> R  // function type
```

### Generic types

```
Result<T, E>           // three-state result: Ok(T), Unknown(str), Err(E)
Vec<T>                 // growable vector
Map<K, V>              // hash map
Set<T>                 // hash set
Deque<T>               // double-ended queue
TernaryTrie<V>         // trit-indexed prefix tree
Mutex<T>               // mutual exclusion wrapper
Task<T>                // async task handle
Channel<T>             // MPSC channel
AtomicTrit             // lock-free trit
Barrier                // synchronisation point
Semaphore              // counting semaphore
```

#### `str` elements and keys

A `Map<str,V>`, `Set<str>` or `Vec<str>` compares its keys and elements **by
text**, never by where the string came from. `m.get(k)` finds the entry whose
key reads the same as `k`, whether `k` is a literal, a slice, or something
concatenated a moment ago; inserting the same text twice makes one entry;
`v.sort()` on a `Vec<str>` sorts alphabetically.

This is worth stating because the obvious implementation does not do it. A
`str` reaches the runtime as a machine word — an address natively, an intern id
on T3 — so a collection that compares what it was handed compares identity
instead of text, and matches only strings that happen to have come from the
same place.

#### Iteration order

`Map` and `Set` iterate in **insertion order**, and that is a rule of the
language rather than an artifact of how a backend stores them. `Map::keys()`
and `Map::values()` return their elements in that order and are aligned with
each other, so they can be paired by index. `Set::for_each` walks it too.

Re-inserting a key that is already present updates its value and leaves its
position alone — the insertion has already happened. Removing a key takes it
out of the sequence and leaves the rest in order.

Set algebra takes its order from its operands: `intersection` and `difference`
keep the receiver's order, and `union` is the receiver's order followed by
whatever the argument adds, in the argument's order.

The rule exists because without it the same program prints different things on
the two backends — the native runtime stores a hash table and the T3 emulator
an ordered map, and walking either one's own storage gives its own sequence.
Insertion order is also the only order the two can agree on without knowing the
key type, since keys reach the runtime type-erased.

### Type inference

Use `_` as a type annotation to let the compiler infer it:

```
let x: _ = 42;   // inferred as int
```

---

## 3. Literals

### Integer and float

```
42          // int
-7          // int (unary minus applied)
3.14        // float
2.0e10      // float with exponent
0xFF        // hex int = 255
0b1010      // binary int = 10
0o17        // octal int = 15
0t+0-       // balanced ternary int = 8
```

### String and char

```
"hello, world"       // str
'a'                  // char
'\n'  '\t'  '\\'  '\''   // escape sequences
```

### Boolean

```
true    false
```

### Three-valued boolean (`bool3`)

```
true     // +1  — definitely true
unknown  // 0   — indeterminate
false    // -1  — definitely false
```

### Trit

```
+    // +1
0    // 0  (the integer zero serves as trit zero)
-    // -1 (the minus sign alone as a trit literal)
```

In pattern matching, use `+`, `0`, `-` as trit patterns.

---

## 4. Variables and bindings

### Local variables

```
let x: int = 42;          // immutable binding
let mut y: float = 3.14;  // mutable binding
let z = "hello";          // type inferred
let mut v: Vec<int> = Vec::new();
```

The type annotation after `:` is optional when the type can be inferred from the
initialiser. The initialiser (`= expr`) is optional if the variable type is given
explicitly (the variable is left uninitialised until first assignment).

### Assignment

```
y = 2.71;       // plain assignment (y must be mut)
y += 1.0;       // compound: +=  -=  *=  /=
```

### Global variables

Declared at the top level outside any function:

```
pub let MAX_SIZE: int = 1024;
let mut COUNTER: int = 0;
```

### Binding a value may consume it

Binding one variable to another **moves** the value when its type is not
`Copy`, and the original binding cannot be read afterwards:

```manit
let s: str = "hello";
let t: str = s;
io::println(s);       // error: use of moved value: 's'
```

This is the one rule in ManiT that most often surprises a reader arriving from
another language, in **both directions** — passing a value to a function does
*not* move it. See [§22 Ownership and moves](#22-ownership-and-moves) for the
complete rule and the list of `Copy` types.

---

## 5. Expressions

### Arithmetic

```
a + b    a - b    a * b    a / b    a % b
-a                // unary negation
```

**Integer division depends on the language version** (§21). Under `v1`, the
default, `/` truncates toward zero and `%` takes the sign of the dividend.
Under `--lang v2`, `/` rounds to the nearest integer with ties away from zero
and `%` is the balanced remainder that pairs with it:

```
             v1              v2
  7 / 2       3               4
  7 % 2       1              -1
 -7 / 2      -3              -4
  2 / 3       0               1
```

`(a / b) * b + (a % b) == a` holds under both. `math::div_trunc`,
`math::rem_trunc`, `math::div_near` and `math::rem_near` name the two
behaviours explicitly and mean the same thing under both versions.

Division by zero **traps** — a named runtime fault and exit status 70 on both
backends, not a wrong answer. A division by a literal `0` is rejected at
compile time instead, since it cannot be intentional. (Earlier revisions of
this document said it was undefined and returned 0; that stopped being true
with the A7 fault-reporting work.)

### Comparison

```
a == b    a != b
a < b    a > b    a <= b    a >= b
```

Returns `bool`.

### Logical

```
a && b    a || b    !a
```

Short-circuit evaluation. Operands must be `bool`.

### Ternary logic operators

These implement Łukasiewicz three-valued logic on `trit` and `bool3` values:

| Op | Meaning | Formula |
|----|---------|---------|
| `a tand b` | conjunction | `min(a, b)` |
| `a tor b` | disjunction | `max(a, b)` |
| `tnot a` | negation | `-a` |
| `a txor b` | exclusive or | `(a + b) mod 3`, balanced — sum without carry |

```
let a: trit = +;
let b: trit = -;
let c = a tand b;   // -1 (false)
let d = a tor b;    // +1 (true)
let e = tnot a;     // -1
let f = a txor b;   //  0 (+1 + -1 = 0)
```

`txor` is the balanced-ternary sum digit — the result of adding two trits and
discarding the carry. A residue of `2` is written `-` and a residue of `-2` is
written `+`, which is what keeps the digit set balanced.

|  | `b = -` | `b = 0` | `b = +` |
|---|---|---|---|
| **`a = -`** | `+` | `-` | `0` |
| **`a = 0`** | `-` | `0` | `+` |
| **`a = +`** | `0` | `+` | `-` |

Two properties follow, and both differ from binary XOR:

* **`txor` is not self-inverse.** `x txor k txor k` is not `x`; you need
  **three** applications, because `3k ≡ 0 (mod 3)`. Binary XOR undoes itself
  after two only because `2 ≡ 0 (mod 2)` — that is an accident of base 2, not
  a property of exclusive-or.
* **For a fixed `k` it is a bijection**, so `x txor k` can be undone and is
  usable as a keying primitive. It is also surjective onto all three trits.

> Before 19 August 2026 this operator computed `|a - b|` clamped to
> `{-1, 0, +1}`. That is a difference detector, not a ternary XOR: it can never
> return `-`, so a third of the digit set was unreachable, and it is not a
> bijection — for any fixed `b`, two of the three inputs map to `+` — so it
> could not be undone at all.

#### Consensus and any

| Op | Meaning | Result |
|----|---------|--------|
| `a tcon b` | consensus | `+` only if both are `+`; `-` only if both are `-`; otherwise `0` |
| `a tany b` | any | `+` if either is `+`; else `-` if either is `-`; else `0` |

|  | `b = -` | `b = 0` | `b = +` |
|---|---|---|---|
| **`a = -`** `tcon` | `-` | `0` | `0` |
| **`a = 0`** `tcon` | `0` | `0` | `0` |
| **`a = +`** `tcon` | `0` | `0` | `+` |
| **`a = -`** `tany` | `-` | `-` | `+` |
| **`a = 0`** `tany` | `-` | `0` | `+` |
| **`a = +`** `tany` | `+` | `+` | `+` |

Note `tany` is not symmetric with `tcon` under negation: `+` wins over `-`
wherever both appear, so `- tany +` is `+`.

#### Implication, equivalence, and the modal operators

These are what make the language's logic **Łukasiewicz L3** rather than
Kleene's K3. The two systems agree exactly on `tand`, `tor` and `tnot`; they
differ in one cell of implication.

| Op | Meaning | Formula | Result type |
|----|---------|---------|-------------|
| `a timp b` | implication | `min(+1, 1 - a + b)` | `trit` |
| `a teq b` | equivalence | `(a timp b) tand (b timp a)` | `trit` |
| `tposs a` | possibility (M) | `+` if `a >= 0` | `bool` |
| `tnec a` | necessity (L) | `+` only if `a = +` | `bool` |

|  | `b = -` | `b = 0` | `b = +` |
|---|---|---|---|
| **`a = -`** `timp` | `+` | `+` | `+` |
| **`a = 0`** `timp` | `0` | **`+`** | `+` |
| **`a = +`** `timp` | `-` | `0` | `+` |

The bolded cell is the whole difference. Kleene's `max(-a, b)` gives `0`
there; Łukasiewicz gives `+`. That single cell is the **deduction theorem**:

```
let u: trit = 0;
let t = u timp u;   // + — a tautology, even though u is unknown
```

In K3 that expression is `0` and `a timp a` is not a tautology. With only
`tand`, `tor` and `tnot` the language could not even express the question.

`tposs` and `tnec` are the bridge back out of three-valued logic: both return
`bool`, not `trit`, because "might this be true?" and "is this definitely
true?" have no unknown answer. They are duals — `tnec a` equals
`tnot tposs tnot a` — and either can be used directly in an `if`.

```
let s: trit = 0;         // a sensor we have not heard from
if tposs s { /* not ruled out */ }
if tnec s  { /* confirmed — not taken here */ }
```

### Lane-wise ternary logic

Everything above operates on ONE three-valued value. The lane-wise operators
apply the same connectives to all **27 trits of a word at once**, in a single
T3 instruction.

This is the one place the language is doing something a binary language cannot
copy cheaply. A 64-bit word holds 64 lanes of *half* a datum each — a bit
cannot carry a three-valued answer — while a 27-trit word holds 27 lanes of a
genuinely three-valued one.

| Op | Meaning | Per lane `i` |
|----|---------|--------------|
| `a tandw b` | lane-wise conjunction | `min(a_i, b_i)` |
| `a torw b` | lane-wise disjunction | `max(a_i, b_i)` |
| `a txorw b` | lane-wise sum mod 3 | balanced `(a_i + b_i) mod 3` |
| `a timpw b` | lane-wise implication | `min(+1, 1 - a_i + b_i)` |
| `a tcmpw b` | lane-wise compare | `sign(a_i - b_i)` |
| `tnotw a` | lane-wise negation | `-a_i` |

Lane `i` is the balanced-ternary digit `d_i` in `a = Σ d_i · 3^i`. The
decomposition is unique, so lanes are well defined without reference to any
storage layout.

```
let x: int = 5;
let y: int = -7;
let a = x tandw y;    // -13
let b = x torw y;     //  11
let c = tnotw x;      //  -5
let d = 121 tandw 40; //  40
```

**These take words, not trits.** Operands must be integer types (`int`, or any
ternary width — `tryte`, `t9`, `t27`, `t54` and every other `t<N>`); `float`,
`tfloat`, `bool` and `bool3` are
rejected. A `bool` is one three-valued answer, so asking for 27 lanes of it is
far more likely to be a typo for `tand` than an intention:

```
let p: bool = true;
let q = p tandw false;
// error: invalid operands: operator `tandw` cannot be applied to `bool` and `bool`
```

The result keeps the operand's type, and the value is always in range: every
lane result is in `{-1, 0, +1}`, so the reassembled word cannot overflow.

**`tnotw` is `tnot` done 27 times, and costs nothing extra.** Negating a
balanced-ternary number flips the sign of every trit in it, so lane-wise NOT is
already ordinary negation. It compiles to `TNEG` — the instruction T3ISA has
had since v1.0 — and the ISA deliberately does *not* define a `TNOTW`.

**`txorw` inherits `txor`'s surprise.** It is not self-inverse: recovering the
original word takes **three** applications of the same key, not two.

```
let k: int = 121;
let once   = 5 txorw k;                 //   99
let twice  = 5 txorw k txorw k;         // -104  — not 5
let thrice = 5 txorw k txorw k txorw k; //    5  — recovered
```

**`timpw` is Łukasiewicz per lane**, so the deduction theorem holds 27 lanes at
a time. `a timpw a` is the all-`+` word — `3812798742493` — for *every* `a`,
including words with zero lanes, where Kleene's rule would leave `0`:

```
let t = 0 timpw 0;    // 3812798742493 — every lane +
let u = 9841 timpw 9841;  // 3812798742493
```

That is the cheapest way to check a backend implements L3 and not K3.

> **Cost.** On the T3 backend each of these is one instruction. Written out by
> hand — extract lane, operate, re-insert, 27 times — the same operation
> measures 3,034 instructions against 2. See §5 of the T3ISA reference, which
> reports the measurement and its method. On the LLVM backend they become calls
> into the C runtime (`manit_lane_*`), because a 27-lane balanced-ternary loop
> is not something binary hardware expresses inline; results are identical on
> both backends.

### Trit intrinsics (`trit::`)

Operations balanced ternary does cheaply and binary does not, named so you can
reach for them. `use std::trit;`

| Function | Meaning |
|----------|---------|
| `trit::sign(x)` | `-1`, `0` or `+1` — the sign of `x`, as a `trit` |
| `trit::abs(x)` | absolute value, exact for every input |
| `trit::count(x, k)` | how many of the 27 lanes of `x` equal the trit `k` |
| `trit::shift3(x, n)` | `x * 3^n` — the machine's native shift |
| `trit::leading_zeros(x)` | leading zero-trits, out of 27 |
| `trit::trailing_zeros(x)` | trailing zero-trits, out of 27 |

```
use std::trit;

let s = trit::sign(-256);        // -1
let a = trit::abs(-256);         // 256
let c = trit::count(9841, +);    // 9  — 9841 is nine +1 lanes
let t = trit::shift3(7, 5);      // 1701 = 7 * 3^5
let z = trit::leading_zeros(9);  // 24
```

**`sign` is the one to notice.** In two's complement it is a branch or a
shift-and-or. Here it is a single instruction: T3ISA's `R0` always reads as
zero, so `TCMP Rd, Ra, R0` *is* the sign. Measured against the same function
written out by hand — `if x > 0 { 1 } elif x < 0 { -1 } else { 0 }` — it costs
**2 instructions per call against 11.5**, a 5.8× difference, and it never
branches.

**`abs` needs no special case.** The 27-trit range is symmetric
(±3,812,798,742,493), so there is no value whose negation overflows. Two's
complement has `abs(INT_MIN)` as undefined behaviour or a wrap to itself; that
question does not arise here. It compiles to `sign` and a multiply.

**`trit::count(x, k)` is not `math::trit_count(x)`.** The `math` one is the
trit *length* of `x` — how many digits it occupies. This one counts lanes
*equal to* `k`, and the three counts always sum to 27. They are different
questions with the same obvious name, which is why the new family has its own
namespace rather than overloading the old one.

```
let n = math::trit_count(9841);   // 9  — 9841 occupies nine trits
let p = trit::count(9841, +);     // 9  — nine of its 27 lanes are +1
let z = trit::count(9841, 0);     // 18 — the other eighteen are 0
```

**`shift3` is the ternary shift.** `x << n` is the binary one and multiplies by
`2^n`; `trit::shift3(x, n)` multiplies by `3^n` and is one instruction (`TSHI`).

> `sign`, `abs`, `count` and `shift3` are lowered to IR directly, so both
> backends get them from one definition. `leading_zeros` and `trailing_zeros`
> are ordinary ManiT — they are not single instructions and there is nothing to
> gain by pretending otherwise.

### Bitwise

```
a & b    a | b    a ^ b    a << n    a >> n
```

### Range

```
0..10     // exclusive range [0, 10)
0..=10    // inclusive range [0, 10]
```

Used as iterable in `for` loops.

### Type cast

```
let f = 3.14;
let i = f as int;    // 3      — saturating: NaN is 0, out of range is the bound
let t = i as trit;   // clamps to {−1, 0, +1}
let c = 300 as char; // clamps to 0..=255, so 255
```

**`as` CLAMPS AT A BOUNDARY; IT DOES NOT WRAP.** All three narrowing casts
saturate to the nearest representable value rather than truncating bits, which
is the rule to predict from when a case is not listed here. `float as int` uses
`llvm.fptosi.sat` on LLVM and Rust's `as` on T3 so the two agree by
construction (report.txt P23).

> **`as char` clamped on neither backend before 29 August 2026**, and they were
> wrong in different directions: T3 did not narrow at all — `300 as char` stayed
> **300**, a value outside the type — while LLVM, where a `char` was an `i8`,
> truncated to **44** and made `255 as char as int` come back **−1**
> (report.txt P48).

A `char` is an **unsigned byte, 0..=255**, so `str::char_at(s, i) as int` is
195 for the first byte of `é` and never a negative number.

> **It was 195 on T3 and −61 on LLVM until 29 August 2026**, and so was every
> ORDERING between characters: `c > 'a'` answered differently on the two
> backends for any byte ≥ 128, which reached every `str::` function that
> compares characters. ASCII agreed throughout, which is why no corpus caught
> it (report.txt P48).

### Question operator

```
let result: Result<int, str> = some_fn()?;
```

On `Err`, propagates the error out of the enclosing function.
On `Unknown`, returns `Unknown` from the enclosing function.
On `Ok(v)`, evaluates to `v`.

### Field access

```
let p = Point { x: 1, y: 2 };
let x = p.x;
```

### Method calls

```
let s = p.to_str();
let len = v.len();
```

### Indexing

```
let arr: [int; 5] = [1, 2, 3, 4, 5];
let x = arr[2];       // 3
```

### Struct literal

```
let p = Point { x: 1, y: 2 };
```

Fields must all be provided. The field order in the literal is irrelevant.

### Array literal

```
let a: [int; 3] = [10, 20, 30];
```

### Tuple literal

```
let t = (1, "hello", 3.14);
let (n, s, f) = t;   // destructuring
```

### Lambda

```
let double = fn(x: int) => x * 2;
let result = double(21);   // 42
```

**A lambda cannot capture.** It is an anonymous function, not a closure:
referring to any variable from the enclosing scope is a compile error, and the
compiler says so directly.

```
let k: int = 3;
let f = fn(x: int) => x * k;
// error: lambda captures outer variable 'k' — closures are not yet
//        supported; use a parameter instead
```

> **This heading read "Lambda / closure" until 26 August 2026.** Nothing in the
> prose promised capture and the single example captured nothing, but the word
> was there for a construct that cannot close over anything, and the one
> example a reader had to generalise from could not tell them otherwise. Pass
> what you need as a parameter.

**Binding an EXISTING function to a variable needs a type annotation.**
`let f: fn(int) -> int = dbl;` works; `let f = dbl;` compiles to a reference to
a symbol named after the *binding* and fails to assemble or link (report.txt
P53). A lambda is unaffected, because it is emitted under the name it is bound
to. Function-typed *parameters* are unaffected too, which is why the stdlib's
higher-order surface — `Vec::map`, `Vec::filter`, `Vec::fold` — is fine.

### Spawn

```
spawn { some_work(); }
```

**Runs the block, in place, to completion.** It is sequential: the statement
after the `spawn` does not begin until the block has finished.

> **This description replaced a false one.** Until 24 August 2026 this section
> read "Creates a cooperative task. Returns `Task<T>` where `T` is the block's
> type." None of that was true of either backend: the lowering inlines the
> block, and the value form `let t = spawn { 42 };` binds 0 on T3 and emits
> invalid LLVM IR that clang rejects. `await` on the result does not
> type-check.
>
> ManiT has no concurrency today. `docs/memory-model.md` is the normative
> statement of what the concurrency primitives do and do not guarantee, and
> report.txt P5 records the defects. Do not write code against the old
> description.

> **Corrected again 2 September 2026 — the notice above is now itself stale,
> and it is kept because the correction it records is the record.** ManiT
> *does* have concurrency: `--sched cooperative` implements
> `docs/semantics.md` §11 on both backends, and §11.12 makes `spawn { B }` an
> expression of type `Task<T>` in **both** scheduling modes.
>
> So the 24 August sentence "Returns `Task<T>` where `T` is the block's type"
> — deleted then as false — is **true now**, which is why the sentence that
> deleted it could not simply be deleted in turn. What remains true of the
> paragraph above it is the DEFAULT: under `--sched inline`, still the default,
> the block runs in place to completion and the statement after it does not
> begin until it has finished. §11.12's first decision is what makes the handle
> work there too — a task that finished long ago is the ordinary case, so
> `await` on it returns immediately.

### Await

```
let value = await task;
```

Yields control until the task completes, and returns the task's result.
`await` is a **prefix** operator and binds like the other unary forms, so
`await t + 1` awaits `t` and then adds. The postfix `t.await` also parses and
is what `examples/concurrency.mt` uses on an `async fn` result.

**Awaiting the same handle twice is a trap**, not a second copy of the value
(`docs/semantics.md` §11.12): a program that does it has almost certainly
confused two handles. A handle may outlive its task, and a value nobody awaits
is discarded.

---

## 6. Statements

A statement is one of:

- A `let` binding
- An assignment (`lhs = rhs` or compound `+=` etc.)
- An expression statement (expression followed by `;`)
- A `return` / `break` / `continue`
- A local struct definition

Blocks (`{ ... }`) are expressions that evaluate to the value of their last
expression (if it lacks a trailing `;`) or `void` otherwise.

---

## 7. Control flow

### if / elif / else

```
if condition {
    ...
} elif other_condition {
    ...
} else {
    ...
}
```

Conditions must be `bool`. `elif` and `else` are optional.

`if` is an expression — all branches must produce the same type:

```
let label = if score > 90 { "A" } else { "B" };
```

### tif — ternary three-way branch

Branches on a `trit` or `bool3` value into exactly three arms:

```
tif sensor {
    + => io::println("OK"),
    0 => io::println("Unknown"),
    - => io::println("Fault"),
}
```

The arms `+`, `0`, `-` are mandatory. `tif` is an expression; all arms must
produce the same type.

### match

```
match expr {
    Pattern => expr,
    Pattern if guard => expr,
    _ => expr,
}
```

Arms are tried in order. The first matching arm wins.
A `match` is an expression; all arms must produce the same type.

See [Pattern matching](#13-pattern-matching) for pattern syntax.

### for

```
for i in 0..10 {
    io::println(i);
}

for item in collection {
    ...
}
```

`for` iterates over ranges, arrays, vectors, and other iterables.

### while

```
while condition {
    ...
}
```

Condition must be `bool`.

### loop

```
loop {
    if done { break; }
}
```

Infinite loop broken by `break`.

### break / continue

`break` exits the enclosing `for`, `while`, or `loop`.
`continue` jumps to the next iteration.

### return

```
return expr;
return;    // returns void
```

Returns from the enclosing function.

---

## 8. Functions

### Definition

```
fn add(a: int, b: int) -> int {
    a + b
}
```

- The return type after `->` is optional; it defaults to `void`.
- The function body is a block. The block's final expression (without `;`) is the
  return value.
- An explicit `return` may appear anywhere in the body.

### Visibility

```
pub fn my_function() { ... }   // visible externally
fn helper() { ... }             // module-private
```

### Async functions

```
async fn fetch_data() -> str {
    ...
}
```

An `async fn` returns a `Task<T>` and may contain `await` expressions.

### Generic functions

```
fn identity<T>(x: T) -> T {
    x
}
```

Type parameters are declared in `<...>` after the function name. They bind
arbitrary types at each call site. (The compiler currently represents unconstrained
type params as `ManiType::Unknown` at runtime.)

### External / stdlib functions

Functions declared without a body are treated as extern imports:

```
fn io::println(s: str);
```

These are typically declared implicitly by `use` statements.

An `extern` declaration gives one a real signature, and can say which backends
provide it:

```
extern "c" fn gui::set_color(r: int, g: int, b: int) -> void
    available(llvm) deprecated("use gfx::color");
```

`available(...)` lists the backends that have an implementation. Omitting it
means *unstated*, which is not the same as "available nowhere".

### Backend availability

ManiT infers, for every function, which backends it can run on: **a function is
available on backend B exactly when every function it calls is.** Compiling for
a backend that something in the reachable call graph cannot reach is an error,
and the error names the chain:

```
extern "c" fn gui::set_color(r: int, g: int, b: int) -> void available(llvm);

fn paint()      { gui::set_color(1, 2, 3); }
fn draw_frame() { paint(); }
fn main()       { draw_frame(); }
```

```
$ manitc compile --target t3 demo.mt
error: demo.mt:8:19: 'main' cannot be compiled for the t3 backend:
       main -> draw_frame -> paint -> gui::set_color
       — and 'gui::set_color' is declared available only on: llvm
```

The same program compiles for `llvm` without complaint. Availability is a
static property of the call graph, so this is decided at compile time rather
than discovered by running the program on both backends and diffing the output.
Without it, the failure above surfaces as `Undefined label: gui::set_color`
from the assembler, with no source location and no indication of which function
is responsible.

Only the **outermost** affected function is reported. One unavailable extern
makes everything above it unavailable too, and its chain already names every
hop including the culprit, so reporting each link would be the same fact N
times.

`manitc check` selects no backend and so reports nothing here — it cannot
answer a question that was not asked.

Recursion needs no special handling: mutually recursive functions settle at the
meet over their cycle, so a function that never names an unavailable symbol is
still reported if its cycle reaches one.

#### Writing it down

Availability is inferred, not declared — writing it on every function would be
unbearable. But it can be written on the functions where it matters, and then
it is an **assertion the compiler checks**:

```
fn render() available(llvm, t3) { gui::flush(); }
// error: 'render' declares `available(t3)` but cannot run there:
//        render -> gui::flush — and 'gui::flush' is declared available only on: llvm
```

This is the relationship Rust has between inferred lifetimes and written ones.
A written clause also constrains callers, so `fn f() available(llvm)` makes
everything that calls `f` llvm-only too — with no extern involved anywhere.

A contradicted assertion is reported whatever backend is selected, and by
`manitc check` as well, because it is a statement about the program rather than
about one invocation.

> This is the `backend-unavailable-chain` lint, and it denies by default. Turn
> it off with `-A backend-unavailable-chain` if you know better — but the build
> will still fail, further down, with a worse message.

---

## 9. Structs

### Definition

```
struct Point {
    pub x: int,
    pub y: int,
}
```

Fields are `pub` or private. Generics are supported:

```
struct Pair<T> {
    first: T,
    second: T,
}
```

### Instantiation

```
let p = Point { x: 3, y: 4 };
```

### Field access

```
let x = p.x;
p.x = 10;   // only if p is mut
```

---

## 10. Enums

### Definition

```
enum Direction {
    North,
    South,
    East,
    West,
}
```

Variants may carry data:

```
enum Shape {
    Circle(float),        // one field: radius
    Rectangle(int, int),  // two fields: width, height
}
```

### Variant expressions

```
let d: Direction = Direction::North;
let s: Shape = Shape::Circle(5.0);
```

Variant expressions produce a value of the enum's type. Plain (no-payload)
variants are represented as integer indices (0, 1, 2, …) internally.

### Matching enums

```
match d {
    Direction::North => "N",
    Direction::South => "S",
    Direction::East  => "E",
    Direction::West  => "W",
}
```

Payload variants bind their fields to pattern variables:

```
match s {
    Shape::Circle(r) => r * r * 3.14,
    Shape::Rectangle(w, h) => (w * h) as float,
}
```

---

## 11. Impl blocks and methods

### Basic impl

```
impl Point {
    fn length(self) -> float {
        math::sqrt((self.x * self.x + self.y * self.y) as float)
    }

    fn add(self, other: Point) -> Point {
        Point { x: self.x + other.x, y: self.y + other.y }
    }
}
```

- `self` as a parameter refers to the receiver. Its type is inferred as the impl
  type — no explicit annotation needed.
- Methods are called as `p.length()` and `p.add(q)`.
- Inside the method body, `Self` refers to the impl type.

### Trait impl

```
impl Describable for Point {
    fn describe(self) -> str {
        fmt::format("Point({}, {})", self.x, self.y)
    }
}
```

---

## 12. Traits

### Definition

```
trait Describable {
    fn describe(self) -> str;    // signature only — no body
}
```

### Implementation

```
impl Describable for Rectangle {
    fn describe(self) -> str {
        fmt::format("Rectangle({}x{})", self.width, self.height)
    }
}
```

The compiler verifies that every method in the trait is provided by the impl.

### Using trait methods

Trait methods are called with ordinary method-call syntax. The compiler resolves
the correct impl at the call site based on the receiver's type.

---

## 13. Pattern matching

Patterns appear in `match` arms and `let` destructuring.

### Wildcard

```
_ => "catch-all"
```

### Literal patterns

```
42     // integer
3.14   // float
"hi"   // string
true   // bool
false
+      // trit +1
0      // trit 0
-      // trit -1
-5     // negative integer
```

### Identifier pattern

Binds the matched value to a name:

```
match x {
    n => io::println(n),   // n is bound to x
}
```

### Struct pattern

```
match p {
    Point { x: 0, y: 0 } => "origin",
    Point { x: px, y: py } => "other",
}
```

### Tuple pattern

```
match pair {
    (0, 0)  => "zero",
    (a, b)  => a + b,
}
```

### Enum patterns

```
match direction {
    Direction::North => "N",
    Direction::South => "S",
}

match shape {
    Shape::Circle(r) => r * 3.14,
    Shape::Rectangle(w, h) => w * h as float,
}
```

Path syntax `Enum::Variant` is required for enum patterns.

### Or patterns

```
match x {
    1 | 2 | 3 => "small",
    _ => "other",
}
```

### Trit patterns

*Added 3 September 2026 (C6).*

A **trit pattern** matches the individual trits of a balanced-ternary word. It
is written like the `0t` literal — `0t` and then the trits, **high trit first**
— with two wildcards and an optional capture name added:

| | |
|---|---|
| `+` `0` `-` | a trit that must have this value |
| `?` | one trit of any value |
| `*` | any number of trits, **leftmost position only** |
| `@name` | binds the wildcard run just written |

```
fn classify(x: int) -> str {
    match x {
        0t++?? => "high pair set",
        0t--?? => "high pair clear",
        0t?0?? => "third trit zero",
        _      => "other",
    }
}
```

The scrutinee must be a balanced-ternary integer — `int`, `trit`, `tryte`,
`t9`, `t27` or `t54`. `bool3` is not one: it is a truth value rather than a
number, and `docs/semantics.md` §10.2 records that the two are not
interchangeable.

#### The trits above the pattern must be zero

A trit pattern is **anchored at the low end**, and every trit above the ones it
names is required to be zero unless the pattern opens with `*`. That is what
makes a wildcard-free trit pattern mean exactly the literal it spells:

```
match x {
    0t++0 => "…",    // matches 12, and nothing else
}
```

This needs no width and no sign-extension rule, **because balanced ternary
needs neither**. In two's complement the pattern for a small negative number
would have to say how many leading `1` bits to expect, and the answer would
depend on the word. In balanced ternary `-1` is `-` with *zeros* above it, so
`0t-` matches `-1` whether the word is 27 trits or 64 bits. The representation
is unique, and the rule falls out of it.

#### `*` may only be leftmost, and that is a portability rule

```
0t*++??      // fine — the trits above are unconstrained
0t+*??       // error
```

A `*` in the middle could only be placed by knowing how many trits the
scrutinee has, and under `--lang v1` that number is not the same on the two
backends: `docs/semantics.md` §10.1 records `int` as a 27-trit word on T3 and
64 bits on LLVM. A pattern whose meaning depended on it would mean different
things in the two places. Leftmost-only `*` needs no width at all, so **a trit
pattern means the same thing on every backend and under both language
versions.**

#### Captures

`@name` binds the wildcard run *immediately before* it, as an `int`:

```
match packed {
    0t++??@lo   => io::println_int(lo),    // the low two trits, -4 ..= 4
    0t*@hi+00+  => io::println_int(hi),    // everything above the low four
    _           => {}
}
```

Note the order. Rust writes `name @ pattern`; this writes `pattern@name`, and
the reason is lexical rather than aesthetic: a letter can begin an operand, so
the run `0t+lo@???` would end at `0t` and fail to lex. `@` cannot begin an
operand, so the postfix form is unambiguous.

A capture is an `int` whatever its width. Thirty-nine is the ceiling because the compilation needs 3^width as a
machine word, and 3^40 is not one — so a trit pattern cannot span the whole of
a `t54`.

> **Updated 3 September 2026 (C3).** This paragraph read: "only three widths
> have names (`tryte`, `t9`, `t27`), so typing a capture by its width wants the
> width-polymorphic `t<N>` of Phase 5's C3, **which does not exist yet**." It
> exists now (§24), so a capture of any run length has a type to be given: a
> 4-trit run is a `t<4>`. The capture itself still *binds* `int` — narrowing it
> to the run's own width is a separate change with its own measurement owed to
> it, and this notice records the gap rather than closing it quietly.

#### A three-way match is a three-way branch

Three arms that fix the **same single trit position** to `+`, `0` and `-`, and
constrain nothing else, cover every value of the word. The compiler knows it,
so no `_` arm is needed — and it compiles the match to one three-way branch
rather than to three equality tests:

```
fn name(t: trit) -> str {
    match t {
        0t*+ => "pos",
        0t*0 => "zero",
        0t*- => "neg",
    }
}
```

On T3 that is a `TSHR`, a `TSHI`, a `TSUB` and a single `TBRANCH`. Written as
a chain of comparisons the same program is three equality tests of eight
instructions each, because an equality has to be reduced to a boolean before a
two-way branch can use it. The extracted trit is already in `{-1, 0, +1}`,
which is exactly what `TBRANCH` consumes.

The leading `*` is doing real work here. Without it each arm *also* demands
that every trit above position 0 is zero, so `0t+ | 0t0 | 0t-` covers only
`-1`, `0` and `+1` — exhaustive over a `trit`, and not over an `int`. The
compiler accepts the three-way form only with the `*`.

#### A match that matches nothing traps

*Corrected 3 September 2026 (P113).* Because a wildcard-free trit pattern is
narrow, it is easy to write a `match` on an `int` that no arm accepts. When
that happens the program **traps**:

```
TRAP: unreachable code reached — commonly a `match` with no arm for this value
```

with exit status 70, identically on both backends. Before this date it did not:
T3 emitted a bare halt, so the program stopped with status 0 and simply
produced no further output, and LLVM emitted an `unreachable`, which is
undefined behaviour and in practice read a garbage value and carried on. Both
were reachable from ordinary literal patterns too, not only from trit patterns.

### Guards

```
match x {
    n if n > 100 => "big",
    n => "small",
}
```

### Result patterns

`Result<T, E>` carries three variants:

```
match result {
    Ok(v) => v,
    Unknown(msg) => { io::println(msg); 0 },
    Err(e) => { io::println(e); 0 },
}
```

### Result methods

| Method | Returns | Notes |
|---|---|---|
| `r.tag()` | `trit` | `+` Ok, `0` Unknown, `-` Err |
| `r.is_ok()` | `bool` | |
| `r.is_unknown()` | `bool` | |
| `r.is_err()` | `bool` | |
| `r.unwrap()` | `T` | **faults** unless the tag is Ok |
| `r.unwrap_or(d)` | `T` | `d` is evaluated either way |

`tag()` is the one to reach for first. The tag is a trit, so it feeds `tif`
directly and takes all three outcomes in one dispatch:

```
tif r.tag() {
    + => io::println("ok"),
    0 => io::println("unknown"),
    - => io::println("failed"),
}
```

`is_ok` / `is_unknown` / `is_err` are that same question asked three times, one
yes-or-no at a time. They are there for convenience; they are not the shape of
the type.

`unwrap` names one of three outcomes, so the other two fault — the program
stops with `TRAP: unwrap on a Result that is Err` (or `… that is Unknown`) and
exit status 70. Use it where a non-Ok really is a bug, and `match`, `tif` or
`unwrap_or` everywhere else.

### There is no `Option<T>`

`Result` is this language's option type. Where another language writes
`Option<T>` with `Some`/`None`, ManiT writes `Result<T, str>` and uses
`Unknown(msg)` for the absent case — which carries a reason, and leaves `Err`
free to mean something different from "not there".

A two-state option beside a three-state result would be a distinction the
hardware does not make. `Option<int>`, `Some(x)` and `None` are compile errors,
and the message names the replacement.

---

## 14. Generics

### Generic functions

```
fn max<T>(a: T, b: T) -> T {
    if a > b { a } else { b }
}
```

Call as `max(3, 7)` — the type parameter is inferred.

**A generic function is compiled once per distinct combination of concrete
argument types.** `max(3, 7)` and `max(1.5, 2.5)` in the same program produce
two separate copies, each compiled with the real type, so the first compares
integers and the second compares floats. Nothing about this is visible in the
source; it matters because a single shared copy could not do both.

> **Before 26 August 2026 there was only one copy, compiled with the type
> parameter erased to a machine word**, and the results for anything but `int`
> and `trit` were wrong rather than approximate: `max(1.5, 2.5)` truncated its
> arguments to 1 and 2, compared those, and returned an integer that the
> printer then read as a float bit pattern — `0.000…001`, a denormal.
> `max(-1.5, -2.5)` printed `NaN`. `int` and `trit` were correct because their
> representation *is* the machine word (report.txt P65).

**A method in an `impl<T>` block is instantiated the same way**, from the
receiver's type arguments rather than from the call's arguments — which is what
makes it work for `fn bigger(self) -> T`, where `T` appears in no argument
position at all.

> **Until 27 August 2026 it was not**, and the notice above described a generic
> METHOD exactly: `Box2 { a: -1.5, b: -2.5 }.bigger()` answered `-2.5`, because
> the erased body compared the two floats as integer bit patterns and negative
> doubles order the opposite way from theirs. Positive floats came out right,
> which is why it survived a session behind a test that used them
> (report.txt P69).

One limit remains:

* **A generic body that does not type-check for the concrete type falls back**
  to the older, erased compilation rather than reporting. So a `<T>` with no
  bound continues to accept whatever it accepted before — bounds remain opt-in
  — and the price is that the error is not reported either.

  **The fallback still knows what the function RETURNS.** `-> T` is part of the
  declaration, so under `T = P` the call has type `P` whether or not the body
  compiled at `P`, and `let q = p.bigger(); q.x` reads the field asked for.

  > **Until 29 August 2026 it did not**, and the wrong field was returned
  > silently on both backends: the call's type stayed unknown, a field lookup
  > on an unknown type matched no struct, and every read took slot 0 — so
  > `q.x` and `q.y` both answered `q`'s first field. It applied to generic free
  > functions as well as to methods (report.txt P71).

  **What the fallback does NOT recover is the VALUE, when the type's
  representation is not a machine word.** For a struct that costs nothing —
  the erased form is an address, which is what a struct already is. For a
  `float` it is P65's denormal again, because the discarded body computed with
  integer semantics; naming the return type is necessary there and not
  sufficient (report.txt P71).

### Trait bounds

A bare `<T>` places no requirement on `T`. The function above compares two
values of it, so calling it with a type that has no ordering is meaningless —
and it used to compile clean and return the wrong answer. Constrain the
parameter instead:

```
fn max<T: Ord>(a: T, b: T) -> T {
    if a > b { a } else { b }
}
```

Several bounds on one parameter are joined with `+`:

```
fn show_max<T: Ord + Display>(a: T, b: T) -> str { ... }
```

A `where` clause says the same thing after the signature, which reads better
when the list is long:

```
fn show_all<T>(v: Vec<T>) -> str where T: Display { ... }
```

The two forms accumulate rather than compete: `fn f<T: Ord>(..) where T: Display`
constrains `T` by both.

A bound is satisfied if the concrete type has an `impl` of that trait, or if it
is a primitive and the trait is one of the structural traits every primitive
has: `Eq`, `PartialEq`, `Display`, `Debug`, `Clone`, `Copy`, `Hash`. Structs
and enums are not covered by that rule — write the `impl`.

**`Ord` and `PartialOrd` are decided differently, and a user `impl` does not
enter into it.** They are satisfied by exactly the types `<` and `>` accept:
`int`, `float`, `trit`, every ternary width `t<N>` (`tryte`, `t9`, `t27` and
`t54` among them), `tfloat`, `bool`, `bool3` and `char`. Not `str`, not an array, a tuple, a `Vec<T>`, a `Result<T, E>`, a
struct or an enum. Writing `impl Ord for str` does not change that and is not
a workaround: maniT's comparison operators are built into the compiler and
never dispatch to a user `cmp`, so the `impl` would satisfy a bound whose
operator still cannot run.

To order values of a type the operators do not accept, compare a component
that they do:

```
fn wider(a: Rect, b: Rect) -> Rect { if a.w > b.w { a } else { b } }
```

An unsatisfied bound is reported under the `unsatisfied-bound` lint, which is
`deny` by default (see [Lint levels](#20-lint-levels)).

> **Until 26 August 2026 this section explained the wrong answer by saying the
> comparison compared the two values' ADDRESSES, and told you to write
> `impl Ord for Point`.** Neither survived being tested. The comparison is not
> an address comparison: swapping the order in which the two values are
> DECLARED does not flip the result, because the comparison is simply always
> false (report.txt P45, pinned by `ord63_address_theory`). And the prescribed
> `impl` made the bound pass while changing nothing about the operator, so
> following this section's own advice restored the silent wrong answer it
> exists to prevent. Both claims were true-sounding descriptions of a
> mechanism nobody had measured.

### Generic structs

```
struct Pair<T> {
    pub first: T,
    pub second: T,
}

let p = Pair { first: 1, second: 2 };
io::print_int(p.first);
```

The type parameter is inferred from the literal, so `p` above is a
`Pair<int>`, and a `Pair` of floats holds floats:

```
let q = Pair { first: 1.5, second: 2.5 };
if q.first > q.second { ... }        // a FLOAT comparison
```

> **The fields in this example gained their `pub` on 27 August 2026.** Without
> it the struct declares and the literal builds, but `p.first` is refused —
> "field 'first' of type 'Pair' is private" — so the example could be read and
> not used. Nothing in it was false; it simply stopped one line before the
> line that fails.
>
> **And until the same day, a `Pair<T>` could not be passed to a function
> expecting one at all**, because `Pair` was on a hardcoded list of built-in
> generic constructors and shadowed the user's own declaration (report.txt
> P67). A struct of floats also could not hold its own values: the literal
> truncated them to integers (P68).

A method in an `impl<T>` block is instantiated per type, like a free function:

```
struct Box2<T> { pub a: T, pub b: T }
impl<T> Box2<T> {
    fn bigger(self) -> T { if self.a > self.b { self.a } else { self.b } }
}

let p = Box2 { a: -1.5, b: -2.5 };
io::print_float(p.bigger());          // -1.5
```

> **Until 27 August 2026 that line printed `-2.5`** — the body was compiled
> once with `T` erased to a machine word, so the comparison read two IEEE-754
> bit patterns as integers, and negative doubles order the opposite way from
> theirs (report.txt P69). The impl's parameters are bound from the receiver's
> type arguments, positionally, so `impl<A, B> Two<A, B>` on a `Two<int, float>`
> gives `A = int` and `B = float`.

### Reserved type names

Fifteen names are the compiler's own, and a `struct` or `enum` may not take
one. The declaration itself is refused, naming what the name already means:

```
struct Vec<T> { pub first: T, pub second: T }
// error: `Vec` is a reserved type name, so this struct cannot be reached
// through it: the annotation `Vec<...>` resolves to the built-in `Vec<T>`,
// never to this declaration. Rename the struct.
```

| | |
|---|---|
| `Vec` `Map` `Set` `Deque` `TernaryTrie` `Channel` `Mutex` `Result` `Range` `Option` | built-in generic types |
| `i64` `f64` `String` | aliases for `int`, `float` and `str` |
| `bool` | the boolean type |
| `Self` | the enclosing `impl` block's type |

The sixteen primitive spellings the lexer reserves — `int`, `float`, `str`,
`char`, `void`, `trit`, `tryte`, `t9`, `t27`, `word`, `t54`, `trint`,
`tfloat`, `bool3`, `tribool`, `T3Bool` — are keywords, so `struct int` is a
parse error rather than this one.

`lint allow(reserved-type-name);` turns the check off for one module. It
restores the previous compiler exactly, which is to say the collision becomes
silent again rather than going away: the declaration is still unreachable
through its own name. The two standard-library modules that declare `Vec`,
`Map`, `Mutex` and the rest use it, because those declarations *are* the
built-ins.

> **Until 27 August 2026 the collision was not reported at all** (report.txt
> P70). Which spelling broke depended on which of the compiler's two
> name-resolution tables answered: the first ten shadowed only
> `struct Name<T>`, so `struct Vec` was fine; `i64`, `f64`, `String` and `bool`
> shadowed only the plain form, so `struct String<T>` was fine. Either way the
> program was refused at its *use*, with a message naming neither cause nor
> remedy — `expected Vec<<unknown>>, found Vec<int>`, and for `bool` the
> uninterpretable `expected bool, found bool`.
>
> `struct Self` was worse and is why this is a defect rather than a wording
> fix. It resolved to `<unknown>`, which is compatible with everything, so the
> program type-checked, `manitc check` exited 0, and the field read took slot 0:
> `struct Self { first, second }` printed `1` for `p.second` on **both**
> backends.

### Generic types in the standard library

```
let v: Vec<int> = Vec::new();
let m: Map<str, int> = Map::new();
let r: Result<int, str> = Ok(42);
```

---

## 15. Concurrency

maniT uses a cooperative, single-threaded task model. All tasks run interleaved
on one thread; there is no preemption.

### spawn

```
let t = spawn {
    do_work()
};
```

Creates a task that runs the block asynchronously.

### await

```
let result = await t;
```

Suspends the current task and resumes when `t` completes. Implemented on both
backends as of 2 September 2026; `docs/semantics.md` §11.12 is normative, and
`t` must be a `Task<T>` — `spawn { B }` is what produces one.

### Channels

```
let ch: Channel<int> = channel<int>();     // unbounded
let bd: Channel<int> = channel<int>(8);    // holds at most 8
ch.send(42);
let v = ch.recv();
ch.close();                                 // no more will be sent
```

Channels are MPSC (multiple producer, single consumer).

> **Added 2 September 2026 (§11.10, §11.11 of `docs/semantics.md`).** This
> section listed only `send` and `recv`. Two operations it did not mention were
> already implemented on both backends, and a third is new:
>
> - **`close()`** — states that no further value will be sent. A closed channel
>   still **drains**; once drained, `recv` returns the zero value rather than
>   blocking, and `try_recv` answers `Err("closed")`. **`try_recv` is the only
>   way to tell a drained channel from a sent zero.** A `send` after a close is
>   a **trap**: the value has nowhere to go.
> - **A capacity** — `channel<T>(n)` holds at most `n` values, and a `send`
>   that finds it full **blocks** until a receive frees a slot. That is a
>   fourth yield point, and it is the only one a program can reach by accident:
>   `channel<T>()` is unbounded and can never be full. A capacity below 1 is a
>   **trap**, not a clamp — a zero-capacity channel can never hold a value.
>
> Blocking on a channel nothing can fill *or drain* is a **detected deadlock**
> with a message naming which, not a hang; that is the property the cooperative
> scheduler exists to provide.

### Mutex

```
let shared: Mutex<int> = Mutex::new(0);
shared.lock();
// ... critical section
shared.unlock();
```

### AtomicTrit

```
let flag: AtomicTrit = AtomicTrit::new(0);
flag.set(+);
let v = flag.get();
```

Lock-free ternary flag.

### Barrier

```
let b: Barrier = Barrier::new(3);
b.wait();   // blocks until 3 tasks have called wait()
```

### Semaphore

```
let sem: Semaphore = Semaphore::new(2);
sem.acquire();
// ... at most 2 tasks here at once
sem.release();
```

### async functions

```
async fn fetch() -> str {
    async::sleep(100);
    "done"
}

let t = fetch();
let s = await t;
```

### async::select

Runs multiple async expressions and returns the value of whichever completes first:

```
let result = async::select(task_a, task_b, task_c);
```

---

## 16. Use declarations

```
use std::io;
use std::math;
use std::ternary;
use std::collections;
use std::sync;
use std::async;
```

`use` makes the named module's functions available by their short name
(`io::println`, `math::sqrt`, etc.). It does not currently affect name resolution
in the compiler — the stdlib functions are always available via their qualified
path.

---

## 17. Global variables

```
pub let GRAVITY: float = 9.81;
let mut TICK_COUNT: int = 0;
```

Global variables must have an explicit type annotation. They are visible to all
functions in the same file and to other files if `pub`.

---

## 18. Operators quick reference

### Precedence (high to low)

| Level | Operators | Associativity |
|-------|-----------|---------------|
| 13 — unary | `-`  `!`  `tnot`  `*`  `&` | right |
| 12 — postfix | `.field`  `.method()`  `[idx]`  `()` | left |
| 11 — multiplicative | `*`  `/`  `%` | left |
| 10 — additive | `+`  `-` | left |
| 9 — shift | `<<`  `>>` | left |
| 8 — bitwise AND | `&` | left |
| 7 — bitwise XOR | `^` | left |
| 6 — bitwise OR | `\|` | left |
| 5 — ternary logic | `tand`  `tor`  `txor` | left |
| 4 — comparison | `==`  `!=`  `<`  `>`  `<=`  `>=` | left |
| 3 — logical AND | `&&` | left |
| 2 — logical OR | `\|\|` | left |
| 1 — range | `..`  `..=` | left |

### Type compatibility

| Operation | Allowed types |
|-----------|--------------|
| `+` `-` `*` `/` `%` | `int`, `float`, `trit`, any `t<N>` (`tryte`, `t9`, `t27`, `t54`) |
| `tand` `tor` `tnot` `txor` | `trit`, `bool3` |
| `&&` `\|\|` `!` | `bool` |
| `&` `\|` `^` `<<` `>>` | `int` |
| `<` `>` `<=` `>=` | `int`, `float`, `trit` and other comparables |
| `==` `!=` | any type |

---

## 19. External declarations

`extern` declares a native — a function the backend supplies as a C symbol or
a T3ISA syscall rather than one compiled from maniT source.

```
extern "c"  fn gui::set_color(r: int, g: int, b: int) -> void
    available(llvm);

extern "t3" fn io::read_key() -> int
    available(t3, llvm);

extern "c"  fn str::to_lower(s: str) -> str
    available(llvm) deprecated("use str::to_lower");
```

The ABI string and the full signature are mandatory. Two optional clauses may
follow, in either order:

| clause | meaning |
|---|---|
| `available(llvm, t3)` | the backends that provide an implementation |
| `deprecated("...")`   | calling it warns, with this message |

**What a declaration buys.** Without one, a native's parameter types are
inferred and its arguments are not checked, so `io::println_int(5 > 0)` was a
silent coercion — printing `-1` on one backend and `1` on the other. With one,
the signature is in maniT's own type system and the call is checked like any
other:

```
error: argument 1 to 'io::println_int': expected `int`, found `bool`
```

**Names.** The declared name is written qualified, exactly as it is called:
`io::println`, not `println`. A native may be declared once; a second
declaration of the same name is an error, because the point of the form is that
the declaration is the authority on the signature.

**Availability.** Omitting `available(...)` means *unstated*, which is not the
same as "available nowhere". Calling something declared unavailable on the
selected backend is reported under the `backend-unavailable` lint.

**Migration.** The standard library's 413 natives are not yet declared. Calls
to an undeclared native behave exactly as they always have. To list the ones a
program reaches — the migration backlog — turn the lint on:

```
manitc check prog.mt --warn undeclared-native
```

---

## 20. Lint levels

Every diagnostic the compiler can emit has a name and a level. Levels are
`allow` (silent), `warn` (reported), `deny` (reported, fails the build) and
`forbid` (deny, and cannot be lowered afterwards).

The name appears in the diagnostic itself, so it is always visible:

```
warning: prog.mt:2:5: unused variable `x`; prefix with `_` if intentional [unused-variable]
```

Set a level for one compilation:

```
manitc check prog.mt --allow unused-variable --deny shadowing
manitc compile prog.mt -W undeclared-native -D unknown-type
```

`-A`, `-W`, `-D` and `-F` are the short forms. An unknown lint name is an
error, not a no-op — a silently ignored `--deny unusd-variable` would leave the
compilation at a strictness nobody chose.

Set a level for one module, at item position:

```
lint allow(unused-variable);
lint deny(shadowing, unknown-type);
```

### The lints

| name | default | reports |
|---|---|---|
| `unused-variable`     | warn  | a binding that is never read |
| `unused-function`     | warn  | a function that is never called |
| `shadowing`           | warn  | a binding that hides an outer one |
| `unreachable-code`    | warn  | statements after a diverging expression |
| `integer-overflow`    | warn  | a constant expression that overflows |
| `division-by-zero`    | warn  | a constant division by zero |
| `unknown-type`        | warn  | an unresolved module, type or item path |
| `undeclared-native`   | allow | a native called with no `extern` declaration |
| `deprecated-native`   | warn  | a call to an extern marked `deprecated` |
| `backend-unavailable` | allow | an extern not `available` on this backend |
| `division-semantics`  | allow | a `/` or `%` whose meaning depends on the language version |
| `unsatisfied-bound`   | deny  | a generic argument that fails a trait bound |
| `literal-out-of-word` | allow | an `int` literal outside the 27-trit range (v1 only) |
| `unproven-refinement` | allow | a call whose argument a parameter's `where` could not be proved for (§26) |
| `backend-unavailable-chain` | deny | a call chain that cannot run on this backend |
| `reserved-type-name`  | deny  | a `struct` or `enum` declared under a name the compiler owns |
| `unlinkable-user-module` | deny | a `use` of a non-`std` module, whose bodies are never emitted |
| `colliding-stdlib-symbol` | deny | a function whose name is the mangled name of an expanded stdlib function |
| `undeclared-type`     | deny  | a type name that is declared nowhere at all |
| `undeclared-field`    | deny  | a field name the struct does not have |
| `unchecked-instantiation` | warn | a generic instantiation that does not type-check, so the erased body runs unchecked (§22) |

`--warn-as-error` still means "raise everything to deny".

`reserved-type-name`, `undeclared-type` and `undeclared-field` are reported
only at `deny` or
above, and the reason is worth knowing because it is a property of the compiler
rather than of the lint: a recorded warning is printed after analysis finishes,
and a program these lints fire on typically fails analysis too, so a
`warn`-level report would be discarded before anyone saw it. See §14.

> **Added 1 September 2026 (P95).** `undeclared-type` is the reverse of
> `reserved-type-name`. That one catches a name you DID declare which the
> compiler answers for first; this one catches a name nothing declares at all.
> Both used to resolve to the unknown type, which is compatible with everything
> — so `struct Holder { pub a: NoSuchType, pub b: int }` type-checked, `manitc
> check` exited 0, and both backends ran the program with the field holding
> whatever it was given. Measured in thirteen type positions: a struct field, a
> parameter, a return type, a `let` annotation, an enum variant payload, an
> array element, a generic argument, a tuple element, a function type's
> parameter, an `impl` target, a cast target, a global's annotation and a
> generic struct's argument. `lint allow(undeclared-type);` restores the
> previous behaviour exactly.

> **Added 2 September 2026 (P103).** `undeclared-field` is `undeclared-type`
> one level in: that refuses a TYPE name nothing declares, this refuses a FIELD
> name a perfectly well declared struct does not have. Both used to resolve to
> the unknown type, and this one then reaches the IR lowerer's
> `field_slot_index`, which has no slot for it and reads **slot 0** — so the
> program runs and returns a *different field's value*, on both backends, with
> `manitc check` exiting 0.
>
> `field_slot_index` has carried a `debug_assert!` for exactly this since P44.
> It is **debug-only**, and `thatteos/build.sh` resolves the compiler to
> `target/release/manitc`, so it never fired in a shipped build. The cost is
> report.txt P102(b): two thatteOS syscalls tested `!desc.valid` on a struct
> whose field is `open`, read `desc.fd` instead, and mis-answered `EBADF` on
> fd 0 while skipping the check entirely on every other fd.
>
> It fires only when the receiver's type is a KNOWN struct. An unresolved
> receiver is `undeclared-type`'s business, a tuple has its own rule, and a
> generic struct's field *names* do not depend on its type arguments — so the
> question is asked of the declaration even when the field's type is still
> unknown. Measured before it was written: **0 occurrences across every `.mt`
> file in maniTC and thatteOS, and 0 across all 2,507 files of the model
> corpus**, the single real instance being the thatteOS one this found.
> `lint allow(undeclared-field);` restores the previous behaviour exactly.
>
> **The name is measured, not chosen.** It is not `unknown-field`, because the
> lexer reads `unknown` as the three-valued literal and `lint
> allow(unknown-field);` is a *parse error*. That is pre-existing rather than
> new — the older `unknown-type` lint has never been writable in an in-source
> directive either, though `-A unknown-type` on the command line works
> (report.txt P104) — but a lint whose `allow` cannot be spelled is not an
> exact restoration of anything.

### The manifest

The effective levels are recorded **in the artifact**, so a compiled program
says what it was checked for:

```
$ strings a.out | grep manitc-lints
manitc-lints v1 compiler=0.1.0 backend-unavailable=allow ... unused-variable=warn
```

On the LLVM backend it is a comment in the `.ll` and a `@manitc.lints`
constant that survives linking. On T3 it is a comment in the `.t3s` and a
`.t3l` sidecar, alongside the existing `.t3d` and `.t3f`. `--print-lints`
prints the same set before compiling.

This exists because strictness used to be a property of the compiler binary
rather than the invocation: changing it invalidated every earlier measurement,
and the only way to keep results comparable was to archive the exact binary
that produced them. A recorded manifest makes a result self-describing instead.

---

## 21. Language versions

A program is compiled under a **language version**, chosen with `--lang` and
defaulting to `v1`:

```
manitc compile prog.mt                 # v1 — the default
manitc compile prog.mt --lang v2       # v2
manitc check prog.mt --lang v2
manitc run-t3 prog.mt --lang v2
```

An unrecognised version is an error, not a fallback to the default: a typo that
quietly selected `v1` would compile the program under arithmetic its author did
not ask for and nothing downstream would say so.

### What v2 changes

**C4 — `/` and `%` round to nearest, ties away from zero.**

```
  7 / 2 == 4     7 % 2 == -1
 -7 / 2 == -4   -7 % 2 ==  1
  2 / 3 == 1     2 % 3 == -1
```

Truncation is C's rule, and this is not a two's-complement machine: in balanced
ternary, dropping low trits *is* rounding to nearest, so truncating is extra
work done to imitate a representation the machine does not use. On T3 the
rounding division is a single instruction (`TDIVN`, T3ISA v1.6); the LLVM
backend needs sixteen to say the same thing.

Ties go away from zero because the balanced range is symmetric and
`(-a) / b == -(a / b)` is worth keeping. Round-half-to-even was the alternative
and was rejected: balanced ternary's unbiasedness comes from the
representation, not from the tie-break.

`%` changes with `/` and not separately — it is *defined* as
`a - (a / b) * b`, which is what keeps `(a / b) * b + (a % b) == a` true. The
practical consequence is that the balanced remainder can be **negative for a
positive dividend**, so `x % 2 == 0` is still an evenness test but
`x % 2 == 1` is not an oddness test.

**N5 — `int` is 27 trits on every backend.**

Under `v1`, `int` is a 27-trit word on T3 and a 64-bit integer on LLVM, so a
value in `(3812798742493, 2^63-1]` exists on one backend and not the other:

```
let m: int = 3812798742493;
m + 1        // v1:  T3 traps,  LLVM gives 3812798742494
             // v2:  both trap
```

Under `v2` the LLVM backend range-checks `int` addition, subtraction and
multiplication and both backends agree. The cost is a guard call before each of
those three operations on LLVM — the same cost the divisor guard has always
paid on every integer division — and it is paid only by code compiled `--lang
v2`. On T3 it costs nothing: the machine's word already *is* 27 trits.

`trint` is the wider type for code that wants the machine word and is **not**
range-checked. Note that a T3 register is 27 trits, so a `trint` still cannot
hold more than that on T3; the wider range is an LLVM-only escape hatch.

Not covered: `int` literals, casts, `<<`, and values returned by natives are
not range-checked. N5's claim is about the three arithmetic operators.

### Migrating

`--warn division-semantics` lists every `/` and `%` whose meaning depends on
the version, with the enclosing function named:

```
$ manitc check prog.mt --warn division-semantics
warning: prog.mt:4:21: `/` in `main` on an integer truncates under --lang v1,
  and rounds to nearest under v2; write `math::div_trunc(a, b)` to mean this in
  both [division-semantics]
```

That list is the migration backlog, generated from the program rather than kept
by hand. Rewriting a site onto `math::div_trunc` / `math::rem_trunc` (or
`div_near` / `rem_near`) pins its meaning, and it then means the same thing
under both versions — those four are the only division spellings that do.

Compile the same source both ways and compare the output; the compiler makes no
attempt to guess which sites were meant to change.

### Why v1 stays the default

Recommendation R2 holds that delay is preferable to making a change of this
kind casually, and moving the default in the same release that introduces the
behaviour would be making it casually. When the default moves, it moves as its
own change, with the backlog already generated.

---

## 22. Ownership and moves

ManiT has a move checker. It runs after type checking and before IR lowering,
reports as `<borrow>`, and rejects programs:

```
error: TypeError: <borrow>:2:63: use of moved value: 's'
```

It is deliberately small: **there are no lifetime annotations, no reference
types, no reborrowing and no non-lexical liveness analysis.** It is a safety
net over bindings, not a Rust-style borrow checker, and the rest of this
section is the whole of it.

### Copy types are never moved

A value of these types is copied wherever it is used, and none of the rules
below apply to it:

`int`, `float`, `bool`, `bool3`, `trit`, any `t<N>` (`tryte`, `t9`, `t27`,
`t54` among them), `tfloat`, `char`, `void`, and function-pointer types.

The concurrency handles are also `Copy`, deliberately — `Mutex`, `Channel`,
`Task`, `AtomicTrit`, `Barrier`, `Semaphore` and `MutexGuard`. Their runtime
representation is a pointer to shared state and the documented usage pattern
aliases them across tasks, so copying a handle copies the reference. See
[§15 Concurrency](#15-concurrency).

Everything else is a **move type**: `str`, `Vec<T>`, arrays, tuples, structs,
enums, `Result`, and any generic instantiated over them.

### What moves, and what does not

This is the part to read carefully, because **assignment moves and passing to a
function does not** — which is the opposite of the convention in several
languages with similar syntax.

| construct | moves? |
|---|---|
| `let t = s;` | **yes** |
| `s2 = s;` (assignment) | **yes** |
| `(s, 1)` — tuple literal element | **yes** |
| `Point { x: s }` — struct literal field | **yes** |
| `take(s)` — argument to a function | no |
| `v.push(s)` — argument to a method | no |
| `let v = [s, "c"];` — array literal **bound to a name** | **yes** |
| `fmt::format("{}", [s])` — array literal **as an argument** | no |

So all three of these are errors, and all three are the *same* error — the
`let` moved `s`, and everything after it is a use of a moved value:

```manit
let s: str = "ab";
let t: str = s;
io::println(s);        // error: use of moved value: 's'
let u: str = s;        // error, likewise
io::print_int(take(s)); // error, likewise
```

while all of these are accepted, because a call never consumes its argument:

```manit
let s: str = "ab";
take(s);
take(s);               // fine — take() borrowed it both times
io::println(s);        // fine
let t: str = s;        // fine — this is the first move
```

> **Corrected 2 September 2026 — the array rows are now two rows, and the
> distinction is designed.** This note used to read: *"The array and tuple rows
> differ from each other, and that is a quirk of the implementation rather than
> a designed distinction. Do not rely on the array row."* That was true, and
> B7's D-3 asked which of the two was wrong.
>
> **Measured, the question was malformed.** An array literal is two constructs
> wearing one syntax:
>
> * **Bound to a name, stored in a field, or returned — a CONTAINER.** It
>   outlives the expression and holds a second name for each element, exactly
>   as a tuple literal and a struct literal do. It consumes, and now does.
> * **Passed as an argument — this language's VARARGS list.** `fmt::format`,
>   `print` and their family take `[T]`. There it is an argument, the row above
>   governs it, and a call does not consume its argument. Consuming here would
>   make `f(s)` and `f([s])` disagree about the same `s` in the same call.
>
> The split is not a carve-out fitted to whatever failed: **1,120 of 1,120
> array-literal sites in the standard library are varargs**, and 36–56 % of the
> ones in ordinary programs. Treating the argument list as a container would
> have refused `fmt::format` itself.
>
> The rule can only fire on a plain move-type **variable** — `["To:", "Sub:"]`
> is untouched — which is why the change costs nothing measurable: **0 verdict
> differences over 366 files in these repositories and 2,507 in the model
> corpus.** One program was affected and it was a real alias, in
> `thatteos/studioMani/email/email.mt`.

### A parameter that consumes: `move`

**Added 3 September 2026 — B7's D-2.** The table above says a call never
consumes its argument, and that is still the default. A parameter marked
`move` is the exception:

```manit
fn eat(x: move str) -> int { return str::len(x); }

let s: str = "ab";
io::print_int(eat(s));
io::println(s);          // error: use of moved value: 's'
```

Without it a function that takes ownership cannot be written at all, which is
what F-4 (regions) needs before it can start.

**Why an annotation rather than a rule about every call.** Making every
argument consume was measured before it was rejected: it refuses **24.7 % of
1,545 corpus programs, 36.4 % of the distinct programs in these repositories,
and fifty functions of this standard library.** Every failure is the same
shape, and the shape is ordinary correct code — `str::take` calls `len(s)` and
then `slice(s, …)`. ManiT has **no reference types**, so a call is the only way
to read a value twice; that is why the default cannot be "consume", and why
the annotation goes on the few parameters that genuinely take ownership. Its
blast radius is zero by construction: nothing written before this existed
carries the word.

**`move` is contextual, not reserved.** `std::fs` declares `fn move(src, dst)`,
so the word remains a usable name; it is an annotation only where a type
follows it. In `x: move` the word is the type, in `x: move str` it is the
annotation.

**Limit, stated rather than left to be discovered:** a call through a function
*pointer* consumes nothing, because the signature is not in hand at the call
site. `let f: fn(str) -> int = eat; f(s);` leaves `s` live.

### What a binding shares, and what it copies

**Added 3 September 2026, and it is the answer to B7's D-1.** Until now this
section said which constructs MOVE and said nothing about whether the new name
shares storage with the old one — so a reader could not tell whether
`let u = s.field; u.x = 9;` changes `s`. It does. Every row below was measured
on both backends before it was written.

| construct | the new name … |
|---|---|
| `fn f(p: Point)` — an aggregate **parameter** | is a **mutable reference** to the caller's object. `p.x = 9` is visible to the caller. |
| `let b = a;` — from a **name** | **copies**. `a` is moved, so only the caller of the enclosing function can observe the difference — and that is exactly who it protects. |
| `let u = s.f;` / `let u = v[i];` — from a **projection** | **aliases**. Writing through `u` writes through to `s` or `v`. |
| `let u = f(…);` — an aggregate **returned** by a call | **copies**, for lifetime rather than for semantics: the callee's frame is gone. |

**The rule to carry away: a call borrows, a name-binding copies, and a
projection shares.** A reader arriving from Rust should note the third row in
particular — there is no `&`, so a projection is the only way to name part of
an aggregate, and it necessarily shares.

**Why sharing rather than copying**, since the alternative was available and
was rejected. Copying every projection is affordable in principle and not on
this target: the T3 heap is 2,536 words with no free, an eleven-field process
record is about twelve of them, and a scheduling pass that copy-constructed
nine of them would exhaust the heap in roughly twenty-three passes. thatteOS's
`src/kernel/process.mt` measured exactly that and designed its process table
around sharing, with its operations named for the mutation they perform. When
regions land (F-4) this becomes a real choice rather than a forced one.

> **This is a designed rule now, and it was not before.** The two backends used
> to disagree about it: T3 shared, and the LLVM emitter produced an aggregate
> load/store pair — a copy — and then failed to link, so `v[i][j]` on a nested
> array did not compile at all while `manitc check` accepted it. report.txt
> P110 and P111. `escape_analysis_tests::d1_*` and `::p111_*` pin both halves.

### Rebinding clears a move

Assigning a fresh value to a moved binding makes it usable again:

```manit
let mut s: str = "ab";
let t: str = s;        // s is moved
s = "cd";              // s is live again
io::println(s);        // fine
```

### Shadowing is per-binding, not per-name

The checker keys a move on the *binding* — its declaration scope and name — not
on the bare name. Moving an inner shadow does not poison the outer variable,
and an inner `let` does not launder an outer move:

```manit
let s: str = "ab";
if true {
    let s: str = "cd";   // a different binding
    let t: str = s;      // moves the INNER s only
}
io::println(s);          // fine — the outer s was never moved
```

### Moving in a loop

Moving a variable declared **outside** a loop, from **inside** its body, is
rejected: the body runs more than once, so the second iteration would consume
an already-moved value.

```manit
let s: str = "ab";
for i in 0..3 {
    let t: str = s;      // error: value moved in a loop body
}
```

A variable declared inside the body is fresh on every iteration, so moving it
is fine. Calling a function with `s` in a loop is also fine, since a call does
not move.

### What the checker does not do

It does not track moves through references (there are none), does not reason
about conditional moves converging at a join point beyond forking the moved-set
across match arms, and does not free anything — see
`KNOWN_ISSUES` on the absence of a free/destroy API. A move is a compile-time
restriction on reading a binding, not a runtime transfer of ownership.

### `affine` — a type that may be used once

**Added 4 September 2026.** A struct may be declared `affine`:

```manit
affine struct Token { }
```

**Affinity is opted into.** It is a property of the declaration, not of a
category of type, because the set of types where a copy is observable is small
and already known.

**What it changes today is narrow, and stating it narrowly is the point.**
Every struct is already a move type, so an affine binding is refused for a
reason that predates the marker — `let a = t; let b = t;` is refused with or
without it. What the marker adds is that **`spawn` may not capture an affine
value**:

```manit
affine struct Token { }

fn main() {
    let t = mk();
    spawn { let u = t; }   // error: `spawn` would capture `t`, whose type
                           // `Token` is declared `affine`
}
```

A capture is a *copy* (§11.2 of `docs/semantics.md`), and an affine value that
has been copied exists twice — which is what affinity forbids. Without the
marker the same capture is allowed, because a fieldless struct is not an
aggregate.

**What it is FOR is wider than what it does**, and the gap is recorded rather
than glossed. `is_move_type` grants a Copy exemption *by name* to `AtomicTrit`,
`Barrier`, `Semaphore`, `MutexGuard`, `Mutex`, `Channel` and `Task`, because a
handle is a pointer to shared state. `MutexGuard` is declared `affine` in
`stdlib/sync.mt` — a guard that can be copied is a guard that can unlock twice
— and **it changes nothing today**, because `Mutex::lock()` returns `Unknown`:
the guard's type never arrives, and affinity is keyed on a type name.
`report.txt` P132.

### What the checker can see: generic bodies

**Added 4 September 2026.** The checker runs over the program the analyzer
produced, so **it sees exactly the function bodies the analyzer managed to
build** — and for a generic function that is more than one body.

A generic function is checked twice over. Once **erased**, with every type
parameter standing for nothing, where a `T` is not a move type because nothing
says it is one. And once **per instantiation**, with `T` bound to a real type,
where it may be. So whether a move inside a generic body is caught can depend
on the type it is called at, and that is the intended behaviour:

```manit
struct P { pub x: int }

fn dup<T>(a: T) -> int {
    let b = a;
    let c = a;           // a move only when T is a move type
    return 1;
}

fn main() {
    dup(1);              // fine — `int` is a Copy type
    dup(P { x: 1 });     // error: use of moved value: 'a'
}
```

**An instantiation that does not type-check is discarded** and the call keeps
the erased body (§25 states the same rule for const parameters, where it is an
error instead). A discarded instantiation was never built, so the move checker
never saw it — and a move that only that binding would have revealed goes
unreported:

```manit
fn dup<T>(a: T) -> int {
    let b = a;
    let c = a;
    let q = a | 1;       // does not check at T = P
    return 1;
}

fn main() {
    // Accepted: the instantiation is discarded, so nothing checked the
    // two lines above `let q` either.
    dup(P { x: 1 });
}
```

That is a hole in the checker's coverage rather than a decision about moves,
and the compiler **names it** — `unchecked-instantiation`, §20, `warn` by
default. It reports the call site and the reason the body failed. Silencing it
with `lint allow(unchecked-instantiation);` changes nothing else: the
instantiation is still discarded and the call still keeps the erased path.

**What is declared, rather than inferred, survives the discard.** A parameter
marked `move` consumes its argument whether or not the callee's body checks at
that binding, because that is a fact about the signature the reader wrote:

```manit
fn eat<T>(a: move T) -> int { let q = a | 1; return 1; }

fn main() {
    let p = P { x: 1 };
    eat(p);
    io::print_int(p.x);  // error: use of moved value: 'p' — still caught
}
```

## 23. Allocation regions

*(F-4, 3 September 2026. Both backends.)*

A **region** is a lexical scope whose allocations are released when it ends:

```manit
region {
    let p: Point = Point { x: 1, y: 2 };
    let q: Point = Point { x: p.y, y: p.x };
    io::println_int(q.x);
}
// every cell allocated above is gone here
```

It is a **statement**, never an expression, and that is the first half of the
safety argument: a region that could produce a value could hand out a pointer
into the memory it is about to release.

### Why this rather than a free/destroy API

`KNOWN_ISSUES` issue 6 has recorded "no free/destroy API — leak by design"
since the initial release. The recommendation attached to it was regions rather
than a garbage collector, for a reason specific to this machine: the T3 heap
**is** a bump pointer, so releasing a region costs one assignment, while a
tracing collector on a 65,536-word address space would spend more than it
recovered. A manual `free` would be the other option and it is worse here — the
language has no ownership yet, so use-after-free would be a runtime surprise
rather than a compile error.

### What it costs and what it saves

The same program, a 200-iteration loop allocating two 2-word structs per pass,
measured on T3 with `run-t3 --profile`:

| form | peak heap |
|---|---|
| with `region` around the body | **4 words** |
| without | **800 words** |

On LLVM, where the heap is the host's, the difference is visible as address
space: a 3,000,000-iteration version of the same loop runs to completion under
an 80 MB cap with the region and **segfaults without it**.

### The three rules

A region releases every cell allocated inside it, so nothing that outlives the
region may still be holding one. Three rules enforce that, and each fails in a
different direction:

1. **No `return` inside a region.** It would leave without releasing, and a
   returned value could point into the released memory. Compute the value, end
   the region, then return.
2. **No `break` or `continue` that leaves a region.** Same reason, minus the
   value. A loop written *inside* a region is ordinary, and its `break` lands
   inside too — the rule is about crossing the boundary, not about loops.
3. **Nothing that outlives the region may be GIVEN a value of storage type
   inside it.** `str`, structs, tuples and arrays are storage; scalars —
   `int`, `trit`, `float`, `bool`, `char` — are not, and may leave a region
   freely, which is how a region returns an answer.

   The rule is asked of the **root** of whatever would be left holding the
   cell, so all four of these are refused, and the last two are the reason the
   rule is not about assignment alone:

   ```manit
   outer = s;            // a binding declared outside the region
   outer.field = s;      // the root is `outer`
   outer[0] = s;         // likewise
   outer_vec.push(s);    // a method call, with `outer_vec` as the receiver
   ```

   A `Vec` handle may leave a region; what may not is the **cell** it would be
   left holding, so the test on a method call is on its ARGUMENTS and not on
   the receiver's own type. A handle built *inside* the region may be filled
   freely, and a scalar may be pushed onto an outer one.

   **A container counts as storage when its ELEMENTS do.** `Vec<int>` may leave
   a region; `Vec<str>` may not, because letting it out is the same escape as
   letting one of its cells out:

   ```manit
   let mut keep: Vec<str> = Vec::new();
   region {
       let inner: Vec<str> = Vec::new();
       inner.push(str::concat("hel", "lo"));   // fine: inner is inner
       keep = inner;                            // refused: it holds cells
   }
   ```

   A value whose type the compiler has not resolved — which is what
   `let v = Vec::new();` binds, with no annotation — counts as storage too,
   because an unknown type could be a cell or a container of cells. **Annotate
   it** if it needs to leave a region.

```manit
let mut n: int = 0;
region {
    let s: str = str::concat("a", "b");
    n = str::len(s);          // fine: an int is not storage
}
io::println_int(n);           // 2
```

`Vec<T>`, `Map`, `Set`, `Channel<T>` and `Mutex<T>` are **not** storage either,
and that is a fact about both backends rather than a convenience: on T3 they
live in the emulator's object table, which the bump pointer does not address,
and on LLVM they are the collections library's own allocations, which the
region does not hold. A handle may leave a region; a cell may not.

The rule is stated on the **type** and not on where the value was allocated. A
provenance analysis would accept more programs, and making one cheap is exactly
what affine types (B7) are for; until then this refuses some safe programs and
no unsafe ones, which is the direction to be wrong in.

### What is not reclaimed yet

- **The COLLECTIONS allocate outside the region.** A `Vec` built inside a
  region is still there afterwards. That is not an oversight: the rules let a
  handle leave a region, so anything reachable from one must not be
  region-owned, and the collections' internals are exactly that. What the rules
  *do* forbid from leaving is storage — which is why the **string** routines
  are region-owned on both backends now (fourteen sites in `runtime/core.c`),
  and why the collections are not. The residual is the collections, the
  scheduler and synchronisation handles, and the GUI and network resources.
- **A region does not reclaim while another task exists.** The allocator is
  shared, so with a second task alive an allocation of *its* may sit above the
  region's mark, and resetting would hand out memory that task still holds.
  With other tasks running the region is a no-op — a leak, which is what
  happened before regions existed, rather than corruption. Per-task regions
  want a per-task heap and are future work.

---

## 24. Ternary widths: `t<N>`

*Added 3 September 2026 (Phase 5, C3).*

A balanced ternary integer of **N trits** is written `t<N>`, for N from 1 to
54. The five names the language has always had are spellings of five of those
widths, and they remain exactly what they were:

| Spelling | Means | Also spelled |
|----------|-------|--------------|
| `trit`   | `t<1>`  | — |
| `tryte`  | `t<6>`  | — |
| `t9`     | `t<9>`  | — |
| `t27`    | `t<27>` | `word` |
| `t54`    | `t<54>` | `trint` |

`t<6>` and `tryte` are the same type, not two types that convert; a value of
one may be used wherever the other is expected, because there is only one of
them.

```
struct Mantissa { pub m: t<18>, pub e: t9 }

fn scale(x: t<18>, k: t<12>) -> t<18> {
    return x + (k as t<18>);
}
```

`t<N>` is a type, so it appears wherever a type appears: `let` annotations,
parameters, return types, struct and enum fields, array elements, tuple
elements, generic arguments and `as` casts.

### Why the widths between the names matter

The five named widths are five points sampled from a continuum, and the gap
that shows is not hypothetical. `stdlib/t27f.mt` — the 27-trit floating-point
format — describes itself as a **9-trit exponent and an 18-trit mantissa**, and
before this section existed the format's own components could not be spelled in
the type system hosting it. The exponent had a name (`t9`) and the mantissa did
not. `t<18>` is that type.

### The range is derived, not tabulated

A `t<N>` holds −(3ᴺ−1)/2 … +(3ᴺ−1)/2, and the compiler computes that from N
rather than looking it up:

```
let x: t<18> = 193710244;   // (3^18 − 1) / 2 — accepted
let y: t<18> = 193710245;   // refused: "value 193710245 overflows type
                            //  't<18>' (range -193710244..193710244)"
```

For N of 40 and above, 3ᴺ exceeds a machine word, and what binds is the i64 the
value is held in — which is exactly what `t54` has always done and what its own
definition says. No width acquires a range the backends cannot represent.

> This is the direct dividend of report.txt **P122**, one day earlier. That
> finding fixed a `tryte` whose width was recorded as 3 in one place and 6 in
> another by making one function the sole authority and **deriving** the range
> from it. Because the range was derived, every width between the names got a
> correct one for free the moment the syntax existed. A table would have needed
> fifty-four new rows and would have been the same defect waiting again.

### `trit` is not `t<1>` by accident, and the difference is real

`trit` and `t<1>` are the same type — and it is the only width that carries the
three-valued logic role:

```
let a: t<1> = 1;
let b: t<1> = 0;
let c = a tand b;     // fine — a width-1 value is a logic value

let p: tryte = 1;
let q: tryte = 0;
let r = p tand q;     // refused: "`tand` is a three-valued logic operator"
```

This is not an irregularity in the family. A width-1 balanced ternary number
has the values {−, 0, +}, and those **are** {`false`, `unknown`, `true`}; the
logic role is what width one *means* here. The other widths have 3ᴺ values and
no such reading, so `tand` on a `tryte` is refused — the same refusal it has
always given.

### `t` is not a keyword

`t<N>` adds a type without adding a reserved word: the sixteen primitive
spellings the lexer reserves (§14) are unchanged, and `t` is not among them. It
is read as a width only in **type** position, so a variable named `t` and a
comparison `t < 0` continue to mean what they always meant:

```
let t: int = -3;
let w: t<18> = 40;
if t < 0 { io::println("t is negative"); }   // a comparison, not a type
```

This is decided structurally rather than by preference. Generic arguments are
parsed in exactly one place — the type parser — and there is no turbofish in
this language, so `<` after a name can only mean a type argument where a type
is already expected. An expression never reaches that code.

The measurement came before the syntax: `t` is declared as a variable **855
times** across both repositories and the model corpus, with **11** of the form
`t < …`. Reserving it would have deleted every one.

### What is not here

> **Corrected 4 September 2026 (Phase 5, B3).** The first paragraph below was
> true when written and is now false: **width polymorphism is implemented**,
> and it is §25. A width may be a `const` generic parameter as well as a
> literal, so `fn widen<const A: int>(x: t<A>) -> t54` compiles and runs on
> both backends. The paragraph is kept rather than rewritten because it records
> what the two refusals looked like and why they differed, which is the
> observation that placed the feature — and because a corrected document should
> show the correction (permanent rule 7).
>
> Two of its claims survive unchanged and are worth separating out. `fn f<T>(x:
> t<T>)` is **still refused**, because a type parameter is not a width; what
> moved is the message, which now names the KIND rather than the token. And the
> two-parameter form `fn widen<const A: int, const B: int>(x: t<A>) -> t<B>` is
> **still refused**, for a reason this paragraph did not know: `B` appears only
> in the return type, and ManiT has no turbofish, so nothing at a call site can
> name it. See §25.

**Width polymorphism is not implemented.** A width must be a literal:

```
fn widen<const A: int, const B: int>(x: t<A>) -> t<B>    // NOT accepted
fn f<T>(x: t<T>)                                          // NOT accepted
```

They fail in different places, which is worth knowing when reading the error:
the second is refused by this section's own message — "`t<N>` needs a trit
width, found `Ident("T")`" — while the first does not reach it at all, because
`const` is not a generic parameter form the language parses ("expected LParen").
Writing an algorithm once and instantiating it at whatever width a device
provides is C3's stated motivation, and it needs const generics (Phase 5's B3).
This section is the half that does not: a literal width is known at parse time,
so no constant evaluator is involved.

That is the second time an item's stated prerequisites turned out not to bind —
C6 (§13) records the same about B4 — and it is recorded rather than assumed,
because four Phase-4 premises were contradicted by measuring them.

**Widths do not constrain arithmetic.** `t<12> + t<18>` is accepted with no
cast and yields the numeric sum, and the range check shown above applies to a
**literal being bound**, not to the result of an operation:

```
let a: t<18> = 193710244;    // the maximum
let b: t<12> = 265720;       // the maximum
let c = a + b;               // 193975964 — past t<18>'s range, and not refused
```

This is not new and is not C3's: `tryte + t9` behaves identically on the
compiler before `t<N>` existed, giving 9300 for 300 + 9000. The cast in the
`scale` example above is therefore explicitness rather than a requirement.
Proving a result stays inside a width is what Phase 5's B6 (refinement types)
is for.

**The IR representation is not tight.** A `t9` lowers to an LLVM `i32` where an
`i16` would hold its ±9,841, and `t<N>` reproduces that step exactly rather
than narrowing it: `i16` up to 6 trits, `i32` to 9, `i64` beyond. Narrowing
`t9` would be a representation change to a shipped type arriving as a side
effect of adding a syntax, so it is left visible here for a change that
measures it.

## 25. Const generics: `const N: int`

*Added 4 September 2026 (Phase 5, B3).*

A generic parameter written `const N: int` binds an **integer** at
instantiation, where an ordinary parameter binds a type. It is what lets one
function be written at no particular width and used at several:

```
fn widen<const A: int>(x: t<A>) -> t54 {
    return x as t54;
}

fn main() {
    let a: t9 = 5;
    let b: tryte = 4;
    io::println_int(widen(a) as int);   // 5
    io::println_int(widen(b) as int);   // 4
}
```

`widen` is compiled twice, once for each width the program used. Neither copy
carries `A` at run time.

### Where a const parameter may be written

Three positions, all of them compile-time:

| Position | Example |
|----------|---------|
| a trit width | `fn f<const A: int>(x: t<A>)` |
| an array length | `fn g<const N: int>(a: [int; N])` |
| an integer value | `fn w<const A: int>(x: t<A>) -> int { return A; }` |

The third is what makes the first two useful, because it lets the body compute
with the width rather than only be annotated by it:

```
fn f<const A: int>(x: t<A>) -> int {
    let y: t<A> = 300;
    return y as int + A;
}

fn main() {
    let a: t9 = 1;
    let b: tryte = 1;
    io::println_int(f(a));   // 309
    io::println_int(f(b));   // 306
}
```

One body, two answers, and the difference is the width. Inside an
instantiation `A` is a literal, so it costs nothing at run time.

### A const argument is inferred from the arguments

There is no turbofish in ManiT, so a const parameter is never written at a call
site. It is bound structurally from the types of the arguments — `t<A>` against
a `t9` binds `A` to 9, `[int; N]` against a `[int; 3]` binds `N` to 3 — exactly
as a type parameter is bound from them.

`trit` binds a width of **one**, because `trit` *is* `t<1>` (§24):

```
fn w<const A: int>(x: t<A>) -> int { return A; }

fn main() {
    let a: trit = 1;   io::println_int(w(a));   // 1
    let b: t54  = 1;   io::println_int(w(b));   // 54
}
```

A call that pins nothing down is refused rather than compiled at some default:

```
fn width<const A: int>(x: t<A>) -> int { return A; }
fn main() {
    let n: int = 5;
    width(n);   // refused: `int` is not `t<A>` for any A, so nothing binds `A`
}
```

and so is a parameter that appears only in the return type, which nothing at a
call site could name:

```
fn make<const B: int>() -> t<B> { return 0; }   // declares
fn main() { let x: t9 = make(); }               // refused at the call
```

This is why the two-parameter `fn widen<const A: int, const B: int>(x: t<A>) ->
t<B>` of the enhancement plan is not writable as such: `A` is bound by `x` and
`B` is bound by nothing.

### Const parameters come last

A parameter list may mix kinds, and the const ones are written after the type
ones:

```
fn f<T, const N: int>(a: [T; N]) -> int { return N; }    // fine
fn g<const N: int, T>(x: t<N>, y: T) -> int { return N; } // refused
```

The refusal names the corrected list rather than a column. The two kinds are
kept as separate lists and recombined by position, so a type parameter written
after a const one has no reading either list can represent.

`int` is the only type a const parameter may be declared with, and one name may
not be both kinds:

```
fn f<const N: str>(x: t<N>)      // refused: `int` is the only type
fn g<N, const N: int>(x: t<N>)   // refused: `N` would be a type and a value
```

### `const` is not a keyword

Like `t` in §24 and `move` in §22, `const` is **contextual**: it introduces a
const parameter only when it is followed by a name inside angle brackets.
Everywhere else it is an ordinary identifier, and it may still be one:

```
fn w<const A: int>(x: t<A>) -> int { return A; }

fn main() {
    let const = 7;                  // still a variable name
    let a: t9 = 1;
    io::println_int(const);         // 7
    io::println_int(w(a));          // 9
}
```

A bare `<const>` with no name after it is still a type parameter named `const`,
which is what it meant before this section existed.

The measurement came before the syntax, as it did for `t`: across both
repositories and the 2,507-program model corpus the word `const` occurs **four
times**, three of them inside comments and the fourth in a generated file
writing Rust. `fn f<const>(x: const)` and `struct const { … }` both compiled on
the previous compiler, so the name was in use in exactly the positions this
feature wanted, and reserving it would have deleted them.

### A const generic that does not check is an error

This is the one place const generics behave differently from ordinary generics,
and the difference is deliberate.

When an ordinary generic's body fails to check at some type, the instantiation
is discarded and the call keeps the erased body (§14). A const generic instead
**reports it**, naming both the instantiation and the reason — and the reason keeps its own
line and column, because the failure is not on the line that caused it:

```
fn f<const A: int>(x: t<A>) -> int {
    let y: t<A> = 365;              // fits a t9; does not fit a tryte
    return y as int;
}

fn main() {
    let a: t9 = 1;      io::println_int(f(a));   // 365
    let b: tryte = 1;   io::println_int(f(b));   // refused: "'f' does not check
}                       //  at A = 6: 2:38: value 365 overflows type 'tryte'
                        //  (range -364..364)"
```

Silence would be worse than a refusal here: the erased body types `t<A>` as the
unknown type, which accepts anything, so the discarded instantiation would have
been replaced by a copy that *printed 365* at a width that cannot hold it. The
strict reading is affordable because nothing can regress — the syntax is new in
this release, so the population of programs relying on the permissive one is
zero.

### What is not here

**A const argument cannot be supplied explicitly.** A struct may declare a
const parameter and its fields resolve, but there is no way to name a width
when using it:

```
struct TVec<const N: int> { data: [trit; N] }        // declares
fn main() { let v: TVec<27> = TVec { data: [0] }; }  // refused
```

A const argument would have to live in a struct type's argument list, which
holds types; a value is not one. So const generics today are a property of
**functions**, and a struct's parameter has nothing to bind it. `impl<const N:
int>` is refused for the same reason and its message points at the method,
where a parameter *is* bound from the arguments.

**A const expression is not a width.** Only a literal or a bare parameter:

```
fn f<const A: int>(x: t<A>) { let y: t<A + 1> = 1; }   // refused
```

This is the one part of B3 that genuinely needs **B4** (`const fn` and
compile-time evaluation). The enhancement plan lists B4 as a prerequisite for
the whole item; measured, a *bound* parameter is already a literal by the time
anything reads it, so no evaluator is involved in any of the above. Only an
expression over one needs it.

That makes three items in a row whose stated prerequisites were contradicted by
measuring them — C6 about B4, C3 about B3 and B4, and now B3 about B4 — and the
first where the dependency turned out to be real for a *named part* rather than
for the whole. Recorded rather than assumed, because four Phase-4 premises were
contradicted the same way.

## 26. Refinement types: `where` on a parameter

*Added 4 September 2026 (Phase 5, B6).*

A parameter may declare the interval its value lies in:

```
fn scale(x: t27 where -100 <= x <= 100) -> t27 { return x * 3; }
```

The compiler then refuses a call that cannot satisfy it:

```
fn main() { let _ = scale(500); }
// refused: "argument 1 of 'scale' is 500, which cannot satisfy
//  `where -100 <= x <= 100`. The parameter is declared to lie in -100..100."
```

### Why this is worth having on this target

T3ISA **traps** on a value that leaves the 27-trit word — it does not saturate,
and the change that made it trap rather than saturate is recorded as having
cost a correct answer in `examples/fibonacci.mt`. A trap is a runtime failure
on the backend whose whole purpose is to be the reference semantics. A bound
that can be proved at compile time is a trap that never happens.

### The forms a `where` may take

| Form | Example |
|------|---------|
| a closed interval | `where -100 <= x <= 100` |
| either side alone | `where x >= 0` · `where x <= 63` |
| strict on either side | `where 0 <= x < 10` |
| a conjunction | `where x >= -5 && x <= 5` |
| a `const` generic bound | `where 0 <= i < N` |

The last is what makes B6 and §25 compose. An array and its index can state
their relationship in the signature:

```
fn get<const N: int>(a: [int; N], i: int where 0 <= i < N) -> int { return a[i]; }
```

A bound is an integer literal or a `const` generic parameter, and nothing else.
A call, a global or arithmetic is refused by name: this checker's whole value is
that its fragment is decidable, and it says where the fragment ends rather than
guessing past it.

`where` is **contextual**, as `t` is in §24 and `move` is in §22 — it is read as
a refinement only inside a parameter list. The word occurs 389 times across both
repositories and the model corpus and not once in code; every occurrence is the
English word in a comment.

### Chained comparison is legal here and nowhere else

`-100 <= x <= 100` is refused in an ordinary expression — "comparison operators
cannot be chained" — because C's reading of it is a bug magnet. Inside a `where`
it is the standard notation for an interval and has exactly one reading, since
the middle term must be the parameter itself. The two positions are different
and the refusal stays where it earns its keep.

### Three verdicts, and only one of them is an error

The checker knows an interval for an integer literal, for a refined parameter,
for an immutable `let` bound to either, and for `+`, `-`, `*` and unary `-` over
those. Everything else is unknown.

| The argument is | Verdict |
|-----------------|---------|
| provably inside the `where` | accepted, silently |
| provably outside it, for every value | **error** |
| neither | accepted; listed by `--warn unproven-refinement` |

```
fn inner(x: int where 0 <= x <= 10) -> int { return x; }

fn a(y: int where 0 <= y <= 3) -> int { return inner(y * 3); }   // 0..9  — proven
fn b(y: int where 0 <= y <= 3) -> int { return inner(y * 5); }   // 0..15 — unproven
fn c(y: int where 0 <= y <= 3) -> int { return inner(y + 100); } // 100..103 — refused
```

The middle row is a **backlog, not a defect**: `y * 5` is correct for some
values of `y` and wrong for others, and the compiler does not know which the
program will produce. `unproven-refinement` defaults to `allow` for the reason
`literal-out-of-word` and `division-semantics` do, and
`--warn unproven-refinement` generates the list on demand.

An interval survives an immutable binding, so splitting an expression across
two lines does not lose the bound:

```
fn c2(y: int where 0 <= y <= 3) -> int { let z = y + 100; return inner(z); }
```

A `let mut` does not keep one. It can be assigned anything later and this pass
does not follow assignments, so claiming a range for it would be the checker
proving something false — which is the one thing it must never do.

### A `where` is checked where it is written

Two declarations are refused at the declaration rather than at every call:

```
fn f(x: int where 10 <= x <= 5) -> int { return x; }
fn g(x: tryte where -1000 <= x <= 1000) -> tryte { return x; }
```

The first is empty — no value satisfies it, so no call could ever be accepted.
The second reaches outside what a `tryte` holds (-364..364).

That second check runs only for a NAMED width. `int` is deliberately exempt: it
is 27 trits on T3 and 64 bits on LLVM (§21), so a bound outside 27 trits is
legal source for one backend and not the other, and refusing it here would make
this check pick a backend.

### What is not here

**A refinement is a precondition, not a postcondition.** There is no `where` on
a return type, so a function cannot promise a range to its caller — only demand
one. That is the natural next increment and nothing in this design blocks it.

**The fragment is constants, not linear arithmetic.** The recommendation calls
for "the linear-arithmetic fragment", and what is implemented is interval
arithmetic over literals and refined parameters. `x + y` where both are refined
is covered; `x < y` between two parameters is not expressible, and neither is a
bound that mentions another parameter.

**Nothing is checked at run time.** A refinement inserts no code and the emitted
programs are byte-for-byte what they were without it. A call the checker cannot
prove is compiled exactly as before, and if the value does leave the word on T3,
the trap is still what reports it.

## 27. Ternary floating point: `t27f`

*Added 4 September 2026 (Phase 5, C5).*

`t27f` is the balanced ternary floating-point format, and it is a type:

```
use std::io;

fn twice(x: t27f) -> t27f { return t27f::add(x, x); }

fn main() { io::println_int(t27f::to_int(twice(21.0t27f))); }   // 42
```

A value is `mantissa × 3^exponent`, packed into one 27-trit word as a **9-trit
exponent** and an **18-trit mantissa**. What that buys over IEEE 754 is not
subtle:

| | `t27f` | `float` (IEEE 754 double) |
|---|---|---|
| sign | inherent — a negative mantissa is a negative number | a separate sign bit |
| zero | exactly one | two, `+0.0` and `-0.0` |
| NaN | none — every 27-trit pattern is a number | many |
| denormals | none — uniform precision | yes |
| negation | exact trit flip | exact |
| range | to about 4.3 × 10⁴⁷⁰³ | to about 1.8 × 10³⁰⁸ |
| precision | 18 trits, about 8.6 decimal digits | about 15.9 decimal digits |

### The literal

A float literal with a `t27f` suffix is a ternary float:

```
let x = 3.5t27f;
io::println_float(t27f::to_float(x));   // 3.4999999883847144
```

That is not a misprint. 3.5 is not representable in 18 trits, and the error is
about 3⁻¹⁸ — the format's precision, visible. A literal that printed `3.5`
exactly would mean the suffix had quietly produced a `float`.

The suffix must be **adjacent** and **complete**: `3.5 t27f` is two tokens and
`3.5t27foo` is a float followed by an identifier. Neither is a literal.

`3.5t27f` desugars to `t27f::from_float(3.5)`, which is also what pulls the
module in — a program that writes a literal, or that names `t27f` in a
signature, needs no `use std::t27f;`.

### The conversion lattice

Everything crosses through the library, in both directions:

| From | To | Call |
|------|----|------|
| `float` | `t27f` | `t27f::from_float(f)`, or the literal suffix |
| `t27f` | `float` | `t27f::to_float(x)` |
| `t27f` | `word` | `t27f::to_int(x)` |
| exponent + mantissa | `t27f` | `t27f::from_parts(e, m)` |
| `t27f` | its parts | `t27f::exponent(x)`, `t27f::mantissa(x)` |

**`as` does not convert a `t27f`.** `x as float` is refused, and that refusal
is new: it used to be accepted and yielded the value's allocation address
reinterpreted as a double — about 3.1 × 10⁻³²⁰ for any `t27f` whatsoever. See
§28 on casts.

### What is not here

**The operators are not overloaded.** `x + y` and `x > y` on two `t27f` values
are refused by name. This language's arithmetic and comparison operators are
built in and never dispatch to a user `impl` — the unsatisfied-bound diagnostic
says so — so `t27f` has a spelling, a literal and a conversion lattice, and its
arithmetic is `t27f::add`, `t27f::sub`, `t27f::mul`.

**`mul` is less precise than `add`.** The product of two full 18-trit mantissas
needs about 3.8 × 10¹⁶ and a 27-trit word holds ±3,812,798,742,493, so both
operands are reduced before multiplying and a product carries about 13 trits
rather than 18. Full-width multiplication needs a double-word intermediate the
language does not have on T3.

**There is no `tfloat`.** It was a keyword and a type, and it was IEEE 754
double — lowered to exactly what `float` lowers to — while its name promised
this section's format. It is refused, and the message offers both honest
answers: `float` for the binary format, `t27f` for the ternary one.

## 28. Casts

*Added 4 September 2026 (Phase 5, C5 / report.txt P129).*

`as` converts between **numbers**:

```
let x: int = 7;
let f: float = x as float;
let n: int = 3.9 as int;
let t: tryte = 2 as tryte;
```

A struct, enum, array, tuple or container has no numeric value, and a cast
between one of those and anything else is refused:

```
struct P { pub v: int }
let p = P { v: 7 };
let _n = p as int;      // refused
let a: [int; 2] = [1, 2];
let _m = a as int;      // refused
```

**This refusal is new, and what it replaced was a wrong answer.** `as` was
unchecked: the compiler resolved the target type and returned it. So `p as int`
answered **63000** — the heap base, the value's allocation address — and
`p as float` the same address reinterpreted as a double. Both backends agreed,
because the mistake was upstream of where they part.

Two things are deliberately still allowed. An aggregate casts to **itself**,
which is a no-op and harmless. And a cast where either side is not yet known —
a generic parameter inside an un-instantiated body — is left alone, because
refusing there would reject a body that is correct at every instantiation.

Measured across both repositories and the 2,507-program model corpus: **0
programs** are refused by this rule that were not refused before.

## 29. Compile-time evaluation: `const fn`

*Added 4 September 2026 (Phase 5, B4).*

A function declared `const fn` may be **run by the compiler**, and its result
may be a trit width or an array length:

```
use std::io;

const fn dbl(x: int) -> int { return x * 2; }

fn main() {
    let a: [int; dbl(2)] = [1, 2, 3, 4];
    io::println_int(a[3]);       // 4
    io::println_int(dbl(21));    // 42 — the same function, at run time
}
```

`const` **adds** a capability and removes none. A `const fn` is an ordinary
function as well: checked, lowered, emitted, and callable at run time like any
other.

### Where a constant expression may be written

| Position | Example |
|----------|---------|
| a trit width | `t<A + 1>` · `t<half(18)>` |
| an array length | `[int; 2 + 1]` · `[int; N * 2]` · `[int; dbl(2)]` |

and what may appear inside one:

* integer literals;
* a `const` generic parameter (§25);
* a module-level constant — a `let` at module scope, which already had to have
  a compile-time initialiser;
* `+ - * / %`, unary `-`, comparisons and `&&` / `||`;
* calls to `const fn`, which may themselves use `let`, assignment, `if`,
  `while` and `return`.

```
const fn fact(n: int) -> int {
    let mut r = 1; let mut i = 1;
    while i <= n { r = r * i; i = i + 1; }
    return r;
}

fn main() { let a: [int; fact(3)] = [1, 2, 3, 4, 5, 6]; }
```

### A width's `>` is a bracket, never an operator

Inside `t<…>` the expression fragment stops **below comparison**, so the first
`>` can only be the closing bracket:

```
fn f<const A: int>(x: t<A>) -> int { let y: t<A + 1> = 1; return y as int; }
```

needs no braces. Rust spells the same construct `t<{A + 1}>` because its
fragment includes comparison and the `>` would otherwise be ambiguous. A width
is a **number** and never a bool, so the precedence floor settles it here
instead. The consequence is that `t<1 < 2>` is not a comparison and is refused.

### Termination is bought, not assumed

A `const fn` may loop, so evaluation carries a step budget:

```
const fn spin(n: int) -> int { let mut i = 0; while i >= 0 { i = i + 1; } return i; }
fn main() { let _a: [int; spin(1)] = [1]; }
// refused: "… did not finish within 100000 evaluation steps — a `const fn`
//  must terminate"
```

A compiler that hangs on a program is the one failure mode a compile-time
evaluator must not have, so exhausting the budget is an ordinary diagnostic.

A constant expression that is broken is reported where it is written — division
by zero, a negative length, a width outside 1…54, and a call to a function that
is not `const`.

### What is not here

**An expression cannot be inferred from.** `fn f<const A: int>(x: t<A + 1>)`
called with a `t9` would need the compiler to invert `A + 1`, and it does not.
Only a bare parameter binds a const argument; an expression is *checked*, never
solved.

**The fragment is integers.** A compile-time `float` would have to agree with
the other constant folder about rounding as well as arithmetic, and widths and
lengths are integers.

**`const` is contextual.** Only the pair `const fn` is claimed, and only inside
angle brackets is `const N: int` (§25). A bare `const` is still an ordinary
identifier — `let const = 7;` compiles — which is what the measurement asked
for: the word occurs four times across both repositories and the
2,507-program model corpus, three of them inside comments.

### On this item's stated reach

The enhancement plan lists `const fn` as a prerequisite for const generics
(§25), for C6's pattern compilation (§13), and for compile-time trit tables.
Measuring found the first two false and the third untried: a **bound** const
parameter is already a literal by the time anything reads it, and a trit
pattern's fixed trits are known at parse time. What genuinely needed this
section was one construct — `t<A + 1>`, an expression *over* a bound
parameter — which §25 recorded as its own limit and this section closes.
