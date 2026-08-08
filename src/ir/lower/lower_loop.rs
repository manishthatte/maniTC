// ir/lower/lower_loop.rs — Loop lowering: for/while/loop + break/continue patching.

use super::IRLowerer;
use crate::ir::types::*;
use crate::semantic::{ManiType, TypedBlock, TypedExprKind, TypedForExpr, TypedWhileExpr};

impl IRLowerer {
    pub(super) fn lower_for(&mut self, fe: &TypedForExpr) {
        // Check if iterating over a fixed-size array
        if let ManiType::Array(ref elem_mty, Some(n)) = fe.iter.ty {
            self.lower_for_array(fe, elem_mty, n);
            return;
        }
        // Check if iterating over a Vec<T>
        if let ManiType::Generic(ref gname, ref gparams) = fe.iter.ty {
            if gname == "Vec" {
                let elem_mty = gparams.first().cloned().unwrap_or(ManiType::Unknown);
                self.lower_for_vec(fe, &elem_mty);
                return;
            }
        }

        // Extract start/end from range, or treat iter as a plain length
        let (start_val, end_val) = match &fe.iter.kind {
            TypedExprKind::Range(start, end, inclusive) => {
                let sv = self.lower_expr(start);
                let ev = self.lower_expr(end);
                if *inclusive {
                    // Inclusive range a..=b: adjust end to b+1 so the loop uses < bound
                    let adj = self.fresh_temp();
                    self.emit(IRInstr::BinOp {
                        dst: adj.clone(), op: IRBinOp::Add,
                        lhs: ev, rhs: IRValue::Const(IRConst::Int(1)), ty: IRType::I64,
                    });
                    (sv, IRValue::Temp(adj))
                } else {
                    (sv, ev)
                }
            }
            _ => {
                let v = self.lower_expr(&fe.iter);
                (IRValue::Const(IRConst::Int(0)), v)
            }
        };

        let var_ty = IRType::I64;
        let var_alloca = self.fresh_temp();
        self.emit(IRInstr::Alloca { dst: var_alloca.clone(), ty: var_ty.clone() });
        self.emit(IRInstr::Store {
            ptr: IRValue::Temp(var_alloca.clone()),
            val: start_val,
            ty: var_ty.clone(),
        });
        self.locals.insert(fe.var.clone(), (var_alloca.clone(), var_ty.clone()));

        // Alloca for the end value so it's stable across iterations
        let end_alloca = self.fresh_temp();
        self.emit(IRInstr::Alloca { dst: end_alloca.clone(), ty: var_ty.clone() });
        self.emit(IRInstr::Store {
            ptr: IRValue::Temp(end_alloca.clone()),
            val: end_val,
            ty: var_ty.clone(),
        });

        let cond_label = self.fresh_label("for_cond");
        let body_label = self.fresh_label("for_body");
        let inc_label  = self.fresh_label("for_inc");
        let exit_label = self.fresh_label("for_exit");

        self.set_term(IRTerminator::Jump(cond_label.clone()));

        // Condition block: i < end
        let cond_idx = self.new_block(cond_label.clone());
        self.switch_to(cond_idx);
        let cur_t = self.fresh_temp();
        self.emit(IRInstr::Load {
            dst: cur_t.clone(), ptr: IRValue::Temp(var_alloca.clone()), ty: var_ty.clone()
        });
        let end_t = self.fresh_temp();
        self.emit(IRInstr::Load {
            dst: end_t.clone(), ptr: IRValue::Temp(end_alloca.clone()), ty: var_ty.clone()
        });
        let cond_t = self.fresh_temp();
        self.emit(IRInstr::BinOp {
            dst: cond_t.clone(),
            op: IRBinOp::ILt,
            lhs: IRValue::Temp(cur_t),
            rhs: IRValue::Temp(end_t),
            ty: IRType::Bool,
        });
        self.set_term(IRTerminator::BinBranch {
            cond: IRValue::Temp(cond_t),
            true_label: body_label.clone(),
            false_label: exit_label.clone(),
        });

        // Body block
        let body_idx = self.new_block(body_label);
        self.switch_to(body_idx);
        self.lower_block(&fe.body);
        if matches!(self.blocks[self.current_block].term, IRTerminator::Unreachable) {
            self.set_term(IRTerminator::Jump(inc_label.clone()));
        }

        // Increment block
        let inc_label_str = inc_label.clone();
        let inc_idx = self.new_block(inc_label);
        self.switch_to(inc_idx);
        let cur_t = self.fresh_temp();
        self.emit(IRInstr::Load {
            dst: cur_t.clone(),
            ptr: IRValue::Temp(var_alloca.clone()),
            ty: var_ty.clone(),
        });
        let inc_t = self.fresh_temp();
        self.emit(IRInstr::BinOp {
            dst: inc_t.clone(),
            op: IRBinOp::Add,
            lhs: IRValue::Temp(cur_t),
            rhs: IRValue::Const(IRConst::Int(1)),
            ty: var_ty.clone(),
        });
        self.emit(IRInstr::Store {
            ptr: IRValue::Temp(var_alloca),
            val: IRValue::Temp(inc_t),
            ty: var_ty,
        });
        self.set_term(IRTerminator::Jump(cond_label));

        // Exit block
        let exit_idx = self.new_block(exit_label.clone());
        self.patch_break_labels(body_idx, &exit_label);
        self.patch_continue_labels(body_idx, &inc_label_str);
        self.switch_to(exit_idx);
        let _ = (inc_idx,);
    }

