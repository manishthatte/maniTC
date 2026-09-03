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
    TernaryInt(i64), // 0t+0- balanced ternary literal
    /// C6: a trit PATTERN — `0t` followed by a run that contains at
    /// least one wildcard (`?` or a leading `*`) or a capture. The
    /// payload is the text after `0t`; the parser turns it into an
    /// `ast::TritPat`, so the grammar lives in one place.
    ///
    /// A run with no wildcard and no capture is a `TernaryInt` as
    /// before, which is what makes `0t++0` mean the same thing in a
    /// pattern as it does in an expression.
    TritPat(String),

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
    Extern,
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
    /// §11.4's first yield point. A keyword rather than a function so the
    /// surface matches `docs/semantics.md` §11.5's `⟨yield; s, σ⟩`; safe
    /// because no `.mt` in either repository or the model corpus uses `yield`
    /// as an identifier — measured, and every corpus hit is inside a string.
    Yield,
    Break,
    Continue,
    Use,
    Pub,
    Mod,
    Async,
    Await,
    Spawn,
    /// F-4: `region { ... }`, a lexical allocation region.
    Region,
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
    // C1: the Lukasiewicz family. `timp` is what makes the logic L3 rather
    // than Kleene's K3 — the two agree on min/max/negation and differ in
    // exactly one cell of implication.
    Timp,
    Teq,
    Tposs,
    Tnec,
    // C2 / T3ISA v1.5: the lane-wise family. Same connectives as above,
    // applied to all 27 trits of a word at once rather than to one trit.
    // `tnotw` is deliberately absent from the ISA: negating a balanced-ternary
    // number flips every trit, so lane-wise NOT already IS `TNEG`.
    Tandw,
    Torw,
    Txorw,
    Timpw,
    Tcmpw,
    Tnotw,

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

/// P104. The spelling a keyword token carries when it appears INSIDE A LINT
/// NAME, for the parser to rejoin.
///
/// `lint allow(unknown-type);` was a parse error, because `keyword_or_ident`
/// turns `unknown` into `TokenKind::Unknown` and a `Token` records only its
/// kind and span — no source text — so the parser had nothing to rejoin.
/// **Measured: 3 of the 26 lint names were unwritable in a directive** —
/// `unknown-type`, `unknown-lint`, and `literal-out-of-word`, `word` being the
/// patent alias for `t27`. The command-line form (`-A unknown-type`) always
/// worked, because that is a plain argument and never reaches the lexer, so
/// exactly one of the two control surfaces was unreachable.
///
/// **This is deliberately not a general keyword→lexeme inverse, because there
/// is no such function**: `"unknown"` and `"Unknown"` are one token, and so
/// are `"word"` and `"t27"`. What a lint name needs is the spelling LINT NAMES
/// use, which is a different question and has a single answer per token.
///
/// It is a registry that must agree with another registry, so it is checked
/// rather than described (permanent rule 5):
/// `parser_tests::every_lint_name_can_be_written_in_a_directive` iterates
/// `lint::LINTS` itself, splits each name on `-`, and fails naming the word if
/// one of them lexes to a keyword this table does not carry.
pub fn lint_word_lexeme(k: &TokenKind) -> Option<&'static str> {
    match k {
        TokenKind::Unknown => Some("unknown"),
        TokenKind::T27Kw => Some("word"),
        _ => None,
    }
}

