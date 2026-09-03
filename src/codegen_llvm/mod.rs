// codegen_llvm/mod.rs — LLVM IR text backend for the maniT compiler.
// Emits valid LLVM IR (.ll file content) from an IRModule (crate::ir).
//
// Usage:
//   let ir_text = emit_llvm_ir(&module);
//
// The output can be fed directly to `clang` or `llc`.

use crate::ir::*;
use std::collections::HashMap;

pub(crate) mod helpers;
use helpers::*;

#[cfg(test)]
mod tests;

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Emit LLVM IR for `module`, using the given target triple.
/// Pass `None` to use the default (`x86_64-pc-linux-gnu`).
pub fn emit_llvm_ir(module: &IRModule, target_triple: Option<&str>) -> String {
    let mut out = String::new();
    let mut ctx = LLVMEmitter::new();
    ctx.target_triple = target_triple
        .unwrap_or("x86_64-pc-linux-gnu")
        .to_string();
    ctx.emit_module(module, &mut out);
    out
}

// ---------------------------------------------------------------------------
// Emitter state
// ---------------------------------------------------------------------------

struct LLVMEmitter {
    /// Tracks simple Assign substitutions: dst_name → src_operand_string.
    /// When a later instruction references the dst, we substitute the src
    /// directly so we avoid emitting a redundant copy.
    assigns: HashMap<String, String>,
    /// Counter for anonymous labels generated inline (e.g. TritBranch
    /// intermediate check block).
    anon_counter: usize,
    /// Return type of the current function being emitted.
    current_ret_ty: IRType,
    /// Counter for DWARF debug metadata nodes (!N).
    #[allow(dead_code)]
    dbg_counter: usize,
    /// Metadata node ID for each function's DISubprogram: function name → !N.
    #[allow(dead_code)]
    dbg_subprograms: Vec<(String, usize)>,
    /// Target triple string (e.g. "x86_64-pc-linux-gnu").
    target_triple: String,
    /// Known function signatures: mangled_name → (param_types, return_type).
    /// Built from STDLIB_DECLARES and user-defined functions so that Call
    /// instructions emit correct LLVM types instead of guessing.
    fn_sigs: HashMap<String, (Vec<String>, String)>,
    /// Tracks the actual LLVM type string for each temp (%tN).
    /// Populated when emitting Load, Call, Alloca, BinOp, etc.
    /// Used to resolve type mismatches (e.g. i8 value in i64 comparison).
    temp_types: HashMap<String, String>,
    /// Tracks the actual stored type for each alloca pointer.
    /// When a Store writes a value of type T to ptr %p, we record %p → T.
    /// When a Load reads from %p, we use this type instead of the IR's declared type.
    alloca_stored_types: HashMap<String, String>,
    /// Maps block label → set of predecessor block labels.
    /// Built once per function before emission.
    block_predecessors: HashMap<String, Vec<String>>,
    /// The label of the block currently being emitted.
    current_block_label: Option<String>,
    /// Struct name → number of fields. Copied from IRModule at emission start.
    struct_sizes: HashMap<String, usize>,
}

impl LLVMEmitter {
    fn new() -> Self {
        LLVMEmitter {
            assigns: HashMap::new(),
            anon_counter: 0,
            current_ret_ty: IRType::Void,
            // Reserve !0 for DICompileUnit, !1 for DIFile
            dbg_counter: 2,
            dbg_subprograms: Vec::new(),
            target_triple: "x86_64-pc-linux-gnu".to_string(),
            fn_sigs: HashMap::new(),
            temp_types: HashMap::new(),
            alloca_stored_types: HashMap::new(),
            block_predecessors: HashMap::new(),
            current_block_label: None,
            struct_sizes: HashMap::new(),
        }
    }

    #[allow(dead_code)]
    fn next_dbg_id(&mut self) -> usize {
        let id = self.dbg_counter;
        self.dbg_counter += 1;
        id
    }

    fn fresh_anon(&mut self, prefix: &str) -> String {
        let n = self.anon_counter;
        self.anon_counter += 1;
        format!("{}{}", prefix, n)
    }

    // -----------------------------------------------------------------------
    // Module-level emission
    // -----------------------------------------------------------------------

