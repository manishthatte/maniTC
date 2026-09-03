//! Simplified borrow / move checker for the ManiT compiler.
//!
//! This pass runs on the **TypedProgram** (after semantic analysis, before IR
//! lowering) and catches three categories of mistakes:
//!
//! 1. **Use-after-move** -- reading a variable that was already moved.
//! 2. **Double-move** -- moving a variable that was already moved.
//! 3. **Move-in-loop** -- moving a non-Copy variable inside a loop body where
//!    it would be consumed on every iteration.
//!
//! We intentionally do NOT implement lifetime annotations, reference borrowing,
//! reborrowing, or NLL. This is a lightweight safety net, not a full Rust-style
//! borrow checker.
//!
//! Scoping: the checker tracks declaration scopes so that the moved-set is
//! keyed by *binding* — (scope depth, name) — not by bare name. This makes
//! shadowing work in both directions (moving an inner shadow does not poison
//! the outer binding, and an inner `let` does not launder an outer move), and
//! lets the move-in-loop check ignore variables that are declared inside the
//! loop body (they are fresh on every iteration).

use std::collections::{HashMap, HashSet};

use crate::ast::{LetPat, Pattern};
use crate::error::{CompileError, CompileResult};
use crate::semantic::types::*;

// ---------------------------------------------------------------------------
// Move rules -- B7's D-2 and D-3, made switchable so they can be MEASURED
// ---------------------------------------------------------------------------

/// Which consuming rules are in force.
///
/// `CURRENT` is what the compiler does today. The other combinations exist so
/// that the blast radius of changing a rule can be measured over the corpus
/// **before** the rule is changed -- `enhance/phase5-type-system-second-half/
/// B7_AFFINE_TYPES.md` §4 asks for exactly this, and P103's field-miss
/// instrument is the precedent: build the sweep, take the number, then decide.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MoveRules {
    /// **D-2** -- does passing a value to a function consume it? Today: no,
    /// which is why `fn consume(x: T)` is inexpressible in this language.
    pub call_args_move: bool,
    /// **D-3, TAKEN 2 September 2026** -- an array literal in BINDING position
    /// consumes its elements, like a tuple and a struct literal always have.
    /// In ARGUMENT position it does not, because there it is this language's
    /// varargs list rather than a container; see the `Array` arm.
    ///
    /// Retained as a switch so the pre-D-3 compiler stays measurable from one
    /// binary: `false` is exactly what shipped before.
    pub array_elems_move: bool,
    /// Apply the candidate rules only at spans the USER wrote, told apart by
    /// P80's `Span::module`.
    ///
    /// This is not a nicety. `stdlib_expand` APPENDS the merged stdlib to
    /// every program before this pass runs, so a rule applied everywhere makes
    /// every file's verdict the STANDARD LIBRARY's verdict -- measured, a
    /// seven-line program reports 163 call-argument sites, 162 of them the
    /// stdlib's. Both scopes are real questions and they are different ones:
    /// "can the language adopt this rule" and "would this program survive it".
    pub user_code_only: bool,
}

impl MoveRules {
    /// Exactly what the compiler does today. Every shipped path uses this.
    pub const CURRENT: MoveRules = MoveRules {
        call_args_move: false,
        array_elems_move: true,
        user_code_only: false,
    };

    /// What shipped before D-3 was taken, kept so the change stays measurable
    /// from one binary rather than needing a second.
    pub const PRE_D3: MoveRules = MoveRules {
        call_args_move: false,
        array_elems_move: false,
        user_code_only: false,
    };
}

/// Population counts for the candidate rules: how many sites of each shape a
/// program actually contains. A site is a plain move-type variable in the
/// position named -- the only shape `consume_if_move` can bite on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MoveSites {
    /// A call ARGUMENT. D-2's population. `.0` = user code, `.1` = stdlib.
    pub call_arg: (usize, usize),
    /// A method call's RECEIVER. Counted and never consumed: whether `self`
    /// is taken by value is a decision D-2 does not take, and the count is
    /// here so that decision starts from a number rather than a guess.
    pub method_recv: (usize, usize),
    /// An ARRAY literal element. D-3's population.
    pub array_elem: (usize, usize),
    /// Of those, the ones where the array literal is ITSELF a call argument
    /// -- i.e. the array is a varargs list, not a container. `.0` = user.
    ///
    /// This is the number that decides D-3, and the design document did not
    /// anticipate it: `fmt::format("{}", [s])` puts `s` in an array literal
    /// only because that is how this language spells varargs, so consuming
    /// there would contradict D-2's "a call argument does not move" for the
    /// same `s` in the same call.
    pub array_elem_in_call: (usize, usize),
    /// A `let` binding an AGGREGATE (struct, tuple or array) from a
    /// PROJECTION -- a field read or an index -- rather than from a bare
    /// local name.
    ///
    /// Not a move site: `consume_if_move` only bites on a plain `Ident`, so
    /// the move checker never fires here. It is counted because the IR
    /// lowerer's value-semantics copy is guarded on the initialiser being a
    /// bare local, so this is exactly the shape that ALIASES instead of
    /// copying, and D-1 needs to know how much code is in it.
    pub aggregate_from_projection: (usize, usize),
}

impl MoveSites {
    fn add(&mut self, other: MoveSites) {
        self.call_arg.0 += other.call_arg.0;
        self.call_arg.1 += other.call_arg.1;
        self.method_recv.0 += other.method_recv.0;
        self.method_recv.1 += other.method_recv.1;
        self.array_elem.0 += other.array_elem.0;
        self.array_elem.1 += other.array_elem.1;
        self.array_elem_in_call.0 += other.array_elem_in_call.0;
        self.array_elem_in_call.1 += other.array_elem_in_call.1;
        self.aggregate_from_projection.0 += other.aggregate_from_projection.0;
        self.aggregate_from_projection.1 += other.aggregate_from_projection.1;
    }
}

/// Bump the user half or the stdlib half according to where the span came
/// from. `Span::module` is `None` exactly for the file the user is compiling.
fn bump(counter: &mut (usize, usize), span: crate::ast::Span) {
    if span.module.is_none() {
        counter.0 += 1;
    } else {
        counter.1 += 1;
    }
}

/// One program's answer to "what would changing a move rule cost?".
///
/// Verdicts are booleans because that is what the blast radius is: a file the
/// compiler accepts today and would refuse tomorrow. The counts are the
/// population -- sites where a rule *could* bite, whether or not it does.
#[derive(Clone, Debug)]
pub struct MoveSweep {
    pub sites: MoveSites,
    /// Does the checker accept the program today?
    pub ok_current: bool,
    /// **D-2's blast radius** -- would this program survive a call argument
    /// consuming, with the rule applied to the USER's code only? D-2 is the
    /// one candidate still open.
    pub ok_call: bool,
    /// **D-3, retrospectively** -- `false` marks a file whose verdict the
    /// container/varargs split changed.
    pub ok_array: bool,
    pub ok_both: bool,
    /// Verdicts with the rule applied EVERYWHERE, stdlib included. This asks
    /// whether the language can adopt the rule at all, and it is the same
    /// answer for every file, so one file suffices to establish it.
    pub ok_call_all: bool,
    pub ok_array_all: bool,
    /// The first diagnostic each candidate rule produces on the user's own
    /// code, so the write-up can quote a real message rather than a count.
    pub first_call_err: Option<String>,
    pub first_array_err: Option<String>,
}

impl MoveSweep {
    /// One line per file, grep-able. `sites` is measured under CURRENT rules,
    /// so a program that already FAILS the baseline check is counted only as
    /// far as its first error -- such a file is already refused and is not in
    /// any blast radius. Counts are `user/stdlib`.
    pub fn line(&self, path: &str) -> String {
        format!(
            "MOVE-SWEEP base={} call={} array={} both={} \
allcall={} allarray={} \
call_arg={}/{} method_recv={}/{} array_elem={}/{} arrcall={}/{} proj={}/{} file={}",
            ok(self.ok_current),
            ok(self.ok_call),
            ok(self.ok_array),
            ok(self.ok_both),
            ok(self.ok_call_all),
            ok(self.ok_array_all),
            self.sites.call_arg.0,
            self.sites.call_arg.1,
            self.sites.method_recv.0,
            self.sites.method_recv.1,
            self.sites.array_elem.0,
            self.sites.array_elem.1,
            self.sites.array_elem_in_call.0,
            self.sites.array_elem_in_call.1,
            self.sites.aggregate_from_projection.0,
            self.sites.aggregate_from_projection.1,
            path,
        )
    }
}

fn ok(b: bool) -> &'static str {
    if b { "ok" } else { "ERR" }
}

// ---------------------------------------------------------------------------
// Move environment: declaration scopes + moved bindings
// ---------------------------------------------------------------------------

/// Scope depth at which a loop body begins. A move of a variable declared at
/// `depth >= boundary` targets a binding that is fresh on each iteration, so
/// the move-in-loop check does not apply to it.
type LoopBoundary = Option<usize>;

