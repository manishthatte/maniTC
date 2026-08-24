// ir/lower/lower_stmt.rs — Statement lowering for IRLowerer.

use super::IRLowerer;
use super::helpers::{binop_to_ir, slot_access_ty};
use crate::ir::types::*;
use crate::ast::LetPat;
use crate::semantic::{ManiType, TypedStmt, TypedExprKind};

impl IRLowerer {
    /// The most precise sized IR type recoverable from an array-literal
    /// initializer. Semantic sizes every array literal, but a nested
    /// annotation like `[[int]]` erases the INNER sizes from the outer
    /// type — recover them from the literal's element expressions so
    /// nested iteration can find its bounds.
    pub(super) fn sized_array_irtype(expr: &crate::semantic::TypedExpr) -> IRType {
        if let TypedExprKind::Array(elems) = &expr.kind {
            if let ManiType::Array(_, Some(n)) = &expr.ty {
                let elem_ir = elems.first()
                    .map(Self::sized_array_irtype)
                    .unwrap_or(IRType::I64);
                return IRType::Array(Box::new(elem_ir), *n);
            }
        }
        IRType::from_mani(&expr.ty)
    }

    pub(super) fn lower_stmt(&mut self, stmt: &TypedStmt) -> IRValue {
        match stmt {
            TypedStmt::Let(ls) => {
                // Special case: LetPat::Tuple — destructure a tuple into separate locals.
                if let LetPat::Tuple(names) = &ls.pat {
                    if let Some(init) = &ls.init {
                        // Element types, so each name is bound at its OWN width.
                        //
                        // Every name used to be bound as I64 regardless. Tuple
                        // slots really are 8 bytes wide (slot_access_ty), so the
                        // LOAD was right — but the binding then claimed i64 for
                        // a `trit`, and a later `carry = c` emitted
                        //     store i64 %v, ptr %carry   ; %carry = alloca i8
                        // an eight-byte write into a one-byte stack slot, which
                        // silently overran the neighbouring allocas. That is
                        // ORACLE_FINDINGS.md Section 10: it read as "LLVM loses
                        // a trit argument in a loop" because the clobbered
                        // neighbour happened to be another local. T3 was
                        // unaffected — its slots are uniformly one word.
                        let elem_manitys: Option<Vec<ManiType>> =
                            match (&init.ty, &ls.ty) {
                                (ManiType::Tuple(ts), _) => Some(ts.clone()),
                                (_, ManiType::Tuple(ts)) => Some(ts.clone()),
                                _ => None,
                            };
                        // Lower the initializer expression (should produce a tuple/struct pointer)
                        let tuple_ptr = self.lower_expr(init);
                        for (i, name) in names.iter().enumerate() {
                            let elem_ir = elem_manitys
                                .as_ref()
                                .and_then(|ts| ts.get(i))
                                .map(IRType::from_mani)
                                .unwrap_or(IRType::I64);
                            let slot_ty = slot_access_ty(&elem_ir);
                            // Compute pointer to field i
                            let field_ptr = self.fresh_temp();
                            self.emit(IRInstr::GetPtr {
                                dst: field_ptr.clone(),
                                ptr: tuple_ptr.clone(),
                                idx: IRValue::Const(IRConst::Int(i as i64)),
                                ty: slot_ty.clone(),
                            });
                            // Load the field value at the slot's width.
                            let val_t = self.fresh_temp();
                            self.emit(IRInstr::Load {
                                dst: val_t.clone(),
                                ptr: IRValue::Temp(field_ptr),
                                ty: slot_ty.clone(),
                            });
                            // Narrow to the element's own type before binding.
                            let mut val = IRValue::Temp(val_t);
                            if slot_ty != elem_ir {
                                let cast_t = self.fresh_temp();
                                self.emit(IRInstr::Cast {
                                    dst: cast_t.clone(),
                                    src: val,
                                    from_ty: slot_ty,
                                    to_ty: elem_ir.clone(),
                                });
                                val = IRValue::Temp(cast_t);
                            }
                            // Alloca a slot for this name and store
                            let alloca_t = self.fresh_temp();
                            self.emit(IRInstr::Alloca {
                                dst: alloca_t.clone(),
                                ty: elem_ir.clone(),
                            });
                            self.emit(IRInstr::Store {
                                ptr: IRValue::Temp(alloca_t.clone()),
                                val,
                                ty: elem_ir.clone(),
                            });
                            self.locals.insert(name.clone(), (alloca_t, elem_ir));
                        }
                    }
                    return IRValue::Void;
                }

                // Special case: Result<T,E> construction via Ok/Unknown/Err.
                // Store as a 2-word pair: [tag (+1/0/-1), value] on stack.
                // The variable's alloca holds a pointer to that pair.
                if matches!(&ls.ty, ManiType::Generic(n, _) if n == "Result") {
                    if let Some(init) = &ls.init {
                        if let TypedExprKind::Call(callee, args) = &init.kind {
                            if let TypedExprKind::Ident(fname) = &callee.kind {
                                if matches!(fname.as_str(), "Ok" | "Unknown" | "Err") {
                                    let tag: i64 = match fname.as_str() {
                                        "Ok" => 1, "Unknown" => 0, _ => -1
                                    };
                                    // Alloca 2 words: [tag, value]
                                    let pair_alloca = self.fresh_temp();
                                    self.emit(IRInstr::Alloca {
                                        dst: pair_alloca.clone(),
                                        ty: IRType::Array(Box::new(IRType::I64), 2),
                                    });
                                    // Store tag at offset 0
                                    self.emit(IRInstr::Store {
                                        ptr: IRValue::Temp(pair_alloca.clone()),
                                        val: IRValue::Const(IRConst::Int(tag)),
                                        ty: IRType::I64,
                                    });
                                    // Store value at offset 1
                                    if let Some(arg) = args.first() {
                                        let val = self.lower_expr(arg);
                                        let val_ptr = self.fresh_temp();
                                        self.emit(IRInstr::GetPtr {
                                            dst: val_ptr.clone(),
                                            ptr: IRValue::Temp(pair_alloca.clone()),
                                            idx: IRValue::Const(IRConst::Int(1)),
                                            ty: IRType::I64,
                                        });
                                        self.emit(IRInstr::Store {
                                            ptr: IRValue::Temp(val_ptr),
                                            val,
                                            ty: IRType::I64,
                                        });
                                    }
                                    // Alloca 1 word to hold a pointer to the pair
                                    let ptr_alloca = self.fresh_temp();
                                    self.emit(IRInstr::Alloca {
                                        dst: ptr_alloca.clone(),
                                        ty: IRType::I64,
                                    });
                                    self.emit(IRInstr::Store {
                                        ptr: IRValue::Temp(ptr_alloca.clone()),
                                        val: IRValue::Temp(pair_alloca),
                                        ty: IRType::I64,
                                    });
                                    self.locals.insert(ls.name.clone(), (ptr_alloca, IRType::I64));
                                    return IRValue::Void;
                                }
                            }
                        }
                    }
                }
                // Regular Let
                let mut ty = IRType::from_mani(&ls.ty);
                // Array locals initialized from a literal keep the deep
                // sized IR type, so later iteration and length queries can
                // recover the bounds — the declared ManiType erases inner
                // sizes for nested cases like `let p: [[int]] = [[..], ..];`.
                if let (ManiType::Array(_, _), Some(init)) = (&ls.ty, &ls.init) {
                    if matches!(&init.kind, TypedExprKind::Array(_))
                        && matches!(&init.ty, ManiType::Array(_, Some(_)))
                    {
                        ty = Self::sized_array_irtype(init);
                    }
                }

                // For real struct types: the init expression returns the struct alloca
                // directly (a StructLit alloca or a sret alloca from a Call).
                // Use it as the variable's storage — no extra alloca wrapper needed.
                if let IRType::Struct(ref sname) = ty {
                    if self.is_real_struct(sname) {
                        if let Some(init) = &ls.init {
                            // Value semantics: `let b = a;` where `a` is an existing
                            // struct local must copy the struct (like struct-returning
                            // calls do via lower_struct_call), not alias a's storage.
                            if let TypedExprKind::Ident(src_name) = &init.kind {
                                if self.locals.contains_key(src_name) {
                                    let sname = sname.clone();
                                    let src_val = self.lower_expr(init);
                                    let n = self.struct_nfields(&sname);
                                    let copy_alloca = self.fresh_temp();
                                    self.emit(IRInstr::Alloca {
                                        dst: copy_alloca.clone(),
                                        ty: ty.clone(),
                                    });
                                    self.emit_struct_copy(src_val, copy_alloca.clone(), n);
                                    self.locals.insert(ls.name.clone(), (copy_alloca, ty));
                                    return IRValue::Void;
                                }
                            }
                            let val = self.lower_expr(init);
                            if let IRValue::Temp(struct_alloca) = val {
                                self.locals.insert(ls.name.clone(), (struct_alloca, ty));
                                return IRValue::Void;
                            }
                        }
                        // No init or non-temp result: alloca an uninitialised struct.
                        let alloca_t = self.fresh_temp();
                        let ty2 = ty.clone();
                        self.emit(IRInstr::Alloca { dst: alloca_t.clone(), ty: ty2 });
                        self.locals.insert(ls.name.clone(), (alloca_t, ty));
                        return IRValue::Void;
                    }
                }

                let alloca_t = self.fresh_temp();
                self.emit(IRInstr::Alloca { dst: alloca_t.clone(), ty: ty.clone() });
                if let Some(init) = &ls.init {
                    let val = self.lower_expr(init);
                    let val = self.coerce_value(val, &init.ty, &ls.ty);
                    self.emit(IRInstr::Store {
                        ptr: IRValue::Temp(alloca_t.clone()),
                        val,
                        ty: ty.clone(),
                    });
                }
                self.locals.insert(ls.name.clone(), (alloca_t, ty));
                IRValue::Void
            }
            TypedStmt::Assign(a) => {
                let val = self.lower_expr(&a.value);
                let val = self.coerce_value(val, &a.value.ty, &a.target.ty);

                // `v[i] = x` on a Vec is `v.set(i, x)`. A Vec's value is a
                // pointer to its {data, len, cap} header, so storing through a
                // GetPtr against it would overwrite the header rather than an
                // element — the write-side twin of the read bug above.
                if let TypedExprKind::Index(base, idx) = &a.target.kind {
                    if super::lower_expr::is_vec(&base.ty) {
                        let base_val = self.lower_expr(base);
                        let idx_val = self.lower_expr(idx);
                        let elem_ty = IRType::from_mani(&a.target.ty);
                        let val = if let Some(op) = &a.op {
                            // Compound assignment: get, apply, set.
                            let cur = self.fresh_temp();
                            self.emit(IRInstr::Call {
                                dst: Some(cur.clone()),
                                func: "Vec::get".to_string(),
                                args: vec![base_val.clone(), idx_val.clone()],
                                ret_ty: elem_ty.clone(),
                            });
                            let result_t = self.fresh_temp();
                            self.emit(IRInstr::BinOp {
                                dst: result_t.clone(),
                                // Same type the `ty` below is built from — see
                                // the note at the scalar compound-assign.
                                op: binop_to_ir(op, &a.value.ty, self.lang),
                                lhs: IRValue::Temp(cur),
                                rhs: val,
                                ty: elem_ty,
                            });
                            IRValue::Temp(result_t)
                        } else {
                            val
                        };
                        self.emit(IRInstr::Call {
                            dst: None,
                            func: "Vec::set".to_string(),
                            args: vec![base_val, idx_val, val],
                            ret_ty: IRType::Void,
                        });
                        return IRValue::Void;
                    }
                }

                // `x = <struct>` where `x` is an existing struct variable must
                // COPY the fields into x's storage — the same value semantics
                // `let b = a;` gets above.
                //
                // A struct local's storage IS its pointer (the StructLit alloca
                // or the callee's sret buffer), so the generic path below
                // emitted `store ptr %new, ptr %x`, which writes the new
                // struct's ADDRESS into x's FIRST FIELD. For
                //     struct P { pub a: int, pub b: int }
                //     x = bump(x);
                // that made `x.a` come back as a pointer — 94229577503440 on
                // LLVM, moving with ASLR between runs — while `x.b` kept its
                // old value forever, because the freshly built struct was
                // discarded and nothing ever read it. Both fields wrong, both
                // backends, for the plainest assignment in the language.
                //
                // The storage is copied INTO rather than rebound: an assignment
                // can sit inside a loop or a branch, where rebinding the local
                // to a new temp would leave later blocks reading a pointer that
                // only one path defines.
                if a.op.is_none() {
                    if let IRType::Struct(sname) = IRType::from_mani(&a.target.ty) {
                        if self.is_real_struct(&sname) {
                            if let TypedExprKind::Ident(name) = &a.target.kind {
                                if let Some((dst, _)) = self.locals.get(name).cloned() {
                                    let n = self.struct_nfields(&sname);
                                    self.emit_struct_copy(val, dst, n);
                                    return IRValue::Void;
                                }
                            }
                        }
                    }
                }

                let ptr = self.lower_expr_as_ptr(&a.target);
                // Struct/tuple fields use the uniform 8-byte slot convention,
                // so field stores/loads must use the slot access width.
                let ty = if matches!(&a.target.kind, TypedExprKind::Field(_, _)) {
                    super::helpers::slot_access_ty(&IRType::from_mani(&a.value.ty))
                } else {
                    IRType::from_mani(&a.value.ty)
                };
                if let Some(op) = &a.op {
                    // Compound assignment: load + op + store
                    let load_t = self.fresh_temp();
                    self.emit(IRInstr::Load {
                        dst: load_t.clone(),
                        ptr: ptr.clone(),
                        ty: ty.clone(),
                    });
                    // `a.value.ty`, deliberately, because that is what `ty`
                    // above is built from and the two MUST agree.
                    //
                    // From C4 onward the operator is type-dependent: `DivNear`
                    // and the `*T27` family are integer-only and have no float
                    // encoding on either backend. Choosing the operator from
                    // one type while typing the instruction from another gives
                    // a `DivNear` with `ty = F64`, which is not an instruction
                    // — the `debug_assert!`s in both emitters exist to say so
                    // loudly rather than let it become `sdiv double`.
                    //
                    // Note what this does NOT fix. A compound assignment types
                    // itself from the ASSIGNED VALUE rather than the target,
                    // so `let mut f: float = 7.0; f /= n;` with `n: int` loads
                    // the double as an i64 and divides it as one. That is
                    // report.txt P10, it predates all of this (measured on the
                    // archived pre-C4 release binary), and it is a semantic-
                    // pass strictness question — which R5 says must not be
                    // landed incidentally alongside a feature.
                    let binop = binop_to_ir(op, &a.value.ty, self.lang);
                    let result_t = self.fresh_temp();
                    self.emit(IRInstr::BinOp {
                        dst: result_t.clone(),
                        op: binop,
                        lhs: IRValue::Temp(load_t),
                        rhs: val,
                        ty: ty.clone(),
                    });
                    self.emit(IRInstr::Store {
                        ptr,
                        val: IRValue::Temp(result_t),
                        ty,
                    });
                } else {
                    self.emit(IRInstr::Store { ptr, val, ty });
                }
                IRValue::Void
            }
            TypedStmt::Expr(e) => self.lower_expr(e),
            TypedStmt::Return(e) => {
                let ret_mani = self.current_fn_ret.clone();
                let val = e.as_ref().map(|expr| {
                    let v = self.lower_expr(expr);
                    self.coerce_value(v, &expr.ty, &ret_mani)
                });
                self.set_term(IRTerminator::Return(val));
                // Start a new unreachable block for any instructions after return
                let after = self.fresh_label("after_return");
                let after_idx = self.new_block(after);
                self.switch_to(after_idx);
                IRValue::Void
            }
            TypedStmt::Break => {
                // Break handled by loop lowering context — emit jump to placeholder
                self.set_term(IRTerminator::Jump("__break__".to_string()));
                let after = self.fresh_label("after_break");
                let after_idx = self.new_block(after);
                self.switch_to(after_idx);
                IRValue::Void
            }
            TypedStmt::Continue => {
                self.set_term(IRTerminator::Jump("__continue__".to_string()));
                let after = self.fresh_label("after_continue");
                let after_idx = self.new_block(after);
                self.switch_to(after_idx);
                IRValue::Void
            }
        }
    }
}