    fn emit_module(&mut self, module: &IRModule, out: &mut String) {
        // Copy struct sizes so we can allocate correct sizes for struct values.
        self.struct_sizes = module.struct_sizes.clone();

        // ---- Preamble -------------------------------------------------------
        out.push_str("; maniT compiler output - LLVM IR\n");
        out.push_str("; Target: x86-64 (via LLVM)\n");
        out.push_str(
            "target datalayout = \
             \"e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-f80:128-n8:16:32:64-S128\"\n",
        );
        out.push_str(&format!("target triple = \"{}\"\n", self.target_triple));
        out.push('\n');

        // ---- Built-in format string globals ---------------------------------
        out.push_str(
            "@fmt_int     = private unnamed_addr constant [4 x i8] c\"%ld\\00\", align 1\n",
        );
        out.push_str(
            "@fmt_float   = private unnamed_addr constant [3 x i8] c\"%f\\00\", align 1\n",
        );
        out.push_str(
            "@fmt_newline = private unnamed_addr constant [2 x i8] c\"\\0A\\00\", align 1\n",
        );
        out.push('\n');

        // ---- User string literals -------------------------------------------
        for (label, content) in &module.string_literals {
            let escaped = llvm_escape_string(content);
            let n = content.len() + 1; // +1 for null terminator
            // Strip a leading '@' that the IR lowerer may have included.
            let gname = label.trim_start_matches('@');
            out.push_str(&format!(
                "@{} = private unnamed_addr constant [{} x i8] c\"{}\\00\", align 1\n",
                gname, n, escaped
            ));
        }
        if !module.string_literals.is_empty() {
            out.push('\n');
        }

        // ---- Static struct payloads -------------------------------------------
        //
        // The storage a struct-valued global points at. Emitted as a literal
        // aggregate of the field SLOT types rather than as `%struct.Name`,
        // because that is what the field access actually walks: GetPtr into
        // an aggregate strides by `i64` (see emit_instr.rs), so the payload
        // must be a flat run of 8-byte slots and not a named struct type
        // whose layout LLVM is free to reason about separately.
        // `llvm_abi_type`, not `llvm_type`: a field whose slot type is a
        // struct holds a POINTER to that struct, so its spelling here is
        // `ptr`. Spelled `%struct.Inner` it is an aggregate by value, and
        // LLVM rejects the initialiser outright — "global variable reference
        // must have pointer type".
        for s in &module.static_structs {
            let tys: Vec<String> = s.fields.iter().map(|(t, _)| llvm_abi_type(t)).collect();
            let vals: Vec<String> = s.fields.iter()
                .map(|(t, v)| format!("{} {}", llvm_abi_type(t), irvalue_to_operand(v, t)))
                .collect();
            out.push_str(&format!(
                "@{} = private global {{ {} }} {{ {} }}  ; {}\n",
                mangle_func_name(&s.label),
                tys.join(", "),
                vals.join(", "),
                s.struct_name,
            ));
        }
        if !module.static_structs.is_empty() {
            out.push('\n');
        }

        // ---- Global variables -----------------------------------------------
        for g in &module.globals {
            self.emit_global(g, out);
        }
        if !module.globals.is_empty() {
            out.push('\n');
        }

        // ---- Struct type declarations ----------------------------------------
        // Sort by name for deterministic output.
        let mut struct_entries: Vec<(&String, &usize)> = module.struct_sizes.iter().collect();
        struct_entries.sort_by_key(|(name, _)| name.as_str());
        for (name, &n_fields) in &struct_entries {
            let fields: Vec<&str> = std::iter::repeat("i64").take(n_fields).collect();
            out.push_str(&format!(
                "%struct.{} = type {{ {} }}\n",
                name,
                fields.join(", ")
            ));
        }
        if !struct_entries.is_empty() {
            out.push('\n');
        }

        // ---- External libc declarations -------------------------------------
        out.push_str("declare i32 @printf(ptr, ...)\n");
        out.push_str("declare i32 @putchar(i32)\n");
        out.push_str("declare ptr @malloc(i64)\n");
        out.push_str("declare ptr @manit_alloc(i64)\n");
        out.push_str("declare void @free(ptr)\n");
        out.push_str("declare i32 @strcmp(ptr, ptr)\n");
        out.push_str("declare i32 @usleep(i32)\n");
        // Saturating float→integer (P23). Raw `fptosi` is undefined behaviour
        // out of range and gives `i64::MIN` for NaN and both infinities on
        // x86; these have Rust's semantics, which is what T3 already does.
        out.push_str("declare i64 @llvm.fptosi.sat.i64.f64(double)\n");
        out.push_str("declare i32 @llvm.fptosi.sat.i32.f64(double)\n");
        out.push_str("declare i16 @llvm.fptosi.sat.i16.f64(double)\n");
        out.push_str("declare i8 @llvm.fptosi.sat.i8.f64(double)\n");
        out.push('\n');

        // ---- maniT stdlib declarations -------------------------------------
        //
        // Skip any declare the module also DEFINES. A stdlib function that
        // moves from the C runtime into ManiT source keeps its mangled name —
        // `fmt::show_bool3` is still `@fmt_show_bool3` — so emitting the
        // declare alongside the new define is `invalid redefinition` and the IR
        // will not assemble.
        //
        // Filtering here rather than deleting lines from STDLIB_DECLARES is
        // deliberate. The C implementations stay in the runtime and stay
        // declared for any program that does not pull in the ManiT body, and
        // the next function to be reimplemented in ManiT needs no edit here at
        // all. Two of these moved on 20 August 2026 (show_trit, show_bool3) and
        // two more followed for the reason in stdlib/fmt.mt's header
        // (align_left, align_right).
        let defined: std::collections::HashSet<String> =
            module.functions.iter().map(|f| mangle_func_name(&f.name)).collect();
        for line in STDLIB_DECLARES.lines() {
            let shadowed = line
                .trim()
                .strip_prefix("declare ")
                .and_then(|rest| rest.find('@').map(|p| &rest[p + 1..]))
                .and_then(|after| after.find('(').map(|p| &after[..p]))
                .is_some_and(|name| defined.contains(name));
            if !shadowed {
                out.push_str(line);
                out.push('\n');
            }
        }
        out.push('\n');

        // ---- Build function signature map -----------------------------------
        // Parse STDLIB_DECLARES (and libc declares) to learn param/return types.
        self.fn_sigs = parse_declare_sigs(STDLIB_DECLARES);
        // Also parse the libc declarations we emitted above.
        let libc_decls = "declare i32 @printf(ptr, ...)\n\
                          declare i32 @putchar(i32)\n\
                          declare ptr @malloc(i64)\n\
                          declare ptr @manit_alloc(i64)\n\
                          declare void @free(ptr)\n\
                          declare i32 @strcmp(ptr, ptr)\n";
        for (k, v) in parse_declare_sigs(libc_decls) {
            self.fn_sigs.insert(k, v);
        }
        // Signatures of the internal helper definitions (emitted below as
        // `define internal` — they must NOT also appear as declares).
        for (k, v) in parse_declare_sigs(INTERNAL_HELPER_SIGS) {
            self.fn_sigs.insert(k, v);
        }
        // Add user-defined and extern function signatures from the module.
        for f in &module.functions {
            let mangled = mangle_func_name(&f.name);
            let params: Vec<String> = f.params.iter().map(|(_, ty)| {
                llvm_abi_type(ty)
            }).collect();
            // Aggregate returns (structs and arrays) are pointers in our ABI.
            let ret = llvm_abi_type(&f.ret_ty);
            self.fn_sigs.insert(mangled, (params, ret));
        }

        // ---- Internal helper functions --------------------------------------
        self.emit_helper_print_trit(out);
        out.push('\n');
        self.emit_helper_print_bool3(out);
        out.push('\n');
        // Ternary/array/IO helpers the C runtime does not export
        // (semantics mirror the T3 emulator syscalls — see helpers.rs).
        out.push_str(INTERNAL_RUNTIME_HELPERS);
        out.push('\n');

        // ---- User-defined / external functions ------------------------------
        for f in &module.functions {
            self.assigns.clear();
            self.temp_types.clear();
            self.alloca_stored_types.clear();
            self.emit_function(f, out);
            out.push('\n');
        }

        // ---- C-ABI main wrapper ---------------------------------------------
        // The user's main was emitted as @__manit_main (see mangle_func_name);
        // emit the i32-returning @main the C runtime start code expects, so
        // the process exit status is main's return value (or 0 for void).
        if let Some(user_main) = module.functions.iter().find(|f| f.name == "main") {
            self.emit_main_wrapper(user_main, out);
            out.push('\n');
        }

        // ---- DWARF debug metadata -------------------------------------------
        // Disabled: function-level !dbg without per-instruction !dbg locations
        // causes clang to reject the IR. Re-enable once span tracking is added
        // to IR instructions.
        // self.emit_debug_metadata(&module.name, out);
    }

