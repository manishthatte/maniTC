// emitter/mod.rs — AsmEmitter, and assembly emission over a FIXED allocation.
//
// F-3. Register assignment happens in `codegen_t3::regalloc` before a single
// line is emitted, and every temp has ONE location for the whole function.
// This file's job is to look locations up, never to choose them.
//
// What that deletes, and why each was there:
//
//   * `rescue_reg` — moved a live value out of a register a syscall was about
//     to clobber. Unnecessary: R1–R3 are never allocated.
//   * the jump reconciliation — moved values back into the registers the
//     target block was emitted against. Unnecessary: a value does not move.
//   * `BlockEntryState` / `block_canonical_regs` — a block's entry state, taken
//     from whichever predecessor was emitted first. Unnecessary: there is one
//     assignment for the whole function, so every path agrees by construction.
//   * `PhiHome` — the location every predecessor of a merge had to agree to
//     write. Unnecessary: the phi's destination has a location like anything
//     else.
//   * `sp_depth` and its per-instruction bookkeeping — the stack pointer moved
//     during a function, so a spill's offset depended on where you were. The
//     frame is now fixed: one `TSUB` in the prologue, one `TADD` before each
//     `RET`, and every slot at a constant offset.
//
// Between them those five were the whole of `KNOWN_ISSUES` issue 2 and
// report.txt P11, P12 and P14: five defects, all of the same shape — a local
// decision about a global question.
//
// © Manish Jagdish Thatte

mod emit_instr;
use emit_instr::{emit_instr, emit_term};

use crate::codegen_t3::regalloc::{self, Allocation, Loc};
use crate::ir::*;
use crate::error::{CompileError, CompileResult, Diagnostic};
use std::collections::HashMap;

/// Registers emission may use as scratch, in the order they are handed out.
///
/// Never allocated to a temp, so taking one cannot destroy a live value. R23 is
/// absent deliberately: it is reserved for a spilled DESTINATION, and an
/// instruction that both reads three spilled operands and writes a spilled
/// result needs the fourth register to still be there.
const SCRATCH_REGS: [usize; 4] = [21, 22, 24, 25];

/// The register a spilled destination is computed into before being stored.
const DST_SCRATCH: usize = 23;

/// Where a call operand lives, resolved before the operands are materialised.
#[derive(Clone)]
enum CallSrc {
    Reg(usize),      // live in a register
    Slot(usize),     // in frame slot n, at [R26 + n]
    Lit(i64),        // plain immediate
    Label(String),   // string literal or global address, resolved by the assembler
    Float(f64),      // needs the float-load syscall
    Missing(String), // temp with no known location — an SSA hole
}

struct AsmEmitter {
    lines:        Vec<String>, // final committed output
    cur_instr:    Vec<String>, // staged body of the current IR instruction
    pending_post: Vec<String>, // post-instruction spill stores
    /// The fixed assignment. Read-only during emission.
    alloc:        Allocation,
    /// Linear index of the instruction being emitted, in the same numbering
    /// `regalloc` used. Only `scratch()` needs it, to ask which registers hold
    /// nothing here.
    cur_idx:      usize,
    /// Scratch registers already handed out for this instruction.
    scratch_taken: Vec<usize>,
    /// Total words the frame reserves: spill slots first, then alloca storage.
    frame_size:   usize,
    /// Current function name, used to qualify block labels.
    fn_name:      String,
    /// Struct name → number of fields (word size on stack).
    struct_sizes: HashMap<String, usize>,
    /// Alloca temp → the frame offset of its storage, in words from R26.
    ///
    /// Constant for the whole function, unlike the old sp-relative depth. An
    /// alloca inside a loop now REUSES its storage every iteration instead of
    /// pushing a fresh copy and never popping it.
    alloca_off:   HashMap<String, usize>,
    /// Phi copies: (pred_label, succ_label) → [(phi_dst, incoming_val)].
    phi_copies:   HashMap<(String, String), Vec<(IRTemp, IRValue)>>,
    /// Current block label, used for phi copy lookup.
    current_block_label: String,
    /// Float literals collected during this function's emit pass.
    float_literals: Vec<(String, i64)>,
    /// Module global variables: name → absolute memory address.
    global_addrs: HashMap<String, i64>,
}

