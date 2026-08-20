// ir/lower/mod.rs — IR lowering from TypedAST to IRModule.
// Struct definition, utility methods, and public entry point live here.
// Statement, expression, control-flow, and loop lowering are in sub-modules.

pub(crate) mod helpers;
mod lower_ctrl;
mod lower_expr;
mod lower_loop;
pub mod lower_result;
mod lower_stmt;

use helpers::*;

use super::types::*;
use crate::semantic::{ManiType, TypedBlock, TypedExpr, TypedExprKind, TypedFnDef, TypedProgram};

// ---------------------------------------------------------------------------
// IR Lowerer
// ---------------------------------------------------------------------------

pub struct IRLowerer {
    temp_counter: usize,
    label_counter: usize,
    string_literals: Vec<(String, String)>,
    // current function's block list
    blocks: Vec<IRBlock>,
    current_block: usize,
    // local variable → alloca temp
    locals: std::collections::HashMap<String, (IRTemp, IRType)>,
    // struct name → ordered list of (field_name, field_type)
    structs: std::collections::HashMap<String, Vec<(String, IRType)>>,
    // enum name → ordered variant names (variant index = integer tag)
    enum_variants: std::collections::HashMap<String, Vec<String>>,
    // function name → positions of `[T]` (unsized array) parameters.
    // Unsized arrays are bare pointers with no runtime length, so every
    // such parameter gets a hidden trailing `__len_<name>` i64 parameter;
    // call sites push the argument's statically-known length for it.
    fn_unsized_array_params: std::collections::HashMap<String, Vec<usize>>,
    // module-level global variables: name → IR type. A read of one of
    // these must go through a Load (IRValue::Global is the address).
    global_vars: std::collections::HashMap<String, IRType>,
    // function name → parameter ManiTypes (for bool→bool3 argument coercion)
    fn_param_manitys: std::collections::HashMap<String, Vec<ManiType>>,
    // struct name → field ManiTypes in declaration order
    struct_field_manitys: std::collections::HashMap<String, Vec<ManiType>>,
    // declared return ManiType of the function currently being lowered
    current_fn_ret: ManiType,
}

impl IRLowerer {
    /// Returns true if `name` refers to a real user-defined struct (not an enum).
    fn is_real_struct(&self, name: &str) -> bool {
        self.structs.contains_key(name)
    }

    /// Returns the number of fields of a struct, or 0 for enums/unknown.
    fn struct_nfields(&self, name: &str) -> usize {
        self.structs.get(name).map_or(0, |f| f.len())
    }

    /// Emit instructions to copy all fields from `src_ptr` into `dst_alloca`.
    fn emit_struct_copy(&mut self, src_ptr: IRValue, dst_alloca: IRTemp, n_fields: usize) {
        for i in 0..n_fields {
            let idx = IRValue::Const(IRConst::Int(i as i64));
            let src_f = self.fresh_temp();
            self.emit(IRInstr::GetPtr {
                dst: src_f.clone(), ptr: src_ptr.clone(), idx: idx.clone(), ty: IRType::I64,
            });
            let fval = self.fresh_temp();
            self.emit(IRInstr::Load { dst: fval.clone(), ptr: IRValue::Temp(src_f), ty: IRType::I64 });
            let dst_f = self.fresh_temp();
            self.emit(IRInstr::GetPtr {
                dst: dst_f.clone(), ptr: IRValue::Temp(dst_alloca.clone()), idx, ty: IRType::I64,
            });
            self.emit(IRInstr::Store {
                ptr: IRValue::Temp(dst_f), val: IRValue::Temp(fval), ty: IRType::I64,
            });
        }
    }

    /// If `func_name` was lowered with hidden `__len_` parameters for its
    /// `[T]` (unsized array) params, push the matching length values after
    /// the regular arguments, in parameter order.
    fn append_unsized_len_args(
        &mut self,
        func_name: &str,
        typed_args: &[&TypedExpr],
        arg_vals: &mut Vec<IRValue>,
    ) {
        let Some(positions) = self.fn_unsized_array_params.get(func_name).cloned() else {
            return;
        };
        for pos in positions {
            let len_val = match typed_args.get(pos) {
                Some(arg) => self.unsized_array_len(arg),
                None => IRValue::Const(IRConst::Int(0)),
            };
            arg_vals.push(len_val);
        }
    }

