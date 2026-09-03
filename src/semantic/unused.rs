//! Unused-binding analysis (A16).
//!
//! The semantic pass already warns about unreachable code but said nothing
//! about bindings that are never read, so a function could bind the result of a
//! meaningful call and silently ignore it — `let allowed = enforce(cap, …);`
//! with no subsequent use reads as "the permission was checked" while the
//! answer is discarded.
//!
//! Implemented as a standalone AST walk rather than by threading usage flags
//! through the symbol table, which would require `&mut` access on every
//! lookup. Conservative on purpose: a name read anywhere in the function marks
//! every binding of that name used, so shadowing never produces a false
//! positive. A leading underscore suppresses both warnings, as elsewhere.
//!
//! Author: Manish Jagdish Thatte

use std::collections::HashSet;

use crate::ast::*;

/// What a binding was declared as but never did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnusedKind {
    /// Bound but never read.
    Variable,
    /// Declared `mut` but never assigned after initialisation.
    Mutability,
}

/// One reported binding: name, where it was declared, and what is unused.
pub struct UnusedBinding {
    pub name: String,
    pub span: Span,
    pub kind: UnusedKind,
}

/// Collect unused `let` bindings in a function body.
pub fn check_fn(f: &FnDef) -> Vec<UnusedBinding> {
    let Some(body) = &f.body else { return Vec::new() };

    let mut usage = Usage::default();
    usage.scan_block(body);

    let mut declared = Vec::new();
    collect_lets(body, &mut declared);

    let mut out = Vec::new();
    for (name, span, mutable) in declared {
        // `_name` opts out, as does the bare `_` placeholder.
        if name.starts_with('_') {
            continue;
        }
        if !usage.read.contains(&name) {
            out.push(UnusedBinding { name, span, kind: UnusedKind::Variable });
        } else if mutable && !usage.assigned.contains(&name) {
            out.push(UnusedBinding { name, span, kind: UnusedKind::Mutability });
        }
    }
    out
}

/// Names read, and names assigned to, anywhere in the function.
#[derive(Default)]
struct Usage {
    read: HashSet<String>,
    assigned: HashSet<String>,
}

/// Every `let` binding in a body, including inside nested blocks and loops.
fn collect_lets(block: &Block, out: &mut Vec<(String, Span, bool)>) {
    for stmt in &block.stmts {
        match stmt {
            Stmt::Let(ls) => {
                match &ls.pat {
                    LetPat::Ident(n) => out.push((n.clone(), ls.span, ls.mutable)),
                    // Destructuring binds several names at once; reporting one
                    // element of a tuple pattern as unused would usually be
                    // noise, so only whole-binding patterns are considered.
                    LetPat::Tuple(_) => {}
                }
                if let Some(init) = &ls.init {
                    collect_lets_in_expr(init, out);
                }
            }
            Stmt::Expr(e) => collect_lets_in_expr(e, out),
            Stmt::Assign(a) => collect_lets_in_expr(&a.value, out),
            Stmt::Return(Some(e), _) => collect_lets_in_expr(e, out),
            Stmt::Return(None, _) | Stmt::Break(_) | Stmt::Continue(_)
            | Stmt::LocalStructDef(_) => {}
            Stmt::Region(b, _) => collect_lets(b, out),
        }
    }
}

fn collect_lets_in_expr(expr: &Expr, out: &mut Vec<(String, Span, bool)>) {
    let mut blocks: Vec<&Block> = Vec::new();
    match expr {
        Expr::Block(b) => blocks.push(b),
        Expr::If(i) => {
            blocks.push(&i.then_block);
            blocks.extend(i.elif_branches.iter().map(|(_, b)| b));
            if let Some(eb) = &i.else_block {
                blocks.push(eb);
            }
        }
        Expr::Tif(t) => blocks.extend([&t.pos_block, &t.zero_block, &t.neg_block]),
        Expr::Tresult(t) => blocks.extend([&t.ok_block, &t.unknown_block, &t.err_block]),
        Expr::Loop(b, _) | Expr::Spawn(b, _) => blocks.push(b),
        Expr::While(w) => blocks.push(&w.body),
        Expr::For(fe) => blocks.push(&fe.body),
        Expr::Match(m) => {
            for arm in &m.arms {
                collect_lets_in_expr(&arm.body, out);
            }
        }
        _ => {}
    }
    for b in blocks {
        collect_lets(b, out);
    }
}

impl Usage {
    fn scan_block(&mut self, block: &Block) {
        for stmt in &block.stmts {
            self.scan_stmt(stmt);
        }
    }

