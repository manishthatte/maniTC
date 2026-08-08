use super::*;
use crate::codegen_t3::assembler::{assemble, write_t3_binary, read_t3_binary};

#[test]
fn test_encode_decode() {
    let word = encode(Opcode::Tadd, 1, 2, 3, 0);
    let (op, r1, r2, r3, imm) = decode(word);
    assert_eq!(op, Opcode::Tadd as i64);
    assert_eq!((r1, r2, r3, imm), (1, 2, 3, 0));
}

#[test]
fn test_tlit_encode_decode() {
    let word = encode_wide(Opcode::Tlit, 5, 1234);
    let (op, r1, _, _, _) = decode(word);
    assert_eq!(op, Opcode::Tlit as i64);
    assert_eq!(r1, 5);
    assert_eq!(decode_tlit_imm(word), 1234);
}

#[test]
fn test_tlit_negative_imm() {
    let word = encode_wide(Opcode::Tlit, 2, -99);
    assert_eq!(decode_tlit_imm(word), -99);
}

#[test]
fn test_clamp27() {
    assert_eq!(clamp27(T3_MAX + 1), T3_MAX);
    assert_eq!(clamp27(T3_MIN - 1), T3_MIN);
    assert_eq!(clamp27(0), 0);
}

#[test]
fn test_emu_tadd() {
    let mut emu = Emulator::new();
    emu.load_program(vec![
        encode_wide(Opcode::Tlit, 1, 10),
        encode_wide(Opcode::Tlit, 2, 20),
        encode(Opcode::Tadd, 3, 1, 2, 0),
        encode(Opcode::Halt, 0, 0, 0, 0),
    ]);
    emu.run();
    assert_eq!(emu.regs[3], 30);
}

#[test]
fn test_emu_tsub() {
    let mut emu = Emulator::new();
    emu.load_program(vec![
        encode_wide(Opcode::Tlit, 1, 15),
        encode_wide(Opcode::Tlit, 2, 7),
        encode(Opcode::Tsub, 3, 1, 2, 0),
        encode(Opcode::Halt, 0, 0, 0, 0),
    ]);
    emu.run();
    assert_eq!(emu.regs[3], 8);
}

#[test]
fn test_emu_tneg() {
    let mut emu = Emulator::new();
    emu.load_program(vec![
        encode_wide(Opcode::Tlit, 1, 42),
        encode(Opcode::Tneg, 2, 1, 0, 0),
        encode(Opcode::Halt, 0, 0, 0, 0),
    ]);
    emu.run();
    assert_eq!(emu.regs[2], -42);
}

#[test]
fn test_emu_tcmp_gt() {
    let mut emu = Emulator::new();
    emu.load_program(vec![
        encode_wide(Opcode::Tlit, 1, 5),
        encode_wide(Opcode::Tlit, 2, 3),
        encode(Opcode::Tcmp, 3, 1, 2, 0),
        encode(Opcode::Halt, 0, 0, 0, 0),
    ]);
    emu.run();
    assert_eq!(emu.regs[3], 1);
}

#[test]
fn test_emu_tcmp_lt() {
    let mut emu = Emulator::new();
    emu.load_program(vec![
        encode_wide(Opcode::Tlit, 1, 2),
        encode_wide(Opcode::Tlit, 2, 9),
        encode(Opcode::Tcmp, 3, 1, 2, 0),
        encode(Opcode::Halt, 0, 0, 0, 0),
    ]);
    emu.run();
    assert_eq!(emu.regs[3], -1);
}

#[test]
fn test_emu_tcmp_eq() {
    let mut emu = Emulator::new();
    emu.load_program(vec![
        encode_wide(Opcode::Tlit, 1, 7),
        encode_wide(Opcode::Tlit, 2, 7),
        encode(Opcode::Tcmp, 3, 1, 2, 0),
        encode(Opcode::Halt, 0, 0, 0, 0),
    ]);
    emu.run();
    assert_eq!(emu.regs[3], 0);
}

#[test]
fn test_emu_tmin_tmax() {
    let mut emu = Emulator::new();
    emu.load_program(vec![
        encode_wide(Opcode::Tlit, 1, -3),
        encode_wide(Opcode::Tlit, 2, 5),
        encode(Opcode::Tmin, 3, 1, 2, 0),
        encode(Opcode::Tmax, 4, 1, 2, 0),
        encode(Opcode::Halt, 0, 0, 0, 0),
    ]);
    emu.run();
    assert_eq!(emu.regs[3], -3);
    assert_eq!(emu.regs[4], 5);
}

