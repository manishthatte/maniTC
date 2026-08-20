// emitter/mod.rs — AsmEmitter struct, register allocation helpers, and assembly emission entry points.
mod liveness;
use liveness::{compute_last_use, RegAlloc};
mod emit_instr;
use emit_instr::{emit_instr, emit_term};

use crate::ir::*;
use std::collections::HashMap;

/// Where a call operand lives, resolved before the caller-save stores run and
/// materialised after them by `emit_call_operands`.
#[derive(Clone)]
enum CallSrc {
    Reg(usize),      // live in a pool register
    Slot(usize),     // spilled, at this absolute stack depth
    Lit(i64),        // plain immediate
    Label(String),   // string literal or global address, resolved by the assembler
    Float(f64),      // needs the float-load syscall
    Missing(String), // temp with no known location — an SSA hole
}

struct AsmEmitter {
    lines:        Vec<String>, // final committed output
    cur_instr:    Vec<String>, // staged body of the current IR instruction
    pending_post: Vec<String>, // post-instruction spill stores
    pending_sp_increments: usize, // spill stores not yet added to sp_depth
    reg_alloc:    RegAlloc,
    spill_read_idx: usize,     // 0 → use R21, 1 → use R22, 2+ → use R25
    /// Canonical sp_depth for each block label, set by the FIRST predecessor that jumps to it.
    /// At block entry we reset sp_depth to this value so all paths agree on stack layout.
    block_canonical_sp: HashMap<String, usize>,
    /// Canonical register assignment for each block, set at the FIRST entry to that block.
    /// Used to restore the register state when a backward edge (loop back-edge) jumps to a
    /// block that has already been emitted.  Without this, rescue moves done in the first
    /// iteration of a loop body corrupt the allocator state for subsequent iterations.
    block_canonical_regs: HashMap<String, (HashMap<String, usize>, HashMap<String, usize>)>,
    /// Current function name, used to qualify block labels (prevents collisions across fns).
    fn_name: String,
    /// Struct name → number of fields (word size on stack).
    struct_sizes: HashMap<String, usize>,
    /// Alloca slots: temp name → sp_depth at alloca time.
    /// Used by Load lowering to emit SP-relative loads ([R26+#offset]) instead of
    /// register-based loads, which avoids register aliasing bugs across loop back-edges.
    alloca_slots: HashMap<String, usize>,
    /// Phi copies: (pred_label, succ_label) → [(phi_dst, incoming_val)].
    /// Emitted before the JUMP in the predecessor block to properly resolve phi nodes.
    phi_copies: HashMap<(String, String), Vec<(IRTemp, IRValue)>>,
    /// Current block label, used for phi copy lookup.
    current_block_label: String,
    /// Float literals collected during this function's emit pass.
    /// Merged into the module's float_literals at the end of emit_function.
    float_literals: Vec<(String, i64)>,
    /// Set of block labels that have already been emitted (for loop back-edge detection).
    emitted_blocks: std::collections::HashSet<String>,
    /// Module global variables: name → absolute memory address (GLOBALS_BASE..).
    global_addrs: HashMap<String, i64>,
    /// Temps relocated by rescue moves since function start. Back-edge jumps
    /// consult this to restore only genuinely displaced temps to the target
    /// block's canonical registers (see emit_term's Jump reconciliation).
    rescued_temps: std::collections::HashSet<String>,
}

impl AsmEmitter {
    fn new(
        last_use: HashMap<String, usize>,
        first_def: HashMap<String, usize>,
        struct_sizes: HashMap<String, usize>,
    ) -> Self {
        AsmEmitter {
            lines: Vec::new(),
            cur_instr: Vec::new(),
            pending_post: Vec::new(),
            pending_sp_increments: 0,
            reg_alloc: RegAlloc::new(last_use, first_def),
            spill_read_idx: 0,
            block_canonical_sp: HashMap::new(),
            block_canonical_regs: HashMap::new(),
            fn_name: String::new(),
            struct_sizes,
            alloca_slots: HashMap::new(),
            phi_copies: HashMap::new(),
            current_block_label: String::new(),
            float_literals: Vec::new(),
            emitted_blocks: std::collections::HashSet::new(),
            global_addrs: HashMap::new(),
            rescued_temps: std::collections::HashSet::new(),
        }
    }

    /// Returns a globally-unique label by prefixing with the current function name.
    fn qlabel(&self, label: &str) -> String {
        format!("{}_{}", self.fn_name, label)
    }

    /// Register the canonical sp_depth for a target label (first-predecessor wins).
    /// Returns the canonical sp_depth for that label.
    fn register_target(&mut self, label: &str) -> usize {
        let cur = self.reg_alloc.sp_depth;
        *self.block_canonical_sp.entry(label.to_string()).or_insert(cur)
    }

    /// Stage a line for the current instruction body.
    fn emit(&mut self, line: impl Into<String>) {
        self.cur_instr.push(line.into());
    }