    // -----------------------------------------------------------------------
    // Global variable
    // -----------------------------------------------------------------------

    fn emit_global(&self, g: &IRGlobal, out: &mut String) {
        let ty_str = llvm_type(&g.ty);
        let init_str = match &g.init {
            Some(v) => irvalue_to_operand(v, &g.ty),
            None => llvm_zero_init(&g.ty),
        };
        out.push_str(&format!(
            "@{} = global {} {}\n",
            mangle_func_name(&g.name),
            ty_str,
            init_str
        ));
    }

    // -----------------------------------------------------------------------
    // C-ABI main wrapper
    // -----------------------------------------------------------------------

    fn emit_main_wrapper(&self, user_main: &IRFunction, out: &mut String) {
        // Zero/default arguments for any parameters main declares (unused in
        // practice, but the call must match the emitted signature).
        let args: Vec<String> = user_main
            .params
            .iter()
            .map(|(_, ty)| {
                let ty_s = llvm_abi_type(ty);
                match ty_s.as_str() {
                    "ptr" => "ptr null".to_string(),
                    "double" => "double 0.0".to_string(),
                    other => format!("{} 0", other),
                }
            })
            .collect();
        let ret_s = llvm_abi_type(&user_main.ret_ty);

        out.push_str("define i32 @main() {\nentry:\n");
        match ret_s.as_str() {
            "void" => {
                out.push_str(&format!(
                    "  call void @__manit_main({})\n  ret i32 0\n",
                    args.join(", ")
                ));
            }
            "i32" => {
                out.push_str(&format!(
                    "  %r = call i32 @__manit_main({})\n  ret i32 %r\n",
                    args.join(", ")
                ));
            }
            "i64" | "i16" | "i8" => {
                let op = if ret_s == "i64" { "trunc" } else { "sext" };
                out.push_str(&format!(
                    "  %r = call {} @__manit_main({})\n  %s = {} {} %r to i32\n  ret i32 %s\n",
                    ret_s,
                    args.join(", "),
                    op,
                    ret_s
                ));
            }
            "i1" => {
                out.push_str(&format!(
                    "  %r = call i1 @__manit_main({})\n  %s = zext i1 %r to i32\n  ret i32 %s\n",
                    args.join(", ")
                ));
            }
            _ => {
                // double / ptr returning main: run it, exit 0.
                out.push_str(&format!(
                    "  %r = call {} @__manit_main({})\n  ret i32 0\n",
                    ret_s,
                    args.join(", ")
                ));
            }
        }
        out.push_str("}\n");
    }

