// semantic/analyzer/expressions.rs — check_expr method for SemanticAnalyzer.

use super::*;

impl SemanticAnalyzer {
    pub(crate) fn check_expr(&mut self, expr: &Expr, hint: Option<&ManiType>) -> CompileResult<TypedExpr> {
        let span = expr.span();
        match expr {
            Expr::Lit(lit, _) => {
                let ty = self.infer_lit_type(lit, hint);
                self.check_literal_fits_word(lit, &ty, span)?;
                Ok(TypedExpr { kind: TypedExprKind::Lit(lit.clone()), ty, span })
            }

            Expr::Ident(name, _) => {
                // Track variable reads for unused-variable detection
                self.read_vars.insert(name.clone());
                let ty = if let Some(sym) = self.symbols.lookup(name) {
                    sym.ty.clone()
                } else if let Some((params, ret)) = self.functions.get(name) {
                    // P53/P54: a bare function name is a function REFERENCE.
                    //
                    // This used to answer the function's RETURN type unless the
                    // context supplied a `fn`-typed hint — its own comment
                    // called that "the legacy view" — and the two open findings
                    // it produced are one defect wearing two costumes.
                    //
                    // P53: `let f = dbl;` has no hint, so `f` was typed `int`,
                    // `dbl`'s return type. The lowerer then emitted a call to
                    // the BINDING's name and neither backend had an `@f`.
                    // `let f: fn(int) -> int = dbl;` worked because the
                    // ANNOTATION IS THE HINT, which is exactly why annotating
                    // was a complete workaround. A lambda worked by
                    // coincidence: it is emitted under the name it is bound to,
                    // so `@f` happened to resolve — and `let dbl2 = dbl;`
                    // failed while `let dbl = dbl;` ran, which is that
                    // coincidence stated as an experiment.
                    //
                    // P54: the CALLEE of `pick()` is checked with no hint, so
                    // `pick` was typed as its return type `fn(int) -> int`, and
                    // the call checker's first arm then read that type's
                    // parameters as `pick`'s own — "function 'pick' expects 1
                    // argument(s), found 0", where the `int` belongs to the
                    // return type. With a zero-argument return type there was
                    // nothing to absorb, so arity passed and the RESULT was
                    // mistyped instead: `fn pick() -> fn() -> int` checks
                    // clean and dies in the assembler.
                    //
                    // A function whose return type is not itself a function is
                    // unaffected either way, which is why this survived: the
                    // call checker's first arm only matches when the answer is
                    // already a `ManiType::Fn`.
                    ManiType::Fn(params.clone(), Box::new(ret.clone()))
                } else if let Some(enum_name) = self.enum_variant_path(name) {
                    // EnumName::Variant.
                    //
                    // P43: a variant that DECLARES fields cannot be named
                    // without them. It used to be accepted, and constructed a
                    // cell whose payload nobody had written — `Shape::Circle`
                    // then matched `Shape::Circle(r)` and bound `r` to 0 on
                    // both backends. That is a silent wrong answer reachable
                    // from a one-word typo.
                    if let Some(n) = self.enum_variant_arity(name) {
                        if n > 0 {
                            return Err(self.err(span, format!(
                                "enum variant '{}' carries {} value(s), so it cannot be                                  named on its own — write `{}({})`",
                                name, n, name,
                                std::iter::repeat("…").take(n).collect::<Vec<_>>().join(", "),
                            )));
                        }
                    }
                    ManiType::Enum(enum_name)
                } else if let Some(pos) = name.rfind("::") {
                    // Unresolved `::` path.
                    // S11: referencing a private item of a loaded module is a
                    // hard error mentioning privacy.
                    if let Some(mod_prefix) = self.module_private_items.get(name.as_str()) {
                        return Err(self.err(span, format!(
                            "'{}' is private in module '{}' — mark it `pub` to make it importable",
                            name, mod_prefix
                        )));
                    }
                    // S12: warn for unknown `::` paths (still permissively
                    // typed Unknown, matching the crate's design).
                    let prefix = &name[..pos];
                    let item = &name[pos + 2..];
                    let bare_mod = prefix.strip_prefix("std::").unwrap_or(prefix);
                    if let Some(members) = super::std_module_members(bare_mod) {
                        if !members.contains(item) {
                            let hint = did_you_mean(item, members.iter().cloned())
                                .unwrap_or_default();
                            // A HARD ERROR, not a warning.  A warning here typed the
                            // path `Unknown` and let it reach codegen, where it died
                            // against a mangled symbol the programmer never wrote —
                            // `@io_print_bool`, `@_get`, `Undefined label:` — carrying
                            // no line, no column and no visible relation to the source
                            // line that caused it.  Three separate debugging sessions
                            // were spent walking back from such a symbol to its call.
                            //
                            // Safe to harden ONLY because the member list is now
                            // checked in both directions by `member_list_tests`: every
                            // registered builtin appears in its module's list (so a
                            // correct program cannot be rejected), and every entry in
                            // STDLIB_EXTRA_MEMBERS has a real referent (so nothing is
                            // waved through).  Without test 1 this change would have
                            // rejected `async::sleep`, which works on both backends.
                            //
                            // The user-module branch below stays a warning on purpose:
                            // module-scope `pub let` globals are never registered by
                            // `load_user_module`, so erroring there would turn a
                            // missing feature into a hard rejection of correct code.
                            // That is also why a user module shadowing a stdlib name
                            // is excluded here — its globals would hit this list.
                            if !self.loaded_module_prefixes.contains(prefix) {
                                return Err(self.err(span, format!(
                                    "std module '{}' has no item '{}'{}",
                                    bare_mod, item, hint,
                                )));
                            }
                            self.warnings.push(CompileWarning::new(
                                WarningKind::UnknownType,
                                &self.dfile(span), span.line, span.col,
                                format!("std module '{}' has no item '{}'{}", bare_mod, item, hint),
                            ));
                        }
                    } else if self.loaded_module_prefixes.contains(prefix) {
                        let mod_prefix = format!("{}::", prefix);
                        let candidates = self.functions.keys()
                            .chain(self.structs.keys())
                            .chain(self.enums.keys())
                            .filter(|n| n.starts_with(&mod_prefix))
                            .map(|n| n[mod_prefix.len()..].to_string());
                        let hint = did_you_mean(item, candidates).unwrap_or_default();
                        self.warnings.push(CompileWarning::new(
                            WarningKind::UnknownType,
                            &self.dfile(span), span.line, span.col,
                            format!("module '{}' has no item '{}'{}", prefix, item, hint),
                        ));
                    } else {
                        let first = &name[..name.find("::").unwrap_or(name.len())];
                        // Namespaces the type checker cannot enumerate (builtin
                        // generic types, structs/enums with impls, generics).
                        const BUILTIN_NAMESPACES: &[&str] = &[
                            "Vec", "Map", "Set", "Deque", "TernaryTrie", "Channel",
                            "Mutex", "MutexGuard", "AtomicTrit", "Barrier", "Semaphore",
                            "Task", "Result", "Pair", "Range", "String", "str",
                            "Self", "std",
                        ];
                        if self.enums.contains_key(first) {
                            self.warnings.push(CompileWarning::new(
                                WarningKind::UnknownType,
                                &self.dfile(span), span.line, span.col,
                                format!("enum '{}' has no variant '{}'", first, item),
                            ));
                        } else if !BUILTIN_NAMESPACES.contains(&first)
                            && !self.structs.contains_key(first)
                            && !self.user_method_types.contains_key(first)
                            && !self.type_params.contains_key(first)
                            && !Self::STDLIB_MODULES.contains(&first)
                        {
                            self.warnings.push(CompileWarning::new(
                                WarningKind::UnknownType,
                                &self.dfile(span), span.line, span.col,
                                format!("unknown module or type '{}' in path '{}'", first, name),
                            ));
                        }
                    }
                    ManiType::Unknown
                } else {
                    // `Some` and `None` are refused outright rather than
                    // warned about. They used to sit in the list below, which
                    // silenced the unknown-identifier warning and let them reach
                    // codegen, where they failed at assembly ("Undefined label:
                    // Some"). See the `Option` arm of `resolve_type`: Result is
                    // this language's option type, and it has a third outcome
                    // that Option cannot express.
                    if name == "Some" || name == "None" {
                        return Err(self.err(span, format!(
                            "`{}` is not a ManiT constructor. `Result<T, E>` is this \
                             language's option type and it has three outcomes rather \
                             than two: `Ok(v)` for a value, `Unknown(msg)` where you \
                             would write `None`, and `Err(e)` for a failure.",
                            name,
                        )));
                    }
                    // Result constructors are handled structurally by the
                    // lowering — never warn for them.
                    const RESULT_CONSTRUCTORS: &[&str] =
                        &["Ok", "Err", "Unknown"];
                    // An unknown identifier is an ERROR, not a warning.
                    //
                    // It was a warning from the beginning, under the comment
                    // "Allow unknown identifiers for now (stdlib etc.)", and
                    // the cost of that `for now` was measured on 23 Aug 2026:
                    // of 771 mutations that `manitc check` failed to catch,
                    // 357 -- the largest class by far -- were nothing more
                    // than a variable referenced under a misspelled name. The
                    // checker SAW every one of them, named them, and even
                    // offered the correct spelling; it just said `warning` and
                    // exited 0.
                    //
                    // That is not only a compiler question. L1, the metric the
                    // Phase A training gate turns on, is DEFINED as "generations
                    // pass `manitc check`" -- so every typo the checker waved
                    // through was scored as a success, and the gate measured
                    // the checker's blindness as the model's skill.
                    //
                    // Flipping this was considered on 22 Aug and rejected,
                    // because `gui_set_color` lives in runtime/gui.c with no
                    // stdlib/*.mt declaration and appeared to need the
                    // leniency. It does not: it is registered in the native
                    // table in analyzer/mod.rs with a full signature, and
                    // calling it wrongly already produced a hard arity error.
                    // The premise was wrong, so the conclusion did not hold.
                    //
                    // Checked rather than assumed, across all 128 shipped .mt
                    // files in maniTC and thatteOS: exactly ONE name in ONE
                    // file still depended on the leniency -- `fs_remove_file`
                    // in thatteos/userspace/gui_fm.mt -- and it depended on it
                    // to call a C symbol that exists in runtime/system.c and
                    // in the LLVM emitter but had never been added to the
                    // native table. It is registered now.
                    //
                    // Nothing legitimate is lost, because the escape hatch this
                    // leniency was standing in for already exists and is
                    // better: any file may write `fn foo(x: str) ;  // native`
                    // and get a TYPED declaration of a runtime symbol instead
                    // of an untyped hole. Forward references are unaffected --
                    // functions are collected in a pre-pass, so calling one
                    // defined later in the file never reached this branch.
                    if !name.contains('<') && !RESULT_CONSTRUCTORS.contains(&name.as_str()) {
                        let var_names = self.symbols.all_names();
                        let fn_names = self.functions.keys().cloned();
                        let candidates = var_names.chain(fn_names);
                        let hint = did_you_mean(name, candidates).unwrap_or_else(|| {
                            ". If this names a C runtime symbol, declare it: \
                             `fn <name>(<params>) -> <type> ;  // native`"
                                .to_string()
                        });
                        return Err(self.err(span, format!(
                            "unknown identifier '{}'{}", name, hint,
                        )));
                    }
                    ManiType::Unknown
                };
                Ok(TypedExpr { kind: TypedExprKind::Ident(name.clone()), ty, span })
            }

            Expr::BinOp(lhs, op, rhs, _) => {
                // Operand hints: ternary-logic operators expect trit operands
                // (so bare 0/1/-1 literals coerce), logical operators expect bool.
                let operand_hint = match op {
                    BinOpKind::Tand | BinOpKind::Tor | BinOpKind::Txor
                    | BinOpKind::Tcon | BinOpKind::Tany
                    | BinOpKind::Timp | BinOpKind::Teq => Some(ManiType::Trit),
                    BinOpKind::And | BinOpKind::Or => Some(ManiType::Bool),
                    _ => None,
                };
                let tlhs = self.check_expr(lhs, operand_hint.as_ref())?;
                let rhs_hint = if operand_hint.is_some() && tlhs.ty == ManiType::Bool3 {
                    ManiType::Bool3
                } else if operand_hint.is_some() {
                    operand_hint.clone().unwrap()
                } else {
                    tlhs.ty.clone()
                };
                let trhs = self.check_expr(rhs, Some(&rhs_hint))?;
                let ty = self.binop_type(op, &tlhs.ty, &trhs.ty, span)?;

                // C4/R2: the migration backlog. `tlhs.ty` is what
                // `binop_to_ir` will look at in the lowerer, so the list this
                // produces is exactly the set of sites the version change
                // moves — not a superset picked by matching on the operator
                // alone.
                self.note_division_semantics(op, &tlhs.ty, span);

                // Division by zero detection
                if matches!(op, BinOpKind::Div | BinOpKind::Rem) {
                    if let TypedExprKind::Lit(Lit::Int(0)) = &trhs.kind {
                        // A7: integer division by a literal zero has no
                        // meaningful result — T3 traps, LLVM raises SIGFPE.
                        // It cannot be intentional, so reject it rather than
                        // warn. (Float /0.0 stays a warning below: IEEE
                        // division by zero is defined and yields inf/nan.)
                        return Err(self.err(
                            span,
                            format!(
                                "division by zero: the right operand of `{}` is the \
                                 literal 0",
                                if matches!(op, BinOpKind::Div) { "/" } else { "%" },
                            ),
                        ));
                    } else if let TypedExprKind::Lit(Lit::Float(f)) = &trhs.kind {
                        if *f == 0.0 {
                            self.warnings.push(CompileWarning::new(
                                WarningKind::DivisionByZero,
                                &self.dfile(span), span.line, span.col,
                                "division by zero",
                            ));
                        }
                    }
                }

                Ok(TypedExpr {
                    kind: TypedExprKind::BinOp(Box::new(tlhs), op.clone(), Box::new(trhs)),
                    ty,
                    span,
                })
            }

            Expr::UnOp(op, operand, _) => {
                let te = self.check_expr(operand, hint)?;
                let ty = self.unop_type(op, &te.ty, span)?;
                Ok(TypedExpr {
                    kind: TypedExprKind::UnOp(op.clone(), Box::new(te)),
                    ty,
                    span,
                })
            }

            Expr::Call(callee, args, _) => {
                // P43: an enum-variant callee is built here rather than through
                // `check_expr`, because the rule that a payload variant cannot
                // be NAMED without its values must not fire on the one position
                // where naming it is exactly right — as the callee of the call
                // that supplies them.
                let mut tcallee = match &**callee {
                    Expr::Ident(n, sp) if self.enum_variant_path(n).is_some() => TypedExpr {
                        kind: TypedExprKind::Ident(n.clone()),
                        ty: ManiType::Enum(self.enum_variant_path(n).unwrap()),
                        span: *sp,
                    },
                    _ => self.check_expr(callee, None)?,
                };
                // Try to resolve function name or fn-type for type lookup.
                // `enforce` is set only when the signature is trustworthy:
                // fn-typed values and user-defined functions always are; builtin
                // registry entries only when fully specified (no Unknown params —
                // e.g. println / fmt::format are variadic-ish placeholders).
                let mut enforce = false;
                let mut display_name = String::from("<fn>");
                // The callee's name whether or not the builtin table knows it.
                // `io` is not in SIG_MODULES, so `io::print` never reaches
                // `self.functions` and `display_name` stays "<fn>" — which is
                // why the P24 check below cannot key on it.
                let mut callee_name = String::new();
                // P53/P54: a bare function name now types as its FUNCTION
                // type, so without this guard the arm below would swallow every
                // direct call — and with it everything the `Ident` arm does
                // besides choosing a signature: A2's call-graph edge,
                // `check_extern_call_site`, `note_undeclared_native`, the
                // enum-variant constructor path, and the `enforce` rule that
                // exempts a builtin whose parameters are `Unknown`
                // (`println` and `fmt::format` are variadic-ish, S53). A direct
                // call therefore stays on the path it has always taken, and
                // this arm keeps exactly its old population: values of function
                // type that are NOT the bare name of a declared function — a
                // local, a parameter, a field, a call result.
                let callee_is_declared_fn_name = match &tcallee.kind {
                    TypedExprKind::Ident(n) => {
                        self.symbols.lookup(n).is_none() && self.functions.contains_key(n)
                    }
                    _ => false,
                };
                let (param_tys, ret_ty) = match &tcallee.ty {
                    ManiType::Fn(pts, rt) if !callee_is_declared_fn_name => {
                        enforce = true;
                        if let TypedExprKind::Ident(n) = &tcallee.kind {
                            display_name = n.clone();
                        }
                        (pts.clone(), *rt.clone())
                    }
                    _ => match &tcallee.kind {
                        TypedExprKind::Ident(name) => {
                            // A1. Two things happen for a native callee, and
                            // which one depends on whether it was declared.
                            //
                            // Declared: `collect_extern_decl` put a fully
                            // resolved signature into `self.functions`, so the
                            // `all(is_known)` test below turns `enforce` ON and
                            // the arguments are checked like any other call.
                            // That is step 2, and it needs no special case here
                            // — the asymmetry it removes existed only because a
                            // native's parameters were Unknown.
                            //
                            // Undeclared: the name goes into the migration
                            // backlog and the call stays unchecked, exactly as
                            // it was before A1.
                            let n = name.clone();
                            callee_name = n.clone();
                            // A2: record the call-graph edge. Done here, in the
                            // checker, rather than in a separate traversal —
                            // a second walk could disagree with this one about
                            // what is a call, and the whole value of the
                            // inference is that it describes the program the
                            // checker actually saw.
                            if let Some(caller) = self.current_fn.clone() {
                                self.call_graph
                                    .entry(caller)
                                    .or_default()
                                    .push((n.clone(), span));
                            }
                            if self.externs.contains_key(&n) {
                                // A DECLARATION is the authority on what is
                                // native — checked before `is_native`, which
                                // keys on a hardcoded list of standard library
                                // module names. Asking that list first meant a
                                // declared `gui::set_color` was invisible to
                                // every A1 diagnostic, because `gui` is not on
                                // it: the form could be written and then did
                                // nothing, which is the failure mode A1 exists
                                // to remove.
                                self.check_extern_call_site(&n, span);
                            } else if self.is_native(&n) {
                                self.note_undeclared_native(&n, span);
                            }
                            if let Some(enum_name) = self.enum_variant_path(name) {
                                // P43: a payload-variant CONSTRUCTOR. Routed
                                // through the ordinary parameter machinery
                                // rather than special-cased, so it gets the
                                // arity diagnostic, the argument type checks and
                                // the right result type from one place. Before
                                // this it fell to `(vec![], Unknown)` below and
                                // was checked for nothing at all —
                                // `Shape::Circle(1, 2)` and `Shape::Circle("x")`
                                // were both accepted on a one-`int` variant.
                                let fields = self
                                    .enums
                                    .get(&enum_name)
                                    .and_then(|vs| {
                                        let v = &name[name.find("::").unwrap() + 2..];
                                        vs.iter().find(|(n, _)| n == v).map(|(_, f)| f.clone())
                                    })
                                    .unwrap_or_default();
                                display_name = name.clone();
                                enforce = true;
                                (fields, ManiType::Enum(enum_name))
                            } else if let Some((pts, rt)) = self.functions.get(name) {
                                display_name = name.clone();
                                enforce = !self.builtin_names.contains(name)
                                    || pts.iter().all(|t| t.is_known());
                                (pts.clone(), rt.clone())
                            } else {
                                (vec![], ManiType::Unknown)
                            }
                        }
                        _ => (vec![], ManiType::Unknown),
                    },
                };
                if enforce && args.len() != param_tys.len() {
                    // P43: say WHAT it is. An enum variant now reaches this
                    // check, and calling one a "function" sends the reader to
                    // look for a `fn` that does not exist.
                    let what = if self.enum_variant_path(&display_name).is_some() {
                        "enum variant"
                    } else {
                        "function"
                    };
                    return Err(self.err(span, format!(
                        "{} '{}' expects {} argument(s), found {}",
                        what, display_name, param_tys.len(), args.len()
                    )));
                }
                let mut typed_args = Vec::new();
                for (i, arg) in args.iter().enumerate() {
                    let hint = param_tys.get(i).cloned().unwrap_or(ManiType::Unknown);
                    let targ = self.check_expr(arg, Some(&hint))?;
                    if enforce
                        && hint.is_known()
                        && targ.ty.is_known()
                        && !types_compatible(&hint, &targ.ty)
                    {
                        return Err(self.err(targ.span, format!(
                            "argument {} to '{}': expected `{}`, found `{}`",
                            i + 1, display_name, hint.display(), targ.ty.display()
                        )));
                    }
                    // P24, and deliberately NARROW. A native's declared
                    // parameter types are known — `register_native_module_sigs`
                    // resolves them into `native_param_manitys` — but enforcing
                    // all of them is wrong here, and the test suite says so:
                    // `fmt::format` is variadic behind a `[str]` placeholder,
                    // and S53 documents that a native ACCEPTS a bool where an
                    // int is declared. Both are intentional.
                    //
                    // What is never intentional is passing a non-pointer where
                    // a `str` is declared. `str` is a POINTER, and the LLVM
                    // backend dereferences whatever it is handed:
                    // `io::print(' ')` passed the char's integer value and
                    // segfaulted, from a program the checker accepted. T3 read
                    // whatever that address happened to hold and printed byte
                    // soup. So this rejects exactly that class and nothing
                    // else.
                    if !enforce && !callee_name.is_empty() && targ.ty.is_known() {
                        if let Some(d) = self.native_param_manitys.get(&callee_name) {
                            if matches!(d.get(i), Some(ManiType::Str))
                                && !matches!(targ.ty, ManiType::Str | ManiType::Unknown)
                            {
                                return Err(self.err(targ.span, format!(
                                    "argument {} to '{}': expected `str`, found `{}` \
                                     — `str` is a pointer, and passing a non-pointer \
                                     here dereferences its value",
                                    i + 1, callee_name, targ.ty.display()
                                )));
                            }
                        }
                    }
                    typed_args.push(targ);
                }

                // B1/A4: the declared bounds, checked against what the
                // arguments actually turned out to be. After the loop, because
                // it needs every argument's inferred type — a bound on `T` used
                // in two parameter positions cannot be judged from one of them.
                let mut ret_ty = ret_ty;
                if !display_name.is_empty() && display_name != "<fn>" {
                    let arg_tys: Vec<ManiType> =
                        typed_args.iter().map(|a| a.ty.clone()).collect();
                    let name = display_name.clone();
                    self.check_generic_bounds(&name, &arg_tys, span);

                    // P65: point this call at an instantiation of the callee
                    // for the types it was actually given.
                    //
                    // Both halves matter and the second is easy to miss. The
                    // NAME change is what makes the body compile with real
                    // types instead of the `Unknown`-erased-to-`i64` copy every
                    // call shared. The RETURN TYPE change is what stops the
                    // result arriving as `Unknown` at the caller — that is the
                    // half responsible for `id(p).second` reading slot 0,
                    // since a field lookup on `<unknown>` finds no struct.
                    //
                    // P71: the two halves are gated SEPARATELY, and which
                    // gate belongs to which is the whole finding. `ensure_mono`
                    // judges the BODY. The NAME must wait on that verdict — a
                    // discarded instantiation defines no symbol, so pointing at
                    // it would fail at link. The RETURN TYPE must NOT: it is a
                    // function of the DECLARATION, which the reader wrote and
                    // which says `-> T` whatever the body does. Gating it on
                    // the body left `pick(P{..}, P{..})` typed `<unknown>`,
                    // and a field read on that takes slot 0.
                    //
                    // P73: a PATH-FORM call to a generic `impl<T>` method —
                    // `Box2::bigger(b)` rather than `b.bigger()`. It arrives
                    // here rather than at the method-call site, and the binding
                    // cannot come from the arguments the way it does for a free
                    // function: `self` is declared `Self`, which is not one of
                    // the impl's generics, so `mono_binding_for` bound nothing
                    // and the call kept the erased body. The receiver IS the
                    // first argument, so the binding is `mono_binding_for_impl`
                    // applied to its type arguments — P69's mechanism reached
                    // through a different syntax.
                    let binding = self
                        .generic_impl_owner
                        .get(&name)
                        .and_then(|_| match arg_tys.first() {
                            Some(ManiType::Struct(_, sargs))
                                if !sargs.is_empty()
                                    && sargs.iter().all(|a| a.fully_known()) =>
                            {
                                self.mono_binding_for_impl(&name, sargs)
                            }
                            _ => None,
                        })
                        .or_else(|| self.mono_binding_for(&name, &arg_tys));
                    if let Some(binding) = binding {
                        let body_ok = self.ensure_mono(&name, &binding);
                        if let Some(rt) = self.mono_ret_ty(&name, &binding) {
                            ret_ty = rt;
                        }
                        if body_ok {
                            tcallee.kind =
                                TypedExprKind::Ident(Self::mono_name(&name, &binding));
                        }
                    }
                }

                Ok(TypedExpr {
                    kind: TypedExprKind::Call(Box::new(tcallee), typed_args),
                    ty: ret_ty,
                    span,
                })
            }

            Expr::MethodCall(obj, method, args, _) => {
                let tobj = self.check_expr(obj, None)?;
                let mut typed_args = Vec::new();
                for arg in args {
                    typed_args.push(self.check_expr(arg, None)?);
                }
                // A method on a `Result` must be one this compiler lowers.
                // `.map()` used to be typed here and emitted by neither backend
                // — the same silence as `.unwrap()` (Section 18): accepted by
                // semantic analysis, undefined at link.
                if crate::ir::lower::lower_result::is_result(&tobj.ty)
                    && !crate::ir::lower::lower_result::RESULT_METHODS.contains(&method.as_str())
                {
                    return Err(self.err(span, format!(
                        "`Result` has no method `{}`. Available: {}. Or take all three \
                         outcomes at once with `match r {{ Ok(v) => …, Unknown(m) => …, \
                         Err(e) => … }}`, or `tif r.tag() {{ + => …, 0 => …, - => … }}`.",
                        method,
                        crate::ir::lower::lower_result::RESULT_METHODS
                            .iter().map(|m| format!("`{}`", m))
                            .collect::<Vec<_>>().join(", "),
                    )));
                }
                // Arity, for user-defined methods.
                //
                // `arity` was 30 of the 771 uncaught mutations (§54) and the
                // asymmetry behind it is stark: a free function called with the
                // wrong number of arguments is a hard error, and has been all
                // along, but the identical mistake through a receiver was not
                // checked at all — `v.slice(2)` for a two-argument slice, or a
                // lambda quietly losing the parameter its body still uses.
                //
                // Only USER methods are covered. Builtin methods on Vec, Map,
                // Set and str have no signature table in the analyzer — that
                // table is the actual missing feature and is deliberately not
                // invented here, because a partial table would reject correct
                // programs, which is far worse than accepting wrong ones.
                // `user_method_arity` holds an entry only where the receiver is
                // certain, so a missing entry means no check rather than a
                // guess.
                if let ManiType::Struct(tn, _) | ManiType::Enum(tn) = &tobj.ty {
                    if let Some(&want) = self.user_method_arity
                        .get(tn.as_str())
                        .and_then(|m| m.get(method.as_str()))
                    {
                        if typed_args.len() != want {
                            return Err(self.err(span, format!(
                                "method '{}::{}' expects {} argument(s), found {}",
                                tn, method, want, typed_args.len(),
                            )));
                        }
                    }
                }
                // For method calls, we do basic resolution
                let mut ret_ty = self.resolve_method_type(&tobj.ty, method, span);

                // P69: point this call at an instantiation of the method for
                // the types the RECEIVER turned out to hold.
                //
                // This is P65's free-function path with its one missing piece
                // supplied. There the binding comes from the ARGUMENTS; here
                // there is no argument carrying `T` at all — `fn bigger(self)
                // -> T` mentions it only in return position — so the binding
                // comes from the receiver's own type arguments, which
                // `ManiType::Struct` has carried since P68. Without that, this
                // increment would have nothing to read.
                //
                // The three halves, and the third has no counterpart in the
                // free-function path: the NAME makes the body check with real
                // types instead of `Unknown`-erased-to-`i64`; the RETURN TYPE
                // stops the result arriving as `Unknown` at the caller; and
                // the CALLEE is recorded on the expression, because the
                // lowerer derives a method's symbol from the receiver's type
                // and the receiver is still a `Box2<float>` whichever
                // instantiation was chosen.
                let mut mono_callee: Option<String> = None;
                if let ManiType::Struct(sname, sargs) = &tobj.ty {
                    if !sargs.is_empty() && sargs.iter().all(|a| a.fully_known()) {
                        let qname = format!("{}::{}", sname, method);
                        //
                        // P71 splits the gate here for the same reason it does
                        // at the free-function site: the RETURN TYPE comes from
                        // the declaration, the NAME and the CALLEE from the
                        // body's verdict.
                        if let Some(binding) = self.mono_binding_for_impl(&qname, sargs) {
                            let body_ok = self.ensure_mono(&qname, &binding);
                            if let Some(rt) = self.mono_ret_ty(&qname, &binding) {
                                ret_ty = rt;
                            }
                            if body_ok {
                                mono_callee = Some(Self::mono_name(&qname, &binding));
                            }
                        }
                    }
                }

                Ok(TypedExpr {
                    kind: TypedExprKind::MethodCall(
                        Box::new(tobj),
                        method.clone(),
                        typed_args,
                        mono_callee,
                    ),
                    ty: ret_ty,
                    span,
                })
            }

            Expr::Index(arr, idx, _) => {
                let tarr = self.check_expr(arr, None)?;
                let tidx = self.check_expr(idx, Some(&ManiType::Int))?;

                // A2: array indexing is otherwise entirely unchecked — the
                // LLVM backend segfaults on a far out-of-range index and the T3
                // backend reads adjacent emulator memory (a[-1] returned the
                // array's own length header). When both the length and the
                // index are statically known, reject it outright.
                // Any index the compiler can compute is checked here — `a[3]`,
                // `a[-1]`, `a[0 - 1]`, `a[2 * 5]` alike. A genuinely dynamic
                // index (`a[n]`) is left to the runtime guard emitted by IR
                // lowering. This used to be a private two-arm match that saw
                // literals and negated literals only; it now shares the one
                // folder with module-level constants, whose identical
                // literals-only match was a silent miscompile rather than
                // merely conservative (see semantic/const_fold.rs).
                let const_idx = crate::semantic::const_fold::fold_int(&tidx);
                if let (ManiType::Array(_, Some(len)), Some(i)) = (&tarr.ty, &const_idx) {
                    let len = *len as i64;
                    if *i < 0 || *i >= len {
                        return Err(self.err(
                            span,
                            format!(
                                "index {} is out of bounds for an array of length {} \
                                 (valid indices are 0..{})",
                                i, len, len.saturating_sub(1),
                            ),
                        ));
                    }
                }

                let elem_ty = match &tarr.ty {
                    ManiType::Array(inner, _) => *inner.clone(),
                    ManiType::Generic(name, args) if name == "Vec" => {
                        args.first().cloned().unwrap_or(ManiType::Unknown)
                    }
                    _ => ManiType::Unknown,
                };
                Ok(TypedExpr {
                    kind: TypedExprKind::Index(Box::new(tarr), Box::new(tidx)),
                    ty: elem_ty,
                    span,
                })
            }

            Expr::Field(obj, field, _) => {
                let tobj = self.check_expr(obj, None)?;
                // Feature 4: pub visibility enforcement
                if let ManiType::Struct(struct_name, _) = &tobj.ty {
                    if let Some(pub_fields) = self.struct_pub_fields.get(struct_name.as_str()) {
                        if let Some((_, is_pub)) = pub_fields.iter().find(|(n, _)| n == field) {
                            if !is_pub {
                                // Check if we are inside an impl block for this type
                                let inside_impl = self.current_impl_type.as_deref()
                                    == Some(struct_name.as_str());
                                if !inside_impl {
                                    return Err(self.err(
                                        span,
                                        format!(
                                            "field '{}' of type '{}' is private",
                                            field, struct_name
                                        ),
                                    ));
                                }
                            }
                        }
                    }
                }
                // P103: a field the struct does not have. Refused HERE rather
                // than left to `field_slot_index`, which has no slot for it and
                // reads SLOT 0 — so the program runs and returns a different
                // field's value, on both backends, with `check` exiting 0.
                //
                // Only when the struct is KNOWN. An unresolved receiver is
                // `ManiType::Unknown` and is P95's finding, not this one; a
                // tuple has its own arm in `resolve_field_type`; and a generic
                // struct's field NAMES do not depend on its type arguments,
                // which is why membership in `self.structs` is the right
                // question even when the field's TYPE is still `Unknown`
                // (P68).
                //
                // Returned as an `Err` for P70's measured reason: `main` prints
                // warnings only after `analyze` RETURNS, so `analyze`'s `?` on
                // the first type error discards them, and every observable case
                // of this defect is one where something else goes wrong later.
                if let ManiType::Struct(sname, _) = &tobj.ty {
                    if self
                        .warnings
                        .effective_level(&WarningKind::UnknownField)
                        .is_error()
                    {
                        if let Some(fields) = self.structs.get(sname.as_str()) {
                            if !fields.iter().any(|(n, _)| n == field) {
                                let hint = did_you_mean(
                                    field,
                                    fields.iter().map(|(n, _)| n.clone()),
                                )
                                .unwrap_or_default();
                                let sname = sname.clone();
                                return Err(self.err(
                                    span,
                                    format!(
                                        "`{sname}` has no field `{field}`{hint}. It used to \
                                         resolve to the unknown type, so the program \
                                         type-checked and the read took SLOT 0 — a \
                                         different field's value, on both backends. \
                                         (`lint allow(undeclared-field);` restores the \
                                         previous behaviour, in which this was silent.)"
                                    ),
                                ));
                            }
                        }
                    }
                }
                let field_ty = self.resolve_field_type(&tobj.ty, field);
                Ok(TypedExpr {
                    kind: TypedExprKind::Field(Box::new(tobj), field.clone()),
                    ty: field_ty,
                    span,
                })
            }

            Expr::Block(block) => {
                let tb = self.check_block(block)?;
                let ty = tb.ty.clone();
                Ok(TypedExpr { kind: TypedExprKind::Block(tb), ty, span })
            }

            Expr::If(ie) => {
                let tcond = self.check_expr(&ie.cond, Some(&ManiType::Bool))?;
                self.check_bool_cond(&tcond, "if condition")?;
                let tthen = self.check_block(&ie.then_block)?;
                let mut telif = Vec::new();
                for (econd, eblock) in &ie.elif_branches {
                    let tec = self.check_expr(econd, Some(&ManiType::Bool))?;
                    self.check_bool_cond(&tec, "elif condition")?;
                    let teb = self.check_block(eblock)?;
                    telif.push((tec, teb));
                }
                let telse = if let Some(eb) = &ie.else_block {
                    Some(self.check_block(eb)?)
                } else {
                    None
                };
                // S7: when the `if` can produce a value (it has an else), all
                // branches with known non-void types must agree.
                if let Some(eb) = &telse {
                    let mut parts = vec![tthen.ty.clone()];
                    parts.extend(telif.iter().map(|(_, b)| b.ty.clone()));
                    parts.push(eb.ty.clone());
                    self.check_branch_agreement(&parts, "if branches", span)?;
                }
                // An `if` with no `else` is a statement and has no value; with
                // one, its type comes from every branch rather than from the
                // `else` alone — see unify_branch_type.
                let ty = match &telse {
                    None => ManiType::Void,
                    Some(eb) => {
                        let mut arms: Vec<&TypedBlock> = vec![&tthen];
                        arms.extend(telif.iter().map(|(_, b)| b));
                        arms.push(eb);
                        Self::unify_branch_type(&arms, eb.ty.clone())
                    }
                };
                Ok(TypedExpr {
                    kind: TypedExprKind::If(TypedIfExpr {
                        cond: Box::new(tcond),
                        then_block: tthen,
                        elif_branches: telif,
                        else_block: telse,
                    }),
                    ty,
                    span,
                })
            }

            Expr::Tif(te) => {
                let tcond = self.check_expr(&te.cond, Some(&ManiType::Trit))?;
                // tif condition must be trit or bool3 — enforce for KNOWN types;
                // Unknown (the permissive placeholder, e.g. generics) is allowed.
                if tcond.ty.is_known()
                    && tcond.ty != ManiType::Trit
                    && tcond.ty != ManiType::Bool3
                {
                    return Err(self.err(
                        te.cond.span(),
                        format!(
                            "tif condition must be `trit` or `bool3`, found `{}`",
                            tcond.ty.display()
                        ),
                    ));
                }
                let tpos = self.check_block(&te.pos_block)?;
                let tzero = self.check_block(&te.zero_block)?;
                let tneg = self.check_block(&te.neg_block)?;
                // S7: all three arms must agree when they produce values.
                self.check_branch_agreement(
                    &[tpos.ty.clone(), tzero.ty.clone(), tneg.ty.clone()],
                    "tif arms",
                    span,
                )?;
                let ty = Self::unify_branch_type(&[&tpos, &tzero, &tneg], tpos.ty.clone());
                Ok(TypedExpr {
                    kind: TypedExprKind::Tif(TypedTifExpr {
                        cond: Box::new(tcond),
                        pos_block: tpos,
                        zero_block: tzero,
                        neg_block: tneg,
                    }),
                    ty,
                    span,
                })
            }

            Expr::Match(me) => {
                let tscrutinee = self.check_expr(&me.scrutinee, None)?;
                let mut typed_arms = Vec::new();
                let mut arm_ty = ManiType::Void;
                let mut arm_tys = Vec::new();
                for arm in &me.arms {
                    // Pattern bindings (e.g. `Ok(v) => ...`) live in a per-arm
                    // scope and are visible to both the guard and the body.
                    self.symbols.push_scope();
                    // C6: a trit pattern reads the trits of a balanced-ternary
                    // word, so its scrutinee must be one.
                    self.check_trit_pattern_scrutinee(&arm.pattern, &tscrutinee.ty)?;
                    self.define_pattern_bindings(&arm.pattern, &tscrutinee.ty);
                    let guard = if let Some(g) = &arm.guard {
                        let tg = self.check_expr(g, Some(&ManiType::Bool))?;
                        self.check_bool_cond(&tg, "match guard")?;
                        Some(tg)
                    } else {
                        None
                    };
                    let tbody = self.check_expr(&arm.body, hint)?;
                    self.symbols.pop_scope();
                    arm_ty = tbody.ty.clone();
                    arm_tys.push(tbody.ty.clone());
                    typed_arms.push(TypedMatchArm {
                        pattern: arm.pattern.clone(),
                        guard,
                        body: tbody,
                    });
                }
                // S7: arms with known non-void types must agree.
                self.check_branch_agreement(&arm_tys, "match arms", span)?;
                // Feature 1: exhaustiveness checking for enum, trit, bool3 types
                self.check_exhaustiveness(&tscrutinee.ty, &typed_arms, span)?;
                Ok(TypedExpr {
                    kind: TypedExprKind::Match(TypedMatchExpr {
                        scrutinee: Box::new(tscrutinee),
                        arms: typed_arms,
                    }),
                    ty: arm_ty,
                    span,
                })
            }

            Expr::For(fe) => {
                let titer = self.check_expr(&fe.iter, None)?;
                let elem_ty = self.iter_elem_type(&titer.ty);
                self.symbols.push_scope();
                self.symbols.define(&fe.var, elem_ty, false);
                let tbody = self.check_block(&fe.body)?;
                self.symbols.pop_scope();
                Ok(TypedExpr {
                    kind: TypedExprKind::For(TypedForExpr {
                        var: fe.var.clone(),
                        iter: Box::new(titer),
                        body: tbody,
                    }),
                    ty: ManiType::Void,
                    span,
                })
            }

            Expr::While(we) => {
                let tcond = self.check_expr(&we.cond, Some(&ManiType::Bool))?;
                self.check_bool_cond(&tcond, "while condition")?;
                let tbody = self.check_block(&we.body)?;
                Ok(TypedExpr {
                    kind: TypedExprKind::While(TypedWhileExpr {
                        cond: Box::new(tcond),
                        body: tbody,
                    }),
                    ty: ManiType::Void,
                    span,
                })
            }

            Expr::Loop(block, _) => {
                let tbody = self.check_block(block)?;
                Ok(TypedExpr {
                    kind: TypedExprKind::Loop(tbody),
                    ty: ManiType::Void,
                    span,
                })
            }

            Expr::Array(elems, _) => {
                // Derive element type hint from outer array hint
                let mut outer_elem_hint: Option<ManiType> = match hint {
                    Some(ManiType::Array(ref et, _)) => Some(*et.clone()),
                    _ => None,
                };

                // With no usable hint, the element type is the type of the
                // whole literal, not of its first element. `[0, 0, +]` is a
                // trit array whose first two entries happen to be writable as
                // integer literals; typing it from element 0 made it an int
                // array, and the trit at index 2 was then stored one byte wide
                // into eight-byte slots. That mis-shaped array reached the
                // runtime helpers and segfaulted.
                //
                // Many stdlib signatures are still Unknown to the analyzer
                // (report.txt S3-S8), so "no usable hint" is the common case
                // for exactly the calls that care: pack_trits, trits_to_str.
                if outer_elem_hint.as_ref().is_none_or(|t| !t.is_known()) && !elems.is_empty() {
                    let mut probe: Option<ManiType> = None;
                    for e in elems {
                        let te = self.check_expr(e, None)?;
                        // A bare integer literal is the weakest claim on the
                        // element type: 0 is a legal trit, tryte, t9 and t27
                        // as well as an int. Anything more specific wins.
                        let specific = te.ty.is_ternary()
                            || matches!(te.ty, ManiType::Float | ManiType::Str | ManiType::Char);
                        if specific && probe.as_ref().is_none_or(|p| !p.is_ternary()) {
                            probe = Some(te.ty.clone());
                        } else if probe.is_none() {
                            probe = Some(te.ty.clone());
                        }
                    }
                    outer_elem_hint = probe.filter(|t| t.is_known());
                }

                let mut typed_elems = Vec::new();
                let mut elem_ty = outer_elem_hint.clone().unwrap_or(ManiType::Unknown);
                for e in elems {
                    let eh = outer_elem_hint.as_ref().or(if elem_ty == ManiType::Unknown { None } else { Some(&elem_ty) });
                    let te = self.check_expr(e, eh)?;
                    if elem_ty == ManiType::Unknown {
                        elem_ty = te.ty.clone();
                    }
                    typed_elems.push(te);
                }
                let arr_ty = ManiType::Array(Box::new(elem_ty), Some(typed_elems.len()));
                Ok(TypedExpr {
                    kind: TypedExprKind::Array(typed_elems),
                    ty: arr_ty,
                    span,
                })
            }

            Expr::Tuple(elems, _) => {
                let mut typed_elems = Vec::new();
                for e in elems {
                    typed_elems.push(self.check_expr(e, None)?);
                }
                let tys = typed_elems.iter().map(|e| e.ty.clone()).collect();
                Ok(TypedExpr {
                    kind: TypedExprKind::Tuple(typed_elems),
                    ty: ManiType::Tuple(tys),
                    span,
                })
            }

            Expr::StructLit(name, fields, _) => {
                let mut typed_fields = Vec::new();
                // S1: an unknown struct name is an error, not a silent guess.
                let struct_fields = match self.structs.get(name) {
                    Some(f) => f.clone(),
                    None => {
                        let hint = did_you_mean(name, self.structs.keys().cloned())
                            .unwrap_or_default();
                        return Err(self.err(span, format!("unknown struct '{}'{}", name, hint)));
                    }
                };

                // --- Struct update syntax: { ..base_expr, field: val, ... } ---
                // The parser encodes the base as a sentinel ("__spread__", base_expr).
                // Expand: for every struct field NOT listed in the explicit overrides,
                // synthesise `base_expr.field_name` as the value.
                let spread_expr = fields.iter().find(|(n, _)| n == "__spread__")
                    .map(|(_, e)| e.clone());

                // S1: explicit field names must exist on the struct and be unique.
                let mut explicit_names: std::collections::HashSet<&str> =
                    std::collections::HashSet::new();
                for (fname, fval) in fields.iter().filter(|(n, _)| n != "__spread__") {
                    if !struct_fields.iter().any(|(n, _)| n == fname) {
                        let hint = did_you_mean(
                            fname,
                            struct_fields.iter().map(|(n, _)| n.clone()),
                        ).unwrap_or_default();
                        return Err(self.err(
                            fval.span(),
                            format!("struct '{}' has no field '{}'{}", name, fname, hint),
                        ));
                    }
                    if !explicit_names.insert(fname.as_str()) {
                        return Err(self.err(
                            fval.span(),
                            format!("field '{}' specified more than once in struct literal", fname),
                        ));
                    }
                }

                if let Some(base_expr) = spread_expr {
                    // Type-check the base expression (must be the same struct type).
                    let typed_base = self.check_expr(&base_expr, Some(&ManiType::Struct(name.clone(), Vec::new())))?;

                    // Build a map of explicit overrides for quick lookup.
                    let mut overrides: std::collections::HashMap<String, Expr> =
                        std::collections::HashMap::new();
                    for (fname, fval) in fields.iter().filter(|(n, _)| n != "__spread__") {
                        overrides.insert(fname.clone(), fval.clone());
                    }

                    // Emit fields IN STRUCT DEFINITION ORDER so the IR lowering
                    // (which assigns by position) gets the right slot for each value.
                    for (sfield_name, sfield_ty) in &struct_fields {
                        if let Some(fval) = overrides.get(sfield_name.as_str()) {
                            // Explicit override for this field.
                            let tval = self.check_expr(fval, Some(sfield_ty))?;
                            typed_fields.push((sfield_name.clone(), tval));
                        } else {
                            // Inherit from base: base.field_name
                            let field_access = TypedExpr {
                                kind: TypedExprKind::Field(
                                    Box::new(typed_base.clone()),
                                    sfield_name.clone(),
                                ),
                                ty: sfield_ty.clone(),
                                span,
                            };
                            typed_fields.push((sfield_name.clone(), field_access));
                        }
                    }
                } else {
                    // Normal struct literal — no spread.
                    // S1: every declared field must be provided.
                    let missing: Vec<&str> = struct_fields
                        .iter()
                        .filter(|(n, _)| !explicit_names.contains(n.as_str()))
                        .map(|(n, _)| n.as_str())
                        .collect();
                    if !missing.is_empty() {
                        return Err(self.err(span, format!(
                            "missing field{} {} in literal of struct '{}'",
                            if missing.len() == 1 { "" } else { "s" },
                            missing.iter().map(|n| format!("'{}'", n))
                                .collect::<Vec<_>>().join(", "),
                            name,
                        )));
                    }
                    // S1: emit fields IN STRUCT DECLARATION ORDER — the IR
                    // lowering assigns by position (same contract as the
                    // spread branch above), so source order must not leak.
                    for (sfname, sfty) in &struct_fields {
                        let fval = fields
                            .iter()
                            .find(|(n, _)| n == sfname)
                            .map(|(_, e)| e)
                            .expect("field presence verified above");
                        let tval = self.check_expr(fval, Some(sfty))?;
                        if sfty.is_known()
                            && tval.ty.is_known()
                            && !types_compatible(sfty, &tval.ty)
                        {
                            return Err(self.err(tval.span, format!(
                                "field '{}' of struct '{}' has type `{}`, found `{}`",
                                sfname, name, sfty.display(), tval.ty.display()
                            )));
                        }
                        typed_fields.push((sfname.clone(), tval));
                    }
                }

                // P68: a generic struct literal records what its type
                // parameters turned out to be. `Box2 { a: 1.5, b: 2.5 }` is a
                // `Box2<float>`, and until this line it was a bare `Box2` —
                // which is why an `impl<T>` method had nothing to instantiate
                // against and stayed type-erased (report.txt P65's open half).
                let field_tys: Vec<(String, ManiType)> = typed_fields
                    .iter()
                    .map(|(n, e)| (n.clone(), e.ty.clone()))
                    .collect();
                let args = self
                    .struct_literal_args(name, &field_tys)
                    .unwrap_or_default();

                Ok(TypedExpr {
                    kind: TypedExprKind::StructLit(name.clone(), typed_fields),
                    ty: ManiType::Struct(name.clone(), args),
                    span,
                })
            }

            Expr::Range(lo, hi, inclusive, _) => {
                let tlo = self.check_expr(lo, Some(&ManiType::Int))?;
                let thi = self.check_expr(hi, Some(&ManiType::Int))?;
                Ok(TypedExpr {
                    kind: TypedExprKind::Range(Box::new(tlo), Box::new(thi), *inclusive),
                    ty: ManiType::Generic("Range".to_string(), vec![ManiType::Int]),
                    span,
                })
            }

            Expr::Cast(inner, ty, _) => {
                let tinner = self.check_expr(inner, None)?;
                let cast_ty = self.resolve_type(ty)?;
                Ok(TypedExpr {
                    kind: TypedExprKind::Cast(Box::new(tinner), cast_ty.clone()),
                    ty: cast_ty,
                    span,
                })
            }

            Expr::Question(inner, _) => {
                let tinner = self.check_expr(inner, hint)?;
                let unwrapped_ty = match &tinner.ty {
                    ManiType::Generic(name, args) if name == "Result" => {
                        args.first().cloned().unwrap_or(ManiType::Unknown)
                    }
                    other => other.clone(),
                };
                Ok(TypedExpr {
                    kind: TypedExprKind::Question(Box::new(tinner)),
                    ty: unwrapped_ty,
                    span,
                })
            }

            Expr::Return(e, _) => {
                let ret_hint = self.current_fn_ret.clone();
                let te = self.check_expr(e, Some(&ret_hint))?;
                // The EXPRESSION form of `return` — what a `tif` arm contains.
                // Same rule as the statement form, and deliberately the same
                // function: see check_return_value_allowed in stmts.rs for why
                // it is not written out twice.
                self.check_return_value_allowed(&te.ty, span)?;
                Ok(TypedExpr {
                    kind: TypedExprKind::Return(Box::new(te)),
                    ty: ManiType::Void,
                    span,
                })
            }

            Expr::Break(_) => Ok(TypedExpr {
                kind: TypedExprKind::Break,
                ty: ManiType::Void,
                span,
            }),

            Expr::Continue(_) => Ok(TypedExpr {
                kind: TypedExprKind::Continue,
                ty: ManiType::Void,
                span,
            }),

            // §11.12 (AWAIT). `await h` on a `Task<T>` has type `T`: a task
            // handle is a one-shot channel of capacity one that the task sends
            // to when it terminates, and `await` is its `recv`.
            //
            // **Anything else keeps the identity typing it has always had**,
            // and that is a decision rather than an oversight. The only live
            // `await` in either repository is `fetch_data(id).await` in
            // `examples/concurrency.mt`, applied to an `async fn` result —
            // a surface §11 does not specify. Typing `Task<T>` and leaving the
            // rest alone moves exactly the construct §11.12 defines.
            Expr::Await(inner, _) => {
                let tinner = self.check_expr(inner, None)?;
                let ty = match &tinner.ty {
                    ManiType::Generic(n, args) if n == "Task" => {
                        args.first().cloned().unwrap_or(ManiType::Unknown)
                    }
                    other => other.clone(),
                };
                Ok(TypedExpr {
                    kind: TypedExprKind::Await(Box::new(tinner)),
                    ty,
                    span,
                })
            }

            // §11.4: `yield` is a yield POINT and produces no value. It type-
            // checks under every scheduling mode — the mode decides what it
            // COMPILES to, and under `inline` it is a no-op, because §4's
            // single task has nothing to yield to.
            Expr::Yield(_) => Ok(TypedExpr {
                kind: TypedExprKind::Yield,
                ty: ManiType::Void,
                span,
            }),

            Expr::Spawn(block, _) => {
                // §11.2's copy of the store, computed with the SAME walker that
                // refuses lambda capture twenty lines below. Only genuine
                // locals of an enclosing scope travel: a global or a function
                // name is reachable from the outlined body by its own name, and
                // `lookup_with_depth`'s `depth >= 1` is what tells them apart —
                // the test the lambda path already uses.
                let mut bound: Vec<std::collections::HashSet<String>> = Vec::new();
                let mut free: std::collections::HashSet<String> = Default::default();
                collect_free_in_block(block, &mut bound, &mut free);
                let mut captures: Vec<(String, ManiType)> = free
                    .iter()
                    .filter_map(|v| match self.symbols.lookup_with_depth(v) {
                        Some((depth, sym)) if depth >= 1 => Some((v.clone(), sym.ty.clone())),
                        _ => None,
                    })
                    .collect();
                // Sorted, because a HashSet's order is not stable across runs
                // and the env layout has to be the same one the outlined body
                // reads back. §11.7's determinism is about the schedule; this
                // is determinism of the CODE, and it is just as required.
                captures.sort_by(|a, b| a.0.cmp(&b.0));
                let tb = self.check_block(block)?;
                // §11.12: `spawn { B } : Task<T>` where `T` is the value of
                // `B`. Used as a STATEMENT its value is discarded like any
                // expression statement's, which is why every existing program
                // is unmoved — the handle is a return value rather than a
                // change to the form.
                let ty = ManiType::Generic("Task".to_string(), vec![tb.ty.clone()]);
                Ok(TypedExpr {
                    kind: TypedExprKind::Spawn(tb, captures),
                    ty,
                    span,
                })
            }

            Expr::Lambda(params, ret_ty_opt, body, _) => {
                // Generate a unique global function name for this lambda
                let name = format!("__lambda_{}", self.lambda_counter);
                self.lambda_counter += 1;

                // Resolve param types
                let mut param_mani_tys = Vec::new();
                let mut typed_params = Vec::new();
                let mut param_names = std::collections::HashSet::new();
                for (pname, pty) in params {
                    let mty = self.resolve_type(pty)?;
                    param_mani_tys.push(mty.clone());
                    typed_params.push(TypedParam {
                        name: pname.clone(),
                        ty: mty,
                        // A lambda parameter: `move` is not spellable there
                        // yet, and lambdas cannot capture at all (P55), so
                        // there is nothing for it to protect.
                        is_move: false,
                    });
                    param_names.insert(pname.clone());
                }
                let ret_mani = if let Some(rt) = ret_ty_opt {
                    self.resolve_type(rt)?
                } else {
                    ManiType::Unknown
                };

                // Closure-capture detection (S2): walk the lambda body's own AST
                // collecting free identifiers (reads not bound by the lambda's
                // params or its local declarations). Any free name that resolves
                // to an enclosing FUNCTION-LOCAL variable is a capture — not
                // supported. Module-level globals (scope depth 0) are fine.
                let mut lambda_bound: Vec<std::collections::HashSet<String>> =
                    vec![param_names.iter().cloned().collect()];
                let mut free_names: std::collections::HashSet<String> =
                    std::collections::HashSet::new();
                collect_free_idents(body, &mut lambda_bound, &mut free_names);
                let mut captured: Vec<&String> = free_names
                    .iter()
                    .filter(|v| {
                        matches!(self.symbols.lookup_with_depth(v), Some((depth, _)) if depth >= 1)
                    })
                    .collect();
                captured.sort();
                if let Some(v) = captured.first() {
                    return Err(self.err(
                        span,
                        format!(
                            "lambda captures outer variable '{}' — closures are not yet supported; use a parameter instead",
                            v
                        ),
                    ));
                }

                // Type-check the body with a fresh scope containing only the params
                self.symbols.push_scope();
                for tp in &typed_params {
                    self.symbols.define(&tp.name, tp.ty.clone(), false);
                }
                let prev_ret = self.current_fn_ret.clone();
                self.current_fn_ret = ret_mani.clone();
                let tbody = self.check_expr(body, Some(&ret_mani))?;
                self.current_fn_ret = prev_ret;
                self.symbols.pop_scope();

                // Wrap body in a block that returns the expression
                let body_block = TypedBlock {
                    stmts: vec![TypedStmt::Return(Some(tbody))],
                    ty: ret_mani.clone(),
                };

                // Register as a known function
                self.functions.insert(name.clone(), (param_mani_tys.clone(), ret_mani.clone()));

                // Create TypedFnDef and store for later
                let lambda_fn = TypedFnDef {
                    name: name.clone(),
                    params: typed_params,
                    ret_ty: ret_mani.clone(),
                    body: Some(body_block),
                    is_pub: false,
                    is_async: false,
                };
                self.lambda_fns.push(lambda_fn);

                // The lambda expression evaluates to the function's label (as an int/ptr)
                let fn_ty = ManiType::Fn(param_mani_tys, Box::new(ret_mani));
                Ok(TypedExpr {
                    kind: TypedExprKind::Ident(name),
                    ty: fn_ty,
                    span,
                })
            }

            Expr::Tresult(tr) => {
                let texpr = self.check_expr(&tr.expr, None)?;
                let expr_ty = texpr.ty.clone();

                // Each arm gets a scope with its binding variable
                self.symbols.push_scope();
                self.symbols.define(&tr.ok_var, expr_ty.clone(), false);
                let ok_block = self.check_block(&tr.ok_block)?;
                self.symbols.pop_scope();

                self.symbols.push_scope();
                self.symbols.define(&tr.unknown_var, expr_ty.clone(), false);
                let unknown_block = self.check_block(&tr.unknown_block)?;
                self.symbols.pop_scope();

                self.symbols.push_scope();
                self.symbols.define(&tr.err_var, expr_ty.clone(), false);
                let err_block = self.check_block(&tr.err_block)?;
                self.symbols.pop_scope();

                let ty = ok_block.ty.clone();
                Ok(TypedExpr {
                    kind: TypedExprKind::Tresult(TypedTresultExpr {
                        expr: Box::new(texpr),
                        ok_var: tr.ok_var.clone(),
                        ok_block,
                        unknown_var: tr.unknown_var.clone(),
                        unknown_block,
                        err_var: tr.err_var.clone(),
                        err_block,
                    }),
                    ty,
                    span,
                })
            }
        }
    }

