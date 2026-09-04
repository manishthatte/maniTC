//! B4 — compile-time evaluation of the integer fragment.
//!
//! © Manish Jagdish Thatte
//!
//! **Why this is not `const_fold.rs`.** That module folds a `TypedExpr`: it
//! answers "is this CHECKED expression a constant", for module-level
//! initialisers and static array bounds. This one folds an `ast::Expr`, because
//! its callers are in TYPE position — the `A + 1` of `t<A + 1>` and the
//! `N * 2` of `[trit; N * 2]` — and a type is not an expression, so nothing
//! there has been type-checked and there is no `TypedExpr` to fold.
//!
//! Two evaluators that must agree is exactly the two-registries hazard
//! permanent rule 5 is about, so they are checked against each other by
//! `const_eval_tests::the_two_evaluators_agree`, over the operators both
//! support. What keeps that tractable is that this one is deliberately
//! INTEGER-ONLY: widths and lengths are integers, and a compile-time `float`
//! would have to agree with the other module about rounding as well.
//!
//! **Termination is bought, not assumed.** A `const fn` may loop, so every
//! evaluation carries a step budget and running out is an ordinary error with
//! a message, not a hang. The emulator's own runaway guard is the precedent.

use crate::ast::{BinOpKind, Block, Expr, FnDef, Lit, Stmt, UnOpKind};
use std::collections::HashMap;

/// How many evaluation steps one constant expression may take.
///
/// Generous for anything a width or a length is computed by, and small enough
/// that a mistake reports in milliseconds. A `const fn` that needs more is
/// doing work that belongs at run time.
pub const CONST_EVAL_BUDGET: u32 = 100_000;

#[derive(Debug, Clone, PartialEq)]
pub enum EvalError {
    /// The form is not in the fragment — a call to a non-`const` function, a
    /// string, a struct, anything with an effect.
    NotConstant(String),
    /// It IS a constant expression and it is broken.
    DivideByZero,
    Overflow,
    /// The budget ran out. Distinguished from `NotConstant` because the remedy
    /// is different: this one is a loop that does not converge.
    Budget,
    /// A name the fragment cannot resolve.
    Unbound(String),
}

impl EvalError {
    pub fn describe(&self) -> String {
        match self {
            EvalError::NotConstant(what) => {
                format!("is not a compile-time constant ({what})")
            }
            EvalError::DivideByZero => "divides by zero".to_string(),
            EvalError::Overflow => "overflows an int".to_string(),
            EvalError::Budget => format!(
                "did not finish within {CONST_EVAL_BUDGET} evaluation steps — a \
                 `const fn` must terminate"
            ),
            EvalError::Unbound(n) => format!("uses `{n}`, which is not a constant here"),
        }
    }
}

/// What is in scope for a constant expression.
///
/// `ints` holds `const` generic parameters and module-level constants; `fns`
/// holds every `const fn` the program declared. Both are borrowed, because the
/// analyzer owns them and an evaluation must not be able to change either.
pub struct ConstCtx<'a> {
    pub ints: &'a HashMap<String, i64>,
    pub fns: &'a HashMap<String, FnDef>,
}

struct Eval<'a> {
    ctx: &'a ConstCtx<'a>,
    /// Locals of the `const fn` frame being evaluated, innermost last.
    scopes: Vec<HashMap<String, i64>>,
    budget: u32,
    /// Guards a `const fn` that calls itself without a base case. The budget
    /// would catch it eventually; this catches it with the right message.
    depth: u32,
}

const MAX_DEPTH: u32 = 64;

/// Evaluate an integer constant expression.
pub fn eval_int(expr: &Expr, ctx: &ConstCtx) -> Result<i64, EvalError> {
    let mut e = Eval { ctx, scopes: Vec::new(), budget: CONST_EVAL_BUDGET, depth: 0 };
    e.expr(expr)
}

/// What flowed out of a statement.
enum Flow {
    Normal,
    Return(i64),
}

impl<'a> Eval<'a> {
    fn step(&mut self) -> Result<(), EvalError> {
        if self.budget == 0 {
            return Err(EvalError::Budget);
        }
        self.budget -= 1;
        Ok(())
    }

    fn lookup(&self, name: &str) -> Option<i64> {
        for s in self.scopes.iter().rev() {
            if let Some(v) = s.get(name) {
                return Some(*v);
            }
        }
        self.ctx.ints.get(name).copied()
    }

    fn assign(&mut self, name: &str, v: i64) -> bool {
        for s in self.scopes.iter_mut().rev() {
            if let std::collections::hash_map::Entry::Occupied(mut o) = s.entry(name.to_string()) {
                o.insert(v);
                return true;
            }
        }
        false
    }

