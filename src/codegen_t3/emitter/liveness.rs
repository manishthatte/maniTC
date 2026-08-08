// emitter/liveness.rs — Liveness analysis, last-use tracking, and register allocator.
use crate::ir::*;
use std::collections::HashMap;


fn record_val_use(val: &IRValue, idx: usize, last_use: &mut HashMap<String, usize>) {
    if let IRValue::Temp(t) = val {
        last_use.insert(t.0.clone(), idx);
    }
}

fn record_def(instr: &IRInstr, idx: usize, first_def: &mut HashMap<String, usize>) {
    macro_rules! def {
        ($t:expr) => { first_def.entry($t.0.clone()).or_insert(idx); }
    }
    match instr {
        IRInstr::BinOp { dst, .. } | IRInstr::UnOp { dst, .. } | IRInstr::Assign { dst, .. }
        | IRInstr::Alloca { dst, .. } | IRInstr::Load { dst, .. } | IRInstr::GetPtr { dst, .. }
        | IRInstr::TritMin { dst, .. } | IRInstr::TritMax { dst, .. } | IRInstr::TritNeg { dst, .. }
        | IRInstr::Cast { dst, .. } | IRInstr::Phi { dst, .. } => { def!(dst); }
        IRInstr::Call { dst: Some(dst), .. } => { def!(dst); }
        IRInstr::CallIndirect { dst: Some(dst), .. } => { def!(dst); }
        _ => {}
    }
}

fn term_successors<'a>(term: &'a IRTerminator) -> Vec<&'a str> {
    match term {
        IRTerminator::Jump(l) => vec![l.as_str()],
        IRTerminator::BinBranch { true_label, false_label, .. } => {
            vec![true_label.as_str(), false_label.as_str()]
        }
        IRTerminator::TritBranch { pos_label, zero_label, neg_label, .. } => {
            vec![pos_label.as_str(), zero_label.as_str(), neg_label.as_str()]
        }
        IRTerminator::Return(_) | IRTerminator::Unreachable => vec![],
    }
}

fn record_uses_instr(instr: &IRInstr, idx: usize, last_use: &mut HashMap<String, usize>) {
    match instr {
        IRInstr::BinOp { lhs, rhs, .. } => {
            record_val_use(lhs, idx, last_use);
            record_val_use(rhs, idx, last_use);
        }
        IRInstr::UnOp { operand, .. } => { record_val_use(operand, idx, last_use); }
        IRInstr::Assign { src, .. } => { record_val_use(src, idx, last_use); }
        IRInstr::Alloca { .. } => {}
        IRInstr::Store { ptr, val, .. } => {
            record_val_use(ptr, idx, last_use);
            record_val_use(val, idx, last_use);
        }
        IRInstr::Load { ptr, .. } => { record_val_use(ptr, idx, last_use); }
        IRInstr::Call { args, .. } => {
            for a in args { record_val_use(a, idx, last_use); }
        }
        IRInstr::CallIndirect { fn_ptr, args, .. } => {
            record_val_use(fn_ptr, idx, last_use);
            for a in args { record_val_use(a, idx, last_use); }
        }
        IRInstr::GetPtr { ptr, idx: i, .. } => {
            record_val_use(ptr, idx, last_use);
            record_val_use(i, idx, last_use);
        }
        IRInstr::TritMin { a, b, .. } | IRInstr::TritMax { a, b, .. } => {
            record_val_use(a, idx, last_use);
            record_val_use(b, idx, last_use);
        }
        IRInstr::TritNeg { a, .. } => { record_val_use(a, idx, last_use); }
        IRInstr::PrintStr(v) | IRInstr::PrintInt(v) | IRInstr::PrintFloat(v)
        | IRInstr::PrintBool3(v) | IRInstr::PrintTrit(v) => {
            record_val_use(v, idx, last_use);
        }
        IRInstr::Phi { incoming, .. } => {
            for (v, _) in incoming { record_val_use(v, idx, last_use); }
        }
        IRInstr::Cast { src, .. } => { record_val_use(src, idx, last_use); }
    }
}

