// emulator/execute.rs — Instruction fetch-decode-execute (step() method).
use super::*;
use super::super::isa::*;

impl Emulator {
    /// Record a runtime fault and stop the machine.
    ///
    /// A5: every TRAP goes through here so that `trapped` is always set
    /// alongside `halted`; the exit status is derived from R1, which a faulting
    /// program has no reason to have left meaningful.
    pub(crate) fn trap(&mut self, msg: impl Into<String>) {
        // Output pieces are written raw and carry their own newlines, so a trap
        // message without one runs into whatever prints next.
        self.push_out(format!("{}\n", msg.into()));
        self.halted = true;
        self.trapped = true;
    }

    /// Result of a value-producing arithmetic op, checked against the 27-trit range.
    ///
    /// Returns `None` having already trapped when the true result does not fit.
    ///
    /// These ops used to run their result through `clamp27`, which silently
    /// substituted ±T3_MAX for the real value. A program could then compute a
    /// wrong answer and still report success — the exact failure `trapped`
    /// exists to prevent, and the same class of defect as the register
    /// allocator handing a callee the wrong operand. It was not hypothetical:
    /// `examples/fibonacci.mt` guards on the 64-bit range, so `fib_safe(70)`
    /// saturated to T3_MAX and returned `Ok(3812798742493)` instead of
    /// `Ok(190392490709135)` — a wrong answer wearing a success label, on the
    /// target whose whole purpose is to be the reference semantics.
    ///
    /// Overflow is a property of the program being run, not a compiler bug, so
    /// it is reported the way division by zero already was.
    ///
    /// `what` NAMES THE LANGUAGE OPERATION, NOT THE OPCODE (report.txt P21
    /// cluster 2). It used to be `TADD`/`TSUB`/`TMUL`, against the C runtime's
    /// `int addition`/`int subtraction`/`int multiplication` for the same
    /// fault — everything else in the two messages was already byte-identical,
    /// and this repo treats message parity as a correctness property
    /// (`manit_check_result_ok` in `runtime/core.c` is kept byte-identical to
    /// SYSCALL #561 and a comment there says why).
    ///
    /// The LANGUAGE side won the tie for three reasons, and the third decides
    /// it:
    ///
    /// 1. the reader is the author of the ManiT program, not someone debugging
    ///    the emulator, and `TADD` is a T3ISA opcode;
    /// 2. there is no `TADD` on the other backend at all, so naming it makes
    ///    the diagnostic un-reproducible for half the toolchain;
    /// 3. **an optimisation must not show through a diagnostic.** After F-2's
    ///    ternary strength reduction `x * 3` lowers to `TSHI`, so reporting
    ///    the opcode would make the message depend on whether the optimiser
    ///    fired — the programmer wrote a multiply and never asked for a shift.
    ///    `TSHI` is therefore `int multiplication` here, which is also what it
    ///    IS: shifting left by k trits is multiplying by 3^k.
    fn checked27(&mut self, v: i64, what: &str) -> Option<i64> {
        if v > T3_MAX || v < T3_MIN {
            self.trap(format!(
                "TRAP: {} overflow: result {} is outside the 27-trit range [{}, {}]",
                what, v, T3_MIN, T3_MAX
            ));
            return None;
        }
        Some(v)
    }

