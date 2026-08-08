use super::*;
use crate::error::CompileResult;
use crate::lexer::TokenKind;

impl Parser {
    // ---------------------------------------------------------------------------
    // Expression parsing — recursive descent with precedence climbing
    // ---------------------------------------------------------------------------

    pub fn parse_expr(&mut self) -> CompileResult<Expr> {
        self.parse_range_expr()
    }

    // Range: lowest precedence above assignment
    fn parse_range_expr(&mut self) -> CompileResult<Expr> {
        let lhs = self.parse_or_expr()?;
        let span = lhs.span();
        match self.peek().clone() {
            TokenKind::DotDot => {
                self.advance();
                let rhs = self.parse_or_expr()?;
                Ok(Expr::Range(Box::new(lhs), Box::new(rhs), false, span))
            }
            TokenKind::DotDotEq => {
                self.advance();
                let rhs = self.parse_or_expr()?;
                Ok(Expr::Range(Box::new(lhs), Box::new(rhs), true, span))
            }
            _ => Ok(lhs),
        }
    }

    // || (logical or)
    fn parse_or_expr(&mut self) -> CompileResult<Expr> {
        let mut lhs = self.parse_and_expr()?;
        while self.peek() == &TokenKind::OrOr {
            let span = lhs.span();
            self.advance();
            let rhs = self.parse_and_expr()?;
            lhs = Expr::BinOp(Box::new(lhs), BinOpKind::Or, Box::new(rhs), span);
        }
        Ok(lhs)
    }

    // && (logical and)
    fn parse_and_expr(&mut self) -> CompileResult<Expr> {
        let mut lhs = self.parse_ternary_logic_expr()?;
        while self.peek() == &TokenKind::AndAnd {
            let span = lhs.span();
            self.advance();
            let rhs = self.parse_ternary_logic_expr()?;
            lhs = Expr::BinOp(Box::new(lhs), BinOpKind::And, Box::new(rhs), span);
        }
        Ok(lhs)
    }

    // ternary logic: tor, tand, txor, tcon, tany
    fn parse_ternary_logic_expr(&mut self) -> CompileResult<Expr> {
        let mut lhs = self.parse_cmp_expr()?;
        loop {
            let op = match self.peek() {
                TokenKind::Tor => BinOpKind::Tor,
                TokenKind::Tand => BinOpKind::Tand,
                TokenKind::Txor => BinOpKind::Txor,
                TokenKind::Tcon => BinOpKind::Tcon,
                TokenKind::Tany => BinOpKind::Tany,
                _ => break,
            };
            let span = lhs.span();
            self.advance();
            let rhs = self.parse_cmp_expr()?;
            lhs = Expr::BinOp(Box::new(lhs), op, Box::new(rhs), span);
        }
        Ok(lhs)
    }

    // comparison: ==, !=, <, >, <=, >=
    fn parse_cmp_expr(&mut self) -> CompileResult<Expr> {
        let mut lhs = self.parse_bitor_expr()?;
        loop {
            let op = match self.peek() {
                TokenKind::EqEq => BinOpKind::Eq,
                TokenKind::BangEq => BinOpKind::Ne,
                TokenKind::Lt => BinOpKind::Lt,
                TokenKind::Gt => BinOpKind::Gt,
                TokenKind::LtEq => BinOpKind::Le,
                TokenKind::GtEq => BinOpKind::Ge,
                _ => break,
            };
            let span = lhs.span();
            self.advance();
            let rhs = self.parse_bitor_expr()?;
            lhs = Expr::BinOp(Box::new(lhs), op, Box::new(rhs), span);
        }
        Ok(lhs)
    }

    // bitwise |
    fn parse_bitor_expr(&mut self) -> CompileResult<Expr> {
        let mut lhs = self.parse_bitxor_expr()?;
        while self.peek() == &TokenKind::Pipe {
            let span = lhs.span();
            self.advance();
            let rhs = self.parse_bitxor_expr()?;
            lhs = Expr::BinOp(Box::new(lhs), BinOpKind::BitOr, Box::new(rhs), span);
        }
        Ok(lhs)
    }

