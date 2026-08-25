// assembler.rs — text assembler for T3ISA
use super::isa::*;
use std::collections::HashMap;

// ---------------------------------------------------------------------------

// Assembler
// ---------------------------------------------------------------------------

/// Find the first `:` in `line` that is NOT part of `::` (path separator).
/// Returns the byte index of that colon, or None.
fn find_label_colon(line: &str) -> Option<usize> {
    let bytes = line.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b == b':' {
            let prev_colon = i > 0 && bytes[i - 1] == b':';
            let next_colon = bytes.get(i + 1).copied() == Some(b':');
            if !prev_colon && !next_colon {
                return Some(i);
            }
        }
    }
    None
}

pub fn assemble(asm_text: &str) -> Result<(Vec<i64>, HashMap<usize, String>, HashMap<usize, i64>), String> {
    let mut raw_instrs: Vec<RawInstr> = Vec::new();
    let mut label_map: HashMap<String, usize> = HashMap::new();
    let mut string_data: HashMap<String, String> = HashMap::new();
    let mut float_data_src: HashMap<String, i64> = HashMap::new();
    // Declaration order of the .data and .float labels. A HashMap has no order
    // of its own, and sorting the label NAMES puts str10 before str2, so the
    // address a literal receives depended on how its label spelled its index.
    // Keep the order the assembly listing declares them in.
    let mut string_order: Vec<String> = Vec::new();
    let mut float_order: Vec<String> = Vec::new();
    let mut in_data = false;
    let mut in_float = false;

    // ---- Pass 1: collect labels, strings, float literals, raw instruction list ----
    for raw_line in asm_text.lines() {
        let line = strip_comment(raw_line).trim().to_string();
        if line.is_empty() { continue; }

        if line == ".float:" || line == ".float" {
            in_float = true;
            in_data = false;
            continue;
        }
        if line == ".data:" || line == ".data" {
            in_data = true;
            in_float = false;
            continue;
        }
        if line == ".globals:" || line == ".globals" {
            in_data = false;
            in_float = false;
            continue;
        }

        if in_float {
            // Use find_label_colon so labels containing '::' (e.g. monomorphized
            // float literals like float_Point::zero_0) parse correctly.
            if let Some(cp) = find_label_colon(&line) {
                let lbl = line[..cp].trim().to_string();
                let rest = line[cp+1..].trim();
                if let Some(s) = rest.strip_prefix(".float64") {
                    if let Ok(bits) = s.trim().parse::<i64>() {
                        if float_data_src.insert(lbl.clone(), bits).is_none() {
                            float_order.push(lbl);
                        }
                    }
                }
            }
            continue;
        }

        if in_data {
            // Expect:  label: .string "content"
            // find_label_colon skips ':' inside '::' path separators.
            if let Some(cp) = find_label_colon(&line) {
                let lbl  = line[..cp].trim().to_string();
                let rest = line[cp+1..].trim();
                if let Some(s) = rest.strip_prefix(".string") {
                    let content = parse_string_literal(s.trim());
                    if string_data.insert(lbl.clone(), content).is_none() {
                        string_order.push(lbl);
                    }
                }
            }
            continue;
        }

        // "  label:" style (label on its own line)
        if line.ends_with(':') && !line.contains(|c: char| c.is_whitespace()) {
            let lbl = line.trim_end_matches(':').to_string();
            label_map.insert(lbl, raw_instrs.len());
            continue;
        }

        // "label:  INSTR ..." or "label:" with trailing content
        // Use find_label_colon to skip ':' that is part of '::' (path separator).
        if let Some(cp) = find_label_colon(&line) {
            let maybe_lbl = line[..cp].trim();
            // A label candidate has no whitespace and only identifier/path chars.
            let is_label = !maybe_lbl.is_empty()
                && maybe_lbl.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.' || c == ':');
            if is_label {
                label_map.insert(maybe_lbl.to_string(), raw_instrs.len());
                let after = line[cp+1..].trim();
                if !after.is_empty() {
                    let raw = parse_raw_instr(after)?;
                    // Labeled instructions must reserve TBRANCH placeholder words
                    // exactly like the bare-instruction path below, or every later
                    // label address shifts by 2.
                    let is_tbranch = raw.mnemonic == "TBRANCH";
                    raw_instrs.push(raw);
                    if is_tbranch {
                        raw_instrs.push(RawInstr { mnemonic: "__TBRANCH_W1".to_string(), operands: vec![] });
                        raw_instrs.push(RawInstr { mnemonic: "__TBRANCH_W2".to_string(), operands: vec![] });
                    }
                }
                continue;
            }
        }

        // Regular instruction line
        let raw = parse_raw_instr(&line)?;
        // TBRANCH expands to 3 words — add 2 placeholders so label offsets are correct
        let is_tbranch = raw.mnemonic == "TBRANCH";
        raw_instrs.push(raw);
        if is_tbranch {
            raw_instrs.push(RawInstr { mnemonic: "__TBRANCH_W1".to_string(), operands: vec![] });
            raw_instrs.push(RawInstr { mnemonic: "__TBRANCH_W2".to_string(), operands: vec![] });
        }
    }

    // Assign string labels to addresses past the code
    let code_size = raw_instrs.len();
    let str_base = code_size + 1024;
    let str_keys: Vec<String> = string_order;
    for (i, key) in str_keys.iter().enumerate() {
        label_map.insert(key.clone(), str_base + i);
    }

    // Build resolved string data: address → content (using same base as label_map)
    let mut resolved_strings: HashMap<usize, String> = HashMap::new();
    for (i, key) in str_keys.iter().enumerate() {
        if let Some(content) = string_data.get(key) {
            resolved_strings.insert(str_base + i, content.clone());
        }
    }

    // Assign float labels to addresses past the string addresses
    let float_base = str_base + str_keys.len();
    let float_keys: Vec<String> = float_order;
    for (i, key) in float_keys.iter().enumerate() {
        label_map.insert(key.clone(), float_base + i);
    }
    let float_data_out: HashMap<usize, i64> = float_keys.iter().enumerate()
        .map(|(i, k)| (float_base + i, float_data_src[k]))
        .collect();

    // **The static image must end below the stack, and nothing checked it
    // until report.txt P38.**
    //
    // The memory map (emulator/mod.rs) puts code at 0 growing UP, then string
    // literals at `code_size + 1024`, then float literals — while the stack
    // starts at `STACK_BASE` and grows DOWN. A program whose image reaches
    // 60,000 words therefore overlaps its own stack: the first `CALL` writes a
    // return address over an instruction, and execution eventually reads a
    // stack word as code. The emulator reports that as
    // `TRAP: unknown opcode <n> at PC=<n>`, which names the SYMPTOM — a word
    // that is not an instruction — and gives no hint that the cause is size.
    //
    // Measured before this check: a program of 59,991 words ran correctly and
    // one of 60,004 trapped, with no diagnostic anywhere in between. It is not
    // an inliner defect, though inlining is what first pushed real programs
    // over the line — 14 of the 1,147-file corpus, all silently.
    //
    // The bound is the hard overlap rather than a headroom estimate: how much
    // stack a program needs is dynamic, so `>= STACK_BASE` is the one line
    // that is certainly wrong for every program rather than arguably wrong for
    // some.
    let image_top = float_base + float_keys.len();
    if image_top >= super::emulator::STACK_BASE {
        return Err(format!(
            "program image is {} words and does not fit below the stack at {} \
             ({} words of code, {} string literals, {} float literals). The \
             stack grows down from {} and would overwrite the image.",
            image_top,
            super::emulator::STACK_BASE,
            code_size,
            str_keys.len(),
            float_keys.len(),
            super::emulator::STACK_BASE,
        ));
    }

    // ---- Pass 2: encode ----
    let mut words = Vec::with_capacity(raw_instrs.len());
    let mut i = 0;
    while i < raw_instrs.len() {
        let raw = &raw_instrs[i];
        if raw.mnemonic == "TBRANCH" {
            // Expand to 3 words: TBR_POS rcond Lpos, TBR_ZERO rcond Lzero, JUMP Lneg
            let ops = &raw.operands;
            let rcond = parse_reg(ops.get(0).map(|s| s.as_str()).unwrap_or("R0"))
                .map_err(|e| e)? as i64;
            let addr_pos  = resolve_label(ops.get(1).map(|s| s.as_str()).unwrap_or("0"), &label_map)?;
            let addr_zero = resolve_label(ops.get(2).map(|s| s.as_str()).unwrap_or("0"), &label_map)?;
            let addr_neg  = resolve_label(ops.get(3).map(|s| s.as_str()).unwrap_or("0"), &label_map)?;
            words.push(encode_wide(Opcode::TbrPos,  rcond, addr_pos));
            words.push(encode_wide(Opcode::TbrZero, rcond, addr_zero));
            words.push(encode_wide(Opcode::Jump,    0,     addr_neg));
            i += 3; // skip the 2 placeholder entries too
        } else if raw.mnemonic.starts_with("__TBRANCH_W") {
            // placeholder — already emitted above, skip
            i += 1;
        } else {
            words.push(encode_raw(raw, i, &label_map)?);
            i += 1;
        }
    }

    Ok((words, resolved_strings, float_data_out))
}