    /// Like rescue_reg but also rescues temps whose last use IS the current
    /// instruction (lu == cur).  Use this before float syscall setup so that
    /// a temp that is consumed by this instruction isn't overwritten by the
    /// R1-based float literal loading that happens inside val_reg.
    fn rescue_reg_inclusive(&mut self, reg: usize) {
        let cur = self.reg_alloc.current_instr;
        let victim: Option<String> = self.reg_alloc.temp_to_reg
            .iter()
            .find(|(name, &r)| {
                r == reg
                    && self.reg_alloc.holds_value(name)
                    && self.reg_alloc.last_use
                        .get(*name)
                        .map_or(false, |&lu| lu >= cur)
            })
            .map(|(n, _)| n.clone());

        if let Some(name) = victim {
            // Never rescue INTO R1-R3: they are the syscall argument/return
            // registers this rescue is protecting against, and a value parked
            // there would be clobbered by the very sequence that follows.
            let free_r = self.reg_alloc.free_regs.iter().copied().find(|&r| r != reg && r > 3);
            if let Some(fr) = free_r {
                self.reg_alloc.free_regs.remove(&fr);
                self.reg_alloc.temp_to_reg.insert(name.clone(), fr);
                self.rescued_temps.insert(name.clone());
                self.emit(format!(
                    "    MOV   R{}, R{}  ; rescue-inc {} from R{}",
                    fr, reg, name, reg
                ));
            } else {
                let slot = self.reg_alloc.sp_depth + 1;
                self.reg_alloc.spill_slots.insert(name.clone(), slot);
                self.reg_alloc.temp_to_reg.remove(&name);
                self.reg_alloc.spill_count += 1;
                self.rescued_temps.insert(name.clone());
                self.emit(format!("    TSUB  R26, R26, #1  ; rescue-inc-spill {} from R{}", name, reg));
                self.emit(format!("    STORE R{}, [R26+#0]  ; rescue-inc-spill {}", reg, name));
                self.reg_alloc.sp_depth += 1;
            }
        }
    }

    /// If any live temp (last_use > current_instr) is currently in `reg`,
    /// move it to a free register or spill it.  Call this before any code
    /// that writes `reg` as a side-effect (syscall arg setup, etc.).
    fn rescue_reg(&mut self, reg: usize) {
        let cur = self.reg_alloc.current_instr;
        let victim: Option<String> = self.reg_alloc.temp_to_reg
            .iter()
            .find(|(name, &r)| {
                r == reg
                    && self.reg_alloc.holds_value(name)
                    && self.reg_alloc.last_use
                        .get(*name)
                        .map_or(false, |&lu| lu > cur)
            })
            .map(|(n, _)| n.clone());

        if let Some(name) = victim {
            // Try to find a free register different from `reg`.  R1-R3 are
            // excluded: they are syscall argument/return registers, so parking
            // a rescued value there would defeat the rescue (see rescue_reg_inclusive).
            let free_r = self.reg_alloc.free_regs.iter().copied().find(|&r| r != reg && r > 3);
            if let Some(fr) = free_r {
                self.reg_alloc.free_regs.remove(&fr);
                self.reg_alloc.temp_to_reg.insert(name.clone(), fr);
                self.rescued_temps.insert(name.clone());
                self.emit(format!(
                    "    MOV   R{}, R{}  ; rescue {} from R{} (syscall clobber)",
                    fr, reg, name, reg
                ));
            } else {
                // Pool exhausted — spill to stack.
                let slot = self.reg_alloc.sp_depth + 1;
                self.reg_alloc.spill_slots.insert(name.clone(), slot);
                self.reg_alloc.temp_to_reg.remove(&name);
                self.reg_alloc.spill_count += 1;
                self.rescued_temps.insert(name.clone());
                self.emit(format!("    TSUB  R26, R26, #1  ; rescue-spill {} from R{}", name, reg));
                self.emit(format!("    STORE R{}, [R26+#0]  ; rescue-spill {}", reg, name));
                self.reg_alloc.sp_depth += 1;
            }
        }
    }

    /// Save each register in `regs` to the stack IF it currently holds a temp
    /// that is live past this instruction.  Returns the list of saved registers;
    /// pass it to `emit_reg_restore` after the clobbering sequence.
    ///
    /// Unlike rescue_reg, the allocator mapping is left untouched — the values
    /// come back into the SAME registers, so cross-block register conventions
    /// (e.g. phi-copy destinations fixed by an earlier predecessor) survive.
    /// Used by inline syscall expansions (StrEq/StrNe) that clobber R1/R2
    /// without going through the caller-save path.
    fn emit_reg_save(&mut self, regs: &[usize], note: &str) -> Vec<usize> {
        let cur = self.reg_alloc.current_instr;
        let to_save: Vec<usize> = regs.iter().copied().filter(|&r| {
            self.reg_alloc.temp_to_reg.iter().any(|(name, &tr)| {
                tr == r
                    && self.reg_alloc.holds_value(name)
                    && self.reg_alloc.last_use
                        .get(name)
                        .map_or(false, |&lu| lu > cur)
            })
        }).collect();
        if !to_save.is_empty() {
            self.emit(format!("    TSUB  R26, R26, #{}  ; save live regs ({})", to_save.len(), note));
            for (i, &r) in to_save.iter().enumerate() {
                self.emit(format!("    STORE R{}, [R26+#{}]  ; save R{} ({})", r, i, r, note));
            }
        }
        to_save
    }

    /// Restore registers saved by `emit_reg_save` (same order) and pop the slots.
    fn emit_reg_restore(&mut self, saved: &[usize], note: &str) {
        if saved.is_empty() { return; }
        for (i, &r) in saved.iter().enumerate() {
            self.emit(format!("    LOAD  R{}, [R26+#{}]  ; restore R{} ({})", r, i, r, note));
        }
        self.emit(format!("    TADD  R26, R26, #{}  ; pop saved regs ({})", saved.len(), note));
    }

    /// Commit current instruction: move cur_instr + pending_post to lines.
    fn flush_instr(&mut self) {
        self.lines.extend(self.cur_instr.drain(..));
        self.lines.extend(self.pending_post.drain(..));
        self.reg_alloc.sp_depth += self.pending_sp_increments;
        self.pending_sp_increments = 0;
        self.spill_read_idx = 0;
        self.reg_alloc.scratch_fallback_count = 0;
    }

