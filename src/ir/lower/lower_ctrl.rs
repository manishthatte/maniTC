// ir/lower/lower_ctrl.rs — Control flow lowering: if/tif/match/pattern/bindings.

use super::IRLowerer;
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

        if ir_result_ty == IRType::Void || incoming.is_empty() {
            IRValue::Void
        } else {
            let phi_dst = self.fresh_temp();
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
            self.emit(IRInstr::Phi {
                dst: phi_dst.clone(),
                ty: ir_result_ty,
                incoming: vec![
                    (pos_val, pos_end_label),
                    (zero_val, zero_end_label),
                    (neg_val, neg_end_label),
                ],
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
            self.emit(IRInstr::Phi {
                dst: phi_dst.clone(),
                ty: ir_result_ty,
                incoming: vec![
                    (ok_val, ok_end_label),
                    (unk_val, unk_end_label),
                    (err_val, err_end_label),
                ],
            });
            IRValue::Temp(phi_dst)
        }
    }

    pub(super) fn lower_match(&mut self, me: &TypedMatchExpr, result_ty: &ManiType) -> IRValue {
        let scrutinee_val = self.lower_expr(&me.scrutinee);
        let ir_result_ty = IRType::from_mani(result_ty);
        let merge_label = self.fresh_label("match_merge");
        let mut incoming = Vec::new();

        for arm in &me.arms {
            let arm_label = self.fresh_label("match_arm");
            let next_label = self.fresh_label("match_next");

            // Emit comparison for pattern
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
                self.bind_pattern_locals(&arm.pattern, &scrutinee_val);
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
            self.bind_pattern_locals(&arm.pattern, &scrutinee_val);
            let arm_val = self.lower_expr(&arm.body);
            let arm_end = self.current_block;
            incoming.push((arm_val, self.blocks[arm_end].label.clone()));
            self.set_term(IRTerminator::Jump(merge_label.clone()));

            let next_idx = self.new_block(next_label);
            self.switch_to(next_idx);
        }

        // Fallthrough (no pattern matched) → jump to merge
        self.set_term(IRTerminator::Jump(merge_label.clone()));

        let merge_idx = self.new_block(merge_label);
        self.switch_to(merge_idx);

        if ir_result_ty == IRType::Void || incoming.is_empty() {
            IRValue::Void
        } else {
            let phi_dst = self.fresh_temp();
            self.emit(IRInstr::Phi {
                dst: phi_dst.clone(),
                ty: ir_result_ty,
                incoming,
            });
            IRValue::Temp(phi_dst)
        }
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
                let dst = self.fresh_temp();
                self.emit(IRInstr::BinOp {
                    dst: dst.clone(),
                    op: IRBinOp::IEq,
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
            crate::ast::Pattern::Enum(variant, enum_name, _, _) => {
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
                if let Some(variants) = self.enum_variants.get(enum_type_name).cloned() {
                    // Custom enum: scrutinee is an integer tag stored directly
                    if let Some(idx) = variants.iter().position(|v| v == variant_name) {
                        let cmp_t = self.fresh_temp();
                        self.emit(IRInstr::BinOp {
                            dst: cmp_t.clone(),
                            op: IRBinOp::IEq,
                            lhs: scrutinee.clone(),
                            rhs: IRValue::Const(IRConst::Int(idx as i64)),
                            ty: IRType::Bool,
                        });
                        return IRValue::Temp(cmp_t);
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
                IRValue::Temp(cmp_t)
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
                    let always_true = matches!(
                        sub_pat,
                        crate::ast::Pattern::Ident(_, _) | crate::ast::Pattern::Wildcard(_)
                    );
                    if always_true {
                        continue;
                    }

                    let idx = field_defs.iter().position(|n| n == field_name)
                        .unwrap_or(0) as i64;

                    let field_ptr = self.fresh_temp();
                    self.emit(IRInstr::GetPtr {
                        dst: field_ptr.clone(),
                        ptr: scrutinee.clone(),
                        idx: IRValue::Const(IRConst::Int(idx)),
                        ty: IRType::I64,
                    });
                    let field_val = self.fresh_temp();
                    self.emit(IRInstr::Load {
                        dst: field_val.clone(),
                        ptr: IRValue::Temp(field_ptr),
                        ty: IRType::I64,
                    });

                    let sub_result = self.lower_pattern_match(
                        sub_pat,
                        &IRValue::Temp(field_val),
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
            _ => IRValue::Const(IRConst::Bool(true)),
        }
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
            Pattern::Enum(_, _, fields, _) => {
                // scrutinee is a pointer to [tag, value].
                // Bind each Ident field to the value word (offset 1).
                for field in fields {
                    if let Pattern::Ident(name, _) = field {
                        let val_ptr_t = self.fresh_temp();
                        self.emit(IRInstr::GetPtr {
                            dst: val_ptr_t.clone(),
                            ptr: scrutinee.clone(),
                            idx: IRValue::Const(IRConst::Int(1)),
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
                        self.bind_pattern_locals(field, scrutinee);
                    }
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
                if let Some(first) = alts.first() {
                    self.bind_pattern_locals(first, scrutinee);
                }
            }
            _ => {}
        }
    }
}
