// semantic/analyzer/stmts.rs — Block and statement type checking.
use super::*;

impl SemanticAnalyzer {
    // A function with no declared return type must not return a value.
    //
    // ONE definition, called from both places a `return` can appear —
    // `Stmt::Return` here and `Expr::Return` in expressions.rs. ManiT has two,
    // because a `return` inside a `tif` arm is an EXPRESSION:
    //
    //     fn perm_str(p: trit) -> str {
    //         tif p { + => return "GRANT", 0 => return "CHECK", - => return "DENY" }
    //     }
    //
    // The first version of this check lived only in the statement arm, and 24
    // `drop_return_type` mutations walked straight past it — every one of them
    // a function returning through `tif`. §53's lesson a third time: a rule
    // re-derived at each site will be wrong at some of them, so it goes in one
    // place and both sites call it.
    //
    // (A `///` doc comment here became a failing DOCTEST: rustdoc compiles an
    // indented block inside one as Rust, and that block is ManiT.)
    pub(super) fn check_return_value_allowed(
        &self, ty: &ManiType, span: Span,
    ) -> CompileResult<()> {
        // Unknown means inference has not settled; rejecting there would refuse
        // correct programs.
        if matches!(self.current_fn_ret, ManiType::Void)
            && !matches!(ty, ManiType::Void | ManiType::Unknown)
        {
            return Err(self.err(span, format!(
                "function has no declared return type but returns a value of \
                 type '{:?}' — add `-> {:?}` to its signature, or drop the \
                 value from `return`",
                ty, ty,
            )));
        }
        Ok(())
    }

    pub(super) fn check_block(&mut self, block: &Block) -> CompileResult<TypedBlock> {
        self.symbols.push_scope();
        let mut typed_stmts = Vec::new();
        let mut block_ty = ManiType::Void;
        let mut hit_terminator = false;
        for (i, stmt) in block.stmts.iter().enumerate() {
            // Unreachable code detection: warn if we already hit return/break/continue
            if hit_terminator {
                let span = stmt.span();
                self.warnings.push(CompileWarning::new(
                    WarningKind::UnreachableCode,
                    &self.dfile(span), span.line, span.col,
                    "unreachable code after return/break/continue",
                ));
            }
            let ts = self.check_stmt(stmt)?;
            // Track if this statement is a terminator
            match &ts {
                TypedStmt::Return(_) => hit_terminator = true,
                TypedStmt::Break | TypedStmt::Continue => hit_terminator = true,
                _ => {}
            }
            // The type of the block is the type of the last expression statement
            if i == block.stmts.len() - 1 {
                if let TypedStmt::Expr(te) = &ts {
                    block_ty = te.ty.clone();
                }
            }
            typed_stmts.push(ts);
        }
        self.symbols.pop_scope();
        Ok(TypedBlock { stmts: typed_stmts, ty: block_ty })
    }

