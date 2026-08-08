use crate::ast::Span;
use crate::error::{CompileError, CompileResult};

// ---------------------------------------------------------------------------
// Token kinds
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // --- Literals ---
    Int(i64),
    Float(f64),
    Str(String),
    Char(char),
    Bool(bool),
    TernaryInt(i64), // 0t+0-+1 balanced ternary literal

    // --- Identifiers / keywords ---
    Ident(String),

    // Keywords
    Let,
    Mut,
    Fn,
    Struct,
    Enum,
    Impl,
    Trait,
    If,
    Elif,
    Else,
    Tif,
    Tresult,
    For,
    While,
    Loop,
    Match,
    In,
    Return,
    Break,
    Continue,
    Use,
    Pub,
    Mod,
    Async,
    Await,
    Spawn,
    Channel,
    As,
    SelfKw,

    // Type keywords
    TritKw,
    TryteKw,
    T9Kw,
    T27Kw,
    T54Kw,
    TrintKw,
    TfloatKw,
    IntKw,
    FloatKw,
    Bool3Kw,   // also accepted as "tribool"
    CharKw,
    StrKw,
    VoidKw,

    // Bool3 literals
    True,
    False,
    Unknown,

    // Ternary logic keyword-operators
    Tand,
    Tor,
    Tnot,
    Txor,
    Tcon,
    Tany,

    // --- Operators ---
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Eq,
    EqEq,
    BangEq,
    Lt,
    Gt,
    LtEq,
    GtEq,
    AndAnd,
    OrOr,
    Bang,
    Tilde,
    Arrow,    // ->
    FatArrow, // =>
    DotDot,
    DotDotEq,
    Question,
    PlusEq,
    MinusEq,
    StarEq,
    SlashEq,
    PercentEq,
    AmpersandEq,
    PipeEq,
    CaretEq,
    LShiftEq,
    RShiftEq,

    // Bitwise
    Ampersand,
    Pipe,
    Caret,
    LShift,
    RShift,

    // --- Delimiters ---
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Comma,
    Semi,
    Colon,
    ColonColon,
    Dot,

    Eof,
}

// ---------------------------------------------------------------------------
// Token
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    pub fn new(kind: TokenKind, span: Span) -> Self {
        Token { kind, span }
    }
}

// ---------------------------------------------------------------------------
// Keyword map helper
// ---------------------------------------------------------------------------

fn keyword_or_ident(s: &str) -> TokenKind {
    match s {
        "let" => TokenKind::Let,
        "mut" => TokenKind::Mut,
        "fn" => TokenKind::Fn,
        "struct" => TokenKind::Struct,
        "enum" => TokenKind::Enum,
        "impl" => TokenKind::Impl,
        "trait" => TokenKind::Trait,
        "if" => TokenKind::If,
        "elif" => TokenKind::Elif,
        "else" => TokenKind::Else,
        "tif" => TokenKind::Tif,
        "tresult" => TokenKind::Tresult,
        "for" => TokenKind::For,
        "while" => TokenKind::While,
        "loop" => TokenKind::Loop,
        "match" => TokenKind::Match,
        "in" => TokenKind::In,
        "return" => TokenKind::Return,
        "break" => TokenKind::Break,
        "continue" => TokenKind::Continue,
        "use" => TokenKind::Use,
        "pub" => TokenKind::Pub,
        "mod" => TokenKind::Mod,
        "async" => TokenKind::Async,
        "await" => TokenKind::Await,
        "spawn" => TokenKind::Spawn,
        "channel" => TokenKind::Channel,
        "as" => TokenKind::As,
        "self" => TokenKind::SelfKw,
        // Type keywords
        "trit" => TokenKind::TritKw,
        "tryte" => TokenKind::TryteKw,
        "t9" => TokenKind::T9Kw,
        "t27" => TokenKind::T27Kw,
        "t54" => TokenKind::T54Kw,
        "word" => TokenKind::T27Kw,       // patent alias for t27
        "trint" => TokenKind::TrintKw,
        "tfloat" => TokenKind::TfloatKw,
        "int" => TokenKind::IntKw,
        "float" => TokenKind::FloatKw,
        "bool3" => TokenKind::Bool3Kw,
        "tribool" => TokenKind::Bool3Kw,  // legacy alias
        "T3Bool"  => TokenKind::Bool3Kw,  // patent name alias
        "char" => TokenKind::CharKw,
        "str" => TokenKind::StrKw,
        "void" => TokenKind::VoidKw,
        // Bool literals (both lowercase and capitalized variants)
        "true" | "True" => TokenKind::True,
        "false" | "False" => TokenKind::False,
        "unknown" | "Unknown" => TokenKind::Unknown,
        // Ternary logic operators
        "tand" => TokenKind::Tand,
        "tor" => TokenKind::Tor,
        "tnot" => TokenKind::Tnot,
        "txor" => TokenKind::Txor,
        "tcon" => TokenKind::Tcon,
        "tany" => TokenKind::Tany,
        _ => TokenKind::Ident(s.to_string()),
    }
}

