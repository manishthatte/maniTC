//! Expansion of ManiT-source standard library modules.
//!
//! Many stdlib modules (io, sync, fs, …) are *native*: their `.mt` files carry
//! only documented signatures, and the implementations live in the C runtime
//! (LLVM backend) or in emulator syscalls / emitter intrinsics (T3 backend).
//! Others are implemented *in ManiT itself*, wholly or in part — see
//! `SOURCE_MODULES` below, which is the authority and carries the reason each
//! one is there. Three were wholly ManiT from the start:
//!
//!   * `std::bridge` — binary/ternary conversion (Claim 17)
//!   * `std::crypto` — ternary hash / HMAC / TRNG / cipher (Thatte5)
//!   * `std::t27f`   — balanced ternary floating point
//!
//! and `ternary`, `str`, `fmt`, `math`, `test` and `env` are *mixed*: natives
//! where a backend lowers the primitive directly, ManiT source for everything
//! derivable from those primitives, so one body serves both backends.
//!
//! Neither backend has native implementations for these bodies, so they
//! must be compiled into the program that uses them. This pass runs
//! on the AST before semantic analysis and, for every `use std::<m>` of a
//! source-implemented module:
//!
//!   1. parses the embedded module source,
//!   2. renames its functions to their qualified form (`m::f`) and rewrites
//!      intra-module calls to match,
//!   3. inlines module-level constants at every use site (module bodies use
//!      the bare name, the host program the qualified `m::NAME`) — globals
//!      that the module itself assigns are kept as real globals under their
//!      qualified name instead,
//!   4. registers the module's structs/enums under their bare names and
//!      rewrites qualified type references (`m::T`) to match, and
//!   5. appends the transformed items to the program.
//!
//! Call sites in the host program already use qualified names
//! (`bridge::bits_to_trit(...)`), so after the rename they resolve to the
//! merged definitions with no further changes. The IR lowerer and both
//! backends then treat the module functions as ordinary user functions.

use std::collections::{HashMap, HashSet};

use crate::ast::*;
use crate::error::CompileResult;

/// The stdlib modules whose implementations are ManiT source.
const SOURCE_MODULES: &[(&str, &str)] = &[
    ("bridge", include_str!("../../stdlib/bridge.mt")),
    ("crypto", include_str!("../../stdlib/crypto.mt")),
    ("t27f", include_str!("../../stdlib/t27f.mt")),
    // `tritfs` — the ternary filesystem. It is wholly ManiT source with no
    // native implementation on either backend, exactly like the three above,
    // and it was MISSING FROM THIS LIST while being registered as a known
    // module in `analyzer/mod.rs::STDLIB_MODULES` (report.txt P60). Two
    // registries, one authoritative for ACCEPTING a call and one for EMITTING
    // it: a module in the first and absent from the second type-checks and
    // then fails to link, on both backends.
    //
    // That is N1 verbatim, one module later. N1 is why this pass exists; the
    // fix was written, documented, and declares itself the authority — and a
    // fourth module of exactly the kind it was written for never reached it.
    ("tritfs", include_str!("../../stdlib/tritfs.mt")),
    // `ternary` is the one mixed module: the primitives the backends lower
    // directly stay `// native` declarations, and everything built on top of
    // them is ManiT source so both backends get it from one definition.
    ("ternary", include_str!("../../stdlib/ternary.mt")),
    // `str` is mixed the same way: len/slice/find/contains/concat/split and
    // the char-typed formatters stay native, everything derivable from them is
    // ManiT source. Method syntax reaches the same symbols — `s.reverse()`
    // lowers to a call to `str::reverse` (ir/lower/lower_expr.rs) — so one
    // body serves both spellings.
    ("str", include_str!("../../stdlib/str.mt")),
    // `fmt` joined them on 20 August 2026. It was wholly native, and 25 of its
    // 31 functions had no implementation on either backend — the module header
    // documented names that failed at link. They are now ManiT source over
    // str:: and ternary::, leaving only format/show_int/show_float/show_bool
    // native, because those four are what everything else is written in terms
    // of.
    ("fmt", include_str!("../../stdlib/fmt.mt")),
    // `math` joined them on 20 August 2026, and it was the worst of the three:
    // a census measured **3 of 52** functions working on both backends. The T3
    // emitter has exactly three `math::` intercepts — trit_count,
    // to_balanced_ternary, from_balanced_ternary — and 35 of the 52 names have
    // no LLVM declare either, so most of the module was documentation for
    // functions that did not exist anywhere.
    //
    // Its module-level constants were 0 of 8, and one of them
    // (`INT_MIN = -9223372036854775808`) was not even LEXABLE, which is why
    // this entry could not have been added before that line was fixed.
    ("math", include_str!("../../stdlib/math.mt")),
    // `test` joined them on 23 August 2026, and unlike the others it was never
    // native and never half-written: it did not exist at all. Three of maniTC's
    // own shipped tests -- 18_short_circuit, 19_bridge, 20_t27f_float -- called
    // `assert(...)` a combined 57 times against a function defined in no
    // stdlib module, no C runtime and no emitter intrinsic, so all three failed
    // to build on BOTH backends and had therefore never once been executed.
    // It is ManiT source for the same reason the others are: `io::println` and
    // `env::exit` are all an assertion needs, and both already work on T3.
    ("test", include_str!("../../stdlib/test.mt")),
    // `env` joined them on 23 August 2026, for one function out of 27:
    // `args()`. It is mixed in the `str`/`ternary` sense — everything else in
    // the module is a scalar native the backends lower directly — but `args()`
    // returns a `Vec<str>`, and a Vec is the one return type the two backends
    // build by completely unrelated means: the C runtime on one side, an
    // emulator heap object on the other. Written as a native it needed two
    // implementations; it had ZERO on LLVM (no `env_args` symbol) and on T3 a
    // syscall that returned an empty Vec to every caller. Written in ManiT over
    // `argc()` and `arg(i)` it needs none, and the backends cannot disagree.
    ("env", include_str!("../../stdlib/env.mt")),
    // `trit` joined them on 24 August 2026 as C7. It is mixed: four native
    // intrinsics lowered straight to IR, and two derived functions that are
    // ordinary ManiT because they are not single instructions. Unlike `math`,
    // whose natives are intercepted separately in each emitter, the natives
    // here are lowered ONCE in ir/lower/lower_expr.rs — see the module header
    // for why that difference matters.
    ("trit", include_str!("../../stdlib/trit.mt")),
];

