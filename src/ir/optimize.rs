use std::collections::{HashMap, HashSet};
use super::types::*;

/// Run all optimization passes on every non-extern function in the module.
pub fn run_passes(module: &mut IRModule) {
    for func in &mut module.functions {
        if func.is_extern {
            continue;
        }
        constant_fold(func);
        constant_propagate(func);
        ternary_peephole(func);
        common_subexpression_eliminate(func);
        strength_reduce(func);
        dead_code_eliminate(func);
    }
    dead_block_eliminate(module);
}

// ---------------------------------------------------------------------------
// Pass 1: Constant Folding
// ---------------------------------------------------------------------------

fn constant_fold(func: &mut IRFunction) {
    for block in &mut func.blocks {
        for instr in &mut block.instrs {
            fold_instruction(instr);
        }
    }
}

fn fold_instruction(instr: &mut IRInstr) {
    match instr {
        IRInstr::BinOp { dst, op, lhs, rhs, ty } => {
            if let Some(result) = try_fold_binop(op, lhs, rhs) {
                *instr = IRInstr::Assign {
                    dst: IRTemp::new(dst.0.clone()),
                    src: IRValue::Const(result),
                    ty: ty.clone(),
                };
            }
        }
        IRInstr::UnOp { dst, op, operand, ty } => {
            if let Some(result) = try_fold_unop(op, operand) {
                *instr = IRInstr::Assign {
                    dst: IRTemp::new(dst.0.clone()),
                    src: IRValue::Const(result),
                    ty: ty.clone(),
                };
            }
        }
        IRInstr::TritMin { dst, a, b } => {
            if let (IRValue::Const(IRConst::Trit(va)), IRValue::Const(IRConst::Trit(vb))) = (a, b) {
                let result = (*va).min(*vb);
                *instr = IRInstr::Assign {
                    dst: IRTemp::new(dst.0.clone()),
                    src: IRValue::Const(IRConst::Trit(result)),
                    ty: IRType::Trit,
                };
            }
        }
        IRInstr::TritMax { dst, a, b } => {
            if let (IRValue::Const(IRConst::Trit(va)), IRValue::Const(IRConst::Trit(vb))) = (a, b) {
                let result = (*va).max(*vb);
                *instr = IRInstr::Assign {
                    dst: IRTemp::new(dst.0.clone()),
                    src: IRValue::Const(IRConst::Trit(result)),
                    ty: IRType::Trit,
                };
            }
        }
        IRInstr::TritNeg { dst, a } => {
            if let IRValue::Const(IRConst::Trit(v)) = a {
                *instr = IRInstr::Assign {
                    dst: IRTemp::new(dst.0.clone()),
                    src: IRValue::Const(IRConst::Trit(-*v)),
                    ty: IRType::Trit,
                };
            }
        }
        _ => {}
    }
}

fn try_fold_binop(op: &IRBinOp, lhs: &IRValue, rhs: &IRValue) -> Option<IRConst> {
    match (lhs, rhs) {
        (IRValue::Const(IRConst::Int(a)), IRValue::Const(IRConst::Int(b))) => {
            fold_int_binop(op, *a, *b)
        }
        (IRValue::Const(IRConst::Float(a)), IRValue::Const(IRConst::Float(b))) => {
            fold_float_binop(op, *a, *b)
        }
        (IRValue::Const(IRConst::Bool(a)), IRValue::Const(IRConst::Bool(b))) => {
            match op {
                IRBinOp::And => Some(IRConst::Bool(*a && *b)),
                IRBinOp::Or => Some(IRConst::Bool(*a || *b)),
                IRBinOp::Xor => Some(IRConst::Bool(*a ^ *b)),
                IRBinOp::IEq => Some(IRConst::Bool(*a == *b)),
                IRBinOp::INe => Some(IRConst::Bool(*a != *b)),
                _ => None,
            }
        }
        _ => None,
    }
}

