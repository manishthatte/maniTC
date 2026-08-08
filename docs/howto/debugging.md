# HOW-TO: Debugging

This guide covers how to diagnose and fix errors at each layer of the maniT
compiler pipeline.

---

## Table of contents

1. [Reading error messages](#1-reading-error-messages)
2. [Lex errors](#2-lex-errors)
3. [Parse errors](#3-parse-errors)
4. [Type errors](#4-type-errors)
5. [IR inspection](#5-ir-inspection)
6. [Assembler errors](#6-assembler-errors)
7. [Emulator crashes and wrong output](#7-emulator-crashes-and-wrong-output)
8. [LLVM backend issues](#8-llvm-backend-issues)
9. [Build tool issues](#9-build-tool-issues)
10. [Common pitfalls](#10-common-pitfalls)

---

## 1. Reading error messages

All compiler errors follow the format:

```
PhaseError: file:line:col: message
```

Example:

```
ParseError: src/main.mt:12:5: expected `:`, found `Ident("foo")`
TypeError: src/main.mt:20:10: type mismatch: expected int, got str
```

The phase prefix tells you where to look:

| Prefix | Phase | Fix in |
|--------|-------|--------|
| `LexError` | Tokenisation | Source file — invalid character or malformed literal |
| `ParseError` | Parsing | Source file — syntax error |
| `TypeError` | Type checking | Source file — type mismatch, undefined name |
| `CodegenError` | Code generation | Usually a compiler bug |

---

## 2. Lex errors

### Unrecognised character

```
LexError: hello.mt:3:1: unexpected character: '@'
```

The source file contains a character the lexer does not recognise. Check for
copy-paste artefacts, Unicode punctuation that looks like ASCII, or characters
not in the maniT character set.

### Malformed balanced ternary literal

```
LexError: hello.mt:5:10: invalid trit digit 'x' in balanced ternary literal
```

Valid trit digits are `+`, `0`, `-`. No other characters are allowed after `0t`.

### Unterminated string

```
LexError: hello.mt:8:1: unterminated string literal
```

A `"` was opened but never closed. Check for a missing closing `"`.

### Debugging tip

Use `manitc lex file.mt` to see every token the lexer produces, with line and
column numbers:

```bash
manitc lex hello.mt
```

```
Ident("main")  (1:4)
LParen  (1:8)
RParen  (1:9)
LBrace  (1:11)
...
```

---

## 3. Parse errors

### Expected token, found something else

```
ParseError: main.mt:10:5: expected `:`, found `Ident("x")`
```

This usually means a missing `:` in a `let` binding, struct field, or function
parameter.

```maniT
let x = 42;        // OK — type inferred
let x: int = 42;   // OK — explicit type
let x int = 42;    // ERROR — missing ':'
```

### Unexpected token at top level

```
ParseError: main.mt:1:1: unexpected token at top level: Int(42)
```

A bare expression appears at the top level (outside any function). Only `fn`,
`struct`, `enum`, `impl`, `trait`, `use`, and `let` are valid at the top level.

### Expected `>` to close generic args

```
ParseError: main.mt:5:20: expected `>` to close generic args
```

A generic type like `Vec<int>` has an unmatched `<`. Check for missing `>`, or
a `>>` that the parser failed to split (this should be handled automatically;
if not, it's a compiler bug — report it).

### Debugging tip

Use `manitc parse file.mt` to see the AST and determine what was successfully
parsed before the error:

```bash
manitc parse hello.mt
```

---

## 4. Type errors

### Undefined variable

```
TypeError: main.mt:7:5: undefined variable `n`
```

The variable `n` was not declared in scope. Check:
- Declared with `let n = ...`
- Declared before the point of use
- Correct scope (not declared inside a nested block that has ended)

### Undefined function

```
TypeError: main.mt:15:10: undefined function `compute`
```

The function `compute` was not declared. Check:
- Correct name (case-sensitive)
- Declared before use (or declared elsewhere and callable — forward references
  within the same file work because of the two-pass strategy in `collect_declarations`)
- If calling a method: the impl block exists for the type

### Type mismatch

```
TypeError: main.mt:12:15: type mismatch: expected int, got str
```

The expression produces the wrong type. Check:
- Function return type declared correctly
- Variable type annotation matches the assigned value
- All arms of a `match` or `if` expression produce the same type

### Wrong number of arguments

```
TypeError: main.mt:8:5: wrong number of arguments: expected 2, got 1
```

A function was called with the wrong number of arguments.

### Method not found

```
TypeError: main.mt:20:10: no method `length` on type Struct("Point")
```

Either:
- The method name is misspelled
- The impl block for `Point` does not define `length`
- The method is defined on a different type

### Self resolution issues

If you see `TypeError` about `Self` inside an `impl` block, ensure the
`impl` block's type name matches exactly how it was declared:

```maniT
struct MyType { ... }

impl MyType {
    fn clone(self) -> Self { ... }  // Self resolves to MyType
}
```

---

## 5. IR inspection

Use `--emit-ir` to see the IR generated for your program:

```bash
manitc compile --target t3 --emit-ir hello.mt
```

Output format:

```
fn main (0 params, 3 blocks)
  entry:
    Alloca { dst: t0, ty: I64 }
    Store { ptr: Temp(t0), val: Const(Int(0)), ty: I64 }
    -> Jump("loop_header0")
  loop_header0:
    Load { dst: t1, ptr: Temp(t0), ty: I64 }
    ...
    -> CondJump(Temp(t5), "loop_body0", "after0")
  ...
```

What to look for:

- **Missing allocas:** Every local variable should have an `Alloca` in the entry
  block and corresponding `Store`/`Load` instructions.
- **Dangling temps:** A `Load` or `BinOp` that references a `Temp` that was
  never produced — indicates a missing IR instruction.
- **Wrong type in a BinOp:** E.g. `op: IAdd, ty: Bool` — the type should match
  the operand types.
- **Missing terminator:** Every block should end with a `Jump`, `CondJump`,
  `TritJump`, or `Return`.

---

## 6. Assembler errors

### Unknown mnemonic

```
[T3ISA] assembler error: Unknown mnemonic 'FOOBAR' at pc=42
```

The emitter produced an unrecognised instruction name. This is a compiler bug.

To diagnose, inspect the assembly file:

```bash
manitc compile --target t3 hello.mt    # produces a.t3s
cat a.t3s | grep -n "FOOBAR"
```

Find the surrounding context in `a.t3s` to understand what the emitter was
trying to generate.

### Undefined label

```
[T3ISA] assembler error: Undefined label: my_fn
```

A `CALL` or `JUMP` instruction references a label that was never defined. The
assembly file will contain `CALL my_fn` but no corresponding `my_fn:` label.

```bash
grep -n "my_fn" a.t3s
```

If the function exists in the source but not in the assembly, check:
- The function was not dead-code eliminated
- The emitter produced the function's label (functions are emitted as
  `TypeName::method_name:` for methods)

### Label with :: not parsed correctly

Labels containing `::` (qualified method names like `Direction::to_str:`) must
be detected by the assembler correctly. If you see a mnemonic that starts with
`::` (e.g. `::TO_STR`), the assembler's `find_label_colon` function was not
triggered.

To debug: open `assembler.rs` and add a `println!` in the `find_label_colon`
function to trace which colon it finds.

### Operand parse failure

```
[T3ISA] assembler error: cannot parse operand 'R99' as register
```

The emitter produced a register number beyond R0–R26. This is a compiler bug —
check the emitter's register allocation logic.

---

## 7. Emulator crashes and wrong output

### Abnormal halt (no HALT instruction)

If the emulator seems to loop forever or produces no output, the program counter
may have run off the end of the code section into the string data area. This
happens when a function does not have a `RET` instruction.

Check `a.t3s` for functions that fall through without returning:

```bash
grep -A 20 "fn_name_entry:" a.t3s | grep "RET"
```

### Wrong integer output

The T3ISA operates in balanced ternary. If you print an integer and get an
unexpected value, check:

1. **Overflow:** The value exceeds T3_MAX (3,812,798,742,493). Arithmetic
   saturates.
2. **Stack misalignment:** A function pushed values to the stack but didn't pop
   them before returning, corrupting the caller's local variables.
3. **Wrong immediate:** `TLIT Rx, #N` supports only ±797,161 (13 trits). Larger
   constants must be constructed differently.

### Wrong string output

If `print_str` prints garbage:
1. Check that the string label exists in `a.t3s` under the `.data:` section.
2. Check that the `.t3d` sidecar file was generated alongside `.t3b`.
3. Check that the address computed at assembly time matches the address used
   by `TLIT Rx, #str_label` at runtime.

### Stack overflow

If the program prints nothing and the emulator runs for a very long time, there
may be unbounded recursion. The emulator does not enforce a stack limit.

To trace execution, temporarily add `io::print_int` calls as breadcrumbs in
the maniT source.

### Cooperative task starvation

If tasks don't produce output in the expected order, check that tasks call
`async::yield_now()` frequently enough. A task that never yields will run
to completion before other tasks get a chance to run.

---

## 8. LLVM backend issues

### LLVM IR type errors

```
error: instruction requires the same type for all operands and result
```

The LLVM IR emitted by `codegen_llvm.rs` has a type inconsistency. Use
`--emit-ir` to inspect the IR and cross-reference with the LLVM IR in `a.ll`.

### clang not found

```
[LLVM] clang not found — LLVM IR written to a.ll
[LLVM] to compile: clang a.ll -o a.out
```

Install clang:

```bash
# Ubuntu / Debian
apt-get install clang

# macOS
brew install llvm

# Arch
pacman -S clang
```

Or compile manually with `llc`:

```bash
llc a.ll -o a.s
clang a.s -o a.out
```

### Missing external function declaration

```
error: use of undefined value '@__maniT_println'
```

A stdlib function call in the IR references a helper function that wasn't
declared. In `codegen_llvm.rs`, find where `declare` lines are emitted in
`emit_module()` and add the missing declaration.

---

## 9. Build tool issues

### `trit: command not found`

The `trit` binary is at `target/debug/trit`. Add the build target directory to
your PATH or use the full path.

### `trit build` fails with "no .mt files found"

The build tool looks for `.mt` files in `src/`. Ensure:
- The current directory contains a `trit.toml`
- At least one `.mt` file is in `src/`

### `trit fmt` mangled my code

The formatter is token-based and works by rewriting lines. If it produces
incorrect output:
1. Run `git diff` to see what changed.
2. The formatter preserves comments and string literals but may alter spacing
   around operators. If the formatting is wrong, file a bug with the input
   that caused the problem.

### `trit doc` produced an empty or incomplete document

Check that your doc comments use `///` (three slashes), not `//` (two). Only
`///` comments are extracted.

---

## 10. Common pitfalls

### Forgetting `mut`

```maniT
let x: int = 0;
x = 1;           // TypeError: cannot assign to immutable variable `x`
```

Fix: `let mut x: int = 0;`

### Missing `use` declaration

```maniT
fn main() {
    println("hello");   // TypeError: undefined function `println`
}
```

Fix: add `use std::io;` and call `io::println("hello")`.

### `self` type annotation in impl methods

The parser accepts `self` as a bare keyword parameter (no `:` and type):

```maniT
impl Point {
    fn to_str(self) -> str { ... }       // CORRECT
    fn to_str(self: Point) -> str { ... } // ParseError
}
```

The receiver type is always inferred from the impl block context.

### Pattern matching on enums: must use `EnumName::Variant`

```maniT
match d {
    North => "N",          // TypeError: unknown pattern
    Direction::North => "N", // CORRECT
}
```

Enum patterns always require the qualified form `TypeName::VariantName`.

### Trit literals vs minus operator

`-` is both the trit literal −1 and the binary subtraction operator. In
ambiguous contexts, the parser may interpret `-` as unary minus rather than a
trit literal. Use explicit parentheses or a `let` binding:

```maniT
let neg_trit: trit = -;     // OK — context: trit type expected
tif my_trit {
    - => ...,               // OK — trit pattern context
}
```

### `>>` in generic type arguments

`Map<str, Vec<int>>` contains `>>` which the lexer tokenises as `RShift`. The
parser handles this automatically via the `pending_gt` mechanism, so nested
generics like `Vec<Vec<int>>` and `Map<str, Result<int, str>>` work correctly.
If you encounter a `ParseError` mentioning `RShift` in a type position, it is
a parser bug.

### Large immediate values in T3ISA

`TLIT Rx, #N` only supports |N| ≤ 797,161 (13-trit signed range). The emitter
does not currently handle larger constants via multi-instruction sequences for
the T3 backend. Very large literal constants may be truncated. Prefer using
variables and arithmetic to build large constants.

### Division by zero

The T3 emulator returns 0 for `a / 0` without signalling an error. Use the
`Result<T, E>` type and explicit guards:

```maniT
fn safe_div(a: int, b: int) -> Result<int, str> {
    if b == 0 {
        Err("division by zero")
    } else {
        Ok(a / b)
    }
}
```
