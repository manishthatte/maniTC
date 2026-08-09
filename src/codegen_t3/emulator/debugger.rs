// emulator/debugger.rs — debugger, run methods, and public run_emulator_* free functions.
// These are continuations of `impl Emulator { }`. Included in mod.rs as `mod debugger;`.

use super::*;

impl Emulator {
    fn disasm_at(&self, addr: usize) -> String {
        if addr >= self.memory.len() {
            return format!("[PC={:>5}] <out of bounds>", addr);
        }
        let word = self.memory[addr];
        let (raw_op, r1, r2, r3, imm) = decode(word);
        let op_name = match Opcode::from_i64(raw_op) {
            Some(op) => format!("{:?}", op),
            None => format!("???({raw_op})"),
        };
        if imm != 0 {
            format!("[PC={:>5}] {:8} R{} R{} R{} #{}", addr, op_name, r1, r2, r3, imm)
        } else {
            format!("[PC={:>5}] {:8} R{} R{} R{}", addr, op_name, r1, r2, r3)
        }
    }

    /// Print all 27 register values.
    fn print_regs(&self) {
        eprintln!("  Registers:");
        for i in 0..27 {
            let name = if i == 0 { "R0(zero)".to_string() }
                       else if i == 26 { "R26(SP)".to_string() }
                       else { format!("R{}", i) };
            eprint!("  {:>9} = {:>14}", name, self.regs[i]);
            if (i + 1) % 4 == 0 { eprintln!(); } else { eprint!("  "); }
        }
        eprintln!("  FLAGS = {}", self.flags);
    }

    /// Print memory at a given address (8 words).
    fn print_mem(&self, addr: usize) {
        eprintln!("  Memory at {}:", addr);
        for i in 0..8 {
            let a = addr + i;
            let val = if a < self.memory.len() { self.memory[a] } else { 0 };
            eprintln!("    [{}] = {}", a, val);
        }
    }

    /// Interactive debug REPL. Returns the action: true = step one, false = quit.
    fn debug_prompt(&mut self) -> DebugAction {
        loop {
            eprint!("(manitc-dbg) ");
            let _ = std::io::stderr().flush();
            let mut line = String::new();
            if std::io::stdin().lock().read_line(&mut line).unwrap_or(0) == 0 {
                return DebugAction::Quit;
            }
            let parts: Vec<&str> = line.trim().split_whitespace().collect();
            if parts.is_empty() {
                // Default: step
                return DebugAction::Step;
            }
            match parts[0] {
                "s" | "step" => return DebugAction::Step,
                "c" | "continue" => return DebugAction::Continue,
                "r" | "regs" => self.print_regs(),
                "m" | "mem" => {
                    if let Some(addr_s) = parts.get(1) {
                        if let Ok(addr) = addr_s.parse::<usize>() {
                            self.print_mem(addr);
                        } else {
                            eprintln!("  invalid address: {}", addr_s);
                        }
                    } else {
                        eprintln!("  usage: m <address>");
                    }
                }
                "b" | "break" => {
                    if let Some(addr_s) = parts.get(1) {
                        if let Ok(addr) = addr_s.parse::<usize>() {
                            self.breakpoints.insert(addr);
                            eprintln!("  breakpoint set at PC={}", addr);
                        } else {
                            eprintln!("  invalid address: {}", addr_s);
                        }
                    } else {
                        // List breakpoints
                        if self.breakpoints.is_empty() {
                            eprintln!("  no breakpoints set");
                        } else {
                            let mut bps: Vec<usize> = self.breakpoints.iter().copied().collect();
                            bps.sort();
                            for bp in bps {
                                eprintln!("  breakpoint at PC={}", bp);
                            }
                        }
                    }
                }
                "d" | "delete" => {
                    if let Some(addr_s) = parts.get(1) {
                        if let Ok(addr) = addr_s.parse::<usize>() {
                            if self.breakpoints.remove(&addr) {
                                eprintln!("  breakpoint removed at PC={}", addr);
                            } else {
                                eprintln!("  no breakpoint at PC={}", addr);
                            }
                        } else {
                            eprintln!("  invalid address: {}", addr_s);
                        }
                    } else {
                        eprintln!("  usage: d <address>");
                    }
                }
                "p" | "print" => {
                    // Print disassembly around current PC
                    let start = self.pc.saturating_sub(2);
                    let end = (self.pc + 8).min(self.memory.len());
                    for addr in start..end {
                        let marker = if addr == self.pc { ">>>" } else { "   " };
                        eprintln!("{} {}", marker, self.disasm_at(addr));
                    }
                }
                "q" | "quit" => return DebugAction::Quit,
                "h" | "help" => {
                    eprintln!("  s/step      - execute one instruction");
                    eprintln!("  c/continue  - run until halt or breakpoint");
                    eprintln!("  r/regs      - print all registers");
                    eprintln!("  m/mem ADDR  - print 8 words at address");
                    eprintln!("  b/break [ADDR] - set breakpoint (or list all)");
                    eprintln!("  d/delete ADDR  - remove breakpoint");
                    eprintln!("  p/print     - disassemble around PC");
                    eprintln!("  q/quit      - stop execution");
                }
                other => {
                    eprintln!("  unknown command '{}' — type 'h' for help", other);
                }
            }
        }
    }

