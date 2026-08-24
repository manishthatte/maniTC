use std::collections::{HashMap, HashSet};
use super::types::*;

/// Which optional passes to run.
#[derive(Debug, Clone, Copy)]
pub struct PassOptions {
    /// F-1: split critical edges, then lift local variables out of memory into
    /// SSA values.
    ///
    /// **On by default since F-3 landed.** It removes about half of the IR —
    /// 42,008 of 79,953 instructions across the shipped examples are loads and
    /// stores of locals, and 8,660 of 9,455 allocas (91.5 %) are promotable —
    /// and until those locals are values rather than memory, none of the passes
    /// below can see them at all.
    ///
    /// It was off by default for as long as the T3 register allocator could not
    /// survive the volume of phi nodes promotion produces: at its worst, 14 of
    /// 17 examples ran and 9 of 17 agreed with LLVM. That allocator was
    /// rewritten (F-3) and the phi defects behind those numbers fixed
    /// (report.txt P11, P12, P14, P16); it is now 17/17 running and 17/17
    /// agreeing, on both language versions.
    ///
    /// Turn it OFF with `--no-mem2reg`. That is the switch that reproduces the
    /// pre-F-1 compiler's output byte for byte, which is what dates a defect as
    /// pre-existing rather than newly introduced — keep it working.
    pub mem2reg: bool,

    /// Print how many instructions each pass removes, to stderr (F-2).
    ///
    /// The point is not curiosity. Every pass below reasons about TEMPS, and
    /// until `mem2reg` ran by default a local variable lived in an alloca and
    /// was invisible to all of them — so their measured effect was on IR they
    /// could barely see through. This is what says whether the existing passes
    /// have become worth more, and where the remaining headroom is, before any
    /// new pass is written.
    pub pass_stats: bool,

    /// How many times to run the per-function passes over each function (F-2).
    ///
    /// The pipeline ran each pass exactly ONCE, in a fixed order, so no pass
    /// ever saw what its neighbours produced — propagation exposes constants
    /// for folding, folding exposes identical expressions for CSE, CSE exposes
    /// dead operands for DCE. Iterating to a fixpoint is the cheapest way to
    /// find out whether the three passes that measured at ~zero (report.txt
    /// P22) are broken, mis-ordered, or genuinely inapplicable.
    ///
    /// Bounded rather than a true fixpoint: a pair of passes that undo each
    /// other would spin, and a bound turns that into a slow compile instead of
    /// a hang. The loop stops early when a round changes nothing.
    pub rounds: usize,
}

impl Default for PassOptions {
    fn default() -> Self {
        Self { mem2reg: true, pass_stats: false, rounds: 1 }
    }
}

/// Instructions in one function, terminators excluded — every pass here edits
/// the instruction lists, and a terminator is not one of them.
fn count_func(func: &IRFunction) -> i64 {
    func.blocks.iter().map(|b| b.instrs.len() as i64).sum()
}

/// A positional snapshot of every instruction, for the REWRITE count.
///
/// Counting instructions alone cannot see four of the six passes. Constant
/// folding turns a `BinOp` into an `Assign` — same count. Propagation and
/// strength reduction rewrite OPERANDS in place — same count. Reporting "0"
/// for those would say they do nothing, when what it actually says is that the
/// instrument is blind to them.
fn snapshot(func: &IRFunction) -> Vec<String> {
    func.blocks
        .iter()
        .flat_map(|b| b.instrs.iter().map(|i| format!("{:?}", i)))
        .collect()
}

/// How many instructions a pass REWROTE without removing.
///
/// Only meaningful when the pass did not change the length; when it did, the
/// removal count is the honest number and this returns None rather than
/// pretending a positional comparison means something.
fn rewritten(before: &[String], after: &[String]) -> Option<i64> {
    if before.len() != after.len() {
        return None;
    }
    Some(before.iter().zip(after).filter(|(a, b)| a != b).count() as i64)
}

/// The same across a whole module, skipping externs, which have no body.
fn count_module(module: &IRModule) -> i64 {
    module.functions.iter().filter(|f| !f.is_extern).map(count_func).sum()
}

/// Run the optimisation passes with the defaults.
pub fn run_passes(module: &mut IRModule) {
    run_passes_with(module, PassOptions::default())
}

