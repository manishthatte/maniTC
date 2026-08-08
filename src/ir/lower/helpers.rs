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
    let is_float = matches!(ty, ManiType::Float);
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
