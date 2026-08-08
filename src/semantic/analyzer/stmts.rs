// semantic/analyzer/stmts.rs — Block and statement type checking.
use super::*;

impl SemanticAnalyzer {
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
                    &self.file, span.line, span.col,
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
                            &self.file, ls.span.line, ls.span.col,
                            format!("variable '{}' shadows a previous binding", ls.name),
                        ));
                    }
                }
                let hint = if let Some(t) = &ls.ty {
                    Some(self.resolve_type(t)?)
                } else {
                    None
                };
                let init = if let Some(e) = &ls.init {
                    let te = self.check_expr(e, hint.as_ref())?;
                    let inferred_ty = te.ty.clone();
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
                    // Integer overflow detection for ternary types
                    if let TypedExprKind::Lit(Lit::Int(val)) = &te.kind {
                        let range = super::type_inference::ternary_type_range(&final_ty);
                        if let Some((lo, hi)) = range {
                            if *val < lo || *val > hi {
                                self.warnings.push(CompileWarning::new(
                                    WarningKind::IntegerOverflow,
                                    &self.file, ls.span.line, ls.span.col,
                                    format!(
                                        "value {} overflows type '{:?}' (range {}..{})",
                                        val, final_ty, lo, hi
                                    ),
                                ));
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
                            // Also define the primary name for backward compat
                            self.symbols.define(&ls.name, final_ty.clone(), ls.mutable);
                        }
                        ast::LetPat::Ident(_) => {
                            self.symbols.define(&ls.name, final_ty.clone(), ls.mutable);
                        }
                    }
                    Some(te)
                } else {
                    let ty = hint.unwrap_or(ManiType::Unknown);
                    self.symbols.define(&ls.name, ty, ls.mutable);
                    None
                };
                let ty = self.symbols.lookup(&ls.name).map(|s| s.ty.clone()).unwrap_or(ManiType::Unknown);
                Ok(TypedStmt::Let(TypedLetStmt {
                    name: ls.name.clone(),
                    pat: ls.pat.clone(),
                    ty,
                    init,
                    mutable: ls.mutable,
                }))
            }
            Stmt::Assign(a) => {
                let target = self.check_expr(&a.target, None)?;
                let value = self.check_expr(&a.value, Some(&target.ty))?;
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
            Stmt::Return(e, _span) => {
                let ret_hint = self.current_fn_ret.clone();
                let te = if let Some(expr) = e {
                    Some(self.check_expr(expr, Some(&ret_hint))?)
                } else {
                    None
                };
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