    fn rn(r: usize) -> String { format!("R{}", r) }

    /// Allocate an anonymous scratch register for this instruction.
    fn scratch(&mut self) -> usize {
        self.reg_alloc.alloc_scratch()
    }

    /// Emit instructions to `self.lines` that load `imm` into register `r`.
    /// Single TLIT when |imm| ≤ 797161 (the balanced 13-trit wide-imm bound).
    /// Larger values are decomposed recursively as hi*1000 + lo using
    /// TLIT+TMUL+TLIT+TADD (all register forms — the TADD 3-trit imm field
    /// can only hold [-13,13]), so the full i64 range is handled.  Note the
    /// machine clamps arithmetic to the 27-trit range at runtime.
    fn emit_lit(&mut self, r: usize, imm: i64) {
        const MAX_LIT: i64 = crate::codegen_t3::isa::WIDE_IMM_MAX; // 797_161 = (3^13 − 1)/2
        if imm.abs() <= MAX_LIT {
            self.lines.push(format!("    TLIT  {}, #{}", Self::rn(r), imm));
        } else {
            let hi = imm / 1000;
            let lo = imm - hi * 1000;
            let tmp = self.scratch();
            // Materialize hi first (recursively — hi may itself exceed the
            // TLIT range for |imm| > ~797 billion), then scale and add lo.
            self.emit_lit(tmp, hi);
            let (rn_r, rn_t) = (Self::rn(r), Self::rn(tmp));
            self.lines.push(format!("    TLIT  {}, #1000  ; large-lit scale", rn_r));
            self.lines.push(format!("    TMUL  {0}, {1}, {0}  ; large-lit r=hi*1000", rn_r, rn_t));
            if lo != 0 {
                self.lines.push(format!("    TLIT  {}, #{}  ; large-lit lo", rn_t, lo));
                self.lines.push(format!("    TADD  {0}, {0}, {1}  ; large-lit r+=lo", rn_r, rn_t));
            }
        }
    }

    /// Emit a SP adjustment of `delta` words (positive = TADD = pop, negative = TSUB = push).
    /// The balanced 3-trit immediate field holds -13..13; for larger values uses R21 as scratch.
    fn emit_sp_adj(&mut self, delta: i64) {
        if delta == 0 { return; }
        if delta > 0 {
            if delta <= 13 {
                self.emit(format!("    TADD  R26, R26, #{}  ; sp pop {}", delta, delta));
            } else {
                self.emit(format!("    TLIT  R21, #{}  ; sp-adj large", delta));
                self.emit(format!("    TADD  R26, R26, R21  ; sp pop {}", delta));
            }
        } else {
            let n = -delta;
            if n <= 13 {
                self.emit(format!("    TSUB  R26, R26, #{}  ; sp push {}", n, n));
            } else {
                self.emit(format!("    TLIT  R21, #{}  ; sp-adj large", n));
                self.emit(format!("    TSUB  R26, R26, R21  ; sp push {}", n));
            }
        }
    }

    /// `emit_lit`, but into the current instruction body rather than `lines`.
    ///
    /// Call operands must be materialised *after* the caller-save stores have
    /// run, so they cannot use `emit_lit` (which writes to `lines`, i.e. ahead of
    /// the saves).  The decomposition scratch is taken from R23/R21/R22 by
    /// recursion depth — all three are dead during call operand setup, and a
    /// depth-indexed choice keeps an inner level from clobbering its caller.
    fn emit_lit_cur(&mut self, r: usize, imm: i64) {
        self.emit_lit_cur_at(r, imm, 0);
    }

    fn emit_lit_cur_at(&mut self, r: usize, imm: i64, depth: usize) {
        const MAX_LIT: i64 = crate::codegen_t3::isa::WIDE_IMM_MAX; // 797_161
        const SCRATCH: [usize; 3] = [23, 21, 22];
        if imm.abs() <= MAX_LIT {
            self.emit(format!("    TLIT  R{}, #{}", r, imm));
            return;
        }
        // |imm| is bounded by the 27-trit clamp, so this recurses at most twice.
        let tmp = SCRATCH[depth.min(SCRATCH.len() - 1)];
        let hi = imm / 1000;
        let lo = imm - hi * 1000;
        self.emit_lit_cur_at(tmp, hi, depth + 1);
        self.emit(format!("    TLIT  R{}, #1000  ; large-lit scale", r));
        self.emit(format!("    TMUL  R{0}, R{1}, R{0}  ; large-lit r=hi*1000", r, tmp));
        if lo != 0 {
            self.emit(format!("    TLIT  R{}, #{}  ; large-lit lo", tmp, lo));
            self.emit(format!("    TADD  R{0}, R{0}, R{1}  ; large-lit r+=lo", r, tmp));
        }
    }

    /// Load the value spilled at absolute stack depth `slot_depth` into `reg`,
    /// SP-relative against the *current* sp_depth.
    fn emit_slot_load(&mut self, reg: usize, slot_depth: usize, note: &str) {
        let offset = self.reg_alloc.sp_depth as i64 - slot_depth as i64;
        if offset >= 0 && offset <= 13 {
            self.emit(format!("    LOAD  R{}, [R26+#{}]  ; {}", reg, offset, note));
        } else if offset > 13 {
            self.emit(format!("    TLIT  R{}, #{}", reg, offset));
            self.emit(format!("    TADD  R{0}, R26, R{0}  ; spill addr", reg));
            self.emit(format!("    LOAD  R{0}, [R{0}+#0]  ; {1}", reg, note));
        } else {
            self.emit(format!("    ; BUG: negative spill offset {} for {}", offset, note));
            self.emit(format!("    TLIT  R{}, #0", reg));
        }
    }

