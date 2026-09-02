// ir/lower/lower_ctrl.rs — Control flow lowering: if/tif/match/pattern/bindings.

use super::IRLowerer;
use super::helpers::sanitize_phi_incoming;
use crate::ir::types::*;
use crate::semantic::{ManiType, TypedIfExpr, TypedMatchExpr, TypedTifExpr, TypedTresultExpr};

impl IRLowerer {
    pub(super) fn lower_if(&mut self, ie: &TypedIfExpr, result_ty: &ManiType) -> IRValue {
        let cond_val = self.lower_expr(&ie.cond);
        let ir_result_ty = IRType::from_mani(result_ty);

        let then_label = self.fresh_label("if_then");
        let else_label = self.fresh_label("if_else");
        let merge_label = self.fresh_label("if_merge");

        self.set_term(IRTerminator::BinBranch {
            cond: cond_val,
            true_label: then_label.clone(),
            false_label: else_label.clone(),
        });

        // Then block
        let then_idx = self.new_block(then_label);
        self.switch_to(then_idx);
        let then_val = self.lower_block(&ie.then_block);
        let then_end = self.current_block;
        self.set_term(IRTerminator::Jump(merge_label.clone()));

        // Elif + else chain
        let mut incoming: Vec<(IRValue, String)> = Vec::new();
        incoming.push((then_val, self.blocks[then_end].label.clone()));

        let else_idx = self.new_block(else_label);
        self.switch_to(else_idx);

        // Handle elif branches
        let mut current_else_idx = else_idx;
        for (i, (econd, eblock)) in ie.elif_branches.iter().enumerate() {
            let econd_val = self.lower_expr(econd);
            let elif_then_label = self.fresh_label("elif_then");
            let elif_else_label = self.fresh_label("elif_else");
            self.set_term(IRTerminator::BinBranch {
                cond: econd_val,
                true_label: elif_then_label.clone(),
                false_label: elif_else_label.clone(),
            });

            let elif_then_idx = self.new_block(elif_then_label);
            self.switch_to(elif_then_idx);
            let elif_val = self.lower_block(eblock);
            let elif_end = self.current_block;
            self.set_term(IRTerminator::Jump(merge_label.clone()));
            incoming.push((elif_val, self.blocks[elif_end].label.clone()));

            let next_else_idx = self.new_block(elif_else_label);
            self.switch_to(next_else_idx);
            current_else_idx = next_else_idx;
            let _ = i;
        }

        let final_else_val = if let Some(else_block) = &ie.else_block {
            let v = self.lower_block(else_block);
            let else_end = self.current_block;
            incoming.push((v.clone(), self.blocks[else_end].label.clone()));
            self.set_term(IRTerminator::Jump(merge_label.clone()));
            v
        } else {
            self.set_term(IRTerminator::Jump(merge_label.clone()));
            IRValue::Void
        };
        let _ = (current_else_idx, final_else_val);

        let merge_idx = self.new_block(merge_label);
        self.switch_to(merge_idx);

        // An if without else is a statement (semantic types it as void); a
        // merge PHI would be missing the fall-through edge and yield undef.
        if ir_result_ty == IRType::Void || ie.else_block.is_none() || incoming.is_empty() {
            IRValue::Void
        } else {
            let phi_dst = self.fresh_temp();
            let incoming = sanitize_phi_incoming(incoming, &ir_result_ty);
            self.emit(IRInstr::Phi {
                dst: phi_dst.clone(),
                ty: ir_result_ty,
                incoming,
            });
            IRValue::Temp(phi_dst)
        }
    }

    pub(super) fn lower_tif(&mut self, te: &TypedTifExpr, result_ty: &ManiType) -> IRValue {
        let cond_val = self.lower_expr(&te.cond);
        let ir_result_ty = IRType::from_mani(result_ty);

        let pos_label = self.fresh_label("tif_pos");
        let zero_label = self.fresh_label("tif_zero");
        let neg_label = self.fresh_label("tif_neg");
        let merge_label = self.fresh_label("tif_merge");

        self.set_term(IRTerminator::TritBranch {
            cond: cond_val,
            pos_label: pos_label.clone(),
            zero_label: zero_label.clone(),
            neg_label: neg_label.clone(),
        });

        let pos_idx = self.new_block(pos_label);
        self.switch_to(pos_idx);
        let pos_val = self.lower_block(&te.pos_block);
        let pos_end = self.current_block;
        let pos_end_label = self.blocks[pos_end].label.clone();
        self.set_term(IRTerminator::Jump(merge_label.clone()));

        let zero_idx = self.new_block(zero_label);
        self.switch_to(zero_idx);
        let zero_val = self.lower_block(&te.zero_block);
        let zero_end = self.current_block;
        let zero_end_label = self.blocks[zero_end].label.clone();
        self.set_term(IRTerminator::Jump(merge_label.clone()));

        let neg_idx = self.new_block(neg_label);
        self.switch_to(neg_idx);
        let neg_val = self.lower_block(&te.neg_block);
        let neg_end = self.current_block;
        let neg_end_label = self.blocks[neg_end].label.clone();
        self.set_term(IRTerminator::Jump(merge_label.clone()));

        let _ = (pos_idx, zero_idx, neg_idx);

        let merge_idx = self.new_block(merge_label);
        self.switch_to(merge_idx);

        if ir_result_ty == IRType::Void {
            IRValue::Void
        } else {
            let phi_dst = self.fresh_temp();
            let incoming = sanitize_phi_incoming(
                vec![
                    (pos_val, pos_end_label),
                    (zero_val, zero_end_label),
                    (neg_val, neg_end_label),
                ],
                &ir_result_ty,
            );
            self.emit(IRInstr::Phi {
                dst: phi_dst.clone(),
                ty: ir_result_ty,
                incoming,
            });
            IRValue::Temp(phi_dst)
        }
    }

