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
                } else if let Some((params, ret)) = self.functions.get(name) {
                    // A bare function name in a context expecting a fn type is
                    // a function reference; otherwise keep the legacy view
                    // (the function's return type).
                    if matches!(hint, Some(ManiType::Fn(_, _))) {
                        ManiType::Fn(params.clone(), Box::new(ret.clone()))
                    } else {
                        ret.clone()
                    }
                } else if let Some(enum_name) = self.enum_variant_path(name) {
                    // EnumName::Variant
                    ManiType::Enum(enum_name)
                } else if let Some(pos) = name.rfind("::") {
                    // Unresolved `::` path.
                    // S11: referencing a private item of a loaded module is a
                    // hard error mentioning privacy.
                    if let Some(mod_prefix) = self.module_private_items.get(name.as_str()) {
                        return Err(self.err(span, format!(
                            "'{}' is private in module '{}' — mark it `pub` to make it importable",
                            name, mod_prefix
                        )));
                    }
                    // S12: warn for unknown `::` paths (still permissively
                    // typed Unknown, matching the crate's design).
                    let prefix = &name[..pos];
                    let item = &name[pos + 2..];
                    let bare_mod = prefix.strip_prefix("std::").unwrap_or(prefix);
                    if let Some(members) = super::std_module_members(bare_mod) {
                        if !members.contains(item) {
                            let hint = did_you_mean(item, members.iter().cloned())
                                .unwrap_or_default();
                            self.warnings.push(CompileWarning::new(
                                WarningKind::UnknownType,
                                &self.file, span.line, span.col,
                                format!("std module '{}' has no item '{}'{}", bare_mod, item, hint),
                            ));
                        }
                    } else if self.loaded_module_prefixes.contains(prefix) {
                        let mod_prefix = format!("{}::", prefix);
                        let candidates = self.functions.keys()
                            .chain(self.structs.keys())
                            .chain(self.enums.keys())
                            .filter(|n| n.starts_with(&mod_prefix))
                            .map(|n| n[mod_prefix.len()..].to_string());
                        let hint = did_you_mean(item, candidates).unwrap_or_default();
                        self.warnings.push(CompileWarning::new(
                            WarningKind::UnknownType,
                            &self.file, span.line, span.col,
                            format!("module '{}' has no item '{}'{}", prefix, item, hint),
                        ));
                    } else {
                        let first = &name[..name.find("::").unwrap_or(name.len())];
                        // Namespaces the type checker cannot enumerate (builtin
                        // generic types, structs/enums with impls, generics).
                        const BUILTIN_NAMESPACES: &[&str] = &[
                            "Vec", "Map", "Set", "Deque", "TernaryTrie", "Channel",
                            "Mutex", "MutexGuard", "AtomicTrit", "Barrier", "Semaphore",
                            "Task", "Result", "Pair", "Range", "String", "str",
                            "Self", "std",
                        ];
                        if self.enums.contains_key(first) {
                            self.warnings.push(CompileWarning::new(
                                WarningKind::UnknownType,
                                &self.file, span.line, span.col,
                                format!("enum '{}' has no variant '{}'", first, item),
                            ));
                        } else if !BUILTIN_NAMESPACES.contains(&first)
                            && !self.structs.contains_key(first)
                            && !self.user_method_types.contains_key(first)
                            && !self.type_params.contains_key(first)
                            && !Self::STDLIB_MODULES.contains(&first)
                        {
                            self.warnings.push(CompileWarning::new(
                                WarningKind::UnknownType,
                                &self.file, span.line, span.col,
                                format!("unknown module or type '{}' in path '{}'", first, name),
                            ));
                        }
                    }
                    ManiType::Unknown
                } else {
                    // `Some` and `None` are refused outright rather than
                    // warned about. They used to sit in the list below, which
                    // silenced the unknown-identifier warning and let them reach
                    // codegen, where they failed at assembly ("Undefined label:
                    // Some"). See the `Option` arm of `resolve_type`: Result is
                    // this language's option type, and it has a third outcome
                    // that Option cannot express.
                    if name == "Some" || name == "None" {
                        return Err(self.err(span, format!(
                            "`{}` is not a ManiT constructor. `Result<T, E>` is this \
                             language's option type and it has three outcomes rather \
                             than two: `Ok(v)` for a value, `Unknown(msg)` where you \
                             would write `None`, and `Err(e)` for a failure.",
                            name,
                        )));
                    }
                    // Result constructors are handled structurally by the
                    // lowering — never warn for them.
                    const RESULT_CONSTRUCTORS: &[&str] =
                        &["Ok", "Err", "Unknown"];
                    // Allow unknown identifiers for now (stdlib etc.)
                    // Emit a warning for likely typos / genuinely unknown names
                    if !name.contains('<') && !RESULT_CONSTRUCTORS.contains(&name.as_str()) {
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
                // Operand hints: ternary-logic operators expect trit operands
                // (so bare 0/1/-1 literals coerce), logical operators expect bool.
                let operand_hint = match op {
                    BinOpKind::Tand | BinOpKind::Tor | BinOpKind::Txor
                    | BinOpKind::Tcon | BinOpKind::Tany => Some(ManiType::Trit),
                    BinOpKind::And | BinOpKind::Or => Some(ManiType::Bool),
                    _ => None,
                };
                let tlhs = self.check_expr(lhs, operand_hint.as_ref())?;
                let rhs_hint = if operand_hint.is_some() && tlhs.ty == ManiType::Bool3 {
                    ManiType::Bool3
                } else if operand_hint.is_some() {
                    operand_hint.clone().unwrap()
                } else {
                    tlhs.ty.clone()
                };
                let trhs = self.check_expr(rhs, Some(&rhs_hint))?;
                let ty = self.binop_type(op, &tlhs.ty, &trhs.ty, span)?;

                // Division by zero detection
                if matches!(op, BinOpKind::Div | BinOpKind::Rem) {
                    if let TypedExprKind::Lit(Lit::Int(0)) = &trhs.kind {
                        // A7: integer division by a literal zero has no
                        // meaningful result — T3 traps, LLVM raises SIGFPE.
                        // It cannot be intentional, so reject it rather than
                        // warn. (Float /0.0 stays a warning below: IEEE
                        // division by zero is defined and yields inf/nan.)
                        return Err(self.err(
                            span,
                            format!(
                                "division by zero: the right operand of `{}` is the \
                                 literal 0",
                                if matches!(op, BinOpKind::Div) { "/" } else { "%" },
                            ),
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
                // Try to resolve function name or fn-type for type lookup.
                // `enforce` is set only when the signature is trustworthy:
                // fn-typed values and user-defined functions always are; builtin
                // registry entries only when fully specified (no Unknown params —
                // e.g. println / fmt::format are variadic-ish placeholders).
                let mut enforce = false;
                let mut display_name = String::from("<fn>");
                let (param_tys, ret_ty) = match &tcallee.ty {
                    ManiType::Fn(pts, rt) => {
                        enforce = true;
                        if let TypedExprKind::Ident(n) = &tcallee.kind {
                            display_name = n.clone();
                        }
                        (pts.clone(), *rt.clone())
                    }
                    _ => match &tcallee.kind {
                        TypedExprKind::Ident(name) => {
                            if let Some((pts, rt)) = self.functions.get(name) {
                                display_name = name.clone();
                                enforce = !self.builtin_names.contains(name)
                                    || pts.iter().all(|t| t.is_known());
                                (pts.clone(), rt.clone())
                            } else {
                                (vec![], ManiType::Unknown)
                            }
                        }
                        _ => (vec![], ManiType::Unknown),
                    },
                };
                if enforce && args.len() != param_tys.len() {
                    return Err(self.err(span, format!(
                        "function '{}' expects {} argument(s), found {}",
                        display_name, param_tys.len(), args.len()
                    )));
                }
                let mut typed_args = Vec::new();
                for (i, arg) in args.iter().enumerate() {
                    let hint = param_tys.get(i).cloned().unwrap_or(ManiType::Unknown);
                    let targ = self.check_expr(arg, Some(&hint))?;
                    if enforce
                        && hint.is_known()
                        && targ.ty.is_known()
                        && !types_compatible(&hint, &targ.ty)
                    {
                        return Err(self.err(targ.span, format!(
                            "argument {} to '{}': expected `{}`, found `{}`",
                            i + 1, display_name, hint.display(), targ.ty.display()
                        )));
                    }
                    typed_args.push(targ);
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
                // A method on a `Result` must be one this compiler lowers.
                // `.map()` used to be typed here and emitted by neither backend
                // — the same silence as `.unwrap()` (Section 18): accepted by
                // semantic analysis, undefined at link.
                if crate::ir::lower::lower_result::is_result(&tobj.ty)
                    && !crate::ir::lower::lower_result::RESULT_METHODS.contains(&method.as_str())
                {
                    return Err(self.err(span, format!(
                        "`Result` has no method `{}`. Available: {}. Or take all three \
                         outcomes at once with `match r {{ Ok(v) => …, Unknown(m) => …, \
                         Err(e) => … }}`, or `tif r.tag() {{ + => …, 0 => …, - => … }}`.",
                        method,
                        crate::ir::lower::lower_result::RESULT_METHODS
                            .iter().map(|m| format!("`{}`", m))
                            .collect::<Vec<_>>().join(", "),
                    )));
                }
                // For method calls, we do basic resolution
                let ret_ty = self.resolve_method_type(&tobj.ty, method, span);
                Ok(TypedExpr {
                    kind: TypedExprKind::MethodCall(Box::new(tobj), method.clone(), typed_args),
                    ty: ret_ty,
                    span,
                })
            }

            Expr::Index(arr, idx, _) => {
                let tarr = self.check_expr(arr, None)?;
                let tidx = self.check_expr(idx, Some(&ManiType::Int))?;

                // A2: array indexing is otherwise entirely unchecked — the
                // LLVM backend segfaults on a far out-of-range index and the T3
                // backend reads adjacent emulator memory (a[-1] returned the
                // array's own length header). When both the length and the
                // index are statically known, reject it outright.
                // Any index the compiler can compute is checked here — `a[3]`,
                // `a[-1]`, `a[0 - 1]`, `a[2 * 5]` alike. A genuinely dynamic
                // index (`a[n]`) is left to the runtime guard emitted by IR
                // lowering. This used to be a private two-arm match that saw
                // literals and negated literals only; it now shares the one
                // folder with module-level constants, whose identical
                // literals-only match was a silent miscompile rather than
                // merely conservative (see semantic/const_fold.rs).
                let const_idx = crate::semantic::const_fold::fold_int(&tidx);
                if let (ManiType::Array(_, Some(len)), Some(i)) = (&tarr.ty, &const_idx) {
                    let len = *len as i64;
                    if *i < 0 || *i >= len {
                        return Err(self.err(
                            span,
                            format!(
                                "index {} is out of bounds for an array of length {} \
                                 (valid indices are 0..{})",
                                i, len, len.saturating_sub(1),
                            ),
                        ));
                    }
                }

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
                self.check_bool_cond(&tcond, "if condition")?;
                let tthen = self.check_block(&ie.then_block)?;
                let mut telif = Vec::new();
                for (econd, eblock) in &ie.elif_branches {
                    let tec = self.check_expr(econd, Some(&ManiType::Bool))?;
                    self.check_bool_cond(&tec, "elif condition")?;
                    let teb = self.check_block(eblock)?;
                    telif.push((tec, teb));
                }
                let telse = if let Some(eb) = &ie.else_block {
                    Some(self.check_block(eb)?)
                } else {
                    None
                };
                // S7: when the `if` can produce a value (it has an else), all
                // branches with known non-void types must agree.
                if let Some(eb) = &telse {
                    let mut parts = vec![tthen.ty.clone()];
                    parts.extend(telif.iter().map(|(_, b)| b.ty.clone()));
                    parts.push(eb.ty.clone());
                    self.check_branch_agreement(&parts, "if branches", span)?;
                }
                // An `if` with no `else` is a statement and has no value; with
                // one, its type comes from every branch rather than from the
                // `else` alone — see unify_branch_type.
                let ty = match &telse {
                    None => ManiType::Void,
                    Some(eb) => {
                        let mut arms: Vec<&TypedBlock> = vec![&tthen];
                        arms.extend(telif.iter().map(|(_, b)| b));
                        arms.push(eb);
                        Self::unify_branch_type(&arms, eb.ty.clone())
                    }
                };
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
                // tif condition must be trit or bool3 — enforce for KNOWN types;
                // Unknown (the permissive placeholder, e.g. generics) is allowed.
                if tcond.ty.is_known()
                    && tcond.ty != ManiType::Trit
                    && tcond.ty != ManiType::Bool3
                {
                    return Err(self.err(
                        te.cond.span(),
                        format!(
                            "tif condition must be `trit` or `bool3`, found `{}`",
                            tcond.ty.display()
                        ),
                    ));
                }
                let tpos = self.check_block(&te.pos_block)?;
                let tzero = self.check_block(&te.zero_block)?;
                let tneg = self.check_block(&te.neg_block)?;
                // S7: all three arms must agree when they produce values.
                self.check_branch_agreement(
                    &[tpos.ty.clone(), tzero.ty.clone(), tneg.ty.clone()],
                    "tif arms",
                    span,
                )?;
                let ty = Self::unify_branch_type(&[&tpos, &tzero, &tneg], tpos.ty.clone());
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
                let mut arm_tys = Vec::new();
                for arm in &me.arms {
                    // Pattern bindings (e.g. `Ok(v) => ...`) live in a per-arm
                    // scope and are visible to both the guard and the body.
                    self.symbols.push_scope();
                    self.define_pattern_bindings(&arm.pattern, &tscrutinee.ty);
                    let guard = if let Some(g) = &arm.guard {
                        let tg = self.check_expr(g, Some(&ManiType::Bool))?;
                        self.check_bool_cond(&tg, "match guard")?;
                        Some(tg)
                    } else {
                        None
                    };
                    let tbody = self.check_expr(&arm.body, hint)?;
                    self.symbols.pop_scope();
                    arm_ty = tbody.ty.clone();
                    arm_tys.push(tbody.ty.clone());
                    typed_arms.push(TypedMatchArm {
                        pattern: arm.pattern.clone(),
                        guard,
                        body: tbody,
                    });
                }
                // S7: arms with known non-void types must agree.
                self.check_branch_agreement(&arm_tys, "match arms", span)?;
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
                self.check_bool_cond(&tcond, "while condition")?;
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
                let mut outer_elem_hint: Option<ManiType> = match hint {
                    Some(ManiType::Array(ref et, _)) => Some(*et.clone()),
                    _ => None,
                };

                // With no usable hint, the element type is the type of the
                // whole literal, not of its first element. `[0, 0, +]` is a
                // trit array whose first two entries happen to be writable as
                // integer literals; typing it from element 0 made it an int
                // array, and the trit at index 2 was then stored one byte wide
                // into eight-byte slots. That mis-shaped array reached the
                // runtime helpers and segfaulted.
                //
                // Many stdlib signatures are still Unknown to the analyzer
                // (report.txt S3-S8), so "no usable hint" is the common case
                // for exactly the calls that care: pack_trits, trits_to_str.
                if outer_elem_hint.as_ref().is_none_or(|t| !t.is_known()) && !elems.is_empty() {
                    let mut probe: Option<ManiType> = None;
                    for e in elems {
                        let te = self.check_expr(e, None)?;
                        // A bare integer literal is the weakest claim on the
                        // element type: 0 is a legal trit, tryte, t9 and t27
                        // as well as an int. Anything more specific wins.
                        let specific = te.ty.is_ternary()
                            || matches!(te.ty, ManiType::Float | ManiType::Str | ManiType::Char);
                        if specific && probe.as_ref().is_none_or(|p| !p.is_ternary()) {
                            probe = Some(te.ty.clone());
                        } else if probe.is_none() {
                            probe = Some(te.ty.clone());
                        }
                    }
                    outer_elem_hint = probe.filter(|t| t.is_known());
                }

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
                // S1: an unknown struct name is an error, not a silent guess.
                let struct_fields = match self.structs.get(name) {
                    Some(f) => f.clone(),
                    None => {
                        let hint = did_you_mean(name, self.structs.keys().cloned())
                            .unwrap_or_default();
                        return Err(self.err(span, format!("unknown struct '{}'{}", name, hint)));
                    }
                };

                // --- Struct update syntax: { ..base_expr, field: val, ... } ---
                // The parser encodes the base as a sentinel ("__spread__", base_expr).
                // Expand: for every struct field NOT listed in the explicit overrides,
                // synthesise `base_expr.field_name` as the value.
                let spread_expr = fields.iter().find(|(n, _)| n == "__spread__")
                    .map(|(_, e)| e.clone());

                // S1: explicit field names must exist on the struct and be unique.
                let mut explicit_names: std::collections::HashSet<&str> =
                    std::collections::HashSet::new();
                for (fname, fval) in fields.iter().filter(|(n, _)| n != "__spread__") {
                    if !struct_fields.iter().any(|(n, _)| n == fname) {
                        let hint = did_you_mean(
                            fname,
                            struct_fields.iter().map(|(n, _)| n.clone()),
                        ).unwrap_or_default();
                        return Err(self.err(
                            fval.span(),
                            format!("struct '{}' has no field '{}'{}", name, fname, hint),
                        ));
                    }
                    if !explicit_names.insert(fname.as_str()) {
                        return Err(self.err(
                            fval.span(),
                            format!("field '{}' specified more than once in struct literal", fname),
                        ));
                    }
                }

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
                    // S1: every declared field must be provided.
                    let missing: Vec<&str> = struct_fields
                        .iter()
                        .filter(|(n, _)| !explicit_names.contains(n.as_str()))
                        .map(|(n, _)| n.as_str())
                        .collect();
                    if !missing.is_empty() {
                        return Err(self.err(span, format!(
                            "missing field{} {} in literal of struct '{}'",
                            if missing.len() == 1 { "" } else { "s" },
                            missing.iter().map(|n| format!("'{}'", n))
                                .collect::<Vec<_>>().join(", "),
                            name,
                        )));
                    }
                    // S1: emit fields IN STRUCT DECLARATION ORDER — the IR
                    // lowering assigns by position (same contract as the
                    // spread branch above), so source order must not leak.
                    for (sfname, sfty) in &struct_fields {
                        let fval = fields
                            .iter()
                            .find(|(n, _)| n == sfname)
                            .map(|(_, e)| e)
                            .expect("field presence verified above");
                        let tval = self.check_expr(fval, Some(sfty))?;
                        if sfty.is_known()
                            && tval.ty.is_known()
                            && !types_compatible(sfty, &tval.ty)
                        {
                            return Err(self.err(tval.span, format!(
                                "field '{}' of struct '{}' has type `{}`, found `{}`",
                                sfname, name, sfty.display(), tval.ty.display()
                            )));
                        }
                        typed_fields.push((sfname.clone(), tval));
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
                    ManiType::Generic(name, args) if name == "Result" => {
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

                // Closure-capture detection (S2): walk the lambda body's own AST
                // collecting free identifiers (reads not bound by the lambda's
                // params or its local declarations). Any free name that resolves
                // to an enclosing FUNCTION-LOCAL variable is a capture — not
                // supported. Module-level globals (scope depth 0) are fine.
                let mut lambda_bound: Vec<std::collections::HashSet<String>> =
                    vec![param_names.iter().cloned().collect()];
                let mut free_names: std::collections::HashSet<String> =
                    std::collections::HashSet::new();
                collect_free_idents(body, &mut lambda_bound, &mut free_names);
                let mut captured: Vec<&String> = free_names
                    .iter()
                    .filter(|v| {
                        matches!(self.symbols.lookup_with_depth(v), Some((depth, _)) if depth >= 1)
                    })
                    .collect();
                captured.sort();
                if let Some(v) = captured.first() {
                    return Err(self.err(
                        span,
                        format!(
                            "lambda captures outer variable '{}' — closures are not yet supported; use a parameter instead",
                            v
                        ),
                    ));
                }

                // Type-check the body with a fresh scope containing only the params
                self.symbols.push_scope();
                for tp in &typed_params {
                    self.symbols.define(&tp.name, tp.ty.clone(), false);
                }
                let prev_ret = self.current_fn_ret.clone();
                self.current_fn_ret = ret_mani.clone();
                let tbody = self.check_expr(body, Some(&ret_mani))?;
                self.current_fn_ret = prev_ret;
                self.symbols.pop_scope();

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

    // -----------------------------------------------------------------------
    // Condition / branch-type helpers
    // -----------------------------------------------------------------------

    /// If `name` is an `EnumName::Variant` path of a known enum, return the
    /// enum's name.
    fn enum_variant_path(&self, name: &str) -> Option<String> {
        let sep = name.find("::")?;
        let enum_name = &name[..sep];
        let variant_name = &name[sep + 2..];
        let variants = self.enums.get(enum_name)?;
        if variants.iter().any(|(v, _)| v == variant_name) {
            Some(enum_name.to_string())
        } else {
            None
        }
    }

    /// S8: enforce that a condition (if/elif/while, match guard) is `bool`
    /// when its type is KNOWN; `Unknown` stays permissive by design.
    pub(crate) fn check_bool_cond(&self, cond: &TypedExpr, what: &str) -> CompileResult<()> {
        if cond.ty.is_known() && cond.ty != ManiType::Bool {
            return Err(self.err(
                cond.span,
                format!("{} must be `bool`, found `{}`", what, cond.ty.display()),
            ));
        }
        Ok(())
    }

    /// S7: branches of a value-producing construct (if/match/tif) must agree.
    /// Branches typed `Void` (statement position or diverging via
    /// return/break/continue) and `Unknown` are tolerated; two KNOWN,
    /// non-void, incompatible branch types are an error.
    /// The result type of a multi-armed expression, chosen from ALL the arms.
    ///
    /// Each site used to take one arm and call it the answer: a `tif` took its
    /// first arm, an `if` took its `else`. That is arbitrary whenever the arms
    /// are compatible but not identical, and in a ternary language they very
    /// often are — a bare `0` is a valid `int` AND a valid `trit`, so which one
    /// it is depends on what sits beside it.
    ///
    /// It produced IR that would not assemble (ORACLE_FINDINGS.md Section 15):
    ///
    /// ```text
    /// tif i { + => +, 0 => +, - => 0 }   typed trit — first arm is a trit
    /// tif i { + => 0, 0 => -, - => - }   typed INT  — first arm is `0`
    /// ```
    ///
    /// Two spellings of the same three-valued function, typed differently
    /// because of which arm came first. Nested inside a `trit`-valued `tif`,
    /// the second fed an `i64` into an `i8` phi and clang rejected the module:
    /// `'%t8' defined with type 'i64' but expected 'i8'`. T3 compiled the same
    /// source correctly, since every value there is one word — so this is a
    /// fault the two-backend oracle DID see, and saw as a build failure rather
    /// than a wrong answer.
    ///
    /// The rule: if any arm has a ternary type, and every other arm is a bare
    /// integer literal that is also a valid trit, the whole expression is that
    /// ternary type. Otherwise `fallback`, which each caller supplies as the arm
    /// it used to take unconditionally — so this only ever CHANGES the answer in
    /// the mixed-ternary case, and every other program types exactly as before.
    ///
    /// A literal is required. An arm of `int` type that is not a literal keeps
    /// the expression `int`: narrowing a computed integer to a trit would turn a
    /// build failure into a wrong answer, which is the worse trade.
    pub(crate) fn unify_branch_type(arms: &[&TypedBlock], fallback: ManiType) -> ManiType {
        let mut ternary: Option<ManiType> = None;
        let mut others_all_trit_literals = true;

        for b in arms {
            if !b.ty.is_known() || b.ty == ManiType::Void {
                continue;
            }
            if b.ty.is_ternary() {
                if ternary.is_none() {
                    ternary = Some(b.ty.clone());
                }
            } else if !(b.ty == ManiType::Int && Self::tail_is_trit_literal(b)) {
                others_all_trit_literals = false;
            }
        }

        match ternary {
            Some(t) if others_all_trit_literals => t,
            _ => fallback,
        }
    }

    /// Does this block's value come from an integer literal that is also a
    /// valid trit — that is, one of -1, 0, +1?
    fn tail_is_trit_literal(b: &TypedBlock) -> bool {
        match b.stmts.last() {
            Some(TypedStmt::Expr(te)) => matches!(
                &te.kind,
                TypedExprKind::Lit(crate::ast::Lit::Int(n)) if (-1..=1).contains(n)
            ),
            _ => false,
        }
    }

    pub(crate) fn check_branch_agreement(
        &self,
        branch_tys: &[ManiType],
        what: &str,
        span: Span,
    ) -> CompileResult<()> {
        let mut first: Option<&ManiType> = None;
        for ty in branch_tys {
            if !ty.is_known() || *ty == ManiType::Void {
                continue;
            }
            match first {
                None => first = Some(ty),
                Some(fty) => {
                    if !types_compatible(fty, ty) {
                        return Err(self.err(span, format!(
                            "{} have incompatible types: `{}` vs `{}`",
                            what, fty.display(), ty.display()
                        )));
                    }
                }
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Free-identifier collection for lambda capture detection (S2)
// ---------------------------------------------------------------------------

fn bind_name(bound: &mut [std::collections::HashSet<String>], name: &str) {
    if let Some(scope) = bound.last_mut() {
        scope.insert(name.to_string());
    }
}

fn is_bound(bound: &[std::collections::HashSet<String>], name: &str) -> bool {
    bound.iter().any(|s| s.contains(name))
}

fn bind_pattern_names(pat: &Pattern, bound: &mut [std::collections::HashSet<String>]) {
    match pat {
        Pattern::Wildcard(_) | Pattern::Lit(_, _) => {}
        Pattern::Ident(n, _) => bind_name(bound, n),
        Pattern::Tuple(ps, _) | Pattern::Or(ps, _) | Pattern::Enum(_, _, ps, _) => {
            for p in ps {
                bind_pattern_names(p, bound);
            }
        }
        Pattern::Struct(_, fields, _) => {
            for (_, p) in fields {
                bind_pattern_names(p, bound);
            }
        }
    }
}

fn collect_free_in_block(
    block: &Block,
    bound: &mut Vec<std::collections::HashSet<String>>,
    free: &mut std::collections::HashSet<String>,
) {
    bound.push(Default::default());
    for stmt in &block.stmts {
        match stmt {
            Stmt::Let(ls) => {
                // The initialiser is evaluated BEFORE the binding exists.
                if let Some(init) = &ls.init {
                    collect_free_idents(init, bound, free);
                }
                match &ls.pat {
                    ast::LetPat::Ident(n) => bind_name(bound, n),
                    ast::LetPat::Tuple(names) => {
                        for n in names {
                            bind_name(bound, n);
                        }
                    }
                }
            }
            Stmt::Assign(a) => {
                collect_free_idents(&a.value, bound, free);
                collect_free_idents(&a.target, bound, free);
            }
            Stmt::Expr(e) => collect_free_idents(e, bound, free),
            Stmt::Return(Some(e), _) => collect_free_idents(e, bound, free),
            Stmt::Return(None, _) | Stmt::Break(_) | Stmt::Continue(_) => {}
            Stmt::LocalStructDef(_) => {}
        }
    }
    bound.pop();
}

/// Collect every identifier read by `expr` that is not bound within `expr`
/// itself (or by an enclosing `bound` scope). Path identifiers (`a::b`) are
/// constants/functions, never local variables — they are skipped.
fn collect_free_idents(
    expr: &Expr,
    bound: &mut Vec<std::collections::HashSet<String>>,
    free: &mut std::collections::HashSet<String>,
) {
    match expr {
        Expr::Lit(_, _) | Expr::Break(_) | Expr::Continue(_) => {}
        Expr::Ident(name, _) => {
            if !name.contains("::") && !is_bound(bound, name) {
                free.insert(name.clone());
            }
        }
        Expr::BinOp(l, _, r, _) => {
            collect_free_idents(l, bound, free);
            collect_free_idents(r, bound, free);
        }
        Expr::UnOp(_, e, _)
        | Expr::Await(e, _)
        | Expr::Return(e, _)
        | Expr::Cast(e, _, _)
        | Expr::Question(e, _) => collect_free_idents(e, bound, free),
        Expr::Call(callee, args, _) => {
            collect_free_idents(callee, bound, free);
            for a in args {
                collect_free_idents(a, bound, free);
            }
        }
        Expr::MethodCall(obj, _, args, _) => {
            collect_free_idents(obj, bound, free);
            for a in args {
                collect_free_idents(a, bound, free);
            }
        }
        Expr::Index(a, i, _) => {
            collect_free_idents(a, bound, free);
            collect_free_idents(i, bound, free);
        }
        Expr::Field(obj, _, _) => collect_free_idents(obj, bound, free),
        Expr::Block(b) => collect_free_in_block(b, bound, free),
        Expr::If(ie) => {
            collect_free_idents(&ie.cond, bound, free);
            collect_free_in_block(&ie.then_block, bound, free);
            for (c, b) in &ie.elif_branches {
                collect_free_idents(c, bound, free);
                collect_free_in_block(b, bound, free);
            }
            if let Some(eb) = &ie.else_block {
                collect_free_in_block(eb, bound, free);
            }
        }
        Expr::Tif(te) => {
            collect_free_idents(&te.cond, bound, free);
            collect_free_in_block(&te.pos_block, bound, free);
            collect_free_in_block(&te.zero_block, bound, free);
            collect_free_in_block(&te.neg_block, bound, free);
        }
        Expr::Match(me) => {
            collect_free_idents(&me.scrutinee, bound, free);
            for arm in &me.arms {
                bound.push(Default::default());
                bind_pattern_names(&arm.pattern, bound);
                if let Some(g) = &arm.guard {
                    collect_free_idents(g, bound, free);
                }
                collect_free_idents(&arm.body, bound, free);
                bound.pop();
            }
        }
        Expr::For(fe) => {
            collect_free_idents(&fe.iter, bound, free);
            bound.push(Default::default());
            bind_name(bound, &fe.var);
            collect_free_in_block(&fe.body, bound, free);
            bound.pop();
        }
        Expr::While(we) => {
            collect_free_idents(&we.cond, bound, free);
            collect_free_in_block(&we.body, bound, free);
        }
        Expr::Loop(b, _) | Expr::Spawn(b, _) => collect_free_in_block(b, bound, free),
        Expr::Array(elems, _) | Expr::Tuple(elems, _) => {
            for e in elems {
                collect_free_idents(e, bound, free);
            }
        }
        Expr::StructLit(_, fields, _) => {
            for (_, e) in fields {
                collect_free_idents(e, bound, free);
            }
        }
        Expr::Range(lo, hi, _, _) => {
            collect_free_idents(lo, bound, free);
            collect_free_idents(hi, bound, free);
        }
        Expr::Lambda(params, _, body, _) => {
            bound.push(params.iter().map(|(n, _)| n.clone()).collect());
            collect_free_idents(body, bound, free);
            bound.pop();
        }
        Expr::Tresult(tr) => {
            collect_free_idents(&tr.expr, bound, free);
            for (var, block) in [
                (&tr.ok_var, &tr.ok_block),
                (&tr.unknown_var, &tr.unknown_block),
                (&tr.err_var, &tr.err_block),
            ] {
                bound.push(Default::default());
                bind_name(bound, var);
                collect_free_in_block(block, bound, free);
                bound.pop();
            }
        }
    }
}