#[test]
fn test_emu_tshi_tshr() {
    let mut emu = Emulator::new();
    emu.load_program(vec![
        encode_wide(Opcode::Tlit, 1, 2),
        encode(Opcode::Tshi, 2, 1, 0, 3), // R2 = R1 * 3^3 = 2*27 = 54
        encode(Opcode::Tshr, 3, 2, 0, 3), // R3 = R2 / 3^3 = 54/27 = 2
        encode(Opcode::Halt, 0, 0, 0, 0),
    ]);
    emu.run();
    assert_eq!(emu.regs[2], 54);
    assert_eq!(emu.regs[3], 2);
}

#[test]
fn test_emu_mov() {
    let mut emu = Emulator::new();
    emu.load_program(vec![
        encode_wide(Opcode::Tlit, 1, 99),
        encode(Opcode::Mov, 2, 1, 0, 0),
        encode(Opcode::Halt, 0, 0, 0, 0),
    ]);
    emu.run();
    assert_eq!(emu.regs[2], 99);
}

#[test]
fn test_emu_r0_always_zero() {
    let mut emu = Emulator::new();
    // Attempt to write to R0 via MOV
    emu.load_program(vec![
        encode_wide(Opcode::Tlit, 1, 42),
        encode(Opcode::Mov, 0, 1, 0, 0),
        encode(Opcode::Halt, 0, 0, 0, 0),
    ]);
    emu.run();
    assert_eq!(emu.regs[0], 0);
}

#[test]
fn test_emu_load_store() {
    let mut emu = Emulator::new();
    // Store 77 at memory[100], then load it back.
    emu.load_program(vec![
        encode_wide(Opcode::Tlit, 1, 77),    // R1 = 77
        encode_wide(Opcode::Tlit, 2, 100),   // R2 = 100 (address)
        encode(Opcode::Store, 1, 2, 0, 0),   // mem[R2+0] = R1
        encode(Opcode::Load,  3, 2, 0, 0),   // R3 = mem[R2+0]
        encode(Opcode::Halt,  0, 0, 0, 0),
    ]);
    emu.run();
    assert_eq!(emu.regs[3], 77);
}

#[test]
fn test_emu_syscall_print_int() {
    let mut emu = Emulator::new();
    emu.load_program(vec![
        encode_wide(Opcode::Tlit, 1, 42),
        encode_wide(Opcode::Syscall, 0, 1),
        encode(Opcode::Halt, 0, 0, 0, 0),
    ]);
    emu.run();
    assert_eq!(emu.output, vec!["42"]);
}

#[test]
fn test_emu_syscall_print_trit() {
    let mut emu = Emulator::new();
    emu.load_program(vec![
        encode_wide(Opcode::Tlit, 1, -1),
        encode_wide(Opcode::Syscall, 0, 0),
        encode_wide(Opcode::Tlit, 1, 0),
        encode_wide(Opcode::Syscall, 0, 0),
        encode_wide(Opcode::Tlit, 1, 1),
        encode_wide(Opcode::Syscall, 0, 0),
        encode(Opcode::Halt, 0, 0, 0, 0),
    ]);
    emu.run();
    assert_eq!(emu.output, vec!["-", "0", "+"]);
}

#[test]
fn test_emu_jump() {
    let mut emu = Emulator::new();
    emu.load_program(vec![
        encode_wide(Opcode::Tlit, 1, 5),           // 0
        encode_wide(Opcode::Jump, 0, 3),            // 1: jump to instr 3
        encode_wide(Opcode::Tlit, 1, 999),          // 2: skipped
        encode(Opcode::Halt, 0, 0, 0, 0),           // 3
    ]);
    emu.run();
    assert_eq!(emu.regs[1], 5);
}

#[test]
fn test_emu_tbranch_pos() {
    // R5 = +1 → TbrPos fires, jumps to pos_target (instr 5)
    // Layout: 0=TLIT R5,1  1=TLIT R1,0  2=TbrPos R5,5  3=TbrZero R5,7  4=Jump 9
    //         5=TLIT R1,111  6=HALT   7=TLIT R1,222  8=HALT  9=TLIT R1,333  10=HALT
    let mut emu = Emulator::new();
    emu.load_program(vec![
        encode_wide(Opcode::Tlit,    5, 1),   // 0: R5 = 1
        encode_wide(Opcode::Tlit,    1, 0),   // 1: R1 = 0
        encode_wide(Opcode::TbrPos,  5, 5),   // 2: if R5>0 → pc=5
        encode_wide(Opcode::TbrZero, 5, 7),   // 3: if R5==0 → pc=7
        encode_wide(Opcode::Jump,    0, 9),   // 4: → pc=9 (neg)
        encode_wide(Opcode::Tlit,    1, 111), // 5: pos
        encode(Opcode::Halt, 0, 0, 0, 0),     // 6
        encode_wide(Opcode::Tlit,    1, 222), // 7: zero
        encode(Opcode::Halt, 0, 0, 0, 0),     // 8
        encode_wide(Opcode::Tlit,    1, 333), // 9: neg
        encode(Opcode::Halt, 0, 0, 0, 0),     // 10
    ]);
    emu.run();
    assert_eq!(emu.regs[1], 111);
}