    /// For-loop over a fixed-size array: `for v in arr { ... }`
    pub(super) fn lower_for_array(
        &mut self,
        fe: &TypedForExpr,
        elem_mty: &ManiType,
        n: usize,
    ) {
        let elem_ty = IRType::from_mani(elem_mty);
        let idx_ty = IRType::I64;

        let arr_ptr = self.lower_expr(&fe.iter);

        // Alloca for internal index __idx = 0
        let idx_alloca = self.fresh_temp();
        self.emit(IRInstr::Alloca { dst: idx_alloca.clone(), ty: idx_ty.clone() });
        self.emit(IRInstr::Store {
            ptr: IRValue::Temp(idx_alloca.clone()),
            val: IRValue::Const(IRConst::Int(0)),
            ty: idx_ty.clone(),
        });

        // Alloca for loop variable (element value)
        let var_alloca = self.fresh_temp();
        self.emit(IRInstr::Alloca { dst: var_alloca.clone(), ty: elem_ty.clone() });
        self.locals.insert(fe.var.clone(), (var_alloca.clone(), elem_ty.clone()));

        let cond_label = self.fresh_label("for_cond");
        let body_label = self.fresh_label("for_body");
        let inc_label  = self.fresh_label("for_inc");
        let exit_label = self.fresh_label("for_exit");

        self.set_term(IRTerminator::Jump(cond_label.clone()));

        // Condition block: __idx < n
        let cond_idx = self.new_block(cond_label.clone());
        self.switch_to(cond_idx);
        let cur_idx_t = self.fresh_temp();
        self.emit(IRInstr::Load {
            dst: cur_idx_t.clone(),
            ptr: IRValue::Temp(idx_alloca.clone()),
            ty: idx_ty.clone(),
        });
        let cond_t = self.fresh_temp();
        self.emit(IRInstr::BinOp {
            dst: cond_t.clone(),
            op: IRBinOp::ILt,
            lhs: IRValue::Temp(cur_idx_t.clone()),
            rhs: IRValue::Const(IRConst::Int(n as i64)),
            ty: IRType::Bool,
        });
        self.set_term(IRTerminator::BinBranch {
            cond: IRValue::Temp(cond_t),
            true_label: body_label.clone(),
            false_label: exit_label.clone(),
        });

        // Body block: load arr[__idx] into var
        let body_blk = self.new_block(body_label);
        self.switch_to(body_blk);
        let idx_t = self.fresh_temp();
        self.emit(IRInstr::Load {
            dst: idx_t.clone(),
            ptr: IRValue::Temp(idx_alloca.clone()),
            ty: idx_ty.clone(),
        });
        let elem_ptr_t = self.fresh_temp();
        self.emit(IRInstr::GetPtr {
            dst: elem_ptr_t.clone(),
            ptr: arr_ptr.clone(),
            idx: IRValue::Temp(idx_t),
            ty: elem_ty.clone(),
        });
        let elem_val_t = self.fresh_temp();
        self.emit(IRInstr::Load {
            dst: elem_val_t.clone(),
            ptr: IRValue::Temp(elem_ptr_t),
            ty: elem_ty.clone(),
        });
        self.emit(IRInstr::Store {
            ptr: IRValue::Temp(var_alloca.clone()),
            val: IRValue::Temp(elem_val_t),
            ty: elem_ty.clone(),
        });
        self.lower_block(&fe.body);
        if matches!(self.blocks[self.current_block].term, IRTerminator::Unreachable) {
            self.set_term(IRTerminator::Jump(inc_label.clone()));
        }

        // Increment block: __idx += 1
        let inc_label_str = inc_label.clone();
        let inc_blk = self.new_block(inc_label);
        self.switch_to(inc_blk);
        let cur_idx_t = self.fresh_temp();
        self.emit(IRInstr::Load {
            dst: cur_idx_t.clone(),
            ptr: IRValue::Temp(idx_alloca.clone()),
            ty: idx_ty.clone(),
        });
        let next_idx_t = self.fresh_temp();
        self.emit(IRInstr::BinOp {
            dst: next_idx_t.clone(),
            op: IRBinOp::Add,
            lhs: IRValue::Temp(cur_idx_t),
            rhs: IRValue::Const(IRConst::Int(1)),
            ty: idx_ty.clone(),
        });
        self.emit(IRInstr::Store {
            ptr: IRValue::Temp(idx_alloca),
            val: IRValue::Temp(next_idx_t),
            ty: idx_ty,
        });
        self.set_term(IRTerminator::Jump(cond_label));

        // Exit block
        let exit_blk = self.new_block(exit_label.clone());
        self.patch_break_labels(body_blk, &exit_label);
        self.patch_continue_labels(body_blk, &inc_label_str);
        self.switch_to(exit_blk);
        let _ = (inc_blk,);
    }

