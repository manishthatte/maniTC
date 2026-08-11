# HOW-TO: Adding a language feature

This guide explains how to extend the maniT compiler with a new language construct.
It walks through every layer of the pipeline using a concrete example: adding a
`repeat N { body }` loop construct that executes `body` exactly N times.

---

## Overview of the layers

Adding any syntactic feature requires touching these files in order:

1. **`lexer.rs`** — new keyword or token if needed
2. **`ast.rs`** — new AST node
3. **`parser/stmts.rs`** or **`parser/exprs.rs`** — parse the new syntax
4. **`semantic/types.rs`** — new `TypedExprKind` or `TypedStmtKind` variant
5. **`semantic/analyzer.rs`** — type-check the new construct
6. **`ir/lower.rs`** — lower to IR basic blocks
7. **`codegen_llvm.rs`** — LLVM emission (if applicable)
8. **`codegen_t3/emitter.rs`** — T3ISA emission (if applicable)

---

## Step 1 — Add the keyword to the lexer

Open `maniTC/src/lexer.rs`.

Find the `keyword_or_ident` function (around line 150). Add a mapping:

```rust
"repeat" => TokenKind::Repeat,
```

Add the variant to `TokenKind`:

```rust
// in the TokenKind enum, near the other keywords
Repeat,
```

The lexer will now produce `TokenKind::Repeat` when it encounters the text
`repeat`.

---

## Step 2 — Add an AST node

Open `maniTC/src/ast.rs`.

Add a new expression variant inside the `Expr` enum:

```rust
/// repeat N { body } — execute body exactly N times
RepeatLoop(Box<Expr>, Block, Span),
// ^^       count      body
```

---

## Step 3 — Parse the new syntax

Open `maniTC/src/parser/exprs.rs`.

In `parse_primary_expr()`, add a case for `TokenKind::Repeat`:

```rust
TokenKind::Repeat => {
    let span = self.span();
    self.advance();                          // consume `repeat`
    let count = self.parse_expr()?;          // parse the count expression
    let body = self.parse_block()?;          // parse { body }
    Ok(Expr::RepeatLoop(Box::new(count), body, span))
}
```

---

## Step 4 — Add typed AST nodes

Open `maniTC/src/semantic/types.rs`.

Add a variant to `TypedExprKind`:

```rust
RepeatLoop(Box<TypedExpr>, TypedBlock),
// ^^        count           body
```

---

## Step 5 — Type-check the new construct

Open `maniTC/src/semantic/analyzer.rs`.

In `check_expr()`, add a case for `Expr::RepeatLoop`:

```rust
Expr::RepeatLoop(count_expr, body, _) => {
    let tcount = self.check_expr(count_expr, Some(&ManiType::Int))?;
    if !matches!(tcount.ty, ManiType::Int) {
        return Err(CompileError::type_err(
            &self.file, span.line, span.col,
            "repeat count must be int",
        ));
    }
    self.symbols.push_scope();
    let tbody = self.check_block(body)?;
    self.symbols.pop_scope();
    Ok(TypedExpr {
        kind: TypedExprKind::RepeatLoop(Box::new(tcount), tbody),
        ty: ManiType::Void,
        span,
    })
}
```

---

## Step 6 — Lower to IR

Open `maniTC/src/ir/lower.rs`.

In `lower_expr()`, add a case for `TypedExprKind::RepeatLoop`:

```rust
TypedExprKind::RepeatLoop(count_expr, body) => {
    // Emit:
    //   %counter = alloca
    //   store 0 → %counter
    // header:
    //   %cur = load %counter
    //   %cmp = cur < count
    //   condjump %cmp → body_block, after_block
    // body:
    //   lower body
    //   %next = cur + 1
    //   store %next → %counter
    //   jump → header
    // after:

    let count_val = self.lower_expr(count_expr);

    let counter_ptr = self.fresh_temp();
    self.emit(IRInstr::Alloca { dst: counter_ptr.clone(), ty: IRType::I64 });
    self.emit(IRInstr::Store {
        ptr: IRValue::Temp(counter_ptr.clone()),
        val: IRValue::Const(IRConst::Int(0)),
        ty: IRType::I64,
    });

    let header_lbl = self.fresh_label("repeat_header");
    let body_lbl   = self.fresh_label("repeat_body");
    let after_lbl  = self.fresh_label("repeat_after");

    self.set_term(IRTerminator::Jump(header_lbl.clone()));
    let header_idx = self.new_block(header_lbl.clone());
    self.switch_to(header_idx);

    let cur = self.fresh_temp();
    self.emit(IRInstr::Load {
        dst: cur.clone(),
        ptr: IRValue::Temp(counter_ptr.clone()),
        ty: IRType::I64,
    });

    let cmp = self.fresh_temp();
    self.emit(IRInstr::BinOp {
        dst: cmp.clone(),
        op: IRBinOp::ILt,
        lhs: IRValue::Temp(cur.clone()),
        rhs: count_val,
        ty: IRType::Bool,
    });
    self.set_term(IRTerminator::CondJump(
        IRValue::Temp(cmp), body_lbl.clone(), after_lbl.clone(),
    ));

    let body_idx = self.new_block(body_lbl);
    self.switch_to(body_idx);
    self.lower_block(body);

    let next = self.fresh_temp();
    self.emit(IRInstr::BinOp {
        dst: next.clone(),
        op: IRBinOp::IAdd,
        lhs: IRValue::Temp(cur),
        rhs: IRValue::Const(IRConst::Int(1)),
        ty: IRType::I64,
    });
    self.emit(IRInstr::Store {
        ptr: IRValue::Temp(counter_ptr),
        val: IRValue::Temp(next),
        ty: IRType::I64,
    });
    self.set_term(IRTerminator::Jump(header_lbl));

    let after_idx = self.new_block(after_lbl);
    self.switch_to(after_idx);

    IRValue::Void
}
```

