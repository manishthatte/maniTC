# Example programs walkthrough

The `examples/` directory contains **seventeen** programs. This document walks
through seven of them, chosen to cover the range of maniT language features.
Each is self-contained and compiles against both backends.

> **Corrected 30 August 2026.** This paragraph previously read *"The
> `examples/` directory contains seven programs"*. It contains seventeen —
> counted on disk rather than from the list below. The count was true when this
> document was written and the other ten arrived without it being revisited;
> nothing said here about the seven that ARE walked through was wrong, which is
> why prose review does not catch this class. The ten with no section below are
> `bridge_demo`, `capability_demo`, `crypto_demo`, `database`, `float_demo`,
> `neural_net`, `patent_classify`, `stream_demo`, `ternary_calculator` and
> `ternary_sort` — every one of them shipped and a subject of the parity
> matrix, and `ternary_sort` is one of the four examples whose `.t3l` is
> tracked. `docs/index.md` carried the same count and additionally called this
> a walkthrough of *all* of them; it now says seven of seventeen. Both claims
> are pinned by
> `tests/audit_regression_tests.rs::examples_walkthrough_matches_the_examples_directory`,
> because a count written into prose goes stale in silence.
>
> **Added 1 September 2026.** Sweeping the same shape across the repository
> found two more stale counts *in this document*: `fibonacci.mt` was billed at
> 139 lines and measures 148, and `three_valued_logic.mt` at 490 against 510.
> The other five `**File:**` counts here are exact. That the correction above
> did not prompt a check of the seven line counts twelve lines below it is the
> repository's own rule — *a fix is not done when the reported site is fixed* —
> applied to the fix that recorded it. All seven are now pinned by
> `tests/audit_regression_tests.rs::documented_line_counts_match_the_source_files`.

```bash
# Compile and run any example
manitc compile --target t3 examples/<name>.mt
manitc run-t3 a.t3b
```

---

## hello.mt — First program and ternary basics

**File:** `examples/hello.mt` (103 lines)

The canonical entry point. Demonstrates the most fundamental maniT constructs.

### Key features shown

**Basic I/O:**

```maniT
use std::io;
fn main() {
    io::println("Hello from maniT!");
}
```

**Trit literals and tif:**

```maniT
let t: trit = +;
tif t {
    + => io::println("Trit is positive (+1)"),
    0 => io::println("Trit is zero (0)"),
    - => io::println("Trit is negative (-1)"),
}
```

`tif` is the ternary equivalent of `if`. The three arms `+`, `0`, `-` cover
all possible trit values — no default is needed.

**bool3 (three-valued boolean):**

```maniT
let sensor_ok: bool3 = true;
let sensor_unknown: bool3 = unknown;
let sensor_fail: bool3 = false;
```

`bool3` represents `true` (+1), `unknown` (0), or `false` (−1). Useful for
sensor data, partial knowledge, and Kleene logic.

**Balanced ternary literal:**

```maniT
let n = 0t+0-;   // 8 in decimal
io::print_int(n);
```

`0t+0-` means (+1)×9 + (0)×3 + (−1)×1 = 8.

**Result type:**

```maniT
let r: Result<int, str> = Ok(42);
match r {
    Ok(v) => { io::print("Ok("); io::print_int(v); io::print(")"); },
    Unknown(msg) => { io::print("Unknown(\""); io::print(msg); io::print("\")"); },
    Err(e) => { io::print("Err(\""); io::print(e); io::print("\")"); },
}
```