    fn expr(&mut self, e: &Expr) -> Result<i64, EvalError> {
        self.step()?;
        match e {
            Expr::Lit(Lit::Int(v), _) | Expr::Lit(Lit::TernaryInt(v), _) => Ok(*v),
            Expr::Lit(Lit::Trit(t), _) => Ok(*t as i64),
            Expr::Lit(Lit::Bool(b), _) => Ok(if *b { 1 } else { 0 }),
            Expr::Lit(other, _) => Err(EvalError::NotConstant(format!("{other:?} is not an integer"))),
            Expr::Ident(n, _) => self
                .lookup(n)
                .ok_or_else(|| EvalError::Unbound(n.clone())),
            Expr::UnOp(op, inner, _) => {
                let v = self.expr(inner)?;
                match op {
                    UnOpKind::Neg => v.checked_neg().ok_or(EvalError::Overflow),
                    UnOpKind::Not => Ok(if v == 0 { 1 } else { 0 }),
                    other => Err(EvalError::NotConstant(format!("unary `{other:?}`"))),
                }
            }
            Expr::BinOp(a, op, b, _) => {
                let (x, y) = (self.expr(a)?, self.expr(b)?);
                bin(x, op, y)
            }
            Expr::Cast(inner, _, _) => self.expr(inner),
            // An `if` is an expression here as it is everywhere else in ManiT.
            Expr::If(ife) => match self.pick_branch(ife)? {
                Some(b) => self.block_value(&b),
                None => Err(EvalError::NotConstant("an `if` with no `else`".to_string())),
            },
            // `while` is an EXPRESSION in this language, not a statement, so a
            // loop inside a `const fn` arrives here.
            Expr::While(w) => {
                self.run_while(w)?;
                Err(EvalError::NotConstant("a `while` used for its value".to_string()))
            }
            Expr::Block(b) => {
                let b = b.clone();
                self.block_value(&b)
            }
            Expr::Call(callee, args, _) => {
                let Expr::Ident(name, _) = &**callee else {
                    return Err(EvalError::NotConstant("an indirect call".to_string()));
                };
                let Some(f) = self.ctx.fns.get(name).cloned() else {
                    return Err(EvalError::NotConstant(format!(
                        "`{name}` is not declared `const fn`"
                    )));
                };
                if self.depth >= MAX_DEPTH {
                    return Err(EvalError::Budget);
                }
                let mut frame = HashMap::new();
                if f.params.len() != args.len() {
                    return Err(EvalError::NotConstant(format!(
                        "`{name}` takes {} argument(s)",
                        f.params.len()
                    )));
                }
                for (p, a) in f.params.iter().zip(args.iter()) {
                    let v = self.expr(a)?;
                    frame.insert(p.name.clone(), v);
                }
                let Some(body) = &f.body else {
                    return Err(EvalError::NotConstant(format!("`{name}` has no body")));
                };
                // A CALL FRAME, not a nested scope: a `const fn` sees its own
                // parameters and the module's constants, never the caller's
                // locals. Getting this wrong would make a constant depend on
                // where it was written.
                let saved = std::mem::take(&mut self.scopes);
                self.scopes.push(frame);
                self.depth += 1;
                let out = self.block_value(body);
                self.depth -= 1;
                self.scopes = saved;
                out
            }
            other => Err(EvalError::NotConstant(format!(
                "`{}` is not in the constant fragment",
                short(other)
            ))),
        }
    }

    /// Run a block and produce its value — from a `return`, or from a tail
    /// expression, which is how ManiT blocks yield anyway.
    fn block_value(&mut self, b: &Block) -> Result<i64, EvalError> {
        self.scopes.push(HashMap::new());
        let r = self.block_inner(b);
        self.scopes.pop();
        r
    }

    fn block_inner(&mut self, b: &Block) -> Result<i64, EvalError> {
        let mut last: Option<i64> = None;
        for st in &b.stmts {
            match self.stmt(st)? {
                Flow::Return(v) => return Ok(v),
                Flow::Normal => {}
            }
            if let Stmt::Expr(e) = st {
                if !matches!(e, Expr::If(_) | Expr::While(_)) {
                    last = self.expr(e).ok();
                }
            }
        }
        last.ok_or_else(|| {
            EvalError::NotConstant("a block that produces no value".to_string())
        })
    }