    pub fn run(&mut self) {
        if self.debug {
            self.run_debug();
            return;
        }
        const MAX_STEPS: usize = 10_000_000;
        let mut steps = 0;
        while !self.halted && steps < MAX_STEPS {
            self.step();
            steps += 1;
            // Track high-water marks
            if self.call_depth > self.profile.max_call_depth {
                self.profile.max_call_depth = self.call_depth;
            }
            if self.heap_ptr > self.profile.max_heap_ptr {
                self.profile.max_heap_ptr = self.heap_ptr;
            }
        }
        if steps >= MAX_STEPS {
            self.output.push("TRAP: step limit exceeded".to_string());
        }
    }

    /// Interactive debug mode: step through instructions with breakpoints.
    fn run_debug(&mut self) {
        const MAX_STEPS: usize = 10_000_000;
        let mut steps = 0;
        let mut continuing = false;

        eprintln!("=== maniT T3ISA Debugger ===");
        eprintln!("Type 'h' for help. Program loaded ({} words).", self.profile.program_words);
        eprintln!();

        while !self.halted && steps < MAX_STEPS {
            // Print current instruction
            eprintln!("{}", self.disasm_at(self.pc));

            // Flush any output produced so far
            for piece in &self.output {
                print!("{}", piece);
            }
            self.output.clear();
            let _ = std::io::stdout().flush();

            // Check breakpoints when continuing
            if continuing && self.breakpoints.contains(&self.pc) {
                eprintln!("  *** breakpoint hit at PC={} ***", self.pc);
                continuing = false;
            }

            // If not continuing, prompt for command
            if !continuing {
                match self.debug_prompt() {
                    DebugAction::Step => {}
                    DebugAction::Continue => { continuing = true; }
                    DebugAction::Quit => {
                        eprintln!("  debugger: quit.");
                        return;
                    }
                }
            }

            self.step();
            steps += 1;

            if self.call_depth > self.profile.max_call_depth {
                self.profile.max_call_depth = self.call_depth;
            }
            if self.heap_ptr > self.profile.max_heap_ptr {
                self.profile.max_heap_ptr = self.heap_ptr;
            }
        }
        if self.halted {
            eprintln!("  program halted normally.");
        } else if steps >= MAX_STEPS {
            self.output.push("TRAP: step limit exceeded".to_string());
        }
    }

    pub fn run_with_output(&mut self) -> Vec<String> {
        self.run();
        self.output.clone()
    }
}

impl Default for Emulator {
    fn default() -> Self { Self::new() }
}

// ---------------------------------------------------------------------------
// Public API helper
// ---------------------------------------------------------------------------

pub fn run_emulator(words: Vec<i64>, string_data: HashMap<usize, String>, float_data: HashMap<usize, i64>) -> Vec<String> {
    run_emulator_with_exit(words, string_data, float_data).0
}

/// Run the emulator and also return the program's exit code: R1 at halt,
/// which holds main's return value (void mains clear R1 in their epilogue).
pub fn run_emulator_with_exit(
    words: Vec<i64>,
    string_data: HashMap<usize, String>,
    float_data: HashMap<usize, i64>,
) -> (Vec<String>, i64) {
    let mut emu = Emulator::new();
    emu.load_program(words);
    emu.string_data = string_data;
    emu.float_data = float_data;
    let out = emu.run_with_output();
    (out, emu.regs[1])
}

/// Run emulator in interactive debug mode.
pub fn run_emulator_debug(
    words: Vec<i64>,
    string_data: HashMap<usize, String>,
    float_data: HashMap<usize, i64>,
) -> Vec<String> {
    let mut emu = Emulator::new();
    emu.load_program(words);
    emu.string_data = string_data;
    emu.float_data = float_data;
    emu.debug = true;
    emu.run_with_output()
}

/// Run emulator and return both output and execution profile.
pub fn run_emulator_profiled(
    words: Vec<i64>,
    string_data: HashMap<usize, String>,
    float_data: HashMap<usize, i64>,
) -> (Vec<String>, ExecProfile) {
    let mut emu = Emulator::new();
    emu.load_program(words);
    emu.string_data = string_data;
    emu.float_data = float_data;
    emu.run();
    let profile = emu.profile.clone();
    (emu.output.clone(), profile)
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