---

## Step 7 — Test

Write a test program `test_repeat.mt`:

```maniT
use std::io;

fn main() {
    let mut total: int = 0;
    repeat 5 {
        total = total + 1;
    }
    io::print_int(total);   // should print 5
    io::newline();
}
```

Compile and run:

```bash
manitc compile --target t3 test_repeat.mt
manitc run-t3 a.t3b
```

Also test edge cases:
- `repeat 0 { ... }` — body should never run
- `repeat 1 { ... }` — body runs exactly once
- A repeat containing a `break` statement

---

## Checklist for any new feature

- [ ] New `TokenKind` variant in `lexer.rs` (if new keyword)
- [ ] New mapping in `keyword_or_ident()` in `lexer.rs`
- [ ] New `Expr` / `Stmt` / `Item` variant in `ast.rs`
- [ ] Parser case in the appropriate `parse_*` function
- [ ] New `TypedExprKind` / `TypedStmtKind` variant in `semantic/types.rs`
- [ ] Type-checking case in `semantic/analyzer.rs`
- [ ] IR lowering case in `ir/lower.rs`
- [ ] LLVM emission case in `codegen_llvm.rs` (if targeting LLVM)
- [ ] T3ISA emission case in `codegen_t3/emitter.rs` (if targeting T3)
- [ ] Test program written and passing
- [ ] Edge cases considered (zero, negative, large values)

---

## Adding a built-in function

Built-in functions (like `io::println`) don't need AST changes — they are
registered programmatically.

### Step 1 — Register in SemanticAnalyzer

In `semantic/analyzer.rs`, `register_builtins()`:

```rust
// io::my_new_function(n: int) -> str
self.functions.insert(
    "io::my_new_function".to_string(),
    (vec![ManiType::Int], ManiType::Str),
);
```

### Step 2 — Handle in T3 emitter

In `codegen_t3/emitter.rs`, in the built-in call dispatch section:

```rust
"io::my_new_function" => {
    // emit: load arg into R1
    // emit: SYSCALL #NNN
    // emit: result in R1
    self.emit_line("SYSCALL #200   ; io::my_new_function");
}
```

### Step 3 — Handle in T3 emulator

In `codegen_t3/emulator.rs`, `handle_syscall(n)`:

```rust
200 => {
    // R1 = int argument
    let n = self.regs[1];
    let result = format!("computed_{}", n);
    // Store result as a string and return its address
    let addr = self.intern_temp_string(result);
    self.regs[1] = addr as i64;
}
```

### Step 4 — Handle in LLVM emitter (optional)

In `codegen_llvm.rs`, in the call emission:

```rust
"io::my_new_function" => {
    format!("call i8* @__maniT_my_new_function(i64 {})", operand)
}
```

Add the declaration at the top of the LLVM IR output:

```rust
"declare i8* @__maniT_my_new_function(i64)"
```

---

## Adding a new type

### Step 1 — Add to `ManiType`

In `semantic/types.rs`:

```rust
pub enum ManiType {
    // ...existing variants...
    MyNewType,
}
```

Update `display()` and `is_numeric()` / `is_ternary()` / `is_comparable()` as
appropriate.

### Step 2 — Add keyword to lexer

In `lexer.rs`:

```rust
"mytype" => TokenKind::MyTypeKw,
```

### Step 3 — Map in parser

In `parser/types.rs`, `type_keyword_to_name()`:

```rust
TokenKind::MyTypeKw => Some("mytype".to_string()),
```

### Step 4 — Map in semantic analyser

In `semantic/analyzer.rs`, `name_to_manitype()`:

```rust
"mytype" => Ok(ManiType::MyNewType),
```

### Step 5 — Map to IR type

In `ir/lower.rs` or `ir/types.rs`, add `IRType::MyNewType` and update
`manitype_to_irtype()`:

```rust
ManiType::MyNewType => IRType::I64,  // or whatever suits
```

---

## Adding a new syscall

1. Choose a syscall number not in the [existing table](../t3isa-reference.md#8-syscall-table).
2. Register the function in `register_builtins()`.
3. Emit `SYSCALL #N` in `emitter.rs` for the function.
4. Handle `N =>` in `emulator.rs` `handle_syscall()`.
5. Optionally emit a C call in `codegen_llvm.rs`.
