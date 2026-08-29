//! Recursive-descent parser for the ManiT core, for the reference interpreter.
//!
//! © Manish Jagdish Thatte
//!
//! Precedence follows docs/semantics.md §2 exactly, loosest to tightest:
//! `||`; `&&`; the three-valued and lane-wise operators (one level, left
//! associative); comparison (non-associative); `+ -`; `* / %`; unary; `as`.
//!
//! Independence rule: see lex.rs.

use super::ast::*;
use super::lex::Tok;

pub struct P {
    t: Vec<Tok>,
    i: usize,
}

type R<T> = Result<T, String>;

impl P {
    pub fn new(t: Vec<Tok>) -> Self { P { t, i: 0 } }

    fn peek(&self) -> &Tok { &self.t[self.i] }
    fn peek2(&self) -> &Tok { self.t.get(self.i + 1).unwrap_or(&Tok::Eof) }
    fn bump(&mut self) -> Tok { let t = self.t[self.i].clone(); self.i += 1; t }
    fn eat(&mut self, t: &Tok) -> bool {
        if self.peek() == t { self.i += 1; true } else { false }
    }
    fn expect(&mut self, t: &Tok) -> R<()> {
        if self.eat(t) { Ok(()) } else {
            Err(format!("expected {:?}, found {:?}", t, self.peek()))
        }
    }
    fn ident(&mut self) -> R<String> {
        match self.bump() {
            Tok::Ident(s) => Ok(s),
            other => Err(format!("expected identifier, found {:?}", other)),
        }
    }

    // ---- program ----------------------------------------------------------

    pub fn program(&mut self) -> R<Vec<Fn>> {
        let mut fns = Vec::new();
        loop {
            match self.peek() {
                Tok::Eof => break,
                // `use std::io;` and friends carry no core meaning. Skipped by
                // BUMPING to the semicolon — `eat` only advances on a match, so
                // a loop written over `eat` spins forever on the first token
                // that is neither the separator nor EOF.
                Tok::Use => {
                    self.bump();
                    loop {
                        match self.bump() {
                            Tok::Semi => break,
                            Tok::Eof => return Err("unterminated `use`".into()),
                            _ => {}
                        }
                    }
                }
                Tok::Fn => fns.push(self.fndef()?),
                other => return Err(format!("only `fn` and `use` are in the core, found {:?}", other)),
            }
        }
        Ok(fns)
    }

    fn ty(&mut self) -> R<Ty> {
        Ok(match self.bump() {
            Tok::TyInt => Ty::Int,
            Tok::TyTrit => Ty::Trit,
            Tok::TyBool3 => Ty::Bool3,
            Tok::TyBool => Ty::Bool,
            Tok::TyVoid => Ty::Void,
            // `Result<T, str>`. The error type is parsed and required to be
            // `str`, rather than ignored: accepting `Result<int, int>` and then
            // evaluating it as though the error were a string would be exactly
            // the kind of quiet mismatch this interpreter exists to catch.
            Tok::Ident(n) if n == "Result" => {
                self.expect(&Tok::Lt)?;
                let ok = self.ty()?;
                self.expect(&Tok::Comma)?;
                let e = self.ty()?;
                if e != Ty::Str {
                    return Err("the core fixes Result's error type to `str`".into());
                }
                self.expect(&Tok::Gt)?;
                Ty::Result(Box::new(ok))
            }
            Tok::TyChan => Ty::Chan,
            Tok::Ident(n) if n == "str" => Ty::Str,
            other => return Err(format!("not a core type: {:?}", other)),
        })
    }

    fn fndef(&mut self) -> R<Fn> {
        self.expect(&Tok::Fn)?;
        let name = self.ident()?;
        self.expect(&Tok::LParen)?;
        let mut params = Vec::new();
        while !self.eat(&Tok::RParen) {
            let p = self.ident()?;
            self.expect(&Tok::Colon)?;
            let t = self.ty()?;
            params.push((p, t));
            if !self.eat(&Tok::Comma) { self.expect(&Tok::RParen)?; break; }
        }
        let ret = if self.eat(&Tok::Arrow) { self.ty()? } else { Ty::Void };
        let body = self.block()?;
        Ok(Fn { name, params, ret, body })
    }

    fn block(&mut self) -> R<Vec<Stmt>> {
        self.expect(&Tok::LBrace)?;
        let mut out = Vec::new();
        while !self.eat(&Tok::RBrace) {
            if *self.peek() == Tok::Eof { return Err("unterminated block".into()); }
            out.push(self.stmt()?);
        }
        Ok(out)
    }