    /// Best-effort length of an array argument: the statically-known size,
    /// the caller's own hidden length when forwarding an unsized param, or
    /// the sized IR type recorded for a let-bound local. Falls back to 0
    /// (an empty iteration) when no length is recoverable.
    fn unsized_array_len(&mut self, arg: &TypedExpr) -> IRValue {
        match &arg.ty {
            ManiType::Array(_, Some(n)) => IRValue::Const(IRConst::Int(*n as i64)),
            ManiType::Array(_, None) => {
                if let TypedExprKind::Ident(name) = &arg.kind {
                    if let Some((len_alloca, _)) =
                        self.locals.get(&Self::unsized_len_key(name)).cloned()
                    {
                        let t = self.fresh_temp();
                        self.emit(IRInstr::Load {
                            dst: t.clone(),
                            ptr: IRValue::Temp(len_alloca),
                            ty: IRType::I64,
                        });
                        return IRValue::Temp(t);
                    }
                    if let Some((_, IRType::Array(_, n))) = self.locals.get(name) {
                        return IRValue::Const(IRConst::Int(*n as i64));
                    }
                }
                IRValue::Const(IRConst::Int(0))
            }
            _ => IRValue::Const(IRConst::Int(0)),
        }
    }

    /// Lower a struct-returning call: allocate sret, issue call, copy fields, return sret.
    fn lower_struct_call(
        &mut self,
        func_name: String,
        arg_vals: Vec<IRValue>,
        struct_name: String,
    ) -> IRValue {
        let n = self.struct_nfields(&struct_name);
        let sret = self.fresh_temp();
        self.emit(IRInstr::Alloca { dst: sret.clone(), ty: IRType::Struct(struct_name.clone()) });
        let call_dst = self.fresh_temp();
        self.emit(IRInstr::Call {
            dst: Some(call_dst.clone()),
            func: func_name,
            args: arg_vals,
            ret_ty: IRType::Struct(struct_name.clone()),
        });
        self.emit_struct_copy(IRValue::Temp(call_dst), sret.clone(), n);

        // Array-typed fields hold pointers into the callee's dead frame on
        // the T3 stack; the shallow slot copy above would leave the caller
        // aliasing memory the next call clobbers. Deep-copy those arrays
        // into caller-owned buffers and repoint the field slots.
        let field_manitys = self.struct_field_manitys.get(&struct_name).cloned();
        if let Some(fields) = field_manitys {
            for (i, fty) in fields.iter().enumerate() {
                let ManiType::Array(elem, Some(len)) = fty else {
                    continue;
                };
                let elem_ir = IRType::from_mani(elem);
                let arr_ir = IRType::Array(Box::new(elem_ir.clone()), *len);
                // Element-width access: aggregate elements are 8-byte
                // pointer slots, scalars use their own width.
                let access = helpers::array_value_ty(&elem_ir);
                // Load the (dangling) source array pointer from the slot.
                let slot_p = self.fresh_temp();
                self.emit(IRInstr::GetPtr {
                    dst: slot_p.clone(),
                    ptr: IRValue::Temp(sret.clone()),
                    idx: IRValue::Const(IRConst::Int(i as i64)),
                    ty: IRType::I64,
                });
                let src_arr = self.fresh_temp();
                self.emit(IRInstr::Load {
                    dst: src_arr.clone(),
                    ptr: IRValue::Temp(slot_p.clone()),
                    ty: IRType::I64,
                });
                let buf = self.fresh_temp();
                self.emit(IRInstr::Alloca { dst: buf.clone(), ty: arr_ir });
                for k in 0..*len {
                    let idx = IRValue::Const(IRConst::Int(k as i64));
                    let sp = self.fresh_temp();
                    self.emit(IRInstr::GetPtr {
                        dst: sp.clone(),
                        ptr: IRValue::Temp(src_arr.clone()),
                        idx: idx.clone(),
                        ty: access.clone(),
                    });
                    let v = self.fresh_temp();
                    self.emit(IRInstr::Load {
                        dst: v.clone(),
                        ptr: IRValue::Temp(sp),
                        ty: access.clone(),
                    });
                    let dp = self.fresh_temp();
                    self.emit(IRInstr::GetPtr {
                        dst: dp.clone(),
                        ptr: IRValue::Temp(buf.clone()),
                        idx,
                        ty: access.clone(),
                    });
                    self.emit(IRInstr::Store {
                        ptr: IRValue::Temp(dp),
                        val: IRValue::Temp(v),
                        ty: access.clone(),
                    });
                }
                self.emit(IRInstr::Store {
                    ptr: IRValue::Temp(slot_p),
                    val: IRValue::Temp(buf),
                    ty: IRType::I64,
                });
            }
        }
        IRValue::Temp(sret)
    }

