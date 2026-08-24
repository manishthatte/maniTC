//! F-3 — the T3ISA register allocator: liveness, live intervals, linear scan.
//!
//! ## Why this replaces what it replaces
//!
//! The allocator this supersedes decided register assignment DURING emission.
//! A temp's register could change part-way through a function — "rescued" out
//! of a syscall-clobbered register, "reconciled" back at a jump — and a block's
//! entry state was whatever its first-emitted predecessor happened to leave
//! behind. Every one of those decisions was local, and the failures were
//! global: `KNOWN_ISSUES` issue 2 records two of them, both producing SILENTLY
//! WRONG ANSWERS and both needing enough register pressure that minimal
//! reproductions passed. F-1's `mem2reg` produced exactly that pressure and
//! surfaced three more (report.txt P11, P12, P14).
//!
//! The fix is not another heuristic. It is to make the question have one answer:
//!
//! > **A temp is assigned ONE location for the whole function.** Emission looks
//! > it up; it never changes it.
//!
//! With that, "rescue", "reconcile", "canonical block state" and "phi home" all
//! stop being questions. There is nothing to rescue, because nothing moves.
//!
//! ## The register invariant
//!
//! F-3 asks for "a written invariant about which registers the call sequence may
//! touch". Here it is, and the allocator enforces it rather than assuming it:
//!
//!   * **R0** reads as zero.
//!   * **R1–R3** are the ABI registers: arguments to syscalls, the syscall
//!     result, the function return value. They are NEVER allocated to a temp.
//!     A syscall writes R1 and reads R1–R3 (and R21/R22 for two `fmt` calls);
//!     it touches nothing else, so no allocated value can be destroyed by one.
//!   * **R4–R20** are the allocatable pool — 17 registers.
//!   * **R21–R25** are emission scratch, never allocated.
//!   * **R26** is the stack pointer.
//!   * **A CALL may destroy every register.** The callee allocates from the
//!     same pool. Therefore: **nothing live across a call is in a register.**
//!     `must_spill` enforces it, which is what lets the call sequence use the
//!     whole machine without saving anything.
//!
//! That last rule replaces caller-save entirely, and costs the same: a value
//! live across a call is stored once and reloaded once either way.
//!
//! ## Liveness, and the one thing that is easy to get wrong
//!
//! **A phi operand is used on its EDGE, not at the phi.** This codebase has
//! now paid for that fact three times in one day — in the SSA verifier's
//! design, in the T3 phi-copy emission (P11/P12), and in the old allocator's
//! last-use scan (P14). It is stated here as two dataflow rules:
//!
//!   * a phi operand is live-out of its incoming block, not live-in to the
//!     phi's block;
//!   * a phi destination is live from the END of every predecessor, because
//!     that is where the copy into it is written.
//!
//! © Manish Jagdish Thatte

use std::collections::{BTreeSet, HashMap, HashSet};

use crate::ir::ssa;
use crate::ir::types::*;

/// The lowest allocatable register. R1–R3 are the ABI's.
pub const POOL_LO: usize = 4;
/// The highest allocatable register. R21–R25 are scratch, R26 is SP.
pub const POOL_HI: usize = 20;
/// Registers a parameter may arrive in, R1..=PARAM_MAX.
pub const PARAM_MAX: usize = 8;

/// Where a temp lives, for the whole function.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Loc {
    /// Register `n`, `POOL_LO <= n <= POOL_HI`.
    Reg(usize),
    /// Word `n` of the spill area, at `[R26 + n]` once the frame is set up.
    Slot(usize),
}

impl Loc {
    pub fn reg(self) -> Option<usize> {
        match self {
            Loc::Reg(r) => Some(r),
            Loc::Slot(_) => None,
        }
    }

    pub fn is_slot(self) -> bool {
        matches!(self, Loc::Slot(_))
    }
}

/// A half-open-at-neither-end live range over the linear instruction numbering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Interval {
    pub start: usize,
    pub end: usize,
}

impl Interval {
    fn extend(&mut self, at: usize) {
        if at < self.start {
            self.start = at;
        }
        if at > self.end {
            self.end = at;
        }
    }

    pub fn covers(&self, at: usize) -> bool {
        self.start <= at && at <= self.end
    }

    /// Whether this range is live ACROSS `at` — defined strictly before and
    /// still needed strictly after. A value defined at a call, or used at one
    /// and dead afterwards, does not cross it.
    fn spans(&self, at: usize) -> bool {
        self.start < at && at < self.end
    }
}

/// The result: one location per temp, plus everything emission needs to lay out
/// the frame and pick scratch registers.
#[derive(Debug, Clone)]
pub struct Allocation {
    /// Temp name → its location, fixed for the whole function.
    pub locs: HashMap<String, Loc>,
    /// Live range of each temp, kept for diagnostics and for `free_regs_at`.
    pub intervals: HashMap<String, Interval>,
    /// Number of one-word spill slots the frame must reserve.
    pub n_slots: usize,
    /// Registers NOT holding a live value at each instruction index, as a
    /// bitmask over `POOL_LO..=POOL_HI`. Emission uses it when it needs a
    /// register that is not one of the five scratch ones.
    free_mask: Vec<u32>,
    /// The linear index of each block's first instruction.
    pub block_start: Vec<usize>,
    /// The linear index of each block's terminator.
    pub block_end: Vec<usize>,
    /// How many temps had to be spilled because they were live across a call.
    pub spilled_across_call: usize,
    /// How many were spilled because the pool was full.
    pub spilled_for_pressure: usize,
    /// Parameters that could not stay in the register they arrived in:
    /// (arrival register, spill slot). The prologue stores each one.
    pub param_slots: Vec<(usize, usize)>,
}

