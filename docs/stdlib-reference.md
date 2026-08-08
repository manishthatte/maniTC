# Standard Library Reference

The maniT standard library provides modules for I/O, formatting, mathematics,
ternary operations, collections, synchronisation, async, time, networking, file
system, environment, and strings. Modules are declared with `use std::module_name`.

All stdlib functions are pre-registered in the semantic analyser. On the T3 backend
they are handled as emulator syscalls; on the LLVM backend they map to C runtime
calls or intrinsics.

---

## std::io

Basic input/output.

| Function | Signature | Description |
|----------|-----------|-------------|
| `io::print` | `(s: str)` | Print a string without a newline |
| `io::println` | `(s: str)` | Print a string followed by a newline |
| `io::print_int` | `(n: int)` | Print an integer in decimal |
| `io::print_float` | `(f: float)` | Print a float in decimal |
| `io::print_char` | `(c: char)` | Print a single character |
| `io::print_trit` | `(t: trit)` | Print `+`, `0`, or `-` |
| `io::print_bool3` | `(b: bool3)` | Print `true`, `unknown`, or `false` |
| `io::print_tryte` | `(t: tryte)` | Print the tryte value as an integer |
| `io::newline` | `()` | Print a newline |
| `io::read_line` | `() -> str` | Read a line from stdin (stub) |
| `io::read_int` | `() -> int` | Read an integer from stdin (stub) |

### Usage

```maniT
use std::io;

fn main() {
    io::println("Hello, world!");
    io::print("Value: ");
    io::print_int(42);
    io::newline();

    let t: trit = +;
    io::print_trit(t);    // prints "+"

    let b: bool3 = unknown;
    io::print_bool3(b);   // prints "unknown"
}
```

---

## std::fmt

String formatting and text utilities.

| Function | Signature | Description |
|----------|-----------|-------------|
| `fmt::format` | `(template: str, ...) -> str` | Format a string (printf-style, `{}` placeholders) |
| `fmt::show_int` | `(n: int) -> str` | Integer to string |
| `fmt::show_float` | `(f: float) -> str` | Float to string |
| `fmt::show_bool` | `(b: bool) -> str` | Bool to `"true"` or `"false"` |
| `fmt::show_trit` | `(t: trit) -> str` | Trit to `"+"`, `"0"`, or `"-"` |
| `fmt::show_bool3` | `(b: bool3) -> str` | Bool3 to `"true"`, `"unknown"`, or `"false"` |
| `fmt::concat` | `(a: str, b: str) -> str` | Concatenate two strings |
| `fmt::repeat` | `(s: str, n: int) -> str` | Repeat string n times |
| `fmt::align_left` | `(s: str, width: int) -> str` | Left-align in a field of given width |
| `fmt::align_right` | `(s: str, width: int) -> str` | Right-align in a field of given width |
| `fmt::pad_zeros` | `(n: int, width: int) -> str` | Zero-pad integer |
| `fmt::to_upper` | `(s: str) -> str` | Uppercase |
| `fmt::to_lower` | `(s: str) -> str` | Lowercase |

### Usage

```maniT
use std::fmt;

let s = fmt::format("Point({}, {})", x, y);
let padded = fmt::align_right(fmt::show_int(42), 8);   // "      42"
```

---

## std::math

Mathematical functions and ternary conversions.

| Function | Signature | Description |
|----------|-----------|-------------|
| `math::abs` | `(n: int) -> int` | Absolute value |
| `math::abs_float` | `(f: float) -> float` | Float absolute value |
| `math::sqrt` | `(f: float) -> float` | Square root |
| `math::pow` | `(base: float, exp: float) -> float` | Exponentiation |
| `math::log` | `(f: float) -> float` | Natural logarithm |
| `math::log2` | `(f: float) -> float` | Base-2 logarithm |
| `math::log3` | `(f: float) -> float` | Base-3 logarithm (ternary-native) |
| `math::floor` | `(f: float) -> float` | Floor |
| `math::ceil` | `(f: float) -> float` | Ceiling |
| `math::round` | `(f: float) -> float` | Round to nearest |
| `math::min` | `(a: int, b: int) -> int` | Minimum |
| `math::max` | `(a: int, b: int) -> int` | Maximum |
| `math::clamp` | `(v: int, lo: int, hi: int) -> int` | Clamp |
| `math::sin` | `(f: float) -> float` | Sine (radians) |
| `math::cos` | `(f: float) -> float` | Cosine (radians) |
| `math::to_balanced_ternary` | `(n: int) -> str` | Int → balanced ternary string |
| `math::from_balanced_ternary` | `(s: str) -> int` | Balanced ternary string → int |
| `math::trit_count` | `(n: int) -> int` | Trits needed to represent n |