#[derive(Debug, Clone)]
struct RawInstr {
    mnemonic: String,
    operands: Vec<String>,
}

fn strip_comment(line: &str) -> &str {
    // Skip ';' characters that are inside double-quoted strings.
    let bytes = line.as_bytes();
    let mut in_string = false;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => in_string = !in_string,
            b'\\' if in_string => i += 1, // skip escaped char
            b';' if !in_string => return &line[..i],
            _ => {}
        }
        i += 1;
    }
    line
}

fn parse_string_literal(s: &str) -> String {
    let s = s.trim();
    let inner = if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
        &s[1..s.len()-1]
    } else {
        s
    };
    let mut out = String::new();
    let mut it = inner.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.next() {
                Some('n')  => out.push('\n'),
                Some('t')  => out.push('\t'),
                Some('r')  => out.push('\r'),
                Some('\\') => out.push('\\'),
                Some('"')  => out.push('"'),
                Some(x)    => { out.push('\\'); out.push(x); }
                None       => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn parse_raw_instr(line: &str) -> Result<RawInstr, String> {
    let line = line.trim();
    let (mnemonic, rest) = if let Some(sp) = line.find(|c: char| c.is_whitespace()) {
        (line[..sp].to_uppercase(), line[sp..].trim().to_string())
    } else {
        (line.to_uppercase(), String::new())
    };
    let operands = if rest.is_empty() { Vec::new() } else { split_operands(&rest) };
    Ok(RawInstr { mnemonic, operands })
}

fn split_operands(s: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut cur   = String::new();
    let mut depth = 0usize;
    for c in s.chars() {
        match c {
            '[' => { depth += 1; cur.push(c); }
            ']' => { if depth > 0 { depth -= 1; } cur.push(c); }
            ',' if depth == 0 => { parts.push(cur.trim().to_string()); cur.clear(); }
            _   => { cur.push(c); }
        }
    }
    if !cur.trim().is_empty() { parts.push(cur.trim().to_string()); }
    parts
}

fn parse_reg(s: &str) -> Result<usize, String> {
    let s = s.trim();
    let upper = s.to_uppercase();
    if upper.starts_with('R') {
        let n = upper[1..].parse::<usize>().map_err(|_| format!("Bad register: {}", s))?;
        if n > 26 {
            return Err(format!("Register out of range (R0-R26): {}", s));
        }
        Ok(n)
    } else {
        Err(format!("Expected register, got: {}", s))
    }
}

fn parse_imm(s: &str, label_map: &HashMap<String, usize>) -> Result<i64, String> {
    let s = s.trim().trim_start_matches('#');
    if let Ok(n) = s.parse::<i64>() { return Ok(n); }
    if let Some(&a) = label_map.get(s) { return Ok(a as i64); }
    // Case-insensitive lookup
    for (k, v) in label_map {
        if k.eq_ignore_ascii_case(s) { return Ok(*v as i64); }
    }
    Err(format!("Cannot resolve: {}", s))
}

fn parse_mem(s: &str) -> Result<(usize, i64), String> {
    let s = s.trim().trim_start_matches('[').trim_end_matches(']');
    // Find last '+' or '-' that separates base from offset.
    // Allow negative offsets: [R2+#-5] or [R2-#5]
    if let Some(pos) = s.rfind('+') {
        let base = parse_reg(s[..pos].trim())?;
        let off  = s[pos+1..].trim().trim_start_matches('#').parse::<i64>().unwrap_or(0);
        return Ok((base, off));
    }
    if let Some(pos) = s.rfind('-') {
        // Make sure it's not part of the register name
        if pos > 0 {
            let base = parse_reg(s[..pos].trim())?;
            let off  = -(s[pos+1..].trim().trim_start_matches('#').parse::<i64>().unwrap_or(0));
            return Ok((base, off));
        }
    }
    let base = parse_reg(s)?;
    Ok((base, 0))
}

fn resolve_label(s: &str, label_map: &HashMap<String, usize>) -> Result<i64, String> {
    let s = s.trim();
    if let Ok(n) = s.trim_start_matches('#').parse::<i64>() { return Ok(n); }
    if let Some(&a) = label_map.get(s) { return Ok(a as i64); }
    for (k, v) in label_map {
        if k.eq_ignore_ascii_case(s) { return Ok(*v as i64); }
    }
    Err(format!("Undefined label: {}", s))
}

fn encode_raw(raw: &RawInstr, pc: usize, label_map: &HashMap<String, usize>) -> Result<i64, String> {
    let ops = &raw.operands;
    let n = ops.len();

    macro_rules! reg {
        ($i:expr) => {{
            if $i >= n { return Err(format!("Too few operands for {} at pc={}", raw.mnemonic, pc)); }
            parse_reg(&ops[$i])? as i64
        }};
    }
    // reg_or_zero!: if operand is an immediate #n, treat as register 0 (always-zero)
    // and put n in the imm field instead
    macro_rules! reg_or_imm_pair {
        ($ri:expr, $ii:expr) => {{
            if $ri >= n {
                return Err(format!("Too few operands for {} at pc={}", raw.mnemonic, pc));
            }
            let s = ops[$ri].trim();
            if s.starts_with('#') || s.chars().next().map_or(false, |c| c.is_ascii_digit() || c == '-') {
                let imm_val = parse_imm(s, label_map)?;
                if !(IMM_MIN..=IMM_MAX).contains(&imm_val) {
                    return Err(format!(
                        "Immediate out of range for {} at pc={}: {} (3-trit field holds {}..={}; use TLIT + register form)",
                        raw.mnemonic, pc, imm_val, IMM_MIN, IMM_MAX));
                }
                (0i64, imm_val)  // register 0, immediate = n
            } else {
                (parse_reg(s)? as i64, 0i64)
            }
        }};
    }
    // mem offset validation: the balanced 3-trit imm field holds [-13, +13]
    macro_rules! check_off {
        ($off:expr) => {{
            let off: i64 = $off;
            if !(IMM_MIN..=IMM_MAX).contains(&off) {
                return Err(format!(
                    "Memory offset out of range for {} at pc={}: {} (3-trit field holds {}..={}; compute the address in a register)",
                    raw.mnemonic, pc, off, IMM_MIN, IMM_MAX));
            }
            off
        }};
    }
    macro_rules! imm {
        ($i:expr) => {{
            if $i >= n { return Err(format!("Too few operands for {} at pc={}", raw.mnemonic, pc)); }
            parse_imm(&ops[$i], label_map)?
        }};
    }
    macro_rules! lbl {
        ($i:expr) => {{
            if $i >= n { return Err(format!("Too few operands for {} at pc={}", raw.mnemonic, pc)); }
            resolve_label(&ops[$i], label_map)?
        }};
    }
    macro_rules! mem {
        ($i:expr) => {{
            if $i >= n { return Err(format!("Too few operands for {} at pc={}", raw.mnemonic, pc)); }
            parse_mem(&ops[$i])?
        }};
    }

    let word = match raw.mnemonic.as_str() {
        "NOP"  => encode(Opcode::Nop,  0, 0, 0, 0),
        "HALT" => encode(Opcode::Halt, 0, 0, 0, 0),
        "RET"  => encode(Opcode::Ret,  0, 0, 0, 0),

        "TADD" => { let (r2, imv) = reg_or_imm_pair!(2, 2); encode(Opcode::Tadd, reg!(0), reg!(1), r2, imv) }
        "TSUB" => { let (r2, imv) = reg_or_imm_pair!(2, 2); encode(Opcode::Tsub, reg!(0), reg!(1), r2, imv) }
        "TMUL" => { let (r2, imv) = reg_or_imm_pair!(2, 2); encode(Opcode::Tmul, reg!(0), reg!(1), r2, imv) }
        "TDIV" => { let (r2, imv) = reg_or_imm_pair!(2, 2); encode(Opcode::Tdiv, reg!(0), reg!(1), r2, imv) }
        // T3ISA v1.6 (C4): the rounding pair. Same operand shape as TDIV/TMOD.
        "TDIVN" => { let (r2, imv) = reg_or_imm_pair!(2, 2); encode(Opcode::Tdivn, reg!(0), reg!(1), r2, imv) }
        "TMODN" => { let (r2, imv) = reg_or_imm_pair!(2, 2); encode(Opcode::Tmodn, reg!(0), reg!(1), r2, imv) }
        "TMOD" => { let (r2, imv) = reg_or_imm_pair!(2, 2); encode(Opcode::Tmod, reg!(0), reg!(1), r2, imv) }
        "TAND" => { let (r2, imv) = reg_or_imm_pair!(2, 2); encode(Opcode::Tand, reg!(0), reg!(1), r2, imv) }
        "TOR"  => { let (r2, imv) = reg_or_imm_pair!(2, 2); encode(Opcode::Tor,  reg!(0), reg!(1), r2, imv) }
        "TMIN" => { let (r2, imv) = reg_or_imm_pair!(2, 2); encode(Opcode::Tmin, reg!(0), reg!(1), r2, imv) }
        "TMAX" => { let (r2, imv) = reg_or_imm_pair!(2, 2); encode(Opcode::Tmax, reg!(0), reg!(1), r2, imv) }
        "TCMP" => { let (r2, imv) = reg_or_imm_pair!(2, 2); encode(Opcode::Tcmp, reg!(0), reg!(1), r2, imv) }

        // T3ISA v1.5: lane-wise ternary logic (C2). Same three-operand shape
        // as their word-level counterparts above, so nothing about the
        // assembler's operand grammar changes.
        "TANDW" => { let (r2, imv) = reg_or_imm_pair!(2, 2); encode(Opcode::Tandw, reg!(0), reg!(1), r2, imv) }
        "TORW"  => { let (r2, imv) = reg_or_imm_pair!(2, 2); encode(Opcode::Torw,  reg!(0), reg!(1), r2, imv) }
        "TXORW" => { let (r2, imv) = reg_or_imm_pair!(2, 2); encode(Opcode::Txorw, reg!(0), reg!(1), r2, imv) }
        "TIMPW" => { let (r2, imv) = reg_or_imm_pair!(2, 2); encode(Opcode::Timpw, reg!(0), reg!(1), r2, imv) }
        "TCMPW" => { let (r2, imv) = reg_or_imm_pair!(2, 2); encode(Opcode::Tcmpw, reg!(0), reg!(1), r2, imv) }
        "TPOPC" => { let (r2, imv) = reg_or_imm_pair!(2, 2); encode(Opcode::Tpopc, reg!(0), reg!(1), r2, imv) }
        // TSELW Rd, Rs, Ra, Rb — the fourth register rides in the 3-trit
        // immediate field, which holds exactly 27 raw values for exactly 27
        // registers. See the emulator's Tselw arm.
        "TSELW" => encode(Opcode::Tselw, reg!(0), reg!(1), reg!(2), (reg!(3) as i64) - (if reg!(3) > 13 { 27 } else { 0 })),

        "TNEG" => encode(Opcode::Tneg, reg!(0), reg!(1), 0, 0),
        "TNOT" => encode(Opcode::Tnot, reg!(0), reg!(1), 0, 0),
        "MOV"  => encode(Opcode::Mov,  reg!(0), reg!(1), 0, 0),

        "TSHI" => {
            // Shift amount: immediate (imm field) or register (r3 field).
            let (r3, shift) = if n >= 3 { reg_or_imm_pair!(2, 2) } else { (0, 0) };
            encode(Opcode::Tshi, reg!(0), reg!(1), r3, shift)
        }
        "TSHR" => {
            let (r3, shift) = if n >= 3 { reg_or_imm_pair!(2, 2) } else { (0, 0) };
            encode(Opcode::Tshr, reg!(0), reg!(1), r3, shift)
        }

        "BAND" => { let (r2, imv) = reg_or_imm_pair!(2, 2); encode(Opcode::Band, reg!(0), reg!(1), r2, imv) }
        "BOR"  => { let (r2, imv) = reg_or_imm_pair!(2, 2); encode(Opcode::Bor,  reg!(0), reg!(1), r2, imv) }
        "BXOR" => { let (r2, imv) = reg_or_imm_pair!(2, 2); encode(Opcode::Bxor, reg!(0), reg!(1), r2, imv) }
        "BSHL" => { let (r2, imv) = reg_or_imm_pair!(2, 2); encode(Opcode::Bshl, reg!(0), reg!(1), r2, imv) }
        "BSHR" => { let (r2, imv) = reg_or_imm_pair!(2, 2); encode(Opcode::Bshr, reg!(0), reg!(1), r2, imv) }

        "TLIT" => {
            let r = reg!(0);
            let imm_val = imm!(1);
            if imm_val.abs() > WIDE_IMM_MAX {
                return Err(format!(
                    "TLIT immediate out of range at pc={}: {} (13-trit wide field holds ±{})",
                    pc, imm_val, WIDE_IMM_MAX));
            }
            encode_wide(Opcode::Tlit, r, imm_val)
        }

        "LOAD" => {
            let r1 = reg!(0);
            let (r2, off) = mem!(1);
            encode(Opcode::Load, r1, r2 as i64, 0, check_off!(off))
        }
        "STORE" => {
            let r1 = reg!(0);
            let (r2, off) = mem!(1);
            encode(Opcode::Store, r1, r2 as i64, 0, check_off!(off))
        }
        "LOADT" => {
            // LOADT Rd, [Ra+imm] — load single trit (clamped -1/0/+1)
            let r1 = reg!(0);
            let (r2, off) = mem!(1);
            encode(Opcode::Loadt, r1, r2 as i64, 0, check_off!(off))
        }
        "STORET" => {
            // STORET Rs, [Ra+imm] — store single trit (clamped -1/0/+1)
            let r1 = reg!(0);
            let (r2, off) = mem!(1);
            encode(Opcode::Storet, r1, r2 as i64, 0, check_off!(off))
        }

        "JUMP" => {
            let addr = lbl!(0);
            if !(0..P13).contains(&addr) {
                return Err(format!("JUMP target out of range at pc={}: {}", pc, addr));
            }
            encode_wide(Opcode::Jump, 0, addr)
        }
        "CALL" => {
            let addr = lbl!(0);
            if !(0..P13).contains(&addr) {
                return Err(format!("CALL target out of range at pc={}: {}", pc, addr));
            }
            encode_wide(Opcode::Call, 0, addr)
        }
        "CALLR" => {
            // CALLR Rx — call through register
            let r1 = reg!(0);
            encode(Opcode::Callr, r1, 0, 0, 0)
        }

        // TBRANCH is handled as a pseudo-instruction in assemble() — expanded to
        // TBR_POS + TBR_ZERO + JUMP (3 words).  It never reaches encode_raw().

        "SYSCALL" => {
            let sc = imm!(0);
            if !(0..P13).contains(&sc) {
                return Err(format!("SYSCALL number out of range at pc={}: {}", pc, sc));
            }
            encode_wide(Opcode::Syscall, 0, sc)
        }

        other => return Err(format!("Unknown mnemonic '{}' at pc={}", other, pc)),
    };

    Ok(word)
}

// ---------------------------------------------------------------------------
// Binary I/O
// ---------------------------------------------------------------------------

/// Magic header word written as the first 8 bytes of every .t3b binary
/// ("T3BMAGIC" little-endian).  It is far outside the valid instruction
/// range, so it can never be confused with a real T3ISA word.
pub const T3B_MAGIC: i64 = i64::from_le_bytes(*b"T3BMAGIC");

pub fn write_t3_binary(words: &[i64], path: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut f = std::fs::File::create(path)?;
    f.write_all(&T3B_MAGIC.to_le_bytes())?;
    for &w in words {
        f.write_all(&w.to_le_bytes())?;
    }
    Ok(())
}

/// Read a .t3b binary, returning the program words and whether the file
/// carried the T3B_MAGIC header (pre-magic binaries are still accepted for
/// backward compatibility; callers can use the flag to reject non-binaries).
pub fn read_t3_binary_with_magic(path: &str) -> std::io::Result<(Vec<i64>, bool)> {
    use std::io::Read;
    let mut f = std::fs::File::open(path)?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf)?;
    let mut words = Vec::new();
    for chunk in buf.chunks_exact(8) {
        let arr: [u8; 8] = chunk.try_into().unwrap();
        words.push(i64::from_le_bytes(arr));
    }
    let has_magic = words.first() == Some(&T3B_MAGIC);
    if has_magic {
        words.remove(0);
    }
    Ok((words, has_magic))
}

pub fn read_t3_binary(path: &str) -> std::io::Result<Vec<i64>> {
    read_t3_binary_with_magic(path).map(|(words, _)| words)
}