    /// Resolve where a call operand lives, without emitting anything.
    ///
    /// Must be called before the caller-save stores run: `Reg` names the register
    /// the value is in right now, which stays valid across the saves because a
    /// STORE copies a register rather than clearing it.
    fn resolve_call_src(&self, val: &IRValue) -> CallSrc {
        match val {
            IRValue::Temp(t) => {
                if let Some(&r) = self.reg_alloc.temp_to_reg.get(&t.0) {
                    CallSrc::Reg(r)
                } else if let Some(&d) = self.reg_alloc.spill_slots.get(&t.0) {
                    CallSrc::Slot(d)
                } else {
                    CallSrc::Missing(t.0.clone())
                }
            }
            IRValue::Const(IRConst::Str(label)) => {
                CallSrc::Label(label.trim_start_matches('@').to_string())
            }
            IRValue::Const(IRConst::Float(f)) => CallSrc::Float(*f),
            IRValue::Const(c) => CallSrc::Lit(irconst_to_i64(c)),
            IRValue::Global(name) => match self.global_addrs.get(name) {
                Some(&addr) => CallSrc::Lit(addr),
                // Unknown global: keep the symbolic form so the assembler fails
                // loudly instead of silently reading address 0.
                None => CallSrc::Label(name.clone()),
            },
            IRValue::Void => CallSrc::Lit(0),
        }
    }

    /// Materialise call operands into their target registers.
    ///
    /// Runs *after* the caller-save stores, when every pool register holding a
    /// live value is already on the stack and so free to overwrite.  The phase
    /// order is load-bearing:
    ///
    ///   0. float constants, which need R1 and the float-load syscall, so they
    ///      must run before any argument register has been written;
    ///   1. register→register moves, sequentialised so no move overwrites a
    ///      register another still needs (cycles broken through R23);
    ///   2. stack reloads and immediates, which read no pool register and so
    ///      cannot disturb a move that is still pending.
    ///
    /// Operands used to be materialised *before* the saves instead, which forced
    /// them to be parked in R21/R22/R24/R25 — the very registers the call
    /// sequence uses for the fn_ptr, the move scratch and the return stash.  From
    /// the third spilled operand onward `val_reg` handed out R25, and the fn_ptr
    /// move then overwrote it, passing the callee a silently wrong argument.
    fn emit_call_operands(&mut self, targets: &[(usize, CallSrc)]) {
        // Staging for phase 0 lives above the argument registers.  Parameters are
        // allocated R1..R8 (see emit_function), so R9+ is never a target.
        const STAGE_BASE: usize = 9;
        assert!(
            targets.iter().all(|&(t, _)| t < STAGE_BASE || t == 25),
            "T3ISA internal error: call operand target R{} overlaps the staging range",
            targets.iter().map(|&(t, _)| t).find(|&t| t >= STAGE_BASE && t != 25).unwrap_or(0)
        );

        let mut resolved: Vec<(usize, CallSrc)> = Vec::with_capacity(targets.len());
        let mut stage = STAGE_BASE;
        for (tgt, src) in targets {
            match src {
                CallSrc::Float(f) => {
                    // Float bit patterns exceed TLIT's range, so they are stored in
                    // the .float section and fetched by syscall.  R1 is dead here:
                    // anything live in it went to the stack in the caller-saves.
                    let bits = f.to_bits() as i64;
                    let label = format!("@float_{}_{}", self.fn_name, self.float_literals.len());
                    let clean = label.trim_start_matches('@').to_string();
                    self.float_literals.push((label, bits));
                    self.emit(format!("    TLIT  R1, #{}  ; float-lit addr", clean));
                    self.emit(format!("    SYSCALL #219  ; float_load bits for {}", f));
                    self.emit(format!("    MOV   R{}, R1  ; stage float operand", stage));
                    resolved.push((*tgt, CallSrc::Reg(stage)));
                    stage += 1;
                }
                other => resolved.push((*tgt, other.clone())),
            }
        }

        // Phase 1: register→register moves.
        const CYCLE: usize = 23; // spill-write scratch — dead during operand setup
        let mut pending: Vec<(usize, usize)> = resolved
            .iter()
            .filter_map(|(tgt, src)| match src {
                CallSrc::Reg(r) if r != tgt => Some((*tgt, *r)),
                _ => None,
            })
            .collect();

        while !pending.is_empty() {
            let sources: std::collections::HashSet<usize> =
                pending.iter().map(|&(_, s)| s).collect();
            // A move is ready once its target is no longer needed as a source.
            if let Some(i) = pending.iter().position(|&(tgt, _)| !sources.contains(&tgt)) {
                let (tgt, src) = pending.remove(i);
                self.emit(format!("    MOV   R{}, R{}  ; call operand", tgt, src));
            } else {
                // Every remaining move is part of a cycle.  Park one target in the
                // scratch, close its move, and redirect readers of it to the scratch.
                // Breaking a cycle leaves a chain whose head is immediately ready, so
                // this branch cannot run again while the scratch is still needed.
                let (tgt0, src0) = pending.remove(0);
                self.emit(format!("    MOV   R{}, R{}  ; break move cycle at R{}", CYCLE, tgt0, tgt0));
                self.emit(format!("    MOV   R{}, R{}  ; call operand", tgt0, src0));
                for m in pending.iter_mut() {
                    if m.1 == tgt0 {
                        m.1 = CYCLE;
                    }
                }
            }
        }

        // Phase 2: everything that reads no pool register.
        for (tgt, src) in &resolved {
            match src {
                CallSrc::Reg(_) | CallSrc::Float(_) => {}
                CallSrc::Slot(d) => self.emit_slot_load(*tgt, *d, "call operand from spill"),
                CallSrc::Lit(v) => self.emit_lit_cur(*tgt, *v),
                CallSrc::Label(l) => self.emit(format!("    TLIT  R{}, #{}  ; call operand", tgt, l)),
                CallSrc::Missing(name) => {
                    self.emit(format!("    ; BUG: call operand {} has no location", name));
                    self.emit(format!("    TLIT  R{}, #0", tgt));
                }
            }
        }
    }

