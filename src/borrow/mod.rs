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
//!
//! Scoping: the checker tracks declaration scopes so that the moved-set is
//! keyed by *binding* — (scope depth, name) — not by bare name. This makes
//! shadowing work in both directions (moving an inner shadow does not poison
//! the outer binding, and an inner `let` does not launder an outer move), and
//! lets the move-in-loop check ignore variables that are declared inside the
//! loop body (they are fresh on every iteration).

use std::collections::HashSet;

use crate::ast::{LetPat, Pattern};
use crate::error::{CompileError, CompileResult};
use crate::semantic::types::*;

// ---------------------------------------------------------------------------
// Move environment: declaration scopes + moved bindings
// ---------------------------------------------------------------------------

/// Scope depth at which a loop body begins. A move of a variable declared at
/// `depth >= boundary` targets a binding that is fresh on each iteration, so
/// the move-in-loop check does not apply to it.
type LoopBoundary = Option<usize>;

#[derive(Debug, Default)]
struct MoveEnv {
    /// Stack of declaration scopes (index = scope depth).
    scopes: Vec<HashSet<String>>,
    /// Moved bindings, keyed by (declaring scope depth, name).
    moved: HashSet<(usize, String)>,
}

impl MoveEnv {
    fn new() -> Self {
        MoveEnv { scopes: vec![HashSet::new()], moved: HashSet::new() }
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashSet::new());
    }

    fn pop_scope(&mut self) {
        let depth = self.scopes.len() - 1;
        self.scopes.pop();
        // Bindings of the dead scope are gone; drop their moved flags so a
        // later scope at the same depth starts clean.
        self.moved.retain(|(d, _)| *d != depth);
    }

    /// Declare (or re-declare) a binding in the current scope. A fresh `let`
    /// always clears any moved flag of the same-depth binding it replaces.
    fn declare(&mut self, name: &str) {
        let depth = self.scopes.len() - 1;
        self.scopes.last_mut().expect("scope stack never empty").insert(name.to_string());
        self.moved.remove(&(depth, name.to_string()));
    }

    /// Depth of the innermost scope declaring `name`. Names never declared in
    /// this environment (globals, unresolved) act like outermost bindings.
    fn depth_of(&self, name: &str) -> usize {
        for (d, scope) in self.scopes.iter().enumerate().rev() {
            if scope.contains(name) {
                return d;
            }
        }
        0
    }

    fn is_moved(&self, name: &str) -> bool {
        self.moved.contains(&(self.depth_of(name), name.to_string()))
    }

    fn mark_moved(&mut self, name: &str) {
        let d = self.depth_of(name);
        self.moved.insert((d, name.to_string()));
    }

    fn clear_moved(&mut self, name: &str) {
        let d = self.depth_of(name);
        self.moved.remove(&(d, name.to_string()));
    }
}

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
        let mut env = MoveEnv::new();
        for param in &func.params {
            env.declare(&param.name);
        }
        check_block_borrows(body, &mut env, None)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn err(span: crate::ast::Span, msg: String) -> CompileError {
    CompileError::type_err("<borrow>", span.line, span.col, msg)
}

/// If `expr` is a plain variable of move type, consume it: enforce the
/// move-in-loop and use-after-move rules, then mark the binding moved.
/// Enum variant constructors (containing "::") are constants, not variables.
fn consume_if_move(
    expr: &TypedExpr,
    env: &mut MoveEnv,
    loop_from: LoopBoundary,
) -> CompileResult<()> {
    if let TypedExprKind::Ident(ref var_name) = expr.kind {
        if is_move_type(&expr.ty) && !var_name.contains("::") {
            let depth = env.depth_of(var_name);
            // Variables declared inside the loop body (depth >= boundary) are
            // fresh each iteration -- moving them is fine.
            if let Some(boundary) = loop_from {
                if depth < boundary {
                    return Err(err(
                        expr.span,
                        format!(
                            "cannot move '{}' in a loop \
                             -- value would be moved on each iteration",
                            var_name
                        ),
                    ));
                }
            }
            if env.is_moved(var_name) {
                return Err(err(
                    expr.span,
                    format!("use of moved value: '{}'", var_name),
                ));
            }
            env.mark_moved(var_name);
        }
    }
    Ok(())
}

