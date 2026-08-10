// emitter/emit_instr.rs — Instruction and terminator emission for the T3ISA backend.
use super::*;
use crate::ir::*;

pub(super) fn emit_instr(em: &mut AsmEmitter, instr: &IRInstr) {
    match instr {
        // ------------------------------------------------------------------ BinOp
        IRInstr::BinOp { dst, op, lhs, rhs, ty: binop_ty } => {
            // String concatenation: ptr-typed `+` calls str_concat (syscall
            // 61), never integer TADD on the two handles (which produced a
            // meaningless address). Mirrors the LLVM backend's @str_concat.
            if matches!(op, IRBinOp::Add) && matches!(binop_ty, IRType::Ptr(_)) {
                em.rescue_reg_inclusive(1);
                em.rescue_reg_inclusive(2);
                let rd = em.dst_reg(dst);
                let rl = em.val_reg(lhs);
                let rr = em.val_reg(rhs);
                if rl != 1 && rr != 2 {
                    if rr == 1 {
                        em.emit(format!("    MOV   R21, {}  ; save rhs", AsmEmitter::rn(rr)));
                        if rl != 1 { em.emit(format!("    MOV   R1, {}", AsmEmitter::rn(rl))); }
                        em.emit("    MOV   R2, R21".to_string());
                    } else {
                        if rl != 1 { em.emit(format!("    MOV   R1, {}", AsmEmitter::rn(rl))); }
                        if rr != 2 { em.emit(format!("    MOV   R2, {}", AsmEmitter::rn(rr))); }
                    }
                } else if rl != 1 {
                    // rhs already in R2; just place lhs.
                    em.emit(format!("    MOV   R1, {}", AsmEmitter::rn(rl)));
                } else if rr != 2 {
                    em.emit(format!("    MOV   R2, {}", AsmEmitter::rn(rr)));
                }
                em.emit("    SYSCALL #61  ; str_concat".to_string());
                if rd != 1 {
                    em.emit(format!("    MOV   {}, R1  ; concat result", AsmEmitter::rn(rd)));
                }
                return;
            }
            let is_float = matches!(binop_ty, IRType::F64);
            // Float comparisons have bool result type, so check op variant directly
            let is_float_syscall = (is_float && matches!(op, IRBinOp::Add | IRBinOp::Sub | IRBinOp::Mul | IRBinOp::Div))
                || matches!(op, IRBinOp::FEq | IRBinOp::FNe | IRBinOp::FLt | IRBinOp::FGt | IRBinOp::FLe | IRBinOp::FGe);

            // For ALL float ops that use syscalls, rescue R1/R2 BEFORE
            // allocating any registers.  Use rescue_reg_inclusive so that
            // temps whose last use IS this instruction are also moved out
            // of R1/R2 — otherwise float-literal loading (which uses R1
            // via SYSCALL #219) would silently overwrite them.
            if is_float_syscall
            {
                em.rescue_reg_inclusive(1); em.rescue_reg_inclusive(2);
                let rd = em.dst_reg(dst);
                let rl = em.val_reg(lhs);
                let rr = em.val_reg(rhs);
                let (d, l, r) = (AsmEmitter::rn(rd), AsmEmitter::rn(rl), AsmEmitter::rn(rr));
                // Helper: place lhs in R1 and rhs in R2 safely even when
                // rhs was just loaded into R1 by float-literal materialization.
                macro_rules! place_args {
                    () => {
                        if rr == 1 && rl == 2 {
                            // Swap: lhs is in R2, rhs is in R1
                            em.emit("    MOV   R25, R1  ; float-args swap: save rhs".to_string());
                            em.emit("    MOV   R1, R2   ; float-args swap: lhs→R1".to_string());
                            em.emit("    MOV   R2, R25  ; float-args swap: rhs→R2".to_string());
                        } else if rr == 1 {
                            // rhs is in R1; move it to R2 first, then put lhs in R1
                            em.emit(format!("    MOV   R2, R1  ; float-args: rhs→R2 (was in R1)"));
                            if rl != 1 { em.emit(format!("    MOV   R1, {}  ; float-args: lhs→R1", l)); }
                        } else {
                            if rl != 1 { em.emit(format!("    MOV   R1, {}  ; float lhs", l)); }
                            if rr != 2 { em.emit(format!("    MOV   R2, {}  ; float rhs", r)); }
                        }
                    }
                }
                match op {
                    IRBinOp::Add | IRBinOp::Sub | IRBinOp::Mul | IRBinOp::Div => {
                        let sc = match op { IRBinOp::Add => 212, IRBinOp::Sub => 213, IRBinOp::Mul => 214, _ => 215 };
                        place_args!();
                        em.emit(format!("    SYSCALL #{}  ; float op", sc));
                        if rd != 1 { em.emit(format!("    MOV   {}, R1  ; float result", d)); }
                    }
                    IRBinOp::FEq => {
                        let tmp = em.scratch(); let one = em.scratch();
                        let (t, o) = (AsmEmitter::rn(tmp), AsmEmitter::rn(one));
                        place_args!();
                        em.emit("    SYSCALL #216  ; fcmp → +1/0/-1".to_string());
                        if rd != 1 { em.emit(format!("    MOV   {}, R1", d)); }
                        em.emit(format!("    TNEG  {}, {}", t, d));
                        em.emit(format!("    TMAX  {}, {}, {}", d, d, t));
                        em.emit(format!("    TLIT  {}, #1", o));
                        em.emit(format!("    TSUB  {}, {}, {}    ; 1-abs → 1 if feq", d, o, d));
                    }
                    IRBinOp::FNe => {
                        let tmp = em.scratch(); let t = AsmEmitter::rn(tmp);
                        place_args!();
                        em.emit("    SYSCALL #216  ; fcmp".to_string());
                        if rd != 1 { em.emit(format!("    MOV   {}, R1  ; fcmp result", d)); }
                        em.emit(format!("    TNEG  {}, {}", t, d));
                        em.emit(format!("    TMAX  {}, {}, {}    ; abs(cmp)=fne", d, d, t));
                    }
                    IRBinOp::FLt => {
                        let one = em.scratch(); let o = AsmEmitter::rn(one);
                        place_args!();
                        em.emit("    SYSCALL #216  ; fcmp → +1/0/-1".to_string());
                        if rd != 1 { em.emit(format!("    MOV   {}, R1", d)); }
                        em.emit(format!("    TNEG  {}, {}         ; +1 if l<r", d, d));
                        em.emit(format!("    TMAX  {}, {}, R0", d, d));
                        em.emit(format!("    TLIT  {}, #1", o));
                        em.emit(format!("    TMIN  {}, {}, {}", d, d, o));
                    }
                    IRBinOp::FGt => {
                        let one = em.scratch(); let o = AsmEmitter::rn(one);
                        place_args!();
                        em.emit("    SYSCALL #216  ; fcmp → +1/0/-1".to_string());
                        if rd != 1 { em.emit(format!("    MOV   {}, R1", d)); }
                        em.emit(format!("    TMAX  {}, {}, R0", d, d));
                        em.emit(format!("    TLIT  {}, #1", o));
                        em.emit(format!("    TMIN  {}, {}, {}", d, d, o));
                    }
                    IRBinOp::FLe => {
                        // fcmp → +1/0/-1; le = 1 - max(0, cmp)
                        let one = em.scratch(); let o = AsmEmitter::rn(one);
                        place_args!();
                        em.emit("    SYSCALL #216  ; fcmp → +1/0/-1".to_string());
                        if rd != 1 { em.emit(format!("    MOV   {}, R1", d)); }
                        em.emit(format!("    TMAX  {}, {}, R0    ; max(0,cmp): 1 if l>r", d, d));
                        em.emit(format!("    TLIT  {}, #1", o));
                        em.emit(format!("    TSUB  {}, {}, {}    ; 1-max=fle", d, o, d));
                    }
                    IRBinOp::FGe => {
                        // fcmp → +1/0/-1; ge = 1 - max(0, -cmp)
                        let one = em.scratch(); let o = AsmEmitter::rn(one);
                        place_args!();
                        em.emit("    SYSCALL #216  ; fcmp → +1/0/-1".to_string());
                        if rd != 1 { em.emit(format!("    MOV   {}, R1", d)); }
                        em.emit(format!("    TNEG  {}, {}         ; negate", d, d));
                        em.emit(format!("    TMAX  {}, {}, R0    ; max(0,-cmp): 1 if l<r", d, d));
                        em.emit(format!("    TLIT  {}, #1", o));
                        em.emit(format!("    TSUB  {}, {}, {}    ; 1-max=fge", d, o, d));
                    }
                    _ => unreachable!("float binop dispatch"),
                }
                return;
            }

            let rd = em.dst_reg(dst);
            let rl = em.val_reg(lhs);
            let rr = em.val_reg(rhs);
            let (d, l, r) = (AsmEmitter::rn(rd), AsmEmitter::rn(rl), AsmEmitter::rn(rr));

            match op {
                IRBinOp::Add => em.emit(format!("    TADD  {}, {}, {}", d, l, r)),
                IRBinOp::Sub => em.emit(format!("    TSUB  {}, {}, {}", d, l, r)),
                IRBinOp::Mul => em.emit(format!("    TMUL  {}, {}, {}", d, l, r)),
                IRBinOp::Div => em.emit(format!("    TDIV  {}, {}, {}", d, l, r)),
                IRBinOp::Rem => em.emit(format!("    TMOD  {}, {}, {}", d, l, r)),
                IRBinOp::And => em.emit(format!("    BAND  {}, {}, {}", d, l, r)),
                IRBinOp::Or  => em.emit(format!("    BOR   {}, {}, {}", d, l, r)),
                IRBinOp::Xor => em.emit(format!("    BXOR  {}, {}, {}", d, l, r)),

                IRBinOp::LShift => {
                    // The balanced 3-trit imm field holds -13..13; larger shift
                    // constants go through the register form (rhs is already
                    // materialized in `r` by val_reg above).
                    if let IRValue::Const(IRConst::Int(n)) = rhs {
                        if (0..=13).contains(n) {
                            em.emit(format!("    BSHL  {}, {}, #{}", d, l, n));
                        } else {
                            em.emit(format!("    BSHL  {}, {}, {}  ; shift amount > imm range", d, l, r));
                        }
                    } else {
                        em.emit(format!("    BSHL  {}, {}, {}", d, l, r));
                    }
                }
                IRBinOp::RShift => {
                    if let IRValue::Const(IRConst::Int(n)) = rhs {
                        if (0..=13).contains(n) {
                            em.emit(format!("    BSHR  {}, {}, #{}", d, l, n));
                        } else {
                            em.emit(format!("    BSHR  {}, {}, {}  ; shift amount > imm range", d, l, r));
                        }
                    } else {
                        em.emit(format!("    BSHR  {}, {}, {}", d, l, r));
                    }
                }

                // ------ Integer/bool comparisons ------
                IRBinOp::IEq => {
                    let tmp  = em.scratch(); let one  = em.scratch();
                    let (t, o) = (AsmEmitter::rn(tmp), AsmEmitter::rn(one));
                    em.emit(format!("    TCMP  {}, {}, {}    ; eq: sign(l-r)", d, l, r));
                    em.emit(format!("    TNEG  {}, {}         ; -cmp", t, d));
                    em.emit(format!("    TMAX  {}, {}, {}    ; abs(cmp)", d, d, t));
                    em.emit(format!("    TLIT  {}, #1", o));
                    em.emit(format!("    TSUB  {}, {}, {}    ; 1-abs → 1 if eq", d, o, d));
                }

                IRBinOp::StrEq => {
                    // The inline SYSCALL #200 sequence clobbers R1/R2.  Save
                    // any live temps parked there and restore them after —
                    // into the SAME registers, so the allocator mapping and
                    // cross-block phi conventions stay valid (B24).  The dst
                    // register is excluded: the restore must not clobber the
                    // result when the allocator hands out R1/R2 as the dst.
                    let save_regs: Vec<usize> = [1usize, 2].into_iter().filter(|&x| x != rd).collect();
                    let saved = em.emit_reg_save(&save_regs, "str_eq");
                    if l == "R2" && r == "R1" {
                        em.emit("    MOV   R25, R1        ; str_eq: save rhs(R1)".to_string());
                        em.emit("    MOV   R1, R2         ; str_eq: lhs(R2)→R1".to_string());
                        em.emit("    MOV   R2, R25        ; str_eq: rhs→R2".to_string());
                    } else if r == "R1" {
                        em.emit(format!("    MOV   R2, R1         ; str_eq rhs (save before clobber)"));
                        em.emit(format!("    MOV   R1, {}         ; str_eq lhs", l));
                    } else {
                        em.emit(format!("    MOV   R1, {}         ; str_eq lhs", l));
                        em.emit(format!("    MOV   R2, {}         ; str_eq rhs", r));
                    }
                    em.emit(format!("    SYSCALL #200         ; str_eq → R1"));
                    em.emit(format!("    MOV   {}, R1         ; str_eq result", d));
                    em.emit_reg_restore(&saved, "str_eq");
                }

                IRBinOp::StrNe => {
                    // Same clobber handling as StrEq (B24).
                    let save_regs: Vec<usize> = [1usize, 2].into_iter().filter(|&x| x != rd).collect();
                    let saved = em.emit_reg_save(&save_regs, "str_ne");
                    if l == "R2" && r == "R1" {
                        em.emit("    MOV   R25, R1        ; str_ne: save rhs(R1)".to_string());
                        em.emit("    MOV   R1, R2         ; str_ne: lhs(R2)→R1".to_string());
                        em.emit("    MOV   R2, R25        ; str_ne: rhs→R2".to_string());
                    } else if r == "R1" {
                        em.emit(format!("    MOV   R2, R1         ; str_ne rhs (save before clobber)"));
                        em.emit(format!("    MOV   R1, {}         ; str_ne lhs", l));
                    } else {
                        em.emit(format!("    MOV   R1, {}         ; str_ne lhs", l));
                        em.emit(format!("    MOV   R2, {}         ; str_ne rhs", r));
                    }
                    em.emit(format!("    SYSCALL #200         ; str_eq → R1 (1=eq, 0=ne)"));
                    em.emit(format!("    TLIT  R2, #1         ; R2 = 1"));
                    em.emit(format!("    TSUB  {}, R2, R1     ; d = 1-eq = ne result", d));
                    em.emit_reg_restore(&saved, "str_ne");
                }

                IRBinOp::INe => {
                    let tmp = em.scratch(); let t = AsmEmitter::rn(tmp);
                    em.emit(format!("    TCMP  {}, {}, {}    ; ne", d, l, r));
                    em.emit(format!("    TNEG  {}, {}", t, d));
                    em.emit(format!("    TMAX  {}, {}, {}    ; abs(cmp)=ne bool", d, d, t));
                }

                IRBinOp::ILt => {
                    let one = em.scratch(); let o = AsmEmitter::rn(one);
                    em.emit(format!("    TCMP  {}, {}, {}    ; lt: sign(l-r)", d, l, r));
                    em.emit(format!("    TNEG  {}, {}         ; +1 if l<r", d, d));
                    em.emit(format!("    TMAX  {}, {}, R0    ; clip neg to 0", d, d));
                    em.emit(format!("    TLIT  {}, #1", o));
                    em.emit(format!("    TMIN  {}, {}, {}    ; clamp to ≤1", d, d, o));
                }

                IRBinOp::IGt => {
                    let one = em.scratch(); let o = AsmEmitter::rn(one);
                    em.emit(format!("    TCMP  {}, {}, {}    ; gt: sign(l-r)", d, l, r));
                    em.emit(format!("    TMAX  {}, {}, R0    ; clip neg to 0", d, d));
                    em.emit(format!("    TLIT  {}, #1", o));
                    em.emit(format!("    TMIN  {}, {}, {}    ; clamp to ≤1", d, d, o));
                }

                IRBinOp::ILe => {
                    let one = em.scratch(); let o = AsmEmitter::rn(one);
                    em.emit(format!("    TCMP  {}, {}, {}    ; le: sign(l-r)", d, l, r));
                    em.emit(format!("    TMAX  {}, {}, R0    ; max(0,sign): 1 if l>r", d, d));
                    em.emit(format!("    TLIT  {}, #1", o));
                    em.emit(format!("    TSUB  {}, {}, {}    ; 1-max(0,sign)=le", d, o, d));
                }

                IRBinOp::IGe => {
                    // Use d in-place to avoid needing a second scratch register.
                    // d = cmp; negate in-place; clamp; subtract from 1.
                    let one = em.scratch();
                    let o = AsmEmitter::rn(one);
                    em.emit(format!("    TCMP  {}, {}, {}    ; ge: sign(l-r)", d, l, r));
                    em.emit(format!("    TNEG  {}, {}         ; -cmp", d, d));
                    em.emit(format!("    TMAX  {}, {}, R0    ; max(-cmp,0): 1 if l<r", d, d));
                    em.emit(format!("    TLIT  {}, #1", o));
                    em.emit(format!("    TSUB  {}, {}, {}    ; 1-max(-cmp,0)=ge", d, o, d));
                }

                // Float comparison ops are handled in the early-exit block above.
                IRBinOp::FEq | IRBinOp::FNe | IRBinOp::FLt | IRBinOp::FGt
                | IRBinOp::FLe | IRBinOp::FGe => {
                    unreachable!("float cmp should be handled in float block");
                }
            }
        }

        // ------------------------------------------------------------------ UnOp
        IRInstr::UnOp { dst, op, operand, .. } => {
            let rd = em.dst_reg(dst);
            let rs = em.val_reg(operand);
            let (d, s) = (AsmEmitter::rn(rd), AsmEmitter::rn(rs));
            match op {
                IRUnOp::Neg => em.emit(format!("    TNEG  {}, {}", d, s)),
                IRUnOp::FNeg => {
                    // Float negate: XOR sign bit (bit 63) via syscall 220
                    em.rescue_reg(1);
                    let rd2 = em.dst_reg(dst);
                    let rs2 = em.val_reg(operand);
                    let (d2, s2) = (AsmEmitter::rn(rd2), AsmEmitter::rn(rs2));
                    if rs2 != 1 { em.emit(format!("    MOV   R1, {}  ; fneg input", s2)); }
                    em.emit("    SYSCALL #220  ; fneg (flip sign bit)".to_string());
                    if rd2 != 1 { em.emit(format!("    MOV   {}, R1  ; fneg result", d2)); }
                }
                IRUnOp::Not => {
                    // Boolean NOT in 0/1 convention: NOT(b) = 1 - b
                    let one = em.scratch();
                    let o = AsmEmitter::rn(one);
                    em.emit(format!("    TLIT  {}, #1  ; bool NOT", o));
                    em.emit(format!("    TSUB  {}, {}, {}  ; 1-b", d, o, s));
                }
            }
        }

        // ------------------------------------------------------------------ Assign
        IRInstr::Assign { dst, src, .. } => {
            let rd = em.dst_reg(dst);
            let rs = em.val_reg(src);
            if rd != rs {
                em.emit(format!("    MOV   {}, {}", AsmEmitter::rn(rd), AsmEmitter::rn(rs)));
            }
        }

        // ------------------------------------------------------------------ Alloca
        IRInstr::Alloca { dst, ty } => {
            // Arrays need n words; structs need n_fields words; everything else needs 1 word.
            let words = match ty {
                IRType::Array(_, n) => *n,
                IRType::Struct(name) => em.struct_sizes.get(name).copied().unwrap_or(1),
                _ => 1,
            };
            // Update sp_depth BEFORE dst_reg so that if dest is spilled,
            // its slot depth accounts for the alloca words already pushed.
            em.reg_alloc.sp_depth += words;
            // Record this alloca slot: sp_depth after push = current sp_depth.
            // This lets Load lowering use SP-relative addressing [R26+offset].
            em.alloca_slots.insert(dst.0.clone(), em.reg_alloc.sp_depth);
            let rd = em.dst_reg(dst);
            // The balanced 3-trit imm field holds -13..13; larger allocations
            // materialize the size in a register (emit_sp_adj handles both).
            if words <= 13 {
                em.emit(format!("    TSUB  R26, R26, #{}         ; alloca {:?}", words, ty));
            } else {
                em.emit(format!("    TLIT  R21, #{}  ; alloca size", words));
                em.emit(format!("    TSUB  R26, R26, R21         ; alloca {:?}", ty));
            }
            em.emit(format!("    MOV   {}, R26", AsmEmitter::rn(rd)));
        }

        // ------------------------------------------------------------------ Store
        IRInstr::Store { ptr, val, .. } => {
            // Use SP-relative addressing for alloca slots (same reason as Load above).
            if let IRValue::Temp(t) = ptr {
                if let Some(&slot_depth) = em.alloca_slots.get(&t.0) {
                    let rv = em.val_reg(val);
                    let offset = em.reg_alloc.sp_depth as i64 - slot_depth as i64;
                    if offset >= 0 && offset <= 13 {
                        em.emit(format!("    STORE {}, [R26+#{}]  ; sp-rel store {}", AsmEmitter::rn(rv), offset, t.0));
                    } else if offset > 13 {
                        let tmp = em.scratch();
                        em.emit(format!("    TLIT  {}, #{}  ; sp-rel offset", AsmEmitter::rn(tmp), offset));
                        em.emit(format!("    TADD  {0}, R26, {0}  ; sp-rel addr", AsmEmitter::rn(tmp)));
                        em.emit(format!("    STORE {}, [{}+#0]  ; sp-rel store {}", AsmEmitter::rn(rv), AsmEmitter::rn(tmp), t.0));
                    } else {
                        let rp = em.val_reg(ptr);
                        let rv = em.val_reg(val);
                        em.emit(format!("    STORE {}, [{}+#0]", AsmEmitter::rn(rv), AsmEmitter::rn(rp)));
                    }
                    return;
                }
            }
            let rp = em.val_reg(ptr);
            let rv = em.val_reg(val);
            em.emit(format!("    STORE {}, [{}+#0]", AsmEmitter::rn(rv), AsmEmitter::rn(rp)));
        }

        // ------------------------------------------------------------------ Load
        IRInstr::Load { dst, ptr, .. } => {
            let rd = em.dst_reg(dst);
            // If the pointer is an alloca result, use SP-relative addressing.
            // This avoids register aliasing bugs where the alloca-address register
            // gets overwritten inside a loop body (e.g., by syscall arg setup or
            // scratch register reuse in the loop header's comparison).
            if let IRValue::Temp(t) = ptr {
                if let Some(&slot_depth) = em.alloca_slots.get(&t.0) {
                    let offset = em.reg_alloc.sp_depth as i64 - slot_depth as i64;
                    if offset >= 0 && offset <= 13 {
                        em.emit(format!("    LOAD  {}, [R26+#{}]  ; sp-rel load {}", AsmEmitter::rn(rd), offset, t.0));
                    } else if offset > 13 {
                        let tmp = em.scratch();
                        em.emit(format!("    TLIT  {}, #{}  ; sp-rel offset", AsmEmitter::rn(tmp), offset));
                        em.emit(format!("    TADD  {0}, R26, {0}  ; sp-rel addr", AsmEmitter::rn(tmp)));
                        em.emit(format!("    LOAD  {}, [{}+#0]  ; sp-rel load {}", AsmEmitter::rn(rd), AsmEmitter::rn(tmp), t.0));
                    } else {
                        // Negative offset should not happen for well-formed IR; fall back
                        let rp = em.val_reg(ptr);
                        em.emit(format!("    LOAD  {}, [{}+#0]", AsmEmitter::rn(rd), AsmEmitter::rn(rp)));
                    }
                    return;
                }
            }
            let rp = em.val_reg(ptr);
            em.emit(format!("    LOAD  {}, [{}+#0]", AsmEmitter::rn(rd), AsmEmitter::rn(rp)));
        }

        // ------------------------------------------------------- BoundsCheck
        // A2: verify 0 <= idx < len before a fixed-length array access.
        // Uses emit_syscall_2arg so the argument registers go through
        // rescue_reg / emit_parallel_moves — the same machinery that fixed the
        // N10 loop-carried-temp corruption — rather than clobbering R1/R2.
        IRInstr::BoundsCheck { idx, len } => {
            let args = [idx.clone(), IRValue::Const(IRConst::Int(*len as i64))];
            emit_syscall_2arg(em, &args, &None, 560, "bounds_check");
        }

        // ------------------------------------------------------------------ GetPtr
        IRInstr::GetPtr { dst, ptr, idx, .. } => {
            let rd = em.dst_reg(dst);
            let rp = em.val_reg(ptr);
            let ri = em.val_reg(idx);
            em.emit(format!("    TADD  {}, {}, {}  ; getptr", AsmEmitter::rn(rd), AsmEmitter::rn(rp), AsmEmitter::rn(ri)));
        }

        // ------------------------------------------------------------------ Call
        IRInstr::Call { dst, func, args, .. } => {
            // Map stdlib functions to SYSCALL sequences
            match func.as_str() {
                "io::println" | "io::print" => {
                    // Syscall writes R1; rescue any live temp that lives there.
                    em.rescue_reg(1);
                    if let Some(arg) = args.first() {
                        match arg {
                            IRValue::Const(IRConst::Str(lbl)) => {
                                let clean = lbl.trim_start_matches('@');
                                em.emit(format!("    TLIT  R1, #{}  ; str ptr", clean));
                            }
                            _ => {
                                let ra = em.val_reg(arg);
                                if ra != 1 { em.emit(format!("    MOV   R1, {}", AsmEmitter::rn(ra))); }
                            }
                        }
                        em.emit("    SYSCALL #3                  ; print_str".to_string());
                    }
                    if func.contains("println") {
                        em.emit("    SYSCALL #4                  ; newline".to_string());
                    }
                    if let Some(d) = dst {
                        let rd = em.dst_reg(d);
                        let _ = rd;
                    }
                }
                "io::print_int" | "io::println_int" => {
                    em.rescue_reg(1);
                    if let Some(arg) = args.first() {
                        let ra = em.val_reg(arg);
                        if ra != 1 { em.emit(format!("    MOV   R1, {}", AsmEmitter::rn(ra))); }
                    }
                    em.emit("    SYSCALL #1                  ; print_int".to_string());
                    if func.contains("println") {
                        em.emit("    SYSCALL #4                  ; newline".to_string());
                    }
                }
                "io::print_trit" | "io::println_trit" => {
                    em.rescue_reg(1);
                    if let Some(arg) = args.first() {
                        let ra = em.val_reg(arg);
                        if ra != 1 { em.emit(format!("    MOV   R1, {}", AsmEmitter::rn(ra))); }
                    }
                    em.emit("    SYSCALL #0                  ; print_trit".to_string());
                    if func.contains("println") {
                        em.emit("    SYSCALL #4                  ; newline".to_string());
                    }
                }
                "io::print_bool" | "io::println_bool" => {
                    em.rescue_reg(1);
                    if let Some(arg) = args.first() {
                        let ra = em.val_reg(arg);
                        if ra != 1 { em.emit(format!("    MOV   R1, {}", AsmEmitter::rn(ra))); }
                    }
                    em.emit("    SYSCALL #16                 ; print_bool".to_string());
                    if func.contains("println") {
                        em.emit("    SYSCALL #4                  ; newline".to_string());
                    }
                }
                "io::print_bool3" | "io::println_bool3" => {
                    // Syscall #217 prints true/false/unknown — same format as
                    // the LLVM backend's __manit_print_bool3 helper.
                    em.rescue_reg(1);
                    if let Some(arg) = args.first() {
                        let ra = em.val_reg(arg);
                        if ra != 1 { em.emit(format!("    MOV   R1, {}", AsmEmitter::rn(ra))); }
                    }
                    em.emit("    SYSCALL #217                ; print_bool3".to_string());
                    if func.contains("println") {
                        em.emit("    SYSCALL #4                  ; newline".to_string());
                    }
                }
                "io::print_float" | "io::println_float" => {
                    // Syscall #2 prints the f64 (R1 holds the bit pattern).
                    em.rescue_reg(1);
                    if let Some(arg) = args.first() {
                        let ra = em.val_reg(arg);
                        if ra != 1 { em.emit(format!("    MOV   R1, {}", AsmEmitter::rn(ra))); }
                    }
                    em.emit("    SYSCALL #2                  ; print_float".to_string());
                    if func.contains("println") {
                        em.emit("    SYSCALL #4                  ; newline".to_string());
                    }
                }
                "io::print_tryte" | "io::println_tryte" => {
                    // Trytes print as their decimal value (the LLVM backend's
                    // io_print_tryte does the same); the trit syscall would
                    // print only the sign.
                    em.rescue_reg(1);
                    if let Some(arg) = args.first() {
                        let ra = em.val_reg(arg);
                        if ra != 1 { em.emit(format!("    MOV   R1, {}", AsmEmitter::rn(ra))); }
                    }
                    em.emit("    SYSCALL #1                  ; print_int (tryte value)".to_string());
                    if func.contains("println") {
                        em.emit("    SYSCALL #4                  ; newline".to_string());
                    }
                }
                "io::newline" => {
                    em.emit("    SYSCALL #4                  ; newline".to_string());
                }
                // Result constructors: Ok(v)/Unknown(m)/Err(e)
                // Allocate 2 words on the stack: [discriminant, payload].
                // Return current SP (R26) as the pointer to the Result.
                "Ok" | "Unknown" | "Err" => {
                    let disc_val: i64 = match func.as_str() { "Ok" => 1, "Unknown" => 0, _ => -1 };
                    // Get payload value BEFORE modifying sp_depth
                    let raw_ra = if let Some(arg) = args.first() { em.val_reg(arg) } else { 0 };
                    // If payload is in R22/R23 (scratch regs we'll overwrite), move it first
                    let ra = if raw_ra == 22 || raw_ra == 23 {
                        em.emit(format!("    MOV   R21, {}  ; save payload to safe reg", AsmEmitter::rn(raw_ra)));
                        21usize
                    } else {
                        raw_ra
                    };
                    // Allocate 2 stack words
                    em.reg_alloc.sp_depth += 2;
                    em.emit(format!("    TSUB  R26, R26, #2  ; alloca Result [disc, payload] ({})", func));
                    em.emit(format!("    TLIT  R22, #{}  ; disc ({})", disc_val, func));
                    em.emit("    STORE R22, [R26+#0]  ; store discriminant".to_string());
                    em.emit(format!("    STORE {}, [R26+#1]  ; store payload", AsmEmitter::rn(ra)));
                    if let Some(d) = dst {
                        let rd = em.dst_reg(d);
                        em.emit(format!("    MOV   {}, R26  ; Result ptr ({})", AsmEmitter::rn(rd), func));
                    }
                }
                // ------------------------------------------------------------------
                // io::println_ternary / io::print_ternary — print t27 as balanced ternary string
                // SYSCALL #7 formats to string, then SYSCALL #3 prints it.
                "io::println_ternary" | "io::print_ternary" => {
                    em.rescue_reg(1);
                    if let Some(arg) = args.first() {
                        let ra = em.val_reg(arg);
                        if ra != 1 { em.emit(format!("    MOV   R1, {}", AsmEmitter::rn(ra))); }
                    }
                    em.emit("    SYSCALL #7  ; t27_to_str (format)".to_string());
                    em.emit("    SYSCALL #3  ; print_str (the ternary string)".to_string());
                    if func.contains("println") {
                        em.emit("    SYSCALL #4  ; newline".to_string());
                    }
                    if let Some(d) = dst {
                        let rd = em.dst_reg(d);
                        let _ = rd;
                    }
                }
                // ------------------------------------------------------------------
                // ternary:: trivial identity casts (trit/t27/tryte are all i64)
                "ternary::trit_to_int" | "ternary::int_to_trit" | "ternary::trit_sign"
                | "ternary::t27_to_int"  | "ternary::int_to_t27"
                | "ternary::t9_to_int"   | "ternary::int_to_t9"
                | "ternary::tryte_to_int" | "ternary::int_to_tryte" => {
                    if let Some(arg) = args.first() {
                        let ra = em.val_reg(arg);
                        if let Some(d) = dst {
                            let rd = em.dst_reg(d);
                            if rd != ra {
                                em.emit(format!("    MOV   {}, {}  ; {}", AsmEmitter::rn(rd), AsmEmitter::rn(ra), func));
                            }
                        }
                    } else if let Some(d) = dst {
                        let rd = em.dst_reg(d);
                        em.emit(format!("    TLIT  {}, #0  ; {} (no arg)", AsmEmitter::rn(rd), func));
                    }
                }
                // ternary:: unary ops
                // trit_median(a, b, c) = majority vote: median of three trit values
                // = TMIN(TMAX(a,b), TMAX(TMIN(a,b), c))
                "ternary::trit_median" => {
                    let ra = args.get(0).map(|a| em.val_reg(a)).unwrap_or(0);
                    let rb = args.get(1).map(|a| em.val_reg(a)).unwrap_or(0);
                    let rc = args.get(2).map(|a| em.val_reg(a)).unwrap_or(0);
                    // t1 = TMAX(a, b)
                    let t1 = em.scratch();
                    em.emit(format!("    TMAX  {}, {}, {}  ; median: max(a,b)", AsmEmitter::rn(t1), AsmEmitter::rn(ra), AsmEmitter::rn(rb)));
                    // t2 = TMIN(a, b)
                    let t2 = em.scratch();
                    em.emit(format!("    TMIN  {}, {}, {}  ; median: min(a,b)", AsmEmitter::rn(t2), AsmEmitter::rn(ra), AsmEmitter::rn(rb)));
                    // t3 = TMAX(t2, c)
                    let t3 = em.scratch();
                    em.emit(format!("    TMAX  {}, {}, {}  ; median: max(min(a,b),c)", AsmEmitter::rn(t3), AsmEmitter::rn(t2), AsmEmitter::rn(rc)));
                    if let Some(d) = dst {
                        let rd = em.dst_reg(d);
                        em.emit(format!("    TMIN  {}, {}, {}  ; median = min(max(a,b), max(min(a,b),c))", AsmEmitter::rn(rd), AsmEmitter::rn(t1), AsmEmitter::rn(t3)));
                    }
                }
                "ternary::trit_neg" | "ternary::t27_neg" => {
                    let ra = args.first().map(|a| em.val_reg(a)).unwrap_or(0);
                    if let Some(d) = dst {
                        let rd = em.dst_reg(d);
                        em.emit(format!("    TNEG  {}, {}  ; {}", AsmEmitter::rn(rd), AsmEmitter::rn(ra), func));
                    }
                }
                // ternary:: binary ops
                "ternary::trit_and" | "ternary::t27_and" => {
                    let ra = args.get(0).map(|a| em.val_reg(a)).unwrap_or(0);
                    let rb = args.get(1).map(|a| em.val_reg(a)).unwrap_or(0);
                    if let Some(d) = dst {
                        let rd = em.dst_reg(d);
                        em.emit(format!("    TAND  {}, {}, {}  ; {}", AsmEmitter::rn(rd), AsmEmitter::rn(ra), AsmEmitter::rn(rb), func));
                    }
                }
                "ternary::trit_or" | "ternary::t27_or" => {
                    let ra = args.get(0).map(|a| em.val_reg(a)).unwrap_or(0);
                    let rb = args.get(1).map(|a| em.val_reg(a)).unwrap_or(0);
                    if let Some(d) = dst {
                        let rd = em.dst_reg(d);
                        em.emit(format!("    TOR   {}, {}, {}  ; {}", AsmEmitter::rn(rd), AsmEmitter::rn(ra), AsmEmitter::rn(rb), func));
                    }
                }
                // ternary:: shift ops
                // ternary::t27_shift_left(n, k) — multiply by 3^k (syscall #201)
                // Parallel-move arg setup (emit_syscall_2arg_ret) is required:
                // a constant k materialized into scratch R1 would otherwise be
                // clobbered by the `MOV R1, n` sequence.
                "ternary::t27_shift_left" => {
                    emit_syscall_2arg_ret(em, args, dst, 201, "t27_shift_left");
                }
                // ternary::t27_shift_right(n, k) — divide by 3^k (syscall #202)
                "ternary::t27_shift_right" => {
                    emit_syscall_2arg_ret(em, args, dst, 202, "t27_shift_right");
                }
                "ternary::trit_shift_left" | "ternary::trit_rotate_left" => {
                    let ra = args.get(0).map(|a| em.val_reg(a)).unwrap_or(0);
                    if let Some(d) = dst {
                        let rd = em.dst_reg(d);
                        // Shift constants outside the balanced 3-trit imm range
                        // (-13..13) fall back to the register form.
                        if let Some(IRValue::Const(IRConst::Int(n))) = args.get(1) {
                            if (0..=13).contains(n) {
                                em.emit(format!("    TSHI  {}, {}, #{}  ; {}", AsmEmitter::rn(rd), AsmEmitter::rn(ra), n, func));
                                return;
                            }
                        }
                        let rb = args.get(1).map(|a| em.val_reg(a)).unwrap_or(0);
                        em.emit(format!("    TSHI  {}, {}, {}  ; {} (dyn)", AsmEmitter::rn(rd), AsmEmitter::rn(ra), AsmEmitter::rn(rb), func));
                    }
                }
                "ternary::trit_shift_right" | "ternary::trit_rotate_right" => {
                    let ra = args.get(0).map(|a| em.val_reg(a)).unwrap_or(0);
                    if let Some(d) = dst {
                        let rd = em.dst_reg(d);
                        if let Some(IRValue::Const(IRConst::Int(n))) = args.get(1) {
                            if (0..=13).contains(n) {
                                em.emit(format!("    TSHR  {}, {}, #{}  ; {}", AsmEmitter::rn(rd), AsmEmitter::rn(ra), n, func));
                                return;
                            }
                        }
                        let rb = args.get(1).map(|a| em.val_reg(a)).unwrap_or(0);
                        em.emit(format!("    TSHR  {}, {}, {}  ; {} (dyn)", AsmEmitter::rn(rd), AsmEmitter::rn(ra), AsmEmitter::rn(rb), func));
                    }
                }
                // ternary::tryte_from_trits(t2, t1, t0) -> tryte = t2*9 + t1*3 + t0
                "ternary::tryte_from_trits" => {
                    let ra = args.get(0).map(|a| em.val_reg(a)).unwrap_or(0);
                    let rb = args.get(1).map(|a| em.val_reg(a)).unwrap_or(0);
                    let rc = args.get(2).map(|a| em.val_reg(a)).unwrap_or(0);
                    if let Some(d) = dst {
                        let rd = em.dst_reg(d);
                        let tmp = em.scratch();
                        em.emit(format!("    TLIT  {}, #9", AsmEmitter::rn(tmp)));
                        em.emit(format!("    TMUL  {}, {}, {}  ; t2*9", AsmEmitter::rn(rd), AsmEmitter::rn(ra), AsmEmitter::rn(tmp)));
                        em.emit(format!("    TLIT  {}, #3", AsmEmitter::rn(tmp)));
                        em.emit(format!("    TMUL  {}, {}, {}  ; t1*3", AsmEmitter::rn(tmp), AsmEmitter::rn(rb), AsmEmitter::rn(tmp)));
                        em.emit(format!("    TADD  {}, {}, {}  ; t2*9+t1*3", AsmEmitter::rn(rd), AsmEmitter::rn(rd), AsmEmitter::rn(tmp)));
                        em.emit(format!("    TADD  {}, {}, {}  ; +t0", AsmEmitter::rn(rd), AsmEmitter::rn(rd), AsmEmitter::rn(rc)));
                    }
                }
                // ternary::t27_to_str — syscall #7: R1=value, returns str ptr in R1
                "ternary::t27_to_str" | "ternary::t27_to_str_padded" | "ternary::t27_explain" => {
                    em.rescue_reg(1);
                    if let Some(arg) = args.first() {
                        let ra = em.val_reg(arg);
                        if ra != 1 { em.emit(format!("    MOV   R1, {}", AsmEmitter::rn(ra))); }
                    }
                    em.emit("    SYSCALL #7  ; t27_to_str".to_string());
                    if let Some(d) = dst {
                        let rd = em.dst_reg(d);
                        if rd != 1 { em.emit(format!("    MOV   {}, R1  ; t27_to_str result", AsmEmitter::rn(rd))); }
                    }
                }
                // ternary::trits_to_str — syscall #8: R1=length-prefixed array ptr → R1=str ptr
                // Uses only R1 (no R2/R3 clobber so loop counters are safe).
                "ternary::trits_to_str" => {
                    em.rescue_reg(1);
                    if let Some(arg) = args.first() {
                        let ra = em.val_reg(arg);
                        if ra != 1 { em.emit(format!("    MOV   R1, {}", AsmEmitter::rn(ra))); }
                    }
                    em.emit("    SYSCALL #8  ; trits_to_str".to_string());
                    if let Some(d) = dst {
                        let rd = em.dst_reg(d);
                        if rd != 1 { em.emit(format!("    MOV   {}, R1  ; trits_to_str result", AsmEmitter::rn(rd))); }
                    }
                }
                // math:: functions
                // math::trit_count(n) — number of balanced-ternary digits in n (syscall #9)
                "math::trit_count" => {
                    em.rescue_reg(1);
                    if let Some(arg) = args.first() {
                        let ra = em.val_reg(arg);
                        if ra != 1 { em.emit(format!("    MOV   R1, {}", AsmEmitter::rn(ra))); }
                    }
                    em.emit("    SYSCALL #9  ; trit_count".to_string());
                    if let Some(d) = dst {
                        let rd = em.dst_reg(d);
                        if rd != 1 { em.emit(format!("    MOV   {}, R1  ; trit_count result", AsmEmitter::rn(rd))); }
                    }
                }
                // math::to_balanced_ternary(n) — convert int to [trit] array (syscall #10)
                "math::to_balanced_ternary" => {
                    em.rescue_reg(1);
                    if let Some(arg) = args.first() {
                        let ra = em.val_reg(arg);
                        if ra != 1 { em.emit(format!("    MOV   R1, {}", AsmEmitter::rn(ra))); }
                    }
                    em.emit("    SYSCALL #10  ; to_balanced_ternary".to_string());
                    if let Some(d) = dst {
                        let rd = em.dst_reg(d);
                        if rd != 1 { em.emit(format!("    MOV   {}, R1  ; to_bal_tern result", AsmEmitter::rn(rd))); }
                    }
                }
                // math::from_balanced_ternary([trit]) — convert [trit] to int (syscall #11)
                // R1 = length-prefixed array ptr; no R2 clobber.
                "math::from_balanced_ternary" => {
                    em.rescue_reg(1);
                    if let Some(arg) = args.first() {
                        let ra = em.val_reg(arg);
                        if ra != 1 { em.emit(format!("    MOV   R1, {}", AsmEmitter::rn(ra))); }
                    }
                    em.emit("    SYSCALL #11  ; from_balanced_ternary".to_string());
                    if let Some(d) = dst {
                        let rd = em.dst_reg(d);
                        if rd != 1 { em.emit(format!("    MOV   {}, R1  ; from_bal_tern result", AsmEmitter::rn(rd))); }
                    }
                }
                // ternary::pack_trits — R1=length-prefixed array ptr → R1=t27
                "ternary::pack_trits" => {
                    em.rescue_reg(1);
                    if let Some(arg) = args.first() {
                        let ra = em.val_reg(arg);
                        if ra != 1 { em.emit(format!("    MOV   R1, {}", AsmEmitter::rn(ra))); }
                    }
                    em.emit("    SYSCALL #12  ; pack_trits".to_string());
                    if let Some(d) = dst {
                        let rd = em.dst_reg(d);
                        if rd != 1 { em.emit(format!("    MOV   {}, R1  ; pack_trits result", AsmEmitter::rn(rd))); }
                    }
                }
                // ternary::unpack_trits — R1=t27 → R1=ptr to length-prefixed [trit;27] array
                "ternary::unpack_trits" => {
                    em.rescue_reg(1);
                    if let Some(arg) = args.first() {
                        let ra = em.val_reg(arg);
                        if ra != 1 { em.emit(format!("    MOV   R1, {}", AsmEmitter::rn(ra))); }
                    }
                    em.emit("    SYSCALL #13  ; unpack_trits".to_string());
                    if let Some(d) = dst {
                        let rd = em.dst_reg(d);
                        if rd != 1 { em.emit(format!("    MOV   {}, R1  ; unpack_trits result", AsmEmitter::rn(rd))); }
                    }
                }
                // fmt:: string formatting functions
                // fmt::show_int(n) — format int as string, returns str ptr (syscall #14)
                "fmt::show_int" | "fmt::int_to_str" => {
                    em.rescue_reg(1);
                    if let Some(arg) = args.first() {
                        let ra = em.val_reg(arg);
                        if ra != 1 { em.emit(format!("    MOV   R1, {}", AsmEmitter::rn(ra))); }
                    }
                    em.emit("    SYSCALL #14  ; fmt_show_int".to_string());
                    if let Some(d) = dst {
                        let rd = em.dst_reg(d);
                        if rd != 1 { em.emit(format!("    MOV   {}, R1  ; fmt_show_int result", AsmEmitter::rn(rd))); }
                    }
                }
                // fmt::align_right(s, width, fill) — right-align string (syscall #15)
                // R1=str_ptr, R21=width, R22=fill_char → R1=result str ptr
                // Uses R21/R22 (dedicated scratch) instead of R2/R3 to avoid clobbering
                // general-purpose temps that might be live loop counters.
                "fmt::align_right" | "fmt::pad_left" => {
                    em.rescue_reg(1);
                    // Parallel moves: a later arg materialized into scratch R1
                    // must not be clobbered by the R1 target move.
                    let targets = [1usize, 21, 22];
                    let moves: Vec<(usize, usize)> = args.iter().take(3).enumerate()
                        .map(|(i, a)| (targets[i], em.val_reg(a)))
                        .collect();
                    emit_parallel_moves(em, moves, "fmt_align_right");
                    em.emit("    SYSCALL #15  ; fmt_align_right".to_string());
                    if let Some(d) = dst {
                        let rd = em.dst_reg(d);
                        if rd != 1 { em.emit(format!("    MOV   {}, R1  ; fmt_align_right result", AsmEmitter::rn(rd))); }
                    }
                }
                // fmt::align_left(s, width, fill) — left-align string (syscall #132)
                "fmt::align_left" | "fmt::pad_right" => {
                    em.rescue_reg(1);
                    let targets = [1usize, 21, 22];
                    let moves: Vec<(usize, usize)> = args.iter().take(3).enumerate()
                        .map(|(i, a)| (targets[i], em.val_reg(a)))
                        .collect();
                    emit_parallel_moves(em, moves, "fmt_align_left");
                    em.emit("    SYSCALL #132  ; fmt_align_left".to_string());
                    if let Some(d) = dst {
                        let rd = em.dst_reg(d);
                        if rd != 1 { em.emit(format!("    MOV   {}, R1  ; fmt_align_left result", AsmEmitter::rn(rd))); }
                    }
                }
                // ------------------------------------------------------------------
                // Collection / String / Channel / FS / Async syscall dispatch
                // ------------------------------------------------------------------
                "Vec::new" => {
                    em.rescue_reg(1);
                    em.emit("    SYSCALL #17  ; Vec::new".to_string());
                    if let Some(d) = dst { let rd = em.dst_reg(d); if rd != 1 { em.emit(format!("    MOV   {}, R1", AsmEmitter::rn(rd))); } }
                }
                "Vec::push" => {
                    emit_syscall_2arg(em, args, dst, 18, "Vec::push");
                }
                "Vec::pop" => {
                    emit_syscall_1arg_ret(em, args, dst, 19, "Vec::pop");
                }
                "Vec::get" => {
                    emit_syscall_2arg_ret(em, args, dst, 20, "Vec::get");
                }
                "Vec::set" => {
                    emit_syscall_3arg(em, args, dst, 21, "Vec::set");
                }
                "Vec::len" => {
                    emit_syscall_1arg_ret(em, args, dst, 22, "Vec::len");
                }
                "Vec::is_empty" => {
                    emit_syscall_1arg_ret(em, args, dst, 23, "Vec::is_empty");
                }
                "Vec::clear" => {
                    emit_syscall_1arg(em, args, dst, 24, "Vec::clear");
                }
                "Vec::contains" => {
                    emit_syscall_2arg_ret(em, args, dst, 25, "Vec::contains");
                }
                "Map::new" => {
                    em.rescue_reg(1);
                    em.emit("    SYSCALL #30  ; Map::new".to_string());
                    if let Some(d) = dst { let rd = em.dst_reg(d); if rd != 1 { em.emit(format!("    MOV   {}, R1", AsmEmitter::rn(rd))); } }
                }
                "Map::insert" => {
                    emit_syscall_3arg(em, args, dst, 31, "Map::insert");
                }
                "Map::get" => {
                    emit_syscall_2arg_ret(em, args, dst, 32, "Map::get");
                }
                "Map::contains_key" => {
                    emit_syscall_2arg_ret(em, args, dst, 33, "Map::contains_key");
                }
                "Map::remove" => {
                    emit_syscall_2arg(em, args, dst, 34, "Map::remove");
                }
                "Map::len" => {
                    emit_syscall_1arg_ret(em, args, dst, 35, "Map::len");
                }
                "Map::is_empty" => {
                    emit_syscall_1arg_ret(em, args, dst, 36, "Map::is_empty");
                }
                "Set::new" => {
                    em.rescue_reg(1);
                    em.emit("    SYSCALL #40  ; Set::new".to_string());
                    if let Some(d) = dst { let rd = em.dst_reg(d); if rd != 1 { em.emit(format!("    MOV   {}, R1", AsmEmitter::rn(rd))); } }
                }
                "Set::insert" => {
                    emit_syscall_2arg(em, args, dst, 41, "Set::insert");
                }
                "Set::contains" => {
                    emit_syscall_2arg_ret(em, args, dst, 42, "Set::contains");
                }
                "Set::remove" => {
                    emit_syscall_2arg(em, args, dst, 43, "Set::remove");
                }
                "Set::len" => {
                    emit_syscall_1arg_ret(em, args, dst, 44, "Set::len");
                }
                "Deque::new" => {
                    em.rescue_reg(1);
                    em.emit("    SYSCALL #50  ; Deque::new".to_string());
                    if let Some(d) = dst { let rd = em.dst_reg(d); if rd != 1 { em.emit(format!("    MOV   {}, R1", AsmEmitter::rn(rd))); } }
                }
                "Deque::push_front" => {
                    emit_syscall_2arg(em, args, dst, 51, "Deque::push_front");
                }
                "Deque::push_back" => {
                    emit_syscall_2arg(em, args, dst, 52, "Deque::push_back");
                }
                "Deque::pop_front" => {
                    emit_syscall_1arg_ret(em, args, dst, 53, "Deque::pop_front");
                }
                "Deque::pop_back" => {
                    emit_syscall_1arg_ret(em, args, dst, 54, "Deque::pop_back");
                }
                "Deque::len" => {
                    emit_syscall_1arg_ret(em, args, dst, 55, "Deque::len");
                }
                "Deque::front" => {
                    emit_syscall_1arg_ret(em, args, dst, 56, "Deque::front");
                }
                "Deque::back" => {
                    emit_syscall_1arg_ret(em, args, dst, 57, "Deque::back");
                }
                "Deque::is_empty" => {
                    emit_syscall_1arg_ret(em, args, dst, 58, "Deque::is_empty");
                }
                "Deque::contains" => {
                    emit_syscall_2arg_ret(em, args, dst, 59, "Deque::contains");
                }
                "str_len" | "str::len" => {
                    emit_syscall_1arg_ret(em, args, dst, 60, "str_len");
                }
                "str_concat" | "str::concat" => {
                    emit_syscall_2arg_ret(em, args, dst, 61, "str_concat");
                }
                "str_slice" | "str::slice" => {
                    emit_syscall_3arg_ret(em, args, dst, 62, "str_slice");
                }
                "str_contains" | "str::contains" => {
                    emit_syscall_2arg_ret(em, args, dst, 63, "str_contains");
                }
                "str_find" | "str::find" => {
                    emit_syscall_2arg_ret(em, args, dst, 64, "str_find");
                }
                "str_to_int" | "str::to_int" => {
                    emit_syscall_1arg_ret(em, args, dst, 65, "str_to_int");
                }
                "int_to_str" | "int::to_str" => {
                    emit_syscall_1arg_ret(em, args, dst, 66, "int_to_str");
                }
                "str_split" | "str::split" => {
                    emit_syscall_2arg_ret(em, args, dst, 67, "str_split");
                }
                "str_trim" | "str::trim" => {
                    emit_syscall_1arg_ret(em, args, dst, 68, "str_trim");
                }
                "str_replace" | "str::replace" => {
                    emit_syscall_3arg_ret(em, args, dst, 69, "str_replace");
                }
                "channel" | "channel_new" => {
                    em.rescue_reg(1);
                    em.emit("    SYSCALL #70  ; channel_new".to_string());
                    if let Some(d) = dst { let rd = em.dst_reg(d); if rd != 1 { em.emit(format!("    MOV   {}, R1", AsmEmitter::rn(rd))); } }
                }
                "channel_send" | "Channel::send" => {
                    emit_syscall_2arg(em, args, dst, 71, "channel_send");
                }
                "channel_recv" | "Channel::recv" => {
                    emit_syscall_1arg_ret(em, args, dst, 72, "channel_recv");
                }
                "channel_len" | "Channel::len" => {
                    emit_syscall_1arg_ret(em, args, dst, 73, "channel_len");
                }
                "channel_close" | "Channel::close" => {
                    emit_syscall_1arg(em, args, dst, 74, "channel_close");
                }
                "Channel::try_recv" | "channel_try_recv" => {
                    emit_syscall_1arg_ret(em, args, dst, 108, "channel_try_recv");
                }
                // Mutex
                "Mutex::new" | "mutex_new" => {
                    emit_syscall_1arg_ret(em, args, dst, 109, "mutex_new");
                }
                "Mutex::lock" | "MutexGuard::lock" | "mutex_lock" => {
                    emit_syscall_1arg_ret(em, args, dst, 110, "mutex_lock");
                }
                "Mutex::get" | "MutexGuard::get" | "mutex_get" => {
                    emit_syscall_1arg_ret(em, args, dst, 111, "mutex_get");
                }
                "Mutex::set" | "MutexGuard::set" | "mutex_set" => {
                    emit_syscall_2arg(em, args, dst, 131, "mutex_set");
                }
                "Mutex::update" | "MutexGuard::update" | "mutex_update" => {
                    // mutex_update(guard_handle=R1, fn_ptr=R2): reads current value,
                    // calls fn(value), stores result back. Uses syscall 112 which
                    // handles the fn-call internally via call_fn_ptr.
                    emit_syscall_2arg(em, args, dst, 112, "mutex_update");
                }
                "Mutex::unlock" | "MutexGuard::unlock" | "mutex_unlock" => {
                    emit_syscall_1arg(em, args, dst, 113, "mutex_unlock");
                }
                // AtomicTrit
                "AtomicTrit::new" => {
                    emit_syscall_1arg_ret(em, args, dst, 114, "atomic_trit_new");
                }
                "AtomicTrit::load" | "AtomicTrit::get" => {
                    emit_syscall_1arg_ret(em, args, dst, 115, "atomic_trit_load");
                }
                "AtomicTrit::store" | "AtomicTrit::set" => {
                    emit_syscall_2arg(em, args, dst, 116, "atomic_trit_store");
                }
                // Barrier
                "Barrier::new" => {
                    emit_syscall_1arg_ret(em, args, dst, 117, "barrier_new");
                }
                "Barrier::wait" => {
                    emit_syscall_1arg_ret(em, args, dst, 118, "barrier_wait");
                }
                // Semaphore
                "Semaphore::new" => {
                    emit_syscall_1arg_ret(em, args, dst, 119, "semaphore_new");
                }
                "Semaphore::acquire" => {
                    emit_syscall_1arg(em, args, dst, 120, "semaphore_acquire");
                }
                "Semaphore::release" => {
                    emit_syscall_1arg(em, args, dst, 121, "semaphore_release");
                }
                // Task join
                "Task::join" => {
                    emit_syscall_1arg_ret(em, args, dst, 122, "task_join");
                }
                // async builtins
                "async::yield_now" | "async::yield_" | "yield_" => {
                    em.emit("    SYSCALL #81  ; yield_now (no-op)".to_string());
                }
                "async::sleep" | "time::sleep" | "async_sleep" => {
                    emit_syscall_1arg(em, args, dst, 123, "async_sleep");
                }
                "async::spawn_task" | "spawn_task" => {
                    emit_syscall_1arg_ret(em, args, dst, 124, "async_spawn_task");
                }
                "async::select" => {
                    emit_syscall_1arg_ret(em, args, dst, 125, "async_select");
                }
                // select result .block_on() — also handles Unknown::block_on
                "SelectResult::block_on" | "::block_on" | "Unknown::block_on" => {
                    emit_syscall_1arg_ret(em, args, dst, 126, "select_block_on");
                }
                // fmt builtins
                "fmt::format" => {
                    emit_syscall_2arg_ret(em, args, dst, 127, "fmt_format");
                }
                "fmt_show_int" => {
                    emit_syscall_1arg_ret(em, args, dst, 128, "fmt_show_int");
                }
                "fmt::show_float" | "fmt_show_float" => {
                    emit_syscall_1arg_ret(em, args, dst, 129, "fmt_show_float");
                }
                "fmt::show_bool" | "fmt_show_bool" => {
                    emit_syscall_1arg_ret(em, args, dst, 130, "fmt_show_bool");
                }
                "fs::open" => {
                    emit_syscall_2arg_ret(em, args, dst, 75, "fs::open");
                }
                "fs::read" => {
                    emit_syscall_2arg_ret(em, args, dst, 76, "fs::read");
                }
                "fs::write" => {
                    emit_syscall_2arg_ret(em, args, dst, 77, "fs::write");
                }
                "fs::close" => {
                    emit_syscall_1arg(em, args, dst, 78, "fs::close");
                }
                "fs::exists" => {
                    emit_syscall_1arg_ret(em, args, dst, 79, "fs::exists");
                }
                "async::spawn" | "spawn" => {
                    emit_syscall_2arg_ret(em, args, dst, 80, "spawn");
                }
                "task_exit" => {
                    em.emit("    SYSCALL #82  ; task_exit".to_string());
                }
                // Vec sort / reverse
                "Vec::remove" => {
                    emit_syscall_2arg(em, args, dst, 26, "Vec::remove");
                }
                "Vec::sort" => {
                    emit_syscall_1arg(em, args, dst, 100, "Vec::sort");
                }
                "Vec::reverse" => {
                    emit_syscall_1arg(em, args, dst, 101, "Vec::reverse");
                }
                "Vec::index_of" => {
                    emit_syscall_2arg_ret(em, args, dst, 102, "Vec::index_of");
                }
                "Vec::fold" => {
                    emit_syscall_3arg_ret(em, args, dst, 103, "Vec::fold");
                }
                // Vec higher-order + slice
                "Vec::for_each" => {
                    emit_syscall_2arg(em, args, dst, 83, "Vec::for_each");
                }
                "Vec::map" => {
                    emit_syscall_2arg_ret(em, args, dst, 84, "Vec::map");
                }
                "Vec::filter" => {
                    emit_syscall_2arg_ret(em, args, dst, 85, "Vec::filter");
                }
                "Vec::slice" => {
                    emit_syscall_3arg_ret(em, args, dst, 86, "Vec::slice");
                }
                // Map extras
                "Map::get_or" => {
                    emit_syscall_3arg_ret(em, args, dst, 87, "Map::get_or");
                }
                "Map::keys" => {
                    emit_syscall_1arg_ret(em, args, dst, 88, "Map::keys");
                }
                // Set set-algebra + for_each
                "Set::intersection" => {
                    emit_syscall_2arg_ret(em, args, dst, 89, "Set::intersection");
                }
                "Set::union" => {
                    emit_syscall_2arg_ret(em, args, dst, 90, "Set::union");
                }
                "Set::difference" => {
                    emit_syscall_2arg_ret(em, args, dst, 91, "Set::difference");
                }
                "Set::for_each" => {
                    emit_syscall_2arg(em, args, dst, 92, "Set::for_each");
                }
                "Set::is_subset" => {
                    emit_syscall_2arg_ret(em, args, dst, 104, "Set::is_subset");
                }
                "Set::is_superset" => {
                    emit_syscall_2arg_ret(em, args, dst, 105, "Set::is_superset");
                }
                "Set::is_disjoint" => {
                    emit_syscall_2arg_ret(em, args, dst, 106, "Set::is_disjoint");
                }
                // TernaryTrie
                "TernaryTrie::new" => {
                    em.rescue_reg(1);
                    em.emit("    SYSCALL #93  ; TernaryTrie::new".to_string());
                    if let Some(d) = dst { let rd = em.dst_reg(d); if rd != 1 { em.emit(format!("    MOV   {}, R1", AsmEmitter::rn(rd))); } }
                }
                "TernaryTrie::insert" => {
                    emit_syscall_3arg(em, args, dst, 94, "TernaryTrie::insert");
                }
                "TernaryTrie::get" => {
                    emit_syscall_2arg_ret(em, args, dst, 95, "TernaryTrie::get");
                }
                "TernaryTrie::len" => {
                    emit_syscall_1arg_ret(em, args, dst, 96, "TernaryTrie::len");
                }
                "TernaryTrie::keys" => {
                    emit_syscall_1arg_ret(em, args, dst, 97, "TernaryTrie::keys");
                }
                "TernaryTrie::contains_key" | "TernaryTrie::contains" => {
                    emit_syscall_2arg_ret(em, args, dst, 98, "TernaryTrie::contains_key");
                }
                "TernaryTrie::for_each" => {
                    emit_syscall_2arg(em, args, dst, 99, "TernaryTrie::for_each");
                }
                "TernaryTrie::keys_with_prefix" => {
                    emit_syscall_2arg_ret(em, args, dst, 107, "TernaryTrie::keys_with_prefix");
                }
                // ------------------------------------------------------------------
                // Extended fs syscalls (500-509): real std::fs file I/O
                // ------------------------------------------------------------------
                "fs::open_file" | "fs::open2" => {
                    emit_syscall_2arg_ret(em, args, dst, 500, "fs::open_file");
                }
                "fs::read_bytes" => {
                    emit_syscall_3arg_ret(em, args, dst, 501, "fs::read_bytes");
                }
                "fs::write_bytes" => {
                    emit_syscall_3arg_ret(em, args, dst, 502, "fs::write_bytes");
                }
                "fs::close_file" => {
                    emit_syscall_1arg(em, args, dst, 503, "fs::close_file");
                }
                "fs::exists2" => {
                    emit_syscall_1arg_ret(em, args, dst, 504, "fs::exists2");
                }
                "fs::read_file" => {
                    emit_syscall_1arg_ret(em, args, dst, 505, "fs::read_file");
                }
                "fs::write_file" => {
                    emit_syscall_2arg_ret(em, args, dst, 506, "fs::write_file");
                }
                "fs::append_file" => {
                    emit_syscall_2arg_ret(em, args, dst, 507, "fs::append_file");
                }
                "fs::delete" | "fs::remove" => {
                    emit_syscall_1arg_ret(em, args, dst, 508, "fs::delete");
                }
                "fs::list_dir" => {
                    emit_syscall_1arg_ret(em, args, dst, 509, "fs::list_dir");
                }
                // ------------------------------------------------------------------
                // TCP networking syscalls (520-525)
                // ------------------------------------------------------------------
                "net::tcp_connect" => {
                    emit_syscall_2arg_ret(em, args, dst, 520, "net::tcp_connect");
                }
                "net::tcp_listen" => {
                    emit_syscall_1arg_ret(em, args, dst, 521, "net::tcp_listen");
                }
                "net::tcp_accept" => {
                    emit_syscall_1arg_ret(em, args, dst, 522, "net::tcp_accept");
                }
                "net::send" => {
                    emit_syscall_3arg_ret(em, args, dst, 523, "net::send");
                }
                "net::recv" => {
                    emit_syscall_3arg_ret(em, args, dst, 524, "net::recv");
                }
                "net::close" => {
                    emit_syscall_1arg(em, args, dst, 525, "net::close");
                }
                // ------------------------------------------------------------------
                // Time syscall (540)
                // ------------------------------------------------------------------
                "time::now" => {
                    em.rescue_reg(1);
                    em.emit("    SYSCALL #540  ; time::now".to_string());
                    if let Some(d) = dst {
                        let rd = em.dst_reg(d);
                        if rd != 1 { em.emit(format!("    MOV   {}, R1  ; time::now result", AsmEmitter::rn(rd))); }
                    }
                }
                // ------------------------------------------------------------------
                // Env syscalls (550-551)
                // ------------------------------------------------------------------
                "env::exit" => {
                    emit_syscall_1arg(em, args, dst, 550, "env::exit");
                }
                "env::args" => {
                    em.rescue_reg(1);
                    em.emit("    SYSCALL #551  ; env::args".to_string());
                    if let Some(d) = dst {
                        let rd = em.dst_reg(d);
                        if rd != 1 { em.emit(format!("    MOV   {}, R1  ; env::args result", AsmEmitter::rn(rd))); }
                    }
                }
                _ => {
                    // General call — full caller-save convention.
                    //
                    // The T3ISA has no callee-save registers: the callee may freely use
                    // R1-R20.  We therefore save every live temp in R1-R20 to the stack
                    // before the call and restore them afterward.
                    //
                    // Saving convention: for each saved temp, emit
                    //   TSUB R26, R26, #1
                    //   STORE Rk, [R26+#0]
                    // and record spill_slots[name] = new sp_depth.
                    // We do NOT add the register back to free_regs, so dst_reg cannot
                    // steal it before the restore.
                    //
                    // After the call, reload each saved temp back into its original
                    // register, then pop all saved slots.

                    let cur_instr = em.reg_alloc.current_instr;

                    // Pre-load all args via val_reg BEFORE the caller-saves loop.
                    // val_reg emits spill-reload LOADs directly to self.lines (not cur_instr),
                    // so they must be emitted while sp_depth still reflects the pre-save state.
                    // After flush_instr, lines order is: [arg LOADs] [caller-saves + CALL].
                    let rargs: Vec<usize> = args.iter().map(|a| em.val_reg(a)).collect();

                    // Collect all live temps in the general-purpose pool (R1-R20) that
                    // survive past this instruction.
                    let to_save: Vec<(String, usize)> = {
                        em.reg_alloc.temp_to_reg
                            .iter()
                            .filter(|(name, &reg)| {
                                reg >= 1 && reg <= 20
                                    && em.reg_alloc.last_use
                                        .get(*name)
                                        .map_or(false, |&lu| lu > cur_instr)
                            })
                            .map(|(n, &r)| (n.clone(), r))
                            .collect()
                    };

                    // Save each to the stack (sorted for determinism).
                    let mut to_save_sorted = to_save.clone();
                    to_save_sorted.sort_by_key(|(n, _)| n.clone());

                    for (name, reg) in &to_save_sorted {
                        let slot = em.reg_alloc.sp_depth + 1;
                        em.reg_alloc.spill_slots.insert(name.clone(), slot);
                        em.reg_alloc.temp_to_reg.remove(name);
                        em.reg_alloc.spill_count += 1;
                        em.emit(format!("    TSUB  R26, R26, #1  ; caller-save {}", name));
                        em.emit(format!("    STORE R{}, [R26+#0]  ; caller-save {}", reg, name));
                        em.reg_alloc.sp_depth += 1;
                    }

                    // Move pre-loaded arg values into argument registers.
                    // Parallel-move sequentialization: emit move (tgt←src) only when
                    // `tgt` is not used as a source by any other pending move — i.e.,
                    // overwriting tgt won't destroy a value still needed elsewhere.
                    // If all pending moves form cycles, break one cycle via R25.
                    {
                        let mut pending: Vec<(usize, usize)> = rargs.iter().enumerate()
                            .filter(|(i, &ra)| ra != i + 1)
                            .map(|(i, &ra)| (i + 1, ra))
                            .collect();

                        const SCRATCH: usize = 25;

                        while !pending.is_empty() {
                            let sources: std::collections::HashSet<usize> =
                                pending.iter().map(|&(_, s)| s).collect();
                            // A move is ready when its TARGET is not used as a source by
                            // any remaining pending move (so overwriting it is safe).
                            if let Some(idx) = pending.iter().position(|&(tgt, _)| !sources.contains(&tgt)) {
                                let (tgt, src) = pending.remove(idx);
                                em.emit(format!("    MOV   R{}, {}  ; arg {}", tgt, AsmEmitter::rn(src), tgt - 1));
                            } else {
                                // Cycle: save tgt0 to scratch, emit tgt0←src0 immediately,
                                // then replace any remaining use of tgt0 as a source with SCRATCH.
                                let (tgt0, src0) = pending.remove(0);
                                em.emit(format!("    MOV   R{}, {}  ; cycle: save R{}", SCRATCH, AsmEmitter::rn(tgt0), tgt0));
                                em.emit(format!("    MOV   R{}, {}  ; arg {}", tgt0, AsmEmitter::rn(src0), tgt0 - 1));
                                for m in pending.iter_mut() {
                                    if m.1 == tgt0 { m.1 = SCRATCH; }
                                }
                            }
                        }
                    }
                    em.emit(format!("    CALL  {}", func));

                    // Stash the return value in R24 (outside R1-R20 pool, never allocated)
                    // so the caller-restore LOADs (which may write R1) don't clobber it.
                    let has_dst = dst.is_some();
                    if has_dst {
                        em.emit("    MOV   R24, R1  ; stash return value".to_string());
                    }

                    // Restore saved temps from the stack (reverse order for clarity).
                    for (name, orig_reg) in to_save_sorted.iter().rev() {
                        let slot = *em.reg_alloc.spill_slots.get(name)
                            .expect("caller-save slot missing at restore");
                        let offset = em.reg_alloc.sp_depth as i64 - slot as i64;
                        if offset >= 0 && offset <= 13 {
                            em.emit(format!(
                                "    LOAD  R{}, [R26+#{}]  ; caller-restore {}",
                                orig_reg, offset, name
                            ));
                        } else if offset > 13 {
                            em.emit(format!("    TLIT  R{0}, #{1}", orig_reg, offset));
                            em.emit(format!("    TADD  R{0}, R26, R{0}", orig_reg));
                            em.emit(format!("    LOAD  R{0}, [R{0}+#0]  ; caller-restore {1}", orig_reg, name));
                        } else {
                            em.emit(format!("    ; BUG: negative restore offset {} for {}", offset, name));
                        }
                        em.reg_alloc.spill_slots.remove(name);
                        em.reg_alloc.temp_to_reg.insert(name.clone(), *orig_reg);
                    }

                    // Pop all caller-save slots.
                    let n_saved = to_save_sorted.len();
                    if n_saved > 0 {
                        em.emit_sp_adj(n_saved as i64);
                        em.reg_alloc.sp_depth -= n_saved;
                    }

                    // Allocate dst AFTER the pop so sp_depth is at its final post-call value.
                    // This ensures any spill slot for dst gets the correct stack offset.
                    if let Some(d) = dst {
                        let rd = em.dst_reg(d);
                        if rd != 24 {
                            em.emit(format!("    MOV   {}, R24  ; unstash return value", AsmEmitter::rn(rd)));
                        }
                    }
                }
            }
        }

        // ------------------------------------------------------------------ Indirect Call
        IRInstr::CallIndirect { dst, fn_ptr, args, .. } => {
            // Same caller-save convention as direct Call, but use CALLR Rx.
            let cur_instr = em.reg_alloc.current_instr;

            // Pre-load fn_ptr and all args via val_reg BEFORE the caller-saves loop.
            // val_reg emits spill-reload LOADs directly to self.lines (not cur_instr),
            // so they must be resolved while sp_depth still reflects the pre-save state.
            // After flush_instr, the output order is: [pre-LOADs] [caller-saves + CALLR].
            let rfp = em.val_reg(fn_ptr);
            let rargs: Vec<usize> = args.iter().map(|a| em.val_reg(a)).collect();

            let to_save: Vec<(String, usize)> = {
                em.reg_alloc.temp_to_reg
                    .iter()
                    .filter(|(name, &reg)| {
                        reg >= 1 && reg <= 20
                            && em.reg_alloc.last_use
                                .get(*name)
                                .map_or(false, |&lu| lu > cur_instr)
                    })
                    .map(|(n, &r)| (n.clone(), r))
                    .collect()
            };

            let mut to_save_sorted = to_save.clone();
            to_save_sorted.sort_by_key(|(n, _)| n.clone());

            for (name, reg) in &to_save_sorted {
                let slot = em.reg_alloc.sp_depth + 1;
                em.reg_alloc.spill_slots.insert(name.clone(), slot);
                em.reg_alloc.temp_to_reg.remove(name);
                em.reg_alloc.spill_count += 1;
                em.emit(format!("    TSUB  R26, R26, #1  ; caller-save {}", name));
                em.emit(format!("    STORE R{}, [R26+#0]  ; caller-save {}", reg, name));
                em.reg_alloc.sp_depth += 1;
            }

            // Move pre-loaded fn_ptr into R25 and args into R1, R2, ...
            if rfp != 25 {
                em.emit(format!("    MOV   R25, {}  ; fn_ptr for indirect call", AsmEmitter::rn(rfp)));
            }
            // Parallel-move arg setup using R24 as scratch (R25 holds fn_ptr here).
            {
                const SCRATCH: usize = 24;
                let mut pending: Vec<(usize, usize)> = rargs.iter().enumerate()
                    .filter(|(i, &ra)| ra != i + 1)
                    .map(|(i, &ra)| (i + 1, ra))
                    .collect();
                while !pending.is_empty() {
                    let sources: std::collections::HashSet<usize> =
                        pending.iter().map(|&(_, s)| s).collect();
                    if let Some(idx) = pending.iter().position(|&(tgt, _)| !sources.contains(&tgt)) {
                        let (tgt, src) = pending.remove(idx);
                        em.emit(format!("    MOV   R{}, {}  ; arg {}", tgt, AsmEmitter::rn(src), tgt - 1));
                    } else {
                        let (tgt0, src0) = pending.remove(0);
                        em.emit(format!("    MOV   R{}, {}  ; cycle: save R{}", SCRATCH, AsmEmitter::rn(tgt0), tgt0));
                        em.emit(format!("    MOV   R{}, {}  ; arg {}", tgt0, AsmEmitter::rn(src0), tgt0 - 1));
                        for m in pending.iter_mut() { if m.1 == tgt0 { m.1 = SCRATCH; } }
                    }
                }
            }
            {
                let _dummy = (); // suppress unused warning
                for (_i, _ra) in rargs.iter().enumerate() {
                    // handled above
                }
            }
            em.emit("    CALLR R25  ; indirect call through fn_ptr".to_string());

            let has_dst = dst.is_some();
            if has_dst {
                em.emit("    MOV   R24, R1  ; stash return value".to_string());
            }

            for (name, orig_reg) in to_save_sorted.iter().rev() {
                let slot = *em.reg_alloc.spill_slots.get(name)
                    .expect("caller-save slot missing at restore");
                let offset = em.reg_alloc.sp_depth as i64 - slot as i64;
                if offset >= 0 && offset <= 13 {
                    em.emit(format!("    LOAD  R{}, [R26+#{}]  ; caller-restore {}", orig_reg, offset, name));
                } else if offset > 13 {
                    em.emit(format!("    TLIT  R{0}, #{1}", orig_reg, offset));
                    em.emit(format!("    TADD  R{0}, R26, R{0}", orig_reg));
                    em.emit(format!("    LOAD  R{0}, [R{0}+#0]  ; caller-restore {1}", orig_reg, name));
                } else {
                    em.emit(format!("    ; BUG: negative restore offset {} for {}", offset, name));
                }
                em.reg_alloc.spill_slots.remove(name);
                em.reg_alloc.temp_to_reg.insert(name.clone(), *orig_reg);
            }

            let n_saved = to_save_sorted.len();
            if n_saved > 0 {
                em.emit_sp_adj(n_saved as i64);
                em.reg_alloc.sp_depth -= n_saved;
            }

            if let Some(d) = dst {
                let rd = em.dst_reg(d);
                if rd != 24 {
                    em.emit(format!("    MOV   {}, R24  ; unstash return value", AsmEmitter::rn(rd)));
                }
            }
        }

        // ------------------------------------------------------------------ Trit ops
        IRInstr::TritMin { dst, a, b } => {
            let rd = em.dst_reg(dst);
            let ra = em.val_reg(a);
            let rb = em.val_reg(b);
            em.emit(format!("    TMIN  {}, {}, {}", AsmEmitter::rn(rd), AsmEmitter::rn(ra), AsmEmitter::rn(rb)));
        }
        IRInstr::TritMax { dst, a, b } => {
            let rd = em.dst_reg(dst);
            let ra = em.val_reg(a);
            let rb = em.val_reg(b);
            em.emit(format!("    TMAX  {}, {}, {}", AsmEmitter::rn(rd), AsmEmitter::rn(ra), AsmEmitter::rn(rb)));
        }
        IRInstr::TritNeg { dst, a } => {
            let rd = em.dst_reg(dst);
            let ra = em.val_reg(a);
            em.emit(format!("    TNEG  {}, {}", AsmEmitter::rn(rd), AsmEmitter::rn(ra)));
        }

        // ------------------------------------------------------------------ Print
        IRInstr::PrintInt(val) => {
            em.rescue_reg(1);
            let rv = em.val_reg(val);
            if rv != 1 { em.emit(format!("    MOV   R1, {}", AsmEmitter::rn(rv))); }
            em.emit("    SYSCALL #1                  ; print_int".to_string());
        }
        IRInstr::PrintFloat(val) => {
            em.rescue_reg(1);
            let rv = em.val_reg(val);
            if rv != 1 { em.emit(format!("    MOV   R1, {}", AsmEmitter::rn(rv))); }
            em.emit("    SYSCALL #2                  ; print_float".to_string());
        }
        IRInstr::PrintStr(val) => {
            em.rescue_reg(1);
            match val {
                IRValue::Const(IRConst::Str(lbl)) => {
                    let clean = lbl.trim_start_matches('@');
                    em.emit(format!("    TLIT  R1, #{}", clean));
                }
                _ => {
                    let rv = em.val_reg(val);
                    if rv != 1 { em.emit(format!("    MOV   R1, {}", AsmEmitter::rn(rv))); }
                }
            }
            em.emit("    SYSCALL #3                  ; print_str".to_string());
        }
        IRInstr::PrintTrit(val) => {
            em.rescue_reg(1);
            let rv = em.val_reg(val);
            if rv != 1 { em.emit(format!("    MOV   R1, {}", AsmEmitter::rn(rv))); }
            em.emit("    SYSCALL #0                  ; print_trit".to_string());
        }
        IRInstr::PrintBool3(val) => {
            // true/false/unknown, no newline — the variadic print()
            // lowering emits one explicit trailing newline per call, so
            // no Print* instruction may add its own (matches the LLVM
            // backend's __manit_print_bool3 helper).
            em.rescue_reg(1);
            let rv = em.val_reg(val);
            if rv != 1 { em.emit(format!("    MOV   R1, {}", AsmEmitter::rn(rv))); }
            em.emit("    SYSCALL #217                ; print_bool3".to_string());
        }

        // ------------------------------------------------------------------ Phi
        IRInstr::Phi { dst, incoming, .. } => {
            // Value was already copied into dst's register by each predecessor block
            // (via phi_copies in emit_term::Jump). Just ensure dst_reg is allocated.
            let _rd = em.dst_reg(dst);
            em.emit(format!(
                "    ; PHI {} ← [{}]  (value set by predecessor)",
                dst.0,
                incoming.iter().map(|(_, l)| l.as_str()).collect::<Vec<_>>().join(", ")
            ));
        }

        // ------------------------------------------------------------------ Cast
        IRInstr::Cast { dst, src, from_ty, to_ty } => {
            match (from_ty, to_ty) {
                (IRType::I64, IRType::F64) | (IRType::I32, IRType::F64) => {
                    // int→float: syscall 210
                    // IMPORTANT: rescue BEFORE dst_reg so rescue doesn't move dst after rd is captured
                    em.rescue_reg(1);
                    let rd = em.dst_reg(dst);   // re-query after rescue
                    let rs = em.val_reg(src);
                    if rs != 1 { em.emit(format!("    MOV   R1, {}  ; itof input", AsmEmitter::rn(rs))); }
                    em.emit("    SYSCALL #210  ; itof (int→float_bits)".to_string());
                    if rd != 1 { em.emit(format!("    MOV   {}, R1  ; itof result", AsmEmitter::rn(rd))); }
                }
                (IRType::F64, IRType::I64) | (IRType::F64, IRType::I32) => {
                    // float→int truncating: syscall 211
                    em.rescue_reg(1);
                    let rd = em.dst_reg(dst);
                    let rs = em.val_reg(src);
                    if rs != 1 { em.emit(format!("    MOV   R1, {}  ; ftoi input", AsmEmitter::rn(rs))); }
                    em.emit("    SYSCALL #211  ; ftoi (float_bits→int, truncate)".to_string());
                    if rd != 1 { em.emit(format!("    MOV   {}, R1  ; ftoi result", AsmEmitter::rn(rd))); }
                }
                _ => {
                    let rd = em.dst_reg(dst);
                    let rs = em.val_reg(src);
                    if rd != rs {
                        em.emit(format!("    MOV   {}, {}  ; cast", AsmEmitter::rn(rd), AsmEmitter::rn(rs)));
                    }
                }
            }
        }
    }
}

