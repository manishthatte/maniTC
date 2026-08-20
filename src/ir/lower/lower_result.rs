// ir/lower/lower_result.rs — methods on `Result<T, E>`, lowered once.
//
// `Result` was half-implemented, and the half that existed was the half that
// hid the problem: `Ok(7)` constructed, `match` consumed, and every method call
// passed semantic analysis and then failed at link time —
// `Undefined label: Result::unwrap` on T3, `undefined value '@Result_unwrap'`
// on LLVM. `type_inference.rs` even computed `unwrap`'s return type correctly.
// The type was usable, but only through `match` (ORACLE_FINDINGS.md Section 18).
//
// The obvious repair — write a body in the C runtime and another in the T3
// emitter — is the mistake this compiler has already paid for three times
// (Section 14a, Section 23, Section 25.1): two implementations of one function
// drift, and the differential oracle only notices once they disagree on an
// input someone happened to test. There WAS such a body, in fact: LLVM carried
// an `@result_unwrap` that returned word 1 with no tag check at all, so
// unwrapping an `Err` would have handed back the message pointer as if it were
// the value. It had no T3 counterpart and is now deleted.
//
// So these lower to IR instead — to the same loads, compares and branches that
// `match` on a `Result` already lowers to. One body, in the shared lowering,
// and neither backend learns anything new. The single exception is the unwrap
// guard, which must be able to fault: that is one runtime primitive per
// backend, mirroring the `manit_check_index` / SYSCALL #560 pair that A2's
// bounds check already uses, with the message spelled identically on both
// sides so a divergence would be visible.
//
// Layout, fixed by the constructors and by `lower_pattern_match`:
//
//     word 0   tag: +1 = Ok, 0 = Unknown, -1 = Err   (a trit, not a flag)
//     word 1   payload: the Ok value, or the Err/Unknown message pointer
//
// Author: Manish Jagdish Thatte

use super::IRLowerer;
use super::helpers::sanitize_phi_incoming;
use crate::ir::types::*;
use crate::semantic::{ManiType, TypedExpr};

/// Tag values, as written by `Ok`/`Unknown`/`Err` and read by pattern matching.
pub(super) const TAG_OK: i64 = 1;
pub(super) const TAG_UNKNOWN: i64 = 0;
pub(super) const TAG_ERR: i64 = -1;

/// The runtime guard `unwrap` calls. Defined in `runtime/core.c` for LLVM and
/// as SYSCALL #561 in the T3 emulator; both print the same message and exit 70,
/// exactly as every other ManiT fault does.
pub(super) const UNWRAP_GUARD: &str = "manit_check_result_ok";

/// Methods this file implements. Anything else on a `Result` is rejected by
/// the semantic pass with this list in the message, rather than being allowed
/// through to fail at link — that silence is what Section 18 was.
pub const RESULT_METHODS: &[&str] = &[
    "unwrap", "unwrap_or", "is_ok", "is_unknown", "is_err", "tag",
];

/// Is `ty` a `Result<T, E>`?
pub fn is_result(ty: &ManiType) -> bool {
    matches!(ty, ManiType::Generic(name, _) if name == "Result")
}