`Result<T, E>` has three states instead of the usual two. `Unknown` is for
pending or indeterminate computations (like "not yet computed" or "divison
by zero in ternary context").

---

## fibonacci.mt — Algorithms and Result

**File:** `examples/fibonacci.mt` (148 lines)

Demonstrates multiple implementations of the same algorithm and the use of
`Result` for safe computation.

### Recursive Fibonacci

```maniT
fn fib_recursive(n: int) -> int {
    if n <= 1 { return n; }
    fib_recursive(n - 1) + fib_recursive(n - 2)
}
```

Simple recursion. O(2^n) — impractical for n > 30 but illustrative.

### Iterative Fibonacci

```maniT
fn fib_iterative(n: int) -> int {
    if n <= 1 { return n; }
    let mut a = 0;
    let mut b = 1;
    let mut i = 2;
    while i <= n {
        let tmp = a + b;
        a = b;
        b = tmp;
        i = i + 1;
    }
    b
}
```

`let mut` declares a mutable binding. Variables are immutable by default.

### Safe Fibonacci with overflow detection

```maniT
fn fib_safe(n: int) -> Result<int, str> {
    // ... iterative with overflow check
    if result < 0 {
        return Unknown("overflow: exceeds 64-bit int range");
    }
    Ok(result)
}
```

`Result<int, str>` propagates failure. The `Unknown` variant signals
"this computation couldn't produce a definitive answer."

### Golden ratio convergence

```maniT
// F(n+1)/F(n) converging to φ ≈ 1.618...
let prev = fib_iterative(n - 1) as float;
let curr = fib_iterative(n) as float;
let ratio = curr / prev;
```

`as float` casts an integer to float. The `as` keyword handles type coercion.

---

## ternary_demo.mt — Balanced ternary in depth

**File:** `examples/ternary_demo.mt` (359 lines)

The definitive reference for balanced ternary features.

### Type sizes

| Type | Trits | Range |
|------|-------|-------|
| `trit` | 1 | −1, 0, +1 |
| `tryte` | 3 | −13 … +13 |
| `t9` | 9 | −9,841 … +9,841 |
| `t27` | 27 | −3,812,798,742,493 … +3,812,798,742,493 |

```maniT
let small: tryte = 0t+0-;   // 8 as a 3-trit word
let medium: t9 = 0t+0-0+0;  // 9-trit word
```

### Ternary operators

```maniT
let a: trit = +;
let b: trit = -;

let c = a tand b;   // min(+1, -1) = -1
let d = a tor  b;   // max(+1, -1) = +1
let e = tnot a;     // -a = -1
let f = a txor b;   // |+1 - (-1)| clamped = +1
```

These implement Łukasiewicz three-valued logic, the standard for balanced
ternary computation.

### Pack and unpack trits

```maniT
use std::ternary;

let trits: Vec<trit> = [+, 0, -, +, 0];
let packed: t27 = ternary::pack_trits(trits);
let unpacked = ternary::unpack_trits(packed, 5);
```

Converting between trit arrays and compact word values. The pack operation
places trit[0] in the least significant position.

### Ternary conversion

```maniT
use std::math;

let n = 42;
let s = math::to_balanced_ternary(n);   // "++-0" (balanced ternary string)
let r = math::from_balanced_ternary(s); // 42
```

Useful for display and for interfacing with external ternary hardware.

### Storage efficiency

Balanced ternary stores more information per digit than binary:
- 1 trit = 1.585 bits of information
- A 27-trit word ≈ 42.7 bits
- Stored as i64 (64 bits) — only 67% overhead vs. dedicated ternary hardware

---

## three_valued_logic.mt — Kleene logic

**File:** `examples/three_valued_logic.mt` (510 lines)

Deep dive into three-valued logic theory with practical applications.

### The three values

In `bool3`:
- `true` (+1) — definitely true
- `unknown` (0) — unknown, uncertain, or indeterminate
- `false` (−1) — definitely false

### Truth tables

```
tand (Łukasiewicz conjunction = minimum):
  +  tand  + = +
  +  tand  0 = 0
  +  tand  - = -
  0  tand  0 = 0
  0  tand  - = -
  -  tand  - = -

tor (Łukasiewicz disjunction = maximum):
  +  tor  + = +
  +  tor  0 = +
  +  tor  - = +
  0  tor  0 = 0
  0  tor  - = 0
  -  tor  - = -
```

### Law of excluded middle fails in 3VL

```maniT
let a: trit = 0;         // unknown
let lm = a tor (tnot a); // max(0, -0) = max(0, 0) = 0  ≠ +1
```

In classical logic, `a || !a` is always true. In three-valued logic, if `a`
is unknown, so is `a || !a`. This is fundamental to epistemic reasoning.

### De Morgan's laws hold

```maniT
// tnot (a tand b) = (tnot a) tor (tnot b)
let lhs = tnot (a tand b);
let rhs = (tnot a) tor (tnot b);
// lhs == rhs for all a, b ∈ {+1, 0, -1}
```

### Sensor fusion example

```maniT
let engine_ok: bool3 = true;
let brakes_ok: bool3 = unknown;   // sensor reading uncertain
let tires_ok:  bool3 = true;

// Safe to proceed only if ALL systems are known-OK
let safe = engine_ok tand brakes_ok tand tires_ok;
// unknown — one uncertain system makes the whole uncertain
```

This models real-world scenarios where you can't proceed unless you have
positive confirmation from every sensor.

### Majority voting (Triple Modular Redundancy)

```maniT
use std::ternary;

let v1 = ternary::trit_to_int(sensor1) as i8;
let v2 = ternary::trit_to_int(sensor2) as i8;
let v3 = ternary::trit_to_int(sensor3) as i8;

let majority = ternary::trit_median(v1, v2, v3);
```

`trit_median` returns the median of three trits — the "majority vote". If
two sensors agree, the majority wins. This is standard in fault-tolerant systems.

---

## data_structures.mt — Collections

**File:** `examples/data_structures.mt` (431 lines)

Comprehensive demonstration of the standard collection types.

### Vec<T>

```maniT
let mut v: Vec<int> = Vec::new();
v.push(10);
v.push(20);
v.push(30);

let doubled = v.map(fn(x: int) => x * 2);   // [20, 40, 60]
let evens = v.filter(fn(x: int) => x % 2 == 0);
let sum = v.fold(0, fn(acc: int, x: int) => acc + x);
```

### Map<K, V>

```maniT
let mut freq: Map<str, int> = Map::new();
for word in words {
    let count = if freq.contains_key(word) { freq.get(word) } else { 0 };
    freq.insert(word, count + 1);
}
```

### Set<T>

```maniT
let mut primes: Set<int> = Set::new();
primes.insert(2);
primes.insert(3);

let mut fibs: Set<int> = Set::new();
fibs.insert(1);
fibs.insert(2);
fibs.insert(3);

let both = primes.intersection(fibs);   // {2, 3}
```

### Deque<T>

```maniT
let mut stack: Deque<int> = Deque::new();
stack.push_back(1);
stack.push_back(2);
stack.push_back(3);

let top = stack.pop_back();    // 3 — LIFO behaviour
```

`Deque` supports both ends efficiently. Use `push_back` + `pop_back` for a
stack, or `push_back` + `pop_front` for a queue.

### TernaryTrie<V>

A balanced ternary native data structure — a prefix tree indexed by sequences
of trits.

```maniT
let mut trie: TernaryTrie<str> = TernaryTrie::new();

// Keys are Vec<trit>
let key1: Vec<trit> = [+, 0, -];   // represents balanced ternary "+0-"
trie.insert(key1, "value_one");

// Prefix queries — return all keys that start with the prefix
let prefix: Vec<trit> = [+];
let matches = trie.keys_with_prefix(prefix);
```

`TernaryTrie` is especially useful for:
- Routing tables indexed by ternary addresses
- Autocompletion over ternary keyspaces
- Ternary number decomposition

---

## oop.mt — Object-oriented programming

**File:** `examples/oop.mt` (301 lines)

Demonstrates maniT's OOP facilities: structs, impl blocks, traits, and enums
with methods.

### Struct with methods

```maniT
struct Point {
    pub x: float,
    pub y: float,
}

impl Point {
    fn add(self, other: Point) -> Point {
        Point { x: self.x + other.x, y: self.y + other.y }
    }

    fn scale(self, factor: float) -> Point {
        Point { x: self.x * factor, y: self.y * factor }
    }

    fn to_str(self) -> str {
        fmt::format("({}, {})", self.x, self.y)
    }
}
```

Method calls use `.` syntax:

```maniT
let p1 = Point { x: 1.0, y: 2.0 };
let p2 = Point { x: 3.0, y: 4.0 };
let sum = p1.add(p2);         // (4.0, 6.0)
let scaled = p1.scale(2.0);   // (2.0, 4.0)
```

### Traits

```maniT
trait Describable {
    fn describe(self) -> str;
}

trait Scalable {
    fn scale(self, factor: float) -> Self;
    fn magnitude(self) -> float;
}
```

Traits define interfaces. Any type can implement them:

```maniT
impl Describable for Rectangle {
    fn describe(self) -> str {
        fmt::format("Rectangle({}x{})", self.width, self.height)
    }
}

impl Scalable for Rectangle {
    fn scale(self, factor: float) -> Self {
        Rectangle {
            width:  (self.width as float * factor) as int,
            height: (self.height as float * factor) as int,
        }
    }
    fn magnitude(self) -> float {
        (self.width * self.height) as float
    }
}
```

### Generic functions

```maniT
fn identity<T>(x: T) -> T { x }
fn max<T>(a: T, b: T) -> T { if a > b { a } else { b } }
fn clamp<T>(v: T, lo: T, hi: T) -> T {
    if v < lo { lo } elif v > hi { hi } else { v }
}
```

Called as:

```maniT
let n = identity(42);          // T inferred as int
let m = max(7, 3);             // 7
let c = clamp(15, 0, 10);     // 10
```

### Enum with impl methods

```maniT
enum Direction { North, South, East, West }

impl Direction {
    fn opposite(self) -> Direction {
        match self {
            Direction::North => Direction::South,
            Direction::South => Direction::North,
            Direction::East  => Direction::West,
            Direction::West  => Direction::East,
        }
    }

    fn to_str(self) -> str {
        match self {
            Direction::North => "North",
            Direction::South => "South",
            Direction::East  => "East",
            Direction::West  => "West",
        }
    }
}
```

Usage:

```maniT
let d = Direction::North;
io::println(d.to_str());            // "North"
io::println(d.opposite().to_str()); // "South"
```

Enum variant expressions (`Direction::North`) evaluate to integer indices
(0, 1, 2, …) at runtime. The `match` arms compare the scrutinee against the
correct index automatically.

---

## concurrency.mt — Cooperative multitasking

**File:** `examples/concurrency.mt` (360 lines)

maniT's concurrency model is cooperative and single-threaded. All tasks run on
one thread; a task runs until it explicitly yields or awaits.

### Producer-consumer with channels

```maniT
let ch: Channel<int> = channel();

let producer = spawn {
    let mut i = 0;
    while i < 5 {
        ch.send(i * i);
        async::yield_now();
        i = i + 1;
    }
};

let consumer = spawn {
    let mut count = 0;
    while count < 5 {
        let v = ch.recv();
        io::print_int(v);
        io::newline();
        count = count + 1;
    }
};

await producer;
await consumer;
```

`spawn { ... }` creates a task. `await task` waits for it to finish.
`async::yield_now()` cooperatively yields to the scheduler.

### Shared state with Mutex

```maniT
let counter: Mutex<int> = Mutex::new(0);

let t1 = spawn {
    let mut i = 0;
    while i < 100 {
        counter.lock();
        let v = counter.get();
        counter.set(v + 1);
        counter.unlock();
        async::yield_now();
        i = i + 1;
    }
};
```

`Mutex<T>` wraps a value with a lock. Since maniT is single-threaded and
cooperative, the mutex is a logical construct that prevents interleaved access
between yield points.

### AtomicTrit for lock-free flags

```maniT
let status: AtomicTrit = AtomicTrit::new(0);

let task = spawn {
    status.set(-);  // starting
    do_work();
    status.set(0);  // running
    more_work();
    status.set(+);  // done
};

// Poll in main
while status.get() != + {
    async::yield_now();
}
```

`AtomicTrit` has three states (−1, 0, +1) without needing a mutex. Perfect for
lifecycle flags.

### Barrier synchronisation

```maniT
let b: Barrier = Barrier::new(3);

let t1 = spawn { do_phase_one(); b.wait(); };
let t2 = spawn { do_phase_one(); b.wait(); };
b.wait();   // main waits at the barrier too

// All three have now completed phase one
```

`Barrier::new(n)` creates a barrier that releases when exactly `n` tasks have
called `.wait()`.

### Async/await with select

```maniT
async fn fast_query() -> str { async::sleep(10); "fast" }
async fn slow_query() -> str { async::sleep(100); "slow" }

let first = async::select(fast_query(), slow_query());
io::println(first);   // "fast"
```

`async::select` returns the value of whichever task completes first, discarding
the others.

### Semaphore rate limiting

```maniT
fn run_workers() {
    let sem: Semaphore = Semaphore::new(2);   // at most 2 concurrent workers
    mut id = 0;
    while id < 5 {
        let wid = id;
        spawn {
            sem.acquire();
            do_limited_work(wid);
            sem.release();
        }
        id = id + 1;
    }
}
```

`Semaphore::new(n)` allows at most `n` tasks past `acquire()` simultaneously.

> **Corrected 2 September 2026 (report.txt P108).** This example used to put
> the `let` at MODULE level, above `fn worker`, and **a reader who copied it
> got an error**: a module-level `let` is stored as a single word written
> before `main` runs, so its initialiser must be a compile-time constant, and
> `Semaphore::new(2)` is a call. The semaphore is created inside a function
> here and reached by the spawned blocks, which is §11.2's copy of the store
> and how `examples/concurrency.mt` does it.
>
> Found by compiling every fenced block in `docs/` rather than reading them.
> **The population is exactly one** — it is the only module-level `let` with a
> non-constant initialiser in any documented block.