    // -----------------------------------------------------------------------
    // Condition / branch-type helpers
    // -----------------------------------------------------------------------

    /// P43: how many values `EnumName::Variant` carries, if it names one.
    pub(crate) fn enum_variant_arity(&self, name: &str) -> Option<usize> {
        let sep = name.find("::")?;
        let variants = self.enums.get(&name[..sep])?;
        let variant_name = &name[sep + 2..];
        variants
            .iter()
            .find(|(v, _)| v == variant_name)
            .map(|(_, fields)| fields.len())
    }

    /// If `name` is an `EnumName::Variant` path of a known enum, return the
    /// enum's name.
    fn enum_variant_path(&self, name: &str) -> Option<String> {
        let sep = name.find("::")?;
        let enum_name = &name[..sep];
        let variant_name = &name[sep + 2..];
        let variants = self.enums.get(enum_name)?;
        if variants.iter().any(|(v, _)| v == variant_name) {
            Some(enum_name.to_string())
        } else {
            None
        }
    }

    /// S8: enforce that a condition (if/elif/while, match guard) is `bool`
    /// when its type is KNOWN; `Unknown` stays permissive by design.
    pub(crate) fn check_bool_cond(&self, cond: &TypedExpr, what: &str) -> CompileResult<()> {
        if cond.ty.is_known() && cond.ty != ManiType::Bool {
            return Err(self.err(
                cond.span,
                format!("{} must be `bool`, found `{}`", what, cond.ty.display()),
            ));
        }
        Ok(())
    }

