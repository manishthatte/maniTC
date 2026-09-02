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
    /// The synthesized second `>` of a split `>>`, returned by peek_tok()
    /// and advance() while `pending_gt` is set.
    pub(super) split_gt: Token,
    /// While set, `ident { ... }` is NOT parsed as a struct literal. Used in
    /// condition / iterator / scrutinee position (`if`, `while`, `for`,
    /// `match`, `tif`, `tresult`) where the `{` belongs to the construct's
    /// body. Cleared inside any parenthesized/bracketed subexpression.
    pub(super) no_struct_lit: bool,
    /// Current recursive-descent nesting depth (A3). Guards against native
    /// stack exhaustion on pathologically nested input, which used to abort
    /// the process with no diagnostic.
    pub(super) depth: usize,
}

/// Maximum recursive-descent nesting depth (A3).
///
/// Deeply nested source — `((((…1…))))`, `1 + (1 + (1 + …))`, nested blocks —
/// used to overflow the native stack and abort with "has overflowed its stack"
/// and no file:line. Refuse past this depth with an ordinary parse error
/// instead. Real code nests far shallower: the deepest construct across the
/// examples, the stdlib and all of thatteos is under 20. main() reserves a
/// large stack (COMPILER_STACK_BYTES) so this limit is reachable, and so that
/// the later passes, which recurse over the same tree, survive it too.
pub const MAX_PARSE_DEPTH: usize = 256;

/// Maximum element count for the `[value; N]` array repeat form (A4).
///
/// The parser expands the form eagerly into N clones of the element
/// expression, so N is a direct multiplier on compiler memory. Bounded far
/// above real use (the largest repeat count in the examples, the stdlib and
/// thatteos is 54) but low enough that a typo cannot exhaust memory.
pub const MAX_ARRAY_REPEAT: i64 = 65_536;

impl Parser {
    pub fn new(mut tokens: Vec<Token>) -> Self {
        // Guarantee at least one token so the cursor helpers never index an
        // empty vector (the lexer always appends Eof, but callers may not).
        if tokens.is_empty() {
            tokens.push(Token::new(TokenKind::Eof, Span::zero()));
        }
        Parser {
            tokens,
            pos: 0,
            file: String::from("<input>"),
            pending_gt: false,
            split_gt: Token::new(TokenKind::Gt, Span::zero()),
            no_struct_lit: false,
            depth: 0,
        }
    }

    // --- recursion depth guard (A3) ---

    /// Enter one level of recursive descent. Returns an ordinary parse error
    /// once the nesting limit is reached, so pathological input is rejected
    /// with a file:line instead of aborting the process on a stack overflow.
    /// Every `enter` must be paired with a `leave` on the success path.
    pub(super) fn enter(&mut self, what: &str) -> CompileResult<()> {
        self.depth += 1;
        if self.depth > MAX_PARSE_DEPTH {
            return Err(self.err(format!(
                "{} nested too deeply (limit {}); simplify the expression \
                 or split it across bindings",
                what, MAX_PARSE_DEPTH,
            )));
        }
        Ok(())
    }

    /// Leave one level of recursive descent.
    pub(super) fn leave(&mut self) {
        self.depth = self.depth.saturating_sub(1);
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
        if self.pending_gt {
            return &self.split_gt;
        }
        &self.tokens[self.pos.min(self.tokens.len().saturating_sub(1))]
    }

    pub(super) fn peek2(&self) -> &TokenKind {
        self.peek_at(1)
    }