fn fold_int_binop(op: &IRBinOp, a: i64, b: i64) -> Option<IRConst> {
    match op {
        IRBinOp::Add => Some(IRConst::Int(a.wrapping_add(b))),
        IRBinOp::Sub => Some(IRConst::Int(a.wrapping_sub(b))),
        IRBinOp::Mul => Some(IRConst::Int(a.wrapping_mul(b))),
        IRBinOp::Div => {
            if b == 0 { None } else { Some(IRConst::Int(a.wrapping_div(b))) }
        }
        IRBinOp::Rem => {
            if b == 0 { None } else { Some(IRConst::Int(a.wrapping_rem(b))) }
        }
        IRBinOp::IEq => Some(IRConst::Bool(a == b)),
        IRBinOp::INe => Some(IRConst::Bool(a != b)),
        IRBinOp::ILt => Some(IRConst::Bool(a < b)),
        IRBinOp::IGt => Some(IRConst::Bool(a > b)),
        IRBinOp::ILe => Some(IRConst::Bool(a <= b)),
        IRBinOp::IGe => Some(IRConst::Bool(a >= b)),
        IRBinOp::And => Some(IRConst::Int(a & b)),
        IRBinOp::Or => Some(IRConst::Int(a | b)),
        IRBinOp::Xor => Some(IRConst::Int(a ^ b)),
        IRBinOp::LShift => Some(IRConst::Int(a.wrapping_shl(b as u32))),
        IRBinOp::RShift => Some(IRConst::Int(a.wrapping_shr(b as u32))),
        _ => None,
    }
}

fn fold_float_binop(op: &IRBinOp, a: f64, b: f64) -> Option<IRConst> {
    match op {
        IRBinOp::Add => Some(IRConst::Float(a + b)),
        IRBinOp::Sub => Some(IRConst::Float(a - b)),
        IRBinOp::Mul => Some(IRConst::Float(a * b)),
        IRBinOp::Div => {
            if b == 0.0 { None } else { Some(IRConst::Float(a / b)) }
        }
        IRBinOp::Rem => {
            if b == 0.0 { None } else { Some(IRConst::Float(a % b)) }
        }
        IRBinOp::FEq => Some(IRConst::Bool(a == b)),
        IRBinOp::FNe => Some(IRConst::Bool(a != b)),
        IRBinOp::FLt => Some(IRConst::Bool(a < b)),
        IRBinOp::FGt => Some(IRConst::Bool(a > b)),
        IRBinOp::FLe => Some(IRConst::Bool(a <= b)),
        IRBinOp::FGe => Some(IRConst::Bool(a >= b)),
        _ => None,
    }
}

