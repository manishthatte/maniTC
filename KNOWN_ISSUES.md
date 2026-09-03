# Known issues

Current, honest state of the two backends. Everything listed here is a real
gap, reproducible from a clean checkout. CI runs the working set on every
push, so anything that works today cannot silently regress; this file is the
list of what does not work yet.

Last measured: **11 August 2026**, after a differential-testing pass that ran
all 17 examples and all 3 benchmarks through both backends and compared their
output byte for byte. That pass found two silent-wrong-answer defects that the
test suite had been green over — one of them recorded *as the expected output*
in a golden file — and both are now fixed with regression tests (issues 1 and
3 below).

**18 of the 20 programs are byte-identical across the two backends.** The two
that are not are characterised exactly, below.

> **Note, 2 September 2026.** Issue 5 below is **fixed** and its "one remaining
> cross-backend divergence" billing is stale; see the dated notice there. The
> divergence still open is **P111** in `../report.txt`, which is a different
> program shape and was found on 2 September.

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
| capability_demo | works | works | byte-identical |
| concurrency | works | works | byte-identical |
| crypto_demo | works | works | byte-identical |
| database | works | works | byte-identical |
| data_structures | works | works | **diverges** in the `Map<str,int>` section only — 11 lines, see issue 5 |
| fibonacci | works | works | byte-identical (was diverging; see issue 1) |
| float_demo | works | works | byte-identical |
| hello | works | works | byte-identical |
| neural_net | works | works | byte-identical |
| oop | works | works | byte-identical |
| patent_classify | works | works | byte-identical |
| stream_demo | works | works | byte-identical |
| ternary_calculator | works | works | byte-identical |
| ternary_demo | works | works | byte-identical |
| ternary_sort | works | works | byte-identical |
| three_valued_logic | works | works | byte-identical |

The benchmarks behave the same way: `02_ternary_native` and `03_control_flow`
are byte-identical across backends. `01_arithmetic` is not, and the reason is
a resource limit rather than a semantic difference — the T3 run stops on
`TRAP: step limit exceeded` at 10,000,000 instructions while the LLVM build
runs to completion. Any instruction count quoted for `01_arithmetic` is the
step ceiling, not the work the program actually does; say so wherever it is
quoted.

`cargo test`: **313 passed, 0 failed, 0 ignored.**

## Open issues

1. ~~**T3 `int` arithmetic saturates at the 27-trit word range.**~~
   **Changed 11 August 2026 — it now traps.** The previous entry called
   silent saturation "an architectural property, not a bug". That was the
   wrong call, and it cost a correct answer: every ALU op ran its result
   through `clamp27`, so a computation that left the range quietly received
   ±T3_MAX and the program carried on and exited 0. `fib_safe(70)` returned
   `Ok(3812798742493)` on T3 against `Ok(190392490709135)` on LLVM — a wrong
   number wearing a success label, which is precisely what the emulator's
   `trapped` flag exists to prevent.

   `TADD`, `TSUB`, `TMUL` and `TSHI` now trap when the true result falls
   outside ±3,812,798,742,493, reporting the operation and the value the way
   division by zero already did. Saturation is gone; nothing silently clamps.

   The underlying width difference remains and is real: **`int` is the
   target's native word — 64 bits on LLVM, 27 trits on T3ISA.** A value
   between T3_MAX and 2⁶³−1 exists on one backend and not the other. Portable
   code guards against the narrower bound (`examples/fibonacci.mt` now
   demonstrates this) or uses `t27` explicitly. The language reference
   documents the divergence; it previously claimed `int` was simply 64-bit.

   Pinned by `test_arithmetic_overflow_traps_instead_of_clamping`.

2. ~~**T3 truth-table cell corruption (register-layout sensitive).**~~
   **Fixed 11 Aug 2026.** Two separate register-allocation bugs, both of
   which needed enough register pressure that minimal reproductions of the
   same code shape passed — which is why this sat here as "suspected".
   Both produced silently wrong answers rather than a trap.

   *Call operands were materialised before the caller-save stores*, which
   forced them into R21/R22/R24/R25 — the registers the call sequence
   itself uses for the fn_ptr, the move scratch and the return stash. From
   the third spilled operand onward the fn_ptr move overwrote an argument,
   so `op(a, b)` reached the callee as `op(a, a)`. Operands are now
   resolved before the saves and materialised into their target registers
   after them, with the fn_ptr folded into the same parallel move.

   *`dst_reg` returned R23 for an already-spilled temp without storing it
   back to its slot*, silently dropping the assignment. At a short-circuit
   join where one predecessor had spilled the phi, a later predecessor left
   its copy in R23 while the slot kept an unrelated temp, so the join read
   the wrong value. It broke tand/tor distributivity.

   Pinned by `tests/28_regalloc.mt` (expected + cross-target), verified to
   fail 4 checks against a pre-fix build and 0 after.