    fn scan_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Let(ls) => {
                if let Some(init) = &ls.init {
                    self.scan_expr(init);
                }
            }
            Stmt::Assign(a) => {
                // A plain `x = v` writes x; a compound `x += v` also reads it,
                // as does any assignment through an index or field projection
                // (`v[i] = 1` needs v).
                match (&a.target, &a.op) {
                    (Expr::Ident(n, _), None) => {
                        self.assigned.insert(n.clone());
                    }
                    (Expr::Ident(n, _), Some(_)) => {
                        self.assigned.insert(n.clone());
                        self.read.insert(n.clone());
                    }
                    (target, _) => {
                        if let Some(base) = base_ident(target) {
                            self.assigned.insert(base.clone());
                            self.read.insert(base);
                        }
                        self.scan_expr(target);
                    }
                }
                self.scan_expr(&a.value);
            }
            Stmt::Expr(e) => self.scan_expr(e),
            Stmt::Return(Some(e), _) => self.scan_expr(e),
            Stmt::Return(None, _) | Stmt::Break(_) | Stmt::Continue(_)
            | Stmt::LocalStructDef(_) => {}
            // A use inside a region is a use: the region bounds the ALLOCATION
            // and not the scope of a name.
            Stmt::Region(b, _) => {
                for st in &b.stmts {
                    self.scan_stmt(st);
                }
            }
        }
    }

    fn scan_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Ident(n, _) => {
                self.read.insert(n.clone());
            }
            // §11.4: `yield` reads no variable.
            Expr::Yield(_) => {}
            Expr::Block(b) => self.scan_block(b),
            Expr::If(i) => {
                self.scan_expr(&i.cond);
                self.scan_block(&i.then_block);
                for (c, b) in &i.elif_branches {
                    self.scan_expr(c);
                    self.scan_block(b);
                }
                if let Some(eb) = &i.else_block {
                    self.scan_block(eb);
                }
            }
            Expr::Tif(t) => {
                self.scan_expr(&t.cond);
                self.scan_block(&t.pos_block);
                self.scan_block(&t.zero_block);
                self.scan_block(&t.neg_block);
            }
            Expr::Tresult(t) => {
                self.scan_expr(&t.expr);
                self.scan_block(&t.ok_block);
                self.scan_block(&t.unknown_block);
                self.scan_block(&t.err_block);
            }
            Expr::Match(m) => {
                self.scan_expr(&m.scrutinee);
                for arm in &m.arms {
                    if let Some(g) = &arm.guard {
                        self.scan_expr(g);
                    }
                    self.scan_expr(&arm.body);
                }
            }
            Expr::While(w) => {
                self.scan_expr(&w.cond);
                self.scan_block(&w.body);
            }
            Expr::For(fe) => {
                self.scan_expr(&fe.iter);
                self.scan_block(&fe.body);
            }
            Expr::Loop(b, _) | Expr::Spawn(b, _) => self.scan_block(b),
            Expr::Lambda(_, _, body, _) => self.scan_expr(body),
            Expr::BinOp(l, _, r, _) => {
                self.scan_expr(l);
                self.scan_expr(r);
            }
            Expr::UnOp(_, e, _)
            | Expr::Cast(e, _, _)
            | Expr::Field(e, _, _)
            | Expr::Await(e, _)
            | Expr::Question(e, _)
            | Expr::Return(e, _) => self.scan_expr(e),
            Expr::Index(a, b, _) | Expr::Range(a, b, _, _) => {
                self.scan_expr(a);
                self.scan_expr(b);
            }
            Expr::Call(c, args, _) => {
                self.scan_expr(c);
                for a in args {
                    self.scan_expr(a);
                }
            }
            Expr::MethodCall(recv, _, args, _) => {
                self.scan_expr(recv);
                for a in args {
                    self.scan_expr(a);
                }
            }
            Expr::Array(items, _) | Expr::Tuple(items, _) => {
                for i in items {
                    self.scan_expr(i);
                }
            }
            Expr::StructLit(_, fields, _) => {
                for (_, e) in fields {
                    self.scan_expr(e);
                }
            }
            Expr::Lit(..) | Expr::Break(_) | Expr::Continue(_) => {}
        }
    }
}

/// The root identifier of an assignment target such as `v[i].f`.
fn base_ident(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Ident(n, _) => Some(n.clone()),
        Expr::Index(b, _, _) | Expr::Field(b, _, _) => base_ident(b),
        _ => None,
    }
}
