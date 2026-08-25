// semantic/analyzer/mod.rs — SemanticAnalyzer: struct, builtins, declarations, type checking.
// check_expr is in expressions.rs.

mod expressions;

use std::collections::HashMap;
use super::types::*;
use super::scope::SymbolTable;
use crate::ast::{self, *};
use crate::error::{CompileError, CompileResult, CompileWarning, WarningKind, WarningCollector};

// ---------------------------------------------------------------------------
// "Did you mean?" helpers
// ---------------------------------------------------------------------------

fn levenshtein(a: &str, b: &str) -> usize {
    let la = a.len();
    let lb = b.len();
    let mut dp = vec![vec![0usize; lb + 1]; la + 1];
    for i in 0..=la { dp[i][0] = i; }
    for j in 0..=lb { dp[0][j] = j; }
    for i in 1..=la {
        for j in 1..=lb {
            let cost = if a.as_bytes()[i - 1] == b.as_bytes()[j - 1] { 0 } else { 1 };
            dp[i][j] = (dp[i - 1][j] + 1)
                .min(dp[i][j - 1] + 1)
                .min(dp[i - 1][j - 1] + cost);
        }
    }
    dp[la][lb]
}

fn did_you_mean(name: &str, candidates: impl Iterator<Item = String>) -> Option<String> {
    let mut best: Option<(usize, String)> = None;
    for c in candidates {
        if c.len() < 3 { continue; }
        let d = levenshtein(name, &c);
        if d <= 3 {
            // Ties are broken lexicographically, NOT by arrival order.
            // Callers pass `HashSet::iter()`, and Rust randomises that order
            // per process, so `d < *bd` — which keeps whichever equally-close
            // candidate happened to come first — made the suggestion differ
            // between two identical runs of the same compiler on the same
            // file: `env::argv` proposed 'arg' one run and 'args' the next,
            // both at distance 1. Harmless while this was a warning nobody
            // pinned; not harmless now that the std-module path is a hard
            // error, because it makes the compiler's own output unpinnable
            // and any test that asserts on it flaky.
            if best.as_ref().map_or(true, |(bd, bc)| (d, &c) < (*bd, bc)) {
                best = Some((d, c));
            }
        }
    }
    best.map(|(_, c)| format!(" — did you mean '{}'?", c))
}

// ---------------------------------------------------------------------------
// Semantic Analyzer
// ---------------------------------------------------------------------------

pub struct SemanticAnalyzer {
    pub(crate) symbols: SymbolTable,
    pub(crate) functions: HashMap<String, (Vec<ManiType>, ManiType)>,
    pub(crate) structs: HashMap<String, Vec<(String, ManiType)>>,
    pub(crate) enums: HashMap<String, Vec<(String, Vec<ManiType>)>>,
    pub(crate) current_fn_ret: ManiType,
    pub(crate) file: String,
    pub(crate) lambda_counter: usize,
    pub(crate) lambda_fns: Vec<TypedFnDef>,
    // User-defined impl method return types: type_name → method_name → return_type
    pub(crate) user_method_types: HashMap<String, HashMap<String, ManiType>>,
    /// User-defined impl method arity: type_name → method_name → the number of
    /// EXPLICIT arguments a call site must supply, i.e. the parameter count
    /// with the `self` receiver already subtracted.
    ///
    /// Separate from `user_method_types` because that map holds only return
    /// types, and separate from `functions` — which does register every method
    /// under `Type::name` with its full parameter list — because `functions`
    /// stores types without names, so it cannot tell whether the first
    /// parameter is the receiver. `Point::shifted(self, dx, dy)` is three
    /// parameters and two arguments, and only the AST knows which.
    ///
    /// An entry exists ONLY where the first parameter is literally `self`. A
    /// method with no entry is not checked at all, which makes false positives
    /// impossible by construction: trait default bodies, associated functions
    /// and every builtin method simply have no entry.
    pub(crate) user_method_arity: HashMap<String, HashMap<String, usize>>,
    // Trait definitions: trait_name → [(method_name, param_types, ret_type, has_default_body)]
    pub(crate) trait_defs: HashMap<String, Vec<(String, Vec<ManiType>, ManiType, bool)>>,
    // Trait implementations: (type_name, trait_name)
    pub(crate) trait_impls: std::collections::HashSet<(String, String)>,
    // Generic type parameters in current scope: param_name → resolved_type
    pub(crate) type_params: HashMap<String, ManiType>,
    // Struct field pub visibility: struct_name → [(field_name, is_pub)]
    pub(crate) struct_pub_fields: HashMap<String, Vec<(String, bool)>>,
    // The type of `self` in the current impl block (for `Self` resolution)
    pub(crate) current_impl_type: Option<String>,
    /// Base directory of the source file being compiled (for resolving relative imports)
    pub(crate) source_dir: std::path::PathBuf,
    /// Set of modules already loaded (prevents circular imports)
    pub(crate) loaded_modules: std::collections::HashSet<std::path::PathBuf>,
    /// Warning collector
    pub warnings: WarningCollector,
    /// Track which variables have been read (for unused variable detection)
    pub(crate) read_vars: std::collections::HashSet<String>,
    /// Names registered by register_builtins (user functions may shadow these)
    pub(crate) builtin_names: std::collections::HashSet<String>,
    /// Prefixes ("foo" / "foo::bar") of successfully loaded user modules
    pub(crate) loaded_module_prefixes: std::collections::HashSet<String>,
    /// Non-`pub` items of loaded user modules: full item name → module prefix
    pub(crate) module_private_items: HashMap<String, String>,
    /// Declared parameter types of every stdlib function, qualified
    /// (`io::println_bool3` → `[bool3]`). Handed to lowering on the
    /// `TypedProgram` so that calls into NATIVE declarations — which have no
    /// body and so never reach `TypedProgram::functions` — still get their
    /// arguments coerced to the declared parameter type.
    pub(crate) native_param_manitys: HashMap<String, Vec<ManiType>>,
    /// A1: explicit `extern` declarations, keyed by qualified name.
    ///
    /// Authoritative where present. A native with an entry here has a
    /// signature in the language's own type system, so its call sites are
    /// checked like any other call instead of going through the unchecked
    /// coercion that made `io::println_int(5 > 0)` a silent conversion.
    pub(crate) externs: HashMap<String, ExternSig>,
    /// Natives called with no `extern` declaration, in source order and
    /// deduplicated. This IS the A1 migration backlog: generated from what the
    /// program actually reaches rather than hand-listed, so it cannot drift
    /// from the code the way a checked-in list would.
    pub undeclared_natives: Vec<String>,
    /// The backend being compiled for ("llvm" / "t3"), when one is selected.
    ///
    /// `None` for `manitc check`, which is backend-agnostic by design — a
    /// check that silently assumed a backend would report an availability
    /// problem the invocation never asked about. A1 step 3 makes this the
    /// input to the availability decision.
    pub backend: Option<String>,
    /// Qualified names of every function that has a ManiT body in the program
    /// being checked, including the stdlib source modules `stdlib_expand`
    /// merges in. The complement of this set, within the stdlib namespace, is
    /// what "native" means: a name a backend must supply as a symbol or a
    /// syscall because no ManiT code was compiled for it.
    pub(crate) bodied_fns: std::collections::HashSet<String>,
    /// A2: the name of the function whose body is being checked, if any.
    ///
    /// Only used to attribute call-graph edges to a caller. `None` while
    /// checking anything that is not inside a function body (a global
    /// initialiser, say), and edges found there are dropped rather than
    /// attributed to whatever was checked last.
    pub(crate) current_fn: Option<String>,
    /// A2: availability clauses WRITTEN on ordinary functions, keyed by name.
    ///
    /// Kept apart from `externs` because the two mean different things. An
    /// extern's clause is a FACT the compiler has no way to verify — nobody
    /// can see inside a C symbol. A function's clause is an ASSERTION about
    /// code the compiler can read, so it is checked.
    pub(crate) declared_fn_avail: HashMap<String, (Vec<String>, crate::ast::Span)>,
    /// A2: the call graph, caller → (callee, call site).
    ///
    /// Built during checking rather than by a separate traversal, so it cannot
    /// disagree with what the checker actually saw. The span is kept per EDGE,
    /// not per callee, because the diagnostic's job is to name a chain and a
    /// chain is a sequence of call sites.
    pub(crate) call_graph: HashMap<String, Vec<(String, crate::ast::Span)>>,
    /// B1: the generic signature of every function that declares a bound,
    /// keyed by name.
    ///
    /// Only functions WITH bounds are recorded. A generic parameter erases to
    /// `Unknown` in `self.functions`, so by the time a call site is checked
    /// there is nothing left to say which parameter was `T` — this keeps the
    /// declaration around for exactly the functions where it matters.
    pub(crate) fn_generic_sigs: HashMap<String, GenericSig>,
    /// R2: the language version being checked against.
    ///
    /// The checker's only use of it is the `division-semantics` lint, whose
    /// message names the meaning `/` has RIGHT NOW and the function that pins
    /// that meaning in both versions. Everything else the version changes is
    /// downstream of here, in the lowerer and the backends.
    pub lang: crate::lang::LangVersion,
}

/// B1: what a bounded generic function declared, preserved for its call sites.
#[derive(Debug, Clone)]
pub struct GenericSig {
    pub generics: Vec<String>,
    pub bounds: Vec<crate::ast::GenericBound>,
    /// The declared type of each parameter, kept in AST form because the
    /// resolved `ManiType` has already erased the generic names.
    pub param_tys: Vec<crate::ast::Type>,
}

/// A1: one recorded `extern` declaration.
#[derive(Debug, Clone)]
pub struct ExternSig {
    pub abi: String,
    pub params: Vec<ManiType>,
    pub ret: ManiType,
    /// `None` = no `available` clause was written. Distinct from `Some(vec![])`,
    /// which would say "available on no backend at all".
    pub available: Option<Vec<String>>,
    pub deprecated: Option<String>,
    pub span: crate::ast::Span,
}

// ---------------------------------------------------------------------------
// Standard library module membership (for `::`-path diagnostics)
// ---------------------------------------------------------------------------

/// Embedded stdlib sources, scanned (textually, so files with known parse
/// gaps still contribute) for their top-level item names.
const STDLIB_SOURCES: &[(&str, &str)] = &[
    ("async",       include_str!("../../../stdlib/async.mt")),
    ("bridge",      include_str!("../../../stdlib/bridge.mt")),
    ("collections", include_str!("../../../stdlib/collections.mt")),
    ("crypto",      include_str!("../../../stdlib/crypto.mt")),
    ("env",         include_str!("../../../stdlib/env.mt")),
    ("fmt",         include_str!("../../../stdlib/fmt.mt")),
    ("fs",          include_str!("../../../stdlib/fs.mt")),
    ("io",          include_str!("../../../stdlib/io.mt")),
    ("math",        include_str!("../../../stdlib/math.mt")),
    ("net",         include_str!("../../../stdlib/net.mt")),
    ("str",         include_str!("../../../stdlib/str.mt")),
    ("sync",        include_str!("../../../stdlib/sync.mt")),
    ("t27f",        include_str!("../../../stdlib/t27f.mt")),
    ("ternary",     include_str!("../../../stdlib/ternary.mt")),
    ("test",        include_str!("../../../stdlib/test.mt")),
    ("trit",        include_str!("../../../stdlib/trit.mt")),
    ("time",        include_str!("../../../stdlib/time.mt")),
    ("tritfs",      include_str!("../../../stdlib/tritfs.mt")),
];

