use super::*;
use crate::error::CompileResult;
use crate::lexer::TokenKind;

impl Parser {
    // ---------------------------------------------------------------------------
    // Blocks and statements
    // ---------------------------------------------------------------------------

    pub(super) fn parse_block(&mut self) -> CompileResult<Block> {
        // A3: nested blocks recurse independently of expression nesting.
        self.enter("block")?;
        let result = self.parse_block_inner();
        self.leave();
        result
    }

    fn parse_block_inner(&mut self) -> CompileResult<Block> {
        let span = self.span();
        self.expect(&TokenKind::LBrace)?;
        // Statements inside a block are a fresh expression context — the
        // no-struct-literal restriction of an enclosing condition ends here.
        let saved = self.no_struct_lit;
        self.no_struct_lit = false;
        let mut stmts = Vec::new();
        while self.peek() != &TokenKind::RBrace && !self.is_at_end() {
            let stmt = self.parse_stmt()?;
            stmts.push(stmt);
        }
        self.expect(&TokenKind::RBrace)?;
        self.no_struct_lit = saved;
        Ok(Block { stmts, span })
    }

    /// Block-like expressions may stand as statements without a trailing `;`.
    fn expr_is_block_like(e: &Expr) -> bool {
        matches!(
            e,
            Expr::Block(_) | Expr::If(_) | Expr::Tif(_) | Expr::Match(_)
                | Expr::For(_) | Expr::While(_) | Expr::Loop(..)
                | Expr::Spawn(..) | Expr::Tresult(_)
        )
    }

    pub(super) fn parse_stmt(&mut self) -> CompileResult<Stmt> {
        let span = self.span();
        let _ = span;
        match self.peek().clone() {
            TokenKind::Let => self.parse_let_stmt(),
            // Bare `mut x = val` — syntactic sugar for `let mut x = val`
            TokenKind::Mut => {
                self.advance(); // eat `mut`
                let (name, _) = self.expect_ident()?;
                let ty = if self.eat(&TokenKind::Colon) {
                    Some(self.parse_type()?)
                } else {
                    None
                };
                let init = if self.eat(&TokenKind::Eq) {
                    Some(self.parse_expr()?)
                } else {
                    None
                };
                self.expect_stmt_semi("variable declaration")?;
                Ok(Stmt::Let(LetStmt {
                    pat: LetPat::Ident(name.clone()),
                    name,
                    ty,
                    init,
                    mutable: true,
                    span,
                }))
            }
            // **F-4**: `region { ... }`. Parsed as a statement, never as an
            // expression, because a region that could produce a value could
            // hand out a pointer into the memory it is about to release.
            TokenKind::Region => {
                self.advance();
                let block = self.parse_block()?;
                Ok(Stmt::Region(block, span))
            }
            // Local struct definition inside a function body
            TokenKind::Struct => {
                let sdef = self.parse_struct_def(false)?;
                Ok(Stmt::LocalStructDef(sdef))
            }
            TokenKind::Return => {
                self.advance();
                if self.peek() == &TokenKind::Semi || self.peek() == &TokenKind::RBrace {
                    self.eat(&TokenKind::Semi);
                    Ok(Stmt::Return(None, span))
                } else {
                    let e = self.parse_expr()?;
                    self.expect_stmt_semi("return statement")?;
                    Ok(Stmt::Return(Some(e), span))
                }
            }
            TokenKind::Break => {
                self.advance();
                self.expect_stmt_semi("break")?;
                Ok(Stmt::Break(span))
            }
            TokenKind::Continue => {
                self.advance();
                self.expect_stmt_semi("continue")?;
                Ok(Stmt::Continue(span))
            }
            _ => {
                // Expression statement or assignment
                let expr = self.parse_expr()?;
                // Check for assignment operators
                let op = match self.peek() {
                    TokenKind::Eq => {
                        self.advance();
                        None // plain assignment
                    }
                    TokenKind::PlusEq => {
                        self.advance();
                        Some(BinOpKind::Add)
                    }
                    TokenKind::MinusEq => {
                        self.advance();
                        Some(BinOpKind::Sub)
                    }
                    TokenKind::StarEq => {
                        self.advance();
                        Some(BinOpKind::Mul)
                    }
                    TokenKind::SlashEq => {
                        self.advance();
                        Some(BinOpKind::Div)
                    }
                    TokenKind::PercentEq => {
                        self.advance();
                        Some(BinOpKind::Rem)
                    }
                    TokenKind::AmpersandEq => {
                        self.advance();
                        Some(BinOpKind::BitAnd)
                    }
                    TokenKind::PipeEq => {
                        self.advance();
                        Some(BinOpKind::BitOr)
                    }
                    TokenKind::CaretEq => {
                        self.advance();
                        Some(BinOpKind::BitXor)
                    }
                    TokenKind::LShiftEq => {
                        self.advance();
                        Some(BinOpKind::LShift)
                    }
                    TokenKind::RShiftEq => {
                        self.advance();
                        Some(BinOpKind::RShift)
                    }
                    _ => {
                        if !self.eat(&TokenKind::Semi) && !Self::expr_is_block_like(&expr) {
                            // No `;` — only allowed for the block tail expression.
                            if self.peek() != &TokenKind::RBrace && !self.is_at_end() {
                                return Err(self.err(format!(
                                    "expected `;` after expression, found {:?}",
                                    self.peek()
                                )));
                            }
                        }
                        return Ok(Stmt::Expr(expr));
                    }
                };
                let value = self.parse_expr()?;
                self.expect_stmt_semi("assignment")?;
                Ok(Stmt::Assign(AssignStmt { target: expr, value, op, span }))
            }
        }
    }

    pub(super) fn parse_let_stmt(&mut self) -> CompileResult<Stmt> {
        let span = self.span();
        self.expect(&TokenKind::Let)?;
        let mutable = self.eat(&TokenKind::Mut);

        // Check for tuple destructuring: let (a, b, c) = ...
        let pat = if self.peek() == &TokenKind::LParen {
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
            LetPat::Tuple(names)
        } else {
            let (name, _) = self.expect_ident()?;
            LetPat::Ident(name)
        };

        let name = pat.first_name().to_string();

        let ty = if self.eat(&TokenKind::Colon) {
            Some(self.parse_type()?)
        } else {
            None
        };
        let init = if self.eat(&TokenKind::Eq) {
            Some(self.parse_expr()?)
        } else {
            None
        };
        self.expect_stmt_semi("let declaration")?;
        Ok(Stmt::Let(LetStmt { pat, name, ty, init, mutable, span }))
    }
}