fn try_fold_unop(op: &IRUnOp, operand: &IRValue) -> Option<IRConst> {
    match (op, operand) {
        (IRUnOp::Neg, IRValue::Const(IRConst::Int(v))) => Some(IRConst::Int(v.wrapping_neg())),
        (IRUnOp::Not, IRValue::Const(IRConst::Bool(v))) => Some(IRConst::Bool(!v)),
        (IRUnOp::FNeg, IRValue::Const(IRConst::Float(v))) => Some(IRConst::Float(-v)),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Pass 2: Constant Propagation
// ---------------------------------------------------------------------------

fn constant_propagate(func: &mut IRFunction) {
    // Build a map from temp name -> constant value
    let mut const_map: HashMap<String, IRConst> = HashMap::new();

    // First pass: collect constants from Assign instructions
    for block in &func.blocks {
        for instr in &block.instrs {
            if let IRInstr::Assign { dst, src: IRValue::Const(c), .. } = instr {
                const_map.insert(dst.0.clone(), c.clone());
            }
        }
    }

    if const_map.is_empty() {
        return;
    }

    // Second pass: substitute constants in all instructions and terminators
    for block in &mut func.blocks {
        for instr in &mut block.instrs {
            propagate_in_instruction(instr, &const_map);
        }
        propagate_in_terminator(&mut block.term, &const_map);
    }

    // Run constant folding again to fold newly-constant expressions
    constant_fold(func);
}

fn propagate_value(val: &mut IRValue, const_map: &HashMap<String, IRConst>) {
    if let IRValue::Temp(ref t) = val {
        if let Some(c) = const_map.get(&t.0) {
            *val = IRValue::Const(c.clone());
        }
    }
}

fn propagate_in_instruction(instr: &mut IRInstr, const_map: &HashMap<String, IRConst>) {
    match instr {
        IRInstr::BinOp { lhs, rhs, .. } => {
            propagate_value(lhs, const_map);
            propagate_value(rhs, const_map);
        }
        IRInstr::UnOp { operand, .. } => {
            propagate_value(operand, const_map);
        }
        IRInstr::Assign { src, .. } => {
            propagate_value(src, const_map);
        }
        IRInstr::Store { ptr, val, .. } => {
            propagate_value(ptr, const_map);
            propagate_value(val, const_map);
        }
        IRInstr::Load { ptr, .. } => {
            propagate_value(ptr, const_map);
        }
        IRInstr::Call { args, .. } => {
            for arg in args.iter_mut() {
                propagate_value(arg, const_map);
            }
        }
        IRInstr::CallIndirect { fn_ptr, args, .. } => {
            propagate_value(fn_ptr, const_map);
            for arg in args.iter_mut() {
                propagate_value(arg, const_map);
            }
        }
        IRInstr::GetPtr { ptr, idx, .. } => {
            propagate_value(ptr, const_map);
            propagate_value(idx, const_map);
        }
        IRInstr::TritMin { a, b, .. } => {
            propagate_value(a, const_map);
            propagate_value(b, const_map);
        }
        IRInstr::TritMax { a, b, .. } => {
            propagate_value(a, const_map);
            propagate_value(b, const_map);
        }
        IRInstr::TritNeg { a, .. } => {
            propagate_value(a, const_map);
        }
        IRInstr::PrintStr(v) | IRInstr::PrintInt(v) | IRInstr::PrintFloat(v)
        | IRInstr::PrintBool3(v) | IRInstr::PrintTrit(v) => {
            propagate_value(v, const_map);
        }
        IRInstr::Cast { src, .. } => {
            propagate_value(src, const_map);
        }
        // Don't propagate into Phi nodes — values differ per incoming edge
        IRInstr::Phi { .. } => {}
        IRInstr::Alloca { .. } => {}
    }
}

fn propagate_in_terminator(term: &mut IRTerminator, const_map: &HashMap<String, IRConst>) {
    match term {
        IRTerminator::Return(Some(ref mut val)) => {
            propagate_value(val, const_map);
        }
        IRTerminator::BinBranch { cond, .. } => {
            propagate_value(cond, const_map);
        }
        IRTerminator::TritBranch { cond, .. } => {
            propagate_value(cond, const_map);
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Pass 3: Dead Code Elimination
// ---------------------------------------------------------------------------

fn dead_code_eliminate(func: &mut IRFunction) {
    loop {
        let used = collect_used_temps(func);
        let mut changed = false;

        for block in &mut func.blocks {
            block.instrs.retain(|instr| {
                if is_side_effecting(instr) {
                    return true;
                }
                if let Some(dst_name) = instr_dst_name(instr) {
                    if !used.contains(dst_name) {
                        changed = true;
                        return false;
                    }
                }
                true
            });
        }

        if !changed {
            break;
        }
    }
}

/// Collect all IRTemp names that are used (read from) anywhere.
fn collect_used_temps(func: &IRFunction) -> HashSet<String> {
    let mut used = HashSet::new();

    for block in &func.blocks {
        for instr in &block.instrs {
            collect_used_in_instruction(instr, &mut used);
        }
        collect_used_in_terminator(&block.term, &mut used);
    }

    used
}

fn collect_used_from_value(val: &IRValue, used: &mut HashSet<String>) {
    if let IRValue::Temp(ref t) = val {
        used.insert(t.0.clone());
    }
}

fn collect_used_in_instruction(instr: &IRInstr, used: &mut HashSet<String>) {
    match instr {
        IRInstr::BinOp { lhs, rhs, .. } => {
            collect_used_from_value(lhs, used);
            collect_used_from_value(rhs, used);
        }
        IRInstr::UnOp { operand, .. } => {
            collect_used_from_value(operand, used);
        }
        IRInstr::Assign { src, .. } => {
            collect_used_from_value(src, used);
        }
        IRInstr::Store { ptr, val, .. } => {
            collect_used_from_value(ptr, used);
            collect_used_from_value(val, used);
        }
        IRInstr::Load { ptr, .. } => {
            collect_used_from_value(ptr, used);
        }
        IRInstr::Call { args, .. } => {
            for arg in args {
                collect_used_from_value(arg, used);
            }
        }
        IRInstr::CallIndirect { fn_ptr, args, .. } => {
            collect_used_from_value(fn_ptr, used);
            for arg in args {
                collect_used_from_value(arg, used);
            }
        }
        IRInstr::GetPtr { ptr, idx, .. } => {
            collect_used_from_value(ptr, used);
            collect_used_from_value(idx, used);
        }
        IRInstr::TritMin { a, b, .. } => {
            collect_used_from_value(a, used);
            collect_used_from_value(b, used);
        }
        IRInstr::TritMax { a, b, .. } => {
            collect_used_from_value(a, used);
            collect_used_from_value(b, used);
        }
        IRInstr::TritNeg { a, .. } => {
            collect_used_from_value(a, used);
        }
        IRInstr::PrintStr(v) | IRInstr::PrintInt(v) | IRInstr::PrintFloat(v)
        | IRInstr::PrintBool3(v) | IRInstr::PrintTrit(v) => {
            collect_used_from_value(v, used);
        }
        IRInstr::Phi { incoming, .. } => {
            for (val, _label) in incoming {
                collect_used_from_value(val, used);
            }
        }
        IRInstr::Cast { src, .. } => {
            collect_used_from_value(src, used);
        }
        IRInstr::Alloca { .. } => {}
    }
}

fn collect_used_in_terminator(term: &IRTerminator, used: &mut HashSet<String>) {
    match term {
        IRTerminator::Return(Some(ref val)) => {
            collect_used_from_value(val, used);
        }
        IRTerminator::BinBranch { cond, .. } => {
            collect_used_from_value(cond, used);
        }
        IRTerminator::TritBranch { cond, .. } => {
            collect_used_from_value(cond, used);
        }
        _ => {}
    }
}

/// Returns true if the instruction has side effects and must not be removed.
fn is_side_effecting(instr: &IRInstr) -> bool {
    matches!(
        instr,
        IRInstr::Store { .. }
        | IRInstr::Call { .. }
        | IRInstr::CallIndirect { .. }
        | IRInstr::PrintStr(_)
        | IRInstr::PrintInt(_)
        | IRInstr::PrintFloat(_)
        | IRInstr::PrintBool3(_)
        | IRInstr::PrintTrit(_)
    )
}

/// Returns the destination temp name of an instruction, if it defines one.
fn instr_dst_name(instr: &IRInstr) -> Option<&str> {
    match instr {
        IRInstr::BinOp { dst, .. } => Some(&dst.0),
        IRInstr::UnOp { dst, .. } => Some(&dst.0),
        IRInstr::Assign { dst, .. } => Some(&dst.0),
        IRInstr::Alloca { dst, .. } => Some(&dst.0),
        IRInstr::Load { dst, .. } => Some(&dst.0),
        IRInstr::GetPtr { dst, .. } => Some(&dst.0),
        IRInstr::TritMin { dst, .. } => Some(&dst.0),
        IRInstr::TritMax { dst, .. } => Some(&dst.0),
        IRInstr::TritNeg { dst, .. } => Some(&dst.0),
        IRInstr::Phi { dst, .. } => Some(&dst.0),
        IRInstr::Cast { dst, .. } => Some(&dst.0),
        // Call/CallIndirect have optional dst but are side-effecting — handled separately
        _ => None,
    }
}

/// Returns the result type of an instruction for use in replacement nodes.
fn instr_ty(instr: &IRInstr) -> IRType {
    match instr {
        IRInstr::BinOp { ty, .. } => ty.clone(),
        IRInstr::UnOp { ty, .. } => ty.clone(),
        IRInstr::Assign { ty, .. } => ty.clone(),
        IRInstr::Alloca { ty, .. } => IRType::Ptr(Box::new(ty.clone())),
        IRInstr::Load { ty, .. } => ty.clone(),
        IRInstr::GetPtr { ty, .. } => IRType::Ptr(Box::new(ty.clone())),
        IRInstr::TritMin { .. } | IRInstr::TritMax { .. } | IRInstr::TritNeg { .. } => IRType::Trit,
        IRInstr::Phi { ty, .. } => ty.clone(),
        IRInstr::Cast { to_ty, .. } => to_ty.clone(),
        IRInstr::Call { ret_ty, .. } => ret_ty.clone(),
        IRInstr::CallIndirect { ret_ty, .. } => ret_ty.clone(),
        // Store, BinBranch, TritBranch produce no value; fallback I64 is unused
        _ => IRType::I64,
    }
}

// ---------------------------------------------------------------------------
// Pass 4: Ternary Peephole Optimizations
// ---------------------------------------------------------------------------

/// Ternary-specific peephole optimizations:
/// - tnot(tnot(x)) → x
/// - tand(x, +1) → x  (min(x, 1) = x for x in {-1,0,+1})
/// - tor(x, -1) → x   (max(x, -1) = x)
/// - tand(x, -1) → -1  (min(x, -1) = -1)
/// - tor(x, +1) → +1   (max(x, +1) = +1)
fn ternary_peephole(func: &mut IRFunction) {
    // Collect tnot results: temp_name → source_operand
    let mut tnot_map: HashMap<String, String> = HashMap::new();
    for block in &func.blocks {
        for instr in &block.instrs {
            if let IRInstr::TritNeg { dst, a } = instr {
                if let IRValue::Temp(ref src) = a {
                    tnot_map.insert(dst.0.clone(), src.0.clone());
                }
            }
        }
    }

    for block in &mut func.blocks {
        for instr in &mut block.instrs {
            match instr {
                // tnot(tnot(x)) → x
                IRInstr::TritNeg { dst, a } => {
                    if let IRValue::Temp(ref src) = a {
                        if let Some(original) = tnot_map.get(&src.0) {
                            *instr = IRInstr::Assign {
                                dst: IRTemp::new(dst.0.clone()),
                                src: IRValue::Temp(IRTemp::new(original.clone())),
                                ty: IRType::Trit,
                            };
                        }
                    }
                }
                // tand(x, +1) → x, tand(x, -1) → -1
                IRInstr::TritMin { dst, a, b } => {
                    if let IRValue::Const(IRConst::Trit(1)) = b {
                        *instr = IRInstr::Assign {
                            dst: IRTemp::new(dst.0.clone()),
                            src: a.clone(),
                            ty: IRType::Trit,
                        };
                    } else if let IRValue::Const(IRConst::Trit(1)) = a {
                        *instr = IRInstr::Assign {
                            dst: IRTemp::new(dst.0.clone()),
                            src: b.clone(),
                            ty: IRType::Trit,
                        };
                    } else if let IRValue::Const(IRConst::Trit(-1)) = b {
                        *instr = IRInstr::Assign {
                            dst: IRTemp::new(dst.0.clone()),
                            src: IRValue::Const(IRConst::Trit(-1)),
                            ty: IRType::Trit,
                        };
                    } else if let IRValue::Const(IRConst::Trit(-1)) = a {
                        *instr = IRInstr::Assign {
                            dst: IRTemp::new(dst.0.clone()),
                            src: IRValue::Const(IRConst::Trit(-1)),
                            ty: IRType::Trit,
                        };
                    }
                }
                // tor(x, -1) → x, tor(x, +1) → +1
                IRInstr::TritMax { dst, a, b } => {
                    if let IRValue::Const(IRConst::Trit(-1)) = b {
                        *instr = IRInstr::Assign {
                            dst: IRTemp::new(dst.0.clone()),
                            src: a.clone(),
                            ty: IRType::Trit,
                        };
                    } else if let IRValue::Const(IRConst::Trit(-1)) = a {
                        *instr = IRInstr::Assign {
                            dst: IRTemp::new(dst.0.clone()),
                            src: b.clone(),
                            ty: IRType::Trit,
                        };
                    } else if let IRValue::Const(IRConst::Trit(1)) = b {
                        *instr = IRInstr::Assign {
                            dst: IRTemp::new(dst.0.clone()),
                            src: IRValue::Const(IRConst::Trit(1)),
                            ty: IRType::Trit,
                        };
                    } else if let IRValue::Const(IRConst::Trit(1)) = a {
                        *instr = IRInstr::Assign {
                            dst: IRTemp::new(dst.0.clone()),
                            src: IRValue::Const(IRConst::Trit(1)),
                            ty: IRType::Trit,
                        };
                    }
                }
                _ => {}
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Pass 5: Common Subexpression Elimination (CSE)
// ---------------------------------------------------------------------------

/// Within each basic block, if the same pure operation appears twice with
/// identical operands, reuse the first result.
fn common_subexpression_eliminate(func: &mut IRFunction) {
    for block in &mut func.blocks {
        // Map from (op_key) → first_dst_name
        let mut seen: HashMap<String, String> = HashMap::new();
        let mut replacements: Vec<(usize, String)> = Vec::new();

        for (idx, instr) in block.instrs.iter().enumerate() {
            if is_side_effecting(instr) {
                continue;
            }
            if let Some(key) = cse_key(instr) {
                if let Some(prev_dst) = seen.get(&key) {
                    if let Some(dst_name) = instr_dst_name(instr) {
                        replacements.push((idx, prev_dst.clone()));
                        let _ = dst_name; // suppress
                    }
                } else if let Some(dst_name) = instr_dst_name(instr) {
                    seen.insert(key, dst_name.to_string());
                }
            }
        }

        // Apply replacements
        for (idx, prev_dst) in replacements.into_iter().rev() {
            let ty = instr_ty(&block.instrs[idx]);
            if let Some(dst_name) = instr_dst_name(&block.instrs[idx]).map(|s| s.to_string()) {
                block.instrs[idx] = IRInstr::Assign {
                    dst: IRTemp::new(dst_name),
                    src: IRValue::Temp(IRTemp::new(prev_dst)),
                    ty,
                };
            }
        }
    }
}

/// Generate a canonical key for an instruction for CSE.
fn cse_key(instr: &IRInstr) -> Option<String> {
    match instr {
        IRInstr::BinOp { op, lhs, rhs, .. } => {
            Some(format!("binop:{:?}:{:?}:{:?}", op, lhs, rhs))
        }
        IRInstr::UnOp { op, operand, .. } => {
            Some(format!("unop:{:?}:{:?}", op, operand))
        }
        IRInstr::TritMin { a, b, .. } => Some(format!("tmin:{:?}:{:?}", a, b)),
        IRInstr::TritMax { a, b, .. } => Some(format!("tmax:{:?}:{:?}", a, b)),
        IRInstr::TritNeg { a, .. } => Some(format!("tneg:{:?}", a)),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Pass 6: Strength Reduction
// ---------------------------------------------------------------------------

/// Replace expensive operations with cheaper equivalents:
/// - x * 0 → 0
/// - x * 1 → x
/// - x + 0 → x
/// - x - 0 → x
fn strength_reduce(func: &mut IRFunction) {
    for block in &mut func.blocks {
        for instr in &mut block.instrs {
            if let IRInstr::BinOp { dst, op, lhs, rhs, ty } = instr {
                let replacement = match op {
                    // x * 0 → 0
                    IRBinOp::Mul if matches!(rhs, IRValue::Const(IRConst::Int(0))) => {
                        Some(IRValue::Const(IRConst::Int(0)))
                    }
                    IRBinOp::Mul if matches!(lhs, IRValue::Const(IRConst::Int(0))) => {
                        Some(IRValue::Const(IRConst::Int(0)))
                    }
                    // x * 1 → x
                    IRBinOp::Mul if matches!(rhs, IRValue::Const(IRConst::Int(1))) => {
                        Some(lhs.clone())
                    }
                    IRBinOp::Mul if matches!(lhs, IRValue::Const(IRConst::Int(1))) => {
                        Some(rhs.clone())
                    }
                    // x + 0 → x
                    IRBinOp::Add if matches!(rhs, IRValue::Const(IRConst::Int(0))) => {
                        Some(lhs.clone())
                    }
                    IRBinOp::Add if matches!(lhs, IRValue::Const(IRConst::Int(0))) => {
                        Some(rhs.clone())
                    }
                    // x - 0 → x
                    IRBinOp::Sub if matches!(rhs, IRValue::Const(IRConst::Int(0))) => {
                        Some(lhs.clone())
                    }
                    _ => None,
                };
                if let Some(val) = replacement {
                    *instr = IRInstr::Assign {
                        dst: IRTemp::new(dst.0.clone()),
                        src: val,
                        ty: ty.clone(),
                    };
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Pass 7: Dead Block Elimination
// ---------------------------------------------------------------------------

/// Remove unreachable blocks. Liveness is a fixpoint computed from the entry
/// block, so dead cycles (blocks that only reference each other) go too.
/// A block is live if:
///   - the entry block reaches it through terminator edges, or
///   - a live block contains a PHI naming it as a predecessor (such blocks
///     must be kept — removing them would leave the PHI referring to a
///     deleted predecessor, which the LLVM backend rejects).
fn dead_block_eliminate(module: &mut IRModule) {
    for func in &mut module.functions {
        if func.is_extern || func.blocks.len() <= 1 {
            continue;
        }

        let index: HashMap<&str, usize> = func.blocks.iter().enumerate()
            .map(|(i, b)| (b.label.as_str(), i))
            .collect();

        let mut live: HashSet<String> = HashSet::new();
        let mut worklist: Vec<String> = vec![func.blocks[0].label.clone()];

        while let Some(label) = worklist.pop() {
            if !live.insert(label.clone()) {
                continue;
            }
            let Some(&i) = index.get(label.as_str()) else { continue };
            let block = &func.blocks[i];
            match &block.term {
                IRTerminator::Jump(target) => {
                    worklist.push(target.clone());
                }
                IRTerminator::BinBranch { true_label, false_label, .. } => {
                    worklist.push(true_label.clone());
                    worklist.push(false_label.clone());
                }
                IRTerminator::TritBranch { pos_label, zero_label, neg_label, .. } => {
                    worklist.push(pos_label.clone());
                    worklist.push(zero_label.clone());
                    worklist.push(neg_label.clone());
                }
                _ => {}
            }
            for instr in &block.instrs {
                if let IRInstr::Phi { incoming, .. } = instr {
                    for (_, pred_label) in incoming {
                        worklist.push(pred_label.clone());
                    }
                }
            }
        }

        func.blocks.retain(|block| live.contains(&block.label));
    }
}
