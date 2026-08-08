// isa.rs — T3ISA instruction set architecture definitions
//
// Architecture summary:
//   27-trit word, values −3812798742493 .. +3812798742493, stored as i64
//   27 registers: R0 (zero), R1–R25 (general), R26 (SP)
//   FLAGS: i8 ∈ {−1, 0, +1}

// ---------------------------------------------------------------------------
// Word-size constants
// ---------------------------------------------------------------------------

pub const T3_MAX: i64 = 3_812_798_742_493; // (3^27 - 1) / 2
pub const T3_MIN: i64 = -3_812_798_742_493;

#[inline]
pub fn clamp27(v: i64) -> i64 {
    v.clamp(T3_MIN, T3_MAX)
}

#[inline]
pub fn sign_i64(v: i64) -> i8 {
    if v > 0 { 1 } else if v < 0 { -1 } else { 0 }
}

// ---------------------------------------------------------------------------
// Opcode table
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i64)]
pub enum Opcode {
    Nop     = 0,
    Tadd    = 1,
    Tsub    = 2,
    Tmul    = 3,
    Tdiv    = 4,
    Tmod    = 5,
    Tneg    = 6,
    Tand    = 7,
    Tor     = 8,
    Tnot    = 9,
    Tshi    = 10,
    Tshr    = 11,
    Tmin    = 12,
    Tmax    = 13,
    Tcmp    = 14,
    Load    = 15,
    Store   = 16,
    Tlit    = 17,
    Mov     = 18,
    Tbranch = 19,
    Jump    = 20,
    Call    = 21,
    Ret     = 22,
    Halt    = 23,
    Syscall = 24,
    TbrPos  = 25,   // jump if reg > 0 (single-address branch)
    TbrZero = 26,   // jump if reg == 0 (single-address branch)
    TbrNeg  = 27,   // jump if reg < 0 (single-address branch)
    Callr   = 28,   // call through register: CALLR Rx
    Band    = 29,   // binary (bitwise) AND
    Bor     = 30,   // binary (bitwise) OR
    Bxor    = 31,   // binary (bitwise) XOR
    Bshl    = 32,   // binary left shift
    Bshr    = 33,   // binary right shift (arithmetic)
    Loadt   = 34,   // load single trit: Rd = clamp(mem[Ra+imm], -1, 1)
    Storet  = 35,   // store single trit: mem[Ra+imm] = clamp(Rs, -1, 1)
}

impl Opcode {
    pub fn from_i64(v: i64) -> Option<Self> {
        match v {
            0  => Some(Opcode::Nop),
            1  => Some(Opcode::Tadd),
            2  => Some(Opcode::Tsub),
            3  => Some(Opcode::Tmul),
            4  => Some(Opcode::Tdiv),
            5  => Some(Opcode::Tmod),
            6  => Some(Opcode::Tneg),
            7  => Some(Opcode::Tand),
            8  => Some(Opcode::Tor),
            9  => Some(Opcode::Tnot),
            10 => Some(Opcode::Tshi),
            11 => Some(Opcode::Tshr),
            12 => Some(Opcode::Tmin),
            13 => Some(Opcode::Tmax),
            14 => Some(Opcode::Tcmp),
            15 => Some(Opcode::Load),
            16 => Some(Opcode::Store),
            17 => Some(Opcode::Tlit),
            18 => Some(Opcode::Mov),
            19 => Some(Opcode::Tbranch),
            20 => Some(Opcode::Jump),
            21 => Some(Opcode::Call),
            22 => Some(Opcode::Ret),
            23 => Some(Opcode::Halt),
            24 => Some(Opcode::Syscall),
            25 => Some(Opcode::TbrPos),
            26 => Some(Opcode::TbrZero),
            27 => Some(Opcode::TbrNeg),
            28 => Some(Opcode::Callr),
            29 => Some(Opcode::Band),
            30 => Some(Opcode::Bor),
            31 => Some(Opcode::Bxor),
            32 => Some(Opcode::Bshl),
            33 => Some(Opcode::Bshr),
            34 => Some(Opcode::Loadt),
            35 => Some(Opcode::Storet),
            _  => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Encoding
//
// Word layout (27-trit, stored as i64):
//   [26..18]  opcode  — 9 trits  — base 3^18 = 387_420_489
//   [17..13]  r1      — 5 trits  — base 3^13 = 1_594_323
//   [12..8]   r2      — 5 trits  — base 3^8  = 6_561
//   [7..3]    r3      — 5 trits  — base 3^3  = 27
//   [2..0]    imm     — 3 trits  — base 1
//
// TLIT / JUMP / CALL use a "wide immediate": lower P13 trits carry the value.
// TBRANCH is a pseudo-instruction: the assembler expands it to three machine
// instructions (TBR_POS Rcond addr+, TBR_ZERO Rcond addr0, JUMP addr-).
// ---------------------------------------------------------------------------

pub const P18: i64 = 387_420_489; // 3^18
pub const P13: i64 = 1_594_323;   // 3^13
pub const P8:  i64 = 6_561;       // 3^8
pub const P3:  i64 = 27;           // 3^3

pub fn encode(opcode: Opcode, r1: i64, r2: i64, r3: i64, imm: i64) -> i64 {
    (opcode as i64) * P18 + r1 * P13 + r2 * P8 + r3 * P3 + imm
}

/// Encode an instruction with a wide immediate occupying the low P13 trits
/// (after the opcode field).  r1 sits in the top 5 trits of that field.
/// Uses rem_euclid so negative wide_imm values don't corrupt the r1 field.
pub fn encode_wide(opcode: Opcode, r1: i64, wide_imm: i64) -> i64 {
    let adj = wide_imm.rem_euclid(P13);
    (opcode as i64) * P18 + r1 * P13 + adj
}

pub fn decode(word: i64) -> (i64, i64, i64, i64, i64) {
    let op   = word / P18;
    let rest = word - op * P18;
    let r1   = rest / P13;
    let rest = rest - r1 * P13;
    let r2   = rest / P8;
    let rest = rest - r2 * P8;
    let r3   = rest / P3;
    let imm  = rest - r3 * P3;
    (op, r1, r2, r3, imm)
}

pub fn decode_tlit_imm(word: i64) -> i64 {
    let op   = word / P18;
    let rest = word - op * P18;
    let r1   = rest / P13;
    let raw  = rest - r1 * P13; // raw ∈ [0, P13)
    // Interpret as balanced/signed: values > P13/2 represent negatives
    if raw > P13 / 2 { raw - P13 } else { raw }
}