// ---------------------------------------------------------------------------
// Lexer
// ---------------------------------------------------------------------------

pub struct Lexer {
    source: Vec<char>,
    pos: usize,
    line: usize,
    col: usize,
    file: String,
}

impl Lexer {
    pub fn new(source: &str) -> Self {
        Lexer {
            source: source.chars().collect(),
            pos: 0,
            line: 1,
            col: 1,
            file: String::from("<input>"),
        }
    }

    pub fn with_file(source: &str, file: impl Into<String>) -> Self {
        let mut l = Lexer::new(source);
        l.file = file.into();
        l
    }

    // --- position helpers ---

    fn current_span(&self) -> Span {
        Span { line: self.line, col: self.col }
    }

    fn peek(&self) -> Option<char> {
        self.source.get(self.pos).copied()
    }

    fn peek2(&self) -> Option<char> {
        self.source.get(self.pos + 1).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.source.get(self.pos).copied();
        if let Some(c) = ch {
            self.pos += 1;
            if c == '\n' {
                self.line += 1;
                self.col = 1;
            } else {
                self.col += 1;
            }
        }
        ch
    }

    #[allow(dead_code)]
    fn skip_while(&mut self, pred: impl Fn(char) -> bool) {
        while self.peek().map_or(false, |c| pred(c)) {
            self.advance();
        }
    }

    fn err(&self, msg: impl Into<String>) -> CompileError {
        CompileError::lex(&self.file, self.line, self.col, msg)
    }

    // --- skip whitespace & comments ---

    fn skip_whitespace_and_comments(&mut self) {
        loop {
            // whitespace
            while self.peek().map_or(false, |c| c.is_ascii_whitespace()) {
                self.advance();
            }
            // line comment //
            if self.peek() == Some('/') && self.peek2() == Some('/') {
                self.advance();
                self.advance();
                while let Some(c) = self.peek() {
                    self.advance();
                    if c == '\n' {
                        break;
                    }
                }
                continue;
            }
            // block comment /* ... */
            if self.peek() == Some('/') && self.peek2() == Some('*') {
                self.advance();
                self.advance();
                let mut depth = 1usize;
                while depth > 0 {
                    match (self.peek(), self.peek2()) {
                        (Some('/'), Some('*')) => {
                            self.advance();
                            self.advance();
                            depth += 1;
                        }
                        (Some('*'), Some('/')) => {
                            self.advance();
                            self.advance();
                            depth -= 1;
                        }
                        (None, _) => break,
                        _ => {
                            self.advance();
                        }
                    }
                }
                continue;
            }
            break;
        }
    }

    // --- number lexing ---

