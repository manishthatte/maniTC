//! Lexer for the ManiT core, for the reference interpreter.
//!
//! © Manish Jagdish Thatte
//!
//! INDEPENDENCE RULE: this module and its siblings must not import anything
//! from the rest of the crate. A reference implementation that shares the
//! compiler's lexer cannot witness a lexer bug, and lexer/parser bugs are
//! precisely the class the two-backend oracle is already blind to. See
//! docs/semantics.md §0.

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Int(i64),
    Ident(String),
    Str(String),
    // keywords
    Fn, Let, Mut, If, Elif, Else, Tif, While, Return, As, Use, Match,
    // §11. Concurrency: three yield points and no others, so three
    // keywords is nearly all of it — `send`/`recv` are methods and
    // `channel()` is a call, both of which the core already parses.
    Spawn, Yield, TyChan,
    Question, Dot, Underscore,
    True, False, B3True, B3Unknown, B3False,
    TyTrit, TyBool3, TyBool, TyInt, TyVoid,
    // three-valued
    Tand, Tor, Tnot, Txor, Tcon, Tany, Timp, Teq, Tposs, Tnec,
    // lane-wise
    Tandw, Torw, Txorw, Timpw, Tcmpw, Tnotw,
    // symbols
    LParen, RParen, LBrace, RBrace, Comma, Semi, Colon, ColonColon,
    Arrow, FatArrow, Assign, Eq, Ne, Lt, Gt, Le, Ge,
    Plus, Minus, Star, Slash, Percent, AndAnd, OrOr,
    Eof,
}

pub fn lex(src: &str) -> Result<Vec<Tok>, String> {
    let b: Vec<char> = src.chars().collect();
    let mut i = 0usize;
    let mut out = Vec::new();

    while i < b.len() {
        let c = b[i];

        // whitespace
        if c.is_whitespace() { i += 1; continue; }

        // comments
        if c == '/' && i + 1 < b.len() && b[i + 1] == '/' {
            while i < b.len() && b[i] != '\n' { i += 1; }
            continue;
        }
        if c == '/' && i + 1 < b.len() && b[i + 1] == '*' {
            i += 2;
            while i + 1 < b.len() && !(b[i] == '*' && b[i + 1] == '/') { i += 1; }
            i = (i + 2).min(b.len());
            continue;
        }

        // string literal — only ever printed, so escapes are minimal
        if c == '"' {
            i += 1;
            let mut s = String::new();
            while i < b.len() && b[i] != '"' {
                if b[i] == '\\' && i + 1 < b.len() {
                    i += 1;
                    s.push(match b[i] { 'n' => '\n', 't' => '\t', o => o });
                } else {
                    s.push(b[i]);
                }
                i += 1;
            }
            if i >= b.len() { return Err("unterminated string literal".into()); }
            i += 1;
            out.push(Tok::Str(s));
            continue;
        }

        // number
        if c.is_ascii_digit() {
            let start = i;
            while i < b.len() && (b[i].is_ascii_digit() || b[i] == '_') { i += 1; }
            let text: String = b[start..i].iter().filter(|c| **c != '_').collect();
            let v: i64 = text.parse().map_err(|_| format!("integer literal out of range: {}", text))?;
            out.push(Tok::Int(v));
            continue;
        }

        // identifier or keyword
        if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < b.len() && (b[i].is_alphanumeric() || b[i] == '_') { i += 1; }
            let w: String = b[start..i].iter().collect();
            out.push(match w.as_str() {
                "fn" => Tok::Fn, "let" => Tok::Let, "mut" => Tok::Mut,
                "if" => Tok::If, "elif" => Tok::Elif, "else" => Tok::Else,
                "tif" => Tok::Tif, "while" => Tok::While, "return" => Tok::Return,
                "as" => Tok::As, "use" => Tok::Use, "match" => Tok::Match,
                "_" => Tok::Underscore,
                "true" => Tok::True, "false" => Tok::False,
                "True" => Tok::B3True, "Unknown" => Tok::B3Unknown, "False" => Tok::B3False,
                "spawn" => Tok::Spawn, "yield" => Tok::Yield,
                "chan" => Tok::TyChan,
                "trit" => Tok::TyTrit, "bool3" => Tok::TyBool3,
                "bool" => Tok::TyBool, "int" => Tok::TyInt, "void" => Tok::TyVoid,
                "tand" => Tok::Tand, "tor" => Tok::Tor, "tnot" => Tok::Tnot,
                "txor" => Tok::Txor, "tcon" => Tok::Tcon, "tany" => Tok::Tany,
                "timp" => Tok::Timp, "teq" => Tok::Teq,
                "tposs" => Tok::Tposs, "tnec" => Tok::Tnec,
                "tandw" => Tok::Tandw, "torw" => Tok::Torw, "txorw" => Tok::Txorw,
                "timpw" => Tok::Timpw, "tcmpw" => Tok::Tcmpw, "tnotw" => Tok::Tnotw,
                _ => Tok::Ident(w),
            });
            continue;
        }

        // symbols, longest first
        let two: String = b[i..(i + 2).min(b.len())].iter().collect();
        let t2 = match two.as_str() {
            "::" => Some(Tok::ColonColon),
            "->" => Some(Tok::Arrow),
            "=>" => Some(Tok::FatArrow),
            "==" => Some(Tok::Eq),
            "!=" => Some(Tok::Ne),
            "<=" => Some(Tok::Le),
            ">=" => Some(Tok::Ge),
            "&&" => Some(Tok::AndAnd),
            "||" => Some(Tok::OrOr),
            _ => None,
        };
        if let Some(t) = t2 { out.push(t); i += 2; continue; }

        out.push(match c {
            '(' => Tok::LParen, ')' => Tok::RParen,
            '{' => Tok::LBrace, '}' => Tok::RBrace,
            ',' => Tok::Comma, ';' => Tok::Semi, ':' => Tok::Colon,
            '=' => Tok::Assign, '<' => Tok::Lt, '>' => Tok::Gt,
            '+' => Tok::Plus, '-' => Tok::Minus, '*' => Tok::Star,
            '/' => Tok::Slash, '%' => Tok::Percent,
            '?' => Tok::Question, '.' => Tok::Dot,
            other => return Err(format!("unexpected character: {:?}", other)),
        });
        i += 1;
    }

    out.push(Tok::Eof);
    Ok(out)
}
