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
tand  tor  tnot  txor
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
| `int` | 64-bit signed integer | −2⁶³ … 2⁶³−1 |
| `float` | 64-bit floating point | IEEE 754 double |
| `bool` | Boolean | `true`, `false` |
| `char` | Unicode scalar value | U+0000 … U+10FFFF |
| `str` | String (UTF-8) | — |
| `void` | Unit / no value | — |

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

---

## 5. Expressions

### Arithmetic

```
a + b    a - b    a * b    a / b    a % b
-a                // unary negation
```

Integer division truncates toward zero. Division by zero is undefined behaviour
in the current emulator (returns 0).

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
| `a txor b` | exclusive or | `|a − b|` clamped to `{−1,0,+1}` |

```
let a: trit = +;
let b: trit = -;
let c = a tand b;   // -1 (false)
let d = a tor b;    // +1 (true)
let e = tnot a;     // -1
```

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

### Lambda / closure

```
let double = fn(x: int) => x * 2;
let result = double(21);   // 42
```

### Spawn

```
let task = spawn { some_work() };
```

Creates a cooperative task. Returns `Task<T>` where `T` is the block's type.

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

---

## 14. Generics

### Generic functions

```
fn max<T>(a: T, b: T) -> T {
    if a > b { a } else { b }
}
```

Call as `max(3, 7)` — the type parameter is inferred.

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