### Usage

```maniT
use std::math;

let d = math::sqrt(2.0);            // 1.41421…
let s = math::to_balanced_ternary(8);   // "+0-"
let n = math::from_balanced_ternary("+0-");  // 8
let t = math::trit_count(100);          // 5
```

---

## std::ternary

Ternary-specific arithmetic and conversion functions.

| Function | Signature | Description |
|----------|-----------|-------------|
| `ternary::trit_to_int` | `(t: trit) -> int` | Trit to integer (−1, 0, +1) |
| `ternary::int_to_trit` | `(n: int) -> trit` | Integer to trit (clamps) |
| `ternary::tryte_to_int` | `(t: tryte) -> int` | Tryte to integer |
| `ternary::t27_to_int` | `(t: t27) -> int` | t27 to integer |
| `ternary::pack_trits` | `(v: Vec<trit>) -> t27` | Pack trit array into t27 word |
| `ternary::unpack_trits` | `(t: t27, n: int) -> Vec<trit>` | Unpack n trits from t27 |
| `ternary::trit_at` | `(t: t27, pos: int) -> trit` | Extract single trit |
| `ternary::trit_median` | `(a: trit, b: trit, c: trit) -> trit` | Majority vote of three trits |
| `ternary::t27_add` | `(a: t27, b: t27) -> t27` | Balanced ternary addition |
| `ternary::t27_mul` | `(a: t27, b: t27) -> t27` | Balanced ternary multiplication |
| `ternary::t27_neg` | `(a: t27) -> t27` | Ternary negation |
| `ternary::consensus` | `(a: trit, b: trit) -> trit` | Unanimous agreement (0 if disagree) |

### Usage

```maniT
use std::ternary;

let trits: Vec<trit> = [+, 0, -, +];
let packed: t27 = ternary::pack_trits(trits);
let unpacked = ternary::unpack_trits(packed, 4);

let t = ternary::trit_at(0t+0-, 0);   // -1 (least significant)
let med = ternary::trit_median(+, +, -);  // + (majority)
```

---

## std::collections

Data structure constructors and methods.

### Vec<T>

Dynamic growable array.

| Method | Signature | Description |
|--------|-----------|-------------|
| `Vec::new()` | `() -> Vec<T>` | Create empty vec |
| `.push(v)` | `(T)` | Append element |
| `.pop()` | `() -> T` | Remove and return last element |
| `.len()` | `() -> int` | Number of elements |
| `.get(i)` | `(int) -> T` | Element at index (panics if OOB) |
| `.set(i, v)` | `(int, T)` | Set element at index |
| `.remove(i)` | `(int)` | Remove element at index |
| `.contains(v)` | `(T) -> bool` | Linear search |
| `.sort()` | `()` | Sort in ascending order |
| `.reverse()` | `()` | Reverse in place |
| `.map(f)` | `(fn(T)->U) -> Vec<U>` | Apply function to each element |
| `.filter(f)` | `(fn(T)->bool) -> Vec<T>` | Keep elements matching predicate |
| `.fold(init, f)` | `(U, fn(U,T)->U) -> U` | Left fold |
| `.slice(lo, hi)` | `(int, int) -> Vec<T>` | Sub-vector [lo, hi) |

### Map<K, V>

Hash map.

| Method | Signature | Description |
|--------|-----------|-------------|
| `Map::new()` | `() -> Map<K,V>` | Create empty map |
| `.insert(k, v)` | `(K, V)` | Insert or overwrite |
| `.get(k)` | `(K) -> V` | Get value (panics if absent) |
| `.get_or(k, default)` | `(K, V) -> V` | Get or return default |
| `.contains_key(k)` | `(K) -> bool` | Key membership test |
| `.remove(k)` | `(K)` | Delete key |
| `.len()` | `() -> int` | Number of entries |
| `.keys()` | `() -> Vec<K>` | All keys |
| `.values()` | `() -> Vec<V>` | All values |

