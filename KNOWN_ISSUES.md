# Known issues

Current, honest state of the two backends. Everything listed here is a real
gap, reproducible from a clean checkout. CI runs the working set on every
push, so anything that works today cannot silently regress; this file is the
list of what does not work yet.

Last measured: 10 August 2026, after the full bug-fix campaign (see
`../report.txt` for the complete finding-by-finding record).

## Example programs

All 17 example programs compile and run to completion with exit status 0 on
BOTH backends. Verification is done by compiling to a real artifact
(`manitc compile --target t3 -o x.t3b` then `manitc run-t3 x.t3b`, and the
LLVM binary directly) and comparing actual output between the backends —
never by `run-t3 <file>.mt`, which historically executed raw source bytes
and proved nothing.

| Example | T3ISA | LLVM | Output parity |
|---|---|---|---|
| bridge_demo | works | works | byte-identical |
| capability_demo | works | works | diverges (T3 prints garbage for one PID string — see below) |
| concurrency | works | works | byte-identical |
| crypto_demo | works | works | byte-identical |
| database | works | works | byte-identical |
| data_structures | works | works | diverges in the trie section (T3 NUL-string print), and T3 takes ~4 min |
| fibonacci | works | works | diverges above fib(62) — T3 saturates at the t27 word range (by design) |
| float_demo | works | works | byte-identical |
| hello | works | works | byte-identical |
| neural_net | works | works | byte-identical |
| oop | works | works | byte-identical |
| patent_classify | works | works | byte-identical |
| stream_demo | works | works | byte-identical |
| ternary_calculator | works | works | byte-identical |
| ternary_demo | works | works | diverges in two truth-table cells (T3) |
| ternary_sort | works | works | byte-identical |
| three_valued_logic | works | works | diverges in truth-table cells (T3) |

`cargo test`: 261 passed, 0 failed, 0 ignored.

## Open issues

1. **T3 `int` arithmetic saturates at the 27-trit word range** (±3 812 798
   742 493). The T3ISA is a 27-trit machine and every ALU op clamps
   (`clamp27`), while the LLVM backend computes in 64-bit. Programs whose
   intermediate values exceed the t27 range diverge between backends
   (fibonacci beyond fib(62) is the visible case). This is an architectural
   property, not a bug; portable code — including the shipped stdlib —
   must keep intermediates inside the t27 range.

2. **T3 truth-table cell corruption (register-layout sensitive).** In
   `three_valued_logic` and `ternary_demo`, some cells of the printed
   tand/tor/txor tables are wrong on T3 (e.g. a row printing the row value
   for every column). Minimal reproductions of the same code shape
   (nested for over `[trit]`, indirect lambda call, `io::print_trit`)
   produce correct output; the corruption appears only in the full
   examples' register/stack layout. Suspected residual back-edge register
   reconciliation gap in the T3 emitter (the general mechanism was added
   for syscalls in loops; some non-syscall relocation path likely remains).

3. **T3 prints garbage/NUL runs for some computed strings** in
   `capability_demo` (one PID line) and the `data_structures` trie section.
   The string address being printed is not a registered string object, so
   the emulator's null-terminated-memory fallback dumps raw memory.
   Same class as issue 2 — a wrong register/stack value used as a string
   handle in large functions.

4. **The T3 emulator is slow on allocation-heavy programs**:
   `data_structures` takes ~4 minutes (interpreted, debug build, close to
   the 10M-step budget). Purely a performance matter.

5. **Loop-body array allocations on T3 are iteration-scoped.** The T3
   backend reuses a loop body's stack allocations each iteration, so an
   array created inside a loop must not be stored/aliased past its
   iteration (the stdlib hoists its buffers accordingly; array-returning
   calls and struct array fields are deep-copied at call sites). User code
   that keeps a pointer to a loop-local array across iterations will read
   clobbered data on T3.

6. **No free/destroy API — leak by design.** Vec/Map/Set/Deque/Trie/
   Channel/Mutex and most string-returning runtime functions allocate and
   are never freed. Fine for the short-lived demo programs; a real
   allocator interface is future work.

## Fixed since the initial release (summary)

The August 2026 campaign closed all 116 findings of the full review plus
the original eight known issues (K1–K8) recorded here. Highlights:

- `run-t3` validates/auto-compiles its input and propagates main's return
  value as the process exit status (K6); SIGPIPE exits quietly (K7).
- The LLVM backend emits valid IR for all examples, links with a clear
  diagnostic when the minimal runtime lacks gui/net (K8), and no longer
  emits illegal casts or unhonored vararg ABIs.
- `std::bridge`, `std::crypto`, and `std::t27f` are fully implemented in
  ManiT and compiled into any program that imports them (they previously
  had no implementation on either backend).
- `print(...)` is a variadic line-printer on both backends; `fmt::format`
  substitutes values (not addresses) identically on both backends.
- String concatenation, string-keyed Map/Set, tryte printing, and module
  globals now work on T3.
- `bool → bool3` coercion produces `false`, not `unknown`.
- Concurrency: sync handles (Mutex, Channel, Task, …) are Copy;
  the mutex is recursive so guard-held accessors don't self-deadlock;
  select/await/barrier/atomics have matching semantics on both backends.

© Manish Jagdish Thatte