    // -----------------------------------------------------------------------
    // DWARF debug metadata (basic line-tables-only)
    // -----------------------------------------------------------------------

    #[allow(dead_code)]
    fn emit_debug_metadata(&self, module_name: &str, out: &mut String) {
        // Extract filename from module name or use a default
        let filename = if module_name.is_empty() { "input.mt" } else { module_name };

        out.push_str("\n; Debug metadata\n");

        // Named metadata anchors
        let _sp_ids: Vec<String> = self.dbg_subprograms.iter()
            .map(|(_, id)| format!("!{}", id))
            .collect();
        out.push_str(&format!("!llvm.dbg.cu = !{{!0}}\n"));
        out.push_str(&format!("!llvm.module.flags = !{{!{}}}\n", self.dbg_counter));

        // !0 = DICompileUnit
        out.push_str(&format!(
            "!0 = distinct !DICompileUnit(language: DW_LANG_C, file: !1, producer: \"manitc 0.1.0\", isOptimized: false, runtimeVersion: 0, emissionKind: LineTablesOnly)\n"
        ));
        // !1 = DIFile
        out.push_str(&format!(
            "!1 = !DIFile(filename: \"{}\", directory: \".\")\n",
            filename.replace('\\', "\\\\").replace('"', "\\\"")
        ));

        // DISubprogram for each function
        for (i, (func_name, sp_id)) in self.dbg_subprograms.iter().enumerate() {
            let line = i + 1; // Approximate line number (1-based, one per function)
            out.push_str(&format!(
                "!{} = distinct !DISubprogram(name: \"{}\", scope: !1, file: !1, line: {}, type: !{}, scopeLine: {}, spFlags: DISPFlagDefinition, unit: !0)\n",
                sp_id,
                func_name.replace('"', "\\\""),
                line,
                sp_id + 1, // subroutine type node
                line
            ));
            // Emit a minimal DISubroutineType for each function
            out.push_str(&format!(
                "!{} = !DISubroutineType(types: !{{}})\n",
                sp_id + 1
            ));
        }

        // Module flags for debug info version
        out.push_str(&format!(
            "!{} = !{{i32 2, !\"Debug Info Version\", i32 3}}\n",
            self.dbg_counter
        ));
    }