/// Expand any used source-implemented stdlib modules into `program`.
/// Returns `None` when the program uses none of them (the common case).
pub fn expand(program: &Program) -> CompileResult<Option<Program>> {
    // Which source modules does the program import?
    let mut used: Vec<&str> = Vec::new();
    for item in &program.items {
        if let Item::UseDecl(u) = item {
            if u.path.len() >= 2 && u.path[0] == "std" {
                if let Some((name, _)) =
                    SOURCE_MODULES.iter().find(|(n, _)| *n == u.path[1])
                {
                    if !used.contains(name) {
                        used.push(name);
                    }
                }
            }
        }
    }

    // ...and which does it merely CALL, without importing?
    //
    // Requiring `use std::ternary;` would be a trap, because `ternary` is a
    // mixed module: `ternary::trits_to_str` is native and has always worked
    // bare, while `ternary::trit_add` is ManiT source and would not — the same
    // qualified prefix behaving two different ways depending on which function
    // you happened to pick, and failing at link time rather than at the call.
    // Referencing a module is intent enough to expand it.
    //
    // The reference set comes from the same traversal that does the rewriting
    // (Rewrite::observed), so a new expression form cannot be handled by one
    // and missed by the other.
    let (referenced, methods) = module_refs(program);
    for (name, src) in SOURCE_MODULES {
        if used.contains(name) {
            continue;
        }
        // A qualified path is unambiguous. A bare method name is not — every
        // container has a `len` — so it only counts against functions the
        // module actually IMPLEMENTS, never its native declarations. That
        // keeps `v.len()` on a Vec from dragging in the whole str module,
        // while `s.reverse()` still pulls in the body it needs.
        let by_method = methods
            .iter()
            .any(|m| bodied_fn_names(src).contains(m.trim_start_matches('.')));
        if referenced.contains(*name) || by_method {
            used.push(name);
        }
    }

    if used.is_empty() {
        return Ok(None);
    }

    // Parse each used module, plus everything it transitively depends on.
    //
    // A module pulls in a dependency two ways, and BOTH are needed — the same
    // two the host program gets above. `use std::X;` is the explicit one. A
    // bare qualified reference (`fmt::show_trit(t)` with no `use`) is the
    // implicit one, and until 20 August 2026 only the explicit one was
    // followed here.
    //
    // That asymmetry was a silent trap rather than an error. `str.mt` has no
    // `use` decls at all yet calls `fmt::show_int`, and `ternary.mt` calls both
    // `fmt::` and `math::` — all of which resolved fine, because every one of
    // those targets happens to be NATIVE, and a native needs a `declare`, not
    // an expansion. The failure only appears when a module references a
    // ManiT-SOURCE function across module lines: the callee is never queued, so
    // it is never merged, and the call fails to resolve with nothing pointing
    // at the missing `use`. `fmt.mt` carries `use std::str;` today purely
    // because it hit this on the day it was written.
    //
    // Following references here rather than requiring the `use` keeps one rule
    // for host programs and modules alike — referencing a module is intent
    // enough to expand it — and it is what stops `math` becoming a source
    // module from breaking `fmt.mt`'s bare `math::` calls.
    let mut parsed: Vec<(String, Program)> = Vec::new();
    let mut queue: Vec<String> = used.iter().map(|s| s.to_string()).collect();
    let mut seen: HashSet<String> = queue.iter().cloned().collect();
    while let Some(name) = queue.pop() {
        // P8: the STATIC name, not the queued `String`. A span's provenance is
        // `&'static str` so that `Span` stays `Copy`, and `SOURCE_MODULES`
        // already holds the only names it can be.
        let (static_name, src) = SOURCE_MODULES
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(n, s)| (*n, *s))
            .expect("queued module is always in SOURCE_MODULES");
        let file = format!("stdlib/{}.mt", static_name);
        let mut lexer = crate::lexer::Lexer::with_module(src, static_name);
        let tokens = lexer.tokenize()?;
        let mut parser = crate::parser::Parser::with_file(tokens, &file);
        let module = parser.parse()?;
        let mut deps: Vec<String> = Vec::new();
        for item in &module.items {
            if let Item::UseDecl(u) = item {
                if u.path.len() >= 2 && u.path[0] == "std" {
                    deps.push(u.path[1].clone());
                }
            }
        }
        // The reference set, from the same traversal used on the host program.
        // Method names are deliberately NOT consulted: inside a module a bare
        // `.len()` is far more likely to be a Vec than a cross-module call, and
        // a false positive here would drag a whole module into every program.
        let (referenced, _) = module_refs(&module);
        deps.extend(referenced);
        for dep in deps {
            // Never re-queue the module currently being parsed. `str` and `fmt`
            // reference each other, so the cycle is real and reachable; `seen`
            // is what terminates it.
            if SOURCE_MODULES.iter().any(|(n, _)| *n == dep) && seen.insert(dep.clone()) {
                queue.push(dep);
            }
        }
        parsed.push((name, module));
    }

    // Types the HOST program defines an `impl` for. A module's impl block on
    // the same type name is skipped rather than merged (P62).
    let host_impl_types: HashSet<String> = program
        .items
        .iter()
        .filter_map(|i| match i {
            Item::ImplBlock(b) => Some(b.ty.clone()),
            _ => None,
        })
        .collect();

    // Build the combined rewrite context and transform each module.
    let mut merged_items: Vec<Item> = Vec::new();
    // Host-program rewrites: qualified const name -> inlined initializer,
    // qualified type name -> bare type name.
    let mut host_consts: HashMap<String, Expr> = HashMap::new();
    let mut host_types: HashMap<String, String> = HashMap::new();

    for (mod_name, module) in &parsed {
        let mut fn_names: HashSet<String> = HashSet::new();
        let mut type_names: HashSet<String> = HashSet::new();
        let mut const_inits: HashMap<String, Expr> = HashMap::new();
        let mut mutable_globals: HashSet<String> = HashSet::new();

        for item in &module.items {
            match item {
                Item::FnDef(f) => {
                    fn_names.insert(f.name.clone());
                }
                Item::StructDef(s) => {
                    type_names.insert(s.name.clone());
                }
                Item::EnumDef(e) => {
                    type_names.insert(e.name.clone());
                }
                Item::GlobalVar(g) => {
                    if let Some(init) = &g.val {
                        const_inits.insert(g.name.clone(), init.clone());
                    }
                }
                _ => {}
            }
        }

        // A "constant" the module itself assigns is really mutable state and
        // must stay a global; find assignments to module-level names.
        for item in &module.items {
            if let Item::FnDef(f) = item {
                if let Some(body) = &f.body {
                    collect_assigned_globals(body, &const_inits, &mut mutable_globals);
                }
            }
        }
        for name in &mutable_globals {
            const_inits.remove(name);
        }

        let ctx = Rewrite {
            prefix: mod_name.clone(),
            fn_names,
            type_names: type_names.clone(),
            const_inits,
            mutable_globals,
            observed: Default::default(),
        };

        // Constant initializers may reference other module constants (or,
        // in principle, module functions) — rewrite them through the same
        // context before they are inlined anywhere.
        let mut rewritten_consts: HashMap<String, Expr> = HashMap::new();
        for (name, init) in &ctx.const_inits {
            let mut e = init.clone();
            ctx.rewrite_expr(&mut e);
            rewritten_consts.insert(name.clone(), e);
        }
        let ctx = Rewrite {
            const_inits: rewritten_consts,
            ..ctx
        };

        for item in &module.items {
            match item {
                // A body-less `fn` is a `// native` declaration: the backends
                // provide it, so merging it in would emit a second, empty
                // definition that shadows the real one. Skipping them lets a
                // module mix native declarations with ManiT implementations,
                // which `ternary` does.
                Item::FnDef(f) if f.body.is_none() => {}
                Item::FnDef(f) => {
                    let mut f = f.clone();
                    f.name = format!("{}::{}", mod_name, f.name);
                    if let Some(body) = &mut f.body {
                        ctx.rewrite_block(body);
                    }
                    for p in &mut f.params {
                        ctx.rewrite_type(&mut p.ty);
                    }
                    if let Some(rt) = &mut f.ret_ty {
                        ctx.rewrite_type(rt);
                    }
                    merged_items.push(Item::FnDef(f));
                }
                Item::StructDef(s) => {
                    let mut s = s.clone();
                    for fd in &mut s.fields {
                        ctx.rewrite_type(&mut fd.ty);
                    }
                    merged_items.push(Item::StructDef(s));
                }
                Item::EnumDef(e) => {
                    merged_items.push(Item::EnumDef(e.clone()));
                }
                // report.txt P61. Impl blocks used to fall into the `_ => {}`
                // below and vanish, so a module's free functions and its
                // structs expanded while its METHODS did not: `TritFS::new()`
                // type-checked (the struct is here, so the analyser resolves
                // the method) and then failed to link on both backends.
                //
                // The methods keep their bare names, unlike free functions.
                // Method resolution is by `Type::method` and the struct is
                // registered under its bare name, so qualifying them would
                // make them unreachable rather than unique.
                //
                // Body-less methods are skipped for exactly the reason
                // body-less `fn`s are, twenty lines above: a `;` body is a
                // native declaration the backends provide, and merging it
                // would emit an empty definition shadowing the real one. That
                // is not hypothetical here — six stdlib modules (async, fs,
                // io, net, sync, time) have 152 impl methods between them and
                // every one is a native declaration.
                Item::ImplBlock(imp) => {
                    // report.txt P62. The HOST'S OWN DEFINITION WINS.
                    //
                    // A module is pulled in by REFERENCE, not only by `use` —
                    // "referencing a module is intent enough to expand it",
                    // and a bare method name counts. So a program that defines
                    // its own type with a method whose name a source module
                    // also implements drags that module in. That was harmless
                    // bloat until impl blocks started expanding (P61): now the
                    // module's `impl` arrives alongside the program's own and
                    // the analyser refuses the duplicate.
                    //
                    // `stdlib/tritfs_test.mt` is exactly that program — it
                    // inlines its own copy of TritFS deliberately and says so.
                    //
                    // Skipping only the COLLIDING block is the narrow repair.
                    // Suppressing the pull-in instead was tried and is worse:
                    // a program that defines its own `reverse` AND calls
                    // `s.reverse()` on a `str` then loses the str body, and
                    // fails at LINK rather than at check — trading visible
                    // bloat for a silent failure.
                    if host_impl_types.contains(&imp.ty) {
                        continue;
                    }
                    let mut imp = imp.clone();
                    imp.methods.retain(|m| m.body.is_some());
                    if imp.methods.is_empty() {
                        continue;
                    }
                    for m in &mut imp.methods {
                        if let Some(body) = &mut m.body {
                            ctx.rewrite_block(body);
                        }
                        for p in &mut m.params {
                            ctx.rewrite_type(&mut p.ty);
                        }
                        if let Some(rt) = &mut m.ret_ty {
                            ctx.rewrite_type(rt);
                        }
                    }
                    merged_items.push(Item::ImplBlock(imp));
                }
                Item::GlobalVar(g) if ctx.mutable_globals.contains(&g.name) => {
                    let mut g = g.clone();
                    g.name = format!("{}::{}", mod_name, g.name);
                    if let Some(init) = &mut g.val {
                        ctx.rewrite_expr(init);
                    }
                    merged_items.push(Item::GlobalVar(g));
                }
                // Inlined constants and use-decls produce no merged item.
                _ => {}
            }
        }

        for (name, init) in &ctx.const_inits {
            host_consts.insert(format!("{}::{}", mod_name, name), init.clone());
        }
        for ty in &type_names {
            host_types.insert(format!("{}::{}", mod_name, ty), ty.clone());
        }
    }

    // Rewrite the host program: inline qualified constants and un-qualify
    // module type references.
    let host_ctx = Rewrite {
        prefix: String::new(),
        fn_names: HashSet::new(),
        type_names: HashSet::new(),
        const_inits: host_consts,
        mutable_globals: HashSet::new(),
        observed: Default::default(),
    };
    let mut items = merged_items;
    for item in &program.items {
        let mut item = item.clone();
        match &mut item {
            Item::FnDef(f) => {
                if let Some(body) = &mut f.body {
                    host_ctx.rewrite_block(body);
                }
                for p in &mut f.params {
                    rewrite_host_type(&mut p.ty, &host_types);
                }
                if let Some(rt) = &mut f.ret_ty {
                    rewrite_host_type(rt, &host_types);
                }
            }
            Item::ImplBlock(imp) => {
                for m in &mut imp.methods {
                    if let Some(body) = &mut m.body {
                        host_ctx.rewrite_block(body);
                    }
                    for p in &mut m.params {
                        rewrite_host_type(&mut p.ty, &host_types);
                    }
                    if let Some(rt) = &mut m.ret_ty {
                        rewrite_host_type(rt, &host_types);
                    }
                }
            }
            Item::TraitDef(t) => {
                for m in &mut t.methods {
                    if let Some(body) = &mut m.body {
                        host_ctx.rewrite_block(body);
                    }
                }
            }
            Item::GlobalVar(g) => {
                rewrite_host_type(&mut g.ty, &host_types);
                if let Some(init) = &mut g.val {
                    host_ctx.rewrite_expr(init);
                }
            }
            _ => {}
        }
        // Host let-annotations and casts inside bodies are handled by
        // rewrite_block via rewrite_type below (the host context carries the
        // type map through host_types applied separately). Types inside
        // bodies still need the qualified->bare mapping:
        if let Item::FnDef(f) = &mut item {
            if let Some(body) = &mut f.body {
                rewrite_types_in_block(body, &host_types);
            }
        }
        if let Item::ImplBlock(imp) = &mut item {
            for m in &mut imp.methods {
                if let Some(body) = &mut m.body {
                    rewrite_types_in_block(body, &host_types);
                }
            }
        }
        items.push(item);
    }

    Ok(Some(Program { items }))
}