#[derive(Debug)]
struct MoveEnv {
    /// Stack of declaration scopes (index = scope depth).
    scopes: Vec<HashSet<String>>,
    /// Moved bindings, keyed by (declaring scope depth, name).
    moved: HashSet<(usize, String)>,
    /// Which consuming rules are in force for this run.
    rules: MoveRules,
    /// Population counts, accumulated as the walk proceeds.
    sites: MoveSites,
    /// Instrument only: the first diagnostic, WITH its originating module.
    first_err: Option<String>,
    /// **B7's D-2**: which parameter positions of which functions CONSUME
    /// their argument, keyed by function name. Empty for a program that uses
    /// no `move` annotation, which is every program written before 3 September
    /// 2026 — so the rule's blast radius is zero by construction rather than
    /// by measurement.
    consuming: std::rc::Rc<HashMap<String, Vec<bool>>>,
    /// Set while checking an argument that IS an array literal, so that
    /// literal can tell whether it is a container or a varargs list. Cleared
    /// on descent into the literal's elements, so only the OUTERMOST array of
    /// `f([[s]])` counts as the argument list.
    array_is_vararg: bool,
    /// **F-4**: scope depth at which the innermost enclosing `region` begins,
    /// or `None` outside one. Mirrors `loop_from`, which is the same idea for
    /// loops — a binding declared at `depth >= region_from` dies when the
    /// region releases, and one below it does not.
    region_from: Option<usize>,
    /// **P118**: struct names that have at least one FIELD. A struct with none
    /// is an opaque built-in handle — `AtomicTrit`, `Barrier` and `Semaphore`
    /// are `Struct(name, [])` with no entry in `struct_fields` — and a handle
    /// is one machine word that copies correctly. A struct with fields is a
    /// heap aggregate whose fields a spawned task would reach through a
    /// pointer, which is the case neither backend gets right. Keyed on having
    /// fields rather than on a name list, because a name list is a registry
    /// that would have to agree with another registry (permanent rule 5).
    aggregates: std::rc::Rc<HashSet<String>>,
}

impl MoveEnv {
    fn new(rules: MoveRules) -> Self {
        MoveEnv {
            scopes: vec![HashSet::new()],
            moved: HashSet::new(),
            rules,
            sites: MoveSites::default(),
            first_err: None,
            array_is_vararg: false,
            consuming: Default::default(),
            aggregates: Default::default(),
            region_from: None,
        }
    }

    fn with_consuming(
        rules: MoveRules,
        consuming: std::rc::Rc<HashMap<String, Vec<bool>>>,
        aggregates: std::rc::Rc<HashSet<String>>,
    ) -> Self {
        let mut e = MoveEnv::new(rules);
        e.consuming = consuming;
        e.aggregates = aggregates;
        e
    }

    /// **F-4**: a type whose value LIVES in the region's allocator, so that a
    /// reference to it dangles once the region releases.
    ///
    /// `str`, a struct, a tuple, an array and an enum payload are storage:
    /// they are cells the compiler allocates. `Vec<T>`, `Map`, `Channel<T>`
    /// and the other `Generic` handles are NOT, and that is a fact about both
    /// backends rather than a convenience — on T3 they live in the emulator's
    /// object table, which the bump pointer does not address, and on LLVM they
    /// are the C runtime's own mallocs, which the region arena does not hold.
    /// A handle may therefore leave a region; a cell may not.
    ///
    /// The rule is deliberately about the TYPE and not about where the value
    /// was allocated: a provenance analysis would be more permissive and is
    /// the thing B7's affine types exist to make cheap. Until then this
    /// refuses some safe programs and no unsafe ones, which is the direction
    /// to be wrong in.
    fn is_region_storage(&self, ty: &ManiType) -> bool {
        matches!(
            ty,
            ManiType::Str
                | ManiType::Struct(_, _)
                | ManiType::Tuple(_)
                | ManiType::Array(_, _)
        )
    }

    /// **P118**: an aggregate whose parts a spawned task would have to reach
    /// through a pointer.
    ///
    /// Built ON `is_aggregate` rather than beside it, so that a type variant
    /// added there arrives here too — two predicates answering almost the same
    /// question are how one of them quietly stops being true. The extra clause
    /// is the only difference and it is the handle exemption: `AtomicTrit`,
    /// `Barrier` and `Semaphore` are `Struct(name, [])` with no fields, and a
    /// field-less struct is one word that copies correctly. `Channel<T>`,
    /// `Mutex<T>` and `Vec<T>` are `Generic` and never reach here at all,
    /// which matters because channels are the one thing §11.2 REQUIRES tasks
    /// to share.
    fn reaches_through_a_pointer(&self, ty: &ManiType) -> bool {
        is_aggregate(ty)
            && match ty {
                ManiType::Struct(name, _) => self.aggregates.contains(name),
                // A tuple or an array always has elements to reach.
                _ => true,
            }
    }

    /// Does `func`'s parameter `i` consume its argument?
    fn consumes(&self, func: &str, i: usize) -> bool {
        self.consuming.get(func).map_or(false, |v| *v.get(i).unwrap_or(&false))
    }

    /// Record the first diagnostic and where it came from, for the sweep.
    /// Never read by the shipped path; the returned `CompileError` is.
    fn note_err(&mut self, span: crate::ast::Span, msg: &str) {
        if self.first_err.is_none() {
            self.first_err = Some(located(span, msg));
        }
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashSet::new());
    }

    fn pop_scope(&mut self) {
        let depth = self.scopes.len() - 1;
        self.scopes.pop();
        // Bindings of the dead scope are gone; drop their moved flags so a
        // later scope at the same depth starts clean.
        self.moved.retain(|(d, _)| *d != depth);
    }

    /// Declare (or re-declare) a binding in the current scope. A fresh `let`
    /// always clears any moved flag of the same-depth binding it replaces.
    fn declare(&mut self, name: &str) {
        let depth = self.scopes.len() - 1;
        self.scopes.last_mut().expect("scope stack never empty").insert(name.to_string());
        self.moved.remove(&(depth, name.to_string()));
    }

    /// Depth of the innermost scope declaring `name`. Names never declared in
    /// this environment (globals, unresolved) act like outermost bindings.
    fn depth_of(&self, name: &str) -> usize {
        for (d, scope) in self.scopes.iter().enumerate().rev() {
            if scope.contains(name) {
                return d;
            }
        }
        0
    }

    fn is_moved(&self, name: &str) -> bool {
        self.moved.contains(&(self.depth_of(name), name.to_string()))
    }

    fn mark_moved(&mut self, name: &str) {
        let d = self.depth_of(name);
        self.moved.insert((d, name.to_string()));
    }

    fn clear_moved(&mut self, name: &str) {
        let d = self.depth_of(name);
        self.moved.remove(&(d, name.to_string()));
    }
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Run the borrow / move checker over every function in the program.
///
/// Setting `MANITC_MOVE_SWEEP` (to the file label to print) additionally emits
/// one `MOVE-SWEEP` line on stderr describing what B7's candidate rules would
/// cost on this program. It is an instrument, not a feature: the VERDICT this
/// function returns is unchanged by it, and it is off unless the variable is
/// set.
pub fn check_borrows(program: &TypedProgram) -> CompileResult<()> {
    if let Ok(label) = std::env::var("MANITC_MOVE_SWEEP") {
        let sweep = sweep_move_sites(program);
        eprintln!("{}", sweep.line(&label));
        if std::env::var("MANITC_MOVE_SWEEP_SURVEY").is_ok() {
            let all = |c, a| MoveRules {
                call_args_move: c,
                array_elems_move: a,
                user_code_only: false,
            };
            for (kind, r) in [("call", all(true, false)), ("array", all(false, true))] {
                for (module, n) in survey_move_failures(program, r) {
                    eprintln!("MOVE-SURVEY {} {} {}", kind, module, n);
                }
            }
        }
        if std::env::var("MANITC_MOVE_SWEEP_WHERE").is_ok() {
            if let Some(ref w) = sweep.first_call_err {
                eprintln!("MOVE-SWEEP-WHERE call  {}  file={}", w, label);
            }
            if let Some(ref w) = sweep.first_array_err {
                eprintln!("MOVE-SWEEP-WHERE array {}  file={}", w, label);
            }
        }
    }
    check_borrows_with(program, MoveRules::CURRENT).0
}

/// The checker, under an explicit rule set. Returns the verdict and the
/// population counts reached before it (counts are complete for a program that
/// passes; a program that fails is counted only up to its first error).
fn check_borrows_with(
    program: &TypedProgram,
    rules: MoveRules,
) -> (CompileResult<()>, MoveSites) {
    check_borrows_located(program, rules).0
}