    pub(super) fn lower_tresult(&mut self, tr: &TypedTresultExpr, result_ty: &ManiType) -> IRValue {
        // tresult branches on the ternary state of tr.expr:
        //   +1 (Ok)      → ok_block, binding ok_var to the expression value
        //   0  (Unknown) → unknown_block, binding unknown_var to the expression value
        //   -1 (Err)     → err_block, binding err_var to the expression value
        let expr_val = self.lower_expr(&tr.expr);
        let ir_result_ty = IRType::from_mani(result_ty);

        let ok_label      = self.fresh_label("tresult_ok");
        let unknown_label = self.fresh_label("tresult_unknown");
        let err_label     = self.fresh_label("tresult_err");
        let merge_label   = self.fresh_label("tresult_merge");

        self.set_term(IRTerminator::TritBranch {
            cond: expr_val.clone(),
            pos_label:  ok_label.clone(),
            zero_label: unknown_label.clone(),
            neg_label:  err_label.clone(),
        });

        // Ok arm — bind ok_var to expr_val
        let ok_idx = self.new_block(ok_label);
        self.switch_to(ok_idx);
        if tr.ok_var != "_" {
            let slot = self.fresh_temp();
            self.emit(IRInstr::Alloca { dst: slot.clone(), ty: IRType::I64 });
            self.emit(IRInstr::Store { ptr: IRValue::Temp(slot.clone()), val: expr_val.clone(), ty: IRType::I64 });
            self.locals.insert(tr.ok_var.clone(), (slot, IRType::I64));
        }
        let ok_val = self.lower_block(&tr.ok_block);
        let ok_end_label = self.blocks[self.current_block].label.clone();
        self.set_term(IRTerminator::Jump(merge_label.clone()));

        // Unknown arm — bind unknown_var to expr_val
        let unk_idx = self.new_block(unknown_label);
        self.switch_to(unk_idx);
        if tr.unknown_var != "_" {
            let slot = self.fresh_temp();
            self.emit(IRInstr::Alloca { dst: slot.clone(), ty: IRType::I64 });
            self.emit(IRInstr::Store { ptr: IRValue::Temp(slot.clone()), val: expr_val.clone(), ty: IRType::I64 });
            self.locals.insert(tr.unknown_var.clone(), (slot, IRType::I64));
        }
        let unk_val = self.lower_block(&tr.unknown_block);
        let unk_end_label = self.blocks[self.current_block].label.clone();
        self.set_term(IRTerminator::Jump(merge_label.clone()));

        // Err arm — bind err_var to expr_val
        let err_idx = self.new_block(err_label);
        self.switch_to(err_idx);
        if tr.err_var != "_" {
            let slot = self.fresh_temp();
            self.emit(IRInstr::Alloca { dst: slot.clone(), ty: IRType::I64 });
            self.emit(IRInstr::Store { ptr: IRValue::Temp(slot.clone()), val: expr_val.clone(), ty: IRType::I64 });
            self.locals.insert(tr.err_var.clone(), (slot, IRType::I64));
        }
        let err_val = self.lower_block(&tr.err_block);
        let err_end_label = self.blocks[self.current_block].label.clone();
        self.set_term(IRTerminator::Jump(merge_label.clone()));

        let _ = (ok_idx, unk_idx, err_idx);

        let merge_idx = self.new_block(merge_label);
        self.switch_to(merge_idx);

        if ir_result_ty == IRType::Void {
            IRValue::Void
        } else {
            let phi_dst = self.fresh_temp();
            let incoming = sanitize_phi_incoming(
                vec![
                    (ok_val, ok_end_label),
                    (unk_val, unk_end_label),
                    (err_val, err_end_label),
                ],
                &ir_result_ty,
            );
            self.emit(IRInstr::Phi {
                dst: phi_dst.clone(),
                ty: ir_result_ty,
                incoming,
            });
            IRValue::Temp(phi_dst)
        }
    }

    /// C6: is this `match` a three-way decision on ONE trit?
    ///
    /// Returns the trit position and the arm indices in `[neg, zero, pos]`
    /// order. The shape is three unguarded arms, each a trit pattern that
    /// fixes the SAME single position, to `-`, `0` and `+` between them, and
    /// constrains nothing else — which is what `sole_fixed_position` asks.
    ///
    /// The leading `*` those patterns must carry is not a formality: without
    /// it each arm ALSO requires every trit above the pattern to be zero, so
    /// the three would not cover a word and a three-way branch would be
    /// wrong. `check_exhaustiveness` accepts exactly this shape, and both
    /// call the same predicate so the two cannot drift apart (P90).
    fn trit_trichotomy(me: &TypedMatchExpr) -> Option<(usize, [usize; 3])> {
        if me.arms.len() != 3 {
            return None;
        }
        let mut at: Option<usize> = None;
        let mut slot: [Option<usize>; 3] = [None, None, None];
        for (i, arm) in me.arms.iter().enumerate() {
            if arm.guard.is_some() {
                return None;
            }
            let tp = match &arm.pattern {
                crate::ast::Pattern::Trit(tp, _) => tp,
                _ => return None,
            };
            let (p, v) = tp.sole_fixed_position()?;
            match at {
                None => at = Some(p),
                Some(q) if q == p => {}
                _ => return None,
            }
            let s = (v + 1) as usize;
            if slot[s].is_some() {
                return None;
            }
            slot[s] = Some(i);
        }
        Some((at?, [slot[0]?, slot[1]?, slot[2]?]))
    }