    pub(super) fn check_stmt(&mut self, stmt: &Stmt) -> CompileResult<TypedStmt> {
        match stmt {
            Stmt::Let(ls) => {
                // Shadowing detection: warn if this variable name already exists in scope
                if !ls.name.starts_with('_') {
                    if let Some(_existing) = self.symbols.lookup(&ls.name) {
                        self.warnings.push(CompileWarning::new(
                            WarningKind::Shadowing,
                            &self.dfile(ls.span), ls.span.line, ls.span.col,
                            format!("variable '{}' shadows a previous binding", ls.name),
                        ));
                    }
                }
                let hint = if let Some(t) = &ls.ty {
                    Some(self.resolve_type(t)?)
                } else {
                    None
                };
                let (init, stmt_ty) = if let Some(e) = &ls.init {
                    let te = self.check_expr(e, hint.as_ref())?;
                    let inferred_ty = te.ty.clone();
                    // S4: a KNOWN annotation must be compatible with a KNOWN
                    // initialiser type (Unknown on either side stays permissive).
                    if let Some(h) = &hint {
                        if h.is_known() && inferred_ty.is_known() && !types_compatible(h, &inferred_ty) {
                            return Err(self.err(ls.span, format!(
                                "type mismatch: '{}' is declared as `{}` but its initialiser has type `{}`",
                                ls.name, h.display(), inferred_ty.display()
                            )));
                        }
                    }
                    // If hint is an unsized array but init provides a size, use the sized type.
                    let final_ty = match (hint, &inferred_ty) {
                        (Some(ManiType::Array(ref elem_h, None)), ManiType::Array(ref elem_i, Some(n)))
                            if elem_h == elem_i =>
                        {
                            ManiType::Array(elem_h.clone(), Some(*n))
                        }
                        (Some(h), _) => h,
                        (None, _) => inferred_ty.clone(),
                    };
                    // A literal outside a ternary type's range is an ERROR.
                    //
                    // It was a warning, and on 23 Aug 2026 that accounted for
                    // 173 of the 771 uncaught mutations -- `wrong_type` (123,
                    // a binding retyped int -> trit) and `trit_range` (50, a
                    // trit given a literal outside {-1, 0, +1}).
                    //
                    // What makes this class worse than a missed diagnostic is
                    // what the program then DOES. `let n: trit = 42;` prints
                    // 42 on the LLVM backend and 42 on T3: the two backends
                    // AGREE, and both are wrong. A trit that holds 42 is not a
                    // trit. Because they agree, the differential oracle -- the
                    // main instrument for finding backend defects -- is blind
                    // to it by construction, exactly as in the module-level
                    // `bool3` case (section 51). Nothing downstream clamps or
                    // rejects the value, so it simply travels.
                    //
                    // The ranges are exact and derived, not conventional:
                    // trit -1..1, tryte +-364, T9 +-9841, T27 +-3_812_798_742_493
                    // (see ternary_type_range). There is no reading of the
                    // language in which a literal outside them is meant.
                    //
                    // Blast radius, measured across all 128 shipped .mt files
                    // in maniTC and thatteOS before changing it: ZERO produce
                    // this diagnostic today. Nothing legitimate depends on the
                    // leniency. Only a `let` with a direct integer literal is
                    // covered -- `let n: trit = 40 + 2;` still slips through,
                    // because constant folding does not run before this check.
                    // BOTH polarities. `-17` does not parse as `Lit::Int(-17)`
                    // — it is `UnOp(Neg, Lit(17))` — so a check that matches
                    // only the literal form catches `let t: trit = 17` and
                    // waves through `let t: trit = -17`. Measured 23 Aug 2026
                    // by re-running the mutation corpus against this very fix:
                    // negative literals were still surviving it.
                    //
                    // This is §53 repeating in a new place. There, four of
                    // eight widening sites sign-extended an i1, and the defect
                    // was invisible for `false` because `sext i1 false` is 0 —
                    // the right answer. A rule that is only tested on one
                    // polarity is only correct on one polarity.
                    let lit_val = match &te.kind {
                        TypedExprKind::Lit(Lit::Int(v)) => Some(*v),
                        TypedExprKind::UnOp(UnOpKind::Neg, inner) => match &inner.kind {
                            TypedExprKind::Lit(Lit::Int(v)) => Some(-*v),
                            _ => None,
                        },
                        _ => None,
                    };
                    if let Some(val) = lit_val {
                        let range = super::type_inference::ternary_type_range(&final_ty);
                        if let Some((lo, hi)) = range {
                            if val < lo || val > hi {
                                return Err(self.err(ls.span, format!(
                                    "value {} overflows type '{:?}' (range {}..{})",
                                    val, final_ty, lo, hi,
                                )));
                            }
                        }
                    }
                    // For tuple destructuring, define each name individually
                    match &ls.pat {
                        ast::LetPat::Tuple(names) => {
                            // Get element types from the tuple type or use Unknown
                            let elem_tys: Vec<ManiType> = match &inferred_ty {
                                ManiType::Tuple(ts) => ts.clone(),
                                _ => vec![ManiType::Unknown; names.len()],
                            };
                            for (i, n) in names.iter().enumerate() {
                                let ety = elem_tys.get(i).cloned().unwrap_or(ManiType::Unknown);
                                self.symbols.define(n, ety, ls.mutable);
                            }
                            // S17: element names only — the first element must
                            // NOT be redefined with the whole tuple type.
                        }
                        ast::LetPat::Ident(_) => {
                            self.symbols.define(&ls.name, final_ty.clone(), ls.mutable);
                        }
                    }
                    (Some(te), final_ty)
                } else {
                    let ty = hint.unwrap_or(ManiType::Unknown);
                    // An uninitialised `let` accepts its first assignment even
                    // without `mut` ("left uninitialised until first assignment").
                    self.symbols.define(&ls.name, ty.clone(), true);
                    (None, ty)
                };
                Ok(TypedStmt::Let(TypedLetStmt {
                    name: ls.name.clone(),
                    pat: ls.pat.clone(),
                    ty: stmt_ty,
                    init,
                    mutable: ls.mutable,
                }))
            }
            Stmt::Assign(a) => {
                // S5: lvalue validity — only variables, index and field
                // expressions (or a deref) can be assigned to.
                match &a.target {
                    Expr::Ident(_, _) | Expr::Index(_, _, _) | Expr::Field(_, _, _) => {}
                    Expr::UnOp(ast::UnOpKind::Deref, _, _) => {}
                    other => {
                        return Err(self.err(
                            other.span(),
                            "invalid assignment target — expected a variable, index, or field",
                        ));
                    }
                }
                let target = self.check_expr(&a.target, None)?;
                // S5: mutability of direct variable assignments. Module-level
                // globals (scope depth 0) are exempt: the parser does not
                // record `mut` for globals.
                if let Expr::Ident(name, ispan) = &a.target {
                    let binding = self.symbols
                        .lookup_with_depth(name)
                        .map(|(depth, info)| (depth, info.mutable));
                    if let Some((depth, mutable)) = binding {
                        if depth >= 1 && !mutable {
                            return Err(self.err(*ispan, format!(
                                "cannot assign to immutable variable '{}' — declare it with `let mut`",
                                name
                            )));
                        }
                    }
                }
                let value = self.check_expr(&a.value, Some(&target.ty))?;
                // S5: assigned value must be type-compatible with the target;
                // compound assignments additionally obey the binop rules.
                if let Some(op) = &a.op {
                    self.binop_type(op, &target.ty, &value.ty, a.span)?;
                    // C4/R2: `x /= 2` is a division site too, and the lowerer
                    // routes it through the same `binop_to_ir` over the same
                    // type — `a.value.ty`. Taking it from the other operand
                    // here would make the backlog and the code generator
                    // disagree about which sites the version change moves,
                    // which is the one thing a migration list must not do.
                    self.note_division_semantics(op, &value.ty, a.span);
                } else if target.ty.is_known()
                    && value.ty.is_known()
                    && !types_compatible(&target.ty, &value.ty)
                {
                    return Err(self.err(a.span, format!(
                        "type mismatch: cannot assign `{}` to a target of type `{}`",
                        value.ty.display(), target.ty.display()
                    )));
                }
                Ok(TypedStmt::Assign(TypedAssignStmt {
                    target,
                    value,
                    op: a.op.clone(),
                }))
            }
            Stmt::Expr(e) => {
                let te = self.check_expr(e, None)?;
                Ok(TypedStmt::Expr(te))
            }
            Stmt::Return(e, span) => {
                let ret_hint = self.current_fn_ret.clone();
                let te = if let Some(expr) = e {
                    Some(self.check_expr(expr, Some(&ret_hint))?)
                } else {
                    None
                };
                // A function with no declared return type must not return a
                // value. §A1 above already enforces the other direction — a
                // non-void function must supply one on every path — but this
                // direction was never checked: `current_fn_ret` was used only
                // as a HINT for inferring the expression, never compared
                // against it.
                //
                // That was `drop_return_type`, 152 of the 771 uncaught
                // mutations (ORACLE_FINDINGS §54) and the only class of the
                // six where the two backends actually DISAGREE, so it is a
                // wrong answer rather than merely an unreported one:
                //
                //     fn f(n: int)     { return n + 1; }
                //     io::println_int(f(41))
                //         LLVM   clang link failure — the IR names a value the
                //                function was never compiled to produce
                //         T3     prints 0, silently, for an expected 42
                if let Some(t) = &te {
                    self.check_return_value_allowed(&t.ty, *span)?;
                }
                Ok(TypedStmt::Return(te))
            }
            Stmt::Break(_) => Ok(TypedStmt::Break),
            Stmt::Continue(_) => Ok(TypedStmt::Continue),
            Stmt::LocalStructDef(s) => {
                // Register the struct so it can be used within the rest of the block
                let mut fields = Vec::new();
                for f in &s.fields {
                    let ty = self.resolve_type(&f.ty)?;
                    fields.push((f.name.clone(), ty));
                }
                self.structs.insert(s.name.clone(), fields);
                // Emit as a no-op expression in the typed output
                Ok(TypedStmt::Expr(TypedExpr {
                    kind: TypedExprKind::Lit(crate::ast::Lit::Null),
                    ty: ManiType::Void,
                    span: s.span,
                }))
            }
        }
    }
}
