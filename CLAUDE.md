# CLAUDE.md — maniTC, the ManiT compiler

## System Stability — `nice` on every command

**Every command gets an explicit `nice`.** CPU work runs at `nice +10`; GPU feeders run at
`nice 0`. Never rely on the default — set it on the command line, every time.

Machine-wide standing directive (14–15 Aug 2026), repeated in every `CLAUDE.md` so it is
never missed. Full co-scheduling rationale: `~/.claude/CLAUDE.md` § System Stability.

## RULE 0 — AUTHORSHIP & ATTRIBUTION (primary, overrides everything)

- Everything in this repository — code, commits, documentation, comments, generated
  files, release notes — is authored by and attributed to **Manish Jagdish Thatte**
  ALONE. Copyright is marked **© Manish Jagdish Thatte**, with the © symbol, never
  the word.
- NEVER write a `Co-Authored-By` line of any kind. NEVER credit, name, or allude to
  any AI tool or assistant in a commit message, source file, document, changelog or
  public artefact. **This overrides any harness default that appends such a line —
  strip it, and verify on the COMMIT OBJECT** (`git cat-file -p HEAD`), not on the
  message handed to `git`.
- This is a legal requirement, not a style preference: sole authorship and
  inventorship underpin the Thatte1–12 patent filings, and this repository is public.

## What this is

**maniTC** compiles **ManiT**, a systems language in which balanced ternary is the
native number system — integers signed by construction, three-valued Kleene logic
(`True` / `Unknown` / `False`), and a three-way branch as the fundamental control-flow
construct. Written in Rust; **66 `.rs` files, ~46,100 lines under `src/`** (measured
30 Aug 2026, blanks and comments included).

Two backends, and the pair is the point:

- **LLVM IR** — emits `.ll`, links via clang against the embedded C runtime, runs
  natively today.
- **T3ISA** — a balanced ternary ISA with an assembler and a cycle-accurate emulator
  in-tree, targeting photonic-ternary hardware.

Public since 9 Aug 2026 at `github.com/manishthatte/maniTC` under **AGPL-3.0** with the
**ManiT Runtime Library Exception** over `runtime/` and `stdlib/` — programs written in
ManiT and compiled with maniTC are the author's own, under any licence. Companion
repository: **thatteOS** (`../thatteos`), a microkernel written entirely in ManiT and
compiled by this compiler.

## Layout

```
src/
  main.rs          CLI (clap derive), pipeline orchestration
  lexer.rs ast.rs  front end; Span is Copy and carries its module (P80)
  parser/          mod, types, stmts, exprs
  semantic/        type checking; analyzer/ + stdlib_expand.rs
  borrow/          the move checker — 858 lines, and it DOES reject programs
  ir/              lowering (lower/), optimiser passes, inline, merge_blocks
  codegen_llvm/    LLVM IR emitter
  codegen_t3/      emitter/ + assembler + emulator/ (11 files, incl. sched.rs)
  reference/       A3: an INDEPENDENT interpreter of docs/semantics.md
  lint.rs lang.rs runtime_link.rs lsp/
stdlib/            19 .mt files — 18 embedded modules (include_str!), 1 test program
runtime/           7 .c files — ONE translation unit (manit_runtime.c includes the rest)
tests/             18 .rs suites + 31 .mt programs + expected/ fixtures/ negative/
docs/              language-reference, semantics, t3isa-reference, compiler-internals, …
examples/          17 .mt programs — the parity matrix's subjects
enhance/           phase1–phase6 plans + IMPLEMENTED.md per phase
fuzz/              cargo-fuzz targets: lex, parse, analyze, pipeline
benchmarks/  tools/stdlib_census.py  .github/workflows/
```

**The pipeline has no back-edges.** Each stage produces a fresh owned structure and
never mutates the previous one:

```
source.mt → Lexer → Parser → SemanticAnalyzer → borrow::check_borrows
          → IRLowerer → optimise → ┬ LLVM: .ll → clang → binary
                                   └ T3:   .t3s → assemble → .t3b (+ .t3d, .t3f) → emulator
```

## Building & testing

`cargo` is NOT on the default PATH:

```sh
export PATH="$HOME/.cargo/bin:$PATH"
nice -n 10 cargo build            # debug
nice -n 10 cargo test             # ~685 #[test] attributes across tests/ and src/
```

