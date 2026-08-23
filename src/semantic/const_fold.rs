// semantic/const_fold.rs — compile-time evaluation for the contexts that
// REQUIRE a constant.
//
// Two places in the compiler need a value before any code runs: a module-level
// `let`, whose word is written by a preamble that executes before main, and a
// static array index, which is checked against a known length rather than
// guarded at run time. Both used to carry their own miniature folder, and both
// were wrong in the same direction — they matched a bare literal and nothing
// else, so `-42` (which parses as `UnOp(Neg, Lit(42))`, not `Lit(-42)`) missed.
//
// At the index site that was merely conservative: an unfolded index fell
// through to the runtime guard and still faulted. At the global site it was a
// silent miscompile — the initialiser fell to a wildcard that returned
// `IRConst::Null`, so EVERY negative module-level constant read as zero, on
// BOTH backends. Two backends agreeing is the differential oracle's normal
// evidence of correctness; here they agreed because the fault was upstream of
// the split, in the shared lowering. It is the first defect on record that the
// two-backend method structurally could not see (ORACLE_FINDINGS.md Section 31).
//
// So there is one folder, and callers that cannot evaluate an expression say
// so out loud rather than substituting a plausible zero.
//
// Author: Manish Jagdish Thatte

use crate::ast::{BinOpKind, Lit, UnOpKind};
use super::types::{TypedExpr, TypedExprKind};

/// A value the compiler can compute for itself.
///
/// The scalar variants mirror `lit_to_irvalue` in `ir/lower/helpers.rs`
/// exactly — notably `char` folds to its code point, as it does there — so
/// that a folded constant and a directly-lowered literal can never disagree
/// about what a given literal means. `Struct` has no counterpart there: it is
/// an aggregate, not a literal, and the lowering gives it static storage.
#[derive(Debug, Clone, PartialEq)]
pub enum ConstValue {
    Int(i64),
    Float(f64),
    Bool(bool),
    Trit(i8),
    Str(String),
    Null,
    /// A struct literal whose every field folded — `T27F { raw: 0 }`.
    ///
    /// Unlike the scalars this is not one word: a struct value is a POINTER to
    /// its fields, so the consumer must place the fields in static storage and
    /// keep the address. `ir/lower` does that (`IRStaticStruct`); the analyzer
    /// only asks whether the fold succeeded.
    ///
    /// Field values are in STRUCT DECLARATION ORDER, which is what
    /// `check_expr` normalises a struct literal to and what field access
    /// assigns by position. Source order never reaches here.
    ///
    /// No coercion is applied to the fields — `bool` in a `bool3` field is
    /// still `Bool` here. The bool→bool3 mapping (`2b − 1`) lives in
    /// `coerce_value` in the lowering, and writing it a second time in this
    /// file is exactly the drift the header warns about. The lowering applies
    /// it when it materialises the payload, where the declared field types are
    /// in hand.
    Struct(String, Vec<ConstValue>),
}

/// Why an expression could not be reduced to a constant.
///
/// The distinction matters for diagnostics: `NotConstant` means "this form is
/// not evaluable at compile time, write it differently", while the other two
/// mean "this IS a constant expression and it is broken" — a fault worth
/// reporting where it is written rather than deferring to a runtime trap.
#[derive(Debug, Clone, PartialEq)]
pub enum ConstError {
    /// The expression form cannot be evaluated at compile time at all.
    NotConstant,
    /// Integer division or remainder by a constant zero.
    DivideByZero,
    /// The result does not fit in an `int`.
    Overflow,
}

impl ConstError {
    /// The clause that follows "…initialiser " in a diagnostic.
    pub fn describe(&self) -> &'static str {
        match self {
            ConstError::NotConstant => "is not a compile-time constant",
            ConstError::DivideByZero => "divides by zero",
            ConstError::Overflow => "overflows an int",
        }
    }
}

type Folded = Result<ConstValue, ConstError>;

