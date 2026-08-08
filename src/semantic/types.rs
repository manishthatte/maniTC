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
    Struct(String),
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
            ManiType::Struct(n) => n.clone(),
            ManiType::Enum(n) => n.clone(),
            ManiType::Fn(ps, r) => format!("fn({}) -> {}", ps.iter().map(|t| t.display()).collect::<Vec<_>>().join(", "), r.display()),
            ManiType::Generic(n, args) => format!("{}<{}>", n, args.iter().map(|t| t.display()).collect::<Vec<_>>().join(", ")),
        }
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
    MethodCall(Box<TypedExpr>, String, Vec<TypedExpr>),
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
    Spawn(TypedBlock),
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