### Set<T>

Hash set.

| Method | Signature | Description |
|--------|-----------|-------------|
| `Set::new()` | `() -> Set<T>` | Create empty set |
| `.insert(v)` | `(T)` | Add element |
| `.contains(v)` | `(T) -> bool` | Membership test |
| `.remove(v)` | `(T)` | Delete element |
| `.len()` | `() -> int` | Size |
| `.to_vec()` | `() -> Vec<T>` | Convert to vector |
| `.intersection(other)` | `(Set<T>) -> Set<T>` | Set intersection |
| `.union(other)` | `(Set<T>) -> Set<T>` | Set union |
| `.difference(other)` | `(Set<T>) -> Set<T>` | Set difference |
| `.is_subset(other)` | `(Set<T>) -> bool` | Subset test |

### Deque<T>

Double-ended queue.

| Method | Signature | Description |
|--------|-----------|-------------|
| `Deque::new()` | `() -> Deque<T>` | Create empty deque |
| `.push_front(v)` | `(T)` | Prepend |
| `.push_back(v)` | `(T)` | Append |
| `.pop_front()` | `() -> T` | Remove and return front |
| `.pop_back()` | `() -> T` | Remove and return back |
| `.front()` | `() -> T` | Peek at front |
| `.back()` | `() -> T` | Peek at back |
| `.len()` | `() -> int` | Size |
| `.is_empty()` | `() -> bool` | Empty test |

### TernaryTrie<V>

Prefix tree indexed by sequences of trits. Native to balanced ternary.

| Method | Signature | Description |
|--------|-----------|-------------|
| `TernaryTrie::new()` | `() -> TernaryTrie<V>` | Create empty trie |
| `.insert(key, val)` | `(Vec<trit>, V)` | Insert at key |
| `.get(key)` | `(Vec<trit>) -> V` | Lookup by trit key |
| `.contains(key)` | `(Vec<trit>) -> bool` | Key membership |
| `.keys_with_prefix(prefix)` | `(Vec<trit>) -> Vec<Vec<trit>>` | Prefix query |
| `.len()` | `() -> int` | Size |

---

## std::sync

Synchronisation primitives for cooperative multitasking.

| Construct | Constructor | Methods |
|-----------|------------|---------|
| `Mutex<T>` | `Mutex::new(initial: T)` | `.lock()`, `.unlock()`, `.get() -> T`, `.set(v: T)` |
| `AtomicTrit` | `AtomicTrit::new(t: trit)` | `.get() -> trit`, `.set(t: trit)`, `.cas(expected, desired) -> bool` |
| `Barrier` | `Barrier::new(n: int)` | `.wait()` — blocks until n tasks have called wait() |
| `Semaphore` | `Semaphore::new(count: int)` | `.acquire()`, `.release()` |

### Usage

```maniT
use std::sync;

// Mutex: protect shared state
let counter: Mutex<int> = Mutex::new(0);
counter.lock();
let v = counter.get();
counter.set(v + 1);
counter.unlock();

// AtomicTrit: lock-free flag
let status: AtomicTrit = AtomicTrit::new(0);
status.set(+);

// Barrier: synchronise N tasks
let b: Barrier = Barrier::new(3);
spawn { do_work_a(); b.wait(); };
spawn { do_work_b(); b.wait(); };
b.wait();   // all three meet here

// Semaphore: rate limiting
let sem: Semaphore = Semaphore::new(2);
sem.acquire();    // at most 2 tasks past here simultaneously
do_critical();
sem.release();
```

---

## std::async

Async/await cooperative scheduler.

| Function | Signature | Description |
|----------|-----------|-------------|
| `async::yield_now` | `()` | Yield to the scheduler (cooperative preemption point) |
| `async::sleep` | `(ms: int)` | Yield for approximately `ms` milliseconds |
| `async::spawn_task` | `(fn() -> T) -> Task<T>` | Spawn a named function as a task |
| `async::select` | `(task_a: Task, ...) -> T` | Wait for the first task to complete |