    /// Get or assign a register for an IRTemp destination.
    /// If the pool is exhausted, schedules a spill: uses R23 as the physical
    /// destination, and appends TSUB+STORE to pending_post to save R23 to the stack.
    fn dst_reg(&mut self, t: &IRTemp) -> usize {
        // Already in a register?
        if let Some(&r) = self.reg_alloc.temp_to_reg.get(&t.0) {
            return r;
        }
        // Already spilled: reuse R23 as the physical destination, but write the
        // result back to the slot the value already owns.  Returning R23 with no
        // store silently dropped the assignment.  It bit phi destinations hardest:
        // when one predecessor of a join spilled the phi and a later predecessor
        // reached this branch, that predecessor's copy stayed in R23 and the slot
        // kept whatever unrelated temp happened to occupy it, so the join read the
        // wrong value with no trap and no diagnostic.
        if let Some(&slot) = self.reg_alloc.spill_slots.get(&t.0) {
            let offset = (self.reg_alloc.sp_depth + self.pending_sp_increments) as i64 - slot as i64;
            if offset >= 0 && offset <= 13 {
                self.pending_post.push(format!(
                    "    STORE R23, [R26+#{}]         ; spill-store {} (existing slot)",
                    offset, t.0
                ));
            } else if offset > 13 {
                // R21 is free after the instruction body: spill reads are done.
                self.pending_post.push(format!("    TLIT  R21, #{}", offset));
                self.pending_post.push("    TADD  R21, R26, R21  ; spill addr".to_string());
                self.pending_post.push(format!(
                    "    STORE R23, [R21+#0]         ; spill-store {} (existing slot)",
                    t.0
                ));
            } else {
                self.pending_post.push(format!(
                    "    ; BUG: negative spill offset {} for {}",
                    offset, t.0
                ));
            }
            return 23;
        }
        // Try to allocate from pool
        if let Some(&r) = self.reg_alloc.free_regs.iter().next() {
            self.reg_alloc.free_regs.remove(&r);
            self.reg_alloc.temp_to_reg.insert(t.0.clone(), r);
            return r;
        }
        // Pool exhausted: spill.  Use R23 as physical dest; store to stack after instruction.
        let future_depth = self.reg_alloc.sp_depth + 1;
        self.reg_alloc.spill_slots.insert(t.0.clone(), future_depth);
        self.reg_alloc.spill_count += 1;
        self.pending_sp_increments += 1;
        self.pending_post.push(format!("    TSUB  R26, R26, #1         ; spill-store {}", t.0));
        self.pending_post.push(format!("    STORE R23, [R26+#0]         ; spill-store {}", t.0));
        23
    }

    /// Materialise an IRValue into a register.  Returns the register number.
    /// For spilled temps, emits a LOAD directly to `lines` (before cur_instr body)
    /// using R21 / R22 as scratch (R25 as last-resort for 3rd spilled input).
    fn val_reg(&mut self, val: &IRValue) -> usize {
        match val {
            IRValue::Temp(t) => {
                // Already in a register?
                if let Some(&r) = self.reg_alloc.temp_to_reg.get(&t.0) {
                    return r;
                }
                // In a spill slot?
                if let Some(&slot_depth) = self.reg_alloc.spill_slots.get(&t.0) {
                    let scratch = match self.spill_read_idx {
                        0 => 21,
                        1 => 22,
                        _ => 25,
                    };
                    self.spill_read_idx += 1;
                    let offset = self.reg_alloc.sp_depth as i64 - slot_depth as i64;
                    self.lines.push(format!("    ; reload spill {} (offset {})", t.0, offset));
                    if offset >= 0 && offset <= 13 {
                        self.lines.push(format!("    LOAD  R{}, [R26+#{}]", scratch, offset));
                    } else if offset > 13 {
                        self.lines.push(format!("    TLIT  R{}, #{}", scratch, offset));
                        self.lines.push(format!("    TADD  R{0}, R26, R{0}  ; spill addr", scratch));
                        self.lines.push(format!("    LOAD  R{0}, [R{0}+#0]  ; spill load", scratch));
                    } else {
                        // negative offset = logic error, emit zero as fallback
                        self.lines.push(format!("    ; BUG: negative spill offset {}", offset));
                        self.lines.push(format!("    TLIT  R{}, #0", scratch));
                    }
                    return scratch;
                }
                // Not found — allocate fresh (shouldn't happen in correct SSA)
                if let Some(&r) = self.reg_alloc.free_regs.iter().next() {
                    self.reg_alloc.free_regs.remove(&r);
                    self.reg_alloc.temp_to_reg.insert(t.0.clone(), r);
                    r
                } else {
                    self.reg_alloc.alloc_scratch()
                }
            }
            IRValue::Const(c) => {
                let r = self.scratch();
                match c {
                    IRConst::Str(label) => {
                        let clean = label.trim_start_matches('@');
                        self.lines.push(format!("    TLIT  {}, #{}", Self::rn(r), clean));
                    }
                    IRConst::Float(f) => {
                        // Float bit patterns are 64-bit values that cannot be encoded via
                        // TLIT (limited to ~800K) or built via TMUL (clamps to T27 range).
                        // Instead, store the bits in the .float section and load via SYSCALL #219.
                        // IMPORTANT: emit to cur_instr (via self.emit) so these instructions are
                        // ordered after any rescue_reg_inclusive calls already in cur_instr.
                        let bits = f.to_bits() as i64;
                        let label = format!("@float_{}_{}", self.fn_name, self.float_literals.len());
                        let clean = label.trim_start_matches('@').to_string();
                        self.float_literals.push((label.clone(), bits));
                        // Rescue R1 if it holds a live value, then use R1 for the syscall.
                        self.rescue_reg(1);
                        self.emit(format!("    TLIT  R1, #{}  ; float-lit addr", clean));
                        self.emit(format!("    SYSCALL #219  ; float_load bits for {}", f));
                        if r != 1 {
                            self.emit(format!("    MOV   {}, R1  ; float-lit to dst", Self::rn(r)));
                        }
                    }
                    _ => {
                        let imm = irconst_to_i64(c);
                        self.emit_lit(r, imm);
                    }
                }
                r
            }
            IRValue::Global(name) => {
                let r = self.scratch();
                if let Some(addr) = self.global_addrs.get(name) {
                    self.lines
                        .push(format!("    TLIT  {}, #{}  ; &{}", Self::rn(r), addr, name));
                } else {
                    // Unknown global: keep the symbolic form so the assembler
                    // fails loudly instead of silently reading address 0.
                    self.lines.push(format!("    TLIT  {}, #{}", Self::rn(r), name));
                }
                r
            }
            IRValue::Void => 0,
        }
    }
}

