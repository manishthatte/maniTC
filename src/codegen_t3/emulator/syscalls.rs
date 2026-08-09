// emulator/syscalls.rs — Syscall router.
// Implementations are split into:
//   syscall_io.rs   — I/O, strings, floats, time, env (0-16, 60-69, 127-132, 200-202, 210-220, 540, 550-551)
//   syscall_fs.rs   — File system + TCP network (75-79, 500-525)
//   syscall_proc.rs — Collections, channels, concurrency (17-59, 70-74, 80-107, 108-131)
use super::*;

impl Emulator {
    pub(super) fn do_syscall(&mut self, num: i64) {
        match num {
            // I/O: print/read, strings, floats, time, env.
            // NOTE: 131 (mutex_set_value) belongs to the proc handler, so the
            // fmt range here is 127-130 + 132.
            0..=16 | 60..=69 | 127..=130 | 132 | 200..=202 | 210..=220 | 540 | 550..=551 => {
                self.do_syscall_io(num);
            }
            // File system and network
            75..=79 | 500..=525 => {
                self.do_syscall_fs(num);
            }
            // Collections, channels, concurrency, scheduler
            17..=59 | 70..=74 | 80..=131 => {
                self.do_syscall_proc(num);
            }
            _ => {
                self.trap_unknown_syscall(num);
            }
        }
    }

    /// Graceful path for syscall numbers with no handler: record a TRAP line
    /// instead of panicking, and leave execution to continue.
    pub(super) fn trap_unknown_syscall(&mut self, num: i64) {
        self.output.push(format!("TRAP: unknown syscall #{}", num));
    }
}
