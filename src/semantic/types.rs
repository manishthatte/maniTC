use crate::ast::{self, *};

// ---------------------------------------------------------------------------
// ManiType — the type system
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum ManiType {
    Int,
    Float,
    Bool,
    Bool3,
    Trit,
    Tryte,
    T9,
    T27,
    T54,     // 54-trit balanced ternary integer (I64-bounded); `trint` is a source alias
    Tfloat,  // 27-trit balanced ternary floating-point
    Str,
    Char,
    Void,
    Array(Box<ManiType>, Option<usize>),
    Tuple(Vec<ManiType>),
    /// A nominal struct, with the type arguments it was instantiated at.
    ///
    /// **The arguments are carried, not compared.** `types_compatible` looks
    /// only at the NAME, exactly as it did when this variant held a bare
    /// `String`, so adding them changes no verdict. They exist so that a
    /// generic struct's value can say what `T` turned out to be, which is what
    /// an `impl<T>` method needs in order to be instantiated (report.txt P65's
    /// open half).
    ///
    /// Why not reuse `Generic(name, args)`: because every question the rest of
    /// the compiler asks about a struct — "is this a struct called n", what
    /// `IRType` is it, is it a move type — is asked by matching `Struct`, and
    /// a second spelling would mean auditing all of them and getting one
    /// wrong. This way the type stays nominally a struct everywhere and the
    /// arguments simply ride along. It is also what made the change safe to
    /// make: every site is a pattern the compiler forced me to visit.
    Struct(String, Vec<ManiType>),
    Enum(String),
    Fn(Vec<ManiType>, Box<ManiType>),
    Generic(String, Vec<ManiType>), // Result<T,E> etc.
    Unknown,                        // type inference placeholder
}

impl ManiType {
    pub fn is_ternary(&self) -> bool {
        matches!(self, ManiType::Trit | ManiType::Tryte | ManiType::T9 | ManiType::T27 | ManiType::T54 | ManiType::Tfloat | ManiType::Bool3)
    }

    pub fn is_numeric(&self) -> bool {
        matches!(self, ManiType::Int | ManiType::Float | ManiType::Trit | ManiType::Tryte
            | ManiType::T9 | ManiType::T27 | ManiType::T54 | ManiType::Tfloat)
    }

    pub fn is_comparable(&self) -> bool {
        self.is_numeric() || matches!(self, ManiType::Bool | ManiType::Bool3 | ManiType::Char)
    }

    /// Permissive-on-Unknown check: `true` when the type is fully known
    /// (contains no `Unknown` placeholder at the top level).
    pub fn is_known(&self) -> bool {
        !matches!(self, ManiType::Unknown)
    }

    /// The same question asked all the way down.
    ///
    /// `is_known` is deliberately shallow, and for most callers that is right.
    /// It is not right for deciding whether a DECLARED signature is precise
    /// enough to enforce against: `async::spawn_task` declares
    /// `Future<<unknown>>`, which is known at the top level and says nothing
    /// about what it accepts. Enforcing on that rejected
    /// `examples/concurrency.mt`, which is correct code.
    pub fn fully_known(&self) -> bool {
        use ManiType::*;
        match self {
            Unknown => false,
            Array(e, _) => e.fully_known(),
            Tuple(ts) => ts.iter().all(|t| t.fully_known()),
            Generic(_, args) => args.iter().all(|t| t.fully_known()),
            Fn(ps, r) => ps.iter().all(|t| t.fully_known()) && r.fully_known(),
            _ => true,
        }
    }

    pub fn display(&self) -> String {
        match self {
            ManiType::Int => "int".to_string(),
            ManiType::Float => "float".to_string(),
            ManiType::Bool => "bool".to_string(),
            ManiType::Bool3 => "bool3".to_string(),
            ManiType::Trit => "trit".to_string(),
            ManiType::Tryte => "tryte".to_string(),
            ManiType::T9 => "t9".to_string(),
            ManiType::T27 => "t27".to_string(),
            ManiType::T54 => "t54".to_string(),
            // Note: `trint` keyword is an alias for `t54` at the source level
            ManiType::Tfloat => "tfloat".to_string(),
            ManiType::Str => "str".to_string(),
            ManiType::Char => "char".to_string(),
            ManiType::Void => "void".to_string(),
            ManiType::Unknown => "<unknown>".to_string(),
            ManiType::Array(t, n) => format!("[{}; {:?}]", t.display(), n),
            ManiType::Tuple(ts) => format!("({})", ts.iter().map(|t| t.display()).collect::<Vec<_>>().join(", ")),
            ManiType::Struct(n, args) if args.is_empty() => n.clone(),
            ManiType::Struct(n, args) => format!(
                "{}<{}>",
                n,
                args.iter().map(|t| t.display()).collect::<Vec<_>>().join(", ")
            ),
            ManiType::Enum(n) => n.clone(),
            ManiType::Fn(ps, r) => format!("fn({}) -> {}", ps.iter().map(|t| t.display()).collect::<Vec<_>>().join(", "), r.display()),
            ManiType::Generic(n, args) => format!("{}<{}>", n, args.iter().map(|t| t.display()).collect::<Vec<_>>().join(", ")),
        }
    }
}