    fn stmt(&mut self, st: &Stmt) -> Result<Flow, EvalError> {
        self.step()?;
        match st {
            Stmt::Let(ls) => {
                let Some(init) = &ls.init else {
                    return Err(EvalError::NotConstant(
                        "a `let` with no initialiser".to_string(),
                    ));
                };
                let v = self.expr(init)?;
                self.scopes
                    .last_mut()
                    .expect("a scope is open")
                    .insert(ls.name.clone(), v);
                Ok(Flow::Normal)
            }
            Stmt::Assign(a) => {
                let Expr::Ident(n, _) = &a.target else {
                    return Err(EvalError::NotConstant(
                        "an assignment to something other than a name".to_string(),
                    ));
                };
                let v = self.expr(&a.value)?;
                if !self.assign(n, v) {
                    return Err(EvalError::Unbound(n.clone()));
                }
                Ok(Flow::Normal)
            }
            Stmt::Return(Some(e), _) => Ok(Flow::Return(self.expr(e)?)),
            Stmt::Return(None, _) => Err(EvalError::NotConstant(
                "a `return` with no value".to_string(),
            )),
            Stmt::Expr(e) => {
                // `if` and `while` are EXPRESSIONS here, so a control-flow
                // statement arrives as `Stmt::Expr`. Both may `return`.
                match e {
                    Expr::If(ife) => {
                        match self.pick_branch(ife)? {
                            Some(b) => self.run_block(&b),
                            None => Ok(Flow::Normal),
                        }
                    }
                    Expr::While(w) => self.run_while(w),
                    _ => {
                        let _ = self.expr(e);
                        Ok(Flow::Normal)
                    }
                }
            }
            other => Err(EvalError::NotConstant(format!(
                "`{}` is not in the constant fragment",
                short_stmt(other)
            ))),
        }
    }
}

impl<'a> Eval<'a> {
    /// Which branch of an `if`/`elif`/`else` chain runs, or `None` when none
    /// does. Kept in one place so the expression form and the statement form
    /// cannot disagree about which arm was taken.
    fn pick_branch(&mut self, ife: &crate::ast::IfExpr) -> Result<Option<Block>, EvalError> {
        if self.expr(&ife.cond)? != 0 {
            return Ok(Some(ife.then_block.clone()));
        }
        for (c, b) in &ife.elif_branches {
            if self.expr(c)? != 0 {
                return Ok(Some(b.clone()));
            }
        }
        Ok(ife.else_block.clone())
    }

    /// Run a block for its FLOW rather than its value: a `return` inside it
    /// leaves the enclosing `const fn`.
    fn run_block(&mut self, b: &Block) -> Result<Flow, EvalError> {
        self.scopes.push(HashMap::new());
        let mut flow = Flow::Normal;
        for st in &b.stmts {
            match self.stmt(st) {
                Ok(Flow::Return(v)) => {
                    flow = Flow::Return(v);
                    break;
                }
                Ok(Flow::Normal) => {}
                Err(e) => {
                    self.scopes.pop();
                    return Err(e);
                }
            }
        }
        self.scopes.pop();
        Ok(flow)
    }

    fn run_while(&mut self, w: &crate::ast::WhileExpr) -> Result<Flow, EvalError> {
        loop {
            self.step()?;
            if self.expr(&w.cond)? == 0 {
                return Ok(Flow::Normal);
            }
            if let Flow::Return(v) = self.run_block(&w.body)? {
                return Ok(Flow::Return(v));
            }
        }
    }
}

/// Integer arithmetic, checked. Every operation that can overflow answers
/// `Overflow` rather than wrapping — a constant that wrapped would be a wrong
/// number computed at compile time and baked into a type.
fn bin(x: i64, op: &BinOpKind, y: i64) -> Result<i64, EvalError> {
    use BinOpKind::*;
    let b = |c: bool| Ok(if c { 1 } else { 0 });
    match op {
        Add => x.checked_add(y).ok_or(EvalError::Overflow),
        Sub => x.checked_sub(y).ok_or(EvalError::Overflow),
        Mul => x.checked_mul(y).ok_or(EvalError::Overflow),
        Div => {
            if y == 0 {
                Err(EvalError::DivideByZero)
            } else {
                x.checked_div(y).ok_or(EvalError::Overflow)
            }
        }
        Rem => {
            if y == 0 {
                Err(EvalError::DivideByZero)
            } else {
                x.checked_rem(y).ok_or(EvalError::Overflow)
            }
        }
        Lt => b(x < y),
        Gt => b(x > y),
        Le => b(x <= y),
        Ge => b(x >= y),
        Eq => b(x == y),
        Ne => b(x != y),
        And => b(x != 0 && y != 0),
        Or => b(x != 0 || y != 0),
        other => Err(EvalError::NotConstant(format!("operator `{other:?}`"))),
    }
}

fn short(e: &Expr) -> &'static str {
    match e {
        Expr::MethodCall(..) => "a method call",
        Expr::Index(..) => "an index",
        Expr::Field(..) => "a field access",
        Expr::StructLit(..) => "a struct literal",
        Expr::Array(..) => "an array literal",
        Expr::Tuple(..) => "a tuple",
        Expr::Match(..) => "a `match`",
        Expr::Tif(..) => "a `tif`",
        _ => "this expression",
    }
}

fn short_stmt(s: &Stmt) -> &'static str {
    match s {
        Stmt::Break(_) => "a `break`",
        Stmt::Continue(_) => "a `continue`",
        Stmt::Region(..) => "a `region`",
        Stmt::LocalStructDef(_) => "a local struct",
        _ => "this statement",
    }
}