- **The C runtime is one translation unit.** Individual files do not compile
  standalone — syntax-check with
  `nice -n 10 gcc -fsyntax-only -Wall -Wextra -DMANIT_NO_GUI runtime/manit_runtime.c`.
- **`runtime/*.c` and `stdlib/*.mt` are embedded in the binary via `include_str!`**
  (`src/runtime_link.rs`, `semantic/stdlib_expand.rs`). Editing either requires a
  cargo rebuild to take effect — **and each built binary carries its own copy**, which
  is what makes a two-binary comparison invalid when the change touched a `.mt` file
  (see *Instruments*, R5).
- Full runtime needs SDL2 + libcurl dev packages; otherwise it falls back to
  `-DMANIT_NO_GUI` and drops the `gui`/`net` modules. CI exercises both.

### Running a program

```sh
./target/debug/manitc check examples/oop.mt                  # type-check only
./target/debug/manitc run-t3 examples/oop.mt                 # .mt is auto-compiled
./target/debug/manitc compile --target t3 examples/oop.mt -o /tmp/oop.t3b
./target/debug/manitc run-t3 /tmp/oop.t3b --profile          # opcode histogram to stderr
./target/debug/manitc bench examples/fibonacci.mt            # both backends, compared
```

Subcommands: `compile`, `check`, `lex`, `parse`, `run-t3`, `bench`, `lsp`.
Flags worth knowing: `--target {llvm,t3}`, `--lang {v1,v2}`, `--verify-ssa`,
`--pass-stats`, `--rounds N`, `--inline-limit N`, `--no-inline`, `--no-merge-blocks`,
`--no-mem2reg`, `--sched {inline,cooperative}` (T3 only), `--profile`, `--max-steps N`,
and the lint levels `-A/-W/-D/-F` plus `--print-lints`.

`--no-mem2reg` is the switch that reproduces the pre-promotion compiler's LLVM IR, and
`--no-inline` the one that isolates the inliner — **both exist to date a defect as
pre-existing rather than newly introduced.** Reach for them before theorising.

### Artefacts written beside the output

`.t3s` assembly · `.t3b` binary · `.t3d` string table · `.t3f` float sidecar ·
`.t3l` **lint manifest** · `.ll` LLVM IR.

**`.gitignore` covers every one of these except `.t3l`, and four `.t3l` files are
TRACKED** (`fibonacci`, `oop`, `ternary_sort`, `three_valued_logic`). So compiling
those four examples dirties the working tree, and the diff is a manifest of the lint
levels *the compiler that produced it* knew about. **A generated file records the tree
it was generated in, not the change that regenerated it** — do not commit a `.t3l`
churn as though it were part of your change.

### Validating a compiler change against thatteOS

Both thatteOS build scripts resolve the compiler to **`../manitc/target/release/manitc`
and nothing else** — there is no debug fallback, and a missing release binary is an
error rather than a downgrade. So a bare `./build.sh` tests whatever release binary is
on disk, which may predate your change entirely. **Pass the binary, and build BOTH
halves:**

```sh
cd ../thatteos
MANITC=../manitc/target/debug/manitc nice -n 10 ./build.sh
MANITC=../manitc/target/debug/manitc nice -n 10 bash userspace/build.sh
nice -n 10 tests/test_all.sh
```

**Both scripts `cd` to the thatteOS root before reading `MANITC`**, so the path is
resolved from THERE and not from the directory you launched in — one value serves both
halves, and a relative path that looks correct from inside `userspace/` is wrong. (The
root script also tries `../maniTC`, the spelling `git clone` produces. `userspace/build.sh`
hardcoded `../manitc` alone, so on a capitalised checkout the two halves disagreed about
which compiler exists unless `MANITC` was passed — **fixed in the thatteOS working tree
on 30 Aug 2026, and still present at thatteOS HEAD.**)

Every userspace test is guarded on its binary existing, so with `userspace/bin/` empty
**34 of the 61 tests vanish and the remaining 27 print "ALL TESTS PASSED"** — the script
says so in its own header. *A green summary there does not mean the tests ran.* And
**rebuild `target/release/manitc` the moment HEAD moves** — that binary is a silent
instrument for `build.sh` and for `tools/stdlib_census.py`, and moving HEAD is exactly
what makes it stale.