fn irconst_to_i64(c: &IRConst) -> i64 {
    match c {
        IRConst::Int(n)   => *n,
        IRConst::Float(f) => f.to_bits() as i64,
        IRConst::Bool(b)  => if *b { 1 } else { 0 },
        IRConst::Trit(t)  => *t as i64,
        IRConst::Str(_)   => 0,
        IRConst::Null     => 0,
    }
}

// ---------------------------------------------------------------------------
// emit_t3_asm
// ---------------------------------------------------------------------------

pub fn emit_t3_asm(module: &IRModule) -> String {
    let mut out = String::new();
    out.push_str("; T3ISA assembly — generated by maniT compiler\n");
    out.push_str("; 27-trit balanced ternary word machine\n\n");

    // Globals live in a fixed memory window between the stack top (SP starts
    // at 60_000 and grows down) and the emulator's reserved areas (62_000+):
    // one word per global, initialized by a preamble emitted before main.
    const GLOBALS_BASE: i64 = 61_000;
    let mut global_addrs: HashMap<String, i64> = HashMap::new();
    for (i, g) in module.globals.iter().enumerate() {
        global_addrs.insert(g.name.clone(), GLOBALS_BASE + i as i64);
    }

    /// Materialize `imm` into `reg` (clobbering `tmp` for the large split),
    /// mirroring AsmEmitter::emit_lit's TLIT range handling.
    fn lit_into(out: &mut String, reg: &str, tmp: &str, imm: i64) {
        let max = crate::codegen_t3::isa::WIDE_IMM_MAX;
        if imm.abs() <= max {
            out.push_str(&format!("    TLIT  {}, #{}\n", reg, imm));
        } else {
            let hi = imm / 1000;
            let lo = imm - hi * 1000;
            lit_into(out, reg, tmp, hi);
            out.push_str(&format!("    TLIT  {}, #1000\n", tmp));
            out.push_str(&format!("    TMUL  {0}, {0}, {1}\n", reg, tmp));
            if lo != 0 {
                out.push_str(&format!("    TLIT  {}, #{}\n", tmp, lo));
                out.push_str(&format!("    TADD  {0}, {0}, {1}\n", reg, tmp));
            }
        }
    }

    let has_main = module.functions.iter().any(|f| f.name == "main" && !f.is_extern);
    let first_is_main = module.functions.iter()
        .find(|f| !f.is_extern)
        .map_or(false, |f| f.name == "main");

    if !module.globals.is_empty() {
        out.push_str("; globals init\n");
        for g in &module.globals {
            let init = match &g.init {
                Some(IRValue::Const(c)) => irconst_to_i64(c),
                _ => 0,
            };
            let addr = global_addrs[&g.name];
            lit_into(&mut out, "R1", "R3", init);
            out.push_str(&format!("    TLIT  R2, #{}  ; &{}\n", addr, g.name));
            out.push_str("    STORE R1, [R2+#0]\n");
        }
        // With an init preamble the program can no longer fall through into
        // the first function; always jump to main explicitly.
        if has_main {
            out.push_str("    JUMP  main  ; program entry point\n\n");
        }
    } else if has_main && !first_is_main {
        // Emit a JUMP to main as the very first instruction so that helper
        // functions defined before main don't get executed as the program entry.
        out.push_str("    JUMP  main  ; program entry point\n\n");
    }

    // Functions
    let mut all_float_literals: Vec<(String, i64)> = Vec::new();
    for func in &module.functions {
        if func.is_extern {
            out.push_str(&format!("; extern fn {}\n\n", func.name));
            continue;
        }
        let (fn_asm, fn_floats) =
            emit_function(func, module.struct_sizes.clone(), global_addrs.clone());
        out.push_str(&fn_asm);
        out.push('\n');
        all_float_literals.extend(fn_floats);
    }

    // Data section for string literals
    if !module.string_literals.is_empty() {
        out.push_str(".data:\n");
        for (label, content) in &module.string_literals {
            let clean = label.trim_start_matches('@');
            let escaped = content
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\n', "\\n")
                .replace('\t', "\\t")
                .replace('\r', "\\r");
            out.push_str(&format!("    {}: .string \"{}\"\n", clean, escaped));
        }
    }

    // Float section for float literals
    let combined_floats: Vec<(String, i64)> = module.float_literals.iter().cloned()
        .chain(all_float_literals.into_iter())
        .collect();
    if !combined_floats.is_empty() {
        out.push_str(".float:\n");
        for (label, bits) in &combined_floats {
            let clean = label.trim_start_matches('@');
            out.push_str(&format!("    {}: .float64 {}\n", clean, bits));
        }
    }

    out
}

