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

// The MSVC CRT exports `_isatty`, not `isatty`; linking the POSIX spelling
// there fails with LNK2019 even though every dependency compiles.
#[cfg(not(target_env = "msvc"))]
extern "C" {
    #[link_name = "isatty"]
    fn libc_isatty(fd: i32) -> i32;
}

#[cfg(target_env = "msvc")]
extern "C" {
    #[link_name = "_isatty"]
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

/// True for characters that terminals render two columns wide (CJK, wide
/// forms, most emoji). Approximation of Unicode East Asian Width = Wide.
fn char_is_wide(ch: char) -> bool {
    matches!(ch as u32,
        0x1100..=0x115F          // Hangul Jamo
        | 0x2E80..=0xA4CF        // CJK radicals … Yi
        | 0xAC00..=0xD7A3        // Hangul syllables
        | 0xF900..=0xFAFF        // CJK compatibility ideographs
        | 0xFE30..=0xFE4F        // CJK compatibility forms
        | 0xFF00..=0xFF60        // fullwidth forms
        | 0xFFE0..=0xFFE6        // fullwidth signs
        | 0x1F300..=0x1FAFF      // emoji & pictographs
        | 0x20000..=0x3FFFD      // CJK extension planes
    )
}

/// Build the whitespace prefix that visually aligns a caret under column
/// `col` (1-based, counted in characters) of `line_text`. Tabs are copied
/// through so they expand identically to the source line, and wide
/// characters count as two columns.
fn caret_padding(line_text: &str, col: usize) -> String {
    let mut pad = String::new();
    for ch in line_text.chars().take(col.saturating_sub(1)) {
        if ch == '\t' {
            pad.push('\t');
        } else if char_is_wide(ch) {
            pad.push_str("  ");
        } else {
            pad.push(' ');
        }
    }
    pad
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
                        caret_padding(line_text, d.col), carets
                    ));
                }
            }
        }
    }

    out
}

/// Render a compile warning with source-line context.
pub fn render_warning(warn: &CompileWarning, source: Option<&str>) -> String {
    render_lint(warn, source, false)
}