3. ~~**T3 prints NUL runs for some computed strings.**~~
   **Fixed 11 August 2026.** The mechanism was `read_lp_string` taking the
   length word on trust: `memory.get(ptr).unwrap_or(0) as usize` turned a
   negative length into ~1.8×10¹⁹, and the character loop then pushed
   `char::from_u32(0)` for every word past the end of memory because that
   read also used `unwrap_or(0)`. One bad address therefore manufactured
   NULs without bound.

   It was not cosmetic and it was not small: `data_structures` emitted
   **7.7 GB** from a single `fmt::align_left` call, and because the run still
   exited 0 it presented as a truncation rather than a fault. Output is now
   2,184 bytes.

   The read validates the length and stays inside memory that exists, and
   never substitutes characters it cannot see. Pinned by
   `test_read_lp_string_rejects_implausible_lengths`.

4. ~~**The T3 emulator is slow on allocation-heavy programs** (~4 min).~~
   No longer reproducible: `data_structures` now completes in well under a
   second. The four minutes were spent generating the 7.7 GB of NULs in
   issue 3, not on allocation.

5. ~~**Arrays on T3 are stack-allocated, and `[str]` elements do not survive a
   loop iteration whose body allocates.**~~ *This is the one remaining
   cross-backend divergence, and it is what `data_structures` diverges on.*

   > **CORRECTED 2 September 2026 — FIXED, and the sentence above was TWICE
   > wrong by the time anyone read it.** The reproduction printed below now
   > gives `w=[aa] w=[bb] w=[aa]` and `len=2` on **both** backends, identical
   > and correct. `report.txt` P77 recorded this as stale on 29 August —
   > "F-4's own cited 'one remaining cross-backend divergence' (issue 5,
   > `[str]` in a loop) is STALE — fixed even on the oldest archived binary" —
   > and this document was never reopened. P94 then heap-allocated escaping
   > arrays outright, which is the repair this entry asked for.
   >
   > **And "the one remaining" is false independently of that**: P111 (2
   > September) is a cross-backend divergence — `let u: [int;3] = v[0];` on an
   > array of arrays gives a wrong answer on T3 and a module clang refuses on
   > LLVM.
   >
   > Kept in place rather than deleted, because the *reasoning* below is the
   > record of why arrays were left on the stack when structs moved to the
   > heap, and a fix can destroy the observation that justified it (permanent
   > rule 7). **Pinned by
   > `tests/audit_regression_tests.rs::known_issues_issue_5_no_longer_reproduces`,
   > which runs the reproduction verbatim on both backends** — a count in
   > prose goes stale exactly as fast as the thing it counts (P93, P96), and
   > prose review does not catch that class (rule 6).

   Structs were fixed on 11 Aug 2026 by moving their allocations to the heap
   via syscall #218, matching the LLVM backend's malloc, because a struct
   pointer stored into a longer-lived container (`pcbs[i] = age_tick(p)`)
   aliased the next iteration's stack slot. **Arrays were left on the stack**,
   and they have the same defect.

   Minimal reproduction — the loop variable is correct on the first iteration
   and an unreadable address on every one after:

   ```manit
   let words: [str] = ["aa", "bb", "aa"];
   let m: Map<str, int> = Map::new();
   for w in words {
       io::print("w=["); io::print(w); io::println("]");
       m.insert(w, 1);
   }
   // T3:   w=[aa]  w=[]  w=[]     → len 3
   // LLVM: w=[aa]  w=[bb] w=[aa]  → len 2
   ```

   The corruption is pressure-dependent, which is why it hides: a loop body
   that only prints `w` is fine, and the same loop with a couple more locals
   and an allocating call is not. In `data_structures` it surfaces as a word
   count of 2 instead of 5, with one key reading empty — the counts are wrong
   rather than absent, so nothing about the output announces a fault.

   The fix mirrors the struct one — heap-allocate arrays whose elements
   outlive the iteration — and is a deliberate design decision rather than a
   patch, since the heap is a bump allocator with no free (issue 6). Until
   then: do not keep a pointer to a loop-local array, or to an element of a
   `[str]`, across iterations on T3.

6. **No free/destroy API — leak by design.** Vec/Map/Set/Deque/Trie/
   Channel/Mutex and most string-returning runtime functions allocate and
   are never freed. Fine for the short-lived demo programs; a real
   allocator interface is future work.

   > **Notice, 3 September 2026 — F-4 landed and this is now HALF true.**
   > `region { ... }` releases everything the block allocated, on both
   > backends: T3 resets its bump pointer, LLVM frees the list of cells handed
   > out while the region was open. Measured on the same program with and
   > without: **peak heap 800 words → 4** on T3, and on LLVM a 3,000,000-pass
   > version runs under an 80 MB cap where the region-free version segfaults.
   > `docs/language-reference.md` §23 has the three rules that make it safe.
   >
   > **What is still true is the list above.** A region reclaims what the
   > COMPILER allocates — struct, tuple, array and enum cells, and on T3 the
   > string cells too. The collections and the string routines call `malloc`
   > directly on LLVM and are not reclaimed: 114 call sites across seven
   > runtime files. Routing them through the region allocator is the next
   > step, and the count is here rather than a vague "most" so that the next
   > person knows what it costs. There is still no `free` a program can call,
   > and that remains deliberate: use-after-free in a language without
   > ownership is a runtime surprise, where a region's rules are compile-time
   > refusals.

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
