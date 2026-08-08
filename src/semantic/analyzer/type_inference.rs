// semantic/analyzer/type_inference.rs — Type inference helpers for literals, operators, fields, methods.
use super::*;

impl SemanticAnalyzer {
    pub(super) fn infer_lit_type(&self, lit: &Lit, hint: Option<&ManiType>) -> ManiType {
        match lit {
            Lit::Int(_) => {
                if let Some(h) = hint {
                    match h {
                        ManiType::Float => ManiType::Float,
                        ManiType::Trit => ManiType::Trit,
                        ManiType::Tryte => ManiType::Tryte,
                        ManiType::T9 => ManiType::T9,
                        ManiType::T27 => ManiType::T27,
                        ManiType::T54 => ManiType::T54,
                        _ => ManiType::Int,
                    }
                } else {
                    ManiType::Int
                }
            }
            Lit::Float(_) => ManiType::Float,
            Lit::Str(_) => ManiType::Str,
            Lit::Char(_) => ManiType::Char,
            Lit::Bool(_) => {
                // Coerce True/False to bool3 when the context demands it
                if matches!(hint, Some(ManiType::Bool3)) { ManiType::Bool3 } else { ManiType::Bool }
            }
            Lit::Bool3(_) => ManiType::Bool3,
            Lit::Trit(_) => ManiType::Trit,
            Lit::TernaryInt(_) => ManiType::Int,
            Lit::Null => ManiType::Unknown,
        }
    }

    pub(super) fn binop_type(&self, op: &BinOpKind, lhs: &ManiType, rhs: &ManiType, _span: Span) -> CompileResult<ManiType> {
        match op {
            BinOpKind::Add | BinOpKind::Sub | BinOpKind::Mul | BinOpKind::Div | BinOpKind::Rem => {
                // Numeric result
                match (lhs, rhs) {
                    (ManiType::Float, _) | (_, ManiType::Float) => Ok(ManiType::Float),
                    (ManiType::Trit, ManiType::Trit) => Ok(ManiType::Trit),
                    (ManiType::Unknown, other) | (other, ManiType::Unknown) => Ok(other.clone()),
                    _ => Ok(lhs.clone()),
                }
            }
            BinOpKind::Eq | BinOpKind::Ne | BinOpKind::Lt | BinOpKind::Gt
            | BinOpKind::Le | BinOpKind::Ge => Ok(ManiType::Bool),
            BinOpKind::And | BinOpKind::Or => Ok(ManiType::Bool),
            BinOpKind::BitAnd | BinOpKind::BitOr | BinOpKind::BitXor => Ok(lhs.clone()),
            BinOpKind::LShift | BinOpKind::RShift => Ok(lhs.clone()),
            BinOpKind::Tand | BinOpKind::Tor | BinOpKind::Txor
            | BinOpKind::Tcon | BinOpKind::Tany => {
                // Preserve bool3 if both operands are bool3; otherwise trit
                match (lhs, rhs) {
                    (ManiType::Bool3, ManiType::Bool3) => Ok(ManiType::Bool3),
                    _ => Ok(ManiType::Trit),
                }
            }
            BinOpKind::Range | BinOpKind::RangeInclusive => {
                Ok(ManiType::Generic("Range".to_string(), vec![lhs.clone()]))
            }
        }
    }

    pub(super) fn unop_type(&self, op: &UnOpKind, operand: &ManiType, _span: Span) -> CompileResult<ManiType> {
        match op {
            UnOpKind::Neg | UnOpKind::TritNeg => Ok(operand.clone()),
            UnOpKind::Not => Ok(ManiType::Bool),
            UnOpKind::Tnot => {
                if *operand == ManiType::Bool3 { Ok(ManiType::Bool3) } else { Ok(ManiType::Trit) }
            }
            UnOpKind::Deref | UnOpKind::Ref => Ok(operand.clone()),
        }
    }

    pub(super) fn resolve_field_type(&self, obj_ty: &ManiType, field: &str) -> ManiType {
        if let ManiType::Struct(name) = obj_ty {
            if let Some(fields) = self.structs.get(name.as_str()) {
                for (fname, fty) in fields {
                    if fname == field {
                        return fty.clone();
                    }
                }
                // Field not found — emit a "did you mean?" hint
                let candidates = fields.iter().map(|(n, _)| n.clone());
                if let Some(hint) = did_you_mean(field, candidates) {
                    eprintln!("note: unknown field '{}' on '{}'{}", field, name, hint);
                }
            }
        }
        ManiType::Unknown
    }