/// Evaluate `expr` at compile time, or explain why that is not possible.
pub fn fold(expr: &TypedExpr) -> Folded {
    match &expr.kind {
        TypedExprKind::Lit(lit) => fold_lit(lit),
        TypedExprKind::UnOp(op, inner) => fold_unop(op, fold(inner)?),
        TypedExprKind::BinOp(lhs, op, rhs) => fold_binop(fold(lhs)?, op, fold(rhs)?),
        TypedExprKind::Cast(inner, ty) => fold_cast(fold(inner)?, ty),
        // A struct literal is constant exactly when every field is. This is
        // what makes `let ZERO: T27F = T27F { raw: 0 };` legal at module
        // level: before it, `stdlib/t27f.mt` did not type-check standalone,
        // and the module only worked because `stdlib_expand` inlines constants
        // at their use sites — where the initialiser is reachable inside a
        // function (ORACLE_FINDINGS.md Section 50).
        TypedExprKind::StructLit(name, fields) => {
            let mut vals = Vec::with_capacity(fields.len());
            for (_, fexpr) in fields {
                vals.push(fold(fexpr)?);
            }
            Ok(ConstValue::Struct(name.clone(), vals))
        }
        _ => Err(ConstError::NotConstant),
    }
}

/// Fold `expr` to an `int`, for callers that only care about integers
/// (the static array-bounds check). Non-integer constants are `None`.
pub fn fold_int(expr: &TypedExpr) -> Option<i64> {
    match fold(expr) {
        Ok(ConstValue::Int(n)) => Some(n),
        _ => None,
    }
}

fn fold_lit(lit: &Lit) -> Folded {
    Ok(match lit {
        Lit::Int(n) => ConstValue::Int(*n),
        Lit::TernaryInt(n) => ConstValue::Int(*n),
        // A `char` is its code point everywhere else in the compiler; keep it
        // that way here so the two paths cannot drift.
        Lit::Char(c) => ConstValue::Int(*c as i64),
        Lit::Float(f) => ConstValue::Float(*f),
        Lit::Bool(b) => ConstValue::Bool(*b),
        Lit::Bool3(v) => ConstValue::Trit(*v),
        Lit::Trit(v) => ConstValue::Trit(*v),
        Lit::Str(s) => ConstValue::Str(s.clone()),
        Lit::Null => ConstValue::Null,
    })
}

fn fold_unop(op: &UnOpKind, v: ConstValue) -> Folded {
    match (op, v) {
        (UnOpKind::Neg, ConstValue::Int(n)) => {
            n.checked_neg().map(ConstValue::Int).ok_or(ConstError::Overflow)
        }
        (UnOpKind::Neg, ConstValue::Float(f)) => Ok(ConstValue::Float(-f)),
        // `-t`, `~t` and `tnot t` all lower to the same TritNeg / Neg —
        // arithmetic negation, which on a trit maps + to - and fixes 0.
        (UnOpKind::Neg, ConstValue::Trit(t))
        | (UnOpKind::TritNeg, ConstValue::Trit(t))
        | (UnOpKind::Tnot, ConstValue::Trit(t)) => Ok(ConstValue::Trit(-t)),
        (UnOpKind::Not, ConstValue::Bool(b)) => Ok(ConstValue::Bool(!b)),
        _ => Err(ConstError::NotConstant),
    }
}