/// Collect every name bound by a match pattern.
fn declare_pattern_names(pat: &Pattern, env: &mut MoveEnv) {
    match pat {
        Pattern::Wildcard(_) | Pattern::Lit(_, _) => {}
        Pattern::Ident(n, _) => env.declare(n),
        Pattern::Tuple(ps, _) | Pattern::Or(ps, _) | Pattern::Enum(_, _, ps, _) => {
            for p in ps {
                declare_pattern_names(p, env);
            }
        }
        Pattern::Struct(_, fields, _) => {
            for (_, p) in fields {
                declare_pattern_names(p, env);
            }
        }
    }
}

/// Fork the moved-set for each branch and union the results afterwards:
/// anything moved in ANY branch is conservatively considered moved.
fn check_branches<F>(
    env: &mut MoveEnv,
    branches: Vec<F>,
) -> CompileResult<()>
where
    F: FnOnce(&mut MoveEnv) -> CompileResult<()>,
{
    let base = env.moved.clone();
    let mut acc = base.clone();
    for branch in branches {
        env.moved = base.clone();
        branch(env)?;
        acc.extend(env.moved.drain());
    }
    env.moved = acc;
    Ok(())
}

// ---------------------------------------------------------------------------
// Block
// ---------------------------------------------------------------------------

fn check_block_borrows(
    block: &TypedBlock,
    env: &mut MoveEnv,
    loop_from: LoopBoundary,
) -> CompileResult<()> {
    env.push_scope();
    let result = (|| {
        for stmt in &block.stmts {
            check_stmt_borrows(stmt, env, loop_from)?;
        }
        Ok(())
    })();
    env.pop_scope();
    result
}

// ---------------------------------------------------------------------------
// Statements
// ---------------------------------------------------------------------------