    pub(super) fn lower_for_vec(&mut self, fe: &TypedForExpr, elem_mty: &ManiType) {
        let elem_ty = IRType::from_mani(elem_mty);
        let idx_ty = IRType::I64;

        let vec_val = self.lower_expr(&fe.iter);
        let vec_alloca = self.fresh_temp();
        self.emit(IRInstr::Alloca { dst: vec_alloca.clone(), ty: idx_ty.clone() });
        self.emit(IRInstr::Store {
            ptr: IRValue::Temp(vec_alloca.clone()),
            val: vec_val,
            ty: idx_ty.clone(),
        });

        // Get Vec::len and cache it in an alloca
        let vec_h0 = self.fresh_temp();
        self.emit(IRInstr::Load {
            dst: vec_h0.clone(), ptr: IRValue::Temp(vec_alloca.clone()), ty: idx_ty.clone()
        });
        let len_t = self.fresh_temp();
        self.emit(IRInstr::Call {
            dst: Some(len_t.clone()),
            func: "Vec::len".to_string(),
            args: vec![IRValue::Temp(vec_h0)],
            ret_ty: idx_ty.clone(),
        });
        let len_alloca = self.fresh_temp();
        self.emit(IRInstr::Alloca { dst: len_alloca.clone(), ty: idx_ty.clone() });
        self.emit(IRInstr::Store {
            ptr: IRValue::Temp(len_alloca.clone()), val: IRValue::Temp(len_t), ty: idx_ty.clone()
        });

        // Index alloca, init to 0
        let idx_alloca = self.fresh_temp();
        self.emit(IRInstr::Alloca { dst: idx_alloca.clone(), ty: idx_ty.clone() });
        self.emit(IRInstr::Store {
            ptr: IRValue::Temp(idx_alloca.clone()),
            val: IRValue::Const(IRConst::Int(0)),
            ty: idx_ty.clone(),
        });

        // Loop variable alloca
        let var_alloca = self.fresh_temp();
        self.emit(IRInstr::Alloca { dst: var_alloca.clone(), ty: elem_ty.clone() });
        self.locals.insert(fe.var.clone(), (var_alloca.clone(), elem_ty.clone()));

        let cond_label = self.fresh_label("for_cond");
        let body_label = self.fresh_label("for_body");
        let inc_label  = self.fresh_label("for_inc");
        let exit_label = self.fresh_label("for_exit");
        self.set_term(IRTerminator::Jump(cond_label.clone()));

        // Condition: idx < len
        let cond_blk = self.new_block(cond_label.clone());
        self.switch_to(cond_blk);
        let cur_idx_t = self.fresh_temp();
        self.emit(IRInstr::Load {
            dst: cur_idx_t.clone(), ptr: IRValue::Temp(idx_alloca.clone()), ty: idx_ty.clone()
        });
        let cur_len_t = self.fresh_temp();
        self.emit(IRInstr::Load {
            dst: cur_len_t.clone(), ptr: IRValue::Temp(len_alloca.clone()), ty: idx_ty.clone()
        });
        let cond_t = self.fresh_temp();
        self.emit(IRInstr::BinOp {
            dst: cond_t.clone(), op: IRBinOp::ILt,
            lhs: IRValue::Temp(cur_idx_t), rhs: IRValue::Temp(cur_len_t), ty: IRType::Bool,
        });
        self.set_term(IRTerminator::BinBranch {
            cond: IRValue::Temp(cond_t),
            true_label: body_label.clone(),
            false_label: exit_label.clone(),
        });

        // Body: load element via Vec::get
        let body_blk = self.new_block(body_label);
        self.switch_to(body_blk);
        let idx_t = self.fresh_temp();
        self.emit(IRInstr::Load {
            dst: idx_t.clone(), ptr: IRValue::Temp(idx_alloca.clone()), ty: idx_ty.clone()
        });
        let vec_h = self.fresh_temp();
        self.emit(IRInstr::Load {
            dst: vec_h.clone(), ptr: IRValue::Temp(vec_alloca.clone()), ty: idx_ty.clone()
        });
        let elem_t = self.fresh_temp();
        self.emit(IRInstr::Call {
            dst: Some(elem_t.clone()),
            func: "Vec::get".to_string(),
            args: vec![IRValue::Temp(vec_h), IRValue::Temp(idx_t)],
            ret_ty: elem_ty.clone(),
        });
        self.emit(IRInstr::Store {
            ptr: IRValue::Temp(var_alloca.clone()), val: IRValue::Temp(elem_t), ty: elem_ty.clone()
        });
        self.lower_block(&fe.body);
        if matches!(self.blocks[self.current_block].term, IRTerminator::Unreachable) {
            self.set_term(IRTerminator::Jump(inc_label.clone()));
        }

        // Increment: idx += 1
        let inc_label_str = inc_label.clone();
        let inc_blk = self.new_block(inc_label);
        self.switch_to(inc_blk);
        let ci = self.fresh_temp();
        self.emit(IRInstr::Load {
            dst: ci.clone(), ptr: IRValue::Temp(idx_alloca.clone()), ty: idx_ty.clone()
        });
        let ni = self.fresh_temp();
        self.emit(IRInstr::BinOp {
            dst: ni.clone(), op: IRBinOp::Add,
            lhs: IRValue::Temp(ci), rhs: IRValue::Const(IRConst::Int(1)), ty: idx_ty.clone(),
        });
        self.emit(IRInstr::Store {
            ptr: IRValue::Temp(idx_alloca), val: IRValue::Temp(ni), ty: idx_ty
        });
        self.patch_continue_labels(body_blk, &inc_label_str);
        self.set_term(IRTerminator::Jump(cond_label));

        // Exit block
        let exit_blk = self.new_block(exit_label.clone());
        self.patch_break_labels(body_blk, &exit_label);
        self.switch_to(exit_blk);
        self.locals.remove(&fe.var);
        let _ = (cond_blk, body_blk, inc_blk, exit_blk);
    }

