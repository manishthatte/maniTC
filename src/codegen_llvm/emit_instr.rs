// codegen_llvm/emit_instr.rs — LLVM IR instruction and terminator emission.
use super::*;

impl LLVMEmitter {
    pub(super) fn emit_instr(&mut self, instr: &IRInstr) -> String {
        // F-2: the ternary shifts exist so the T3 backend can reach TSHI/TSHR,
        // which are one instruction there. LLVM has no ternary shift, so they
        // are rewritten into the very operations they were reduced FROM and
        // handed to the normal path. Parity is then by construction rather
        // than by a second implementation kept in step with the first — which
        // matters most for TShr, whose partner `DivNear` is a twenty-line
        // round-to-nearest sequence.
        if let IRInstr::BinOp {
            dst,
            op: op @ (IRBinOp::TShl | IRBinOp::TShlT27 | IRBinOp::TShr),
            lhs,
            rhs,
            ty,
        } = instr
        {
            if let IRValue::Const(IRConst::Int(k)) = rhs {
                let pow = 3i64.checked_pow((*k).clamp(0, 38) as u32).unwrap_or(i64::MAX);
                let equivalent = IRInstr::BinOp {
                    dst: dst.clone(),
                    op: match op {
                        IRBinOp::TShl => IRBinOp::Mul,
                        IRBinOp::TShlT27 => IRBinOp::MulT27,
                        _ => IRBinOp::DivNear,
                    },
                    lhs: lhs.clone(),
                    rhs: IRValue::Const(IRConst::Int(pow)),
                    ty: ty.clone(),
                };
                return self.emit_instr(&equivalent);
            }
        }
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

                // C4/N5: the version-dependent operators are integer-only by
                // construction — `binop_to_ir` never produces one for a float
                // type — and the arms below emit `sdiv double` and friends if
                // one arrives anyway. clang rejects that, so the failure is at
                // least loud here; this makes it loud in the test suite too,
                // and names the cause instead of a line of .ll.
                debug_assert!(
                    !(is_float && matches!(op,
                        IRBinOp::DivNear | IRBinOp::RemNear
                        | IRBinOp::AddT27 | IRBinOp::SubT27 | IRBinOp::MulT27)),
                    "a version-dependent integer op reached the LLVM emitter \
                     with a float type: {:?}", op
                );

                match op {
                    // --- N5: `int` arithmetic bounded to the 27-trit word ---
                    //
                    // The guard is called BEFORE the arithmetic, on the
                    // OPERANDS, for the same reason `manit_check_divisor` is:
                    // it is exact that way. `manit_check_t27_mul` computes the
                    // product in __int128, so a multiplication that overflows
                    // int64 is caught on its true value; checking the result
                    // afterwards in i64 would have missed exactly the products
                    // that overflow hardest, because they wrap into range.
                    //
                    // A call, not a compare-and-branch — this emitter produces
                    // one straight-line sequence per IR instruction and cannot
                    // open a basic block in the middle of one. It is the cost
                    // the divisor guard has always paid on every integer
                    // division, and only code compiled `--lang v2` pays it.
                    IRBinOp::AddT27 | IRBinOp::SubT27 | IRBinOp::MulT27 => {
                        let t = llvm_type(ty);
                        let (lp, l) = self.resolve_with_coerce(lhs, &t, &format!("{}_l", dst.0));
                        let (rp, r) = self.resolve_with_coerce(rhs, &t, &format!("{}_r", dst.0));
                        let (lwp, lw) = self.widen_to_i64(&l, &t, &format!("{}_gl", dst.0));
                        let (rwp, rw) = self.widen_to_i64(&r, &t, &format!("{}_gr", dst.0));
                        let (guard, op_name) = match op {
                            IRBinOp::AddT27 => ("manit_check_t27_add", "add"),
                            IRBinOp::SubT27 => ("manit_check_t27_sub", "sub"),
                            _ => ("manit_check_t27_mul", "mul"),
                        };
                        format!(
                            "{}{}{}{}  call void @{}(i64 {}, i64 {})\n{} = {} {} {}, {}",
                            lp, rp, lwp, rwp, guard, lw, rw,
                            dst_name, op_name, t, l, r,
                        )
                    }
                    // --- C4: round-to-nearest division and its remainder ---
                    //
                    // Sixteen instructions here against ONE on T3 (TDIVN,
                    // T3ISA v1.6). That ratio is the recommendation's point
                    // rather than an embarrassment: in balanced ternary
                    // dropping low trits already rounds to nearest, so the
                    // machine gets this for free and a two's-complement one
                    // has to build it. Emitted only under `--lang v2`.
                    //
                    // Branchless on purpose. A branch here would need new
                    // basic blocks in the middle of an instruction that the
                    // rest of this emitter assumes produces a straight line,
                    // and the select chain is what the T3 side is too — no
                    // control flow on either backend.
                    IRBinOp::DivNear | IRBinOp::RemNear => {
                        let t = llvm_type(ty);
                        let (lp, l) = self.resolve_with_coerce(lhs, &t, &format!("{}_l", dst.0));
                        let (rp, r) = self.resolve_with_coerce(rhs, &t, &format!("{}_r", dst.0));
                        // A7: the same divisor guard the truncating pair uses.
                        // Without it a zero divisor is SIGFPE — a hard crash
                        // with no message and no buffered output — where T3
                        // reports a clean trap.
                        let (wp, w) = self.widen_to_i64(&r, &t, &format!("{}_dz", dst.0));
                        let n = &dst.0;
                        // `nr` and `nb` are NEGATIVE magnitudes: −|r| and −|b|.
                        // Every i64 has one, including i64::MIN, whose
                        // positive magnitude is not representable — see
                        // lang::div_nearest for why the test is written this
                        // way rather than as `2*abs(r) >= abs(b)`.
                        let mut out = String::new();
                        out.push_str(&lp);
                        out.push_str(&rp);
                        out.push_str(&wp);
                        out.push_str(&format!("  call void @manit_check_divisor(i64 {})\n", w));
                        out.push_str(&format!("  %{n}_q = sdiv {t} {l}, {r}\n"));
                        out.push_str(&format!("  %{n}_rm = srem {t} {l}, {r}\n"));
                        out.push_str(&format!("  %{n}_rn = sub {t} 0, %{n}_rm\n"));
                        out.push_str(&format!("  %{n}_rp = icmp sgt {t} %{n}_rm, 0\n"));
                        out.push_str(&format!("  %{n}_nr = select i1 %{n}_rp, {t} %{n}_rn, {t} %{n}_rm\n"));
                        out.push_str(&format!("  %{n}_bn = sub {t} 0, {r}\n"));
                        out.push_str(&format!("  %{n}_bp = icmp sgt {t} {r}, 0\n"));
                        out.push_str(&format!("  %{n}_nb = select i1 %{n}_bp, {t} %{n}_bn, {t} {r}\n"));
                        // nb − nr is −(|b| − |r|), always in [MIN, −1].
                        out.push_str(&format!("  %{n}_th = sub {t} %{n}_nb, %{n}_nr\n"));
                        out.push_str(&format!("  %{n}_tie = icmp sle {t} %{n}_nr, %{n}_th\n"));
                        // Away from zero: the direction is the quotient's own
                        // sign, i.e. the two operand signs taken together.
                        out.push_str(&format!("  %{n}_ls = icmp slt {t} {l}, 0\n"));
                        out.push_str(&format!("  %{n}_bs = icmp slt {t} {r}, 0\n"));
                        out.push_str(&format!("  %{n}_sm = icmp eq i1 %{n}_ls, %{n}_bs\n"));
                        out.push_str(&format!("  %{n}_st = select i1 %{n}_sm, {t} 1, {t} -1\n"));
                        out.push_str(&format!("  %{n}_ad = select i1 %{n}_tie, {t} %{n}_st, {t} 0\n"));
                        if matches!(op, IRBinOp::DivNear) {
                            out.push_str(&format!("{dst_name} = add {t} %{n}_q, %{n}_ad"));
                        } else {
                            // The balanced remainder is DEFINED as
                            // a − div_nearest(a, b) * b rather than computed
                            // by a rule of its own. That is what keeps
                            // `(a / b) * b + (a % b) == a` true, and it is why
                            // C4 changes both operators or neither.
                            out.push_str(&format!("  %{n}_qn = add {t} %{n}_q, %{n}_ad\n"));
                            out.push_str(&format!("  %{n}_qb = mul {t} %{n}_qn, {r}\n"));
                            out.push_str(&format!("{dst_name} = sub {t} {l}, %{n}_qb"));
                        }
                        out
                    }
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
                            // P92: either operand may be a TYPE-ERASED payload
                            // word loaded as i64, which needs a bitcast — not a
                            // conversion — to double. The INTEGER branch below
                            // has had `resolve_with_coerce` throughout; this one
                            // went through a bare `resolve_pair`, which is the
                            // same asymmetry the float comparisons had.
                            let (pfx, l, r) =
                                self.resolve_float_pair(lhs, rhs, &format!("{}_a", dst.0));
                            let op_name = match op {
                                IRBinOp::Add => "fadd",
                                IRBinOp::Sub => "fsub",
                                IRBinOp::Mul => "fmul",
                                IRBinOp::Div => "fdiv",
                                IRBinOp::Rem => "frem",
                                _ => unreachable!(),
                            };
                            format!("{}{} = {} {} {}, {}", pfx, dst_name, op_name, ty_str, l, r)
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
                    // All six go through `emit_fcmp_coerced` (P92): either
                    // operand may be a TYPE-ERASED payload word loaded as i64,
                    // which needs a bitcast — not a conversion — to double.
                    IRBinOp::FEq => self.emit_fcmp_coerced(&dst_name, "oeq", lhs, rhs),
                    IRBinOp::FNe => {
                        // `une`, not `one` (P20). IEEE-754 says `!=` is TRUE
                        // when either operand is a NaN, and it is the ONE
                        // comparison of the six for which that is so — the
                        // other five are ordered predicates, which return
                        // false on a NaN, which is the right answer for them.
                        // `one` is ORDERED not-equal and returned false, so
                        // `nan != 1.0` was false on LLVM.
                        self.emit_fcmp_coerced(&dst_name, "une", lhs, rhs)
                    }
                    IRBinOp::FLt => self.emit_fcmp_coerced(&dst_name, "olt", lhs, rhs),
                    IRBinOp::FGt => self.emit_fcmp_coerced(&dst_name, "ogt", lhs, rhs),
                    IRBinOp::FLe => self.emit_fcmp_coerced(&dst_name, "ole", lhs, rhs),
                    IRBinOp::FGe => self.emit_fcmp_coerced(&dst_name, "oge", lhs, rhs),
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
                    // Handled by the rewrite at the top of `emit_instr`, which
                    // only fires for a CONSTANT shift amount — the only kind
                    // `strength_reduce` produces. A dynamic one would need
                    // 3^k computed at runtime and has no source form.
                    IRBinOp::TShl | IRBinOp::TShlT27 | IRBinOp::TShr => unreachable!(
                        "ternary shift with a non-constant amount reached the \
                         LLVM emitter: {:?}", instr
                    ),
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
                    //
                    // Tuples are not in struct_sizes — they are structural, not
                    // declared — so their arity is carried in the name and read
                    // back here. Before 19 August 2026 they shared the single
                    // name "<tuple>", missed this lookup, and took the
                    // `unwrap_or(1)` path: 8 bytes for a tuple of any arity.
                    // A 2-tuple then overflowed its allocation by 8 bytes on
                    // every construction, corrupting whatever the allocator had
                    // placed next. That is ORACLE_FINDINGS.md Section 10, which
                    // presented as "LLVM loses a trit argument in a loop"
                    // because the damage depended on allocator state.
                    let n = self
                        .struct_sizes
                        .get(sname)
                        .copied()
                        .or_else(|| tuple_arity_from_name(sname))
                        .unwrap_or_else(|| {
                            // A tuple-shaped name whose arity would not parse is
                            // a naming regression, and silently sizing it at one
                            // slot is exactly the bug this whole path had.
                            assert!(
                                !sname.starts_with("<tuple"),
                                "internal error: tuple type `{}` carries no \
                                 parsable arity — IRType::from_mani must emit \
                                 `<tuple:N>`. Sizing it at one slot would \
                                 overflow the allocation, which is \
                                 ORACLE_FINDINGS.md Section 10.",
                                sname
                            );
                            // Everything else reaching here is a native opaque
                            // handle — Vec, Map, Set, Deque, TernaryTrie,
                            // Channel, Mutex, AtomicTrit, Barrier, Task — which
                            // the runtime hands back as a single pointer-sized
                            // value. Declared structs and enums are registered
                            // in struct_sizes, so they never land here.
                            1
                        });
                    let bytes = (n * 8).max(8);
                    return format!(
                        "%{} = call ptr @manit_alloc(i64 {})",
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
                    return format!("%{} = call ptr @manit_alloc(i64 {})", dst.0, bytes);
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
                    // Both are integer types but widths differ. Either way the
                    // ALLOCA is the authority on how many bytes exist here, so
                    // the value is converted to the slot's width — never the
                    // other way round.
                    let aw = int_width(&actual_ty);
                    let dw = int_width(&declared_ty);
                    if aw > 0 && dw > 0 && aw != dw {
                        // Narrower value: sext, so a subsequent wider load does
                        // not read garbage bytes.
                        //
                        // Wider value: trunc. This case used to fall through to
                        // `actual_ty` below — storing at the VALUE's width into
                        // a slot sized for the DECLARED type, walking off the
                        // end of the allocation. `tryte` is i16 and
                        // `@ternary_tryte_from_trits` returns i64, so
                        //     %t0 = alloca i16, align 2
                        //     store i64 %t1, ptr %t0, align 2
                        // put six bytes past a two-byte stack slot, through the
                        // return address. The stored value was CORRECT and the
                        // function computed the right answer — it segfaulted on
                        // return, after its output had been printed, which is
                        // why `tests/23_t3isa_instructions.mt` printed 132 PASS
                        // lines and then died. `int_to_tryte` and `int_to_t9`
                        // did the same; `fs::is_dir` overran into alignment
                        // padding and survived, which is worse, not better.
                        //
                        // Truncating cannot lose information for any type that
                        // reaches here: `tryte` holds ±364 and `t9` ±9841, both
                        // inside their slots by construction. If a native ever
                        // does return something out of range, a truncated value
                        // is a bug confined to one variable rather than a
                        // corrupted return address.
                        let op = if aw < dw { widen_op(&actual_ty) } else { "trunc" };
                        let ext_name = format!("%__store_{}_{}", op, self.fresh_anon("sx"));
                        let ptr_s2 = self.resolve_ptr_val(ptr);
                        return format!(
                            "{} = {} {} {} to {}\n  store {} {}, ptr {}, align {}",
                            ext_name, op, actual_ty, val_s, declared_ty,
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
                                let op = widen_op(&actual);
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
                                let op = if aw < dw { widen_op(&actual) } else { "trunc" };
                                call_prefix.push_str(&format!(
                                    "{} = {} {} {} to {}\n  ",
                                    coerce_name, op, actual, val_s, declared
                                ));
                                return format!("{} {}", declared, coerce_name);
                            }
                            // double vs i64: reinterpret the bits, do not convert
                            // the value.
                            //
                            // This is not a numeric conversion — `n as float`
                            // is lowered as an explicit cast long before here,
                            // so a raw double/i64 mismatch at a call boundary
                            // only arises where the callee's slot is
                            // TYPE-ERASED. The Result/Option box is the case
                            // that matters: `@Ok(i64)` stores its payload into
                            // a raw 8-byte word, so `Ok(1.5)` handed a double
                            // to an i64 parameter and the IR would not
                            // assemble (added 20 August 2026). A double and an
                            // i64 are both 8 bytes, which is what makes the
                            // bitcast legal and lossless.
                            //
                            // The width guard matters: bitcast requires equal
                            // bit widths, so a float/i32 pairing must fall
                            // through to the mismatch below rather than emit
                            // invalid IR.
                            if (declared == "i64" && actual == "double")
                                || (declared == "double" && actual == "i64")
                            {
                                let uid = self.fresh_anon("argb");
                                let coerce_name = format!("%{}", uid);
                                call_prefix.push_str(&format!(
                                    "{} = bitcast {} {} to {}\n  ",
                                    coerce_name, actual, val_s, declared
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

                // P13: when the declared C return is WIDER than the IR says the
                // value is, narrow it HERE, at the definition, and let the temp
                // carry its IR type from then on.
                //
                // The backend used to record the declared width and narrow at
                // each USE. That works for every construct whose type comes
                // from its operands, and fails for the one that does not: a phi
                // takes its type from the IR, so promoting a variable fed by
                // `Call { ret_ty: Trit }` emitted `phi i8` with an operand
                // defined as `i64` and clang rejected the module. Narrowing
                // once at the definition makes every use consistent, the phi
                // included, and lets `ssa::promotable_allocas` stop refusing
                // those slots.
                let ir_ret = llvm_type(ret_ty);
                // P46 generalises P13 from "narrower integer" to "any
                // representation change", because narrowing was never the
                // point — reconciling the DECLARED type with the IR's type at
                // the definition was. The case P13 missed is a generic
                // collection native: `Vec::get` is declared `i64` because an
                // element is one machine word, while the IR knows the element
                // type, so on a `Vec<str>` the IR says `ptr`. Unpromoted, the
                // variable was an `alloca ptr` and the store/load coerced;
                // promoted, the phi takes `ptr` from the IR and one of its
                // arms is the `i64` call — which clang rejects. So this was a
                // DEFAULT-BUILD failure from the day `--mem2reg` became the
                // default, and `--no-mem2reg` still compiles it.
                //
                // TWO conversions, not four, and the two absent ones are
                // absent for measured reasons rather than for tidiness.
                //
                // Integer WIDENING would have to choose between `sext` and
                // `zext`, which is a decision about the native's contract and
                // not a repair.
                //
                // The MIRROR case — declared `ptr`, IR says integer — looks
                // like the natural symmetric partner and is wrong. It was
                // written, and it turned three tests red: a `Result` handle
                // comes back from `@Ok` as a declared `ptr` and the IR types
                // it `i64`, but the backend then USES it as an address
                // (`load i64, ptr %t0`), so it deliberately keeps the declared
                // pointer type and coerces at each use. Converting at the
                // definition there destroys exactly what P13 was protecting
                // in the other direction. Symmetry is not an argument.
                let conv: Option<&'static str> = if dst.is_none() || actual_ret == ir_ret {
                    None
                } else if actual_ret.starts_with('i')
                    && ir_ret.starts_with('i')
                    && int_width(&actual_ret) > int_width(&ir_ret)
                {
                    Some("trunc")
                } else if actual_ret.starts_with('i') && ir_ret == "ptr" {
                    Some("inttoptr")
                } else {
                    None
                };
                let narrow = conv.is_some();
                let raw_name = if narrow {
                    Some(self.fresh_anon("callret"))
                } else {
                    None
                };

                // Record the return type for dst temp.
                if let Some(d) = dst {
                    let recorded = if narrow { &ir_ret } else { &actual_ret };
                    self.record_temp_type(&d.0, recorded);
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
                        Some(d) => match &raw_name {
                            // P13/P46: call into a scratch name, then convert
                            // into the temp the rest of the module refers to.
                            // The three opcodes share this syntax exactly.
                            Some(raw) => format!(
                                "{}%{} = call {} @{}({})\n  %{} = {} {} %{} to {}",
                                call_prefix,
                                raw,
                                callee_ty,
                                mangled,
                                args_str.join(", "),
                                d.0,
                                conv.unwrap(),
                                actual_ret,
                                raw,
                                ir_ret
                            ),
                            None => format!(
                                "{}%{} = call {} @{}({})",
                                call_prefix,
                                d.0,
                                callee_ty,
                                mangled,
                                args_str.join(", ")
                            ),
                        },
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
            // C2. A runtime call, not inline IR.
            //
            // A lane-wise operation is a loop over 27 balanced-ternary digits,
            // and balanced ternary is not something binary hardware has a
            // representation for — there is no i64 bit trick that extracts a
            // balanced trit. Emitting the loop inline would put twenty-odd
            // basic blocks at every use site. On T3 this same instruction is
            // ONE opcode, and that gap is exactly the performance argument for
            // the ISA: the LLVM backend pays what a binary machine has to pay.
            IRInstr::TritLane { dst, op, a, b } => {
                self.record_temp_type(&dst.0, "i64");
                let (ap, a_s) = self.resolve_with_coerce(a, "i64", &format!("{}_a", dst.0));
                let (bp, b_s) = self.resolve_with_coerce(b, "i64", &format!("{}_b", dst.0));
                let func = match op {
                    IRLaneOp::And => "manit_lane_and",
                    IRLaneOp::Or => "manit_lane_or",
                    IRLaneOp::Xor => "manit_lane_xor",
                    IRLaneOp::Imp => "manit_lane_imp",
                    IRLaneOp::Cmp => "manit_lane_cmp",
                    IRLaneOp::Popcount => "manit_lane_popcount",
                };
                format!(
                    "{ap}{bp}%{dst} = call i64 @{func}(i64 {a}, i64 {b})",
                    ap = ap, bp = bp, dst = dst.0, func = func, a = a_s, b = b_s
                )
            }

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

            // C7: sign of a WORD. The operand is resolved at i64, NOT i8 —
            // that asymmetry is the whole reason this is its own instruction.
            // Every other Trit* op here coerces to i8 because its operand
            // really is a trit; this one's operand is a 27-trit word, and
            // truncating it would make `sign(256)` report 0. The RESULT is a
            // trit, so the destination is i8 as usual.
            //
            // T3 does this in one instruction (TCMP against R0). Binary has no
            // three-way compare, so it takes two comparisons and a subtract —
            // which is exactly the asymmetry C7 exists to point at.
            IRInstr::TritSign { dst, a } => {
                self.record_temp_type(&dst.0, "i8");
                let (ap, a_s) = self.resolve_with_coerce(a, "i64", &format!("{}_a", dst.0));
                format!(
                    "{ap}%{d}_pos = icmp sgt i64 {a}, 0\n  \
                     %{d}_neg = icmp slt i64 {a}, 0\n  \
                     %{d}_p8 = zext i1 %{d}_pos to i8\n  \
                     %{d}_n8 = zext i1 %{d}_neg to i8\n  \
                     %{d} = sub i8 %{d}_p8, %{d}_n8",
                    ap = ap, d = dst.0, a = a_s
                )
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
                // P94/P97: an array-returning function is DEFINED as
                // returning `ptr` (the `Alloca` arm mallocs arrays, and
                // `emit_function` returns that pointer), and the direct `Call`
                // arm above gets that from the declared signature. An indirect
                // call has no signature to consult and fell through to
                // `llvm_type`, which renders `[int; 6]` as `[6 x i64]` — so
                // the module said `%t3 = call [6 x i64] @mk()` against
                // `define ptr @mk()` and clang refused it. `manitc check`
                // exited 0, T3 ran the program and printed the callee's dead
                // frame, and LLVM would not link it at all.
                //
                // P97 fixed that with a hand-written `IRType::Array` arm, and
                // an AGGREGATE RETURNED BY A STRUCT TYPE went on falling
                // through — including a TUPLE, which `IRType::from_mani`
                // spells `Struct("<tuple:N>")`. That renders as
                // `%struct.<tuple:2>`, which is not a legal unquoted LLVM
                // identifier at all, so clang rejected the module on the TYPE
                // NAME rather than on a mismatch (P87's shape: a name the
                // compiler built that LLVM cannot spell).
                //
                // The repair is to ask the function `emit_function` itself
                // asks. `llvm_abi_type` maps every `%struct.*` and every
                // `[...]` to `ptr`, so the call site and the `define` cannot
                // disagree by construction rather than by a second list being
                // kept in step — this file's own recurring failure (P7, P46,
                // P59, P97: a declared type against the type the IR wants).
                let ret_str = llvm_abi_type(ret_ty);

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
                            let op = if aw < tw { widen_op(&actual) } else { "trunc" };
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

            // P113: `unreachable` alone is UNDEFINED BEHAVIOUR, not a trap.
            // The call goes first so the fault is taken; the `unreachable`
            // stays because it is still true after a noreturn call and it is
            // what lets clang drop everything after it.
            IRTerminator::Unreachable => vec![
                "call void @manit_unreachable()".to_string(),
                "unreachable".to_string(),
            ],
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
        // The hint matters for CONSTANTS, and it began to matter for FLOAT
        // constants with P92: `irconst_to_string` now spells a float as a
        // decimal bit pattern under an integer hint (the type-erased one-word
        // payload slot) and as an LLVM hex double otherwise. Resolving a
        // DOUBLE-targeted operand under a hardcoded i64 hint therefore emitted
        // `@llvm.fptosi.sat.i64.f64(double 4615964438073389875)`, which clang
        // rejects: "integer constant must have integer type".
        //
        // The hardcoded i64 was harmless for exactly as long as no arm of
        // `irconst_to_string` distinguished the two — which is this repo's own
        // note that teaching a type to CARRY something leaves every reader of
        // it newly incomplete, and the compiler cannot say so.
        let hint = if target_ty == "double" { IRType::F64 } else { IRType::I64 };
        let resolved = self.resolve_val(val, &hint);
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
        let op = if aw < tw { widen_op(&actual) } else { "trunc" };
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
        let op = widen_op(&operand_ty);
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

#[cfg(test)]
mod tuple_arity_tests {
    use crate::ir::types::tuple_arity_from_name;

    #[test]
    fn arity_round_trips_for_every_shape_the_lowering_emits() {
        for n in 0..=8 {
            let name = format!("<tuple:{}>", n);
            assert_eq!(tuple_arity_from_name(&name), Some(n), "name {}", name);
        }
    }

    #[test]
    fn declared_structs_and_the_old_name_are_not_mistaken_for_tuples() {
        // The old shared name carried no arity; it must NOT parse, so the
        // caller panics instead of silently under-allocating again.
        assert_eq!(tuple_arity_from_name("<tuple>"), None);
        assert_eq!(tuple_arity_from_name("Point"), None);
        assert_eq!(tuple_arity_from_name("<tuple:>"), None);
        assert_eq!(tuple_arity_from_name("<tuple:x>"), None);
    }
}