    // -----------------------------------------------------------------------
    // Function
    // -----------------------------------------------------------------------

    fn emit_function(&mut self, f: &IRFunction, out: &mut String) {
        // Struct return types are always returned as pointers in our ABI.
        let ret_ty_str = llvm_abi_type(&f.ret_ty);

        if f.is_extern {
            // Emit only a `declare` — no body.
            let params_str: Vec<String> =
                f.params.iter().map(|(_, ty)| {
                    llvm_abi_type(ty)
                }).collect();
            out.push_str(&format!(
                "declare {} @{}({})\n",
                ret_ty_str,
                mangle_func_name(&f.name),
                params_str.join(", ")
            ));
            return;
        }

        // The IR lowerer stores each parameter under the name "param_<name>"
        // (it allocas the storage and stores the incoming SSA value into it).
        // Struct-typed parameters are always passed as pointers.
        let params_str: Vec<String> = f
            .params
            .iter()
            .map(|(name, ty)| {
                format!("{} %param_{}", llvm_abi_type(ty), name)
            })
            .collect();

        out.push_str(&format!(
            "define {} @{}({}) {{\n",
            ret_ty_str,
            mangle_func_name(&f.name),
            params_str.join(", ")
        ));

        self.current_ret_ty = f.ret_ty.clone();

        // Build predecessor map for PHI node fixups. The map must reflect
        // the CFG as EMITTED, not the IR: a TritBranch lowers to two
        // two-way branches, and its zero/neg targets are really reached
        // from the synthesized `<label>__tneg_check` block (B20). The
        // check-block name matches what emit_terminator generates.
        self.block_predecessors.clear();
        for block in &f.blocks {
            // (successor, emitted predecessor label)
            let edges: Vec<(String, String)> = match &block.term {
                IRTerminator::Jump(lbl) => vec![(lbl.clone(), block.label.clone())],
                IRTerminator::BinBranch { true_label, false_label, .. } => vec![
                    (true_label.clone(), block.label.clone()),
                    (false_label.clone(), block.label.clone()),
                ],
                IRTerminator::TritBranch { pos_label, zero_label, neg_label, .. } => {
                    let check_lbl = format!("{}__tneg_check", block.label);
                    vec![
                        (pos_label.clone(), block.label.clone()),
                        (zero_label.clone(), check_lbl.clone()),
                        (neg_label.clone(), check_lbl),
                    ]
                }
                _ => vec![],
            };
            for (succ, pred) in edges {
                self.block_predecessors
                    .entry(succ)
                    .or_insert_with(Vec::new)
                    .push(pred);
            }
        }

        // Record parameter types so downstream instructions can look them up.
        for (name, ty) in &f.params {
            self.record_temp_type(&format!("param_{}", name), &llvm_abi_type(ty));
        }

        for block in &f.blocks {
            self.emit_block(block, out);
        }

        out.push_str("}\n");
    }

    // -----------------------------------------------------------------------
    // Basic block
    // -----------------------------------------------------------------------