    /// Replace all `Jump("__break__")` terminators in blocks [body_start..)
    /// with `Jump(exit_label)`.
    pub(super) fn patch_break_labels(&mut self, body_start: usize, exit_label: &str) {
        let n = self.blocks.len();
        for i in body_start..n {
            let patched = match &self.blocks[i].term {
                IRTerminator::Jump(ref l) if l == "__break__" => {
                    Some(IRTerminator::Jump(exit_label.to_string()))
                }
                _ => None,
            };
            if let Some(t) = patched {
                self.blocks[i].term = t;
            }
        }
    }

    /// Replace all `Jump("__continue__")` terminators in blocks [body_start..)
    /// with `Jump(continue_label)`.
    pub(super) fn patch_continue_labels(&mut self, body_start: usize, continue_label: &str) {
        let n = self.blocks.len();
        for i in body_start..n {
            let patched = match &self.blocks[i].term {
                IRTerminator::Jump(ref l) if l == "__continue__" => {
                    Some(IRTerminator::Jump(continue_label.to_string()))
                }
                _ => None,
            };
            if let Some(t) = patched {
                self.blocks[i].term = t;
            }
        }
    }

    pub(super) fn lower_while(&mut self, we: &TypedWhileExpr) {
        let cond_label = self.fresh_label("while_cond");
        let body_label = self.fresh_label("while_body");
        let exit_label = self.fresh_label("while_exit");
        let cond_label_str = cond_label.clone();

        self.set_term(IRTerminator::Jump(cond_label.clone()));

        let cond_idx = self.new_block(cond_label.clone());
        self.switch_to(cond_idx);
        let cond_val = self.lower_expr(&we.cond);
        self.set_term(IRTerminator::BinBranch {
            cond: cond_val,
            true_label: body_label.clone(),
            false_label: exit_label.clone(),
        });

        let body_idx = self.new_block(body_label);
        self.switch_to(body_idx);
        self.lower_block(&we.body);
        if matches!(self.blocks[self.current_block].term, IRTerminator::Unreachable) {
            self.set_term(IRTerminator::Jump(cond_label));
        }

        let exit_idx = self.new_block(exit_label.clone());
        self.patch_break_labels(body_idx, &exit_label);
        self.patch_continue_labels(body_idx, &cond_label_str);
        self.switch_to(exit_idx);
        let _ = (cond_idx, body_idx);
    }

    pub(super) fn lower_loop(&mut self, block: &TypedBlock) {
        let body_label = self.fresh_label("loop_body");
        let exit_label = self.fresh_label("loop_exit");

        self.set_term(IRTerminator::Jump(body_label.clone()));

        let body_label_str = body_label.clone();
        let body_idx = self.new_block(body_label.clone());
        self.switch_to(body_idx);
        self.lower_block(block);
        if matches!(self.blocks[self.current_block].term, IRTerminator::Unreachable) {
            self.set_term(IRTerminator::Jump(body_label));
        }

        let exit_idx = self.new_block(exit_label.clone());
        self.patch_break_labels(body_idx, &exit_label);
        self.patch_continue_labels(body_idx, &body_label_str);
        self.switch_to(exit_idx);
        let _ = body_idx;
    }
}