`spawn { block }` is syntax sugar for creating and scheduling an anonymous task.
`await task` suspends the current task until `task` completes and returns the value.

### Usage

```maniT
use std::async;

async fn worker(id: int) -> int {
    async::sleep(10 * id);
    id * id
}

fn main() {
    let t1 = worker(1);
    let t2 = worker(2);
    let t3 = worker(3);

    let first = async::select(t1, t2, t3);
    io::println("First result:");
    io::print_int(first);
    io::newline();
}
```

---

## std::str

String operations.

| Function | Signature | Description |
|----------|-----------|-------------|
| `str::len` | `(s: str) -> int` | Length in bytes |
| `str::char_at` | `(s: str, i: int) -> char` | Character at position |
| `str::concat` | `(a: str, b: str) -> str` | Concatenate |
| `str::substr` | `(s: str, lo: int, hi: int) -> str` | Substring [lo, hi) |
| `str::starts_with` | `(s: str, prefix: str) -> bool` | Prefix test |
| `str::ends_with` | `(s: str, suffix: str) -> bool` | Suffix test |
| `str::contains` | `(s: str, sub: str) -> bool` | Substring search |
| `str::find` | `(s: str, sub: str) -> int` | First occurrence index (−1 if absent) |
| `str::replace` | `(s: str, from: str, to: str) -> str` | Replace all occurrences |
| `str::split` | `(s: str, delim: str) -> Vec<str>` | Split by delimiter |
| `str::trim` | `(s: str) -> str` | Trim whitespace |
| `str::parse_int` | `(s: str) -> Result<int, str>` | Parse integer |
| `str::parse_float` | `(s: str) -> Result<float, str>` | Parse float |
| `str::from_int` | `(n: int) -> str` | Integer to string |
| `str::from_char` | `(c: char) -> str` | Char to string |
| `str::to_upper` | `(s: str) -> str` | Uppercase |
| `str::to_lower` | `(s: str) -> str` | Lowercase |

---

## std::fs

File system access.

| Function | Signature | Description |
|----------|-----------|-------------|
| `fs::read_file` | `(path: str) -> Result<str, str>` | Read entire file as string |
| `fs::write_file` | `(path: str, content: str) -> Result<void, str>` | Write string to file |
| `fs::append_file` | `(path: str, content: str) -> Result<void, str>` | Append to file |
| `fs::exists` | `(path: str) -> bool` | Check if path exists |
| `fs::delete` | `(path: str) -> Result<void, str>` | Delete file |
| `fs::list_dir` | `(path: str) -> Result<Vec<str>, str>` | List directory entries |
| `fs::open` | `(path: str, mode: str) -> Result<int, str>` | Open file, return handle |
| `fs::close` | `(handle: int)` | Close file handle |
| `fs::read_line` | `(handle: int) -> Result<str, str>` | Read one line |
| `fs::write` | `(handle: int, s: str) -> Result<void, str>` | Write to handle |

---

## std::net

Network stubs (not yet implemented in the emulator).

| Function | Description |
|----------|-------------|
| `net::tcp_connect(host: str, port: int) -> Result<int, str>` | TCP connect; returns socket handle |
| `net::tcp_listen(port: int) -> Result<int, str>` | TCP listen |
| `net::tcp_accept(handle: int) -> Result<int, str>` | Accept connection |
| `net::send(handle: int, data: str) -> Result<int, str>` | Send data |
| `net::recv(handle: int, max: int) -> Result<str, str>` | Receive data |
| `net::close(handle: int)` | Close socket |

---

## std::time

Time utilities.

| Function | Signature | Description |
|----------|-----------|-------------|
| `time::now` | `() -> int` | Current time in milliseconds since epoch |
| `time::sleep` | `(ms: int)` | Busy-wait for ms milliseconds |
| `time::format` | `(ms: int) -> str` | Format timestamp as HH:MM:SS |

---

## std::env

Environment and process.

| Function | Signature | Description |
|----------|-----------|-------------|
| `env::get` | `(name: str) -> Result<str, str>` | Get environment variable |
| `env::set` | `(name: str, val: str)` | Set environment variable |
| `env::args` | `() -> Vec<str>` | Command-line arguments |
| `env::exit` | `(code: int)` | Exit process |