    /// C6: lower a one-trit three-way match to a `TritBranch`.
    ///
    /// This is the whole point of the feature. Compiled as an ordinary chain
    /// the same program is three equality tests — on T3 each is eight
    /// instructions (`TSHI TSUB TCMP TNEG TMAX TLIT TSUB TBRANCH`) because an
    /// equality has to be reduced to a boolean before a two-way branch can use
    /// it. The machine does not need any of that: the extracted trit is
    /// already in {-1, 0, +1}, which is exactly what `TBRANCH` consumes, so
    /// one instruction per level replaces binary's two.
    fn lower_trit_trichotomy(
        &mut self,
        me: &TypedMatchExpr,
        result_ty: &ManiType,
        at: usize,
        order: [usize; 3],
    ) -> IRValue {
        let x = self.lower_expr(&me.scrutinee);
        // The trit itself. On LLVM `TritBranch` tests the SIGN, which for a
        // value already in {-1, 0, +1} is the value.
        //
        // On a `trit` scrutinee at position 0 the extraction is the IDENTITY —
        // the value already IS its own low trit — so it is skipped, and the
        // trichotomy then costs exactly what `tif` costs: measured, 54,016
        // dynamic instructions against `tif`'s 54,016 over the same 3,000
        // iterations, where extracting cost 69,016.
        //
        // The assumption is that a `trit` holds a value in {-1, 0, +1}, and it
        // is no stronger than the one `tif` already makes: `tif` branches on
        // the SIGN of its operand, so a `trit` holding 2 would mis-branch there
        // too. Any wider type keeps the extraction, because trit 0 of 2 is `-1`
        // while its sign is `+1` — the two are not the same question.
        let t = if at == 0 && matches!(me.scrutinee.ty, ManiType::Trit) {
            x.clone()
        } else {
            self.emit_trit_slice(&x, at, 1)
        };

        let ir_result_ty = IRType::from_mani(result_ty);
        let merge_label = self.fresh_label("tmatch_merge");
        let labels: Vec<String> =
            (0..3).map(|_| self.fresh_label("tmatch_arm")).collect();

        self.set_term(IRTerminator::TritBranch {
            cond: t,
            pos_label: labels[2].clone(),
            zero_label: labels[1].clone(),
            neg_label: labels[0].clone(),
        });

        let mut incoming = Vec::new();
        for (k, arm_idx) in order.iter().enumerate() {
            let idx = self.new_block(labels[k].clone());
            self.switch_to(idx);
            let arm = &me.arms[*arm_idx];
            let bind_val = x.clone();
            self.bind_pattern_locals(&arm.pattern, &bind_val);
            let val = self.lower_expr(&arm.body);
            let end = self.current_block;
            incoming.push((val, self.blocks[end].label.clone()));
            self.set_term(IRTerminator::Jump(merge_label.clone()));
        }

        let merge_idx = self.new_block(merge_label);
        self.switch_to(merge_idx);
        if ir_result_ty == IRType::Void || incoming.is_empty() {
            IRValue::Void
        } else {
            let phi_dst = self.fresh_temp();
            let incoming = sanitize_phi_incoming(incoming, &ir_result_ty);
            self.emit(IRInstr::Phi {
                dst: phi_dst.clone(),
                ty: ir_result_ty,
                incoming,
            });
            IRValue::Temp(phi_dst)
        }
    }

    pub(super) fn lower_match(&mut self, me: &TypedMatchExpr, result_ty: &ManiType) -> IRValue {
        // C6: a radix-3 decision compiles to a radix-3 branch.
        if let Some((at, order)) = Self::trit_trichotomy(me) {
            return self.lower_trit_trichotomy(me, result_ty, at, order);
        }
        let raw_scrutinee = self.lower_expr(&me.scrutinee);
        // String scrutinees are compared with the str_eq runtime call, which
        // (on the T3 backend) clobbers the scrutinee's register. Keep the
        // scrutinee in a stack slot and reload a fresh temp wherever an arm
        // needs it, so every compare sees the original pointer.
        let scrutinee_slot = if matches!(&me.scrutinee.ty, ManiType::Str) {
            let slot = self.fresh_temp();
            let sty = IRType::Ptr(Box::new(IRType::I8));
            self.emit(IRInstr::Alloca { dst: slot.clone(), ty: sty.clone() });
            self.emit(IRInstr::Store {
                ptr: IRValue::Temp(slot.clone()),
                val: raw_scrutinee.clone(),
                ty: sty,
            });
            Some(slot)
        } else {
            None
        };
        let ir_result_ty = IRType::from_mani(result_ty);
        let merge_label = self.fresh_label("match_merge");
        let mut incoming = Vec::new();

        for arm in &me.arms {
            let arm_label = self.fresh_label("match_arm");
            let next_label = self.fresh_label("match_next");

            // Emit comparison for pattern
            let scrutinee_val = self.reload_scrutinee(&scrutinee_slot, &raw_scrutinee);
            let matches_cond = self.lower_pattern_match(&arm.pattern, &scrutinee_val, &me.scrutinee.ty);

            // If there is a guard we need pattern bindings to be in scope BEFORE
            // evaluating the guard expression. Emit bindings now, then evaluate
            // the guard, then branch.
            let final_cond = if let Some(guard) = &arm.guard {
                // Jump from pattern-check block into a "guard-eval" block on match,
                // so that bindings are available for the guard expression.
                let guard_label = self.fresh_label("match_guard");
                self.set_term(IRTerminator::BinBranch {
                    cond: matches_cond,
                    true_label: guard_label.clone(),
                    false_label: next_label.clone(),
                });
                let guard_idx = self.new_block(guard_label);
                self.switch_to(guard_idx);
                // Bind pattern variables so the guard can reference them.
                let bind_val = self.reload_scrutinee(&scrutinee_slot, &raw_scrutinee);
                self.bind_pattern_locals(&arm.pattern, &bind_val);
                // Now lower the guard expression.
                self.lower_expr(guard)
            } else {
                matches_cond
            };

            self.set_term(IRTerminator::BinBranch {
                cond: final_cond,
                true_label: arm_label.clone(),
                false_label: next_label.clone(),
            });

            let arm_idx = self.new_block(arm_label);
            self.switch_to(arm_idx);
            // For non-guard arms, bind pattern variables here.
            // For guard arms, variables were already bound in the guard-eval block,
            // but we re-bind to ensure they're in the locals map for the arm body.
            let bind_val = self.reload_scrutinee(&scrutinee_slot, &raw_scrutinee);
            self.bind_pattern_locals(&arm.pattern, &bind_val);
            let arm_val = self.lower_expr(&arm.body);
            let arm_end = self.current_block;
            incoming.push((arm_val, self.blocks[arm_end].label.clone()));
            self.set_term(IRTerminator::Jump(merge_label.clone()));

            let next_idx = self.new_block(next_label);
            self.switch_to(next_idx);
        }

        // Fallthrough (no pattern matched). A void match just continues at the
        // merge block. A value-producing match must not reach the merge PHI
        // through this edge (it has no incoming value and would yield undef):
        // it is provably dead when the match is exhaustive, and traps
        // (Unreachable) otherwise.
        if ir_result_ty == IRType::Void || incoming.is_empty() {
            self.set_term(IRTerminator::Jump(merge_label.clone()));
        } else {
            self.set_term(IRTerminator::Unreachable);
        }

        let merge_idx = self.new_block(merge_label);
        self.switch_to(merge_idx);

        if ir_result_ty == IRType::Void || incoming.is_empty() {
            IRValue::Void
        } else {
            let phi_dst = self.fresh_temp();
            let incoming = sanitize_phi_incoming(incoming, &ir_result_ty);
            self.emit(IRInstr::Phi {
                dst: phi_dst.clone(),
                ty: ir_result_ty,
                incoming,
            });
            IRValue::Temp(phi_dst)
        }
    }

