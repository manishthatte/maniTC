// ir/lower/helpers.rs — Default impl and free lowering helper functions.
// Included in ir/lower/mod.rs as mod helpers + use helpers::*.

use super::*;
use crate::ast::{BinOpKind, Lit, Span, UnOpKind};
use crate::semantic::ManiType;

impl Default for IRLowerer {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Helper free functions
// ---------------------------------------------------------------------------

pub(crate) fn lit_to_irvalue(lit: &Lit) -> IRValue {
    match lit {
        Lit::Int(n) => IRValue::Const(IRConst::Int(*n)),
        Lit::Float(f) => IRValue::Const(IRConst::Float(*f)),
        Lit::Bool(b) => IRValue::Const(IRConst::Bool(*b)),
        Lit::Bool3(v) => IRValue::Const(IRConst::Trit(*v)),
        Lit::Trit(v) => IRValue::Const(IRConst::Trit(*v)),
        Lit::TernaryInt(n) => IRValue::Const(IRConst::Int(*n)),
        Lit::Str(s) => IRValue::Const(IRConst::Str(s.clone())),
        Lit::Char(c) => IRValue::Const(IRConst::Int(*c as i64)),
        Lit::Null => IRValue::Const(IRConst::Null),
    }
}

pub(crate) fn binop_to_ir(op: &BinOpKind, ty: &ManiType) -> IRBinOp {
    // Tfloat lowers to F64 just like Float, so both must use float compares.
    let is_float = matches!(ty, ManiType::Float | ManiType::Tfloat);
    match op {
        BinOpKind::Add => IRBinOp::Add,
        BinOpKind::Sub => IRBinOp::Sub,
        BinOpKind::Mul => IRBinOp::Mul,
        BinOpKind::Div => IRBinOp::Div,
        BinOpKind::Rem => IRBinOp::Rem,
        BinOpKind::Eq => if is_float { IRBinOp::FEq } else if matches!(ty, ManiType::Str) { IRBinOp::StrEq } else { IRBinOp::IEq },
        BinOpKind::Ne => if is_float { IRBinOp::FNe } else if matches!(ty, ManiType::Str) { IRBinOp::StrNe } else { IRBinOp::INe },
        BinOpKind::Lt => if is_float { IRBinOp::FLt } else { IRBinOp::ILt },
        BinOpKind::Gt => if is_float { IRBinOp::FGt } else { IRBinOp::IGt },
        BinOpKind::Le => if is_float { IRBinOp::FLe } else { IRBinOp::ILe },
        BinOpKind::Ge => if is_float { IRBinOp::FGe } else { IRBinOp::IGe },
        BinOpKind::And => IRBinOp::And,
        BinOpKind::Or => IRBinOp::Or,
        BinOpKind::BitAnd => IRBinOp::And,
        BinOpKind::BitOr => IRBinOp::Or,
        BinOpKind::BitXor => IRBinOp::Xor,
        BinOpKind::LShift => IRBinOp::LShift,
        BinOpKind::RShift => IRBinOp::RShift,
        BinOpKind::Tand | BinOpKind::Tor | BinOpKind::Txor
        | BinOpKind::Tcon | BinOpKind::Tany => IRBinOp::And, // all handled elsewhere
        BinOpKind::Range | BinOpKind::RangeInclusive => IRBinOp::Add,       // handled elsewhere
    }
}

/// Replace `IRValue::Void` PHI operands with a typed zero placeholder.
///
/// An arm that ends in `return`/`break`/`continue` produces no value; its
/// merge edge comes from an unreachable trailing block, so the placeholder
/// is never observed — but the backends need a real value token to emit.
pub(crate) fn sanitize_phi_incoming(
    incoming: Vec<(IRValue, String)>,
    ty: &IRType,
) -> Vec<(IRValue, String)> {
    incoming
        .into_iter()
        .map(|(v, label)| {
            let v = if matches!(v, IRValue::Void) {
                match ty {
                    IRType::F64 => IRValue::Const(IRConst::Float(0.0)),
                    _ => IRValue::Const(IRConst::Int(0)),
                }
            } else {
                v
            };
            (v, label)
        })
        .collect()
}

/// The type to use when LOADING or STORING an array-typed value.
///
/// Array-typed values are pointers at runtime; loading them with the bare
/// `Array` type would make the LLVM backend read a first-class aggregate.
/// The sized `Array` type is still what gets registered in `locals` (loop
/// lowering recovers nested bounds from it) — only the access type is
/// pointer-ized.
pub(crate) fn array_value_ty(ty: &IRType) -> IRType {
    match ty {
        IRType::Array(elem, _) => IRType::Ptr(elem.clone()),
        other => other.clone(),
    }
}

/// Uniform 8-byte slot convention for struct/tuple aggregates.
///
/// Every struct/tuple field occupies one 8-byte slot: the LLVM backend
/// mallocs structs as n_fields * 8 bytes (all-i64 field layout) and the
/// T3 backend indexes memory in unscaled words. GetPtr into an aggregate
/// therefore always uses `IRType::I64` as its element type, and the
/// matching load/store width is the type returned here: F64 for float
/// fields, pointer/struct types unchanged (8 bytes each), and I64 for
/// every narrower integer-like type (trit/bool/char/tryte/t9/...).
pub(crate) fn slot_access_ty(field_ty: &IRType) -> IRType {
    match field_ty {
        IRType::F64 => IRType::F64,
        IRType::Ptr(_) | IRType::Struct(_) => field_ty.clone(),
        _ => IRType::I64,
    }
}

pub(crate) fn unop_to_ir(op: &UnOpKind) -> IRUnOp {
    match op {
        UnOpKind::Neg => IRUnOp::Neg,
        UnOpKind::Not => IRUnOp::Not,
        UnOpKind::TritNeg => IRUnOp::Neg,
        UnOpKind::Tnot => IRUnOp::Neg, // handled before this call
        UnOpKind::Deref | UnOpKind::Ref => IRUnOp::Not, // placeholder
    }
}

// Span is imported for potential future use in error reporting
#[allow(dead_code)]
pub(crate) fn _use_span(_s: Span) {}
