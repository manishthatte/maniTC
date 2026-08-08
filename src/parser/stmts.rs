use super::*;
use crate::error::CompileResult;
use crate::lexer::TokenKind;

impl Parser {
    // ---------------------------------------------------------------------------
    // Blocks and statements
    // ---------------------------------------------------------------------------

    pub(super) fn parse_block(&mut self) -> CompileResult<Block> {
        let span = self.span();
        self.expect(&TokenKind::LBrace)?;
        let mut stmts = Vec::new();
        while self.peek() != &TokenKind::RBrace && !self.is_at_end() {
            let stmt = self.parse_stmt()?;
            stmts.push(stmt);
        }
        self.expect(&TokenKind::RBrace)?;
        Ok(Block { stmts, span })
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
                self.eat(&TokenKind::Semi);
                Ok(Stmt::Let(LetStmt {
                    pat: LetPat::Ident(name.clone()),
                    name,
                    ty,
                    init,
                    mutable: true,
                    span,
                }))
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
                    self.eat(&TokenKind::Semi);
                    Ok(Stmt::Return(Some(e), span))
                }
            }
            TokenKind::Break => {
                self.advance();
                self.eat(&TokenKind::Semi);
                Ok(Stmt::Break(span))
            }
            TokenKind::Continue => {
                self.advance();
                self.eat(&TokenKind::Semi);
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
                        self.eat(&TokenKind::Semi);
                        return Ok(Stmt::Expr(expr));
                    }
                };
                let value = self.parse_expr()?;
                self.eat(&TokenKind::Semi);
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
        self.eat(&TokenKind::Semi);
        Ok(Stmt::Let(LetStmt { pat, name, ty, init, mutable, span }))
    }
}