    pub(super) fn resolve_method_type(&self, obj_ty: &ManiType, method: &str) -> ManiType {
        // Check user-defined impl methods first (before built-in fallbacks)
        let type_name_str = match obj_ty {
            ManiType::Struct(n) | ManiType::Enum(n) => Some(n.as_str()),
            _ => None,
        };
        if let Some(tn) = type_name_str {
            if let Some(type_methods) = self.user_method_types.get(tn) {
                if let Some(ret_ty) = type_methods.get(method) {
                    return ret_ty.clone();
                }
            }
        }

        // Built-in methods
        match (obj_ty, method) {
            (ManiType::Str, "len") => ManiType::Int,
            (ManiType::Str, "concat") => ManiType::Str,
            (ManiType::Str, "contains") => ManiType::Bool,
            (ManiType::Str, "find") => ManiType::Int,
            (ManiType::Str, "trim") => ManiType::Str,
            (ManiType::Str, "replace") => ManiType::Str,
            (ManiType::Str, "slice") => ManiType::Str,
            (ManiType::Str, "to_int") => ManiType::Int,
            (ManiType::Str, "split") => ManiType::Generic("Vec".to_string(), vec![ManiType::Str]),
            (ManiType::Str, "char_at") => ManiType::Char,
            (ManiType::Int, "to_str") => ManiType::Str,
            (ManiType::Float, "to_str") => ManiType::Str,
            (ManiType::Generic(name, _), "len") if name == "Vec" => ManiType::Int,
            (ManiType::Generic(name, args), "unwrap") if name == "Option" || name == "Result" => {
                args.first().cloned().unwrap_or(ManiType::Unknown)
            }
            (ManiType::Generic(name, args), "map") if name == "Option" || name == "Result" => {
                ManiType::Generic(name.clone(), args.clone())
            }
            (ManiType::Array(_, _), "len") => ManiType::Int,

            // Vec methods
            (ManiType::Generic(name, _), "push") if name == "Vec" => ManiType::Void,
            (ManiType::Generic(name, args), "pop") if name == "Vec" => {
                args.first().cloned().unwrap_or(ManiType::Unknown)
            }
            (ManiType::Generic(name, args), "get") if name == "Vec" => {
                args.first().cloned().unwrap_or(ManiType::Unknown)
            }
            (ManiType::Generic(name, _), "set") if name == "Vec" => ManiType::Void,
            (ManiType::Generic(name, _), "is_empty") if name == "Vec" => ManiType::Bool,
            (ManiType::Generic(name, _), "clear") if name == "Vec" => ManiType::Void,
            (ManiType::Generic(name, _), "contains") if name == "Vec" => ManiType::Bool,
            (ManiType::Generic(name, _), "sort") if name == "Vec" => ManiType::Void,
            (ManiType::Generic(name, _), "reverse") if name == "Vec" => ManiType::Void,
            (ManiType::Generic(name, _), "for_each") if name == "Vec" => ManiType::Void,
            (ManiType::Generic(name, args), "map") if name == "Vec" => {
                ManiType::Generic("Vec".to_string(), args.clone())
            }
            (ManiType::Generic(name, args), "filter") if name == "Vec" => {
                ManiType::Generic("Vec".to_string(), args.clone())
            }
            (ManiType::Generic(name, args), "slice") if name == "Vec" => {
                ManiType::Generic("Vec".to_string(), args.clone())
            }
            (ManiType::Generic(name, _), "index_of") if name == "Vec" => ManiType::Int,
            (ManiType::Generic(name, _), "fold") if name == "Vec" => ManiType::Int,

            // Map methods
            (ManiType::Generic(name, _), "insert") if name == "Map" => ManiType::Void,
            (ManiType::Generic(name, args), "get") if name == "Map" => {
                // Map<K,V> — return value type (second arg)
                args.get(1).cloned().unwrap_or(ManiType::Unknown)
            }
            (ManiType::Generic(name, args), "get_or") if name == "Map" => {
                args.get(1).cloned().unwrap_or(ManiType::Unknown)
            }
            (ManiType::Generic(name, args), "keys") if name == "Map" => {
                let key_ty = args.first().cloned().unwrap_or(ManiType::Str);
                ManiType::Generic("Vec".to_string(), vec![key_ty])
            }
            (ManiType::Generic(name, _), "contains_key") if name == "Map" => ManiType::Bool,
            (ManiType::Generic(name, _), "remove") if name == "Map" => ManiType::Void,
            (ManiType::Generic(name, _), "len") if name == "Map" => ManiType::Int,
            (ManiType::Generic(name, _), "is_empty") if name == "Map" => ManiType::Bool,

            // Set methods
            (ManiType::Generic(name, _), "insert") if name == "Set" => ManiType::Void,
            (ManiType::Generic(name, _), "contains") if name == "Set" => ManiType::Bool,
            (ManiType::Generic(name, _), "remove") if name == "Set" => ManiType::Void,
            (ManiType::Generic(name, _), "len") if name == "Set" => ManiType::Int,
            (ManiType::Generic(name, _), "for_each") if name == "Set" => ManiType::Void,
            (ManiType::Generic(name, _), "is_subset") if name == "Set" => ManiType::Bool,
            (ManiType::Generic(name, _), "is_superset") if name == "Set" => ManiType::Bool,
            (ManiType::Generic(name, _), "is_disjoint") if name == "Set" => ManiType::Bool,
            (ManiType::Generic(name, args), "intersection") if name == "Set" => {
                ManiType::Generic("Set".to_string(), args.clone())
            }
            (ManiType::Generic(name, args), "union") if name == "Set" => {
                ManiType::Generic("Set".to_string(), args.clone())
            }
            (ManiType::Generic(name, args), "difference") if name == "Set" => {
                ManiType::Generic("Set".to_string(), args.clone())
            }

            // Deque methods
            (ManiType::Generic(name, _), "push_front") if name == "Deque" => ManiType::Void,
            (ManiType::Generic(name, _), "push_back") if name == "Deque" => ManiType::Void,
            (ManiType::Generic(name, args), "pop_front") if name == "Deque" => {
                args.first().cloned().unwrap_or(ManiType::Unknown)
            }
            (ManiType::Generic(name, args), "pop_back") if name == "Deque" => {
                args.first().cloned().unwrap_or(ManiType::Unknown)
            }
            (ManiType::Generic(name, _), "len") if name == "Deque" => ManiType::Int,
            (ManiType::Generic(name, _), "is_empty") if name == "Deque" => ManiType::Bool,
            (ManiType::Generic(name, _), "contains") if name == "Deque" => ManiType::Bool,
            (ManiType::Generic(name, args), "front") if name == "Deque" => {
                args.first().cloned().unwrap_or(ManiType::Unknown)
            }
            (ManiType::Generic(name, args), "back") if name == "Deque" => {
                args.first().cloned().unwrap_or(ManiType::Unknown)
            }

            // Channel methods
            (ManiType::Generic(name, _), "send") if name == "Channel" => ManiType::Void,
            (ManiType::Generic(name, args), "recv") if name == "Channel" => {
                args.first().cloned().unwrap_or(ManiType::Unknown)
            }
            (ManiType::Generic(name, _), "try_recv") if name == "Channel" => ManiType::Unknown,
            (ManiType::Generic(name, _), "close") if name == "Channel" => ManiType::Void,
            (ManiType::Generic(name, _), "len") if name == "Channel" => ManiType::Int,

            // Mutex methods
            (ManiType::Generic(name, args), "lock") if name == "Mutex" => {
                ManiType::Generic("MutexGuard".to_string(), args.clone())
            }
            (ManiType::Generic(name, _), "unlock") if name == "Mutex" => ManiType::Void,
            (ManiType::Generic(name, args), "get") if name == "Mutex" || name == "MutexGuard" => {
                args.first().cloned().unwrap_or(ManiType::Unknown)
            }
            (ManiType::Generic(name, _), "update") if name == "MutexGuard" => ManiType::Void,
            (ManiType::Generic(name, _), "unlock") if name == "MutexGuard" => ManiType::Void,

            // AtomicTrit methods
            (ManiType::Struct(name), "load") if name == "AtomicTrit" => ManiType::Trit,
            (ManiType::Struct(name), "store") if name == "AtomicTrit" => ManiType::Void,

            // Barrier methods
            (ManiType::Struct(name), "wait") if name == "Barrier" => ManiType::Bool,

            // Semaphore methods
            (ManiType::Struct(name), "acquire") if name == "Semaphore" => ManiType::Void,
            (ManiType::Struct(name), "release") if name == "Semaphore" => ManiType::Void,

            // Task methods
            (ManiType::Generic(name, args), "join") if name == "Task" => {
                args.first().cloned().unwrap_or(ManiType::Unknown)
            }

            // Future methods (block_on, select result)
            (ManiType::Unknown, "block_on") => ManiType::Tuple(vec![ManiType::Int, ManiType::Unknown]),
            (ManiType::Generic(_name, args), "block_on") => {
                ManiType::Tuple(vec![ManiType::Int, args.first().cloned().unwrap_or(ManiType::Unknown)])
            }

            // TernaryTrie methods
            (ManiType::Generic(name, _), "insert") if name == "TernaryTrie" => ManiType::Void,
            (ManiType::Generic(name, args), "get") if name == "TernaryTrie" => {
                args.first().cloned().unwrap_or(ManiType::Unknown)
            }
            (ManiType::Generic(name, _), "len") if name == "TernaryTrie" => ManiType::Int,
            (ManiType::Generic(name, _), "contains_key") | (ManiType::Generic(name, _), "contains") if name == "TernaryTrie" => ManiType::Bool,
            (ManiType::Generic(name, _), "for_each") if name == "TernaryTrie" => ManiType::Void,
            (ManiType::Generic(name, _), "keys") if name == "TernaryTrie" => {
                ManiType::Generic("Vec".to_string(), vec![ManiType::Unknown])
            }
            (ManiType::Generic(name, _), "keys_with_prefix") if name == "TernaryTrie" => {
                ManiType::Generic("Vec".to_string(), vec![ManiType::Unknown])
            }

            // Catch-all for unknown method calls on Generic (collection) types — permissive
            (ManiType::Generic(_, _), _) => ManiType::Int,

            _ => ManiType::Unknown,
        }
    }

