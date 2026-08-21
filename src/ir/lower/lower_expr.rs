// ir/lower/lower_expr.rs — Expression and pointer lowering for IRLowerer.

use super::IRLowerer;
use super::helpers::{array_value_ty, binop_to_ir, slot_access_ty, unop_to_ir};
use crate::ir::types::*;
use crate::ast::{BinOpKind, UnOpKind};
use crate::semantic::{ManiType, TypedExpr, TypedExprKind};

/// Strip generic parameters from a type name string.
/// e.g. "Vec<int>" → "Vec", "Map<str,int>" → "Map", "int" → "int"
fn strip_generics(type_name: &str) -> &str {
    if let Some(idx) = type_name.find('<') {
        &type_name[..idx]
    } else {
        type_name
    }
}

/// Methods that compare or order the elements a collection holds, per receiver.
///
/// `str` is a value for `==` but an IDENTITY once it is inside a collection:
/// every element reaches the runtime type-erased to i64, which is a pointer on
/// the native backend and an intern id on T3.  So a map keyed by strings used
/// to answer `contains_key` by pointer, and matched only literals the C
/// compiler had already merged — a key built at run time missed its own entry,
/// and the same text could be inserted twice.  Sorting was worse: it ordered
/// pointers on BOTH backends, so the two agreed on an answer that was not
/// sorted, which no amount of cross-backend comparison could catch.
///
/// These are the methods that actually look at the value.  For each, a
/// `str`-typed element routes to a `_str` variant that compares text.  The rest
/// of the surface — len, keys, push, the set algebra — needs nothing: it either
/// does not compare, or it compares entries that these have already made
/// canonical.
const STR_SENSITIVE: &[(&str, &str)] = &[
    ("Map", "insert"),
    ("Map", "get"),
    ("Map", "get_or"),
    ("Map", "contains_key"),
    ("Map", "remove"),
    ("Set", "insert"),
    ("Set", "contains"),
    ("Set", "remove"),
    ("Vec", "contains"),
    ("Vec", "index_of"),
    ("Vec", "sort"),
];

/// Does `method` on this receiver compare `str` values, and so need the text
/// comparison rather than the pointer one?
///
/// The type argument that matters is the first one in every case: `Map<K,V>`
/// compares keys, `Set<T>` and `Vec<T>` their elements.  A `Map<int,str>` needs
/// nothing — its values are never compared.
fn needs_str_compare(recv: &ManiType, method: &str) -> bool {
    let ManiType::Generic(name, args) = recv else { return false };
    STR_SENSITIVE.contains(&(name.as_str(), method))
        && matches!(args.first(), Some(ManiType::Str))
}

/// Is this a `Vec<T>`?
///
/// A `Vec` is a heap container whose value is a pointer to a `{data, len, cap}`
/// header, so it must NOT be indexed the way a flat `[T; N]` array is: a
/// GetPtr+Load against the header reads the pointer, length and capacity fields
/// as though they were elements 0, 1 and 2. Indexing lowers to the same native
/// `Vec::get` / `Vec::set` calls the methods use.
pub(super) fn is_vec(ty: &ManiType) -> bool {
    matches!(ty, ManiType::Generic(name, _) if name == "Vec")
}

