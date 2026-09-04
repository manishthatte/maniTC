use super::*;
use crate::error::CompileResult;
use crate::lexer::TokenKind;

impl Parser {
    // ---------------------------------------------------------------------------
    // Types
    // ---------------------------------------------------------------------------

    pub(super) fn parse_type(&mut self) -> CompileResult<Type> {
        // A3: nested generics (Vec<Vec<Vec<…>>>) recurse here.
        self.enter("type")?;
        let result = self.parse_type_inner();
        self.leave();
        result
    }

    fn parse_type_inner(&mut self) -> CompileResult<Type> {
        let span = self.span();
        match self.peek().clone() {
            // Infer: `_`
            TokenKind::Ident(ref s) if s == "_" => {
                self.advance();
                Ok(Type::Infer(span))
            }

            // Reference: &T or &mut T
            TokenKind::Ampersand => {
                self.advance();
                let mutable = self.eat(&TokenKind::Mut);
                let inner = self.parse_type()?;
                Ok(Type::Ref(Box::new(inner), mutable, span))
            }

            // Pointer: *T or *mut T
            TokenKind::Star => {
                self.advance();
                let mutable = self.eat(&TokenKind::Mut);
                let inner = self.parse_type()?;
                Ok(Type::Ptr(Box::new(inner), mutable, span))
            }

            // Array: [T] or [T; N]
            TokenKind::LBracket => {
                self.advance();
                let inner = self.parse_type()?;
                let size = if self.eat(&TokenKind::Semi) {
                    match self.peek().clone() {
                        // A bare literal length keeps its own variant: it is
                        // the overwhelming majority, it needs no evaluator, and
                        // `[int; 3]` must not depend on constant evaluation
                        // working. Anything else — including `2 + 1` — is an
                        // expression, so the `]` lookahead is what separates
                        // them.
                        TokenKind::Int(n) if self.peek2() == &TokenKind::RBracket => {
                            self.advance();
                            ArrayLen::Fixed(n as usize)
                        }
                        // B3/B4: `[trit; N]` and `[trit; N * 2]` — a constant
                        // EXPRESSION as the length. Accepted for any shape here
                        // and evaluated in the resolver, because the parser
                        // does not know what is in scope: a name that turns out
                        // not to be a constant gets a diagnostic that can say
                        // so, which "expected integer size" could not.
                        //
                        // `parse_add_expr` and not `parse_expr`: a length is a
                        // NUMBER, so the fragment stops below comparison. That
                        // is also what lets `t<A + 1>` close on its own `>`
                        // without braces — see `parse_ternary_width`.
                        _ => ArrayLen::Expr(Box::new(self.parse_add_expr()?)),
                    }
                } else {
                    ArrayLen::Unsized
                };
                self.expect(&TokenKind::RBracket)?;
                Ok(Type::Array(Box::new(inner), size, span))
            }

            // Tuple: (T, T, ...)  or unit ()
            TokenKind::LParen => {
                self.advance();
                if self.eat(&TokenKind::RParen) {
                    return Ok(Type::Tuple(vec![], span));
                }
                let mut types = vec![self.parse_type()?];
                while self.eat(&TokenKind::Comma) {
                    if self.peek() == &TokenKind::RParen {
                        break;
                    }
                    types.push(self.parse_type()?);
                }
                self.expect(&TokenKind::RParen)?;
                if types.len() == 1 {
                    Ok(types.remove(0))
                } else {
                    Ok(Type::Tuple(types, span))
                }
            }

            // Fn type: fn(T, T) -> T
            TokenKind::Fn => {
                self.advance();
                self.expect(&TokenKind::LParen)?;
                let mut params = Vec::new();
                while self.peek() != &TokenKind::RParen {
                    params.push(self.parse_type()?);
                    if !self.eat(&TokenKind::Comma) {
                        break;
                    }
                }
                self.expect(&TokenKind::RParen)?;
                let ret = if self.eat(&TokenKind::Arrow) {
                    self.parse_type()?
                } else {
                    Type::Named("void".to_string(), span)
                };
                Ok(Type::Fn(params, Box::new(ret), span))
            }

            // Named type or type keyword
            tok => {
                let name = self.type_keyword_to_name(&tok);
                match name {
                    Some(n) => {
                        self.advance();
                        // C3: `t<N>` is a WIDTH, not a generic argument list.
                        //
                        // It is unambiguous, and structurally rather than by
                        // preference: `parse_generic_args` has exactly one
                        // caller — this function — so `<` after a name means
                        // "generic arguments" only in TYPE position, and an
                        // expression never reaches type position. There is no
                        // turbofish in this language. Measured on the compiler
                        // before the change: `let x: t<18> = 5;` is a parse
                        // error ("expected type, found Int(18)"), so the
                        // syntax was free.
                        //
                        // `t` is NOT a keyword and must not become one — it is
                        // declared as a variable 855 times across both
                        // repositories and the corpus (156 + 699), with 11 of
                        // the form `t < 0`. Reserving it would delete every
                        // one, which is P104's lesson met before it bit rather
                        // than after (`stdlib/fs.mt` declares `fn move`).
                        // Contextual here, exactly as `move` is in D-2.
                        if n == "t" && self.peek() == &TokenKind::Lt {
                            return self.parse_ternary_width(span);
                        }
                        // check for generic params: Name<T, U>
                        if self.peek() == &TokenKind::Lt {
                            let args = self.parse_generic_args()?;
                            Ok(Type::Generic(n, args, span))
                        } else if self.peek() == &TokenKind::ColonColon
                            && matches!(self.peek2(), TokenKind::Ident(_))
                        {
                            // Path type
                            let mut path = vec![n];
                            while self.eat(&TokenKind::ColonColon) {
                                if let TokenKind::Ident(seg) = self.peek().clone() {
                                    self.advance();
                                    path.push(seg);
                                } else {
                                    break;
                                }
                            }
                            Ok(Type::Path(path, span))
                        } else {
                            Ok(Type::Named(n, span))
                        }
                    }
                    None => Err(self.err(format!("expected type, found {:?}", self.peek()))),
                }
            }
        }
    }