// ---------------------------------------------------------------------------
// Rewrite context for one module (or, with empty prefix, the host program)
// ---------------------------------------------------------------------------

/// Source-module prefixes the program refers to by qualified name, e.g. a call
/// to `ternary::trit_add` yields `"ternary"`.
///
/// Runs the ordinary rewrite traversal over a throwaway clone purely to collect
/// `Rewrite::observed`. Sharing the traversal is the point: a hand-written
/// second visitor would silently stop seeing new `Expr` variants.
fn module_refs(program: &Program) -> (HashSet<String>, HashSet<String>) {
    let probe = Rewrite {
        prefix: String::new(),
        fn_names: HashSet::new(),
        type_names: HashSet::new(),
        const_inits: HashMap::new(),
        mutable_globals: HashSet::new(),
        observed: Default::default(),
    };
    let mut copy = program.clone();
    for item in &mut copy.items {
        match item {
            Item::FnDef(f) => {
                if let Some(body) = &mut f.body {
                    probe.rewrite_block(body);
                }
            }
            Item::GlobalVar(g) => {
                if let Some(init) = &mut g.val {
                    probe.rewrite_expr(init);
                }
            }
            _ => {}
        }
    }
    let observed = probe.observed.into_inner();
    let prefixes = observed
        .iter()
        .filter_map(|n| n.split_once("::").map(|(head, _)| head.to_string()))
        .collect();
    let methods = observed
        .iter()
        .filter(|n| n.starts_with('.'))
        .cloned()
        .collect();
    (prefixes, methods)
}