impl IRLowerer {
    /// Lower a method call whose receiver is a `Result`, or return `None` if
    /// this is not one (in which case the caller emits an ordinary call).
    pub(super) fn lower_result_method(
        &mut self,
        obj: &TypedExpr,
        method: &str,
        args: &[TypedExpr],
        result_ty: &IRType,
    ) -> Option<IRValue> {
        if !is_result(&obj.ty) {
            return None;
        }
        if !RESULT_METHODS.contains(&method) {
            return None;
        }

        let recv = self.lower_expr(obj);

        // The tag is word 0 — the pointer itself, no offset. `lower_pattern_match`
        // reads it exactly this way.
        let tag = self.fresh_temp();
        self.emit(IRInstr::Load {
            dst: tag.clone(),
            ptr: recv.clone(),
            ty: IRType::I64,
        });

        match method {
            "is_ok" => Some(self.tag_equals(&tag, TAG_OK)),
            "is_unknown" => Some(self.tag_equals(&tag, TAG_UNKNOWN)),
            "is_err" => Some(self.tag_equals(&tag, TAG_ERR)),

            // The ternary accessor, and the reason the other three are a
            // convenience rather than the interface: the tag IS a trit, so
            // `tif r.tag() { + => …, 0 => …, - => … }` dispatches on all three
            // outcomes at once, where `is_ok`/`is_err`/`is_unknown` decompose
            // one three-valued question into three two-valued ones.
            "tag" => {
                let dst = self.fresh_temp();
                self.emit(IRInstr::Cast {
                    dst: dst.clone(),
                    src: IRValue::Temp(tag),
                    from_ty: IRType::I64,
                    to_ty: IRType::Trit,
                });
                Some(IRValue::Temp(dst))
            }

            "unwrap" => {
                self.emit(IRInstr::Call {
                    dst: None,
                    func: UNWRAP_GUARD.to_string(),
                    args: vec![IRValue::Temp(tag)],
                    ret_ty: IRType::Void,
                });
                Some(self.load_payload(&recv))
            }

            "unwrap_or" => {
                // The default is evaluated unconditionally, as Rust's
                // `unwrap_or` is — `unwrap_or_else` is the lazy one, and this
                // language has no closures on Result yet to spell it with.
                let dflt = match args.first() {
                    Some(a) => self.lower_expr(a),
                    None => IRValue::Const(IRConst::Int(0)),
                };
                // Reading word 1 is safe whatever the tag: the box is always
                // two words, and on a non-Ok it holds the message pointer.
                // Only the select below decides whether that value is used.
                let payload = self.load_payload(&recv);
                let cond = self.tag_equals(&tag, TAG_OK);
                Some(self.select(cond, payload, dflt, result_ty))
            }

            _ => None,
        }
    }

    /// `tag == want`, as a Bool temp.
    fn tag_equals(&mut self, tag: &IRTemp, want: i64) -> IRValue {
        let dst = self.fresh_temp();
        self.emit(IRInstr::BinOp {
            dst: dst.clone(),
            op: IRBinOp::IEq,
            lhs: IRValue::Temp(tag.clone()),
            rhs: IRValue::Const(IRConst::Int(want)),
            ty: IRType::Bool,
        });
        IRValue::Temp(dst)
    }

    /// Load word 1 of the Result box. Deliberately the same GetPtr + Load pair
    /// that `bind_pattern_locals` emits for `Ok(v) => …`, including the `I64`
    /// element type: the payload slot is type-erased, and the consumer coerces
    /// it, which is why a `Result<float, str>` already prints correctly through
    /// `match`.
    fn load_payload(&mut self, recv: &IRValue) -> IRValue {
        let ptr = self.fresh_temp();
        self.emit(IRInstr::GetPtr {
            dst: ptr.clone(),
            ptr: recv.clone(),
            idx: IRValue::Const(IRConst::Int(1)),
            ty: IRType::I64,
        });
        let val = self.fresh_temp();
        self.emit(IRInstr::Load {
            dst: val.clone(),
            ptr: IRValue::Temp(ptr),
            ty: IRType::I64,
        });
        IRValue::Temp(val)
    }

    /// Choose between two already-computed values.
    ///
    /// Built from blocks and a Phi rather than arithmetic, because that is the
    /// shape every `if` expression in every program already compiles through —
    /// the arithmetic form would need an i8/i64 round trip, which is the exact
    /// neighbourhood of the open mixed-phi fault in Section 15.
    fn select(
        &mut self,
        cond: IRValue,
        if_true: IRValue,
        if_false: IRValue,
        ty: &IRType,
    ) -> IRValue {
        let t_label = self.fresh_label("res_sel_t");
        let f_label = self.fresh_label("res_sel_f");
        let m_label = self.fresh_label("res_sel_m");

        self.set_term(IRTerminator::BinBranch {
            cond,
            true_label: t_label.clone(),
            false_label: f_label.clone(),
        });

        let t_idx = self.new_block(t_label);
        self.switch_to(t_idx);
        self.set_term(IRTerminator::Jump(m_label.clone()));
        let t_end = self.blocks[self.current_block].label.clone();

        let f_idx = self.new_block(f_label);
        self.switch_to(f_idx);
        self.set_term(IRTerminator::Jump(m_label.clone()));
        let f_end = self.blocks[self.current_block].label.clone();

        let m_idx = self.new_block(m_label);
        self.switch_to(m_idx);

        let dst = self.fresh_temp();
        let incoming = sanitize_phi_incoming(
            vec![(if_true, t_end), (if_false, f_end)],
            ty,
        );
        self.emit(IRInstr::Phi {
            dst: dst.clone(),
            ty: ty.clone(),
            incoming,
        });
        IRValue::Temp(dst)
    }
}
