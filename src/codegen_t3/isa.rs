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

    // ---- T3ISA v1.5: lane-wise ternary logic (C2) ------------------------
    //
    // TAND/TOR/TNOT are NUMERIC min/max/negate on a whole 27-trit word. That
    // is a useful operation and it is not three-valued logic across the word:
    // it compares two 27-trit NUMBERS. These treat the same word as 27
    // independent trit lanes, which is what a 27-trit register actually holds
    // when it holds data rather than a magnitude.
    //
    // The distinction is the point of the language. A binary word gives 64
    // lanes of one-valued data — a bit is not a datum, it is half of one. A
    // ternary word gives 27 lanes of genuinely three-valued data, and every
    // lane-wise instruction below replaces 27 extract-operate-insert cycles
    // (each a division and a multiply by a power of three) with one.
    Tandw   = 36,   // lane-wise min
    Torw    = 37,   // lane-wise max
    Txorw   = 38,   // lane-wise sum mod 3, balanced
    Timpw   = 39,   // lane-wise Lukasiewicz implication
    Tcmpw   = 40,   // lane-wise sign compare: sign(a_i - b_i) per lane
    Tpopc   = 41,   // count lanes of Ra equal to the trit in Rb (or imm)
    Tselw   = 42,   // per-lane select: s_i > 0 -> a_i, s_i < 0 -> b_i, else 0

    // ---- T3ISA v1.6: round-to-nearest division (C4) ----------------------
    //
    // TDIV and TMOD truncate, which is C's rule imported wholesale. On this
    // machine that is doubly wrong: dropping low trits IS rounding to nearest
    // — TSHR has always rounded correctly — so truncation is extra work done
    // to imitate a representation the machine does not use.
    //
    // These two round to nearest with ties away from zero, and they move
    // together: TMODN is defined as `a - TDIVN(a, b) * b`, so
    // `(a / b) * b + (a % b) == a` holds for this pair exactly as it does for
    // TDIV/TMOD. Rounding one and truncating the other would break it.
    //
    // The compiler emits them for the surface `/` and `%` only under
    // `--lang v2`; TDIV and TMOD remain, are still emitted for `math::div_trunc`
    // and `math::rem_trunc`, and are what the compiler's own lowerings use.
    Tdivn   = 43,   // divide, rounding to nearest (ties away from zero)
    Tmodn   = 44,   // the balanced remainder pairing with TDIVN
}

/// One past the highest assigned opcode — the number of distinct opcodes
/// T3ISA v1.6 defines (0..=44).
///
/// Anything indexed by opcode must be sized from this rather than from a
/// literal. The profiler's histogram was a hard-coded `[usize; 36]` when the
/// lane-wise group landed at 36–42, so every `TANDW` executed was counted in
/// the totals and then dropped from the per-opcode breakdown — the instrument
/// could not see the instructions the architecture had just added.
pub const T3_OPCODE_COUNT: usize = 45;

// ---------------------------------------------------------------------------
// Lane decomposition (T3ISA v1.5)
// ---------------------------------------------------------------------------

/// Number of trit lanes in a word.
pub const T3_LANES: usize = 27;

/// Split a word into its 27 balanced-ternary trits, least significant first.
///
/// This is the operation every lane-wise instruction is defined in terms of,
/// and it is exact for the whole word range: the balanced representation of a
/// value in [T3_MIN, T3_MAX] needs at most 27 trits and has no sign bit to
/// special-case, which is why there is no asymmetric-minimum wart here.
#[inline]
pub fn trits27(v: i64) -> [i8; T3_LANES] {
    let mut out = [0i8; T3_LANES];
    let mut n = clamp27(v);
    for lane in out.iter_mut() {
        // rem_euclid then re-centre: 2 becomes -1 with a carry, which is what
        // makes the representation balanced rather than merely base-3.
        //
        // `div_euclid`, NOT `/`. Rust's `/` truncates toward zero, which
        // disagrees with `rem_euclid` for negative operands and silently
        // produced the wrong digits for every negative word: -8 decomposed to
        // +4. The two must be the same division or the digit and the carry
        // describe different quotients.
        let mut r = n.rem_euclid(3);
        n = n.div_euclid(3);
        if r == 2 {
            r = -1;
            n += 1;
        }
        *lane = r as i8;
    }
    out
}

/// Reassemble 27 trits, least significant first, into a word.
#[inline]
pub fn from_trits27(lanes: &[i8; T3_LANES]) -> i64 {
    let mut v: i64 = 0;
    for &t in lanes.iter().rev() {
        v = v * 3 + t as i64;
    }
    clamp27(v)
}

/// Apply a binary function to every lane pair.
#[inline]
pub fn lanewise2(a: i64, b: i64, f: impl Fn(i8, i8) -> i8) -> i64 {
    let (la, lb) = (trits27(a), trits27(b));
    let mut out = [0i8; T3_LANES];
    for i in 0..T3_LANES {
        out[i] = f(la[i], lb[i]);
    }
    from_trits27(&out)
}