fn emit_function(
    func: &IRFunction,
    struct_sizes: HashMap<String, usize>,
    global_addrs: HashMap<String, i64>,
) -> (String, Vec<(String, i64)>) {
    let (last_use, first_def) = compute_last_use(&func.blocks);
    let mut em = AsmEmitter::new(last_use, first_def, struct_sizes);
    em.fn_name = func.name.clone();
    em.global_addrs = global_addrs;

    // Pre-allocate param registers: first param → R1, second → R2, …, max R8.
    for (i, (pname, _)) in func.params.iter().enumerate() {
        let reg_num = (i + 1).min(8);
        em.reg_alloc.reserve(&format!("param_{}", pname), reg_num);
        em.reg_alloc.reserve(pname, reg_num);
    }

    // Precompute phi copies: for each Phi node, record what the predecessor must copy.
    // phi_copies[(pred_label, phi_block_label)] = [(phi_dst, incoming_val)]
    for block in &func.blocks {
        for instr in &block.instrs {
            if let IRInstr::Phi { dst, incoming, .. } = instr {
                for (val, pred_label) in incoming {
                    em.phi_copies
                        .entry((pred_label.clone(), block.label.clone()))
                        .or_default()
                        .push((dst.clone(), val.clone()));
                }
            }
        }
    }

    // Function label goes directly to output (not staged)
    em.lines.push(format!("{}:", func.name));

    for block in &func.blocks {
        emit_block(&mut em, block);
    }

    let float_lits = em.float_literals.clone();
    (em.lines.join("\n") + "\n", float_lits)
}

fn emit_block(em: &mut AsmEmitter, block: &IRBlock) {
    // Restore sp_depth to the canonical value for this block (set by first predecessor).
    // This ensures all code-gen within the block uses consistent spill offsets regardless
    // of which execution path reaches it at runtime.
    if let Some(&canonical) = em.block_canonical_sp.get(&block.label) {
        em.reg_alloc.sp_depth = canonical;
    }

    // If this block has already been emitted (a loop back-edge target), restore the
    // register assignment to the canonical state recorded on the first visit.
    // This prevents rescue moves from one iteration contaminating the next.
    if em.emitted_blocks.contains(&block.label) {
        if let Some((regs, spills)) = em.block_canonical_regs.get(&block.label).cloned() {
            em.reg_alloc.temp_to_reg = regs;
            em.reg_alloc.spill_slots = spills;
        }
    }

    em.current_block_label = block.label.clone();

    // Snapshot register assignment at block entry (first visit only).
    // Two purposes:
    // 1. Loop header restoration (see above): on a back-edge, we restore to this snapshot
    //    so every iteration starts with the same register assignment.
    // 2. Return-block isolation: if this block terminates with Return, rescue moves inside
    //    it must NOT carry over to the next linearly-emitted block (which is not a successor).
    let reg_snapshot = em.reg_alloc.temp_to_reg.clone();
    let spill_snapshot = em.reg_alloc.spill_slots.clone();

    // Record canonical register state on first visit.
    em.block_canonical_regs
        .entry(block.label.clone())
        .or_insert_with(|| (reg_snapshot.clone(), spill_snapshot.clone()));
    em.emitted_blocks.insert(block.label.clone());

    // Block label goes directly to output (not staged)
    em.lines.push(format!("  {}:", em.qlabel(&block.label)));
    for instr in &block.instrs {
        emit_instr(em, instr);
        em.flush_instr();
        em.reg_alloc.advance();
    }
    emit_term(em, &block.term);
    em.flush_instr();
    em.reg_alloc.advance();

    // If this block ends in Return, the next emitted block is not a control-flow
    // successor.  Restore the register mapping to the block-entry snapshot so that
    // rescue moves performed inside this dead-end block do not corrupt the allocator
    // state for the next block.
    if matches!(block.term, IRTerminator::Return(_)) {
        em.reg_alloc.temp_to_reg = reg_snapshot;
        em.reg_alloc.spill_slots = spill_snapshot;
    }
}

// ---------------------------------------------------------------------------
// Syscall helper emitters for collection/string dispatch
// ---------------------------------------------------------------------------

