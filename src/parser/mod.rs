use crate::ast::*;
use crate::error::{CompileError, CompileResult};
use crate::lexer::{Token, TokenKind};

pub mod types;
pub mod stmts;
pub mod exprs;

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

pub struct Parser {
    pub(super) tokens: Vec<Token>,
    pub(super) pos: usize,
    pub(super) file: String,
    /// When we split a `>>` (RShift) into two `>`, we set this flag so the
    /// next call to peek()/advance() sees the second `>` without consuming
    /// further tokens.
    pub(super) pending_gt: bool,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Parser {
            tokens,
            pos: 0,
            file: String::from("<input>"),
            pending_gt: false,
        }
    }

    pub fn with_file(tokens: Vec<Token>, file: impl Into<String>) -> Self {
        let mut p = Parser::new(tokens);
        p.file = file.into();
        p
    }

    // --- position helpers ---

    pub(super) fn peek(&self) -> &TokenKind {
        if self.pending_gt {
            return &TokenKind::Gt;
        }
        self.tokens.get(self.pos).map(|t| &t.kind).unwrap_or(&TokenKind::Eof)
    }

    pub(super) fn peek_tok(&self) -> &Token {
        &self.tokens[self.pos.min(self.tokens.len().saturating_sub(1))]
    }

    pub(super) fn peek2(&self) -> &TokenKind {
        if self.pending_gt {
            return self.tokens.get(self.pos).map(|t| &t.kind).unwrap_or(&TokenKind::Eof);
        }
        self.tokens.get(self.pos + 1).map(|t| &t.kind).unwrap_or(&TokenKind::Eof)
    }

    /// Consume a `>` token. Also handles `>>` (RShift) split: consumes the
    /// first half and leaves `pending_gt` set for the second.
    pub(super) fn eat_gt(&mut self) -> bool {
        if self.pending_gt {
            self.pending_gt = false;
            return true;
        }
        match self.peek().clone() {
            TokenKind::Gt => { self.advance(); true }
            TokenKind::RShift => {
                // Split >> into two >
                self.advance(); // consume >>
                self.pending_gt = true; // leave one > pending
                true
            }
            _ => false,
        }
    }

    pub(super) fn span(&self) -> Span {
        self.peek_tok().span
    }

    pub(super) fn advance(&mut self) -> &Token {
        let tok = &self.tokens[self.pos.min(self.tokens.len() - 1)];
        if self.pos < self.tokens.len() - 1 {
            self.pos += 1;
        }
        tok
    }

    pub(super) fn eat(&mut self, kind: &TokenKind) -> bool {
        if self.peek() == kind {
            self.advance();
            true
        } else {
            false
        }
    }

    pub(super) fn expect(&mut self, kind: &TokenKind) -> CompileResult<Span> {
        let sp = self.span();
        if self.peek() == kind {
            self.advance();
            Ok(sp)
        } else {
            Err(self.err(format!("expected {:?}, found {:?}", kind, self.peek())))
        }
    }

    pub(super) fn expect_ident(&mut self) -> CompileResult<(String, Span)> {
        let sp = self.span();
        if let TokenKind::Ident(name) = self.peek().clone() {
            self.advance();
            Ok((name, sp))
        } else {
            Err(self.err(format!("expected identifier, found {:?}", self.peek())))
        }
    }

    pub(super) fn err(&self, msg: impl Into<String>) -> CompileError {
        let sp = self.span();
        CompileError::parse(&self.file, sp.line, sp.col, msg)
    }

    pub(super) fn is_at_end(&self) -> bool {
        matches!(self.peek(), TokenKind::Eof)
    }

    // ---------------------------------------------------------------------------
    // Public entry point
    // ---------------------------------------------------------------------------

    pub fn parse(&mut self) -> CompileResult<Program> {
        let mut items = Vec::new();
        while !self.is_at_end() {
            items.push(self.parse_item()?);
        }
        Ok(Program { items })
    }

    // ---------------------------------------------------------------------------
    // Items
    // ---------------------------------------------------------------------------

    fn parse_item(&mut self) -> CompileResult<Item> {
        let is_pub = self.eat(&TokenKind::Pub);

        match self.peek().clone() {
            TokenKind::Fn | TokenKind::Async => {
                let f = self.parse_fn_def(is_pub)?;
                Ok(Item::FnDef(f))
            }
            TokenKind::Struct => {
                let s = self.parse_struct_def(is_pub)?;
                Ok(Item::StructDef(s))
            }
            TokenKind::Enum => {
                let e = self.parse_enum_def(is_pub)?;
                Ok(Item::EnumDef(e))
            }
            TokenKind::Impl => {
                let i = self.parse_impl_block()?;
                Ok(Item::ImplBlock(i))
            }
            TokenKind::Trait => {
                let t = self.parse_trait_def(is_pub)?;
                Ok(Item::TraitDef(t))
            }
            TokenKind::Use => {
                let u = self.parse_use_decl()?;
                Ok(Item::UseDecl(u))
            }
            TokenKind::Let => {
                let span = self.span();
                self.advance(); // eat `let`
                let _is_mut = self.eat(&TokenKind::Mut);
                let (name, _) = self.expect_ident()?;
                self.expect(&TokenKind::Colon)?;
                let ty = self.parse_type()?;
                let val = if self.eat(&TokenKind::Eq) {
                    Some(self.parse_expr()?)
                } else {
                    None
                };
                self.eat(&TokenKind::Semi);
                Ok(Item::GlobalVar(GlobalVar {
                    name,
                    ty,
                    val,
                    is_pub,
                    span,
                }))
            }
            _ => Err(self.err(format!("unexpected token at top level: {:?}", self.peek()))),
        }
    }

    // --- fn ---

    pub(super) fn parse_fn_def(&mut self, is_pub: bool) -> CompileResult<FnDef> {
        let span = self.span();
        let is_async = self.eat(&TokenKind::Async);
        self.expect(&TokenKind::Fn)?;
        let (name, _) = self.expect_ident()?;

        // Parse optional generic parameters <T, U, ...>
        let mut generics = Vec::new();
        if *self.peek() == TokenKind::Lt {
            self.advance(); // consume <
            loop {
                if let TokenKind::Ident(gname) = self.peek().clone() {
                    generics.push(gname.clone());
                    self.advance();
                }
                if !self.eat(&TokenKind::Comma) { break; }
            }
            self.eat_gt(); // consume >
        }

        self.expect(&TokenKind::LParen)?;

        let mut params = Vec::new();
        while self.peek() != &TokenKind::RParen {
            let pspan = self.span();
            // allow `self` parameter
            if self.peek() == &TokenKind::SelfKw {
                self.advance();
                params.push(Param {
                    name: "self".to_string(),
                    ty: Type::Named("Self".to_string(), pspan),
                    span: pspan,
                });
            } else {
                let (pname, _) = self.expect_ident()?;
                self.expect(&TokenKind::Colon)?;
                let ty = self.parse_type()?;
                params.push(Param { name: pname, ty, span: pspan });
            }
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        self.expect(&TokenKind::RParen)?;

        let ret_ty = if self.eat(&TokenKind::Arrow) {
            Some(self.parse_type()?)
        } else {
            None
        };

        let body = if self.peek() == &TokenKind::LBrace {
            Some(self.parse_block()?)
        } else {
            self.eat(&TokenKind::Semi);
            None
        };

        Ok(FnDef { name, generics, params, ret_ty, body, is_pub, is_async, span })
    }

    // --- struct ---

    pub(super) fn parse_struct_def(&mut self, is_pub: bool) -> CompileResult<StructDef> {
        let span = self.span();
        self.expect(&TokenKind::Struct)?;
        let (name, _) = self.expect_ident()?;

        // Parse optional generic parameters <T, U, ...>
        let mut generics = Vec::new();
        if *self.peek() == TokenKind::Lt {
            self.advance(); // consume <
            loop {
                if let TokenKind::Ident(gname) = self.peek().clone() {
                    generics.push(gname.clone());
                    self.advance();
                }
                if !self.eat(&TokenKind::Comma) { break; }
            }
            self.eat_gt(); // consume >
        }

        self.expect(&TokenKind::LBrace)?;

        let mut fields = Vec::new();
        while self.peek() != &TokenKind::RBrace {
            let fspan = self.span();
            let field_pub = self.eat(&TokenKind::Pub);
            let (fname, _) = self.expect_ident()?;
            self.expect(&TokenKind::Colon)?;
            let fty = self.parse_type()?;
            fields.push(FieldDef { name: fname, ty: fty, is_pub: field_pub, span: fspan });
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        self.expect(&TokenKind::RBrace)?;
        Ok(StructDef { name, generics, fields, is_pub, span })
    }

    // --- enum ---

    fn parse_enum_def(&mut self, is_pub: bool) -> CompileResult<EnumDef> {
        let span = self.span();
        self.expect(&TokenKind::Enum)?;
        let (name, _) = self.expect_ident()?;
        self.expect(&TokenKind::LBrace)?;

        let mut variants = Vec::new();
        while self.peek() != &TokenKind::RBrace {
            let vspan = self.span();
            let (vname, _) = self.expect_ident()?;
            let mut fields = Vec::new();
            if self.peek() == &TokenKind::LParen {
                self.advance();
                while self.peek() != &TokenKind::RParen {
                    fields.push(self.parse_type()?);
                    if !self.eat(&TokenKind::Comma) {
                        break;
                    }
                }
                self.expect(&TokenKind::RParen)?;
            }
            variants.push(EnumVariant { name: vname, fields, span: vspan });
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        self.expect(&TokenKind::RBrace)?;
        Ok(EnumDef { name, variants, is_pub, span })
    }

    // --- impl ---

    fn parse_impl_block(&mut self) -> CompileResult<ImplBlock> {
        let span = self.span();
        self.expect(&TokenKind::Impl)?;
        let (first, _) = self.expect_ident()?;

        // impl Trait for Type  OR  impl Type
        // `for` is a keyword so we check TokenKind::For directly
        let (ty, trait_) = if self.peek() == &TokenKind::For {
            self.advance(); // eat `for`
            let (type_name, _) = self.expect_ident()?;
            (type_name, Some(first))
        } else {
            (first, None)
        };

        self.expect(&TokenKind::LBrace)?;
        let mut methods = Vec::new();
        while self.peek() != &TokenKind::RBrace {
            let is_pub = self.eat(&TokenKind::Pub);
            let m = self.parse_fn_def(is_pub)?;
            methods.push(m);
        }
        self.expect(&TokenKind::RBrace)?;
        Ok(ImplBlock { ty, trait_, methods, span })
    }

    // --- trait ---

    fn parse_trait_def(&mut self, is_pub: bool) -> CompileResult<TraitDef> {
        let span = self.span();
        self.expect(&TokenKind::Trait)?;
        let (name, _) = self.expect_ident()?;
        self.expect(&TokenKind::LBrace)?;
        let mut methods = Vec::new();
        while self.peek() != &TokenKind::RBrace {
            let method_pub = self.eat(&TokenKind::Pub);
            let m = self.parse_fn_def(method_pub)?;
            methods.push(m);
        }
        self.expect(&TokenKind::RBrace)?;
        Ok(TraitDef { name, methods, is_pub, span })
    }

    // --- use ---

    /// Accept both identifiers AND keywords as module path segments.
    /// This allows `use async::spawn`, `use io::println`, etc.
    pub(super) fn expect_path_segment(&mut self) -> CompileResult<String> {
        let _sp = self.span();
        let seg = match self.peek().clone() {
            TokenKind::Ident(name) => name,
            // Allow common module-name keywords as path segments
            TokenKind::Async => "async".to_string(),
            // literal keyword names used as module identifiers
            tok => {
                // Check if this keyword could be a module name
                let s = match &tok {
                    TokenKind::IntKw => "int".to_string(),
                    TokenKind::FloatKw => "float".to_string(),
                    TokenKind::CharKw => "char".to_string(),
                    TokenKind::StrKw => "str".to_string(),
                    TokenKind::VoidKw => "void".to_string(),
                    TokenKind::Bool3Kw => "bool3".to_string(),
                    TokenKind::TritKw => "trit".to_string(),
                    TokenKind::TryteKw => "tryte".to_string(),
                    TokenKind::T9Kw => "t9".to_string(),
                    TokenKind::T27Kw => "t27".to_string(),
                    TokenKind::T54Kw => "t54".to_string(),
                    TokenKind::TrintKw => "trint".to_string(),
                    TokenKind::TfloatKw => "tfloat".to_string(),
                    _ => {
                        return Err(self.err(format!("expected module path segment, found {:?}", tok)));
                    }
                };
                s
            }
        };
        self.advance();
        Ok(seg)
    }

    fn parse_use_decl(&mut self) -> CompileResult<UseDecl> {
        let span = self.span();
        self.expect(&TokenKind::Use)?;
        let mut path = Vec::new();
        // First segment may be a keyword (e.g. `async`)
        let seg = self.expect_path_segment()?;
        path.push(seg);
        while self.eat(&TokenKind::ColonColon) {
            let seg = self.expect_path_segment()?;
            path.push(seg);
        }
        self.eat(&TokenKind::Semi);
        Ok(UseDecl { path, span })
    }
}