    /// Fetch the match scrutinee for use at the current emission point.
    /// If it was spilled to a stack slot (string scrutinees), load a fresh
    /// temp; otherwise reuse the original value.
    fn reload_scrutinee(&mut self, slot: &Option<IRTemp>, raw: &IRValue) -> IRValue {
        if let Some(slot) = slot {
            let t = self.fresh_temp();
            self.emit(IRInstr::Load {
                dst: t.clone(),
                ptr: IRValue::Temp(slot.clone()),
                ty: IRType::Ptr(Box::new(IRType::I8)),
            });
            IRValue::Temp(t)
        } else {
            raw.clone()
        }
    }

    /// C6: `shr(x, k)` — the value of the trits at position `k` and above,
    /// with trit `k` becoming the units place.
    ///
    /// This is round-to-nearest division by 3^k and NOT truncating division.
    /// The low `k` trits of a balanced-ternary word have magnitude at most
    /// `(3^k - 1) / 2`, strictly inside half a step, so dropping them *is*
    /// rounding to nearest and no tie can arise. `strength_reduce` then turns
    /// it into a single `TSHR` on T3 (P22); on LLVM it stays the branchless
    /// `DivNear` sequence, so the two backends agree by construction rather
    /// than by a second lowering.
    fn emit_trit_shr(&mut self, x: &IRValue, k: usize) -> IRValue {
        if k == 0 {
            return x.clone();
        }
        let divisor = 3i64.pow(k as u32);
        let dst = self.fresh_temp();
        self.emit(IRInstr::BinOp {
            dst: dst.clone(),
            op: IRBinOp::DivNear,
            lhs: x.clone(),
            rhs: IRValue::Const(IRConst::Int(divisor)),
            ty: IRType::I64,
        });
        IRValue::Temp(dst)
    }

    /// C6: the value of the trits `[lo, lo + len)`, read as a balanced-ternary
    /// number in their own right — `shr(x, lo) - 3^len * shr(x, lo + len)`.
    fn emit_trit_slice(&mut self, x: &IRValue, lo: usize, len: usize) -> IRValue {
        let low = self.emit_trit_shr(x, lo);
        let high = self.emit_trit_shr(x, lo + len);
        let scaled = self.fresh_temp();
        self.emit(IRInstr::BinOp {
            dst: scaled.clone(),
            op: IRBinOp::Mul,
            lhs: high,
            rhs: IRValue::Const(IRConst::Int(3i64.pow(len as u32))),
            ty: IRType::I64,
        });
        let dst = self.fresh_temp();
        self.emit(IRInstr::BinOp {
            dst: dst.clone(),
            op: IRBinOp::Sub,
            lhs: low,
            rhs: IRValue::Temp(scaled),
            ty: IRType::I64,
        });
        IRValue::Temp(dst)
    }

    /// C6: `<value> == <k>` as a `Bool`.
    fn emit_trit_eq(&mut self, value: IRValue, k: i64) -> IRValue {
        let dst = self.fresh_temp();
        self.emit(IRInstr::BinOp {
            dst: dst.clone(),
            op: IRBinOp::IEq,
            lhs: value,
            rhs: IRValue::Const(IRConst::Int(k)),
            ty: IRType::Bool,
        });
        IRValue::Temp(dst)
    }

    /// C6: the runtime test for a trit pattern.
    ///
    /// Every condition is a comparison against a constant, and there are at
    /// most `runs + 1` of them. The `+ 1` is the requirement that the trits
    /// ABOVE the pattern are zero, which is what makes a wildcard-free trit
    /// pattern mean exactly the literal it spells.
    ///
    /// One collapse is worth the four lines it costs, because it is the
    /// common case: when the pattern's topmost element is fixed and there is
    /// no `*`, the top run's test and the zero test are the SAME test.
    /// `shr(x, 3) == 4` already says "trits 3 and 4 are `+ +` and everything
    /// above is zero", because a balanced-ternary representation is unique —
    /// 4 is `++` and nothing else. So `0t++???` is one comparison, not three.
    fn lower_trit_pattern(&mut self, tp: &crate::ast::TritPat, x: &IRValue) -> IRValue {
        let w = tp.width();
        let runs = tp.fixed_runs();
        let mut conds: Vec<IRValue> = Vec::new();

        // Does the topmost fixed run reach the top of the pattern?
        let top_absorbed = !tp.open
            && matches!(runs.last(), Some(&(_, hi, _)) if hi == w);

        if !tp.open && !top_absorbed {
            let high = self.emit_trit_shr(x, w);
            conds.push(self.emit_trit_eq(high, 0));
        }

        for (i, &(lo, hi, value)) in runs.iter().enumerate() {
            let last = i + 1 == runs.len();
            let v = if top_absorbed && last {
                // The collapse: compare the trits from `lo` upward against
                // the run's value, which also forces everything above to zero.
                self.emit_trit_shr(x, lo)
            } else {
                self.emit_trit_slice(x, lo, hi - lo)
            };
            conds.push(self.emit_trit_eq(v, value));
        }

        let mut acc = match conds.pop() {
            None => return IRValue::Const(IRConst::Bool(true)),
            Some(c) => c,
        };
        while let Some(c) = conds.pop() {
            let dst = self.fresh_temp();
            self.emit(IRInstr::BinOp {
                dst: dst.clone(),
                op: IRBinOp::And,
                lhs: acc,
                rhs: c,
                ty: IRType::Bool,
            });
            acc = IRValue::Temp(dst);
        }
        acc
    }

    /// C6: the value each capture of a trit pattern binds, in the order
    /// `TritPat::bound_names` reports them, so the two cannot disagree.
    fn trit_capture_values(
        &mut self,
        tp: &crate::ast::TritPat,
        x: &IRValue,
    ) -> Vec<(String, IRValue)> {
        let mut out = Vec::new();
        if let Some(n) = &tp.open_capture {
            let v = self.emit_trit_shr(x, tp.width());
            out.push((n.clone(), v));
        }
        for (n, lo, len) in tp.captures.iter().rev() {
            let v = self.emit_trit_slice(x, *lo, *len);
            out.push((n.clone(), v));
        }
        out
    }

