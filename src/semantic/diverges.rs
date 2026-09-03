//! Control-flow divergence analysis (A1).
//!
//! Answers one question: does this block always leave the function, on every
//! path? A non-void function whose body can fall off the end returns whatever
//! happens to be in the return slot — for `-> str` that is an uninitialised
//! pointer, which the print path then dereferences (T3 printed raw memory,
//! LLVM printed `(null)`). This is the dual of the existing unreachable-code
//! warning: that one asks "is anything after the terminator", this one asks
//! "is there a path with no terminator at all".
//!
//! The analysis is deliberately CONSERVATIVE in one direction: when unsure it
//! answers `false` ("might fall through"), which at worst asks the author for
//! an explicit `return`. It never claims a path diverges when it might not, so
//! it cannot mask the unsafety it exists to catch.
//!
//! Author: Manish Jagdish Thatte

use crate::ast::*;

/// True when `block` leaves the enclosing function on every path — via
/// `return`, or by diverging (an infinite `loop`, or a call to a function that
/// never returns such as `env::exit`).
pub fn block_diverges(block: &Block) -> bool {
    // Any diverging statement makes the whole block diverge: everything after
    // it is unreachable (which the unreachable-code warning already reports).
    block.stmts.iter().any(stmt_diverges)
}

fn stmt_diverges(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Return(..) => true,
        Stmt::Expr(e) => expr_diverges(e),
        // A `let` whose initialiser diverges never binds anything.
        Stmt::Let(ls) => ls.init.as_ref().is_some_and(expr_diverges),
        Stmt::Assign(a) => expr_diverges(&a.value),
        // `break`/`continue` leave a loop, not the function. Treated as
        // non-diverging here; `loop_diverges` accounts for them separately.
        Stmt::Break(_) | Stmt::Continue(_) => false,
        Stmt::LocalStructDef(_) => false,
        // A region diverges when its body does: it is a block with a bump
        // pointer saved around it, and saving a pointer cannot return.
        Stmt::Region(b, _) => block_diverges(b),
    }
}

fn expr_diverges(expr: &Expr) -> bool {
    match expr {
        Expr::Return(..) => true,
        // §11.4: `yield` suspends and RESUMES. It is not divergence — the
        // statement after it runs, just later.
        Expr::Yield(_) => false,

        // A conditional diverges only if every arm exists and diverges.
        Expr::If(if_expr) => {
            let Some(else_block) = &if_expr.else_block else {
                // No else: the implicit empty path falls through.
                return false;
            };
            block_diverges(&if_expr.then_block)
                && if_expr.elif_branches.iter().all(|(_, b)| block_diverges(b))
                && block_diverges(else_block)
        }

        // `tif` is total over the three trit states, so all three arms count.
        Expr::Tif(t) => {
            block_diverges(&t.pos_block)
                && block_diverges(&t.zero_block)
                && block_diverges(&t.neg_block)
        }

        // `tresult` is likewise total over Ok / Unknown / Err.
        Expr::Tresult(t) => {
            block_diverges(&t.ok_block)
                && block_diverges(&t.unknown_block)
                && block_diverges(&t.err_block)
        }

        // A match diverges when every arm does. Exhaustiveness is not checked
        // here; assuming the match is total is the lenient direction, so an
        // author who covers every case with a `return` is not nagged.
        Expr::Match(m) => !m.arms.is_empty() && m.arms.iter().all(|a| expr_diverges(&a.body)),

        Expr::Block(b) => block_diverges(b),

        // `loop` with no `break` out of it never finishes.
        Expr::Loop(body, _) => !block_has_break(body),

        // `while`/`for` may run zero times, so they never guarantee divergence.
        Expr::While(_) | Expr::For(_) => false,

        // A call to a function that never returns ends the current one.
        Expr::Call(callee, args, _) => {
            is_never_returning_call(callee) || args.iter().any(expr_diverges)
        }

        // Operand positions: the expression diverges if evaluating an operand
        // does. `&&`/`||` short-circuit, so only the left operand is certain.
        Expr::BinOp(l, op, r, _) => {
            expr_diverges(l)
                || (!matches!(op, BinOpKind::And | BinOpKind::Or) && expr_diverges(r))
        }
        Expr::UnOp(_, e, _)
        | Expr::Cast(e, _, _)
        | Expr::Field(e, _, _)
        | Expr::Await(e, _)
        | Expr::Question(e, _) => expr_diverges(e),
        Expr::Index(a, b, _) => expr_diverges(a) || expr_diverges(b),
        Expr::MethodCall(recv, _, args, _) => {
            expr_diverges(recv) || args.iter().any(expr_diverges)
        }
        Expr::Array(items, _) | Expr::Tuple(items, _) => items.iter().any(expr_diverges),
        Expr::StructLit(_, fields, _) => fields.iter().any(|(_, e)| expr_diverges(e)),
        Expr::Range(a, b, _, _) => expr_diverges(a) || expr_diverges(b),

        // A lambda body runs later, not here. `spawn` likewise.
        Expr::Lambda(..) | Expr::Spawn(..) => false,

        Expr::Lit(..) | Expr::Ident(..) | Expr::Break(_) | Expr::Continue(_) => false,
    }
}

/// Calls that never return, so code after them is unreachable.
///
/// `env::exit` and `env::abort` both end the process (`exit`/`abort` in the C
/// runtime, the corresponding syscalls on T3), and `panic` is the same idea at
/// the language level.
fn is_never_returning_call(callee: &Expr) -> bool {
    let name = match callee {
        Expr::Ident(n, _) => n.clone(),
        Expr::Field(base, field, _) => match &**base {
            Expr::Ident(m, _) => format!("{}::{}", m, field),
            _ => return false,
        },
        _ => return false,
    };
    matches!(
        name.as_str(),
        "env::exit" | "env::abort" | "panic" | "process::exit" | "process::abort"
    )
}