    pub(super) fn type_keyword_to_name(&self, tok: &TokenKind) -> Option<String> {
        match tok {
            TokenKind::Ident(s) => Some(s.clone()),
            TokenKind::TritKw => Some("trit".to_string()),
            TokenKind::TryteKw => Some("tryte".to_string()),
            TokenKind::T9Kw => Some("t9".to_string()),
            TokenKind::T27Kw => Some("t27".to_string()),
            TokenKind::T54Kw => Some("t54".to_string()),
            TokenKind::TrintKw => Some("trint".to_string()),
            TokenKind::TfloatKw => Some("tfloat".to_string()),
            TokenKind::IntKw => Some("int".to_string()),
            TokenKind::FloatKw => Some("float".to_string()),
            TokenKind::Bool3Kw => Some("bool3".to_string()),
            TokenKind::CharKw => Some("char".to_string()),
            TokenKind::StrKw => Some("str".to_string()),
            TokenKind::VoidKw => Some("void".to_string()),
            TokenKind::SelfKw => Some("Self".to_string()),
            _ => None,
        }
    }

    /// C3: parse the `<N>` of a `t<N>`, having just consumed the `t`.
    ///
    /// The width is a literal, and that is what makes C3's core independent of
    /// B3 and B4 — the item lists both as prerequisites, and a LITERAL width is
    /// known at parse time, so no constant evaluator is involved. C6 reported
    /// the same thing about its own rationale ("it did not turn out to need
    /// B4"), for the same reason and one item earlier. What genuinely waits on
    /// B3 is width POLYMORPHISM — `fn widen<const A: int>(x: t<A>)` — which is
    /// not built and is not claimed.
    fn parse_ternary_width(&mut self, span: Span) -> CompileResult<Type> {
        self.expect(&TokenKind::Lt)?;
        let w = match self.peek().clone() {
            // A BARE literal width is resolved here, as C3 always did. The `>`
            // lookahead is what separates it from `t<0 + 99>`, which leads with
            // an `Int` too and is an expression.
            TokenKind::Int(v)
                if matches!(self.peek2(), TokenKind::Gt | TokenKind::RShift) =>
            {
                self.advance();
                v
            }
            // B3/B4: `t<A>` and `t<A + 1>` — a width that is not a literal.
            //
            // It survives parsing as an EXPRESSION rather than being desugared,
            // which is the one asymmetry with the literal case below: a width
            // that can be computed here is computed here, and one that cannot
            // is left for the instantiation that binds it.
            //
            // **`parse_add_expr`, not `parse_expr`, and that is what removes
            // the ambiguity.** `t<A + 1 > b>` would be a disaster if `>` could
            // be a comparison here; the fragment stops below comparison, so the
            // first `>` can only be the closing bracket. Rust reaches for
            // braces (`t<{A + 1}>`) to solve the same problem; a width is a
            // NUMBER and never a bool, so the precedence floor solves it
            // without them.
            TokenKind::Ident(_) | TokenKind::Minus | TokenKind::LParen | TokenKind::Int(_) => {
                let e = self.parse_add_expr()?;
                if !self.eat_gt() {
                    return Err(self.err(format!(
                        "expected `>` to close `t<{}>`. A width is a number, so the \
                         expression fragment here stops below comparison — `<` and \
                         `>` inside a width are the brackets, never operators.",
                        crate::ast::expr_sketch(&e)
                    )));
                }
                return Ok(Type::TernaryWidth(Box::new(e), span));
            }
            other => {
                return Err(self.err(format!(
                    "`t<N>` needs a trit width, found {:?}. A width is a plain \
                     integer literal between 1 and {} — `t<18>` is an 18-trit \
                     balanced ternary integer — or the name of a `const` \
                     generic parameter in scope, as in \
                     `fn widen<const A: int>(x: t<A>)`.",
                    other,
                    crate::semantic::types::MAX_TERNARY_WIDTH
                )))
            }
        };
        if !self.eat_gt() {
            return Err(self.err("expected `>` to close a `t<N>` width".to_string()));
        }
        // The bound is checked HERE, in the parser, and not left to the type
        // resolver, because the resolver's remedy for an unknown name is "did
        // you mean" over declared types (P95) and `t<0>` is not a misspelling
        // of anything. A width names its own problem.
        if w < 1 || w > crate::semantic::types::MAX_TERNARY_WIDTH as i64 {
            return Err(self.err(format!(
                "`t<{}>` is not a valid trit width: a width runs from 1 to {}. \
                 {} is the last width the machine word can hold — a `t<{}>` is \
                 already bounded by i64 rather than by 3^{}, and past it a value \
                 could not be written or printed at all.",
                w,
                crate::semantic::types::MAX_TERNARY_WIDTH,
                crate::semantic::types::MAX_TERNARY_WIDTH,
                crate::semantic::types::MAX_TERNARY_WIDTH,
                crate::semantic::types::MAX_TERNARY_WIDTH
            )));
        }
        // Desugared to the canonical SPELLING rather than to a new `Type`
        // variant, so every consumer of `Type` downstream is unchanged and the
        // one place that turns a name into a `ManiType` stays the one place.
        Ok(Type::Named(
            if w == 1 {
                // `t<1>` IS `trit`, not a plain 1-trit integer beside it: the
                // three values of a width-1 balanced ternary number ARE the
                // three values of the logic. Measured, not assumed — `tand`
                // accepts `trit` and refuses the other four.
                "trit".to_string()
            } else {
                crate::semantic::types::ternary_width_name(w as u32)
            },
            span,
        ))
    }