/// Compute the last-use instruction index for each named temp in the function.
/// Extends last-use across back edges (loop-carried values) so registers aren't
/// freed prematurely during loop iterations.
pub(super) fn compute_last_use(blocks: &[IRBlock]) -> HashMap<String, usize> {
    let mut last_use: HashMap<String, usize> = HashMap::new();
    let mut first_def: HashMap<String, usize> = HashMap::new();
    let mut block_start: Vec<usize> = Vec::new();
    let mut block_end: Vec<usize> = Vec::new();

    // Phase 1: Forward scan — collect first_def and last_use
    let mut idx = 0usize;
    for block in blocks {
        block_start.push(idx);
        for instr in &block.instrs {
            record_def(instr, idx, &mut first_def);
            record_uses_instr(instr, idx, &mut last_use);
            idx += 1;
        }
        match &block.term {
            IRTerminator::Return(Some(v)) => { record_val_use(v, idx, &mut last_use); }
            IRTerminator::BinBranch { cond, .. } => { record_val_use(cond, idx, &mut last_use); }
            IRTerminator::TritBranch { cond, .. } => { record_val_use(cond, idx, &mut last_use); }
            _ => {}
        }
        block_end.push(idx);
        idx += 1;
    }

    // Phase 2: Extend last_use for loop-carried values across back edges.
    // A back edge is (bi → si) where si <= bi (successor block comes earlier in linear order).
    // Any temp defined before the loop header (si) and used inside the loop (si..bi)
    // must be kept alive until the end of the loop back-edge block (block_end[bi]).
    let block_pos: HashMap<&str, usize> = blocks.iter().enumerate()
        .map(|(i, b)| (b.label.as_str(), i))
        .collect();

    for (bi, block) in blocks.iter().enumerate() {
        for succ_label in term_successors(&block.term) {
            if let Some(&si) = block_pos.get(succ_label) {
                if si <= bi {
                    // Back edge bi → si: loop spans blocks si..=bi
                    let loop_start = block_start[si];
                    let loop_end = block_end[bi];

                    let to_extend: Vec<String> = last_use.iter()
                        .filter(|(name, &lu)| {
                            lu >= loop_start && lu <= loop_end
                                && first_def.get(*name)
                                    .map_or(true, |&d| d < loop_start)
                        })
                        .map(|(name, _)| name.clone())
                        .collect();

                    for name in to_extend {
                        if let Some(lu) = last_use.get_mut(&name) {
                            *lu = loop_end;
                        }
                    }
                }
            }
        }
    }

    last_use
}

// ---------------------------------------------------------------------------
// Register allocator — pool-based with liveness-driven freeing and stack spills
// ---------------------------------------------------------------------------
//
// Register layout:
//   R0        = always zero
//   R1–R20    = general-purpose pool (20 regs)
//   R21       = spill-read scratch 1  (loaded before instruction)
//   R22       = spill-read scratch 2  (loaded before instruction, 2nd spilled input)
//   R23       = spill-write scratch   (instruction destination, stored after)
//   R24       = anonymous scratch fallback 1 (when pool is empty)
//   R25       = anonymous scratch fallback 2 (last resort)
//   R26       = SP (stack pointer, grows down)

pub(super) struct RegAlloc {
    pub(super) temp_to_reg:   HashMap<String, usize>,
    pub(super) spill_slots:   HashMap<String, usize>, // temp → sp_depth at spill time
    pub(super) free_regs:     std::collections::BTreeSet<usize>,
    pub(super) last_use:      HashMap<String, usize>,
    pub(super) current_instr: usize,
    pub(super) scratch_in_instr: Vec<usize>,
    pub spill_count: usize,
    pub sp_depth:  usize,               // total words pushed (alloca + spill stores)
    pub(super) scratch_fallback_count: usize,
}

impl RegAlloc {
    /// Create allocator with pre-computed last-use map.
    /// Pool: R1–R20.  R21/R22 = spill-read scratch.  R23 = spill-write scratch.
    pub(super) fn new(last_use: HashMap<String, usize>) -> Self {
        let free_regs = (1..=20usize).collect();
        RegAlloc {
            temp_to_reg: HashMap::new(),
            spill_slots: HashMap::new(),
            free_regs,
            last_use,
            current_instr: 0,
            scratch_in_instr: Vec::new(),
            spill_count: 0,
            sp_depth: 0,
            scratch_fallback_count: 0,
        }
    }

    /// Allocate an anonymous scratch register for use within this instruction.
    /// Returns from the pool if available, otherwise R24 then R25.
    pub(super) fn alloc_scratch(&mut self) -> usize {
        if let Some(&r) = self.free_regs.iter().next() {
            self.free_regs.remove(&r);
            self.scratch_in_instr.push(r);
            r
        } else {
            let r = match self.scratch_fallback_count {
                0 => 24,
                _ => 25,
            };
            self.scratch_fallback_count += 1;
            r
        }
    }

    /// Reserve a specific register (e.g. for function parameters).
    pub(super) fn reserve(&mut self, name: &str, reg: usize) {
        self.free_regs.remove(&reg);
        self.temp_to_reg.insert(name.to_string(), reg);
    }

    /// Advance past the current instruction: free scratch regs and dead named temps.
    pub(super) fn advance(&mut self) {
        // Free scratch registers used within this instruction
        for r in self.scratch_in_instr.drain(..) {
            self.free_regs.insert(r);
        }
        self.scratch_fallback_count = 0;
        // Free named temps whose last use was this instruction
        let dead: Vec<String> = self.last_use.iter()
            .filter(|(_, &last)| last == self.current_instr)
            .map(|(k, _)| k.clone())
            .collect();
        for name in dead {
            if let Some(r) = self.temp_to_reg.remove(&name) {
                if r >= 1 && r <= 20 {
                    self.free_regs.insert(r);
                }
            }
            self.spill_slots.remove(&name); // free tracking (not stack space)
        }
        self.current_instr += 1;
    }
}

// ---------------------------------------------------------------------------
// Assembly emitter state
// ---------------------------------------------------------------------------