    pub(super) fn lower_pattern_match(
        &mut self,
        pattern: &crate::ast::Pattern,
        scrutinee: &IRValue,
        _scrutinee_ty: &ManiType,
    ) -> IRValue {
        match pattern {
            crate::ast::Pattern::Wildcard(_) => {
                // Always matches
                IRValue::Const(IRConst::Bool(true))
            }
            crate::ast::Pattern::Lit(lit, _) => {
                let lit_val = self.lower_lit(lit);
                // String literals need runtime string equality (pointer compare
                // never matches runtime-built strings), floats need FEq
                // (bitwise i64 compare mishandles -0.0 vs 0.0). This mirrors
                // how `==` is lowered for BinOp (see binop_to_ir).
                let op = match lit {
                    crate::ast::Lit::Str(_) => IRBinOp::StrEq,
                    crate::ast::Lit::Float(_) => IRBinOp::FEq,
                    _ => IRBinOp::IEq,
                };
                let dst = self.fresh_temp();
                self.emit(IRInstr::BinOp {
                    dst: dst.clone(),
                    op,
                    lhs: scrutinee.clone(),
                    rhs: lit_val,
                    ty: IRType::Bool,
                });
                IRValue::Temp(dst)
            }
            crate::ast::Pattern::Ident(_name, _) => {
                // Binding pattern — always matches, bind the variable
                IRValue::Const(IRConst::Bool(true))
            }
            // C6
            crate::ast::Pattern::Trit(tp, _) => {
                let x = scrutinee.clone();
                self.lower_trit_pattern(tp, &x)
            }
            crate::ast::Pattern::Enum(variant, enum_name, fields, _) => {
                // Parser produces Pattern::Enum(variant_name, Some(enum_type_name), ...) for
                // "EnumType::VariantName" patterns. e.g. "Direction::North" →
                // Pattern::Enum("North", Some("Direction"), [], _).
                // For plain constructors like "Ok(v)" (no path), enum_name is None.
                //
                // Determine the enum type name and the variant name:
                let (enum_type_name, variant_name): (&str, &str) =
                    if let Some(etype) = enum_name.as_deref() {
                        (etype, variant.as_str())
                    } else {
                        // No path prefix — variant name IS all we have (e.g. "Ok", "North")
                        (variant.as_str(), variant.as_str())
                    };

                // Check if this is a known custom enum
                if self.enum_variants.contains_key(enum_type_name) {
                    if let Some((idx, _)) = self.enum_variant_info(enum_type_name, variant_name) {
                        // P43: a BOXED enum's scrutinee is the cell's address,
                        // so the tag has to be LOADED. This arm used to compare
                        // the scrutinee itself against the index in every case —
                        // which is right for a bare-integer enum and cannot be
                        // right for one whose payload the binder below reads
                        // through the very same value as a pointer. The two
                        // halves of one `match` disagreed about what they were
                        // looking at.
                        let lhs = if self.enum_is_boxed(enum_type_name) {
                            let tag_t = self.fresh_temp();
                            self.emit(IRInstr::Load {
                                dst: tag_t.clone(),
                                ptr: scrutinee.clone(),
                                ty: IRType::I64,
                            });
                            IRValue::Temp(tag_t)
                        } else {
                            scrutinee.clone()
                        };
                        let cmp_t = self.fresh_temp();
                        self.emit(IRInstr::BinOp {
                            dst: cmp_t.clone(),
                            op: IRBinOp::IEq,
                            lhs,
                            rhs: IRValue::Const(IRConst::Int(idx as i64)),
                            ty: IRType::Bool,
                        });
                        return self.and_payload_tests(
                            IRValue::Temp(cmp_t),
                            fields,
                            scrutinee,
                            _scrutinee_ty,
                        );
                    }
                    // Variant not found in this enum — wildcard (always match)
                    return IRValue::Const(IRConst::Bool(true));
                }

                // Result<T,E> patterns: scrutinee is a pointer to [tag, value].
                // Load the tag word and compare to the expected trit value.
                let is_unknown = enum_name.as_deref() == Some("Unknown");
                let expected_tag: i64 = if is_unknown {
                    0
                } else {
                    match variant.as_str() {
                        "Ok"      =>  1,
                        "Unknown" =>  0,
                        "Err"     => -1,
                        _ => return IRValue::Const(IRConst::Bool(true)), // unknown variant
                    }
                };
                let tag_t = self.fresh_temp();
                self.emit(IRInstr::Load {
                    dst: tag_t.clone(),
                    ptr: scrutinee.clone(),
                    ty: IRType::I64,
                });
                let cmp_t = self.fresh_temp();
                self.emit(IRInstr::BinOp {
                    dst: cmp_t.clone(),
                    op: IRBinOp::IEq,
                    lhs: IRValue::Temp(tag_t),
                    rhs: IRValue::Const(IRConst::Int(expected_tag)),
                    ty: IRType::Bool,
                });
                self.and_payload_tests(IRValue::Temp(cmp_t), fields, scrutinee, _scrutinee_ty)
            }
            crate::ast::Pattern::Tuple(elems, _) => {
                // P90. A tuple has no tag: the scrutinee points straight at
                // `[e0, e1, ...]`, one word per element, which is the layout
                // `TypedExprKind::Tuple` builds. So the test is the AND of the
                // element tests, with no guard needed — every element is
                // present whatever the value is.
                let mut result: IRValue = IRValue::Const(IRConst::Bool(true));
                for (i, sub_pat) in elems.iter().enumerate() {
                    if sub_pat.is_irrefutable() {
                        continue;
                    }
                    let elem_val = self.load_slot(scrutinee, i as i64);
                    let sub_result =
                        self.lower_pattern_match(sub_pat, &elem_val, _scrutinee_ty);
                    let and_t = self.fresh_temp();
                    self.emit(IRInstr::BinOp {
                        dst: and_t.clone(),
                        op: IRBinOp::And,
                        lhs: result,
                        rhs: sub_result,
                        ty: IRType::Bool,
                    });
                    result = IRValue::Temp(and_t);
                }
                result
            }
            crate::ast::Pattern::Or(alts, _) => {
                let mut result = IRValue::Const(IRConst::Bool(false));
                for alt in alts {
                    let alt_match = self.lower_pattern_match(alt, scrutinee, _scrutinee_ty);
                    let dst = self.fresh_temp();
                    self.emit(IRInstr::BinOp {
                        dst: dst.clone(),
                        op: IRBinOp::Or,
                        lhs: result,
                        rhs: alt_match,
                        ty: IRType::Bool,
                    });
                    result = IRValue::Temp(dst);
                }
                result
            }
            crate::ast::Pattern::Struct(struct_name, field_pats, _) => {
                let field_defs: Vec<String> = self.structs.get(struct_name)
                    .map(|fs| fs.iter().map(|(n, _)| n.clone()).collect())
                    .unwrap_or_default();

                let mut result: IRValue = IRValue::Const(IRConst::Bool(true));
                for (field_name, sub_pat) in field_pats {
                    // P90: one shared predicate rather than a local `matches!`.
                    // The local one named Ident and Wildcard only, so a nested
                    // `(a, b)` or `a | b` sub-pattern was tested when it need
                    // not be — and, worse, the checker used a DIFFERENT rule.
                    if sub_pat.is_irrefutable() {
                        continue;
                    }

                    let idx = field_defs.iter().position(|n| n == field_name)
                        .unwrap_or(0) as i64;

                    let field_val = self.load_slot(scrutinee, idx);

                    let sub_result = self.lower_pattern_match(
                        sub_pat,
                        &field_val,
                        _scrutinee_ty,
                    );

                    let and_t = self.fresh_temp();
                    self.emit(IRInstr::BinOp {
                        dst: and_t.clone(),
                        op: IRBinOp::And,
                        lhs: result,
                        rhs: sub_result,
                        ty: IRType::Bool,
                    });
                    result = IRValue::Temp(and_t);
                }
                result
            }
            // NO CATCH-ALL. Every `Pattern` variant is handled above, and the
            // `_ => true` that used to sit here is precisely what made P90
            // silent: an unhandled pattern did not fail to compile, it
            // compiled into "matches everything". A new variant must now be
            // given a test deliberately, and rustc names the site.
        }
    }