    pub(super) fn parse_generic_args(&mut self) -> CompileResult<Vec<Type>> {
        self.expect(&TokenKind::Lt)?;
        let mut args = Vec::new();
        while self.peek() != &TokenKind::Gt && self.peek() != &TokenKind::RShift {
            // B3: a const generic ARGUMENT — `TVec<27>`. The parameter form
            // `<const N: int>` is implemented and the argument form is not, so
            // the limit is named here rather than reported as "expected type,
            // found Int(27)", which describes the token and not the feature.
            if let TokenKind::Int(v) = self.peek().clone() {
                return Err(self.err(format!(
                    "`{v}` is a const generic ARGUMENT, and supplying one explicitly \
                     is not implemented. A `const` parameter is INFERRED from a \
                     call's arguments today — `fn f<const N: int>(x: t<N>)` binds `N` \
                     from the width of `x` — so a struct cannot yet be written at a \
                     chosen width. (A trit width is spelled `t<{v}>`, which does \
                     work.)"
                )));
            }
            args.push(self.parse_type()?);
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        if !self.eat_gt() {
            return Err(self.err("expected `>` to close generic args".to_string()));
        }
        Ok(args)
    }

    // ---------------------------------------------------------------------------
    // Pattern parsing
    // ---------------------------------------------------------------------------

    pub(super) fn parse_pattern(&mut self) -> CompileResult<Pattern> {
        let span = self.span();
        let first = self.parse_single_pattern()?;
        // Or pattern: pat1 | pat2 | ...
        if self.peek() == &TokenKind::Pipe {
            let mut alts = vec![first];
            while self.eat(&TokenKind::Pipe) {
                alts.push(self.parse_single_pattern()?);
            }
            return Ok(Pattern::Or(alts, span));
        }
        Ok(first)
    }

    /// C6: turn the text a `TritPat` token carries into an `ast::TritPat`.
    ///
    /// The grammar lives here and not in the lexer, which absorbs the run
    /// without interpreting it. Elements are read left to right, HIGH trit
    /// first, exactly as the `0t` literal reads.
    fn build_trit_pattern(&self, text: &str, span: Span) -> CompileResult<TritPat> {
        let mut elems: Vec<TritElem> = Vec::new();
        let mut open = false;
        let mut open_capture: Option<String> = None;
        // `(name, elem_index, len)` while scanning; converted to trit
        // positions once the width is known.
        let mut caps: Vec<(String, usize, usize)> = Vec::new();
        // The wildcard run a following `@` would name: `None` if the last
        // element was not a wildcard, `Some(None)` for the leading `*`,
        // `Some(Some((start, len)))` for a run of `?`.
        let mut run: Option<Option<(usize, usize)>> = None;

        let chars: Vec<char> = text.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            match chars[i] {
                '*' => {
                    if i != 0 {
                        return Err(self.err_at(span, format!(
                            "`*` may appear only as the first element of a trit pattern \
                             (`0t{}`) — a `*` anywhere else would have to be placed by \
                             knowing the word's trit width, which differs between the \
                             backends", text)));
                    }
                    open = true;
                    run = Some(None);
                    i += 1;
                }
                '?' => {
                    let start = elems.len();
                    elems.push(TritElem::Any);
                    run = match run {
                        Some(Some((s, l))) => Some(Some((s, l + 1))),
                        _ => Some(Some((start, 1))),
                    };
                    i += 1;
                }
                '+' | '0' | '-' => {
                    let v: i8 = match chars[i] { '+' => 1, '-' => -1, _ => 0 };
                    elems.push(TritElem::Fixed(v));
                    run = None;
                    i += 1;
                }
                '@' => {
                    i += 1;
                    let start = i;
                    while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                        i += 1;
                    }
                    let name: String = chars[start..i].iter().collect();
                    match run.take() {
                        Some(None) => {
                            if open_capture.is_some() {
                                return Err(self.err_at(span,
                                    "the `*` of a trit pattern is already named"));
                            }
                            open_capture = Some(name);
                        }
                        Some(Some((s, l))) => caps.push((name, s, l)),
                        None => return Err(self.err_at(span,
                            "`@` in a trit pattern must follow the wildcard run it names")),
                    }
                }
                c => return Err(self.err_at(span, format!(
                    "`{}` is not a trit pattern element — expected `+`, `0`, `-`, `?`, \
                     a leading `*`, or `@name`", c))),
            }
        }