/// Whether a value of type `b` may be used where type `a` is expected
/// (and vice versa — the relation is symmetric).
///
/// Design rule of the crate: `Unknown` is a permissive placeholder, so any
/// pairing that involves `Unknown` is compatible. All numeric types are
/// mutually coercible (int literals flow into trit/tryte/t9/t27/t54/float
/// contexts — a coercion the language reference blesses); `bool`/`bool3` and
/// `trit`/`bool3` share literal forms and are likewise interchangeable.
pub fn types_compatible(a: &ManiType, b: &ManiType) -> bool {
    use ManiType::*;
    if a == b {
        return true;
    }
    match (a, b) {
        (Unknown, _) | (_, Unknown) => true,
        _ if a.is_numeric() && b.is_numeric() => true,
        (Bool, Bool3) | (Bool3, Bool) => true,
        (Trit, Bool3) | (Bool3, Trit) => true,
        (Array(ea, na), Array(eb, nb)) => {
            types_compatible(ea, eb) && (na.is_none() || nb.is_none() || na == nb)
        }
        (Tuple(ta), Tuple(tb)) => {
            ta.len() == tb.len() && ta.iter().zip(tb).all(|(x, y)| types_compatible(x, y))
        }
        // Generic args are frequently `Unknown` (e.g. `Vec::new()`), so only the
        // constructor name is compared strictly.
        (Generic(na, aa), Generic(nb, ab)) => {
            na == nb && aa.iter().zip(ab.iter()).all(|(x, y)| types_compatible(x, y))
        }
        (Fn(pa, ra), Fn(pb, rb)) => {
            pa.len() == pb.len()
                && pa.iter().zip(pb).all(|(x, y)| types_compatible(x, y))
                && types_compatible(ra, rb)
        }
        // A module-qualified struct/enum name matches its unqualified form.
        // NAME ONLY, deliberately. `Struct` gained its type arguments so that
        // an instantiation could be identified; comparing them here would make
        // `Box2<int>` and `Box2<float>` incompatible, which is correct in
        // principle and a strictness change in practice, so it is a separate
        // decision rather than a side effect of carrying the arguments.
        (Struct(x, _), Struct(y, _)) => {
            x == y
                || x.ends_with(&format!("::{}", y))
                || y.ends_with(&format!("::{}", x))
        }
        (Enum(x), Enum(y)) => {
            x.ends_with(&format!("::{}", y)) || y.ends_with(&format!("::{}", x))
        }
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Typed AST nodes
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct TypedProgram {
    pub functions: Vec<TypedFnDef>,
    pub structs: Vec<StructDef>,
    pub struct_fields: std::collections::HashMap<String, Vec<(String, ManiType)>>,
    pub enums: Vec<EnumDef>,
    pub globals: Vec<TypedGlobal>,
    /// Declared parameter types of the NATIVE stdlib functions — the ones whose
    /// bodies live in a backend rather than in `.mt` source, so they never
    /// appear in `functions` and lowering would otherwise know nothing about
    /// their signatures. Keyed by qualified name (`io::println_bool3`).
    ///
    /// This exists because lowering must coerce call arguments to the declared
    /// parameter type, and a native declaration is exactly the case where the
    /// declared type is invisible. See `IRLowerer::lower` and S45.
    pub native_param_manitys: std::collections::HashMap<String, Vec<ManiType>>,
}

#[derive(Debug, Clone)]
pub struct TypedGlobal {
    pub name: String,
    pub ty: ManiType,
    pub init: Option<TypedExpr>,
    pub is_pub: bool,
}

#[derive(Debug, Clone)]
pub struct TypedFnDef {
    pub name: String,
    pub params: Vec<TypedParam>,
    pub ret_ty: ManiType,
    pub body: Option<TypedBlock>,
    pub is_pub: bool,
    pub is_async: bool,
}

#[derive(Debug, Clone)]
pub struct TypedParam {
    pub name: String,
    pub ty: ManiType,
    /// **B7's D-2**: passing an argument here CONSUMES it. See
    /// `ast::Param::is_move` for why this is per-parameter and not a change to
    /// what all calls do.
    pub is_move: bool,
}

#[derive(Debug, Clone)]
pub struct TypedBlock {
    pub stmts: Vec<TypedStmt>,
    pub ty: ManiType,
}

#[derive(Debug, Clone)]
pub enum TypedStmt {
    Let(TypedLetStmt),
    Assign(TypedAssignStmt),
    Expr(TypedExpr),
    Return(Option<TypedExpr>),
    Break,
    Continue,
}

#[derive(Debug, Clone)]
pub struct TypedLetStmt {
    pub name: String,
    pub pat: ast::LetPat,
    pub ty: ManiType,
    pub init: Option<TypedExpr>,
    pub mutable: bool,
}

#[derive(Debug, Clone)]
pub struct TypedAssignStmt {
    pub target: TypedExpr,
    pub value: TypedExpr,
    pub op: Option<BinOpKind>,
}

#[derive(Debug, Clone)]
pub struct TypedExpr {
    pub kind: TypedExprKind,
    pub ty: ManiType,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum TypedExprKind {
    Lit(Lit),
    Ident(String),
    BinOp(Box<TypedExpr>, BinOpKind, Box<TypedExpr>),
    UnOp(UnOpKind, Box<TypedExpr>),
    Call(Box<TypedExpr>, Vec<TypedExpr>),
    /// Receiver, method name, arguments, and — P69 — the resolved callee
    /// symbol when it is not derivable from the receiver's type.
    ///
    /// The lowerer builds a method's callee name as `<receiver base type>::<method>`,
    /// which is exactly right until an `impl<T>` method is monomorphised: the
    /// instantiation is called `Box2::bigger$float` and the receiver still
    /// displays as `Box2<float>`. Nothing in the receiver's type says which
    /// instantiation the checker chose, so the checker says it here. `None`
    /// everywhere else, which is every method call that existed before P69.
    MethodCall(Box<TypedExpr>, String, Vec<TypedExpr>, Option<String>),
    Index(Box<TypedExpr>, Box<TypedExpr>),
    Field(Box<TypedExpr>, String),
    Block(TypedBlock),
    If(TypedIfExpr),
    Tif(TypedTifExpr),
    Match(TypedMatchExpr),
    For(TypedForExpr),
    While(TypedWhileExpr),
    Loop(TypedBlock),
    Array(Vec<TypedExpr>),
    Tuple(Vec<TypedExpr>),
    StructLit(String, Vec<(String, TypedExpr)>),
    Range(Box<TypedExpr>, Box<TypedExpr>, bool),
    Return(Box<TypedExpr>),
    Break,
    Continue,
    Cast(Box<TypedExpr>, ManiType),
    Question(Box<TypedExpr>),
    /// §11.2: a spawned task gets a COPY of the spawning task's store, so the
    /// values the block reads from its enclosing scope travel with it.
    ///
    /// Computed in the analyzer, where `collect_free_in_block` already exists —
    /// it is what refuses lambda capture (P55) — rather than by a second walker
    /// over the TYPED tree, because a walker that misses a variant misses a
    /// capture, and a missing capture is a silently wrong program.
    ///
    /// The T3 backend ignores this list: P89's `spawn` is a FORK, so the child
    /// reaches its enclosing locals by sharing the frame layout. The LLVM
    /// backend cannot copy a live C stack (P88, P99) and outlines the body
    /// instead, which is where these are needed.
    Spawn(TypedBlock, Vec<(String, ManiType)>),
    /// §11.4's explicit yield point.
    Yield,
    Await(Box<TypedExpr>),
    Tresult(TypedTresultExpr),
}

#[derive(Debug, Clone)]
pub struct TypedIfExpr {
    pub cond: Box<TypedExpr>,
    pub then_block: TypedBlock,
    pub elif_branches: Vec<(TypedExpr, TypedBlock)>,
    pub else_block: Option<TypedBlock>,
}

#[derive(Debug, Clone)]
pub struct TypedTifExpr {
    pub cond: Box<TypedExpr>,
    pub pos_block: TypedBlock,
    pub zero_block: TypedBlock,
    pub neg_block: TypedBlock,
}

#[derive(Debug, Clone)]
pub struct TypedMatchExpr {
    pub scrutinee: Box<TypedExpr>,
    pub arms: Vec<TypedMatchArm>,
}

#[derive(Debug, Clone)]
pub struct TypedMatchArm {
    pub pattern: Pattern,
    pub guard: Option<TypedExpr>,
    pub body: TypedExpr,
}

#[derive(Debug, Clone)]
pub struct TypedForExpr {
    pub var: String,
    pub iter: Box<TypedExpr>,
    pub body: TypedBlock,
}

#[derive(Debug, Clone)]
pub struct TypedWhileExpr {
    pub cond: Box<TypedExpr>,
    pub body: TypedBlock,
}

#[derive(Debug, Clone)]
pub struct TypedTresultExpr {
    pub expr: Box<TypedExpr>,
    pub ok_var: String,
    pub ok_block: TypedBlock,
    pub unknown_var: String,
    pub unknown_block: TypedBlock,
    pub err_var: String,
    pub err_block: TypedBlock,
}
