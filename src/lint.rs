//! Lint identity and severity levels.
//!
//! Recommendation A5. Before this module the compiler's strictness was a
//! global compile-time constant: a check either warned or, with
//! `--warn-as-error`, failed on the first warning of any kind. That made the
//! checker's severity a property of the BINARY rather than of the invocation,
//! and section 54 demonstrated the cost — turning 682 of 771 uncaught
//! mutations from warnings into errors invalidated every previous L1
//! measurement, because L1 is defined as "passes `manitc check`". The
//! model-training campaign then had to preserve the exact compiler binary that
//! scored a run, keyed by sha256, to keep results comparable at all.
//!
//! Two things follow, and this module provides both:
//!
//!   1. Severity is per-lint and settable per invocation (`--deny`, `--warn`,
//!      `--allow`, `--forbid`) and per module (a `lint` item in the source).
//!   2. The EFFECTIVE set is recorded in the artifact, so a compiled program
//!      says what it was checked for. A result becomes self-describing
//!      instead of needing a side-channel record of the compiler that made it.
//!
//! © Manish Jagdish Thatte

use crate::error::WarningKind;
use std::collections::BTreeMap;
use std::fmt;

/// How severely a lint is treated.
///
/// The ordering is the override ordering: a level may be raised freely, and
/// `Forbid` may not be lowered at all. That is the one asymmetry — it exists
/// so a compilation can pin a lint that a module cannot then opt out of, which
/// is what makes a recorded manifest trustworthy rather than advisory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LintLevel {
    /// Not reported at all.
    Allow,
    /// Reported; does not fail the compilation.
    Warn,
    /// Reported and fails the compilation.
    Deny,
    /// Reported, fails the compilation, and cannot be lowered afterwards.
    Forbid,
}

impl LintLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            LintLevel::Allow => "allow",
            LintLevel::Warn => "warn",
            LintLevel::Deny => "deny",
            LintLevel::Forbid => "forbid",
        }
    }

    pub fn from_name(s: &str) -> Option<LintLevel> {
        match s {
            "allow" => Some(LintLevel::Allow),
            "warn" => Some(LintLevel::Warn),
            "deny" => Some(LintLevel::Deny),
            "forbid" => Some(LintLevel::Forbid),
            _ => None,
        }
    }

    /// Whether a diagnostic at this level should fail the compilation.
    pub fn is_error(self) -> bool {
        matches!(self, LintLevel::Deny | LintLevel::Forbid)
    }
}