    pub(super) fn iter_elem_type(&self, iter_ty: &ManiType) -> ManiType {
        match iter_ty {
            ManiType::Array(elem, _) => *elem.clone(),
            ManiType::Generic(name, args) if name == "Vec" || name == "Range" => {
                args.first().cloned().unwrap_or(ManiType::Unknown)
            }
            _ => ManiType::Unknown,
        }
    }

    // ---------------------------------------------------------------------------
    // Ternary type range helpers
    // ---------------------------------------------------------------------------

    // (moved to module-level function below)

    // ---------------------------------------------------------------------------
    // Feature 1: Match exhaustiveness checking
    // ---------------------------------------------------------------------------

    /// Return an error if a match expression is not exhaustive.
    /// Checks enum types (all variants), trit (+, 0, -), and bool3 (True, Unknown, False).
    pub(super) fn check_exhaustiveness(
        &self,
        scrutinee_ty: &ManiType,
        arms: &[TypedMatchArm],
        span: Span,
    ) -> CompileResult<()> {
        // Wildcard or variable binding arm = always exhaustive
        let has_catch_all = arms.iter().any(|arm| {
            matches!(&arm.pattern, Pattern::Wildcard(_) | Pattern::Ident(_, _))
        });
        if has_catch_all {
            return Ok(());
        }

        match scrutinee_ty {
            ManiType::Enum(enum_name) => {
                let all_variants = match self.enums.get(enum_name.as_str()) {
                    Some(v) => v,
                    None => return Ok(()), // unknown enum, skip
                };
                let mut covered = std::collections::HashSet::new();
                for arm in arms {
                    if let Pattern::Enum(variant, _, _, _) = &arm.pattern {
                        covered.insert(variant.as_str());
                    }
                }
                let missing: Vec<&str> = all_variants.iter()
                    .filter(|(name, _)| !covered.contains(name.as_str()))
                    .map(|(name, _)| name.as_str())
                    .collect();
                if !missing.is_empty() {
                    return Err(self.err(span, format!(
                        "non-exhaustive match on enum '{}' — missing variants: {}. Add the missing arms or a wildcard '_' pattern",
                        enum_name, missing.join(", ")
                    )));
                }
            }
            ManiType::Trit => {
                // Must cover all three trit values: +1, 0, -1
                let mut has_pos = false;
                let mut has_zero = false;
                let mut has_neg = false;
                for arm in arms {
                    match &arm.pattern {
                        Pattern::Lit(Lit::Trit(1), _) | Pattern::Lit(Lit::Int(1), _) => has_pos = true,
                        Pattern::Lit(Lit::Trit(0), _) | Pattern::Lit(Lit::Int(0), _) => has_zero = true,
                        Pattern::Lit(Lit::Trit(-1), _) | Pattern::Lit(Lit::Int(-1), _) => has_neg = true,
                        Pattern::Lit(Lit::Trit(v), _) => {
                            match v { 1 => has_pos = true, 0 => has_zero = true, -1 => has_neg = true, _ => {} }
                        }
                        _ => {}
                    }
                }
                let mut missing = Vec::new();
                if !has_pos { missing.push("+1"); }
                if !has_zero { missing.push("0"); }
                if !has_neg { missing.push("-1"); }
                if !missing.is_empty() {
                    return Err(self.err(span, format!(
                        "non-exhaustive match on trit — missing values: {}. Add the missing arms or a wildcard '_' pattern",
                        missing.join(", ")
                    )));
                }
            }
            ManiType::Bool3 => {
                // Must cover True (+1), Unknown (0), False (-1)
                let mut has_true = false;
                let mut has_unknown = false;
                let mut has_false = false;
                for arm in arms {
                    match &arm.pattern {
                        Pattern::Lit(Lit::Bool3(1), _) => has_true = true,
                        Pattern::Lit(Lit::Bool3(0), _) => has_unknown = true,
                        Pattern::Lit(Lit::Bool3(-1), _) => has_false = true,
                        _ => {}
                    }
                }
                let mut missing = Vec::new();
                if !has_true { missing.push("True"); }
                if !has_unknown { missing.push("Unknown"); }
                if !has_false { missing.push("False"); }
                if !missing.is_empty() {
                    return Err(self.err(span, format!(
                        "non-exhaustive match on bool3 — missing values: {}. Add the missing arms or a wildcard '_' pattern",
                        missing.join(", ")
                    )));
                }
            }
            ManiType::Bool => {
                // Must cover true and false
                let mut has_true = false;
                let mut has_false = false;
                for arm in arms {
                    match &arm.pattern {
                        Pattern::Lit(Lit::Bool(true), _) => has_true = true,
                        Pattern::Lit(Lit::Bool(false), _) => has_false = true,
                        _ => {}
                    }
                }
                if !has_true || !has_false {
                    let mut missing = Vec::new();
                    if !has_true { missing.push("true"); }
                    if !has_false { missing.push("false"); }
                    return Err(self.err(span, format!(
                        "non-exhaustive match on bool — missing values: {}. Add the missing arms or a wildcard '_' pattern",
                        missing.join(", ")
                    )));
                }
            }
            // For other types (int, str, etc.), exhaustiveness is not checkable
            _ => {}
        }
        Ok(())
    }

