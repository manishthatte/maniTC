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
        let max_steps = self.max_steps;
        // The budget is counted in INSTRUCTIONS EXECUTED, not in iterations of
        // this loop, and the two are not the same thing (report.txt P33): a
        // syscall handed a maniT callback runs it in a re-entrant loop inside
        // one `step()`, so an iteration here can be worth thousands.
        // `profile.total_instructions` is the counter both loops now charge.
        while !self.halted && self.profile.total_instructions < max_steps {
            self.step();
            // Track high-water marks
            if self.call_depth > self.profile.max_call_depth {
                self.profile.max_call_depth = self.call_depth;
            }
            if self.heap_ptr > self.profile.max_heap_ptr {
                self.profile.max_heap_ptr = self.heap_ptr;
            }
        }
        // `!self.halted` FIRST, and it is not decoration (report.txt P32).
        // The loop also exits when the program halts, and a program that halts
        // on its `max_steps`-th instruction leaves `steps == max_steps` — so
        // testing the budget alone reports a program that RAN TO COMPLETION as
        // cut off, and `run-t3` returns 71 instead of the program's own exit
        // code. `run_debug` below has always had the check in this order.
        //
        // It made `--max-steps` off by exactly one, which is how it was found:
        // the smallest budget `hello` completes under bisected to 719 while the
        // profile the same run collected said it executed 718 instructions.
        if !self.halted && self.profile.total_instructions >= max_steps {
            self.stop_at_step_limit(max_steps);
        }
    }

    /// Stop because the instruction budget ran out.
    ///
    /// Deliberately NOT `trap()`. A trap says the program did something
    /// illegal; this says the program was still running legally when we ran out
    /// of patience. Conflating them cost real accuracy: a truncated run reported
    /// the same exit status as a fault, so the oracle's adjudicator took the
    /// "one backend ran and the other did not" branch and called it DIVERGENT —
    /// which reads as a compiler bug when nothing had gone wrong at all.
    fn stop_at_step_limit(&mut self, max_steps: usize) {
        self.push_out(format!(
            "TRAP: step limit exceeded ({} steps; raise it with \
             `manitc run-t3 --max-steps N`)\n",
            max_steps,
        ));
        self.halted = true;
        self.step_limited = true;
    }

    /// Interactive debug mode: step through instructions with breakpoints.
    fn run_debug(&mut self) {
        let max_steps = self.max_steps;
        let mut steps = 0;
        let mut continuing = false;

        eprintln!("=== maniT T3ISA Debugger ===");
        eprintln!("Type 'h' for help. Program loaded ({} words).", self.profile.program_words);
        eprintln!();

        while !self.halted && steps < max_steps {
            // Print current instruction
            eprintln!("{}", self.disasm_at(self.pc));

            // Flush any output produced so far
            for piece in &self.output {
                // P82: bytes straight out, so the debugger shows what the
                // program actually emitted.
                use std::io::Write as _;
                let _ = std::io::stdout().write_all(piece);
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
        } else if steps >= max_steps {
            self.stop_at_step_limit(max_steps);
        }
    }

    /// P82: BYTES. maniT strings are byte strings and a `String` cannot hold
    /// one that is not valid UTF-8; returning text here is where the emulator
    /// used to lose the difference. Callers that want text decode explicitly.
    pub fn run_with_output(&mut self) -> Vec<Vec<u8>> {
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

pub fn run_emulator(words: Vec<i64>, string_data: HashMap<usize, String>, float_data: HashMap<usize, i64>) -> Vec<Vec<u8>> {
    run_emulator_with_exit(words, string_data, float_data).0
}

/// Run the emulator and also return the program's exit code: R1 at halt,
/// which holds main's return value (void mains clear R1 in their epilogue).
pub fn run_emulator_with_exit(
    words: Vec<i64>,
    string_data: HashMap<usize, String>,
    float_data: HashMap<usize, i64>,
) -> (Vec<Vec<u8>>, i64) {
    run_emulator_with_exit_capped(words, string_data, float_data, DEFAULT_MAX_STEPS)
}

/// As [`run_emulator_with_exit`], with an explicit instruction budget.
pub fn run_emulator_with_exit_capped(
    words: Vec<i64>,
    string_data: HashMap<usize, String>,
    float_data: HashMap<usize, i64>,
    max_steps: usize,
) -> (Vec<Vec<u8>>, i64) {
    run_emulator_with_exit_capped_argv(words, string_data, float_data, max_steps,
                                       Emulator::new().argv)
}

/// As [`run_emulator_with_exit_capped`], with the program's command-line
/// arguments — `argv[0]` first — for `env::argc` and `env::arg` to report.
pub fn run_emulator_with_exit_capped_argv(
    words: Vec<i64>,
    string_data: HashMap<usize, String>,
    float_data: HashMap<usize, i64>,
    max_steps: usize,
    argv: Vec<String>,
) -> (Vec<Vec<u8>>, i64) {
    let (out, code, _) =
        run_emulator_with_exit_capped_argv_profiled(words, string_data, float_data, max_steps, argv);
    (out, code)
}

/// As [`run_emulator_with_exit_capped_argv`], also returning the execution
/// profile the emulator collected on the way (report.txt P31).
///
/// The profile was ALWAYS collected — `Emulator::step` counts every opcode
/// unconditionally and `run` tracks the high-water marks — and until this
/// existed the only way out of the emulator was `manitc bench`, which runs
/// uncapped, ignores argv and reports no exit code. So the dynamic instruction
/// count that P22 established as the right measure of an optimiser pass was
/// being obtained by bisecting `--max-steps`: about forty emulator runs to read
/// out a number the emulator was already holding in a field, and the per-opcode
/// histogram was thrown away with it.
pub fn run_emulator_with_exit_capped_argv_profiled(
    words: Vec<i64>,
    string_data: HashMap<usize, String>,
    float_data: HashMap<usize, i64>,
    max_steps: usize,
    argv: Vec<String>,
) -> (Vec<Vec<u8>>, i64, ExecProfile) {
    let mut emu = Emulator::new();
    emu.load_program(words);
    emu.string_data = string_data.into_iter().map(|(k, v)| (k, v.into_bytes())).collect();
    emu.float_data = float_data;
    emu.max_steps = max_steps;
    emu.argv = argv;
    let out = emu.run_with_output();
    // A5: the exit status normally comes from main's return value in R1, but a
    // program stopped by a TRAP never returned, so R1 holds whatever the
    // faulting instruction left behind — including 0, which would report
    // success. Report a distinct, documented failure status instead.
    //
    // The step limit gets its OWN status rather than reusing 70: "you cut me
    // off" and "I faulted" are different facts, and a caller that cannot tell
    // them apart will misclassify the first as the second.
    let code = if emu.step_limited {
        T3_STEP_LIMIT_EXIT
    } else if emu.trapped {
        T3_TRAP_EXIT
    } else {
        emu.regs[1]
    };
    (out, code, emu.profile.clone())
}

/// Process exit status used when a T3 program stops on a TRAP (A5).
/// Chosen to be non-zero and below 128 so it is not confused with death by
/// signal, and stable so scripts can test for it.
pub const T3_TRAP_EXIT: i64 = 70;

/// Process exit status when the program was cut off at the step limit.
///
/// Separate from `T3_TRAP_EXIT` because the two are different events and a
/// caller has to be able to tell them apart: 70 means the program faulted, 71
/// means it was still running and we stopped it. Sharing 70 is what let a
/// truncated run be scored as a cross-backend divergence.
pub const T3_STEP_LIMIT_EXIT: i64 = 71;

/// Default instruction budget for `run-t3`, overridable with `--max-steps`.
///
/// **This number is measured, not chosen for looking generous** (ORACLE_FINDINGS
/// §2, 22 Aug 2026). Over the whole 89-program corpus that compiles and runs on
/// T3, the median program takes 4,775 steps and p90 is 26,620. Exactly one
/// program exceeded the old 10 M cap — `benchmarks/01_arithmetic.mt` at
/// 20,891,806 — with ManiTBench T1-13 next at 11,656,667. Ordinary programs sit
/// three to four orders of magnitude below.
///
/// The old cap was not protecting anything. Uncapped, `01_arithmetic` finishes
/// in 0.183 s: the emulator runs at ~114 M steps/s, so 10 M was **0.09 seconds**
/// of emulation, and it was truncating real work rather than catching runaways.
///
/// 1e9 is ~8.8 s at that rate — the largest round value whose worst case still
/// lands inside the oracle's own 20 s per-stage wall-clock timeout with better
/// than 2x margin. That ordering is the entire point: the DETERMINISTIC bound
/// must always fire before the non-deterministic one, or the same program starts
/// producing different verdicts on a loaded machine and an idle one. It is also
/// 48x the largest real program.
///
/// Keep a step cap; never replace it with a wall-clock timeout.
pub const DEFAULT_MAX_STEPS: usize = 1_000_000_000;

/// Run emulator in interactive debug mode.
pub fn run_emulator_debug(
    words: Vec<i64>,
    string_data: HashMap<usize, String>,
    float_data: HashMap<usize, i64>,
) -> Vec<Vec<u8>> {
    run_emulator_debug_argv(words, string_data, float_data, Emulator::new().argv)
}

/// As [`run_emulator_debug`], with the program's command-line arguments.
pub fn run_emulator_debug_argv(
    words: Vec<i64>,
    string_data: HashMap<usize, String>,
    float_data: HashMap<usize, i64>,
    argv: Vec<String>,
) -> Vec<Vec<u8>> {
    let mut emu = Emulator::new();
    emu.load_program(words);
    emu.string_data = string_data.into_iter().map(|(k, v)| (k, v.into_bytes())).collect();
    emu.float_data = float_data;
    emu.debug = true;
    emu.argv = argv;
    emu.run_with_output()
}

/// Run emulator and return both output and execution profile.
pub fn run_emulator_profiled(
    words: Vec<i64>,
    string_data: HashMap<usize, String>,
    float_data: HashMap<usize, i64>,
) -> (Vec<Vec<u8>>, ExecProfile) {
    let mut emu = Emulator::new();
    emu.load_program(words);
    emu.string_data = string_data.into_iter().map(|(k, v)| (k, v.into_bytes())).collect();
    emu.float_data = float_data;
    emu.run();
    let profile = emu.profile.clone();
    (emu.output.clone(), profile)
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

