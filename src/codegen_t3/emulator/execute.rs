// emulator/execute.rs — Instruction fetch-decode-execute (step() method).
use super::*;
use super::super::isa::*;

impl Emulator {
    pub fn step(&mut self) {
        if self.halted { return; }
        if self.pc >= self.memory.len() {
            self.halted = true;
            return;
        }

        let word = self.memory[self.pc];
        self.pc += 1;

        let (raw_op, r1, r2, r3, imm) = decode(word);
        let Some(op) = Opcode::from_i64(raw_op) else {
            // Silently skipping unknown opcodes let garbage (e.g. a source file
            // read as a binary) "run" to a clean exit — trap loudly instead.
            self.output.push(format!(
                "TRAP: unknown opcode {} at PC={} (word {})", raw_op, self.pc - 1, word));
            self.halted = true;
            return;
        };

        // Record instruction in profile
        self.profile.record(op);

        macro_rules! wreg {
            ($r:expr, $v:expr) => {
                if ($r as usize) != 0 {
                    self.regs[$r as usize] = clamp27($v);
                }
            };
        }

        // Safe register accessors — clamp to valid range (0..26) for safety with encode_wide ops
        let sr1 = (r1 as usize).min(26);
        let sr2 = (r2 as usize).min(26);
        let sr3 = (r3 as usize).min(26);
        // Effective rhs: regs[r3] + imm. This lets r3=0 with imm=n encode immediates.
        let rhs_eff = self.regs[sr3] + imm;

        match op {
            Opcode::Nop => {}

            Opcode::Tadd => {
                let v = clamp27(self.regs[sr2] + rhs_eff);
                self.flags = sign_i64(v);
                wreg!(r1, v);
            }
            Opcode::Tsub => {
                let v = clamp27(self.regs[sr2] - rhs_eff);
                self.flags = sign_i64(v);
                wreg!(r1, v);
            }
            Opcode::Tmul => {
                let v = clamp27(self.regs[sr2].saturating_mul(rhs_eff));
                self.flags = sign_i64(v);
                wreg!(r1, v);
            }
            Opcode::Tdiv => {
                if rhs_eff == 0 {
                    self.output.push("TRAP: division by zero".to_string());
                    self.halted = true;
                    return;
                }
                let v = clamp27(self.regs[sr2] / rhs_eff);
                self.flags = sign_i64(v);
                wreg!(r1, v);
            }
            Opcode::Tmod => {
                if rhs_eff == 0 {
                    self.output.push("TRAP: modulo by zero".to_string());
                    self.halted = true;
                    return;
                }
                let v = clamp27(self.regs[sr2] % rhs_eff);
                self.flags = sign_i64(v);
                wreg!(r1, v);
            }
            Opcode::Tneg => {
                let v = clamp27(-self.regs[sr2]);
                self.flags = sign_i64(v);
                wreg!(r1, v);
            }
            Opcode::Tand => {
                let v = self.regs[sr2].min(rhs_eff);
                self.flags = sign_i64(v);
                wreg!(r1, v);
            }
            Opcode::Tor => {
                let v = self.regs[sr2].max(rhs_eff);
                self.flags = sign_i64(v);
                wreg!(r1, v);
            }
            Opcode::Tnot => {
                // same as tneg for a word value
                let v = clamp27(-self.regs[sr2]);
                self.flags = sign_i64(v);
                wreg!(r1, v);
            }
            Opcode::Tshi => {
                // multiply by 3^n; shift amount from register (r3) or immediate
                let n = rhs_eff.clamp(0, 26) as u32;
                let v = clamp27(self.regs[sr2].saturating_mul(3i64.pow(n)));
                self.flags = sign_i64(v);
                wreg!(r1, v);
            }
            Opcode::Tshr => {
                // Balanced ternary shift right: drop the n low trits.  Because
                // balanced digits are -1/0/+1, dropping k trits is ROUND-TO-
                // NEAREST division by 3^k (ties impossible: 3^k is odd), not
                // truncation: 5 >> 1 = 2, -5 >> 1 = -2.
                let n = rhs_eff.clamp(0, 26) as u32;
                let p = 3i64.pow(n);
                let v = clamp27((self.regs[sr2] + (p - 1) / 2).div_euclid(p));
                self.flags = sign_i64(v);
                wreg!(r1, v);
            }
            Opcode::Tmin => {
                let v = self.regs[sr2].min(rhs_eff);
                wreg!(r1, v);
            }
            Opcode::Tmax => {
                let v = self.regs[sr2].max(rhs_eff);
                wreg!(r1, v);
            }
            Opcode::Band => {
                let v = clamp27((self.regs[sr2] as i64) & (rhs_eff as i64));
                self.flags = sign_i64(v);
                wreg!(r1, v);
            }
            Opcode::Bor => {
                let v = clamp27((self.regs[sr2] as i64) | (rhs_eff as i64));
                self.flags = sign_i64(v);
                wreg!(r1, v);
            }
            Opcode::Bxor => {
                let v = clamp27((self.regs[sr2] as i64) ^ (rhs_eff as i64));
                self.flags = sign_i64(v);
                wreg!(r1, v);
            }
            Opcode::Bshl => {
                let n = (rhs_eff.clamp(0, 63)) as u32;
                let v = clamp27(self.regs[sr2].wrapping_shl(n));
                self.flags = sign_i64(v);
                wreg!(r1, v);
            }
            Opcode::Bshr => {
                let n = (rhs_eff.clamp(0, 63)) as u32;
                let v = clamp27(self.regs[sr2].wrapping_shr(n));
                self.flags = sign_i64(v);
                wreg!(r1, v);
            }
            Opcode::Tcmp => {
                let diff = self.regs[sr2] - rhs_eff;
                let v = sign_i64(diff) as i64;
                self.flags = v as i8;
                wreg!(r1, v);
            }

            Opcode::Load => {
                // encoding: LOAD r1, [r2 + imm]
                let addr = (self.regs[sr2] + imm) as usize;
                let v = self.memory.get(addr).copied().unwrap_or(0);
                if (r1 as usize) != 0 {
                    self.regs[r1 as usize] = v;
                }
            }
            Opcode::Store => {
                // encoding: STORE r1, [r2 + imm]
                let addr = (self.regs[sr2] + imm) as usize;
                if addr < self.memory.len() {
                    self.memory[addr] = self.regs[sr1];
                }
            }

            Opcode::Tlit => {
                // The wide immediate occupies lower P13 trits.
                let imm_val = decode_tlit_imm(word);
                wreg!(r1, imm_val);
            }

            Opcode::Mov => {
                let v = self.regs[sr2];
                if (r1 as usize) != 0 {
                    self.regs[r1 as usize] = v;
                }
            }

            Opcode::Tbranch => {
                // Legacy 3-address packed format (kept for compatibility with manually written asm)
                let packed = word - (raw_op * P18) - (r1 * P13);
                let p12 = 531_441i64;
                let p6  = 729i64;
                let addr_pos  = packed / p12;
                let rem       = packed % p12;
                let addr_zero = rem / p6;
                let addr_neg  = rem % p6;
                let cond = self.regs[sr1];
                self.pc = if cond > 0 { addr_pos as usize }
                          else if cond == 0 { addr_zero as usize }
                          else { addr_neg as usize };
            }
            Opcode::TbrPos => {
                // encode_wide(TbrPos, rcond, addr): jump to addr if regs[rcond] > 0
                let rcond = r1;
                let addr = word - raw_op * P18 - rcond * P13;
                if self.regs[rcond as usize] > 0 {
                    self.pc = addr as usize;
                }
            }
            Opcode::TbrZero => {
                let rcond = r1;
                let addr = word - raw_op * P18 - rcond * P13;
                if self.regs[rcond as usize] == 0 {
                    self.pc = addr as usize;
                }
            }
            Opcode::TbrNeg => {
                let rcond = r1;
                let addr = word - raw_op * P18 - rcond * P13;
                if self.regs[rcond as usize] < 0 {
                    self.pc = addr as usize;
                }
            }

            Opcode::Jump => {
                let addr = word - raw_op * P18;
                self.pc = addr as usize;
            }

            Opcode::Call => {
                let addr = word - raw_op * P18;
                // Track return address in the internal call stack only.
                // We do NOT write to memory[SP]: the codegen uses a full
                // caller-save convention (explicit STORE before CALL / LOAD
                // after RET), so any memory write here would corrupt the
                // top of the caller-save stack.
                self.call_depth += 1;
                if self.call_depth > 10000 {
                    panic!("Stack overflow: call depth exceeded 10000 (likely infinite recursion)");
                }
                self.call_stack.push(self.pc);
                self.regs[26] = clamp27(self.regs[26] - 1);
                if self.regs[26] < 0 {
                    panic!("Stack overflow: stack pointer went negative");
                }
                self.pc = addr as usize;
            }

            Opcode::Callr => {
                // CALLR Rx: call to address stored in register r1
                let r1 = ((word - raw_op * P18) / P13) as usize;
                let addr = self.regs[r1];
                self.call_depth += 1;
                if self.call_depth > 10000 {
                    panic!("Stack overflow: call depth exceeded 10000 (likely infinite recursion)");
                }
                self.call_stack.push(self.pc);
                self.regs[26] = clamp27(self.regs[26] - 1);
                if self.regs[26] < 0 {
                    panic!("Stack overflow: stack pointer went negative");
                }
                self.pc = addr as usize;
            }

            Opcode::Ret => {
                if let Some(ret_addr) = self.call_stack.pop() {
                    if self.call_depth > 0 { self.call_depth -= 1; }
                    self.regs[26] = clamp27(self.regs[26] + 1);
                    self.pc = ret_addr;
                } else {
                    self.halted = true;
                }
            }

            Opcode::Halt => {
                self.halted = true;
            }

            Opcode::Syscall => {
                self.do_syscall(decode_tlit_imm(word));
            }

            Opcode::Loadt => {
                // LOADT Rd, [Ra+imm] — load a single trit from memory, clamped to -1/0/+1
                let addr = (self.regs[sr2] + imm) as usize;
                let raw = self.memory.get(addr).copied().unwrap_or(0);
                let trit = raw.clamp(-1, 1);
                if (r1 as usize) != 0 {
                    self.regs[r1 as usize] = trit;
                }
                self.flags = sign_i64(trit);
            }
            Opcode::Storet => {
                // STORET Rs, [Ra+imm] — store a single trit to memory
                let addr = (self.regs[sr2] + imm) as usize;
                let trit = self.regs[sr1].clamp(-1, 1);
                if addr < self.memory.len() {
                    self.memory[addr] = trit;
                }
            }
        }
    }
}