    /// Load word `idx` of an aggregate the scrutinee points at.
    ///
    /// The uniform one-word slot convention every aggregate in this lowerer
    /// uses: tuples index from 0, a boxed enum's payload from 1 (word 0 is the
    /// tag), a struct by declaration order.
    fn load_slot(&mut self, base: &IRValue, idx: i64) -> IRValue {
        let ptr_t = self.fresh_temp();
        self.emit(IRInstr::GetPtr {
            dst: ptr_t.clone(),
            ptr: base.clone(),
            idx: IRValue::Const(IRConst::Int(idx)),
            ty: IRType::I64,
        });
        let val_t = self.fresh_temp();
        self.emit(IRInstr::Load {
            dst: val_t.clone(),
            ptr: IRValue::Temp(ptr_t),
            ty: IRType::I64,
        });
        IRValue::Temp(val_t)
    }

    /// AND the payload sub-pattern tests onto an enum arm's tag test (P90).
    ///
    /// **The payload tests are GUARDED BY THE TAG, and that is the whole
    /// design.** Testing them eagerly and ANDing the results is one line
    /// shorter and it segfaults: matching `Err("closed")` against an `Ok(5)`
    /// would load word 1 of the cell — the integer 5 — and hand it to
    /// `StrEq`, which dereferences it as a `char*`. The tag says the payload
    /// is not a string; the payload test must not run before the tag is
    /// known. `cross_variant` in the tests is exactly that program, and it is
    /// the row that separates this design from the eager one.
    ///
    /// Built from `BinBranch`, `Jump` and `Phi` — no new IR instruction, no
    /// new terminator and no new kind of CFG edge, so no pass needed auditing
    /// (P72's hazard, avoided by construction rather than survived).
    fn and_payload_tests(
        &mut self,
        tag_ok: IRValue,
        fields: &[crate::ast::Pattern],
        scrutinee: &IRValue,
        scrutinee_ty: &ManiType,
    ) -> IRValue {
        if fields.iter().all(|f| f.is_irrefutable()) {
            // `Ok(v)`, `Err(_)`, `North` — the tag test IS the answer, and the
            // emitted code is what it was before this change, instruction for
            // instruction.
            return tag_ok;
        }

        let payload_label = self.fresh_label("pat_payload");
        let join_label = self.fresh_label("pat_join");
        let tag_block = self.blocks[self.current_block].label.clone();
        self.set_term(IRTerminator::BinBranch {
            cond: tag_ok,
            true_label: payload_label.clone(),
            false_label: join_label.clone(),
        });

        let payload_idx = self.new_block(payload_label);
        self.switch_to(payload_idx);
        let mut acc: IRValue = IRValue::Const(IRConst::Bool(true));
        for (fi, field) in fields.iter().enumerate() {
            if field.is_irrefutable() {
                continue;
            }
            // Payload word *i* is at slot 1+i — the SAME arithmetic
            // `bind_pattern_locals` uses. P43 is the record of what it costs
            // when the test and the binder disagree about where a field is.
            let field_val = self.load_slot(scrutinee, fi as i64 + 1);
            let sub = self.lower_pattern_match(field, &field_val, scrutinee_ty);
            let and_t = self.fresh_temp();
            self.emit(IRInstr::BinOp {
                dst: and_t.clone(),
                op: IRBinOp::And,
                lhs: acc,
                rhs: sub,
                ty: IRType::Bool,
            });
            acc = IRValue::Temp(and_t);
        }
        // Read the label AFTER the loop: a nested enum sub-pattern runs this
        // same function and leaves the cursor in its own join block, so the
        // phi's incoming edge is not the block this one started in.
        let payload_end = self.blocks[self.current_block].label.clone();
        self.set_term(IRTerminator::Jump(join_label.clone()));

        let join_idx = self.new_block(join_label);
        self.switch_to(join_idx);
        let dst = self.fresh_temp();
        self.emit(IRInstr::Phi {
            dst: dst.clone(),
            ty: IRType::Bool,
            incoming: vec![
                (IRValue::Const(IRConst::Bool(false)), tag_block),
                (acc, payload_end),
            ],
        });
        IRValue::Temp(dst)
    }