/// Run all optimization passes on every non-extern function in the module.
///
/// `mem2reg` (F-1) runs FIRST when enabled, and the ordering is the point of
/// it. Every other pass here reasons about temps: constant propagation keys a
/// `HashMap` on temp names, CSE keys on operand names, strength reduction
/// matches on a constant operand. A local variable in an `Alloca` is invisible
/// to all of them, because its value is in memory and only its address is a
/// temp. Lifting it out first is what gives the passes below something to see.
///
/// `dead_block_eliminate` runs first as well as last. `mem2reg` skips any
/// variable used in an unreachable block — it would otherwise leave a load
/// referring to an alloca it had deleted — so removing those blocks before it
/// runs is what keeps that conservatism from costing anything in practice.
pub fn run_passes_with(module: &mut IRModule, opts: PassOptions) {
    let start = if opts.pass_stats { count_module(module) } else { 0 };
    let mut module_marks: Vec<(&str, i64)> = Vec::new();

    dead_block_eliminate(module);
    if opts.pass_stats {
        module_marks.push(("dead-block-eliminate", count_module(module)));
    }
    if opts.mem2reg {
        // A phi's value has to be placed ON the edge it came from, and a
        // critical edge has nowhere to put it. Split before promoting, so every
        // phi `mem2reg` inserts already has a predecessor that ends in a plain
        // jump — which is the only shape the T3 backend emits copies for.
        super::ssa::split_critical_edges_module(module);
        if opts.pass_stats {
            module_marks.push(("split-critical-edges", count_module(module)));
        }
        super::mem2reg::run(module);
        if opts.pass_stats {
            module_marks.push(("mem2reg", count_module(module)));
        }
    }

    // The per-function passes are attributed by DELTA rather than by running
    // each one over the whole module in turn: they are independent per
    // function, so accumulating differences preserves the order each function
    // actually sees while still giving a per-pass total.
    const FN_PASSES: [&str; 6] = [
        "constant-fold",
        "constant-propagate",
        "ternary-peephole",
        "common-subexpression",
        "strength-reduce",
        "dead-code-eliminate",
    ];
    let mut removed = [0i64; 6];
    let mut rewrote = [0i64; 6];
    for func in &mut module.functions {
        if func.is_extern {
            continue;
        }
        let mut n = if opts.pass_stats { count_func(func) } else { 0 };
        let mut snap = if opts.pass_stats { snapshot(func) } else { Vec::new() };
        // Round 0 is the pipeline as it always was; `rounds` above 1 repeats it
        // until a round changes nothing.
        //
        // The snapshot is taken ONLY when iterating. It Debug-formats every
        // instruction in the function, so doing it unconditionally would put
        // that cost on every compile to support a flag that is off by default.
        let iterating = opts.rounds.max(1) > 1;
        let mut round_start = if iterating { snapshot(func) } else { Vec::new() };
        macro_rules! step {
            ($i:expr, $call:expr) => {{
                $call;
                if opts.pass_stats {
                    let m = count_func(func);
                    removed[$i] += n - m;
                    n = m;
                    let after = snapshot(func);
                    if let Some(r) = rewritten(&snap, &after) {
                        rewrote[$i] += r;
                    }
                    snap = after;
                }
            }};
        }
        for round in 0..opts.rounds.max(1) {
            if iterating && round > 0 {
                round_start = snapshot(func);
            }
            step!(0, constant_fold(func));
            step!(1, constant_propagate(func));
            step!(2, ternary_peephole(func));
            step!(3, common_subexpression_eliminate(func));
            step!(4, strength_reduce(func));
            step!(5, dead_code_eliminate(func));
            if !iterating || snapshot(func) == round_start {
                break; // one round only, or converged
            }
        }
    }
    dead_block_eliminate(module);

    if opts.pass_stats {
        let end = count_module(module);
        let mut running = start;
        eprintln!("pass-stats  {:<22} {:>9}", "after lowering", start);
        for (name, n) in &module_marks {
            eprintln!("pass-stats  {:<22} {:+9}  -> {}", name, n - running, n);
            running = *n;
        }
        for (i, name) in FN_PASSES.iter().enumerate() {
            running -= removed[i];
            eprintln!(
                "pass-stats  {:<22} {:+9}  -> {:<8} rewrote {}",
                name, -removed[i], running, rewrote[i]
            );
        }
        eprintln!("pass-stats  {:<22} {:+9}  -> {}", "final dead-block", end - running, end);
        eprintln!(
            "pass-stats  {:<22} {} -> {}  ({:+.1} %)",
            "TOTAL", start, end,
            if start > 0 { (end - start) as f64 * 100.0 / start as f64 } else { 0.0 }
        );
    }
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

/// N5: a folded result, or `None` when it does not fit a 27-trit word.
fn fold_in_word(v: i128) -> Option<IRConst> {
    if v > crate::lang::T27_MAX as i128 || v < crate::lang::T27_MIN as i128 {
        None
    } else {
        Some(IRConst::Int(v as i64))
    }
}

fn fold_int_binop(op: &IRBinOp, a: i64, b: i64) -> Option<IRConst> {
    match op {
        IRBinOp::Add => Some(IRConst::Int(a.wrapping_add(b))),
        IRBinOp::Sub => Some(IRConst::Int(a.wrapping_sub(b))),
        IRBinOp::Mul => Some(IRConst::Int(a.wrapping_mul(b))),
        // N5. Folded only when the answer FITS. An out-of-range constant
        // expression must reach the backend and trap there, exactly as the
        // same expression would if its operands were variables — folding it to
        // a wrapped value would turn a fault into a wrong answer, which is the
        // defect `checked27` replaced clamping to fix. Returning None leaves
        // the instruction in place and the guard runs.
        IRBinOp::AddT27 => fold_in_word((a as i128) + (b as i128)),
        IRBinOp::SubT27 => fold_in_word((a as i128) - (b as i128)),
        IRBinOp::MulT27 => fold_in_word((a as i128) * (b as i128)),
        IRBinOp::Div => {
            if b == 0 { None } else { Some(IRConst::Int(a.wrapping_div(b))) }
        }
        IRBinOp::Rem => {
            if b == 0 { None } else { Some(IRConst::Int(a.wrapping_rem(b))) }
        }
        // C4. Folded through the same `lang::div_nearest` the emulator and the
        // LLVM sequence are defined by, so a constant-folded division and a
        // run-time one cannot answer differently. A second implementation of
        // the rounding rule here would be a second thing to get wrong, and the
        // difference would only ever show up as a program whose behaviour
        // depended on whether its operands happened to be constants.
        IRBinOp::DivNear => {
            if b == 0 { None } else { Some(IRConst::Int(crate::lang::div_nearest(a, b))) }
        }
        IRBinOp::RemNear => {
            if b == 0 { None } else { Some(IRConst::Int(crate::lang::rem_balanced(a, b))) }
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
        IRInstr::TritLane { a, b, .. } => {
            propagate_value(a, const_map);
            propagate_value(b, const_map);
        }
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
        IRInstr::BoundsCheck { idx, .. } => {
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
        IRInstr::TritNeg { a, .. } | IRInstr::TritSign { a, .. } => {
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
        IRInstr::TritLane { a, b, .. } => {
            collect_used_from_value(a, used);
            collect_used_from_value(b, used);
        }
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
        IRInstr::BoundsCheck { idx, .. } => {
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
        IRInstr::TritNeg { a, .. } | IRInstr::TritSign { a, .. } => {
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
        IRInstr::TritSign { dst, .. } => Some(&dst.0),
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
        IRInstr::TritMin { .. } | IRInstr::TritMax { .. } | IRInstr::TritNeg { .. }
        // C7: the OPERAND is a word, but the RESULT is a trit -- it is always
        // in {-1, 0, +1}. Typing the result is what this function does.
        | IRInstr::TritSign { .. } => IRType::Trit,
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
        IRInstr::TritSign { a, .. } => Some(format!("tsign:{:?}", a)),
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
/// The shift amount k when `op` is reducible to a ternary shift by 3^k.
///
/// Only `Mul` and `DivNear` — see the note at the call site for why `MulT27`
/// and `Div` are excluded. k >= 1, so `x * 1` stays for the identity rule
/// above; k <= 26, the widest shift a 27-trit word can take.
fn pow3_shift(op: &IRBinOp, rhs: &IRValue) -> Option<i64> {
    if !matches!(op, IRBinOp::Mul | IRBinOp::DivNear) {
        return None;
    }
    let IRValue::Const(IRConst::Int(v)) = rhs else { return None };
    if *v < 3 {
        return None;
    }
    let mut n = *v;
    let mut k = 0i64;
    while n % 3 == 0 {
        n /= 3;
        k += 1;
    }
    (n == 1 && (1..=26).contains(&k)).then_some(k)
}

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

                // F-2: the reduction this pass is NAMED for, which it did not
                // do. Everything above is an algebraic identity needing a
                // literal `* 1` or `+ 0` that nobody writes; measured across
                // the 17 examples this pass fired ZERO times (report.txt P22).
                //
                // On a balanced-ternary machine the reduction that pays is
                // multiply and divide by a power of THREE, and T3ISA has
                // `TSHI`/`TSHR` for exactly that — one instruction. 118 of
                // 1,708 multiplies in the examples are by a power of three,
                // and `ternary_sort` emitted 33 `TMUL` and no `TSHI` at all.
                //
                // WHICH OPERATIONS, and the two exclusions are the whole
                // correctness argument:
                //
                //   Mul      -> TShl.  `TSHI` traps on 27-trit overflow via
                //                      `checked27` exactly as `TMUL` does, and
                //                      on LLVM both are a wrapping `mul i64`.
                //   DivNear  -> TShr.  Dropping k low trits IS round-to-nearest
                //                      division by 3^k.
                //   MulT27   -> NO.    On LLVM it emits N5's overflow guard;
                //                      dropping that would silently weaken
                //                      `--lang v2`.
                //   Div      -> NO.    `Div` TRUNCATES and `TSHR` ROUNDS. They
                //                      differ for every negative operand that
                //                      does not divide exactly: -5/3 is -1
                //                      truncating and -2 rounding.
                if replacement.is_none() {
                    if let Some(k) = pow3_shift(op, rhs) {
                        *instr = IRInstr::BinOp {
                            dst: IRTemp::new(dst.0.clone()),
                            op: if matches!(op, IRBinOp::Mul) {
                                IRBinOp::TShl
                            } else {
                                IRBinOp::TShr
                            },
                            lhs: lhs.clone(),
                            rhs: IRValue::Const(IRConst::Int(k)),
                            ty: ty.clone(),
                        };
                        continue;
                    }
                }
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