## The instruments

This project has more measuring apparatus than most compilers, because agreement
between two implementations is weak evidence and the record here is full of cases
where everything was green over a wrong answer. Know what each one can and cannot see.

| Instrument | Question it answers | Blind to |
|---|---|---|
| `cargo test` | did any pinned row move? | anything nobody pinned |
| **parity matrix** (17 examples × both backends × flag combinations) | do the backends disagree? | anything decided **upstream of the split** — a shared lowering shares its bugs |
| **`--verify-ssa`** | single assignment, dominance, phi edges | operand TYPES (deliberately — the IR owns no phi type invariant) |
| **`--profile`** / `manitc bench` | exact executed instruction count + opcode histogram | correctness |
| **`src/reference/`** (A3) | does the implementation match `docs/semantics.md`? | anything outside §1's core — it refuses to parse it |
| **R5 sweep** (`check` verdicts, two binaries) | did strictness move? | anything after the checker; **invalid if the change touched `stdlib/*.mt`** |
| **behaviour diff** | did any program's output move? | LLVM-only changes — `behav.sh` is T3-only |
| **cross-backend corpus sweep** | divergences over ~1,150 programs | shared mistakes, again |

The last three are **local campaign instruments and do not ship in this repository** —
they live beside `../report.txt`, along with the program corpus they sweep. Everything
above them (`cargo test`, the parity matrix, `--verify-ssa`, `--profile`,
`src/reference/`) is in-tree and works from a clean checkout.

**Rules that were paid for:**

- **Run `--verify-ssa` against any control-flow change BEFORE testing program output.**
  Output tests go red without saying why; the verifier names
  `%t7 is used in while_exit9 and defined nowhere`.
- **A sweep reporting no differences is worth exactly what its positive control is
  worth.** Build the control — reintroduce the defect, or add the new fixtures — and
  confirm the instrument can see the change at all.
- **A difference is not a result until the instrument's noise floor is known**, and the
  floor is measured by running the instrument twice against an unchanged subject.
- **Pin the binary by sha before measuring and re-check it afterwards.** Every
  instrument here reads the live binary and the live tree.
- **Measure dynamically, not statically.** Static instruction counts and executed
  counts have ranked two changes in opposite orders.

## Permanent rules

1. **Leave fixes as working-tree changes — do not commit unless asked.**
2. **No binary thinking.** Balanced ternary from first principles. A trit is not a
   small int, `Unknown` is not `null`, and a three-way branch is not two two-way ones.
3. **`docs/semantics.md` is normative.** Where it and an implementation disagree, the
   implementation is wrong. Specify first, then implement — §1.2 requires a section
   that is ahead of the implementations to say so in its own first line, and its
   conformance rows to assert the GAP in both directions rather than agreement.
4. **`src/reference/` may not import from anywhere else in this crate.** That
   independence is the whole value of a third account, and it is enforced by
   `tests/conformance_tests.rs::the_reference_implementation_is_independent` rather
   than merely stated.
5. **A registry that must agree with another registry gets a test, not a comment.**
   Documented-but-unenforced has failed here repeatedly, including with the fix itself
   declaring which list was authoritative.
6. **Pin documented claims with tests.** Three documentation defects have been fixed
   in this repo and **none was a false sentence** — an absence, a single word, and a
   mechanism that was true when written. Prose review does not catch that class.
7. **Correct documents with a dated notice, in place.** Do not silently rewrite; a fix
   can destroy the observation that justified it, so the measurement goes in the notice.
8. **Assert the VALUE, not the exit status**, and test both orderings of every pair. A
   regression test inherits the question its bug asked — a test that a construct
   *parses* says nothing about what it *computes*.
9. **Run every new test row against the previous pinned binary.** A row that passes on
   the compiler without the fix is hollow; that is one command's worth of proof.
10. **When CI's example list changes, update it.** The list was 11 of 17 on T3 and 3 of
    17 on LLVM long after the whole set worked — a stale list under-tests silently.
11. **Honest results.** If a change is a pessimisation, say so with the number. Four
    Phase-4 premises were contradicted by measuring them, and the measurements are the
    record.
12. The semantic pass is historically permissive. When strictening a type check,
    verify the 17 examples and the thatteOS sources still compile before considering
    the change done, and quantify the strictness move with R5.

