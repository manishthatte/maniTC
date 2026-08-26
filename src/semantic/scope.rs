use std::collections::HashMap;
use super::types::ManiType;

// ---------------------------------------------------------------------------
// Symbol table with scopes
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub(crate) struct SymbolInfo {
    pub ty: ManiType,
    pub mutable: bool,
}

pub(crate) struct Scope {
    pub(crate) symbols: HashMap<String, SymbolInfo>,
}

impl Scope {
    pub(crate) fn new() -> Self {
        Scope { symbols: HashMap::new() }
    }
}

pub(crate) struct SymbolTable {
    pub(crate) scopes: Vec<Scope>,
}

impl SymbolTable {
    pub(crate) fn new() -> Self {
        SymbolTable { scopes: vec![Scope::new()] }
    }

    pub(crate) fn push_scope(&mut self) {
        self.scopes.push(Scope::new());
    }

    /// How many scopes are open. P65: a monomorphisation is checked by
    /// re-entering `check_fn` from inside expression checking, and `check_fn`
    /// leaves its scope open when the body fails — harmless when analysis
    /// aborts, not harmless when the caller carries on. The instantiation
    /// records this depth and restores it either way.
    pub(crate) fn depth(&self) -> usize {
        self.scopes.len()
    }

    /// Discard scopes back to `d`, which must be one this table reached.
    pub(crate) fn truncate_to(&mut self, d: usize) {
        while self.scopes.len() > d.max(1) {
            self.scopes.pop();
        }
    }

    pub(crate) fn pop_scope(&mut self) {
        debug_assert!(self.scopes.len() > 1, "scope underflow — unmatched push/pop");
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    pub(crate) fn define(&mut self, name: &str, ty: ManiType, mutable: bool) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.symbols.insert(name.to_string(), SymbolInfo { ty, mutable });
        }
    }

    pub(crate) fn lookup(&self, name: &str) -> Option<&SymbolInfo> {
        for scope in self.scopes.iter().rev() {
            if let Some(info) = scope.symbols.get(name) {
                return Some(info);
            }
        }
        None
    }

    /// Like `lookup`, but also returns the scope depth of the binding
    /// (0 = module-level globals, >= 1 = function-local scopes).
    pub(crate) fn lookup_with_depth(&self, name: &str) -> Option<(usize, &SymbolInfo)> {
        for (depth, scope) in self.scopes.iter().enumerate().rev() {
            if let Some(info) = scope.symbols.get(name) {
                return Some((depth, info));
            }
        }
        None
    }

    pub(crate) fn all_names(&self) -> impl Iterator<Item = String> + '_ {
        self.scopes.iter().flat_map(|s| s.symbols.keys().cloned())
    }
}