        // The cap is arithmetic, not arbitrary: the lowering needs 3^width as
        // a machine word and 3^40 does not fit one. `t54` is therefore wider
        // than a trit pattern can span; `int`, `t27` and below are not.
        const MAX_TRITS: usize = 39;
        if elems.len() > MAX_TRITS {
            return Err(self.err_at(span, format!(
                "a trit pattern may name at most {} trits, this one names {} — beyond \
                 that 3^width does not fit a 64-bit word", MAX_TRITS, elems.len())));
        }

        // elems are high-to-low, so element `idx` sits at position
        // `width - 1 - idx` counting the LOW trit as 0.
        let w = elems.len();
        let mut captures: Vec<(String, usize, usize)> = Vec::new();
        for (name, s, l) in caps {
            captures.push((name, w - s - l, l));
        }
        // Low-to-high, so `fixed_runs` and the binder walk in one direction.
        captures.sort_by_key(|(_, lo, _)| *lo);

        let mut seen: Vec<&str> = Vec::new();
        if let Some(n) = &open_capture {
            seen.push(n.as_str());
        }
        for (n, _, _) in &captures {
            if seen.contains(&n.as_str()) {
                return Err(self.err_at(span, format!(
                    "trit pattern binds `{}` twice", n)));
            }
            seen.push(n.as_str());
        }