    // ---------------------------------------------------------------------------
    // Feature 2: Generic trait bound enforcement
    // ---------------------------------------------------------------------------

    /// Check that `concrete_type` implements `trait_name`.
    /// Returns an error if the bound is not satisfied.
    #[allow(dead_code)]
    pub(super) fn check_trait_bound(
        &self,
        concrete_type: &str,
        trait_name: &str,
        span: Span,
    ) -> CompileResult<()> {
        // If we have no record of this trait, allow (could be unknown/stdlib)
        if !self.trait_defs.contains_key(trait_name) {
            return Ok(());
        }
        if !self.trait_impls.contains(&(concrete_type.to_string(), trait_name.to_string())) {
            return Err(self.err(
                span,
                format!(
                    "type '{}' does not implement trait '{}' required by generic parameter",
                    concrete_type, trait_name
                ),
            ));
        }
        Ok(())
    }
}

impl Default for SemanticAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Ternary type range helpers (module-level)
// ---------------------------------------------------------------------------

/// Returns (min, max) range for balanced ternary types, or None for non-ternary types.
pub(super) fn ternary_type_range(ty: &ManiType) -> Option<(i64, i64)> {
    match ty {
        ManiType::Trit => Some((-1, 1)),
        ManiType::Tryte => Some((-364, 364)),             // 6 trits: -(3^6-1)/2 .. (3^6-1)/2
        ManiType::T9 => Some((-9841, 9841)),             // 9 trits: -(3^9-1)/2 .. (3^9-1)/2
        ManiType::T27 => Some((-3_812_798_742_493, 3_812_798_742_493)), // 27 trits
        ManiType::T54 => None, // 54 trits fits in i64 but range is nearly full — skip
        _ => None,
    }
}
