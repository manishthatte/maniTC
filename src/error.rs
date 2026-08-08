/// Diagnostic information for a compiler error.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub file: String,
    pub line: usize,
    pub col: usize,
    pub message: String,
}

impl Diagnostic {
    pub fn new(file: impl Into<String>, line: usize, col: usize, message: impl Into<String>) -> Self {
        Diagnostic {
            file: file.into(),
            line,
            col,
            message: message.into(),
        }
    }

    pub fn unknown(message: impl Into<String>) -> Self {
        Diagnostic {
            file: String::from("<unknown>"),
            line: 0,
            col: 0,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}:{}: {}", self.file, self.line, self.col, self.message)
    }
}

/// Top-level compiler error enum.
#[derive(Debug, Clone)]
pub enum CompileError {
    Lex(Diagnostic),
    Parse(Diagnostic),
    Type(Diagnostic),
    Codegen(Diagnostic),
}

impl CompileError {
    pub fn diagnostic(&self) -> &Diagnostic {
        match self {
            CompileError::Lex(d) => d,
            CompileError::Parse(d) => d,
            CompileError::Type(d) => d,
            CompileError::Codegen(d) => d,
        }
    }

    pub fn lex(file: &str, line: usize, col: usize, msg: impl Into<String>) -> Self {
        CompileError::Lex(Diagnostic::new(file, line, col, msg))
    }

    pub fn parse(file: &str, line: usize, col: usize, msg: impl Into<String>) -> Self {
        CompileError::Parse(Diagnostic::new(file, line, col, msg))
    }

    pub fn type_err(file: &str, line: usize, col: usize, msg: impl Into<String>) -> Self {
        CompileError::Type(Diagnostic::new(file, line, col, msg))
    }

    pub fn codegen(msg: impl Into<String>) -> Self {
        CompileError::Codegen(Diagnostic::unknown(msg))
    }
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompileError::Lex(d) => write!(f, "LexError: {}", d),
            CompileError::Parse(d) => write!(f, "ParseError: {}", d),
            CompileError::Type(d) => write!(f, "TypeError: {}", d),
            CompileError::Codegen(d) => write!(f, "CodegenError: {}", d),
        }
    }
}

impl std::error::Error for CompileError {}

/// Convenience Result alias used throughout the compiler.
pub type CompileResult<T> = std::result::Result<T, CompileError>;

// ---------------------------------------------------------------------------
// Rich error rendering with source context
// ---------------------------------------------------------------------------

/// Returns true if stderr supports ANSI colors.
fn stderr_is_tty() -> bool {
    unsafe { libc_isatty(2) != 0 }
}

extern "C" {
    #[link_name = "isatty"]
    fn libc_isatty(fd: i32) -> i32;
}

/// ANSI color codes (used only when stderr is a TTY).
struct Colors {
    red: &'static str,
    yellow: &'static str,
    cyan: &'static str,
    bold: &'static str,
    reset: &'static str,
}

impl Colors {
    fn for_stderr() -> Self {
        if stderr_is_tty() {
            Colors {
                red: "\x1b[31m",
                yellow: "\x1b[33m",
                cyan: "\x1b[36m",
                bold: "\x1b[1m",
                reset: "\x1b[0m",
            }
        } else {
            Colors { red: "", yellow: "", cyan: "", bold: "", reset: "" }
        }
    }
}

