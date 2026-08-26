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
| `tryte` | 3 | −13 … +13 |
| `t9` | 9 | −9841 … +9841 |
| `t27` | 27 | −3,812,798,742,493 … +3,812,798,742,493 |
| `t54` | 54 | ≈ ±1.46×10²⁵ |
| `bool3` | — | `true` (+1), `unknown` (0), `false` (−1) |

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

**These take words, not trits.** Operands must be integer types (`int`,
`tryte`, `t9`, `t27`, `t54`); `float`, `tfloat`, `bool` and `bool3` are
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
let i = f as int;    // 3
let t = i as trit;   // clamps to {−1, 0, +1}
```

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

### Await

```
let value = await task;
```

Yields control until the task completes, returns the task's result.

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

### Trait bounds

A bare `<T>` places no requirement on `T`. The function above compares two
values of it, so calling it with a type that has no ordering is meaningless —
and it used to compile clean and return the wrong answer, comparing the two
values' addresses rather than the values. Constrain the parameter instead:

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
has: `Ord`, `PartialOrd`, `Eq`, `PartialEq`, `Display`, `Debug`, `Clone`,
`Copy`, `Hash`. Structs and enums are not covered by that rule — write the
`impl`:

```
impl Ord for Point { fn cmp(self, other: Point) -> int { ... } }
```

An unsatisfied bound is reported under the `unsatisfied-bound` lint, which is
`deny` by default (see [Lint levels](#20-lint-levels)).

### Generic structs

```
struct Pair<T> {
    first: T,
    second: T,
}

let p = Pair { first: 1, second: 2 };
```

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

Suspends the current task and resumes when `t` completes.

### Channels

```
let ch: Channel<int> = channel();
ch.send(42);
let v = ch.recv();
```

Channels are MPSC (multiple producer, single consumer).

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
| `+` `-` `*` `/` `%` | `int`, `float`, `trit`, `tryte`, `t9`, `t27`, `t54` |
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

`--warn-as-error` still means "raise everything to deny".

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

`int`, `float`, `bool`, `bool3`, `trit`, `tryte`, `t9`, `t27`, `t54`,
`tfloat`, `char`, `void`, and function-pointer types.

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
| `[s, "c"]` — array literal element | no |

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

> **Note.** The array and tuple rows differ from each other, and that is a
> quirk of the implementation rather than a designed distinction. Do not rely
> on the array row.

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