/// Names of functions a module source actually implements, as opposed to the
/// `// native` signatures it merely declares. A body-less declaration is
/// provided by the backends and must not trigger expansion.
fn bodied_fn_names(src: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    for line in src.lines() {
        let line = line.trim_start();
        if let Some(rest) = line.strip_prefix("fn ") {
            if line.ends_with('{') {
                if let Some(name) = rest.split(['(', '<', ' ']).next() {
                    out.insert(name.to_string());
                }
            }
        }
    }
    out
}

struct Rewrite {
    prefix: String,
    /// Every `Expr::Ident` name the traversal has seen.
    ///
    /// Recorded by `rewrite_expr` itself, rather than by a parallel visitor,
    /// so detection and rewriting can never drift apart as the AST grows new
    /// expression forms. `module_refs` runs the traversal purely to collect
    /// this; `expand`'s real passes ignore it.
    observed: std::cell::RefCell<HashSet<String>>,
    /// Module function names — call callees are renamed to `prefix::name`.
    fn_names: HashSet<String>,
    /// Module struct/enum names — `prefix::T` type refs become bare `T`.
    type_names: HashSet<String>,
    /// Constants to inline: identifier -> initializer expression.
    const_inits: HashMap<String, Expr>,
    /// Module globals kept as real globals: identifier renamed to
    /// `prefix::identifier` (reads and assignment targets alike).
    mutable_globals: HashSet<String>,
}

impl Rewrite {
    fn rewrite_block(&self, block: &mut Block) {
        for stmt in &mut block.stmts {
            self.rewrite_stmt(stmt);
        }
    }

    fn rewrite_stmt(&self, stmt: &mut Stmt) {
        match stmt {
            Stmt::Let(ls) => {
                if let Some(ty) = &mut ls.ty {
                    self.rewrite_type(ty);
                }
                if let Some(init) = &mut ls.init {
                    self.rewrite_expr(init);
                }
            }
            Stmt::Assign(a) => {
                self.rewrite_expr(&mut a.target);
                self.rewrite_expr(&mut a.value);
            }
            Stmt::Expr(e) => self.rewrite_expr(e),
            Stmt::Return(Some(e), _) => self.rewrite_expr(e),
            Stmt::Return(None, _) | Stmt::Break(_) | Stmt::Continue(_) => {}
            Stmt::LocalStructDef(_) => {}
        }
    }

