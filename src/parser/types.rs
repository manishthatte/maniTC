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
                    if let TokenKind::Int(n) = self.peek().clone() {
                        self.advance();
                        Some(n as usize)
                    } else {
                        return Err(self.err("expected integer size in array type"));
                    }
                } else {
                    None
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

    pub(super) fn parse_generic_args(&mut self) -> CompileResult<Vec<Type>> {
        self.expect(&TokenKind::Lt)?;
        let mut args = Vec::new();
        while self.peek() != &TokenKind::Gt && self.peek() != &TokenKind::RShift {
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

    pub(super) fn parse_single_pattern(&mut self) -> CompileResult<Pattern> {
        let span = self.span();
        match self.peek().clone() {
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
