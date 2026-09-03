# Phase 1 — what landed

© Manish Jagdish Thatte
24 August 2026

Against `MANITC_FEATURE_RECOMMENDATIONS.txt` §11 Phase 1: A1 steps 1–2, A5,
B1, F-8. Working-tree changes, uncommitted, per the repo convention.

## Verification

| | before | after |
|---|---|---|
| `cargo test` | 413 pass, **1 fail** | **438 pass, 0 fail** |
| examples on T3 | 17/17 | 17/17 |
| examples on LLVM | 17/17 | 17/17 |
| thatteos `tests/test_all.sh` | 61/61 | 61/61 |
| fuzz campaign | — | ~481,000 runs, 0 crashes |

The one pre-existing failure was `s23_the_whole_math_float_surface_runs_on_both_backends`,
confirmed failing at pristine HEAD `d85bf06` in a separate worktree before any
of this landed. See "N-findings" below.

## A1 — `extern` declarations (steps 1 and 2)

New item form, parsed and recorded:

```
extern "c" fn io::println_int(n: int) -> void
    available(llvm, t3) deprecated("use fmt::print");
```

- `extern` is a keyword; `available` and `deprecated` are contextual, claimed
  only in clause position. No `.mt` file in either repo used any of the three
  as an identifier, so nothing had to change.
- **Step 2 falls out of step 1 rather than needing its own mechanism.** A
  declaration puts a fully resolved signature into `self.functions`, and the
  call checker already enforces when every parameter type is known — the
  asymmetry existed only because a native's parameters were `Unknown`. So
  section 53's case is now a type error at the call site:

  ```
  error: argument 1 to 'io::println_int': expected `int`, found `bool`
  ```

  The two `audit_regression_tests.rs` tests that PIN the old asymmetry were
  left alone deliberately: they describe an **undeclared** native, which A1
  does not touch. Narrowing them belongs to the migration, not to the
  mechanism.
- The migration backlog is **generated, not hand-written**:
  `manitc check prog.mt --warn undeclared-native` lists every native the
  program reaches without a declaration.

**The standard library was not migrated.** The 413 body-less declarations in
`stdlib/*.mt` are untouched, and calls to them behave exactly as before. Three
reasons: step 3 (`available` enforcement) is explicitly *not* Phase 1 and is
what makes migration urgent; the stdlib source is the model-training corpus, so
changing it mid-campaign compounds the attribution loss the handoff is already
managing; and the value of steps 1–2 is the mechanism, which is proven by tests
and by the backlog the compiler now generates on demand.

## A5 — lint levels, recorded in the artifact

`src/lint.rs`. Eleven named lints, each with a default chosen so that
introducing the system changes **no existing diagnostic**.

- `--allow` / `--warn` / `--deny` / `--forbid` (`-A -W -D -F`), repeatable.
- `lint deny(shadowing, unknown-type);` at item position, for per-module levels.
- An unknown lint name is an **error**, and the message lists the real names.
  A silently ignored `--deny unusd-variable` would be the same class of defect
  A5 exists to fix.
- `--warn-as-error` is retained as "raise everything to deny" — it is what
  §54's strict binary was built with, and dropping it would break every
  recorded invocation.
- A denied lint is now reported **once**, as an error, then the compilation
  aborts with a count. It used to print as a warning and again as the error.

The manifest is recorded in the artifact, which is the point:

```
$ strings a.out | grep manitc-lints
manitc-lints v1 compiler=0.1.0 backend-unavailable=allow ... unused-variable=warn
```

LLVM: a comment plus a `@manitc.lints` constant that survives linking. T3: a
comment in the `.t3s` plus a `.t3l` sidecar, matching the existing `.t3d`/`.t3f`
convention — the `.t3b` is magic-word-plus-words with no room for metadata, and
widening it would break every reader.

## B1 — trait bounds and where-clauses

`fn max<T: Ord>(a: T, b: T) -> T` and `fn f<T>(..) -> .. where T: Display` both
parse and are checked. `+` joins several bounds; the two forms accumulate.

Bounds are satisfied by a user `impl`, or intrinsically by a primitive for the
structural traits (`Ord`, `Eq`, `Display`, …). Structs and enums need the impl.

### A4's open question, answered

A4 said of an unbounded `max<T>` on a non-comparable `T`: *"What happens today
is either a lowering-time crash or a silently wrong comparison; I have not
verified which."*

Measured. It is **neither, and worse than both**:

```
struct P { pub x: int }
fn max2<T>(a: T, b: T) -> T { if a > b { a } else { b } }
...
io::println_int(max2(P{x:9}, P{x:1}).x);
```

`manitc check` passes. Both backends compile. Both print **1**. The correct
answer is 9 — the comparison is on the two allocation addresses, so the result
depends on allocation order, not on the values. **Both backends agree**, which
is trap 10 exactly: the differential oracle is structurally blind to it.

Writing the bound now rejects it. An unbounded `<T>` still compiles exactly as
before — the hole is closed by *writing* the bound, because inferring one would
reject programs that check today.

## F-8 — coverage-guided fuzzing

`fuzz/` with four cargo-fuzz targets (lex, parse, analyze, pipeline-through-IR),
a seed script that builds the corpus from real maniT source, and a CI job.
Codegen is deliberately excluded: the two emitters are separate surfaces, and
mixing them makes a crash say "the compiler panicked" without saying which half.

Campaign run 24 Aug 2026:

