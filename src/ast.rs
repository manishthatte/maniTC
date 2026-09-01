// ---------------------------------------------------------------------------
// Span
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    pub line: usize,
    pub col: usize,
    /// P8: the merged stdlib module this span came from, if any.
    ///
    /// `stdlib_expand` parses each ManiT-source stdlib module with its OWN line
    /// numbering and appends the items to the user's program. A span carried a
    /// line and a column and nothing else, and every diagnostic was reported
    /// under the analyzer's `self.file` — so a warning inside `fmt::to_radix`
    /// came out as `hello.mt:230:22` for a `hello.mt` five lines long. Right
    /// line, wrong file, and it affects EVERY diagnostic rather than one lint.
    ///
    /// `&'static str` rather than an interned index or an `Rc<str>` because the
    /// only files a span can come from other than the one being compiled are
    /// the stdlib modules, whose names are `&'static str` in `STDLIB_SOURCES`
    /// already. That keeps `Span: Copy`, which every expression in the AST
    /// relies on.
    pub module: Option<&'static str>,
}

impl Span {
    pub fn zero() -> Self {
        Span { line: 0, col: 0, module: None }
    }

    pub fn new(line: usize, col: usize) -> Self {
        Span { line, col, module: None }
    }

    /// P8: a span inside merged stdlib source.
    pub fn in_module(line: usize, col: usize, module: &'static str) -> Self {
        Span { line, col, module: Some(module) }
    }