    fn emit_block(&mut self, block: &IRBlock, out: &mut String) {
        self.current_block_label = Some(block.label.clone());
        out.push_str(&format!("{}:\n", block.label));

        for instr in &block.instrs {
            let line = self.emit_instr(instr);
            if !line.is_empty() {
                // Multi-line instructions (e.g. TritMin/Max which emit an
                // icmp + select) are pre-formatted with embedded "\n  " so
                // we just push them as-is with the standard indent prefix.
                out.push_str("  ");
                out.push_str(&line);
                out.push('\n');
            }
        }

        // Terminators may produce multiple lines (TritBranch).
        for line in self.emit_terminator(&block.term) {
            if line.starts_with('\n') {
                // A bare label line (TritBranch intermediate block).
                out.push_str(&line);
                out.push('\n');
            } else {
                out.push_str("  ");
                out.push_str(&line);
                out.push('\n');
            }
        }
    }

    // -----------------------------------------------------------------------
    // Helper: emit an integer comparison with automatic type coercion.
    // If one operand is i8 (trit/bool3) and the comparison type is i64,
    // emit a sext instruction to widen the i8 to i64.
    // Returns a multi-line string if coercion is needed.
    // -----------------------------------------------------------------------

    fn emit_icmp_coerced(
        &mut self,
        dst_name: &str,
        cmp_kind: &str,
        lhs: &IRValue,
        rhs: &IRValue,
        target_ty: &str,
    ) -> String {
        let mut prefix = String::new();
        let l_actual = self.actual_type_of(lhs);
        let r_actual = self.actual_type_of(rhs);

        let l_str = self.resolve_val(lhs, &IRType::I64);
        let r_str = self.resolve_val(rhs, &IRType::I64);

        // Determine the comparison type.
        let cmp_ty = if l_actual == "ptr" || r_actual == "ptr" {
            "ptr".to_string()
        } else if target_ty == "ptr" {
            // If target says ptr but neither operand is actually ptr,
            // fall back to the widest integer type.
            let types = [l_actual.as_str(), r_actual.as_str()];
            let widest = types.iter().max_by_key(|t| int_width(t)).unwrap();
            widest.to_string()
        } else {
            // Use the widest integer type among target, l_actual, r_actual
            let types = [target_ty, l_actual.as_str(), r_actual.as_str()];
            let widest = types.iter().max_by_key(|t| int_width(t)).unwrap();
            widest.to_string()
        };

        // When comparing pointers, integer constants must be "null" not "0".
        let l_final = if cmp_ty == "ptr" && l_actual != "ptr" {
            // This is an integer constant being compared with a ptr.
            // If it's a literal "0", use "null" instead.
            if l_str == "0" { "null".to_string() }
            else {
                // inttoptr conversion
                let ext_name = format!("{}__lptr", dst_name);
                prefix.push_str(&format!("{} = inttoptr {} {} to ptr\n  ", ext_name, l_actual, l_str));
                ext_name
            }
        } else if l_actual != cmp_ty && cmp_ty != "ptr" && l_actual != "ptr" {
            let aw = int_width(&l_actual);
            let tw = int_width(&cmp_ty);
            if aw != tw {
                let ext_name = format!("{}__lext", dst_name);
                let op = if aw < tw { widen_op(&l_actual) } else { "trunc" };
                prefix.push_str(&format!("{} = {} {} {} to {}\n  ", ext_name, op, l_actual, l_str, cmp_ty));
                ext_name
            } else {
                l_str
            }
        } else {
            l_str
        };

        let r_final = if cmp_ty == "ptr" && r_actual != "ptr" {
            if r_str == "0" { "null".to_string() }
            else {
                let ext_name = format!("{}__rptr", dst_name);
                prefix.push_str(&format!("{} = inttoptr {} {} to ptr\n  ", ext_name, r_actual, r_str));
                ext_name
            }
        } else if r_actual != cmp_ty && cmp_ty != "ptr" && r_actual != "ptr" {
            let aw = int_width(&r_actual);
            let tw = int_width(&cmp_ty);
            if aw != tw {
                let ext_name = format!("{}__rext", dst_name);
                let op = if aw < tw { widen_op(&r_actual) } else { "trunc" };
                prefix.push_str(&format!("{} = {} {} {} to {}\n  ", ext_name, op, r_actual, r_str, cmp_ty));
                ext_name
            } else {
                r_str
            }
        } else {
            r_str
        };

        format!("{}{} = icmp {} {} {}, {}", prefix, dst_name, cmp_kind, cmp_ty, l_final, r_final)
    }