fn check_stmt_borrows(
    stmt: &TypedStmt,
    env: &mut MoveEnv,
    loop_from: LoopBoundary,
) -> CompileResult<()> {
    match stmt {
        TypedStmt::Let(let_stmt) => {
            // Check the initialiser BEFORE the new binding exists, so
            // `let s = s;` reads the outer `s`.
            if let Some(ref init_expr) = let_stmt.init {
                check_expr_borrows(init_expr, env, loop_from)?;
                consume_if_move(init_expr, env, loop_from)?;
            }

            // A new `let` binding shadows / rebinds in the CURRENT scope.
            match &let_stmt.pat {
                LetPat::Ident(_) => env.declare(&let_stmt.name),
                // Tuple destructuring declares each element name; the first
                // element is NOT redefined with the whole tuple type.
                LetPat::Tuple(names) => {
                    for n in names {
                        env.declare(n);
                    }
                }
            }
        }

        TypedStmt::Assign(assign_stmt) => {
            // Check RHS first (evaluate value before assigning).
            check_expr_borrows(&assign_stmt.value, env, loop_from)?;
            consume_if_move(&assign_stmt.value, env, loop_from)?;

            match &assign_stmt.target.kind {
                // A plain-identifier target is a REBIND, not a read: it must
                // not trip use-after-move, and it clears the moved flag.
                // (Compound assignments like `s += x` do read the target.)
                TypedExprKind::Ident(ref target_name) => {
                    if assign_stmt.op.is_some()
                        && !target_name.contains("::")
                        && env.is_moved(target_name)
                    {
                        return Err(err(
                            assign_stmt.target.span,
                            format!("use of moved value: '{}'", target_name),
                        ));
                    }
                    env.clear_moved(target_name);
                }
                // Index / field targets read their base expression.
                _ => check_expr_borrows(&assign_stmt.target, env, loop_from)?,
            }
        }

        TypedStmt::Expr(expr) => {
            check_expr_borrows(expr, env, loop_from)?;
        }

        TypedStmt::Return(opt_expr) => {
            if let Some(ref expr) = opt_expr {
                check_expr_borrows(expr, env, loop_from)?;
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
    env: &mut MoveEnv,
    loop_from: LoopBoundary,
) -> CompileResult<()> {
    match &expr.kind {
        // --- Identifier (variable read) ---
        TypedExprKind::Ident(name) => {
            // Enum variant constructors (e.g. "Season::Summer") are constant
            // expressions, not variables -- skip them.
            if !name.contains("::") && env.is_moved(name) {
                return Err(err(
                    expr.span,
                    format!("use of moved value: '{}'", name),
                ));
            }
        }

        // --- Literals ---
        TypedExprKind::Lit(_) => {}

        // --- Binary / Unary operators ---
        TypedExprKind::BinOp(lhs, _op, rhs) => {
            check_expr_borrows(lhs, env, loop_from)?;
            check_expr_borrows(rhs, env, loop_from)?;
        }
        TypedExprKind::UnOp(_op, operand) => {
            check_expr_borrows(operand, env, loop_from)?;
        }

        // --- Function call ---
        // NOTE: In this simplified borrow checker we do NOT mark call
        // arguments as moved.  ManiT has no explicit borrow/move syntax
        // yet, so treating every call argument as a move would reject
        // valid programs.  We still check that none of the arguments are
        // already-moved variables (use-after-move).
        TypedExprKind::Call(callee, args) => {
            check_expr_borrows(callee, env, loop_from)?;
            for arg in args {
                check_expr_borrows(arg, env, loop_from)?;
            }
        }

        // --- Method call ---
        TypedExprKind::MethodCall(receiver, _method, args) => {
            check_expr_borrows(receiver, env, loop_from)?;
            for arg in args {
                check_expr_borrows(arg, env, loop_from)?;
            }
        }

        // --- Index / Field access ---
        TypedExprKind::Index(base, idx) => {
            check_expr_borrows(base, env, loop_from)?;
            check_expr_borrows(idx, env, loop_from)?;
        }
        TypedExprKind::Field(base, _field) => {
            check_expr_borrows(base, env, loop_from)?;
        }

        // --- Block ---
        TypedExprKind::Block(block) => {
            check_block_borrows(block, env, loop_from)?;
        }

        // --- If ---
        TypedExprKind::If(if_expr) => {
            check_expr_borrows(&if_expr.cond, env, loop_from)?;
            for (elif_cond, _) in &if_expr.elif_branches {
                check_expr_borrows(elif_cond, env, loop_from)?;
            }

            // Each branch forks the moved-set; the results are unioned.
            let mut branches: Vec<Box<dyn FnOnce(&mut MoveEnv) -> CompileResult<()>>> =
                vec![Box::new(|env: &mut MoveEnv| {
                    check_block_borrows(&if_expr.then_block, env, loop_from)
                })];
            for (_, elif_block) in &if_expr.elif_branches {
                branches.push(Box::new(move |env: &mut MoveEnv| {
                    check_block_borrows(elif_block, env, loop_from)
                }));
            }
            if let Some(ref else_block) = if_expr.else_block {
                branches.push(Box::new(move |env: &mut MoveEnv| {
                    check_block_borrows(else_block, env, loop_from)
                }));
            }
            check_branches(env, branches)?;
        }

        // --- Tif (ternary if: pos / zero / neg) ---
        TypedExprKind::Tif(tif_expr) => {
            check_expr_borrows(&tif_expr.cond, env, loop_from)?;
            check_branches(env, vec![
                Box::new(|env: &mut MoveEnv| {
                    check_block_borrows(&tif_expr.pos_block, env, loop_from)
                }) as Box<dyn FnOnce(&mut MoveEnv) -> CompileResult<()>>,
                Box::new(|env: &mut MoveEnv| {
                    check_block_borrows(&tif_expr.zero_block, env, loop_from)
                }),
                Box::new(|env: &mut MoveEnv| {
                    check_block_borrows(&tif_expr.neg_block, env, loop_from)
                }),
            ])?;
        }

        // --- Match ---
        TypedExprKind::Match(match_expr) => {
            check_expr_borrows(&match_expr.scrutinee, env, loop_from)?;

            let branches: Vec<Box<dyn FnOnce(&mut MoveEnv) -> CompileResult<()>>> =
                match_expr.arms.iter().map(|arm| {
                    Box::new(move |env: &mut MoveEnv| {
                        // Pattern bindings are fresh per arm.
                        env.push_scope();
                        let r = (|| {
                            declare_pattern_names(&arm.pattern, env);
                            if let Some(ref guard) = arm.guard {
                                check_expr_borrows(guard, env, loop_from)?;
                            }
                            check_expr_borrows(&arm.body, env, loop_from)
                        })();
                        env.pop_scope();
                        r
                    }) as Box<dyn FnOnce(&mut MoveEnv) -> CompileResult<()>>
                }).collect();
            check_branches(env, branches)?;
        }

        // --- For loop ---
        TypedExprKind::For(for_expr) => {
            check_expr_borrows(&for_expr.iter, env, loop_from)?;
            // The loop variable is fresh each iteration: declare it inside
            // the loop boundary so moving it is not a move-in-loop.
            env.push_scope();
            let boundary = env.scopes.len() - 1;
            env.declare(&for_expr.var);
            let r = check_block_borrows(&for_expr.body, env, Some(boundary));
            env.pop_scope();
            r?;
        }

        // --- While loop ---
        TypedExprKind::While(while_expr) => {
            check_expr_borrows(&while_expr.cond, env, loop_from)?;
            let boundary = env.scopes.len();
            check_block_borrows(&while_expr.body, env, Some(boundary))?;
        }

        // --- Infinite loop ---
        TypedExprKind::Loop(body) => {
            let boundary = env.scopes.len();
            check_block_borrows(body, env, Some(boundary))?;
        }

        // --- Array literal ---
        TypedExprKind::Array(elems) => {
            for elem in elems {
                check_expr_borrows(elem, env, loop_from)?;
            }
        }

        // --- Tuple literal ---
        TypedExprKind::Tuple(elems) => {
            for elem in elems {
                check_expr_borrows(elem, env, loop_from)?;
                // Elements of move type are consumed.
                consume_if_move(elem, env, loop_from)?;
            }
        }

        // --- Struct literal ---
        TypedExprKind::StructLit(_name, fields) => {
            for (_field_name, field_expr) in fields {
                check_expr_borrows(field_expr, env, loop_from)?;
                consume_if_move(field_expr, env, loop_from)?;
            }
        }

        // --- Range ---
        TypedExprKind::Range(start, end, _inclusive) => {
            check_expr_borrows(start, env, loop_from)?;
            check_expr_borrows(end, env, loop_from)?;
        }

        // --- Return (expression form) ---
        TypedExprKind::Return(inner) => {
            check_expr_borrows(inner, env, loop_from)?;
        }

        // --- Cast ---
        TypedExprKind::Cast(inner, _ty) => {
            check_expr_borrows(inner, env, loop_from)?;
        }

        // --- ? operator ---
        TypedExprKind::Question(inner) => {
            check_expr_borrows(inner, env, loop_from)?;
        }

        // --- Spawn ---
        TypedExprKind::Spawn(block) => {
            // Spawned block gets its own moved set (it captures by move);
            // anything it moves is also moved in the parent scope.
            check_branches(env, vec![Box::new(|env: &mut MoveEnv| {
                check_block_borrows(block, env, loop_from)
            }) as Box<dyn FnOnce(&mut MoveEnv) -> CompileResult<()>>])?;
        }

        // --- Await ---
        TypedExprKind::Await(inner) => {
            check_expr_borrows(inner, env, loop_from)?;
        }

        // --- Break / Continue (expression form) ---
        TypedExprKind::Break | TypedExprKind::Continue => {}

        // --- Tresult ---
        // The three arms are mutually exclusive at runtime: fork + union,
        // exactly like if/match (a move in ok_block must not poison
        // err_block). Each arm's binding variable is fresh in its scope.
        TypedExprKind::Tresult(tr) => {
            check_expr_borrows(&tr.expr, env, loop_from)?;
            let arms: [(&String, &TypedBlock); 3] = [
                (&tr.ok_var, &tr.ok_block),
                (&tr.unknown_var, &tr.unknown_block),
                (&tr.err_var, &tr.err_block),
            ];
            let branches: Vec<Box<dyn FnOnce(&mut MoveEnv) -> CompileResult<()>>> =
                arms.into_iter().map(|(var, block)| {
                    Box::new(move |env: &mut MoveEnv| {
                        env.push_scope();
                        env.declare(var);
                        let r = check_block_borrows(block, env, loop_from);
                        env.pop_scope();
                        r
                    }) as Box<dyn FnOnce(&mut MoveEnv) -> CompileResult<()>>
                }).collect();
            check_branches(env, branches)?;
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

        // Concurrency handles are shared references by design: the runtime
        // representation is a pointer to shared state, and the documented
        // usage pattern aliases them across tasks (`let c = counter; spawn
        // { c.lock(); ... }`). Copying the handle copies the reference, so
        // they are Copy, not move.
        ManiType::Struct(name)
            if matches!(
                name.as_str(),
                "AtomicTrit" | "Barrier" | "Semaphore" | "MutexGuard"
            ) =>
        {
            false
        }
        ManiType::Generic(name, _)
            if matches!(name.as_str(), "Mutex" | "Channel" | "Task") =>
        {
            false
        }

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

    /// Helper: build a `let` of a str literal.
    fn let_str(name: &str, val: &str) -> TypedStmt {
        TypedStmt::Let(TypedLetStmt {
            name: name.to_string(),
            pat: crate::ast::LetPat::Ident(name.to_string()),
            ty: ManiType::Str,
            init: Some(TypedExpr {
                kind: TypedExprKind::Lit(Lit::Str(val.to_string())),
                ty: ManiType::Str,
                span: Span { line: 1, col: 1 },
            }),
            mutable: false,
        })
    }

    /// Helper: build `let <name> = <src>;` where src is a str variable.
    fn let_move(name: &str, src: &str) -> TypedStmt {
        TypedStmt::Let(TypedLetStmt {
            name: name.to_string(),
            pat: crate::ast::LetPat::Ident(name.to_string()),
            ty: ManiType::Str,
            init: Some(ident_expr(src, ManiType::Str)),
            mutable: false,
        })
    }

    fn check_stmts(stmts: Vec<TypedStmt>) -> CompileResult<()> {
        let block = TypedBlock { stmts, ty: ManiType::Void };
        let mut env = MoveEnv::new();
        check_block_borrows(&block, &mut env, None)
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
        assert!(check_stmts(stmts).is_ok());
    }

    #[test]
    fn test_use_after_move_str() {
        // let s: str = "hi"; let t = s; let u = s;  -- error: use of moved 's'
        let stmts = vec![let_str("s", "hi"), let_move("t", "s"), let_move("u", "s")];
        let result = check_stmts(stmts);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("use of moved value: 's'"), "got: {}", msg);
    }

    #[test]
    fn test_move_in_loop() {
        // let s = "a"; while true { let t = s; }  -- error: move in loop
        let loop_body = TypedBlock {
            stmts: vec![let_move("t", "s")],
            ty: ManiType::Void,
        };
        let stmts = vec![
            let_str("s", "a"),
            TypedStmt::Expr(TypedExpr {
                kind: TypedExprKind::While(TypedWhileExpr {
                    cond: Box::new(TypedExpr {
                        kind: TypedExprKind::Lit(Lit::Bool(true)),
                        ty: ManiType::Bool,
                        span: Span { line: 1, col: 1 },
                    }),
                    body: loop_body,
                }),
                ty: ManiType::Void,
                span: Span { line: 1, col: 1 },
            }),
        ];
        let result = check_stmts(stmts);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("cannot move 's' in a loop"), "got: {}", msg);
    }

    #[test]
    fn test_move_of_loop_local_is_ok() {
        // while true { let a = "x"; let b = a; }  -- OK: 'a' is fresh each iteration (S15)
        let loop_body = TypedBlock {
            stmts: vec![let_str("a", "x"), let_move("b", "a")],
            ty: ManiType::Void,
        };
        let stmts = vec![TypedStmt::Expr(TypedExpr {
            kind: TypedExprKind::While(TypedWhileExpr {
                cond: Box::new(TypedExpr {
                    kind: TypedExprKind::Lit(Lit::Bool(true)),
                    ty: ManiType::Bool,
                    span: Span { line: 1, col: 1 },
                }),
                body: loop_body,
            }),
            ty: ManiType::Void,
            span: Span { line: 1, col: 1 },
        })];
        assert!(check_stmts(stmts).is_ok(), "loop-local move must be accepted");
    }

    #[test]
    fn test_rebind_clears_moved() {
        // let s: str = "a"; let t = s; let s: str = "b"; let u = s; -- OK
        let stmts = vec![
            let_str("s", "a"),
            let_move("t", "s"),
            let_str("s", "b"),
            let_move("u", "s"),
        ];
        assert!(check_stmts(stmts).is_ok());
    }

    #[test]
    fn test_tresult_arms_fork_moved_set() {
        // S16: a move in the ok arm must not poison the err arm — the three
        // arms are mutually exclusive at runtime.
        let span = Span { line: 1, col: 1 };
        let arm_block = |dst: &str| TypedBlock {
            stmts: vec![let_move(dst, "s")],
            ty: ManiType::Void,
        };
        let stmts = vec![
            let_str("s", "shared"),
            TypedStmt::Expr(TypedExpr {
                kind: TypedExprKind::Tresult(TypedTresultExpr {
                    expr: Box::new(int_lit(1)),
                    ok_var: "v".to_string(),
                    ok_block: arm_block("a"),
                    unknown_var: "u".to_string(),
                    unknown_block: arm_block("b"),
                    err_var: "e".to_string(),
                    err_block: arm_block("c"),
                }),
                ty: ManiType::Void,
                span,
            }),
        ];
        assert!(
            check_stmts(stmts).is_ok(),
            "a move in one tresult arm must not poison the others"
        );
    }

    #[test]
    fn test_shadowed_inner_move_does_not_poison_outer() {
        // S14: moving an inner shadow must not mark the outer binding moved.
        let inner = TypedBlock {
            stmts: vec![let_str("s", "inner"), let_move("t", "s")],
            ty: ManiType::Void,
        };
        let stmts = vec![
            let_str("s", "outer"),
            TypedStmt::Expr(TypedExpr {
                kind: TypedExprKind::Block(inner),
                ty: ManiType::Void,
                span: Span { line: 1, col: 1 },
            }),
            let_move("u", "s"),
        ];
        assert!(check_stmts(stmts).is_ok(), "inner-shadow move must not poison outer binding");
    }

    #[test]
    fn test_inner_let_does_not_launder_outer_move() {
        // S14 (converse): an inner-scope `let s` must not clear the OUTER
        // binding's moved flag.
        let inner = TypedBlock {
            stmts: vec![let_str("s", "inner")],
            ty: ManiType::Void,
        };
        let stmts = vec![
            let_str("s", "outer"),
            let_move("t", "s"),
            TypedStmt::Expr(TypedExpr {
                kind: TypedExprKind::Block(inner),
                ty: ManiType::Void,
                span: Span { line: 1, col: 1 },
            }),
            let_move("u", "s"),
        ];
        let result = check_stmts(stmts);
        assert!(result.is_err(), "outer move must survive an inner-scope shadow");
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("use of moved value: 's'"), "got: {}", msg);
    }

    #[test]
    fn test_reassign_after_move_is_ok() {
        // let s = "a"; let t = s; s = "b"; let u = s; -- OK (S13)
        let stmts = vec![
            let_str("s", "a"),
            let_move("t", "s"),
            TypedStmt::Assign(TypedAssignStmt {
                target: ident_expr("s", ManiType::Str),
                value: TypedExpr {
                    kind: TypedExprKind::Lit(Lit::Str("b".to_string())),
                    ty: ManiType::Str,
                    span: Span { line: 3, col: 1 },
                },
                op: None,
            }),
            let_move("u", "s"),
        ];
        assert!(check_stmts(stmts).is_ok(), "rebinding a moved variable must clear the move");
    }
}