/// Render one lint diagnostic, as a warning or as an error.
///
/// A denied lint is an error, and printing it as "warning" and then AGAIN as
/// the compilation error was both misleading and duplicated — the reader saw
/// the same span twice under two severities. It is reported once, at the
/// severity its level says, and the compilation then aborts with a count.
pub fn render_lint(warn: &CompileWarning, source: Option<&str>, as_error: bool) -> String {
    let c = Colors::for_stderr();
    let d = &warn.diagnostic;
    let (word, colour) = if as_error {
        ("error", c.red)
    } else {
        ("warning", c.yellow)
    };

    // The lint name is part of the diagnostic, not decoration: it is the
    // string the reader has to type into `--allow` or `--deny` to change what
    // happens next, and a diagnostic that does not name its own control is a
    // diagnostic you cannot act on.
    let mut out = format!(
        "{}{}{}{}: {}:{}:{}: {} {}[{}]{}\n",
        c.bold, colour, word, c.reset,
        d.file, d.line, d.col, d.message,
        c.cyan, crate::lint::lint_name(&warn.kind), c.reset
    );

    if let Some(src) = source {
        if d.line > 0 {
            if let Some(line_text) = src.lines().nth(d.line - 1) {
                let line_num = format!("{}", d.line);
                let padding = " ".repeat(line_num.len());
                out.push_str(&format!(" {} {}|{}\n", padding, c.cyan, c.reset));
                out.push_str(&format!(" {} {}|{} {}\n", line_num, c.cyan, c.reset, line_text));
                if d.col > 0 {
                    let carets = format!("{}{}{}", colour, "^", c.reset);
                    out.push_str(&format!(
                        " {} {}|{} {}{}\n",
                        padding, c.cyan, c.reset,
                        caret_padding(line_text, d.col), carets
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
    /// A1 step 1: a native called with no `extern` declaration.
    UndeclaredNative,
    /// A call to an extern declared `deprecated("...")`.
    DeprecatedNative,
    /// A1 step 3 (not yet enforced): an extern not `available` on the
    /// selected backend.
    BackendUnavailable,
    /// B1/A4: a generic argument that does not satisfy a declared bound.
    UnsatisfiedBound,
    /// N5/P21: an `int` literal too wide for the 27-trit word.
    ///
    /// A migration backlog under v1, where `int` is deliberately the host word
    /// on LLVM and such a literal is legal there. Under v2 it is not a warning
    /// at all — `int` means 27 trits, so the literal has no value and the
    /// analyzer rejects it outright.
    LiteralOutOfWord,
    /// C4/R2: a `/` or `%` on an integer type, whose meaning differs between
    /// language versions. The migration backlog for the division change.
    DivisionSemantics,
    /// A2: a function is unavailable on the selected backend because something
    /// in its reachable call graph is. Reported with the call chain.
    ///
    /// Distinct from `BackendUnavailable`, which fires at the call site of a
    /// declared extern and says only "this one name is not available". This one
    /// is the INFERRED, transitive property, and the thing it can tell you that
    /// the other cannot is WHICH PATH makes an ordinary ManiT function
    /// uncompilable.
    BackendUnavailableChain,
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
///
/// Since A5 the collector also owns the lint level table, because the level is
/// what decides whether a diagnostic is recorded at all: an `allow` lint is
/// dropped at `push`, not filtered at print time. Dropping it early is what
/// makes `--allow` cost nothing and, more importantly, keeps `count()` — which
/// `manitc check` prints — honest about what was actually reported.
#[derive(Debug, Clone, Default)]
pub struct WarningCollector {
    pub warnings: Vec<CompileWarning>,
    /// `--warn-as-error`. Retained as the name of "raise every lint to deny",
    /// which is what section 54's strict binary was built with.
    pub warn_as_error: bool,
    /// Effective per-lint severity for this compilation (A5).
    pub lints: crate::lint::LintTable,
}

impl WarningCollector {
    pub fn new() -> Self {
        Self {
            warnings: Vec::new(),
            warn_as_error: false,
            lints: crate::lint::LintTable::new(),
        }
    }

    /// Record a warning, unless its lint is set to `allow`.
    pub fn push(&mut self, warning: CompileWarning) {
        if self.effective_level(&warning.kind) == crate::lint::LintLevel::Allow {
            return;
        }
        self.warnings.push(warning);
    }

    /// The level in force for a lint, with `--warn-as-error` folded in.
    ///
    /// `warn_as_error` is applied here rather than by mutating the table so
    /// that the table keeps saying what the user asked for, and the manifest
    /// records the request rather than its consequence.
    pub fn effective_level(&self, kind: &WarningKind) -> crate::lint::LintLevel {
        let lvl = self.lints.level(kind);
        if self.warn_as_error && lvl < crate::lint::LintLevel::Deny {
            crate::lint::LintLevel::Deny
        } else {
            lvl
        }
    }

    /// Print all warnings to stderr, with optional source context.
    pub fn emit_all(&self) {
        for w in &self.warnings {
            eprintln!("{}", w);
        }
    }

    /// Print all warnings with source-line context, each at the severity its
    /// lint level gives it.
    pub fn emit_all_rich(&self, source: &str) {
        for w in &self.warnings {
            let as_error = self.effective_level(&w.kind).is_error();
            eprint!("{}", render_lint(w, Some(source), as_error));
        }
    }

    /// Fail the compilation if any recorded warning is at `deny` or `forbid`.
    ///
    /// The first such warning becomes the error, so the exit status is decided
    /// by severity rather than by order: a `deny` lint reported after twenty
    /// `warn`s still fails the build, which was not true when the only control
    /// was the blanket `--warn-as-error`.
    pub fn check_error(&self) -> CompileResult<()> {
        let n = self.error_count();
        if n == 0 {
            return Ok(());
        }
        // The individual diagnostics were already printed by `emit_all_rich`,
        // so this is the summary line, not a repeat of the first one. Callers
        // that never printed them still get a message that says what happened
        // and which lints to look at.
        let mut lints: Vec<&str> = self
            .warnings
            .iter()
            .filter(|w| self.effective_level(&w.kind).is_error())
            .map(|w| crate::lint::lint_name(&w.kind))
            .collect();
        lints.sort_unstable();
        lints.dedup();
        let first = self
            .warnings
            .iter()
            .find(|w| self.effective_level(&w.kind).is_error())
            .map(|w| w.diagnostic.clone())
            .unwrap_or_else(|| Diagnostic::unknown(""));
        Err(CompileError::Type(Diagnostic::new(
            first.file,
            first.line,
            first.col,
            format!(
                "aborting: {} denied lint{} ({})",
                n,
                if n == 1 { "" } else { "s" },
                lints.join(", ")
            ),
        )))
    }

    /// How many recorded warnings are at `deny` or `forbid`.
    pub fn error_count(&self) -> usize {
        self.warnings
            .iter()
            .filter(|w| self.effective_level(&w.kind).is_error())
            .count()
    }

    pub fn count(&self) -> usize {
        self.warnings.len()
    }
}