    /// The token `n` positions ahead of the cursor, counting the pending `>`
    /// of a split `>>` as position 0 so lookahead agrees with `peek()`.
    pub(super) fn peek_at(&self, n: usize) -> &TokenKind {
        if self.pending_gt {
            if n == 0 {
                return &TokenKind::Gt;
            }
            return self
                .tokens
                .get(self.pos + n - 1)
                .map(|t| &t.kind)
                .unwrap_or(&TokenKind::Eof);
        }
        self.tokens.get(self.pos + n).map(|t| &t.kind).unwrap_or(&TokenKind::Eof)
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
                let sp = self.span();
                self.advance(); // consume >>
                self.split_gt = Token::new(TokenKind::Gt, Span::new(sp.line, sp.col + 1));
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
        // A pending `>` from a split `>>` must be consumed first, otherwise
        // peek() and advance() desynchronize.
        if self.pending_gt {
            self.pending_gt = false;
            return &self.split_gt;
        }
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

    pub(super) fn err_at(&self, sp: Span, msg: impl Into<String>) -> CompileError {
        CompileError::parse(&self.file, sp.line, sp.col, msg)
    }

    /// Require the `;` that terminates a statement. The semicolon may only be
    /// omitted before a closing `}` (block tail expression) or at end of
    /// input; anywhere else, two statements running together is an error.
    pub(super) fn expect_stmt_semi(&mut self, what: &str) -> CompileResult<()> {
        if self.eat(&TokenKind::Semi)
            || self.peek() == &TokenKind::RBrace
            || self.is_at_end()
        {
            Ok(())
        } else {
            Err(self.err(format!("expected `;` after {}, found {:?}", what, self.peek())))
        }
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
            TokenKind::Extern => {
                let e = self.parse_extern_decl(is_pub)?;
                Ok(Item::ExternDecl(e))
            }
            // A5: `lint deny(shadowing);`. Recognised contextually — `lint` is
            // an identifier everywhere else, and only the shape `lint <level>(`
            // is claimed, so a variable or function called `lint` is unaffected.
            TokenKind::Ident(ref w) if w == "lint" && self.lint_item_ahead() => {
                let l = self.parse_lint_decl()?;
                Ok(Item::LintDecl(l))
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
                self.expect_stmt_semi("global variable declaration")?;
                Ok(Item::GlobalVar(GlobalVar {
                    name,
                    ty,
                    val,
                    is_pub,
                    span,
                }))
            }
            TokenKind::Mod => Err(self.err(
                "`mod` is a reserved keyword but module blocks are not supported yet — \
                 ManiT uses one module per file; move this code into its own file",
            )),
            _ => Err(self.err(format!("unexpected token at top level: {:?}", self.peek()))),
        }
    }

    // --- fn ---

    /// **B7's D-2.** Consume a `move` annotation before a parameter's type.
    ///
    /// `move` is CONTEXTUAL, not reserved: `stdlib/fs.mt` declares
    /// `fn move(src: str, dst: str)`, and reserving the word would delete a
    /// shipped function (P104's lesson). It lexes as an ordinary identifier,
    /// so this arm has to tell the annotation from a TYPE that happens to be
    /// called `move` — and it does so by requiring something to follow: in
    /// `x: move` the word is the type, in `x: move str` it is the annotation.
    fn eat_move_annotation(&mut self) -> bool {
        if !matches!(self.peek(), TokenKind::Ident(w) if w == "move") {
            return false;
        }
        // A type must follow for this to be an annotation rather than a type.
        if matches!(self.peek_at(1), TokenKind::Comma | TokenKind::RParen) {
            return false;
        }
        self.advance();
        true
    }