    /// Array-returning calls copy the result into a caller-owned buffer:
    /// the callee's array lives in its (now dead) frame on the T3 stack, so
    /// the copy must happen before any further call reuses that memory.
    /// This also gives arrays the same value semantics as structs.
    fn lower_array_call(
        &mut self,
        func_name: String,
        arg_vals: Vec<IRValue>,
        arr_ty: IRType,
    ) -> IRValue {
        let IRType::Array(ref elem, n) = arr_ty else {
            unreachable!("lower_array_call requires an array type");
        };
        let elem_ty = (**elem).clone();
        let buf = self.fresh_temp();
        self.emit(IRInstr::Alloca { dst: buf.clone(), ty: arr_ty.clone() });
        let call_dst = self.fresh_temp();
        self.emit(IRInstr::Call {
            dst: Some(call_dst.clone()),
            func: func_name,
            args: arg_vals,
            ret_ty: arr_ty.clone(),
        });
        // Element-typed access: arrays index by their element width on both
        // backends (unlike the uniform 8-byte struct slots). Nested arrays
        // store their elements as pointers.
        let access = helpers::array_value_ty(&elem_ty);
        for i in 0..n {
            let idx = IRValue::Const(IRConst::Int(i as i64));
            let src_p = self.fresh_temp();
            self.emit(IRInstr::GetPtr {
                dst: src_p.clone(),
                ptr: IRValue::Temp(call_dst.clone()),
                idx: idx.clone(),
                ty: access.clone(),
            });
            let v = self.fresh_temp();
            self.emit(IRInstr::Load {
                dst: v.clone(),
                ptr: IRValue::Temp(src_p),
                ty: access.clone(),
            });
            let dst_p = self.fresh_temp();
            self.emit(IRInstr::GetPtr {
                dst: dst_p.clone(),
                ptr: IRValue::Temp(buf.clone()),
                idx,
                ty: access.clone(),
            });
            self.emit(IRInstr::Store {
                ptr: IRValue::Temp(dst_p),
                val: IRValue::Temp(v),
                ty: access.clone(),
            });
        }
        IRValue::Temp(buf)
    }
}

impl IRLowerer {
    pub fn new() -> Self {
        IRLowerer {
            temp_counter: 0,
            label_counter: 0,
            string_literals: Vec::new(),
            blocks: Vec::new(),
            current_block: 0,
            locals: std::collections::HashMap::new(),
            structs: std::collections::HashMap::new(),
            enum_variants: std::collections::HashMap::new(),
            fn_unsized_array_params: std::collections::HashMap::new(),
            global_vars: std::collections::HashMap::new(),
            fn_param_manitys: std::collections::HashMap::new(),
            struct_field_manitys: std::collections::HashMap::new(),
            current_fn_ret: ManiType::Void,
        }
    }

    /// Key under which the hidden length of an unsized-array local is
    /// registered in `locals`. The `#` prefix cannot appear in user
    /// identifiers, so this can never collide with a real variable.
    fn unsized_len_key(name: &str) -> String {
        format!("#len:{}", name)
    }

    fn fresh_temp(&mut self) -> IRTemp {
        let n = self.temp_counter;
        self.temp_counter += 1;
        IRTemp::new(format!("t{}", n))
    }

    fn fresh_label(&mut self, prefix: &str) -> String {
        let n = self.label_counter;
        self.label_counter += 1;
        format!("{}{}", prefix, n)
    }

