// semantic/analyzer/expressions.rs — check_expr method for SemanticAnalyzer.

use super::*;

impl SemanticAnalyzer {
    pub(crate) fn check_expr(&mut self, expr: &Expr, hint: Option<&ManiType>) -> CompileResult<TypedExpr> {
        let span = expr.span();
        match expr {
            Expr::Lit(lit, _) => {
                let ty = self.infer_lit_type(lit, hint);
                Ok(TypedExpr { kind: TypedExprKind::Lit(lit.clone()), ty, span })
            }

            Expr::Ident(name, _) => {
                // Track variable reads for unused-variable detection
                self.read_vars.insert(name.clone());
                let ty = if let Some(sym) = self.symbols.lookup(name) {
                    sym.ty.clone()
                } else if let Some((_, ret)) = self.functions.get(name) {
                    ret.clone()
                } else if let Some(sep) = name.find("::") {
                    // Check if this looks like EnumName::Variant
                    let enum_name = &name[..sep];
                    let variant_name = &name[sep + 2..];
                    if self.enums.contains_key(enum_name)
                        && self.enums[enum_name].iter().any(|(v, _)| v == variant_name)
                    {
                        ManiType::Enum(enum_name.to_string())
                    } else {
                        ManiType::Unknown
                    }
                } else {
                    // Allow unknown identifiers for now (stdlib etc.)
                    // Emit a warning for likely typos / genuinely unknown names
                    if !name.contains("::") && !name.contains('<') {
                        let var_names = self.symbols.all_names();
                        let fn_names = self.functions.keys().cloned();
                        let candidates = var_names.chain(fn_names);
                        let msg = if let Some(hint) = did_you_mean(name, candidates) {
                            format!("unknown identifier '{}'{}", name, hint)
                        } else {
                            format!("unknown identifier '{}' — type inferred as Unknown", name)
                        };
                        self.warnings.push(CompileWarning::new(
                            WarningKind::UnknownType,
                            &self.file, span.line, span.col,
                            msg,
                        ));
                    }
                    ManiType::Unknown
                };
                Ok(TypedExpr { kind: TypedExprKind::Ident(name.clone()), ty, span })
            }

            Expr::BinOp(lhs, op, rhs, _) => {
                let tlhs = self.check_expr(lhs, None)?;
                let trhs = self.check_expr(rhs, Some(&tlhs.ty))?;
                let ty = self.binop_type(op, &tlhs.ty, &trhs.ty, span)?;

                // Division by zero detection
                if matches!(op, BinOpKind::Div | BinOpKind::Rem) {
                    if let TypedExprKind::Lit(Lit::Int(0)) = &trhs.kind {
                        self.warnings.push(CompileWarning::new(
                            WarningKind::DivisionByZero,
                            &self.file, span.line, span.col,
                            "division by zero",
                        ));
                    } else if let TypedExprKind::Lit(Lit::Float(f)) = &trhs.kind {
                        if *f == 0.0 {
                            self.warnings.push(CompileWarning::new(
                                WarningKind::DivisionByZero,
                                &self.file, span.line, span.col,
                                "division by zero",
                            ));
                        }
                    }
                }

                Ok(TypedExpr {
                    kind: TypedExprKind::BinOp(Box::new(tlhs), op.clone(), Box::new(trhs)),
                    ty,
                    span,
                })
            }

            Expr::UnOp(op, operand, _) => {
                let te = self.check_expr(operand, hint)?;
                let ty = self.unop_type(op, &te.ty, span)?;
                Ok(TypedExpr {
                    kind: TypedExprKind::UnOp(op.clone(), Box::new(te)),
                    ty,
                    span,
                })
            }

            Expr::Call(callee, args, _) => {
                let tcallee = self.check_expr(callee, None)?;
                // Try to resolve function name or fn-type for type lookup
                let (param_tys, ret_ty) = match &tcallee.ty {
                    ManiType::Fn(pts, rt) => (pts.clone(), *rt.clone()),
                    _ => match &tcallee.kind {
                        TypedExprKind::Ident(name) => {
                            if let Some((pts, rt)) = self.functions.get(name) {
                                (pts.clone(), rt.clone())
                            } else {
                                (vec![], ManiType::Unknown)
                            }
                        }
                        _ => (vec![], ManiType::Unknown),
                    },
                };
                let mut typed_args = Vec::new();
                for (i, arg) in args.iter().enumerate() {
                    let hint = param_tys.get(i).unwrap_or(&ManiType::Unknown);
                    typed_args.push(self.check_expr(arg, Some(hint))?);
                }
                Ok(TypedExpr {
                    kind: TypedExprKind::Call(Box::new(tcallee), typed_args),
                    ty: ret_ty,
                    span,
                })
            }

            Expr::MethodCall(obj, method, args, _) => {
                let tobj = self.check_expr(obj, None)?;
                let mut typed_args = Vec::new();
                for arg in args {
                    typed_args.push(self.check_expr(arg, None)?);
                }
                // For method calls, we do basic resolution
                let ret_ty = self.resolve_method_type(&tobj.ty, method);
                Ok(TypedExpr {
                    kind: TypedExprKind::MethodCall(Box::new(tobj), method.clone(), typed_args),
                    ty: ret_ty,
                    span,
                })
            }

            Expr::Index(arr, idx, _) => {
                let tarr = self.check_expr(arr, None)?;
                let tidx = self.check_expr(idx, Some(&ManiType::Int))?;
                let elem_ty = match &tarr.ty {
                    ManiType::Array(inner, _) => *inner.clone(),
                    ManiType::Generic(name, args) if name == "Vec" => {
                        args.first().cloned().unwrap_or(ManiType::Unknown)
                    }
                    _ => ManiType::Unknown,
                };
                Ok(TypedExpr {
                    kind: TypedExprKind::Index(Box::new(tarr), Box::new(tidx)),
                    ty: elem_ty,
                    span,
                })
            }

            Expr::Field(obj, field, _) => {
                let tobj = self.check_expr(obj, None)?;
                // Feature 4: pub visibility enforcement
                if let ManiType::Struct(struct_name) = &tobj.ty {
                    if let Some(pub_fields) = self.struct_pub_fields.get(struct_name.as_str()) {
                        if let Some((_, is_pub)) = pub_fields.iter().find(|(n, _)| n == field) {
                            if !is_pub {
                                // Check if we are inside an impl block for this type
                                let inside_impl = self.current_impl_type.as_deref()
                                    == Some(struct_name.as_str());
                                if !inside_impl {
                                    return Err(self.err(
                                        span,
                                        format!(
                                            "field '{}' of type '{}' is private",
                                            field, struct_name
                                        ),
                                    ));
                                }
                            }
                        }
                    }
                }
                let field_ty = self.resolve_field_type(&tobj.ty, field);
                Ok(TypedExpr {
                    kind: TypedExprKind::Field(Box::new(tobj), field.clone()),
                    ty: field_ty,
                    span,
                })
            }

            Expr::Block(block) => {
                let tb = self.check_block(block)?;
                let ty = tb.ty.clone();
                Ok(TypedExpr { kind: TypedExprKind::Block(tb), ty, span })
            }

            Expr::If(ie) => {
                let tcond = self.check_expr(&ie.cond, Some(&ManiType::Bool))?;
                let tthen = self.check_block(&ie.then_block)?;
                let mut telif = Vec::new();
                for (econd, eblock) in &ie.elif_branches {
                    let tec = self.check_expr(econd, Some(&ManiType::Bool))?;
                    let teb = self.check_block(eblock)?;
                    telif.push((tec, teb));
                }
                let telse = if let Some(eb) = &ie.else_block {
                    Some(self.check_block(eb)?)
                } else {
                    None
                };
                let ty = telse.as_ref().map(|b| b.ty.clone()).unwrap_or(ManiType::Void);
                Ok(TypedExpr {
                    kind: TypedExprKind::If(TypedIfExpr {
                        cond: Box::new(tcond),
                        then_block: tthen,
                        elif_branches: telif,
                        else_block: telse,
                    }),
                    ty,
                    span,
                })
            }

            Expr::Tif(te) => {
                let tcond = self.check_expr(&te.cond, Some(&ManiType::Trit))?;
                // tif condition must be trit or bool3 — enforce it explicitly
                if tcond.ty != ManiType::Trit && tcond.ty != ManiType::Bool3 {
                    return Err(self.err(
                        te.cond.span(),
                        format!(
                            "tif condition must be `trit` or `bool3`, found `{:?}`",
                            tcond.ty
                        ),
                    ));
                }
                let tpos = self.check_block(&te.pos_block)?;
                let tzero = self.check_block(&te.zero_block)?;
                let tneg = self.check_block(&te.neg_block)?;
                let ty = tpos.ty.clone();
                Ok(TypedExpr {
                    kind: TypedExprKind::Tif(TypedTifExpr {
                        cond: Box::new(tcond),
                        pos_block: tpos,
                        zero_block: tzero,
                        neg_block: tneg,
                    }),
                    ty,
                    span,
                })
            }

            Expr::Match(me) => {
                let tscrutinee = self.check_expr(&me.scrutinee, None)?;
                let mut typed_arms = Vec::new();
                let mut arm_ty = ManiType::Void;
                for arm in &me.arms {
                    let guard = if let Some(g) = &arm.guard {
                        Some(self.check_expr(g, Some(&ManiType::Bool))?)
                    } else {
                        None
                    };
                    let tbody = self.check_expr(&arm.body, hint)?;
                    arm_ty = tbody.ty.clone();
                    typed_arms.push(TypedMatchArm {
                        pattern: arm.pattern.clone(),
                        guard,
                        body: tbody,
                    });
                }
                // Feature 1: exhaustiveness checking for enum, trit, bool3 types
                self.check_exhaustiveness(&tscrutinee.ty, &typed_arms, span)?;
                Ok(TypedExpr {
                    kind: TypedExprKind::Match(TypedMatchExpr {
                        scrutinee: Box::new(tscrutinee),
                        arms: typed_arms,
                    }),
                    ty: arm_ty,
                    span,
                })
            }

            Expr::For(fe) => {
                let titer = self.check_expr(&fe.iter, None)?;
                let elem_ty = self.iter_elem_type(&titer.ty);
                self.symbols.push_scope();
                self.symbols.define(&fe.var, elem_ty, false);
                let tbody = self.check_block(&fe.body)?;
                self.symbols.pop_scope();
                Ok(TypedExpr {
                    kind: TypedExprKind::For(TypedForExpr {
                        var: fe.var.clone(),
                        iter: Box::new(titer),
                        body: tbody,
                    }),
                    ty: ManiType::Void,
                    span,
                })
            }

            Expr::While(we) => {
                let tcond = self.check_expr(&we.cond, Some(&ManiType::Bool))?;
                let tbody = self.check_block(&we.body)?;
                Ok(TypedExpr {
                    kind: TypedExprKind::While(TypedWhileExpr {
                        cond: Box::new(tcond),
                        body: tbody,
                    }),
                    ty: ManiType::Void,
                    span,
                })
            }

            Expr::Loop(block, _) => {
                let tbody = self.check_block(block)?;
                Ok(TypedExpr {
                    kind: TypedExprKind::Loop(tbody),
                    ty: ManiType::Void,
                    span,
                })
            }

            Expr::Array(elems, _) => {
                // Derive element type hint from outer array hint
                let outer_elem_hint: Option<ManiType> = match hint {
                    Some(ManiType::Array(ref et, _)) => Some(*et.clone()),
                    _ => None,
                };
                let mut typed_elems = Vec::new();
                let mut elem_ty = outer_elem_hint.clone().unwrap_or(ManiType::Unknown);
                for e in elems {
                    let eh = outer_elem_hint.as_ref().or(if elem_ty == ManiType::Unknown { None } else { Some(&elem_ty) });
                    let te = self.check_expr(e, eh)?;
                    if elem_ty == ManiType::Unknown {
                        elem_ty = te.ty.clone();
                    }
                    typed_elems.push(te);
                }
                let arr_ty = ManiType::Array(Box::new(elem_ty), Some(typed_elems.len()));
                Ok(TypedExpr {
                    kind: TypedExprKind::Array(typed_elems),
                    ty: arr_ty,
                    span,
                })
            }

            Expr::Tuple(elems, _) => {
                let mut typed_elems = Vec::new();
                for e in elems {
                    typed_elems.push(self.check_expr(e, None)?);
                }
                let tys = typed_elems.iter().map(|e| e.ty.clone()).collect();
                Ok(TypedExpr {
                    kind: TypedExprKind::Tuple(typed_elems),
                    ty: ManiType::Tuple(tys),
                    span,
                })
            }

            Expr::StructLit(name, fields, _) => {
                let mut typed_fields = Vec::new();
                let struct_fields = self.structs.get(name).cloned().unwrap_or_default();

                // --- Struct update syntax: { ..base_expr, field: val, ... } ---
                // The parser encodes the base as a sentinel ("__spread__", base_expr).
                // Expand: for every struct field NOT listed in the explicit overrides,
                // synthesise `base_expr.field_name` as the value.
                let spread_expr = fields.iter().find(|(n, _)| n == "__spread__")
                    .map(|(_, e)| e.clone());
                let _explicit_names: std::collections::HashSet<&str> = fields
                    .iter()
                    .filter(|(n, _)| n != "__spread__")
                    .map(|(n, _)| n.as_str())
                    .collect();

                if let Some(base_expr) = spread_expr {
                    // Type-check the base expression (must be the same struct type).
                    let typed_base = self.check_expr(&base_expr, Some(&ManiType::Struct(name.clone())))?;

                    // Build a map of explicit overrides for quick lookup.
                    let mut overrides: std::collections::HashMap<String, Expr> =
                        std::collections::HashMap::new();
                    for (fname, fval) in fields.iter().filter(|(n, _)| n != "__spread__") {
                        overrides.insert(fname.clone(), fval.clone());
                    }

                    // Emit fields IN STRUCT DEFINITION ORDER so the IR lowering
                    // (which assigns by position) gets the right slot for each value.
                    for (sfield_name, sfield_ty) in &struct_fields {
                        if let Some(fval) = overrides.get(sfield_name.as_str()) {
                            // Explicit override for this field.
                            let tval = self.check_expr(fval, Some(sfield_ty))?;
                            typed_fields.push((sfield_name.clone(), tval));
                        } else {
                            // Inherit from base: base.field_name
                            let field_access = TypedExpr {
                                kind: TypedExprKind::Field(
                                    Box::new(typed_base.clone()),
                                    sfield_name.clone(),
                                ),
                                ty: sfield_ty.clone(),
                                span,
                            };
                            typed_fields.push((sfield_name.clone(), field_access));
                        }
                    }
                } else {
                    // Normal struct literal — no spread.
                    for (fname, fval) in fields {
                        let field_ty = struct_fields
                            .iter()
                            .find(|(n, _)| n == fname)
                            .map(|(_, t)| t.clone());
                        let tval = self.check_expr(fval, field_ty.as_ref())?;
                        typed_fields.push((fname.clone(), tval));
                    }
                }

                Ok(TypedExpr {
                    kind: TypedExprKind::StructLit(name.clone(), typed_fields),
                    ty: ManiType::Struct(name.clone()),
                    span,
                })
            }

            Expr::Range(lo, hi, inclusive, _) => {
                let tlo = self.check_expr(lo, Some(&ManiType::Int))?;
                let thi = self.check_expr(hi, Some(&ManiType::Int))?;
                Ok(TypedExpr {
                    kind: TypedExprKind::Range(Box::new(tlo), Box::new(thi), *inclusive),
                    ty: ManiType::Generic("Range".to_string(), vec![ManiType::Int]),
                    span,
                })
            }

            Expr::Cast(inner, ty, _) => {
                let tinner = self.check_expr(inner, None)?;
                let cast_ty = self.resolve_type(ty)?;
                Ok(TypedExpr {
                    kind: TypedExprKind::Cast(Box::new(tinner), cast_ty.clone()),
                    ty: cast_ty,
                    span,
                })
            }

            Expr::Question(inner, _) => {
                let tinner = self.check_expr(inner, hint)?;
                let unwrapped_ty = match &tinner.ty {
                    ManiType::Generic(name, args) if name == "Result" || name == "Option" => {
                        args.first().cloned().unwrap_or(ManiType::Unknown)
                    }
                    other => other.clone(),
                };
                Ok(TypedExpr {
                    kind: TypedExprKind::Question(Box::new(tinner)),
                    ty: unwrapped_ty,
                    span,
                })
            }

            Expr::Return(e, _) => {
                let ret_hint = self.current_fn_ret.clone();
                let te = self.check_expr(e, Some(&ret_hint))?;
                Ok(TypedExpr {
                    kind: TypedExprKind::Return(Box::new(te)),
                    ty: ManiType::Void,
                    span,
                })
            }

            Expr::Break(_) => Ok(TypedExpr {
                kind: TypedExprKind::Break,
                ty: ManiType::Void,
                span,
            }),

            Expr::Continue(_) => Ok(TypedExpr {
                kind: TypedExprKind::Continue,
                ty: ManiType::Void,
                span,
            }),

            Expr::Await(inner, _) => {
                let tinner = self.check_expr(inner, hint)?;
                let ty = tinner.ty.clone();
                Ok(TypedExpr {
                    kind: TypedExprKind::Await(Box::new(tinner)),
                    ty,
                    span,
                })
            }

            Expr::Spawn(block, _) => {
                let tb = self.check_block(block)?;
                Ok(TypedExpr {
                    kind: TypedExprKind::Spawn(tb),
                    ty: ManiType::Void,
                    span,
                })
            }

            Expr::Lambda(params, ret_ty_opt, body, _) => {
                // Generate a unique global function name for this lambda
                let name = format!("__lambda_{}", self.lambda_counter);
                self.lambda_counter += 1;

                // Resolve param types
                let mut param_mani_tys = Vec::new();
                let mut typed_params = Vec::new();
                let mut param_names = std::collections::HashSet::new();
                for (pname, pty) in params {
                    let mty = self.resolve_type(pty)?;
                    param_mani_tys.push(mty.clone());
                    typed_params.push(TypedParam { name: pname.clone(), ty: mty });
                    param_names.insert(pname.clone());
                }
                let ret_mani = if let Some(rt) = ret_ty_opt {
                    self.resolve_type(rt)?
                } else {
                    ManiType::Unknown
                };

                // Capture the outer scope names before pushing the lambda scope.
                // Any read of an outer-scope variable that is not a lambda param is a
                // closure capture — not supported; emit a compile error.
                let outer_names: std::collections::HashSet<String> =
                    self.symbols.all_names().collect();

                // Type-check the body with a fresh scope containing only the params
                let read_before: std::collections::HashSet<String> = self.read_vars.clone();
                self.symbols.push_scope();
                for tp in &typed_params {
                    self.symbols.define(&tp.name, tp.ty.clone(), false);
                }
                let prev_ret = self.current_fn_ret.clone();
                self.current_fn_ret = ret_mani.clone();
                let tbody = self.check_expr(body, Some(&ret_mani))?;
                self.current_fn_ret = prev_ret;
                self.symbols.pop_scope();

                // Find any variables read during body-check that are outer captures
                let newly_read: std::collections::HashSet<String> =
                    self.read_vars.difference(&read_before).cloned().collect();
                for v in &newly_read {
                    if !param_names.contains(v) && outer_names.contains(v) {
                        return Err(self.err(
                            span,
                            format!(
                                "lambda captures outer variable '{}' — closures are not yet supported; use a parameter instead",
                                v
                            ),
                        ));
                    }
                }

                // Wrap body in a block that returns the expression
                let body_block = TypedBlock {
                    stmts: vec![TypedStmt::Return(Some(tbody))],
                    ty: ret_mani.clone(),
                };

                // Register as a known function
                self.functions.insert(name.clone(), (param_mani_tys.clone(), ret_mani.clone()));

                // Create TypedFnDef and store for later
                let lambda_fn = TypedFnDef {
                    name: name.clone(),
                    params: typed_params,
                    ret_ty: ret_mani.clone(),
                    body: Some(body_block),
                    is_pub: false,
                    is_async: false,
                };
                self.lambda_fns.push(lambda_fn);

                // The lambda expression evaluates to the function's label (as an int/ptr)
                let fn_ty = ManiType::Fn(param_mani_tys, Box::new(ret_mani));
                Ok(TypedExpr {
                    kind: TypedExprKind::Ident(name),
                    ty: fn_ty,
                    span,
                })
            }

            Expr::Tresult(tr) => {
                let texpr = self.check_expr(&tr.expr, None)?;
                let expr_ty = texpr.ty.clone();

                // Each arm gets a scope with its binding variable
                self.symbols.push_scope();
                self.symbols.define(&tr.ok_var, expr_ty.clone(), false);
                let ok_block = self.check_block(&tr.ok_block)?;
                self.symbols.pop_scope();

                self.symbols.push_scope();
                self.symbols.define(&tr.unknown_var, expr_ty.clone(), false);
                let unknown_block = self.check_block(&tr.unknown_block)?;
                self.symbols.pop_scope();

                self.symbols.push_scope();
                self.symbols.define(&tr.err_var, expr_ty.clone(), false);
                let err_block = self.check_block(&tr.err_block)?;
                self.symbols.pop_scope();

                let ty = ok_block.ty.clone();
                Ok(TypedExpr {
                    kind: TypedExprKind::Tresult(TypedTresultExpr {
                        expr: Box::new(texpr),
                        ok_var: tr.ok_var.clone(),
                        ok_block,
                        unknown_var: tr.unknown_var.clone(),
                        unknown_block,
                        err_var: tr.err_var.clone(),
                        err_block,
                    }),
                    ty,
                    span,
                })
            }
        }
    }
}
