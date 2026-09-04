// ir/lower/helpers.rs — Default impl and free lowering helper functions.
// Included in ir/lower/mod.rs as mod helpers + use helpers::*.

use super::*;
use crate::ast::{BinOpKind, Lit, Span, UnOpKind};
use crate::lang::LangVersion;
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

pub(crate) fn binop_to_ir(op: &BinOpKind, ty: &ManiType, lang: LangVersion) -> IRBinOp {
    // Tfloat lowers to F64 just like Float, so both must use float compares.
    let is_float = matches!(ty, ManiType::Float | ManiType::Tfloat);
    // C4. Float division is untouched: IEEE division already rounds, and
    // `frem` is not a truncating integer remainder in the first place, so
    // there is nothing for the new rule to correct there.
    let round_to_nearest = lang.division_rounds_to_nearest() && !is_float;
    // N5. `int` and `t27` are the 27-trit types; `trint` is deliberately not
    // one of them — it is the wider type v2 provides for code that wants the
    // machine word, so checking it would remove the escape hatch. `tryte` and
    // `t9` are narrower than their IR type too, but N5 is about `int`, and
    // widening the change to them would be a separate decision made silently.
    // C3: `TN(27)` and not `TN(_)`. The guard is about the 27-trit WORD
    // specifically — `AddT27` emits N5's overflow check against that width —
    // so a `t<18>` or a `t54` must not acquire it by looking similar. Matching
    // the width is the whole reason the width is a parameter.
    let checked_word = lang.int_is_27_trits()
        && matches!(ty, ManiType::Int | ManiType::TN(27));
    match op {
        BinOpKind::Add => if checked_word { IRBinOp::AddT27 } else { IRBinOp::Add },
        BinOpKind::Sub => if checked_word { IRBinOp::SubT27 } else { IRBinOp::Sub },
        BinOpKind::Mul => if checked_word { IRBinOp::MulT27 } else { IRBinOp::Mul },
        BinOpKind::Div => if round_to_nearest { IRBinOp::DivNear } else { IRBinOp::Div },
        BinOpKind::Rem => if round_to_nearest { IRBinOp::RemNear } else { IRBinOp::Rem },
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
        | BinOpKind::Tcon | BinOpKind::Tany
        | BinOpKind::Timp | BinOpKind::Teq
        | BinOpKind::Tandw | BinOpKind::Torw | BinOpKind::Txorw
        | BinOpKind::Timpw | BinOpKind::Tcmpw => IRBinOp::And, // all handled elsewhere
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
        // C2: lane-wise NOT. Also handled before this call, and unlike the
        // arms below this one is not a placeholder — `TritNeg` really is what
        // `tnotw` lowers to, because negating a balanced-ternary word flips
        // every trit in it.
        UnOpKind::Tnotw => IRUnOp::Neg,
        // C1. Also handled before this call, in lower_expr — each expands to
        // a clamp against a constant rather than to one IR unary op.
        UnOpKind::Tposs | UnOpKind::Tnec => IRUnOp::Neg,
        UnOpKind::Deref | UnOpKind::Ref => IRUnOp::Not, // placeholder
    }
}

// Span is imported for potential future use in error reporting
#[allow(dead_code)]
pub(crate) fn _use_span(_s: Span) {}
