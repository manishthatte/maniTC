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

    /// Render a binary operator for diagnostics.
    fn binop_symbol(op: &BinOpKind) -> &'static str {
        match op {
            BinOpKind::Add => "+", BinOpKind::Sub => "-", BinOpKind::Mul => "*",
            BinOpKind::Div => "/", BinOpKind::Rem => "%",
            BinOpKind::Eq => "==", BinOpKind::Ne => "!=",
            BinOpKind::Lt => "<", BinOpKind::Gt => ">",
            BinOpKind::Le => "<=", BinOpKind::Ge => ">=",
            BinOpKind::And => "&&", BinOpKind::Or => "||",
            BinOpKind::BitAnd => "&", BinOpKind::BitOr => "|", BinOpKind::BitXor => "^",
            BinOpKind::LShift => "<<", BinOpKind::RShift => ">>",
            BinOpKind::Tand => "tand", BinOpKind::Tor => "tor", BinOpKind::Txor => "txor",
            BinOpKind::Tcon => "tcon", BinOpKind::Tany => "tany",
            BinOpKind::Timp => "timp", BinOpKind::Teq => "teq",
            BinOpKind::Tandw => "tandw", BinOpKind::Torw => "torw",
            BinOpKind::Txorw => "txorw", BinOpKind::Timpw => "timpw",
            BinOpKind::Tcmpw => "tcmpw",
            BinOpKind::Range => "..", BinOpKind::RangeInclusive => "..=",
        }
    }

    fn binop_operand_err(&self, op: &BinOpKind, lhs: &ManiType, rhs: &ManiType, span: Span) -> CompileError {
        self.err(span, format!(
            "invalid operands: operator `{}` cannot be applied to `{}` and `{}`",
            Self::binop_symbol(op), lhs.display(), rhs.display()
        ))
    }

    /// Infer (and, for KNOWN operand types, enforce) the result type of a
    /// binary operation. Operands of type `Unknown` are always accepted —
    /// permissive-on-Unknown is the crate's design rule.
    pub(super) fn binop_type(&self, op: &BinOpKind, lhs: &ManiType, rhs: &ManiType, span: Span) -> CompileResult<ManiType> {
        use ManiType::Unknown;
        let both_known = lhs.is_known() && rhs.is_known();
        match op {
            BinOpKind::Add | BinOpKind::Sub | BinOpKind::Mul | BinOpKind::Div | BinOpKind::Rem => {
                // String concatenation: `str + str`.
                if matches!(op, BinOpKind::Add)
                    && *lhs == ManiType::Str && *rhs == ManiType::Str
                {
                    return Ok(ManiType::Str);
                }
                // Arithmetic requires numeric operands.
                if (lhs.is_known() && !lhs.is_numeric()) || (rhs.is_known() && !rhs.is_numeric()) {
                    return Err(self.binop_operand_err(op, lhs, rhs, span));
                }
                // Numeric result
                match (lhs, rhs) {
                    (ManiType::Float, _) | (_, ManiType::Float) => Ok(ManiType::Float),
                    (ManiType::Trit, ManiType::Trit) => Ok(ManiType::Trit),
                    (Unknown, other) | (other, Unknown) => Ok(other.clone()),
                    _ => Ok(lhs.clone()),
                }
            }
            BinOpKind::Lt | BinOpKind::Gt | BinOpKind::Le | BinOpKind::Ge => {
                if (lhs.is_known() && !lhs.is_comparable())
                    || (rhs.is_known() && !rhs.is_comparable())
                    || (both_known && !types_compatible(lhs, rhs))
                {
                    return Err(self.binop_operand_err(op, lhs, rhs, span));
                }
                Ok(ManiType::Bool)
            }
            BinOpKind::Eq | BinOpKind::Ne => {
                if both_known && !types_compatible(lhs, rhs) {
                    return Err(self.binop_operand_err(op, lhs, rhs, span));
                }
                Ok(ManiType::Bool)
            }
            BinOpKind::And | BinOpKind::Or => {
                let ok = |t: &ManiType| matches!(t, ManiType::Bool | Unknown);
                if !ok(lhs) || !ok(rhs) {
                    return Err(self.binop_operand_err(op, lhs, rhs, span));
                }
                Ok(ManiType::Bool)
            }
            BinOpKind::BitAnd | BinOpKind::BitOr | BinOpKind::BitXor
            | BinOpKind::LShift | BinOpKind::RShift => {
                // Integer (binary or balanced-ternary) operands only.
                let ok = |t: &ManiType| !t.is_known() || (t.is_numeric() && *t != ManiType::Float);
                if !ok(lhs) || !ok(rhs) {
                    return Err(self.binop_operand_err(op, lhs, rhs, span));
                }
                Ok(lhs.clone())
            }
            BinOpKind::Tand | BinOpKind::Tor | BinOpKind::Txor
            | BinOpKind::Tcon | BinOpKind::Tany
            | BinOpKind::Timp | BinOpKind::Teq => {
                // Ternary logic operates on ternary-valued types — and on
                // `bool`, which the lowering converts to `bool3` first (see
                // IRLowerer::lower_ternary_operand).
                //
                // Refusing `bool` here made `a > b tand c > d` unwritable, since
                // a comparison produces `bool` and nothing produces a trit.
                // Four real sources did not compile, two of them in thatteOS.
                // The `bool` to `bool3` coercion this needs was already blessed
                // by the language and already emitted by `coerce_value`; the
                // type-checker was refusing an operand pair the lowering knew
                // how to build.
                let ok = |t: &ManiType| !t.is_known() || t.is_ternary() || *t == ManiType::Bool;
                if !ok(lhs) || !ok(rhs) {
                    return Err(self.binop_operand_err(op, lhs, rhs, span));
                }
                // A ternary-logic connective takes ONE three-valued value.
                // `is_ternary()` is wider than that — it admits `tryte`, `t9`,
                // `t27`, `t54` and `tfloat`, which are ternary NUMBERS — and
                // accepting those produced a silent backend divergence rather
                // than a wrong answer on one backend.
                //
                // Measured on `let a: t27 = 9841; let b: t27 = 121`, seven of
                // the eight operators disagreed:
                //
                //     tnot a     T3 -9841   LLVM   -113
                //     a tand b   T3   121   LLVM    113
                //     a txor b   T3 -19921  LLVM      1
                //     a timp b   T3 -9719   LLVM      1
                //
                // Both are defensible readings of an ill-defined request. T3
                // computes a word-width TMIN/TNEG; the LLVM backend types the
                // whole `Trit*` IR family `i8`, so it truncates to 8 bits
                // first (9841 & 0xFF = 113). Neither is a `trit`, which is
                // what this function then claims the result is.
                //
                // So the type was never inhabited and the operation was never
                // defined. Rejecting is the fix, not making the backends
                // agree: the language reference documents these as operating
                // "on `trit` and `bool3` values", C2's lane-wise family is the
                // well-defined thing to do to a whole word, and a caller who
                // wants the numeric min of two words wants `math::min`.
                //
                // Safe to reject: instrumenting `binop_type` and checking all
                // 268 shipped `.mt` files found ZERO sites. Nothing shipped is
                // miscompiled today, and nothing shipped stops compiling.
                let too_wide = |t: &ManiType| matches!(
                    t,
                    ManiType::Tryte | ManiType::T9 | ManiType::T27
                        | ManiType::T54 | ManiType::Tfloat
                );
                if too_wide(lhs) || too_wide(rhs) {
                    let wide = if too_wide(lhs) { lhs } else { rhs };
                    return Err(self.err(span, format!(
                        "invalid operands: `{}` is a three-valued logic operator and \
                         takes `trit` or `bool3`, not `{}`, which is a {}-trit number. \
                         For the same connective applied to every trit of a word, use \
                         the lane-wise form `{}w`; for a numeric comparison, use \
                         `math::min`/`math::max`; to test one trit, narrow it first.",
                        Self::binop_symbol(op), wide.display(),
                        match wide {
                            ManiType::Tryte => "3", ManiType::T9 => "9",
                            ManiType::T27 => "27", ManiType::T54 => "54",
                            _ => "multi",
                        },
                        Self::binop_symbol(op),
                    )));
                }
                // Two `bool`s under `tand`/`tor`/`tany` give a `bool`, and that
                // is provable rather than convenient: with false as -1 and true
                // as +1, min, max and "either +1 wins" are CLOSED on {-1, +1},
                // so no two-valued pair can produce `unknown`. The lowering
                // emits `&&`/`||` for exactly these three (see
                // IRLowerer::lower_expr), which is why `if a > b tand c > d`
                // now type-checks — an `if` needs a `bool`, and this is one.
                //
                // `txor` and `tcon` are NOT closed: `true txor false` and
                // `true tcon false` are both `unknown`. They stay `bool3`, and
                // a caller has to reach for `tif`, which is correct — the value
                // really does have three outcomes.
                // `timp` and `teq` join the closed set, and for the same
                // reason: on {-1, +1} implication is classical material
                // implication and equivalence is the biconditional, both of
                // which are two-valued. The one cell where L3 departs from
                // classical logic — `a = b = 0` — is unreachable when neither
                // operand can be 0.
                let closed_on_bools = matches!(
                    op,
                    BinOpKind::Tand | BinOpKind::Tor | BinOpKind::Tany
                        | BinOpKind::Timp | BinOpKind::Teq
                );
                match (lhs, rhs) {
                    (ManiType::Bool, ManiType::Bool) if closed_on_bools => Ok(ManiType::Bool),
                    (ManiType::Bool, ManiType::Bool)
                    | (ManiType::Bool3, ManiType::Bool3)
                    | (ManiType::Bool, ManiType::Bool3)
                    | (ManiType::Bool3, ManiType::Bool) => Ok(ManiType::Bool3),
                    _ => Ok(ManiType::Trit),
                }
            }
            BinOpKind::Tandw | BinOpKind::Torw | BinOpKind::Txorw
            | BinOpKind::Timpw | BinOpKind::Tcmpw => {
                // C2. The lane-wise family takes WORDS, not trits — the whole
                // point is the 27 lanes, and a `trit` has one. So the operand
                // rule is the integer rule, not the ternary-logic rule above:
                // any balanced-ternary or binary integer, and not a float.
                //
                // Both float types are excluded explicitly. `Tfloat` is a
                // 27-trit float, so it is tempting to think its trits are
                // lanes — they are not. They are a mantissa and an exponent,
                // and min-ing them lane-by-lane produces a number that means
                // nothing. (`is_numeric()` admits `Tfloat`, so leaving it to
                // that predicate the way the bitwise arm does would have let
                // `x tandw 1.0t` through.)
                //
                // `bool`/`bool3` are excluded too, and deliberately, even
                // though the scalar operators accept them: a `bool` is one
                // three-valued answer, and asking for 27 lanes of it is far
                // more likely to be a typo for `tand` than an intention.
                let ok = |t: &ManiType| {
                    !t.is_known()
                        || (t.is_numeric()
                            && *t != ManiType::Float
                            && *t != ManiType::Tfloat)
                };
                if !ok(lhs) || !ok(rhs) {
                    return Err(self.binop_operand_err(op, lhs, rhs, span));
                }
                // Result is a word of the same width as the operands. Follows
                // the bitwise arm: keep the operand type rather than promoting
                // everything to a single lane-width type, which would make
                // `t27 x = a tandw b` need a cast it should not need.
                if lhs.is_known() { Ok(lhs.clone()) } else { Ok(rhs.clone()) }
            }
            BinOpKind::Range | BinOpKind::RangeInclusive => {
                let ok = |t: &ManiType| !t.is_known() || t.is_numeric();
                if !ok(lhs) || !ok(rhs) {
                    return Err(self.binop_operand_err(op, lhs, rhs, span));
                }
                Ok(ManiType::Generic("Range".to_string(), vec![lhs.clone()]))
            }
        }
    }

    pub(super) fn unop_type(&self, op: &UnOpKind, operand: &ManiType, span: Span) -> CompileResult<ManiType> {
        // The unary half of the same defect. `tnot` on a `t27` was accepted,
        // typed `trit`, and then computed as a word-width TNEG on T3 and an
        // i8 negation on LLVM — `tnot 9841` gave -9841 and -113. See the
        // binary arm above for the full measurement and for why the fix is to
        // reject rather than to make the two agree.
        //
        // `tnotw` is the lane-wise form and is the thing to reach for; it is
        // word-width on both backends by construction.
        if matches!(op, UnOpKind::Tnot | UnOpKind::Tposs | UnOpKind::Tnec)
            && matches!(
                operand,
                ManiType::Tryte | ManiType::T9 | ManiType::T27
                    | ManiType::T54 | ManiType::Tfloat
            )
        {
            let name = match op {
                UnOpKind::Tnot => "tnot",
                UnOpKind::Tposs => "tposs",
                _ => "tnec",
            };
            return Err(self.err(span, format!(
                "invalid operand: `{}` is a three-valued logic operator and takes \
                 `trit` or `bool3`, not `{}`.{}",
                name, operand.display(),
                if matches!(op, UnOpKind::Tnot) {
                    " To negate every trit of a word, use `tnotw`."
                } else {
                    " Narrow the value to a single trit first."
                },
            )));
        }
        match op {
            UnOpKind::Neg | UnOpKind::TritNeg => Ok(operand.clone()),
            UnOpKind::Not => Ok(ManiType::Bool),
            UnOpKind::Tnot => {
                if *operand == ManiType::Bool3 { Ok(ManiType::Bool3) } else { Ok(ManiType::Trit) }
            }
            // C1. The modal operators are two-valued whatever they are given:
            // `tposs` answers "might this be true?" and `tnec` answers "is this
            // definitely true?", and neither question has an unknown answer.
            // That is what makes them the bridge out of three-valued logic —
            // `if tnec x { … }` is exactly the chain of `tif` that real
            // three-valued code writes by hand today.
            UnOpKind::Tposs | UnOpKind::Tnec => Ok(ManiType::Bool),
            // C2: lane-wise negation preserves the word it was given. Unlike
            // `tnot`, which collapses whatever it is handed to a `trit`, this
            // one is width-preserving because it operates on all the lanes.
            UnOpKind::Tnotw => Ok(operand.clone()),
            UnOpKind::Deref | UnOpKind::Ref => Ok(operand.clone()),
        }
    }

    pub(super) fn resolve_field_type(&self, obj_ty: &ManiType, field: &str) -> ManiType {
        // Tuple indexing: `p.0`, `p.1`, … The parser turns the integer token
        // into a field name, so the digits arrive here as a string.
        //
        // This arm did not exist until 20 August 2026, so every tuple field
        // access fell through to `ManiType::Unknown` below — and the matching
        // hole in the IR lowerer made it load element 0. `let p = (7, 9); p.1`
        // evaluated to 7 on both backends with no diagnostic. Destructuring
        // (`let (a, b) = …`) has its own path and was always correct, which is
        // why nothing in the stdlib ever tripped over it.
        if let ManiType::Tuple(elems) = obj_ty {
            if let Ok(i) = field.parse::<usize>() {
                if i < elems.len() {
                    return elems[i].clone();
                }
            }
            return ManiType::Unknown;
        }
        if let ManiType::Struct(name, args) = obj_ty {
            // P68: on a struct whose type arguments are known, a field's type
            // comes from the DECLARATION resolved under them. `self.structs`
            // holds `Unknown` for every field declared as a type parameter,
            // because a struct's parameters are not in scope when it is
            // registered — so without this the arguments carried by the type
            // would be inert.
            if !args.is_empty() {
                let (n, a) = (name.clone(), args.clone());
                if let Some(t) = self.struct_field_ty_at(&n, &a, field) {
                    if t.is_known() {
                        return t;
                    }
                }
            }
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

    pub(super) fn resolve_method_type(&mut self, obj_ty: &ManiType, method: &str, span: Span) -> ManiType {
        // Check user-defined impl methods first (before built-in fallbacks)
        let type_name_str = match obj_ty {
            ManiType::Struct(n, _) | ManiType::Enum(n) => Some(n.as_str()),
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
            // Result<T, E> — see ir/lower/lower_result.rs, which lowers each of
            // these to the loads and branches `match` already uses. The list
            // here and RESULT_METHODS there must agree; `check_expr` rejects
            // any other method on a Result rather than letting it reach codegen
            // and fail at link, which is what Section 18 was.
            (ManiType::Generic(name, args), "unwrap") if name == "Result" => {
                args.first().cloned().unwrap_or(ManiType::Unknown)
            }
            (ManiType::Generic(name, args), "unwrap_or") if name == "Result" => {
                args.first().cloned().unwrap_or(ManiType::Unknown)
            }
            (ManiType::Generic(name, _), "is_ok") if name == "Result" => ManiType::Bool,
            (ManiType::Generic(name, _), "is_err") if name == "Result" => ManiType::Bool,
            (ManiType::Generic(name, _), "is_unknown") if name == "Result" => ManiType::Bool,
            // The tag is a trit, so `tif r.tag() { … }` dispatches on all three
            // outcomes at once rather than asking three yes/no questions.
            (ManiType::Generic(name, _), "tag") if name == "Result" => ManiType::Trit,
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
            (ManiType::Generic(name, args), "remove") if name == "Vec" => {
                args.first().cloned().unwrap_or(ManiType::Unknown)
            }
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
            // Same order as keys(), so the two can be paired by index.
            (ManiType::Generic(name, args), "values") if name == "Map" => {
                let val_ty = args.get(1).cloned().unwrap_or(ManiType::Unknown);
                ManiType::Generic("Vec".to_string(), vec![val_ty])
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
            (ManiType::Generic(name, _), "set") if name == "Mutex" || name == "MutexGuard" => {
                ManiType::Void
            }
            (ManiType::Generic(name, args), "get") if name == "Mutex" || name == "MutexGuard" => {
                args.first().cloned().unwrap_or(ManiType::Unknown)
            }
            (ManiType::Generic(name, _), "update") if name == "MutexGuard" => ManiType::Void,
            (ManiType::Generic(name, _), "unlock") if name == "MutexGuard" => ManiType::Void,

            // AtomicTrit methods
            (ManiType::Struct(name, _), "load") if name == "AtomicTrit" => ManiType::Trit,
            (ManiType::Struct(name, _), "store") if name == "AtomicTrit" => ManiType::Void,

            // Barrier methods
            (ManiType::Struct(name, _), "wait") if name == "Barrier" => ManiType::Bool,

            // Semaphore methods
            (ManiType::Struct(name, _), "acquire") if name == "Semaphore" => ManiType::Void,
            (ManiType::Struct(name, _), "release") if name == "Semaphore" => ManiType::Void,

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

            // Catch-all for unknown method calls on Generic (collection) types —
            // permissive (type Unknown), but flag the likely typo.
            (ManiType::Generic(gname, _), _) => {
                let known: Vec<&str> = vec![
                    "len", "is_empty", "push", "pop", "get", "set", "insert", "remove",
                    "contains", "contains_key", "keys", "values", "clear", "sort", "reverse",
                    "map", "filter", "for_each", "slice", "index_of", "fold",
                    "push_front", "push_back", "pop_front", "pop_back", "front", "back",
                    "send", "recv", "try_recv", "close", "lock", "unlock", "update",
                    "join", "block_on", "unwrap", "keys_with_prefix",
                    "is_subset", "is_superset", "is_disjoint",
                    "intersection", "union", "difference", "get_or",
                ];
                let hint = did_you_mean(method, known.iter().map(|s| s.to_string()))
                    .unwrap_or_default();
                self.warnings.push(CompileWarning::new(
                    WarningKind::UnknownType,
                    &self.file, span.line, span.col,
                    format!(
                        "unknown method '{}' on '{}'{} — type inferred as Unknown",
                        method, gname, hint
                    ),
                ));
                ManiType::Unknown
            }

            _ => ManiType::Unknown,
        }
    }

    // ---------------------------------------------------------------------------
    // Match pattern bindings
    // ---------------------------------------------------------------------------

    /// Define every variable bound by a match pattern in the current scope,
    /// deriving payload types from the scrutinee type where possible
    /// (Result/Option payloads, user enum variant fields, tuple elements,
    /// struct fields). Unknown payloads fall back to `ManiType::Unknown`.
    pub(super) fn define_pattern_bindings(&mut self, pat: &Pattern, scrut_ty: &ManiType) {
        match pat {
            Pattern::Wildcard(_) | Pattern::Lit(_, _) => {}
            Pattern::Ident(name, _) => {
                self.symbols.define(name, scrut_ty.clone(), false);
            }
            Pattern::Tuple(pats, _) => {
                let elem_tys: Vec<ManiType> = match scrut_ty {
                    ManiType::Tuple(ts) => ts.clone(),
                    _ => vec![ManiType::Unknown; pats.len()],
                };
                for (i, p) in pats.iter().enumerate() {
                    let ety = elem_tys.get(i).cloned().unwrap_or(ManiType::Unknown);
                    self.define_pattern_bindings(p, &ety);
                }
            }
            Pattern::Struct(sname, field_pats, _) => {
                let struct_name = match scrut_ty {
                    ManiType::Struct(n, _) => n.clone(),
                    _ => sname.clone(),
                };
                let fields = self.structs.get(&struct_name).cloned().unwrap_or_default();
                for (fname, fpat) in field_pats {
                    let fty = fields
                        .iter()
                        .find(|(n, _)| n == fname)
                        .map(|(_, t)| t.clone())
                        .unwrap_or(ManiType::Unknown);
                    self.define_pattern_bindings(fpat, &fty);
                }
            }
            Pattern::Enum(variant, enum_name, subpats, _) => {
                // The parser encodes the bare `Unknown(msg)` result pattern as
                // Enum("Result", Some("Unknown"), ...): normalise the variant name.
                let variant_name: &str =
                    if variant == "Result" && enum_name.as_deref() == Some("Unknown") {
                        "Unknown"
                    } else {
                        variant.as_str()
                    };
                let payload_tys: Vec<ManiType> = match scrut_ty {
                    // Result<T, E>: Ok(T) / Err(E) / Unknown(str)
                    ManiType::Generic(g, args) if g == "Result" => match variant_name {
                        "Ok" => vec![args.first().cloned().unwrap_or(ManiType::Unknown)],
                        "Err" => vec![args.get(1).cloned().unwrap_or(ManiType::Unknown)],
                        "Unknown" => vec![ManiType::Str],
                        _ => vec![ManiType::Unknown; subpats.len()],
                    },
                    _ => {
                        // User enum: field types from the variant declaration.
                        let ename = match (enum_name, scrut_ty) {
                            (Some(en), _) => Some(en.clone()),
                            (None, ManiType::Enum(n)) => Some(n.clone()),
                            _ => None,
                        };
                        ename
                            .and_then(|en| self.enums.get(&en).cloned())
                            .and_then(|variants| {
                                variants
                                    .iter()
                                    .find(|(v, _)| v == variant_name)
                                    .map(|(_, tys)| tys.clone())
                            })
                            .unwrap_or_else(|| vec![ManiType::Unknown; subpats.len()])
                    }
                };
                for (i, p) in subpats.iter().enumerate() {
                    let pty = payload_tys.get(i).cloned().unwrap_or(ManiType::Unknown);
                    self.define_pattern_bindings(p, &pty);
                }
            }
            Pattern::Or(pats, _) => {
                for p in pats {
                    self.define_pattern_bindings(p, scrut_ty);
                }
            }
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
        // A guarded arm can fail at runtime, so it never counts toward
        // exhaustiveness — only unguarded arms are considered below.
        let arms: Vec<&TypedMatchArm> = arms.iter().filter(|a| a.guard.is_none()).collect();

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
            // P6: `Result<T, E>` is a THREE-variant closed type and must be
            // matched exhaustively, exactly as an enum, a trit and a bool3
            // already are. It was the one closed type with no arm here, so
            // `match r { Ok(v) => .., Err(e) => .. }` compiled — and when `r`
            // was `Unknown` the third state vanished, differently on each
            // backend: T3 halted with exit status 24 and lost the rest of the
            // program, LLVM fell through the match and carried on. Neither said
            // anything.
            //
            // That is the exact failure the type exists to prevent. The
            // language's own reference puts it plainly: "Err means it failed;
            // Unknown means we do not know, which is not a failure and must not
            // be collapsed into one." Here it was collapsed into nothing.
            //
            // Safe to enforce, measured not assumed: the checker was
            // instrumented and every `.mt` file in both repos checked — ZERO
            // non-exhaustive Result matches. A wildcard `_` arm remains the
            // escape hatch and is handled above.
            ManiType::Generic(g, _) if g == "Result" => {
                let mut covered: Vec<&str> = Vec::new();
                for arm in &arms {
                    if let Pattern::Enum(variant, enum_name, _, _) = &arm.pattern {
                        // The parser encodes bare `Unknown(msg)` as
                        // Enum("Result", Some("Unknown"), ..); normalise it the
                        // same way define_pattern_bindings does.
                        let v: &str = if variant == "Result"
                            && enum_name.as_deref() == Some("Unknown")
                        {
                            "Unknown"
                        } else {
                            variant.as_str()
                        };
                        covered.push(v);
                    }
                }
                let missing: Vec<&str> = ["Ok", "Unknown", "Err"]
                    .iter()
                    .filter(|w| !covered.contains(w))
                    .cloned()
                    .collect();
                if !missing.is_empty() {
                    return Err(self.err(span, format!(
                        "non-exhaustive match on `Result` — missing: {}. A Result has \
                         THREE outcomes, and `Unknown` is not a kind of `Err`: it means \
                         the answer is not known, which is not a failure. Add the \
                         missing arm(s), use `tresult` (which requires all three), or \
                         add a wildcard `_` if the omission is deliberate.",
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