impl IRLowerer {
    pub(super) fn lower_expr(&mut self, expr: &TypedExpr) -> IRValue {
        let ty = IRType::from_mani(&expr.ty);
        match &expr.kind {
            TypedExprKind::Lit(lit) => self.lower_lit_typed(&lit.clone(), Some(&expr.ty)),

            TypedExprKind::Ident(name) => {
                if let Some((alloca, var_ty)) = self.locals.get(name).cloned() {
                    // Real struct variables: the alloca IS the struct data (no load needed).
                    if let IRType::Struct(sname) = &var_ty {
                        if self.is_real_struct(sname) {
                            return IRValue::Temp(alloca);
                        }
                    }
                    let dst = self.fresh_temp();
                    self.emit(IRInstr::Load {
                        dst: dst.clone(),
                        ptr: IRValue::Temp(alloca),
                        // Array-typed locals hold a pointer; load it as one.
                        ty: array_value_ty(&var_ty),
                    });
                    IRValue::Temp(dst)
                } else if name.contains("::") {
                    // Check if this is an enum variant constructor: "EnumName::VariantName"
                    let mut parts = name.splitn(2, "::");
                    if let (Some(enum_name), Some(variant_name)) = (parts.next(), parts.next()) {
                        if let Some(variants) = self.enum_variants.get(enum_name).cloned() {
                            if let Some(idx) = variants.iter().position(|v| v == variant_name) {
                                return IRValue::Const(IRConst::Int(idx as i64));
                            }
                        }
                    }
                    self.lower_global_read(name)
                } else {
                    self.lower_global_read(name)
                }
            }

            TypedExprKind::BinOp(lhs, op, rhs) => {
                // `tand`, `tor` and `tany` applied to two `bool`s ARE `&&` and
                // `||`, and that is provable rather than convenient: with false
                // as -1 and true as +1, min, max and "either +1 wins" are all
                // CLOSED on {-1, +1}. No two-valued pair can produce 0, so the
                // three-valued spelling of a two-valued question is the
                // two-valued operator, and the result is a `bool` that an `if`
                // will take.
                //
                // `txor` and `tcon` are deliberately NOT in that list: mod-3
                // addition and consensus both reach 0 from two-valued inputs —
                // `true txor false` is `unknown`, `true tcon false` is
                // `unknown` — so they stay three-valued and want a `tif`.
                // See binop_type, which types this the same way.
                let op = &match (op, &lhs.ty, &rhs.ty) {
                    (BinOpKind::Tand, ManiType::Bool, ManiType::Bool) => BinOpKind::And,
                    (BinOpKind::Tor, ManiType::Bool, ManiType::Bool)
                    | (BinOpKind::Tany, ManiType::Bool, ManiType::Bool) => BinOpKind::Or,
                    (other, _, _) => other.clone(),
                };
                // Special: ternary logic ops
                match op {
                    BinOpKind::And => {
                        // Short-circuit &&: if lhs is false, skip rhs; result = false.
                        let lv = self.lower_expr(lhs);
                        let rhs_label   = self.fresh_label("sc_and_rhs");
                        let false_label = self.fresh_label("sc_and_false");
                        let end_label   = self.fresh_label("sc_and_end");
                        self.set_term(IRTerminator::BinBranch {
                            cond: lv.clone(),
                            true_label: rhs_label.clone(),
                            false_label: false_label.clone(),
                        });
                        // False path block: result = 0
                        let false_idx = self.new_block(false_label.clone());
                        self.switch_to(false_idx);
                        self.set_term(IRTerminator::Jump(end_label.clone()));
                        // RHS block
                        let rhs_idx = self.new_block(rhs_label);
                        self.switch_to(rhs_idx);
                        let rv = self.lower_expr(rhs);
                        let rhs_end = self.blocks[self.current_block].label.clone();
                        self.set_term(IRTerminator::Jump(end_label.clone()));
                        // End block: PHI(0 from false_label, rv from rhs path)
                        let end_idx = self.new_block(end_label);
                        self.switch_to(end_idx);
                        let dst = self.fresh_temp();
                        self.emit(IRInstr::Phi {
                            dst: dst.clone(),
                            ty: IRType::Bool,
                            incoming: vec![
                                (IRValue::Const(IRConst::Int(0)), false_label),
                                (rv, rhs_end),
                            ],
                        });
                        let _ = (false_idx, rhs_idx, end_idx);
                        return IRValue::Temp(dst);
                    }
                    BinOpKind::Or => {
                        // Short-circuit ||: if lhs is true, skip rhs; result = true.
                        let lv = self.lower_expr(lhs);
                        let rhs_label  = self.fresh_label("sc_or_rhs");
                        let true_label = self.fresh_label("sc_or_true");
                        let end_label  = self.fresh_label("sc_or_end");
                        self.set_term(IRTerminator::BinBranch {
                            cond: lv.clone(),
                            true_label: true_label.clone(),
                            false_label: rhs_label.clone(),
                        });
                        // True path block: result = 1
                        let true_idx = self.new_block(true_label.clone());
                        self.switch_to(true_idx);
                        self.set_term(IRTerminator::Jump(end_label.clone()));
                        // RHS block
                        let rhs_idx = self.new_block(rhs_label);
                        self.switch_to(rhs_idx);
                        let rv = self.lower_expr(rhs);
                        let rhs_end = self.blocks[self.current_block].label.clone();
                        self.set_term(IRTerminator::Jump(end_label.clone()));
                        // End block: PHI(1 from true_label, rv from rhs path)
                        let end_idx = self.new_block(end_label);
                        self.switch_to(end_idx);
                        let dst = self.fresh_temp();
                        self.emit(IRInstr::Phi {
                            dst: dst.clone(),
                            ty: IRType::Bool,
                            incoming: vec![
                                (IRValue::Const(IRConst::Int(1)), true_label),
                                (rv, rhs_end),
                            ],
                        });
                        let _ = (true_idx, rhs_idx, end_idx);
                        return IRValue::Temp(dst);
                    }
                    BinOpKind::Tand => {
                        // Three-way short-circuit TAND (Claim 23):
                        //   LHS = -1 → short-circuit to -1 (don't evaluate RHS)
                        //   LHS =  0 → evaluate RHS, result = TritMin(0, RHS)
                        //   LHS = +1 → evaluate RHS, result = TritMin(+1, RHS) = RHS
                        let lv = self.lower_ternary_operand(lhs);
                        let rhs_label   = self.fresh_label("sc_tand_rhs");
                        let neg_label   = self.fresh_label("sc_tand_neg");
                        let end_label   = self.fresh_label("sc_tand_end");
                        self.set_term(IRTerminator::TritBranch {
                            cond: lv.clone(),
                            pos_label: rhs_label.clone(),    // +1 → evaluate RHS
                            zero_label: rhs_label.clone(),   //  0 → evaluate RHS
                            neg_label: neg_label.clone(),    // -1 → short-circuit
                        });
                        // Negative path: short-circuit result = -1
                        let neg_idx = self.new_block(neg_label.clone());
                        self.switch_to(neg_idx);
                        self.set_term(IRTerminator::Jump(end_label.clone()));
                        // RHS evaluation path
                        let rhs_idx = self.new_block(rhs_label);
                        self.switch_to(rhs_idx);
                        let rv = self.lower_ternary_operand(rhs);
                        let rhs_result = self.fresh_temp();
                        self.emit(IRInstr::TritMin { dst: rhs_result.clone(), a: lv, b: rv });
                        let rhs_end = self.blocks[self.current_block].label.clone();
                        self.set_term(IRTerminator::Jump(end_label.clone()));
                        // Merge: PHI(-1 from neg path, TritMin result from rhs path)
                        let end_idx = self.new_block(end_label);
                        self.switch_to(end_idx);
                        let dst = self.fresh_temp();
                        self.emit(IRInstr::Phi {
                            dst: dst.clone(),
                            ty: IRType::Trit,
                            incoming: vec![
                                (IRValue::Const(IRConst::Trit(-1)), neg_label),
                                (IRValue::Temp(rhs_result), rhs_end),
                            ],
                        });
                        let _ = (neg_idx, rhs_idx, end_idx);
                        return IRValue::Temp(dst);
                    }
                    BinOpKind::Tor => {
                        // Three-way short-circuit TOR (Claim 23):
                        //   LHS = +1 → short-circuit to +1 (don't evaluate RHS)
                        //   LHS =  0 → evaluate RHS, result = TritMax(0, RHS)
                        //   LHS = -1 → evaluate RHS, result = TritMax(-1, RHS) = RHS
                        let lv = self.lower_ternary_operand(lhs);
                        let rhs_label  = self.fresh_label("sc_tor_rhs");
                        let pos_label  = self.fresh_label("sc_tor_pos");
                        let end_label  = self.fresh_label("sc_tor_end");
                        self.set_term(IRTerminator::TritBranch {
                            cond: lv.clone(),
                            pos_label: pos_label.clone(),    // +1 → short-circuit
                            zero_label: rhs_label.clone(),   //  0 → evaluate RHS
                            neg_label: rhs_label.clone(),    // -1 → evaluate RHS
                        });
                        // Positive path: short-circuit result = +1
                        let pos_idx = self.new_block(pos_label.clone());
                        self.switch_to(pos_idx);
                        self.set_term(IRTerminator::Jump(end_label.clone()));
                        // RHS evaluation path
                        let rhs_idx = self.new_block(rhs_label);
                        self.switch_to(rhs_idx);
                        let rv = self.lower_ternary_operand(rhs);
                        let rhs_result = self.fresh_temp();
                        self.emit(IRInstr::TritMax { dst: rhs_result.clone(), a: lv, b: rv });
                        let rhs_end = self.blocks[self.current_block].label.clone();
                        self.set_term(IRTerminator::Jump(end_label.clone()));
                        // Merge: PHI(+1 from pos path, TritMax result from rhs path)
                        let end_idx = self.new_block(end_label);
                        self.switch_to(end_idx);
                        let dst = self.fresh_temp();
                        self.emit(IRInstr::Phi {
                            dst: dst.clone(),
                            ty: IRType::Trit,
                            incoming: vec![
                                (IRValue::Const(IRConst::Trit(1)), pos_label),
                                (IRValue::Temp(rhs_result), rhs_end),
                            ],
                        });
                        let _ = (pos_idx, rhs_idx, end_idx);
                        return IRValue::Temp(dst);
                    }
                    BinOpKind::Txor => {
                        // txor(a, b) = balanced (a + b) mod 3 — the sum digit of
                        // balanced-ternary addition, i.e. sum without carry.
                        //
                        // Changed 19 August 2026 from clamped |a - b|, which was
                        // never a ternary XOR: it could not return `-` at all, so
                        // a third of the digit set was unreachable in its range,
                        // and it was not invertible — for any fixed b, two of the
                        // three inputs mapped to `+`, so `x txor k` could not be
                        // undone. tests/22_crypto.mt had to hand-roll mod-3 as its
                        // own `txor_trit` for exactly that reason. Mod-3 addition
                        // is a bijection for fixed b, is surjective onto all three
                        // trits, and is the true analogue of binary XOR.
                        //
                        // Branch-free. With s = a + b in [-2, 2] and c = s clamped
                        // to [-1, 1], the wrap
                        //     -2 -> +1,  -1 -> -1,  0 -> 0,  +1 -> +1,  +2 -> -1
                        // is exactly 3c - 2s:
                        //     s=+2: 3(+1) - 4 = -1     s=-2: 3(-1) + 4 = +1
                        //     s=+1: 3(+1) - 2 = +1     s=-1: 3(-1) + 2 = -1
                        //     s= 0: 0 - 0 = 0
                        let lv = self.lower_ternary_operand(lhs);
                        let rv = self.lower_ternary_operand(rhs);
                        let s = self.fresh_temp();
                        self.emit(IRInstr::BinOp {
                            dst: s.clone(), op: IRBinOp::Add,
                            lhs: lv, rhs: rv, ty: IRType::I64,
                        });
                        // c = max(min(s, +1), -1)
                        let lo = self.fresh_temp();
                        self.emit(IRInstr::TritMin {
                            dst: lo.clone(),
                            a: IRValue::Temp(s.clone()),
                            b: IRValue::Const(IRConst::Int(1)),
                        });
                        let c = self.fresh_temp();
                        self.emit(IRInstr::TritMax {
                            dst: c.clone(),
                            a: IRValue::Temp(lo),
                            b: IRValue::Const(IRConst::Int(-1)),
                        });
                        let c3 = self.fresh_temp();
                        self.emit(IRInstr::BinOp {
                            dst: c3.clone(), op: IRBinOp::Mul,
                            lhs: IRValue::Temp(c), rhs: IRValue::Const(IRConst::Int(3)),
                            ty: IRType::I64,
                        });
                        let s2 = self.fresh_temp();
                        self.emit(IRInstr::BinOp {
                            dst: s2.clone(), op: IRBinOp::Mul,
                            lhs: IRValue::Temp(s), rhs: IRValue::Const(IRConst::Int(2)),
                            ty: IRType::I64,
                        });
                        let raw = self.fresh_temp();
                        self.emit(IRInstr::BinOp {
                            dst: raw.clone(), op: IRBinOp::Sub,
                            lhs: IRValue::Temp(c3), rhs: IRValue::Temp(s2),
                            ty: IRType::I64,
                        });
                        // Numerically a no-op — raw is already in [-1, 1] — but it
                        // makes the result carry the trit type the backends expect,
                        // exactly as the previous lowering's trailing TritMin did.
                        let dst = self.fresh_temp();
                        self.emit(IRInstr::TritMin {
                            dst: dst.clone(),
                            a: IRValue::Temp(raw),
                            b: IRValue::Const(IRConst::Int(1)),
                        });
                        return IRValue::Temp(dst);
                    }
                    BinOpKind::Tcon => {
                        // tcon(a,b) = consensus: +1 if both +1, -1 if both -1, else 0.
                        // tcon(a,b) = TritMin(TritMax(a,b), 0) + TritMax(TritMin(a,b), 0)
                        let lv = self.lower_ternary_operand(lhs);
                        let rv = self.lower_ternary_operand(rhs);
                        let t_max = self.fresh_temp();
                        self.emit(IRInstr::TritMax { dst: t_max.clone(), a: lv.clone(), b: rv.clone() });
                        let t_min = self.fresh_temp();
                        self.emit(IRInstr::TritMin { dst: t_min.clone(), a: lv, b: rv });
                        // clamp_neg(max): TritMin(max, 0) — 0 or negative
                        let neg_part = self.fresh_temp();
                        self.emit(IRInstr::TritMin { dst: neg_part.clone(), a: IRValue::Temp(t_max), b: IRValue::Const(IRConst::Int(0)) });
                        // clamp_pos(min): TritMax(min, 0) — 0 or positive
                        let pos_part = self.fresh_temp();
                        self.emit(IRInstr::TritMax { dst: pos_part.clone(), a: IRValue::Temp(t_min), b: IRValue::Const(IRConst::Int(0)) });
                        let dst = self.fresh_temp();
                        self.emit(IRInstr::BinOp {
                            dst: dst.clone(), op: IRBinOp::Add,
                            lhs: IRValue::Temp(neg_part), rhs: IRValue::Temp(pos_part), ty: IRType::I8,
                        });
                        return IRValue::Temp(dst);
                    }
                    BinOpKind::Tany => {
                        // tany(a,b) = +1 if either +1, -1 if either -1, else 0.
                        // pos = TritMax(a,b); neg = TritMin(a,b)
                        // pos_clamped = TritMax(pos, 0); neg_clamped = TritMin(neg, 0)
                        // result = pos_clamped + neg_clamped * (1 - pos_clamped)
                        let lv = self.lower_ternary_operand(lhs);
                        let rv = self.lower_ternary_operand(rhs);
                        let t_max = self.fresh_temp();
                        self.emit(IRInstr::TritMax { dst: t_max.clone(), a: lv.clone(), b: rv.clone() });
                        let t_min = self.fresh_temp();
                        self.emit(IRInstr::TritMin { dst: t_min.clone(), a: lv, b: rv });
                        let pos_clamped = self.fresh_temp();
                        self.emit(IRInstr::TritMax { dst: pos_clamped.clone(), a: IRValue::Temp(t_max), b: IRValue::Const(IRConst::Int(0)) });
                        let neg_clamped = self.fresh_temp();
                        self.emit(IRInstr::TritMin { dst: neg_clamped.clone(), a: IRValue::Temp(t_min), b: IRValue::Const(IRConst::Int(0)) });
                        // weight = 1 - pos_clamped
                        let weight = self.fresh_temp();
                        self.emit(IRInstr::BinOp {
                            dst: weight.clone(), op: IRBinOp::Sub,
                            lhs: IRValue::Const(IRConst::Int(1)), rhs: IRValue::Temp(pos_clamped.clone()), ty: IRType::I8,
                        });
                        // neg_contrib = neg_clamped * weight
                        let neg_contrib = self.fresh_temp();
                        self.emit(IRInstr::BinOp {
                            dst: neg_contrib.clone(), op: IRBinOp::Mul,
                            lhs: IRValue::Temp(neg_clamped), rhs: IRValue::Temp(weight), ty: IRType::I8,
                        });
                        let dst = self.fresh_temp();
                        self.emit(IRInstr::BinOp {
                            dst: dst.clone(), op: IRBinOp::Add,
                            lhs: IRValue::Temp(pos_clamped), rhs: IRValue::Temp(neg_contrib), ty: IRType::I8,
                        });
                        return IRValue::Temp(dst);
                    }
                    BinOpKind::Range | BinOpKind::RangeInclusive => {
                        let lv = self.lower_expr(lhs);
                        let rv = self.lower_expr(rhs);
                        let dst = self.fresh_temp();
                        self.emit(IRInstr::Assign { dst: dst.clone(), src: lv, ty: ty.clone() });
                        let _ = rv;
                        return IRValue::Temp(dst);
                    }
                    _ => {}
                }

                let lv = self.lower_expr(lhs);
                let rv = self.lower_expr(rhs);
                let ir_op = binop_to_ir(op, &lhs.ty);
                let dst = self.fresh_temp();
                self.emit(IRInstr::BinOp {
                    dst: dst.clone(),
                    op: ir_op,
                    lhs: lv,
                    rhs: rv,
                    ty,
                });
                IRValue::Temp(dst)
            }

            TypedExprKind::UnOp(op, operand) => {
                match op {
                    UnOpKind::Tnot => {
                        let val = self.lower_expr(operand);
                        let dst = self.fresh_temp();
                        self.emit(IRInstr::TritNeg { dst: dst.clone(), a: val });
                        return IRValue::Temp(dst);
                    }
                    _ => {}
                }
                let val = self.lower_expr(operand);
                let ir_op = if matches!(op, UnOpKind::Neg) && matches!(ty, IRType::F64) {
                    IRUnOp::FNeg
                } else {
                    unop_to_ir(op)
                };
                let dst = self.fresh_temp();
                self.emit(IRInstr::UnOp {
                    dst: dst.clone(),
                    op: ir_op,
                    operand: val,
                    ty,
                });
                IRValue::Temp(dst)
            }

            TypedExprKind::Call(callee, args) => {
                // Detect print intrinsics. Bare `print`/`println` are
                // variadic line-printers: every argument is printed in
                // order by its type, followed by one newline (all example
                // and ThatteOS call sites treat one call as one line).
                if let TypedExprKind::Ident(name) = &callee.kind {
                    if name == "println" || name == "print" {
                        for arg in args {
                            let val = self.lower_expr(arg);
                            match &arg.ty {
                                ManiType::Str => self.emit(IRInstr::PrintStr(val)),
                                ManiType::Int => self.emit(IRInstr::PrintInt(val)),
                                ManiType::Float => self.emit(IRInstr::PrintFloat(val)),
                                ManiType::Bool3 => self.emit(IRInstr::PrintBool3(val)),
                                ManiType::Trit => self.emit(IRInstr::PrintTrit(val)),
                                // A char is a Unicode scalar, so printing one
                                // prints the character — not its codepoint.
                                // Routed through str::from_char rather than a
                                // new IR instruction so both backends share
                                // the single primitive added for str::.
                                ManiType::Char => {
                                    let t = self.fresh_temp();
                                    self.emit(IRInstr::Call {
                                        dst: Some(t.clone()),
                                        func: "str::from_char".to_string(),
                                        args: vec![val],
                                        ret_ty: IRType::from_mani(&ManiType::Str),
                                    });
                                    self.emit(IRInstr::PrintStr(IRValue::Temp(t)));
                                }
                                ManiType::Tryte
                                | ManiType::T9
                                | ManiType::T27
                                | ManiType::T54
                                | ManiType::Bool
                                | ManiType::Unknown => {
                                    self.emit(IRInstr::PrintInt(val))
                                }
                                _ => self.emit(IRInstr::PrintStr(val)),
                            }
                        }
                        let nl = self.intern_string("\n");
                        self.emit(IRInstr::PrintStr(IRValue::Const(IRConst::Str(nl))));
                        return IRValue::Void;
                    }
                }

                // Check if callee is an fn-type variable (indirect call).
                let is_indirect = if let ManiType::Fn(..) = &callee.ty {
                    if let TypedExprKind::Ident(n) = &callee.kind {
                        !n.starts_with("__lambda_") && !n.contains("::")
                    } else {
                        true
                    }
                } else {
                    false
                };

                // fmt::format(tmpl, [a, b, ...]) — the argument array is a
                // syntactic wrapper: both backends receive the elements as
                // individual trailing arguments (T3 syscall 127 reads them
                // from R2.., the C runtime via varargs). Splat a literal
                // array; a non-literal array value has no known arity here
                // and falls through unchanged.
                let is_fmt_format = matches!(
                    &callee.kind,
                    TypedExprKind::Ident(n) if n == "fmt::format" || n == "fmt_format"
                );

                let callee_param_manitys = match &callee.kind {
                    TypedExprKind::Ident(n) => self.fn_param_manitys.get(n).cloned(),
                    _ => None,
                };

                let mut arg_vals = Vec::new();
                for (arg_i, arg) in args.iter().enumerate() {
                    if is_fmt_format && arg_i > 0 {
                        // Substitution arguments (everything after the
                        // template) become individual string arguments:
                        // a literal array is splatted, and non-string
                        // values go through the matching fmt::show_*
                        // conversion so the runtime only ever sees
                        // strings for its {} slots.
                        if let TypedExprKind::Array(elems) = &arg.kind {
                            for elem in elems {
                                let v = self.lower_expr(elem);
                                let v = self.fmt_arg_to_str(v, &elem.ty);
                                arg_vals.push(v);
                            }
                            continue;
                        }
                        let v = self.lower_expr(arg);
                        let v = self.fmt_arg_to_str(v, &arg.ty);
                        arg_vals.push(v);
                        continue;
                    }
                    let mut v = self.lower_expr(arg);
                    if let Some(ptys) = &callee_param_manitys {
                        if let Some(pty) = ptys.get(arg_i) {
                            let pty = pty.clone();
                            v = self.coerce_value(v, &arg.ty, &pty);
                        }
                    }
                    arg_vals.push(v);
                }

                if is_indirect {
                    let fn_ptr = self.lower_expr(callee);
                    if ty == IRType::Void {
                        self.emit(IRInstr::CallIndirect {
                            dst: None,
                            fn_ptr,
                            args: arg_vals,
                            ret_ty: IRType::Void,
                        });
                        IRValue::Void
                    } else {
                        let dst = self.fresh_temp();
                        self.emit(IRInstr::CallIndirect {
                            dst: Some(dst.clone()),
                            fn_ptr,
                            args: arg_vals,
                            ret_ty: ty,
                        });
                        IRValue::Temp(dst)
                    }
                } else {
                    let func_name = match &callee.kind {
                        TypedExprKind::Ident(n) => n.clone(),
                        _ => {
                            let v = self.lower_expr(callee);
                            format!("__indirect_{:?}", v)
                        }
                    };

                    // Stdlib functions expecting a length-prefixed [trit] array.
                    //
                    // These names are matched against what the source actually
                    // writes, so the module qualifier has to be right.
                    // `from_balanced_ternary` is declared in stdlib/math.mt:209,
                    // NOT stdlib/ternary.mt — the old `ternary::` spelling here
                    // matched nothing, so every call went unconverted. Measured
                    // 18 Aug 2026 with a sized `[+, 0, -]` (= -8): T3 returned 0
                    // and LLVM segfaulted. Fixed by spelling it `math::`.
                    //
                    // Not listed, because their layout is unverified and they do
                    // not currently compile (see §8/§9): `ternary::pack_t9`,
                    // `fmt::show_trit_slice`. `TernaryTrie`'s key methods do not
                    // belong here at all — the runtime reads a key as a Vec, not
                    // as a trit array, so they are declared `Vec<trit>` in
                    // stdlib/collections.mt rather than bridged.
                    const LP_FUNCS: &[&str] = &[
                        "ternary::pack_trits",
                        "math::from_balanced_ternary",
                        "ternary::trits_to_str",
                    ];
                    if LP_FUNCS.contains(&func_name.as_str()) {
                        if let Some(first_arg) = args.first() {
                            // Unsized `[trit]` PARAMETER — flat, and its length
                            // is only known at run time, so the prefixed buffer
                            // cannot be an alloca here. Hand the flat pointer
                            // and the hidden length to __lp_from_flat, which
                            // mallocs the copy (LLVM helper in
                            // codegen_llvm/helpers.rs, T3 syscall #203).
                            //
                            // The gate is the hidden `#len:` local, NOT the
                            // type: a runtime-produced array — say
                            // `let a = math::to_balanced_ternary(-137)` — is
                            // also `Array(Trit, None)` but is ALREADY prefixed,
                            // and wrapping it would prefix it twice. `#len:` is
                            // registered (mod.rs, in the prologue) only for a
                            // genuine unsized array parameter, so it is exactly
                            // the flat-provenance test.
                            let flat_param = match &first_arg.kind {
                                TypedExprKind::Ident(name) => self
                                    .locals
                                    .contains_key(&Self::unsized_len_key(name)),
                                _ => false,
                            };
                            if flat_param {
                                if let ManiType::Array(elem_mty, None) = &first_arg.ty {
                                    if **elem_mty == ManiType::Trit {
                                        let raw_ptr = arg_vals[0].clone();
                                        let len_val = self.unsized_array_len(first_arg);
                                        let buf_t = self.fresh_temp();
                                        self.emit(IRInstr::Call {
                                            dst: Some(buf_t.clone()),
                                            func: "__lp_from_flat".to_string(),
                                            args: vec![raw_ptr, len_val],
                                            ret_ty: IRType::Ptr(Box::new(IRType::I64)),
                                        });
                                        arg_vals[0] = IRValue::Temp(buf_t);
                                    }
                                }
                            }
                            if let ManiType::Array(elem_mty, Some(n)) = &first_arg.ty {
                                if **elem_mty == ManiType::Trit {
                                    let n = *n;
                                    let raw_ptr = arg_vals[0].clone();
                                    // The runtime/emulator expect one word per slot:
                                    // memory[ptr] = len, memory[ptr+1+i] = trit i
                                    // (see read_lp_string / syscall pack_trits).
                                    // Use I64 slots throughout so the length word
                                    // never overlaps trit slots and the buffer is
                                    // large enough on the byte-scaled LLVM path.
                                    let buf_t = self.fresh_temp();
                                    self.emit(IRInstr::Alloca {
                                        dst: buf_t.clone(),
                                        ty: IRType::Array(Box::new(IRType::I64), n + 1),
                                    });
                                    let lp_t = self.fresh_temp();
                                    self.emit(IRInstr::GetPtr { dst: lp_t.clone(), ptr: IRValue::Temp(buf_t.clone()), idx: IRValue::Const(IRConst::Int(0)), ty: IRType::I64 });
                                    self.emit(IRInstr::Store { ptr: IRValue::Temp(lp_t), val: IRValue::Const(IRConst::Int(n as i64)), ty: IRType::I64 });
                                    for j in 0..n {
                                        let sp_t = self.fresh_temp();
                                        self.emit(IRInstr::GetPtr { dst: sp_t.clone(), ptr: raw_ptr.clone(), idx: IRValue::Const(IRConst::Int(j as i64)), ty: IRType::Trit });
                                        let ev_t = self.fresh_temp();
                                        self.emit(IRInstr::Load { dst: ev_t.clone(), ptr: IRValue::Temp(sp_t), ty: IRType::Trit });
                                        let dp_t = self.fresh_temp();
                                        self.emit(IRInstr::GetPtr { dst: dp_t.clone(), ptr: IRValue::Temp(buf_t.clone()), idx: IRValue::Const(IRConst::Int((j + 1) as i64)), ty: IRType::I64 });
                                        self.emit(IRInstr::Store { ptr: IRValue::Temp(dp_t), val: IRValue::Temp(ev_t), ty: IRType::I64 });
                                    }
                                    arg_vals[0] = IRValue::Temp(buf_t);
                                }
                            }
                        }
                    }

                    // Hidden trailing lengths for `[T]` (unsized array) params.
                    let typed_args: Vec<&TypedExpr> = args.iter().collect();
                    self.append_unsized_len_args(&func_name, &typed_args, &mut arg_vals);

                    if ty == IRType::Void {
                        self.emit(IRInstr::Call {
                            dst: None,
                            func: func_name,
                            args: arg_vals,
                            ret_ty: IRType::Void,
                        });
                        IRValue::Void
                    } else if let IRType::Struct(ref sname) = ty {
                        if self.is_real_struct(sname) {
                            self.lower_struct_call(func_name, arg_vals, sname.clone())
                        } else {
                            let dst = self.fresh_temp();
                            self.emit(IRInstr::Call { dst: Some(dst.clone()), func: func_name, args: arg_vals, ret_ty: ty });
                            IRValue::Temp(dst)
                        }
                    } else if let IRType::Array(_, _) = ty {
                        self.lower_array_call(func_name, arg_vals, ty)
                    } else {
                        let dst = self.fresh_temp();
                        self.emit(IRInstr::Call {
                            dst: Some(dst.clone()),
                            func: func_name,
                            args: arg_vals,
                            ret_ty: ty,
                        });
                        IRValue::Temp(dst)
                    }
                }
            }

            TypedExprKind::MethodCall(obj, method, args) => {
                // A method on a `Result` lowers to the same loads and branches
                // that `match` on one already does — see ir/lower/lower_result.rs.
                // Falling through to the generic call below is what produced
                // `Undefined label: Result::unwrap` (Section 18).
                if let Some(v) = self.lower_result_method(obj, method, args, &ty) {
                    return v;
                }
                let obj_val = self.lower_expr(obj);
                let mut arg_vals = vec![obj_val];
                for arg in args {
                    arg_vals.push(self.lower_expr(arg));
                }
                let obj_ty_display = obj.ty.display();
                let base_type = strip_generics(obj_ty_display.as_str());
                // A collection of `str` compares text, not pointers — route the
                // comparing methods to their `_str` variants (see STR_SENSITIVE).
                let suffix = if needs_str_compare(&obj.ty, method) { "_str" } else { "" };
                let func_name = format!("{}::{}{}", base_type, method, suffix);
                // Hidden trailing lengths for `[T]` (unsized array) params —
                // method params include self, so prepend the receiver.
                let typed_args: Vec<&TypedExpr> =
                    std::iter::once(obj.as_ref()).chain(args.iter()).collect();
                self.append_unsized_len_args(&func_name, &typed_args, &mut arg_vals);
                if ty == IRType::Void {
                    self.emit(IRInstr::Call {
                        dst: None,
                        func: func_name,
                        args: arg_vals,
                        ret_ty: IRType::Void,
                    });
                    IRValue::Void
                } else if let IRType::Struct(ref sname) = ty {
                    if self.is_real_struct(sname) {
                        self.lower_struct_call(func_name, arg_vals, sname.clone())
                    } else {
                        let dst = self.fresh_temp();
                        self.emit(IRInstr::Call { dst: Some(dst.clone()), func: func_name, args: arg_vals, ret_ty: ty });
                        IRValue::Temp(dst)
                    }
                } else if let IRType::Array(_, _) = ty {
                    return self.lower_array_call(func_name, arg_vals, ty);
                } else {
                    let dst = self.fresh_temp();
                    self.emit(IRInstr::Call {
                        dst: Some(dst.clone()),
                        func: func_name,
                        args: arg_vals,
                        ret_ty: ty,
                    });
                    IRValue::Temp(dst)
                }
            }

            TypedExprKind::Index(arr, idx) => {
                // `v[i]` on a Vec is `v.get(i)`. Falling through to GetPtr+Load
                // here would read the Vec's {data, len, cap} header instead of
                // its elements, silently and on both backends.
                if is_vec(&arr.ty) {
                    let arr_val = self.lower_expr(arr);
                    let idx_val = self.lower_expr(idx);
                    let dst = self.fresh_temp();
                    self.emit(IRInstr::Call {
                        dst: Some(dst.clone()),
                        func: "Vec::get".to_string(),
                        args: vec![arr_val, idx_val],
                        ret_ty: ty,
                    });
                    return IRValue::Temp(dst);
                }

                // A2: bounds-check when the element count is statically known.
                let arr_len = match &arr.ty {
                    ManiType::Array(_, Some(n)) => Some(*n),
                    _ => None,
                };
                let arr_val = self.lower_expr(arr);
                let idx_val = self.lower_expr(idx);
                if let Some(len) = arr_len {
                    self.emit(IRInstr::BoundsCheck { idx: idx_val.clone(), len });
                }
                let ptr_t = self.fresh_temp();
                self.emit(IRInstr::GetPtr {
                    dst: ptr_t.clone(),
                    ptr: arr_val,
                    idx: idx_val,
                    ty: ty.clone(),
                });
                let dst = self.fresh_temp();
                self.emit(IRInstr::Load {
                    dst: dst.clone(),
                    ptr: IRValue::Temp(ptr_t),
                    ty,
                });
                IRValue::Temp(dst)
            }

            TypedExprKind::Field(obj, field) => {
                let field_idx = self.field_slot_index(&obj.ty, field);

                let obj_val = self.lower_expr(obj);
                let idx = IRValue::Const(IRConst::Int(field_idx));
                // Uniform 8-byte slot convention for aggregates (see slot_access_ty).
                let ptr_t = self.fresh_temp();
                self.emit(IRInstr::GetPtr {
                    dst: ptr_t.clone(),
                    ptr: obj_val,
                    idx,
                    ty: IRType::I64,
                });
                let dst = self.fresh_temp();
                self.emit(IRInstr::Load {
                    dst: dst.clone(),
                    ptr: IRValue::Temp(ptr_t),
                    ty: slot_access_ty(&ty),
                });
                IRValue::Temp(dst)
            }

            TypedExprKind::Block(block) => self.lower_block(block),

            TypedExprKind::If(ie) => self.lower_if(ie, &expr.ty),

            TypedExprKind::Tif(te) => self.lower_tif(te, &expr.ty),

            TypedExprKind::Match(me) => self.lower_match(me, &expr.ty),

            TypedExprKind::For(fe) => {
                self.lower_for(fe);
                IRValue::Void
            }

            TypedExprKind::While(we) => {
                self.lower_while(we);
                IRValue::Void
            }

            TypedExprKind::Loop(block) => {
                self.lower_loop(block);
                IRValue::Void
            }

            TypedExprKind::Array(elems) => {
                let arr_ty = ty.clone();
                let alloca_t = self.fresh_temp();
                self.emit(IRInstr::Alloca { dst: alloca_t.clone(), ty: arr_ty.clone() });
                for (i, elem) in elems.iter().enumerate() {
                    let val = self.lower_expr(elem);
                    let idx_val = IRValue::Const(IRConst::Int(i as i64));
                    // Array-typed elements (nested arrays) are stored as
                    // pointers, one 8-byte slot each.
                    let elem_access = array_value_ty(&IRType::from_mani(&elem.ty));
                    let ptr_t = self.fresh_temp();
                    self.emit(IRInstr::GetPtr {
                        dst: ptr_t.clone(),
                        ptr: IRValue::Temp(alloca_t.clone()),
                        idx: idx_val,
                        ty: elem_access.clone(),
                    });
                    self.emit(IRInstr::Store {
                        ptr: IRValue::Temp(ptr_t),
                        val,
                        ty: elem_access,
                    });
                }
                IRValue::Temp(alloca_t)
            }

            TypedExprKind::Tuple(elems) => {
                let alloca_t = self.fresh_temp();
                self.emit(IRInstr::Alloca { dst: alloca_t.clone(), ty: ty.clone() });
                for (i, elem) in elems.iter().enumerate() {
                    let val = self.lower_expr(elem);
                    let idx_val = IRValue::Const(IRConst::Int(i as i64));
                    // Uniform 8-byte slot convention for aggregates (see slot_access_ty).
                    let ptr_t = self.fresh_temp();
                    self.emit(IRInstr::GetPtr {
                        dst: ptr_t.clone(),
                        ptr: IRValue::Temp(alloca_t.clone()),
                        idx: idx_val,
                        ty: IRType::I64,
                    });
                    self.emit(IRInstr::Store {
                        ptr: IRValue::Temp(ptr_t),
                        val,
                        ty: slot_access_ty(&IRType::from_mani(&elem.ty)),
                    });
                }
                IRValue::Temp(alloca_t)
            }

            TypedExprKind::StructLit(name, fields) => {
                let alloca_t = self.fresh_temp();
                self.emit(IRInstr::Alloca {
                    dst: alloca_t.clone(),
                    ty: IRType::Struct(name.clone()),
                });
                let field_manitys = self.struct_field_manitys.get(name).cloned();
                for (i, (_, fval)) in fields.iter().enumerate() {
                    let mut val = self.lower_expr(fval);
                    if let Some(fmt) = field_manitys.as_ref().and_then(|f| f.get(i)) {
                        let fmt = fmt.clone();
                        val = self.coerce_value(val, &fval.ty, &fmt);
                    }
                    let idx = IRValue::Const(IRConst::Int(i as i64));
                    // Uniform 8-byte slot convention for aggregates (see slot_access_ty).
                    let ptr_t = self.fresh_temp();
                    self.emit(IRInstr::GetPtr {
                        dst: ptr_t.clone(),
                        ptr: IRValue::Temp(alloca_t.clone()),
                        idx,
                        ty: IRType::I64,
                    });
                    self.emit(IRInstr::Store {
                        ptr: IRValue::Temp(ptr_t),
                        val,
                        ty: slot_access_ty(&IRType::from_mani(&fval.ty)),
                    });
                }
                IRValue::Temp(alloca_t)
            }

            TypedExprKind::Range(lo, hi, _inclusive) => {
                // Evaluate lo before hi for left-to-right side-effect order,
                // matching the other range lowerings.
                let lo_val = self.lower_expr(lo);
                let _hi_val = self.lower_expr(hi);
                lo_val
            }

            TypedExprKind::Cast(inner, target_ty) => {
                let val = self.lower_expr(inner);
                let from_ty = IRType::from_mani(&inner.ty);
                let to_ty = IRType::from_mani(target_ty);
                if from_ty == to_ty {
                    return val;
                }
                let dst = self.fresh_temp();
                self.emit(IRInstr::Cast {
                    dst: dst.clone(),
                    src: val,
                    from_ty,
                    to_ty,
                });
                IRValue::Temp(dst)
            }

            TypedExprKind::Question(inner) => {
                // ? operator on Result<T,E> — three-way branch.
                let is_result = matches!(&inner.ty,
                    ManiType::Generic(n, _) if n == "Result");

                let inner_val = self.lower_expr(inner);

                if !is_result {
                    return inner_val;
                }

                // inner_val is a pointer to [tag, value]. Load the tag word (offset 0).
                let tag_t = self.fresh_temp();
                self.emit(IRInstr::Load {
                    dst: tag_t.clone(),
                    ptr: inner_val.clone(),
                    ty: IRType::I64,
                });

                let ok_label      = self.fresh_label("q_ok");
                let err_label     = self.fresh_label("q_err");
                let unknown_label = self.fresh_label("q_unknown");
                let merge_label   = self.fresh_label("q_merge");

                let is_ok = self.fresh_temp();
                self.emit(IRInstr::BinOp {
                    dst: is_ok.clone(),
                    op: IRBinOp::IEq,
                    lhs: IRValue::Temp(tag_t.clone()),
                    rhs: IRValue::Const(IRConst::Int(1)),
                    ty: IRType::Bool,
                });
                self.set_term(IRTerminator::BinBranch {
                    cond: IRValue::Temp(is_ok),
                    true_label: ok_label.clone(),
                    false_label: err_label.clone(),
                });

                // Err / Unknown disambiguation block
                let err_idx = self.new_block(err_label.clone());
                self.switch_to(err_idx);
                let is_err = self.fresh_temp();
                self.emit(IRInstr::BinOp {
                    dst: is_err.clone(),
                    op: IRBinOp::IEq,
                    lhs: IRValue::Temp(tag_t.clone()),
                    rhs: IRValue::Const(IRConst::Int(-1)),
                    ty: IRType::Bool,
                });
                self.set_term(IRTerminator::BinBranch {
                    cond: IRValue::Temp(is_err),
                    true_label: err_label.clone() + "_real",
                    false_label: unknown_label.clone(),
                });

                let err_real_label = err_label.clone() + "_real";
                let err_real_idx = self.new_block(err_real_label);
                self.switch_to(err_real_idx);
                self.set_term(IRTerminator::Return(Some(inner_val.clone())));

                let unk_idx = self.new_block(unknown_label.clone());
                self.switch_to(unk_idx);
                self.set_term(IRTerminator::Return(Some(inner_val.clone())));

                let ok_idx = self.new_block(ok_label);
                self.switch_to(ok_idx);
                let val_ptr = self.fresh_temp();
                self.emit(IRInstr::GetPtr {
                    dst: val_ptr.clone(),
                    ptr: inner_val.clone(),
                    idx: IRValue::Const(IRConst::Int(1)),
                    ty: IRType::I64,
                });
                let val_t = self.fresh_temp();
                self.emit(IRInstr::Load {
                    dst: val_t.clone(),
                    ptr: IRValue::Temp(val_ptr),
                    ty: IRType::I64,
                });
                self.set_term(IRTerminator::Jump(merge_label.clone()));

                let merge_idx = self.new_block(merge_label.clone());
                self.switch_to(merge_idx);
                let _ = (err_idx, unk_idx, ok_idx, merge_idx);
                IRValue::Temp(val_t)
            }

            TypedExprKind::Return(e) => {
                let val = self.lower_expr(e);
                self.set_term(IRTerminator::Return(Some(val)));
                let after = self.fresh_label("after_ret");
                let idx = self.new_block(after);
                self.switch_to(idx);
                IRValue::Void
            }

            TypedExprKind::Break => {
                self.set_term(IRTerminator::Jump("__break__".to_string()));
                let after = self.fresh_label("after_break");
                let idx = self.new_block(after);
                self.switch_to(idx);
                IRValue::Void
            }

            TypedExprKind::Continue => {
                self.set_term(IRTerminator::Jump("__continue__".to_string()));
                let after = self.fresh_label("after_cont");
                let idx = self.new_block(after);
                self.switch_to(idx);
                IRValue::Void
            }

            TypedExprKind::Await(inner) => self.lower_expr(inner),

            TypedExprKind::Spawn(block) => {
                self.lower_block(block);
                IRValue::Void
            }

            TypedExprKind::Tresult(tr) => self.lower_tresult(tr, &expr.ty),
        }
    }