/// Whether a `break` can leave *this* loop — that is, a `break` not enclosed in
/// a nested loop, which would bind to the inner loop instead.
fn block_has_break(block: &Block) -> bool {
    block.stmts.iter().any(stmt_has_break)
}

fn stmt_has_break(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Break(_) => true,
        Stmt::Expr(e) => expr_has_break(e),
        Stmt::Let(ls) => ls.init.as_ref().is_some_and(expr_has_break),
        Stmt::Assign(a) => expr_has_break(&a.value),
        Stmt::Return(..) | Stmt::Continue(_) | Stmt::LocalStructDef(_) => false,
        // A `break` inside a region is refused by the borrow pass (it would
        // skip the release), but this function runs first and must still
        // answer honestly about the syntax in front of it.
        Stmt::Region(b, _) => b.stmts.iter().any(stmt_has_break),
    }
}

fn expr_has_break(expr: &Expr) -> bool {
    match expr {
        Expr::Break(_) => true,
        // Nested loops capture their own `break`s.
        Expr::Loop(..) | Expr::While(_) | Expr::For(_) => false,
        // A lambda body cannot break out of an enclosing loop.
        Expr::Lambda(..) | Expr::Spawn(..) => false,
        Expr::Yield(_) => false,
        Expr::Block(b) => block_has_break(b),
        Expr::If(i) => {
            block_has_break(&i.then_block)
                || i.elif_branches.iter().any(|(_, b)| block_has_break(b))
                || i.else_block.as_ref().is_some_and(block_has_break)
        }
        Expr::Tif(t) => {
            block_has_break(&t.pos_block)
                || block_has_break(&t.zero_block)
                || block_has_break(&t.neg_block)
        }
        Expr::Tresult(t) => {
            block_has_break(&t.ok_block)
                || block_has_break(&t.unknown_block)
                || block_has_break(&t.err_block)
        }
        Expr::Match(m) => m.arms.iter().any(|a| expr_has_break(&a.body)),
        Expr::BinOp(l, _, r, _) => expr_has_break(l) || expr_has_break(r),
        Expr::UnOp(_, e, _)
        | Expr::Cast(e, _, _)
        | Expr::Field(e, _, _)
        | Expr::Await(e, _)
        | Expr::Question(e, _)
        | Expr::Return(e, _) => expr_has_break(e),
        Expr::Index(a, b, _) => expr_has_break(a) || expr_has_break(b),
        Expr::Call(c, args, _) => expr_has_break(c) || args.iter().any(expr_has_break),
        Expr::MethodCall(recv, _, args, _) => {
            expr_has_break(recv) || args.iter().any(expr_has_break)
        }
        Expr::Array(items, _) | Expr::Tuple(items, _) => items.iter().any(expr_has_break),
        Expr::StructLit(_, fields, _) => fields.iter().any(|(_, e)| expr_has_break(e)),
        Expr::Range(a, b, _, _) => expr_has_break(a) || expr_has_break(b),
        Expr::Lit(..) | Expr::Ident(..) | Expr::Continue(_) => false,
    }
}

/// True when a block's final statement is a bare tail expression of non-void
/// value — ManiT's implicit-return form, `fn f() -> int { 1 }`.
///
/// Such a block does not "diverge" but does supply the return value, so it is
/// accepted by the A1 check.
pub fn block_has_value_tail(block: &Block) -> bool {
    match block.stmts.last() {
        Some(Stmt::Expr(e)) => expr_can_be_tail_value(e),
        _ => false,
    }
}

/// Whether a tail expression yields a value rather than being a statement.
///
/// Control-flow forms count when every arm supplies one, mirroring the
/// divergence rules above so `fn f() -> int { if c { 1 } else { 2 } }` is
/// accepted.
fn expr_can_be_tail_value(expr: &Expr) -> bool {
    match expr {
        Expr::If(i) => match &i.else_block {
            Some(eb) => {
                block_supplies_value(&i.then_block)
                    && i.elif_branches.iter().all(|(_, b)| block_supplies_value(b))
                    && block_supplies_value(eb)
            }
            None => false,
        },
        Expr::Tif(t) => {
            block_supplies_value(&t.pos_block)
                && block_supplies_value(&t.zero_block)
                && block_supplies_value(&t.neg_block)
        }
        Expr::Tresult(t) => {
            block_supplies_value(&t.ok_block)
                && block_supplies_value(&t.unknown_block)
                && block_supplies_value(&t.err_block)
        }
        Expr::Match(m) => !m.arms.is_empty() && m.arms.iter().all(|a| expr_can_be_tail_value(&a.body)),
        Expr::Block(b) => block_supplies_value(b),
        // Statement-like forms produce no value.
        Expr::While(_) | Expr::For(_) | Expr::Break(_) | Expr::Continue(_) => false,
        // `loop` used as a tail expression only yields via `break value`, which
        // the language does not have; treat as diverging instead.
        Expr::Loop(..) => false,
        // Everything else (literals, calls, operators, …) is a value.
        _ => true,
    }
}

/// A block supplies a value if it diverges (never falls through) or ends in a
/// value tail expression.
fn block_supplies_value(block: &Block) -> bool {
    block_diverges(block) || block_has_value_tail(block)
}