    fn rewrite_expr(&self, expr: &mut Expr) {
        match expr {
            // §11.4: `yield` names nothing, so nothing in it can be rewritten.
            Expr::Yield(_) => {}
            Expr::Ident(name, span) => {
                self.observed.borrow_mut().insert(name.clone());
                if let Some(init) = self.const_inits.get(name.as_str()) {
                    let mut inlined = init.clone();
                    reassign_spans(&mut inlined, *span);
                    *expr = inlined;
                } else if self.mutable_globals.contains(name.as_str()) {
                    *name = format!("{}::{}", self.prefix, name);
                }
            }
            Expr::Call(callee, args, _) => {
                // Rename direct calls to module-local functions.
                if let Expr::Ident(name, _) = callee.as_mut() {
                    self.observed.borrow_mut().insert(name.clone());
                    if self.fn_names.contains(name.as_str()) {
                        *name = format!("{}::{}", self.prefix, name);
                    } else {
                        self.rewrite_expr(callee);
                    }
                } else {
                    self.rewrite_expr(callee);
                }
                for a in args {
                    self.rewrite_expr(a);
                }
            }
            Expr::BinOp(l, _, r, _) => {
                self.rewrite_expr(l);
                self.rewrite_expr(r);
            }
            Expr::UnOp(_, e, _)
            | Expr::Await(e, _)
            | Expr::Return(e, _)
            | Expr::Question(e, _) => self.rewrite_expr(e),
            Expr::Cast(e, ty, _) => {
                self.rewrite_expr(e);
                self.rewrite_type(ty);
            }
            Expr::MethodCall(recv, name, args, _) => {
                // Recorded for module_refs: `s.reverse()` never produces an
                // Ident named "str::reverse", so without this a program that
                // only uses method syntax would not pull the module in.
                self.observed.borrow_mut().insert(format!(".{}", name));
                self.rewrite_expr(recv);
                for a in args {
                    self.rewrite_expr(a);
                }
            }
            Expr::Index(base, idx, _) => {
                self.rewrite_expr(base);
                self.rewrite_expr(idx);
            }
            Expr::Field(base, _, _) => self.rewrite_expr(base),
            Expr::Block(b) => self.rewrite_block(b),
            Expr::If(i) => {
                self.rewrite_expr(&mut i.cond);
                self.rewrite_block(&mut i.then_block);
                for (c, b) in &mut i.elif_branches {
                    self.rewrite_expr(c);
                    self.rewrite_block(b);
                }
                if let Some(e) = &mut i.else_block {
                    self.rewrite_block(e);
                }
            }
            Expr::Tif(t) => {
                self.rewrite_expr(&mut t.cond);
                self.rewrite_block(&mut t.pos_block);
                self.rewrite_block(&mut t.zero_block);
                self.rewrite_block(&mut t.neg_block);
            }
            Expr::Tresult(t) => {
                self.rewrite_expr(&mut t.expr);
                self.rewrite_block(&mut t.ok_block);
                self.rewrite_block(&mut t.unknown_block);
                self.rewrite_block(&mut t.err_block);
            }
            Expr::Match(m) => {
                self.rewrite_expr(&mut m.scrutinee);
                for arm in &mut m.arms {
                    if let Some(g) = &mut arm.guard {
                        self.rewrite_expr(g);
                    }
                    self.rewrite_expr(&mut arm.body);
                }
            }
            Expr::For(f) => {
                self.rewrite_expr(&mut f.iter);
                self.rewrite_block(&mut f.body);
            }
            Expr::While(w) => {
                self.rewrite_expr(&mut w.cond);
                self.rewrite_block(&mut w.body);
            }
            Expr::Loop(b, _) | Expr::Spawn(b, _) => self.rewrite_block(b),
            Expr::Array(elems, _) | Expr::Tuple(elems, _) => {
                for e in elems {
                    self.rewrite_expr(e);
                }
            }
            Expr::StructLit(name, fields, _) => {
                if let Some(bare) = name.strip_prefix(&format!("{}::", self.prefix)) {
                    if self.type_names.contains(bare) {
                        *name = bare.to_string();
                    }
                }
                for (_, e) in fields {
                    self.rewrite_expr(e);
                }
            }
            Expr::Range(lo, hi, _, _) => {
                self.rewrite_expr(lo);
                self.rewrite_expr(hi);
            }
            Expr::Lambda(params, ret, body, _) => {
                for (_, ty) in params {
                    self.rewrite_type(ty);
                }
                if let Some(rt) = ret {
                    self.rewrite_type(rt);
                }
                self.rewrite_expr(body);
            }
            Expr::Lit(_, _) | Expr::Break(_) | Expr::Continue(_) => {}
        }
    }

    fn rewrite_type(&self, ty: &mut Type) {
        match ty {
            Type::Named(name, _) => {
                if let Some(bare) = name
                    .strip_prefix(&format!("{}::", self.prefix))
                    .filter(|b| self.type_names.contains(*b))
                {
                    *name = bare.to_string();
                }
            }
            Type::Path(parts, span) => {
                if parts.len() == 2
                    && parts[0] == self.prefix
                    && self.type_names.contains(&parts[1])
                {
                    *ty = Type::Named(parts[1].clone(), *span);
                }
            }
            Type::Ref(inner, _, _) | Type::Ptr(inner, _, _) => self.rewrite_type(inner),
            Type::Array(inner, _, _) => self.rewrite_type(inner),
            Type::Tuple(tys, _) => {
                for t in tys {
                    self.rewrite_type(t);
                }
            }
            Type::Fn(params, ret, _) => {
                for t in params {
                    self.rewrite_type(t);
                }
                self.rewrite_type(ret);
            }
            Type::Generic(_, args, _) => {
                for t in args {
                    self.rewrite_type(t);
                }
            }
            Type::Infer(_) => {}
        }
    }
}