#[test]
fn test_emu_tbranch_zero() {
    let mut emu = Emulator::new();
    emu.load_program(vec![
        encode_wide(Opcode::Tlit,    5, 0),   // 0: R5 = 0
        encode_wide(Opcode::Tlit,    1, 0),   // 1: R1 = 0
        encode_wide(Opcode::TbrPos,  5, 5),   // 2: if R5>0 → pc=5
        encode_wide(Opcode::TbrZero, 5, 7),   // 3: if R5==0 → pc=7
        encode_wide(Opcode::Jump,    0, 9),   // 4: → pc=9 (neg)
        encode_wide(Opcode::Tlit,    1, 111), // 5
        encode(Opcode::Halt, 0, 0, 0, 0),     // 6
        encode_wide(Opcode::Tlit,    1, 222), // 7
        encode(Opcode::Halt, 0, 0, 0, 0),     // 8
        encode_wide(Opcode::Tlit,    1, 333), // 9
        encode(Opcode::Halt, 0, 0, 0, 0),     // 10
    ]);
    emu.run();
    assert_eq!(emu.regs[1], 222);
}

#[test]
fn test_emu_tbranch_neg() {
    let mut emu = Emulator::new();
    emu.load_program(vec![
        encode_wide(Opcode::Tlit,    5, -1),  // 0: R5 = -1
        encode_wide(Opcode::Tlit,    1, 0),   // 1: R1 = 0
        encode_wide(Opcode::TbrPos,  5, 5),   // 2: if R5>0 → pc=5
        encode_wide(Opcode::TbrZero, 5, 7),   // 3: if R5==0 → pc=7
        encode_wide(Opcode::Jump,    0, 9),   // 4: → pc=9 (neg)
        encode_wide(Opcode::Tlit,    1, 111), // 5
        encode(Opcode::Halt, 0, 0, 0, 0),     // 6
        encode_wide(Opcode::Tlit,    1, 222), // 7
        encode(Opcode::Halt, 0, 0, 0, 0),     // 8
        encode_wide(Opcode::Tlit,    1, 333), // 9
        encode(Opcode::Halt, 0, 0, 0, 0),     // 10
    ]);
    emu.run();
    assert_eq!(emu.regs[1], 333);
}

#[test]
fn test_emu_call_ret() {
    // CALL to a "double" function at instr 3: R1 = R1 + R1; RET
    let mut emu = Emulator::new();
    emu.load_program(vec![
        encode_wide(Opcode::Tlit, 1, 5),           // 0: R1 = 5
        encode_wide(Opcode::Call, 0, 3),            // 1: call instr 3
        encode(Opcode::Halt, 0, 0, 0, 0),           // 2: halt after return
        encode(Opcode::Tadd, 1, 1, 1, 0),           // 3: R1 = R1 + R1
        encode(Opcode::Ret,  0, 0, 0, 0),           // 4: return
    ]);
    emu.run();
    assert_eq!(emu.regs[1], 10);
}

#[test]
fn test_emu_tmul() {
    let mut emu = Emulator::new();
    emu.load_program(vec![
        encode_wide(Opcode::Tlit, 1, 6),
        encode_wide(Opcode::Tlit, 2, 7),
        encode(Opcode::Tmul, 3, 1, 2, 0),
        encode(Opcode::Halt, 0, 0, 0, 0),
    ]);
    emu.run();
    assert_eq!(emu.regs[3], 42);
}

#[test]
fn test_emu_tdiv_tmod() {
    let mut emu = Emulator::new();
    emu.load_program(vec![
        encode_wide(Opcode::Tlit, 1, 17),
        encode_wide(Opcode::Tlit, 2, 5),
        encode(Opcode::Tdiv, 3, 1, 2, 0),
        encode(Opcode::Tmod, 4, 1, 2, 0),
        encode(Opcode::Halt, 0, 0, 0, 0),
    ]);
    emu.run();
    assert_eq!(emu.regs[3], 3);
    assert_eq!(emu.regs[4], 2);
}

#[test]
fn test_assembler_basic() {
    let asm = r#"
main:
  entry:
TLIT  R1, #7
TLIT  R2, #3
TADD  R3, R1, R2
HALT
"#;
    let (words, _, _) = assemble(asm).expect("assemble failed");
    let mut emu = Emulator::new();
    emu.load_program(words);
    emu.run();
    assert_eq!(emu.regs[3], 10);
}