    /// The float sibling of `emit_icmp_coerced` (P92).
    ///
    /// An `fcmp` needs both operands typed `double`, and one of them may be a
    /// TYPE-ERASED machine word: the uniform one-word payload slot of an enum
    /// or a `Result` is loaded as `i64`, because that is what the slot is. So
    /// `match r { Ok(2.5) => .. }` emitted
    /// `fcmp oeq double %t7, 0x4004...` with `%t7` an `i64`, and clang refused
    /// the module with "defined with type 'i64' but expected 'double'".
    ///
    /// A `bitcast` and not a conversion: the word HOLDS the f64 bit pattern,
    /// so `sitofp` would turn 1.5 into 4.6e18. The call-argument path has
    /// carried exactly this bitcast since 20 August for exactly this reason —
    /// the comparison boundary is the same situation and never got it, which
    /// is this repository's own rule about fixing the reported site only.
    ///
    /// The integer comparisons have had `emit_icmp_coerced` throughout; the
    /// six float ones went through a bare `resolve_pair`. Routing all six here
    /// means a payload word compares correctly whichever predicate is used,
    /// rather than only for the one a test happened to exercise.
    /// Resolve a float operand pair, bitcasting either side that is actually a
    /// TYPE-ERASED machine word. Returns `(prefix, lhs, rhs)`.
    ///
    /// The uniform one-word payload slot of an enum or a `Result` is loaded as
    /// `i64`, because that is what the slot is — so a float that came out of
    /// one arrives at an `fadd` or an `fcmp` still typed `i64`, and clang
    /// refuses the module: "defined with type 'i64' but expected 'double'".
    ///
    /// A `bitcast` and NOT a conversion: the word HOLDS the f64 bit pattern,
    /// so `sitofp` would turn 1.5 into 4.6e18. The call-argument path has
    /// carried exactly this bitcast since 20 August for exactly this reason
    /// (see `emit_instr.rs`); the comparison and arithmetic boundaries are the
    /// same situation and never got it. P92.
    pub(super) fn resolve_float_pair(
        &mut self,
        lhs: &IRValue,
        rhs: &IRValue,
        tag: &str,
    ) -> (String, String, String) {
        let mut prefix = String::new();
        let (l_str, r_str) = self.resolve_pair(lhs, rhs, &IRType::F64);
        let l_actual = self.actual_type_of(lhs);
        let r_actual = self.actual_type_of(rhs);

        let cast = |actual: &str, val: &str, side: &str, prefix: &mut String| -> String {
            if actual == "i64" {
                let name = format!("%{}__{}fb", tag, side);
                prefix.push_str(&format!("{} = bitcast i64 {} to double\n  ", name, val));
                name
            } else {
                val.to_string()
            }
        };
        let l_final = cast(&l_actual, &l_str, "l", &mut prefix);
        let r_final = cast(&r_actual, &r_str, "r", &mut prefix);
        (prefix, l_final, r_final)
    }

    /// The float sibling of `emit_icmp_coerced` (P92).
    ///
    /// The integer comparisons have had `emit_icmp_coerced` throughout; the
    /// six float ones went through a bare `resolve_pair`. Routing all six here
    /// means a payload word compares correctly whichever predicate is used,
    /// rather than only for the one a test happened to exercise.
    fn emit_fcmp_coerced(
        &mut self,
        dst_name: &str,
        pred: &str,
        lhs: &IRValue,
        rhs: &IRValue,
    ) -> String {
        let tag = dst_name.trim_start_matches('%').to_string();
        let (prefix, l, r) = self.resolve_float_pair(lhs, rhs, &tag);
        format!("{}{} = fcmp {} double {}, {}", prefix, dst_name, pred, l, r)
    }

    // -----------------------------------------------------------------------
    // Instruction → single String (may contain embedded "\n  " for multi-line)
    // -----------------------------------------------------------------------

}

mod emit_instr;
