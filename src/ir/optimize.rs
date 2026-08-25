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
    ///
    /// It is what found that three of the six did essentially nothing, which
    /// is what F-2 has been repairing since: `strength_reduce` fired ZERO
    /// times across all 17 examples, `common_subexpression_eliminate` three
    /// times, and `ternary_peephole` twice. Both numbers it prints are needed
    /// — counting instructions alone cannot see four of the six passes,
    /// because folding turns a `BinOp` into an `Assign` and propagation
    /// rewrites operands in place, so a removal count of 0 there says the
    /// instrument is blind rather than that the pass is idle.
    pub pass_stats: bool,

    /// How many times to run the per-function passes over each function (F-2).
    ///
    /// The pipeline ran each pass exactly ONCE, in a fixed order, so no pass
    /// ever saw what its neighbours produced — propagation exposes constants
    /// for folding, folding exposes identical expressions for CSE, CSE exposes
    /// dead operands for DCE. Iterating to a fixpoint was the cheapest way to
    /// ask whether the three passes that measured at ~zero (report.txt P22)
    /// were broken, mis-ordered, or genuinely inapplicable — and the ANSWER IS
    /// THAT THEY WERE NOT MIS-ORDERED. Across the 17 examples `--rounds 5`
    /// removes 5 instructions of 40,942, one hundredth of a percent, and it
    /// was 4 of 41,306 before the two repairs below. Each of the three turned
    /// out to be a different thing: `strength_reduce` did no strength
    /// reduction, `common_subexpression_eliminate` was scoped to a basic block
    /// in an IR whose blocks average 2.78 instructions, and `ternary_peephole`
    /// was matching literal trit operands nobody writes.
    ///
    /// The flag stays, because it is the instrument that says so. Bounded
    /// rather than a true fixpoint: a pair of passes that undo each other
    /// would spin, and a bound turns that into a slow compile instead of a
    /// hang. The loop stops early when a round changes nothing, and the
    /// snapshot it needs is taken only when N > 1, so the default path pays
    /// nothing.
    pub rounds: usize,

    /// F-2: the largest single-block callee, in IR instructions, that
    /// `inline::run_with` will splice into its callers. `0` turns the pass off.
    ///
    /// A size limit is the whole of the heuristic the recommendations ask for.
    /// Splicing an N-instruction body at S sites adds `S * (N - 1)`
    /// instructions net of the calls it removes, so the cost grows with the
    /// limit and the benefit — one call frame per site — does not.
    pub inline_limit: usize,

    /// P26: merge a block into its single predecessor, to a fixpoint.
    ///
    /// 35.9 % of blocks in this IR are empty and 27.7 % are empty with a plain
    /// `Jump`, which is why the mean block is 2.78 instructions — and a block
    /// scoped pass in an IR shaped like that has nowhere to look.
    ///
    /// Turn it OFF with `--no-merge-blocks`, which exists so the pass can be
    /// measured against itself on the same binary.
    pub merge_blocks: bool,
}