/// Find module-level names that function bodies assign to (making them
/// mutable state rather than inlinable constants).
fn collect_assigned_globals(
    block: &Block,
    candidates: &HashMap<String, Expr>,
    out: &mut HashSet<String>,
) {
    struct Walker<'a> {
        candidates: &'a HashMap<String, Expr>,
        out: &'a mut HashSet<String>,
    }
    impl Walker<'_> {
        fn block(&mut self, b: &Block) {
            for s in &b.stmts {
                self.stmt(s);
            }
        }
        fn stmt(&mut self, s: &Stmt) {
            match s {
                Stmt::Assign(a) => {
                    if let Expr::Ident(name, _) = &a.target {
                        if self.candidates.contains_key(name) {
                            self.out.insert(name.clone());
                        }
                    }
                    self.expr(&a.target);
                    self.expr(&a.value);
                }
                Stmt::Let(ls) => {
                    if let Some(e) = &ls.init {
                        self.expr(e);
                    }
                }
                Stmt::Expr(e) | Stmt::Return(Some(e), _) => self.expr(e),
                _ => {}
            }
        }
        fn expr(&mut self, e: &Expr) {
            match e {
                Expr::Block(b) => self.block(b),
                Expr::Loop(b, _) | Expr::Spawn(b, _) => self.block(b),
                Expr::If(i) => {
                    self.expr(&i.cond);
                    self.block(&i.then_block);
                    for (c, b) in &i.elif_branches {
                        self.expr(c);
                        self.block(b);
                    }
                    if let Some(b) = &i.else_block {
                        self.block(b);
                    }
                }
                Expr::Tif(t) => {
                    self.expr(&t.cond);
                    self.block(&t.pos_block);
                    self.block(&t.zero_block);
                    self.block(&t.neg_block);
                }
                Expr::Tresult(t) => {
                    self.expr(&t.expr);
                    self.block(&t.ok_block);
                    self.block(&t.unknown_block);
                    self.block(&t.err_block);
                }
                Expr::Match(m) => {
                    self.expr(&m.scrutinee);
                    for arm in &m.arms {
                        if let Some(g) = &arm.guard {
                            self.expr(g);
                        }
                        self.expr(&arm.body);
                    }
                }
                Expr::For(f) => {
                    self.expr(&f.iter);
                    self.block(&f.body);
                }
                Expr::While(w) => {
                    self.expr(&w.cond);
                    self.block(&w.body);
                }
                Expr::BinOp(l, _, r, _) => {
                    self.expr(l);
                    self.expr(r);
                }
                Expr::UnOp(_, x, _)
                | Expr::Await(x, _)
                | Expr::Return(x, _)
                | Expr::Question(x, _)
                | Expr::Cast(x, _, _)
                | Expr::Field(x, _, _)
                | Expr::Lambda(_, _, x, _) => self.expr(x),
                Expr::Call(c, args, _) => {
                    self.expr(c);
                    for a in args {
                        self.expr(a);
                    }
                }
                Expr::MethodCall(r, _, args, _) => {
                    self.expr(r);
                    for a in args {
                        self.expr(a);
                    }
                }
                Expr::Index(b, i, _) => {
                    self.expr(b);
                    self.expr(i);
                }
                Expr::Array(es, _) | Expr::Tuple(es, _) => {
                    for x in es {
                        self.expr(x);
                    }
                }
                Expr::StructLit(_, fs, _) => {
                    for (_, x) in fs {
                        self.expr(x);
                    }
                }
                Expr::Range(l, h, _, _) => {
                    self.expr(l);
                    self.expr(h);
                }
                _ => {}
            }
        }
    }
    let mut w = Walker { candidates, out };
    w.block(block);
}

/// Give every span in an inlined constant expression the span of the use
/// site, so diagnostics point at the program being compiled rather than at
/// stdlib source the user cannot see.
fn reassign_spans(expr: &mut Expr, span: Span) {
    match expr {
        Expr::Lit(_, s) | Expr::Ident(_, s) => *s = span,
        Expr::BinOp(l, _, r, s) => {
            *s = span;
            reassign_spans(l, span);
            reassign_spans(r, span);
        }
        Expr::UnOp(_, e, s) | Expr::Cast(e, _, s) => {
            *s = span;
            reassign_spans(e, span);
        }
        Expr::StructLit(_, fields, s) => {
            *s = span;
            for (_, e) in fields {
                reassign_spans(e, span);
            }
        }
        Expr::Array(es, s) | Expr::Tuple(es, s) => {
            *s = span;
            for e in es {
                reassign_spans(e, span);
            }
        }
        Expr::Call(c, args, s) => {
            *s = span;
            reassign_spans(c, span);
            for a in args {
                reassign_spans(a, span);
            }
        }
        // Constants are simple value expressions; anything more exotic keeps
        // its stdlib spans (still compiles, only diagnostics point away).
        _ => {}
    }
}

/// Host-program type rewriting: `m::T` -> `T` for merged module types.
fn rewrite_host_type(ty: &mut Type, map: &HashMap<String, String>) {
    match ty {
        Type::Named(name, _) => {
            if let Some(bare) = map.get(name.as_str()) {
                *name = bare.clone();
            }
        }
        Type::Path(parts, span) => {
            let joined = parts.join("::");
            if let Some(bare) = map.get(&joined) {
                *ty = Type::Named(bare.clone(), *span);
            }
        }
        Type::Ref(inner, _, _) | Type::Ptr(inner, _, _) => rewrite_host_type(inner, map),
        Type::Array(inner, _, _) => rewrite_host_type(inner, map),
        Type::Tuple(tys, _) => {
            for t in tys {
                rewrite_host_type(t, map);
            }
        }
        Type::Fn(params, ret, _) => {
            for t in params {
                rewrite_host_type(t, map);
            }
            rewrite_host_type(ret, map);
        }
        Type::Generic(_, args, _) => {
            for t in args {
                rewrite_host_type(t, map);
            }
        }
        Type::Infer(_) => {}
    }
}