    /// S7: branches of a value-producing construct (if/match/tif) must agree.
    /// Branches typed `Void` (statement position or diverging via
    /// return/break/continue) and `Unknown` are tolerated; two KNOWN,
    /// non-void, incompatible branch types are an error.
    /// The result type of a multi-armed expression, chosen from ALL the arms.
    ///
    /// Each site used to take one arm and call it the answer: a `tif` took its
    /// first arm, an `if` took its `else`. That is arbitrary whenever the arms
    /// are compatible but not identical, and in a ternary language they very
    /// often are — a bare `0` is a valid `int` AND a valid `trit`, so which one
    /// it is depends on what sits beside it.
    ///
    /// It produced IR that would not assemble (ORACLE_FINDINGS.md Section 15):
    ///
    /// ```text
    /// tif i { + => +, 0 => +, - => 0 }   typed trit — first arm is a trit
    /// tif i { + => 0, 0 => -, - => - }   typed INT  — first arm is `0`
    /// ```
    ///
    /// Two spellings of the same three-valued function, typed differently
    /// because of which arm came first. Nested inside a `trit`-valued `tif`,
    /// the second fed an `i64` into an `i8` phi and clang rejected the module:
    /// `'%t8' defined with type 'i64' but expected 'i8'`. T3 compiled the same
    /// source correctly, since every value there is one word — so this is a
    /// fault the two-backend oracle DID see, and saw as a build failure rather
    /// than a wrong answer.
    ///
    /// The rule: if any arm has a ternary type, and every other arm is a bare
    /// integer literal that is also a valid trit, the whole expression is that
    /// ternary type. Otherwise `fallback`, which each caller supplies as the arm
    /// it used to take unconditionally — so this only ever CHANGES the answer in
    /// the mixed-ternary case, and every other program types exactly as before.
    ///
    /// A literal is required. An arm of `int` type that is not a literal keeps
    /// the expression `int`: narrowing a computed integer to a trit would turn a
    /// build failure into a wrong answer, which is the worse trade.
    pub(crate) fn unify_branch_type(arms: &[&TypedBlock], fallback: ManiType) -> ManiType {
        let mut ternary: Option<ManiType> = None;
        let mut others_all_trit_literals = true;

        for b in arms {
            if !b.ty.is_known() || b.ty == ManiType::Void {
                continue;
            }
            if b.ty.is_ternary() {
                if ternary.is_none() {
                    ternary = Some(b.ty.clone());
                }
            } else if !(b.ty == ManiType::Int && Self::tail_is_trit_literal(b)) {
                others_all_trit_literals = false;
            }
        }

        match ternary {
            Some(t) if others_all_trit_literals => t,
            _ => fallback,
        }
    }

