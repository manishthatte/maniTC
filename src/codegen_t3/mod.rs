// codegen_t3/mod.rs — T3ISA balanced ternary codegen and emulator for maniT
//
// Architecture summary:
//   27-trit word, values −3812798742493 .. +3812798742493, stored as i64
//   27 registers: R0 (zero), R1–R25 (general), R26 (SP)
//   FLAGS: i8 ∈ {−1, 0, +1}

pub mod isa;
pub mod regalloc;
pub mod assembler;
pub mod emulator;
pub mod emitter;

pub use isa::*;
pub use assembler::{assemble, write_t3_binary, read_t3_binary, read_t3_binary_with_magic, T3B_MAGIC};
pub use emulator::{run_emulator, run_emulator_debug, run_emulator_debug_argv,
                   run_emulator_profiled, run_emulator_with_exit,
                   run_emulator_with_exit_capped, run_emulator_with_exit_capped_argv,
                   run_emulator_with_exit_capped_argv_profiled,
                   DEFAULT_MAX_STEPS, T3_STEP_LIMIT_EXIT, Emulator, ExecProfile};
pub use emitter::emit_t3_asm;