impl Allocation {
    pub fn loc(&self, temp: &str) -> Option<Loc> {
        self.locs.get(temp).copied()
    }

    /// A register in the pool that holds no live value at `idx`, if any.
    ///
    /// Emission needs this for the rare case of a value that is not a temp —
    /// a materialised constant that several operands share, say. It is exact
    /// rather than heuristic: the mask comes from the same intervals the
    /// assignment did.
    pub fn free_reg_at(&self, idx: usize) -> Option<usize> {
        let mask = *self.free_mask.get(idx)?;
        (POOL_LO..=POOL_HI).find(|&r| mask & (1u32 << r) != 0)
    }

    /// Every free register at `idx`, lowest first.
    pub fn free_regs_at(&self, idx: usize) -> Vec<usize> {
        let Some(&mask) = self.free_mask.get(idx) else {
            return Vec::new();
        };
        (POOL_LO..=POOL_HI).filter(|&r| mask & (1u32 << r) != 0).collect()
    }
}

// ---------------------------------------------------------------------------
// Liveness
// ---------------------------------------------------------------------------

/// Per-block sets, in the order `func.blocks` has them.
struct BlockInfo {
    /// Temps this block defines, phi destinations included.
    defs: HashSet<String>,
    /// Temps read before being written in this block. Phi OPERANDS are not
    /// here — see the module header.
    uses: HashSet<String>,
    live_in: HashSet<String>,
    live_out: HashSet<String>,
    succs: Vec<usize>,
    /// Phi operands this block must supply, per successor: (succ, operand).
    edge_out: Vec<(usize, String)>,
}

fn block_info(func: &IRFunction) -> Vec<BlockInfo> {
    let index: HashMap<&str, usize> = func
        .blocks
        .iter()
        .enumerate()
        .map(|(i, b)| (b.label.as_str(), i))
        .collect();

    let mut out = Vec::with_capacity(func.blocks.len());
    for block in &func.blocks {
        let mut defs: HashSet<String> = HashSet::new();
        let mut uses: HashSet<String> = HashSet::new();
        for instr in &block.instrs {
            // Uses first: a temp read before this block writes it is an
            // upward-exposed use, and one read after is not.
            for u in ssa::instr_uses(instr) {
                if !defs.contains(u) {
                    uses.insert(u.to_string());
                }
            }
            if let Some(d) = ssa::instr_def(instr) {
                defs.insert(d.to_string());
            }
        }
        for u in ssa::term_uses(&block.term) {
            if !defs.contains(u) {
                uses.insert(u.to_string());
            }
        }

        let succs: Vec<usize> = ssa::terminator_targets(&block.term)
            .into_iter()
            .filter_map(|l| index.get(l.as_str()).copied())
            .collect();

        out.push(BlockInfo {
            defs,
            uses,
            live_in: HashSet::new(),
            live_out: HashSet::new(),
            succs,
            edge_out: Vec::new(),
        });
    }

    // Phi operands, attributed to the edge they travel on.
    for (si, block) in func.blocks.iter().enumerate() {
        for instr in &block.instrs {
            for (temp, pred_label) in ssa::phi_uses(instr) {
                let Some(&pi) = index.get(pred_label) else { continue };
                out[pi].edge_out.push((si, temp.to_string()));
            }
        }
    }

    out
}

