//! AST for the ManiT core, for the reference interpreter.
//!
//! © Manish Jagdish Thatte
//!
//! Deliberately NOT `crate::ast`. See lex.rs for the independence rule.
//! This is the shape docs/semantics.md §2 describes, and nothing more —
//! anything the core does not cover has no node here, so an out-of-scope
//! program fails to parse rather than being silently mis-evaluated.

#[derive(Debug, Clone, PartialEq)]
pub enum Ty {
    Int, Trit, Bool3, Bool, Void, Str,
    /// §11. A channel. The core has one element type — `int` — because §11
    /// exists to specify INTERLEAVING, and a second element type would add
    /// nothing to it while adding generics to a core that has none.
    Chan,
    /// `Result<T, str>`. The core fixes the error type to `str`, which is what
    /// the language reference itself recommends: "ManiT writes `Result<T, str>`
    /// and uses `Unknown(msg)` for the absent case".
    Result(Box<Ty>),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Bin {
    Add, Sub, Mul, Div, Rem,
    Eq, Ne, Lt, Gt, Le, Ge,
    AndAnd, OrOr,
    Tand, Tor, Txor, Tcon, Tany, Timp, Teq,
    Tandw, Torw, Txorw, Timpw, Tcmpw,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Un { Neg, Tnot, Tposs, Tnec, Tnotw }

#[derive(Debug, Clone)]
pub enum Expr {
    Int(i64),
    TritLit(i8),
    BoolLit(bool),
    Bool3Lit(i8),
    Str(String),
    Var(String),
    Call(String, Vec<Expr>),
    Un(Un, Box<Expr>),
    Bin(Bin, Box<Expr>, Box<Expr>),
    Cast(Box<Expr>, Ty),
    /// `r.method(args)` — only the six `Result` accessors are in the core.
    Method(Box<Expr>, String, Vec<Expr>),
    /// `e?` — propagate `Unknown` and `Err` out of the enclosing function,
    /// evaluate to the payload on `Ok`.
    Try(Box<Expr>),
    /// `match e { Ok(v) => .., Unknown(m) => .., Err(e) => .. }`.
    Match(Box<Expr>, Vec<MatchArm>),
}

/// One arm of a `match` on a `Result`. The core specifies no other scrutinee
/// type for `match`, so the pattern is exactly a variant plus a binding.
#[derive(Debug, Clone)]
pub struct MatchArm {
    /// "Ok" | "Unknown" | "Err" | "_"
    pub variant: String,
    /// The name the payload binds to; absent for `_`.
    pub binding: Option<String>,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone)]
pub enum Stmt {
    Let { name: String, mutable: bool, ty: Option<Ty>, init: Expr },
    Assign { name: String, val: Expr },
    If { arms: Vec<(Expr, Vec<Stmt>)>, els: Option<Vec<Stmt>> },
    /// `tif`: all three arms are required (semantics.md §7), so they are three
    /// fields rather than a list that could be short.
    Tif { scrutinee: Expr, pos: Vec<Stmt>, zero: Vec<Stmt>, neg: Vec<Stmt> },
    While { cond: Expr, body: Vec<Stmt> },
    Return(Option<Expr>),
    Expr(Expr),
    /// §11.5 (SPAWN). A STATEMENT, not an expression: `spawn` produces no
    /// value in this core, and making it an expression would be the first half
    /// of the `Task<T>` decision §11.1 declines to take.
    Spawn(Vec<Stmt>),
    /// §11.5 (YIELD).
    Yield,
}

#[derive(Debug, Clone)]
pub struct Fn {
    pub name: String,
    pub params: Vec<(String, Ty)>,
    pub ret: Ty,
    pub body: Vec<Stmt>,
}