        Ok(TritPat { elems, open, open_capture, captures, text: text.to_string() })
    }

    pub(super) fn parse_single_pattern(&mut self) -> CompileResult<Pattern> {
        let span = self.span();
        match self.peek().clone() {
            // C6: a trit pattern.
            TokenKind::TritPat(text) => {
                self.advance();
                let tp = self.build_trit_pattern(&text, span)?;
                Ok(Pattern::Trit(tp, span))
            }
            // Wildcard _
            TokenKind::Ident(ref s) if s == "_" => {
                self.advance();
                Ok(Pattern::Wildcard(span))
            }
            // Literal patterns
            TokenKind::Int(n) => {
                self.advance();
                Ok(Pattern::Lit(Lit::Int(n), span))
            }
            TokenKind::Float(f) => {
                self.advance();
                Ok(Pattern::Lit(Lit::Float(f), span))
            }
            TokenKind::Str(s) => {
                self.advance();
                Ok(Pattern::Lit(Lit::Str(s), span))
            }
            TokenKind::Char(c) => {
                self.advance();
                Ok(Pattern::Lit(Lit::Char(c), span))
            }
            TokenKind::TernaryInt(n) => {
                self.advance();
                Ok(Pattern::Lit(Lit::TernaryInt(n), span))
            }
            TokenKind::True => {
                self.advance();
                Ok(Pattern::Lit(Lit::Bool(true), span))
            }
            TokenKind::False => {
                self.advance();
                Ok(Pattern::Lit(Lit::Bool(false), span))
            }
            TokenKind::Unknown => {
                self.advance();
                // If followed by '(', treat as enum variant pattern Unknown(...)
                if self.peek() == &TokenKind::LParen {
                    self.advance();
                    let mut fields = Vec::new();
                    while self.peek() != &TokenKind::RParen && !self.is_at_end() {
                        fields.push(self.parse_pattern()?);
                        if !self.eat(&TokenKind::Comma) { break; }
                    }
                    self.expect(&TokenKind::RParen)?;
                    return Ok(Pattern::Enum(
                        "Result".to_string(), Some("Unknown".to_string()), fields, span));
                }
                Ok(Pattern::Lit(Lit::Bool3(0), span))
            }
            TokenKind::Minus => {
                self.advance();
                match self.peek().clone() {
                    TokenKind::Int(n) => {
                        self.advance();
                        Ok(Pattern::Lit(Lit::Int(-n), span))
                    }
                    TokenKind::Float(f) => {
                        self.advance();
                        Ok(Pattern::Lit(Lit::Float(-f), span))
                    }
                    TokenKind::TernaryInt(n) => {
                        self.advance();
                        Ok(Pattern::Lit(Lit::TernaryInt(-n), span))
                    }
                    // Bare `-` is the trit literal -1
                    _ => Ok(Pattern::Lit(Lit::Trit(-1), span)),
                }
            }
            TokenKind::Plus => {
                self.advance();
                match self.peek().clone() {
                    TokenKind::Int(n) => {
                        self.advance();
                        Ok(Pattern::Lit(Lit::Int(n), span))
                    }
                    TokenKind::Float(f) => {
                        self.advance();
                        Ok(Pattern::Lit(Lit::Float(f), span))
                    }
                    TokenKind::TernaryInt(n) => {
                        self.advance();
                        Ok(Pattern::Lit(Lit::TernaryInt(n), span))
                    }
                    // Bare `+` is the trit literal +1
                    _ => Ok(Pattern::Lit(Lit::Trit(1), span)),
                }
            }
            // Tuple pattern
            TokenKind::LParen => {
                self.advance();
                if self.eat(&TokenKind::RParen) {
                    return Ok(Pattern::Tuple(vec![], span));
                }
                let mut pats = vec![self.parse_pattern()?];
                while self.eat(&TokenKind::Comma) {
                    if self.peek() == &TokenKind::RParen {
                        break;
                    }
                    pats.push(self.parse_pattern()?);
                }
                self.expect(&TokenKind::RParen)?;
                Ok(Pattern::Tuple(pats, span))
            }
            // Named pattern (identifier, enum variant, or struct)
            TokenKind::Ident(name) => {
                self.advance();
                if self.peek() == &TokenKind::ColonColon {
                    // Enum variant with path: Name::Variant or Name::Variant(pats)
                    let mut path = vec![name];
                    while self.eat(&TokenKind::ColonColon) {
                        if let TokenKind::Ident(seg) = self.peek().clone() {
                            self.advance();
                            path.push(seg);
                        } else {
                            break;
                        }
                    }
                    let variant = path.pop().unwrap();
                    let enum_name = if path.is_empty() {
                        None
                    } else {
                        Some(path.join("::"))
                    };
                    if self.peek() == &TokenKind::LParen {
                        self.advance();
                        let mut fields = Vec::new();
                        while self.peek() != &TokenKind::RParen {
                            fields.push(self.parse_pattern()?);
                            if !self.eat(&TokenKind::Comma) {
                                break;
                            }
                        }
                        self.expect(&TokenKind::RParen)?;
                        Ok(Pattern::Enum(variant, enum_name, fields, span))
                    } else {
                        Ok(Pattern::Enum(variant, enum_name, vec![], span))
                    }
                } else if self.peek() == &TokenKind::LParen {
                    // Enum variant(pats)
                    self.advance();
                    let mut fields = Vec::new();
                    while self.peek() != &TokenKind::RParen {
                        fields.push(self.parse_pattern()?);
                        if !self.eat(&TokenKind::Comma) {
                            break;
                        }
                    }
                    self.expect(&TokenKind::RParen)?;
                    Ok(Pattern::Enum(name, None, fields, span))
                } else if self.peek() == &TokenKind::LBrace {
                    // Struct pattern
                    self.advance();
                    let mut field_pats = Vec::new();
                    while self.peek() != &TokenKind::RBrace {
                        let (fname, _) = self.expect_ident()?;
                        self.expect(&TokenKind::Colon)?;
                        let pat = self.parse_pattern()?;
                        field_pats.push((fname, pat));
                        if !self.eat(&TokenKind::Comma) {
                            break;
                        }
                    }
                    self.expect(&TokenKind::RBrace)?;
                    Ok(Pattern::Struct(name, field_pats, span))
                } else {
                    Ok(Pattern::Ident(name, span))
                }
            }
            tok => Err(self.err(format!("expected pattern, found {:?}", tok))),
        }
    }
}