pub(super) fn emit_term(em: &mut AsmEmitter, term: &IRTerminator) {
    match term {
        IRTerminator::Return(None) => {
            // main's return value becomes the process exit status (R1 at halt).
            // A void main must exit 0, so clear R1 instead of leaving garbage.
            if em.fn_name == "main" {
                em.emit("    MOV   R1, R0  ; void main → exit status 0".to_string());
            }
            // Epilogue: restore SP to the callee entry point so the caller's stack is intact.
            let depth = em.reg_alloc.sp_depth;
            if depth > 0 {
                em.emit_sp_adj(depth as i64);
            }
            em.emit("    RET".to_string());
        }
        IRTerminator::Return(Some(val)) => {
            // Load the return value first (may read from spill slot relative to current sp).
            let rv = em.val_reg(val);
            if rv != 1 {
                em.emit(format!("    MOV   R1, {}  ; return value", AsmEmitter::rn(rv)));
            }
            // Epilogue: pop all allocas/spills so caller's SP is unchanged after CALL+RET.
            let depth = em.reg_alloc.sp_depth;
            if depth > 0 {
                em.emit_sp_adj(depth as i64);
            }
            em.emit("    RET".to_string());
        }
        IRTerminator::Jump(label) => {
            // Emit phi copies for this predecessor → target edge before jumping.
            let src = em.current_block_label.clone();
            let dst = label.clone();
            if let Some(copies) = em.phi_copies.get(&(src, dst)).cloned() {
                for (phi_dst, incoming_val) in &copies {
                    let rv = em.val_reg(incoming_val);
                    let rd = em.dst_reg(phi_dst);
                    if rv != rd {
                        em.emit(format!("    MOV   {}, {}  ; phi-copy {}", AsmEmitter::rn(rd), AsmEmitter::rn(rv), phi_dst.0));
                    }
                    em.flush_instr();
                }
            }
            // Normalize sp to the canonical depth expected at the target block.
            // First predecessor sets the canonical depth; subsequent jumps adjust.
            let canonical = em.register_target(label);
            let cur = em.reg_alloc.sp_depth;
            let delta = cur as i64 - canonical as i64;
            if delta != 0 {
                em.emit_sp_adj(delta);
            }
            // Back-edge register reconciliation: a jump to an ALREADY-EMITTED
            // block must put every live temp back into the register that
            // block's code was emitted against (its canonical assignment).
            // Rescue moves earlier in this block may have relocated temps
            // (e.g. the loop's array base out of a syscall-clobbered R1-R3);
            // without these fix-up moves the next iteration reads stale
            // registers.
            if em.emitted_blocks.contains(label) {
                if let Some((canon_regs, _)) = em.block_canonical_regs.get(label).cloned() {
                    // (dst_reg, src_reg) register moves and spill reloads.
                    // Restrict to temps a rescue actually displaced, and never
                    // clobber a canonical register that another live temp
                    // currently occupies (that temp's value takes priority —
                    // the target block reads it from that register).
                    let mut moves: Vec<(usize, usize)> = Vec::new();
                    let mut reloads: Vec<(usize, usize, String)> = Vec::new();
                    for (name, &creg) in &canon_regs {
                        if !em.rescued_temps.contains(name) {
                            continue;
                        }
                        let occupied = em
                            .reg_alloc
                            .temp_to_reg
                            .iter()
                            .any(|(other, &r)| r == creg && other != name);
                        if occupied {
                            continue;
                        }
                        if let Some(&cur_reg) = em.reg_alloc.temp_to_reg.get(name) {
                            if cur_reg != creg {
                                moves.push((creg, cur_reg));
                            }
                        } else if let Some(&slot) = em.reg_alloc.spill_slots.get(name) {
                            reloads.push((creg, slot, name.clone()));
                        }
                    }
                    // Emit register moves in a conflict-safe order: pick moves
                    // whose destination is not a pending source; break cycles
                    // through the R21 scratch register.
                    while !moves.is_empty() {
                        let ready = moves
                            .iter()
                            .position(|&(dst, _)| !moves.iter().any(|&(_, src)| src == dst));
                        match ready {
                            Some(i) => {
                                let (dst, src) = moves.remove(i);
                                em.emit(format!(
                                    "    MOV   R{}, R{}  ; backedge reconcile",
                                    dst, src
                                ));
                            }
                            None => {
                                let (dst, src) = moves.remove(0);
                                em.emit(format!(
                                    "    MOV   R21, R{}  ; backedge cycle break",
                                    src
                                ));
                                for m in moves.iter_mut() {
                                    if m.1 == src {
                                        m.1 = 21;
                                    }
                                }
                                em.emit(format!(
                                    "    MOV   R{}, R21  ; backedge reconcile",
                                    dst
                                ));
                            }
                        }
                    }
                    for (creg, slot, name) in reloads {
                        let offset = em.reg_alloc.sp_depth as i64 - slot as i64;
                        if offset >= 0 && offset <= 13 {
                            em.emit(format!(
                                "    LOAD  R{}, [R26+#{}]  ; backedge reload {}",
                                creg, offset, name
                            ));
                        } else if offset > 13 {
                            em.emit(format!("    TLIT  R21, #{}", offset));
                            em.emit("    TADD  R21, R26, R21  ; backedge spill addr".to_string());
                            em.emit(format!(
                                "    LOAD  R{}, [R21+#0]  ; backedge reload {}",
                                creg, name
                            ));
                        }
                    }
                }
            }
            let ql = em.qlabel(label);
            em.emit(format!("    JUMP  {}", ql));
        }
        IRTerminator::BinBranch { cond, true_label, false_label } => {
            let rc = em.val_reg(cond);
            // Register canonical sp for both targets (first occurrence wins).
            // For conditional branches the delta is always 0 on first occurrence,
            // so no sp adjustment is needed before the TBRANCH.
            em.register_target(true_label);
            em.register_target(false_label);
            // cond is 0 or 1 (bool).  1→pos, 0→false, -1→false.
            let (qt, qf) = (em.qlabel(true_label), em.qlabel(false_label));
            em.emit(format!(
                "    TBRANCH {}, {}, {}, {}",
                AsmEmitter::rn(rc), qt, qf, qf
            ));
        }
        IRTerminator::TritBranch { cond, pos_label, zero_label, neg_label } => {
            let rc = em.val_reg(cond);
            em.register_target(pos_label);
            em.register_target(zero_label);
            em.register_target(neg_label);
            let (qp, qz, qn) = (em.qlabel(pos_label), em.qlabel(zero_label), em.qlabel(neg_label));
            em.emit(format!(
                "    TBRANCH {}, {}, {}, {}",
                AsmEmitter::rn(rc), qp, qz, qn
            ));
        }
        IRTerminator::Unreachable => {
            em.emit("    HALT  ; unreachable".to_string());
        }
    }
}