    fn intern_string(&mut self, s: &str) -> String {
        for (label, content) in &self.string_literals {
            if content == s {
                return label.clone();
            }
        }
        let label = format!("@str{}", self.string_literals.len());
        self.string_literals.push((label.clone(), s.to_string()));
        label
    }

    fn lower_lit(&mut self, lit: &crate::ast::Lit) -> IRValue {
        self.lower_lit_typed(lit, None)
    }

    fn lower_lit_typed(
        &mut self,
        lit: &crate::ast::Lit,
        ty_hint: Option<&ManiType>,
    ) -> IRValue {
        match lit {
            crate::ast::Lit::Str(s) => {
                let label = self.intern_string(s);
                IRValue::Const(IRConst::Str(label))
            }
            crate::ast::Lit::Bool(b) if matches!(ty_hint, Some(ManiType::Bool3)) => {
                IRValue::Const(IRConst::Trit(if *b { 1 } else { -1 }))
            }
            other => lit_to_irvalue(other),
        }
    }

    /// Insert a representation conversion where a blessed implicit coercion
    /// changes the value encoding. Today that is exactly bool → bool3:
    /// a logical 0/1 must become -1/+1 (0 would read as `unknown`).
    pub(super) fn coerce_value(
        &mut self,
        val: IRValue,
        from: &ManiType,
        to: &ManiType,
    ) -> IRValue {
        if matches!(from, ManiType::Bool) && matches!(to, ManiType::Bool3) {
            // bool3 = 2*b - 1
            let doubled = self.fresh_temp();
            self.emit(IRInstr::BinOp {
                dst: doubled.clone(),
                op: IRBinOp::Mul,
                lhs: val,
                rhs: IRValue::Const(IRConst::Int(2)),
                ty: IRType::I8,
            });
            let dst = self.fresh_temp();
            self.emit(IRInstr::BinOp {
                dst: dst.clone(),
                op: IRBinOp::Sub,
                lhs: IRValue::Temp(doubled),
                rhs: IRValue::Const(IRConst::Int(1)),
                ty: IRType::I8,
            });
            return IRValue::Temp(dst);
        }
        val
    }

    fn emit(&mut self, instr: IRInstr) {
        self.blocks[self.current_block].instrs.push(instr);
    }

    fn set_term(&mut self, term: IRTerminator) {
        self.blocks[self.current_block].term = term;
    }

    fn new_block(&mut self, label: String) -> usize {
        let idx = self.blocks.len();
        self.blocks.push(IRBlock::new(label));
        idx
    }

    fn switch_to(&mut self, block_idx: usize) {
        self.current_block = block_idx;
    }

    // ---------------------------------------------------------------------------
    // Public entry point
    // ---------------------------------------------------------------------------