/// Walk a block rewriting the types that appear inside statements and
/// expressions (let annotations, casts, lambda signatures).
fn rewrite_types_in_block(block: &mut Block, map: &HashMap<String, String>) {
    for stmt in &mut block.stmts {
        rewrite_types_in_stmt(stmt, map);
    }
}

fn rewrite_types_in_stmt(stmt: &mut Stmt, map: &HashMap<String, String>) {
    match stmt {
        Stmt::Let(ls) => {
            if let Some(ty) = &mut ls.ty {
                rewrite_host_type(ty, map);
            }
            if let Some(e) = &mut ls.init {
                rewrite_types_in_expr(e, map);
            }
        }
        Stmt::Assign(a) => {
            rewrite_types_in_expr(&mut a.target, map);
            rewrite_types_in_expr(&mut a.value, map);
        }
        Stmt::Expr(e) | Stmt::Return(Some(e), _) => rewrite_types_in_expr(e, map),
        _ => {}
    }
}

fn rewrite_types_in_expr(expr: &mut Expr, map: &HashMap<String, String>) {
    match expr {
        // §11.4: `yield` mentions no type.
        Expr::Yield(_) => {}
        Expr::Cast(e, ty, _) => {
            rewrite_types_in_expr(e, map);
            rewrite_host_type(ty, map);
        }
        Expr::Lambda(params, ret, body, _) => {
            for (_, ty) in params {
                rewrite_host_type(ty, map);
            }
            if let Some(rt) = ret {
                rewrite_host_type(rt, map);
            }
            rewrite_types_in_expr(body, map);
        }
        Expr::Block(b) => rewrite_types_in_block(b, map),
        Expr::Loop(b, _) | Expr::Spawn(b, _) => rewrite_types_in_block(b, map),
        Expr::If(i) => {
            rewrite_types_in_expr(&mut i.cond, map);
            rewrite_types_in_block(&mut i.then_block, map);
            for (c, b) in &mut i.elif_branches {
                rewrite_types_in_expr(c, map);
                rewrite_types_in_block(b, map);
            }
            if let Some(b) = &mut i.else_block {
                rewrite_types_in_block(b, map);
            }
        }
        Expr::Tif(t) => {
            rewrite_types_in_expr(&mut t.cond, map);
            rewrite_types_in_block(&mut t.pos_block, map);
            rewrite_types_in_block(&mut t.zero_block, map);
            rewrite_types_in_block(&mut t.neg_block, map);
        }
        Expr::Tresult(t) => {
            rewrite_types_in_expr(&mut t.expr, map);
            rewrite_types_in_block(&mut t.ok_block, map);
            rewrite_types_in_block(&mut t.unknown_block, map);
            rewrite_types_in_block(&mut t.err_block, map);
        }
        Expr::Match(m) => {
            rewrite_types_in_expr(&mut m.scrutinee, map);
            for arm in &mut m.arms {
                if let Some(g) = &mut arm.guard {
                    rewrite_types_in_expr(g, map);
                }
                rewrite_types_in_expr(&mut arm.body, map);
            }
        }
        Expr::For(f) => {
            rewrite_types_in_expr(&mut f.iter, map);
            rewrite_types_in_block(&mut f.body, map);
        }
        Expr::While(w) => {
            rewrite_types_in_expr(&mut w.cond, map);
            rewrite_types_in_block(&mut w.body, map);
        }
        Expr::BinOp(l, _, r, _) => {
            rewrite_types_in_expr(l, map);
            rewrite_types_in_expr(r, map);
        }
        Expr::UnOp(_, e, _)
        | Expr::Await(e, _)
        | Expr::Return(e, _)
        | Expr::Question(e, _)
        | Expr::Field(e, _, _) => rewrite_types_in_expr(e, map),
        Expr::Call(c, args, _) => {
            rewrite_types_in_expr(c, map);
            for a in args {
                rewrite_types_in_expr(a, map);
            }
        }
        Expr::MethodCall(r, _, args, _) => {
            rewrite_types_in_expr(r, map);
            for a in args {
                rewrite_types_in_expr(a, map);
            }
        }
        Expr::Index(b, i, _) => {
            rewrite_types_in_expr(b, map);
            rewrite_types_in_expr(i, map);
        }
        Expr::Array(es, _) | Expr::Tuple(es, _) => {
            for e in es {
                rewrite_types_in_expr(e, map);
            }
        }
        Expr::StructLit(name, fs, _) => {
            if let Some(bare) = map.get(name.as_str()) {
                *name = bare.clone();
            }
            for (_, e) in fs {
                rewrite_types_in_expr(e, map);
            }
        }
        Expr::Range(l, h, _, _) => {
            rewrite_types_in_expr(l, map);
            rewrite_types_in_expr(h, map);
        }
        Expr::Lit(_, _) | Expr::Ident(_, _) | Expr::Break(_) | Expr::Continue(_) => {}
    }
}

#[cfg(test)]
mod registry_tests {
    use super::{expand, SOURCE_MODULES};
    use crate::semantic::analyzer::SemanticAnalyzer;