/// Lukasiewicz implication on one trit: `min(+1, 1 - a + b)`.
///
/// The same connective the language's `timp` computes, per lane. See
/// `BinOpKind::Timp` for why the a = b = 0 cell is the one that matters.
#[inline]
pub fn trit_imp(a: i8, b: i8) -> i8 {
    (1 - a + b).min(1)
}

/// Balanced sum mod 3 on one trit — the lane-wise `txor`.
///
/// Not an involution: `x txor k txor k` is not `x`, because 3k = 0 (mod 3)
/// needs THREE applications. The language reference already documents this for
/// the word-level operator and it is inherited unchanged here.
#[inline]
pub fn trit_xor(a: i8, b: i8) -> i8 {
    let s = (a as i32 + b as i32).rem_euclid(3);
    (if s == 2 { -1 } else { s }) as i8
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
            // T3ISA v1.5 (C2)
            36 => Some(Opcode::Tandw),
            37 => Some(Opcode::Torw),
            38 => Some(Opcode::Txorw),
            39 => Some(Opcode::Timpw),
            40 => Some(Opcode::Tcmpw),
            41 => Some(Opcode::Tpopc),
            42 => Some(Opcode::Tselw),
            // T3ISA v1.6 (C4)
            43 => Some(Opcode::Tdivn),
            44 => Some(Opcode::Tmodn),
            _  => None,
        }
    }

    /// True when the instruction carries a wide immediate in the trits below
    /// the r1 field, so r2 / r3 / imm hold immediate digits rather than
    /// register indices (A5).
    ///
    /// These are the `encode_wide` users: TLIT (value), JUMP / CALL / SYSCALL
    /// (r1 is 0, the whole low field is the address or syscall number) and the
    /// single-address branches (r1 is the condition register, the low field is
    /// the target). TBRANCH is the legacy packed 3-address form, whose fields
    /// below r1 are likewise addresses, not registers.
    pub fn uses_wide_immediate(self) -> bool {
        matches!(
            self,
            Opcode::Tlit | Opcode::Jump | Opcode::Call | Opcode::Syscall
                | Opcode::TbrPos | Opcode::TbrZero | Opcode::TbrNeg
                | Opcode::Tbranch
        )
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

/// Balanced 3-trit immediate field range: [-13, +13].
pub const IMM_MIN: i64 = -13;
pub const IMM_MAX: i64 = 13;
/// Balanced 13-trit wide-immediate range for TLIT: ±(3^13 − 1)/2.
pub const WIDE_IMM_MAX: i64 = (P13 - 1) / 2; // 797_161

/// Encode a standard 5-field instruction word.
///
/// Field ranges are validated at encode time (out-of-range values used to
/// silently corrupt adjacent fields): r1/r2/r3 must be register indices
/// 0..=26, imm must fit the balanced 3-trit field [-13, +13].  Negative
/// immediates are stored as balanced digits (rem_euclid 27) and decoded
/// symmetrically by `decode`.
pub fn encode(opcode: Opcode, r1: i64, r2: i64, r3: i64, imm: i64) -> i64 {
    assert!((0..=26).contains(&r1), "T3ISA internal error: encode {:?}: r1 out of range (0..=26): {}", opcode, r1);
    assert!((0..=26).contains(&r2), "T3ISA internal error: encode {:?}: r2 out of range (0..=26): {}", opcode, r2);
    assert!((0..=26).contains(&r3), "T3ISA internal error: encode {:?}: r3 out of range (0..=26): {}", opcode, r3);
    assert!((IMM_MIN..=IMM_MAX).contains(&imm),
        "T3ISA internal error: encode {:?}: imm out of range ({}..={}): {}", opcode, IMM_MIN, IMM_MAX, imm);
    (opcode as i64) * P18 + r1 * P13 + r2 * P8 + r3 * P3 + imm.rem_euclid(P3)
}

/// Encode an instruction with a wide immediate occupying the low P13 trits
/// (after the opcode field).  r1 sits in the top 5 trits of that field.
/// Uses rem_euclid so negative wide_imm values don't corrupt the r1 field.
/// Accepted range: balanced [-WIDE_IMM_MAX, +WIDE_IMM_MAX] for signed users
/// (TLIT) or unsigned [0, P13) for address users (JUMP/CALL/TBR_*/SYSCALL);
/// the union is validated here, per-mnemonic bounds live in the assembler.
pub fn encode_wide(opcode: Opcode, r1: i64, wide_imm: i64) -> i64 {
    assert!((0..=26).contains(&r1), "T3ISA internal error: encode_wide {:?}: r1 out of range (0..=26): {}", opcode, r1);
    assert!(wide_imm >= -WIDE_IMM_MAX && wide_imm < P13,
        "T3ISA internal error: encode_wide {:?}: wide_imm out of range ({}..{}): {}", opcode, -WIDE_IMM_MAX, P13, wide_imm);
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
    let raw  = rest - r3 * P3;
    // The 3-trit imm field is balanced: stored digits 14..=26 represent -13..=-1.
    let imm  = if raw > IMM_MAX { raw - P3 } else { raw };
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