/// Backward dataflow to a fixpoint.
fn solve_liveness(info: &mut [BlockInfo]) {
    let mut changed = true;
    while changed {
        changed = false;
        // Reverse order converges faster on a forward-ordered block list.
        for b in (0..info.len()).rev() {
            let mut out: HashSet<String> = HashSet::new();
            for &s in &info[b].succs {
                for t in &info[s].live_in {
                    out.insert(t.clone());
                }
            }
            // A phi operand is live out of THIS block even though the phi is
            // in the successor and the successor does not have it live-in.
            for (_, t) in &info[b].edge_out {
                out.insert(t.clone());
            }

            let mut inn = info[b].uses.clone();
            for t in &out {
                if !info[b].defs.contains(t) {
                    inn.insert(t.clone());
                }
            }

            if out != info[b].live_out || inn != info[b].live_in {
                changed = true;
                info[b].live_out = out;
                info[b].live_in = inn;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Intervals
// ---------------------------------------------------------------------------

/// Number the instructions the way emission walks them: blocks in order, each
/// block's instructions then its terminator.
///
/// The terminator gets an index of its own because that is where phi copies,
/// branch conditions and the epilogue live — three things that read values.
fn number(func: &IRFunction) -> (Vec<usize>, Vec<usize>, usize) {
    let mut block_start = Vec::with_capacity(func.blocks.len());
    let mut block_end = Vec::with_capacity(func.blocks.len());
    // Index 0 is FUNCTION ENTRY, before the first instruction. Parameters are
    // defined there, and giving them an index of their own is what makes
    // "live across the first instruction" expressible: a parameter used after
    // a call in the entry block is live across it, and would not be if it
    // started at the call's own index.
    let mut idx = 1usize;
    for block in &func.blocks {
        block_start.push(idx);
        idx += block.instrs.len();
        block_end.push(idx);
        idx += 1;
    }
    (block_start, block_end, idx)
}

fn intervals_of(
    func: &IRFunction,
    info: &[BlockInfo],
    block_start: &[usize],
    block_end: &[usize],
) -> HashMap<String, Interval> {
    let mut iv: HashMap<String, Interval> = HashMap::new();
    let extend = |iv: &mut HashMap<String, Interval>, name: &str, at: usize| {
        iv.entry(name.to_string())
            .and_modify(|i| i.extend(at))
            .or_insert(Interval { start: at, end: at });
    };

    // Parameters are live from function entry.
    for (pname, _) in &func.params {
        extend(&mut iv, &format!("param_{}", pname), 0);
    }

    for (bi, block) in func.blocks.iter().enumerate() {
        let (bs, be) = (block_start[bi], block_end[bi]);

        for t in &info[bi].live_in {
            extend(&mut iv, t, bs);
        }
        for t in &info[bi].live_out {
            extend(&mut iv, t, be);
        }

        for (k, instr) in block.instrs.iter().enumerate() {
            let at = bs + k;
            if let Some(d) = ssa::instr_def(instr) {
                extend(&mut iv, d, at);
            }
            for u in ssa::instr_uses(instr) {
                extend(&mut iv, u, at);
            }
            // A phi's destination is written by every predecessor, at that
            // predecessor's terminator. Its range has to reach back there, or
            // the copy writes a register another value still owns.
            if let IRInstr::Phi { dst, incoming, .. } = instr {
                for (_, pred_label) in incoming {
                    let Some(pi) = func.blocks.iter().position(|b| b.label == *pred_label) else {
                        continue;
                    };
                    extend(&mut iv, &dst.0, block_end[pi]);
                }
            }
        }

        for u in ssa::term_uses(&block.term) {
            extend(&mut iv, u, be);
        }
        // Phi operands leave on this block's terminator.
        for (_, t) in &info[bi].edge_out {
            extend(&mut iv, t, be);
        }
    }

    iv
}

/// The linear indices at which a call happens. Everything live across one of
/// these must be in memory — see the register invariant in the module header.
fn call_sites(func: &IRFunction, block_start: &[usize]) -> Vec<usize> {
    let mut out = Vec::new();
    for (bi, block) in func.blocks.iter().enumerate() {
        for (k, instr) in block.instrs.iter().enumerate() {
            if matches!(instr, IRInstr::Call { .. } | IRInstr::CallIndirect { .. }) {
                out.push(block_start[bi] + k);
            }
        }
    }
    out
}

/// Instructions that may destroy **R1–R3**, which is a strictly larger set than
/// the calls.
///
/// A CALL may destroy every register, so `call_sites` answers "what may destroy
/// a POOL register". A SYSCALL destroys only the ABI registers, so it never
/// endangers the pool and is absent from that set — but it is exactly what
/// endangers a parameter still sitting in the register it arrived in.
///
/// The two must stay separate. Folding these sites into `call_sites` would
/// apply the "nothing live across a call is in a register" rule to every
/// arithmetic instruction and spill the whole function.
///
/// This MIRRORS the syscall decisions in `emitter::emit_instr`, and the two
/// have to be read together — the conditions below are copied from its `BinOp`
/// and `UnOp` arms deliberately, not derived independently.
///
/// An earlier version was conservative by KIND, counting every `BinOp` and
/// `UnOp` rather than only the ones that lower to a syscall. That is safe but
/// it is not free: once promotion is on by default, a parameter's live range
/// reaches real arithmetic, so over-approximating here spills the first three
/// parameters of every arithmetic function to the frame. Precision is worth the
/// coupling, and `s23_the_whole_math_float_surface_runs_on_both_backends` is
/// the test that fails if this drifts out of step with the emitter.
fn abi_clobber_sites(
    func: &IRFunction,
    block_start: &[usize],
    struct_sizes: &HashMap<String, usize>,
) -> Vec<usize> {
    // A float literal is materialised with `TLIT R1, #addr; SYSCALL #219`
    // wherever it appears, so an operand can clobber R1 on its own.
    let float_const =
        |v: &IRValue| matches!(v, IRValue::Const(IRConst::Float(_)));

    let mut out = Vec::new();
    for (bi, block) in func.blocks.iter().enumerate() {
        for (k, instr) in block.instrs.iter().enumerate() {
            let clobbers = match instr {
                // A CALL may destroy everything, R1–R3 included.
                IRInstr::Call { .. } | IRInstr::CallIndirect { .. } => true,
                // A struct or tuple alloca is `TLIT R1, #n; SYSCALL #218`.
                // This is the one that bit first: the lowerer's opening act in
                // the entry block is to alloca the storage a parameter is about
                // to be stored into, so the syscall lands inside the
                // parameter's live range, between its arrival and its store.
                IRInstr::Alloca { ty, .. } => is_heap_alloca(ty, struct_sizes),
                // The bounds check is SYSCALL #560.
                IRInstr::BoundsCheck { .. } => true,
                IRInstr::BinOp { op, lhs, rhs, ty, .. } => {
                    // str_concat (#61), float arithmetic (#219), float compare
                    // (#216), str_eq/str_ne (#200).
                    let concat =
                        matches!(op, IRBinOp::Add) && matches!(ty, IRType::Ptr(_));
                    let float_arith = matches!(ty, IRType::F64)
                        && matches!(
                            op,
                            IRBinOp::Add
                                | IRBinOp::Sub
                                | IRBinOp::Mul
                                | IRBinOp::Div
                                | IRBinOp::Rem // frem, SYSCALL #221 (P19)
                        );
                    // Float comparisons carry a Bool result type, so the op
                    // variant is the only thing that identifies them.
                    let float_cmp = matches!(
                        op,
                        IRBinOp::FEq
                            | IRBinOp::FNe
                            | IRBinOp::FLt
                            | IRBinOp::FGt
                            | IRBinOp::FLe
                            | IRBinOp::FGe
                    );
                    let str_cmp = matches!(op, IRBinOp::StrEq | IRBinOp::StrNe);
                    concat
                        || float_arith
                        || float_cmp
                        || str_cmp
                        || float_const(lhs)
                        || float_const(rhs)
                }
                // fneg is SYSCALL #220.
                IRInstr::UnOp { op, operand, .. } => {
                    matches!(op, IRUnOp::FNeg) || float_const(operand)
                }
                IRInstr::Assign { src, .. } => float_const(src),
                IRInstr::Store { val, .. } => float_const(val),
                _ => false,
            };
            if clobbers {
                out.push(block_start[bi] + k);
            }
        }
    }
    out
}

/// Whether `emit_instr`'s `Alloca` arm will put this type on the HEAP rather
/// than in the frame.
///
/// Named structs and tuples escape — returning one is often the whole point —
/// so they cannot live in a frame that gets popped. The predicate lives here
/// so the frame layout, the emitter and this analysis cannot drift apart:
/// disagreeing about which allocas are heap ones desynchronises the frame
/// offsets from the addresses actually used.
pub fn is_heap_alloca(ty: &IRType, struct_sizes: &HashMap<String, usize>) -> bool {
    match ty {
        IRType::Struct(name) => {
            struct_sizes.contains_key(name)
                || crate::ir::types::tuple_arity_from_name(name).is_some()
        }
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Linear scan
// ---------------------------------------------------------------------------

/// The ABI register a parameter arrives in: the first in R1, and so on.
///
/// `emit_function` refuses a function with more than `PARAM_MAX` parameters —
/// this convention has no stack argument area — so the clamp here is
/// unreachable rather than a silent aliasing of the ninth argument onto R8.
pub fn param_reg(i: usize) -> usize {
    (i + 1).min(PARAM_MAX)
}

/// Assign every temp in `func` a fixed location, with no named structs in
/// scope. Tuples are still recognised as heap allocas, since their arity rides
/// in the type name.
pub fn allocate(func: &IRFunction) -> Allocation {
    allocate_with(func, &HashMap::new())
}

/// Assign every temp in `func` a fixed location.
///
/// `struct_sizes` must be the same map `emit_function` lays the frame out
/// with: it decides which allocas are heap ones, which decides in turn which
/// instructions can destroy a parameter still in its arrival register.
pub fn allocate_with(func: &IRFunction, struct_sizes: &HashMap<String, usize>) -> Allocation {
    let (block_start, block_end, n_idx) = number(func);
    let mut info = block_info(func);
    solve_liveness(&mut info);
    let intervals = intervals_of(func, &info, &block_start, &block_end);
    let calls = call_sites(func, &block_start);
    let abi_clobbers = abi_clobber_sites(func, &block_start, struct_sizes);

    // Order by start, then by end, then by name — the last only so that two
    // identical intervals resolve the same way on every run. Register choice
    // must not depend on hash iteration order; a compiler that emits different
    // code for the same input cannot be bisected.
    let mut order: Vec<(&String, &Interval)> = intervals.iter().collect();
    order.sort_by(|a, b| {
        a.1.start
            .cmp(&b.1.start)
            .then_with(|| a.1.end.cmp(&b.1.end))
            .then_with(|| a.0.cmp(b.0))
    });

    let mut locs: HashMap<String, Loc> = HashMap::new();
    let mut free: BTreeSet<usize> = (POOL_LO..=POOL_HI).collect();
    // (interval end, name, register) — the values currently holding registers.
    let mut active: Vec<(usize, String, usize)> = Vec::new();
    let mut n_slots = 0usize;
    let mut spilled_across_call = 0usize;
    let mut spilled_for_pressure = 0usize;

    let new_slot = |n_slots: &mut usize| {
        let s = *n_slots;
        *n_slots += 1;
        Loc::Slot(s)
    };

    // Parameters are pre-bound to the register they ARRIVE in, so the prologue
    // has no moves to make and no parallel-copy problem to solve. The lowerer
    // stores every parameter into an alloca in the entry block before anything
    // else runs, so a parameter's live range is a handful of instructions —
    // which is what can make binding one to R1–R3 safe despite the rule that
    // those are never allocated.
    //
    // When it is NOT safe the binding is refused and the parameter gets a
    // slot, which the prologue stores into before any block runs. The check is
    // here rather than assumed because the cost of assuming is a clobbered
    // argument with no diagnostic — and that is precisely what happened: the
    // check used to ask only about calls, but a parameter in R1–R3 is equally
    // dead if a SYSCALL runs before its store, and the alloca the lowerer
    // emits to hold a struct or enum parameter IS a syscall. `fn f(d: Dir)`
    // lowered to `alloca; store param_d` and the alloca's `TLIT R1, #1;
    // SYSCALL #218` destroyed `param_d` one instruction before it was read,
    // so every enum parameter arrived as a heap address: `match` fell through
    // every arm and `==` was false against all four variants.
    //
    // Which set to consult depends on the register, because the two hazards
    // have different reach. A parameter in the POOL (R4 upward) is endangered
    // only by a call; one in R1–R3 is endangered by any syscall as well.
    let mut param_slots: Vec<(usize, usize)> = Vec::new();
    for (i, (pname, _)) in func.params.iter().enumerate() {
        let name = format!("param_{}", pname);
        let Some(iv) = intervals.get(&name).copied() else { continue };
        let abi = param_reg(i);
        // INCLUSIVE for the ABI registers, strict for the pool, and the
        // difference is the whole subtlety.
        //
        // `spans` is strict at both ends because a value used AT a call and
        // dead afterwards does not cross it: the call sequence reads the
        // argument while placing it, and the CALL itself happens after.
        //
        // A syscall-lowered instruction is not like that. It writes R1 as part
        // of its own emission, BEFORE consuming its operands — a float `x *
        // K` loads K with `TLIT R1, #addr; SYSCALL #219` and only then
        // multiplies. So when such an instruction is the parameter's LAST use,
        // `at == end` and the strict test called it safe. `math::to_radians`
        // returned (PI/180)^2: the constant landed in R1, destroying the `x`
        // that arrived there, and the multiply squared the constant. Same for
        // `to_degrees`, and invisible until `mem2reg` stopped routing every
        // parameter through memory first.
        let clobbered = if abi < POOL_LO {
            abi_clobbers.iter().any(|&c| iv.covers(c))
        } else {
            calls.iter().any(|&c| iv.spans(c))
        };
        if clobbered {
            let Loc::Slot(s) = new_slot(&mut n_slots) else { unreachable!() };
            locs.insert(name, Loc::Slot(s));
            param_slots.push((abi, s));
            spilled_across_call += 1;
            continue;
        }
        locs.insert(name.clone(), Loc::Reg(abi));
        if free.remove(&abi) {
            active.push((iv.end, name, abi));
        }
    }

    for (name, iv) in order {
        if locs.contains_key(name) {
            continue; // a pre-bound parameter
        }
        // Expire everything that ended before this one starts.
        active.retain(|(end, _, reg)| {
            if *end < iv.start {
                free.insert(*reg);
                false
            } else {
                true
            }
        });

        // Rule: nothing live across a call is in a register.
        if calls.iter().any(|&c| iv.spans(c)) {
            locs.insert(name.clone(), new_slot(&mut n_slots));
            spilled_across_call += 1;
            continue;
        }

        match free.iter().next().copied() {
            Some(r) => {
                free.remove(&r);
                active.push((iv.end, name.clone(), r));
                locs.insert(name.clone(), Loc::Reg(r));
            }
            None => {
                // Pool full. Spill whichever of the candidates lives longest,
                // which is the standard linear-scan choice: it frees the
                // register for the greatest span.
                let victim = active
                    .iter()
                    .enumerate()
                    .max_by_key(|(_, (end, _, _))| *end)
                    .map(|(i, (end, n, r))| (i, *end, n.clone(), *r));
                match victim {
                    Some((i, vend, vname, vreg)) if vend > iv.end => {
                        active.remove(i);
                        locs.insert(vname, new_slot(&mut n_slots));
                        spilled_for_pressure += 1;
                        active.push((iv.end, name.clone(), vreg));
                        locs.insert(name.clone(), Loc::Reg(vreg));
                    }
                    _ => {
                        locs.insert(name.clone(), new_slot(&mut n_slots));
                        spilled_for_pressure += 1;
                    }
                }
            }
        }
    }

    // Per-instruction free-register mask, recomputed from the final assignment
    // rather than tracked during the scan — the scan reassigns registers when
    // it spills a victim, and a mask built as it went would record the register
    // as busy for a value that no longer holds it.
    let mut free_mask = vec![0u32; n_idx + 1];
    for m in free_mask.iter_mut() {
        for r in POOL_LO..=POOL_HI {
            *m |= 1u32 << r;
        }
    }
    for (name, loc) in &locs {
        let Loc::Reg(r) = loc else { continue };
        let Some(iv) = intervals.get(name) else { continue };
        for i in iv.start..=iv.end.min(n_idx) {
            free_mask[i] &= !(1u32 << r);
        }
    }

    Allocation {
        locs,
        intervals,
        n_slots,
        free_mask,
        block_start,
        block_end,
        spilled_across_call,
        spilled_for_pressure,
        param_slots,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(name: &str) -> IRValue {
        IRValue::Temp(IRTemp::new(name))
    }

    fn func(blocks: Vec<IRBlock>) -> IRFunction {
        IRFunction {
            name: "f".into(),
            params: Vec::new(),
            ret_ty: IRType::I64,
            blocks,
            is_extern: false,
        }
    }

    fn block(label: &str, instrs: Vec<IRInstr>, term: IRTerminator) -> IRBlock {
        IRBlock { label: label.into(), instrs, term }
    }

    fn assign(dst: &str, v: i64) -> IRInstr {
        IRInstr::Assign {
            dst: IRTemp::new(dst),
            src: IRValue::Const(IRConst::Int(v)),
            ty: IRType::I64,
        }
    }

    fn add(dst: &str, a: &str, b: &str) -> IRInstr {
        IRInstr::BinOp {
            dst: IRTemp::new(dst),
            op: IRBinOp::Add,
            lhs: t(a),
            rhs: t(b),
            ty: IRType::I64,
        }
    }

    fn call(dst: Option<&str>, name: &str, args: Vec<IRValue>) -> IRInstr {
        IRInstr::Call {
            dst: dst.map(IRTemp::new),
            func: name.into(),
            args,
            ret_ty: IRType::I64,
        }
    }

    /// Every pair of temps whose live ranges overlap must have different
    /// registers. This is the allocator's whole correctness condition, and it
    /// is checked directly rather than inferred from the output looking right.
    fn assert_no_overlap_shares_a_register(a: &Allocation) {
        let regs: Vec<(&String, usize, Interval)> = a
            .locs
            .iter()
            .filter_map(|(n, l)| l.reg().map(|r| (n, r, a.intervals[n])))
            .collect();
        for (i, (n1, r1, i1)) in regs.iter().enumerate() {
            for (n2, r2, i2) in regs.iter().skip(i + 1) {
                if r1 != r2 {
                    continue;
                }
                let overlap = i1.start <= i2.end && i2.start <= i1.end;
                assert!(
                    !overlap,
                    "{} {:?} and {} {:?} overlap and share R{}",
                    n1, i1, n2, i2, r1
                );
            }
        }
    }

    #[test]
    fn a_straight_line_function_uses_the_pool_from_the_bottom() {
        let f = func(vec![block(
            "entry",
            vec![assign("a", 1), assign("b", 2), add("c", "a", "b")],
            IRTerminator::Return(Some(t("c"))),
        )]);
        let a = allocate(&f);
        assert_eq!(a.n_slots, 0, "nothing needs spilling: {:?}", a.locs);
        assert_eq!(a.loc("a"), Some(Loc::Reg(POOL_LO)));
        assert_no_overlap_shares_a_register(&a);
    }

    #[test]
    fn a_dead_value_frees_its_register_for_the_next_one() {
        // `a` is dead after the add; `d` should reuse its register.
        let f = func(vec![block(
            "entry",
            vec![
                assign("a", 1),
                assign("b", 2),
                add("c", "a", "b"),
                assign("d", 4),
                add("e", "c", "d"),
            ],
            IRTerminator::Return(Some(t("e"))),
        )]);
        let a = allocate(&f);
        assert_no_overlap_shares_a_register(&a);
        assert_eq!(a.n_slots, 0);
        // Four registers would be needed without reuse; three suffice.
        let used: HashSet<usize> = a.locs.values().filter_map(|l| l.reg()).collect();
        assert!(used.len() <= 4, "{:?}", a.locs);
    }

    #[test]
    fn a_value_live_across_a_call_is_spilled() {
        let f = func(vec![block(
            "entry",
            vec![
                assign("keep", 1),
                call(Some("r"), "g", Vec::new()),
                add("sum", "keep", "r"),
            ],
            IRTerminator::Return(Some(t("sum"))),
        )]);
        let a = allocate(&f);
        assert!(
            a.loc("keep").unwrap().is_slot(),
            "`keep` is live across the call and must not be in a register: {:?}",
            a.locs
        );
        assert_eq!(a.spilled_across_call, 1);
        // The call's own RESULT is not live across it.
        assert!(a.loc("r").unwrap().reg().is_some(), "{:?}", a.locs);
    }

    #[test]
    fn a_value_that_only_reaches_a_call_is_not_spilled() {
        // `arg` dies at the call, so it never has to survive one.
        let f = func(vec![block(
            "entry",
            vec![assign("arg", 1), call(Some("r"), "g", vec![t("arg")])],
            IRTerminator::Return(Some(t("r"))),
        )]);
        let a = allocate(&f);
        assert_eq!(a.spilled_across_call, 0, "{:?}", a.locs);
        assert!(a.loc("arg").unwrap().reg().is_some(), "{:?}", a.locs);
    }

    #[test]
    fn register_pressure_beyond_the_pool_spills_rather_than_reusing() {
        // Define more simultaneously-live values than the pool holds.
        let n = (POOL_HI - POOL_LO + 1) + 5;
        let mut instrs: Vec<IRInstr> = (0..n).map(|i| assign(&format!("v{}", i), i as i64)).collect();
        // One long chain of adds keeps them all live to the end.
        instrs.push(assign("acc", 0));
        for i in 0..n {
            instrs.push(add(&format!("s{}", i), "acc", &format!("v{}", i)));
        }
        let f = func(vec![block("entry", instrs, IRTerminator::Return(Some(t("acc"))))]);
        let a = allocate(&f);
        assert!(a.n_slots > 0, "the pool cannot hold {} live values", n);
        assert_no_overlap_shares_a_register(&a);
    }

    /// entry → head → (body → head | exit). The loop-carried value must be live
    /// across the whole loop, not just from its definition to its last textual
    /// use — which is the mistake P14 was.
    #[test]
    fn a_loop_carried_phi_and_its_operand_do_not_share_a_register() {
        let f = func(vec![
            block("entry", vec![assign("init", 0)], IRTerminator::Jump("head".into())),
            block(
                "head",
                vec![
                    IRInstr::Phi {
                        dst: IRTemp::new("i"),
                        ty: IRType::I64,
                        incoming: vec![(t("init"), "entry".into()), (t("next"), "body".into())],
                    },
                    assign("one", 1),
                ],
                IRTerminator::BinBranch {
                    cond: t("i"),
                    true_label: "body".into(),
                    false_label: "exit".into(),
                },
            ),
            block("body", vec![add("next", "i", "one")], IRTerminator::Jump("head".into())),
            block("exit", Vec::new(), IRTerminator::Return(Some(t("i")))),
        ]);
        let a = allocate(&f);
        assert_no_overlap_shares_a_register(&a);

        // `next` is written in the body and read by the phi copy at the body's
        // terminator, so it must still be live there.
        let body_end = a.block_end[2];
        assert!(
            a.intervals["next"].covers(body_end),
            "`next` must reach the back edge: {:?}",
            a.intervals["next"]
        );
        // The phi destination is written by BOTH predecessors, so its range has
        // to reach back to the entry block's terminator.
        let entry_end = a.block_end[0];
        assert!(
            a.intervals["i"].covers(entry_end),
            "`i` must be live where `entry` writes it: {:?}",
            a.intervals["i"]
        );
        assert_ne!(a.loc("i"), a.loc("next"), "they overlap; different homes");
    }

    #[test]
    fn a_value_live_only_in_one_arm_does_not_reserve_a_register_in_the_other() {
        let f = func(vec![
            block(
                "entry",
                vec![assign("c", 1)],
                IRTerminator::BinBranch {
                    cond: t("c"),
                    true_label: "then".into(),
                    false_label: "els".into(),
                },
            ),
            block(
                "then",
                vec![assign("x", 1), add("tx", "x", "x")],
                IRTerminator::Return(Some(t("tx"))),
            ),
            block(
                "els",
                vec![assign("y", 2), add("ty", "y", "y")],
                IRTerminator::Return(Some(t("ty"))),
            ),
        ]);
        let a = allocate(&f);
        assert_no_overlap_shares_a_register(&a);
        assert_eq!(a.n_slots, 0);
    }

    #[test]
    fn a_parameter_stays_in_the_register_it_arrived_in() {
        // No prologue moves, and therefore no prologue parallel-copy problem.
        let mut f = func(vec![block(
            "entry",
            vec![add("c", "param_a", "param_b")],
            IRTerminator::Return(Some(t("c"))),
        )]);
        f.params = vec![("a".into(), IRType::I64), ("b".into(), IRType::I64)];
        let a = allocate(&f);
        assert_eq!(a.loc("param_a"), Some(Loc::Reg(1)));
        assert_eq!(a.loc("param_b"), Some(Loc::Reg(2)));
        assert!(a.param_slots.is_empty());
        assert_no_overlap_shares_a_register(&a);
    }

    /// A float parameter used by a float multiply may NOT stay in R1, even
    /// though that multiply is its last use.
    ///
    /// The syscall writes R1 while setting up and reads the operands after, so
    /// "live across" is not the question — "live INTO" is. `math::to_radians`
    /// returned (PI/180)^2 when this was got wrong: the constant landed in R1
    /// on top of the argument and the multiply squared it.
    #[test]
    fn a_float_parameter_consumed_by_a_syscall_op_does_not_stay_in_r1() {
        let mut f = func(vec![block(
            "entry",
            vec![IRInstr::BinOp {
                dst: IRTemp("c".into()),
                op: IRBinOp::Mul,
                lhs: IRValue::Temp(IRTemp("param_x".into())),
                rhs: IRValue::Const(IRConst::Float(0.017_453_292_519_943_295)),
                ty: IRType::F64,
            }],
            IRTerminator::Return(Some(t("c"))),
        )]);
        f.params = vec![("x".into(), IRType::F64)];
        let a = allocate(&f);
        assert!(
            a.loc("param_x").unwrap().is_slot(),
            "a float parameter consumed by a float syscall must not sit in R1, got {:?}",
            a.loc("param_x")
        );
        assert_eq!(a.param_slots.len(), 1, "the prologue must store it");
    }

    /// The same shape with INTEGER arithmetic keeps its register.
    ///
    /// This is the other half of the trade: `TADD` is a real instruction and
    /// touches nothing but its operands, so refusing R1 here would spill the
    /// first three parameters of every arithmetic function for nothing.
    #[test]
    fn an_int_parameter_consumed_by_plain_arithmetic_keeps_r1() {
        let mut f = func(vec![block(
            "entry",
            vec![add("c", "param_a", "param_b")],
            IRTerminator::Return(Some(t("c"))),
        )]);
        f.params = vec![("a".into(), IRType::I64), ("b".into(), IRType::I64)];
        let a = allocate(&f);
        assert_eq!(a.loc("param_a"), Some(Loc::Reg(1)));
        assert_eq!(a.loc("param_b"), Some(Loc::Reg(2)));
        assert!(a.param_slots.is_empty());
    }

    #[test]
    fn a_parameter_in_the_pool_reserves_its_register() {
        // The fourth parameter arrives in R4, which IS in the pool. Nothing
        // else may be given R4 while it is live.
        let mut f = func(vec![block(
            "entry",
            vec![assign("x", 1), add("c", "param_d", "x")],
            IRTerminator::Return(Some(t("c"))),
        )]);
        f.params = vec![
            ("a".into(), IRType::I64),
            ("b".into(), IRType::I64),
            ("c".into(), IRType::I64),
            ("d".into(), IRType::I64),
        ];
        let a = allocate(&f);
        assert_eq!(a.loc("param_d"), Some(Loc::Reg(4)));
        assert_ne!(a.loc("x"), Some(Loc::Reg(4)), "x must not take the parameter's register");
        assert_no_overlap_shares_a_register(&a);
    }

    #[test]
    fn a_parameter_live_across_a_call_gets_a_slot_and_a_prologue_store() {
        let mut f = func(vec![block(
            "entry",
            vec![call(Some("r"), "g", Vec::new()), add("c", "param_a", "r")],
            IRTerminator::Return(Some(t("c"))),
        )]);
        f.params = vec![("a".into(), IRType::I64)];
        let a = allocate(&f);
        assert!(a.loc("param_a").unwrap().is_slot(), "{:?}", a.locs);
        assert_eq!(a.param_slots.len(), 1);
        assert_eq!(a.param_slots[0].0, 1, "it arrived in R1");
    }

    #[test]
    fn a_parameter_is_live_from_entry() {
        let mut f = func(vec![
            block("entry", Vec::new(), IRTerminator::Jump("body".into())),
            block(
                "body",
                vec![add("c", "param_x", "param_x")],
                IRTerminator::Return(Some(t("c"))),
            ),
        ]);
        f.params = vec![("x".into(), IRType::I64)];
        let a = allocate(&f);
        assert_eq!(a.intervals["param_x"].start, 0, "{:?}", a.intervals["param_x"]);
        assert_no_overlap_shares_a_register(&a);
    }

    #[test]
    fn allocation_is_deterministic() {
        let build = || {
            func(vec![block(
                "entry",
                vec![
                    assign("zeta", 1),
                    assign("alpha", 2),
                    assign("mu", 3),
                    add("s1", "zeta", "alpha"),
                    add("s2", "s1", "mu"),
                ],
                IRTerminator::Return(Some(t("s2"))),
            )])
        };
        let a = allocate(&build());
        let b = allocate(&build());
        let mut la: Vec<(&String, &Loc)> = a.locs.iter().collect();
        let mut lb: Vec<(&String, &Loc)> = b.locs.iter().collect();
        la.sort();
        lb.sort();
        assert_eq!(la, lb, "the same input must produce the same assignment");
    }

    #[test]
    fn the_free_mask_agrees_with_the_assignment() {
        let f = func(vec![block(
            "entry",
            vec![assign("a", 1), assign("b", 2), add("c", "a", "b")],
            IRTerminator::Return(Some(t("c"))),
        )]);
        let a = allocate(&f);
        for (name, loc) in &a.locs {
            let Some(r) = loc.reg() else { continue };
            let iv = a.intervals[name];
            for i in iv.start..=iv.end {
                assert!(
                    !a.free_regs_at(i).contains(&r),
                    "R{} holds {} at {} but the mask calls it free",
                    r, name, i
                );
            }
        }
        // …and a register nothing was given is free everywhere.
        let used: HashSet<usize> = a.locs.values().filter_map(|l| l.reg()).collect();
        let unused = (POOL_LO..=POOL_HI).find(|r| !used.contains(r)).expect("some register is spare");
        assert!(a.free_regs_at(0).contains(&unused));
    }

    #[test]
    fn no_allocated_register_is_outside_the_pool() {
        let n = (POOL_HI - POOL_LO + 1) + 3;
        let mut instrs: Vec<IRInstr> = (0..n).map(|i| assign(&format!("v{}", i), i as i64)).collect();
        instrs.push(assign("acc", 0));
        for i in 0..n {
            instrs.push(add(&format!("s{}", i), "acc", &format!("v{}", i)));
        }
        let f = func(vec![block("entry", instrs, IRTerminator::Return(Some(t("acc"))))]);
        let a = allocate(&f);
        for (name, loc) in &a.locs {
            if let Some(r) = loc.reg() {
                assert!(
                    (POOL_LO..=POOL_HI).contains(&r),
                    "{} was given R{}, outside the pool",
                    name, r
                );
            }
        }
    }
}