    fn lex_number(&mut self, span: Span) -> CompileResult<Token> {
        // Check for 0t (ternary literal) or 0x / 0b / 0o
        if self.peek() == Some('0') {
            match self.peek2() {
                Some('t') => {
                    // Balanced ternary literal: 0t followed by +, 0, -
                    self.advance(); // '0'
                    self.advance(); // 't'
                    let mut digits: Vec<char> = Vec::new();
                    while let Some(c) = self.peek() {
                        if c == '+' || c == '-' || c == '0' {
                            digits.push(c);
                            self.advance();
                        } else {
                            break;
                        }
                    }
                    if digits.is_empty() {
                        return Err(self.err("empty balanced ternary literal after '0t'"));
                    }
                    let value = balanced_ternary_to_i64(&digits);
                    return Ok(Token::new(TokenKind::TernaryInt(value), span));
                }
                Some('x') | Some('X') => {
                    self.advance(); // '0'
                    self.advance(); // 'x'
                    let mut s = String::new();
                    while self.peek().map_or(false, |c| c.is_ascii_hexdigit() || c == '_') {
                        let c = self.advance().unwrap();
                        if c != '_' {
                            s.push(c);
                        }
                    }
                    let v = i64::from_str_radix(&s, 16)
                        .map_err(|_| self.err(format!("invalid hex literal: 0x{}", s)))?;
                    return Ok(Token::new(TokenKind::Int(v), span));
                }
                Some('b') | Some('B') => {
                    self.advance(); // '0'
                    self.advance(); // 'b'
                    let mut s = String::new();
                    while self.peek().map_or(false, |c| c == '0' || c == '1' || c == '_') {
                        let c = self.advance().unwrap();
                        if c != '_' {
                            s.push(c);
                        }
                    }
                    let v = i64::from_str_radix(&s, 2)
                        .map_err(|_| self.err(format!("invalid binary literal: 0b{}", s)))?;
                    return Ok(Token::new(TokenKind::Int(v), span));
                }
                Some('o') | Some('O') => {
                    self.advance(); // '0'
                    self.advance(); // 'o'
                    let mut s = String::new();
                    while self.peek().map_or(false, |c| matches!(c, '0'..='7') || c == '_') {
                        let c = self.advance().unwrap();
                        if c != '_' {
                            s.push(c);
                        }
                    }
                    let v = i64::from_str_radix(&s, 8)
                        .map_err(|_| self.err(format!("invalid octal literal: 0o{}", s)))?;
                    return Ok(Token::new(TokenKind::Int(v), span));
                }
                _ => {}
            }
        }

        // Regular decimal / float
        let mut s = String::new();
        while self.peek().map_or(false, |c| c.is_ascii_digit() || c == '_') {
            let c = self.advance().unwrap();
            if c != '_' {
                s.push(c);
            }
        }

        // Check for float: decimal point not followed by another dot (range), or 'e'/'E'
        let is_float = (self.peek() == Some('.')
            && self.peek2().map_or(false, |c| c.is_ascii_digit()))
            || self.peek() == Some('e')
            || self.peek() == Some('E');

        if is_float {
            if self.peek() == Some('.') {
                s.push('.');
                self.advance();
                while self.peek().map_or(false, |c| c.is_ascii_digit() || c == '_') {
                    let c = self.advance().unwrap();
                    if c != '_' {
                        s.push(c);
                    }
                }
            }
            if self.peek() == Some('e') || self.peek() == Some('E') {
                s.push('e');
                self.advance();
                if self.peek() == Some('+') || self.peek() == Some('-') {
                    s.push(self.advance().unwrap());
                }
                while self.peek().map_or(false, |c| c.is_ascii_digit()) {
                    s.push(self.advance().unwrap());
                }
            }
            let v: f64 = s.parse().map_err(|_| self.err(format!("invalid float literal: {}", s)))?;
            Ok(Token::new(TokenKind::Float(v), span))
        } else {
            let v: i64 = s.parse().map_err(|_| self.err(format!("invalid integer literal: {}", s)))?;
            Ok(Token::new(TokenKind::Int(v), span))
        }
    }

    // --- string lexing ---

    fn lex_string(&mut self, span: Span) -> CompileResult<Token> {
        // opening " already consumed
        let mut s = String::new();
        loop {
            match self.advance() {
                None => return Err(self.err("unterminated string literal")),
                Some('"') => break,
                Some('\\') => {
                    match self.advance() {
                        Some('n') => s.push('\n'),
                        Some('t') => s.push('\t'),
                        Some('r') => s.push('\r'),
                        Some('\\') => s.push('\\'),
                        Some('"') => s.push('"'),
                        Some('0') => s.push('\0'),
                        Some(c) => {
                            return Err(self.err(format!("unknown escape sequence: \\{}", c)));
                        }
                        None => return Err(self.err("unterminated escape sequence")),
                    }
                }
                Some(c) => s.push(c),
            }
        }
        Ok(Token::new(TokenKind::Str(s), span))
    }

    // --- char lexing ---

