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
            // Float comparisons have bool result type, so check op variant directly.
            //
            // `Rem` belongs in this list and was missing from it (report.txt
            // P19): a float `%` fell through to the integer path and emitted
            // TMOD against two IEEE-754 bit patterns.
            let is_float_syscall = (is_float && matches!(op, IRBinOp::Add | IRBinOp::Sub | IRBinOp::Mul | IRBinOp::Div | IRBinOp::Rem))
                || matches!(op, IRBinOp::FEq | IRBinOp::FNe | IRBinOp::FLt | IRBinOp::FGt | IRBinOp::FLe | IRBinOp::FGe);

            // The float syscalls write R1 while setting up, BEFORE the operands
            // are read. Nothing is rescued here any more — `rescue_reg` is gone
            // with the rest of F-3's predecessor — so the guarantee comes from
            // the allocator instead: `regalloc::abi_clobber_sites` lists these
            // same conditions, and a parameter whose live range REACHES one
            // (last use included, not just crossed) is refused R1–R3 and given
            // a frame slot. Inclusive is the point: `x * K` loads K into R1 via
            // SYSCALL #219 and only then multiplies, so a parameter whose final
            // use is this instruction is still destroyed by it. That is what
            // made `math::to_radians` return (PI/180)^2.
            if is_float_syscall
            {
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
                    IRBinOp::Add | IRBinOp::Sub | IRBinOp::Mul | IRBinOp::Div | IRBinOp::Rem => {
                        let sc = match op {
                            IRBinOp::Add => 212,
                            IRBinOp::Sub => 213,
                            IRBinOp::Mul => 214,
                            IRBinOp::Div => 215,
                            _ => 221, // frem (P19)
                        };
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
                // P20: force the IEEE-754 answer when the comparison is
                // UNORDERED. SYSCALL #216 leaves R2 = 1 if either operand is a
                // NaN, and every arm above has finished with its 0/1 result in
                // `d`, so the correction is the same two shapes for all six:
                //
                //   ==, <, >, <=, >=   ->  false when unordered  ->  min(d, 1-R2)
                //   !=                 ->  TRUE  when unordered  ->  max(d, R2)
                //
                // Both operands are 0/1 here, so TMIN is AND and TMAX is OR.
                // Without this, `nan == nan` was true and `nan != nan` false —
                // a guard against NaN passed exactly when it should not.
                // Comparisons only: the arithmetic arm above reaches here too,
                // and syscalls 212-215/221 do not write R2 at all, so applying
                // this to a float sum would corrupt it with a stale register.
                let is_cmp = matches!(
                    op,
                    IRBinOp::FEq
                        | IRBinOp::FNe
                        | IRBinOp::FLt
                        | IRBinOp::FGt
                        | IRBinOp::FLe
                        | IRBinOp::FGe
                );
                if !is_cmp {
                    return;
                }
                if matches!(op, IRBinOp::FNe) {
                    em.emit(format!("    TMAX  {}, {}, R2    ; unordered → ne is true (P20)", d, d));
                } else {
                    let u = em.scratch();
                    let un = AsmEmitter::rn(u);
                    em.emit(format!("    TLIT  {}, #1", un));
                    em.emit(format!("    TSUB  {0}, {0}, R2   ; 1-unordered", un));
                    em.emit(format!("    TMIN  {}, {}, {}    ; unordered → false (P20)", d, d, un));
                }
                return;
            }

            // C4/N5: the version-dependent operators are integer-only by
            // construction — `binop_to_ir` never produces one for a float type
            // — and there is no float encoding for them below, so a mismatch
            // would emit a TDIVN on a double bit pattern and answer garbage
            // rather than fail. Loud in the test suite, free in release.
            debug_assert!(
                !(is_float && matches!(op,
                    IRBinOp::DivNear | IRBinOp::RemNear
                    | IRBinOp::AddT27 | IRBinOp::SubT27 | IRBinOp::MulT27)),
                "a version-dependent integer op reached the T3 emitter with a \
                 float type: {:?}", op
            );

            let rd = em.dst_reg(dst);
            let rl = em.val_reg(lhs);
            let rr = em.val_reg(rhs);
            let (d, l, r) = (AsmEmitter::rn(rd), AsmEmitter::rn(rl), AsmEmitter::rn(rr));

            match op {
                // N5: the checked variants are the SAME instructions here.
                // A T3 register is a 27-trit word and `checked27` already
                // traps on anything that does not fit, so `--lang v2` costs
                // this backend nothing at all — the whole cost of N5 falls on
                // LLVM, which is the backend that was disagreeing.
                IRBinOp::Add | IRBinOp::AddT27 => em.emit(format!("    TADD  {}, {}, {}", d, l, r)),
                IRBinOp::Sub | IRBinOp::SubT27 => em.emit(format!("    TSUB  {}, {}, {}", d, l, r)),
                IRBinOp::Mul | IRBinOp::MulT27 => em.emit(format!("    TMUL  {}, {}, {}", d, l, r)),
                IRBinOp::Div => em.emit(format!("    TDIV  {}, {}, {}", d, l, r)),
                IRBinOp::Rem => em.emit(format!("    TMOD  {}, {}, {}", d, l, r)),
                // C4. One instruction, because T3ISA v1.6 has the operation.
                // The LLVM backend needs seven to say the same thing (see
                // codegen_llvm/emit_instr.rs), which is the measurement C4
                // predicted: rounding to nearest is what dropping low trits
                // already does in this representation, and it costs nothing
                // extra to ask for it.
                IRBinOp::DivNear => em.emit(format!("    TDIVN {}, {}, {}", d, l, r)),
                IRBinOp::RemNear => em.emit(format!("    TMODN {}, {}, {}", d, l, r)),
                IRBinOp::And => em.emit(format!("    BAND  {}, {}, {}", d, l, r)),
                IRBinOp::Or  => em.emit(format!("    BOR   {}, {}, {}", d, l, r)),
                IRBinOp::Xor => em.emit(format!("    BXOR  {}, {}, {}", d, l, r)),

                // F-2: the ternary shifts, which is what these exist for.
                // TSHI is `x * 3^k` with the same `checked27` trap TMUL has;
                // TSHR drops k low trits, which IS round-to-nearest division
                // by 3^k (ties impossible, 3^k being odd).
                IRBinOp::TShl | IRBinOp::TShlT27 | IRBinOp::TShr => {
                    // TShl and TShlT27 are the SAME instruction here: TSHI
                    // traps on 27-trit overflow via `checked27` whether or not
                    // the IR asked for a check. The distinction is LLVM's.
                    let mn = if matches!(op, IRBinOp::TShr) { "TSHR" } else { "TSHI" };
                    if let IRValue::Const(IRConst::Int(n)) = rhs {
                        // The immediate field is THREE TRITS and holds -13..=13,
                        // not the 0..=26 a 27-trit word can be shifted by. A
                        // multiply by 3^18 exists in the stdlib's ternary
                        // conversions and produced "Immediate out of range for
                        // TSHI"; the register form is the same fallback the
                        // binary shifts below already use.
                        if (0..=13).contains(n) {
                            em.emit(format!("    {}  {}, {}, #{}  ; ternary shift", mn, d, l, n));
                        } else {
                            em.emit(format!("    {}  {}, {}, {}  ; ternary shift (wide)", mn, d, l, r));
                        }
                    } else {
                        em.emit(format!("    {}  {}, {}, {}  ; ternary shift (dyn)", mn, d, l, r));
                    }
                }
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
                }

                IRBinOp::StrNe => {
                    // Same clobber handling as StrEq (B24).
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
            // Real structs are heap-allocated, not stack-allocated.  A stack slot
            // is scoped to its loop iteration: the emitter pops back to the
            // block's canonical depth on the back edge, so every iteration's
            // alloca lands on the same address.  That is invisible while the
            // pointer stays inside the iteration, and wrong the moment it
            // escapes — `pcbs[i] = age_tick(p)` stores a struct pointer into an
            // array that outlives the loop, so all nine slots aliased one buffer
            // and every process read back the same PCB.  The LLVM backend mallocs
            // struct allocas for exactly this reason; this matches it.
            //
            // Heap allocations are deliberately NOT recorded in alloca_slots:
            // their address is not SP-relative, so Load/Store must go through the
            // register-based path.
            //
            // Tuples take this path too. They are structural, so they are not
            // in struct_sizes and used to fall through to the stack path — and
            // a tuple escapes constantly, because returning one IS the reason
            // it exists. `fn f() -> (int,int,int)` returned a pointer into its
            // own popped frame, and the caller's destructuring allocas then
            // grew down over it as it read: `(11,22,33)` came back `11 22 22`,
            // `(11,22,33,44,55)` came back `11 22 33 33 22`. Found by the
            // regression test for ORACLE_FINDINGS.md Section 10.
            if let IRType::Struct(name) = ty {
                let n = em
                    .struct_sizes
                    .get(name)
                    .copied()
                    .or_else(|| crate::ir::types::tuple_arity_from_name(name));
                if let Some(n) = n {
                    let n = n.max(1);
                    em.emit(format!("    TLIT  R1, #{}  ; heap alloca {} ({} words)", n, name, n));
                    em.emit("    SYSCALL #218  ; heap_alloc_words".to_string());
                    let rd = em.dst_reg(dst);
                    if rd != 1 {
                        em.emit(format!("    MOV   {}, R1  ; heap alloca base", AsmEmitter::rn(rd)));
                    }
                    return;
                }
            }

            // Arrays need n words; structs need n_fields words; everything else needs 1 word.
            //
            // Tuples are structural, so they are not in struct_sizes; their
            // arity rides in the type name. Without that they took the
            // `unwrap_or(1)` path and a tuple of ANY arity got ONE word, so
            // element 1 was written past the top of the frame and then popped
            // — `(11, 22)` read back as `11 11`. Same root cause as the LLVM
            // side of ORACLE_FINDINGS.md Section 10, found by the regression
            // test written for it.
            let words = match ty {
                IRType::Array(_, n) => *n,
                IRType::Struct(name) => em
                    .struct_sizes
                    .get(name)
                    .copied()
                    .or_else(|| crate::ir::types::tuple_arity_from_name(name))
                    .unwrap_or_else(|| {
                        debug_assert!(
                            !name.starts_with("<tuple"),
                            "internal error: tuple type `{}` carries no parsable \
                             arity — IRType::from_mani must emit `<tuple:N>`",
                            name
                        );
                        // Native opaque handles (Vec, Map, Channel, AtomicTrit,
                        // ...) are a single word. Declared structs and enums are
                        // registered in struct_sizes and never reach here.
                        1
                    }),
                _ => 1,
            };
            // F-3: the storage is a REGION OF THE FRAME at a constant offset,
            // reserved once in the prologue. The old code pushed the stack here
            // and never popped it, so an alloca inside a loop grew the frame by
            // `words` every iteration and each iteration's spill offsets were
            // measured against a different R26. Now the address is simply
            // `R26 + off`, the same on every path and every iteration.
            //
            // Reusing the storage across iterations is a change, and the right
            // one: it is what a stack local means, and it is what the LLVM
            // backend already did.
            let _ = words;
            let off = em.alloca_off.get(&dst.0).copied().unwrap_or(0);
            let rd = em.dst_reg(dst);
            let d = AsmEmitter::rn(rd);
            if off == 0 {
                em.emit(format!("    MOV   {}, R26  ; alloca {:?} @0", d, ty));
            } else if off <= 13 {
                em.emit(format!("    TADD  {}, R26, #{}  ; alloca {:?}", d, off, ty));
            } else {
                em.emit_lit_cur(rd, off as i64);
                em.emit(format!("    TADD  {0}, R26, {0}  ; alloca {1:?}", d, ty));
            }
        }

        // ------------------------------------------------------------------ Store
        IRInstr::Store { ptr, val, .. } => {
            // Use SP-relative addressing for alloca slots (same reason as Load above).
            if let IRValue::Temp(t) = ptr {
                if let Some(&off) = em.alloca_off.get(&t.0) {
                    let rv = em.val_reg(val);
                    if off <= 13 {
                        em.emit(format!("    STORE {}, [R26+#{}]  ; frame store {}", AsmEmitter::rn(rv), off, t.0));
                    } else {
                        let tmp = em.scratch();
                        em.emit(format!("    TLIT  {}, #{}  ; frame offset", AsmEmitter::rn(tmp), off));
                        em.emit(format!("    TADD  {0}, R26, {0}  ; frame addr", AsmEmitter::rn(tmp)));
                        em.emit(format!("    STORE {}, [{}+#0]  ; frame store {}", AsmEmitter::rn(rv), AsmEmitter::rn(tmp), t.0));
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
                if let Some(&off) = em.alloca_off.get(&t.0) {
                    if off <= 13 {
                        em.emit(format!("    LOAD  {}, [R26+#{}]  ; frame load {}", AsmEmitter::rn(rd), off, t.0));
                    } else {
                        let tmp = em.scratch();
                        em.emit(format!("    TLIT  {}, #{}  ; frame offset", AsmEmitter::rn(tmp), off));
                        em.emit(format!("    TADD  {0}, R26, {0}  ; frame addr", AsmEmitter::rn(tmp)));
                        em.emit(format!("    LOAD  {}, [{}+#0]  ; frame load {}", AsmEmitter::rn(rd), AsmEmitter::rn(tmp), t.0));
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
                // `.unwrap()`'s tag guard, the T3 half of the pair whose LLVM
                // half is `manit_check_result_ok` in runtime/core.c. Everything
                // else about Result methods is shared IR — see
                // ir/lower/lower_result.rs.
                "manit_check_result_ok" => {
                    emit_syscall_1arg(em, args, &None, 561, "result_ok_check");
                }
                "io::println" | "io::print" => {
                    // Syscall writes R1; rescue any live temp that lives there.
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
                // Two words on the HEAP: [tag, payload]. The pointer is the
                // value, so it must outlive the frame that built it.
                //
                // These used to allocate on the stack and return R26. A Result
                // therefore died at the `RET` of the function that produced it:
                //
                //     fn ok() -> Result<int,str> { Ok(7) }
                //     fn show(r: Result<int,str>) { match r { Ok(v) => … } }
                //     show(ok());          // T3: no output at all
                //
                // The callee's frame grew straight over the box, so the tag read
                // as garbage, no `match` arm was taken, and the program stopped
                // silently. LLVM had always `malloc`ed (@Ok_new), so this was a
                // representation divergence, not a tuning difference — the same
                // fault, and the same fix, as struct and tuple allocas in
                // ORACLE_FINDINGS.md Section 10.
                "Ok" | "Unknown" | "Err" => {
                    let disc_val: i64 = match func.as_str() { "Ok" => 1, "Unknown" => 0, _ => -1 };
                    // Read the payload BEFORE the allocation: SYSCALL #218
                    // takes its word count in R1 and returns the base there.
                    let raw_ra = if let Some(arg) = args.first() { em.val_reg(arg) } else { 0 };
                    // The sequence needs three registers besides the payload:
                    // R1 (the syscall's argument AND result), R21 (a private
                    // save of whatever R1 held), and R22 (the tag constant).
                    // Move the payload clear of all of them, and of R23, which
                    // belongs to the post-instruction spill store.
                    let ra = if matches!(raw_ra, 1 | 21 | 22 | 23) {
                        em.emit(format!("    MOV   R25, {}  ; save payload to safe reg", AsmEmitter::rn(raw_ra)));
                        25usize
                    } else {
                        raw_ra
                    };
                    // R1 is saved and restored by hand rather than through
                    // `rescue_reg_inclusive`, and that distinction is the whole
                    // bug. A rescue REBINDS the temp to a different register in
                    // the allocator, and the allocator's state runs forward
                    // through emission in block order — but not through
                    // EXECUTION in block order. Rescuing inside one arm of a
                    // `match` left the sibling arm, reached directly from the
                    // branch, reading the temp out of a register that arm never
                    // wrote:
                    //
                    //     match sd(a, b) { Ok(v) => Ok(v), Err(e) => Err(e), … }
                    //
                    // the Ok arm rescued the scrutinee pointer from R1 into R4,
                    // and the Err arm then did `LOAD R5, [R4+#0]` on a register
                    // holding whatever was left there. Save-and-restore moves a
                    // value without ever changing where the allocator believes
                    // it lives, so nothing leaks across the branch.
                    em.emit("    MOV   R21, R1  ; save R1 across the alloc syscall".to_string());
                    em.emit(format!("    TLIT  R1, #2  ; heap alloc Result [tag, payload] ({})", func));
                    em.emit("    SYSCALL #218  ; heap_alloc_words".to_string());
                    em.emit(format!("    TLIT  R22, #{}  ; tag ({})", disc_val, func));
                    em.emit("    STORE R22, [R1+#0]  ; store tag".to_string());
                    em.emit(format!("    STORE {}, [R1+#1]  ; store payload", AsmEmitter::rn(ra)));
                    if let Some(d) = dst {
                        let rd = em.dst_reg(d);
                        // `dst_reg` returns R1 only when no live temp is mapped
                        // there, so in that case the saved value is dead and the
                        // pointer stays put.
                        if rd != 1 {
                            em.emit(format!("    MOV   {}, R1  ; Result ptr ({})", AsmEmitter::rn(rd), func));
                            em.emit("    MOV   R1, R21  ; restore R1".to_string());
                        }
                    } else {
                        em.emit("    MOV   R1, R21  ; restore R1".to_string());
                    }
                }
                // ------------------------------------------------------------------
                // io::println_ternary / io::print_ternary — print t27 as balanced ternary string
                // SYSCALL #7 formats to string, then SYSCALL #3 prints it.
                "io::println_ternary" | "io::print_ternary" => {
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
                // NOT int_to_trit: that one is a genuine narrowing to the sign,
                // and treating it as an identity move here is what made it
                // DIVERGENT — `int_to_trit(5)` returned 5 on T3, which is not a
                // trit at all, while LLVM clamped. It and trit_sign are ManiT
                // source in stdlib/ternary.mt now.
                "ternary::trit_to_int"
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
                // trit_median is ManiT source in stdlib/ternary.mt — it was
                // DIVERGENT while each backend had its own version.
                "ternary::t27_neg" => {
                    let ra = args.first().map(|a| em.val_reg(a)).unwrap_or(0);
                    if let Some(d) = dst {
                        let rd = em.dst_reg(d);
                        em.emit(format!("    TNEG  {}, {}  ; {}", AsmEmitter::rn(rd), AsmEmitter::rn(ra), func));
                    }
                }
                // ternary:: binary ops
                "ternary::t27_and" => {
                    let ra = args.get(0).map(|a| em.val_reg(a)).unwrap_or(0);
                    let rb = args.get(1).map(|a| em.val_reg(a)).unwrap_or(0);
                    if let Some(d) = dst {
                        let rd = em.dst_reg(d);
                        em.emit(format!("    TAND  {}, {}, {}  ; {}", AsmEmitter::rn(rd), AsmEmitter::rn(ra), AsmEmitter::rn(rb), func));
                    }
                }
                "ternary::t27_or" => {
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
                // NOT an alias for trit_rotate_left. A shift DISCARDS the trits it
                // pushes out; a rotation wraps them round. They were aliased here
                // until 19 August 2026, so `trit_rotate_left` silently computed a
                // shift on T3 while failing to compile on LLVM — the backends
                // would have disagreed the moment LLVM gained it. Rotation is now
                // ManiT source in stdlib/ternary.mt.
                "ternary::trit_shift_left" => {
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
                "ternary::trit_shift_right" => {
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
                // Only the plain form. `t27_to_str_padded` (fixed 27 glyphs) and
                // `t27_explain` (a multi-line breakdown that returns nothing)
                // were aliased to this until 19 August 2026 and so produced the
                // unpadded string on T3 while failing to compile on LLVM. Both
                // are ManiT source in stdlib/ternary.mt now.
                "ternary::t27_to_str" => {
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
                // unpack_trits is ManiT source in stdlib/ternary.mt — it was
                // DIVERGENT while syscall #13 and the LLVM helper disagreed.
                // __lp_from_flat(flat_ptr, len) — compiler-internal, syscall #203.
                // R1 = flat trit array ptr, R2 = length → R1 = length-prefixed copy.
                // Emitted by the IR lowering when an unsized `[trit]` parameter
                // reaches a stdlib function that reads length-prefixed.
                "__lp_from_flat" => {
                    emit_syscall_2arg_ret(em, args, dst, 203, "lp_from_flat");
                }
                // fmt:: string formatting functions
                // fmt::show_int(n) — format int as string, returns str ptr (syscall #14)
                "fmt::show_int" | "fmt::int_to_str" => {
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
                "Vec::contains_str" => {
                    emit_syscall_2arg_ret(em, args, dst, 38, "Vec::contains_str");
                }
                "Vec::contains" => {
                    emit_syscall_2arg_ret(em, args, dst, 25, "Vec::contains");
                }
                "Map::new" => {
                    em.emit("    SYSCALL #30  ; Map::new".to_string());
                    if let Some(d) = dst { let rd = em.dst_reg(d); if rd != 1 { em.emit(format!("    MOV   {}, R1", AsmEmitter::rn(rd))); } }
                }
                // The _str variants exist for the native backend, which keys
                // on the raw pointer. The emulator interns by content already,
                // so on T3 they ARE the base syscalls.
                "Map::insert" | "Map::insert_str" => {
                    emit_syscall_3arg(em, args, dst, 31, "Map::insert");
                }
                "Map::get" | "Map::get_str" => {
                    emit_syscall_2arg_ret(em, args, dst, 32, "Map::get");
                }
                "Map::contains_key" | "Map::contains_key_str" => {
                    emit_syscall_2arg_ret(em, args, dst, 33, "Map::contains_key");
                }
                "Map::remove" | "Map::remove_str" => {
                    emit_syscall_2arg(em, args, dst, 34, "Map::remove");
                }
                "Map::len" => {
                    emit_syscall_1arg_ret(em, args, dst, 35, "Map::len");
                }
                "Map::is_empty" => {
                    emit_syscall_1arg_ret(em, args, dst, 36, "Map::is_empty");
                }
                "Set::new" => {
                    em.emit("    SYSCALL #40  ; Set::new".to_string());
                    if let Some(d) = dst { let rd = em.dst_reg(d); if rd != 1 { em.emit(format!("    MOV   {}, R1", AsmEmitter::rn(rd))); } }
                }
                "Set::insert" | "Set::insert_str" => {
                    emit_syscall_2arg(em, args, dst, 41, "Set::insert");
                }
                "Set::contains" | "Set::contains_str" => {
                    emit_syscall_2arg_ret(em, args, dst, 42, "Set::contains");
                }
                "Set::remove" | "Set::remove_str" => {
                    emit_syscall_2arg(em, args, dst, 43, "Set::remove");
                }
                "Set::len" => {
                    emit_syscall_1arg_ret(em, args, dst, 44, "Set::len");
                }
                "Deque::new" => {
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
                // The two char primitives. Everything else that needs a char —
                // to_upper, to_lower, pad_left, pad_right, center — is ManiT
                // source in stdlib/str.mt built on these, so there is exactly
                // one implementation of each and the backends cannot diverge.
                // 133/134 rather than 70/71: the 60-69 str block is full.
                "str_char_at" | "str::char_at" => {
                    emit_syscall_2arg_ret(em, args, dst, 133, "str_char_at");
                }
                "str_from_char" | "str::from_char" => {
                    emit_syscall_1arg_ret(em, args, dst, 134, "str_from_char");
                }
                "ternary_int_to_trits" | "ternary::int_to_trits" => {
                    emit_syscall_2arg_ret(em, args, dst, 135, "ternary_int_to_trits");
                }
                "channel" | "channel_new" => {
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
                "Vec::sort_str" => {
                    emit_syscall_1arg(em, args, dst, 39, "Vec::sort_str");
                }
                "Vec::sort" => {
                    emit_syscall_1arg(em, args, dst, 100, "Vec::sort");
                }
                "Vec::reverse" => {
                    emit_syscall_1arg(em, args, dst, 101, "Vec::reverse");
                }
                "Vec::index_of_str" => {
                    emit_syscall_2arg_ret(em, args, dst, 45, "Vec::index_of_str");
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
                "Map::get_or" | "Map::get_or_str" => {
                    emit_syscall_3arg_ret(em, args, dst, 87, "Map::get_or");
                }
                "Map::keys" => {
                    emit_syscall_1arg_ret(em, args, dst, 88, "Map::keys");
                }
                "Map::values" => {
                    emit_syscall_1arg_ret(em, args, dst, 37, "Map::values");
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
                    em.emit("    SYSCALL #540  ; time::now".to_string());
                    if let Some(d) = dst {
                        let rd = em.dst_reg(d);
                        if rd != 1 { em.emit(format!("    MOV   {}, R1  ; time::now result", AsmEmitter::rn(rd))); }
                    }
                }
                // ------------------------------------------------------------------
                // Env syscalls (550, 552-553)
                // ------------------------------------------------------------------
                //
                // `env::args` is NOT here, and its absence is the fix rather than
                // an omission. It used to be syscall 551, which returned an empty
                // Vec unconditionally — a stub that answered "no arguments" to
                // every question and could not be caught by the differential
                // oracle, because the LLVM side had no `env_args` symbol at all
                // and so never got far enough to disagree. It is now ordinary
                // maniT in stdlib/env.mt, built over the two scalar natives
                // below, and falls through to the general-call path.
                "env::exit" => {
                    emit_syscall_1arg(em, args, dst, 550, "env::exit");
                }
                "env::argc" => {
                    em.emit("    SYSCALL #552  ; env::argc".to_string());
                    if let Some(d) = dst {
                        let rd = em.dst_reg(d);
                        if rd != 1 { em.emit(format!("    MOV   {}, R1  ; env::argc result", AsmEmitter::rn(rd))); }
                    }
                }
                "env::arg" => {
                    emit_syscall_1arg_ret(em, args, dst, 553, "env::arg");
                }
                _ => {
                    // F-3: no caller-save, and that is the invariant rather
                    // than an optimisation.
                    //
                    // A CALL may destroy every register — the callee allocates
                    // from the same pool — so the allocator refuses to put
                    // anything live across a call in one (see
                    // `codegen_t3::regalloc`). Whatever the callee clobbers was
                    // already dead or already in the frame.
                    //
                    // What this replaces was fifty lines of pushing live temps,
                    // renaming them into ad-hoc spill slots, calling, reloading
                    // and popping — the machinery `KNOWN_ISSUES` issue 2 records
                    // two silently-wrong-answer defects in.
                    let targets: Vec<(usize, CallSrc)> = args
                        .iter()
                        .map(|a| em.resolve_call_src(a))
                        .enumerate()
                        .map(|(i, s)| (i + 1, s))
                        .collect();
                    em.emit_call_operands(&targets, 9); // parameters are R1..R8

                    em.emit(format!("    CALL  {}", func));

                    if let Some(d) = dst {
                        let rd = em.dst_reg(d);
                        if rd != 1 {
                            em.emit(format!(
                                "    MOV   {}, R1  ; call result",
                                AsmEmitter::rn(rd)
                            ));
                        }
                    }
                }
            }
        }

        // ------------------------------------------------------------------ Indirect Call
        IRInstr::CallIndirect { dst, fn_ptr, args, .. } => {
            // As above: nothing to save. The callee address is materialised
            // into R25 as just another operand target, which is what keeps it
            // from overwriting an argument that shares its register.
            let mut targets: Vec<(usize, CallSrc)> = args
                .iter()
                .map(|a| em.resolve_call_src(a))
                .enumerate()
                .map(|(i, s)| (i + 1, s))
                .collect();
            targets.push((25, em.resolve_call_src(fn_ptr)));
            em.emit_call_operands(&targets, 9);

            em.emit("    CALLR R25  ; indirect call through fn_ptr".to_string());

            if let Some(d) = dst {
                let rd = em.dst_reg(d);
                if rd != 1 {
                    em.emit(format!("    MOV   {}, R1  ; call result", AsmEmitter::rn(rd)));
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
        // C2 / T3ISA v1.5. One instruction each — this is the entire point:
        // what the standard library writes as 27 iterations of
        // divide-by-3^i, operate, multiply-back becomes a single opcode.
        IRInstr::TritLane { dst, op, a, b } => {
            let rd = em.dst_reg(dst);
            let ra = em.val_reg(a);
            let rb = em.val_reg(b);
            let mnemonic = match op {
                IRLaneOp::And => "TANDW",
                IRLaneOp::Or => "TORW",
                IRLaneOp::Xor => "TXORW",
                IRLaneOp::Imp => "TIMPW",
                IRLaneOp::Cmp => "TCMPW",
                IRLaneOp::Popcount => "TPOPC",
            };
            em.emit(format!("    {:<5} {}, {}, {}", mnemonic,
                AsmEmitter::rn(rd), AsmEmitter::rn(ra), AsmEmitter::rn(rb)));
        }
        IRInstr::TritNeg { dst, a } => {
            let rd = em.dst_reg(dst);
            let ra = em.val_reg(a);
            em.emit(format!("    TNEG  {}, {}", AsmEmitter::rn(rd), AsmEmitter::rn(ra)));
        }
        // C7: sign(x) in ONE instruction. R0 always reads as zero, so
        // `TCMP Rd, Ra, R0` is sign(Ra - 0) = sign(Ra). This is the operation
        // the recommendations single out — a branch in two's complement, a
        // single compare here.
        IRInstr::TritSign { dst, a } => {
            let rd = em.dst_reg(dst);
            let ra = em.val_reg(a);
            em.emit(format!("    TCMP  {}, {}, R0  ; sign", AsmEmitter::rn(rd), AsmEmitter::rn(ra)));
        }

        // ------------------------------------------------------------------ Print
        IRInstr::PrintInt(val) => {
            let rv = em.val_reg(val);
            if rv != 1 { em.emit(format!("    MOV   R1, {}", AsmEmitter::rn(rv))); }
            em.emit("    SYSCALL #1                  ; print_int".to_string());
        }
        IRInstr::PrintFloat(val) => {
            let rv = em.val_reg(val);
            if rv != 1 { em.emit(format!("    MOV   R1, {}", AsmEmitter::rn(rv))); }
            em.emit("    SYSCALL #2                  ; print_float".to_string());
        }
        IRInstr::PrintStr(val) => {
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
            let rv = em.val_reg(val);
            if rv != 1 { em.emit(format!("    MOV   R1, {}", AsmEmitter::rn(rv))); }
            em.emit("    SYSCALL #0                  ; print_trit".to_string());
        }
        IRInstr::PrintBool3(val) => {
            // true/false/unknown, no newline — the variadic print()
            // lowering emits one explicit trailing newline per call, so
            // no Print* instruction may add its own (matches the LLVM
            // backend's __manit_print_bool3 helper).
            let rv = em.val_reg(val);
            if rv != 1 { em.emit(format!("    MOV   R1, {}", AsmEmitter::rn(rv))); }
            em.emit("    SYSCALL #217                ; print_bool3".to_string());
        }

        // ------------------------------------------------------------------ Phi
        IRInstr::Phi { dst, incoming, .. } => {
            // A phi emits NOTHING. Every predecessor has already written the
            // value to this phi's location on its own edge, so there is nothing
            // left to do here — and doing something is actively wrong.
            //
            // Calling `dst_reg` for it, as this arm used to, schedules a store
            // of the destination scratch into the phi's frame slot for a phi
            // that lives in one. That store runs AFTER the predecessors' copies
            // and overwrites the value with whatever R23 last held: a function
            // whose body is a tail expression — `if n <= 1 { 1 } else { … }`,
            // which is a phi at the merge — returned a frame address.
            em.emit(format!(
                "    ; PHI {} ← [{}]  (value placed by each predecessor)",
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
                    let rd = em.dst_reg(dst);   // re-query after rescue
                    let rs = em.val_reg(src);
                    if rs != 1 { em.emit(format!("    MOV   R1, {}  ; itof input", AsmEmitter::rn(rs))); }
                    em.emit("    SYSCALL #210  ; itof (int→float_bits)".to_string());
                    if rd != 1 { em.emit(format!("    MOV   {}, R1  ; itof result", AsmEmitter::rn(rd))); }
                }
                // A3: integer → trit CLAMPS to {-1, 0, +1}.
                //
                // The language reference has always said so ("`let t = i as
                // trit;  // clamps to {-1, 0, +1}`") and the LLVM backend has
                // always done it. T3 fell through to the bare `MOV` below, so
                // `5 as trit` evaluated to **5 on T3 and 1 on LLVM** — a value
                // outside the type's own carrier set, on the backend whose
                // whole point is that the carrier set is the hardware's.
                //
                // Found by writing the normative semantics (A3) rather than by
                // either backend failing: each is internally consistent, and
                // nothing in the corpus casts a wide int to a trit.
                //
                // Two instructions, no branch. TMIN/TMAX are NUMERIC min/max
                // on a whole register here, and both take a 3-trit immediate,
                // so ±1 encode directly.
                (IRType::I8 | IRType::I16 | IRType::I32 | IRType::I64, IRType::Trit) => {
                    let rd = em.dst_reg(dst);
                    let rs = em.val_reg(src);
                    let (d, s) = (AsmEmitter::rn(rd), AsmEmitter::rn(rs));
                    em.emit(format!("    TMIN  {}, {}, #1   ; int->trit clamp (upper)", d, s));
                    em.emit(format!("    TMAX  {}, {}, #-1  ; int->trit clamp (lower)", d, d));
                }
                (IRType::F64, IRType::I64) | (IRType::F64, IRType::I32) => {
                    // float→int truncating: syscall 211
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

/// Emit a parallel copy between LOCATIONS.
///
/// The copies on one control-flow edge are simultaneous: every source is read
/// as it was before any destination is written. Emitting them in list order is
/// correct only while no copy's destination is another's source — which is rare
/// while locals live in memory, and stops being rare the moment they do not.
/// `a, b = b, a + b`, the iterative Fibonacci loop, is two phis whose homes are
/// each other's sources; copying them one after the other made `fib(4)` come out
/// as 4 (report.txt P11).
///
/// So: emit a copy only when nothing still pending READS its destination, and
/// when everything left is blocked — a cycle — park one source in R23 and read
/// it from there. Slots never block anything: a store to a frame slot cannot
/// disturb a register another copy still has to read.
fn emit_phi_copies(em: &mut AsmEmitter, copies: &[(IRTemp, IRValue)]) {
    /// Holds a source parked to break a cycle, from the break until that
    /// copy is emitted — possibly several copies later.
    const CYCLE: usize = 23;
    /// Moves a value into a frame slot. Must differ from CYCLE, which may be
    /// occupied, and from R21, the address scratch of the wide-offset store.
    const XFER: usize = 22;

    // Resolve every copy to (destination location, source) without emitting.
    let plan: Vec<(Loc, CallSrc)> = copies
        .iter()
        .filter_map(|(d, v)| em.alloc.loc(&d.0).map(|dl| (dl, em.resolve_call_src(v))))
        .collect();

    let mut pending: Vec<usize> = (0..plan.len()).collect();
    // Copies whose source has been parked in the scratch register.
    let mut staged: Vec<bool> = vec![false; plan.len()];

    // Does `src` read the location `loc` is about to be written?
    //
    // A frame slot interferes exactly as a register does, and saying otherwise
    // is what made `fib` return a power of two under `--mem2reg`. The back-edge
    // of `a, b = b, a + b` carries three copies, one of which has a slot for
    // its home:
    //
    //     i' ← R7      b'(slot0) ← R6      a'(R5) ← slot0
    //
    // A slot destination used to be declared unconditionally ready, so the
    // store to slot0 was emitted before the load from it and `a'` read the
    // value `b'` had just overwritten. Nothing here is a cycle — it is a plain
    // ordering constraint that the interference test could not see.
    let reads = |src: &CallSrc, loc: Loc| match (src, loc) {
        (CallSrc::Reg(s), Loc::Reg(d)) => *s == d,
        (CallSrc::Slot(s), Loc::Slot(d)) => *s == d,
        _ => false,
    };

    while !pending.is_empty() {
        // Ready = writing this destination destroys nobody's unread source.
        // A staged copy no longer reads its original location, so it is skipped.
        let ready = pending.iter().position(|&i| {
            let writes = plan[i].0;
            !pending
                .iter()
                .any(|&j| j != i && !staged[j] && reads(&plan[j].1, writes))
        });
        let idx = match ready {
            Some(k) => pending.remove(k),
            None => {
                // A genuine cycle. Park one member's source in the scratch and
                // let the copy it was blocking proceed; that member reads the
                // scratch when its turn comes.
                //
                // Only ONE cycle is ever open at a time: after a break, the
                // rest of that cycle is a chain, and the ready-first policy
                // drains it — the staged copy included — before anything can
                // stall again. That is what makes a single scratch enough.
                //
                // Slot sources are eligible victims too. A cycle living wholly
                // in the frame is expressible, and when only register sources
                // could be picked it fell through to the `remove(0)` arm below
                // and emitted the copies in a silently wrong order.
                let victim = pending
                    .iter()
                    .copied()
                    .find(|&j| !staged[j] && matches!(plan[j].1, CallSrc::Reg(_) | CallSrc::Slot(_)));
                match victim {
                    Some(j) => {
                        match plan[j].1 {
                            CallSrc::Reg(sr) => {
                                em.emit(format!("    MOV   R{}, R{}  ; break phi cycle", CYCLE, sr))
                            }
                            CallSrc::Slot(sl) => {
                                em.emit_slot_load(CYCLE, sl, "break phi cycle (from frame)")
                            }
                            _ => unreachable!("victim is a register or slot source"),
                        }
                        staged[j] = true;
                        continue;
                    }
                    // Unreachable: only a register or slot source can block
                    // anything, and a stall means something is blocked.
                    None => {
                        em.emit("    ; BUG: phi cycle with no breakable source".to_string());
                        pending.remove(0)
                    }
                }
            }
        };

        let (dst_loc, src) = (plan[idx].0, plan[idx].1.clone());
        let src = if staged[idx] { CallSrc::Reg(CYCLE) } else { src };
        match dst_loc {
            Loc::Reg(d) => match src {
                CallSrc::Reg(s) => {
                    if s != d {
                        em.emit(format!("    MOV   R{}, R{}  ; phi-copy", d, s));
                    }
                }
                CallSrc::Slot(sl) => em.emit_slot_load(d, sl, "phi-copy from frame"),
                CallSrc::Lit(v) => em.emit_lit_cur(d, v),
                CallSrc::Label(l) => em.emit(format!("    TLIT  R{}, #{}  ; phi-copy", d, l)),
                CallSrc::Float(f) => {
                    let bits = f.to_bits() as i64;
                    let label = format!("@float_{}_{}", em.fn_name, em.float_literals.len());
                    let clean = label.trim_start_matches('@').to_string();
                    em.float_literals.push((label, bits));
                    em.emit(format!("    TLIT  R1, #{}  ; float-lit addr", clean));
                    em.emit(format!("    SYSCALL #219  ; float_load bits for {}", f));
                    if d != 1 {
                        em.emit(format!("    MOV   R{}, R1  ; phi-copy float", d));
                    }
                }
                CallSrc::Missing(n) => {
                    em.emit(format!("    ; BUG: phi operand {} has no location", n));
                    em.emit(format!("    TLIT  R{}, #0", d));
                }
            },
            Loc::Slot(ds) => {
                // Into the frame: bring the value into a scratch first.
                //
                // XFER, not CYCLE. A cycle may be open — CYCLE holding a source
                // that a later copy in this same parallel assignment still has
                // to read — and staging through it here would destroy that
                // value. R21 is not available either: it is the address scratch
                // the `ds > 13` store below uses.
                let sr = match src {
                    CallSrc::Reg(s) => s,
                    CallSrc::Slot(sl) => {
                        em.emit_slot_load(XFER, sl, "phi-copy via frame");
                        XFER
                    }
                    CallSrc::Lit(v) => {
                        em.emit_lit_cur(XFER, v);
                        XFER
                    }
                    CallSrc::Label(l) => {
                        em.emit(format!("    TLIT  R{}, #{}  ; phi-copy", XFER, l));
                        XFER
                    }
                    CallSrc::Float(f) => {
                        let bits = f.to_bits() as i64;
                        let label = format!("@float_{}_{}", em.fn_name, em.float_literals.len());
                        let clean = label.trim_start_matches('@').to_string();
                        em.float_literals.push((label, bits));
                        em.emit(format!("    TLIT  R1, #{}  ; float-lit addr", clean));
                        em.emit(format!("    SYSCALL #219  ; float_load bits for {}", f));
                        em.emit(format!("    MOV   R{}, R1  ; phi-copy float", XFER));
                        XFER
                    }
                    CallSrc::Missing(n) => {
                        em.emit(format!("    ; BUG: phi operand {} has no location", n));
                        em.emit(format!("    TLIT  R{}, #0", XFER));
                        XFER
                    }
                };
                if ds <= 13 {
                    em.emit(format!("    STORE R{}, [R26+#{}]  ; phi-copy to frame", sr, ds));
                } else {
                    em.emit(format!("    TLIT  R21, #{}", ds));
                    em.emit("    TADD  R21, R26, R21  ; frame addr".to_string());
                    em.emit(format!("    STORE R{}, [R21+#0]  ; phi-copy to frame", sr));
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
            em.emit_epilogue();
            em.emit("    RET".to_string());
        }
        IRTerminator::Return(Some(val)) => {
            // The value first: it may come from a frame slot, and the epilogue
            // is what makes those addresses stop being valid.
            let rv = em.val_reg(val);
            if rv != 1 {
                em.emit(format!("    MOV   R1, {}  ; return value", AsmEmitter::rn(rv)));
            }
            em.emit_epilogue();
            em.emit("    RET".to_string());
        }
        IRTerminator::Jump(label) => {
            let src = em.current_block_label.clone();
            if let Some(copies) = em.phi_copies.get(&(src, label.clone())).cloned() {
                emit_phi_copies(em, &copies);
            }
            em.emit(format!("    JUMP  {}", em.qlabel(label)));
        }
        IRTerminator::BinBranch { cond, true_label, false_label } => {
            // A conditional branch carries no phi copies: `ssa::split_critical_edges`
            // puts a block on any edge that would need them, and that block's
            // jump does the copying. Without it, an edge from a branch into a
            // merge has nowhere to put the value (report.txt P12).
            let rc = em.val_reg(cond);
            let (qt, qf) = (em.qlabel(true_label), em.qlabel(false_label));
            em.emit(format!(
                "    TBRANCH {}, {}, {}, {}",
                AsmEmitter::rn(rc), qt, qf, qf
            ));
        }
        IRTerminator::TritBranch { cond, pos_label, zero_label, neg_label } => {
            let rc = em.val_reg(cond);
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