| target | runs | crashes |
|---|---:|---:|
| `fuzz_lex` | 81,412 | 0 |
| `fuzz_parse` | 327,105 | 0 |
| `fuzz_analyze` | 40,633 | 0 |
| `fuzz_pipeline` | 31,997 | 0 |

`tests/fuzz_corpus_tests.rs` replays every shipped source and every recorded
reproducer **on stable**, so a finding outlives the nightly toolchain that found
it. `fuzz/corpus/` is gitignored (fuzzer memory, thousands of mutated files);
`fuzz/artifacts/` is not — a reproducer is the most valuable thing a run
produces.

## N-findings — found while implementing, fixed here

**N-A. The T3 emulator aborted the process on arithmetic overflow.**
`src/codegen_t3/emulator/execute.rs`: `rhs_eff = self.regs[sr3] + imm`, plain
`+`, where every arithmetic opcode around it routes through `saturating_*` and
then `checked27`. In a debug build a wide intermediate aborted with "attempt to
add with overflow" and no file:line in the ManiT source. This is what made
`s23_the_whole_math_float_surface...` fail at HEAD. Saturating changes no
in-range result; it only lets the existing T3 fault path do its job instead of
being pre-empted by a panic. **This is precisely the F-8 defect class, found by
running the tests F-8 motivated.**

**N-B. The parser's depth guard belonged to the binary, not the library.**
`MAX_PARSE_DEPTH` refuses input nested past 256, but the limit is only
*enforceable* on a stack deep enough to reach it — and the 256 MB reservation
lived in `main()`. Every other embedder got the default stack and a **process
abort**: the F-8 corpus harness hit it immediately, and so would the **language
server**, which parses on tokio workers and is exactly where half-written,
deeply-nested code lives. `COMPILER_STACK_BYTES` and `with_compiler_stack` moved
into the library; `src/lsp/mod.rs` now uses them on both of its parse paths.
No change to what the compiler accepts.

**N-C. CI was under-testing the example matrix.** The lists were 11 of 17 on T3
and 3 of 17 on LLVM, left from when that was true; all 17 pass on both backends
today. The file's own instruction is to add each one as it starts working "so
that a fix can never silently regress", which only holds if the list is current.
Both lists now name all 17.

## Not done, deliberately

- **A1 step 3** (`available` enforcement against the selected target) — not
  Phase 1. The diagnostic is produced and the `backend-unavailable` lint exists
  at `allow`, so the step-3 backlog can be generated the same way the step-1
  one is, before it becomes blocking.

  > **MEASURED, 3 September 2026, and the backlog is not what "generate it the
  > same way" would report.** Running `check -W backend-unavailable --backend
  > t3` over a hello-world reports **zero**, and that zero is the finding
  > rather than a clean bill: the lint fires when an `available(...)` clause
  > EXCLUDES the target, and almost nothing declares one. Counted directly in
  > `stdlib/`: **2 `extern` declarations, both carrying `available(...)`,
  > against 414 native declarations in the older `fn f(…) -> T ;  // native`
  > form.** So step 3's precondition is 2 of 416, and its real content is the
  > stdlib migration below rather than a level change in `lint.rs`.
  >
  > **The truth it would be migrating to already exists in the compiler**, and
  > that is the cheaper design worth weighing before anyone hand-writes 414
  > clauses: P85 measured **ninety** registered builtins that link on LLVM and
  > have no T3ISA syscall — `fs_*` (13), `io_*` (7), `terminal_*` (4), `env_*`
  > (3), `path_*` (3), one each of `net_*`, `process_*`, `shell_*`, and the
  > `gui_*` surface §3.4 names — and it measured them by reading the backends'
  > own registries, in
  > `analyzer::tests::every_registered_flat_builtin_is_linkable`. A check that
  > consults those registries needs no migration at all; 414 hand-written
  > clauses are a second registry that must agree with the first, which is
  > exactly the shape permanent rule 5 exists to refuse.
  >
  > Note also what step 3 costs the instruments: it edits `stdlib/*.mt`, and
  > each built binary embeds its own copy (`include_str!`), so **R5 is invalid
  > across the change unless its denominator is rebuilt per binary** — P70's
  > rule, and no change since has needed it.
- **The stdlib migration** — see A1 above, where its size is now measured.
- **A2, A3** — Phase 2 and Phase 3 respectively.

## R5 — what this does to the L1 metric

Two changes affect what `manitc check` accepts, both narrowly:

1. A declared extern's arguments are checked. No shipped program declares one,
   so nothing existing is affected.
2. `unsatisfied-bound` defaults to `deny`. No shipped program writes a bound —
   the syntax was a parse error until today — so nothing existing is affected.

Everything else is additive or opt-in.

**Measured, not asserted.** The archived strict binary and the Phase 1 binary
were both run over all 268 `.mt` files in `manitc/` and `thatteos/`:

- **Verdict differences: 0.** Every source gets the same exit status.
- **Text differences: 59 files**, all the same change — a warning now carries
  its lint name, `… prefix with \`_\` if intentional [unused-variable]`.

`l1_probe.manitc_check` classifies on the return code, and on `"parse" in blob`
to separate PARSE_ERROR from CHECK_ERROR. No lint name contains the substring
"parse", and neither does the `aborting: N denied lints` summary — so the
classification cannot shift and **no L1 number needs re-scoring**.

The strict binary that scored the live L1 run (sha `14178d80b64116d9`, manitc
`d85bf06`) was archived to `manit-model/runs/checkers/` **before** any rebuild;
the Phase 1 binary is archived beside it as `608f8723c7545cfc`. The reasoning
is written down in that directory's `README.md`, which is now tracked.