    fn lex_char(&mut self, span: Span) -> CompileResult<Token> {
        // opening ' already consumed
        let ch = match self.advance() {
            None => return Err(self.err("unterminated character literal")),
            Some('\\') => match self.advance() {
                Some('n') => '\n',
                Some('t') => '\t',
                Some('r') => '\r',
                Some('\\') => '\\',
                Some('\'') => '\'',
                Some('0') => '\0',
                Some(c) => return Err(self.err(format!("unknown escape in char: \\{}", c))),
                None => return Err(self.err("unterminated char escape")),
            },
            Some(c) => c,
        };
        match self.advance() {
            Some('\'') => {}
            _ => return Err(self.err("char literal must contain exactly one character")),
        }
        Ok(Token::new(TokenKind::Char(ch), span))
    }

    // --- main tokenize ---

    pub fn tokenize(&mut self) -> CompileResult<Vec<Token>> {
        let mut tokens: Vec<Token> = Vec::new();
        loop {
            self.skip_whitespace_and_comments();
            let span = self.current_span();
            let ch = match self.peek() {
                None => {
                    tokens.push(Token::new(TokenKind::Eof, span));
                    break;
                }
                Some(c) => c,
            };

            let tok = match ch {
                // Number
                c if c.is_ascii_digit() => {
                    self.lex_number(span)?
                }

                // Identifier or keyword
                c if c.is_alphabetic() || c == '_' => {
                    let mut s = String::new();
                    while self.peek().map_or(false, |c| c.is_alphanumeric() || c == '_') {
                        s.push(self.advance().unwrap());
                    }
                    Token::new(keyword_or_ident(&s), span)
                }

                // String
                '"' => {
                    self.advance(); // consume "
                    self.lex_string(span)?
                }

                // Char
                '\'' => {
                    self.advance(); // consume '
                    self.lex_char(span)?
                }

                // Operators and delimiters
                '+' => {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        Token::new(TokenKind::PlusEq, span)
                    } else {
                        Token::new(TokenKind::Plus, span)
                    }
                }
                '-' => {
                    self.advance();
                    if self.peek() == Some('>') {
                        self.advance();
                        Token::new(TokenKind::Arrow, span)
                    } else if self.peek() == Some('=') {
                        self.advance();
                        Token::new(TokenKind::MinusEq, span)
                    } else {
                        Token::new(TokenKind::Minus, span)
                    }
                }
                '*' => {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        Token::new(TokenKind::StarEq, span)
                    } else {
                        Token::new(TokenKind::Star, span)
                    }
                }
                '/' => {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        Token::new(TokenKind::SlashEq, span)
                    } else {
                        Token::new(TokenKind::Slash, span)
                    }
                }
                '%' => {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        Token::new(TokenKind::PercentEq, span)
                    } else {
                        Token::new(TokenKind::Percent, span)
                    }
                }
                '=' => {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        Token::new(TokenKind::EqEq, span)
                    } else if self.peek() == Some('>') {
                        self.advance();
                        Token::new(TokenKind::FatArrow, span)
                    } else {
                        Token::new(TokenKind::Eq, span)
                    }
                }
                '!' => {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        Token::new(TokenKind::BangEq, span)
                    } else {
                        Token::new(TokenKind::Bang, span)
                    }
                }
                '<' => {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        Token::new(TokenKind::LtEq, span)
                    } else if self.peek() == Some('<') {
                        self.advance();
                        if self.peek() == Some('=') {
                            self.advance();
                            Token::new(TokenKind::LShiftEq, span)
                        } else {
                            Token::new(TokenKind::LShift, span)
                        }
                    } else {
                        Token::new(TokenKind::Lt, span)
                    }
                }
                '>' => {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        Token::new(TokenKind::GtEq, span)
                    } else if self.peek() == Some('>') {
                        self.advance();
                        if self.peek() == Some('=') {
                            self.advance();
                            Token::new(TokenKind::RShiftEq, span)
                        } else {
                            Token::new(TokenKind::RShift, span)
                        }
                    } else {
                        Token::new(TokenKind::Gt, span)
                    }
                }
                '&' => {
                    self.advance();
                    if self.peek() == Some('&') {
                        self.advance();
                        Token::new(TokenKind::AndAnd, span)
                    } else if self.peek() == Some('=') {
                        self.advance();
                        Token::new(TokenKind::AmpersandEq, span)
                    } else {
                        Token::new(TokenKind::Ampersand, span)
                    }
                }
                '|' => {
                    self.advance();
                    if self.peek() == Some('|') {
                        self.advance();
                        Token::new(TokenKind::OrOr, span)
                    } else if self.peek() == Some('=') {
                        self.advance();
                        Token::new(TokenKind::PipeEq, span)
                    } else {
                        Token::new(TokenKind::Pipe, span)
                    }
                }
                '^' => {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        Token::new(TokenKind::CaretEq, span)
                    } else {
                        Token::new(TokenKind::Caret, span)
                    }
                }
                '~' => {
                    self.advance();
                    Token::new(TokenKind::Tilde, span)
                }
                '.' => {
                    self.advance();
                    if self.peek() == Some('.') {
                        self.advance();
                        if self.peek() == Some('=') {
                            self.advance();
                            Token::new(TokenKind::DotDotEq, span)
                        } else {
                            Token::new(TokenKind::DotDot, span)
                        }
                    } else {
                        Token::new(TokenKind::Dot, span)
                    }
                }
                '?' => {
                    self.advance();
                    Token::new(TokenKind::Question, span)
                }
                ':' => {
                    self.advance();
                    if self.peek() == Some(':') {
                        self.advance();
                        Token::new(TokenKind::ColonColon, span)
                    } else {
                        Token::new(TokenKind::Colon, span)
                    }
                }
                '(' => { self.advance(); Token::new(TokenKind::LParen, span) }
                ')' => { self.advance(); Token::new(TokenKind::RParen, span) }
                '[' => { self.advance(); Token::new(TokenKind::LBracket, span) }
                ']' => { self.advance(); Token::new(TokenKind::RBracket, span) }
                '{' => { self.advance(); Token::new(TokenKind::LBrace, span) }
                '}' => { self.advance(); Token::new(TokenKind::RBrace, span) }
                ',' => { self.advance(); Token::new(TokenKind::Comma, span) }
                ';' => { self.advance(); Token::new(TokenKind::Semi, span) }

                c => {
                    self.advance();
                    return Err(self.err(format!("unexpected character: '{}'", c)));
                }
            };
            tokens.push(tok);
        }
        Ok(tokens)
    }
}