impl fmt::Display for LintLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Every lint the compiler knows, with its stable name and default level.
///
/// The name is the user-facing identity and is part of the interface: it
/// appears in `--deny <name>`, in a `lint deny(<name>);` item, and in the
/// recorded manifest. Renaming one is a breaking change to every build script
/// and every recorded artifact that mentions it.
///
/// Defaults are chosen so that this table's introduction changes NO existing
/// diagnostic. Every lint that existed before A5 defaults to `warn`, which is
/// what it was; every lint added alongside it defaults to `allow` unless it
/// reports a condition that was already an error.
pub const LINTS: &[(WarningKind, &str, LintLevel)] = &[
    (WarningKind::UnusedVariable, "unused-variable", LintLevel::Warn),
    (WarningKind::UnusedFunction, "unused-function", LintLevel::Warn),
    (WarningKind::Shadowing, "shadowing", LintLevel::Warn),
    (WarningKind::UnreachableCode, "unreachable-code", LintLevel::Warn),
    (WarningKind::IntegerOverflow, "integer-overflow", LintLevel::Warn),
    (WarningKind::DivisionByZero, "division-by-zero", LintLevel::Warn),
    (WarningKind::UnknownType, "unknown-type", LintLevel::Warn),
    // A1 step 1. The migration backlog: every native reachable without an
    // `extern` declaration. Defaults to `allow` because today that is EVERY
    // native in the standard library — 413 of them — so warning by default
    // would bury every real diagnostic under the backlog. Turn it on with
    // `--warn undeclared-native` to generate the backlog on demand; that
    // generated list is the migration plan.
    (WarningKind::UndeclaredNative, "undeclared-native", LintLevel::Allow),
    // Calling an extern whose declaration carries `deprecated("...")`.
    (WarningKind::DeprecatedNative, "deprecated-native", LintLevel::Warn),
    // A1 step 3, not yet enforced: calling an extern not `available` on the
    // selected backend. Recorded here so the manifest names it and so the
    // level can be raised experimentally before step 3 makes it the default.
    (WarningKind::BackendUnavailable, "backend-unavailable", LintLevel::Allow),
    // A2. Deny by default, unlike its call-site cousin above.
    //
    // `backend-unavailable` is `allow` because it is the A1 step-3 BACKLOG: it
    // fires on every call to an extern that has not yet been declared
    // available for the target, and most of those are declarations nobody has
    // written yet rather than real problems.
    //
    // This one is different in kind. It fires only when an `available(...)`
    // clause someone actually WROTE is contradicted by a call chain that
    // actually EXISTS, for the backend being compiled RIGHT NOW. That is not a
    // backlog item, it is a program that cannot run — and the whole point of
    // A2 is that discovering it should not require compiling, running, and
    // diffing against the other backend.
    (WarningKind::BackendUnavailableChain, "backend-unavailable-chain", LintLevel::Deny),
    // C4/R2 step 2. The migration backlog for the division change: every `/`
    // and `%` on an integer type, which is to say every site whose meaning
    // depends on which language version it is compiled under.
    //
    // `allow` by default, and for A1's reason rather than out of timidity —
    // integer division is everywhere, so warning by default would bury every
    // real diagnostic under a list of sites that are almost all fine.
    // `--warn division-semantics` generates the list on demand, and that
    // generated list IS the migration plan. This is the pattern
    // `undeclared-native` established, and it is the established answer here
    // to "a change you must find every site of before you make it".
    (WarningKind::DivisionSemantics, "division-semantics", LintLevel::Allow),
    // B1/A4. A generic argument that does not satisfy a declared trait bound.
    // Defaults to `deny`: an unsatisfied bound is the soundness hole A4
    // describes, and no shipped program declares a bound yet, so denying it
    // cannot break code that exists.
    (WarningKind::UnsatisfiedBound, "unsatisfied-bound", LintLevel::Deny),
    // N5/R2, report.txt P21 cluster 1. An `int` literal too wide for the
    // 27-trit word — the migration backlog for the change v2 already made.
    //
    // `allow` by default, and for `undeclared-native`'s reason rather than out
    // of timidity: under v1 `int` IS the host word on LLVM, so such a literal
    // is legal there and warning by default would report working v1 programs.
    // `--warn literal-out-of-word` generates the list on demand and that list
    // is the migration plan.
    //
    // The level applies only under v1. Under v2 this is not a lint at all:
    // `int` means 27 trits, the literal has no value, and the analyzer rejects
    // it the way it rejects any literal that does not fit its type. A lint
    // level cannot allow away a value that does not exist.
    (WarningKind::LiteralOutOfWord, "literal-out-of-word", LintLevel::Allow),
    // P70. A `struct` or `enum` declared under a name the type resolver
    // answers before it ever consults the struct table.
    //
    // `deny` by default, and it is the only level that reports it at all — see
    // the note in `analyzer::reserved_type_name`. The reason is not severity
    // in the abstract: a warning is DROPPED when the compilation also errors,
    // and every observable case of this defect errors, so a `warn` default
    // would be silent in exactly the case that motivated the lint.
    //
    // `allow` is an exact restoration of the pre-P70 compiler rather than a
    // softening, which is what makes it a safe escape hatch: nothing is
    // checked differently, the declaration simply stops being reported.
    // `stdlib/collections.mt` and `stdlib/sync.mt` use it because their
    // declarations ARE the built-ins this lint protects.
    (WarningKind::ReservedTypeName, "reserved-type-name", LintLevel::Deny),
];

/// Resolve a lint name to its kind. `None` for an unknown name.
pub fn lint_by_name(name: &str) -> Option<WarningKind> {
    LINTS.iter().find(|(_, n, _)| *n == name).map(|(k, _, _)| k.clone())
}

/// The stable name of a lint.
pub fn lint_name(kind: &WarningKind) -> &'static str {
    LINTS
        .iter()
        .find(|(k, _, _)| k == kind)
        .map(|(_, n, _)| *n)
        .unwrap_or("unknown-lint")
}

/// Every known lint name, for diagnostics that list the alternatives.
pub fn all_lint_names() -> Vec<&'static str> {
    LINTS.iter().map(|(_, n, _)| *n).collect()
}

/// The effective severity of every lint for one compilation.
#[derive(Debug, Clone)]
pub struct LintTable {
    levels: BTreeMap<String, LintLevel>,
    /// Names set to `forbid`, which later `set` calls may not lower.
    forbidden: BTreeMap<String, ()>,
}

impl Default for LintTable {
    fn default() -> Self {
        LintTable {
            levels: LINTS
                .iter()
                .map(|(_, n, lvl)| (n.to_string(), *lvl))
                .collect(),
            forbidden: BTreeMap::new(),
        }
    }
}

impl LintTable {
    pub fn new() -> Self {
        Self::default()
    }

    /// The effective level of a lint.
    pub fn level(&self, kind: &WarningKind) -> LintLevel {
        self.levels
            .get(lint_name(kind))
            .copied()
            .unwrap_or(LintLevel::Warn)
    }