    /// A `tif` arm is either a block or a single expression.
    fn arm(&mut self) -> R<Vec<Stmt>> {
        if *self.peek() == Tok::LBrace { self.block() }
        else { Ok(vec![Stmt::Expr(self.expr()?)]) }
    }

    fn stmt(&mut self) -> R<Stmt> {
        match self.peek().clone() {
            Tok::Let => {
                self.bump();
                let mutable = self.eat(&Tok::Mut);
                let name = self.ident()?;
                let ty = if self.eat(&Tok::Colon) { Some(self.ty()?) } else { None };
                self.expect(&Tok::Assign)?;
                let init = self.expr()?;
                self.expect(&Tok::Semi)?;
                Ok(Stmt::Let { name, mutable, ty, init })
            }
            Tok::If => { self.bump(); self.if_tail() }
            Tok::Tif => {
                self.bump();
                let scrutinee = self.expr()?;
                self.expect(&Tok::LBrace)?;
                // All three arms required, in any order, each named once.
                let mut pos = None; let mut zero = None; let mut neg = None;
                for _ in 0..3 {
                    let which = self.bump();
                    self.expect(&Tok::FatArrow)?;
                    let body = self.arm()?;
                    match which {
                        Tok::Plus => { if pos.is_some() { return Err("duplicate `+` arm".into()); } pos = Some(body); }
                        Tok::Int(0) => { if zero.is_some() { return Err("duplicate `0` arm".into()); } zero = Some(body); }
                        Tok::Minus => { if neg.is_some() { return Err("duplicate `-` arm".into()); } neg = Some(body); }
                        other => return Err(format!("a tif arm must be `+`, `0` or `-`, found {:?}", other)),
                    }
                    self.eat(&Tok::Comma);
                }
                self.expect(&Tok::RBrace)?;
                match (pos, zero, neg) {
                    (Some(p), Some(z), Some(n)) =>
                        Ok(Stmt::Tif { scrutinee, pos: p, zero: z, neg: n }),
                    _ => Err("tif requires `+`, `0` and `-` arms".into()),
                }
            }
            // A `match` in statement position takes no trailing semicolon,
            // the same as `if`, `tif` and `while`. It is parsed as an
            // expression (it can produce a value) and then accepted as a
            // statement without one.
            Tok::Match => {
                let e = self.expr()?;
                self.eat(&Tok::Semi);
                Ok(Stmt::Expr(e))
            }
            Tok::While => {
                self.bump();
                let cond = self.expr()?;
                let body = self.block()?;
                Ok(Stmt::While { cond, body })
            }
            // §11.5 (SPAWN). No semicolon after the block, like `while`.
            Tok::Spawn => {
                self.bump();
                let body = self.block()?;
                Ok(Stmt::Spawn(body))
            }
            // §11.5 (YIELD).
            Tok::Yield => {
                self.bump();
                self.expect(&Tok::Semi)?;
                Ok(Stmt::Yield)
            }
            Tok::Return => {
                self.bump();
                if self.eat(&Tok::Semi) { return Ok(Stmt::Return(None)); }
                let e = self.expr()?;
                self.expect(&Tok::Semi)?;
                Ok(Stmt::Return(Some(e)))
            }
            // assignment vs expression: `x = ...` where x is a bare ident
            Tok::Ident(name) if *self.peek2() == Tok::Assign => {
                self.bump(); self.bump();
                let val = self.expr()?;
                self.expect(&Tok::Semi)?;
                Ok(Stmt::Assign { name, val })
            }
            _ => {
                let e = self.expr()?;
                self.expect(&Tok::Semi)?;
                Ok(Stmt::Expr(e))
            }
        }
    }

    fn if_tail(&mut self) -> R<Stmt> {
        let mut arms = Vec::new();
        let c = self.expr()?;
        let b = self.block()?;
        arms.push((c, b));
        let mut els = None;
        loop {
            if self.eat(&Tok::Elif) {
                let c = self.expr()?;
                let b = self.block()?;
                arms.push((c, b));
            } else if self.eat(&Tok::Else) {
                els = Some(self.block()?);
                break;
            } else { break; }
        }
        Ok(Stmt::If { arms, els })
    }

    // ---- expressions ------------------------------------------------------

    pub fn expr(&mut self) -> R<Expr> { self.or_expr() }

    fn or_expr(&mut self) -> R<Expr> {
        let mut l = self.and_expr()?;
        while self.eat(&Tok::OrOr) {
            let r = self.and_expr()?;
            l = Expr::Bin(Bin::OrOr, Box::new(l), Box::new(r));
        }
        Ok(l)
    }