fn keyword_or_ident(s: &str) -> TokenKind {
    match s {
        "let" => TokenKind::Let,
        "mut" => TokenKind::Mut,
        "fn" => TokenKind::Fn,
        "struct" => TokenKind::Struct,
        "enum" => TokenKind::Enum,
        "impl" => TokenKind::Impl,
        "trait" => TokenKind::Trait,
        // A1. `lint` is deliberately NOT a keyword: it is a pragma, recognised
        // contextually at item position, and reserving a common English word
        // for it would cost every program that wants it as an identifier.
        "extern" => TokenKind::Extern,
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
        "yield" => TokenKind::Yield,
        "break" => TokenKind::Break,
        "continue" => TokenKind::Continue,
        "use" => TokenKind::Use,
        "pub" => TokenKind::Pub,
        "mod" => TokenKind::Mod,
        "async" => TokenKind::Async,
        "await" => TokenKind::Await,
        "spawn" => TokenKind::Spawn,
        "region" => TokenKind::Region,
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
        "timp" => TokenKind::Timp,
        "teq" => TokenKind::Teq,
        "tposs" => TokenKind::Tposs,
        "tnec" => TokenKind::Tnec,
        "tandw" => TokenKind::Tandw,
        "torw" => TokenKind::Torw,
        "txorw" => TokenKind::Txorw,
        "timpw" => TokenKind::Timpw,
        "tcmpw" => TokenKind::Tcmpw,
        "tnotw" => TokenKind::Tnotw,
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
    /// P8: the merged stdlib module being lexed, stamped onto every span.
    module: Option<&'static str>,
    /// A21: `lex_number` folded a preceding unary minus into the literal.
    neg_folded: bool,
}

impl Lexer {
    pub fn new(source: &str) -> Self {
        Lexer {
            source: source.chars().collect(),
            neg_folded: false,
            pos: 0,
            line: 1,
            col: 1,
            file: String::from("<input>"),
            module: None,
        }
    }

    /// A21: set by `lex_number` when it folded a preceding unary minus into
    /// the literal, so `tokenize` knows to drop that `-` from the stream.
    ///
    /// A flag rather than an inference from the VALUE, because `i64::MIN` can
    /// also arrive from a binary or ternary literal and popping the minus there
    /// would silently change what that program means.
    fn neg_was_folded(&self) -> bool {
        self.neg_folded
    }

    pub fn with_file(source: &str, file: impl Into<String>) -> Self {
        let mut l = Lexer::new(source);
        l.file = file.into();
        l
    }

    /// P8: lex merged stdlib source, stamping every span with its module.
    pub fn with_module(source: &str, module: &'static str) -> Self {
        let mut l = Lexer::new(source);
        l.file = format!("stdlib/{}.mt", module);
        l.module = Some(module);
        l
    }

    // --- position helpers ---

    fn current_span(&self) -> Span {
        // P8: stamp the merged-stdlib module, so a diagnostic inside one names
        // its own file instead of the user's.
        Span { line: self.line, col: self.col, module: self.module }
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

    /// Build a lexer error pointing at an explicit span (the start of the
    /// offending token) rather than at the current cursor position, which
    /// has usually advanced past the token by the time the error is built.
    fn err_at(&self, span: Span, msg: impl Into<String>) -> CompileError {
        CompileError::lex(&self.file, span.line, span.col, msg)
    }

    // --- skip whitespace & comments ---

    fn skip_whitespace_and_comments(&mut self) -> CompileResult<()> {
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
                let start = self.current_span();
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
                        (None, _) => {
                            return Err(self.err_at(
                                start,
                                "unterminated block comment: missing closing `*/`",
                            ));
                        }
                        _ => {
                            self.advance();
                        }
                    }
                }
                continue;
            }
            break;
        }
        Ok(())
    }

    // --- number lexing ---

    fn lex_number(&mut self, span: Span, allow_neg: bool) -> CompileResult<Token> {
        // Check for 0t (ternary literal) or 0x / 0b / 0o
        if self.peek() == Some('0') {
            match self.peek2() {
                Some('t') => {
                    // Balanced ternary literal: 0t followed by +, 0, -
                    //
                    // Lexing rule for the sign characters: a `+` or `-` is
                    // consumed as a trit digit only when the character after
                    // it could NOT begin a new operand. If the sign is
                    // immediately followed by an operand start (an ASCII
                    // digit, letter, `_`, `.`, `(`, `"` or `'`), the literal
                    // ends before the sign and the sign is lexed as an
                    // ordinary binary/unary operator. This makes
                    // `0t+0-1` lex as `0t+0` `-` `1` (= 3 - 1 = 2) while
                    // keeping multi-trit literals like `0t+-0+` intact.
                    self.advance(); // '0'
                    self.advance(); // 't'
                    let mut digits: Vec<char> = Vec::new();
                    // C6: the same run also spells a trit PATTERN. `text` is
                    // everything absorbed, `digits` only the trit digits, so
                    // a run with no wildcard and no capture still produces
                    // exactly the `TernaryInt` it always did.
                    let mut text = String::new();
                    let mut is_pattern = false;
                    while let Some(c) = self.peek() {
                        match c {
                            // C6: `*` is the run wildcard, and it is accepted
                            // ONLY as the first character. That is the
                            // language rule (a `*` elsewhere would have to be
                            // placed by knowing the word's trit width, which
                            // differs between the backends under v1 — see
                            // `ast::TritPat`), and it happens to also remove
                            // the lexical hazard: `0t+*3` keeps its meaning,
                            // `0t+` times 3, because that `*` is not first.
                            // A `*` is the run wildcard when it is the first
                            // element, when the run is already unambiguously a
                            // pattern, or when the character after it could not
                            // begin an operand — the same one-character
                            // lookahead this function already applies to `+`
                            // and `-`, and the reason `0t+*3` keeps meaning
                            // `0t+` times 3.
                            //
                            // Only `?`, `*` and `@` are in that set. `+`, `0`
                            // and `-` are left out on purpose: they ARE legal
                            // operands, so `0t+*+` stays a multiplication. The
                            // effect is that a misplaced `*` is absorbed
                            // exactly when it cannot be anything else, which is
                            // what makes the parser's explanation of the
                            // leftmost-only rule reachable at all.
                            '*' if text.is_empty()
                                || is_pattern
                                || matches!(self.peek2(), Some('?') | Some('*') | Some('@')) =>
                            {
                                is_pattern = true;
                                text.push(c);
                                self.advance();
                            }
                            // C6: `?` is the single-trit wildcard. Absorbing
                            // it changes no existing program: a `?` after an
                            // integer literal is the `Result` try operator,
                            // which is ill-typed on an integer, and there is
                            // no such site in either repository or in the
                            // 2,507-file corpus (the one textual hit is a
                            // comment, copied 126 times).
                            '?' => {
                                is_pattern = true;
                                text.push(c);
                                self.advance();
                            }
                            // C6: `name@` binds the wildcard run just read.
                            // The name FOLLOWS the run, where Rust's `@`
                            // precedes its pattern, and the lexer is why: a
                            // letter is an operand start, so `0t+lo@???`
                            // would end the literal at `0t`. `@` is not a
                            // token in this language, so the postfix form is
                            // unambiguous.
                            '@' => {
                                if !matches!(text.chars().last(), Some('?') | Some('*')) {
                                    return Err(self.err_at(
                                        span,
                                        "`@` in a trit pattern must follow the wildcard run it names",
                                    ));
                                }
                                is_pattern = true;
                                text.push(c);
                                self.advance();
                                let mut name = String::new();
                                while let Some(n) = self.peek() {
                                    if n.is_alphanumeric() || n == '_' {
                                        name.push(n);
                                        self.advance();
                                    } else {
                                        break;
                                    }
                                }
                                if name.is_empty() || name.chars().next().unwrap().is_ascii_digit()
                                {
                                    return Err(self.err_at(
                                        span,
                                        "expected a capture name after `@` in a trit pattern",
                                    ));
                                }
                                text.push_str(&name);
                            }
                            '0' => {
                                digits.push(c);
                                text.push(c);
                                self.advance();
                            }
                            '+' | '-' => {
                                // `0` is excluded from the operand-start set:
                                // it is itself a trit digit, so `0t+0-0`
                                // stays one literal (longest match).
                                let starts_operand = self.peek2().map_or(false, |n| {
                                    (n.is_ascii_digit() && n != '0')
                                        || n.is_alphabetic()
                                        || n == '_'
                                        || n == '.'
                                        || n == '('
                                        || n == '"'
                                        || n == '\''
                                });
                                if starts_operand {
                                    break;
                                }
                                digits.push(c);
                                text.push(c);
                                self.advance();
                            }
                            _ => break,
                        }
                    }
                    if is_pattern {
                        // C6: `text` always ends a complete element here —
                        // the `@` arm consumes its own name — so the parser
                        // receives a run it can read left to right.
                        return Ok(Token::new(TokenKind::TritPat(text), span));
                    }
                    if digits.is_empty() {
                        return Err(self.err_at(span, "empty balanced ternary literal after '0t'"));
                    }
                    let value = balanced_ternary_to_i64(&digits).ok_or_else(|| {
                        self.err_at(
                            span,
                            format!(
                                "balanced ternary literal 0t{} is out of range for a 64-bit integer",
                                digits.iter().collect::<String>()
                            ),
                        )
                    })?;
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
                        .map_err(|_| self.err_at(span, format!("invalid hex literal: 0x{}", s)))?;
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
                        .map_err(|_| self.err_at(span, format!("invalid binary literal: 0b{}", s)))?;
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
                        .map_err(|_| self.err_at(span, format!("invalid octal literal: 0o{}", s)))?;
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
            let v: f64 = s.parse().map_err(|_| self.err_at(span, format!("invalid float literal: {}", s)))?;
            Ok(Token::new(TokenKind::Float(v), span))
        } else {
            // A21: `i64::MIN` has no positive magnitude, so a decimal literal
            // spelling it is rejected here before the parser ever sees the
            // unary minus that makes it representable —
            // `let x: t54 = -9223372036854775808;` was
            // "invalid integer literal: 9223372036854775808", and
            // `stdlib/math.mt` declared `INT_MIN` exactly that way, which is
            // why that file had never passed `check`. (Half of that is stale:
            // under N5 an `int` IS 27 trits, so `INT_MIN` is now the 27-trit
            // minimum and the declaration was wrong independently. What remains
            // is that a `t54` or `--lang v1` context still cannot spell the
            // most negative machine word. `i64::MAX` itself is fine.)
            //
            // Folding is attempted ONLY when the unsigned parse overflows and
            // the signed one succeeds, which is true of exactly one magnitude,
            // 2^63 — and that magnitude is an error today. So the blast radius
            // is one literal that no program can currently contain. Every other
            // negative literal keeps its `Minus` token, which matters:
            // `parse_unary_expr` reads `-` followed by a delimiter as the trit
            // literal `-1`, and folding eagerly would take that away.
            let v: i64 = match s.parse::<i64>() {
                Ok(v) => v,
                Err(_) if allow_neg => {
                    let signed = format!("-{}", s);
                    match signed.parse::<i64>() {
                        Ok(v) => {
                            self.neg_folded = true;
                            v
                        }
                        Err(_) => {
                            return Err(self.err_at(
                                span,
                                format!("invalid integer literal: {}", s),
                            ))
                        }
                    }
                }
                Err(_) => {
                    return Err(
                        self.err_at(span, format!("invalid integer literal: {}", s))
                    )
                }
            };
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
        // `''` (including the first two quotes of `'''`) is an empty char
        // literal — report it here, before consuming any following token.
        if self.peek() == Some('\'') {
            self.advance(); // consume the closing '
            return Err(self.err_at(
                span,
                "empty character literal — write '\\'' for a quote character",
            ));
        }
        let ch = match self.advance() {
            None => return Err(self.err_at(span, "unterminated character literal")),
            Some('\\') => match self.advance() {
                Some('n') => '\n',
                Some('t') => '\t',
                Some('r') => '\r',
                Some('\\') => '\\',
                Some('\'') => '\'',
                Some('0') => '\0',
                Some(c) => return Err(self.err_at(span, format!("unknown escape in char: \\{}", c))),
                None => return Err(self.err_at(span, "unterminated char escape")),
            },
            Some(c) => c,
        };
        match self.peek() {
            Some('\'') => {
                self.advance();
            }
            _ => return Err(self.err_at(span, "char literal must contain exactly one character")),
        }
        Ok(Token::new(TokenKind::Char(ch), span))
    }

    // --- main tokenize ---

    pub fn tokenize(&mut self) -> CompileResult<Vec<Token>> {
        let mut tokens: Vec<Token> = Vec::new();
        loop {
            self.skip_whitespace_and_comments()?;
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
                    // A21: a decimal literal may absorb an immediately
                    // preceding unary minus, and ONLY when that is the
                    // difference between a value and an error. `allow_neg` asks
                    // whether the `-` is in prefix position, decided from the
                    // token BEFORE it: after an identifier, a literal, a `)` or
                    // a `]` the minus is subtraction, so `x - 9223372036854775808`
                    // keeps its error rather than quietly becoming
                    // `x - (-9223372036854775808)`.
                    let allow_neg = matches!(
                        tokens.last().map(|t| &t.kind),
                        Some(TokenKind::Minus)
                    ) && !matches!(
                        tokens.get(tokens.len().wrapping_sub(2)).map(|t| &t.kind),
                        Some(TokenKind::Ident(_))
                            | Some(TokenKind::Int(_))
                            | Some(TokenKind::Float(_))
                            | Some(TokenKind::TernaryInt(_))
                            | Some(TokenKind::Str(_))
                            | Some(TokenKind::Char(_))
                            | Some(TokenKind::Bool(_))
                            | Some(TokenKind::RParen)
                            | Some(TokenKind::RBracket)
                    );
                    self.neg_folded = false;
                    let t = self.lex_number(span, allow_neg)?;
                    if self.neg_was_folded() {
                        // The `-` is part of the literal now, so it must not
                        // also become a `UnOp::Neg` around it — that would
                        // negate `i64::MIN` and overflow.
                        tokens.pop();
                    }
                    t
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
                    return Err(self.err_at(span, format!("unexpected character: '{}'", c)));
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

/// Returns `None` when the literal does not fit in an i64.
fn balanced_ternary_to_i64(digits: &[char]) -> Option<i64> {
    let mut value: i64 = 0;
    for &d in digits {
        let trit: i64 = match d {
            '+' => 1,
            '-' => -1,
            '0' => 0,
            _ => 0,
        };
        value = value.checked_mul(3)?.checked_add(trit)?;
    }
    Some(value)
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
    fn test_ternary_int_stops_before_operand() {
        // F3 regression: a sign followed by an operand start terminates the
        // literal, so `0t+0-1` is `0t+0` `-` `1` (3 - 1 = 2), not 0t"+0-"
        // with a stray `1`.
        let toks = lex("0t+0-1");
        assert_eq!(
            toks,
            vec![
                TokenKind::TernaryInt(3),
                TokenKind::Minus,
                TokenKind::Int(1),
                TokenKind::Eof,
            ]
        );
        // Multi-trit literals keep working, including trailing signs and
        // interior zeros.
        assert_eq!(lex("0t+-0+"), vec![TokenKind::TernaryInt(19), TokenKind::Eof]);
        assert_eq!(lex("0t+0-0"), vec![TokenKind::TernaryInt(24), TokenKind::Eof]);
        // A sign before an identifier is a binary operator, not a digit.
        assert_eq!(
            lex("0t0-x"),
            vec![
                TokenKind::TernaryInt(0),
                TokenKind::Minus,
                TokenKind::Ident("x".into()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn test_ternary_int_overflow_is_error() {
        // F1 regression: a 41-trit literal must report an error instead of
        // panicking (debug) or wrapping (release).
        let src = format!("0t{}0", "+".repeat(40));
        let err = Lexer::new(&src).tokenize().unwrap_err();
        assert!(err.to_string().contains("out of range"), "got: {}", err);
    }

    #[test]
    fn test_unterminated_block_comment_is_error() {
        // F10 regression: an unterminated /* must not silently swallow the
        // rest of the file.
        let err = Lexer::new("let x = 1; /* never closed").tokenize().unwrap_err();
        assert!(err.to_string().contains("unterminated block comment"), "got: {}", err);
    }

    #[test]
    fn test_bad_char_literals() {
        // F11 regression: `'''` must not lex as Char('\'') and `''` must not
        // consume the token after it before erroring.
        let err = Lexer::new("'''").tokenize().unwrap_err();
        assert!(err.to_string().contains("empty character literal"), "got: {}", err);
        let err = Lexer::new("'' x").tokenize().unwrap_err();
        assert!(err.to_string().contains("empty character literal"), "got: {}", err);
        // The escaped form still works.
        assert_eq!(lex("'\\''"), vec![TokenKind::Char('\''), TokenKind::Eof]);
    }

    #[test]
    fn test_error_column_points_at_offending_char() {
        // F12 regression: the reported column is the character's own column,
        // not one past it.
        let err = Lexer::new("let romba<3 @").tokenize().unwrap_err();
        let d = err.diagnostic();
        assert_eq!((d.line, d.col), (1, 13), "got {}:{}", d.line, d.col);
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
