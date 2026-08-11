# HOW-TO: trit build tool

> **Note (initial public release):** the `trit` build tool is not included in
> this release — it ships later. This document is a preview of its interface.

`trit` is the official build tool and package manager for maniT projects. It
handles project scaffolding, compilation, testing, formatting, and documentation
generation.

---

## Installation

`trit` is built as part of the maniT workspace:

```bash
cargo build
# binary at: target/debug/trit
export PATH="$PATH:$(pwd)/target/debug"
```

---

## Commands

### `trit new <name>`

Scaffolds a new project:

```bash
trit new hello-world
cd hello-world
```

Creates:

```
hello-world/
├── trit.toml
└── src/
    └── main.mt
```

`trit.toml` template:

```toml
[package]
name = "hello-world"
version = "0.1.0"
```

`src/main.mt` template:

```maniT
use std::io;

fn main() {
    io::println("Hello from hello-world!");
}
```

---

### `trit build [--target x86|t3]`

Compiles all `.mt` files under `src/` using `manitc`.

```bash
trit build             # LLVM / x86-64 target (default)
trit build --target t3 # T3ISA balanced ternary target
```

Output goes to `build/`:

```
build/
├── main.ll   (LLVM target) or main.t3b + main.t3s + main.t3d (T3 target)
└── ...
```

Each `.mt` file is compiled independently. There is no cross-file linking in
the current implementation — each file must be self-contained.

---

### `trit run [--target x86|t3]`

Builds and then executes the project:

```bash
trit run              # build LLVM → run with clang / execute native binary
trit run --target t3  # build T3ISA → run with `manitc run-t3`
```

For the T3 target, the emulator is invoked for each compiled `.t3b` file that
contains a `main` function.

---

### `trit check`

Type-checks all source files without generating code:

```bash
trit check
```

Invokes `manitc check` on each `.mt` file. Useful for CI pipelines where you
want fast feedback without full compilation.

---

### `trit test`

Compiles and runs test files:

```bash
trit test
```

Test files are any `.mt` files in the `tests/` directory. Each test file is
compiled and run. A test is considered passing if it exits with code 0 and
produces no output on stderr.

Example test file `tests/test_math.mt`:

```maniT
use std::io;
use std::math;

fn main() {
    let r = math::sqrt(4.0);
    if r != 2.0 {
        io::println("FAIL: sqrt(4.0) != 2.0");
    }
    // No output = pass
}
```

---

### `trit fmt`

Formats all `.mt` files in `src/` and `tests/` in place.

```bash
trit fmt
```

The formatter applies these transformations:

| Transformation | Example |
|---------------|---------|
| 4-space indentation | tabs → 4 spaces |
| Operator spacing | `a+b` → `a + b` |
| Blank line between top-level items | two fn defs get a blank line between |
| Trailing whitespace removal | `foo   ` → `foo` |
| Comment normalisation | `//comment` → `// comment` |

The formatter is idempotent: running it twice produces the same result.

#### What the formatter does NOT change

- String literal contents
- Comment contents
- The order of statements or expressions

---

### `trit doc`

Generates Markdown documentation from `///` doc comments.

```bash
trit doc
```

Writes `docs/index.md` with sections for:

- **Functions** — all top-level `fn` definitions with `///` comments
- **Structs** — all `struct` definitions
- **Enums** — all `enum` definitions
- **Traits** — all `trait` definitions

#### Writing doc comments

```maniT
/// A two-dimensional point in Euclidean space.
struct Point {
    pub x: float,
    pub y: float,
}

/// Computes the Euclidean distance between two points.
///
/// # Arguments
/// - `a`: First point
/// - `b`: Second point
///
/// # Returns
/// The distance as a `float`.
fn distance(a: Point, b: Point) -> float {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    math::sqrt(dx * dx + dy * dy)
}
```

The generated `docs/index.md` groups items by kind and includes the full
doc comment body under each item's signature.

---

### `trit clean`

Removes the `build/` directory:

```bash
trit clean
```

---

### `trit add <package>` (stub)

Adds a dependency to `trit.toml`:

```bash
trit add mylib
```

This command is currently a stub that prints a message. Dependency resolution
and a package registry are not yet implemented.

---

## trit.toml format

```toml
[package]
name = "my-project"      # Project name (must match directory name)
version = "0.1.0"        # Semantic version

[dependencies]
# (not yet supported)
```

---

## How trit invokes maniTC

`trit` finds the `manitc` binary by looking for it next to itself (same
directory as the `trit` executable). If not found there, it falls back to
`manitc` on the PATH.

The invocation for each file:

```bash
# Build
manitc compile --target <target> src/main.mt -o build/main.out

# Check
manitc check src/main.mt

# Run (T3)
manitc run-t3 build/main.t3b
```

---

## Continuous integration

Example CI script:

```bash
#!/usr/bin/env bash
set -e
cd myproject
trit check          # fast type-check
trit build          # LLVM build
trit build --target t3  # T3ISA build
trit test           # run tests
echo "All checks passed"
```