impl AsmEmitter {
    fn new(alloc: Allocation, struct_sizes: HashMap<String, usize>, frame_size: usize) -> Self {
        AsmEmitter {
            lines: Vec::new(),
            cur_instr: Vec::new(),
            pending_post: Vec::new(),
            alloc,
            cur_idx: 0,
            scratch_taken: Vec::new(),
            frame_size,
            fn_name: String::new(),
            struct_sizes,
            alloca_off: HashMap::new(),
            phi_copies: HashMap::new(),
            current_block_label: String::new(),
            float_literals: Vec::new(),
            global_addrs: HashMap::new(),
        }
    }

    /// Returns a globally-unique label by prefixing with the current function name.
    fn qlabel(&self, label: &str) -> String {
        format!("{}_{}", self.fn_name, label)
    }

    fn emit(&mut self, line: impl Into<String>) {
        self.cur_instr.push(line.into());
    }

    /// Commit the current instruction: body first, then any spill stores it
    /// scheduled, then reset the per-instruction scratch pool.
    fn flush_instr(&mut self) {
        self.lines.extend(self.cur_instr.drain(..));
        self.lines.extend(self.pending_post.drain(..));
        self.scratch_taken.clear();
    }

    fn rn(r: usize) -> String { format!("R{}", r) }

    /// A register free to use for the rest of this instruction.
    ///
    /// Asks the allocation which registers hold nothing live at this index and
    /// takes one; falls back to the dedicated scratch registers when the pool
    /// is fully committed. Both sources are exact rather than hopeful — a pool
    /// register is offered only when no interval covers this index, and a
    /// scratch register is never allocated to anything.
    ///
    /// It cannot run out in practice: `SCRATCH_REGS` alone covers four
    /// simultaneous needs and no IR instruction has more than three operands
    /// plus a materialised constant. If it ever does, it says so in the output
    /// rather than silently reusing a register.
    fn scratch(&mut self) -> usize {
        for r in self.alloc.free_regs_at(self.cur_idx) {
            if !self.scratch_taken.contains(&r) {
                self.scratch_taken.push(r);
                return r;
            }
        }
        for &r in SCRATCH_REGS.iter() {
            if !self.scratch_taken.contains(&r) {
                self.scratch_taken.push(r);
                return r;
            }
        }
        self.lines.push(
            "    ; BUG: scratch registers exhausted — reusing R25".to_string(),
        );
        25
    }