/// As `check_borrows_with`, and also the first diagnostic with its module.
fn check_borrows_located(
    program: &TypedProgram,
    rules: MoveRules,
) -> ((CompileResult<()>, MoveSites), Option<String>) {
    // D-2: one pass over the program's signatures, shared by every function
    // check. Built here rather than looked up per call site, because the map
    // is a property of the PROGRAM and rebuilding it per function would be
    // quadratic in a language whose stdlib is merged into every module.
    let consuming: std::rc::Rc<HashMap<String, Vec<bool>>> = std::rc::Rc::new(
        program
            .functions
            .iter()
            .filter(|f| f.params.iter().any(|p| p.is_move))
            .map(|f| {
                (f.name.clone(), f.params.iter().map(|p| p.is_move).collect())
            })
            .collect(),
    );
    // P118: one pass over the struct table, for the same reason as `consuming`
    // above — it is a property of the PROGRAM.
    let aggregates: std::rc::Rc<HashSet<String>> = std::rc::Rc::new(
        program
            .struct_fields
            .iter()
            .filter(|(_, fields)| !fields.is_empty())
            .map(|(name, _)| name.clone())
            .collect(),
    );
    let mut sites = MoveSites::default();
    for func in &program.functions {
        let (result, fn_sites, where_) =
            check_fn_borrows(func, rules, consuming.clone(), aggregates.clone());
        sites.add(fn_sites);
        if let Err(e) = result {
            return ((Err(e), sites), where_);
        }
    }
    ((Ok(()), sites), None)
}

/// Every function that fails under `rules`, grouped by the module its first
/// diagnostic came from.
///
/// `check_borrows_with` stops at the FIRST failing function, which answers
/// "is the program refused" and nothing else -- so a survey built on it can
/// only ever name one module and would report a one-module problem whether
/// there was one module or twelve. Each function gets a fresh `MoveEnv`, so
/// continuing past a failure is sound rather than merely convenient.
pub fn survey_move_failures(
    program: &TypedProgram,
    rules: MoveRules,
) -> std::collections::BTreeMap<String, usize> {
    let mut by_module = std::collections::BTreeMap::new();
    for func in &program.functions {
        let (result, _, where_) =
            check_fn_borrows(func, rules, Default::default(), Default::default());
        if result.is_err() {
            let module = where_
                .as_deref()
                .and_then(|w| w.split(':').next())
                .unwrap_or("<unknown>")
                .to_string();
            *by_module.entry(module).or_insert(0) += 1;
        }
    }
    by_module
}

/// Measure B7's D-2 and D-3 against one program: the population of sites each
/// rule would bite on, and whether the program survives each rule.
///
/// The four verdicts are taken by RUNNING the checker four times rather than
/// by reasoning about the counts, because a site only costs something when the
/// value is used again afterwards -- the population is an upper bound on the
/// blast radius and not the blast radius itself.
pub fn sweep_move_sites(program: &TypedProgram) -> MoveSweep {
    let rules = |call, array, user_only| MoveRules {
        call_args_move: call,
        array_elems_move: array,
        user_code_only: user_only,
    };
    let run = |r| {
        let ((verdict, sites), where_) = check_borrows_located(program, r);
        (verdict.is_ok(), sites, where_)
    };
    let (ok_current, sites, _) = run(MoveRules::CURRENT);
    // D-2 is the candidate still open, so it is measured ON TOP of what ships.
    let (ok_call, _, call_where) = run(rules(true, true, true));
    // D-3 is measured RETROSPECTIVELY: `false` here marks a file whose verdict
    // the container/varargs split changed.
    let (ok_array, _, array_where) = run(MoveRules::PRE_D3);
    let (ok_both, _, _) = run(rules(true, false, true));
    let (ok_call_all, _, call_all_where) = run(rules(true, true, false));
    let (ok_array_all, _, array_all_where) = run(rules(false, true, false));
    MoveSweep {
        sites,
        ok_current,
        ok_call,
        ok_array,
        ok_both,
        ok_call_all,
        ok_array_all,
        first_call_err: call_where.or(call_all_where),
        first_array_err: array_where.or(array_all_where),
    }
}

// ---------------------------------------------------------------------------
// Per-function analysis
// ---------------------------------------------------------------------------

fn check_fn_borrows(
    func: &TypedFnDef,
    rules: MoveRules,
    consuming: std::rc::Rc<HashMap<String, Vec<bool>>>,
    aggregates: std::rc::Rc<HashSet<String>>,
) -> (CompileResult<()>, MoveSites, Option<String>) {
    let mut env = MoveEnv::with_consuming(rules, consuming, aggregates);
    let result = if let Some(ref body) = func.body {
        for param in &func.params {
            env.declare(&param.name);
        }
        check_block_borrows(body, &mut env, None)
    } else {
        Ok(())
    };
    (result, env.sites, env.first_err)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn err(span: crate::ast::Span, msg: String) -> CompileError {
    CompileError::type_err("<borrow>", span.line, span.col, msg)
}

/// The same diagnostic, with WHERE it came from, for the sweep only.
///
/// The shipped `err` above renders the file as `<borrow>` and drops
/// `Span::module` -- P80's second site, one more time -- so a message from the
/// merged stdlib is indistinguishable from the user's own. Rather than change
/// a user-visible diagnostic from inside an instrument, the sweep formats its
/// own copy and the shipped path is untouched.
fn located(span: crate::ast::Span, msg: &str) -> String {
    format!("{}:{}:{}: {}", span.module.unwrap_or("<user>"), span.line, span.col, msg)
}

/// If `expr` is a plain variable of move type, consume it: enforce the
/// move-in-loop and use-after-move rules, then mark the binding moved.
/// Enum variant constructors (containing "::") are constants, not variables.
fn consume_if_move(
    expr: &TypedExpr,
    env: &mut MoveEnv,
    loop_from: LoopBoundary,
) -> CompileResult<()> {
    if let TypedExprKind::Ident(ref var_name) = expr.kind {
        if is_move_type(&expr.ty) && !var_name.contains("::") {
            let depth = env.depth_of(var_name);
            // Variables declared inside the loop body (depth >= boundary) are
            // fresh each iteration -- moving them is fine.
            // WITHDRAWN, 2 September 2026, and the measurement is the result.
            // An exemption was built here for a binding the enclosing
            // assignment restores -- `out = f(out)` is moved and replaced, not
            // moved on each iteration -- because `stdlib/ternary.mt`'s
            // accumulator appeared to need it under D-3. It did not: that site
            // is a VARARGS list, which the array rule below exempts for a
            // reason of its own, and the assignment exemption then measured
            // **0 verdict differences over 366 repo files and 2,507 corpus
            // files**. A loosening of a soundness rule, introduced for a cause
            // that turned out to be something else and needed by nothing, is
            // withdrawn rather than shipped (P66's shape). The tuple
            // accumulator `out = (out, i).0` in a loop is still refused, and
            // that is a decision for B7 to take on its own evidence.
            if let Some(boundary) = loop_from {
                if depth < boundary {
                    let msg = format!(
                        "cannot move '{}' in a loop \
                         -- value would be moved on each iteration",
                        var_name
                    );
                    env.note_err(expr.span, &msg);
                    return Err(err(expr.span, msg));
                }
            }
            if env.is_moved(var_name) {
                let msg = format!("use of moved value: '{}'", var_name);
                env.note_err(expr.span, &msg);
                return Err(err(expr.span, msg));
            }
            env.mark_moved(var_name);
        }
    }
    Ok(())
}

/// **F-4 rule 3**, in one place because it has two callers that look nothing
/// alike: an assignment whose target outlives the region, and a method call
/// that hands a cell to a receiver that does.
///
/// `holder` is the binding that would still be holding the cell after the
/// release, and `value` is what it would be holding.
fn region_escape_check(
    env: &MoveEnv,
    holder: &str,
    value: &TypedExpr,
    span: crate::ast::Span,
) -> CompileResult<()> {
    let Some(rf) = env.region_from else { return Ok(()) };
    if env.depth_of(holder) >= rf || !env.is_region_storage(&value.ty) {
        return Ok(());
    }
    Err(err(span, format!(
        "`{}` outlives this `region`, so it may not be given a value of type \
         `{}` inside it — the region releases every cell allocated in it, and \
         this binding would still be holding one. Scalars may leave a region \
         freely; `str`, structs, tuples and arrays may not",
        holder, type_name(&value.ty)
    )))
}

/// The binding a projection chain is rooted at: `v[i].f` is rooted at `v`.
///
/// **P119**: F-4's rule 3 first asked only about a plain-identifier assignment
/// target, so `outer[0] = s` and `outer.f = s` walked past it — the target was
/// an `Index`/`Field`, not an `Ident`, and the check never ran. The root is
/// what the rule was always about: whichever binding still holds the cell
/// after the region releases.
fn root_binding(expr: &TypedExpr) -> Option<&str> {
    match &expr.kind {
        TypedExprKind::Ident(n) => Some(n),
        TypedExprKind::Index(base, _) | TypedExprKind::Field(base, _) => root_binding(base),
        _ => None,
    }
}

/// A short name for a type, for diagnostics. `ManiType` has no `Display`, and
/// F-4's message is more useful naming the kind than enumerating the variant.
fn type_name(ty: &ManiType) -> &'static str {
    match ty {
        ManiType::Str => "str",
        ManiType::Struct(_, _) => "a struct",
        ManiType::Tuple(_) => "a tuple",
        ManiType::Array(_, _) => "an array",
        _ => "a storage type",
    }
}