    pub(super) fn parse_fn_def(&mut self, is_pub: bool) -> CompileResult<FnDef> {
        let span = self.span();
        let is_async = self.eat(&TokenKind::Async);
        self.expect(&TokenKind::Fn)?;
        // A keyword is a legal name here — nothing else can follow `fn`.
        let name = self.expect_name("function name")?;

        let (generics, mut bounds) = self.parse_generic_params_bounded();

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
                    // A `move self` is B7's business, not D-2's: consuming the
                    // receiver is a separate decision (see `method_recv` in the
                    // move-site sweep, which counts it and never consumes it).
                    is_move: false,
                });
            } else {
                let (pname, _) = self.expect_ident()?;
                self.expect(&TokenKind::Colon)?;
                let is_move = self.eat_move_annotation();
                let ty = self.parse_type()?;
                params.push(Param { name: pname, ty, span: pspan, is_move });
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

        // B1: a `where` clause sits between the return type and the body, and
        // adds to whatever the angle brackets already said rather than
        // replacing it.
        bounds.extend(self.parse_where_clause());

        // A2: an optional `available(...)` assertion, in the same position and
        // with the same spelling as the one `extern` already takes. Contextual,
        // not a keyword: `available` stays usable as an identifier, which it
        // has to be — stdlib/sync.mt declares a method called exactly that.
        let available = self.parse_available_clause()?;

        let body = if self.peek() == &TokenKind::LBrace {
            Some(self.parse_block()?)
        } else {
            self.eat(&TokenKind::Semi);
            None
        };

        Ok(FnDef { name, generics, bounds, params, ret_ty, body, available, is_pub, is_async, span })
    }

    // --- struct ---

    pub(super) fn parse_struct_def(&mut self, is_pub: bool) -> CompileResult<StructDef> {
        let span = self.span();
        self.expect(&TokenKind::Struct)?;
        let (name, _) = self.expect_ident()?;

        let generics = self.parse_generic_params();

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

    /// Parse optional generic parameters `<T, U, …>`, or nothing.
    ///
    /// One copy, used by `fn`, `struct` and `impl`. It was two verbatim copies
    /// (fn and struct) and `impl` had none, which is exactly how the third site
    /// came to be missing: there was no single place that adding a declaration
    /// form would have made you look at.
    fn parse_generic_params(&mut self) -> Vec<String> {
        self.parse_generic_params_bounded().0
    }

    /// Generic parameters and any bounds written in the angle brackets.
    ///
    /// B1. Before this, `fn max<T: Ord>` was a parse error at the colon — the
    /// bound could not be written at all, which is why A4's soundness hole had
    /// no expressible fix. The two results are returned separately because
    /// every existing caller wants only the names.
    fn parse_generic_params_bounded(&mut self) -> (Vec<String>, Vec<GenericBound>) {
        let mut generics = Vec::new();
        let mut bounds = Vec::new();
        if *self.peek() == TokenKind::Lt {
            self.advance(); // consume <
            loop {
                let bspan = self.span();
                if let TokenKind::Ident(gname) = self.peek().clone() {
                    generics.push(gname.clone());
                    self.advance();
                    if self.eat(&TokenKind::Colon) {
                        let traits = self.parse_bound_list();
                        if !traits.is_empty() {
                            bounds.push(GenericBound { param: gname, traits, span: bspan });
                        }
                    }
                }
                if !self.eat(&TokenKind::Comma) { break; }
            }
            self.eat_gt(); // consume >
        }
        (generics, bounds)
    }

    /// One bound list: `Ord`, or `Ord + Display`.
    fn parse_bound_list(&mut self) -> Vec<String> {
        let mut traits = Vec::new();
        loop {
            match self.peek().clone() {
                TokenKind::Ident(t) => {
                    traits.push(t);
                    self.advance();
                }
                _ => break,
            }
            if !self.eat(&TokenKind::Plus) { break; }
        }
        traits
    }

    /// A `where` clause: `where T: Ord, U: Display`.
    ///
    /// `where` is contextual, like `lint`. It is claimed only in the one
    /// position a clause can appear — between the return type and the body —
    /// so it stays usable as an identifier everywhere else.
    /// A2/A1: parse an optional `available(backend, ...)` clause.
    ///
    /// Shared by `extern` declarations and by ordinary functions so the two
    /// spellings cannot drift. `available` is matched as a CONTEXTUAL word, not
    /// a keyword: making it a keyword would have broken `stdlib/sync.mt`, which
    /// declares a method named `available`, and an identifier that stops being
    /// usable is too high a price for a clause this narrow.
    ///
    /// Returns `None` when no clause is present — unstated, which is not the
    /// same as "available on no backend".
    fn parse_available_clause(&mut self) -> CompileResult<Option<Vec<String>>> {
        match self.peek() {
            TokenKind::Ident(w) if w == "available" => {}
            _ => return Ok(None),
        }
        self.advance();
        self.expect(&TokenKind::LParen)?;
        let mut backends = Vec::new();
        while self.peek() != &TokenKind::RParen {
            let (b, _) = self.expect_ident()?;
            if !backends.contains(&b) {
                backends.push(b);
            }
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        self.expect(&TokenKind::RParen)?;
        Ok(Some(backends))
    }

    fn parse_where_clause(&mut self) -> Vec<GenericBound> {
        let mut bounds = Vec::new();
        let is_where = matches!(self.peek(), TokenKind::Ident(w) if w == "where");
        if !is_where {
            return bounds;
        }
        self.advance(); // eat `where`
        loop {
            let bspan = self.span();
            let TokenKind::Ident(param) = self.peek().clone() else { break };
            self.advance();
            if !self.eat(&TokenKind::Colon) {
                break;
            }
            let traits = self.parse_bound_list();
            if !traits.is_empty() {
                bounds.push(GenericBound { param, traits, span: bspan });
            }
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        bounds
    }

    /// Whether the `lint` identifier at the cursor starts a lint item.
    ///
    /// The shape claimed is exactly `lint <ident> (`. Anything else — a
    /// function called `lint`, a global named `lint` — parses as it always did.
    fn lint_item_ahead(&self) -> bool {
        matches!(self.peek_at(1), TokenKind::Ident(_))
            && matches!(self.peek_at(2), TokenKind::LParen)
    }

    /// A5: `lint deny(unused-variable, shadowing);`
    ///
    /// Lint names contain hyphens, which the lexer sees as `Ident Minus Ident`.
    /// They are re-joined here rather than lexed specially, so the hyphen stays
    /// out of the token grammar.
    fn parse_lint_decl(&mut self) -> CompileResult<LintDecl> {
        let span = self.span();
        self.advance(); // eat `lint`
        let (level, _) = self.expect_ident()?;
        self.expect(&TokenKind::LParen)?;
        let mut lints = Vec::new();
        while self.peek() != &TokenKind::RParen {
            lints.push(self.parse_lint_name()?);
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        self.expect(&TokenKind::RParen)?;
        self.expect_stmt_semi("lint declaration")?;
        Ok(LintDecl { level, lints, span })
    }

    /// A hyphenated lint name, re-joined from `Ident (- Ident)*` — or from a
    /// KEYWORD, which is report.txt P104.
    ///
    /// `lint allow(unknown-type);` was a parse error: `unknown` lexes as the
    /// three-valued literal, and a `Token` carries only a kind and a span, so
    /// there was nothing to rejoin. Three of the 26 lint names were unwritable
    /// this way while their command-line form worked — and **a lint whose
    /// `allow` cannot be spelled is not an exact restoration of anything**,
    /// which is the guarantee `src/lint.rs` makes for every entry.
    fn parse_lint_name(&mut self) -> CompileResult<String> {
        let mut name = self.lint_name_word()?;
        while self.eat(&TokenKind::Minus) {
            let part = self.lint_name_word()?;
            name.push('-');
            name.push_str(&part);
        }
        Ok(name)
    }

    /// One word of a lint name: an identifier, or a keyword that
    /// `lexer::lint_word_lexeme` can spell.
    ///
    /// Claimed only here. A keyword is a keyword everywhere else in the
    /// grammar, so this widens nothing outside `lint <level>( … )`.
    fn lint_name_word(&mut self) -> CompileResult<String> {
        if let TokenKind::Ident(name) = self.peek().clone() {
            self.advance();
            return Ok(name);
        }
        if let Some(w) = crate::lexer::lint_word_lexeme(self.peek()) {
            self.advance();
            return Ok(w.to_string());
        }
        Err(self.err(format!(
            "expected a lint name, found {:?}. A lint name is words joined by \
             `-`; if this is a keyword, `lexer::lint_word_lexeme` has to carry \
             its spelling (report.txt P104)",
            self.peek()
        )))
    }

    /// A1: an explicit native declaration.
    ///
    /// ```text
    /// extern "c" fn io::println(s: str) -> void
    ///     available(llvm, t3) deprecated("use fmt::print");
    /// ```
    ///
    /// The ABI string is mandatory and the signature is mandatory; the two
    /// clauses are optional and may be written in either order. A missing
    /// `available` means UNSTATED, not "available nowhere" — see `ExternDecl`.
    fn parse_extern_decl(&mut self, is_pub: bool) -> CompileResult<ExternDecl> {
        let span = self.span();
        self.expect(&TokenKind::Extern)?;

        // The ABI. Required, and required to be a string: `extern fn` with no
        // ABI is the implicit registration A1 exists to replace, so accepting
        // it here would leave the old hole open under the new syntax.
        let abi = match self.peek().clone() {
            TokenKind::Str(a) => {
                self.advance();
                a
            }
            other => {
                return Err(self.err(format!(
                    "expected an ABI string after `extern` (for example \
                     `extern \"c\"`), found {:?}",
                    other
                )));
            }
        };

        self.expect(&TokenKind::Fn)?;

        // The name, qualified as it is called: `io::println`.
        let mut name = self.expect_name("extern function name")?;
        while self.eat(&TokenKind::ColonColon) {
            let part = self.expect_name("extern function name")?;
            name.push_str("::");
            name.push_str(&part);
        }

        self.expect(&TokenKind::LParen)?;
        let mut params = Vec::new();
        while self.peek() != &TokenKind::RParen {
            let pspan = self.span();
            let (pname, _) = self.expect_ident()?;
            self.expect(&TokenKind::Colon)?;
            let is_move = self.eat_move_annotation();
            let ty = self.parse_type()?;
            params.push(Param { name: pname, ty, span: pspan, is_move });
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

        let mut available: Option<Vec<String>> = self.parse_available_clause()?;
        let mut deprecated: Option<String> = None;
        loop {
            let clause = match self.peek().clone() {
                TokenKind::Ident(w) if w == "available" || w == "deprecated" => w,
                _ => break,
            };
            self.advance();
            self.expect(&TokenKind::LParen)?;
            if clause == "available" {
                let mut backends = Vec::new();
                while self.peek() != &TokenKind::RParen {
                    let (b, _) = self.expect_ident()?;
                    backends.push(b);
                    if !self.eat(&TokenKind::Comma) {
                        break;
                    }
                }
                self.expect(&TokenKind::RParen)?;
                // Two `available` clauses union rather than replace: writing
                // the same backend twice is harmless, and silently dropping
                // the first list would be the kind of quiet loss A1 is about.
                let entry = available.get_or_insert_with(Vec::new);
                for b in backends {
                    if !entry.contains(&b) {
                        entry.push(b);
                    }
                }
            } else {
                let msg = match self.peek().clone() {
                    TokenKind::Str(m) => {
                        self.advance();
                        m
                    }
                    other => {
                        return Err(self.err(format!(
                            "`deprecated` takes a message string, found {:?}",
                            other
                        )));
                    }
                };
                self.expect(&TokenKind::RParen)?;
                deprecated = Some(msg);
            }
        }

        self.expect_stmt_semi("extern declaration")?;

        Ok(ExternDecl {
            abi,
            name,
            params,
            ret_ty,
            available,
            deprecated,
            is_pub,
            span,
        })
    }

    /// The base name of a type, for the places that key on it rather than on
    /// the full type: `Vec<T>` → `Vec`, `collections::Vec` → `Vec`.
    fn type_base_name(t: &Type) -> String {
        match t {
            Type::Named(n, _) => n.clone(),
            Type::Generic(n, _, _) => n.clone(),
            Type::Path(segs, _) => segs.last().cloned().unwrap_or_default(),
            other => other.display(),
        }
    }

    fn parse_impl_block(&mut self) -> CompileResult<ImplBlock> {
        let span = self.span();
        self.expect(&TokenKind::Impl)?;

        // `impl<T> …` — the impl's OWN generic parameters. Without this the
        // parser stopped at the `<` with "expected identifier, found Lt", which
        // is why stdlib/collections.mt, stdlib/async.mt and stdlib/sync.mt --
        // the language's own Vec, Future and Mutex -- did not parse. They are
        // the three modules STDLIB_SOURCES describes as having "known parse
        // gaps"; this was the gap.
        let generics = self.parse_generic_params();

        // The name that follows may itself carry generic ARGUMENTS
        // (`impl<T> Vec<T>`), so parse a full type rather than a bare
        // identifier. That also admits `impl<T> Foo<Vec<T>>` for free, because
        // parse_type is already recursive.
        let first = self.parse_type()?;

        // impl Trait for Type  OR  impl Type
        // `for` is a keyword so we check TokenKind::For directly
        let (ty, trait_) = if self.peek() == &TokenKind::For {
            self.advance(); // eat `for`
            let target = self.parse_type()?;
            (Self::type_base_name(&target), Some(Self::type_base_name(&first)))
        } else {
            (Self::type_base_name(&first), None)
        };

        self.expect(&TokenKind::LBrace)?;
        let mut methods = Vec::new();
        while self.peek() != &TokenKind::RBrace {
            let is_pub = self.eat(&TokenKind::Pub);
            let m = self.parse_fn_def(is_pub)?;
            methods.push(m);
        }
        self.expect(&TokenKind::RBrace)?;
        Ok(ImplBlock { ty, generics, trait_, methods, span })
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

    /// The source spelling of a keyword that may legitimately also be a NAME.
    ///
    /// maniT already decided this question for module paths: `use async::spawn`
    /// has to work, so `expect_path_segment` has always mapped keyword tokens
    /// back to their spelling. The same reasoning applies anywhere the position
    /// admits only a name — after `fn`, and after `.` — because no keyword's
    /// statement form can appear there. `stdlib/async.mt` declares
    /// `fn spawn<T>(self, fut: Future<T>) -> Task<T>` and `runtime.spawn(fut)`
    /// is the natural way to call it; without this the method could not be
    /// declared or invoked, and the module did not parse.
    ///
    /// This is one table used by three positions. It was one table used by one
    /// position, inline, which is why the other two never got it.
    pub(super) fn keyword_spelling(tok: &TokenKind) -> Option<&'static str> {
        Some(match tok {
            TokenKind::Async => "async",
            TokenKind::Spawn => "spawn",
            TokenKind::Await => "await",
            TokenKind::Channel => "channel",
            TokenKind::IntKw => "int",
            TokenKind::FloatKw => "float",
            TokenKind::CharKw => "char",
            TokenKind::StrKw => "str",
            TokenKind::VoidKw => "void",
            TokenKind::Bool3Kw => "bool3",
            TokenKind::TritKw => "trit",
            TokenKind::TryteKw => "tryte",
            TokenKind::T9Kw => "t9",
            TokenKind::T27Kw => "t27",
            TokenKind::T54Kw => "t54",
            TokenKind::TrintKw => "trint",
            TokenKind::TfloatKw => "tfloat",
            _ => return None,
        })
    }

    /// Accept an identifier, or a keyword in a position where only a name can
    /// appear (a function/method declaration name, or a field/method after
    /// `.`). See `keyword_spelling`.
    pub(super) fn expect_name(&mut self, what: &str) -> CompileResult<String> {
        if let TokenKind::Ident(name) = self.peek().clone() {
            self.advance();
            return Ok(name);
        }
        match Self::keyword_spelling(self.peek()) {
            Some(s) => { self.advance(); Ok(s.to_string()) }
            None => Err(self.err(format!(
                "expected {}, found {:?}", what, self.peek()))),
        }
    }

    /// Accept both identifiers AND keywords as module path segments.
    /// This allows `use async::spawn`, `use io::println`, etc.
    pub(super) fn expect_path_segment(&mut self) -> CompileResult<String> {
        let _sp = self.span();
        let seg = match self.peek().clone() {
            TokenKind::Ident(name) => name,
            // literal keyword names used as module identifiers
            tok => {
                // Check if this keyword could be a module name
                let s = match Self::keyword_spelling(&tok) {
                    Some(s) => s.to_string(),
                    None => {
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
        self.expect_stmt_semi("use declaration")?;
        Ok(UseDecl { path, span })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_token_vector_does_not_panic() {
        // F15 regression: Parser::new used to panic (index underflow) when
        // handed a token vector without a trailing Eof.
        let mut p = Parser::new(Vec::new());
        let program = p.parse().expect("empty input parses to an empty program");
        assert!(program.items.is_empty());
    }
}
