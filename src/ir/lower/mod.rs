// ir/lower/mod.rs — IR lowering from TypedAST to IRModule.
// Struct definition, utility methods, and public entry point live here.
// Statement, expression, control-flow, and loop lowering are in sub-modules.

pub(crate) mod helpers;
mod lower_ctrl;
mod lower_expr;
mod lower_loop;
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
            ret_ty: IRType::Struct(struct_name),
        });
        self.emit_struct_copy(IRValue::Temp(call_dst), sret.clone(), n);
        IRValue::Temp(sret)
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
        }
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
        }

        for g in &typed_program.globals {
            let ty = IRType::from_mani(&g.ty);
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
        let struct_sizes = lowerer.structs.iter()
            .map(|(name, fields)| (name.clone(), fields.len()))
            .collect();
        IRModule {
            name: "main".to_string(),
            functions,
            globals,
            string_literals,
            float_literals: Vec::new(),
            struct_sizes,
        }
    }

    fn lower_expr_to_const(&mut self, expr: &TypedExpr) -> IRValue {
        match expr.kind.clone() {
            TypedExprKind::Lit(lit) => self.lower_lit(&lit),
            _ => IRValue::Const(IRConst::Null),
        }
    }

    // ---------------------------------------------------------------------------
    // Function lowering
    // ---------------------------------------------------------------------------

    fn lower_fn(&mut self, f: &TypedFnDef) -> IRFunction {
        let ret_ty = IRType::from_mani(&f.ret_ty);
        let params: Vec<(String, IRType)> = f.params.iter()
            .map(|p| (p.name.clone(), IRType::from_mani(&p.ty)))
            .collect();

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
            let alloca_t = self.fresh_temp();
            self.emit(IRInstr::Alloca { dst: alloca_t.clone(), ty: pty.clone() });
            let param_val = IRValue::Temp(IRTemp::new(format!("param_{}", pname)));
            self.emit(IRInstr::Store {
                ptr: IRValue::Temp(alloca_t.clone()),
                val: param_val,
                ty: pty.clone(),
            });
            self.locals.insert(pname.clone(), (alloca_t, pty.clone()));
        }

        let body = f.body.as_ref().unwrap();
        let block_val = self.lower_block(body);

        if matches!(self.blocks[self.current_block].term, IRTerminator::Unreachable) {
            if ret_ty == IRType::Void {
                self.set_term(IRTerminator::Return(None));
            } else {
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
