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

// ---------------------------------------------------------------------------
// Encode/decode field-range regression tests (B1/B2/B4/B14)
// ---------------------------------------------------------------------------

#[test]
fn test_encode_decode_roundtrip_full_field_ranges() {
    // Every legal (r1, r2, r3, imm) combination must round-trip exactly,
    // including the balanced negative immediates that used to corrupt
    // adjacent fields.
    for r1 in 0..=26i64 {
        for r2 in (0..=26i64).step_by(5) {
            for r3 in (0..=26i64).step_by(5) {
                for imm in IMM_MIN..=IMM_MAX {
                    let word = encode(Opcode::Tadd, r1, r2, r3, imm);
                    let (op, d1, d2, d3, dimm) = decode(word);
                    assert_eq!(
                        (op, d1, d2, d3, dimm),
                        (Opcode::Tadd as i64, r1, r2, r3, imm),
                        "roundtrip failed for r1={} r2={} r3={} imm={}", r1, r2, r3, imm
                    );
                }
            }
        }
    }
}

#[test]
fn test_encode_wide_tlit_roundtrip_full_range_boundaries() {
    // The balanced 13-trit wide immediate holds exactly ±(3^13 − 1)/2.
    for &imm in &[0, 1, -1, 1000, -1000, 796_161, -796_161, 796_162, -796_162,
                  WIDE_IMM_MAX, -WIDE_IMM_MAX] {
        let word = encode_wide(Opcode::Tlit, 1, imm);
        assert_eq!(decode_tlit_imm(word), imm, "TLIT roundtrip failed for {}", imm);
    }
    // Strided sweep across the whole legal range.
    let mut imm = -WIDE_IMM_MAX;
    while imm <= WIDE_IMM_MAX {
        let word = encode_wide(Opcode::Tlit, 3, imm);
        assert_eq!(decode_tlit_imm(word), imm);
        imm += 997; // prime stride
    }
}

#[test]
#[should_panic(expected = "imm out of range")]
fn test_encode_rejects_imm_too_large() {
    let _ = encode(Opcode::Tadd, 1, 2, 0, 14);
}

#[test]
#[should_panic(expected = "imm out of range")]
fn test_encode_rejects_imm_too_small() {
    let _ = encode(Opcode::Tadd, 1, 2, 0, -14);
}

#[test]
#[should_panic(expected = "r1 out of range")]
fn test_encode_rejects_register_out_of_range() {
    let _ = encode(Opcode::Tadd, 27, 0, 0, 0);
}

#[test]
#[should_panic(expected = "wide_imm out of range")]
fn test_encode_wide_rejects_out_of_range() {
    let _ = encode_wide(Opcode::Tlit, 1, P13);
}

#[test]
fn test_assembler_rejects_out_of_range_immediates() {
    // 3-trit imm field holds -13..13: larger constants must be a clean error,
    // not silent field corruption.
    assert!(assemble("TADD R1, R2, #20").is_err());
    assert!(assemble("TSUB R1, R2, #-20").is_err());
    assert!(assemble("LOAD R1, [R2+#20]").is_err());
    assert!(assemble("STORE R1, [R2-20]").is_err());
    assert!(assemble("TLIT R1, #797162").is_err());
    assert!(assemble("TLIT R1, #-797162").is_err());
    assert!(assemble("SYSCALL #-1").is_err());
    // Legal boundary values still assemble.
    assert!(assemble("TADD R1, R2, #13").is_ok());
    assert!(assemble("TADD R1, R2, #-13").is_ok());
    assert!(assemble("TLIT R1, #797161").is_ok());
    assert!(assemble("TLIT R1, #-797161").is_ok());
}

#[test]
fn test_negative_immediates_execute_correctly() {
    // TADD R3, R2, #-5 — used to decode as garbage (B1).
    let asm = r#"
TLIT R2, #100
TADD R3, R2, #-5
HALT
"#;
    let (words, _, _) = assemble(asm).expect("assemble failed");
    let mut emu = Emulator::new();
    emu.load_program(words);
    emu.run();
    assert_eq!(emu.regs[3], 95);
}