    /// Set one lint's level by name.
    ///
    /// Returns `Err` with a message if the name is unknown, or if the lint was
    /// forbidden and this call would lower it. Both are reported rather than
    /// silently ignored: a typo in `--deny unusd-variable` that quietly did
    /// nothing would be the same class of defect as the one A5 exists to fix.
    pub fn set(&mut self, name: &str, level: LintLevel) -> Result<(), String> {
        if lint_by_name(name).is_none() {
            let mut names = all_lint_names();
            names.sort_unstable();
            return Err(format!(
                "unknown lint '{}'; known lints: {}",
                name,
                names.join(", ")
            ));
        }
        if self.forbidden.contains_key(name) && level < LintLevel::Forbid {
            return Err(format!(
                "lint '{}' is forbidden and cannot be set to '{}'",
                name, level
            ));
        }
        if level == LintLevel::Forbid {
            self.forbidden.insert(name.to_string(), ());
        }
        self.levels.insert(name.to_string(), level);
        Ok(())
    }

    /// Raise every lint to at least `Deny`.
    ///
    /// This is what `--warn-as-error` means once lints have levels, and it is
    /// kept because that flag is what section 54's strict binary was built
    /// with — dropping it would break every recorded invocation.
    pub fn deny_all(&mut self) {
        let names: Vec<String> = self.levels.keys().cloned().collect();
        for n in names {
            let cur = self.levels[&n];
            if cur < LintLevel::Deny {
                self.levels.insert(n, LintLevel::Deny);
            }
        }
    }

    /// The manifest recorded in the artifact.
    ///
    /// One line, stable field order, every lint present whether or not its
    /// level was changed. Recording only the deltas would make the manifest
    /// unreadable without also knowing the defaults of the compiler that
    /// produced it, which is the coupling A5 exists to remove.
    pub fn manifest(&self) -> String {
        let mut out = String::from("manitc-lints v1");
        out.push_str(&format!(" compiler={}", env!("CARGO_PKG_VERSION")));
        for (name, level) in &self.levels {
            out.push_str(&format!(" {}={}", name, level));
        }
        out
    }

    /// The manifest as a multi-line human-readable block.
    pub fn manifest_lines(&self) -> Vec<String> {
        let mut out = vec![format!(
            "manitc-lints v1 (compiler {})",
            env!("CARGO_PKG_VERSION")
        )];
        for (name, level) in &self.levels {
            out.push(format!("  {:<22} {}", name, level));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_lint_has_a_unique_name() {
        let mut names = all_lint_names();
        let total = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), total, "two lints share a name");
    }

    #[test]
    fn names_round_trip_through_the_table() {
        for (kind, name, _) in LINTS {
            assert_eq!(lint_name(kind), *name);
            assert_eq!(lint_by_name(name).as_ref(), Some(kind));
        }
    }

    #[test]
    fn default_levels_match_the_table() {
        let t = LintTable::new();
        for (kind, _, default) in LINTS {
            assert_eq!(t.level(kind), *default);
        }
    }

    #[test]
    fn an_unknown_lint_name_is_an_error_not_a_no_op() {
        let mut t = LintTable::new();
        let err = t.set("unusd-variable", LintLevel::Deny).unwrap_err();
        assert!(err.contains("unknown lint"), "{}", err);
        assert!(err.contains("unused-variable"), "should list the real names: {}", err);
    }

    #[test]
    fn forbid_cannot_be_lowered() {
        let mut t = LintTable::new();
        t.set("shadowing", LintLevel::Forbid).unwrap();
        let err = t.set("shadowing", LintLevel::Allow).unwrap_err();
        assert!(err.contains("forbidden"), "{}", err);
        assert_eq!(t.level(&WarningKind::Shadowing), LintLevel::Forbid);
    }

    #[test]
    fn deny_all_raises_but_does_not_lower_forbid() {
        let mut t = LintTable::new();
        t.set("shadowing", LintLevel::Forbid).unwrap();
        t.deny_all();
        assert_eq!(t.level(&WarningKind::Shadowing), LintLevel::Forbid);
        assert_eq!(t.level(&WarningKind::UnusedVariable), LintLevel::Deny);
        // Even an allow-by-default lint is raised: --warn-as-error means all.
        assert_eq!(t.level(&WarningKind::UndeclaredNative), LintLevel::Deny);
    }

    #[test]
    fn the_manifest_names_every_lint_and_its_level() {
        let mut t = LintTable::new();
        t.set("shadowing", LintLevel::Deny).unwrap();
        let m = t.manifest();
        assert!(m.starts_with("manitc-lints v1 "), "{}", m);
        assert!(m.contains("shadowing=deny"), "{}", m);
        for name in all_lint_names() {
            assert!(m.contains(&format!("{}=", name)), "manifest omits {}: {}", name, m);
        }
    }
}