    // bitwise ^
    fn parse_bitxor_expr(&mut self) -> CompileResult<Expr> {
        let mut lhs = self.parse_bitand_expr()?;
        while self.peek() == &TokenKind::Caret {
            let span = lhs.span();
            self.advance();
            let rhs = self.parse_bitand_expr()?;
            lhs = Expr::BinOp(Box::new(lhs), BinOpKind::BitXor, Box::new(rhs), span);
        }
        Ok(lhs)
    }

    // bitwise &
    fn parse_bitand_expr(&mut self) -> CompileResult<Expr> {
        let mut lhs = self.parse_shift_expr()?;
        while self.peek() == &TokenKind::Ampersand {
            let span = lhs.span();
            self.advance();
            let rhs = self.parse_shift_expr()?;
            lhs = Expr::BinOp(Box::new(lhs), BinOpKind::BitAnd, Box::new(rhs), span);
        }
        Ok(lhs)
    }

    // shift: <<, >>
    fn parse_shift_expr(&mut self) -> CompileResult<Expr> {
        let mut lhs = self.parse_add_expr()?;
        loop {
            let op = match self.peek() {
                TokenKind::LShift => BinOpKind::LShift,
                TokenKind::RShift => BinOpKind::RShift,
                _ => break,
            };
            let span = lhs.span();
            self.advance();
            let rhs = self.parse_add_expr()?;
            lhs = Expr::BinOp(Box::new(lhs), op, Box::new(rhs), span);
        }
        Ok(lhs)
    }

    // additive: +, -
    fn parse_add_expr(&mut self) -> CompileResult<Expr> {
        let mut lhs = self.parse_mul_expr()?;
        loop {
            let op = match self.peek() {
                TokenKind::Plus => BinOpKind::Add,
                TokenKind::Minus => BinOpKind::Sub,
                _ => break,
            };
            let span = lhs.span();
            self.advance();
            let rhs = self.parse_mul_expr()?;
            lhs = Expr::BinOp(Box::new(lhs), op, Box::new(rhs), span);
        }
        Ok(lhs)
    }

    // multiplicative: *, /, %
    fn parse_mul_expr(&mut self) -> CompileResult<Expr> {
        let mut lhs = self.parse_cast_expr()?;
        loop {
            let op = match self.peek() {
                TokenKind::Star => BinOpKind::Mul,
                TokenKind::Slash => BinOpKind::Div,
                TokenKind::Percent => BinOpKind::Rem,
                _ => break,
            };
            let span = lhs.span();
            self.advance();
            let rhs = self.parse_cast_expr()?;
            lhs = Expr::BinOp(Box::new(lhs), op, Box::new(rhs), span);
        }
        Ok(lhs)
    }

    // cast: expr as Type
    fn parse_cast_expr(&mut self) -> CompileResult<Expr> {
        let mut expr = self.parse_unary_expr()?;
        while self.peek() == &TokenKind::As {
            let span = expr.span();
            self.advance();
            let ty = self.parse_type()?;
            expr = Expr::Cast(Box::new(expr), ty, span);
        }
        Ok(expr)
    }

    // unary: -, !, ~, tnot, &, *
    fn parse_unary_expr(&mut self) -> CompileResult<Expr> {
        let span = self.span();
        match self.peek().clone() {
            TokenKind::Minus => {
                self.advance();
                // If `-` is followed by a delimiter, it is a trit literal -1
                match self.peek() {
                    TokenKind::Comma | TokenKind::RBracket | TokenKind::RParen
                    | TokenKind::RBrace | TokenKind::Semi | TokenKind::FatArrow
                    | TokenKind::Eof => {
                        return Ok(Expr::Lit(Lit::Trit(-1), span));
                    }
                    _ => {}
                }
                let expr = self.parse_unary_expr()?;
                Ok(Expr::UnOp(UnOpKind::Neg, Box::new(expr), span))
            }
            TokenKind::Bang => {
                self.advance();
                let expr = self.parse_unary_expr()?;
                Ok(Expr::UnOp(UnOpKind::Not, Box::new(expr), span))
            }
            TokenKind::Tilde => {
                self.advance();
                let expr = self.parse_unary_expr()?;
                Ok(Expr::UnOp(UnOpKind::Not, Box::new(expr), span))
            }
            TokenKind::Tnot => {
                self.advance();
                let expr = self.parse_unary_expr()?;
                Ok(Expr::UnOp(UnOpKind::Tnot, Box::new(expr), span))
            }
            TokenKind::Ampersand => {
                self.advance();
                let _mutable = self.eat(&TokenKind::Mut);
                let expr = self.parse_unary_expr()?;
                Ok(Expr::UnOp(UnOpKind::Ref, Box::new(expr), span))
            }
            TokenKind::Star => {
                self.advance();
                let expr = self.parse_unary_expr()?;
                Ok(Expr::UnOp(UnOpKind::Deref, Box::new(expr), span))
            }
            // Unary + used as trit literal +1
            TokenKind::Plus => {
                self.advance();
                // Treat standalone `+` as Trit(+1)
                Ok(Expr::Lit(Lit::Trit(1), span))
            }
            _ => self.parse_postfix_expr(),
        }
    }

