// ---------------------------------------------------------------------------
// Span
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    pub line: usize,
    pub col: usize,
}

impl Span {
    pub fn zero() -> Self {
        Span { line: 0, col: 0 }
    }

    pub fn new(line: usize, col: usize) -> Self {
        Span { line, col }
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
}

// ---------------------------------------------------------------------------
// Function definition
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct FnDef {
    pub name: String,
    pub generics: Vec<String>,
    pub params: Vec<Param>,
    pub ret_ty: Option<Type>,
    pub body: Option<Block>, // None = extern / trait method without body
    pub is_pub: bool,
    pub is_async: bool,
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
    Range,
    RangeInclusive,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnOpKind {
    Neg,
    Not,
    Tnot,
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