/// Render a compile error with source-line context, caret underline, and color.
pub fn render_error(err: &CompileError, source: Option<&str>) -> String {
    let c = Colors::for_stderr();
    let d = err.diagnostic();
    let kind = match err {
        CompileError::Lex(_) => "LexError",
        CompileError::Parse(_) => "ParseError",
        CompileError::Type(_) => "TypeError",
        CompileError::Codegen(_) => "CodegenError",
    };

    let mut out = format!(
        "{}{}error{}: {}{}:{}{}\n",
        c.bold, c.red, c.reset,
        c.bold, kind, c.reset,
        format_args!(" {}:{}:{}: {}", d.file, d.line, d.col, d.message)
    );

    // Show source line if available
    if let Some(src) = source {
        if d.line > 0 {
            if let Some(line_text) = src.lines().nth(d.line - 1) {
                let line_num = format!("{}", d.line);
                let padding = " ".repeat(line_num.len());
                out.push_str(&format!(" {} {}|{}\n", padding, c.cyan, c.reset));
                out.push_str(&format!(" {} {}|{} {}\n", line_num, c.cyan, c.reset, line_text));
                // Caret underline
                if d.col > 0 {
                    let carets = format!("{}{}{}", c.red, "^", c.reset);
                    out.push_str(&format!(
                        " {} {}|{} {}{}\n",
                        padding, c.cyan, c.reset,
                        " ".repeat(d.col - 1), carets
                    ));
                }
            }
        }
    }

    out
}

/// Render a compile warning with source-line context.
pub fn render_warning(warn: &CompileWarning, source: Option<&str>) -> String {
    let c = Colors::for_stderr();
    let d = &warn.diagnostic;

    let mut out = format!(
        "{}{}warning{}: {}:{}:{}: {}\n",
        c.bold, c.yellow, c.reset,
        d.file, d.line, d.col, d.message
    );

    if let Some(src) = source {
        if d.line > 0 {
            if let Some(line_text) = src.lines().nth(d.line - 1) {
                let line_num = format!("{}", d.line);
                let padding = " ".repeat(line_num.len());
                out.push_str(&format!(" {} {}|{}\n", padding, c.cyan, c.reset));
                out.push_str(&format!(" {} {}|{} {}\n", line_num, c.cyan, c.reset, line_text));
                if d.col > 0 {
                    let carets = format!("{}{}{}", c.yellow, "^", c.reset);
                    out.push_str(&format!(
                        " {} {}|{} {}{}\n",
                        padding, c.cyan, c.reset,
                        " ".repeat(d.col - 1), carets
                    ));
                }
            }
        }
    }

    out
}

// ---------------------------------------------------------------------------
// Warning system
// ---------------------------------------------------------------------------

/// Category of compiler warning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WarningKind {
    UnusedVariable,
    UnusedFunction,
    Shadowing,
    UnreachableCode,
    IntegerOverflow,
    DivisionByZero,
    UnknownType,
}

/// A compiler warning — non-fatal but indicates likely mistakes.
#[derive(Debug, Clone)]
pub struct CompileWarning {
    pub kind: WarningKind,
    pub diagnostic: Diagnostic,
}

impl CompileWarning {
    pub fn new(kind: WarningKind, file: &str, line: usize, col: usize, msg: impl Into<String>) -> Self {
        CompileWarning {
            kind,
            diagnostic: Diagnostic::new(file, line, col, msg),
        }
    }
}

impl std::fmt::Display for CompileWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "warning: {}", self.diagnostic)
    }
}

/// Collects warnings produced during compilation.
#[derive(Debug, Clone, Default)]
pub struct WarningCollector {
    pub warnings: Vec<CompileWarning>,
    pub warn_as_error: bool,
}

impl WarningCollector {
    pub fn new() -> Self {
        Self { warnings: Vec::new(), warn_as_error: false }
    }

    pub fn push(&mut self, warning: CompileWarning) {
        self.warnings.push(warning);
    }

    /// Print all warnings to stderr, with optional source context.
    pub fn emit_all(&self) {
        for w in &self.warnings {
            eprintln!("{}", w);
        }
    }

    /// Print all warnings with source-line context.
    pub fn emit_all_rich(&self, source: &str) {
        for w in &self.warnings {
            eprint!("{}", render_warning(w, Some(source)));
        }
    }

    /// If --warn-as-error is set and there are warnings, return the first as an error.
    pub fn check_error(&self) -> CompileResult<()> {
        if self.warn_as_error {
            if let Some(w) = self.warnings.first() {
                return Err(CompileError::Type(w.diagnostic.clone()));
            }
        }
        Ok(())
    }

    pub fn count(&self) -> usize {
        self.warnings.len()
    }
}