    // postfix: call, index, field, .await, ?
    fn parse_postfix_expr(&mut self) -> CompileResult<Expr> {
        let mut expr = self.parse_primary()?;
        loop {
            match self.peek().clone() {
                TokenKind::LParen => {
                    let span = expr.span();
                    self.advance();
                    let args = self.parse_call_args()?;
                    self.expect(&TokenKind::RParen)?;
                    expr = Expr::Call(Box::new(expr), args, span);
                }
                TokenKind::LBracket => {
                    let span = expr.span();
                    self.advance();
                    let idx = self.parse_expr()?;
                    self.expect(&TokenKind::RBracket)?;
                    expr = Expr::Index(Box::new(expr), Box::new(idx), span);
                }
                TokenKind::Dot => {
                    let span = expr.span();
                    self.advance();
                    if self.peek() == &TokenKind::Await {
                        self.advance();
                        expr = Expr::Await(Box::new(expr), span);
                    } else {
                        let (field, _) = self.expect_ident()?;
                        if self.peek() == &TokenKind::LParen {
                            self.advance();
                            let args = self.parse_call_args()?;
                            self.expect(&TokenKind::RParen)?;
                            expr = Expr::MethodCall(Box::new(expr), field, args, span);
                        } else {
                            expr = Expr::Field(Box::new(expr), field, span);
                        }
                    }
                }
                TokenKind::Question => {
                    let span = expr.span();
                    self.advance();
                    expr = Expr::Question(Box::new(expr), span);
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    fn parse_call_args(&mut self) -> CompileResult<Vec<Expr>> {
        let mut args = Vec::new();
        while self.peek() != &TokenKind::RParen && !self.is_at_end() {
            args.push(self.parse_expr()?);
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        Ok(args)
    }

    // primary expressions
    fn parse_primary(&mut self) -> CompileResult<Expr> {
        let span = self.span();
        match self.peek().clone() {
            // Integer literal
            TokenKind::Int(n) => {
                self.advance();
                Ok(Expr::Lit(Lit::Int(n), span))
            }
            // Float literal
            TokenKind::Float(f) => {
                self.advance();
                Ok(Expr::Lit(Lit::Float(f), span))
            }
            // String literal
            TokenKind::Str(s) => {
                self.advance();
                Ok(Expr::Lit(Lit::Str(s), span))
            }
            // Char literal
            TokenKind::Char(c) => {
                self.advance();
                Ok(Expr::Lit(Lit::Char(c), span))
            }
            // Balanced ternary integer
            TokenKind::TernaryInt(n) => {
                self.advance();
                Ok(Expr::Lit(Lit::TernaryInt(n), span))
            }
            // Bool literals
            TokenKind::True => {
                self.advance();
                Ok(Expr::Lit(Lit::Bool(true), span))
            }
            TokenKind::False => {
                self.advance();
                Ok(Expr::Lit(Lit::Bool(false), span))
            }
            TokenKind::Unknown => {
                self.advance();
                // `Unknown(...)` is a Result constructor; standalone `Unknown` is bool3 literal
                if self.peek() == &TokenKind::LParen {
                    Ok(Expr::Ident("Unknown".to_string(), span))
                } else {
                    Ok(Expr::Lit(Lit::Bool3(0), span))
                }
            }

            // Identifiers (possibly struct literal, path, or plain ident)
            TokenKind::Ident(name) => {
                self.advance();
                // Check for path separator or struct literal
                if self.peek() == &TokenKind::ColonColon {
                    // Could be a path: a::b::c or a::b { }
                    let mut path = vec![name];
                    while self.eat(&TokenKind::ColonColon) {
                        if let TokenKind::Ident(seg) = self.peek().clone() {
                            self.advance();
                            path.push(seg);
                        } else {
                            break;
                        }
                    }
                    if self.peek() == &TokenKind::LBrace {
                        // Struct literal with path name
                        let struct_name = path.join("::");
                        let fields = self.parse_struct_lit_fields()?;
                        Ok(Expr::StructLit(struct_name, fields, span))
                    } else {
                        // Path expression — represent as nested Field or just Ident for now
                        // Simplification: emit as Ident with joined path
                        Ok(Expr::Ident(path.join("::"), span))
                    }
                } else if self.peek() == &TokenKind::LBrace {
                    // Could be struct literal — we try to parse it as such
                    // Peek ahead: if next is `ident:`, it's a struct lit, else a block
                    if self.is_struct_lit() {
                        let fields = self.parse_struct_lit_fields()?;
                        Ok(Expr::StructLit(name, fields, span))
                    } else {
                        // Just the identifier, block is separate
                        Ok(Expr::Ident(name, span))
                    }
                } else {
                    Ok(Expr::Ident(name, span))
                }
            }

            // Grouped expression or tuple
            TokenKind::LParen => {
                self.advance();
                if self.eat(&TokenKind::RParen) {
                    return Ok(Expr::Tuple(vec![], span));
                }
                let first = self.parse_expr()?;
                if self.eat(&TokenKind::Comma) {
                    let mut elems = vec![first];
                    while self.peek() != &TokenKind::RParen && !self.is_at_end() {
                        elems.push(self.parse_expr()?);
                        if !self.eat(&TokenKind::Comma) {
                            break;
                        }
                    }
                    self.expect(&TokenKind::RParen)?;
                    Ok(Expr::Tuple(elems, span))
                } else {
                    self.expect(&TokenKind::RParen)?;
                    Ok(first)
                }
            }

            // Array literal
            TokenKind::LBracket => {
                self.advance();
                let mut elems = Vec::new();
                while self.peek() != &TokenKind::RBracket && !self.is_at_end() {
                    elems.push(self.parse_expr()?);
                    if !self.eat(&TokenKind::Comma) {
                        break;
                    }
                }
                self.expect(&TokenKind::RBracket)?;
                Ok(Expr::Array(elems, span))
            }

            // Block expression
            TokenKind::LBrace => {
                let block = self.parse_block()?;
                Ok(Expr::Block(block))
            }

            // if expression
            TokenKind::If => self.parse_if_expr(),

            // tif expression
            TokenKind::Tif => self.parse_tif_expr(),

            // tresult expression
            TokenKind::Tresult => self.parse_tresult_expr(),

            // match expression
            TokenKind::Match => self.parse_match_expr(),

            // for expression
            TokenKind::For => self.parse_for_expr(),

            // while expression
            TokenKind::While => self.parse_while_expr(),

            // loop expression
            TokenKind::Loop => {
                self.advance();
                let block = self.parse_block()?;
                Ok(Expr::Loop(Box::new(block), span))
            }

            // spawn expression
            TokenKind::Spawn => {
                self.advance();
                let block = self.parse_block()?;
                Ok(Expr::Spawn(Box::new(block), span))
            }

            // Keyword-prefixed module paths: async::foo(), time::sleep(), etc.
            TokenKind::Async => {
                self.advance();
                self.expect(&TokenKind::ColonColon)?;
                let (seg, _) = self.expect_ident()?;
                Ok(Expr::Ident(format!("async::{}", seg), span))
            }

            // channel<T>() constructor
            TokenKind::Channel => {
                self.advance(); // eat `channel`
                self.expect(&TokenKind::Lt)?; // eat `<`
                let _ty = self.parse_type()?; // parse element type (ignored at runtime)
                self.expect(&TokenKind::Gt)?; // eat `>`
                self.expect(&TokenKind::LParen)?;
                self.expect(&TokenKind::RParen)?;
                // Represent as call to builtin "channel"
                Ok(Expr::Call(
                    Box::new(Expr::Ident("channel".to_string(), span)),
                    vec![],
                    span,
                ))
            }

            // return expression
            TokenKind::Return => {
                self.advance();
                let e = if self.peek() == &TokenKind::Semi || self.peek() == &TokenKind::RBrace {
                    Expr::Lit(Lit::Null, span)
                } else {
                    self.parse_expr()?
                };
                Ok(Expr::Return(Box::new(e), span))
            }

            // break / continue
            TokenKind::Break => {
                self.advance();
                Ok(Expr::Break(span))
            }
            TokenKind::Continue => {
                self.advance();
                Ok(Expr::Continue(span))
            }

            // SelfKw
            TokenKind::SelfKw => {
                self.advance();
                Ok(Expr::Ident("self".to_string(), span))
            }

            // Lambda / anonymous function: fn(params) -> ret => expr
            TokenKind::Fn => {
                self.advance(); // consume `fn`
                self.expect(&TokenKind::LParen)?;
                let mut params: Vec<(String, Type)> = Vec::new();
                while self.peek() != &TokenKind::RParen && !self.is_at_end() {
                    let (pname, _) = self.expect_ident()?;
                    self.expect(&TokenKind::Colon)?;
                    let pty = self.parse_type()?;
                    params.push((pname, pty));
                    if !self.eat(&TokenKind::Comma) { break; }
                }
                self.expect(&TokenKind::RParen)?;
                let ret_ty = if self.eat(&TokenKind::Arrow) {
                    Some(self.parse_type()?)
                } else {
                    None
                };
                // Body: => expr  OR  { block }
                let body = if self.eat(&TokenKind::FatArrow) {
                    self.parse_expr()?
                } else {
                    let block = self.parse_block()?;
                    Expr::Block(block)
                };
                Ok(Expr::Lambda(params, ret_ty, Box::new(body), span))
            }

            tok => Err(self.err(format!("unexpected token in expression: {:?}", tok))),
        }
    }

    // ---------------------------------------------------------------------------
    // Struct literal helper
    // ---------------------------------------------------------------------------

    fn is_struct_lit(&self) -> bool {
        // Look ahead: after `{`, if we see `ident :`, `..`, or `}` it's a struct literal.
        let saved = self.pos + 1; // pos is at `{`, so pos+1 is the token after
        if let Some(tok) = self.tokens.get(saved) {
            // { } — empty struct literal
            if matches!(tok.kind, TokenKind::RBrace) {
                return true;
            }
            // { ident : ... } — normal struct literal
            if let TokenKind::Ident(_) = &tok.kind {
                if let Some(tok2) = self.tokens.get(saved + 1) {
                    return matches!(tok2.kind, TokenKind::Colon);
                }
            }
            // { ..base, ... } — struct update syntax
            if matches!(tok.kind, TokenKind::DotDot) {
                return true;
            }
        }
        false
    }

    fn parse_struct_lit_fields(&mut self) -> CompileResult<Vec<(String, Expr)>> {
        self.expect(&TokenKind::LBrace)?;
        let mut fields = Vec::new();

        // Struct update syntax: { ..base_expr, field: val, ... }
        // Encoded as a sentinel field ("__spread__", base_expr) so the AST
        // type is unchanged; the semantic analyzer expands it.
        if self.eat(&TokenKind::DotDot) {
            let base = self.parse_expr()?;
            fields.push(("__spread__".to_string(), base));
            // Trailing comma before the explicit overrides is optional
            self.eat(&TokenKind::Comma);
        }

        while self.peek() != &TokenKind::RBrace && !self.is_at_end() {
            let (fname, _) = self.expect_ident()?;
            self.expect(&TokenKind::Colon)?;
            let val = self.parse_expr()?;
            fields.push((fname, val));
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        self.expect(&TokenKind::RBrace)?;
        Ok(fields)
    }

    // ---------------------------------------------------------------------------
    // Control flow parsers
    // ---------------------------------------------------------------------------

    pub(super) fn parse_if_expr(&mut self) -> CompileResult<Expr> {
        let span = self.span();
        self.expect(&TokenKind::If)?;
        let cond = self.parse_expr()?;
        let then_block = self.parse_block()?;

        let mut elif_branches = Vec::new();
        let mut else_block = None;

        loop {
            if self.peek() == &TokenKind::Elif {
                self.advance();
                let econd = self.parse_expr()?;
                let eblock = self.parse_block()?;
                elif_branches.push((econd, eblock));
            } else if self.peek() == &TokenKind::Else {
                self.advance();
                else_block = Some(self.parse_block()?);
                break;
            } else {
                break;
            }
        }

        Ok(Expr::If(IfExpr {
            cond: Box::new(cond),
            then_block,
            elif_branches,
            else_block,
            span,
        }))
    }

    pub(super) fn parse_tif_expr(&mut self) -> CompileResult<Expr> {
        // Syntax:  tif <expr> { + => <expr>, 0 => <expr>, - => <expr> }
        // Arms may appear in any order; all three are required.
        let span = self.span();
        self.expect(&TokenKind::Tif)?;
        let cond = self.parse_expr()?;
        self.expect(&TokenKind::LBrace)?;

        let mut pos_block: Option<Block> = None;
        let mut zero_block: Option<Block> = None;
        let mut neg_block: Option<Block> = None;

        while self.peek() != &TokenKind::RBrace && !self.is_at_end() {
            let _arm_span = self.span();
            let arm_label = match self.peek().clone() {
                TokenKind::Plus  => { self.advance(); 1i8  }
                TokenKind::Int(0) => { self.advance(); 0i8  }
                TokenKind::Minus => { self.advance(); -1i8 }
                other => return Err(self.err(format!(
                    "tif arm must start with +, 0, or -, found {:?}", other))),
            };
            self.expect(&TokenKind::FatArrow)?;
            // Accept either a block `{ ... }` or a single expression followed by optional comma
            let block = if self.peek() == &TokenKind::LBrace {
                self.parse_block()?
            } else {
                let e = self.parse_expr()?;
                let block_span = e.span();
                Block { stmts: vec![Stmt::Expr(e)], span: block_span }
            };
            self.eat(&TokenKind::Comma);
            match arm_label {
                1  => pos_block  = Some(block),
                0  => zero_block = Some(block),
                -1 => neg_block  = Some(block),
                _  => unreachable!(),
            }
        }
        self.expect(&TokenKind::RBrace)?;

        let empty_block = Block { stmts: vec![], span };
        Ok(Expr::Tif(TifExpr {
            cond: Box::new(cond),
            pos_block:  pos_block.unwrap_or(empty_block.clone()),
            zero_block: zero_block.unwrap_or(empty_block.clone()),
            neg_block:  neg_block.unwrap_or(empty_block),
            span,
        }))
    }

    pub(super) fn parse_match_expr(&mut self) -> CompileResult<Expr> {
        let span = self.span();
        self.expect(&TokenKind::Match)?;
        let scrutinee = self.parse_expr()?;
        self.expect(&TokenKind::LBrace)?;

        let mut arms = Vec::new();
        while self.peek() != &TokenKind::RBrace && !self.is_at_end() {
            let aspan = self.span();
            let pattern = self.parse_pattern()?;
            let guard = if self.peek() == &TokenKind::If {
                self.advance();
                Some(self.parse_expr()?)
            } else {
                None
            };
            self.expect(&TokenKind::FatArrow)?;
            let body = self.parse_expr()?;
            let is_block_body = matches!(body, Expr::Block(..));
            arms.push(MatchArm { pattern, guard, body, span: aspan });
            // Comma is optional after block bodies (common style); required otherwise
            if !self.eat(&TokenKind::Comma) && !is_block_body {
                break;
            }
        }
        self.expect(&TokenKind::RBrace)?;
        Ok(Expr::Match(MatchExpr { scrutinee: Box::new(scrutinee), arms, span }))
    }

    pub(super) fn parse_for_expr(&mut self) -> CompileResult<Expr> {
        let span = self.span();
        self.expect(&TokenKind::For)?;

        // Support tuple destructuring: for (a, b) in expr { ... }
        // For now, use the first name as the loop variable (full destructuring handled in semantic/IR)
        let var = if self.peek() == &TokenKind::LParen {
            self.advance(); // eat `(`
            let mut names = Vec::new();
            while self.peek() != &TokenKind::RParen && !self.is_at_end() {
                let (n, _) = self.expect_ident()?;
                names.push(n);
                if !self.eat(&TokenKind::Comma) {
                    break;
                }
            }
            self.expect(&TokenKind::RParen)?;
            if names.len() > 1 {
                return Err(self.err("tuple destructuring in for-loop bindings is not yet supported — use a single loop variable"));
            }
            names.into_iter().next().unwrap_or_else(|| "_".to_string())
        } else {
            let (v, _) = self.expect_ident()?;
            v
        };

        self.expect(&TokenKind::In)?;
        let iter = self.parse_expr()?;
        let body = self.parse_block()?;
        Ok(Expr::For(ForExpr { var, iter: Box::new(iter), body, span }))
    }

    pub(super) fn parse_tresult_expr(&mut self) -> CompileResult<Expr> {
        // Syntax: tresult <expr> {
        //     Ok(v)      => <expr_or_block>,
        //     Unknown(h) => <expr_or_block>,
        //     Err(e)     => <expr_or_block>,
        // }
        // All three arms are required. Arms may appear in any order.
        let span = self.span();
        self.expect(&TokenKind::Tresult)?;
        let expr = self.parse_expr()?;
        self.expect(&TokenKind::LBrace)?;

        let mut ok_arm: Option<(String, Block)> = None;
        let mut unknown_arm: Option<(String, Block)> = None;
        let mut err_arm: Option<(String, Block)> = None;

        while self.peek() != &TokenKind::RBrace && !self.is_at_end() {
            // Arm label: Ident "(" ident ")" "=>"
            let label = if let TokenKind::Ident(s) = self.peek().clone() { s }
                else {
                    return Err(self.err("tresult arm must start with Ok, Unknown, or Err"));
                };
            self.advance();
            self.expect(&TokenKind::LParen)?;
            let (var, _) = self.expect_ident()?;
            self.expect(&TokenKind::RParen)?;
            self.expect(&TokenKind::FatArrow)?;
            let block = if self.peek() == &TokenKind::LBrace {
                self.parse_block()?
            } else {
                let e = self.parse_expr()?;
                let bs = e.span();
                Block { stmts: vec![Stmt::Expr(e)], span: bs }
            };
            self.eat(&TokenKind::Comma);
            match label.as_str() {
                "Ok"      => ok_arm      = Some((var, block)),
                "Unknown" => unknown_arm = Some((var, block)),
                "Err"     => err_arm     = Some((var, block)),
                other => return Err(self.err(format!(
                    "tresult arm must be Ok, Unknown, or Err — found '{}'", other))),
            }
        }
        self.expect(&TokenKind::RBrace)?;

        let empty = Block { stmts: vec![], span };
        let (ok_var, ok_block) = ok_arm.unwrap_or_else(|| ("_".to_string(), empty.clone()));
        let (unknown_var, unknown_block) = unknown_arm.unwrap_or_else(|| ("_".to_string(), empty.clone()));
        let (err_var, err_block) = err_arm.unwrap_or_else(|| ("_".to_string(), empty.clone()));
        Ok(Expr::Tresult(crate::ast::TresultExpr {
            expr: Box::new(expr),
            ok_var, ok_block,
            unknown_var, unknown_block,
            err_var, err_block,
            span,
        }))
    }

    pub(super) fn parse_while_expr(&mut self) -> CompileResult<Expr> {
        let span = self.span();
        self.expect(&TokenKind::While)?;
        let cond = self.parse_expr()?;
        let body = self.parse_block()?;
        Ok(Expr::While(WhileExpr { cond: Box::new(cond), body, span }))
    }
}