    /// Get a pointer (IRValue) for assignment targets.
    /// Reading a name that is not a local: module globals load their value
    /// through the global's address; anything else (function references,
    /// extern symbols) stays an address value.
    fn lower_global_read(&mut self, name: &str) -> IRValue {
        if let Some(gty) = self.global_vars.get(name).cloned() {
            let dst = self.fresh_temp();
            self.emit(IRInstr::Load {
                dst: dst.clone(),
                ptr: IRValue::Global(name.to_string()),
                ty: array_value_ty(&gty),
            });
            return IRValue::Temp(dst);
        }
        IRValue::Global(name.to_string())
    }

    /// Convert a fmt::format substitution argument to a string value.
    /// Strings pass through; scalars are routed through the matching
    /// fmt::show_* runtime conversion (both backends implement these).
    fn fmt_arg_to_str(&mut self, val: IRValue, ty: &ManiType) -> IRValue {
        let conv = match ty {
            ManiType::Str => return val,
            ManiType::Float | ManiType::Tfloat => "fmt::show_float",
            ManiType::Bool => "fmt::show_bool",
            // Trit/bool3 print as their numeric value: only show_int has a
            // T3 syscall, and both backends must format identically.
            ManiType::Int
            | ManiType::Trit
            | ManiType::Bool3
            | ManiType::Tryte
            | ManiType::T9
            | ManiType::T27
            | ManiType::T54
            | ManiType::Char => "fmt::show_int",
            // Unknown and composites: assume the caller already produced a
            // string (the documented [fmt::show_*(..)] pattern).
            _ => return val,
        };
        let dst = self.fresh_temp();
        self.emit(IRInstr::Call {
            dst: Some(dst.clone()),
            func: conv.to_string(),
            args: vec![val],
            ret_ty: IRType::Ptr(Box::new(IRType::I8)),
        });
        IRValue::Temp(dst)
    }

