//! Simplified borrow / move checker for the ManiT compiler.
//!
//! This pass runs on the **TypedProgram** (after semantic analysis, before IR
//! lowering) and catches three categories of mistakes:
//!
//! 1. **Use-after-move** -- reading a variable that was already moved.
//! 2. **Double-move** -- moving a variable that was already moved.
//! 3. **Move-in-loop** -- moving a non-Copy variable inside a loop body where
//!    it would be consumed on every iteration.
//!
//! We intentionally do NOT implement lifetime annotations, reference borrowing,
//! reborrowing, or NLL. This is a lightweight safety net, not a full Rust-style
//! borrow checker.

use std::collections::HashSet;

use crate::error::{CompileError, CompileResult};
use crate::semantic::types::*;

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Run the borrow / move checker over every function in the program.
pub fn check_borrows(program: &TypedProgram) -> CompileResult<()> {
    for func in &program.functions {
        check_fn_borrows(func)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Per-function analysis
// ---------------------------------------------------------------------------

fn check_fn_borrows(func: &TypedFnDef) -> CompileResult<()> {
    if let Some(ref body) = func.body {
        let mut moved: HashSet<String> = HashSet::new();
        check_block_borrows(body, &mut moved, false)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Block
// ---------------------------------------------------------------------------

fn check_block_borrows(
    block: &TypedBlock,
    moved: &mut HashSet<String>,
    in_loop: bool,
) -> CompileResult<()> {
    for stmt in &block.stmts {
        check_stmt_borrows(stmt, moved, in_loop)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Statements
// ---------------------------------------------------------------------------

fn check_stmt_borrows(
    stmt: &TypedStmt,
    moved: &mut HashSet<String>,
    in_loop: bool,
) -> CompileResult<()> {
    match stmt {
        TypedStmt::Let(let_stmt) => {
            if let Some(ref init_expr) = let_stmt.init {
                // Check the initialiser expression for use-after-move first.
                check_expr_borrows(init_expr, moved, in_loop)?;

                // If the initialiser is a plain variable of move type, that
                // variable is now moved.  Enum variant constructors (containing
                // "::") are constants, not variables.
                if let TypedExprKind::Ident(ref var_name) = init_expr.kind {
                    if is_move_type(&init_expr.ty) && !var_name.contains("::") {
                        if in_loop {
                            return Err(CompileError::type_err(
                                "<borrow>",
                                init_expr.span.line,
                                init_expr.span.col,
                                format!(
                                    "cannot move '{}' in a loop \
                                     -- value would be moved on each iteration",
                                    var_name
                                ),
                            ));
                        }
                        if moved.contains(var_name.as_str()) {
                            return Err(CompileError::type_err(
                                "<borrow>",
                                init_expr.span.line,
                                init_expr.span.col,
                                format!("use of moved value: '{}'", var_name),
                            ));
                        }
                        moved.insert(var_name.clone());
                    }
                }
            }

            // A new `let` binding shadows / rebinds, so remove from moved set.
            moved.remove(&let_stmt.name);
        }

        TypedStmt::Assign(assign_stmt) => {
            // Check RHS first (evaluate value before assigning).
            check_expr_borrows(&assign_stmt.value, moved, in_loop)?;

            // If the RHS is a plain variable of move type, it is moved.
            // Enum variant constructors (containing "::") are constants.
            if let TypedExprKind::Ident(ref var_name) = assign_stmt.value.kind {
                if is_move_type(&assign_stmt.value.ty) && !var_name.contains("::") {
                    if in_loop {
                        return Err(CompileError::type_err(
                            "<borrow>",
                            assign_stmt.value.span.line,
                            assign_stmt.value.span.col,
                            format!(
                                "cannot move '{}' in a loop \
                                 -- value would be moved on each iteration",
                                var_name
                            ),
                        ));
                    }
                    if moved.contains(var_name.as_str()) {
                        return Err(CompileError::type_err(
                            "<borrow>",
                            assign_stmt.value.span.line,
                            assign_stmt.value.span.col,
                            format!("use of moved value: '{}'", var_name),
                        ));
                    }
                    moved.insert(var_name.clone());
                }
            }

            // Check target expression (e.g. index / field on LHS).
            check_expr_borrows(&assign_stmt.target, moved, in_loop)?;

            // If target is a plain ident, rebinding un-moves it.
            if let TypedExprKind::Ident(ref target_name) = assign_stmt.target.kind {
                moved.remove(target_name);
            }
        }

        TypedStmt::Expr(expr) => {
            check_expr_borrows(expr, moved, in_loop)?;
        }

        TypedStmt::Return(opt_expr) => {
            if let Some(ref expr) = opt_expr {
                check_expr_borrows(expr, moved, in_loop)?;
            }
        }

        TypedStmt::Break | TypedStmt::Continue => {
            // Nothing to check.
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Expressions
// ---------------------------------------------------------------------------

fn check_expr_borrows(
    expr: &TypedExpr,
    moved: &mut HashSet<String>,
    in_loop: bool,
) -> CompileResult<()> {
    match &expr.kind {
        // --- Identifier (variable read) ---
        TypedExprKind::Ident(name) => {
            // Enum variant constructors (e.g. "Season::Summer") are constant
            // expressions, not variables -- skip them.
            if !name.contains("::") && moved.contains(name.as_str()) {
                return Err(CompileError::type_err(
                    "<borrow>",
                    expr.span.line,
                    expr.span.col,
                    format!("use of moved value: '{}'", name),
                ));
            }
        }

        // --- Literals ---
        TypedExprKind::Lit(_) => {}

        // --- Binary / Unary operators ---
        TypedExprKind::BinOp(lhs, _op, rhs) => {
            check_expr_borrows(lhs, moved, in_loop)?;
            check_expr_borrows(rhs, moved, in_loop)?;
        }
        TypedExprKind::UnOp(_op, operand) => {
            check_expr_borrows(operand, moved, in_loop)?;
        }

        // --- Function call ---
        // NOTE: In this simplified borrow checker we do NOT mark call
        // arguments as moved.  ManiT has no explicit borrow/move syntax
        // yet, so treating every call argument as a move would reject
        // valid programs.  We still check that none of the arguments are
        // already-moved variables (use-after-move).
        TypedExprKind::Call(callee, args) => {
            check_expr_borrows(callee, moved, in_loop)?;
            for arg in args {
                check_expr_borrows(arg, moved, in_loop)?;
            }
        }

        // --- Method call ---
        TypedExprKind::MethodCall(receiver, _method, args) => {
            check_expr_borrows(receiver, moved, in_loop)?;
            for arg in args {
                check_expr_borrows(arg, moved, in_loop)?;
            }
        }

        // --- Index / Field access ---
        TypedExprKind::Index(base, idx) => {
            check_expr_borrows(base, moved, in_loop)?;
            check_expr_borrows(idx, moved, in_loop)?;
        }
        TypedExprKind::Field(base, _field) => {
            check_expr_borrows(base, moved, in_loop)?;
        }

        // --- Block ---
        TypedExprKind::Block(block) => {
            check_block_borrows(block, moved, in_loop)?;
        }

        // --- If ---
        TypedExprKind::If(if_expr) => {
            check_expr_borrows(&if_expr.cond, moved, in_loop)?;

            // Each branch gets its own snapshot; we conservatively union the
            // moved sets afterwards.
            let mut then_moved = moved.clone();
            check_block_borrows(&if_expr.then_block, &mut then_moved, in_loop)?;

            // elif branches
            let mut all_branch_moved: Vec<HashSet<String>> = vec![then_moved];
            for (elif_cond, elif_block) in &if_expr.elif_branches {
                check_expr_borrows(elif_cond, moved, in_loop)?;
                let mut branch_moved = moved.clone();
                check_block_borrows(elif_block, &mut branch_moved, in_loop)?;
                all_branch_moved.push(branch_moved);
            }

            if let Some(ref else_block) = if_expr.else_block {
                let mut else_moved = moved.clone();
                check_block_borrows(else_block, &mut else_moved, in_loop)?;
                all_branch_moved.push(else_moved);
            }

            // Conservatively: anything moved in ANY branch is considered moved.
            for branch in &all_branch_moved {
                for m in branch {
                    moved.insert(m.clone());
                }
            }
        }

        // --- Tif (ternary if: pos / zero / neg) ---
        TypedExprKind::Tif(tif_expr) => {
            check_expr_borrows(&tif_expr.cond, moved, in_loop)?;

            let mut pos_moved = moved.clone();
            check_block_borrows(&tif_expr.pos_block, &mut pos_moved, in_loop)?;

            let mut zero_moved = moved.clone();
            check_block_borrows(&tif_expr.zero_block, &mut zero_moved, in_loop)?;

            let mut neg_moved = moved.clone();
            check_block_borrows(&tif_expr.neg_block, &mut neg_moved, in_loop)?;

            for m in pos_moved.union(&zero_moved).chain(neg_moved.iter()) {
                moved.insert(m.clone());
            }
        }

        // --- Match ---
        TypedExprKind::Match(match_expr) => {
            check_expr_borrows(&match_expr.scrutinee, moved, in_loop)?;

            let mut all_arm_moved: Vec<HashSet<String>> = Vec::new();
            for arm in &match_expr.arms {
                if let Some(ref guard) = arm.guard {
                    check_expr_borrows(guard, moved, in_loop)?;
                }
                let mut arm_moved = moved.clone();
                check_expr_borrows(&arm.body, &mut arm_moved, in_loop)?;
                all_arm_moved.push(arm_moved);
            }

            for arm in &all_arm_moved {
                for m in arm {
                    moved.insert(m.clone());
                }
            }
        }

        // --- For loop ---
        TypedExprKind::For(for_expr) => {
            check_expr_borrows(&for_expr.iter, moved, in_loop)?;
            check_block_borrows(&for_expr.body, moved, /* in_loop = */ true)?;
        }

        // --- While loop ---
        TypedExprKind::While(while_expr) => {
            check_expr_borrows(&while_expr.cond, moved, in_loop)?;
            check_block_borrows(&while_expr.body, moved, /* in_loop = */ true)?;
        }

        // --- Infinite loop ---
        TypedExprKind::Loop(body) => {
            check_block_borrows(body, moved, /* in_loop = */ true)?;
        }

        // --- Array literal ---
        TypedExprKind::Array(elems) => {
            for elem in elems {
                check_expr_borrows(elem, moved, in_loop)?;
            }
        }

        // --- Tuple literal ---
        TypedExprKind::Tuple(elems) => {
            for elem in elems {
                check_expr_borrows(elem, moved, in_loop)?;
                // Elements of move type are consumed.
                if let TypedExprKind::Ident(ref var_name) = elem.kind {
                    if is_move_type(&elem.ty) && !var_name.contains("::") {
                        if in_loop {
                            return Err(CompileError::type_err(
                                "<borrow>",
                                elem.span.line,
                                elem.span.col,
                                format!(
                                    "cannot move '{}' in a loop \
                                     -- value would be moved on each iteration",
                                    var_name
                                ),
                            ));
                        }
                        moved.insert(var_name.clone());
                    }
                }
            }
        }

        // --- Struct literal ---
        TypedExprKind::StructLit(_name, fields) => {
            for (_field_name, field_expr) in fields {
                check_expr_borrows(field_expr, moved, in_loop)?;
                if let TypedExprKind::Ident(ref var_name) = field_expr.kind {
                    if is_move_type(&field_expr.ty) && !var_name.contains("::") {
                        if in_loop {
                            return Err(CompileError::type_err(
                                "<borrow>",
                                field_expr.span.line,
                                field_expr.span.col,
                                format!(
                                    "cannot move '{}' in a loop \
                                     -- value would be moved on each iteration",
                                    var_name
                                ),
                            ));
                        }
                        moved.insert(var_name.clone());
                    }
                }
            }
        }

        // --- Range ---
        TypedExprKind::Range(start, end, _inclusive) => {
            check_expr_borrows(start, moved, in_loop)?;
            check_expr_borrows(end, moved, in_loop)?;
        }

        // --- Return (expression form) ---
        TypedExprKind::Return(inner) => {
            check_expr_borrows(inner, moved, in_loop)?;
        }

        // --- Cast ---
        TypedExprKind::Cast(inner, _ty) => {
            check_expr_borrows(inner, moved, in_loop)?;
        }

        // --- ? operator ---
        TypedExprKind::Question(inner) => {
            check_expr_borrows(inner, moved, in_loop)?;
        }

        // --- Spawn ---
        TypedExprKind::Spawn(block) => {
            // Spawned block gets its own moved set (it captures by move).
            let mut spawn_moved = moved.clone();
            check_block_borrows(block, &mut spawn_moved, in_loop)?;
            // Anything the spawn block references as moved is also moved in
            // the parent scope.
            for m in spawn_moved {
                moved.insert(m);
            }
        }

        // --- Await ---
        TypedExprKind::Await(inner) => {
            check_expr_borrows(inner, moved, in_loop)?;
        }

        // --- Break / Continue (expression form) ---
        TypedExprKind::Break | TypedExprKind::Continue => {}

        // --- Tresult ---
        TypedExprKind::Tresult(tr) => {
            check_expr_borrows(&tr.expr, moved, in_loop)?;
            check_block_borrows(&tr.ok_block, moved, in_loop)?;
            check_block_borrows(&tr.unknown_block, moved, in_loop)?;
            check_block_borrows(&tr.err_block, moved, in_loop)?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Copy vs Move classification
// ---------------------------------------------------------------------------

/// Returns `true` if the type is "moved" when passed / assigned (non-Copy).
/// Copy types (numeric scalars, bool, trit, char, void, function pointers)
/// are never moved.
fn is_move_type(ty: &ManiType) -> bool {
    match ty {
        // Copy types -- small scalars, function pointers.
        ManiType::Int
        | ManiType::Float
        | ManiType::Bool
        | ManiType::Bool3
        | ManiType::Trit
        | ManiType::Tryte
        | ManiType::T9
        | ManiType::T27
        | ManiType::T54
        // ManiType::Trint merged into T54
        | ManiType::Tfloat
        | ManiType::Char
        | ManiType::Void
        | ManiType::Unknown => false,

        // Function types are Copy (pointer-sized).
        ManiType::Fn(_, _) => false,

        // Move types -- heap-allocated or composite.
        ManiType::Str => true,
        ManiType::Struct(_) => true,
        ManiType::Enum(_) => true,
        ManiType::Generic(_, _) => true,
        ManiType::Array(_, _) => true,
        ManiType::Tuple(_) => true,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Lit, Span};

    /// Helper: build a TypedExpr with Ident kind.
    fn ident_expr(name: &str, ty: ManiType) -> TypedExpr {
        TypedExpr {
            kind: TypedExprKind::Ident(name.to_string()),
            ty,
            span: Span { line: 1, col: 1 },
        }
    }

    /// Helper: build a TypedExpr with Int literal.
    fn int_lit(val: i64) -> TypedExpr {
        TypedExpr {
            kind: TypedExprKind::Lit(Lit::Int(val)),
            ty: ManiType::Int,
            span: Span { line: 1, col: 1 },
        }
    }

    #[test]
    fn test_copy_type_no_move() {
        // let x: int = 42; let y = x; let z = x;  -- should be fine (int is Copy)
        let stmts = vec![
            TypedStmt::Let(TypedLetStmt {
                name: "x".to_string(),
                pat: crate::ast::LetPat::Ident("x".to_string()),
                ty: ManiType::Int,
                init: Some(int_lit(42)),
                mutable: false,
            }),
            TypedStmt::Let(TypedLetStmt {
                name: "y".to_string(),
                pat: crate::ast::LetPat::Ident("y".to_string()),
                ty: ManiType::Int,
                init: Some(ident_expr("x", ManiType::Int)),
                mutable: false,
            }),
            TypedStmt::Let(TypedLetStmt {
                name: "z".to_string(),
                pat: crate::ast::LetPat::Ident("z".to_string()),
                ty: ManiType::Int,
                init: Some(ident_expr("x", ManiType::Int)),
                mutable: false,
            }),
        ];

        let block = TypedBlock { stmts, ty: ManiType::Void };
        let mut moved = HashSet::new();
        assert!(check_block_borrows(&block, &mut moved, false).is_ok());
    }

    #[test]
    fn test_use_after_move_str() {
        // let s: str = "hi"; let t = s; let u = s;  -- error: use of moved 's'
        let stmts = vec![
            TypedStmt::Let(TypedLetStmt {
                name: "s".to_string(),
                pat: crate::ast::LetPat::Ident("s".to_string()),
                ty: ManiType::Str,
                init: Some(TypedExpr {
                    kind: TypedExprKind::Lit(Lit::Str("hi".to_string())),
                    ty: ManiType::Str,
                    span: Span { line: 1, col: 1 },
                }),
                mutable: false,
            }),
            TypedStmt::Let(TypedLetStmt {
                name: "t".to_string(),
                pat: crate::ast::LetPat::Ident("t".to_string()),
                ty: ManiType::Str,
                init: Some(ident_expr("s", ManiType::Str)),
                mutable: false,
            }),
            TypedStmt::Let(TypedLetStmt {
                name: "u".to_string(),
                pat: crate::ast::LetPat::Ident("u".to_string()),
                ty: ManiType::Str,
                init: Some(ident_expr("s", ManiType::Str)),
                mutable: false,
            }),
        ];

        let block = TypedBlock { stmts, ty: ManiType::Void };
        let mut moved = HashSet::new();
        let result = check_block_borrows(&block, &mut moved, false);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("use of moved value: 's'"), "got: {}", msg);
    }

    #[test]
    fn test_move_in_loop() {
        // while true { let t = s; }  -- error: move in loop
        let stmts = vec![
            TypedStmt::Let(TypedLetStmt {
                name: "t".to_string(),
                pat: crate::ast::LetPat::Ident("t".to_string()),
                ty: ManiType::Str,
                init: Some(ident_expr("s", ManiType::Str)),
                mutable: false,
            }),
        ];

        let block = TypedBlock { stmts, ty: ManiType::Void };
        let mut moved = HashSet::new();
        let result = check_block_borrows(&block, &mut moved, true);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("cannot move 's' in a loop"), "got: {}", msg);
    }

    #[test]
    fn test_rebind_clears_moved() {
        // let s: str = "a"; let t = s; let s: str = "b"; let u = s; -- OK
        let stmts = vec![
            TypedStmt::Let(TypedLetStmt {
                name: "s".to_string(),
                pat: crate::ast::LetPat::Ident("s".to_string()),
                ty: ManiType::Str,
                init: Some(TypedExpr {
                    kind: TypedExprKind::Lit(Lit::Str("a".to_string())),
                    ty: ManiType::Str,
                    span: Span { line: 1, col: 1 },
                }),
                mutable: false,
            }),
            TypedStmt::Let(TypedLetStmt {
                name: "t".to_string(),
                pat: crate::ast::LetPat::Ident("t".to_string()),
                ty: ManiType::Str,
                init: Some(ident_expr("s", ManiType::Str)),
                mutable: false,
            }),
            TypedStmt::Let(TypedLetStmt {
                name: "s".to_string(),
                pat: crate::ast::LetPat::Ident("s".to_string()),
                ty: ManiType::Str,
                init: Some(TypedExpr {
                    kind: TypedExprKind::Lit(Lit::Str("b".to_string())),
                    ty: ManiType::Str,
                    span: Span { line: 2, col: 1 },
                }),
                mutable: false,
            }),
            TypedStmt::Let(TypedLetStmt {
                name: "u".to_string(),
                pat: crate::ast::LetPat::Ident("u".to_string()),
                ty: ManiType::Str,
                init: Some(ident_expr("s", ManiType::Str)),
                mutable: false,
            }),
        ];

        let block = TypedBlock { stmts, ty: ManiType::Void };
        let mut moved = HashSet::new();
        assert!(check_block_borrows(&block, &mut moved, false).is_ok());
    }
}