/// A struct, tuple or array -- a type whose storage has more than one word
/// and for which copy-versus-alias is therefore observable.
fn is_aggregate(ty: &ManiType) -> bool {
    matches!(
        ty,
        ManiType::Struct(_, _) | ManiType::Tuple(_) | ManiType::Array(_, _)
    )
}

/// Does a candidate rule apply at this span? Under `user_code_only` the
/// merged stdlib is exempt, so the verdict is about the program rather than
/// about the library every program carries.
fn in_scope(rules: MoveRules, span: crate::ast::Span) -> bool {
    !rules.user_code_only || span.module.is_none()
}

/// Is `expr` the shape `consume_if_move` can bite on -- a plain variable of
/// move type? Enum variant constructors (containing "::") are constants.
///
/// This is the POPULATION predicate, and it is deliberately the same condition
/// `consume_if_move` tests, so a counted site and a consumed site cannot drift
/// apart.
fn is_move_site(expr: &TypedExpr) -> bool {
    match expr.kind {
        TypedExprKind::Ident(ref name) => {
            is_move_type(&expr.ty) && !name.contains("::")
        }
        _ => false,
    }
}

/// Collect every name bound by a match pattern.
fn declare_pattern_names(pat: &Pattern, env: &mut MoveEnv) {
    match pat {
        Pattern::Wildcard(_) | Pattern::Lit(_, _) => {}
        Pattern::Ident(n, _) => env.declare(n),
        // C6: a trit capture is a fresh `int`, computed from the scrutinee
        // rather than aliasing it, so it is declared and never a move.
        Pattern::Trit(tp, _) => {
            for n in tp.bound_names() {
                env.declare(&n);
            }
        }
        Pattern::Tuple(ps, _) | Pattern::Or(ps, _) | Pattern::Enum(_, _, ps, _) => {
            for p in ps {
                declare_pattern_names(p, env);
            }
        }
        Pattern::Struct(_, fields, _) => {
            for (_, p) in fields {
                declare_pattern_names(p, env);
            }
        }
    }
}

/// Fork the moved-set for each branch and union the results afterwards:
/// anything moved in ANY branch is conservatively considered moved.
fn check_branches<F>(
    env: &mut MoveEnv,
    branches: Vec<F>,
) -> CompileResult<()>
where
    F: FnOnce(&mut MoveEnv) -> CompileResult<()>,
{
    let base = env.moved.clone();
    let mut acc = base.clone();
    for branch in branches {
        env.moved = base.clone();
        branch(env)?;
        acc.extend(env.moved.drain());
    }
    env.moved = acc;
    Ok(())
}

// ---------------------------------------------------------------------------
// Block
// ---------------------------------------------------------------------------

fn check_block_borrows(
    block: &TypedBlock,
    env: &mut MoveEnv,
    loop_from: LoopBoundary,
) -> CompileResult<()> {
    env.push_scope();
    let result = (|| {
        for stmt in &block.stmts {
            check_stmt_borrows(stmt, env, loop_from)?;
        }
        Ok(())
    })();
    env.pop_scope();
    result
}

// ---------------------------------------------------------------------------
// Statements
// ---------------------------------------------------------------------------