    /// Does this block's value come from an integer literal that is also a
    /// valid trit — that is, one of -1, 0, +1?
    fn tail_is_trit_literal(b: &TypedBlock) -> bool {
        match b.stmts.last() {
            Some(TypedStmt::Expr(te)) => matches!(
                &te.kind,
                TypedExprKind::Lit(crate::ast::Lit::Int(n)) if (-1..=1).contains(n)
            ),
            _ => false,
        }
    }

    pub(crate) fn check_branch_agreement(
        &self,
        branch_tys: &[ManiType],
        what: &str,
        span: Span,
    ) -> CompileResult<()> {
        let mut first: Option<&ManiType> = None;
        for ty in branch_tys {
            if !ty.is_known() || *ty == ManiType::Void {
                continue;
            }
            match first {
                None => first = Some(ty),
                Some(fty) => {
                    if !types_compatible(fty, ty) {
                        return Err(self.err(span, format!(
                            "{} have incompatible types: `{}` vs `{}`",
                            what, fty.display(), ty.display()
                        )));
                    }
                }
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Free-identifier collection for lambda capture detection (S2)
// ---------------------------------------------------------------------------

fn bind_name(bound: &mut [std::collections::HashSet<String>], name: &str) {
    if let Some(scope) = bound.last_mut() {
        scope.insert(name.to_string());
    }
}

fn is_bound(bound: &[std::collections::HashSet<String>], name: &str) -> bool {
    bound.iter().any(|s| s.contains(name))
}

fn bind_pattern_names(pat: &Pattern, bound: &mut [std::collections::HashSet<String>]) {
    match pat {
        Pattern::Wildcard(_) | Pattern::Lit(_, _) => {}
        Pattern::Ident(n, _) => bind_name(bound, n),
        // C6: a trit pattern's captures are ordinary bindings. This walker is
        // what `collect_free_in_block` uses to decide a spawned block's
        // captures (P99b), so a missing arm here is a silently wrong program,
        // not a diagnostic.
        Pattern::Trit(tp, _) => {
            for n in tp.bound_names() {
                bind_name(bound, &n);
            }
        }
        Pattern::Tuple(ps, _) | Pattern::Or(ps, _) | Pattern::Enum(_, _, ps, _) => {
            for p in ps {
                bind_pattern_names(p, bound);
            }
        }
        Pattern::Struct(_, fields, _) => {
            for (_, p) in fields {
                bind_pattern_names(p, bound);
            }
        }
    }
}

fn collect_free_in_block(
    block: &Block,
    bound: &mut Vec<std::collections::HashSet<String>>,
    free: &mut std::collections::HashSet<String>,
) {
    bound.push(Default::default());
    for stmt in &block.stmts {
        match stmt {
            Stmt::Let(ls) => {
                // The initialiser is evaluated BEFORE the binding exists.
                if let Some(init) = &ls.init {
                    collect_free_idents(init, bound, free);
                }
                match &ls.pat {
                    ast::LetPat::Ident(n) => bind_name(bound, n),
                    ast::LetPat::Tuple(names) => {
                        for n in names {
                            bind_name(bound, n);
                        }
                    }
                }
            }
            Stmt::Assign(a) => {
                collect_free_idents(&a.value, bound, free);
                collect_free_idents(&a.target, bound, free);
            }
            Stmt::Expr(e) => collect_free_idents(e, bound, free),
            Stmt::Return(Some(e), _) => collect_free_idents(e, bound, free),
            Stmt::Return(None, _) | Stmt::Break(_) | Stmt::Continue(_) => {}
            Stmt::LocalStructDef(_) => {}
        }
    }
    bound.pop();
}

/// Collect every identifier read by `expr` that is not bound within `expr`
/// itself (or by an enclosing `bound` scope). Path identifiers (`a::b`) are
/// constants/functions, never local variables — they are skipped.
fn collect_free_idents(
    expr: &Expr,
    bound: &mut Vec<std::collections::HashSet<String>>,
    free: &mut std::collections::HashSet<String>,
) {
    match expr {
        Expr::Lit(_, _) | Expr::Break(_) | Expr::Continue(_) => {}
        Expr::Ident(name, _) => {
            if !name.contains("::") && !is_bound(bound, name) {
                free.insert(name.clone());
            }
        }
        Expr::BinOp(l, _, r, _) => {
            collect_free_idents(l, bound, free);
            collect_free_idents(r, bound, free);
        }
        Expr::UnOp(_, e, _)
        | Expr::Await(e, _)
        | Expr::Return(e, _)
        | Expr::Cast(e, _, _)
        | Expr::Question(e, _) => collect_free_idents(e, bound, free),
        Expr::Call(callee, args, _) => {
            collect_free_idents(callee, bound, free);
            for a in args {
                collect_free_idents(a, bound, free);
            }
        }
        Expr::MethodCall(obj, _, args, _) => {
            collect_free_idents(obj, bound, free);
            for a in args {
                collect_free_idents(a, bound, free);
            }
        }
        Expr::Index(a, i, _) => {
            collect_free_idents(a, bound, free);
            collect_free_idents(i, bound, free);
        }
        Expr::Field(obj, _, _) => collect_free_idents(obj, bound, free),
        Expr::Block(b) => collect_free_in_block(b, bound, free),
        Expr::If(ie) => {
            collect_free_idents(&ie.cond, bound, free);
            collect_free_in_block(&ie.then_block, bound, free);
            for (c, b) in &ie.elif_branches {
                collect_free_idents(c, bound, free);
                collect_free_in_block(b, bound, free);
            }
            if let Some(eb) = &ie.else_block {
                collect_free_in_block(eb, bound, free);
            }
        }
        Expr::Tif(te) => {
            collect_free_idents(&te.cond, bound, free);
            collect_free_in_block(&te.pos_block, bound, free);
            collect_free_in_block(&te.zero_block, bound, free);
            collect_free_in_block(&te.neg_block, bound, free);
        }
        Expr::Match(me) => {
            collect_free_idents(&me.scrutinee, bound, free);
            for arm in &me.arms {
                bound.push(Default::default());
                bind_pattern_names(&arm.pattern, bound);
                if let Some(g) = &arm.guard {
                    collect_free_idents(g, bound, free);
                }
                collect_free_idents(&arm.body, bound, free);
                bound.pop();
            }
        }
        Expr::For(fe) => {
            collect_free_idents(&fe.iter, bound, free);
            bound.push(Default::default());
            bind_name(bound, &fe.var);
            collect_free_in_block(&fe.body, bound, free);
            bound.pop();
        }
        Expr::While(we) => {
            collect_free_idents(&we.cond, bound, free);
            collect_free_in_block(&we.body, bound, free);
        }
        Expr::Loop(b, _) | Expr::Spawn(b, _) => collect_free_in_block(b, bound, free),
        // §11.4: `yield` reads nothing, so it captures nothing.
        Expr::Yield(_) => {}
        Expr::Array(elems, _) | Expr::Tuple(elems, _) => {
            for e in elems {
                collect_free_idents(e, bound, free);
            }
        }
        Expr::StructLit(_, fields, _) => {
            for (_, e) in fields {
                collect_free_idents(e, bound, free);
            }
        }
        Expr::Range(lo, hi, _, _) => {
            collect_free_idents(lo, bound, free);
            collect_free_idents(hi, bound, free);
        }
        Expr::Lambda(params, _, body, _) => {
            bound.push(params.iter().map(|(n, _)| n.clone()).collect());
            collect_free_idents(body, bound, free);
            bound.pop();
        }
        Expr::Tresult(tr) => {
            collect_free_idents(&tr.expr, bound, free);
            for (var, block) in [
                (&tr.ok_var, &tr.ok_block),
                (&tr.unknown_var, &tr.unknown_block),
                (&tr.err_var, &tr.err_block),
            ] {
                bound.push(Default::default());
                bind_name(bound, var);
                collect_free_in_block(block, bound, free);
                bound.pop();
            }
        }
    }
}