    /// Inject pattern-bound variables as locals aliased to `scrutinee`.
    pub(super) fn bind_pattern_locals(
        &mut self,
        pattern: &crate::ast::Pattern,
        scrutinee: &IRValue,
    ) {
        use crate::ast::Pattern;
        match pattern {
            Pattern::Ident(name, _) => {
                let ty = IRType::I64;
                let alloca = self.fresh_temp();
                self.emit(IRInstr::Alloca { dst: alloca.clone(), ty: ty.clone() });
                self.emit(IRInstr::Store {
                    ptr: IRValue::Temp(alloca.clone()),
                    val: scrutinee.clone(),
                    ty: ty.clone(),
                });
                self.locals.insert(name.clone(), (alloca, ty));
            }
            // C6: each capture is a fresh `int` computed from the scrutinee.
            Pattern::Trit(tp, _) => {
                let x = scrutinee.clone();
                for (name, val) in self.trit_capture_values(tp, &x) {
                    let ty = IRType::I64;
                    let alloca = self.fresh_temp();
                    self.emit(IRInstr::Alloca { dst: alloca.clone(), ty: ty.clone() });
                    self.emit(IRInstr::Store {
                        ptr: IRValue::Temp(alloca.clone()),
                        val,
                        ty: ty.clone(),
                    });
                    self.locals.insert(name, (alloca, ty));
                }
            }
            Pattern::Enum(_, _, fields, _) => {
                // scrutinee is a pointer to [tag, field0, field1, …].
                //
                // P43: field *i* lives at word 1+i. Every field used to be read
                // from word 1, so `Rect(w, h) => w * h` bound BOTH names to the
                // first payload word and squared it. Invisible because nothing
                // could construct a `Rect` to begin with.
                for (fi, field) in fields.iter().enumerate() {
                    if let Pattern::Ident(name, _) = field {
                        let val_ptr_t = self.fresh_temp();
                        self.emit(IRInstr::GetPtr {
                            dst: val_ptr_t.clone(),
                            ptr: scrutinee.clone(),
                            idx: IRValue::Const(IRConst::Int(fi as i64 + 1)),
                            ty: IRType::I64,
                        });
                        let val_t = self.fresh_temp();
                        self.emit(IRInstr::Load {
                            dst: val_t.clone(),
                            ptr: IRValue::Temp(val_ptr_t),
                            ty: IRType::I64,
                        });
                        let bound_alloca = self.fresh_temp();
                        self.emit(IRInstr::Alloca {
                            dst: bound_alloca.clone(),
                            ty: IRType::I64,
                        });
                        self.emit(IRInstr::Store {
                            ptr: IRValue::Temp(bound_alloca.clone()),
                            val: IRValue::Temp(val_t),
                            ty: IRType::I64,
                        });
                        self.locals.insert(name.clone(), (bound_alloca, IRType::I64));
                    } else {
                        // P90: hand DOWN the payload word, not the cell. This
                        // branch passed `scrutinee` — the enum cell — so a
                        // nested `Ok((a, b))` or `Ok(Point { x, y })` read its
                        // parts from `[tag, payload]` and bound the TAG. It
                        // was unreachable while no nested pattern bound
                        // anything at all (there was no Tuple arm, and a
                        // Struct payload is not constructible), so the two
                        // defects hid each other.
                        let payload = self.load_slot(scrutinee, fi as i64 + 1);
                        self.bind_pattern_locals(field, &payload);
                    }
                }
            }
            Pattern::Tuple(elems, _) => {
                // P90's second half. There was no arm here at all, so
                // `Ok((a, b))` bound NEITHER name: `a` and `b` resolved to
                // globals and the failure surfaced past codegen -- as
                // `Cannot resolve: a` from the T3 assembler and `global
                // variable reference must have pointer type` from clang.
                // `manitc check` said OK, because the analyzer's
                // `define_pattern_bindings` has handled tuples all along. This
                // arm restores agreement between the two rather than adding a
                // capability.
                for (i, sub_pat) in elems.iter().enumerate() {
                    let elem_val = self.load_slot(scrutinee, i as i64);
                    self.bind_pattern_locals(sub_pat, &elem_val);
                }
            }
            Pattern::Struct(struct_name, field_pats, _) => {
                let field_defs: Vec<String> = self.structs.get(struct_name)
                    .map(|fs| fs.iter().map(|(n, _)| n.clone()).collect())
                    .unwrap_or_default();
                for (field_name, sub_pat) in field_pats {
                    if let Pattern::Ident(var_name, _) = sub_pat {
                        let idx = field_defs.iter().position(|n| n == field_name)
                            .unwrap_or(0) as i64;
                        let ptr_t = self.fresh_temp();
                        self.emit(IRInstr::GetPtr {
                            dst: ptr_t.clone(),
                            ptr: scrutinee.clone(),
                            idx: IRValue::Const(IRConst::Int(idx)),
                            ty: IRType::I64,
                        });
                        let val_t = self.fresh_temp();
                        self.emit(IRInstr::Load {
                            dst: val_t.clone(),
                            ptr: IRValue::Temp(ptr_t),
                            ty: IRType::I64,
                        });
                        let alloca = self.fresh_temp();
                        self.emit(IRInstr::Alloca { dst: alloca.clone(), ty: IRType::I64 });
                        self.emit(IRInstr::Store {
                            ptr: IRValue::Temp(alloca.clone()),
                            val: IRValue::Temp(val_t),
                            ty: IRType::I64,
                        });
                        self.locals.insert(var_name.clone(), (alloca, IRType::I64));
                    } else {
                        self.bind_pattern_locals(sub_pat, scrutinee);
                    }
                }
            }
            Pattern::Or(alts, _) => {
                if alts.len() == 1 {
                    if let Some(first) = alts.first() {
                        self.bind_pattern_locals(first, scrutinee);
                    }
                    return;
                }
                // Each alternative may bind the same name from a different
                // extraction position, so test the alternatives in order and
                // bind from the first one that matches. All alternatives share
                // one alloca slot per bound name.
                let mut names: Vec<String> = Vec::new();
                for alt in alts {
                    for n in Self::pattern_bound_names(alt) {
                        if !names.contains(&n) {
                            names.push(n);
                        }
                    }
                }
                if names.is_empty() {
                    return;
                }
                let mut slots: std::collections::HashMap<String, IRTemp> =
                    std::collections::HashMap::new();
                for name in &names {
                    let alloca = self.fresh_temp();
                    self.emit(IRInstr::Alloca { dst: alloca.clone(), ty: IRType::I64 });
                    self.locals.insert(name.clone(), (alloca.clone(), IRType::I64));
                    slots.insert(name.clone(), alloca);
                }
                let end_label = self.fresh_label("orbind_end");
                for (i, alt) in alts.iter().enumerate() {
                    if i + 1 == alts.len() {
                        // Last alternative: we are only here because the whole
                        // or-pattern matched, so bind unconditionally.
                        self.bind_pattern_into(alt, scrutinee, &slots);
                        self.set_term(IRTerminator::Jump(end_label.clone()));
                    } else {
                        let cond =
                            self.lower_pattern_match(alt, scrutinee, &ManiType::Unknown);
                        let bind_label = self.fresh_label("orbind_alt");
                        let next_label = self.fresh_label("orbind_next");
                        self.set_term(IRTerminator::BinBranch {
                            cond,
                            true_label: bind_label.clone(),
                            false_label: next_label.clone(),
                        });
                        let bind_idx = self.new_block(bind_label);
                        self.switch_to(bind_idx);
                        self.bind_pattern_into(alt, scrutinee, &slots);
                        self.set_term(IRTerminator::Jump(end_label.clone()));
                        let next_idx = self.new_block(next_label);
                        self.switch_to(next_idx);
                    }
                }
                let end_idx = self.new_block(end_label);
                self.switch_to(end_idx);
            }
            _ => {}
        }
    }

