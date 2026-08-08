// emulator/profiler.rs — Task (cooperative scheduler) and ExecProfile.
// Included as pub mod profiler in emulator/mod.rs.
use super::super::isa::Opcode;

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

#[derive(Debug, Clone)]
pub struct ExecProfile {
    /// Total instructions executed.
    pub total_instructions: usize,
    /// Per-opcode instruction counts (indexed by Opcode discriminant).
    pub opcode_counts: [usize; 34],
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
            opcode_counts: [0; 34],
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
        if idx < 34 {
            self.opcode_counts[idx] += 1;
        }
        match op {
            Opcode::Tand | Opcode::Tor | Opcode::Tnot |
            Opcode::Tbranch | Opcode::Tmin | Opcode::Tmax |
            Opcode::Tshi | Opcode::Tshr => {
                self.ternary_native_ops += 1;
            }
            Opcode::Tadd | Opcode::Tsub | Opcode::Tmul |
            Opcode::Tdiv | Opcode::Tmod | Opcode::Tneg => {
                self.arithmetic_ops += 1;
            }
            Opcode::Jump | Opcode::Call | Opcode::Ret | Opcode::Callr |
            Opcode::TbrPos | Opcode::TbrZero | Opcode::TbrNeg => {
                self.control_flow_ops += 1;
            }
            Opcode::Load | Opcode::Store => {
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
            self.max_heap_ptr.saturating_sub(64_000)));

        // Top opcodes
        let mut sorted: Vec<(usize, &str)> = Vec::new();
        let names = [
            "NOP","TADD","TSUB","TMUL","TDIV","TMOD","TNEG",
            "TAND","TOR","TNOT","TSHI","TSHR","TMIN","TMAX",
            "TCMP","LOAD","STORE","TLIT","MOV","TBRANCH","JUMP",
            "CALL","RET","HALT","SYSCALL","TBRPOS","TBRZERO","TBRNEG",
            "CALLR","BAND","BOR","BXOR","BSHL","BSHR",
        ];
        for (i, &count) in self.opcode_counts.iter().enumerate() {
            if count > 0 && i < names.len() {
                sorted.push((count, names[i]));
            }
        }
        sorted.sort_by(|a, b| b.0.cmp(&a.0));
        s.push_str("  Top opcodes:\n");
        for (count, name) in sorted.iter().take(10) {
            s.push_str(&format!("    {:>8}  {}\n", count, name));
        }
        s
    }
}