#[test]
fn test_negative_load_store_offsets() {
    // LOAD/STORE with negative offsets (explicitly supported by parse_mem).
    let asm = r#"
TLIT R2, #1000
TLIT R1, #42
STORE R1, [R2-3]
LOAD R4, [R2+#-3]
HALT
"#;
    let (words, _, _) = assemble(asm).expect("assemble failed");
    let mut emu = Emulator::new();
    emu.load_program(words);
    emu.run();
    assert_eq!(emu.memory[997], 42);
    assert_eq!(emu.regs[4], 42);
}

// ---------------------------------------------------------------------------
// Labeled-instruction TBRANCH placeholders (B8)
// ---------------------------------------------------------------------------

#[test]
fn test_labeled_tbranch_same_encoding_as_bare() {
    // A `label: TBRANCH ...` line must reserve the same 3 words as a bare
    // TBRANCH, or every later label shifts by 2.
    let bare = r#"
start:
TBRANCH R1, pos, zero, neg
pos:
TLIT R2, #1
HALT
zero:
TLIT R2, #2
HALT
neg:
TLIT R2, #3
HALT
"#;
    let labeled = r#"
start: TBRANCH R1, pos, zero, neg
pos: TLIT R2, #1
HALT
zero: TLIT R2, #2
HALT
neg: TLIT R2, #3
HALT
"#;
    let (w1, _, _) = assemble(bare).expect("bare assemble failed");
    let (w2, _, _) = assemble(labeled).expect("labeled assemble failed");
    assert_eq!(w1, w2, "labeled TBRANCH must encode identically to bare TBRANCH");

    // And the branch targets must actually be correct at runtime.
    let (words, _, _) = assemble(labeled).unwrap();
    for (cond, expect) in [(1i64, 1i64), (0, 2), (-1, 3)] {
        let mut emu = Emulator::new();
        emu.load_program(words.clone());
        emu.regs[1] = cond;
        emu.run();
        assert_eq!(emu.regs[2], expect, "TBRANCH cond={} took wrong arm", cond);
    }
}

// ---------------------------------------------------------------------------
// TSHR round-to-nearest semantics (B13) + register shift amounts (B14)
// ---------------------------------------------------------------------------

#[test]
fn test_tshr_round_to_nearest() {
    // Dropping k low balanced trits is round-to-nearest division by 3^k.
    for (val, shift, expect) in [
        (5i64, 1i64, 2i64), (-5, 1, -2), (7, 1, 2), (-7, 1, -2),
        (4, 1, 1), (-4, 1, -1), (27, 1, 9), (9, 2, 1), (13, 0, 13),
    ] {
        let asm = format!("TLIT R1, #{}\nTSHR R2, R1, #{}\nHALT\n", val, shift);
        let (words, _, _) = assemble(&asm).unwrap();
        let mut emu = Emulator::new();
        emu.load_program(words);
        emu.run();
        assert_eq!(emu.regs[2], expect, "TSHR {} >> {} should be {}", val, shift, expect);
    }
}

#[test]
fn test_tshr_syscall_202_matches_instruction() {
    // Syscall 202 (t27_shift_right) must agree with the TSHR instruction.
    for (val, shift, expect) in [(5i64, 1i64, 2i64), (-5, 1, -2), (42, 1, 14), (9, 2, 1)] {
        let mut emu = Emulator::new();
        emu.regs[1] = val;
        emu.regs[2] = shift;
        emu.do_syscall(202);
        assert_eq!(emu.regs[1], expect, "syscall 202: {} >> {} should be {}", val, shift, expect);
    }
}

#[test]
fn test_shift_amount_in_register() {
    // TSHI/TSHR with a register shift amount (needed for constants > 13).
    let asm = r#"
TLIT R1, #2
TLIT R2, #14
TSHI R3, R1, R2
TLIT R4, #5
TLIT R5, #1
TSHR R6, R4, R5
HALT
"#;
    let (words, _, _) = assemble(asm).unwrap();
    let mut emu = Emulator::new();
    emu.load_program(words);
    emu.run();
    assert_eq!(emu.regs[3], 2 * 3i64.pow(14));
    assert_eq!(emu.regs[6], 2); // 5 >> 1 rounds to 2
}

// ---------------------------------------------------------------------------
// Syscall router totality (B3/B6): every claimed number dispatches without
// panicking — real handler or the graceful "TRAP: unknown syscall" path.
// ---------------------------------------------------------------------------

#[test]
fn test_syscall_router_totality() {
    let claimed: Vec<i64> = (0..=16)
        .chain(17..=59)
        .chain(60..=69)
        .chain(70..=74)
        .chain(75..=79)
        .chain(80..=131)
        .chain([132])
        .chain(200..=202)
        .chain(210..=220)
        .chain(500..=525)
        .chain([540, 550, 551])
        .collect();
    for num in claimed {
        let mut emu = Emulator::new();
        emu.input_queue.push_back(0); // syscall 5 (read int) must not block
        emu.do_syscall(num); // must not panic
    }
    // A number outside every claimed range takes the graceful TRAP path.
    let mut emu = Emulator::new();
    emu.do_syscall(9999);
    assert!(emu.output.iter().any(|l| l.contains("TRAP: unknown syscall #9999")));
}

#[test]
fn test_syscall_131_routes_to_mutex_set_value() {
    let mut emu = Emulator::new();
    emu.regs[1] = 7;
    emu.do_syscall(109); // mutex_new(7)
    let handle = emu.regs[1];
    emu.regs[1] = handle;
    emu.regs[2] = 99;
    emu.do_syscall(131); // mutex_set_value(handle, 99) — used to panic (B3)
    emu.regs[1] = handle;
    emu.do_syscall(111); // mutex_get(handle)
    assert_eq!(emu.regs[1], 99);
}

#[test]
fn test_unclaimed_syscalls_in_ranges_trap_gracefully() {
    // Numbers inside claimed router ranges but without a handler arm used to
    // hit unreachable!() and panic the emulator (B6).
    for num in [27, 28, 29, 37, 38, 39, 45, 46, 47, 48, 49, 217, 218, 513, 519] {
        let mut emu = Emulator::new();
        emu.do_syscall(num);
        // 37 (Map::values), 217 (print_bool3) and 218 (heap_alloc_words) now
        // have real handlers; everything else still traps.
        if num != 37 && num != 217 && num != 218 {
            assert!(
                emu.output.iter().any(|l| l.contains("TRAP: unknown syscall")),
                "syscall {} should trap gracefully", num
            );
        }
    }
}

// ---------------------------------------------------------------------------
// print_bool3 (B11), print_float routing (B7)
// ---------------------------------------------------------------------------

#[test]
fn test_syscall_218_heap_alloc_words() {
    let mut emu = Emulator::new();

    // Successive allocations must not overlap: a struct pointer that outlives
    // its loop iteration used to alias the next iteration's stack slot.
    emu.regs[1] = 4;
    emu.do_syscall(218);
    let first = emu.regs[1];
    emu.regs[1] = 4;
    emu.do_syscall(218);
    let second = emu.regs[1];
    assert_eq!(second - first, 4, "allocations must be disjoint");

    // Memory is handed back zeroed: struct literals read fields they have not
    // written yet.
    emu.memory[first as usize] = 7;
    emu.regs[1] = 2;
    emu.do_syscall(218);
    let third = emu.regs[1] as usize;
    assert!(emu.memory[third..third + 2].iter().all(|&w| w == 0));

    // A zero-word request still yields a distinct, usable address.
    emu.regs[1] = 0;
    emu.do_syscall(218);
    assert!(emu.regs[1] as usize >= third + 2);
}

#[test]
fn test_syscall_218_traps_when_heap_exhausted() {
    let mut emu = Emulator::new();
    // Overshoot the top of memory in one request rather than looping.
    emu.regs[1] = emu.memory.len() as i64;
    emu.do_syscall(218);
    assert!(emu.trapped, "heap exhaustion must trap, not drop the writes");
    assert!(
        emu.output.iter().any(|l| l.contains("heap exhausted")),
        "expected a heap-exhaustion message, got {:?}",
        emu.output
    );
}

// Arithmetic that leaves the 27-trit range must trap rather than clamp.
//
// Clamping was silent: the machine substituted ±T3_MAX for the true result and
// carried on, so a program computed a wrong number and still exited 0. It was
// caught by differential-testing the two backends against each other —
// `fib_safe(70)` returned Ok(3812798742493) on T3 and Ok(190392490709135) on
// LLVM — and the golden file had recorded the wrong answer as expected output,
// which is why the suite had been green over it.
#[test]
fn test_arithmetic_overflow_traps_instead_of_clamping() {
    // Run a single instruction with the given register contents.
    fn run_one(op: Opcode, lhs: i64, rhs: i64) -> Emulator {
        let mut emu = Emulator::new();
        emu.regs[2] = lhs;
        emu.regs[3] = rhs;
        emu.memory[0] = encode(op, 1, 2, 3, 0);
        emu.pc = 0;
        emu.step();
        emu
    }

    for (name, op, rhs) in [("TADD", Opcode::Tadd, 1), ("TSUB", Opcode::Tsub, -1)] {
        let emu = run_one(op, T3_MAX, rhs);
        assert!(emu.trapped, "{name} past T3_MAX must trap, not clamp");
        assert!(
            emu.output.iter().any(|l| l.contains("overflow")),
            "{name} overflow must say so, got {:?}",
            emu.output
        );
    }

    // Multiplication overshoots by far more than one, and saturating_mul must
    // not launder that into a plausible-looking T3_MAX.
    assert!(run_one(Opcode::Tmul, T3_MAX, 3).trapped, "TMUL overflow must trap");

    // The negative boundary is not symmetric by accident — check it too.
    assert!(run_one(Opcode::Tsub, T3_MIN, 1).trapped, "TSUB past T3_MIN must trap");

    // In-range arithmetic is untouched.
    let emu = run_one(Opcode::Tadd, T3_MAX - 1, 1);
    assert!(!emu.trapped, "arithmetic that fits must not trap");
    assert_eq!(emu.regs[1], T3_MAX);
}

// A length-prefixed string read must never invent characters it cannot see.
//
// The length word used to be taken on trust: a negative length became ~1.8e19
// as a usize and the body then pushed a NUL for every word past the end of
// memory. One bad address in `examples/data_structures.mt` produced 7.7 GB of
// output, and the run still exited 0.
#[test]
fn test_read_lp_string_rejects_implausible_lengths() {
    let mut emu = Emulator::new();

    // Negative length is a bad address, not a short string.
    emu.memory[5000] = -1;
    assert_eq!(emu.read_lp_string(5000), "");

    // A length running past the end of memory is not readable.
    emu.memory[5000] = emu.memory.len() as i64;
    assert_eq!(emu.read_lp_string(5000), "");

    // A well-formed string still reads back exactly.
    emu.memory[5000] = 3;
    for (i, c) in "abc".chars().enumerate() {
        emu.memory[5001 + i] = c as i64;
    }
    assert_eq!(emu.read_lp_string(5000), "abc");
}

#[test]
fn test_syscall_217_print_bool3_llvm_format() {
    for (val, expect) in [(1i64, "true"), (0, "unknown"), (-1, "false")] {
        let mut emu = Emulator::new();
        emu.regs[1] = val;
        emu.do_syscall(217);
        assert_eq!(emu.output, vec![expect.to_string()]);
    }
}

#[test]
fn test_syscall_2_prints_float() {
    let mut emu = Emulator::new();
    emu.regs[1] = 2.5f64.to_bits() as i64;
    emu.do_syscall(2);
    assert_eq!(emu.output, vec!["2.5".to_string()]);
}

// ---------------------------------------------------------------------------
// Barrier reuse (B16)
// ---------------------------------------------------------------------------

#[test]
fn test_barrier_resets_after_each_cycle() {
    let mut emu = Emulator::new();
    emu.regs[1] = 2;
    emu.do_syscall(117); // barrier_new(2)
    let handle = emu.regs[1];
    let mut results = Vec::new();
    for _ in 0..4 {
        emu.regs[1] = handle;
        emu.do_syscall(118); // barrier_wait
        results.push(emu.regs[1]);
    }
    // Two full cycles: waiter, leader, waiter, leader.
    assert_eq!(results, vec![0, 1, 0, 1], "barrier must reset after each cycle");
}

// ---------------------------------------------------------------------------
// Buffer length validation (B15)
// ---------------------------------------------------------------------------

#[test]
fn test_syscall_501_rejects_bad_buffer_lengths() {
    for bad_len in [-1i64, i64::MIN, 1 << 40] {
        let mut emu = Emulator::new();
        emu.regs[1] = 3;       // fd (nonexistent is fine)
        emu.regs[2] = 100;     // buf addr
        emu.regs[3] = bad_len; // used to abort with capacity overflow / OOM
        emu.do_syscall(501);
        assert_eq!(emu.regs[1], -1);
        assert!(emu.output.iter().any(|l| l.contains("invalid buffer length")));
    }
}

#[test]
fn test_syscall_524_rejects_bad_buffer_lengths() {
    let mut emu = Emulator::new();
    emu.regs[1] = 100;
    emu.regs[2] = 100;
    emu.regs[3] = -5;
    emu.do_syscall(524);
    assert_eq!(emu.regs[1], -1);
}

// ---------------------------------------------------------------------------
// align_right lp-string fallback (B17)
// ---------------------------------------------------------------------------

#[test]
fn test_align_right_reads_lp_string_from_memory() {
    let mut emu = Emulator::new();
    // Write "ab" as a length-prefixed string at address 2000.
    emu.memory[2000] = 2;
    emu.memory[2001] = 'a' as i64;
    emu.memory[2002] = 'b' as i64;
    emu.regs[1] = 2000;
    emu.regs[21] = 5;             // width
    emu.regs[22] = '.' as i64;    // fill
    emu.do_syscall(15);
    let addr = emu.regs[1] as usize;
    assert_eq!(emu.string_data.get(&addr).map(|s| s.as_str()), Some("...ab"));
}

// ---------------------------------------------------------------------------
// Unknown opcodes trap (B22) and binary magic header
// ---------------------------------------------------------------------------

#[test]
fn test_unknown_opcode_traps_instead_of_silent_skip() {
    let mut emu = Emulator::new();
    // 99 is not a valid opcode; the word decodes to raw_op = 99.
    emu.load_program(vec![99 * P18]);
    emu.run();
    assert!(emu.halted);
    assert!(
        emu.output.iter().any(|l| l.contains("TRAP: unknown opcode")),
        "invalid opcodes must trap, got: {:?}", emu.output
    );
}

#[test]
fn test_t3_binary_magic_header() {
    let words = vec![
        encode_wide(Opcode::Tlit, 1, 5),
        encode(Opcode::Halt, 0, 0, 0, 0),
    ];
    let path = "/tmp/test_t3_magic.bin";
    write_t3_binary(&words, path).unwrap();
    // The raw file must start with the magic word...
    let raw = std::fs::read(path).unwrap();
    assert_eq!(&raw[0..8], &crate::codegen_t3::assembler::T3B_MAGIC.to_le_bytes());
    // ...and read_t3_binary_with_magic strips it and reports it.
    let (loaded, has_magic) = crate::codegen_t3::assembler::read_t3_binary_with_magic(path).unwrap();
    assert!(has_magic);
    assert_eq!(loaded, words);
    // Legacy magic-less binaries are still readable (backward compat).
    use std::io::Write;
    let legacy_path = "/tmp/test_t3_legacy.bin";
    let mut f = std::fs::File::create(legacy_path).unwrap();
    for w in &words { f.write_all(&w.to_le_bytes()).unwrap(); }
    drop(f);
    let (legacy, legacy_magic) = crate::codegen_t3::assembler::read_t3_binary_with_magic(legacy_path).unwrap();
    assert!(!legacy_magic);
    assert_eq!(legacy, words);
}

// ---------------------------------------------------------------------------
// call_fn_ptr depth balance (B12)
// ---------------------------------------------------------------------------

#[test]
fn test_call_fn_ptr_balances_call_depth() {
    // fn at addr 3: R1 = R1 + 1; RET.  Vec::map over [10, 20] calls it twice.
    let asm = r#"
JUMP setup
fnadd:
TADD R1, R1, #1
RET
setup:
HALT
"#;
    let (words, _, _) = assemble(asm).unwrap();
    let mut emu = Emulator::new();
    emu.load_program(words);
    // Build a Vec [10, 20]
    emu.do_syscall(17);
    let handle = emu.regs[1];
    for v in [10, 20] {
        emu.regs[1] = handle;
        emu.regs[2] = v;
        emu.do_syscall(18);
    }
    let depth_before = emu.call_depth;
    emu.regs[1] = handle;
    emu.regs[2] = 1; // fnadd label address (after the JUMP word)
    emu.do_syscall(84); // Vec::map
    assert_eq!(emu.call_depth, depth_before,
        "call_fn_ptr must leave call_depth balanced (Ret decrements it)");
    let mapped = emu.regs[1] as usize;
    if let Some(HeapObj::Vec(v)) = emu.heap_objs.get(&mapped) {
        assert_eq!(v, &vec![11, 21]);
    } else {
        panic!("Vec::map did not produce a Vec");
    }
}

// ---------------------------------------------------------------------------
// Profiler covers Loadt/Storet (B18)
// ---------------------------------------------------------------------------

#[test]
fn test_profiler_counts_loadt_storet() {
    let mut emu = Emulator::new();
    emu.load_program(vec![
        encode_wide(Opcode::Tlit, 1, 1),
        encode_wide(Opcode::Tlit, 2, 5000),
        encode(Opcode::Storet, 1, 2, 0, 0),
        encode(Opcode::Loadt, 3, 2, 0, 0),
        encode(Opcode::Halt, 0, 0, 0, 0),
    ]);
    emu.run();
    assert_eq!(emu.regs[3], 1);
    assert_eq!(emu.profile.opcode_counts[Opcode::Loadt as usize], 1);
    assert_eq!(emu.profile.opcode_counts[Opcode::Storet as usize], 1);
    assert_eq!(emu.profile.memory_ops, 2);
    let summary = emu.profile.summary();
    assert!(summary.contains("LOADT"));
    assert!(summary.contains("STORET"));
}

// ---------------------------------------------------------------------------
// env::exit halts with code instead of killing the host process
// ---------------------------------------------------------------------------

#[test]
fn test_env_exit_halts_with_code() {
    let mut emu = Emulator::new();
    emu.regs[1] = 3;
    emu.do_syscall(550);
    assert!(emu.halted);
    assert_eq!(emu.regs[1], 3);
}

// ---------------------------------------------------------------------------
// Assembler .float/.data sections with '::' labels (K4)
// ---------------------------------------------------------------------------

#[test]
fn test_float_section_labels_with_path_separator() {
    // Monomorphized labels like float_Point::zero_0 must resolve (K4).
    let asm = r#"
main:
TLIT R1, #float_Point::zero_0
HALT
.float:
    float_Point::zero_0: .float64 4614256656552045848
"#;
    let (words, _strings, floats) = assemble(asm).expect("assemble failed");
    assert_eq!(words.len(), 2);
    assert_eq!(floats.len(), 1);
    let (&addr, &bits) = floats.iter().next().unwrap();
    assert_eq!(bits, 4614256656552045848);
    // The TLIT must load the float label's address.
    let mut emu = Emulator::new();
    emu.load_program(words);
    emu.run();
    assert_eq!(emu.regs[1] as usize, addr);
}
