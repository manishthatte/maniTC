# Compiler internals

This document covers every source file in `maniTC/src/`, explaining the data
structures, key functions, and how the pieces fit together.

---

## Table of contents

1. [Overall pipeline](#1-overall-pipeline)
2. [main.rs — CLI and orchestration](#2-mainrs--cli-and-orchestration)
3. [error.rs — diagnostics](#3-errorrs--diagnostics)
4. [lexer.rs — tokenisation](#4-lexerrs--tokenisation)
5. [ast.rs — abstract syntax tree](#5-astrs--abstract-syntax-tree)
6. [parser/ — parsing](#6-parser--parsing)
7. [semantic/ — type checking](#7-semantic--type-checking)
8. [ir/ — intermediate representation](#8-ir--intermediate-representation)
9. [codegen_llvm.rs — LLVM backend](#9-codegen_llvmrs--llvm-backend)
10. [codegen_t3/ — T3ISA backend](#10-codegen_t3--t3isa-backend)
11. [Data flow between modules](#11-data-flow-between-modules)

---

## 1. Overall pipeline

```
File on disk
   │
   ▼  read_source()
Source text (&str)
   │
   ▼  Lexer::tokenize()
Vec<Token>
   │
   ▼  Parser::parse()
Program  (untyped AST)
   │
   ▼  SemanticAnalyzer::analyze()
TypedProgram  (every expression annotated with ManiType)
   │
   ▼  IRLowerer::lower()
IRModule  (three-address code, basic blocks, SSA-style temps)
   │
   ├──▶ [LLVM] emit_llvm_ir()  →  a.ll  →  clang  →  a.out
   │
   └──▶ [T3]   emit_t3_asm()   →  a.t3s
                 assemble()      →  a.t3b  +  a.t3d
                 run_emulator()  →  output
```

Each stage only ever calls the next — there are no back-edges. Every stage
produces a fresh, owned data structure; it does not mutate the previous stage's
output.

---

## 2. main.rs — CLI and orchestration

**File:** `maniTC/src/main.rs` (303 lines)

### CLI definition

Uses the `clap` crate. Subcommands are defined as an enum:

```rust
enum Commands {
    Compile { file, target, output, emit_ir },
    Check   { file },
    Lex     { file },
    Parse   { file },
    RunT3   { file },
}
```

### Pipeline functions

| Function | What it does |
|----------|-------------|
| `run_compile(file, target, output, emit_ir)` | Full pipeline: lex → parse → analyze → IR → codegen |
| `run_check(file)` | Lex → parse → analyze only (no codegen) |
| `run_lex(file)` | Lex and print each token with location |
| `run_parse(file)` | Lex → parse and pretty-print the AST |
| `run_t3(file)` | Load `.t3b` + `.t3d` sidecar and run the emulator |

### LLVM path

After writing `a.ll`, `run_compile` tries to exec `clang` via
`std::process::Command`. If `clang` is absent, it prints the manual command.

### T3 path

The assembler can fail independently from code generation. If `assemble()` returns
an error, the `.t3s` file is still written to disk so you can inspect it.

The string sidecar (`.t3d`) maps `address → string_literal_content` and is used
by the emulator to serve the `print_str` syscall. It is a newline-separated file
of `addr:content` pairs.

---

## 3. error.rs — diagnostics

**File:** `maniTC/src/error.rs` (86 lines)

### `Diagnostic`

Holds a source location plus a human-readable message:

```rust
pub struct Diagnostic {
    pub file: String,
    pub line: usize,
    pub col:  usize,
    pub message: String,
}
```

`Display` implementation produces `file:line:col: message`.

`Diagnostic::unknown(msg)` creates a location-less diagnostic for errors that
arise after line-number information is no longer available (e.g. during codegen).

### `CompileError`

```rust
pub enum CompileError {
    Lex(Diagnostic),
    Parse(Diagnostic),
    Type(Diagnostic),
    Codegen(Diagnostic),
}
```

Each variant wraps a `Diagnostic`. `Display` prepends a phase prefix
(`LexError:`, `ParseError:`, etc.).

### `CompileResult<T>`

Type alias `Result<T, CompileError>` — used as the return type of nearly every
function in the compiler. The `?` operator propagates errors through the pipeline
automatically.

### Error construction helpers

```rust
CompileError::lex(file, line, col, msg)     // from Lexer
CompileError::parse(file, line, col, msg)   // from Parser
CompileError::type_err(file, line, col, msg)// from SemanticAnalyzer
CompileError::codegen(msg)                  // from backends (no location)
```

---

## 4. lexer.rs — tokenisation

**File:** `maniTC/src/lexer.rs` (767 lines)

### `Span`

```rust
pub struct Span { pub line: usize, pub col: usize }
```

Every token carries a `Span`. The parser propagates spans into AST nodes so that
error messages always point to the original source.

### `Token`

```rust
pub struct Token { pub kind: TokenKind, pub span: Span }
```

### `TokenKind`

An enum with ~120 variants covering:

- **Literals:** `Int(i64)`, `Float(f64)`, `Str(String)`, `Char(char)`,
  `Bool(bool)`, `TernaryInt(i64)`
- **Keywords:** `Fn`, `Let`, `Mut`, `Pub`, `Struct`, `Enum`, `Impl`, `Trait`,
  `If`, `Elif`, `Else`, `Tif`, `Match`, `For`, `In`, `While`, `Loop`,
  `Return`, `Break`, `Continue`, `Use`, `Async`, `Spawn`, `Await`, `SelfKw`
- **Type keywords:** `IntKw`, `FloatKw`, `CharKw`, `StrKw`, `VoidKw`,
  `TritKw`, `TryteKw`, `T9Kw`, `T27Kw`, `T54Kw`, `Bool3Kw`
- **Bool3 values:** `True`, `False`, `Unknown`
- **Ternary ops:** `Tand`, `Tor`, `Tnot`, `Txor`
- **Operators:** all arithmetic, comparison, logical, bitwise, and compound
  assignment operators
- **Delimiters:** `LParen`, `RParen`, `LBrace`, `RBrace`, `LBracket`,
  `RBracket`, `Comma`, `Colon`, `ColonColon`, `Semi`, `Arrow`, `FatArrow`,
  `Dot`, `DotDot`, `DotDotEq`, `Pipe`, `Ampersand`, `Star`, `Plus`, `Minus`,
  `Lt`, `Gt`, `RShift`
- `Eof`

### `Lexer`

```rust
pub struct Lexer<'a> {
    src: &'a [char],
    pos: usize,
    line: usize,
    col:  usize,
    file: String,
}
```

### `tokenize() -> CompileResult<Vec<Token>>`

Main entry point. Iterates `pos` forward, calling helpers for different lexeme
classes:

| Helper | Handles |
|--------|---------|
| `lex_number()` | integers (dec/hex/bin/oct/balanced ternary), floats |
| `lex_string()` | string literals with escape sequences |
| `lex_char()` | character literals |
| `keyword_or_ident()` | maps text → keyword token or `Ident(String)` |

Standalone `+`, `-`, `0` at certain positions are lexed as integer literals
when appearing as trit literals. This is context-free — the parser must
disambiguate.

### `balanced_ternary_to_i64(s: &str) -> i64`

Iterates the digit string right-to-left. `+` → +1, `0` → 0, `-` → −1.
Accumulates `digit * 3^position`.

### Lexer tests

`#[cfg(test)]` block at the bottom verifies:
- `trit`, `tryte` etc. lex as type keywords
- `0t+0-` produces `TernaryInt(8)`

---

## 5. ast.rs — abstract syntax tree

**File:** `maniTC/src/ast.rs` (419 lines)

The AST represents the programmer's source code **after parsing** but **before**
type checking. Every node carries a `Span` for error reporting.

### Top-level structure

```rust
pub struct Program { pub items: Vec<Item> }

pub enum Item {
    FnDef(FnDef),
    StructDef(StructDef),
    EnumDef(EnumDef),
    ImplBlock(ImplBlock),
    TraitDef(TraitDef),
    UseDecl(UseDecl),
    GlobalVar(GlobalVar),
}
```

### `FnDef`

```rust
pub struct FnDef {
    pub name:     String,
    pub generics: Vec<String>,     // e.g. ["T", "U"]
    pub params:   Vec<Param>,
    pub ret_ty:   Option<Type>,
    pub body:     Option<Block>,   // None for extern/trait signatures
    pub is_pub:   bool,
    pub is_async: bool,
    pub span:     Span,
}
```

`generics` holds the raw type-parameter names. The semantic analyzer binds them
to concrete types via `type_params: HashMap<String, ManiType>`.

### `StructDef`

```rust
pub struct StructDef {
    pub name:     String,
    pub generics: Vec<String>,
    pub fields:   Vec<FieldDef>,
    pub is_pub:   bool,
    pub span:     Span,
}
```

### `EnumDef` / `EnumVariant`

```rust
pub struct EnumDef {
    pub name:     String,
    pub variants: Vec<EnumVariant>,
    pub is_pub:   bool,
    pub span:     Span,
}

pub struct EnumVariant {
    pub name:   String,
    pub fields: Vec<Type>,    // empty for unit variants
    pub span:   Span,
}
```

### `ImplBlock`

```rust
pub struct ImplBlock {
    pub ty:      String,          // type being implemented
    pub trait_:  Option<String>,  // Some("Describable") for trait impls
    pub methods: Vec<FnDef>,
    pub span:    Span,
}
```

### `TraitDef`

```rust
pub struct TraitDef {
    pub name:    String,
    pub methods: Vec<FnDef>,   // body is None for each
    pub is_pub:  bool,
    pub span:    Span,
}
```

### `Stmt`

```rust
pub enum Stmt {
    Let { name, ty, val, is_mut, span },
    Assign { lhs, op, rhs, span },       // op: None or Some(BinOpKind)
    Expr(Expr),
    Return(Option<Expr>, Span),
    Break(Span),
    Continue(Span),
    LocalStructDef(StructDef),
}
```

### `Expr`

A large enum. Key variants:

| Variant | Description |
|---------|-------------|
| `Lit(Lit, Span)` | Literal value |
| `Ident(String, Span)` | Name reference |
| `BinOp(Box<Expr>, BinOpKind, Box<Expr>, Span)` | Binary operation |
| `UnOp(UnOpKind, Box<Expr>, Span)` | Unary operation |
| `Call(Box<Expr>, Vec<Expr>, Span)` | Function call |
| `MethodCall(Box<Expr>, String, Vec<Expr>, Span)` | Method call |
| `Field(Box<Expr>, String, Span)` | Field access |
| `Index(Box<Expr>, Box<Expr>, Span)` | Array indexing |
| `If(IfExpr)` | Conditional |
| `Tif(TifExpr)` | Three-way branch |
| `Match(MatchExpr)` | Pattern match |
| `For(ForExpr)` | For loop |
| `While(WhileExpr)` | While loop |
| `Loop(Block, Span)` | Infinite loop |
| `Block(Block, Span)` | Block expression |
| `Struct(String, Vec<(String, Expr)>, Span)` | Struct literal |
| `Array(Vec<Expr>, Span)` | Array literal |
| `Tuple(Vec<Expr>, Span)` | Tuple literal |
| `Range(Box<Expr>, Box<Expr>, bool, Span)` | Range (bool = inclusive) |
| `Cast(Box<Expr>, Type, Span)` | Type cast |
| `Question(Box<Expr>, Span)` | `?` operator |
| `Lambda(Vec<Param>, Box<Expr>, Span)` | Anonymous function |
| `Spawn(Block, Span)` | Spawn a task |
| `Await(Box<Expr>, Span)` | Await a task |
| `Return(Option<Box<Expr>>, Span)` | Return expression |

### `Type`

```rust
pub enum Type {
    Named(String, Span),              // "int", "Point"
    Path(Vec<String>, Span),          // "std::io::T"
    Ref(Box<Type>, bool, Span),       // &T  or  &mut T
    Ptr(Box<Type>, bool, Span),       // *T  or  *mut T
    Array(Box<Type>, Option<usize>, Span),  // [T] or [T; N]
    Tuple(Vec<Type>, Span),           // (T, U)
    Fn(Vec<Type>, Box<Type>, Span),   // fn(T) -> U
    Generic(String, Vec<Type>, Span), // Vec<T>
    Infer(Span),                      // _
}
```

### `Lit`

```rust
pub enum Lit {
    Int(i64),
    Float(f64),
    Str(String),
    Char(char),
    Bool(bool),
    Bool3(i8),          // +1 / 0 / -1
    Trit(i8),           // +1 / 0 / -1
    TernaryInt(i64),    // from 0t+0- literals
    Null,
}
```

### `Pattern`

```rust
pub enum Pattern {
    Wildcard(Span),
    Ident(String, Span),
    Lit(Lit, Span),
    Struct(String, Vec<(String, Pattern)>, Span),
    Enum(String, Option<String>, Vec<Pattern>, Span),
    // ^^^^ variant_name, Some(enum_type_name), field_patterns
    Tuple(Vec<Pattern>, Span),
    Or(Vec<Pattern>, Span),
}
```

**Note on `Pattern::Enum`:** The first field is the *variant name*, the second is
`Some(enum_type_name)` when path syntax `EnumName::Variant` is used, or `None`
when just the variant name is written. The IR lowerer uses the second field to
look up the variant's integer index.

---

## 6. parser/ — parsing

The parser is a hand-written recursive-descent parser split into three files:

### parser/mod.rs — driver

Holds the `Parser` struct and all item-level parsing (functions, structs, enums,
impl blocks, traits, use declarations, globals).

**`Parser` struct:**

```rust
pub struct Parser {
    pub(super) tokens:     Vec<Token>,
    pub(super) pos:        usize,
    pub(super) file:       String,
    pub(super) pending_gt: bool,   // >> split into > + >
}
```

**`pending_gt` flag:** Generic type syntax `Vec<Map<str, int>>` contains `>>` at
the end, which the lexer produces as `RShift`. When the parser needs a `>` to
close generic arguments, it calls `eat_gt()`. If it sees `RShift`, it consumes
it, sets `pending_gt = true`, and returns `true`. The next call to `peek()` then
returns `Gt` (the second `>`) without consuming another real token.

**Key position helpers:**

| Method | Behaviour |
|--------|-----------|
| `peek()` | Returns `&TokenKind` of the current token (or `&Gt` if `pending_gt`) |
| `peek2()` | Returns `&TokenKind` of the next-next token |
| `peek_tok()` | Returns `&Token` for span information |
| `advance()` | Consumes and returns the current token |
| `eat(kind)` | Consumes if current matches; returns bool |
| `expect(kind)` | Consumes if current matches; returns `Span` or error |
| `expect_ident()` | Consumes an identifier; returns `(String, Span)` or error |
| `eat_gt()` | Consumes `>` or the first half of `>>` |
| `span()` | Span of the current token |

**Item parsing methods:**

| Method | Produces |
|--------|---------|
| `parse()` | `Program` |
| `parse_item()` | `Item` — dispatches on first token |
| `parse_fn_def(is_pub)` | `FnDef` |
| `parse_struct_def(is_pub)` | `StructDef` |
| `parse_enum_def(is_pub)` | `EnumDef` |
| `parse_impl_block()` | `ImplBlock` |
| `parse_trait_def(is_pub)` | `TraitDef` |
| `parse_use_decl()` | `UseDecl` |

`parse_fn_def` also handles `async fn`, `self` receiver parameters, optional
generic parameters `<T, U>`, optional return type `-> T`, and optional body.

### parser/types.rs — type expressions

**`parse_type()`** dispatches on the current token:

| First token | Parsed as |
|-------------|-----------|
| `_` ident | `Type::Infer` |
| `&` | `Type::Ref(parse_type(), mutable)` |
| `*` | `Type::Ptr(parse_type(), mutable)` |
| `[` | `Type::Array(parse_type(), opt_size)` |
| `(` | `Type::Tuple(...)` or parenthesised type |
| `fn` | `Type::Fn(params, ret)` |
| identifier / keyword | Named type, optionally followed by `<...>` for generic args or `::...` for path |

**`parse_generic_args()`** handles `<T, U, ...>` after a type name, using
`eat_gt()` to handle `>>` correctly.

**`parse_pattern()`** / **`parse_single_pattern()`** parse patterns, including
or-patterns (`|`), struct patterns, enum patterns, tuple patterns, and guards.

### parser/stmts.rs — statements

**`parse_block()`** repeatedly calls `parse_stmt()` until `}`.

**`parse_stmt()`** dispatches on:
- `let` / `mut` → variable binding
- `struct` → local struct definition
- `return` / `break` / `continue` → control flow
- otherwise → expression, then check for assignment operator

### parser/exprs.rs — expressions

Implements a precedence-climbing recursive descent:

```
parse_expr()
  → parse_range_expr()  (..)
    → parse_or_expr()   (||)
      → parse_and_expr()  (&&)
        → parse_ternary_logic_expr()  (tand, tor, txor)
          → parse_cmp_expr()   (==, <, >, etc.)
            → parse_bitor_expr()   (|)
              → parse_bitxor_expr()  (^)
                → parse_bitand_expr()  (&)
                  → parse_shift_expr()  (<<, >>)
                    → parse_additive_expr()  (+, -)
                      → parse_multiplicative_expr()  (*, /, %)
                        → parse_unary_expr()  (-, !, tnot, *, &)
                          → parse_postfix_expr()  (., [], ())
                            → parse_primary_expr()
```

`parse_primary_expr` handles literals, identifiers, `(expr)`, blocks `{...}`,
`if`, `tif`, `match`, `for`, `while`, `loop`, `spawn`, `await`, struct literals,
array literals, tuple literals, and lambda expressions.

---

## 7. semantic/ — type checking

### semantic/types.rs — type system

**`ManiType`** is the canonical type representation inside the compiler:

```rust
pub enum ManiType {
    Int, Float, Bool, Bool3, Trit, Tryte, T9, T27, T54, Str, Char, Void,
    Array(Box<ManiType>, Option<usize>),
    Tuple(Vec<ManiType>),
    Struct(String),            // user-defined struct
    Enum(String),              // user-defined enum
    Fn(Vec<ManiType>, Box<ManiType>),
    Generic(String, Vec<ManiType>),
    Unknown,                   // type inference placeholder
}
```

Helper predicates: `is_ternary()`, `is_numeric()`, `is_comparable()`, `display()`.

**`TypedProgram`** — the output of semantic analysis:

```rust
pub struct TypedProgram {
    pub functions:    Vec<TypedFnDef>,
    pub structs:      Vec<StructDef>,
    pub struct_fields: HashMap<String, Vec<(String, ManiType)>>,
    pub enums:        Vec<EnumDef>,
    pub globals:      Vec<TypedGlobal>,
}
```

Every `TypedExpr` carries a `ty: ManiType`:

```rust
pub struct TypedExpr {
    pub kind: TypedExprKind,
    pub ty:   ManiType,
    pub span: Span,
}
```

`TypedExprKind` mirrors `Expr` but with sub-expressions replaced by `TypedExpr`.

### semantic/scope.rs — symbol table

```rust
pub struct SymbolInfo { pub ty: ManiType, pub is_mut: bool }
pub type Scope = HashMap<String, SymbolInfo>;

pub struct SymbolTable { scopes: Vec<Scope> }
```

`push_scope()` / `pop_scope()` manage nesting (one scope per block).
`define(name, ty, is_mut)` adds to the current (innermost) scope.
`lookup(name)` searches from innermost to outermost; returns `None` if not found.

### semantic/analyzer.rs — the heart of the type checker

**`SemanticAnalyzer` fields:**

| Field | Purpose |
|-------|---------|
| `symbols` | Symbol table for local variables |
| `functions` | `name → (param_types, ret_type)` for all functions |
| `structs` | `name → [(field_name, field_type)]` |
| `enums` | `name → [(variant_name, [field_types])]` |
| `current_fn_ret` | Expected return type of the function being checked |
| `lambda_counter` | Generates `__lambda_0`, `__lambda_1`, … names |
| `lambda_fns` | Collected lambda functions (added to TypedProgram) |
| `user_method_types` | `type_name → method_name → return_type` |
| `trait_defs` | `trait_name → [(method_name, param_tys, ret_ty)]` |
| `trait_impls` | Set of `(type_name, trait_name)` pairs |
| `type_params` | Generic type parameter bindings in current scope |
| `current_impl_type` | Type name of the `impl` block being processed (`Some("Point")`) |

**Two-pass strategy for items:**

1. `collect_declarations()` — first pass: registers all function signatures,
   struct fields, enum variants, trait definitions, and impl method signatures
   *without* checking bodies. This allows forward references.
2. `analyze()` — second pass: walks every function body, impl block method, etc.
   and calls `check_fn()` which calls `check_block()` → `check_stmt()` →
   `check_expr()`.

**`collect_declarations()` for `ImplBlock`:**

```
for each method in impl block:
    qm.name = "TypeName::method_name"   // qualified name
    register_fn(&qm)                    // adds to self.functions
    user_method_types[type][method] = return_type
if trait impl:
    validate all trait methods are present
    trait_impls.insert((type, trait_name))
```

**`register_fn()`** populates `self.functions` with the function's parameter types
and return type. For generic functions, `type_params` is temporarily populated
during resolution.

**`resolve_type(ast::Type) -> ManiType`:**

1. Check `type_params` first (handles generic `T` parameters).
2. Call `name_to_manitype()` for named types.
3. Recurse for compound types.

**`name_to_manitype(name) -> ManiType`:**

- Primitive names (`"int"`, `"float"`, `"trit"`, …) map directly.
- `"Self"` resolves via `current_impl_type` — checks `self.enums` to return
  `ManiType::Enum` vs `ManiType::Struct`.
- User-defined names: checks `self.structs` then `self.enums`.
- Generic names (`"Vec"`, `"Result"`, `"Map"`, …): returns `ManiType::Generic`.

**`check_expr()` — key cases:**

| Expression kind | Type inference |
|-----------------|----------------|
| `Lit` | Directly mapped (Int → Int, Float → Float, etc.) |
| `Ident(name)` | Looks up symbol table, then `functions`. If name is `"EnumName::Variant"`, checks `self.enums` and returns `ManiType::Enum("EnumName")`. |
| `BinOp` | `binop_type()` — checks operand types, infers result |
| `Call` | Looks up callee in `functions`, returns ret type |
| `MethodCall(obj, method, args)` | `resolve_method_type(obj_ty, method)` |
| `Field(obj, field)` | Looks up `structs[obj_type][field]` |
| `If / Tif / Match` | All arms must agree on type |
| `Block` | Type of last expression (or void) |

**`resolve_method_type(obj_ty, method)`:**

1. Check `user_method_types[type_name][method]` — user-defined impl methods.
2. Fall through to built-in method table (`.len()`, `.push()`, `.get()`, etc.).

**Built-in function registration (`register_builtins()`):**

All stdlib functions are pre-registered with their signatures:
- `io::println`, `io::print`, `io::print_trit`, `io::print_bool3`, …
- `math::sqrt`, `math::abs`, `math::to_balanced_ternary`, …
- `fmt::format`, `fmt::show_int`, …
- `ternary::trit_to_int`, `ternary::pack_trits`, …
- Collection constructors: `Vec::new`, `Map::new`, `Set::new`, …
- Concurrency: `channel`, `Mutex::new`, `AtomicTrit::new`, `Barrier::new`, …
- Async: `async::sleep`, `async::yield_now`, `async::select`, …

---

## 8. ir/ — intermediate representation

### ir/types.rs — IR data structures

The IR is a simple three-address code with basic blocks. It is not full SSA;
variables are stored in stack slots (via `Alloca`) and loaded/stored explicitly.

**`IRModule`:**

```rust
pub struct IRModule {
    pub name:            String,
    pub functions:       Vec<IRFunction>,
    pub globals:         Vec<IRGlobal>,
    pub string_literals: Vec<(String, String)>,  // (label, content)
    pub struct_sizes:    HashMap<String, usize>,  // name → field count
}
```

**`IRFunction`:**

```rust
pub struct IRFunction {
    pub name:      String,
    pub params:    Vec<(String, IRType)>,
    pub ret_ty:    IRType,
    pub blocks:    Vec<IRBlock>,
    pub is_extern: bool,
}
```

**`IRBlock`:**

```rust
pub struct IRBlock {
    pub label:  String,
    pub instrs: Vec<IRInstr>,
    pub term:   IRTerminator,
}
```

**`IRTerminator`:**

```rust
pub enum IRTerminator {
    Jump(String),                               // unconditional
    CondJump(IRValue, String, String),          // bool condition
    TritJump(IRValue, String, String, String),  // pos/zero/neg targets
    Return(Option<IRValue>),
    Unreachable,
}
```

**`IRInstr`:**

```rust
pub enum IRInstr {
    Alloca  { dst, ty },
    Store   { ptr, val, ty },
    Load    { dst, ptr, ty },
    BinOp   { dst, op, lhs, rhs, ty },
    UnOp    { dst, op, operand, ty },
    Call    { dst, func, args, ret_ty },
    CallIndirect { dst, fp, args, ret_ty },     // call through function pointer
    GetPtr  { dst, ptr, idx, ty },              // array element pointer
    GetField { dst, ptr, field_idx, ty },       // struct field pointer
    SetField { ptr, field_idx, val, ty },       // struct field store
    Cast    { dst, val, from_ty, to_ty },
    Phi     { dst, inputs, ty },                // SSA φ-node (rarely used)
    Nop,
}
```

**`IRValue`:**

```rust
pub enum IRValue {
    Temp(IRTemp),           // %t0, %t1, …
    Const(IRConst),         // literal constant
    Global(String),         // @global_name
    Arg(usize),             // function argument
    Void,
}
```

**`IRType`:**

```rust
pub enum IRType {
    I64, F64, Bool, Void,
    Ptr(Box<IRType>),
    Array(Box<IRType>, usize),
    Struct(String),
}
```

### ir/lower.rs — IR lowerer

**`IRLowerer` struct:**

```rust
struct IRLowerer {
    temp_counter:    usize,
    label_counter:   usize,
    string_literals: Vec<(String, String)>,
    blocks:          Vec<IRBlock>,
    current_block:   usize,
    locals:          HashMap<String, IRValue>,  // name → alloca ptr
    structs:         HashMap<String, usize>,    // name → field count
    enum_variants:   HashMap<String, Vec<String>>, // enum_name → variant names
}
```

**`enum_variants`** is populated during `lower()` before any function body is
lowered:

```rust
for enum_def in &typed_program.enums {
    lowerer.enum_variants.insert(
        enum_def.name.clone(),
        enum_def.variants.iter().map(|v| v.name.clone()).collect(),
    );
}
```

This map is used in two places:

1. **`TypedExprKind::Ident` with `::` in name** — `Direction::North` is lowered
   as `IRValue::Const(IRConst::Int(0))` (index 0), not as a global reference.

2. **`Pattern::Enum` matching** — the scrutinee is compared against the variant's
   integer index using `IRBinOp::IEq`.

**Control flow lowering:**

Loops are lowered as:

```
  [header block]  ← loop body continues here
  cond test
  [body block]
  jump → header
  [after block]   ← break targets here
```

`if` expressions produce `then_block`, `else_block`, and `merge_block`.
`tif` expressions produce `pos_block`, `zero_block`, `neg_block`, `merge_block`.
`match` produces one block per arm plus a `merge_block`.

**`intern_string(s) -> String`** deduplicates string literals:

```rust
fn intern_string(&mut self, s: &str) -> String {
    if let Some(lbl) = self.string_literals.iter()
        .find(|(_, v)| v == s).map(|(l, _)| l.clone())
    {
        return lbl;
    }
    let lbl = format!("str{}", self.string_literals.len());
    self.string_literals.push((lbl.clone(), s.to_string()));
    lbl
}
```

---

## 9. codegen_llvm.rs — LLVM backend

**File:** `maniTC/src/codegen_llvm.rs` (1153 lines)

Entry point: `emit_llvm_ir(module: &IRModule) -> String`

Produces a valid LLVM IR text file (`.ll`) that `clang` can compile to a native
binary.

### Structure of emitted file

1. **Preamble** — `target triple`, `target datalayout`, `declare` for external
   C functions (`printf`, `malloc`, etc.)
2. **String literal globals** — `@str0 = internal constant [...] c"..."` for
   each interned string
3. **Global variables** — one `@name = global i64 0` per `IRGlobal`
4. **Helper functions** — `__maniT_print_trit`, `__maniT_print_bool3`
5. **User functions** — one LLVM `define` per `IRFunction`

### Type mapping (`emit_type`)

| `IRType` | LLVM type |
|----------|-----------|
| `I64` | `i64` |
| `F64` | `double` |
| `Bool` | `i1` |
| `Void` | `void` |
| `Ptr(T)` | `T*` |
| `Array(T, N)` | `[N x T]` |
| `Struct(name)` | `%struct.name` (opaque, as `i64*`) |

### Function emission

Each `IRBlock` is emitted as an LLVM basic block label followed by its
instructions. `IRTerminator` maps to:

- `Jump(lbl)` → `br label %lbl`
- `CondJump(val, t, f)` → `br i1 %val, label %t, label %f`
- `TritJump(val, p, z, n)` → comparison chain
- `Return(Some(v))` → `ret T %v`
- `Return(None)` → `ret void`

### Instruction emission

`IRInstr::BinOp` maps `IRBinOp` to LLVM instructions:

| `IRBinOp` | LLVM | Notes |
|-----------|------|-------|
| `IAdd` | `add` | |
| `ISub` | `sub` | |
| `IMul` | `mul` | |
| `IDiv` | `sdiv` | signed division |
| `IMod` | `srem` | |
| `FAdd` | `fadd` | |
| `FMul` | `fmul` | |
| `IEq` | `icmp eq` | |
| `ILt` | `icmp slt` | |
| `And` | `and` | logical/bitwise |
| `Or` | `or` | |
| `Xor` | `xor` | |
| `Shl` | `shl` | |
| `Shr` | `ashr` | arithmetic right-shift |

`IRInstr::Call` emits `call T @func(...)`. Built-in functions map to syscall
helpers:

```
io::println  →  call to __maniT_println
math::sqrt   →  call to llvm.sqrt.f64
```

`IRInstr::CallIndirect` emits `call T (T1, T2) %fp(...)` — a function pointer
call using LLVM's typed function pointer syntax.

### LLVMEmitter

Internal state:
```rust
struct LLVMEmitter {
    assigns:     HashMap<String, String>,  // simple copy-prop optimisation
    anon_counter: usize,
    current_ret_ty: IRType,
}
```

`assigns` tracks `%t0 = %t1` copy instructions so downstream uses can reference
`%t1` directly, reducing register pressure in the emitted IR.

---

## 10. codegen_t3/ — T3ISA backend

### codegen_t3/mod.rs

Re-exports the public API:

```rust
pub use emitter::emit_t3_asm;
pub use assembler::{assemble, write_t3_binary, read_t3_binary};
pub use emulator::run_emulator;
```

### codegen_t3/isa.rs — instruction set

See the full [T3ISA Reference](t3isa-reference.md) for the instruction set.

Key items in this file:

- Constants `T3_MAX = 3_812_798_742_493` and `T3_MIN` (±(3²⁷−1)/2)
- `clamp27(v)` — saturating clamp to 27-trit range
- `sign_i64(v) -> i8` — extract sign as trit
- `Opcode` enum (29 variants, repr(i64), each variant = its opcode number)
- `Opcode::from_i64(v)` — decode opcode from word
- Encoding constants: `P18 = 3^18`, `P13 = 3^13`, `P8 = 3^8`, `P3 = 3^3`
- `encode(opcode, r1, r2, r3, imm) -> i64` — 5-field standard encoding
- `encode_wide(opcode, r1, wide_imm) -> i64` — 13-trit immediate (TLIT/JUMP/CALL)
- `decode(word) -> (op, r1, r2, r3, imm)` — inverse of encode
- `decode_tlit_imm(word) -> i64` — extract signed immediate from TLIT

### codegen_t3/assembler.rs

**Entry:** `assemble(asm_text: &str) -> Result<(Vec<i64>, HashMap<usize, String>), String>`

**Two-pass assembly:**

Pass 1 — `label_map: HashMap<String, usize>` populated by two detection rules:

1. **Rule 1** (line ends with `:`, no whitespace): Standalone label on its own
   line. `trim_end_matches(':')` gives the label name.

2. **Rule 2** (`find_label_colon(line)` finds a non-`::` colon): Label followed
   by an instruction on the same line. `find_label_colon` skips `:` that are
   part of `::` (path separators in qualified names like `Direction::to_str:`).

`find_label_colon(line)` — helper added to fix the `::` ambiguity:

```rust
fn find_label_colon(line: &str) -> Option<usize> {
    let bytes = line.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b == b':' {
            let prev_colon = i > 0 && bytes[i-1] == b':';
            let next_colon = bytes.get(i+1).copied() == Some(b':');
            if !prev_colon && !next_colon { return Some(i); }
        }
    }
    None
}
```

`.data:` / `.data` sections hold string literals as `label: .string "content"`.
Their labels are placed past code (at `code_size + 1024 + i`) to keep them out
of the instruction range.

Pass 2 — encodes each `RawInstr` to a 27-trit word using the ISA encoding
functions. `TBRANCH` pseudo-instruction expands to three real words:
`TBR_POS`, `TBR_ZERO`, and `JUMP`.

**`write_t3_binary` / `read_t3_binary`:** Simple 8-byte-per-word little-endian
binary file I/O.

### codegen_t3/emitter.rs

**Entry:** `emit_t3_asm(module: &IRModule) -> String`

Translates `IRModule` to T3ISA assembly text. The emitter maintains its own
stack frame layout — variables are spill-stored relative to `R26` (stack pointer).

**Register allocation:** Purely stack-based. All temporaries are stored to and
loaded from the stack. No register allocation is performed. Registers `R1`–`R25`
are used for function arguments and intermediate calculations; `R24` is the
dedicated return-value stash register; `R26` is the stack pointer.

**Calling convention:**

- Arguments: `R1` = arg 0, `R2` = arg 1, …, `R8` = arg 7
- Return value: `R1`
- The callee saves/restores nothing; the caller saves any live values it needs
  across a call by pushing them to the stack.

**Function structure:**

```asm
FunctionName:
  FunctionName_entry:
    TSUB R26, R26, #N     ; allocate N stack slots for locals
    ...
    ; body
    ...
    TADD R26, R26, #N     ; deallocate
    RET
```

**Syscall numbers** used by built-in functions:

| Syscall # | Function |
|-----------|---------|
| 1 | `print_int` |
| 2 | `print_float` |
| 3 | `print_str` |
| 4 | `print_newline` |
| 5 | `print_trit` |
| 6 | `print_bool3` |
| 20 | `Vec::new` |
| 21 | `Vec::push` |
| 22 | `Vec::pop` |
| 23 | `Vec::len` |
| 24 | `Vec::get` |
| 50 | `Map::new` |
| 51–57 | `Map` operations |
| 80 | `Set::new` |
| 81–85 | `Set` operations |
| 100 | `Mutex::new` |
| 101–102 | `Mutex::lock / unlock` |
| 103 | `Channel::new` (channel()) |
| 104–105 | `Channel::send / recv` |
| 109 | `AtomicTrit::new` |
| 110–111 | `AtomicTrit::get / set` |
| … | many more |

See `emitter.rs` for the complete syscall table.

### codegen_t3/emulator.rs

**Entry:** `run_emulator(words: Vec<i64>, str_data: HashMap<usize, String>) -> Vec<String>`

Returns accumulated output pieces (printed strings, integers, newlines). The
caller joins them to display the program's output.

**`Emulator` struct (key fields):**

```rust
struct Emulator {
    regs:        [i64; 27],           // R0–R26
    pc:          usize,
    memory:      Vec<i64>,            // 65536 word-addressable cells
    flags:       i8,                  // -1 / 0 / +1
    halted:      bool,
    output:      Vec<String>,
    string_data: HashMap<usize, String>,
    call_stack:  Vec<usize>,
    heap_ptr:    usize,
    heap_objs:   HashMap<usize, HeapObj>,
    tasks:       Vec<Task>,           // cooperative task queue
    current_task: usize,
}
```

`memory` is indexed by T3ISA word addresses. `R26` (stack pointer) starts at
the top of memory and grows downward. The heap starts at a fixed base address
and grows upward via `heap_alloc_obj()`.

**`HeapObj`** — runtime heap objects:

```rust
enum HeapObj {
    Vec(Vec<i64>),
    Map(HashMap<i64, i64>),
    Set(std::collections::HashSet<i64>),
    Deque(std::collections::VecDeque<i64>),
    Channel(std::collections::VecDeque<i64>),
    Trie(HashMap<Vec<i8>, i64>),
    Mutex { locked: bool, value: i64 },
    AtomicTrit(i8),
    Barrier { count: usize, needed: usize },
    Semaphore(i64),
    TaskResult(Option<i64>),
}
```

Heap objects are reference-counted by address. The emulator passes addresses
(handle integers) as the value of collection variables.

**Instruction execution loop:**

```rust
loop {
    let word = memory[pc];
    let (op, r1, r2, r3, imm) = decode(word);
    match Opcode::from_i64(op) {
        Tadd  → regs[r1] = clamp27(regs[r2] + regs[r3])
        Tsub  → regs[r1] = clamp27(regs[r2] - regs[r3])
        Tlit  → regs[r1] = decode_tlit_imm(word)
        Load  → regs[r1] = memory[(regs[r2] + imm) as usize]
        Store → memory[(regs[r1] + imm) as usize] = regs[r2]
        Call  → { call_stack.push(pc+1); pc = label_addr }
        Ret   → { pc = call_stack.pop() }
        Syscall → handle_syscall(imm)
        ...
    }
}
```

**Cooperative multitasking:**

When `SYSCALL #async_yield` is executed, the emulator saves the current
task's state (`pc`, `regs`, `call_stack`, `flags`) and switches to the
next task in the queue. Tasks are stored in `tasks: Vec<Task>` and
`current_task` tracks the active one.

---

## 11. Data flow between modules

```
lexer.rs        produces:  Vec<Token>
                             │
parser/         consumes:  Vec<Token>
                produces:  Program (AST)
                             │
semantic/       consumes:  &Program
                produces:  TypedProgram
                             │
ir/             consumes:  &TypedProgram
                produces:  IRModule
                             │
codegen_llvm.rs consumes:  &IRModule
                produces:  String (LLVM IR text)
                             │
codegen_t3/     consumes:  &IRModule
                produces:  String (T3ISA ASM text)
                             │
assembler.rs    consumes:  &str (ASM text)
                produces:  (Vec<i64>, HashMap<usize,String>)
                             │
emulator.rs     consumes:  Vec<i64> + HashMap<usize,String>
                produces:  Vec<String> (output)
```

Each arrow is a function call with no shared mutable state. The compiler is
entirely single-threaded.