    /// Push a call frame, enforcing the recursion and stack-pointer limits.
    ///
    /// Returns false when the call must not proceed, having already trapped.
    /// A5: runaway recursion in a ManiT program used to abort the emulator
    /// with a Rust `panic!`; it is a property of the program being run, not a
    /// compiler bug, so report it the same way as every other runtime fault.
    fn push_call_frame(&mut self) -> bool {
        self.call_depth += 1;
        if self.call_depth > 10000 {
            self.trap("TRAP: call depth exceeded 10000 (likely infinite recursion)");
            return false;
        }
        self.call_stack.push(self.pc);
        self.regs[26] = clamp27(self.regs[26] - 1);
        if self.regs[26] < 0 {
            self.trap("TRAP: stack pointer went negative");
            return false;
        }
        true
    }

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
            self.trap(format!(
                "TRAP: unknown opcode {} at PC={} (word {})", raw_op, self.pc - 1, word));
            return;
        };

        // A5: `decode` pulls r1/r2/r3 out of 5-trit fields, so each spans
        // 0..=242 while the register file has 27 entries. encode/encode_wide
        // assert 0..=26, so an out-of-range field can only come from a binary
        // manitc did not produce (a hand-written or third-party .t3b, or a
        // corrupted file). Trap loudly, exactly like an unknown opcode above,
        // instead of panicking with an index-out-of-bounds.
        let bad_reg = if op.uses_wide_immediate() {
            // r2/r3/imm are immediate digits here, so only r1 is a register.
            (r1 > 26).then_some(r1)
        } else {
            [r1, r2, r3].into_iter().find(|&r| r > 26)
        };
        if let Some(r) = bad_reg {
            self.trap(format!(
                "TRAP: register index {} out of range (0..=26) at PC={} (word {})",
                r, self.pc - 1, word));
            return;
        }

        // Record instruction in profile
        self.profile.record(op);

        macro_rules! wreg {
            ($r:expr, $v:expr) => {
                if ($r as usize) != 0 {
                    self.regs[($r as usize).min(26)] = clamp27($v);
                }
            };
        }

        // Safe register accessors — clamp to valid range (0..26) for safety with encode_wide ops
        let sr1 = (r1 as usize).min(26);
        let sr2 = (r2 as usize).min(26);
        let sr3 = (r3 as usize).min(26);
        // Effective rhs: regs[r3] + imm. This lets r3=0 with imm=n encode immediates.
        //
        // Saturating, not plain `+`. Every arithmetic opcode below already
        // routes its result through `saturating_*` and then `checked27`, which
        // turns an out-of-range value into a diagnosed T3 fault; this one line
        // was the exception, and in a debug build it aborted the whole process
        // with "attempt to add with overflow" and no file:line in the ManiT
        // source. Found by the math::float surface (s23), where an intermediate
        // reaches i64::MAX before checked27 ever sees it. Saturating here
        // changes no in-range result — it only lets the existing fault path do
        // its job instead of being pre-empted by a panic.
        let rhs_eff = self.regs[sr3].saturating_add(imm);

        match op {
            Opcode::Nop => {}

            Opcode::Tadd => {
                let raw = self.regs[sr2].saturating_add(rhs_eff);
                let Some(v) = self.checked27(raw, "int addition") else { return };
                self.flags = sign_i64(v);
                wreg!(r1, v);
            }
            Opcode::Tsub => {
                let raw = self.regs[sr2].saturating_sub(rhs_eff);
                let Some(v) = self.checked27(raw, "int subtraction") else { return };
                self.flags = sign_i64(v);
                wreg!(r1, v);
            }
            Opcode::Tmul => {
                let raw = self.regs[sr2].saturating_mul(rhs_eff);
                let Some(v) = self.checked27(raw, "int multiplication") else { return };
                self.flags = sign_i64(v);
                wreg!(r1, v);
            }
            Opcode::Tdiv => {
                if rhs_eff == 0 {
                    self.trap("TRAP: division by zero");
                    return;
                }
                let v = clamp27(self.regs[sr2] / rhs_eff);
                self.flags = sign_i64(v);
                wreg!(r1, v);
            }
            Opcode::Tmod => {
                if rhs_eff == 0 {
                    self.trap("TRAP: modulo by zero");
                    return;
                }
                let v = clamp27(self.regs[sr2] % rhs_eff);
                self.flags = sign_i64(v);
                wreg!(r1, v);
            }
            // ---- T3ISA v1.6: round-to-nearest division (C4) --------------
            //
            // The rule is `crate::lang::div_nearest`, the same function the
            // constant folder uses and the same one `docs/semantics.md`
            // states. The emulator does not restate it: a machine and a folder
            // that each implement "round to nearest" from the prose would
            // agree on the seven obvious cases and diverge somewhere in the
            // negatives, and nothing in the test suite would be looking there.
            //
            // Neither can overflow the word. |q| <= |a| for every divisor of
            // magnitude >= 2, and for |b| == 1 the quotient is exactly ±a; the
            // rounding adjustment moves q by one only when it does not (r == 0
            // when |b| == 1, so the adjustment is not taken). checked27 is
            // still applied rather than assumed, on the principle that the
            // machine reports what it cannot represent instead of clamping —
            // the defect that made `fib_safe(70)` return a plausible T3_MAX.
            Opcode::Tdivn => {
                if rhs_eff == 0 {
                    self.trap("TRAP: division by zero");
                    return;
                }
                let raw = crate::lang::div_nearest(self.regs[sr2], rhs_eff);
                let Some(v) = self.checked27(raw, "int division") else { return };
                self.flags = sign_i64(v);
                wreg!(r1, v);
            }
            Opcode::Tmodn => {
                if rhs_eff == 0 {
                    self.trap("TRAP: modulo by zero");
                    return;
                }
                let raw = crate::lang::rem_balanced(self.regs[sr2], rhs_eff);
                let Some(v) = self.checked27(raw, "int remainder") else { return };
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
            // ---- T3ISA v1.5: lane-wise ternary logic (C2) ----------------
            //
            // Each of these does in one instruction what the standard library
            // currently does with 27 iterations of divide-by-3^i, operate,
            // multiply-back. The emulator pays a 27-iteration loop in Rust;
            // the COMPILED program does not, which is where the win lives.
            //
            // None of them can trap: every lane result is in {-1, 0, +1} by
            // construction, so the reassembled word is in range by
            // construction. That is a property of the balanced representation,
            // not a bound we are choosing to enforce.
            Opcode::Tandw => {
                let v = lanewise2(self.regs[sr2], rhs_eff, |a, b| a.min(b));
                self.flags = sign_i64(v);
                wreg!(r1, v);
            }
            Opcode::Torw => {
                let v = lanewise2(self.regs[sr2], rhs_eff, |a, b| a.max(b));
                self.flags = sign_i64(v);
                wreg!(r1, v);
            }
            Opcode::Txorw => {
                let v = lanewise2(self.regs[sr2], rhs_eff, trit_xor);
                self.flags = sign_i64(v);
                wreg!(r1, v);
            }
            Opcode::Timpw => {
                let v = lanewise2(self.regs[sr2], rhs_eff, trit_imp);
                self.flags = sign_i64(v);
                wreg!(r1, v);
            }
            Opcode::Tcmpw => {
                // Per-lane three-way compare. TCMP already returns -1/0/+1 for
                // whole words in one instruction; this is the same answer for
                // all 27 lanes at once.
                let v = lanewise2(self.regs[sr2], rhs_eff, |a, b| {
                    if a > b { 1 } else if a < b { -1 } else { 0 }
                });
                self.flags = sign_i64(v);
                wreg!(r1, v);
            }
            Opcode::Tpopc => {
                // Count lanes of Ra equal to the trit k. `k` comes from r3 or
                // the immediate, clamped into {-1, 0, +1}: a "count of lanes
                // equal to 7" has no meaning, and silently counting zero would
                // hide the mistake.
                let k = rhs_eff.clamp(-1, 1) as i8;
                let lanes = trits27(self.regs[sr2]);
                let v = lanes.iter().filter(|&&t| t == k).count() as i64;
                self.flags = sign_i64(v);
                wreg!(r1, v);
            }
            Opcode::Tselw => {
                // TSELW Rd, Rs, Ra, Rb — per-lane select on a mask word. This
                // is the branchless conditional the lane-wise set needs to be
                // useful, and it is genuinely THREE-way: the zero lane selects
                // zero, it does not pick one of two arms. A binary select has
                // no such case to make.
                //
                // Four registers in a three-register encoding. The 3-trit
                // immediate field holds 3^3 = 27 raw values and the register
                // file is R0..R26 — exactly 27. So Rb rides in the immediate
                // field read as UNSIGNED, which costs nothing and needs no new
                // instruction format. That the two numbers coincide is not a
                // coincidence: both are "what three trits address".
                let rb = imm.rem_euclid(P3) as usize;
                let sel = trits27(self.regs[sr2]);
                let a = trits27(self.regs[sr3]);
                let b = trits27(self.regs[rb.min(26)]);
                let mut out = [0i8; T3_LANES];
                for i in 0..T3_LANES {
                    out[i] = match sel[i] {
                        1 => a[i],
                        -1 => b[i],
                        _ => 0,
                    };
                }
                let v = from_trits27(&out);
                self.flags = sign_i64(v);
                wreg!(r1, v);
            }
            Opcode::Tshi => {
                // multiply by 3^n; shift amount from register (r3) or immediate
                let n = rhs_eff.clamp(0, 26) as u32;
                let raw = self.regs[sr2].saturating_mul(3i64.pow(n));
                let Some(v) = self.checked27(raw, "int multiplication") else { return };
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
                if sr1 != 0 {
                    self.regs[sr1] = v;
                }
            }
            Opcode::Store => {
                // encoding: STORE r1, [r2 + imm]
                let addr = (self.regs[sr2] + imm) as usize;
                // P76: a store BELOW the program image is the stack having grown
                // down into the code, and it was unbounded. The upper bound has
                // always been here; the lower one had not, so a deep enough
                // recursion overwrote its own instructions and the emulator then
                // executed them — reported as
                // `TRAP: register index 43 out of range (0..=26)`, which names
                // the symptom and not the cause. P38 checked that the IMAGE fits
                // below the stack; nothing checked that the STACK stays above
                // the image, and the call-depth guard cannot: it counts FRAMES,
                // so a 45-word frame overflows at depth ~1,300 while the guard
                // waits for 10,000.
                if addr < self.profile.program_words {
                    self.trap(format!(
                        "TRAP: stack overflow — store to {} is inside the program \
                         image (0..{}); the stack has grown down into the code",
                        addr, self.profile.program_words
                    ));
                    return;
                }
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
                if sr1 != 0 {
                    self.regs[sr1] = v;
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
                if self.regs[sr1] > 0 {
                    self.pc = addr as usize;
                }
            }
            Opcode::TbrZero => {
                let rcond = r1;
                let addr = word - raw_op * P18 - rcond * P13;
                if self.regs[sr1] == 0 {
                    self.pc = addr as usize;
                }
            }
            Opcode::TbrNeg => {
                let rcond = r1;
                let addr = word - raw_op * P18 - rcond * P13;
                if self.regs[sr1] < 0 {
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
                if !self.push_call_frame() { return; }
                self.pc = addr as usize;
            }

            Opcode::Callr => {
                // CALLR Rx: call to address stored in register r1
                let addr = self.regs[sr1];
                if !self.push_call_frame() { return; }
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
                if sr1 != 0 {
                    self.regs[sr1] = trit;
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
