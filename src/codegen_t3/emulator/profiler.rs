// emulator/profiler.rs — Task (cooperative scheduler) and ExecProfile.
// Included as pub mod profiler in emulator/mod.rs.
use super::super::isa::{Opcode, T3_OPCODE_COUNT};
use super::HEAP_BASE;

// ---------------------------------------------------------------------------
// Cooperative scheduler task
// ---------------------------------------------------------------------------

#[allow(dead_code)]
pub(crate) struct Task {
    pub(crate) pc: usize,
    pub(crate) regs: [i64; 27],
    pub(crate) stack: Vec<i64>,
}

// ---------------------------------------------------------------------------
// Debug action — interactive debugger commands
// ---------------------------------------------------------------------------

pub(crate) enum DebugAction {
    Step,
    Continue,
    Quit,
}

// ---------------------------------------------------------------------------
// Execution profile — collected during emulation
// ---------------------------------------------------------------------------

/// Opcode mnemonics, indexed by discriminant. Shared by `summary` (which shows
/// the top ten) and `report` (which shows all of them), so the two cannot drift
/// apart; the `debug_assert` below is what keeps the table in step with the ISA.
pub const OPCODE_NAMES: [&str; T3_OPCODE_COUNT] = [
    "NOP", "TADD", "TSUB", "TMUL", "TDIV", "TMOD", "TNEG",
    "TAND", "TOR", "TNOT", "TSHI", "TSHR", "TMIN", "TMAX",
    "TCMP", "LOAD", "STORE", "TLIT", "MOV", "TBRANCH", "JUMP",
    "CALL", "RET", "HALT", "SYSCALL", "TBRPOS", "TBRZERO", "TBRNEG",
    "CALLR", "BAND", "BOR", "BXOR", "BSHL", "BSHR", "LOADT", "STORET",
    // v1.5 lane-wise group, opcodes 36-42.
    "TANDW", "TORW", "TXORW", "TIMPW", "TCMPW", "TPOPC", "TSELW",
    // v1.6 rounding pair, opcodes 43-44.
    "TDIVN", "TMODN",
];

#[derive(Debug, Clone)]
pub struct ExecProfile {
    /// Total instructions executed.
    pub total_instructions: usize,
    /// Per-opcode instruction counts (indexed by Opcode discriminant, 0..=35).
    pub opcode_counts: [usize; T3_OPCODE_COUNT],
    /// Maximum call depth reached.
    pub max_call_depth: usize,
    /// Maximum heap pointer (bytes allocated).
    pub max_heap_ptr: usize,
    /// Program size in words.
    pub program_words: usize,
    /// Number of ternary-native operations (Tand, Tor, Tnot, Tbranch, Tmin, Tmax, Tshi, Tshr).
    pub ternary_native_ops: usize,
    /// Number of arithmetic operations (Tadd, Tsub, Tmul, Tdiv, Tmod).
    pub arithmetic_ops: usize,
    /// Number of control flow operations (Jump, Call, Ret, TbrPos, TbrZero, TbrNeg, Tbranch).
    pub control_flow_ops: usize,
    /// Number of memory operations (Load, Store).
    pub memory_ops: usize,
}

impl ExecProfile {
    pub fn new() -> Self {
        ExecProfile {
            total_instructions: 0,
            opcode_counts: [0; T3_OPCODE_COUNT],
            max_call_depth: 0,
            max_heap_ptr: 0,
            program_words: 0,
            ternary_native_ops: 0,
            arithmetic_ops: 0,
            control_flow_ops: 0,
            memory_ops: 0,
        }
    }

    /// Classify an opcode and increment the appropriate category counter.
    pub(crate) fn record(&mut self, op: Opcode) {
        self.total_instructions += 1;
        let idx = op as usize;
        if idx < self.opcode_counts.len() {
            self.opcode_counts[idx] += 1;
        }
        match op {
            Opcode::Tand | Opcode::Tor | Opcode::Tnot |
            Opcode::Tbranch | Opcode::Tmin | Opcode::Tmax |
            Opcode::Tshi | Opcode::Tshr |
            // v1.5 lane-wise ops are the MOST ternary-native instructions in
            // the set — each replaces 27 extract-operate-insert cycles — so
            // they belong in this count rather than outside it.
            Opcode::Tandw | Opcode::Torw | Opcode::Txorw | Opcode::Timpw |
            Opcode::Tcmpw | Opcode::Tpopc | Opcode::Tselw => {
                self.ternary_native_ops += 1;
            }
            Opcode::Tadd | Opcode::Tsub | Opcode::Tmul |
            Opcode::Tdiv | Opcode::Tmod | Opcode::Tneg |
            // v1.6 (C4). Arithmetic, not ternary-native: rounding to nearest
            // is what the representation makes CHEAP, but the instruction
            // computes a quotient, and counting it as ternary-native would
            // inflate that share for every program that divides.
            Opcode::Tdivn | Opcode::Tmodn => {
                self.arithmetic_ops += 1;
            }
            Opcode::Jump | Opcode::Call | Opcode::Ret | Opcode::Callr |
            Opcode::TbrPos | Opcode::TbrZero | Opcode::TbrNeg => {
                self.control_flow_ops += 1;
            }
            Opcode::Load | Opcode::Store | Opcode::Loadt | Opcode::Storet => {
                self.memory_ops += 1;
            }
            _ => {}
        }
    }