fn check_stmt_borrows(
    stmt: &TypedStmt,
    env: &mut MoveEnv,
    loop_from: LoopBoundary,
) -> CompileResult<()> {
    match stmt {
        TypedStmt::Let(let_stmt) => {
            // Check the initialiser BEFORE the new binding exists, so
            // `let s = s;` reads the outer `s`.
            if let Some(ref init_expr) = let_stmt.init {
                check_expr_borrows(init_expr, env, loop_from)?;
                if is_aggregate(&init_expr.ty)
                    && matches!(
                        init_expr.kind,
                        TypedExprKind::Field(_, _) | TypedExprKind::Index(_, _)
                    )
                {
                    bump(
                        &mut env.sites.aggregate_from_projection,
                        init_expr.span,
                    );
                }
                consume_if_move(init_expr, env, loop_from)?;
            }

            // A new `let` binding shadows / rebinds in the CURRENT scope.
            match &let_stmt.pat {
                LetPat::Ident(_) => env.declare(&let_stmt.name),
                // Tuple destructuring declares each element name; the first
                // element is NOT redefined with the whole tuple type.
                LetPat::Tuple(names) => {
                    for n in names {
                        env.declare(n);
                    }
                }
            }
        }

        TypedStmt::Assign(assign_stmt) => {
            // Check RHS first (evaluate value before assigning).
            check_expr_borrows(&assign_stmt.value, env, loop_from)?;
            consume_if_move(&assign_stmt.value, env, loop_from)?;

            // F-4 rule 3: inside a region, anything that OUTLIVES the region
            // may not be given a value of storage type. The region is about to
            // invalidate every cell allocated inside it, and an outer binding
            // is exactly what would still be holding one.
            //
            // Asked of the ROOT of the target and not of the target itself
            // (P119): `outer[0] = s` and `outer.f = s` are an `Index` and a
            // `Field`, so a check written for `Ident` alone walked straight
            // past them and the same cell escaped by a different spelling.
            //
            // Names this environment never declared — globals, and anything
            // unresolved — report depth 0, so they count as outer, which is
            // the conservative direction.
            if let Some(root) = root_binding(&assign_stmt.target) {
                region_escape_check(env, root, &assign_stmt.value, assign_stmt.target.span)?;
            }

            match &assign_stmt.target.kind {
                // A plain-identifier target is a REBIND, not a read: it must
                // not trip use-after-move, and it clears the moved flag.
                // (Compound assignments like `s += x` do read the target.)
                TypedExprKind::Ident(ref target_name) => {
                    if assign_stmt.op.is_some()
                        && !target_name.contains("::")
                        && env.is_moved(target_name)
                    {
                        let msg =
                            format!("use of moved value: '{}'", target_name);
                        env.note_err(assign_stmt.target.span, &msg);
                        return Err(err(assign_stmt.target.span, msg));
                    }
                    env.clear_moved(target_name);
                }
                // Index / field targets read their base expression.
                _ => check_expr_borrows(&assign_stmt.target, env, loop_from)?,
            }
        }

        TypedStmt::Expr(expr) => {
            check_expr_borrows(expr, env, loop_from)?;
        }

        // **F-4**: `region { B }` — everything B allocates is released when it
        // ends. The three rules below are the whole safety argument, and each
        // fails in a different direction.
        TypedStmt::Region(block, span) => {
            let outer = env.region_from;
            // `check_block_borrows` pushes a scope, so the block's own
            // bindings live at this depth and anything BELOW it outlives the
            // region.
            env.region_from = Some(env.scopes.len());
            let result = check_block_borrows(block, env, loop_from);
            env.region_from = outer;
            result?;
            let _ = span;
        }

        TypedStmt::Return(opt_expr) => {
            // F-4 rule 1: a `return` inside a region would leave without
            // releasing, and — worse — could carry out a value the release is
            // about to invalidate. Refusing it is both halves at once.
            if let Some(depth) = env.region_from {
                let _ = depth;
                let span = opt_expr
                    .as_ref()
                    .map(|e| e.span)
                    .unwrap_or_else(crate::ast::Span::default);
                return Err(err(span, "`return` inside a `region` is not \
                    allowed: it would leave without releasing the region, and \
                    a returned value could point into the memory the release \
                    invalidates. Compute the value, end the region, then \
                    return".to_string()));
            }
            if let Some(ref expr) = opt_expr {
                check_expr_borrows(expr, env, loop_from)?;
            }
        }

        TypedStmt::Break | TypedStmt::Continue => {
            // F-4 rule 2: a `break` or `continue` that leaves a region skips
            // its release. Refused only when the LOOP is outside the region —
            // a loop written inside one is ordinary, and its `break` lands
            // inside too. The depths are what tell the two apart, which is why
            // `loop_from` and `region_from` are both scope depths and not
            // flags.
            if let Some(rf) = env.region_from {
                let escapes = match loop_from {
                    None => true,
                    Some(lf) => lf < rf,
                };
                if escapes {
                    return Err(err(
                        crate::ast::Span::default(),
                        "`break`/`continue` out of a `region` is not allowed: \
                         it would leave the region without releasing it. Put \
                         the loop inside the region, or the region inside the \
                         loop body"
                            .to_string(),
                    ));
                }
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Expressions
// ---------------------------------------------------------------------------

fn check_expr_borrows(
    expr: &TypedExpr,
    env: &mut MoveEnv,
    loop_from: LoopBoundary,
) -> CompileResult<()> {
    match &expr.kind {
        // --- Identifier (variable read) ---
        TypedExprKind::Ident(name) => {
            // Enum variant constructors (e.g. "Season::Summer") are constant
            // expressions, not variables -- skip them.
            if !name.contains("::") && env.is_moved(name) {
                let msg = format!("use of moved value: '{}'", name);
                env.note_err(expr.span, &msg);
                return Err(err(expr.span, msg));
            }
        }

        // --- Literals ---
        TypedExprKind::Lit(_) => {}

        // --- Binary / Unary operators ---
        TypedExprKind::BinOp(lhs, _op, rhs) => {
            check_expr_borrows(lhs, env, loop_from)?;
            check_expr_borrows(rhs, env, loop_from)?;
        }
        TypedExprKind::UnOp(_op, operand) => {
            check_expr_borrows(operand, env, loop_from)?;
        }

        // --- Function call ---
        // **D-2.** By default a call argument does NOT move: ManiT has no
        // borrow/move syntax, so consuming every argument would reject valid
        // programs. That default is also why `fn consume(x: T)` cannot be
        // written, which is B7's D-2 and F-4's precondition. The site is
        // always COUNTED so the cost of changing the default is measurable.
        TypedExprKind::Call(callee, args) => {
            check_expr_borrows(callee, env, loop_from)?;
            // **D-2**: the callee's name decides which arguments are consumed.
            // A call through a function POINTER consumes nothing, because the
            // signature is not in hand — stated rather than left implicit, and
            // pinned by a row.
            let callee_name = match &callee.kind {
                TypedExprKind::Ident(n) => Some(n.clone()),
                _ => None,
            };
            for (i, arg) in args.iter().enumerate() {
                env.array_is_vararg =
                    matches!(arg.kind, TypedExprKind::Array(_));
                let r = check_expr_borrows(arg, env, loop_from);
                env.array_is_vararg = false;
                r?;
                if is_move_site(arg) {
                    bump(&mut env.sites.call_arg, arg.span);
                }
                // D-2 is checked BEFORE the sweep's candidate rule, so a
                // `move` parameter consumes whether or not the experimental
                // whole-language rule is switched on.
                let consumed = callee_name
                    .as_deref()
                    .is_some_and(|n| env.consumes(n, i));
                if consumed
                    || (env.rules.call_args_move && in_scope(env.rules, arg.span))
                {
                    consume_if_move(arg, env, loop_from)?;
                }
            }
        }

        // --- Method call ---
        // The RECEIVER is counted and never consumed: whether `self` is taken
        // by value is a separate decision from D-2, and the count is here so
        // that decision can start from a number.
        TypedExprKind::MethodCall(receiver, _method, args, _) => {
            check_expr_borrows(receiver, env, loop_from)?;
            if is_move_site(receiver) {
                bump(&mut env.sites.method_recv, receiver.span);
            }
            // **F-4 rule 3, second half (P119).** `v.push(s)` where `v`
            // outlives the region and `s` was allocated inside it puts a cell
            // somewhere that survives the release — the same escape as an
            // assignment, by a route an assignment rule cannot see.
            //
            // Measured before the rule was written, on the compiler that
            // shipped without it: `let v: Vec<str>` outside, `v.push(s)`
            // inside, and afterwards **T3 printed nothing for `v[0]` while
            // LLVM printed `hello`** — a silent wrong answer on one backend
            // and a divergence between them, in code F-4 accepted.
            //
            // The receiver's own type is not the question: a `Vec` handle may
            // leave a region, and what may not is the CELL it would be left
            // holding. So the test is on the ARGUMENTS.
            if env.region_from.is_some() {
                if let Some(root) = root_binding(receiver) {
                    for arg in args {
                        region_escape_check(env, root, arg, arg.span)?;
                    }
                }
            }
            for arg in args {
                env.array_is_vararg =
                    matches!(arg.kind, TypedExprKind::Array(_));
                let r = check_expr_borrows(arg, env, loop_from);
                env.array_is_vararg = false;
                r?;
                if is_move_site(arg) {
                    bump(&mut env.sites.call_arg, arg.span);
                }
                if env.rules.call_args_move && in_scope(env.rules, arg.span) {
                    consume_if_move(arg, env, loop_from)?;
                }
            }
        }

        // --- Index / Field access ---
        TypedExprKind::Index(base, idx) => {
            check_expr_borrows(base, env, loop_from)?;
            check_expr_borrows(idx, env, loop_from)?;
        }
        TypedExprKind::Field(base, _field) => {
            check_expr_borrows(base, env, loop_from)?;
        }

        // --- Block ---
        TypedExprKind::Block(block) => {
            check_block_borrows(block, env, loop_from)?;
        }

        // --- If ---
        TypedExprKind::If(if_expr) => {
            check_expr_borrows(&if_expr.cond, env, loop_from)?;
            for (elif_cond, _) in &if_expr.elif_branches {
                check_expr_borrows(elif_cond, env, loop_from)?;
            }

            // Each branch forks the moved-set; the results are unioned.
            let mut branches: Vec<Box<dyn FnOnce(&mut MoveEnv) -> CompileResult<()>>> =
                vec![Box::new(|env: &mut MoveEnv| {
                    check_block_borrows(&if_expr.then_block, env, loop_from)
                })];
            for (_, elif_block) in &if_expr.elif_branches {
                branches.push(Box::new(move |env: &mut MoveEnv| {
                    check_block_borrows(elif_block, env, loop_from)
                }));
            }
            if let Some(ref else_block) = if_expr.else_block {
                branches.push(Box::new(move |env: &mut MoveEnv| {
                    check_block_borrows(else_block, env, loop_from)
                }));
            }
            check_branches(env, branches)?;
        }

        // --- Tif (ternary if: pos / zero / neg) ---
        TypedExprKind::Tif(tif_expr) => {
            check_expr_borrows(&tif_expr.cond, env, loop_from)?;
            check_branches(env, vec![
                Box::new(|env: &mut MoveEnv| {
                    check_block_borrows(&tif_expr.pos_block, env, loop_from)
                }) as Box<dyn FnOnce(&mut MoveEnv) -> CompileResult<()>>,
                Box::new(|env: &mut MoveEnv| {
                    check_block_borrows(&tif_expr.zero_block, env, loop_from)
                }),
                Box::new(|env: &mut MoveEnv| {
                    check_block_borrows(&tif_expr.neg_block, env, loop_from)
                }),
            ])?;
        }

        // --- Match ---
        TypedExprKind::Match(match_expr) => {
            check_expr_borrows(&match_expr.scrutinee, env, loop_from)?;

            let branches: Vec<Box<dyn FnOnce(&mut MoveEnv) -> CompileResult<()>>> =
                match_expr.arms.iter().map(|arm| {
                    Box::new(move |env: &mut MoveEnv| {
                        // Pattern bindings are fresh per arm.
                        env.push_scope();
                        let r = (|| {
                            declare_pattern_names(&arm.pattern, env);
                            if let Some(ref guard) = arm.guard {
                                check_expr_borrows(guard, env, loop_from)?;
                            }
                            check_expr_borrows(&arm.body, env, loop_from)
                        })();
                        env.pop_scope();
                        r
                    }) as Box<dyn FnOnce(&mut MoveEnv) -> CompileResult<()>>
                }).collect();
            check_branches(env, branches)?;
        }

        // --- For loop ---
        TypedExprKind::For(for_expr) => {
            check_expr_borrows(&for_expr.iter, env, loop_from)?;
            // The loop variable is fresh each iteration: declare it inside
            // the loop boundary so moving it is not a move-in-loop.
            env.push_scope();
            let boundary = env.scopes.len() - 1;
            env.declare(&for_expr.var);
            let r = check_block_borrows(&for_expr.body, env, Some(boundary));
            env.pop_scope();
            r?;
        }

        // --- While loop ---
        TypedExprKind::While(while_expr) => {
            check_expr_borrows(&while_expr.cond, env, loop_from)?;
            let boundary = env.scopes.len();
            check_block_borrows(&while_expr.body, env, Some(boundary))?;
        }

        // --- Infinite loop ---
        TypedExprKind::Loop(body) => {
            let boundary = env.scopes.len();
            check_block_borrows(body, env, Some(boundary))?;
        }

        // --- Array literal ---
        //
        // **D-3, and the decision is not the one the design document framed.**
        // It asked which of the tuple row and the array row is wrong, given
        // that a tuple literal consumes its elements and an array literal did
        // not. Measured, the question is malformed: an array literal is TWO
        // constructs wearing one syntax.
        //
        //   * In BINDING position — `let v = [a, b];`, a struct field, a
        //     return — it is a CONTAINER that outlives the expression and
        //     holds a second name for each element. It consumes, exactly as a
        //     tuple and a struct literal always have.
        //   * In ARGUMENT position — `fmt::format("{}", [g, out])` — it is
        //     this language's VARARGS list. It is an argument, so D-2 governs
        //     it, and a call does not consume its argument. Consuming here
        //     would make `f(s)` and `f([s])` disagree about the same `s` in
        //     the same call.
        //
        // The split is not a carve-out fitted to the failures: **1,120 of
        // 1,120 array-literal sites in the standard library are varargs**, and
        // 36-56 % of the user ones. Treating the argument list as a container
        // would have refused `fmt::format` itself.
        TypedExprKind::Array(elems) => {
            let is_vararg = std::mem::replace(&mut env.array_is_vararg, false);
            for elem in elems {
                check_expr_borrows(elem, env, loop_from)?;
                if is_move_site(elem) {
                    bump(&mut env.sites.array_elem, elem.span);
                    if is_vararg {
                        bump(&mut env.sites.array_elem_in_call, elem.span);
                    }
                }
                if env.rules.array_elems_move
                    && !is_vararg
                    && in_scope(env.rules, elem.span)
                {
                    consume_if_move(elem, env, loop_from)?;
                }
            }
        }

        // --- Tuple literal ---
        TypedExprKind::Tuple(elems) => {
            for elem in elems {
                check_expr_borrows(elem, env, loop_from)?;
                // Elements of move type are consumed.
                consume_if_move(elem, env, loop_from)?;
            }
        }

        // --- Struct literal ---
        TypedExprKind::StructLit(_name, fields) => {
            for (_field_name, field_expr) in fields {
                check_expr_borrows(field_expr, env, loop_from)?;
                consume_if_move(field_expr, env, loop_from)?;
            }
        }

        // --- Range ---
        TypedExprKind::Range(start, end, _inclusive) => {
            check_expr_borrows(start, env, loop_from)?;
            check_expr_borrows(end, env, loop_from)?;
        }

        // --- Return (expression form) ---
        TypedExprKind::Return(inner) => {
            check_expr_borrows(inner, env, loop_from)?;
        }

        // --- Cast ---
        TypedExprKind::Cast(inner, _ty) => {
            check_expr_borrows(inner, env, loop_from)?;
        }

        // --- ? operator ---
        TypedExprKind::Question(inner) => {
            check_expr_borrows(inner, env, loop_from)?;
        }

        // --- Spawn ---
        // §11.4: `yield` moves nothing and reads nothing.
        TypedExprKind::Yield => {}
        TypedExprKind::Spawn(block, captures) => {
            // **P118.** §11.2: "a spawned task gets a COPY of the spawning
            // task's store at the moment of the spawn, and its writes are its
            // own." Neither backend makes that copy for an AGGREGATE, and they
            // fail in opposite directions — measured on both, with the task's
            // write ordered before the spawner's read:
            //
            //   T3   shares the heap cell: `p.x = 99` inside the task is
            //        visible to the spawner. §11.2 violated.
            //   LLVM binds the captured ADDRESS as though it were the value,
            //        into a fresh cell — so the task reads `x=94299331632576
            //        y=0`, and its write lands on a cell nobody else holds.
            //
            // Refused here rather than fixed in the backends, because the fix
            // is a deep copy at the spawn site and a deep copy needs regions
            // to be affordable (P63's heap is 2,536 words with no free) — that
            // is F-4's work and B7's D-4 decision. **The population was
            // measured before the refusal was written** (P103's method): 0 of
            // the 34 `spawn` sites in both repositories and the corpus capture
            // an aggregate. Every one captures a channel, a `Mutex`/atomic
            // handle, or a scalar.
            for (name, ty) in captures {
                if env.reaches_through_a_pointer(ty) {
                    return Err(err(expr.span, format!(
                        "`spawn` would capture `{name}`, which is an aggregate. \
                        docs/semantics.md §11.2 gives a spawned task a COPY of the store, \
                        and neither backend makes one for an aggregate: T3 shares it, so a \
                        write inside the task escapes, and LLVM binds its address as the \
                        value, so the task reads garbage. Send it over a channel instead, \
                        or read what you need before the spawn"
                    )));
                }
            }
            // §11.2 again, in the other direction: because the task's store is
            // a COPY, a move INSIDE the task consumes the task's binding and
            // not the spawner's. This used to say "anything it moves is also
            // moved in the parent scope", which is a plain block's rule — and
            // a `spawn` is not a block. The loosening is sound only because of
            // the refusal above: what remains capturable is scalars, strings
            // and handles, all of which really are copied by both backends.
            let before = env.moved.clone();
            let result = check_block_borrows(block, env, loop_from);
            env.moved = before;
            result?;
        }

        // --- Await ---
        TypedExprKind::Await(inner) => {
            check_expr_borrows(inner, env, loop_from)?;
        }

        // --- Break / Continue (expression form) ---
        TypedExprKind::Break | TypedExprKind::Continue => {}

        // --- Tresult ---
        // The three arms are mutually exclusive at runtime: fork + union,
        // exactly like if/match (a move in ok_block must not poison
        // err_block). Each arm's binding variable is fresh in its scope.
        TypedExprKind::Tresult(tr) => {
            check_expr_borrows(&tr.expr, env, loop_from)?;
            let arms: [(&String, &TypedBlock); 3] = [
                (&tr.ok_var, &tr.ok_block),
                (&tr.unknown_var, &tr.unknown_block),
                (&tr.err_var, &tr.err_block),
            ];
            let branches: Vec<Box<dyn FnOnce(&mut MoveEnv) -> CompileResult<()>>> =
                arms.into_iter().map(|(var, block)| {
                    Box::new(move |env: &mut MoveEnv| {
                        env.push_scope();
                        env.declare(var);
                        let r = check_block_borrows(block, env, loop_from);
                        env.pop_scope();
                        r
                    }) as Box<dyn FnOnce(&mut MoveEnv) -> CompileResult<()>>
                }).collect();
            check_branches(env, branches)?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Copy vs Move classification
// ---------------------------------------------------------------------------

/// Returns `true` if the type is "moved" when passed / assigned (non-Copy).
/// Copy types (numeric scalars, bool, trit, char, void, function pointers)
/// are never moved.
fn is_move_type(ty: &ManiType) -> bool {
    match ty {
        // Copy types -- small scalars, function pointers.
        ManiType::Int
        | ManiType::Float
        | ManiType::Bool
        | ManiType::Bool3
        | ManiType::Trit
        | ManiType::Tryte
        | ManiType::T9
        | ManiType::T27
        | ManiType::T54
        // ManiType::Trint merged into T54
        | ManiType::Tfloat
        | ManiType::Char
        | ManiType::Void
        | ManiType::Unknown => false,

        // Function types are Copy (pointer-sized).
        ManiType::Fn(_, _) => false,

        // Concurrency handles are shared references by design: the runtime
        // representation is a pointer to shared state, and the documented
        // usage pattern aliases them across tasks (`let c = counter; spawn
        // { c.lock(); ... }`). Copying the handle copies the reference, so
        // they are Copy, not move.
        ManiType::Struct(name, _)
            if matches!(
                name.as_str(),
                "AtomicTrit" | "Barrier" | "Semaphore" | "MutexGuard"
            ) =>
        {
            false
        }
        ManiType::Generic(name, _)
            if matches!(name.as_str(), "Mutex" | "Channel" | "Task") =>
        {
            false
        }

        // Move types -- heap-allocated or composite.
        ManiType::Str => true,
        ManiType::Struct(_, _) => true,
        ManiType::Enum(_) => true,
        ManiType::Generic(_, _) => true,
        ManiType::Array(_, _) => true,
        ManiType::Tuple(_) => true,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Lit, Span};

    /// Helper: build a TypedExpr with Ident kind.
    fn ident_expr(name: &str, ty: ManiType) -> TypedExpr {
        TypedExpr {
            kind: TypedExprKind::Ident(name.to_string()),
            ty,
            span: Span::new(1, 1),
        }
    }

    /// Helper: build a TypedExpr with Int literal.
    fn int_lit(val: i64) -> TypedExpr {
        TypedExpr {
            kind: TypedExprKind::Lit(Lit::Int(val)),
            ty: ManiType::Int,
            span: Span::new(1, 1),
        }
    }

    /// Helper: build a `let` of a str literal.
    fn let_str(name: &str, val: &str) -> TypedStmt {
        TypedStmt::Let(TypedLetStmt {
            name: name.to_string(),
            pat: crate::ast::LetPat::Ident(name.to_string()),
            ty: ManiType::Str,
            init: Some(TypedExpr {
                kind: TypedExprKind::Lit(Lit::Str(val.to_string())),
                ty: ManiType::Str,
                span: Span::new(1, 1),
            }),
            mutable: false,
        })
    }

    /// Helper: build `let <name> = <src>;` where src is a str variable.
    fn let_move(name: &str, src: &str) -> TypedStmt {
        TypedStmt::Let(TypedLetStmt {
            name: name.to_string(),
            pat: crate::ast::LetPat::Ident(name.to_string()),
            ty: ManiType::Str,
            init: Some(ident_expr(src, ManiType::Str)),
            mutable: false,
        })
    }

    fn check_stmts(stmts: Vec<TypedStmt>) -> CompileResult<()> {
        check_stmts_under(stmts, MoveRules::CURRENT).0
    }

    /// As `check_stmts`, under an explicit rule set, returning the population
    /// counts beside the verdict.
    fn check_stmts_under(
        stmts: Vec<TypedStmt>,
        rules: MoveRules,
    ) -> (CompileResult<()>, MoveSites) {
        let block = TypedBlock { stmts, ty: ManiType::Void };
        let mut env = MoveEnv::new(rules);
        let r = check_block_borrows(&block, &mut env, None);
        (r, env.sites)
    }

    /// `f(s)` where `s` is a `str` variable -- a call ARGUMENT, D-2's shape.
    fn call_with(src: &str) -> TypedStmt {
        TypedStmt::Expr(TypedExpr {
            kind: TypedExprKind::Call(
                Box::new(ident_expr("f", ManiType::Void)),
                vec![ident_expr(src, ManiType::Str)],
            ),
            ty: ManiType::Void,
            span: Span::new(1, 1),
        })
    }

    /// `f([<src>, <src>]);` -- an array literal in ARGUMENT position, which
    /// is this language's varargs list rather than a container.
    fn call_with_array(src: &str) -> TypedStmt {
        let elem = || ident_expr(src, ManiType::Str);
        TypedStmt::Expr(TypedExpr {
            kind: TypedExprKind::Call(
                Box::new(ident_expr("f", ManiType::Void)),
                vec![TypedExpr {
                    kind: TypedExprKind::Array(vec![elem(), elem()]),
                    ty: ManiType::Array(Box::new(ManiType::Str), Some(2)),
                    span: Span::new(1, 1),
                }],
            ),
            ty: ManiType::Void,
            span: Span::new(1, 1),
        })
    }

    /// `let <name> = [<lit>, <lit>];` -- an array literal whose elements are
    /// LITERALS, which no consuming rule can bite on.
    fn let_array_of_literals(name: &str) -> TypedStmt {
        let lit = || TypedExpr {
            kind: TypedExprKind::Lit(Lit::Str("k".to_string())),
            ty: ManiType::Str,
            span: Span::new(1, 1),
        };
        TypedStmt::Let(TypedLetStmt {
            name: name.to_string(),
            pat: crate::ast::LetPat::Ident(name.to_string()),
            ty: ManiType::Array(Box::new(ManiType::Str), Some(2)),
            init: Some(TypedExpr {
                kind: TypedExprKind::Array(vec![lit(), lit()]),
                ty: ManiType::Array(Box::new(ManiType::Str), Some(2)),
                span: Span::new(1, 1),
            }),
            mutable: false,
        })
    }

    /// `let <name> = [<src>, <src>];` -- an ARRAY literal, D-3's shape.
    fn let_array_of(name: &str, src: &str) -> TypedStmt {
        TypedStmt::Let(TypedLetStmt {
            name: name.to_string(),
            pat: crate::ast::LetPat::Ident(name.to_string()),
            ty: ManiType::Array(Box::new(ManiType::Str), Some(2)),
            init: Some(TypedExpr {
                kind: TypedExprKind::Array(vec![
                    ident_expr(src, ManiType::Str),
                    ident_expr(src, ManiType::Str),
                ]),
                ty: ManiType::Array(Box::new(ManiType::Str), Some(2)),
                span: Span::new(1, 1),
            }),
            mutable: false,
        })
    }

    // -----------------------------------------------------------------------
    // B7's D-3 -- an array literal is a CONTAINER or a VARARGS LIST, and the
    // two are different constructs wearing one syntax.
    //
    // The design document asked which of the tuple row and the array row is
    // wrong. Measured, that question is malformed: 1,120 of 1,120 array-literal
    // sites in the standard library are varargs, so treating the argument list
    // as a container would have refused `fmt::format` itself.
    // -----------------------------------------------------------------------

    #[test]
    fn d3_an_array_literal_in_binding_position_consumes_its_elements() {
        // A container outlives the expression and holds a second name for each
        // element, exactly as a tuple and a struct literal do -- and those two
        // have consumed since the checker was written.
        let e = check_stmts(vec![
            let_str("s", "ab"),
            let_array_of("v", "s"),
        ]).unwrap_err();
        assert!(
            format!("{}", e).contains("use of moved value"),
            "a container array literal must consume, got: {}", e,
        );
    }

    #[test]
    fn d3_an_array_literal_in_argument_position_does_not() {
        // `fmt::format("{}", [g, out])` is a CALL, and a call does not consume
        // its argument (D-2). Consuming inside the varargs list would make
        // `f(s)` and `f([s])` disagree about the same `s` in the same call.
        assert!(
            check_stmts(vec![let_str("s", "ab"), call_with_array("s")]).is_ok(),
            "a varargs array literal must not consume",
        );
        // Twice, because once cannot distinguish "does not consume" from
        // "consumes but nothing read it afterwards".
        assert!(
            check_stmts(vec![
                let_str("s", "ab"),
                call_with_array("s"),
                call_with_array("s"),
            ]).is_ok(),
            "a varargs array literal must not consume, on any call",
        );
    }

    #[test]
    fn d3_only_the_outermost_array_of_a_call_argument_is_the_varargs_list() {
        // `f([[s]])`: the outer literal is the argument list, the inner one is
        // an element of it and therefore a container. Pinned because the first
        // implementation used a DEPTH counter, under which both were exempt.
        let inner = TypedExpr {
            kind: TypedExprKind::Array(vec![
                ident_expr("s", ManiType::Str),
                ident_expr("s", ManiType::Str),
            ]),
            ty: ManiType::Array(Box::new(ManiType::Str), Some(2)),
            span: Span::new(1, 1),
        };
        let outer = TypedExpr {
            ty: ManiType::Array(Box::new(inner.ty.clone()), Some(1)),
            kind: TypedExprKind::Array(vec![inner]),
            span: Span::new(1, 1),
        };
        let call = TypedStmt::Expr(TypedExpr {
            kind: TypedExprKind::Call(
                Box::new(ident_expr("f", ManiType::Void)),
                vec![outer],
            ),
            ty: ManiType::Void,
            span: Span::new(1, 1),
        });
        assert!(
            check_stmts(vec![let_str("s", "ab"), call]).is_err(),
            "the INNER array of `f([[s, s]])` is a container and must consume",
        );
    }

    #[test]
    fn d3_bites_only_on_a_plain_variable() {
        // `["To:", "Subject:"]` is a container and must still be accepted: the
        // rule can only fire on a plain move-type variable, which is what
        // keeps its blast radius narrow (measured: 0 verdict changes over 366
        // repo and 2,507 corpus files).
        assert!(
            check_stmts(vec![let_array_of_literals("labels")]).is_ok(),
            "an array of literals has no move site at all",
        );
    }

    // -----------------------------------------------------------------------
    // B7's instrument -- the sweep must keep DISCRIMINATING
    //
    // These rows are a positive control made permanent. The whole value of the
    // move-site sweep is that it separates "the rule bites here" from "the
    // rule is invisible here"; an instrument that quietly stopped doing so
    // would report a smaller blast radius and read as good news. Permanent
    // rule 9's reasoning, applied to apparatus rather than to a fix.
    // -----------------------------------------------------------------------

    #[test]
    fn b7_the_shipped_rules_consume_at_neither_candidate_site() {
        // The instrument must be INERT in the compiler as shipped: `f(s)`
        // twice and `[s, s]` are both accepted today, and D-2/D-3 exist
        // precisely because they are.
        assert!(check_stmts(vec![
            let_str("s", "ab"),
            call_with("s"),
            call_with("s"),
        ]).is_ok(), "a call argument must not move under CURRENT rules");

        // D-3 was TAKEN, so a CONTAINER array literal now consumes; the
        // varargs form is what must stay inert.
        assert!(check_stmts(vec![
            let_str("s", "ab"),
            call_with_array("s"),
            call_with_array("s"),
        ]).is_ok(), "a varargs array element must not move under CURRENT rules");
    }

    #[test]
    fn b7_each_candidate_rule_bites_on_its_own_site_and_no_other() {
        let call_rule = MoveRules {
            call_args_move: true,
            array_elems_move: false,
            user_code_only: false,
        };
        let array_rule = MoveRules {
            call_args_move: false,
            array_elems_move: true,
            user_code_only: false,
        };

        // D-2 refuses the twice-passed value...
        assert!(check_stmts_under(
            vec![let_str("s", "ab"), call_with("s"), call_with("s")],
            call_rule,
        ).0.is_err(), "D-2 must refuse a value passed twice");
        // ...and D-3 leaves it alone. Each rule is tested against the OTHER's
        // site as well, or "the rule fires" would not distinguish it from
        // "everything fires".
        assert!(check_stmts_under(
            vec![let_str("s", "ab"), call_with("s"), call_with("s")],
            array_rule,
        ).0.is_ok(), "D-3 must not touch a call argument");

        // D-3 refuses the doubled CONTAINER element...
        assert!(check_stmts_under(
            vec![let_str("s", "ab"), let_array_of("v", "s")],
            array_rule,
        ).0.is_err(), "D-3 must refuse an element used twice");
        // ...and D-2 leaves it alone. (`call_rule` has D-3 off, so this also
        // pins that PRE_D3 really is the previous compiler.)
        assert!(check_stmts_under(
            vec![let_str("s", "ab"), let_array_of("v", "s")],
            call_rule,
        ).0.is_ok(), "D-2 must not touch an array element");
    }

    #[test]
    fn b7_the_population_count_is_taken_whether_or_not_the_rule_is_on() {
        // The COUNT and the CONSUME are deliberately separate: the population
        // is what makes a rule's cost measurable BEFORE it is adopted, so it
        // has to be collected under the shipped rules. `is_move_site` is the
        // same predicate `consume_if_move` tests, so the two cannot drift.
        let (verdict, sites) = check_stmts_under(
            vec![let_str("s", "ab"), call_with("s"), call_with("s")],
            MoveRules::CURRENT,
        );
        assert!(verdict.is_ok());
        assert_eq!(sites.call_arg, (2, 0), "two user call-argument sites");

        // Counted on a program that PASSES, deliberately. Under the shipped
        // rules a CONTAINER array literal consumes, so `[s, s]` errors on its
        // second element and the walk stops -- the counts would then be a
        // report of where the checker gave up rather than of the program.
        // The varargs form has the same two sites and no error.
        let (verdict, sites) = check_stmts_under(
            vec![let_str("s", "ab"), call_with_array("s")],
            MoveRules::CURRENT,
        );
        assert!(verdict.is_ok());
        assert_eq!(sites.array_elem, (2, 0), "two user array-element sites");
        assert_eq!(
            sites.array_elem_in_call, (2, 0),
            "both of them in ARGUMENT position -- the split's own counter",
        );

        // And the container form, counted under PRE_D3 so the walk completes.
        let (verdict, sites) = check_stmts_under(
            vec![let_str("s", "ab"), let_array_of("v", "s")],
            MoveRules::PRE_D3,
        );
        assert!(verdict.is_ok(), "PRE_D3 is what shipped before D-3");
        assert_eq!(sites.array_elem, (2, 0), "two user array-element sites");
        assert_eq!(
            sites.array_elem_in_call, (0, 0),
            "neither of them in argument position",
        );
    }

    #[test]
    fn test_copy_type_no_move() {
        // let x: int = 42; let y = x; let z = x;  -- should be fine (int is Copy)
        let stmts = vec![
            TypedStmt::Let(TypedLetStmt {
                name: "x".to_string(),
                pat: crate::ast::LetPat::Ident("x".to_string()),
                ty: ManiType::Int,
                init: Some(int_lit(42)),
                mutable: false,
            }),
            TypedStmt::Let(TypedLetStmt {
                name: "y".to_string(),
                pat: crate::ast::LetPat::Ident("y".to_string()),
                ty: ManiType::Int,
                init: Some(ident_expr("x", ManiType::Int)),
                mutable: false,
            }),
            TypedStmt::Let(TypedLetStmt {
                name: "z".to_string(),
                pat: crate::ast::LetPat::Ident("z".to_string()),
                ty: ManiType::Int,
                init: Some(ident_expr("x", ManiType::Int)),
                mutable: false,
            }),
        ];
        assert!(check_stmts(stmts).is_ok());
    }

    #[test]
    fn test_use_after_move_str() {
        // let s: str = "hi"; let t = s; let u = s;  -- error: use of moved 's'
        let stmts = vec![let_str("s", "hi"), let_move("t", "s"), let_move("u", "s")];
        let result = check_stmts(stmts);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("use of moved value: 's'"), "got: {}", msg);
    }

    #[test]
    fn test_move_in_loop() {
        // let s = "a"; while true { let t = s; }  -- error: move in loop
        let loop_body = TypedBlock {
            stmts: vec![let_move("t", "s")],
            ty: ManiType::Void,
        };
        let stmts = vec![
            let_str("s", "a"),
            TypedStmt::Expr(TypedExpr {
                kind: TypedExprKind::While(TypedWhileExpr {
                    cond: Box::new(TypedExpr {
                        kind: TypedExprKind::Lit(Lit::Bool(true)),
                        ty: ManiType::Bool,
                        span: Span::new(1, 1),
                    }),
                    body: loop_body,
                }),
                ty: ManiType::Void,
                span: Span::new(1, 1),
            }),
        ];
        let result = check_stmts(stmts);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("cannot move 's' in a loop"), "got: {}", msg);
    }

    #[test]
    fn test_move_of_loop_local_is_ok() {
        // while true { let a = "x"; let b = a; }  -- OK: 'a' is fresh each iteration (S15)
        let loop_body = TypedBlock {
            stmts: vec![let_str("a", "x"), let_move("b", "a")],
            ty: ManiType::Void,
        };
        let stmts = vec![TypedStmt::Expr(TypedExpr {
            kind: TypedExprKind::While(TypedWhileExpr {
                cond: Box::new(TypedExpr {
                    kind: TypedExprKind::Lit(Lit::Bool(true)),
                    ty: ManiType::Bool,
                    span: Span::new(1, 1),
                }),
                body: loop_body,
            }),
            ty: ManiType::Void,
            span: Span::new(1, 1),
        })];
        assert!(check_stmts(stmts).is_ok(), "loop-local move must be accepted");
    }

    #[test]
    fn test_rebind_clears_moved() {
        // let s: str = "a"; let t = s; let s: str = "b"; let u = s; -- OK
        let stmts = vec![
            let_str("s", "a"),
            let_move("t", "s"),
            let_str("s", "b"),
            let_move("u", "s"),
        ];
        assert!(check_stmts(stmts).is_ok());
    }

    #[test]
    fn test_tresult_arms_fork_moved_set() {
        // S16: a move in the ok arm must not poison the err arm — the three
        // arms are mutually exclusive at runtime.
        let span = Span::new(1, 1);
        let arm_block = |dst: &str| TypedBlock {
            stmts: vec![let_move(dst, "s")],
            ty: ManiType::Void,
        };
        let stmts = vec![
            let_str("s", "shared"),
            TypedStmt::Expr(TypedExpr {
                kind: TypedExprKind::Tresult(TypedTresultExpr {
                    expr: Box::new(int_lit(1)),
                    ok_var: "v".to_string(),
                    ok_block: arm_block("a"),
                    unknown_var: "u".to_string(),
                    unknown_block: arm_block("b"),
                    err_var: "e".to_string(),
                    err_block: arm_block("c"),
                }),
                ty: ManiType::Void,
                span,
            }),
        ];
        assert!(
            check_stmts(stmts).is_ok(),
            "a move in one tresult arm must not poison the others"
        );
    }

    #[test]
    fn test_shadowed_inner_move_does_not_poison_outer() {
        // S14: moving an inner shadow must not mark the outer binding moved.
        let inner = TypedBlock {
            stmts: vec![let_str("s", "inner"), let_move("t", "s")],
            ty: ManiType::Void,
        };
        let stmts = vec![
            let_str("s", "outer"),
            TypedStmt::Expr(TypedExpr {
                kind: TypedExprKind::Block(inner),
                ty: ManiType::Void,
                span: Span::new(1, 1),
            }),
            let_move("u", "s"),
        ];
        assert!(check_stmts(stmts).is_ok(), "inner-shadow move must not poison outer binding");
    }

    #[test]
    fn test_inner_let_does_not_launder_outer_move() {
        // S14 (converse): an inner-scope `let s` must not clear the OUTER
        // binding's moved flag.
        let inner = TypedBlock {
            stmts: vec![let_str("s", "inner")],
            ty: ManiType::Void,
        };
        let stmts = vec![
            let_str("s", "outer"),
            let_move("t", "s"),
            TypedStmt::Expr(TypedExpr {
                kind: TypedExprKind::Block(inner),
                ty: ManiType::Void,
                span: Span::new(1, 1),
            }),
            let_move("u", "s"),
        ];
        let result = check_stmts(stmts);
        assert!(result.is_err(), "outer move must survive an inner-scope shadow");
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("use of moved value: 's'"), "got: {}", msg);
    }

    #[test]
    fn test_reassign_after_move_is_ok() {
        // let s = "a"; let t = s; s = "b"; let u = s; -- OK (S13)
        let stmts = vec![
            let_str("s", "a"),
            let_move("t", "s"),
            TypedStmt::Assign(TypedAssignStmt {
                target: ident_expr("s", ManiType::Str),
                value: TypedExpr {
                    kind: TypedExprKind::Lit(Lit::Str("b".to_string())),
                    ty: ManiType::Str,
                    span: Span::new(3, 1),
                },
                op: None,
            }),
            let_move("u", "s"),
        ];
        assert!(check_stmts(stmts).is_ok(), "rebinding a moved variable must clear the move");
    }
}