    /// Collect the names bound by a pattern (in order of appearance).
    fn pattern_bound_names(pattern: &crate::ast::Pattern) -> Vec<String> {
        use crate::ast::Pattern;
        let mut out = Vec::new();
        match pattern {
            Pattern::Ident(name, _) => out.push(name.clone()),
            Pattern::Enum(_, _, fields, _) => {
                for f in fields {
                    out.extend(Self::pattern_bound_names(f));
                }
            }
            Pattern::Tuple(elems, _) => {
                for p in elems {
                    out.extend(Self::pattern_bound_names(p));
                }
            }
            Pattern::Struct(_, field_pats, _) => {
                for (_, p) in field_pats {
                    out.extend(Self::pattern_bound_names(p));
                }
            }
            Pattern::Or(alts, _) => {
                for a in alts {
                    out.extend(Self::pattern_bound_names(a));
                }
            }
            // C6: `TritPat::bound_names` is the one definition of this, shared
            // with the analyzer and the borrow checker, so an or-pattern's
            // slots and the values written into them cannot disagree.
            Pattern::Trit(tp, _) => out.extend(tp.bound_names()),
            _ => {}
        }
        out
    }

    /// Like `bind_pattern_locals`, but store each bound value into the
    /// pre-allocated slot for its name instead of allocating fresh slots.
    /// Used for or-patterns, where every alternative must write to the same
    /// slot while extracting from its own positions.
    fn bind_pattern_into(
        &mut self,
        pattern: &crate::ast::Pattern,
        scrutinee: &IRValue,
        slots: &std::collections::HashMap<String, IRTemp>,
    ) {
        use crate::ast::Pattern;
        match pattern {
            Pattern::Ident(name, _) => {
                if let Some(slot) = slots.get(name) {
                    self.emit(IRInstr::Store {
                        ptr: IRValue::Temp(slot.clone()),
                        val: scrutinee.clone(),
                        ty: IRType::I64,
                    });
                }
            }
            // C6: an alternative of an or-pattern writes its captures into the
            // shared slots, so `0t+?@n | 0t-?@n` binds `n` from whichever arm
            // matched.
            Pattern::Trit(tp, _) => {
                let x = scrutinee.clone();
                for (name, val) in self.trit_capture_values(tp, &x) {
                    if let Some(slot) = slots.get(&name) {
                        self.emit(IRInstr::Store {
                            ptr: IRValue::Temp(slot.clone()),
                            val,
                            ty: IRType::I64,
                        });
                    }
                }
            }
            Pattern::Enum(_, _, fields, _) => {
                // scrutinee is a pointer to [tag, field0, field1, …]; field *i*
                // is at word 1+i (P43 — see `bind_pattern_locals`).
                for (fi, field) in fields.iter().enumerate() {
                    if let Pattern::Ident(name, _) = field {
                        if let Some(slot) = slots.get(name) {
                            let val_ptr_t = self.fresh_temp();
                            self.emit(IRInstr::GetPtr {
                                dst: val_ptr_t.clone(),
                                ptr: scrutinee.clone(),
                                idx: IRValue::Const(IRConst::Int(fi as i64 + 1)),
                                ty: IRType::I64,
                            });
                            let val_t = self.fresh_temp();
                            self.emit(IRInstr::Load {
                                dst: val_t.clone(),
                                ptr: IRValue::Temp(val_ptr_t),
                                ty: IRType::I64,
                            });
                            self.emit(IRInstr::Store {
                                ptr: IRValue::Temp(slot.clone()),
                                val: IRValue::Temp(val_t),
                                ty: IRType::I64,
                            });
                        }
                    } else {
                        // P90, as in `bind_pattern_locals`: the payload word,
                        // not the cell.
                        let payload = self.load_slot(scrutinee, fi as i64 + 1);
                        self.bind_pattern_into(field, &payload, slots);
                    }
                }
            }
            Pattern::Tuple(elems, _) => {
                // P90: the or-pattern path needs the same arm, or
                // `Ok((a, b)) | Err((a, b))` allocates slots for `a` and `b`
                // (pattern_bound_names finds them) and then writes to neither.
                for (i, sub_pat) in elems.iter().enumerate() {
                    let elem_val = self.load_slot(scrutinee, i as i64);
                    self.bind_pattern_into(sub_pat, &elem_val, slots);
                }
            }
            Pattern::Struct(struct_name, field_pats, _) => {
                let field_defs: Vec<String> = self.structs.get(struct_name)
                    .map(|fs| fs.iter().map(|(n, _)| n.clone()).collect())
                    .unwrap_or_default();
                for (field_name, sub_pat) in field_pats {
                    if let Pattern::Ident(var_name, _) = sub_pat {
                        if let Some(slot) = slots.get(var_name) {
                            let idx = field_defs.iter().position(|n| n == field_name)
                                .unwrap_or(0) as i64;
                            let ptr_t = self.fresh_temp();
                            self.emit(IRInstr::GetPtr {
                                dst: ptr_t.clone(),
                                ptr: scrutinee.clone(),
                                idx: IRValue::Const(IRConst::Int(idx)),
                                ty: IRType::I64,
                            });
                            let val_t = self.fresh_temp();
                            self.emit(IRInstr::Load {
                                dst: val_t.clone(),
                                ptr: IRValue::Temp(ptr_t),
                                ty: IRType::I64,
                            });
                            self.emit(IRInstr::Store {
                                ptr: IRValue::Temp(slot.clone()),
                                val: IRValue::Temp(val_t),
                                ty: IRType::I64,
                            });
                        }
                    } else {
                        self.bind_pattern_into(sub_pat, scrutinee, slots);
                    }
                }
            }
            Pattern::Or(alts, _) => {
                if let Some(first) = alts.first() {
                    self.bind_pattern_into(first, scrutinee, slots);
                }
            }
            _ => {}
        }
    }
}