    /// Emit instructions to `self.lines` that load `imm` into register `r`.
    /// Single TLIT when |imm| ≤ 797161 (the balanced 13-trit wide-imm bound).
    /// Larger values are decomposed recursively as hi*1000 + lo.
    fn emit_lit(&mut self, r: usize, imm: i64) {
        const MAX_LIT: i64 = crate::codegen_t3::isa::WIDE_IMM_MAX; // 797_161
        if imm.abs() <= MAX_LIT {
            self.lines.push(format!("    TLIT  {}, #{}", Self::rn(r), imm));
        } else {
            let hi = imm / 1000;
            let lo = imm - hi * 1000;
            let tmp = self.scratch();
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

    /// `emit_lit`, but into the current instruction body rather than `lines`.
    fn emit_lit_cur(&mut self, r: usize, imm: i64) {
        self.emit_lit_cur_at(r, imm, 0);
    }

    fn emit_lit_cur_at(&mut self, r: usize, imm: i64, depth: usize) {
        const MAX_LIT: i64 = crate::codegen_t3::isa::WIDE_IMM_MAX;
        const SCRATCH: [usize; 3] = [23, 21, 22];
        if imm.abs() <= MAX_LIT {
            self.emit(format!("    TLIT  R{}, #{}", r, imm));
            return;
        }
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

    /// Adjust the stack pointer by `delta` words. Used only for the prologue
    /// and the epilogue; nothing else moves R26 any more.
    fn emit_sp_adj(&mut self, delta: i64) {
        if delta == 0 { return; }
        if delta > 0 {
            if delta <= 13 {
                self.emit(format!("    TADD  R26, R26, #{}  ; frame pop {}", delta, delta));
            } else {
                self.emit(format!("    TLIT  R21, #{}  ; frame size", delta));
                self.emit("    TADD  R26, R26, R21  ; frame pop".to_string());
            }
        } else {
            let n = -delta;
            if n <= 13 {
                self.emit(format!("    TSUB  R26, R26, #{}  ; frame push {}", n, n));
            } else {
                self.emit(format!("    TLIT  R21, #{}  ; frame size", n));
                self.emit("    TSUB  R26, R26, R21  ; frame push".to_string());
            }
        }
    }

    /// Emit `LOAD reg, [R26 + slot]` into the current instruction body.
    ///
    /// The offset is a CONSTANT — the frame does not move — so unlike the
    /// sp-relative version this replaces, it cannot be measured against the
    /// wrong stack depth. That was where several of the old defects lived.
    fn emit_slot_load(&mut self, reg: usize, slot: usize, note: &str) {
        if slot <= 13 {
            self.emit(format!("    LOAD  R{}, [R26+#{}]  ; {}", reg, slot, note));
        } else {
            self.emit(format!("    TLIT  R{}, #{}", reg, slot));
            self.emit(format!("    TADD  R{0}, R26, R{0}  ; slot addr", reg));
            self.emit(format!("    LOAD  R{0}, [R{0}+#0]  ; {1}", reg, note));
        }
    }

    /// The same, but written ahead of the instruction body (spill reads run
    /// before the body that consumes them).
    fn emit_slot_load_pre(&mut self, reg: usize, slot: usize, note: &str) {
        if slot <= 13 {
            self.lines.push(format!("    LOAD  R{}, [R26+#{}]  ; {}", reg, slot, note));
        } else {
            self.lines.push(format!("    TLIT  R{}, #{}", reg, slot));
            self.lines.push(format!("    TADD  R{0}, R26, R{0}  ; slot addr", reg));
            self.lines.push(format!("    LOAD  R{0}, [R{0}+#0]  ; {1}", reg, note));
        }
    }

    /// Schedule `STORE reg, [R26 + slot]` for after the instruction body.
    fn emit_slot_store_post(&mut self, reg: usize, slot: usize, note: &str) {
        if slot <= 13 {
            self.pending_post
                .push(format!("    STORE R{}, [R26+#{}]  ; {}", reg, slot, note));
        } else {
            // R21 is free once the body has run: every spill read is done.
            self.pending_post.push(format!("    TLIT  R21, #{}", slot));
            self.pending_post.push("    TADD  R21, R26, R21  ; slot addr".to_string());
            self.pending_post
                .push(format!("    STORE R{}, [R21+#0]  ; {}", reg, note));
        }
    }

    /// Resolve where a call operand lives, without emitting anything.
    fn resolve_call_src(&self, val: &IRValue) -> CallSrc {
        match val {
            IRValue::Temp(t) => match self.alloc.loc(&t.0) {
                Some(Loc::Reg(r)) => CallSrc::Reg(r),
                Some(Loc::Slot(s)) => CallSrc::Slot(s),
                None => CallSrc::Missing(t.0.clone()),
            },
            IRValue::Const(IRConst::Str(label)) => {
                CallSrc::Label(label.trim_start_matches('@').to_string())
            }
            IRValue::Const(IRConst::Float(f)) => CallSrc::Float(*f),
            IRValue::Const(c) => CallSrc::Lit(irconst_to_i64(c)),
            IRValue::Global(name) => match self.global_addrs.get(name) {
                Some(&addr) => CallSrc::Lit(addr),
                None => CallSrc::Label(name.clone()),
            },
            IRValue::Void => CallSrc::Lit(0),
        }
    }

    /// Materialise call operands into their target registers.
    ///
    /// A CALL may destroy every register (see `regalloc`'s invariant), and
    /// nothing live across it is in one, so this is free to overwrite the whole
    /// pool. The phases exist only to keep the operands from overwriting EACH
    /// OTHER:
    ///
    ///   0. float constants, which need R1 and the float-load syscall, so they
    ///      run before any argument register is written;
    ///   1. register→register moves, sequentialised so no move overwrites a
    ///      register another still needs (cycles broken through R23);
    ///   2. slot reloads and immediates, which read no allocated register.
    ///
    /// `stage_base` is the exclusive upper bound on an argument target and the
    /// first register phase 0 may park a float in. A CALL passes 9 — parameters
    /// are R1–R8 and there is no stack argument area. A SYSCALL passes one past
    /// its own highest target, since the emulator reads R1.. directly.
    fn emit_call_operands(&mut self, targets: &[(usize, CallSrc)], stage_base: usize) {
        // R25 is excluded: it is the indirect-call fn_ptr target, not an
        // argument position.
        assert!(
            targets.iter().all(|&(t, _)| t < stage_base || t == 25),
            "T3ISA internal error: call operand target R{} is at or above the \
             staging base R{}",
            targets.iter().map(|&(t, _)| t).find(|&t| t >= stage_base && t != 25).unwrap_or(0),
            stage_base,
        );
        let n_float = targets.iter().filter(|(_, s)| matches!(s, CallSrc::Float(_))).count();
        assert!(
            stage_base + n_float <= 26,
            "T3ISA internal error: {} call operands plus {} float staging register(s) \
             would reach R26 (SP)",
            targets.len(), n_float,
        );

        let mut resolved: Vec<(usize, CallSrc)> = Vec::with_capacity(targets.len());
        let mut stage = stage_base;
        for (tgt, src) in targets {
            match src {
                CallSrc::Float(f) => {
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
        const CYCLE: usize = 23;
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
            if let Some(i) = pending.iter().position(|&(tgt, _)| !sources.contains(&tgt)) {
                let (tgt, src) = pending.remove(i);
                self.emit(format!("    MOV   R{}, R{}  ; call operand", tgt, src));
            } else {
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

        // Phase 2: everything that reads no allocated register.
        for (tgt, src) in &resolved {
            match src {
                CallSrc::Reg(_) | CallSrc::Float(_) => {}
                CallSrc::Slot(s) => self.emit_slot_load(*tgt, *s, "call operand from frame"),
                CallSrc::Lit(v) => self.emit_lit_cur(*tgt, *v),
                CallSrc::Label(l) => self.emit(format!("    TLIT  R{}, #{}  ; call operand", tgt, l)),
                CallSrc::Missing(name) => {
                    self.emit(format!("    ; BUG: call operand {} has no location", name));
                    self.emit(format!("    TLIT  R{}, #0", tgt));
                }
            }
        }
    }

    /// The register an instruction should write its result to.
    ///
    /// For a temp in a register, that register. For a spilled temp, the
    /// dedicated destination scratch, with the store to its frame slot
    /// scheduled for after the body. There is no third case and no allocation:
    /// the answer was decided before emission began.
    fn dst_reg(&mut self, t: &IRTemp) -> usize {
        match self.alloc.loc(&t.0) {
            Some(Loc::Reg(r)) => r,
            Some(Loc::Slot(s)) => {
                self.emit_slot_store_post(DST_SCRATCH, s, &format!("spill {}", t.0));
                DST_SCRATCH
            }
            None => {
                // A destination the allocator never saw. That is a hole in the
                // IR rather than a shortage of registers, so it is reported
                // instead of papered over with a fresh register.
                self.lines
                    .push(format!("    ; BUG: {} has no allocated location", t.0));
                DST_SCRATCH
            }
        }
    }

    /// Materialise a value into a register and return it.
    ///
    /// Spilled temps are reloaded into scratch AHEAD of the instruction body,
    /// which is why this writes to `lines` rather than `cur_instr`.
    fn val_reg(&mut self, val: &IRValue) -> usize {
        match val {
            IRValue::Temp(t) => match self.alloc.loc(&t.0) {
                Some(Loc::Reg(r)) => r,
                Some(Loc::Slot(s)) => {
                    let scratch = self.scratch();
                    self.emit_slot_load_pre(scratch, s, &format!("reload {}", t.0));
                    scratch
                }
                None => {
                    self.lines
                        .push(format!("    ; BUG: {} has no allocated location", t.0));
                    let scratch = self.scratch();
                    self.lines.push(format!("    TLIT  R{}, #0", scratch));
                    scratch
                }
            },
            IRValue::Const(c) => {
                let r = self.scratch();
                match c {
                    IRConst::Str(label) => {
                        let clean = label.trim_start_matches('@');
                        self.lines.push(format!("    TLIT  {}, #{}", Self::rn(r), clean));
                    }
                    IRConst::Float(f) => {
                        // Float bit patterns exceed TLIT's range: the bits live
                        // in the .float section and come back through a syscall.
                        // R1 is never allocated, so nothing needs saving first.
                        let bits = f.to_bits() as i64;
                        let label = format!("@float_{}_{}", self.fn_name, self.float_literals.len());
                        let clean = label.trim_start_matches('@').to_string();
                        self.float_literals.push((label.clone(), bits));
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
                    self.lines.push(format!("    TLIT  {}, #{}", Self::rn(r), name));
                }
                r
            }
            IRValue::Void => 0,
        }
    }

    /// Emit the function epilogue: give the frame back, then the caller's R26
    /// is exactly what it was.
    fn emit_epilogue(&mut self) {
        self.emit_sp_adj(self.frame_size as i64);
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

pub fn emit_t3_asm(module: &IRModule) -> CompileResult<String> {
    let mut out = String::new();
    out.push_str("; T3ISA assembly — generated by maniT compiler\n");
    out.push_str("; 27-trit balanced ternary word machine\n\n");

    // Globals live in a fixed memory window between the stack top (SP starts
    // at 60_000 and grows down) and the emulator's reserved areas (62_000+):
    // one word per global, initialized by a preamble emitted before main.
    const GLOBALS_BASE: i64 = 61_000;
    // The first address the globals window may NOT use: 62_000 is the
    // emulator's RESULT_AREA / TUPLE_AREA scratch (emulator/mod.rs).
    const GLOBALS_LIMIT: i64 = 62_000;
    let mut global_addrs: HashMap<String, i64> = HashMap::new();
    for (i, g) in module.globals.iter().enumerate() {
        global_addrs.insert(g.name.clone(), GLOBALS_BASE + i as i64);
    }
    // Static struct payloads follow the one-word globals in the same window.
    // A struct-valued global holds the ADDRESS of its payload, so the payload
    // needs storage of its own that outlives no scope and is written before
    // main — which is exactly what this window and its preamble are.
    let mut payload_next = GLOBALS_BASE + module.globals.len() as i64;
    for s in &module.static_structs {
        global_addrs.insert(s.label.clone(), payload_next);
        payload_next += s.fields.len() as i64;
    }
    if payload_next > GLOBALS_LIMIT {
        // Silence here would corrupt the emulator's scratch area at run time,
        // with no sign of where it came from.
        return Err(CompileError::Codegen(Diagnostic::unknown(format!(
            "T3 backend: module globals need {} words but the globals window holds {} \
             (addresses {}..{}, above which the emulator's scratch areas begin). \
             {} globals and {} static struct payloads.",
            payload_next - GLOBALS_BASE,
            GLOBALS_LIMIT - GLOBALS_BASE,
            GLOBALS_BASE,
            GLOBALS_LIMIT - 1,
            module.globals.len(),
            module.static_structs.len(),
        ))));
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

    // Float bit patterns that the globals preamble needs; merged into the
    // module's .float section below, alongside the ones functions emit.
    let mut global_float_literals: Vec<(String, i64)> = Vec::new();

    /// Materialise one compile-time initialiser into R1, clobbering R3.
    ///
    /// Shared by the globals themselves and by the words of a static struct
    /// payload, because a payload field is the same kind of thing as a global
    /// initialiser: a constant that must exist before main runs.
    fn init_into_r1(
        out: &mut String,
        global_float_literals: &mut Vec<(String, i64)>,
        global_addrs: &HashMap<String, i64>,
        init: Option<&IRValue>,
    ) {
        match init {
            // A float's bits are a 64-bit pattern — far outside the 27-trit
            // word, so `lit_into` cannot build it: the TMUL split trapped at
            // run time before the program reached main (`let P: float = 1.5`
            // died on "TMUL overflow: result 4609434218000"). Take the same
            // route function-local float literals already take: park the bits
            // in the .float section and fetch them with the float-load
            // syscall.
            Some(IRValue::Const(IRConst::Float(f))) => {
                let label = format!("@float_global_{}", global_float_literals.len());
                let clean = label.trim_start_matches('@').to_string();
                global_float_literals.push((label, f.to_bits() as i64));
                out.push_str(&format!("    TLIT  R1, #{}  ; float-lit addr\n", clean));
                out.push_str(&format!("    SYSCALL #219  ; float_load bits for {}\n", f));
            }
            // A `str` global holds the ADDRESS of its .data entry. The old
            // code ran the label through `irconst_to_i64`, whose `Str` arm
            // returns 0, so every string global was the null address and
            // printing one dumped emulator memory. LLVM emitted
            // `@S = global ptr @str0` and was right all along, so this one
            // did diverge — unlike the negative-integer case, which was
            // wrong identically on both sides.
            Some(IRValue::Const(IRConst::Str(label))) => {
                let clean = label.trim_start_matches('@');
                out.push_str(&format!("    TLIT  R1, #{}  ; &{}\n", clean, clean));
            }
            // A struct constant — either a struct-valued global or a field
            // that is itself one — holds the ADDRESS of its static payload,
            // for the same reason a `str` holds the address of its `.data`
            // entry: the word cannot hold the aggregate.
            Some(IRValue::Global(label)) => match global_addrs.get(label) {
                Some(&addr) => out.push_str(&format!("    TLIT  R1, #{}  ; &{}\n", addr, label)),
                // Unknown symbol: keep the symbolic form so the assembler
                // fails loudly rather than the program reading address 0.
                None => out.push_str(&format!("    TLIT  R1, #{}\n", label)),
            },
            Some(IRValue::Const(c)) => lit_into(out, "R1", "R3", irconst_to_i64(c)),
            _ => lit_into(out, "R1", "R3", 0),
        }
    }

    if !module.globals.is_empty() {
        // Payloads first: a global's word is the address of one, and writing
        // the address before the thing it points at would still be correct
        // here (nothing reads memory until main), but this order keeps the
        // listing readable.
        if !module.static_structs.is_empty() {
            out.push_str("; static struct payloads\n");
            for s in &module.static_structs {
                let base = global_addrs[&s.label];
                for (i, (_, v)) in s.fields.iter().enumerate() {
                    init_into_r1(&mut out, &mut global_float_literals, &global_addrs, Some(v));
                    out.push_str(&format!(
                        "    TLIT  R2, #{}  ; &{}.{} ({})\n",
                        base + i as i64, s.label, i, s.struct_name,
                    ));
                    out.push_str("    STORE R1, [R2+#0]\n");
                }
            }
        }
        out.push_str("; globals init\n");
        for g in &module.globals {
            let addr = global_addrs[&g.name];
            init_into_r1(&mut out, &mut global_float_literals, &global_addrs, g.init.as_ref());
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
            emit_function(func, module.struct_sizes.clone(), global_addrs.clone())?;
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
        .chain(global_float_literals.into_iter())
        .chain(all_float_literals.into_iter())
        .collect();
    if !combined_floats.is_empty() {
        out.push_str(".float:\n");
        for (label, bits) in &combined_floats {
            let clean = label.trim_start_matches('@');
            out.push_str(&format!("    {}: .float64 {}\n", clean, bits));
        }
    }

    Ok(out)
}

/// Words of frame storage one `Alloca` needs.
fn alloca_words(ty: &IRType, struct_sizes: &HashMap<String, usize>) -> usize {
    match ty {
        IRType::Array(_, n) => (*n).max(1),
        IRType::Struct(name) => {
            if let Some(k) = crate::ir::types::tuple_arity_from_name(name) {
                k.max(1)
            } else {
                struct_sizes.get(name).copied().unwrap_or(1).max(1)
            }
        }
        _ => 1,
    }
}

fn emit_function(
    func: &IRFunction,
    struct_sizes: HashMap<String, usize>,
    global_addrs: HashMap<String, i64>,
) -> CompileResult<(String, Vec<(String, i64)>)> {
    // The convention has no stack argument area, so a ninth parameter has
    // nowhere to arrive. Refused through the error channel rather than an
    // assertion: a panic prints a Rust backtrace where every other refusal in
    // this compiler prints an `error:` line.
    if func.params.len() > regalloc::PARAM_MAX {
        return Err(CompileError::Codegen(Diagnostic::unknown(format!(
            "[T3ISA] `{}` takes {} parameters, but the T3 calling convention \
             passes arguments only in R1-R{} and has no stack argument area. \
             Pass the extra values in a struct, or split the function.",
            func.name,
            func.params.len(),
            regalloc::PARAM_MAX,
        ))));
    }

    // 1. Assign every temp a location, once, before anything is emitted.
    let alloc = regalloc::allocate_with(func, &struct_sizes);

    // 2. Lay out the frame: spill slots first, then one region per alloca.
    //    Both are at CONSTANT offsets from R26 — the frame does not move.
    let mut alloca_off: HashMap<String, usize> = HashMap::new();
    let mut off = alloc.n_slots;
    for block in &func.blocks {
        for instr in &block.instrs {
            if let IRInstr::Alloca { dst, ty } = instr {
                let w = alloca_words(ty, &struct_sizes);
                alloca_off.insert(dst.0.clone(), off);
                off += w;
            }
        }
    }
    let frame_size = off;

    let param_slots = alloc.param_slots.clone();
    let mut em = AsmEmitter::new(alloc, struct_sizes, frame_size);
    em.fn_name = func.name.clone();
    em.global_addrs = global_addrs;
    em.alloca_off = alloca_off;

    // Phi copies, indexed by the edge they travel on.
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

    em.lines.push(format!("{}:", func.name));

    // 3. Prologue. One push for the whole frame, and a store for any parameter
    //    that could not stay in the register it arrived in.
    em.cur_idx = 0;
    em.emit_sp_adj(-(frame_size as i64));
    for (abi, slot) in &param_slots {
        em.emit_slot_store_post(*abi, *slot, "parameter to frame");
    }
    em.flush_instr();

    for (bi, block) in func.blocks.iter().enumerate() {
        emit_block(&mut em, block, bi);
    }

    let float_lits = em.float_literals.clone();
    Ok((em.lines.join("\n") + "\n", float_lits))
}

fn emit_block(em: &mut AsmEmitter, block: &IRBlock, bi: usize) {
    em.current_block_label = block.label.clone();
    em.lines.push(format!("  {}:", em.qlabel(&block.label)));

    // The same numbering `regalloc` used, so `scratch()` asks about the right
    // instruction. Nothing else in emission depends on it.
    let base = em.alloc.block_start.get(bi).copied().unwrap_or(0);
    for (k, instr) in block.instrs.iter().enumerate() {
        em.cur_idx = base + k;
        emit_instr(em, instr);
        em.flush_instr();
    }
    em.cur_idx = em.alloc.block_end.get(bi).copied().unwrap_or(base);
    emit_term(em, &block.term);
    em.flush_instr();
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
    if let Some(a) = args.first() {
        let ra = em.val_reg(a);
        if ra != 1 { em.emit(format!("    MOV   R1, {}  ; {}", AsmEmitter::rn(ra), name)); }
    }
    em.emit(format!("    SYSCALL #{}  ; {}", sc, name));
    if let Some(d) = dst { let rd = em.dst_reg(d); let _ = rd; }
}

/// Emit SYSCALL with 1 arg (R1=arg[0]), returns R1.
fn emit_syscall_1arg_ret(em: &mut AsmEmitter, args: &[IRValue], dst: &Option<IRTemp>, sc: i64, name: &str) {
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

/// Place `args` into R1, R2, R3, ... ready for a SYSCALL.
///
/// The arity in each caller's name is the COMMON case, not a limit: these
/// helpers have always walked the whole `args` slice, and `fmt::format` reaches
/// `emit_syscall_2arg_ret` with a template plus one argument per `{}` after
/// `lower_expr` splats the substitution array. Seven substitutions is eight
/// registers, R1 through R8.
///
/// The old shape was `args.iter().map(|a| em.val_reg(a)).collect()` followed by
/// one `emit_parallel_moves`. That is wrong for any argument that is SPILLED,
/// because `val_reg` does not merely report where a value is — for a spilled
/// temp it EMITS a reload into a scratch register and returns that. There are
/// exactly three such scratch registers (R21, R22, then R25 for every reload
/// after the second — `val_reg`'s `spill_read_idx` match saturates), while the
/// collect above holds all of them live at once. With four spilled arguments
/// the third and fourth both reload into R25 and the second overwrites the
/// first, so two different arguments arrive at the syscall holding one value:
///
/// ```text
/// ; reload spill t88  (offset 21)   -> R25
/// ; reload spill t100 (offset 8)    -> R25   <- t88 is gone
/// MOV R4, R25    ; wanted t88, gets t100
/// MOV R5, R25    ; t100
/// ```
///
/// That is S44: `fmt::format("... tand={} tor={} ...", [a tand b, a tor b, ..])`
/// printed the `tor` value in the `tand` column on T3 while LLVM was right, and
/// the program's own `min=yes` self-check still passed because the VALUE was
/// computed correctly — only the copy reaching the argument list was not.
///
/// **This exact defect was already found and fixed once** — for the general
/// call path, whose `emit_call_operands` says so in as many words: *"From the
/// third spilled operand onward `val_reg` handed out R25, and the fn_ptr move
/// then overwrote it, passing the callee a silently wrong argument."* The
/// syscall emitters were simply never migrated to it, so the bug survived here
/// and came back as S44. They share the machinery now, rather than growing a
/// second copy that can drift out of step with the first:
///
///   1. Rescue EVERY destination, not just R1/R2(/R3). All of R1..Rn are
///      written here, so a live temp sitting in R5 was being destroyed too.
///      (Nothing had noticed, because the callers all claim 1-3 arguments.)
///   2. Resolve where each argument lives, without emitting. This must come
///      AFTER the rescues: a rescue moves a live temp to another register and
///      updates the map, so a location read before it can be stale.
///   3. Hand the resolved locations to `emit_call_operands`, which orders the
///      float staging, the register-to-register moves and the stack reloads so
///      that none can disturb another.
fn emit_syscall_args(em: &mut AsmEmitter, args: &[IRValue], name: &str) {
    // R26 is SP: writing it would destroy the stack, and the emulator would then
    // read SP as an argument (syscall 127 indexes regs[i+2] behind nothing but a
    // bounds check). 25 destinations = a template plus 24 substitutions.
    const MAX_ARG_REGS: usize = 25;
    let n = args.len().min(MAX_ARG_REGS);
    if args.len() > MAX_ARG_REGS {
        em.emit(format!(
            "    ; BUG: {} takes {} arguments but only R1-R{} are available",
            name, args.len(), MAX_ARG_REGS,
        ));
    }

    let targets: Vec<(usize, CallSrc)> = args.iter().take(n).enumerate()
        .map(|(i, a)| (i + 1, em.resolve_call_src(a)))
        .collect();
    // Stage floats one past our own highest target: a syscall's arguments are
    // read straight out of R1.. by the emulator, so they are not bounded by the
    // R1-R8 parameter convention a ManiT callee imposes.
    em.emit_call_operands(&targets, (n + 1).max(9));
}

/// Emit SYSCALL with 2 args (R1=arg[0], R2=arg[1]).  No return value.
fn emit_syscall_2arg(em: &mut AsmEmitter, args: &[IRValue], dst: &Option<IRTemp>, sc: i64, name: &str) {
    emit_syscall_args(em, args, name);
    em.emit(format!("    SYSCALL #{}  ; {}", sc, name));
    if let Some(d) = dst { let rd = em.dst_reg(d); let _ = rd; }
}

/// Emit SYSCALL with 2 args, returns R1.
fn emit_syscall_2arg_ret(em: &mut AsmEmitter, args: &[IRValue], dst: &Option<IRTemp>, sc: i64, name: &str) {
    emit_syscall_args(em, args, name);
    em.emit(format!("    SYSCALL #{}  ; {}", sc, name));
    if let Some(d) = dst {
        let rd = em.dst_reg(d);
        if rd != 1 { em.emit(format!("    MOV   {}, R1  ; {} result", AsmEmitter::rn(rd), name)); }
    }
}

/// Emit SYSCALL with 3 args (R1, R2, R3).  No return value.
fn emit_syscall_3arg(em: &mut AsmEmitter, args: &[IRValue], dst: &Option<IRTemp>, sc: i64, name: &str) {
    emit_syscall_args(em, args, name);
    em.emit(format!("    SYSCALL #{}  ; {}", sc, name));
    if let Some(d) = dst { let rd = em.dst_reg(d); let _ = rd; }
}

/// Emit SYSCALL with 3 args, returns R1.
fn emit_syscall_3arg_ret(em: &mut AsmEmitter, args: &[IRValue], dst: &Option<IRTemp>, sc: i64, name: &str) {
    emit_syscall_args(em, args, name);
    em.emit(format!("    SYSCALL #{}  ; {}", sc, name));
    if let Some(d) = dst {
        let rd = em.dst_reg(d);
        if rd != 1 { em.emit(format!("    MOV   {}, R1  ; {} result", AsmEmitter::rn(rd), name)); }
    }
}