    /// P8: the file a diagnostic at this span should name, given the file the
    /// compiler was invoked on. A merged stdlib span names its own source.
    pub fn file_or(&self, current: &str) -> String {
        match self.module {
            Some(m) => format!("stdlib/{}.mt", m),
            None => current.to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Top-level program
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Program {
    pub items: Vec<Item>,
}

#[derive(Debug, Clone)]
pub enum Item {
    FnDef(FnDef),
    StructDef(StructDef),
    EnumDef(EnumDef),
    ImplBlock(ImplBlock),
    TraitDef(TraitDef),
    UseDecl(UseDecl),
    GlobalVar(GlobalVar),
    /// A1: an explicit native declaration.
    ExternDecl(ExternDecl),
    /// A5: a module-level lint level setting.
    LintDecl(LintDecl),
}

// ---------------------------------------------------------------------------
// Function definition
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct FnDef {
    pub name: String,
    pub generics: Vec<String>,
    /// B1: declared bounds on the generic parameters, from either the angle
    /// brackets (`<T: Ord>`) or a `where` clause. Kept beside `generics`
    /// rather than inside it because every existing consumer keys on the bare
    /// name, and a bound is a constraint ON a parameter, not part of its
    /// identity.
    pub bounds: Vec<GenericBound>,
    pub params: Vec<Param>,
    pub ret_ty: Option<Type>,
    pub body: Option<Block>, // None = extern / trait method without body
    /// A2: a WRITTEN availability assertion, `fn f() available(t3) { .. }`.
    ///
    /// This is not how availability is determined — that is inferred from the
    /// call graph. It is an assertion the compiler checks against the
    /// inference, the same relationship Rust has between inferred lifetimes
    /// and written ones: writing it on every function would be unbearable, but
    /// writing it on the few that matter pins them, so a later edit that
    /// quietly reaches an unavailable native is caught at the function that
    /// promised not to.
    ///
    /// `None` = unstated, which is NOT "available nowhere" — the same
    /// distinction `ExternDecl::available` makes.
    pub available: Option<Vec<String>>,
    pub is_pub: bool,
    pub is_async: bool,
    pub span: Span,
}

/// A declared constraint on one generic parameter: the `T: Ord` in
/// `fn max<T: Ord>(a: T, b: T) -> T`, or one clause of a `where`.
///
/// Several bounds on the same parameter accumulate rather than replace, so
/// `fn f<T: Ord>(..) where T: Display` constrains `T` by both.
#[derive(Debug, Clone)]
pub struct GenericBound {
    pub param: String,
    pub traits: Vec<String>,
    pub span: Span,
}

/// A1: an explicit native declaration.
///
/// ```text
/// extern "c" fn gui::set_color(r: int, g: int, b: int) -> void
///     available(llvm);
/// extern "c" fn str::to_lower(s: str) -> str
///     available(llvm) deprecated("use str::to_lower");
/// ```
///
/// The point of the form is that all three of section 52's defects become
/// inexpressible. A name that is not declared cannot be called; a declaration
/// with no implementation on the selected backend is a diagnostic at the CALL
/// SITE with a source span, not an undefined label out of the assembler; and
/// the signature is in the language's own type system, so passing a `bool`
/// where a `bool3` was declared is a type error like any other rather than a
/// silent coercion.
#[derive(Debug, Clone)]
pub struct ExternDecl {
    /// The ABI string: `"c"` for a C-runtime symbol, `"t3"` for a T3ISA
    /// syscall. Recorded rather than checked — the backends already know how
    /// to reach their own natives, and an ABI the compiler cannot honour is a
    /// backend error, not a parse error.
    pub abi: String,
    /// The declared name, qualified as it is called: `io::println`.
    pub name: String,
    pub params: Vec<Param>,
    pub ret_ty: Option<Type>,
    /// Backends that provide an implementation. `None` = no `available`
    /// clause was written, which is NOT the same as "available nowhere": it
    /// means unstated, and A1 step 3 is what turns unstated into an error.
    pub available: Option<Vec<String>>,
    pub deprecated: Option<String>,
    pub is_pub: bool,
    pub span: Span,
}

/// A5: `lint deny(unused-variable);` at item position.
///
/// Module-level rather than compilation-level, so a file can pin its own
/// strictness. Names are the same strings `--deny` takes.
#[derive(Debug, Clone)]
pub struct LintDecl {
    pub level: String,
    pub lints: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub ty: Type,
    pub span: Span,
}

// ---------------------------------------------------------------------------
// Struct / Enum / Impl / Trait
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct FieldDef {
    pub name: String,
    pub ty: Type,
    pub is_pub: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct StructDef {
    pub name: String,
    pub generics: Vec<String>,
    pub fields: Vec<FieldDef>,
    pub is_pub: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct EnumDef {
    pub name: String,
    pub variants: Vec<EnumVariant>,
    pub is_pub: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct EnumVariant {
    pub name: String,
    pub fields: Vec<Type>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ImplBlock {
    pub ty: String,
    /// The impl's own generic parameters: the `T` in `impl<T> Vec<T>`.
    ///
    /// `ty` stays the BASE name (`Vec`), because method resolution is by base
    /// name throughout — `Analyzer::current_impl_type` is set from it, and the
    /// collection methods are resolved in `semantic/analyzer/type_inference.rs`
    /// against `Vec`/`Map`/`Set` regardless of element type.
    pub generics: Vec<String>,
    pub trait_: Option<String>,
    pub methods: Vec<FnDef>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct TraitDef {
    pub name: String,
    pub methods: Vec<FnDef>,
    pub is_pub: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct UseDecl {
    pub path: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct GlobalVar {
    pub name: String,
    pub ty: Type,
    pub val: Option<Expr>,
    pub is_pub: bool,
    pub span: Span,
}

// ---------------------------------------------------------------------------
// Blocks and statements
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum Stmt {
    Let(LetStmt),
    Assign(AssignStmt),
    Expr(Expr),
    Return(Option<Expr>, Span),
    Break(Span),
    Continue(Span),
    LocalStructDef(StructDef), // struct defined inside a function body
}

impl Stmt {
    pub fn span(&self) -> Span {
        match self {
            Stmt::Let(ls) => ls.span,
            Stmt::Assign(a) => a.span,
            Stmt::Expr(e) => e.span(),
            Stmt::Return(_, span) => *span,
            Stmt::Break(span) => *span,
            Stmt::Continue(span) => *span,
            Stmt::LocalStructDef(s) => s.span,
        }
    }
}

/// Pattern in a let/for binding.
#[derive(Debug, Clone)]
pub enum LetPat {
    /// Simple identifier: `let x = ...`
    Ident(String),
    /// Tuple destructuring: `let (a, b, c) = ...`
    Tuple(Vec<String>),
}

impl LetPat {
    /// Return the first (or only) name for backward-compat lookups.
    pub fn first_name(&self) -> &str {
        match self {
            LetPat::Ident(n) => n.as_str(),
            LetPat::Tuple(ns) => ns.first().map(|s| s.as_str()).unwrap_or("_"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LetStmt {
    /// The binding pattern (simple ident or tuple destructure).
    pub pat: LetPat,
    /// Backward-compat accessor — same as `pat.first_name()` for `Ident` pats.
    pub name: String,
    pub ty: Option<Type>,
    pub init: Option<Expr>,
    pub mutable: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct AssignStmt {
    pub target: Expr,
    pub value: Expr,
    /// None = plain `=`, Some(op) = compound assignment like `+=`
    pub op: Option<BinOpKind>,
    pub span: Span,
}

// ---------------------------------------------------------------------------
// Expressions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum Expr {
    Lit(Lit, Span),
    Ident(String, Span),
    BinOp(Box<Expr>, BinOpKind, Box<Expr>, Span),
    UnOp(UnOpKind, Box<Expr>, Span),
    Call(Box<Expr>, Vec<Expr>, Span),
    MethodCall(Box<Expr>, String, Vec<Expr>, Span),
    Index(Box<Expr>, Box<Expr>, Span),
    Field(Box<Expr>, String, Span),
    Block(Block),
    If(IfExpr),
    Tif(TifExpr),
    Match(MatchExpr),
    For(ForExpr),
    While(WhileExpr),
    Loop(Box<Block>, Span),
    Spawn(Box<Block>, Span),
    /// §11.4: `yield;` — the explicit yield point.
    Yield(Span),
    Await(Box<Expr>, Span),
    Array(Vec<Expr>, Span),
    Tuple(Vec<Expr>, Span),
    StructLit(String, Vec<(String, Expr)>, Span),
    Range(Box<Expr>, Box<Expr>, bool, Span), // bool = inclusive
    Return(Box<Expr>, Span),
    Break(Span),
    Continue(Span),
    Cast(Box<Expr>, Type, Span),
    Question(Box<Expr>, Span), // ? operator
    Lambda(Vec<(String, Type)>, Option<Type>, Box<Expr>, Span), // fn(params) -> ret => body
    Tresult(TresultExpr),
}

impl Expr {
    pub fn span(&self) -> Span {
        match self {
            Expr::Lit(_, s) => *s,
            Expr::Ident(_, s) => *s,
            Expr::BinOp(_, _, _, s) => *s,
            Expr::UnOp(_, _, s) => *s,
            Expr::Call(_, _, s) => *s,
            Expr::MethodCall(_, _, _, s) => *s,
            Expr::Index(_, _, s) => *s,
            Expr::Field(_, _, s) => *s,
            Expr::Block(b) => b.span,
            Expr::If(i) => i.span,
            Expr::Tif(t) => t.span,
            Expr::Match(m) => m.span,
            Expr::For(f) => f.span,
            Expr::While(w) => w.span,
            Expr::Loop(_, s) => *s,
            Expr::Spawn(_, s) => *s,
            Expr::Yield(s) => *s,
            Expr::Await(_, s) => *s,
            Expr::Array(_, s) => *s,
            Expr::Tuple(_, s) => *s,
            Expr::StructLit(_, _, s) => *s,
            Expr::Range(_, _, _, s) => *s,
            Expr::Return(_, s) => *s,
            Expr::Break(s) => *s,
            Expr::Continue(s) => *s,
            Expr::Cast(_, _, s) => *s,
            Expr::Question(_, s) => *s,
            Expr::Lambda(_, _, _, s) => *s,
            Expr::Tresult(t) => t.span,
        }
    }
}

// ---------------------------------------------------------------------------
// Literals
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum Lit {
    Int(i64),
    Float(f64),
    Str(String),
    Char(char),
    Bool(bool),
    Bool3(i8),       // -1 = false, 0 = unknown, +1 = true
    Trit(i8),        // -1, 0, +1
    TernaryInt(i64), // balanced ternary integer literal
    Null,
}

// ---------------------------------------------------------------------------
// Operators
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum BinOpKind {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    And,
    Or,
    BitAnd,
    BitOr,
    BitXor,
    LShift,
    RShift,
    // Ternary logic
    Tand,
    Tor,
    Txor,
    Tcon, // consensus: +1 if both +1, -1 if both -1, else 0
    Tany, // any: +1 if either +1, -1 if either -1, else 0
    /// C1: Lukasiewicz implication, `min(+1, 1 - a + b)` on -1/0/+1.
    ///
    /// The connective that decides WHICH three-valued logic this is. Kleene's
    /// K3 and Lukasiewicz's L3 share conjunction (min), disjunction (max) and
    /// negation exactly; they differ in one cell of implication — `a = b = 0`,
    /// where Kleene gives unknown and Lukasiewicz gives TRUE. That cell is the
    /// deduction theorem: in L3 `a timp a` is a tautology, in K3 it is not.
    /// With only min/max/negation the language could not express the question.
    Timp,
    /// C1: Lukasiewicz equivalence, `(a timp b) tand (b timp a)`.
    Teq,
    /// C2 / T3ISA v1.5: lane-wise conjunction — per-trit `min` across all 27
    /// lanes of a word.
    ///
    /// The lane-wise family is the same six connectives as the scalar ones,
    /// but reading a word as 27 independent trits rather than as a magnitude.
    /// `a tand b` asks one three-valued question; `a tandw b` asks 27 of them
    /// in a single T3 instruction. That is the 27-way SIMD a balanced-ternary
    /// word already pays for and a binary machine cannot copy without a loop.
    ///
    /// Operands are words, not trits, and are read as exactly 27 lanes: a
    /// value outside the 27-trit range is clamped, not wrapped.
    Tandw,
    /// C2: lane-wise disjunction — per-trit `max`.
    Torw,
    /// C2: lane-wise balanced sum mod 3. Inherits the scalar operator's
    /// surprise: it is not an involution, since 3k = 0 (mod 3) takes THREE
    /// applications to recover the original, not two.
    Txorw,
    /// C2: lane-wise Lukasiewicz implication, `min(+1, 1 - a + b)` per lane.
    /// The a = b = 0 cell is +1 here for the same reason it is in `Timp`.
    Timpw,
    /// C2: lane-wise three-way compare, `sign(a_i - b_i)` per lane. No scalar
    /// spelling — the word-level comparison operators already cover that case.
    Tcmpw,
    Range,
    RangeInclusive,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnOpKind {
    Neg,
    Not,
    Tnot,
    /// C1: possibility (Lukasiewicz M) — `+1` if a >= 0, else `-1`.
    /// "might be true".
    Tposs,
    /// C1: necessity (Lukasiewicz L) — `+1` only if a = +1, else `-1`.
    /// "is definitely true". Dual to Tposs: `tnec a == tnot tposs tnot a`.
    Tnec,
    /// C2: lane-wise negation.
    ///
    /// Lowers to `TritNeg` — the SAME instruction `tnot` uses, and that is the
    /// point rather than an oversight. Negating a balanced-ternary number
    /// flips the sign of every trit in it, so lane-wise NOT already IS `TNEG`;
    /// adding a `TNOTW` opcode to a published ISA would have bought a second
    /// encoding of an instruction that exists. This variant is a SURFACE
    /// spelling only: it differs from `Tnot` in the type rule (it takes a
    /// word, not a trit) and in what it tells the reader.
    Tnotw,
    Deref,
    Ref,
    TritNeg,
}

// ---------------------------------------------------------------------------
// Control flow expressions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct IfExpr {
    pub cond: Box<Expr>,
    pub then_block: Block,
    pub elif_branches: Vec<(Expr, Block)>,
    pub else_block: Option<Block>,
    pub span: Span,
}

/// Three-way ternary branch: tif cond { +block } { 0block } { -block }
#[derive(Debug, Clone)]
pub struct TifExpr {
    pub cond: Box<Expr>,
    pub pos_block: Block,  // +1 / true branch
    pub zero_block: Block, // 0  / unknown branch
    pub neg_block: Block,  // -1 / false branch
    pub span: Span,
}

/// Three-armed ternary result handler:
/// tresult <expr> { Ok(v) => ..., Unknown(h) => ..., Err(e) => ... }
/// Branches on the ternary state of <expr>; each arm binds the result to a variable.
#[derive(Debug, Clone)]
pub struct TresultExpr {
    pub expr: Box<Expr>,
    pub ok_var: String,        // binding name in Ok (+1) arm
    pub ok_block: Block,
    pub unknown_var: String,   // binding name in Unknown (0) arm
    pub unknown_block: Block,
    pub err_var: String,       // binding name in Err (-1) arm
    pub err_block: Block,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct MatchExpr {
    pub scrutinee: Box<Expr>,
    pub arms: Vec<MatchArm>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub guard: Option<Expr>,
    pub body: Expr,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ForExpr {
    pub var: String,
    pub iter: Box<Expr>,
    pub body: Block,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct WhileExpr {
    pub cond: Box<Expr>,
    pub body: Block,
    pub span: Span,
}

// ---------------------------------------------------------------------------
// Patterns
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum Pattern {
    Wildcard(Span),
    Ident(String, Span),
    Lit(Lit, Span),
    Struct(String, Vec<(String, Pattern)>, Span),
    Enum(String, Option<String>, Vec<Pattern>, Span),
    Tuple(Vec<Pattern>, Span),
    Or(Vec<Pattern>, Span),
}

impl Pattern {
    /// Whether this pattern matches every value of its type.
    ///
    /// P90: the answer decides two things that must agree, and did not. The
    /// lowerer uses it to decide whether a sub-pattern needs a runtime test,
    /// and `check_exhaustiveness` uses it to decide whether an arm covers its
    /// whole variant. When the lowerer silently answered "irrefutable" for a
    /// literal and the checker silently agreed, `Err("closed")` both matched
    /// every `Err` and counted as covering `Err`, so the wrong arm ran and no
    /// arm was missing. One predicate, so the two cannot drift apart again.
    ///
    /// `Struct` is deliberately NOT irrefutable even when all its fields are:
    /// a struct pattern names a type, and the lowerer emits a test for it.
    /// Answering `true` here would suppress that test.
    pub fn is_irrefutable(&self) -> bool {
        match self {
            Pattern::Wildcard(_) | Pattern::Ident(_, _) => true,
            // A tuple is irrefutable exactly when every element is: there is
            // no tag to test, only the elements.
            Pattern::Tuple(elems, _) => elems.iter().all(Pattern::is_irrefutable),
            // One irrefutable alternative makes the whole alternation match
            // everything, whatever the others test.
            Pattern::Or(alts, _) => alts.iter().any(Pattern::is_irrefutable),
            Pattern::Lit(_, _) | Pattern::Struct(_, _, _) | Pattern::Enum(_, _, _, _) => false,
        }
    }
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Named(String, Span),
    Path(Vec<String>, Span),
    Ref(Box<Type>, bool, Span),   // bool = mutable
    Ptr(Box<Type>, bool, Span),   // bool = mutable
    Array(Box<Type>, Option<usize>, Span),
    Tuple(Vec<Type>, Span),
    Fn(Vec<Type>, Box<Type>, Span),
    Generic(String, Vec<Type>, Span), // e.g. Result<T, E>
    Infer(Span),                      // _
}

impl Type {
    /// Render the type back in ManiT surface syntax.
    ///
    /// Used for user-facing output such as LSP hover, which previously leaked
    /// Rust `{:?}` debug formatting of this enum (A9).
    pub fn display(&self) -> String {
        match self {
            Type::Named(n, _) => n.clone(),
            Type::Path(parts, _) => parts.join("::"),
            Type::Ref(t, true, _) => format!("&mut {}", t.display()),
            Type::Ref(t, false, _) => format!("&{}", t.display()),
            Type::Ptr(t, true, _) => format!("*mut {}", t.display()),
            Type::Ptr(t, false, _) => format!("*{}", t.display()),
            Type::Array(t, Some(n), _) => format!("[{}; {}]", t.display(), n),
            Type::Array(t, None, _) => format!("[{}]", t.display()),
            Type::Tuple(ts, _) => format!(
                "({})",
                ts.iter().map(|t| t.display()).collect::<Vec<_>>().join(", "),
            ),
            Type::Fn(ps, r, _) => format!(
                "fn({}) -> {}",
                ps.iter().map(|t| t.display()).collect::<Vec<_>>().join(", "),
                r.display(),
            ),
            Type::Generic(n, args, _) => format!(
                "{}<{}>",
                n,
                args.iter().map(|t| t.display()).collect::<Vec<_>>().join(", "),
            ),
            Type::Infer(_) => "_".to_string(),
        }
    }

    pub fn span(&self) -> Span {
        match self {
            Type::Named(_, s) => *s,
            Type::Path(_, s) => *s,
            Type::Ref(_, _, s) => *s,
            Type::Ptr(_, _, s) => *s,
            Type::Array(_, _, s) => *s,
            Type::Tuple(_, s) => *s,
            Type::Fn(_, _, s) => *s,
            Type::Generic(_, _, s) => *s,
            Type::Infer(s) => *s,
        }
    }
}