    /// Every registered stdlib module whose `.mt` defines a function BODY must
    /// also be in `SOURCE_MODULES` (report.txt P60).
    ///
    /// There are two registries. `SemanticAnalyzer::STDLIB_MODULES` is
    /// authoritative for ACCEPTING `use std::X` and the calls that follow;
    /// `SOURCE_MODULES` here is authoritative for EMITTING the bodies. A module
    /// in the first and absent from the second type-checks and then fails to
    /// link — `Undefined label` on T3, `use of undefined value` on LLVM — from
    /// a program `manitc check` exits 0 on.
    ///
    /// **That is N1, and this test exists because N1 recurred.** N1 is why this
    /// pass was written; `tritfs` was a fourth module of exactly the kind it
    /// was written for and never reached the list. The hazard was documented
    /// in prose directly above `STDLIB_MODULES` — "Miss that last one and the
    /// module resolves, type-checks, and then fails at link" — and the prose
    /// did not stop it happening. A registry that must agree with another
    /// registry should be checked, not described.
    ///
    /// The discriminator is syntactic and exact: a native declaration ends in
    /// a semicolon (`fn println(s: str) ;  // native`) and a source-implemented
    /// one has a brace body. Mixed modules like `ternary` and `str` have both,
    /// and belong in `SOURCE_MODULES` on the strength of the braces.
    #[test]
    fn every_source_implemented_module_is_expanded() {
        // (name, file text) for each registered module that ships a .mt.
        let sources: &[(&str, &str)] = &[
            ("io", include_str!("../../stdlib/io.mt")),
            ("math", include_str!("../../stdlib/math.mt")),
            ("ternary", include_str!("../../stdlib/ternary.mt")),
            ("collections", include_str!("../../stdlib/collections.mt")),
            ("fmt", include_str!("../../stdlib/fmt.mt")),
            ("str", include_str!("../../stdlib/str.mt")),
            ("sync", include_str!("../../stdlib/sync.mt")),
            ("async", include_str!("../../stdlib/async.mt")),
            ("env", include_str!("../../stdlib/env.mt")),
            ("time", include_str!("../../stdlib/time.mt")),
            ("fs", include_str!("../../stdlib/fs.mt")),
            ("net", include_str!("../../stdlib/net.mt")),
            ("t27f", include_str!("../../stdlib/t27f.mt")),
            ("crypto", include_str!("../../stdlib/crypto.mt")),
            ("bridge", include_str!("../../stdlib/bridge.mt")),
            ("tritfs", include_str!("../../stdlib/tritfs.mt")),
            ("test", include_str!("../../stdlib/test.mt")),
            ("trit", include_str!("../../stdlib/trit.mt")),
        ];

        // Every module the analyser accepts must appear above, or this test
        // silently stops covering it — the failure mode it exists to prevent.
        for m in SemanticAnalyzer::STDLIB_MODULES {
            assert!(
                sources.iter().any(|(n, _)| n == m),
                "`{}` is in STDLIB_MODULES but this test has no source for it; \
                 add it here or the registry check stops covering it",
                m
            );
        }

        let has_body = |text: &str| {
            text.lines().any(|l| {
                let t = l.trim_start();
                (t.starts_with("fn ") || t.starts_with("pub fn "))
                    && t.contains('(')
                    && t.trim_end().ends_with('{')
                    || ((t.starts_with("fn ") || t.starts_with("pub fn "))
                        && t.contains(") {")
                        || (t.starts_with("fn ") || t.starts_with("pub fn "))
                            && t.contains("-> ")
                            && t.contains('{'))
            })
        };

        for (name, text) in sources {
            let expanded = SOURCE_MODULES.iter().any(|(n, _)| n == name);
            if has_body(text) {
                assert!(
                    expanded,
                    "stdlib/{}.mt defines function bodies and `{}` is a \
                     registered module, but it is NOT in SOURCE_MODULES. Every \
                     call to it will type-check and then fail to link. Add it \
                     to SOURCE_MODULES in this file.",
                    name, name
                );
            } else {
                assert!(
                    !expanded,
                    "stdlib/{}.mt is declarations only — every `fn` ends in a \
                     semicolon and the backends implement them — so expanding \
                     it compiles nothing and only costs time. Remove `{}` from \
                     SOURCE_MODULES.",
                    name, name
                );
            }
        }
    }

    /// **Every source-implemented impl METHOD survives expansion**
    /// (report.txt P61).
    ///
    /// The test above checks that a module is expanded. It cannot see whether
    /// everything IN the module is, and for a while nothing was: `ImplBlock`
    /// fell into the catch-all of the merge loop, so a module's free functions
    /// and its structs expanded while its methods vanished. `TritFS::new()`
    /// then type-checked — the struct is present, so the analyser resolves the
    /// method — and failed to link on both backends.
    ///
    /// **The defect had a population of one.** `tritfs` is the only module in
    /// the standard library with source-implemented impl methods; the other six
    /// with impl blocks (async, fs, io, net, sync, time) have 152 methods
    /// between them and every one is a native declaration. **A defect with a
    /// population of one is not rare, it is untested** — nothing else in the
    /// stdlib could ever have exercised this path.
    ///
    /// So this asserts the BEHAVIOUR rather than the table: expand a program
    /// that imports each source module, and count the brace-bodied impl
    /// methods that come out against the number that went in.
    #[test]
    fn every_source_implemented_impl_method_survives_expansion() {
        use crate::ast::{Item, Program, UseDecl};

        for (name, text) in SOURCE_MODULES {
            // Brace-bodied impl methods in the module source, counted by
            // parsing rather than by regex.
            let mut lexer = crate::lexer::Lexer::with_file(text, *name);
            let tokens = lexer.tokenize().expect("stdlib module must lex");
            let mut parser = crate::parser::Parser::with_file(tokens, *name);
            let module = parser.parse().expect("stdlib module must parse");
            let want: usize = module
                .items
                .iter()
                .filter_map(|i| match i {
                    Item::ImplBlock(b) => Some(b),
                    _ => None,
                })
                .map(|b| b.methods.iter().filter(|m| m.body.is_some()).count())
                .sum();

            // A one-line program that imports the module, expanded.
            let prog = Program {
                items: vec![Item::UseDecl(UseDecl {
                    path: vec!["std".to_string(), (*name).to_string()],
                    span: Default::default(),
                })],
            };
            let expanded = expand(&prog)
                .expect("expansion must succeed")
                .unwrap_or_else(|| panic!("`{}` is in SOURCE_MODULES but expand() declined it", name));
            let got: usize = expanded
                .items
                .iter()
                .filter_map(|i| match i {
                    Item::ImplBlock(b) => Some(b),
                    _ => None,
                })
                .map(|b| b.methods.iter().filter(|m| m.body.is_some()).count())
                .sum();

            assert_eq!(
                got, want,
                "stdlib/{}.mt defines {} impl method(s) with a body and \
                 expansion emitted {}. Every call to a dropped method \
                 type-checks and then fails to link — `Undefined label` on T3, \
                 `use of undefined value` on LLVM.",
                name, want, got
            );
        }
    }
}