    pub(super) fn lower_expr_as_ptr(&mut self, expr: &TypedExpr) -> IRValue {
        use crate::ast::UnOpKind;
        match &expr.kind {
            TypedExprKind::Ident(name) => {
                if let Some((alloca, _)) = self.locals.get(name) {
                    IRValue::Temp(alloca.clone())
                } else {
                    IRValue::Global(name.clone())
                }
            }
            TypedExprKind::Index(arr, idx) => {
                let arr_val = self.lower_expr(arr);
                let idx_val = self.lower_expr(idx);
                let ptr_t = self.fresh_temp();
                self.emit(IRInstr::GetPtr {
                    dst: ptr_t.clone(),
                    ptr: arr_val,
                    idx: idx_val,
                    ty: IRType::from_mani(&expr.ty),
                });
                IRValue::Temp(ptr_t)
            }
            TypedExprKind::Field(obj, field) => {
                // Return a pointer to the struct field, NOT the field's value.
                // lower_expr for a Field does GetPtr + Load; here we want only
                // the GetPtr so that assignment stores into the field slot itself.
                let field_idx = self.field_slot_index(&obj.ty, field);
                let obj_val = self.lower_expr(obj);
                let idx = IRValue::Const(IRConst::Int(field_idx));
                // Uniform 8-byte slot convention for aggregates (see slot_access_ty).
                let ptr_t = self.fresh_temp();
                self.emit(IRInstr::GetPtr {
                    dst: ptr_t.clone(),
                    ptr: obj_val,
                    idx,
                    ty: IRType::I64,
                });
                IRValue::Temp(ptr_t)
            }
            TypedExprKind::UnOp(UnOpKind::Deref, inner) => self.lower_expr(inner),
            _ => self.lower_expr(expr),
        }
    }