    pub fn lower(typed_program: &TypedProgram) -> IRModule {
        let mut lowerer = IRLowerer::new();
        let mut functions = Vec::new();
        let mut globals = Vec::new();

        for enum_def in &typed_program.enums {
            let variants: Vec<String> = enum_def.variants.iter()
                .map(|v| v.name.clone())
                .collect();
            lowerer.enum_variants.insert(enum_def.name.clone(), variants);
        }

        for (sname, sfields) in &typed_program.struct_fields {
            let fields: Vec<(String, IRType)> = sfields.iter()
                .map(|(name, mty)| (name.clone(), IRType::from_mani(mty)))
                .collect();
            lowerer.structs.insert(sname.clone(), fields);
            lowerer.struct_field_manitys.insert(
                sname.clone(),
                sfields.iter().map(|(_, mty)| mty.clone()).collect(),
            );
        }

        for f in &typed_program.functions {
            lowerer.fn_param_manitys.insert(
                f.name.clone(),
                f.params.iter().map(|p| p.ty.clone()).collect(),
            );
            if f.body.is_none() {
                continue;
            }
            let positions: Vec<usize> = f.params.iter().enumerate()
                .filter(|(_, p)| matches!(p.ty, ManiType::Array(_, None)))
                .map(|(i, _)| i)
                .collect();
            if !positions.is_empty() {
                lowerer.fn_unsized_array_params.insert(f.name.clone(), positions);
            }
        }

        for g in &typed_program.globals {
            let ty = IRType::from_mani(&g.ty);
            lowerer.global_vars.insert(g.name.clone(), ty.clone());
            let init = g.init.as_ref().map(|e| lowerer.lower_expr_to_const(e));
            globals.push(IRGlobal { name: g.name.clone(), ty, init });
        }

        for f in &typed_program.functions {
            lowerer.temp_counter = 0;
            lowerer.label_counter = 0;
            lowerer.blocks = Vec::new();
            lowerer.locals = std::collections::HashMap::new();
            let ir_fn = lowerer.lower_fn(f);
            functions.push(ir_fn);
        }

        let string_literals = lowerer.string_literals.clone();
        // Declared structs by field count, plus every enum at one slot.
        //
        // Enums lower to `IRType::Struct(name)` too (see IRType::from_mani) but
        // are not in `lowerer.structs`, so they used to reach the LLVM
        // backend's allocation-size lookup with no entry and be sized by its
        // silent `unwrap_or(1)` default. One slot is in fact right for a
        // tag-only enum — but it was right by accident, and the same default
        // silently under-allocated tuples (ORACLE_FINDINGS.md Section 10).
        // Recording enums explicitly lets that default become a hard error.
        let mut struct_sizes: std::collections::HashMap<String, usize> = lowerer.structs.iter()
            .map(|(name, fields)| (name.clone(), fields.len()))
            .collect();
        for name in lowerer.enum_variants.keys() {
            struct_sizes.entry(name.clone()).or_insert(1);
        }
        IRModule {
            name: "main".to_string(),
            functions,
            globals,
            string_literals,
            float_literals: Vec::new(),
            struct_sizes,
        }
    }

    /// Lower a module-level initialiser, which must be a compile-time constant.
    ///
    /// The semantic pass has already folded this same expression and refused
    /// the program if it would not fold (`analyzer/mod.rs`, S31), so reaching
    /// the `Null` below means the two disagree — a compiler bug, not a user
    /// error, and one that would otherwise reappear as a silent zero.
    ///
    /// This match used to have exactly one arm, `Lit`, and a wildcard that
    /// returned `Null`. `-42` is `UnOp(Neg, Lit(42))`, so it missed, and every
    /// negative module-level constant read as 0 on both backends.
    fn lower_expr_to_const(&mut self, expr: &TypedExpr) -> IRValue {
        use crate::semantic::const_fold::{fold, ConstValue};
        match fold(expr) {
            Ok(ConstValue::Int(n)) => IRValue::Const(IRConst::Int(n)),
            Ok(ConstValue::Float(f)) => IRValue::Const(IRConst::Float(f)),
            Ok(ConstValue::Bool(b)) => IRValue::Const(IRConst::Bool(b)),
            Ok(ConstValue::Trit(t)) => IRValue::Const(IRConst::Trit(t)),
            // Strings are interned to a label, exactly as `lower_lit_typed`
            // does — a global `str` holds the address of the .data entry.
            Ok(ConstValue::Str(s)) => IRValue::Const(IRConst::Str(self.intern_string(&s))),
            Ok(ConstValue::Null) => IRValue::Const(IRConst::Null),
            Err(e) => panic!(
                "maniT internal error: a global initialiser that the semantic pass accepted \
                 will not fold during lowering ({}). This is a compiler bug — the two folders \
                 must agree.",
                e.describe(),
            ),
        }
    }

    // ---------------------------------------------------------------------------
    // Function lowering
    // ---------------------------------------------------------------------------