// ---------------------------------------------------------------------------
// Balanced ternary digit string → i64
// ---------------------------------------------------------------------------

fn balanced_ternary_to_i64(digits: &[char]) -> i64 {
    let mut value: i64 = 0;
    for &d in digits {
        let trit: i64 = match d {
            '+' => 1,
            '-' => -1,
            '0' => 0,
            _ => 0,
        };
        value = value * 3 + trit;
    }
    value
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn lex(src: &str) -> Vec<TokenKind> {
        Lexer::new(src)
            .tokenize()
            .unwrap()
            .into_iter()
            .map(|t| t.kind)
            .collect()
    }

    #[test]
    fn test_basic_tokens() {
        let toks = lex("let x = 42;");
        assert_eq!(
            toks,
            vec![
                TokenKind::Let,
                TokenKind::Ident("x".into()),
                TokenKind::Eq,
                TokenKind::Int(42),
                TokenKind::Semi,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn test_ternary_int() {
        let toks = lex("0t+0-");
        // +0- = 1*9 + 0*3 + (-1) = 8
        assert_eq!(toks, vec![TokenKind::TernaryInt(8), TokenKind::Eof]);
    }

    #[test]
    fn test_keywords() {
        let toks = lex("tif tand tor tnot bool3 trit tryte t27");
        assert_eq!(
            toks,
            vec![
                TokenKind::Tif,
                TokenKind::Tand,
                TokenKind::Tor,
                TokenKind::Tnot,
                TokenKind::Bool3Kw,
                TokenKind::TritKw,
                TokenKind::TryteKw,
                TokenKind::T27Kw,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn test_patent_type_aliases() {
        // Patent canonical names must lex to the same tokens as implementation names
        let toks = lex("tribool word trint tfloat");
        assert_eq!(
            toks,
            vec![
                TokenKind::Bool3Kw,   // tribool = bool3
                TokenKind::T27Kw,     // word = t27
                TokenKind::TrintKw,
                TokenKind::TfloatKw,
                TokenKind::Eof,
            ]
        );
    }
}
