// emulator/syscalls.rs — Syscall router.
// Implementations are split into:
//   syscall_io.rs   — I/O, strings, floats, time, env (0-16, 60-69, 127-132, 133-136, 200-203, 210-221, 540, 550, 552-553)
//   syscall_fs.rs   — File system + TCP network (75-79, 500-525)
//   syscall_proc.rs — Collections, channels, concurrency (17-59, 70-74, 80-107, 108-131, 137)
use super::*;

impl Emulator {
    pub(super) fn do_syscall(&mut self, num: i64) {
        match num {
            // I/O: print/read, strings, floats, time, env.
            // NOTE: 131 (mutex_set_value) belongs to the proc handler, so the
            // fmt range here is 127-130 + 132.
            // 133-135: the two char primitives plus int_to_trits. They are not
            // in the 60-69 str block because that block is full and 70-82 are
            // already taken by the proc handler — 133 was the first genuinely
            // free number.
            // 551 was env::args and is now UNASSIGNED, on purpose: env::args is
            // maniT source (stdlib/env.mt) built over 552/553, so nothing emits
            // 551 any more and a binary that still does is stale, not valid.
            // Leaving the number out of the range makes that a visible TRAP
            // rather than a silent empty Vec.
            0..=16 | 60..=69 | 127..=130 | 132 | 133..=136 | 200..=203 | 210..=221 | 540
            | 550 | 552..=553 => {
                self.do_syscall_io(num);
            }
            // File system and network
            75..=79 | 500..=525 => {
                self.do_syscall_fs(num);
            }
            // Collections, channels, concurrency, scheduler.
            // 137 is `channel_bounded` (§11.11), out of line with the 70-74
            // channel block because 75-79 is the FILE SYSTEM — the first draft
            // used 75 and every bounded channel silently became an fs call.
            // 138-140 are §11.12's task handles (await, exit-with-value,
            // done-value), taken next to 137 for the same reason: the number
            // must be inside a range this router actually forwards, and an
            // unrouted number is a TRAP rather than a compile error.
            17..=59 | 70..=74 | 80..=131 | 137..=140 => {
                self.do_syscall_proc(num);
            }
            // `.unwrap()` guard — R1 = the Result's tag trit.
            //
            // The message text must stay byte-identical to
            // `manit_check_result_ok` in runtime/core.c. They are the one
            // hand-written pair in the whole Result implementation (everything
            // else is shared IR from ir/lower/lower_result.rs), so a
            // divergence between them is precisely what a two-backend test
            // would catch — and the reason there is only one such pair.
            561 => {
                let tag = self.regs[1];
                if tag != 1 {
                    self.trap(format!(
                        "TRAP: unwrap on a Result that is {}",
                        if tag == 0 { "Unknown" } else { "Err" }));
                }
            }
            // P113: the `unreachable` terminator. Takes no operands.
            //
            // The message text must stay byte-identical to
            // `manit_unreachable` in runtime/core.c — the second hand-written
            // pair in this file, and recorded as such for the same reason the
            // first one is.
            562 => {
                self.trap("TRAP: unreachable code reached — commonly a `match` with no arm for this value".to_string());
            }
            // A2: array bounds guard — R1 = index, R2 = length.
            560 => {
                let idx = self.regs[1];
                let len = self.regs[2];
                if idx < 0 || idx >= len {
                    self.trap(format!(
                        "TRAP: index {} is out of bounds for an array of length {}",
                        idx, len));
                }
            }
            _ => {
                self.trap_unknown_syscall(num);
            }
        }
    }

    /// Graceful path for syscall numbers with no handler: record a TRAP line
    /// instead of panicking, and leave execution to continue.
    pub(super) fn trap_unknown_syscall(&mut self, num: i64) {
        self.trap(format!("TRAP: unknown syscall #{}", num));
    }
}