#[test]
fn test_assembler_jump() {
    let asm = r#"
main:
  entry:
TLIT  R1, #99
JUMP  done
TLIT  R1, #0
  done:
HALT
"#;
    let (words, _, _) = assemble(asm).expect("assemble failed");
    let mut emu = Emulator::new();
    emu.load_program(words);
    emu.run();
    assert_eq!(emu.regs[1], 99);
}

#[test]
fn test_assembler_tbranch() {
    let asm = r#"
main:
  entry:
TLIT  R5, #1
TBRANCH R5, pos, zero, neg
  pos:
TLIT  R1, #111
HALT
  zero:
TLIT  R1, #222
HALT
  neg:
TLIT  R1, #333
HALT
"#;
    let (words, _, _) = assemble(asm).expect("assemble failed");
    let mut emu = Emulator::new();
    emu.load_program(words);
    emu.run();
    assert_eq!(emu.regs[1], 111);
}

#[test]
fn test_assembler_call_ret() {
    let asm = r#"
main:
  entry:
TLIT  R1, #5
CALL  double
HALT
double:
TADD  R1, R1, R1
RET
"#;
    let (words, _, _) = assemble(asm).expect("assemble failed");
    let mut emu = Emulator::new();
    emu.load_program(words);
    emu.run();
    assert_eq!(emu.regs[1], 10);
}

#[test]
fn test_assembler_syscall_print_int() {
    let asm = r#"
main:
  entry:
TLIT  R1, #42
SYSCALL #1
HALT
"#;
    let (words, _, _) = assemble(asm).expect("assemble failed");
    let mut emu = Emulator::new();
    emu.load_program(words);
    emu.run();
    assert_eq!(emu.output, vec!["42"]);
}

#[test]
fn test_assembler_load_store() {
    let asm = r#"
main:
  entry:
TLIT  R1, #55
TLIT  R2, #200
STORE R1, [R2+#0]
LOAD  R3, [R2+#0]
HALT
"#;
    let (words, _, _) = assemble(asm).expect("assemble failed");
    let mut emu = Emulator::new();
    emu.load_program(words);
    emu.run();
    assert_eq!(emu.regs[3], 55);
}

#[test]
fn test_assembler_mov() {
    let asm = r#"
main:
  entry:
TLIT  R1, #77
MOV   R4, R1
HALT
"#;
    let (words, _, _) = assemble(asm).expect("assemble failed");
    let mut emu = Emulator::new();
    emu.load_program(words);
    emu.run();
    assert_eq!(emu.regs[4], 77);
}

#[test]
fn test_binary_roundtrip() {
    let words = vec![
        encode_wide(Opcode::Tlit, 1, 100),
        encode(Opcode::Halt, 0, 0, 0, 0),
    ];
    let path = "/tmp/test_t3.bin";
    write_t3_binary(&words, path).unwrap();
    let loaded = read_t3_binary(path).unwrap();
    assert_eq!(words, loaded);
}

#[test]
fn test_run_emulator_api() {
    let words = vec![
        encode_wide(Opcode::Tlit, 1, 7),
        encode_wide(Opcode::Syscall, 0, 1),
        encode(Opcode::Halt, 0, 0, 0, 0),
    ];
    let out = run_emulator(words, HashMap::new(), HashMap::new());
    assert_eq!(out, vec!["7"]);
}

#[test]
fn test_emu_tand_tor() {
    // tand = min, tor = max
    let mut emu = Emulator::new();
    emu.load_program(vec![
        encode_wide(Opcode::Tlit, 1, -1),
        encode_wide(Opcode::Tlit, 2, 1),
        encode(Opcode::Tand, 3, 1, 2, 0),   // min(-1,1) = -1
        encode(Opcode::Tor,  4, 1, 2, 0),   // max(-1,1) = 1
        encode(Opcode::Halt, 0, 0, 0, 0),
    ]);
    emu.run();
    assert_eq!(emu.regs[3], -1);
    assert_eq!(emu.regs[4], 1);
}

#[test]
fn test_emu_tnot() {
    let mut emu = Emulator::new();
    emu.load_program(vec![
        encode_wide(Opcode::Tlit, 1, 5),
        encode(Opcode::Tnot, 2, 1, 0, 0),
        encode(Opcode::Halt, 0, 0, 0, 0),
    ]);
    emu.run();
    assert_eq!(emu.regs[2], -5);
}

#[test]
fn test_emu_flags() {
    let mut emu = Emulator::new();
    emu.load_program(vec![
        encode_wide(Opcode::Tlit, 1, 3),
        encode_wide(Opcode::Tlit, 2, 3),
        encode(Opcode::Tsub, 3, 1, 2, 0), // 3 - 3 = 0 → flags = 0
        encode(Opcode::Halt, 0, 0, 0, 0),
    ]);
    emu.run();
    assert_eq!(emu.flags, 0);
    assert_eq!(emu.regs[3], 0);
}