/// Extract top-level (column-0) declaration names from a stdlib module source.
fn scan_module_members(src: &str) -> std::collections::HashSet<String> {
    fn leading_ident(s: &str) -> Option<String> {
        let end = s
            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(s.len());
        if end == 0 { None } else { Some(s[..end].to_string()) }
    }
    let mut out = std::collections::HashSet::new();
    for line in src.lines() {
        if line.is_empty() || line.starts_with(char::is_whitespace) {
            continue;
        }
        let l = line.strip_prefix("pub ").unwrap_or(line);
        // `async fn f()` declares `f` just as `fn f()` does.  Missing this
        // hid all six of async.mt's native declarations, and the gap was
        // papered over by guessing names into STDLIB_EXTRA_MEMBERS — `yield_`
        // for `yield_now`, `spawn` for `spawn_task`.  Both guesses were wrong,
        // and because an extras entry SUPPRESSES the unknown-item diagnostic,
        // each one silently vouched for a function that does not exist.
        let l = l.strip_prefix("async ").unwrap_or(l);
        let ident = if let Some(rest) = l.strip_prefix("fn ") {
            leading_ident(rest)
        } else if let Some(rest) = l.strip_prefix("struct ") {
            leading_ident(rest)
        } else if let Some(rest) = l.strip_prefix("enum ") {
            leading_ident(rest)
        } else if let Some(rest) = l.strip_prefix("trait ") {
            leading_ident(rest)
        } else if let Some(rest) = l.strip_prefix("let ") {
            leading_ident(rest.strip_prefix("mut ").unwrap_or(rest))
        } else {
            None
        };
        if let Some(id) = ident {
            out.insert(id);
        }
    }
    out
}

/// Backend-intrinsic module functions that the code generators support but
/// that are not (yet) declared in the corresponding stdlib .mt source.
/// Extracted from the T3 emitter / IR lowering intrinsic tables.
const STDLIB_EXTRA_MEMBERS: &[(&str, &[&str])] = &[
    ("io", &[
        "newline", "print_int", "println_int", "print_float",
        "print_ternary", "println_ternary", "print_tryte", "println_tryte",
    ]),
    ("ternary", &[
        "t27_shift_left", "t27_shift_right", "t27_and", "t27_or", "t27_neg",
        "t27_explain", "t27_to_int", "t27_to_str", "t27_to_str_padded",
        "int_to_t27", "int_to_t9", "int_to_trit", "int_to_tryte",
        "t9_to_int", "trit_to_int", "tryte_to_int", "tryte_from_trits",
        "trits_to_str",
        // NOT `from_balanced_ternary` — that is declared in math.mt and nowhere
        // else, so `ternary::from_balanced_ternary` names nothing.  Listing it
        // here made the analyzer wave it through: on LLVM it reached a dead
        // helper that skips `__lp_from_flat`, read the first trit as the array
        // length and SEGFAULTED; on T3 it failed to assemble.  Removing it
        // turns both into one located error at the call.
    ]),
    ("math", &["from_balanced_ternary", "to_balanced_ternary", "trit_count"]),
    ("fmt", &["int_to_str", "align_left", "align_right", "pad_left", "pad_right"]),
    ("fs", &[
        "open", "open2", "close", "read", "write", "exists2",
        "read_bytes", "write_bytes", "remove", "close_file", "open_file",
    ]),
    // async.mt declares everything itself, now that `async fn` is scanned.
    ("str", &["to_int"]),
];

/// Top-level item names of a stdlib module, or `None` for unknown modules.
pub(crate) fn std_module_members(
    module: &str,
) -> Option<&'static std::collections::HashSet<String>> {
    use std::sync::OnceLock;
    static MEMBERS: OnceLock<HashMap<&'static str, std::collections::HashSet<String>>> =
        OnceLock::new();
    MEMBERS
        .get_or_init(|| {
            let mut map: HashMap<&'static str, std::collections::HashSet<String>> =
                STDLIB_SOURCES
                    .iter()
                    .map(|(name, src)| (*name, scan_module_members(src)))
                    .collect();
            for (module, extras) in STDLIB_EXTRA_MEMBERS {
                let entry = map.entry(module).or_default();
                for e in *extras {
                    entry.insert((*e).to_string());
                }
            }
            map
        })
        .get(module)
}

impl SemanticAnalyzer {
    pub fn new() -> Self {
        let mut analyzer = SemanticAnalyzer {
            symbols: SymbolTable::new(),
            functions: HashMap::new(),
            structs: HashMap::new(),
            enums: HashMap::new(),
            current_fn_ret: ManiType::Void,
            file: String::from("<input>"),
            lambda_counter: 0,
            lambda_fns: Vec::new(),
            user_method_types: HashMap::new(),
            user_method_arity: HashMap::new(),
            trait_defs: HashMap::new(),
            trait_impls: std::collections::HashSet::new(),
            type_params: HashMap::new(),
            struct_pub_fields: HashMap::new(),
            current_impl_type: None,
            source_dir: std::path::PathBuf::from("."),
            loaded_modules: std::collections::HashSet::new(),
            warnings: WarningCollector::new(),
            read_vars: std::collections::HashSet::new(),
            builtin_names: std::collections::HashSet::new(),
            loaded_module_prefixes: std::collections::HashSet::new(),
            module_private_items: HashMap::new(),
            native_param_manitys: HashMap::new(),
            externs: HashMap::new(),
            undeclared_natives: Vec::new(),
            backend: None,
            current_fn: None,
            call_graph: HashMap::new(),
            declared_fn_avail: HashMap::new(),
            bodied_fns: std::collections::HashSet::new(),
            fn_generic_sigs: HashMap::new(),
            lang: crate::lang::LangVersion::default(),
        };
        analyzer.register_builtins();
        analyzer
    }