    fn and_expr(&mut self) -> R<Expr> {
        let mut l = self.tlogic_expr()?;
        while self.eat(&Tok::AndAnd) {
            let r = self.tlogic_expr()?;
            l = Expr::Bin(Bin::AndAnd, Box::new(l), Box::new(r));
        }
        Ok(l)
    }

    fn tlogic_expr(&mut self) -> R<Expr> {
        let mut l = self.cmp_expr()?;
        loop {
            let op = match self.peek() {
                Tok::Tand => Bin::Tand, Tok::Tor => Bin::Tor, Tok::Txor => Bin::Txor,
                Tok::Tcon => Bin::Tcon, Tok::Tany => Bin::Tany,
                Tok::Timp => Bin::Timp, Tok::Teq => Bin::Teq,
                Tok::Tandw => Bin::Tandw, Tok::Torw => Bin::Torw,
                Tok::Txorw => Bin::Txorw, Tok::Timpw => Bin::Timpw,
                Tok::Tcmpw => Bin::Tcmpw,
                _ => break,
            };
            self.bump();
            let r = self.cmp_expr()?;
            l = Expr::Bin(op, Box::new(l), Box::new(r));
        }
        Ok(l)
    }

    fn cmp_expr(&mut self) -> R<Expr> {
        let l = self.add_expr()?;
        let op = match self.peek() {
            Tok::Eq => Bin::Eq, Tok::Ne => Bin::Ne,
            Tok::Lt => Bin::Lt, Tok::Gt => Bin::Gt,
            Tok::Le => Bin::Le, Tok::Ge => Bin::Ge,
            _ => return Ok(l),
        };
        self.bump();
        let r = self.add_expr()?;
        // Non-associative: `a < b < c` is a syntax error, not `(a<b)<c`.
        if matches!(self.peek(), Tok::Eq | Tok::Ne | Tok::Lt | Tok::Gt | Tok::Le | Tok::Ge) {
            return Err("comparison operators cannot be chained".into());
        }
        Ok(Expr::Bin(op, Box::new(l), Box::new(r)))
    }

    fn add_expr(&mut self) -> R<Expr> {
        let mut l = self.mul_expr()?;
        loop {
            let op = match self.peek() { Tok::Plus => Bin::Add, Tok::Minus => Bin::Sub, _ => break };
            self.bump();
            let r = self.mul_expr()?;
            l = Expr::Bin(op, Box::new(l), Box::new(r));
        }
        Ok(l)
    }

    fn mul_expr(&mut self) -> R<Expr> {
        let mut l = self.unary()?;
        loop {
            let op = match self.peek() {
                Tok::Star => Bin::Mul, Tok::Slash => Bin::Div, Tok::Percent => Bin::Rem,
                _ => break,
            };
            self.bump();
            let r = self.unary()?;
            l = Expr::Bin(op, Box::new(l), Box::new(r));
        }
        Ok(l)
    }

    /// Can this token begin an expression? Used only to disambiguate a prefix
    /// `-`: it is unary minus when an operand follows, and the trit literal
    /// `-` when one does not (`let t: trit = -;`). §2 calls this rule
    /// positional, and this predicate is what makes it so.
    fn starts_expr(t: &Tok) -> bool {
        matches!(t,
            Tok::Int(_) | Tok::Ident(_) | Tok::Str(_) | Tok::LParen
            | Tok::True | Tok::False | Tok::B3True | Tok::B3Unknown | Tok::B3False
            | Tok::Minus | Tok::Plus
            | Tok::Tnot | Tok::Tposs | Tok::Tnec | Tok::Tnotw)
    }

    fn unary(&mut self) -> R<Expr> {
        let e = match self.peek().clone() {
            Tok::Minus => {
                self.bump();
                if Self::starts_expr(self.peek()) {
                    Expr::Un(Un::Neg, Box::new(self.unary()?))
                } else {
                    Expr::TritLit(-1)
                }
            }
            // There is no unary plus in the core, so a prefix `+` is the trit
            // literal, unconditionally.
            Tok::Plus => { self.bump(); Expr::TritLit(1) }
            Tok::Tnot => { self.bump(); Expr::Un(Un::Tnot, Box::new(self.unary()?)) }
            Tok::Tposs => { self.bump(); Expr::Un(Un::Tposs, Box::new(self.unary()?)) }
            Tok::Tnec => { self.bump(); Expr::Un(Un::Tnec, Box::new(self.unary()?)) }
            Tok::Tnotw => { self.bump(); Expr::Un(Un::Tnotw, Box::new(self.unary()?)) }
            _ => self.postfix()?,
        };
        Ok(e)
    }