    /// Slot index for `obj.field`, for both the rvalue and lvalue paths.
    ///
    /// Two shapes reach here. On a struct the field is a name and the index is
    /// its position in the declaration. On a tuple the field is a decimal
    /// literal — `p.0`, `p.1` — and the index is that number.
    ///
    /// The tuple case was missing until 20 August 2026. A tuple's type displays
    /// as `(int, int)`, which is never a key in `self.structs`, so the lookup
    /// missed and the `unwrap_or(0)` fallback below took over: EVERY tuple
    /// field access loaded element 0, identically on both backends and with no
    /// diagnostic. That fallback is the defect, not the missing arm — a lookup
    /// that cannot fail silently would have surfaced this the first time
    /// anyone wrote `p.1`.
    fn field_slot_index(&self, obj_ty: &ManiType, field: &str) -> i64 {
        if let ManiType::Tuple(elems) = obj_ty {
            if let Ok(i) = field.parse::<usize>() {
                if i < elems.len() {
                    return i as i64;
                }
            }
            return 0;
        }
        let struct_name = match obj_ty {
            ManiType::Struct(name) => name.clone(),
            _ => obj_ty.display().to_string(),
        };
        self.structs
            .get(&struct_name)
            .and_then(|fields| fields.iter().position(|(n, _)| n == field))
            .unwrap_or(0) as i64
    }
}