    pub fn with_file(file: impl Into<String>) -> Self {
        let mut a = Self::new();
        a.file = file.into();
        a.source_dir = std::path::Path::new(&a.file)
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| std::path::PathBuf::from("."));
        a
    }

    /// C4/R2: record a `/` or `%` whose meaning depends on the language
    /// version, for the `division-semantics` migration backlog.
    ///
    /// `ty` is the type the LOWERER will look at when it chooses between
    /// `Div` and `DivNear` — the left operand for an expression, the assigned
    /// value for a compound assignment. Taking it from anywhere else would let
    /// the backlog and the code generator disagree about which sites are
    /// affected, which is the one thing a migration list must not do.
    ///
    /// Float division is not reported: IEEE division already rounds, and C4
    /// does not touch it.
    pub(crate) fn note_division_semantics(
        &mut self,
        op: &crate::ast::BinOpKind,
        ty: &ManiType,
        span: crate::ast::Span,
    ) {
        use crate::ast::BinOpKind;
        if !matches!(op, BinOpKind::Div | BinOpKind::Rem) {
            return;
        }
        if matches!(ty, ManiType::Float | ManiType::Tfloat) {
            return;
        }
        if self.warnings.effective_level(&WarningKind::DivisionSemantics)
            == crate::lint::LintLevel::Allow
        {
            return;
        }
        let is_div = matches!(op, BinOpKind::Div);
        let symbol = if is_div { "/" } else { "%" };
        let (now, other, pin) = if self.lang.division_rounds_to_nearest() {
            (
                "rounds to nearest (ties away from zero)",
                "truncates under v1",
                if is_div { "math::div_near" } else { "math::rem_near" },
            )
        } else {
            (
                "truncates",
                "rounds to nearest under v2",
                if is_div { "math::div_trunc" } else { "math::rem_trunc" },
            )
        };
        // Name the enclosing function. The span alone is not enough to find
        // the site: `stdlib_expand` parses each merged module with its OWN
        // line numbers and the analyzer reports them under the user's file
        // name, so a site inside `math::` is reported as
        // `hello.mt:135` — right line, wrong file. That is a pre-existing
        // defect in span provenance (report.txt P8) which this lint is simply
        // the first diagnostic to fire inside merged stdlib source; the
        // function name is what makes the backlog a LIST rather than a count
        // until it is fixed.
        let where_ = match &self.current_fn {
            Some(f) => format!(" in `{}`", f),
            None => String::new(),
        };
        let msg = format!(
            "`{}`{} on an integer {} under --lang {}, and {}; write `{}(a, b)` to mean this in both",
            symbol, where_, now, self.lang, other, pin,
        );
        self.warnings.push(CompileWarning::new(
            WarningKind::DivisionSemantics,
            &self.file,
            span.line,
            span.col,
            msg,
        ));
    }

    /// N5 / report.txt P21 cluster 1 — an `int` literal too wide for the word.
    ///
    /// THE SHARPEST CASE IN P21 IS NOT A COMPUTATION, IT IS A LITERAL:
    ///
    /// ```text
    /// fn main() { io::print_int(g(9223372036854775807)); }
    ///
    /// T3    72854775807      the literal does not fit 27 trits and is
    ///                        reshaped before any arithmetic happens
    /// LLVM  TRAP: int addition overflow: result 9223372036854775807 …
    /// ```
    ///
    /// From that point the two backends are computing with different numbers,
    /// which is why four of the ten corpus files that "only differ in trap
    /// wording" go on differing after the wording is fixed: they trap at a
    /// different operation on a different value. It is a FRONT-END question —
    /// does a literal outside the word mean anything? — and not a codegen one.
    ///
    /// TWO ANSWERS, because the versions ask different questions:
    ///
    /// * **v2 rejects it.** N5 says `int` IS 27 trits, so the literal has no
    ///   value, exactly as `let b: i8 = 300` has none. Not a lint: a level
    ///   cannot allow away a value that does not exist.
    /// * **v1 lints it, defaulting to `allow`.** Under v1 `int` is
    ///   deliberately the host word on LLVM and the literal is legal there, so
    ///   this is the MIGRATION BACKLOG, the pattern `undeclared-native` and
    ///   `division-semantics` established. `--warn literal-out-of-word`
    ///   generates the list, and the list is the migration plan.
    ///
    /// L1 IS UNMOVED BY CONSTRUCTION (R5), and that is why it is shaped this
    /// way. `eval/l1_probe.py` runs `manitc check` with no `--lang`, so it
    /// scores v1, where this defaults to `allow` and no verdict can change.
    ///
    /// The type test is `Int | T27` — the SAME predicate `binop_to_ir` uses to
    /// decide N5's checked arithmetic, and for the same reason: `trint` (t54)
    /// is the wider type v2 provides for code that wants the machine word, so
    /// checking it would remove the escape hatch.
    fn check_literal_fits_word(
        &mut self,
        lit: &Lit,
        ty: &ManiType,
        span: crate::ast::Span,
    ) -> CompileResult<()> {
        let v = match lit {
            Lit::Int(v) | Lit::TernaryInt(v) => *v,
            _ => return Ok(()),
        };
        if !matches!(ty, ManiType::Int | ManiType::T27) {
            return Ok(());
        }
        // Balanced ternary is symmetric — no extra negative value — so one
        // magnitude test decides it for both signs.
        if v.unsigned_abs() <= crate::lang::T27_MAX as u64 {
            return Ok(());
        }

        if self.lang.int_is_27_trits() {
            return Err(self.err(
                span,
                format!(
                    "the literal {} does not fit `int`: under --lang v2 an `int` is \
                     a 27-trit word and holds [{}, {}]. `trint` is the wider type \
                     for a value that needs the machine word.",
                    v,
                    crate::lang::T27_MIN,
                    crate::lang::T27_MAX,
                ),
            ));
        }

        if self.warnings.effective_level(&WarningKind::LiteralOutOfWord)
            == crate::lint::LintLevel::Allow
        {
            return Ok(());
        }
        self.warnings.push(CompileWarning::new(
            WarningKind::LiteralOutOfWord,
            &self.file,
            span.line,
            span.col,
            format!(
                "the literal {} is outside the 27-trit range [{}, {}]: it fits `int` \
                 on the LLVM backend and is reshaped on T3, and it will not compile \
                 under --lang v2. `trint` is the wider type.",
                v,
                crate::lang::T27_MIN,
                crate::lang::T27_MAX,
            ),
        ));
        Ok(())
    }

    fn register_builtins(&mut self) {
        use ManiType::*;

        // Helper: construct a builtin table entry.
        // Each entry is (name, param_types, return_type).
        type Entry = (&'static str, Vec<ManiType>, ManiType);

        // Shorthand slices for common patterns.
        let ii  = vec![Int, Int];
        let iiii = vec![Int, Int, Int, Int];
        let s   = vec![Str];
        let sii = vec![Str, Int, Int];
        let ss  = vec![Str, Str];
        let sss = vec![Str, Str, Str];
        let is_ = vec![Int, Int, Str];   // renamed to avoid conflict with `is`
        let si_  = vec![Str, Int];

        // Entries grouped by subsystem.
        let entries: Vec<Entry> = vec![
            // I/O intrinsics
            ("println",       vec![Unknown], Void),
            ("print",         vec![Unknown], Void),
            // Math
            ("abs",           vec![Int],     Int),
            ("sqrt",          vec![Float],   Float),
            // fmt module
            ("fmt::format",      vec![Str, Unknown], Str),
            ("fmt::show_int",    vec![Int],           Str),
            ("fmt::show_float",  vec![Float],         Str),
            ("fmt::show_bool",   vec![Bool],          Str),
            // String operations
            ("str_len",          s.clone(),           Int),
            ("str_find",         ss.clone(),          Int),
            ("str_concat",       ss.clone(),          Str),
            ("str_slice",        sii.clone(),         Str),
            ("str_substr",       sii.clone(),         Str),
            ("str_trim",         s.clone(),           Str),
            ("str_replace",      sss.clone(),         Str),
            ("str_parse_int",    s.clone(),           Int),
            ("str_ends_with",    ss.clone(),          Bool),
            ("str_starts_with",  ss.clone(),          Bool),
            ("str_from_int",     vec![Int],           Str),
            ("str_from_char",    vec![Int],           Str),
            // str_to_upper / str_to_lower are deliberately absent: they became
            // ManiT source in stdlib/str.mt on 19 Aug 2026, so the analyzer
            // sees their real declared signature. A static entry here would be
            // a second source of truth that can drift from the stdlib.
            // async module
            ("async::yield_now",   vec![],                 Void),
            ("async::sleep",       vec![Int],              Void),
            ("async::spawn_task",  vec![Unknown],          Generic("Task".to_string(), vec![Unknown])),
            ("async::select",      vec![Unknown],          Unknown),
            // time module
            ("time::sleep",     vec![Int], Void),
            ("env_timestamp",   vec![],    Str),
            ("env_date",        vec![],    Str),
            ("env_time",        vec![],    Str),
            // Filesystem
            ("fs_list_dir_open",  s.clone(),    Int),
            ("fs_list_dir_entry", vec![Int],    Str),
            ("fs_copy",           ss.clone(),   Int),
            ("fs_move",           ss.clone(),   Int),
            ("fs_mkdir",          s.clone(),    Int),
            ("fs_is_dir",         s.clone(),    Int),
            ("fs_file_size",      s.clone(),    Int),
            ("fs_read_file",      s.clone(),    Str),
            ("fs_write_file",     ss.clone(),   Void),
            ("fs_copy_file",      ss.clone(),   Void),
            ("fs_rename",         ss.clone(),   Void),
            ("fs_delete",         s.clone(),    Int),
            // Defined in runtime/system.c and declared in the LLVM emitter, but
            // absent from this table until 23 Aug 2026 — so every call to it
            // resolved to Unknown and got a warning instead of a signature.
            // It was the ONLY name in all 128 shipped .mt files still relying
            // on that leniency, which is why it had to be registered before
            // unknown identifiers could become errors.
            ("fs_remove_file",    s.clone(),    Void),
            // Path helpers
            ("path_join",         ss.clone(),   Str),
            ("path_parent",       s.clone(),    Str),
            ("path_file_name",    s.clone(),    Str),
            // Shell / process
            ("process_spawn",   s.clone(),  Int),
            ("shell_exec",      s.clone(),  Str),
            // Network
            ("net_http_get",    s.clone(),  Str),
            // Terminal (TUI)
            ("terminal_set_raw",      vec![], Int),
            ("terminal_set_cooked",   vec![], Int),
            ("terminal_get_rows",     vec![], Int),
            ("terminal_get_cols",     vec![], Int),
            ("io_read_char",          vec![], Int),
            ("io_read_key",           vec![], Int),
            ("io_clear_screen",       vec![], Int),
            ("io_move_cursor",        ii.clone(), Int),
            ("io_set_reverse",        vec![], Int),
            ("io_reset_attr",         vec![], Int),
            ("io_set_bold",           vec![], Int),
            // SDL2 GUI — window / render
            ("gui_init",          is_.clone(),  Int),
            ("gui_quit",          vec![],       Int),
            ("gui_clear",         vec![],       Int),
            ("gui_present",       vec![],       Int),
            ("gui_set_color",     iiii.clone(), Int),
            ("gui_fill_rect",     iiii.clone(), Int),
            ("gui_draw_rect",     iiii.clone(), Int),
            ("gui_draw_line",     iiii.clone(), Int),
            ("gui_draw_text",     sii.clone(),  Int),
            ("gui_draw_text_lg",  sii.clone(),  Int),
            ("gui_text_width",    s.clone(),    Int),
            ("gui_font_height",   vec![],       Int),
            ("gui_window_width",  vec![],       Int),
            ("gui_window_height", vec![],       Int),
            // SDL2 GUI — events
            ("gui_poll_event",        vec![],      Int),
            ("gui_wait_event",        vec![Int],   Int),
            ("gui_event_type",        vec![],      Int),
            ("gui_event_key",         vec![],      Int),
            ("gui_mouse_x",           vec![],      Int),
            ("gui_mouse_y",           vec![],      Int),
            ("gui_mouse_btn",         vec![],      Int),
            ("gui_event_text_char",   vec![],      Int),
            ("gui_event_text_str",    vec![],      Str),
            ("gui_wheel_dy",          vec![],      Int),
            ("gui_ticks",             vec![],      Int),
            ("gui_delay",             vec![Int],   Int),
            // SDL2 GUI — named key constants (all return Int)
            ("gui_key_return",    vec![], Int), ("gui_key_escape",  vec![], Int),
            ("gui_key_backspace", vec![], Int), ("gui_key_delete",  vec![], Int),
            ("gui_key_up",        vec![], Int), ("gui_key_down",    vec![], Int),
            ("gui_key_left",      vec![], Int), ("gui_key_right",   vec![], Int),
            ("gui_key_home",      vec![], Int), ("gui_key_end",     vec![], Int),
            ("gui_key_pageup",    vec![], Int), ("gui_key_pagedown",vec![], Int),
            ("gui_key_tab",       vec![], Int), ("gui_key_space",   vec![], Int),
            ("gui_key_f1",  vec![], Int), ("gui_key_f2",  vec![], Int),
            ("gui_key_f3",  vec![], Int), ("gui_key_f4",  vec![], Int),
            ("gui_key_f5",  vec![], Int), ("gui_key_f6",  vec![], Int),
            ("gui_key_f7",  vec![], Int), ("gui_key_f8",  vec![], Int),
            ("gui_key_f9",  vec![], Int), ("gui_key_f10", vec![], Int),
            ("gui_key_f11", vec![], Int), ("gui_key_f12", vec![], Int),
            ("gui_key_mod_ctrl",  vec![], Int),
            ("gui_key_mod_shift", vec![], Int),
            ("gui_key_mod_alt",   vec![], Int),
            // SDL2 GUI — clipboard
            ("gui_clipboard_get", vec![],   Str),
            ("gui_clipboard_set", s.clone(), Void),
        ];

        for (name, params, ret) in entries {
            self.functions.insert(name.to_string(), (params, ret));
        }

        self.register_native_module_sigs();

        // Letter keys a–z: gui_key_a .. gui_key_z
        for ch in b'a'..=b'z' {
            self.functions.insert(format!("gui_key_{}", ch as char), (vec![], Int));
        }

        // Concurrency primitives (opaque generic return types — not representable in static table)
        let chan_ty = Generic("Channel".to_string(), vec![Unknown]);
        self.functions.insert("channel".to_string(),     (vec![], chan_ty.clone()));
        self.functions.insert("channel_new".to_string(), (vec![], chan_ty));
        self.functions.insert("Mutex::new".to_string(),
            (vec![Unknown], Generic("Mutex".to_string(), vec![Unknown])));
        self.functions.insert("AtomicTrit::new".to_string(), (vec![Trit],  Struct("AtomicTrit".to_string())));
        self.functions.insert("Barrier::new".to_string(),    (vec![Int],   Struct("Barrier".to_string())));
        self.functions.insert("Semaphore::new".to_string(),  (vec![Int],   Struct("Semaphore".to_string())));

        // Record all registered names so user functions may shadow them.
        self.builtin_names = self.functions.keys().cloned().collect();

        // Suppress unused-variable warnings for the shorthand vectors constructed above.
        drop(si_);
    }

    fn err(&self, span: Span, msg: impl Into<String>) -> CompileError {
        CompileError::type_err(&self.file, span.line, span.col, msg)
    }

    // Convert AST type to ManiType
    fn resolve_type(&self, ty: &Type) -> CompileResult<ManiType> {
        match ty {
            Type::Named(name, span) => {
                // Generic type params take priority over everything else
                if let Some(ty) = self.type_params.get(name.as_str()) {
                    return Ok(ty.clone());
                }
                Ok(self.name_to_manitype(name, *span)?)
            }
            Type::Path(parts, span) => {
                // Join path and resolve
                let joined = parts.join("::");
                self.name_to_manitype(&joined, *span)
            }
            Type::Generic(name, args, _span) => {
                let resolved_args: CompileResult<Vec<ManiType>> =
                    args.iter().map(|a| self.resolve_type(a)).collect();
                let resolved_args = resolved_args?;
                match name.as_str() {
                    "Result" => Ok(ManiType::Generic("Result".to_string(), resolved_args)),
                    // `Option<T>` is refused, not implemented. It was
                    // half-declared surface — resolvable as a type, typed for
                    // `.unwrap()`, and with no constructor on either backend, so
                    // `Some(7)` reached codegen and died at assembly with
                    // "Undefined label: Some". No `.mt` source in any tree has
                    // ever used it (the one test called `test_option` is written
                    // entirely in `Result`).
                    //
                    // It is not implemented because it is the wrong shape for
                    // this language: `Result` already carries THREE outcomes —
                    // Ok / Unknown / Err — and `Unknown` is precisely what
                    // `None` means. Adding a two-state type beside a three-state
                    // one that subsumes it is the binary habit this stack exists
                    // to break.
                    "Option" => Err(self.err(*_span, format!(
                        "there is no `Option<T>` in ManiT. `Result<T, E>` is this \
                         language's option type and it has three outcomes rather \
                         than two: `Ok(v)` for a value, `Unknown(msg)` where you \
                         would write `None`, and `Err(e)` for a failure. Write \
                         `Result<{}, str>`.",
                        resolved_args.first().map(|t| t.display()).unwrap_or_else(|| "T".to_string()),
                    ))),
                    "Vec" => Ok(ManiType::Generic("Vec".to_string(), resolved_args)),
                    "Map" => Ok(ManiType::Generic("Map".to_string(), resolved_args)),
                    "Set" => Ok(ManiType::Generic("Set".to_string(), resolved_args)),
                    "Deque" => Ok(ManiType::Generic("Deque".to_string(), resolved_args)),
                    "TernaryTrie" => Ok(ManiType::Generic("TernaryTrie".to_string(), resolved_args)),
                    "Channel" => Ok(ManiType::Generic("Channel".to_string(), resolved_args)),
                    "Mutex" => Ok(ManiType::Generic("Mutex".to_string(), resolved_args)),
                    "AtomicTrit" => Ok(ManiType::Struct("AtomicTrit".to_string())),
                    "Barrier" => Ok(ManiType::Struct("Barrier".to_string())),
                    "Semaphore" => Ok(ManiType::Struct("Semaphore".to_string())),
                    "Pair" => Ok(ManiType::Generic("Pair".to_string(), resolved_args)),
                    "Range" => Ok(ManiType::Generic("Range".to_string(), resolved_args)),
                    _ => {
                        // Check if it's a known struct
                        if self.structs.contains_key(name.as_str()) {
                            Ok(ManiType::Struct(name.clone()))
                        } else {
                            // Permissive: treat as unknown generic
                            Ok(ManiType::Generic(name.clone(), resolved_args))
                        }
                    }
                }
            }
            Type::Array(inner, size, _) => {
                let inner_ty = self.resolve_type(inner)?;
                Ok(ManiType::Array(Box::new(inner_ty), *size))
            }
            Type::Tuple(types, _) => {
                let resolved: CompileResult<Vec<ManiType>> =
                    types.iter().map(|t| self.resolve_type(t)).collect();
                Ok(ManiType::Tuple(resolved?))
            }
            Type::Fn(params, ret, _) => {
                let param_tys: CompileResult<Vec<ManiType>> =
                    params.iter().map(|p| self.resolve_type(p)).collect();
                let ret_ty = self.resolve_type(ret)?;
                Ok(ManiType::Fn(param_tys?, Box::new(ret_ty)))
            }
            Type::Ref(inner, _, _) => self.resolve_type(inner),
            Type::Ptr(inner, _, _) => self.resolve_type(inner),
            Type::Infer(_) => Ok(ManiType::Unknown),
        }
    }

    fn name_to_manitype(&self, name: &str, _span: Span) -> CompileResult<ManiType> {
        match name {
            // `Self` resolves to the current impl type
            "Self" => {
                if let Some(impl_ty) = &self.current_impl_type {
                    return Ok(ManiType::Struct(impl_ty.clone()));
                }
                return Ok(ManiType::Unknown);
            }
            "int" | "i64" => Ok(ManiType::Int),
            "float" | "f64" => Ok(ManiType::Float),
            "bool" => Ok(ManiType::Bool),
            "bool3" | "tribool" | "T3Bool" => Ok(ManiType::Bool3),
            "trit" => Ok(ManiType::Trit),
            "tryte" => Ok(ManiType::Tryte),
            "t9" => Ok(ManiType::T9),
            "t27" | "word" => Ok(ManiType::T27),
            "t54" => Ok(ManiType::T54),
            "trint" => Ok(ManiType::T54), // trint is a source-level alias for t54
            "tfloat" => Ok(ManiType::Tfloat),
            "str" | "String" => Ok(ManiType::Str),
            "char" => Ok(ManiType::Char),
            "void" | "()" => Ok(ManiType::Void),
            // Concurrency types treated as opaque structs
            "AtomicTrit" | "Barrier" | "Semaphore" | "MutexGuard" => {
                Ok(ManiType::Struct(name.to_string()))
            }
            _ => {
                if self.structs.contains_key(name) {
                    Ok(ManiType::Struct(name.to_string()))
                } else if self.enums.contains_key(name) {
                    Ok(ManiType::Enum(name.to_string()))
                } else {
                    // Unknown type — emit Unknown to allow partial compilation
                    // In strict mode this would be an error
                    Ok(ManiType::Unknown)
                }
            }
        }
    }

    // ---------------------------------------------------------------------------
    // Public entry point
    // ---------------------------------------------------------------------------

    /// Register return types for the qualified names of native stdlib modules.
    ///
    /// The table above lists the MANGLED C symbols (`str_slice`, `str_concat`),
    /// never the qualified spelling a program actually writes (`str::slice`).
    /// That went unnoticed for as long as `str::` was unreachable — the lexer
    /// made `str` a type keyword, so no program could name the module at all
    /// (ORACLE_FINDINGS.md Section 9). The moment the parser accepted it, every
    /// `str::` call typed as Unknown, and `Unknown == Str` lowers to INTEGER
    /// equality on two pointers, so `str::slice("hello", 2, 3) == "l"` was
    /// false on both backends even though the slice really is `"l"`.
    /// Binding to a `let s: str` first made it work, which is what made this
    /// look like a memory bug rather than a typing one.
    ///
    /// Derived from the module sources rather than hand-listed, so the two
    /// cannot drift. Only RETURN types enter `self.functions`: parameters there
    /// are left Unknown so this fixes the typing without turning on argument
    /// enforcement for calls that were previously unchecked — that is a
    /// separate change.
    ///
    /// The declared PARAMETER types are recorded separately, in
    /// `native_param_manitys`, and travel to lowering on the `TypedProgram`.
    /// That map does not gate anything the analyzer accepts; it exists so the
    /// IR lowerer can apply the same argument coercion to a stdlib call that it
    /// already applies to a user-defined one. Every stdlib module contributes
    /// to it, not just the four whose return types are registered here — a
    /// native declaration is precisely the case where the declared parameter
    /// type exists nowhere else, since a body-less function never reaches
    /// `TypedProgram::functions`. Missing it made `io::println_bool3(false)`
    /// print `unknown` on BOTH backends (S45).
    /// A5: apply one `lint <level>(<names>);` item.
    ///
    /// An unknown level or an unknown lint name is an ERROR, not a warning.
    /// The whole point of the item is to control diagnostics, so a typo that
    /// quietly did nothing would silently leave the compilation at a
    /// strictness the author did not choose — which is the failure mode A5
    /// exists to remove, reintroduced one layer up.
    fn apply_lint_decl(&mut self, decl: &ast::LintDecl) -> CompileResult<()> {
        let Some(level) = crate::lint::LintLevel::from_name(&decl.level) else {
            return Err(CompileError::Type(crate::error::Diagnostic::new(
                &self.file,
                decl.span.line,
                decl.span.col,
                format!(
                    "unknown lint level '{}'; expected allow, warn, deny or forbid",
                    decl.level
                ),
            )));
        };
        for name in &decl.lints {
            self.warnings.lints.set(name, level).map_err(|e| {
                CompileError::Type(crate::error::Diagnostic::new(
                    &self.file,
                    decl.span.line,
                    decl.span.col,
                    e,
                ))
            })?;
        }
        Ok(())
    }

    /// A1: record one `extern` declaration.
    ///
    /// The declaration is registered in three places, and all three matter:
    /// `externs` for call-site checking, `functions` so the return type is
    /// inferred at every call, and `native_param_manitys` so the lowerer
    /// coerces arguments the same way it does for a user function. Registering
    /// only the first would type-check the call and then lower it wrongly,
    /// which is the shape of S45.
    fn collect_extern_decl(&mut self, decl: &ast::ExternDecl) -> CompileResult<()> {
        if let Some(prev) = self.externs.get(&decl.name) {
            return Err(CompileError::Type(crate::error::Diagnostic::new(
                &self.file,
                decl.span.line,
                decl.span.col,
                format!(
                    "extern '{}' is already declared at line {}; a native may be \
                     declared once, so that the declaration is the authority on \
                     its signature",
                    decl.name, prev.span.line
                ),
            )));
        }

        let mut params = Vec::with_capacity(decl.params.len());
        for p in &decl.params {
            params.push(self.resolve_type(&p.ty)?);
        }
        let ret = match &decl.ret_ty {
            Some(t) => self.resolve_type(t)?,
            None => ManiType::Void,
        };

        // A backend name that is not one the compiler has is a typo, and a
        // typo in `available(llmv)` would make step 3 refuse the call on every
        // backend for a reason the source does not show.
        if let Some(backends) = &decl.available {
            for b in backends {
                if !matches!(b.as_str(), "llvm" | "t3") {
                    return Err(CompileError::Type(crate::error::Diagnostic::new(
                        &self.file,
                        decl.span.line,
                        decl.span.col,
                        format!(
                            "unknown backend '{}' in `available(...)`; the backends \
                             are llvm and t3",
                            b
                        ),
                    )));
                }
            }
        }

        self.externs.insert(
            decl.name.clone(),
            ExternSig {
                abi: decl.abi.clone(),
                params: params.clone(),
                ret: ret.clone(),
                available: decl.available.clone(),
                deprecated: decl.deprecated.clone(),
                span: decl.span,
            },
        );

        // Authoritative over the signature derived by scanning the stdlib
        // sources: that scan infers, this one is written down.
        self.native_param_manitys.insert(decl.name.clone(), params.clone());
        self.functions.insert(decl.name.clone(), (params, ret));
        Ok(())
    }

    /// B1: traits every primitive satisfies without an `impl`.
    ///
    /// A bound is only useful if the ordinary types satisfy the ordinary
    /// traits: requiring `impl Ord for int` before `max<T: Ord>(1, 2)` would
    /// compile is a tax with no safety return, since the compiler already
    /// knows how to compare an int. Structs and enums are NOT covered — they
    /// are exactly the case A4 is about, where the comparison silently became
    /// a pointer comparison.
    pub(crate) const BUILTIN_TRAITS: &'static [&'static str] = &[
        "Ord", "PartialOrd", "Eq", "PartialEq", "Display", "Debug", "Clone", "Copy", "Hash",
    ];

    /// Whether a concrete type satisfies one trait.
    fn type_satisfies(&self, ty: &ManiType, trait_name: &str) -> bool {
        // A user impl always satisfies, for any type including a primitive.
        let base = Self::type_impl_key(ty);
        if let Some(base) = &base {
            if self.trait_impls.contains(&(base.clone(), trait_name.to_string())) {
                return true;
            }
        }
        // Primitives satisfy the structural traits intrinsically.
        let is_primitive = !matches!(
            ty,
            ManiType::Struct(_) | ManiType::Enum(_) | ManiType::Unknown | ManiType::Fn(_, _)
        );
        is_primitive && Self::BUILTIN_TRAITS.contains(&trait_name)
    }

    /// The name a type is keyed under in `trait_impls`, if it has one.
    fn type_impl_key(ty: &ManiType) -> Option<String> {
        match ty {
            ManiType::Struct(n) | ManiType::Enum(n) => Some(n.clone()),
            ManiType::Generic(n, _) => Some(n.clone()),
            ManiType::Int => Some("int".to_string()),
            ManiType::Float => Some("float".to_string()),
            ManiType::Str => Some("str".to_string()),
            ManiType::Char => Some("char".to_string()),
            ManiType::Bool => Some("bool".to_string()),
            ManiType::Bool3 => Some("bool3".to_string()),
            ManiType::Trit => Some("trit".to_string()),
            ManiType::Tryte => Some("tryte".to_string()),
            ManiType::T9 => Some("t9".to_string()),
            ManiType::T27 => Some("t27".to_string()),
            ManiType::T54 => Some("t54".to_string()),
            ManiType::Tfloat => Some("tfloat".to_string()),
            _ => None,
        }
    }

    /// B1: bind generic parameter names to the concrete types an argument
    /// supplied, structurally.
    ///
    /// Handles the two forms the bound examples use: a bare `T`, and a `T`
    /// inside a constructor such as `Vec<T>`. A parameter that binds twice
    /// keeps the FIRST binding — reporting a conflict is a different check
    /// (and a different diagnostic) from reporting an unsatisfied bound, and
    /// conflating them would make the bound error fire for the wrong reason.
    fn bind_generics(
        declared: &crate::ast::Type,
        actual: &ManiType,
        generics: &[String],
        out: &mut HashMap<String, ManiType>,
    ) {
        use crate::ast::Type as AT;
        match declared {
            AT::Named(n, _) => {
                if generics.iter().any(|g| g == n) && !out.contains_key(n) {
                    out.insert(n.clone(), actual.clone());
                }
            }
            AT::Generic(_, args, _) => {
                if let ManiType::Generic(_, actual_args) = actual {
                    for (d, a) in args.iter().zip(actual_args.iter()) {
                        Self::bind_generics(d, a, generics, out);
                    }
                }
            }
            AT::Array(inner, _, _) => {
                if let ManiType::Array(a, _) = actual {
                    Self::bind_generics(inner, a, generics, out);
                }
            }
            AT::Ref(inner, _, _) | AT::Ptr(inner, _, _) => {
                Self::bind_generics(inner, actual, generics, out);
            }
            AT::Tuple(items, _) => {
                if let ManiType::Tuple(actuals) = actual {
                    for (d, a) in items.iter().zip(actuals.iter()) {
                        Self::bind_generics(d, a, generics, out);
                    }
                }
            }
            _ => {}
        }
    }

    /// B1/A4: check a call against the callee's declared bounds.
    ///
    /// This is the A4 soundness hole closing. Before bounds existed,
    /// `max<T>(a, b)` on a struct with no ordering compiled clean, checked
    /// clean, and returned the wrong value on BOTH backends — it compared the
    /// two allocation addresses, so `max(P{9}, P{1})` returned `P{1}`. The two
    /// backends agreed, which is why the differential oracle never saw it.
    pub(crate) fn check_generic_bounds(
        &mut self,
        callee: &str,
        arg_tys: &[ManiType],
        span: crate::ast::Span,
    ) {
        let Some(sig) = self.fn_generic_sigs.get(callee).cloned() else {
            return;
        };
        let mut binding: HashMap<String, ManiType> = HashMap::new();
        for (declared, actual) in sig.param_tys.iter().zip(arg_tys.iter()) {
            Self::bind_generics(declared, actual, &sig.generics, &mut binding);
        }

        for bound in &sig.bounds {
            let Some(ty) = binding.get(&bound.param) else {
                // The parameter appears in no argument position, so nothing
                // pins it. Silent by design: reporting here would fire on
                // return-position-only generics, which the bound cannot be
                // checked against at the call site at all.
                continue;
            };
            if !ty.is_known() {
                // Inference did not reach a concrete type. Reporting an
                // unsatisfied bound for a type we could not determine would be
                // a false positive, and A4 is about a REAL wrong answer.
                continue;
            }
            for trait_name in &bound.traits {
                if self.type_satisfies(ty, trait_name) {
                    continue;
                }
                let known_trait = self.trait_defs.contains_key(trait_name)
                    || Self::BUILTIN_TRAITS.contains(&trait_name.as_str());
                let msg = if known_trait {
                    format!(
                        "`{}` does not satisfy the bound `{}: {}` required by '{}'; \
                         add `impl {} for {}`",
                        ty.display(), bound.param, trait_name, callee,
                        trait_name, ty.display()
                    )
                } else {
                    format!(
                        "'{}' requires the bound `{}: {}`, but no trait named `{}` \
                         is declared",
                        callee, bound.param, trait_name, trait_name
                    )
                };
                self.warnings.push(CompileWarning::new(
                    WarningKind::UnsatisfiedBound,
                    &self.file,
                    span.line,
                    span.col,
                    msg,
                ));
            }
        }
    }

    /// A1: whether a callee reaches a backend as a native rather than as
    /// compiled ManiT.
    ///
    /// The test is "qualified by a standard library module, and has no body in
    /// this program". The second half is what keeps the source modules —
    /// bridge, crypto, t27f, and anything `stdlib_expand` merges — out of the
    /// backlog: they are ManiT, they are compiled, and declaring them `extern`
    /// would be a lie.
    pub(crate) fn is_native(&self, name: &str) -> bool {
        let Some((prefix, _)) = name.split_once("::") else {
            return false;
        };
        Self::STDLIB_MODULES.contains(&prefix) && !self.bodied_fns.contains(name)
    }

    /// A1: record that a native was called with no `extern` declaration.
    ///
    /// Deduplicated by name — the backlog is a list of natives to declare, not
    /// a list of call sites, and one entry per call would make a program that
    /// prints in a loop dominate its own migration report.
    pub(crate) fn note_undeclared_native(&mut self, name: &str, span: crate::ast::Span) {
        if self.externs.contains_key(name) {
            return;
        }
        if !self.undeclared_natives.iter().any(|n| n == name) {
            self.undeclared_natives.push(name.to_string());
        }
        self.warnings.push(CompileWarning::new(
            WarningKind::UndeclaredNative,
            &self.file,
            span.line,
            span.col,
            format!(
                "native '{}' is called with no `extern` declaration; its signature \
                 is inferred, so its arguments are not checked",
                name
            ),
        ));
    }

    /// A1: diagnostics that depend on a native's declaration, at the call site.
    pub(crate) fn check_extern_call_site(&mut self, name: &str, span: crate::ast::Span) {
        let Some(sig) = self.externs.get(name) else {
            return;
        };
        if let Some(msg) = sig.deprecated.clone() {
            self.warnings.push(CompileWarning::new(
                WarningKind::DeprecatedNative,
                &self.file,
                span.line,
                span.col,
                format!("'{}' is deprecated: {}", name, msg),
            ));
        }
        // A1 step 3 is not enforced here — the level defaults to `allow` — but
        // the diagnostic is produced so that `--warn backend-unavailable`
        // reports the step-3 backlog the same way `--warn undeclared-native`
        // reports the step-1 one.
        if let Some(avail) = sig.available.clone() {
            let target = self.backend.clone();
            if let Some(target) = target {
                if !avail.contains(&target) {
                    self.warnings.push(CompileWarning::new(
                        WarningKind::BackendUnavailable,
                        &self.file,
                        span.line,
                        span.col,
                        format!(
                            "'{}' is not available on the {} backend (declared \
                             available on: {})",
                            name,
                            target,
                            if avail.is_empty() { "no backend".to_string() } else { avail.join(", ") }
                        ),
                    ));
                }
            }
        }
    }

    fn register_native_module_sigs(&mut self) {
        // Modules whose RETURN types are registered in `self.functions`.
        // Narrower than the parameter scan below, deliberately: adding a return
        // type changes what the analyzer infers at every call site, whereas the
        // parameter map only reaches codegen.
        const SIG_MODULES: &[&str] = &["str", "fmt", "math", "ternary"];

        for (module, src) in STDLIB_SOURCES {
            let file = format!("<std::{}>", module);
            let Ok(tokens) = crate::lexer::Lexer::with_file(src, &file).tokenize() else {
                continue;
            };
            let Ok(program) = crate::parser::Parser::with_file(tokens, &file).parse() else {
                continue;
            };
            let register_ret = SIG_MODULES.contains(module);
            for item in &program.items {
                let Item::FnDef(f) = item else { continue };
                let qualified = format!("{}::{}", module, f.name);

                // Declared parameter types, for lowering. Recorded whether or
                // not the return type is registered, and whether or not the
                // builtin table already states a signature: the source is the
                // more precise authority on parameters, and this map is read
                // only by the lowerer's coercion.
                let mut ptys = Vec::with_capacity(f.params.len());
                let mut all_resolved = true;
                for p in &f.params {
                    match self.resolve_type(&p.ty) {
                        Ok(ty) => ptys.push(ty),
                        Err(_) => {
                            all_resolved = false;
                            break;
                        }
                    }
                }
                if all_resolved {
                    self.native_param_manitys.insert(qualified.clone(), ptys);
                }

                if !register_ret {
                    continue;
                }
                // Never shadow a signature the table states explicitly, and
                // never a real user function of the same name.
                if self.functions.contains_key(&qualified) {
                    continue;
                }
                let ret = match &f.ret_ty {
                    Some(t) => match self.resolve_type(t) {
                        Ok(ty) => ty,
                        Err(_) => continue,
                    },
                    None => ManiType::Void,
                };
                let params = vec![ManiType::Unknown; f.params.len()];
                self.functions.insert(qualified, (params, ret));
            }
        }
    }

    pub fn analyze(&mut self, program: &Program) -> CompileResult<TypedProgram> {
        // Source-implemented stdlib modules (std::bridge / std::crypto /
        // std::t27f) have no native backend implementations; merge their
        // parsed items into the program so they compile with it.
        let expanded;
        let program = match super::stdlib_expand::expand(program)? {
            Some(p) => {
                expanded = p;
                &expanded
            }
            None => program,
        };

        // First pass: collect type definitions and function signatures
        self.collect_declarations(program)?;

        // Second pass: type-check function bodies
        let mut typed_fns = Vec::new();
        let mut typed_globals = Vec::new();
        let mut structs_out = Vec::new();
        let mut enums_out = Vec::new();

        for item in &program.items {
            match item {
                Item::FnDef(f) => {
                    typed_fns.push(self.check_fn(f)?);
                }
                Item::StructDef(s) => {
                    structs_out.push(s.clone());
                }
                Item::EnumDef(e) => {
                    enums_out.push(e.clone());
                }
                Item::ImplBlock(imp) => {
                    self.current_impl_type = Some(imp.ty.clone());
                    for method in &imp.methods {
                        // Check under the qualified name so IR sees TypeName::method
                        let mut qm = method.clone();
                        qm.name = format!("{}::{}", imp.ty, method.name);
                        typed_fns.push(self.check_fn(&qm)?);
                    }
                    // Feature 3: for methods in the trait that the impl doesn't override,
                    // include the default implementations
                    if let Some(trait_name) = &imp.trait_.clone() {
                        if let Some(trait_methods) = self.trait_defs.get(trait_name).cloned() {
                            let provided: std::collections::HashSet<String> =
                                imp.methods.iter().map(|m| m.name.clone()).collect();
                            // Look for trait default methods not overridden
                            for item in &program.items {
                                if let Item::TraitDef(tr) = item {
                                    if &tr.name == trait_name {
                                        for method in &tr.methods {
                                            if !provided.contains(&method.name) && method.body.is_some() {
                                                // Use default implementation: emit it under the qualified name
                                                let mut qm = method.clone();
                                                qm.name = format!("{}::{}", imp.ty, method.name);
                                                typed_fns.push(self.check_fn(&qm)?);
                                            }
                                        }
                                    }
                                }
                            }
                            let _ = trait_methods; // suppress unused warning
                        }
                    }
                    self.current_impl_type = None;
                }
                Item::TraitDef(tr) => {
                    for method in &tr.methods {
                        if method.body.is_some() {
                            typed_fns.push(self.check_fn(method)?);
                        }
                    }
                }
                Item::GlobalVar(gv) => {
                    let ty = self.resolve_type(&gv.ty)?;
                    let init = if let Some(e) = &gv.val {
                        let te = self.check_expr(e, Some(&ty))?;
                        // S4: global initialiser must match the declared type.
                        if ty.is_known() && te.ty.is_known() && !types_compatible(&ty, &te.ty) {
                            return Err(self.err(gv.span, format!(
                                "type mismatch: global '{}' is declared as `{}` but its initialiser has type `{}`",
                                gv.name, ty.display(), te.ty.display()
                            )));
                        }
                        // S31: a module-level `let` is one word, written by a
                        // preamble that runs before main — so its value must
                        // be computable now. It used to be lowered by a match
                        // whose only arm was a bare literal, with a wildcard
                        // that returned `IRConst::Null`: every negative
                        // constant (`-42` is `UnOp(Neg, Lit(42))`, not a
                        // literal) read as 0 on BOTH backends, and an
                        // unrepresentable initialiser such as a struct literal
                        // became a null pointer that faulted at first use.
                        // Fold it here, and say so when it will not fold.
                        if let Err(e) = crate::semantic::const_fold::fold(&te) {
                            return Err(self.err(gv.span, format!(
                                "the initialiser of global '{}' {}. A module-level `let` is \
                                 stored as a single word written before `main` runs, so its \
                                 value must be known at compile time: literals, `+ - * / %` \
                                 and comparisons on int and float, `!`, unary `-`, and \
                                 int/float casts. Anything else belongs inside a function.",
                                gv.name, e.describe(),
                            )));
                        }
                        Some(te)
                    } else {
                        None
                    };
                    typed_globals.push(TypedGlobal {
                        name: gv.name.clone(),
                        ty,
                        init,
                        is_pub: gv.is_pub,
                    });
                }
                Item::UseDecl(_) => {
                    // No type checking needed for use declarations
                }
                // Both were fully handled in collect_declarations. They carry
                // no body to check and produce no typed item.
                Item::ExternDecl(_) | Item::LintDecl(_) => {}
            }
        }

        // Append any lambdas discovered during type-checking
        typed_fns.append(&mut self.lambda_fns);

        // A2: infer backend availability over the call graph now that every
        // body has been checked and every edge recorded.
        self.infer_availability();

        Ok(TypedProgram {
            functions: typed_fns,
            structs: structs_out,
            struct_fields: self.structs.clone(),
            enums: enums_out,
            globals: typed_globals,
            native_param_manitys: self.native_param_manitys.clone(),
        })
    }

    // ---------------------------------------------------------------------------
    // A2: backend availability, inferred over the call graph
    // ---------------------------------------------------------------------------

    /// The backends this compiler knows about.
    ///
    /// Availability is represented as a SET of these rather than as a boolean
    /// per backend, so "no `available` clause was written" and "available
    /// everywhere" are the same value and need no special case downstream.
    pub(crate) const KNOWN_BACKENDS: &'static [&'static str] = &["llvm", "t3"];

    /// A2. Infer, for every function with a body, which backends it can run on,
    /// and report any function that cannot run on the one being compiled for.
    ///
    /// A maniT function is available on backend B exactly when every function
    /// it calls is. That makes availability a backwards dataflow over the call
    /// graph, and the lattice is finite and tiny — subsets of two backends —
    /// so a fixpoint iteration converges immediately and handles recursion
    /// without a separate SCC pass: mutually recursive functions simply settle
    /// at the meet over their cycle, which is what the specification asks for.
    ///
    /// Only externs with an explicit `available(...)` clause constrain
    /// anything. Everything else starts unconstrained, which keeps this from
    /// becoming a second undeclared-native backlog: A2 reports contradictions
    /// between what someone WROTE and what the program actually calls, not the
    /// absence of annotations.
    fn infer_availability(&mut self) {
        use std::collections::HashSet;

        let all: HashSet<String> =
            Self::KNOWN_BACKENDS.iter().map(|s| s.to_string()).collect();

        // Seed. An extern with a written clause is the only source of a
        // constraint; every other name is unconstrained until propagation
        // narrows it.
        let mut avail: HashMap<String, HashSet<String>> = HashMap::new();
        for (name, sig) in &self.externs {
            if let Some(list) = &sig.available {
                avail.insert(name.clone(), list.iter().cloned().collect());
            }
        }
        // A written clause on an ordinary function seeds the lattice too, so
        // its callers inherit the restriction. Propagation can only NARROW it
        // from here, and a narrowing is exactly what makes the assertion false
        // — which is what the check after the fixpoint looks for.
        for (name, (list, _)) in &self.declared_fn_avail {
            avail.insert(name.clone(), list.iter().cloned().collect());
        }

        // Propagate. Iterate to a fixpoint; the lattice height is the number
        // of backends, so this terminates in a handful of rounds even for a
        // deeply recursive graph.
        let callers: Vec<String> = self.call_graph.keys().cloned().collect();
        loop {
            let mut changed = false;
            for caller in &callers {
                let mut acc = avail.get(caller).cloned().unwrap_or_else(|| all.clone());
                if let Some(edges) = self.call_graph.get(caller) {
                    for (callee, _) in edges {
                        if let Some(callee_av) = avail.get(callee) {
                            acc = acc.intersection(callee_av).cloned().collect();
                        }
                    }
                }
                let prev = avail.get(caller);
                if prev.map(|p| p != &acc).unwrap_or(acc.len() < all.len()) {
                    avail.insert(caller.clone(), acc);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        // A2, the assertion half. A function that WROTE `available(B)` and
        // whose body reaches something unavailable on B has made a claim the
        // call graph contradicts. This is checked whatever backend is being
        // compiled for, and even when none is — it is a statement about the
        // program, not about this invocation, so `manitc check` reports it.
        let mut asserted: Vec<(&String, &(Vec<String>, crate::ast::Span))> =
            self.declared_fn_avail.iter().collect();
        asserted.sort_by(|a, b| a.0.cmp(b.0));
        let mut assertion_failures = Vec::new();
        for (name, (claimed, span)) in asserted {
            let Some(actual) = avail.get(name) else { continue };
            for b in claimed {
                if actual.contains(b) {
                    continue;
                }
                let chain = self
                    .availability_witness(name, b, &avail)
                    .map(|(c, _)| c)
                    .unwrap_or_else(|| name.clone());
                assertion_failures.push(CompileWarning::new(
                    WarningKind::BackendUnavailableChain,
                    &self.file,
                    span.line,
                    span.col,
                    format!(
                        "'{}' declares `available({})` but cannot run there: {}",
                        name, b, chain
                    ),
                ));
            }
        }
        let had_assertion_failure = !assertion_failures.is_empty();
        for w in assertion_failures {
            self.warnings.push(w);
        }

        // Report. Backend-agnostic invocations (`manitc check`) ask no
        // availability question, so they get no availability answer — the same
        // rule A1 step 3 already follows.
        let Some(target) = self.backend.clone() else {
            return;
        };
        // A broken assertion already names the same chain; adding the inferred
        // diagnostic on top would report one mistake twice.
        if had_assertion_failure {
            return;
        }

        // Sorted so the output is deterministic: the call graph is a HashMap
        // and its iteration order is not.
        let mut offenders: Vec<&String> = avail
            .keys()
            .filter(|f| self.bodied_fns.contains(*f) || self.call_graph.contains_key(*f))
            .filter(|f| !avail[*f].contains(&target))
            .filter(|f| !self.externs.contains_key(*f))
            .collect();
        offenders.sort();
        let offenders: Vec<String> = offenders.into_iter().cloned().collect();

        // Report only the OUTERMOST offenders — those no other offender calls.
        //
        // A single unavailable extern makes every function above it in the
        // graph unavailable too, so reporting all of them means N copies of one
        // fact: a three-deep chain produced three errors that differed only in
        // where they started. The outermost one's chain already names every hop
        // including the culprit, and the inner functions are consequences of it
        // rather than separate problems.
        //
        // Reporting the INNERMOST instead would have been the other obvious
        // choice and is wrong here: the direct call to an unavailable extern is
        // already A1 step 3's diagnostic (`backend-unavailable`, at the call
        // site). What A2 knows that A1 cannot is the transitive part.
        let called_by_offender: std::collections::HashSet<&String> = offenders
            .iter()
            .filter_map(|f| self.call_graph.get(f))
            .flatten()
            .map(|(callee, _)| callee)
            .collect();
        let mut outermost: Vec<String> = offenders
            .iter()
            .filter(|f| !called_by_offender.contains(*f))
            .cloned()
            .collect();
        // A cycle of unavailable functions with no caller outside it would
        // leave nothing outermost, because every member is called by another
        // member. Fall back to the first offender so a mutually recursive group
        // still reports rather than passing silently.
        if outermost.is_empty() {
            outermost = offenders.iter().take(1).cloned().collect();
        }
        let offenders = outermost;

        for f in offenders {
            // Externs are already reported at their call sites by A1 step 3;
            // repeating them here would double-report the same fact.
            if self.externs.contains_key(&f) {
                continue;
            }
            let Some((chain, span)) = self.availability_witness(&f, &target, &avail) else {
                continue;
            };
            self.warnings.push(CompileWarning::new(
                WarningKind::BackendUnavailableChain,
                &self.file,
                span.line,
                span.col,
                format!(
                    "'{}' cannot be compiled for the {} backend: {}",
                    f, target, chain
                ),
            ));
        }
    }

    /// Find a shortest call chain from `from` to something not available on
    /// `target`, and render it.
    ///
    /// Breadth-first, so the chain reported is the shortest one rather than
    /// whichever the recursion happened to reach first — a 12-deep path to the
    /// same extern explains less than a 2-deep one. The span returned is the
    /// FIRST call site in the chain, because that is the line the programmer
    /// has to change.
    fn availability_witness(
        &self,
        from: &str,
        target: &str,
        avail: &HashMap<String, std::collections::HashSet<String>>,
    ) -> Option<(String, crate::ast::Span)> {
        use std::collections::{HashSet, VecDeque};
        let mut seen: HashSet<String> = HashSet::new();
        let mut queue: VecDeque<(String, Vec<String>, Option<crate::ast::Span>)> = VecDeque::new();
        seen.insert(from.to_string());
        queue.push_back((from.to_string(), vec![from.to_string()], None));

        while let Some((node, path, first_span)) = queue.pop_front() {
            // A leaf that is itself unavailable ends the chain — but only if
            // its unavailability was DECLARED rather than inferred.
            //
            // The distinction is the whole point of the chain: an inferred
            // unavailability is a consequence of something further down, so
            // stopping there would report "f is unavailable because g is"
            // without ever naming the reason. A declared one is the reason.
            //
            // Both kinds of declaration count. Checking only `externs` here
            // meant a written `available(...)` clause on an ordinary function
            // constrained nothing: its callers came out unavailable in the
            // lattice, the search for a witness then ran off the end of the
            // graph and found none, and the diagnostic was silently dropped.
            let declared_here = self
                .externs
                .get(&node)
                .and_then(|s| s.available.clone())
                .or_else(|| self.declared_fn_avail.get(&node).map(|(l, _)| l.clone()));
            if node != from
                && avail.get(&node).map(|a| !a.contains(target)).unwrap_or(false)
                && declared_here.is_some()
            {
                let declared = declared_here.unwrap_or_default();
                let where_ = if declared.is_empty() {
                    "no backend".to_string()
                } else {
                    declared.join(", ")
                };
                return Some((
                    format!(
                        "{} — and '{}' is declared available only on: {}",
                        path.join(" -> "),
                        node,
                        where_
                    ),
                    first_span.unwrap_or_else(|| crate::ast::Span { line: 1, col: 1 }),
                ));
            }
            let Some(edges) = self.call_graph.get(&node) else {
                continue;
            };
            for (callee, span) in edges {
                // Only follow edges that are themselves unavailable — any other
                // branch cannot lead to the reason this function is unavailable.
                if avail.get(callee).map(|a| a.contains(target)).unwrap_or(true) {
                    continue;
                }
                if !seen.insert(callee.clone()) {
                    continue;
                }
                let mut p = path.clone();
                p.push(callee.clone());
                queue.push_back((callee.clone(), p, first_span.or(Some(*span))));
            }
        }
        None
    }

    fn collect_declarations(&mut self, program: &Program) -> CompileResult<()> {
        // Pass 1: collect structs, enums, functions, traits, globals, use-decls.
        // ImplBlocks are deferred to pass 2 so that trait validation always sees
        // fully populated trait_defs regardless of source order.
        // A5: module lint levels are applied BEFORE anything else is collected,
        // so a `lint allow(...)` at the top of a file governs the diagnostics
        // this very pass produces. Applying them later would make the level
        // depend on where in the file the item sits, which is exactly the kind
        // of order-dependence a recorded manifest is supposed to rule out.
        for item in &program.items {
            if let Item::LintDecl(l) = item {
                self.apply_lint_decl(l)?;
            }
        }

        // A1: extern declarations, before functions, so a call anywhere in the
        // program sees the declaration regardless of source order.
        for item in &program.items {
            if let Item::ExternDecl(e) = item {
                self.collect_extern_decl(e)?;
            }
        }

        // Everything that has a body, so `is_native` can tell a stdlib module
        // written in ManiT from one the backend has to supply.
        for item in &program.items {
            match item {
                Item::FnDef(f) if f.body.is_some() => {
                    self.bodied_fns.insert(f.name.clone());
                    if !f.bounds.is_empty() {
                        self.fn_generic_sigs.insert(
                            f.name.clone(),
                            GenericSig {
                                generics: f.generics.clone(),
                                bounds: f.bounds.clone(),
                                param_tys: f.params.iter().map(|p| p.ty.clone()).collect(),
                            },
                        );
                    }
                }
                Item::ImplBlock(imp) => {
                    for m in &imp.methods {
                        if m.body.is_some() {
                            self.bodied_fns.insert(format!("{}::{}", imp.ty, m.name));
                        }
                    }
                }
                _ => {}
            }
        }

        for item in &program.items {
            match item {
                Item::StructDef(s) => {
                    let mut fields = Vec::new();
                    let mut pub_fields = Vec::new();
                    for f in &s.fields {
                        let ty = self.resolve_type(&f.ty)?;
                        fields.push((f.name.clone(), ty));
                        pub_fields.push((f.name.clone(), f.is_pub));
                    }
                    self.structs.insert(s.name.clone(), fields);
                    self.struct_pub_fields.insert(s.name.clone(), pub_fields);
                }
                Item::EnumDef(e) => {
                    let mut variants = Vec::new();
                    for v in &e.variants {
                        let field_tys: CompileResult<Vec<ManiType>> =
                            v.fields.iter().map(|f| self.resolve_type(f)).collect();
                        variants.push((v.name.clone(), field_tys?));
                    }
                    self.enums.insert(e.name.clone(), variants);
                }
                Item::FnDef(f) => {
                    self.register_fn(f)?;
                }
                Item::TraitDef(tr) => {
                    // Store trait signatures for later validation of impl-for-trait blocks
                    let mut methods = Vec::new();
                    for method in &tr.methods {
                        let saved = self.type_params.clone();
                        for gp in &method.generics {
                            self.type_params.insert(gp.clone(), ManiType::Unknown);
                        }
                        let param_tys: Vec<ManiType> = method.params.iter()
                            .map(|p| self.resolve_type(&p.ty).unwrap_or(ManiType::Unknown))
                            .collect();
                        let ret_ty = if let Some(rt) = &method.ret_ty {
                            self.resolve_type(rt).unwrap_or(ManiType::Unknown)
                        } else {
                            ManiType::Void
                        };
                        let has_default = method.body.is_some();
                        self.type_params = saved;
                        methods.push((method.name.clone(), param_tys, ret_ty, has_default));
                        // Register default method implementations if they have a body
                        if method.body.is_some() {
                            self.register_fn(method)?;
                        }
                    }
                    self.trait_defs.insert(tr.name.clone(), methods);
                }
                Item::GlobalVar(gv) => {
                    let ty = self.resolve_type(&gv.ty)?;
                    // The parser does not record `mut` for globals, so they are
                    // conservatively treated as mutable.
                    self.symbols.define(&gv.name, ty, true);
                }
                Item::UseDecl(u) => {
                    self.resolve_use(u)?;
                }
                Item::ImplBlock(_) => {} // deferred to pass 2
                // Collected in the dedicated passes above this match, which run
                // first so that ordering cannot affect either one.
                Item::ExternDecl(_) | Item::LintDecl(_) => {}
            }
        }

        // Pass 2: process impl blocks — all traits are now registered.
        for item in &program.items {
            if let Item::ImplBlock(imp) = item {
                self.current_impl_type = Some(imp.ty.clone());
                for method in &imp.methods {
                    // Register under the qualified name TypeName::method_name
                    let mut qm = method.clone();
                    qm.name = format!("{}::{}", imp.ty, method.name);
                    self.register_fn(&qm)?;
                    // Track the return type for method-call resolution
                    let ret_ty = if let Some(rt) = &method.ret_ty {
                        self.resolve_type(rt).unwrap_or(ManiType::Unknown)
                    } else {
                        ManiType::Void
                    };
                    self.user_method_types
                        .entry(imp.ty.clone())
                        .or_default()
                        .insert(method.name.clone(), ret_ty);
                    // Record the explicit-argument count only for real methods
                    // — those whose first parameter is the `self` receiver.
                    // Associated functions (`Point::new(x, y)`) are called
                    // through the path form, which `functions` already checks.
                    if method.params.first().is_some_and(|p| p.name == "self") {
                        self.user_method_arity
                            .entry(imp.ty.clone())
                            .or_default()
                            .insert(method.name.clone(), method.params.len() - 1);
                    }
                }
                // Validate trait implementation: every required method must be present
                if let Some(trait_name) = &imp.trait_ {
                    if let Some(required) = self.trait_defs.get(trait_name).cloned() {
                        let provided: std::collections::HashSet<String> =
                            imp.methods.iter().map(|m| m.name.clone()).collect();
                        for (method_name, _, _, has_default) in &required {
                            if !provided.contains(method_name) && !has_default {
                                return Err(self.err(
                                    imp.span,
                                    format!(
                                        "type '{}' implements trait '{}' but is missing required method '{}'",
                                        imp.ty, trait_name, method_name
                                    ),
                                ));
                            }
                        }
                    } else {
                        return Err(self.err(
                            imp.span,
                            format!("unknown trait '{}'", trait_name),
                        ));
                    }
                    self.trait_impls.insert((imp.ty.clone(), trait_name.clone()));
                }
                self.current_impl_type = None;
            }
        }
        Ok(())
    }

    // ---------------------------------------------------------------------------
    // Module system: use-declaration resolution
    // ---------------------------------------------------------------------------

    /// Known standard library module names (matching stdlib/ directory)
    ///
    /// THIS IS THE THIRD OF THREE LISTS a new stdlib module must be added to,
    /// and it is the one that fails loudest when forgotten -- `use std::test;`
    /// reports "unknown standard library module" even though the module file
    /// exists and is embedded. The other two are STDLIB_SOURCES above (for
    /// `::`-path diagnostics) and, for modules implemented in ManiT rather than
    /// natively, SOURCE_MODULES in semantic/stdlib_expand.rs (which compiles
    /// the bodies into the program). Miss that last one and the module resolves,
    /// type-checks, and then fails at link or assembly with an undefined symbol.
    pub(crate) const STDLIB_MODULES: &'static [&'static str] = &[
        "io", "math", "ternary", "collections", "fmt", "str",
        "sync", "async", "env", "time", "fs", "net",
        "t27f", "crypto", "bridge", "tritfs", "test", "trit",
    ];

    fn resolve_use(&mut self, decl: &UseDecl) -> CompileResult<()> {
        if decl.path.is_empty() {
            return Ok(());
        }

        if decl.path[0] == "std" {
            // Standard library — validate the module name exists.
            // All stdlib functions are already registered as builtins.
            if decl.path.len() >= 2 {
                let mod_name = &decl.path[1];
                if !Self::STDLIB_MODULES.contains(&mod_name.as_str()) {
                    return Err(self.err(
                        decl.span,
                        format!("unknown standard library module: std::{}", mod_name),
                    ));
                }
            }
            Ok(())
        } else {
            // User module — resolve to filesystem
            self.load_user_module(decl)
        }
    }

    fn load_user_module(&mut self, decl: &UseDecl) -> CompileResult<()> {
        // Build path: foo::bar → source_dir/foo/bar.mt
        let mut module_path = self.source_dir.clone();
        for part in &decl.path {
            module_path = module_path.join(part);
        }
        module_path.set_extension("mt");

        // Check if already loaded (circular import guard)
        let canonical = module_path.canonicalize().unwrap_or(module_path.clone());
        if self.loaded_modules.contains(&canonical) {
            return Ok(()); // Already loaded
        }
        self.loaded_modules.insert(canonical.clone());

        // Read and parse the module file
        let source = std::fs::read_to_string(&module_path).map_err(|e| {
            self.err(
                decl.span,
                format!(
                    "cannot load module '{}': {} (looked in {})",
                    decl.path.join("::"),
                    e,
                    module_path.display()
                ),
            )
        })?;

        let file_str = module_path.to_string_lossy().to_string();
        let mut lexer = crate::lexer::Lexer::with_file(&source, &file_str);
        let tokens = lexer.tokenize()?;

        let mut parser = crate::parser::Parser::with_file(tokens, &file_str);
        let program = parser.parse()?;

        // Build module prefix from path: foo::bar → "foo::bar"
        let prefix = decl.path.join("::");
        self.loaded_module_prefixes.insert(prefix.clone());

        // Register all PUBLIC definitions from the module with prefixed names.
        // Non-`pub` items are recorded so that referencing them produces a
        // privacy error instead of a generic unknown-path diagnostic (S11).
        for item in &program.items {
            match item {
                Item::FnDef(f) => {
                    let full_name = format!("{}::{}", prefix, f.name);
                    if !f.is_pub {
                        self.module_private_items.insert(full_name, prefix.clone());
                        continue;
                    }
                    let param_types: Vec<ManiType> = f
                        .params
                        .iter()
                        .map(|p| self.resolve_type(&p.ty).unwrap_or(ManiType::Unknown))
                        .collect();
                    let ret_ty = f
                        .ret_ty
                        .as_ref()
                        .map(|t| self.resolve_type(t).unwrap_or(ManiType::Unknown))
                        .unwrap_or(ManiType::Void);
                    self.functions.insert(full_name, (param_types, ret_ty));
                }
                Item::StructDef(s) => {
                    let full_name = format!("{}::{}", prefix, s.name);
                    if !s.is_pub {
                        self.module_private_items.insert(full_name, prefix.clone());
                        continue;
                    }
                    let fields: Vec<(String, ManiType)> = s
                        .fields
                        .iter()
                        .map(|f| {
                            (
                                f.name.clone(),
                                self.resolve_type(&f.ty).unwrap_or(ManiType::Unknown),
                            )
                        })
                        .collect();
                    self.structs.insert(full_name, fields);
                }
                Item::EnumDef(e) => {
                    let full_name = format!("{}::{}", prefix, e.name);
                    if !e.is_pub {
                        self.module_private_items.insert(full_name, prefix.clone());
                        continue;
                    }
                    let variants: Vec<(String, Vec<ManiType>)> = e
                        .variants
                        .iter()
                        .map(|v| {
                            let types: Vec<ManiType> = v
                                .fields
                                .iter()
                                .map(|t| self.resolve_type(t).unwrap_or(ManiType::Unknown))
                                .collect();
                            (v.name.clone(), types)
                        })
                        .collect();
                    self.enums.insert(full_name, variants);
                }
                Item::UseDecl(u) => {
                    // S11: transitive imports — modules used by the loaded
                    // module are loaded too (the loaded_modules set guards
                    // against cycles).
                    self.resolve_use(u)?;
                }
                _ => {} // Ignore impl blocks, globals, etc. in imported modules
            }
        }

        Ok(())
    }

    fn register_fn(&mut self, f: &FnDef) -> CompileResult<()> {
        // Reject duplicate user-defined function names (builtins may be shadowed)
        if self.functions.contains_key(&f.name) && !self.builtin_names.contains(&f.name) {
            return Err(self.err(f.span, format!("duplicate function definition `{}`", f.name)));
        }
        // Push generic type params as Unknown so they resolve correctly
        let saved_type_params = self.type_params.clone();
        for gp in &f.generics {
            self.type_params.insert(gp.clone(), ManiType::Unknown);
        }
        let param_tys: CompileResult<Vec<ManiType>> =
            f.params.iter().map(|p| self.resolve_type(&p.ty)).collect();
        let param_tys = param_tys?;
        let ret_ty = if let Some(rt) = &f.ret_ty {
            self.resolve_type(rt)?
        } else {
            ManiType::Void
        };
        self.functions.insert(f.name.clone(), (param_tys, ret_ty));
        self.type_params = saved_type_params;
        Ok(())
    }

    // ---------------------------------------------------------------------------
    // Function checking
    // ---------------------------------------------------------------------------

    fn check_fn(&mut self, f: &FnDef) -> CompileResult<TypedFnDef> {
        // Push generic type params as Unknown for this function's scope
        let saved_type_params = std::mem::take(&mut self.type_params);
        for gp in &f.generics {
            self.type_params.insert(gp.clone(), ManiType::Unknown);
        }

        let ret_ty = if let Some(rt) = &f.ret_ty {
            self.resolve_type(rt)?
        } else {
            ManiType::Void
        };
        let old_ret = self.current_fn_ret.clone();
        self.current_fn_ret = ret_ty.clone();
        // A2: attribute every call in this body to this function.
        let old_fn = self.current_fn.replace(f.name.clone());
        if let Some(list) = &f.available {
            self.declared_fn_avail
                .insert(f.name.clone(), (list.clone(), f.span));
        }

        self.symbols.push_scope();
        let mut typed_params = Vec::new();
        for p in &f.params {
            let ty = self.resolve_type(&p.ty)?;
            self.symbols.define(&p.name, ty.clone(), false);
            typed_params.push(TypedParam { name: p.name.clone(), ty });
        }

        // A1: a non-void function must supply a value on every path. Falling
        // off the end left the return slot uninitialised — harmless-looking
        // for `-> int` (a silent 0) but an uninitialised pointer for `-> str`,
        // which the print path then dereferences.
        if let Some(block) = &f.body {
            if !matches!(ret_ty, ManiType::Void)
                && !crate::semantic::diverges::block_diverges(block)
                && !crate::semantic::diverges::block_has_value_tail(block)
            {
                return Err(self.err(
                    f.span,
                    format!(
                        "function `{}` declares a return type of `{}` but can \
                         finish without returning a value; add a `return` (or a \
                         tail expression) on every path",
                        f.name, ret_ty.display(),
                    ),
                ));
            }
        }

        // A16: report bindings that are never read, and `mut` bindings that
        // are never assigned. Warnings only — this catches discarded results
        // (`let allowed = enforce(cap, ..);` with no use) without breaking any
        // existing source.
        for u in crate::semantic::unused::check_fn(f) {
            let msg = match u.kind {
                crate::semantic::unused::UnusedKind::Variable =>
                    format!("unused variable `{}`; prefix with `_` if intentional", u.name),
                crate::semantic::unused::UnusedKind::Mutability =>
                    format!("variable `{}` does not need to be mutable", u.name),
            };
            self.warnings.push(CompileWarning::new(
                WarningKind::UnusedVariable,
                &self.file, u.span.line, u.span.col, msg,
            ));
        }

        let body = if let Some(block) = &f.body {
            Some(self.check_block(block)?)
        } else {
            None
        };
        self.symbols.pop_scope();
        self.current_fn_ret = old_ret;
        self.current_fn = old_fn;
        self.type_params = saved_type_params;

        Ok(TypedFnDef {
            name: f.name.clone(),
            params: typed_params,
            ret_ty,
            body,
            is_pub: f.is_pub,
            is_async: f.is_async,
        })
    }

    // ---------------------------------------------------------------------------
    // Block and statement checking
    // ---------------------------------------------------------------------------

}

mod stmts;
mod type_inference;

#[cfg(test)]
mod member_list_tests {
    use super::*;

    /// Every builtin the analyzer registers under `mod::item` must also appear
    /// in that module's member list.
    ///
    /// This is what lets `std module 'io' has no item 'print_bool'` be a hard
    /// error rather than a warning.  The two facts are built by different
    /// means — `register_builtins` is a hand-written table, the member list is
    /// scanned out of the stdlib sources plus `STDLIB_EXTRA_MEMBERS` — so
    /// nothing but this test compares them.  A builtin missing from the member
    /// list would make a CORRECT program fail to compile, which is a far worse
    /// failure than the one the hard error fixes.  Add the name to
    /// `STDLIB_EXTRA_MEMBERS` when this fires.
    #[test]
    fn every_registered_builtin_is_in_its_module_member_list() {
        let analyzer = SemanticAnalyzer::new();
        let mut missing: Vec<String> = Vec::new();

        for name in &analyzer.builtin_names {
            let Some(pos) = name.rfind("::") else { continue };
            let module = &name[..pos];
            let item = &name[pos + 2..];
            // Only std modules gate on the member list; `Vec::push` and friends
            // are builtin namespaces the check deliberately does not enumerate.
            if !SemanticAnalyzer::STDLIB_MODULES.contains(&module) {
                continue;
            }
            match std_module_members(module) {
                Some(members) if members.contains(item) => {}
                _ => missing.push(name.clone()),
            }
        }

        missing.sort();
        assert!(
            missing.is_empty(),
            "these builtins are registered but invisible to the member list, so \
             calling them would be rejected as unknown: {:#?}",
            missing
        );
    }

    /// The mirror invariant: an entry in `STDLIB_EXTRA_MEMBERS` must name
    /// something that actually exists.
    ///
    /// An extras entry SUPPRESSES the unknown-item diagnostic, so a name that
    /// is merely guessed vouches for a function nobody wrote — the call sails
    /// through the analyzer and dies at link time against a symbol the user
    /// never typed.  `async` carried two such phantoms, `yield_` and `spawn`,
    /// which existed only because `scan_module_members` could not see
    /// `async fn`; `ternary::from_balanced_ternary` was a third, and it
    /// segfaulted rather than merely failing to link.
    ///
    /// Extras name intrinsics the BACKENDS implement, and the backends keep
    /// those names in `match` arms rather than in any table a test could
    /// import — so this reads the backend sources as text.  That is the whole
    /// point: this compiler's recurring bug is several sources of truth with
    /// nothing comparing them, and text is what these ones have in common.
    #[test]
    fn no_stdlib_extra_member_is_a_phantom() {
        // The backends that give an intrinsic its meaning.
        const BACKEND_SOURCES: &[&str] = &[
            include_str!("../../codegen_t3/emitter/emit_instr.rs"),
            include_str!("../../codegen_llvm/helpers.rs"),
            include_str!("../../ir/lower/lower_expr.rs"),
        ];

        let analyzer = SemanticAnalyzer::new();
        let mut phantoms: Vec<String> = Vec::new();

        for (module, extras) in STDLIB_EXTRA_MEMBERS {
            for item in *extras {
                let qualified = format!("{}::{}", module, item);
                let in_source = STDLIB_SOURCES
                    .iter()
                    .find(|(m, _)| m == module)
                    .map(|(_, src)| scan_module_members(src).contains(*item))
                    .unwrap_or(false);
                // Quoted, so `io::print_int` never matches `io::print_integer`.
                let quoted = format!("\"{}\"", qualified);
                let in_backend = BACKEND_SOURCES.iter().any(|s| s.contains(&quoted));
                if !analyzer.builtin_names.contains(&qualified) && !in_source && !in_backend {
                    phantoms.push(qualified);
                }
            }
        }

        phantoms.sort();
        assert!(
            phantoms.is_empty(),
            "these names are listed as stdlib members but are neither \
             registered builtins nor declared in the module source, so they \
             silence the unknown-item error for a function that does not \
             exist: {:#?}",
            phantoms
        );
    }
}