    /// Pretty-print the profile.
    pub fn summary(&self) -> String {
        let mut s = String::new();
        s.push_str(&format!("  Program size:       {} words ({} trits)\n",
            self.program_words, self.program_words * 27));
        s.push_str(&format!("  Instructions exec:  {}\n", self.total_instructions));
        s.push_str(&format!("  Arithmetic ops:     {} ({:.1}%)\n",
            self.arithmetic_ops,
            if self.total_instructions > 0 { self.arithmetic_ops as f64 / self.total_instructions as f64 * 100.0 } else { 0.0 }));
        s.push_str(&format!("  Ternary-native ops: {} ({:.1}%)\n",
            self.ternary_native_ops,
            if self.total_instructions > 0 { self.ternary_native_ops as f64 / self.total_instructions as f64 * 100.0 } else { 0.0 }));
        s.push_str(&format!("  Control flow ops:   {} ({:.1}%)\n",
            self.control_flow_ops,
            if self.total_instructions > 0 { self.control_flow_ops as f64 / self.total_instructions as f64 * 100.0 } else { 0.0 }));
        s.push_str(&format!("  Memory ops:         {} ({:.1}%)\n",
            self.memory_ops,
            if self.total_instructions > 0 { self.memory_ops as f64 / self.total_instructions as f64 * 100.0 } else { 0.0 }));
        s.push_str(&format!("  Max call depth:     {}\n", self.max_call_depth));
        s.push_str(&format!("  Max heap used:      {} words\n",
            self.max_heap_ptr.saturating_sub(HEAP_BASE)));

        // Top opcodes
        debug_assert_eq!(OPCODE_NAMES.len(), T3_OPCODE_COUNT,
            "opcode name table out of step with the opcode set");
        let mut sorted: Vec<(usize, &str)> = self.executed_opcodes();
        sorted.sort_by(|a, b| b.0.cmp(&a.0));
        s.push_str("  Top opcodes:\n");
        for (count, name) in sorted.iter().take(10) {
            s.push_str(&format!("    {:>8}  {}\n", count, name));
        }
        s
    }

    /// Every opcode that executed at least once, as `(count, mnemonic)`.
    pub fn executed_opcodes(&self) -> Vec<(usize, &'static str)> {
        self.opcode_counts
            .iter()
            .enumerate()
            .filter(|(i, &c)| c > 0 && *i < OPCODE_NAMES.len())
            .map(|(i, &c)| (c, OPCODE_NAMES[i]))
            .collect()
    }

    /// The profile as one prefixed line per fact, for `run-t3 --profile`.
    ///
    /// **This is the machine-readable form, and that is the point of it.**
    /// `summary()` is a human report and truncates the histogram to the top
    /// ten; comparing two compilations of one program needs the WHOLE
    /// histogram, because the question a pass raises is usually "what did it
    /// trade" — CSE removes arithmetic and adds spill traffic (report.txt P22),
    /// and a top-ten view can hide either side of that.
    ///
    /// Every line carries the `[T3ISA]` prefix that the corpus sweep scripts
    /// already filter with `grep -v '^\[T3ISA\]'`, so a profiled run stays
    /// byte-comparable with an unprofiled one even if a caller merges the two
    /// streams. Fixed-width columns, opcodes sorted descending, and one fact
    /// per line, so `diff` on two profiles reads as a list of what moved.
    pub fn report(&self) -> String {
        let mut s = String::new();
        let mut line = |k: &str, v: usize| {
            s.push_str(&format!("[T3ISA] profile  {:<20} {:>12}\n", k, v));
        };
        line("total-instructions", self.total_instructions);
        line("program-words", self.program_words);
        line("arithmetic-ops", self.arithmetic_ops);
        line("ternary-native-ops", self.ternary_native_ops);
        line("control-flow-ops", self.control_flow_ops);
        line("memory-ops", self.memory_ops);
        line("max-call-depth", self.max_call_depth);
        line("max-heap-words", self.max_heap_ptr.saturating_sub(HEAP_BASE));

        let mut ops = self.executed_opcodes();
        // Descending by count, then by name, so the order is total and two
        // runs that executed the same opcodes the same number of times produce
        // byte-identical reports.
        ops.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(b.1)));
        for (count, name) in ops {
            s.push_str(&format!("[T3ISA] profile  opcode {:<13} {:>12}\n", name, count));
        }
        s
    }
}