    fn postfix(&mut self) -> R<Expr> {
        let mut e = self.primary()?;
        loop {
            if self.eat(&Tok::As) {
                let t = self.ty()?;
                e = Expr::Cast(Box::new(e), t);
            } else if self.eat(&Tok::Question) {
                e = Expr::Try(Box::new(e));
            } else if self.eat(&Tok::Dot) {
                let name = self.ident()?;
                self.expect(&Tok::LParen)?;
                let mut args = Vec::new();
                while !self.eat(&Tok::RParen) {
                    args.push(self.expr()?);
                    if !self.eat(&Tok::Comma) { self.expect(&Tok::RParen)?; break; }
                }
                e = Expr::Method(Box::new(e), name, args);
            } else {
                break;
            }
        }
        Ok(e)
    }

    fn primary(&mut self) -> R<Expr> {
        match self.bump() {
            Tok::Int(v) => Ok(Expr::Int(v)),
            Tok::Str(s) => Ok(Expr::Str(s)),
            Tok::True => Ok(Expr::BoolLit(true)),
            Tok::False => Ok(Expr::BoolLit(false)),
            Tok::B3True => Ok(Expr::Bool3Lit(1)),
            // `Unknown` is BOTH the bool3 literal and the `Result`
            // constructor, and the language disambiguates them positionally:
            // followed by `(` it constructs a Result, otherwise it is the
            // literal. Worth stating because it is the only keyword in the
            // core that names two different things.
            Tok::B3Unknown => {
                if *self.peek() == Tok::LParen {
                    self.bump();
                    let mut args = Vec::new();
                    while !self.eat(&Tok::RParen) {
                        args.push(self.expr()?);
                        if !self.eat(&Tok::Comma) { self.expect(&Tok::RParen)?; break; }
                    }
                    Ok(Expr::Call("Unknown".to_string(), args))
                } else {
                    Ok(Expr::Bool3Lit(0))
                }
            }
            Tok::B3False => Ok(Expr::Bool3Lit(-1)),
            Tok::LParen => {
                let e = self.expr()?;
                self.expect(&Tok::RParen)?;
                Ok(e)
            }
            Tok::Match => {
                let scrut = self.expr()?;
                self.expect(&Tok::LBrace)?;
                let mut arms = Vec::new();
                while !self.eat(&Tok::RBrace) {
                    if *self.peek() == Tok::Eof { return Err("unterminated match".into()); }
                    let (variant, binding) = match self.bump() {
                        Tok::Underscore => ("_".to_string(), None),
                        // Same collision as in `primary`: the pattern
                        // `Unknown(m)` lexes as the bool3 literal.
                        Tok::B3Unknown => {
                            self.expect(&Tok::LParen)?;
                            let b = self.ident()?;
                            self.expect(&Tok::RParen)?;
                            ("Unknown".to_string(), Some(b))
                        }
                        Tok::Ident(v) => {
                            self.expect(&Tok::LParen)?;
                            let b = self.ident()?;
                            self.expect(&Tok::RParen)?;
                            (v, Some(b))
                        }
                        other => return Err(format!("not a Result pattern: {:?}", other)),
                    };
                    self.expect(&Tok::FatArrow)?;
                    let body = self.arm()?;
                    self.eat(&Tok::Comma);
                    arms.push(MatchArm { variant, binding, body });
                }
                // semantics.md: `Result` is a three-variant closed type and a
                // `match` on one must cover all three, or say `_`. Enforced
                // HERE, in the parser, so an out-of-scope program is refused
                // rather than quietly mis-evaluated.
                let has_wild = arms.iter().any(|a| a.variant == "_");
                if !has_wild {
                    for want in ["Ok", "Unknown", "Err"] {
                        if !arms.iter().any(|a| a.variant == want) {
                            return Err(format!(
                                "non-exhaustive match on Result — missing `{}`", want));
                        }
                    }
                }
                Ok(Expr::Match(Box::new(scrut), arms))
            }
            Tok::Ident(first) => {
                // A qualified name: `io::println_int`. Joined with "::" so the
                // evaluator can match on the whole path.
                let mut name = first;
                while self.eat(&Tok::ColonColon) {
                    name.push_str("::");
                    name.push_str(&self.ident()?);
                }
                if self.eat(&Tok::LParen) {
                    let mut args = Vec::new();
                    while !self.eat(&Tok::RParen) {
                        args.push(self.expr()?);
                        if !self.eat(&Tok::Comma) { self.expect(&Tok::RParen)?; break; }
                    }
                    Ok(Expr::Call(name, args))
                } else {
                    Ok(Expr::Var(name))
                }
            }
            other => Err(format!("not an expression in the core: {:?}", other)),
        }
    }
}