    fn lower_fn(&mut self, f: &TypedFnDef) -> IRFunction {
        self.current_fn_ret = f.ret_ty.clone();
        let ret_ty = IRType::from_mani(&f.ret_ty);
        let mut params: Vec<(String, IRType)> = f.params.iter()
            .map(|p| (p.name.clone(), IRType::from_mani(&p.ty)))
            .collect();
        // Hidden trailing length parameters for `[T]` (unsized array) params:
        // unsized arrays are bare pointers with no runtime length, so callers
        // pass the statically-known length as an extra i64 argument.
        let unsized_params: Vec<String> = f.params.iter()
            .filter(|p| matches!(p.ty, ManiType::Array(_, None)))
            .map(|p| p.name.clone())
            .collect();
        if f.body.is_some() {
            for pname in &unsized_params {
                params.push((format!("__len_{}", pname), IRType::I64));
            }
        }

        if f.body.is_none() {
            return IRFunction {
                name: f.name.clone(),
                params,
                ret_ty,
                blocks: Vec::new(),
                is_extern: true,
            };
        }

        let entry_label = "entry".to_string();
        self.new_block(entry_label);
        self.current_block = 0;

        for (pname, pty) in &params {
            // Hidden `__len_<name>` params are registered under the reserved
            // "#len:<name>" key so user code can never reference them, and
            // for-loops over the matching array param can find the length.
            if let Some(orig) = pname.strip_prefix("__len_") {
                if unsized_params.iter().any(|p| p == orig) {
                    let alloca_t = self.fresh_temp();
                    self.emit(IRInstr::Alloca { dst: alloca_t.clone(), ty: IRType::I64 });
                    let param_val = IRValue::Temp(IRTemp::new(format!("param_{}", pname)));
                    self.emit(IRInstr::Store {
                        ptr: IRValue::Temp(alloca_t.clone()),
                        val: param_val,
                        ty: IRType::I64,
                    });
                    self.locals.insert(Self::unsized_len_key(orig), (alloca_t, IRType::I64));
                    continue;
                }
            }
            if let IRType::Struct(sname) = pty {
                if self.is_real_struct(sname) {
                    // Store the struct pointer in an alloca slot at function entry.
                    // This ensures the pointer survives loop iterations: instead of
                    // relying on a parameter register (which rescue moves can clobber
                    // across loop back-edges), we always load it from the stack.
                    // The local maps to an I64 alloca holding the struct pointer value.
                    let alloca_t = self.fresh_temp();
                    self.emit(IRInstr::Alloca { dst: alloca_t.clone(), ty: IRType::I64 });
                    let param_val = IRValue::Temp(IRTemp::new(format!("param_{}", pname)));
                    self.emit(IRInstr::Store {
                        ptr: IRValue::Temp(alloca_t.clone()),
                        val: param_val,
                        ty: IRType::I64,
                    });
                    // Register with I64 type (the pointer is an integer handle).
                    // lower_expr(Ident("self")) will then emit a Load to get the pointer,
                    // producing a stable value from the stack for every use — including
                    // inside loop bodies where register rescues would otherwise corrupt it.
                    self.locals.insert(pname.clone(), (alloca_t, IRType::I64));
                    continue;
                }
            }
            // Array parameters are pointer values: spill the pointer into a
            // one-word slot (not an array-sized alloca that would then be
            // loaded as array bytes). Ident loads already read the slot with
            // array_value_ty, so the local keeps its Array type.
            let slot_ty = helpers::array_value_ty(pty);
            let alloca_t = self.fresh_temp();
            self.emit(IRInstr::Alloca { dst: alloca_t.clone(), ty: slot_ty.clone() });
            let param_val = IRValue::Temp(IRTemp::new(format!("param_{}", pname)));
            self.emit(IRInstr::Store {
                ptr: IRValue::Temp(alloca_t.clone()),
                val: param_val,
                ty: slot_ty,
            });
            self.locals.insert(pname.clone(), (alloca_t, pty.clone()));
        }

        let body = f.body.as_ref().unwrap();
        let block_val = self.lower_block(body);

        if matches!(self.blocks[self.current_block].term, IRTerminator::Unreachable) {
            if ret_ty == IRType::Void {
                self.set_term(IRTerminator::Return(None));
            } else {
                let block_val =
                    self.coerce_value(block_val, &body.ty.clone(), &f.ret_ty.clone());
                self.set_term(IRTerminator::Return(Some(block_val)));
            }
        }

        IRFunction {
            name: f.name.clone(),
            params,
            ret_ty,
            blocks: self.blocks.clone(),
            is_extern: false,
        }
    }

    // ---------------------------------------------------------------------------
    // Block lowering
    // ---------------------------------------------------------------------------

    fn lower_block(&mut self, block: &TypedBlock) -> IRValue {
        let mut last = IRValue::Void;
        for stmt in &block.stmts {
            last = self.lower_stmt(stmt);
        }
        last
    }
}