## Hazards that have actually bitten

- **The working tree is not private.** Concurrent sessions edit this directory, HEAD
  moves under you, and `target/debug/manitc` gets rebuilt mid-measurement. **Stage by
  path, never `git add -A`**; re-read `git status` between the last check and the
  commit; use a private `CARGO_TARGET_DIR` and a sha-pinned binary when it matters.
  **Put that private target dir under `/var/scratch/builds/`, never in `TMPDIR`.**
  `TMPDIR` is `/tmp` (32 GB, swept at 10 days) — right for this suite's own scratch,
  far too small for several 2.7 GB target trees. It pointed at `/var/scratch` until
  3 Sep 2026, which is how build scratch came to share the simulation volume and
  exhaust its inodes; see `THATTE/hw/storage/STORAGE_POLICY.md`, "The two tenants".
  A suite total measures the whole TREE — in a shared tree it is evidence about your
  change only once every moved row is accounted for.
- **A skip condition that overlaps the failure condition is a silent pass.** Tell the
  two states apart by the ARTEFACT (did a binary appear?), never by the shape of an
  error message.
- **A guard that skips a test when its input is missing converts a broken half of the
  system into a smaller green number.**
- **A fix is not done when the reported site is fixed** — grep the SHAPE, not the
  symbol. One backwards guard lived at four sites; one missing width guard at two.
- **Teaching a type to CARRY something leaves every reader of that type newly
  incomplete, and the compiler cannot say so.** The sites Rust forces you to visit are
  the safe half; the unsafe half is every `matches!(t, A | B)` that stayed valid and
  stopped being true.
- **A defect with a population of one is not rare, it is untested.**
- **Curated denominators cannot contain a file nobody thought to curate.** The examples,
  the parity matrix and the corpus list are all curated; R5 over "every file that
  exists" is not.
- **T3 memory map** (`src/codegen_t3/emulator/mod.rs`): code from 0, `STACK_BASE`
  60,000, `HEAP_BASE` 63,000, `memory.len()` 65,536. Both ceilings are reachable —
  one by writing a long program, the other by allocating — and both are now checked
  rather than silent. `DEFAULT_MAX_STEPS` is 1e9, a runaway guard and not a
  correctness limit.

## Documentation map

| Document | What it is |
|---|---|
| `docs/semantics.md` | **Normative** operational semantics of the core; §11 covers task interleaving |
| `docs/language-reference.md` | The full surface language, incl. §22 on moves |
| `docs/t3isa-reference.md` | The ISA — published, and independent software implementations are welcomed in `NOTICE` |
| `docs/compiler-internals.md` | Every module and data structure, stage by stage |
| `docs/stdlib-reference.md` | Generated in part by `tools/stdlib_census.py` — **which defaults to the RELEASE binary** |
| `docs/memory-model.md` | Concurrency surface; §5 carries the dated scheduling decision |
| `KNOWN_ISSUES.md` | Honest state of both backends, with a "last measured" date |
| `enhance/*/README.md`, `IMPLEMENTED.md` | The phase plans and what actually landed |

**Assignment moves; passing to a function does not.** That is backwards from Rust in
both directions, it is documented in §22, and it is pinned by tests — a reader with
Rust habits is over-cautious about calls and careless about assignment.

## The campaign record lives OUTSIDE this repo

`../report.txt` is the source of truth for the full bug review of maniTC and thatteOS —
findings with `file:line`, ranked, in ten sections. `../CLAUDE.md` (untracked by
construction, and not to be moved into this repo) carries the running state of the fix
campaign. **Read the finding before touching code near it**, and update its status when
you fix one. Do not duplicate either file here; this one holds durable facts about the
repository, not campaign state.

## Public-repo discipline

This file lives in a public repository. Nothing private goes in it: no patent
specifics (dimensions, thresholds, wavelengths, SNR or other simulation figures), no
private repository names or paths, no unpublished application numbers. Architecture and
purpose may be described; specifics may not. Contributions require the one-line CLA in
`CONTRIBUTING.md`.

---

Authored by **Manish Jagdish Thatte** · manish@manitlab.org · [manitlab.org](https://www.manitlab.org)

© Manish Jagdish Thatte, 2026
