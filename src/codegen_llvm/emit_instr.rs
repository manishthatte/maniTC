// codegen_llvm/emit_instr.rs — LLVM IR instruction and terminator emission.
use super::*;

impl LLVMEmitter {
    pub(super) fn emit_instr(&mut self, instr: &IRInstr) -> String {
        match instr {
            // ---- BinOp ------------------------------------------------------
            IRInstr::BinOp { dst, op, lhs, rhs, ty } => {
                let dst_name = format!("%{}", dst.0);
                let is_float = matches!(ty, IRType::F64);

                // Record result type for this temp.
                let result_ty = match op {
                    IRBinOp::IEq | IRBinOp::INe | IRBinOp::ILt | IRBinOp::IGt
                    | IRBinOp::ILe | IRBinOp::IGe | IRBinOp::FEq | IRBinOp::FNe
                    | IRBinOp::FLt | IRBinOp::FGt | IRBinOp::FLe | IRBinOp::FGe
                    | IRBinOp::StrEq | IRBinOp::StrNe => "i1".to_string(),
                    _ => llvm_type(ty),
                };
                self.record_temp_type(&dst.0, &result_ty);

                match op {
                    // --- Arithmetic ---
                    IRBinOp::Add | IRBinOp::Sub | IRBinOp::Mul | IRBinOp::Div | IRBinOp::Rem => {
                        let ty_str = llvm_type(ty);
                        // String concatenation: str + str → call str_concat
                        if matches!(op, IRBinOp::Add) && ty_str == "ptr" {
                            let l = self.resolve_val(lhs, ty);
                            let r = self.resolve_val(rhs, ty);
                            self.record_temp_type(&dst.0, "ptr");
                            return format!("{} = call ptr @str_concat(ptr {}, ptr {})", dst_name, l, r);
                        }
                        if is_float {
                            let (l, r) = self.resolve_pair(lhs, rhs, ty);
                            let op_name = match op {
                                IRBinOp::Add => "fadd",
                                IRBinOp::Sub => "fsub",
                                IRBinOp::Mul => "fmul",
                                IRBinOp::Div => "fdiv",
                                IRBinOp::Rem => "frem",
                                _ => unreachable!(),
                            };
                            format!("{} = {} {} {}, {}", dst_name, op_name, ty_str, l, r)
                        } else {
                            let op_name = match op {
                                IRBinOp::Add => "add",
                                IRBinOp::Sub => "sub",
                                IRBinOp::Mul => "mul",
                                IRBinOp::Div => "sdiv",
                                IRBinOp::Rem => "srem",
                                _ => unreachable!(),
                            };
                            // Coerce operands to the target type if they differ.
                            let (lp, l) = self.resolve_with_coerce(lhs, &ty_str, &format!("{}_l", dst.0));
                            let (rp, r) = self.resolve_with_coerce(rhs, &ty_str, &format!("{}_r", dst.0));
                            // A7: guard integer division/remainder. Unguarded,
                            // a zero divisor raised SIGFPE — a hard crash with
                            // no message, losing all buffered output — while
                            // the T3 emulator reported a clean TRAP. The guard
                            // is a call rather than a branch so no extra basic
                            // blocks are needed here; it aborts via
                            // manit_fault with the same text and exit status.
                            let guard = if matches!(op, IRBinOp::Div | IRBinOp::Rem) {
                                let (wp, w) = self.widen_to_i64(&r, &ty_str, &format!("{}_dz", dst.0));
                                format!("{}  call void @manit_check_divisor(i64 {})\n", wp, w)
                            } else {
                                String::new()
                            };
                            format!("{}{}{}{} = {} {} {}, {}", lp, rp, guard, dst_name, op_name, ty_str, l, r)
                        }
                    }
                    // --- Integer comparisons (result is i1; operands keep their type) ---
                    // Use emit_icmp_coerced to handle i8/i64 mismatches.
                    IRBinOp::IEq => {
                        let op_ty = operand_type_for_cmp(lhs, rhs, ty);
                        self.record_temp_type(&dst.0, "i1");
                        self.emit_icmp_coerced(&dst_name, "eq", lhs, rhs, &llvm_type(&op_ty))
                    }
                    IRBinOp::INe => {
                        let op_ty = operand_type_for_cmp(lhs, rhs, ty);
                        self.record_temp_type(&dst.0, "i1");
                        self.emit_icmp_coerced(&dst_name, "ne", lhs, rhs, &llvm_type(&op_ty))
                    }
                    IRBinOp::ILt => {
                        let op_ty = operand_type_for_cmp(lhs, rhs, ty);
                        self.record_temp_type(&dst.0, "i1");
                        self.emit_icmp_coerced(&dst_name, "slt", lhs, rhs, &llvm_type(&op_ty))
                    }
                    IRBinOp::IGt => {
                        let op_ty = operand_type_for_cmp(lhs, rhs, ty);
                        self.record_temp_type(&dst.0, "i1");
                        self.emit_icmp_coerced(&dst_name, "sgt", lhs, rhs, &llvm_type(&op_ty))
                    }
                    IRBinOp::ILe => {
                        let op_ty = operand_type_for_cmp(lhs, rhs, ty);
                        self.record_temp_type(&dst.0, "i1");
                        self.emit_icmp_coerced(&dst_name, "sle", lhs, rhs, &llvm_type(&op_ty))
                    }
                    IRBinOp::IGe => {
                        let op_ty = operand_type_for_cmp(lhs, rhs, ty);
                        self.record_temp_type(&dst.0, "i1");
                        self.emit_icmp_coerced(&dst_name, "sge", lhs, rhs, &llvm_type(&op_ty))
                    }
                    // --- Float comparisons ---
                    IRBinOp::FEq => {
                        let (l, r) = self.resolve_pair(lhs, rhs, &IRType::F64);
                        format!("{} = fcmp oeq double {}, {}", dst_name, l, r)
                    }
                    IRBinOp::FNe => {
                        let (l, r) = self.resolve_pair(lhs, rhs, &IRType::F64);
                        format!("{} = fcmp one double {}, {}", dst_name, l, r)
                    }
                    IRBinOp::FLt => {
                        let (l, r) = self.resolve_pair(lhs, rhs, &IRType::F64);
                        format!("{} = fcmp olt double {}, {}", dst_name, l, r)
                    }
                    IRBinOp::FGt => {
                        let (l, r) = self.resolve_pair(lhs, rhs, &IRType::F64);
                        format!("{} = fcmp ogt double {}, {}", dst_name, l, r)
                    }
                    IRBinOp::FLe => {
                        let (l, r) = self.resolve_pair(lhs, rhs, &IRType::F64);
                        format!("{} = fcmp ole double {}, {}", dst_name, l, r)
                    }
                    IRBinOp::FGe => {
                        let (l, r) = self.resolve_pair(lhs, rhs, &IRType::F64);
                        format!("{} = fcmp oge double {}, {}", dst_name, l, r)
                    }
                    // --- Logical / bitwise ---
                    IRBinOp::And => {
                        let (l, r) = self.resolve_pair(lhs, rhs, ty);
                        format!("{} = and {} {}, {}", dst_name, llvm_type(ty), l, r)
                    }
                    IRBinOp::Or => {
                        let (l, r) = self.resolve_pair(lhs, rhs, ty);
                        format!("{} = or {} {}, {}", dst_name, llvm_type(ty), l, r)
                    }
                    IRBinOp::Xor => {
                        let (l, r) = self.resolve_pair(lhs, rhs, ty);
                        format!("{} = xor {} {}, {}", dst_name, llvm_type(ty), l, r)
                    }
                    IRBinOp::LShift => {
                        let (l, r) = self.resolve_pair(lhs, rhs, ty);
                        format!("{} = shl {} {}, {}", dst_name, llvm_type(ty), l, r)
                    }
                    IRBinOp::RShift => {
                        let (l, r) = self.resolve_pair(lhs, rhs, ty);
                        format!("{} = ashr {} {}, {}", dst_name, llvm_type(ty), l, r)
                    }
                    IRBinOp::StrEq => {
                        // String equality: call strcmp and check result == 0
                        let mut prefix = String::new();
                        let l_s = self.resolve_val(lhs, ty);
                        let r_s = self.resolve_val(rhs, ty);
                        let l_actual = self.actual_type_of(lhs);
                        let r_actual = self.actual_type_of(rhs);
                        // Convert non-ptr operands to ptr via inttoptr.
                        let l_ptr = if l_actual != "ptr" && !matches!(lhs, IRValue::Global(_) | IRValue::Const(IRConst::Str(_))) {
                            let tmp = format!("%__streq_lptr_{}", dst.0);
                            prefix.push_str(&format!("{} = inttoptr {} {} to ptr\n  ", tmp, l_actual, l_s));
                            tmp
                        } else { l_s };
                        let r_ptr = if r_actual != "ptr" && !matches!(rhs, IRValue::Global(_) | IRValue::Const(IRConst::Str(_))) {
                            let tmp = format!("%__streq_rptr_{}", dst.0);
                            prefix.push_str(&format!("{} = inttoptr {} {} to ptr\n  ", tmp, r_actual, r_s));
                            tmp
                        } else { r_s };
                        let cmp_tmp = format!("%strcmptmp_{}", dst.0);
                        format!("{}{} = call i32 @strcmp(ptr {}, ptr {})\n  {} = icmp eq i32 {}, 0",
                            prefix, cmp_tmp, l_ptr, r_ptr, dst_name, cmp_tmp)
                    }
                    IRBinOp::StrNe => {
                        let mut prefix = String::new();
                        let l_s = self.resolve_val(lhs, ty);
                        let r_s = self.resolve_val(rhs, ty);
                        let l_actual = self.actual_type_of(lhs);
                        let r_actual = self.actual_type_of(rhs);
                        let l_ptr = if l_actual != "ptr" && !matches!(lhs, IRValue::Global(_) | IRValue::Const(IRConst::Str(_))) {
                            let tmp = format!("%__strne_lptr_{}", dst.0);
                            prefix.push_str(&format!("{} = inttoptr {} {} to ptr\n  ", tmp, l_actual, l_s));
                            tmp
                        } else { l_s };
                        let r_ptr = if r_actual != "ptr" && !matches!(rhs, IRValue::Global(_) | IRValue::Const(IRConst::Str(_))) {
                            let tmp = format!("%__strne_rptr_{}", dst.0);
                            prefix.push_str(&format!("{} = inttoptr {} {} to ptr\n  ", tmp, r_actual, r_s));
                            tmp
                        } else { r_s };
                        let cmp_tmp = format!("%strcmptmpne_{}", dst.0);
                        format!("{}{} = call i32 @strcmp(ptr {}, ptr {})\n  {} = icmp ne i32 {}, 0",
                            prefix, cmp_tmp, l_ptr, r_ptr, dst_name, cmp_tmp)
                    }
                }
            }

            // ---- UnOp -------------------------------------------------------
            IRInstr::UnOp { dst, op, operand, ty } => {
                let dst_name = format!("%{}", dst.0);
                let dst_llvm_ty = if matches!(op, IRUnOp::Not) { "i1".to_string() } else { llvm_type(ty) };
                self.record_temp_type(&dst.0, &dst_llvm_ty);
                let operand_s = self.resolve_val(operand, ty);
                match op {
                    IRUnOp::Neg => {
                        if matches!(ty, IRType::F64) {
                            format!("{} = fneg double {}", dst_name, operand_s)
                        } else {
                            format!("{} = sub {} 0, {}", dst_name, llvm_type(ty), operand_s)
                        }
                    }
                    IRUnOp::Not => {
                        // Boolean NOT: xor i1 %x, true
                        // If operand is integer (i64), first convert to i1 via icmp ne 0.
                        let actual_ty = self.actual_type_of(operand);
                        if actual_ty == "i64" || actual_ty == "i32" {
                            let bool_tmp = format!("%{}_bool", dst.0);
                            format!(
                                "{} = icmp ne {} {}, 0\n  {} = xor i1 {}, true",
                                bool_tmp, actual_ty, operand_s,
                                dst_name, bool_tmp
                            )
                        } else {
                            format!("{} = xor i1 {}, true", dst_name, operand_s)
                        }
                    }
                    IRUnOp::FNeg => {
                        format!("{} = fneg double {}", dst_name, operand_s)
                    }
                }
            }

            // ---- Assign -----------------------------------------------------
            // Emit an identity add/fadd that produces a valid SSA name and also
            // record the substitution for downstream uses.
            IRInstr::Assign { dst, src, ty } => {
                let src_s = self.resolve_val(src, ty);
                self.assigns.insert(dst.0.clone(), src_s.clone());
                // Propagate actual type from source to destination.
                let actual = self.actual_type_of(src);
                self.record_temp_type(&dst.0, &actual);
                let dst_name = format!("%{}", dst.0);
                match ty {
                    IRType::F64 => {
                        format!("{} = fadd double {}, 0.0", dst_name, src_s)
                    }
                    IRType::Void => String::new(),
                    _ => {
                        // Use the actual type for the identity add.
                        let use_ty = if actual == "ptr" {
                            // Can't add 0 to a pointer; use ptrtoint+inttoptr.
                            // Just skip and rely on the assign substitution.
                            return String::new();
                        } else {
                            actual.clone()
                        };
                        format!("{} = add {} {}, 0", dst_name, use_ty, src_s)
                    }
                }
            }

            // ---- Alloca -----------------------------------------------------
            IRInstr::Alloca { dst, ty } => {
                // Skip alloca for void type — can't allocate void.
                if matches!(ty, IRType::Void) {
                    return String::new();
                }
                self.record_temp_type(&dst.0, "ptr"); // alloca always produces a pointer
                let alloca_ty = llvm_type(ty);
                if let IRType::Struct(ref sname) = ty {
                    // Struct types: use malloc so the pointer survives function
                    // returns (alloca would create a dangling pointer). All struct
                    // fields are i64, so allocate n_fields * 8 bytes.
                    let n = self.struct_sizes.get(sname).copied().unwrap_or(1);
                    let bytes = n * 8;
                    return format!(
                        "%{} = call ptr @malloc(i64 {})",
                        dst.0,
                        bytes
                    );
                }
                if let IRType::Array(elem, n) = ty {
                    // Arrays too: they can be returned by pointer, and a
                    // stack alloca would dangle. Aggregate-typed elements
                    // are stored as 8-byte pointers.
                    let elem_bytes: usize = match &**elem {
                        IRType::I8 | IRType::Bool | IRType::Trit => 1,
                        IRType::I16 => 2,
                        IRType::I32 => 4,
                        _ => 8,
                    };
                    let bytes = (n * elem_bytes).max(1);
                    return format!("%{} = call ptr @malloc(i64 {})", dst.0, bytes);
                }
                let (alloca_ty, align) = (alloca_ty, llvm_align(ty));
                format!(
                    "%{} = alloca {}, align {}",
                    dst.0,
                    alloca_ty,
                    align
                )
            }

            // ---- Store ------------------------------------------------------
            IRInstr::Store { ptr, val, ty } => {
                // Skip store of void values.
                if matches!(ty, IRType::Void) {
                    return String::new();
                }
                let val_s = self.resolve_val(val, ty);
                let ptr_s = self.resolve_ptr_val(ptr);
                // Use the actual type of the value if it differs from the
                // declared type (e.g. ptr value stored as i64, or i8 stored as i64).
                let actual_ty = self.actual_type_of(val);
                let declared_ty = llvm_type(ty);
                // Determine the store type. Priority:
                // 1. If declared is struct, use i64 (enums/structs as ints).
                // 2. If actual is a struct, use ptr.
                // 3. If actual is ptr but declared isn't (or vice versa), use
                //    the non-i64 side (to avoid type mismatches).
                // 4. For integer types, use the declared type if the value is
                //    a constant (constants adapt to any width). Otherwise use
                //    the actual type if it differs.
                // 5. Fallback: declared type.
                let is_const = matches!(val, IRValue::Const(_));
                let store_ty = if declared_ty.starts_with("%struct.") {
                    match val {
                        IRValue::Const(IRConst::Int(_)) => "i64".to_string(),
                        _ => if actual_ty == "ptr" { "ptr".to_string() } else { "i64".to_string() }
                    }
                } else if actual_ty.starts_with("%struct.") {
                    "ptr".to_string()
                } else if actual_ty == "ptr" && declared_ty != "ptr" {
                    // Value is ptr, store as ptr.
                    "ptr".to_string()
                } else if declared_ty == "ptr" && actual_ty != "ptr" && !is_const {
                    // Declared is ptr but actual is an integer temp — use actual type.
                    actual_ty.clone()
                } else if !is_const && actual_ty != declared_ty
                    && actual_ty != "void" && declared_ty != "void"
                    && actual_ty != "ptr" && declared_ty != "ptr"
                {
                    // Both are integer types but widths differ.
                    // If actual is narrower, sext to declared width so the
                    // store matches the alloca. This avoids reading garbage
                    // bytes on a subsequent wider load.
                    let aw = int_width(&actual_ty);
                    let dw = int_width(&declared_ty);
                    if aw > 0 && dw > 0 && aw < dw {
                        // Emit sext inline: rewrite val_s to a sext expression.
                        // We need to return a multi-line store that includes the sext.
                        let ext_name = format!("%__store_sext_{}", self.fresh_anon("sx"));
                        let ptr_s2 = self.resolve_ptr_val(ptr);
                        return format!(
                            "{} = sext {} {} to {}\n  store {} {}, ptr {}, align {}",
                            ext_name, actual_ty, val_s, declared_ty,
                            declared_ty, ext_name, ptr_s2,
                            llvm_align(ty)
                        );
                    }
                    actual_ty.clone()
                } else {
                    declared_ty
                };
                let align = if store_ty == "ptr" { "8" } else { llvm_align(ty) };

                // Record what type was stored into this pointer, so that
                // subsequent Loads from the same pointer use the right type.
                if let IRValue::Temp(t) = ptr {
                    self.alloca_stored_types.insert(t.0.clone(), store_ty.clone());
                }

                format!(
                    "store {} {}, ptr {}, align {}",
                    store_ty,
                    val_s,
                    ptr_s,
                    align
                )
            }

            // ---- Load -------------------------------------------------------
            IRInstr::Load { dst, ptr, ty } => {
                let mut ty_str = llvm_type(ty);
                // Struct types: use ptr (struct values are always pointers).
                if ty_str.starts_with("%struct.") {
                    ty_str = "ptr".to_string();
                }
                // If we know a different type was stored into this pointer,
                // use that type instead of the IR-declared one.
                if let IRValue::Temp(t) = ptr {
                    if let Some(stored_ty) = self.alloca_stored_types.get(&t.0) {
                        if *stored_ty != ty_str {
                            ty_str = stored_ty.clone();
                        }
                    }
                }
                self.record_temp_type(&dst.0, &ty_str);
                let ptr_s = self.resolve_ptr_val(ptr);
                let align = if ty_str == "ptr" { "8" } else { llvm_align(ty) };
                format!(
                    "%{} = load {}, ptr {}, align {}",
                    dst.0,
                    ty_str,
                    ptr_s,
                    align
                )
            }

            // ---- Call -------------------------------------------------------
            IRInstr::Call { dst, func, args, ret_ty } => {
                let mangled = mangle_func_name(func);

                // Look up the declared signature for correct types.
                let sig = self.fn_sigs.get(&mangled).cloned();

                // Vararg callees keep a trailing "..." in their parsed
                // signature (see parse_declare_sigs).
                let is_vararg = sig
                    .as_ref()
                    .map(|(p, _)| p.last().map(|s| s == "...").unwrap_or(false))
                    .unwrap_or(false);
                let fixed_params = sig
                    .as_ref()
                    .map(|(p, _)| p.len() - if is_vararg { 1 } else { 0 })
                    .unwrap_or(0);

                let mut call_prefix = String::new();
                let args_str: Vec<String> = args
                    .iter()
                    .enumerate()
                    .map(|(i, a)| {
                        let declared = if let Some((ref param_types, _)) = sig {
                            if i < fixed_params {
                                param_types[i].clone()
                            } else if is_vararg {
                                "...".to_string()
                            } else {
                                self.actual_type_of(a)
                            }
                        } else {
                            self.actual_type_of(a)
                        };
                        let actual = self.actual_type_of(a);
                        let guessed = guess_value_type(a);
                        let val_s = self.resolve_val(a, &guessed);

                        // Vararg slot: apply the C default argument
                        // promotions — integers narrower than i64 widen to
                        // i64 (the runtime reads them with va_arg(int64_t)).
                        if declared == "..." {
                            if matches!(actual.as_str(), "i1" | "i8" | "i16" | "i32") {
                                let uid = self.fresh_anon("argv");
                                let coerce_name = format!("%{}", uid);
                                let op = if actual == "i1" { "zext" } else { "sext" };
                                call_prefix.push_str(&format!(
                                    "{} = {} {} {} to i64\n  ",
                                    coerce_name, op, actual, val_s
                                ));
                                return format!("i64 {}", coerce_name);
                            }
                            return format!("{} {}", actual, val_s);
                        }

                        // If declared and actual differ, insert coercion
                        if declared != actual {
                            let dw = int_width(&declared);
                            let aw = int_width(&actual);
                            if dw > 0 && aw > 0 && dw != aw {
                                let uid = self.fresh_anon("argc");
                                let coerce_name = format!("%{}", uid);
                                let op = if aw < dw { "sext" } else { "trunc" };
                                call_prefix.push_str(&format!(
                                    "{} = {} {} {} to {}\n  ",
                                    coerce_name, op, actual, val_s, declared
                                ));
                                return format!("{} {}", declared, coerce_name);
                            }
                            // ptr vs i64 mismatch: use inttoptr/ptrtoint
                            if declared == "ptr" && actual != "ptr" {
                                let uid = self.fresh_anon("argc");
                                let coerce_name = format!("%{}", uid);
                                call_prefix.push_str(&format!(
                                    "{} = inttoptr {} {} to ptr\n  ",
                                    coerce_name, actual, val_s
                                ));
                                return format!("ptr {}", coerce_name);
                            }
                            if actual == "ptr" && declared != "ptr" {
                                let uid = self.fresh_anon("argc");
                                let coerce_name = format!("%{}", uid);
                                call_prefix.push_str(&format!(
                                    "{} = ptrtoint ptr {} to {}\n  ",
                                    coerce_name, val_s, declared
                                ));
                                return format!("{} {}", declared, coerce_name);
                            }
                        }
                        format!("{} {}", declared, val_s)
                    })
                    .collect();

                // Use declared return type when available.
                let actual_ret = if let Some((_, ref ret_str)) = sig {
                    ret_str.clone()
                } else {
                    llvm_type(ret_ty)
                };

                // Record the return type for dst temp.
                if let Some(d) = dst {
                    self.record_temp_type(&d.0, &actual_ret);
                }

                // Vararg calls must spell out the full function type —
                // `call ptr (ptr, ...) @fmt_format(...)` — for x86-64 ABI
                // correctness (only the fixed-arg form was correct before).
                let callee_ty = if is_vararg {
                    let (params, _) = sig.as_ref().unwrap();
                    format!("{} ({})", actual_ret, params.join(", "))
                } else {
                    actual_ret.clone()
                };

                if actual_ret == "void" {
                    // The IR may still bind a dst temp to a void call (e.g. a
                    // lambda whose tail expression is a void print). A `call
                    // void` defines no SSA name, so substitute 0 for any
                    // later reference to keep the module well-formed.
                    if let Some(d) = dst {
                        self.assigns.insert(d.0.clone(), "0".to_string());
                        self.record_temp_type(&d.0, "i64");
                    }
                    format!(
                        "{}call {} @{}({})",
                        call_prefix,
                        callee_ty,
                        mangled,
                        args_str.join(", ")
                    )
                } else {
                    match dst {
                        Some(d) => format!(
                            "{}%{} = call {} @{}({})",
                            call_prefix,
                            d.0,
                            callee_ty,
                            mangled,
                            args_str.join(", ")
                        ),
                        None => format!(
                            "{}call {} @{}({})",
                            call_prefix,
                            callee_ty,
                            mangled,
                            args_str.join(", ")
                        ),
                    }
                }
            }

            // ---- GetPtr (getelementptr) -------------------------------------
            IRInstr::GetPtr { dst, ptr, idx, ty } => {
                self.record_temp_type(&dst.0, "ptr");
                let mut prefix = String::new();
                let mut ptr_s = self.resolve_ptr_val(ptr);
                // The base may be an integer temp (e.g. an address that came
                // through an i64 slot) — getelementptr requires a ptr operand.
                let base_actual = self.actual_type_of(ptr);
                if matches!(base_actual.as_str(), "i8" | "i16" | "i32" | "i64") {
                    let uid = self.fresh_anon("gepp");
                    prefix.push_str(&format!(
                        "%{} = inttoptr {} {} to ptr\n  ",
                        uid, base_actual, ptr_s
                    ));
                    ptr_s = format!("%{}", uid);
                }
                let (idx_prefix, idx_s) =
                    self.resolve_with_coerce(idx, "i64", &format!("{}_idx", dst.0));
                // A struct value is a pointer occupying one 8-byte slot (see
                // slot_access_ty), so an array of structs is an array of
                // pointers. Indexing it with %struct.X would stride by
                // sizeof(%struct.X) — 24 bytes for a three-field struct — and
                // run off the end of a malloc sized at n * 8.
                let gep_ty = match ty {
                    IRType::Struct(_) => "i64".to_string(),
                    _ => llvm_type(ty),
                };
                format!(
                    "{}{}%{} = getelementptr {}, ptr {}, i64 {}",
                    prefix,
                    idx_prefix,
                    dst.0,
                    gep_ty,
                    ptr_s,
                    idx_s
                )
            }

            // A2: array bounds guard. A call rather than a branch so no extra
            // basic blocks are needed; manit_check_index aborts via manit_fault
            // with the same message and exit status the T3 emulator uses.
            IRInstr::BoundsCheck { idx, len } => {
                let idx_ty = self.actual_type_of(idx);
                let idx_s = self.resolve_val(idx, &IRType::I64);
                let uid = self.fresh_anon("bc");
                let (wp, w) = self.widen_to_i64(&idx_s, &idx_ty, &uid);
                format!(
                    "{}call void @manit_check_index(i64 {}, i64 {})",
                    wp, w, len
                )
            }

            // ---- Ternary ops ------------------------------------------------
            IRInstr::TritMin { dst, a, b } => {
                self.record_temp_type(&dst.0, "i8");
                // ternary AND = min(a, b) — coerce operands to i8
                let (ap, a_s) = self.resolve_with_coerce(a, "i8", &format!("{}_a", dst.0));
                let (bp, b_s) = self.resolve_with_coerce(b, "i8", &format!("{}_b", dst.0));
                let cmp = format!("%__tmin_cmp_{}", dst.0);
                format!(
                    "{ap}{bp}{cmp} = icmp slt i8 {a}, {b}\n  %{dst} = select i1 {cmp}, i8 {a}, i8 {b}",
                    ap = ap, bp = bp,
                    cmp = cmp,
                    dst = dst.0,
                    a = a_s,
                    b = b_s
                )
            }

            IRInstr::TritMax { dst, a, b } => {
                self.record_temp_type(&dst.0, "i8");
                // ternary OR = max(a, b) — coerce operands to i8
                let (ap, a_s) = self.resolve_with_coerce(a, "i8", &format!("{}_a", dst.0));
                let (bp, b_s) = self.resolve_with_coerce(b, "i8", &format!("{}_b", dst.0));
                let cmp = format!("%__tmax_cmp_{}", dst.0);
                format!(
                    "{ap}{bp}{cmp} = icmp sgt i8 {a}, {b}\n  %{dst} = select i1 {cmp}, i8 {a}, i8 {b}",
                    ap = ap, bp = bp,
                    cmp = cmp,
                    dst = dst.0,
                    a = a_s,
                    b = b_s
                )
            }

            IRInstr::TritNeg { dst, a } => {
                self.record_temp_type(&dst.0, "i8");
                let (ap, a_s) = self.resolve_with_coerce(a, "i8", &format!("{}_a", dst.0));
                format!("{}%{} = sub i8 0, {}", ap, dst.0, a_s)
            }

            // ---- Print intrinsics -------------------------------------------
            IRInstr::PrintStr(val) => {
                let s = self.resolve_ptr_val(val);
                format!("call i32 (ptr, ...) @printf(ptr {})", s)
            }

            IRInstr::PrintInt(val) => {
                // The value may live in a narrower register (e.g. an i32
                // runtime call result); widen it to the i64 printf slot.
                let uid = self.fresh_anon("pint");
                let (prefix, s) = self.resolve_with_coerce(val, "i64", &uid);
                format!("{}call i32 (ptr, ...) @printf(ptr @fmt_int, i64 {})", prefix, s)
            }

            IRInstr::PrintFloat(val) => {
                let s = self.resolve_val(val, &IRType::F64);
                format!("call i32 (ptr, ...) @printf(ptr @fmt_float, double {})", s)
            }

            IRInstr::PrintTrit(val) => {
                let uid = self.fresh_anon("ptrit");
                let (prefix, s) = self.resolve_with_coerce(val, "i8", &uid);
                format!("{}call void @__manit_print_trit(i8 {})", prefix, s)
            }

            IRInstr::PrintBool3(val) => {
                let uid = self.fresh_anon("pb3");
                let (prefix, s) = self.resolve_with_coerce(val, "i8", &uid);
                format!("{}call void @__manit_print_bool3(i8 {})", prefix, s)
            }

            // ---- Phi --------------------------------------------------------
            IRInstr::Phi { dst, ty, incoming } => {
                let mut phi_ty = llvm_type(ty);
                // Struct types are stored as i64 in our IR.
                if phi_ty.starts_with("%struct.") {
                    phi_ty = "i64".to_string();
                }
                // Check if any incoming value has a different actual type.
                for (v, _) in incoming {
                    let actual = self.actual_type_of(v);
                    if actual != phi_ty && actual != "void" {
                        if actual == "ptr" {
                            phi_ty = "ptr".to_string();
                            break;
                        }
                    }
                }
                self.record_temp_type(&dst.0, &phi_ty);

                // Emitted predecessors of the block this PHI lives in
                // (self.current_block_label). block_predecessors reflects
                // the CFG as EMITTED: for a TritBranch, the zero/neg targets
                // are reached through the synthesized <pred>__tneg_check
                // block, not the IR block itself (B20).
                let preds: Vec<String> = self
                    .current_block_label
                    .as_ref()
                    .and_then(|b| self.block_predecessors.get(b))
                    .cloned()
                    .unwrap_or_default();

                // Build the incoming edges. Incoming labels that name the
                // IR-level TritBranch block are rewritten to the emitted
                // __tneg_check block when that is the real predecessor.
                let mut seen_labels: Vec<String> = Vec::new();
                let mut pairs: Vec<String> = incoming
                    .iter()
                    .map(|(v, label)| {
                        let vs = if phi_ty == "ptr" {
                            let s = self.resolve_val(v, ty);
                            if s == "0" { "null".to_string() } else { s }
                        } else {
                            self.resolve_val(v, ty)
                        };
                        let mut lbl = label.clone();
                        if !preds.iter().any(|p| *p == lbl) {
                            let check = format!("{}__tneg_check", lbl);
                            if preds.iter().any(|p| *p == check) {
                                lbl = check;
                            }
                        }
                        seen_labels.push(lbl.clone());
                        format!("[ {}, %{} ]", vs, lbl)
                    })
                    .collect();

                // LLVM requires every predecessor of a block to appear in
                // the PHI. If the predecessor map has more predecessors
                // than the IR's incoming list, add undef entries.
                for pred in &preds {
                    if !seen_labels.iter().any(|l| l == pred) {
                        let undef_val = if phi_ty == "ptr" { "null" } else { "undef" };
                        pairs.push(format!("[ {}, %{} ]", undef_val, pred));
                    }
                }

                format!("%{} = phi {} {}", dst.0, phi_ty, pairs.join(", "))
            }

            // ---- Cast -------------------------------------------------------
            IRInstr::Cast { dst, src, from_ty, to_ty } => {
                self.record_temp_type(&dst.0, &llvm_type(to_ty));
                // If the temp's actual width differs from the declared
                // from-type (e.g. an i64 temp cast "from" Trit), coerce it
                // first so the cast sequence sees the type it expects.
                let (prefix, src_s) =
                    self.resolve_with_coerce(src, &llvm_type(from_ty), &format!("{}_cast", dst.0));
                format!("{}{}", prefix, cast_sequence(&dst.0, &src_s, from_ty, to_ty))
            }
            // ---- CallIndirect -----------------------------------------------
            // Emits:  %dst = call <ret_ty> %fn_ptr(<typed_args>)
            // Using LLVM 15+ opaque-pointer style: the callee is a `ptr` value.
            // The function type is NOT emitted between ret_ty and the pointer —
            // LLVM infers it from the call arguments and return type.
            IRInstr::CallIndirect { dst, fn_ptr, args, ret_ty } => {
                // Build the typed argument list: "i64 %x, double %y, ..."
                let typed_args: Vec<String> = args
                    .iter()
                    .map(|a| {
                        let ty_s = self.actual_type_of(a);
                        let guessed = guess_value_type(a);
                        format!("{} {}", ty_s, self.resolve_val(a, &guessed))
                    })
                    .collect();

                // Resolve the function-pointer operand.
                let fp_s = self.resolve_ptr_val(fn_ptr);
                let ret_str = llvm_type(ret_ty);

                // Record return type for dst.
                if let Some(d) = dst {
                    self.record_temp_type(&d.0, &ret_str);
                }

                // A void indirect call defines no SSA name (see Call above).
                if ret_str == "void" {
                    if let Some(d) = dst {
                        self.assigns.insert(d.0.clone(), "0".to_string());
                        self.record_temp_type(&d.0, "i64");
                    }
                    return format!("call void {}({})", fp_s, typed_args.join(", "));
                }

                match dst {
                    Some(d) => format!(
                        "%{} = call {} {}({})",
                        d.0,
                        ret_str,
                        fp_s,
                        typed_args.join(", ")
                    ),
                    None => format!(
                        "call {} {}({})",
                        ret_str,
                        fp_s,
                        typed_args.join(", ")
                    ),
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Terminator → Vec<String> of lines
    // -----------------------------------------------------------------------

    pub(super) fn emit_terminator(&mut self, term: &IRTerminator) -> Vec<String> {
        match term {
            IRTerminator::Return(None) => {
                // If the function has a non-void return type, return a zero default
                // to avoid LLVM "ret void in non-void function" errors.
                let ret_ty = &self.current_ret_ty;
                if *ret_ty == crate::ir::types::IRType::Void {
                    vec!["ret void".to_string()]
                } else {
                    let ty_str = llvm_abi_type(ret_ty);
                    let zero = if ty_str == "ptr" { "null" } else { "0" };
                    vec![format!("ret {} {}", ty_str, zero)]
                }
            }

            IRTerminator::Return(Some(val)) => {
                // If the value is Void but the function returns non-void,
                // this is a dead merge block — return zero default.
                if matches!(val, IRValue::Void) && self.current_ret_ty != IRType::Void {
                    let ty_str = llvm_abi_type(&self.current_ret_ty);
                    let zero = if ty_str == "ptr" { "null" } else { "0" };
                    return vec![format!("ret {} {}", ty_str, zero)];
                }
                // Determine the return type. Use the function's declared
                // return type (which already maps struct→ptr) — constants
                // adapt to the declared width; guessing i64 for a plain `0`
                // in an i8-returning function produced `ret i64 0` (K1).
                let ty = match val {
                    IRValue::Temp(_) => self.current_ret_ty.clone(),
                    _ if self.current_ret_ty != IRType::Void => self.current_ret_ty.clone(),
                    _ => guess_value_type(val),
                };
                // Aggregate returns (structs and arrays) use ptr in the ABI.
                let ty_str = llvm_abi_type(&ty);
                let vs = self.resolve_val(val, &ty);

                // Fix type mismatch: if the actual value type differs from
                // the function return type, insert a cast. Integer constants
                // need no cast — they adapt to the declared type (but string
                // and null constants are genuinely ptr-typed, so keep those).
                let actual = self.actual_type_of(val);
                let adapts = matches!(
                    val,
                    IRValue::Const(IRConst::Int(_))
                        | IRValue::Const(IRConst::Bool(_))
                        | IRValue::Const(IRConst::Trit(_))
                );
                if !adapts && actual != ty_str && actual != "void" && ty_str != "void" {
                    if actual == "ptr" && ty_str != "ptr" {
                        // ptr → int: ptrtoint
                        let ext_name = format!("%{}", self.fresh_anon("__ret_coerce"));
                        return vec![
                            format!("{} = ptrtoint ptr {} to {}", ext_name, vs, ty_str),
                            format!("ret {} {}", ty_str, ext_name),
                        ];
                    } else if ty_str == "ptr" && actual != "ptr" {
                        // int → ptr: inttoptr
                        let ext_name = format!("%{}", self.fresh_anon("__ret_coerce"));
                        return vec![
                            format!("{} = inttoptr {} {} to ptr", ext_name, actual, vs),
                            format!("ret ptr {}", ext_name),
                        ];
                    } else {
                        let aw = int_width(&actual);
                        let tw = int_width(&ty_str);
                        if aw != tw && aw > 0 && tw > 0 {
                            let op = if aw < tw { "sext" } else { "trunc" };
                            let ext_name = format!("%{}", self.fresh_anon("__ret_coerce"));
                            return vec![
                                format!("{} = {} {} {} to {}", ext_name, op, actual, vs, ty_str),
                                format!("ret {} {}", ty_str, ext_name),
                            ];
                        }
                    }
                }

                // Null-coerce: ptr type must use "null", never literal "0".
                let vs = if ty_str == "ptr" && vs == "0" { "null".to_string() } else { vs };
                vec![format!("ret {} {}", ty_str, vs)]
            }

            IRTerminator::Jump(label) => vec![format!("br label %{}", label)],

            IRTerminator::BinBranch { cond, true_label, false_label } => {
                let cs = self.resolve_val(cond, &IRType::Bool);
                let actual = self.actual_type_of(cond);
                if actual == "i64" || actual == "i32" || actual == "i8" {
                    // Coerce integer condition to i1 via icmp ne 0
                    let cond_name = match cond {
                        IRValue::Temp(t) => t.0.clone(),
                        _ => "cond".to_string(),
                    };
                    let bool_tmp = format!("%__brbool_{}", cond_name);
                    vec![
                        format!("{} = icmp ne {} {}, 0", bool_tmp, actual, cs),
                        format!("br i1 {}, label %{}, label %{}", bool_tmp, true_label, false_label),
                    ]
                } else {
                    vec![format!(
                        "br i1 {}, label %{}, label %{}",
                        cs, true_label, false_label
                    )]
                }
            }

            IRTerminator::TritBranch { cond, pos_label, zero_label, neg_label } => {
                // Emits:
                //   %__tposN = icmp sgt i8 %cond, 0
                //   br i1 %__tposN, label %pos, label %__tneg_checkN
                // __tneg_checkN:
                //   %__tnegN = icmp slt i8 %cond, 0
                //   br i1 %__tnegN, label %neg, label %zero
                //
                // The intermediate check block is emitted as a bare label
                // line (prefixed with '\n' so emit_block knows not to indent it).
                // Its name is derived from the current block label so that
                // the predecessor map built before emission (emit_function)
                // names the same block (B20).
                let mut cs = self.resolve_val(cond, &IRType::I8);
                // The condition temp may actually be wider than i8 (e.g. a
                // trit loaded through an i64 slot) — compare in its real type.
                let mut pre = Vec::new();
                let cond_ty = match self.actual_type_of(cond).as_str() {
                    "i1" => {
                        // A two-valued bool as trit condition: widen so the
                        // signed compares below see 0 / +1 (not i1's 0 / -1).
                        let z = self.fresh_anon("__tzext");
                        pre.push(format!("%{} = zext i1 {} to i8", z, cs));
                        cs = format!("%{}", z);
                        "i8".to_string()
                    }
                    t @ ("i16" | "i32" | "i64") => t.to_string(),
                    _ => "i8".to_string(),
                };
                let check_lbl = format!(
                    "{}__tneg_check",
                    self.current_block_label.clone().unwrap_or_default()
                );
                let tpos = self.fresh_anon("__tpos");
                let tneg = self.fresh_anon("__tneg");
                let mut lines = pre;
                lines.extend([
                    format!("%{} = icmp sgt {} {}, 0", tpos, cond_ty, cs),
                    format!("br i1 %{}, label %{}, label %{}", tpos, pos_label, check_lbl),
                    // A '\n'-prefixed string is treated as a bare label by emit_block.
                    format!("\n{}:", check_lbl),
                    format!("%{} = icmp slt {} {}, 0", tneg, cond_ty, cs),
                    format!("br i1 %{}, label %{}, label %{}", tneg, neg_label, zero_label),
                ]);
                lines
            }

            IRTerminator::Unreachable => vec!["unreachable".to_string()],
        }
    }

    // -----------------------------------------------------------------------
    // Value resolution helpers
    // -----------------------------------------------------------------------

    /// Resolve an IRValue to its LLVM operand string.
    /// Applies Assign substitutions so that aliased temps are replaced inline.
    pub(super) fn resolve_val(&self, val: &IRValue, ty: &IRType) -> String {
        match val {
            IRValue::Temp(t) => {
                if let Some(sub) = self.assigns.get(&t.0) {
                    sub.clone()
                } else {
                    format!("%{}", t.0)
                }
            }
            IRValue::Const(c) => irconst_to_string(c, ty),
            // Global names may carry module paths (`t27f::ZERO`) — mangle
            // them the same way the definition site does.
            IRValue::Global(name) => format!("@{}", mangle_func_name(name)),
            IRValue::Void => String::new(),
        }
    }

    /// Resolve a pointer operand (always `ptr` in LLVM 15+, but also handles
    /// string-literal and null constants).
    pub(super) fn resolve_ptr_val(&self, val: &IRValue) -> String {
        match val {
            IRValue::Temp(t) => {
                if let Some(sub) = self.assigns.get(&t.0) {
                    sub.clone()
                } else {
                    format!("%{}", t.0)
                }
            }
            IRValue::Global(name) => format!("@{}", mangle_func_name(name)),
            IRValue::Const(IRConst::Null) => "null".to_string(),
            IRValue::Const(IRConst::Str(label)) => {
                format!("@{}", label.trim_start_matches('@'))
            }
            other => self.resolve_val(other, &IRType::Ptr(Box::new(IRType::I8))),
        }
    }

    /// Resolve both operands of a binary operation with the same type hint.
    pub(super) fn resolve_pair(&self, lhs: &IRValue, rhs: &IRValue, ty: &IRType) -> (String, String) {
        (self.resolve_val(lhs, ty), self.resolve_val(rhs, ty))
    }

    /// Resolve an operand and emit sext/trunc if its actual type differs from
    /// the target type. Returns (prefix_lines, resolved_name).
    /// prefix_lines is empty if no coercion is needed; otherwise it's a
    /// string like "%ext = sext i8 %val to i64\n  ".
    pub(super) fn resolve_with_coerce(
        &self,
        val: &IRValue,
        target_ty: &str,
        suffix: &str,
    ) -> (String, String) {
        let resolved = self.resolve_val(val, &IRType::I64);
        let actual = self.actual_type_of(val);

        // No coercion needed if types match, or if we're dealing with
        // non-integer types, or if target/actual is ptr/double/void.
        if actual == target_ty
            || actual == "ptr" || target_ty == "ptr"
            || actual == "double" || target_ty == "double"
            || actual == "void" || target_ty == "void"
        {
            return (String::new(), resolved);
        }

        let aw = int_width(&actual);
        let tw = int_width(target_ty);
        if aw == tw {
            return (String::new(), resolved);
        }

        let ext_name = format!("%__coerce_{}", suffix);
        // i1 holds a logical 0/1 — widening it must zero-extend (sext would
        // turn `true` into -1). All wider integer types are signed values.
        let op = if aw < tw {
            if actual == "i1" { "zext" } else { "sext" }
        } else {
            "trunc"
        };
        let prefix = format!(
            "{} = {} {} {} to {}\n  ",
            ext_name, op, actual, resolved, target_ty
        );
        (prefix, ext_name)
    }

    /// Sign-extend an already-resolved integer operand to i64.
    ///
    /// Used by the runtime fault guards (A7/A2), whose runtime helpers take
    /// i64 regardless of the width the arithmetic itself runs at. Returns
    /// (instruction prefix, operand name); the prefix is empty when the operand
    /// is already i64 or is a literal, which adapts to any integer width.
    pub(super) fn widen_to_i64(
        &self,
        operand: &str,
        operand_ty: &str,
        suffix: &str,
    ) -> (String, String) {
        if operand_ty == "i64"
            || int_width(operand_ty) == 0
            || operand.parse::<i64>().is_ok()
            || operand == "true"
            || operand == "false"
        {
            return (String::new(), operand.to_string());
        }
        let ext_name = format!("%__w64_{}", suffix);
        let op = if operand_ty == "i1" { "zext" } else { "sext" };
        (
            format!("{} = {} {} {} to i64\n  ", ext_name, op, operand_ty, operand),
            ext_name,
        )
    }

    /// Look up the actual LLVM type of a value.
    /// For Temp values, check temp_types first; fall back to guess_value_type.
    pub(super) fn actual_type_of(&self, val: &IRValue) -> String {
        match val {
            IRValue::Temp(t) => {
                let key = &t.0;
                if let Some(ty) = self.temp_types.get(key) {
                    ty.clone()
                } else {
                    // If this temp was substituted via assigns, it may point
                    // to %another_temp — try to resolve that temp's type.
                    if let Some(sub) = self.assigns.get(key) {
                        if sub.starts_with('%') {
                            let sub_key = &sub[1..]; // strip leading %
                            if let Some(ty) = self.temp_types.get(sub_key) {
                                return ty.clone();
                            }
                        }
                        // If it's a literal like "null", it's a pointer
                        if sub == "null" {
                            return "ptr".to_string();
                        }
                    }
                    llvm_type(&guess_value_type(val))
                }
            }
            IRValue::Global(_) => "ptr".to_string(),
            IRValue::Const(IRConst::Str(_)) => "ptr".to_string(),
            IRValue::Const(IRConst::Null) => "ptr".to_string(),
            _ => llvm_type(&guess_value_type(val)),
        }
    }

    /// Record the actual LLVM type for a temp.
    pub(super) fn record_temp_type(&mut self, name: &str, ty_str: &str) {
        self.temp_types.insert(name.to_string(), ty_str.to_string());
    }

    // -----------------------------------------------------------------------
    // Internal helper function definitions
    // -----------------------------------------------------------------------

    pub(super) fn emit_helper_print_trit(&self, out: &mut String) {
        out.push_str(concat!(
            "define internal void @__manit_print_trit(i8 %t) {\n",
            "entry:\n",
            "  %ispos = icmp sgt i8 %t, 0\n",
            "  br i1 %ispos, label %pos, label %notpos\n",
            "pos:\n",
            "  call i32 @putchar(i32 43)\n",  // '+'
            "  ret void\n",
            "notpos:\n",
            "  %isneg = icmp slt i8 %t, 0\n",
            "  br i1 %isneg, label %neg, label %zero\n",
            "neg:\n",
            "  call i32 @putchar(i32 45)\n",  // '-'
            "  ret void\n",
            "zero:\n",
            "  call i32 @putchar(i32 48)\n",  // '0'
            "  ret void\n",
            "}\n"
        ));
    }

    pub(super) fn emit_helper_print_bool3(&self, out: &mut String) {
        // Bool3: positive → "true", zero → "unknown", negative → "false"
        out.push_str(concat!(
            "define internal void @__manit_print_bool3(i8 %t) {\n",
            "entry:\n",
            "  %ispos = icmp sgt i8 %t, 0\n",
            "  br i1 %ispos, label %pos, label %notpos\n",
            "pos:\n",
            "; print \"true\\n\"\n",
            "  call i32 @putchar(i32 116)\n",  // 't'
            "  call i32 @putchar(i32 114)\n",  // 'r'
            "  call i32 @putchar(i32 117)\n",  // 'u'
            "  call i32 @putchar(i32 101)\n",  // 'e'
            "  ret void\n",
            "notpos:\n",
            "  %isneg = icmp slt i8 %t, 0\n",
            "  br i1 %isneg, label %neg, label %unknown\n",
            "neg:\n",
            "; print \"false\\n\"\n",
            "  call i32 @putchar(i32 102)\n",  // 'f'
            "  call i32 @putchar(i32 97)\n",   // 'a'
            "  call i32 @putchar(i32 108)\n",  // 'l'
            "  call i32 @putchar(i32 115)\n",  // 's'
            "  call i32 @putchar(i32 101)\n",  // 'e'
            "  ret void\n",
            "unknown:\n",
            "; print \"unknown\\n\"\n",
            "  call i32 @putchar(i32 117)\n",  // 'u'
            "  call i32 @putchar(i32 110)\n",  // 'n'
            "  call i32 @putchar(i32 107)\n",  // 'k'
            "  call i32 @putchar(i32 110)\n",  // 'n'
            "  call i32 @putchar(i32 111)\n",  // 'o'
            "  call i32 @putchar(i32 119)\n",  // 'w'
            "  call i32 @putchar(i32 110)\n",  // 'n'
            "  ret void\n",
            "}\n"
        ));
    }
}