fn fold_binop(a: ConstValue, op: &BinOpKind, b: ConstValue) -> Folded {
    use BinOpKind::*;
    match (a, b) {
        (ConstValue::Int(x), ConstValue::Int(y)) => match op {
            Add => x.checked_add(y).map(ConstValue::Int).ok_or(ConstError::Overflow),
            Sub => x.checked_sub(y).map(ConstValue::Int).ok_or(ConstError::Overflow),
            Mul => x.checked_mul(y).map(ConstValue::Int).ok_or(ConstError::Overflow),
            // Integer division by zero is a fault, not a value. Reporting it
            // here turns a runtime trap into a diagnostic at the line that
            // wrote it.
            Div if y == 0 => Err(ConstError::DivideByZero),
            Rem if y == 0 => Err(ConstError::DivideByZero),
            Div => x.checked_div(y).map(ConstValue::Int).ok_or(ConstError::Overflow),
            Rem => x.checked_rem(y).map(ConstValue::Int).ok_or(ConstError::Overflow),
            Eq => Ok(ConstValue::Bool(x == y)),
            Ne => Ok(ConstValue::Bool(x != y)),
            Lt => Ok(ConstValue::Bool(x < y)),
            Gt => Ok(ConstValue::Bool(x > y)),
            Le => Ok(ConstValue::Bool(x <= y)),
            Ge => Ok(ConstValue::Bool(x >= y)),
            _ => Err(ConstError::NotConstant),
        },
        (ConstValue::Float(x), ConstValue::Float(y)) => match op {
            // Float division by zero is defined by IEEE 754 and produces an
            // infinity — a value, unlike the integer case. Fold it as such.
            Add => Ok(ConstValue::Float(x + y)),
            Sub => Ok(ConstValue::Float(x - y)),
            Mul => Ok(ConstValue::Float(x * y)),
            Div => Ok(ConstValue::Float(x / y)),
            Rem => Ok(ConstValue::Float(x % y)),
            Eq => Ok(ConstValue::Bool(x == y)),
            Ne => Ok(ConstValue::Bool(x != y)),
            Lt => Ok(ConstValue::Bool(x < y)),
            Gt => Ok(ConstValue::Bool(x > y)),
            Le => Ok(ConstValue::Bool(x <= y)),
            Ge => Ok(ConstValue::Bool(x >= y)),
            _ => Err(ConstError::NotConstant),
        },
        (ConstValue::Bool(x), ConstValue::Bool(y)) => match op {
            And => Ok(ConstValue::Bool(x && y)),
            Or => Ok(ConstValue::Bool(x || y)),
            Eq => Ok(ConstValue::Bool(x == y)),
            Ne => Ok(ConstValue::Bool(x != y)),
            _ => Err(ConstError::NotConstant),
        },
        // Deliberately absent: tand / tor / txor / tcon / tany on trits.
        //
        // They are foldable in principle, but folding them means writing a
        // SECOND definition of each — one here and one in each backend — and
        // this compiler has already paid for that mistake more than once. A
        // constant ternary expression in a global is reported as unfoldable,
        // which is a diagnostic the author can act on; a folder that quietly
        // disagreed with the backends would be the same class of bug this file
        // exists to close.
        _ => Err(ConstError::NotConstant),
    }
}

fn fold_cast(v: ConstValue, ty: &crate::semantic::types::ManiType) -> Folded {
    use crate::semantic::types::ManiType;

    // Types whose runtime representation is a plain integer word. `t27`/`t54`
    // and friends are balanced-ternary integers — a different notation for the
    // same word, not a different value — which is why a `t27` literal already
    // lowers through `IRConst::Int`.
    let integral = matches!(
        ty,
        ManiType::Int | ManiType::Char | ManiType::Tryte
            | ManiType::T9 | ManiType::T27 | ManiType::T54
    );
    let floating = matches!(ty, ManiType::Float | ManiType::Tfloat);

    match v {
        ConstValue::Int(n) if floating => Ok(ConstValue::Float(n as f64)),
        ConstValue::Int(_) if integral => Ok(v),
        ConstValue::Float(f) if integral => Ok(ConstValue::Int(f as i64)),
        ConstValue::Float(_) if floating => Ok(v),
        // A trit widened to an integer keeps its value. The reverse is not
        // folded: narrowing an arbitrary int to a trit is a range question the
        // backends answer, and answering it a second time here is exactly the
        // divergence this file exists to avoid.
        ConstValue::Trit(t) if integral => Ok(ConstValue::Int(t as i64)),
        ConstValue::Trit(_) if matches!(ty, ManiType::Trit | ManiType::Bool3) => Ok(v),
        _ => Err(ConstError::NotConstant),
    }
}