impl Default for PassOptions {
    fn default() -> Self {
        Self {
            mem2reg: true,
            pass_stats: false,
            rounds: 1,
            inline_limit: super::inline::SIZE_LIMIT,
            merge_blocks: true,
        }
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

    // F-2. AFTER promotion and BEFORE the per-function passes, and both halves
    // of that are deliberate.
    //
    // After `mem2reg`, because a callee body still living in allocas is twice
    // the size and mostly loads and stores — a size limit measured against it
    // would be measuring memory traffic rather than work, and it would admit
    // far fewer of the small wrappers this pass exists for.
    //
    // Before constant folding and propagation, because that is where the win
    // compounds: once a body is spliced, its parameters ARE the caller's
    // actual arguments, and a constant argument makes the whole body foldable.
    // Inlining last would leave every one of those unspecialised.
    let spliced = super::inline::run_with(module, opts.inline_limit);
    if opts.pass_stats {
        eprintln!("pass-stats  {:<22} {:>9}  call sites", "inline (sites)", spliced);
        module_marks.push(("inline", count_module(module)));
    }

    // P26. AFTER `split_critical_edges`, and that ordering is safe rather than
    // lucky: the shapes the two passes act on are disjoint. Splitting inserts a
    // block on an edge whose PREDECESSOR branches, and merging requires a
    // predecessor that ends in a plain `Jump` — see the module header.
    //
    // It removes no instructions, so `count_module` will not show it. What it
    // changes is the SHAPE the six passes below see: two-thirds of blocks were
    // too short to hold a redundancy at all, and every one of those passes but
    // the re-scoped CSE still reasons one block at a time.
    let merged = if opts.merge_blocks { super::merge_blocks::run(module) } else { 0 };
    if opts.pass_stats {
        eprintln!("pass-stats  {:<22} {:>9}  blocks", "merge-blocks", merged);
        module_marks.push(("merge-blocks", count_module(module)));
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

/// Every operand of an instruction, mutably.
///
/// ONE exhaustive match rather than one per rewriting pass. Both passes that
/// substitute into operands — constant propagation and CSE — used to carry
/// their own, and the only thing that distinguished them was `Phi`; two
/// exhaustive matches that differ in one arm are two matches that drift, and
/// the arm that goes missing is a temp that keeps a stale name after its
/// definition is gone.
///
/// `include_phi` is that difference, made explicit. Constant propagation says
/// no: a phi operand belongs to its incoming EDGE, and replacing it with a
/// constant states that the value is the same however the block was reached.
/// CSE says yes: it renames a temp to one that reaches every use of it, and a
/// phi that still named the old one would outlive its definition.
pub(super) fn for_each_operand_mut(
    instr: &mut IRInstr,
    include_phi: bool,
    mut f: impl FnMut(&mut IRValue),
) {
    match instr {
        IRInstr::TritLane { a, b, .. } => {
            f(a);
            f(b);
        }
        IRInstr::BinOp { lhs, rhs, .. } => {
            f(lhs);
            f(rhs);
        }
        IRInstr::UnOp { operand, .. } => f(operand),
        IRInstr::Assign { src, .. } => f(src),
        IRInstr::Store { ptr, val, .. } => {
            f(ptr);
            f(val);
        }
        IRInstr::Load { ptr, .. } => f(ptr),
        IRInstr::Call { args, .. } => {
            for arg in args.iter_mut() {
                f(arg);
            }
        }
        IRInstr::CallIndirect { fn_ptr, args, .. } => {
            f(fn_ptr);
            for arg in args.iter_mut() {
                f(arg);
            }
        }
        IRInstr::GetPtr { ptr, idx, .. } => {
            f(ptr);
            f(idx);
        }
        IRInstr::BoundsCheck { idx, .. } => f(idx),
        IRInstr::TritMin { a, b, .. } => {
            f(a);
            f(b);
        }
        IRInstr::TritMax { a, b, .. } => {
            f(a);
            f(b);
        }
        IRInstr::TritNeg { a, .. } | IRInstr::TritSign { a, .. } => f(a),
        IRInstr::PrintStr(v) | IRInstr::PrintInt(v) | IRInstr::PrintFloat(v)
        | IRInstr::PrintBool3(v) | IRInstr::PrintTrit(v) => f(v),
        IRInstr::Cast { src, .. } => f(src),
        IRInstr::Phi { incoming, .. } => {
            if include_phi {
                for (val, _) in incoming.iter_mut() {
                    f(val);
                }
            }
        }
        IRInstr::Alloca { .. } => {}
    }
}

fn propagate_in_instruction(instr: &mut IRInstr, const_map: &HashMap<String, IRConst>) {
    // Not into a Phi: its operands differ per incoming edge.
    for_each_operand_mut(instr, false, |v| propagate_value(v, const_map));
}

/// Every operand of a terminator, mutably. The companion to
/// `for_each_operand_mut`, and shared for the same reason.
///
/// It is `pub(super)` because the inliner needs it too, and needed it before
/// it asked: a terminator has OPERANDS as well as labels, and a pass that
/// copies a body has to rename both (report.txt P34).
pub(super) fn for_each_term_operand_mut(term: &mut IRTerminator, mut f: impl FnMut(&mut IRValue)) {
    match term {
        IRTerminator::Return(Some(val)) => f(val),
        IRTerminator::BinBranch { cond, .. } => f(cond),
        IRTerminator::TritBranch { cond, .. } => f(cond),
        IRTerminator::Return(None) | IRTerminator::Jump(_) | IRTerminator::Unreachable => {}
    }
}

fn propagate_in_terminator(term: &mut IRTerminator, const_map: &HashMap<String, IRConst>) {
    for_each_term_operand_mut(term, |v| propagate_value(v, const_map));
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
        // C2's lane-wise word operation. It was missing here, which made it
        // invisible to three things at once: dead-code elimination never
        // removed a dead one, CSE could not rewrite one, and
        // `is_single_assignment` could not see a temp it defined twice. It is
        // pure — `manit_lanewise2` in `runtime/core.c` is arithmetic on two
        // words — so being listed is safe as well as correct.
        IRInstr::TritLane { dst, .. } => Some(&dst.0),
        IRInstr::Phi { dst, .. } => Some(&dst.0),
        IRInstr::Cast { dst, .. } => Some(&dst.0),
        // Call/CallIndirect have optional dst but are side-effecting — handled separately
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Pass 4: Ternary Peephole Optimizations
// ---------------------------------------------------------------------------

/// Ternary-specific peephole optimisations, in two parts.
///
/// The identities came first and are what the pass used to be in full.
/// Measured across the 17 examples they rewrite TWO instructions, in one
/// program (report.txt P22), and the reason is the same one that made
/// `strength_reduce` fire zero times: every rule needs a literal `+`/`-` trit
/// operand that nobody writes, and where both sides are literal
/// `constant_fold` has already taken it. Their ceiling is negligible in any
/// case — `TritMin`, `TritMax` and `TritNeg` together are 149 of the 41,306
/// instructions the examples reduce to, 0.36 %.
///
/// The trichotomy collapse is the one that pays, and it is ternary in the way
/// the ternary strength reduction is ternary: **the machine has the
/// instruction and the compiler never emitted it.** See
/// `collapse_sign_trichotomy`.
fn ternary_peephole(func: &mut IRFunction) {
    trit_identities(func);
    if collapse_sign_trichotomy(func) > 0 {
        // The absorbed block is unreachable now. Drop it BEFORE asking which
        // edges are critical, or a dead predecessor makes a target look like a
        // join and buys an empty block nothing ever jumps to.
        dead_block_eliminate_func(func);
        // A block that had two successors now has three, so an edge into a
        // join point that was not critical may have become one. The T3 backend
        // places phi copies only on a predecessor that ends in a plain jump
        // (report.txt P12), and a critical edge has nowhere to put them.
        super::ssa::split_critical_edges(func);
    }
}

/// - tnot(tnot(x)) → x
/// - tand(x, +1) → x  (min(x, 1) = x for x in {-1,0,+1})
/// - tor(x, -1) → x   (max(x, -1) = x)
/// - tand(x, -1) → -1  (min(x, -1) = -1)
/// - tor(x, +1) → +1   (max(x, +1) = +1)
fn trit_identities(func: &mut IRFunction) {
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

/// Which arm of a three-way branch a comparison against zero selects when it
/// is TRUE.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SignArm {
    Pos,
    Zero,
    Neg,
}

/// `x <op> 0` or `0 <op> x`, as the arm it selects — or `None`.
///
/// Only the three STRICT comparisons, because only they name exactly one arm.
/// `>=`, `<=` and `!=` each cover two of the three, so a second comparison
/// does not divide what is left into the remaining one plus one.
///
/// INTEGER only, and that exclusion is P20 seen from the other side. A NaN
/// answers false to `<`, to `>` and to `==` alike: it belongs in a fourth arm,
/// and `TritBranch` has three. Collapsing a float comparison here would put
/// every NaN down whichever arm the sign of its bit pattern happened to pick —
/// which is exactly the wrong answer P20 fixed. 49 of the 107 sites measured
/// across the examples are float, and they stay as they are.
///
/// The operand must be a word (`I64`). `TritSign` is defined on a word, and a
/// narrower or `Bool`-typed operand reaches it through the backends' own
/// widening rules — `i1` zero-extends where everything else sign-extends —
/// which is a second question this transform does not need to open.
fn sign_test(instr: &IRInstr, types: &HashMap<String, IRType>) -> Option<(IRValue, SignArm)> {
    let IRInstr::BinOp { op, lhs, rhs, .. } = instr else { return None };
    let zero = |v: &IRValue| matches!(v, IRValue::Const(IRConst::Int(0)));
    let is_word = |v: &IRValue| match v {
        IRValue::Const(IRConst::Int(_)) => true,
        IRValue::Temp(t) => matches!(types.get(&t.0), Some(IRType::I64)),
        _ => false,
    };

    // (the operand that is not the zero, which side it was on)
    let (x, x_on_left) = if zero(rhs) {
        (lhs.clone(), true)
    } else if zero(lhs) {
        (rhs.clone(), false)
    } else {
        return None;
    };
    if !is_word(&x) {
        return None;
    }

    let arm = match (op, x_on_left) {
        (IRBinOp::IGt, true) | (IRBinOp::ILt, false) => SignArm::Pos,
        (IRBinOp::ILt, true) | (IRBinOp::IGt, false) => SignArm::Neg,
        (IRBinOp::IEq, _) => SignArm::Zero,
        _ => return None,
    };
    Some((x, arm))
}

/// Collapse `if x > 0 … else if x < 0 … else …` into ONE three-way branch.
///
/// T3ISA has `TCMP Rd, Ra, R0`, which puts sign(Ra) ∈ {−1, 0, +1} in a
/// register in one instruction, and `TBRANCH`, which branches three ways on
/// it. The IR has both — `TritSign` (C7) and `TritBranch`. What it did not
/// have was anything that PRODUCED that pair from ordinary source: a
/// `TritBranch` only ever appeared where the programmer wrote a `tif`, and the
/// sign trichotomy — which is how a balanced-ternary standard library is
/// naturally written — lowered to two two-way comparisons and two two-way
/// branches. Measured over the 17 examples:
///
/// | operand pairs carrying 2 or 3 distinct comparisons | 147, in 14 of 17 files |
/// | of those, all three | 30 |
/// | collapsible as CONTROL FLOW, not just as expressions | 111, in 13 files |
/// | of those, against a literal zero | 107 |
/// | of those, integer rather than float | 58 |
///
/// and the functions they are in are `ternary::int_to_trit`,
/// `ternary::trit_mul`, `bridge::trit_to_bits`, `fmt::digit_glyph` — the
/// ternary primitives of the standard library itself.
///
/// The five conditions below are each a correctness argument, not a
/// convenience:
///
/// 1. the two comparisons name DIFFERENT arms of the same `x`;
/// 2. the second block holds NOTHING but the comparison that feeds its
///    branch, or the collapse would skip whatever else is in it;
/// 3. the second block is reachable ONLY through this edge, so that bypassing
///    it makes it dead rather than leaving a second way in whose phis would
///    then need a value from a block that never computed one;
/// 4. the three targets are DISTINCT, so no phi ends up with two entries for
///    the same predecessor;
/// 5. the comparison is on a WORD and not a float — see `sign_test`.
///
/// Returns how many sites were collapsed.
fn collapse_sign_trichotomy(func: &mut IRFunction) -> usize {
    // Same soundness condition as the CSE below, for the same reason: this
    // reads a branch's condition back to the instruction that DEFINED it, and
    // a temp defined twice would resolve to whichever definition came last in
    // the block list rather than the one that actually reaches the branch.
    if func.blocks.len() < 2 || !is_single_assignment(func) {
        return 0;
    }
    let types = super::ssa::value_types(func);
    let cfg = super::ssa::Cfg::of(func);

    // Every temp's defining instruction. The comparison need not sit in the
    // branching block: SSA guarantees a definition dominates its use, so if
    // the branch can read `c1` here then so can this.
    let mut def_of: HashMap<&str, &IRInstr> = HashMap::new();
    for block in &func.blocks {
        for instr in &block.instrs {
            if let Some(d) = instr_dst_name(instr) {
                def_of.insert(d, instr);
            }
        }
    }

    struct Plan {
        at: usize,
        x: IRValue,
        pos: String,
        zero: String,
        neg: String,
        /// The block whose branch was absorbed; phis in the targets name it.
        absorbed: String,
    }

    let sign_of = |v: &IRValue| -> Option<(IRValue, SignArm)> {
        let IRValue::Temp(t) = v else { return None };
        sign_test(def_of.get(t.0.as_str())?, &types)
    };

    let mut plans: Vec<Plan> = Vec::new();
    let mut claimed: HashSet<usize> = HashSet::new();

    for b in 0..func.blocks.len() {
        let IRTerminator::BinBranch { cond, true_label, false_label } = &func.blocks[b].term
        else {
            continue;
        };
        let Some((x1, arm1)) = sign_of(cond) else { continue };

        // Follow the false edge through empty blocks that only jump, and
        // require every block on the way to have this as its ONLY way in.
        let mut cur = match cfg.index.get(false_label) {
            Some(&i) => i,
            None => continue,
        };
        let mut chain: Vec<usize> = Vec::new();
        let second = loop {
            if cfg.preds[cur].len() != 1 || chain.len() > func.blocks.len() {
                break None;
            }
            let blk = &func.blocks[cur];
            if blk.instrs.is_empty() {
                if let IRTerminator::Jump(next) = &blk.term {
                    chain.push(cur);
                    match cfg.index.get(next) {
                        Some(&i) => {
                            cur = i;
                            continue;
                        }
                        None => break None,
                    }
                }
            }
            break Some(cur);
        };
        let Some(second) = second else { continue };
        if second == b {
            continue;
        }

        let IRTerminator::BinBranch { cond: cond2, true_label: t2, false_label: f2 } =
            &func.blocks[second].term
        else {
            continue;
        };
        let Some((x2, arm2)) = sign_of(cond2) else { continue };
        // Structural equality by Debug, the same way `cse_key` decides two
        // operands are the same one: `IRValue` carries no `PartialEq`.
        if format!("{:?}", x1) != format!("{:?}", x2) || arm1 == arm2 {
            continue;
        }

        // Condition 2: nothing in the second block but the comparison itself.
        let IRValue::Temp(c2) = cond2 else { continue };
        let extra = func.blocks[second]
            .instrs
            .iter()
            .any(|i| instr_dst_name(i) != Some(c2.0.as_str()));
        if extra {
            continue;
        }

        // **Condition 3: the second comparison's RESULT is used ONLY by the
        // branch this collapse absorbs.**
        //
        // Condition 2 establishes that the block holds nothing else. It does
        // NOT establish that nothing else holds the block's VALUE, and those
        // are different questions. Absorbing the block makes it unreachable,
        // so its comparison goes with it — and any other block still reading
        // that temp is left reading a name nothing defines.
        //
        // report.txt P37, and it shipped: `let neg = val < 0;` tested once to
        // normalise the sign and again at the end to put it back is
        // `to_balanced_ternary`, copied into five thatteOS files. Collapsed,
        // it returned the POSITIVE representation for every negative input on
        // T3 — silently, because a free temp gets a register — and would not
        // link at all on LLVM.
        //
        // The FIRST comparison needs no such test. Block `b` KEEPS its
        // instructions; only its terminator is replaced, so its temp survives
        // and dead-code elimination removes it exactly when it really is
        // unused.
        let c2_name = c2.0.as_str();
        let used_elsewhere = func.blocks.iter().enumerate().any(|(i, blk)| {
            blk.instrs.iter().any(|ins| {
                super::ssa::instr_uses(ins).contains(&c2_name)
                    || super::ssa::phi_uses(ins).iter().any(|(t, _)| *t == c2_name)
            }) || (i != second && super::ssa::term_uses(&blk.term).contains(&c2_name))
        });
        if used_elsewhere {
            continue;
        }

        // Condition 4.
        let (t1, t2, f2) = (true_label.clone(), t2.clone(), f2.clone());
        if t1 == t2 || t1 == f2 || t2 == f2 {
            continue;
        }

        // Each comparison names one arm; the third is where both being false
        // leaves you, which is the second branch's own false target.
        let named = [(arm1, t1), (arm2, t2)];
        let pick = |want: SignArm| -> String {
            named
                .iter()
                .find(|(a, _)| *a == want)
                .map(|(_, l)| l.clone())
                .unwrap_or_else(|| f2.clone())
        };
        let (pos, zero, neg) = (pick(SignArm::Pos), pick(SignArm::Zero), pick(SignArm::Neg));

        let mut claims = chain;
        claims.push(second);
        if claims.iter().any(|c| claimed.contains(c)) || claimed.contains(&b) {
            continue;
        }
        claimed.insert(b);
        claimed.extend(claims.iter().copied());

        plans.push(Plan {
            at: b,
            x: x1,
            pos,
            zero,
            neg,
            absorbed: func.blocks[second].label.clone(),
        });
    }

    if plans.is_empty() {
        return 0;
    }

    let prefix = fresh_sign_prefix(func);
    let n = plans.len();
    for (k, plan) in plans.into_iter().enumerate() {
        let s = format!("{}{}", prefix, k);
        let here = func.blocks[plan.at].label.clone();
        func.blocks[plan.at].instrs.push(IRInstr::TritSign {
            dst: IRTemp::new(s.clone()),
            a: plan.x,
        });
        func.blocks[plan.at].term = IRTerminator::TritBranch {
            cond: IRValue::Temp(IRTemp::new(s)),
            pos_label: plan.pos.clone(),
            zero_label: plan.zero.clone(),
            neg_label: plan.neg.clone(),
        };
        // The absorbed block was the predecessor of two of the three targets;
        // it is now unreachable, and this block took its place on those edges.
        for target in [&plan.pos, &plan.zero, &plan.neg] {
            let Some(&ti) = cfg.index.get(target.as_str()) else { continue };
            for instr in &mut func.blocks[ti].instrs {
                if let IRInstr::Phi { incoming, .. } = instr {
                    for (_, label) in incoming.iter_mut() {
                        if *label == plan.absorbed {
                            *label = here.clone();
                        }
                    }
                }
            }
        }
    }
    n
}

/// A temp-name prefix no existing temp in this function uses, for the signs
/// this pass introduces. Checked rather than assumed, for the reason
/// `mem2reg::fresh_prefix` gives: a collision silently merges two values.
fn fresh_sign_prefix(func: &IRFunction) -> String {
    let mut used: HashSet<&str> = HashSet::new();
    for b in &func.blocks {
        for i in &b.instrs {
            if let Some(d) = instr_dst_name(i) {
                used.insert(d);
            }
        }
    }
    let mut prefix = String::from("tsg");
    while used.iter().any(|u| u.starts_with(&prefix)) {
        prefix.push('_');
    }
    prefix
}

// ---------------------------------------------------------------------------
// Pass 5: Common Subexpression Elimination (CSE)
// ---------------------------------------------------------------------------

/// Reuse a pure computation that has already been made on every path here.
///
/// **SCOPE WAS THIS PASS'S WHOLE PROBLEM** (F-2, report.txt P22). It used to
/// look for a repeat only inside one basic block, and measured across the 17
/// examples it fired THREE times in total — for a pass whose entire job is
/// finding redundant computation, on IR of up to 4,000 instructions.
///
/// That is not a pass that is broken and it is not one that is inapplicable.
/// It is one confined to a scope that barely exists in this IR:
///
/// | blocks | 14,857 | mean length **2.78 instructions** |
/// | with 0 or 1 instruction | 9,769 | **65.8 %** — cannot hold a repeat at all |
/// | empty, terminator a plain `Jump` | 4,119 | 27.7 %, `if`/`else` scaffolding |
///
/// A block-local pass on two-and-a-half-instruction blocks has nowhere to
/// look. Scoped to DOMINANCE instead — an expression is available in every
/// block its definition dominates — the same key set finds **186**.
///
/// The key set was the other half. `GetPtr` is 21.0 % of the IR (8,678
/// instructions, the second-largest kind after `BinOp`) and was not keyed at
/// all; with `GetPtr` and `Cast` the count is 238, and with `BoundsCheck` 308.
///
/// WHAT IS DELIBERATELY NOT KEYED, because each would be a wrong answer rather
/// than a missed one:
///
/// - **`Load`** — a `Store` between two loads of the same address makes the
///   second a different value, and there is no alias analysis here to say
///   there was not one.
/// - **`Call`** — the same arguments give the same answer only for a pure
///   function, and nothing in the IR records purity.
/// - **`Alloca`** — two allocas are two distinct cells however identical they
///   look. Collapsing them would alias two variables onto one.
///
/// `ty` is part of every key. The old key omitted it, so two `BinOp`s alike in
/// operator and operands but not in result type were interchangeable; that is
/// a narrowing waiting to be dropped, and it is the same property of the IR
/// that F-1 found in `Load`/`Store` (report.txt, mem2reg).
fn common_subexpression_eliminate(func: &mut IRFunction) {
    if func.blocks.is_empty() || !is_single_assignment(func) {
        return;
    }
    let cfg = super::ssa::Cfg::of(func);
    let dom = super::ssa::Dominators::of(&cfg);
    if dom.rpo.is_empty() {
        return;
    }

    // Children in the dominator tree. Unreachable blocks have no idom and so
    // are never visited — which is right, since a definition in one reaches
    // nothing.
    let mut kids: Vec<Vec<usize>> = vec![Vec::new(); cfg.len()];
    for b in 0..cfg.len() {
        if let Some(p) = dom.idom[b] {
            kids[p].push(b);
        }
    }

    // Pre-order walk of the dominator tree with an undo journal, so a key is
    // available exactly in the subtree its definition dominates and not one
    // block further. `None` is a `BoundsCheck`, which defines no temp — the
    // key alone records that the check has already been made.
    let mut avail: HashMap<String, Option<String>> = HashMap::new();
    let mut rewrites: Vec<(usize, usize, String)> = Vec::new();
    let mut removals: Vec<(usize, usize)> = Vec::new();

    enum Step {
        Visit(usize),
        Undo(Vec<String>),
    }
    // An explicit stack rather than recursion: the dominator tree of a long
    // `else if` chain is as deep as the chain is long, and these are compiler
    // inputs.
    let mut stack = vec![Step::Visit(dom.rpo[0])];
    while let Some(step) = stack.pop() {
        match step {
            Step::Undo(keys) => {
                for k in keys {
                    avail.remove(&k);
                }
            }
            Step::Visit(b) => {
                let mut journal: Vec<String> = Vec::new();
                for (i, instr) in func.blocks[b].instrs.iter().enumerate() {
                    let Some(key) = cse_key(instr) else { continue };
                    match avail.get(&key) {
                        Some(Some(prev)) => rewrites.push((b, i, prev.clone())),
                        Some(None) => removals.push((b, i)),
                        None => {
                            // Only ever inserted when absent, so the undo is a
                            // removal and never a restore.
                            avail.insert(key.clone(), instr_dst_name(instr).map(str::to_string));
                            journal.push(key);
                        }
                    }
                }
                stack.push(Step::Undo(journal));
                for &k in &kids[b] {
                    stack.push(Step::Visit(k));
                }
            }
        }
    }

    // Rewrite the redundant definition's USES rather than leaving a copy
    // behind. This is sound exactly because the IR is SSA and the reuse is
    // scoped to dominance: every use of `dst` is dominated by `dst`'s
    // definition, `prev`'s definition dominates that, so `prev` reaches every
    // one of them — including a phi use, which is on its incoming EDGE and so
    // asks the same question of the predecessor block. The types match by
    // construction, since `ty` is part of the key.
    let mut subst: HashMap<String, String> = HashMap::new();
    for (b, i, prev) in &rewrites {
        if let Some(dst) = instr_dst_name(&func.blocks[*b].instrs[*i]) {
            subst.insert(dst.to_string(), prev.clone());
        }
    }
    if !subst.is_empty() {
        for block in &mut func.blocks {
            for instr in &mut block.instrs {
                substitute_in_instruction(instr, &subst);
            }
            substitute_in_terminator(&mut block.term, &subst);
        }
    }

    // Both the redundant definitions and the redundant checks go, deepest
    // index first so the earlier positions stay valid.
    let mut dead: Vec<(usize, usize)> =
        rewrites.iter().map(|(b, i, _)| (*b, *i)).chain(removals).collect();
    dead.sort_unstable();
    for (b, i) in dead.into_iter().rev() {
        func.blocks[b].instrs.remove(i);
    }
}

/// Whether every temp this function defines is defined exactly once.
///
/// The walk above reuses a definition from a block that DOMINATES the
/// redundant one, which names the same value only if nothing reassigned it or
/// its operands in between — that is, only if the function is in SSA form.
/// It is: F-1 measured ZERO single-assignment violations over 79,953
/// instructions, after lowering and again after the optimiser. But "measured"
/// and "guaranteed" are different words, and a lowerer that stopped producing
/// SSA would turn this pass into a silently wrong answer rather than a failed
/// check. A function that violates it is skipped instead.
fn is_single_assignment(func: &IRFunction) -> bool {
    let mut seen: HashSet<&str> = HashSet::new();
    for block in &func.blocks {
        for instr in &block.instrs {
            if let Some(dst) = instr_dst_name(instr) {
                if !seen.insert(dst) {
                    return false;
                }
            }
        }
    }
    true
}

/// Rename temps throughout an instruction, including inside a `Phi`.
fn substitute_in_instruction(instr: &mut IRInstr, subst: &HashMap<String, String>) {
    for_each_operand_mut(instr, true, |v| substitute_value(v, subst));
}

fn substitute_in_terminator(term: &mut IRTerminator, subst: &HashMap<String, String>) {
    for_each_term_operand_mut(term, |v| substitute_value(v, subst));
}

fn substitute_value(val: &mut IRValue, subst: &HashMap<String, String>) {
    if let IRValue::Temp(t) = val {
        if let Some(name) = subst.get(&t.0) {
            *val = IRValue::Temp(IRTemp::new(name.clone()));
        }
    }
}

/// A canonical key for a pure instruction: everything that decides its result.
///
/// Two instructions with the same key compute the same value, so the key must
/// carry the result TYPE as well as the operator and operands — see the note
/// on the pass above for why omitting it was a latent narrowing.
fn cse_key(instr: &IRInstr) -> Option<String> {
    match instr {
        IRInstr::BinOp { op, lhs, rhs, ty, .. } => {
            Some(format!("binop:{:?}:{:?}:{:?}:{:?}", op, lhs, rhs, ty))
        }
        IRInstr::UnOp { op, operand, ty, .. } => {
            Some(format!("unop:{:?}:{:?}:{:?}", op, operand, ty))
        }
        IRInstr::TritMin { a, b, .. } => Some(format!("tmin:{:?}:{:?}", a, b)),
        IRInstr::TritMax { a, b, .. } => Some(format!("tmax:{:?}:{:?}", a, b)),
        IRInstr::TritNeg { a, .. } => Some(format!("tneg:{:?}", a)),
        IRInstr::TritSign { a, .. } => Some(format!("tsign:{:?}", a)),
        IRInstr::TritLane { op, a, b, .. } => Some(format!("tlane:{:?}:{:?}:{:?}", op, a, b)),
        // Address arithmetic, and the largest keyable kind in the IR after
        // `BinOp`. Pure: the same base and the same index give the same
        // address, and in SSA neither operand can have changed.
        IRInstr::GetPtr { ptr, idx, ty, .. } => {
            Some(format!("getptr:{:?}:{:?}:{:?}", ptr, idx, ty))
        }
        IRInstr::Cast { src, from_ty, to_ty, .. } => {
            Some(format!("cast:{:?}:{:?}:{:?}", src, from_ty, to_ty))
        }
        // A2's bounds check. It defines nothing and it traps, so a repeat is
        // removed outright rather than rewritten — and removing it is safe for
        // the reason a repeat is redundant at all: same index, same length,
        // and the check that dominates this one already passed.
        IRInstr::BoundsCheck { idx, len } => Some(format!("bounds:{:?}:{}", idx, len)),
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
/// Only `Mul`, `MulT27` and `DivNear` — see the note at the call site for why
/// `Div` is excluded. k >= 1, so `x * 1` stays for the identity rule above;
/// k <= 26, the widest shift a 27-trit word can take.
fn pow3_shift(op: &IRBinOp, rhs: &IRValue) -> Option<i64> {
    if !matches!(op, IRBinOp::Mul | IRBinOp::MulT27 | IRBinOp::DivNear) {
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
                //   MulT27   -> TShlT27. The CHECKED shift. On T3 it is the
                //                      same `TSHI` — `checked27` traps either
                //                      way — and on LLVM it carries N5's
                //                      overflow guard, so `--lang v2` keeps
                //                      the check it exists to provide.
                //   Div      -> NO.    `Div` TRUNCATES and `TSHR` ROUNDS. They
                //                      differ for every negative operand that
                //                      does not divide exactly: -5/3 is -1
                //                      truncating and -2 rounding.
                if replacement.is_none() {
                    if let Some(k) = pow3_shift(op, rhs) {
                        *instr = IRInstr::BinOp {
                            dst: IRTemp::new(dst.0.clone()),
                            op: match op {
                                IRBinOp::Mul => IRBinOp::TShl,
                                IRBinOp::MulT27 => IRBinOp::TShlT27,
                                _ => IRBinOp::TShr,
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
        if !func.is_extern {
            dead_block_eliminate_func(func);
        }
    }
}

/// The same for one function. Split out because the ternary peephole leaves
/// the block whose branch it absorbed unreachable, and has to drop it before
/// it can ask which of the new edges are critical.
fn dead_block_eliminate_func(func: &mut IRFunction) {
    if func.blocks.len() <= 1 {
        return;
    }
    {
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