/// Emit parallel register moves, sequentializing to avoid cycles.
/// Uses R24 as a scratch register to break any cycles found.
/// `moves`: list of (target_reg, source_reg) pairs.
fn emit_parallel_moves(em: &mut AsmEmitter, moves: Vec<(usize, usize)>, note: &str) {
    const SCRATCH: usize = 24;
    let mut pending: Vec<(usize, usize)> = moves.into_iter()
        .filter(|&(tgt, src)| tgt != src)
        .collect();

    while !pending.is_empty() {
        let sources: std::collections::HashSet<usize> =
            pending.iter().map(|&(_, s)| s).collect();
        if let Some(idx) = pending.iter().position(|&(tgt, _)| !sources.contains(&tgt)) {
            let (tgt, src) = pending.remove(idx);
            em.emit(format!("    MOV   R{}, {}  ; {}", tgt, AsmEmitter::rn(src), note));
        } else {
            // Cycle: save tgt0 to scratch, emit tgt0←src0, replace tgt0 as source with SCRATCH.
            let (tgt0, src0) = pending.remove(0);
            em.emit(format!("    MOV   R{}, {}  ; cycle: save R{} ({})", SCRATCH, AsmEmitter::rn(tgt0), tgt0, note));
            em.emit(format!("    MOV   R{}, {}  ; {}", tgt0, AsmEmitter::rn(src0), note));
            for m in pending.iter_mut() {
                if m.1 == tgt0 { m.1 = SCRATCH; }
            }
        }
    }
}

/// Emit SYSCALL with 1 arg (R1=arg[0]).  No return value.
fn emit_syscall_1arg(em: &mut AsmEmitter, args: &[IRValue], dst: &Option<IRTemp>, sc: i64, name: &str) {
    em.rescue_reg(1);
    if let Some(a) = args.first() {
        let ra = em.val_reg(a);
        if ra != 1 { em.emit(format!("    MOV   R1, {}  ; {}", AsmEmitter::rn(ra), name)); }
    }
    em.emit(format!("    SYSCALL #{}  ; {}", sc, name));
    if let Some(d) = dst { let rd = em.dst_reg(d); let _ = rd; }
}

/// Emit SYSCALL with 1 arg (R1=arg[0]), returns R1.
fn emit_syscall_1arg_ret(em: &mut AsmEmitter, args: &[IRValue], dst: &Option<IRTemp>, sc: i64, name: &str) {
    em.rescue_reg(1);
    if let Some(a) = args.first() {
        let ra = em.val_reg(a);
        if ra != 1 { em.emit(format!("    MOV   R1, {}  ; {}", AsmEmitter::rn(ra), name)); }
    }
    em.emit(format!("    SYSCALL #{}  ; {}", sc, name));
    if let Some(d) = dst {
        let rd = em.dst_reg(d);
        if rd != 1 { em.emit(format!("    MOV   {}, R1  ; {} result", AsmEmitter::rn(rd), name)); }
    }
}

/// Emit SYSCALL with 2 args (R1=arg[0], R2=arg[1]).  No return value.
fn emit_syscall_2arg(em: &mut AsmEmitter, args: &[IRValue], dst: &Option<IRTemp>, sc: i64, name: &str) {
    em.rescue_reg(1);
    em.rescue_reg(2);
    let moves: Vec<(usize, usize)> = args.iter().enumerate()
        .filter_map(|(i, a)| { let ra = em.val_reg(a); Some((i + 1, ra)) })
        .collect();
    emit_parallel_moves(em, moves, name);
    em.emit(format!("    SYSCALL #{}  ; {}", sc, name));
    if let Some(d) = dst { let rd = em.dst_reg(d); let _ = rd; }
}

/// Emit SYSCALL with 2 args, returns R1.
fn emit_syscall_2arg_ret(em: &mut AsmEmitter, args: &[IRValue], dst: &Option<IRTemp>, sc: i64, name: &str) {
    em.rescue_reg(1);
    em.rescue_reg(2);
    let moves: Vec<(usize, usize)> = args.iter().enumerate()
        .filter_map(|(i, a)| { let ra = em.val_reg(a); Some((i + 1, ra)) })
        .collect();
    emit_parallel_moves(em, moves, name);
    em.emit(format!("    SYSCALL #{}  ; {}", sc, name));
    if let Some(d) = dst {
        let rd = em.dst_reg(d);
        if rd != 1 { em.emit(format!("    MOV   {}, R1  ; {} result", AsmEmitter::rn(rd), name)); }
    }
}

/// Emit SYSCALL with 3 args (R1, R2, R3).  No return value.
fn emit_syscall_3arg(em: &mut AsmEmitter, args: &[IRValue], dst: &Option<IRTemp>, sc: i64, name: &str) {
    em.rescue_reg(1);
    em.rescue_reg(2);
    em.rescue_reg(3);
    let moves: Vec<(usize, usize)> = args.iter().enumerate()
        .filter_map(|(i, a)| { let ra = em.val_reg(a); Some((i + 1, ra)) })
        .collect();
    emit_parallel_moves(em, moves, name);
    em.emit(format!("    SYSCALL #{}  ; {}", sc, name));
    if let Some(d) = dst { let rd = em.dst_reg(d); let _ = rd; }
}

/// Emit SYSCALL with 3 args, returns R1.
fn emit_syscall_3arg_ret(em: &mut AsmEmitter, args: &[IRValue], dst: &Option<IRTemp>, sc: i64, name: &str) {
    em.rescue_reg(1);
    em.rescue_reg(2);
    em.rescue_reg(3);
    let moves: Vec<(usize, usize)> = args.iter().enumerate()
        .filter_map(|(i, a)| { let ra = em.val_reg(a); Some((i + 1, ra)) })
        .collect();
    emit_parallel_moves(em, moves, name);
    em.emit(format!("    SYSCALL #{}  ; {}", sc, name));
    if let Some(d) = dst {
        let rd = em.dst_reg(d);
        if rd != 1 { em.emit(format!("    MOV   {}, R1  ; {} result", AsmEmitter::rn(rd), name)); }
    }
}

