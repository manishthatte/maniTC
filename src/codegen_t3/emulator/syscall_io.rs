// emulator/syscall_io.rs — I/O syscalls: print, read, strings, floats, time, env.
// Syscall ranges: 0-16, 60-69, 127-132, 200-202, 210-220, 540, 550-551.
use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

impl Emulator {
    pub(super) fn do_syscall_io(&mut self, num: i64) {
        match num {
            0 => {
                // print trit
                let t = self.regs[1];
                let s = if t > 0 { "+".to_string() } else if t == 0 { "0".to_string() } else { "-".to_string() };
                self.push_out(s);
            }
            1 => {
                // print int
                self.push_out(self.regs[1].to_string());
            }
            2 => {
                // print float — bitcast i64 → f64
                let bits = self.regs[1] as u64;
                let f = f64::from_bits(bits);
                self.push_out(format!("{}", f));
            }
            3 => {
                // print string at R1 address
                let addr = self.regs[1] as usize;
                let s = if let Some(content) = self.string_data.get(&addr) {
                    content.clone()
                } else {
                    // Try null-terminated read from memory
                    // P82: byte by byte. This is the printing path, and it is
                    // where the divergence showed.
                    let mut buf: Vec<u8> = Vec::new();
                    let mut a = addr;
                    while a < self.memory.len() && self.memory[a] != 0 {
                        buf.push(self.memory[a] as u8);
                        a += 1;
                    }
                    buf
                };
                self.push_out(s);
            }
            4 => {
                self.push_out("\n".to_string());
            }
            5 => {
                // read int
                let v = if let Some(val) = self.input_queue.pop_front() {
                    val
                } else {
                    let mut line = String::new();
                    let _ = std::io::stdin().read_line(&mut line);
                    line.trim().parse::<i64>().unwrap_or(0)
                };
                self.regs[1] = clamp27(v);
            }
            6 => {
                self.halted = true;
            }
            7 => {
                // t27_to_str / print_t27_ternary: R1 = t27 value
                // Formats value as balanced ternary string, stores in heap, returns address in R1.
                let val = self.regs[1];
                let s = Self::t27_to_ternary_str(val);
                let addr = self.heap_alloc_str(s);
                self.regs[1] = addr as i64;
            }
            8 => {
                // trits_to_str: R1 = length-prefixed array ptr (memory[R1] = len, then trits)
                let ptr = self.regs[1] as usize;
                let len = self.memory.get(ptr).copied().unwrap_or(0) as usize;
                let mut s = String::new();
                for i in 0..len {
                    let t = self.memory.get(ptr + 1 + i).copied().unwrap_or(0);
                    s.push(if t > 0 { '+' } else if t == 0 { '0' } else { '-' });
                }
                if s.is_empty() { s = "0".to_string(); }
                let addr = self.heap_alloc_str(s);
                self.regs[1] = addr as i64;
            }
            9 => {
                // trit_count: R1 = n, returns number of balanced ternary digits in R1
                let mut n = self.regs[1];
                if n == 0 { self.regs[1] = 1; return; }
                let mut count = 0i64;
                while n != 0 {
                    let rem = n.rem_euclid(3);
                    let d = if rem <= 1 { rem as i64 } else { -1i64 };
                    n = (n - d) / 3;
                    count += 1;
                }
                self.regs[1] = count;
            }
            10 => {
                // to_balanced_ternary: R1 = int, returns ptr to length-prefixed [trit] array
                // Format: memory[ptr] = len, memory[ptr+1..ptr+1+len] = trits (LST-first)
                let mut n = self.regs[1];
                let mut digits = Vec::new();
                if n == 0 {
                    digits.push(0i64);
                } else {
                    while n != 0 {
                        let rem = n.rem_euclid(3);
                        let d = if rem <= 1 { rem as i64 } else { -1i64 };
                        digits.push(d);
                        n = (n - d) / 3;
                    }
                }
                let mut words = vec![digits.len() as i64];
                words.extend_from_slice(&digits);
                let addr = self.heap_alloc_array(&words);
                self.regs[1] = addr as i64;
            }
            11 => {
                // from_balanced_ternary: R1 = ptr to length-prefixed [trit] array, returns int
                let ptr = self.regs[1] as usize;
                let len = self.memory.get(ptr).copied().unwrap_or(0) as usize;
                let mut result = 0i64;
                let mut base = 1i64;
                for i in 0..len {
                    let t = self.memory.get(ptr + 1 + i).copied().unwrap_or(0);
                    result += t * base;
                    base *= 3;
                }
                self.regs[1] = result;
            }
            12 => {
                // pack_trits: R1 = ptr to length-prefixed [trit] array, returns t27
                let ptr = self.regs[1] as usize;
                let len = self.memory.get(ptr).copied().unwrap_or(0) as usize;
                let mut result = 0i64;
                let mut base = 1i64;
                for i in 0..len {
                    let t = self.memory.get(ptr + 1 + i).copied().unwrap_or(0);
                    result += t * base;
                    base *= 3;
                }
                self.regs[1] = clamp27(result);
            }
            13 => {
                // unpack_trits: R1 = t27 value, returns ptr to raw [trit; 27] array (no length prefix)
                let mut val = self.regs[1];
                let mut trits = [0i64; 27];
                for i in 0..27 {
                    let rem = val.rem_euclid(3);
                    let d = if rem <= 1 { rem as i64 } else { -1i64 };
                    trits[i] = d;
                    val = (val - d) / 3;
                }
                let addr = self.heap_alloc_array(&trits);
                self.regs[1] = addr as i64;
            }
            14 => {
                // fmt::show_int: R1 = int value, returns str ptr in R1
                let n = self.regs[1];
                let s = n.to_string();
                let addr = self.heap_alloc_str(s);
                self.regs[1] = addr as i64;
            }
            15 => {
                // fmt::align_right: R1=str_ptr, R21=width, R22=fill_char → returns new str ptr in R1
                let str_addr = self.regs[1] as usize;
                let width = self.regs[21] as usize;
                let fill = char::from_u32(self.regs[22] as u32).unwrap_or(' ');
                let s = if let Some(content) = self.string_data.get(&str_addr) {
                    content.clone()
                } else {
                    // Same fallback as align_left (132): in-memory lp-string.
                    self.bytes_at(str_addr as i64)
                };
                let padded = if s.len() < width {
                    let mut pad: Vec<u8> = Vec::new();
                    let mut fb = [0u8; 4];
                    let fs = fill.encode_utf8(&mut fb).as_bytes().to_vec();
                    for _ in 0..(width - s.len()) { pad.extend_from_slice(&fs); }
                    pad.extend_from_slice(&s);
                    pad
                } else {
                    s
                };
                let addr = self.heap_alloc_str(padded);
                self.regs[1] = addr as i64;
            }
            16 => {
                // print_bool: R1 = 1 (true) or 0 (false), outputs "true"/"false"
                let s = if self.regs[1] != 0 { "true".to_string() } else { "false".to_string() };
                self.push_out(s);
            }

            // ----------------------------------------------------------------
            // String syscalls (60-69)
            // ----------------------------------------------------------------
            60 => {
                // str_len(ptr=R1) → R1 = length
                let s = self.get_string_r1();
                self.regs[1] = s.len() as i64;
            }
            61 => {
                // str_concat(p1=R1, p2=R2) → R1 = new ptr
                // P82: bytes. `str::to_upper` builds its result by
                // concatenating one-byte strings, so this is on the critical
                // path for the divergence P50 deferred.
                let s1 = self.bytes_r1();
                let addr2 = self.regs[2] as usize;
                let s2 = self.bytes_at(addr2 as i64);
                let combined = [s1, s2].concat();
                let addr = self.heap_alloc_str(combined);
                self.regs[1] = addr as i64;
            }
            62 => {
                // str_slice(ptr=R1, start=R2, end=R3) → R1 = new ptr
                //
                // P50: BYTE indices, and it must not be able to panic.
                //
                // This used to slice the Rust `String` directly, and Rust
                // refuses a slice that splits a character: `s[1..2]` inside
                // 'é' PANICS AND TAKES THE WHOLE EMULATOR PROCESS WITH IT.
                // So an ordinary ManiT program containing a non-ASCII literal
                // crashed the toolchain — not a wrong answer, not a T3 trap,
                // a host panic with a Rust backtrace. `str::reverse` is ManiT
                // source that walks a string one index at a time, so it
                // reached this on the first multi-byte character.
                //
                // Bytes is also what PARITY requires rather than merely what
                // is safe: the C runtime's `str_slice` is `manit_substr` over
                // a `char*`, so LLVM has always sliced bytes.
                //
                // P82 CLOSES WHAT P50 COULD NOT. P50's own note here read: "the
                // emulator holds strings as `String`, so a slice landing inside
                // a character comes back with U+FFFD where LLVM keeps the raw
                // bytes — still a divergence, just no longer a crash. Byte-exact
                // parity means holding `Vec<u8>`, which is a representation
                // change for the whole string surface." That change is now made,
                // so the slice is taken from the bytes and stays bytes.
                //
                // `str::reverse` is what shows it: ManiT source walking a string
                // one index at a time, so on "aéb" it slices INSIDE the 'é' twice
                // and used to hand back two U+FFFD.
                let bytes = self.bytes_r1();
                let end = (self.regs[3] as usize).min(bytes.len());
                let start = (self.regs[2] as usize).min(end);
                let addr = self.heap_alloc_str(bytes[start..end].to_vec());
                self.regs[1] = addr as i64;
            }
            63 => {
                // str_contains(p1=R1, p2=R2) → R1 = bool
                let s1 = self.bytes_r1();
                let addr2 = self.regs[2] as usize;
                let s2 = self.bytes_at(addr2 as i64);
                self.regs[1] = if Self::bytes_find(&s1, &s2).is_some() { 1 } else { 0 };
            }
            64 => {
                // str_find(p1=R1, p2=R2) → R1 = index or -1
                let s1 = self.bytes_r1();
                let addr2 = self.regs[2] as usize;
                let s2 = self.bytes_at(addr2 as i64);
                let idx = Self::bytes_find(&s1, &s2).map(|i| i as i64).unwrap_or(-1);
                self.regs[1] = idx;
            }
            65 => {
                // str_to_int(ptr=R1) → R1 = int
                let s = self.get_string_r1();
                self.regs[1] = s.trim().parse::<i64>().unwrap_or(0);
            }
            66 => {
                // int_to_str(val=R1) → R1 = ptr
                let n = self.regs[1];
                let s = n.to_string();
                let addr = self.heap_alloc_str(s);
                self.regs[1] = addr as i64;
            }
            67 => {
                // str_split(ptr=R1, delim=R2) → R1 = Vec handle of str ptrs
                let s = self.bytes_r1();
                let addr2 = self.regs[2] as usize;
                let delim = self.bytes_at(addr2 as i64);
                let parts: Vec<i64> = Self::bytes_split(&s, &delim).into_iter()
                    .map(|part| {
                        let a = self.heap_ptr;
                        self.heap_alloc_str(part) as i64;
                        a as i64
                    })
                    .collect();
                let vec_handle = self.heap_alloc_obj(HeapObj::Vec(parts));
                self.regs[1] = vec_handle as i64;
            }
            68 => {
                // str_trim(ptr=R1) → R1 = new ptr
                let s = self.get_string_r1();
                let trimmed = s.trim().to_string();
                let addr = self.heap_alloc_str(trimmed);
                self.regs[1] = addr as i64;
            }
            69 => {
                // str_replace(ptr=R1, find=R2, replace=R3) → R1 = new ptr
                let s = self.bytes_r1();
                let addr2 = self.regs[2] as usize;
                let addr3 = self.regs[3] as usize;
                let find_s = self.bytes_at(addr2 as i64);
                let repl_s = self.bytes_at(addr3 as i64);
                let result = Self::bytes_replace(&s, &find_s, &repl_s);
                let addr = self.heap_alloc_str(result);
                self.regs[1] = addr as i64;
            }

            // ----------------------------------------------------------------
            // char primitives (133-134)
            //
            // These are the only two char operations that cannot be written in
            // ManiT, so every other char-dependent str:: function is built on
            // them and shares one body across both backends. Byte-indexed, to
            // match the rest of this block and the C runtime's str_char_at.
            // ----------------------------------------------------------------
            133 => {
                // str_char_at(ptr=R1, i=R2) → R1 = byte value, or 0 if out of range
                let s = self.get_string_r1();
                let i = self.regs[2];
                self.regs[1] = if i < 0 || i >= s.len() as i64 {
                    0
                } else {
                    s.as_bytes()[i as usize] as i64
                };
            }
            134 => {
                // str_from_char(c=R1) → R1 = ptr to a ONE-BYTE string.
                //
                // **P82: THIS IS THE SITE.** It built a `char` and formatted
                // it, so `from_char(0xC3)` produced the TWO-byte UTF-8 encoding
                // of U+00C3 rather than the single byte 0xC3. A char in maniT is
                // an unsigned byte (P72) and the C runtime has always agreed —
                // `str_from_char` there is `out[0] = (char)(c & 0xFF)`. The
                // byte-exact `string_data` this change introduces is what makes
                // storing one byte possible at all: a Rust `String` cannot hold
                // 0xC3 on its own.
                let c = self.regs[1];
                let addr = self.heap_alloc_str(vec![(c & 0xFF) as u8]);
                self.regs[1] = addr as i64;
            }
            136 => {
                // str_char_count(ptr=R1) → R1 = number of Unicode scalar values
                //
                // P48. `str::len` is BYTES — that is what the whole `str`
                // surface is indexed by — so this is the function that makes
                // `byte_len`'s existence mean something instead of being a
                // synonym for `len`. It is deliberately NOT an index: nothing
                // takes a codepoint offset, and offering one would invite a
                // loop that mixes the two.
                let s = self.get_string_r1();
                self.regs[1] = s.chars().count() as i64;
            }
            135 => {
                // ternary_int_to_trits(n=R1, width=R2) → R1 = ptr to a
                // length-prefixed array of exactly `width` trits, LST-first,
                // zero-padded, higher trits discarded.
                //
                // Mirrors @ternary_int_to_trits in codegen_llvm/helpers.rs.
                // val == 0 needs no special case: rem 0 gives digit 0 and val
                // stays 0, which is exactly the zero padding.
                let mut val = self.regs[1];
                let width = self.regs[2].max(0);
                let mut words = Vec::with_capacity(width as usize + 1);
                words.push(width);
                for _ in 0..width {
                    let rem = val.rem_euclid(3);
                    let d = if rem == 2 { -1 } else { rem };
                    words.push(d);
                    val = (val - d) / 3;
                }
                let addr = self.heap_alloc_array(&words);
                self.regs[1] = addr as i64;
            }

            // ----------------------------------------------------------------
            // String comparison (200) and t27 shifts (201-202)
            // ----------------------------------------------------------------
            200 => {
                // str_eq(p1=R1, p2=R2) → R1 = 1 if equal, 0 if not
                let s1 = self.bytes_r1();
                let addr2 = self.regs[2] as usize;
                let s2 = self.bytes_at(addr2 as i64);
                self.regs[1] = if s1 == s2 { 1 } else { 0 };
            }
            201 => {
                // t27_shift_left(R1=n, R2=k) → R1 = n * 3^k
                let n = self.regs[1];
                let k = self.regs[2].clamp(0, 26) as u32;
                self.regs[1] = clamp27(n.saturating_mul(3i64.pow(k)));
            }
            202 => {
                // t27_shift_right(R1=n, R2=k) → R1: drop the k low trits.
                // Round-to-nearest division by 3^k — same semantics as the
                // TSHR instruction (balanced digits make ties impossible).
                let n = self.regs[1];
                let k = self.regs[2].clamp(0, 26) as u32;
                let p = 3i64.pow(k);
                self.regs[1] = (n + (p - 1) / 2).div_euclid(p);
            }
            203 => {
                // __lp_from_flat(R1 = flat array ptr, R2 = len) → R1 = ptr to a
                // length-prefixed copy (mem[p] = len, trits at mem[p+1..=len]).
                //
                // Compiler-internal: emitted only by the IR lowering, when an
                // unsized `[trit]` parameter — which is flat, element i at slot
                // i, length passed separately — reaches a stdlib function that
                // reads length-prefixed (syscalls 8, 11, 12). Without it the
                // first trit was read as the length.
                //
                // T3 memory is one word per slot, so this is a plain copy; the
                // LLVM counterpart @__lp_from_flat has to sign-extend i8 trits.
                let ptr = self.regs[1] as usize;
                let len = self.regs[2].max(0) as usize;
                let mut words = Vec::with_capacity(len + 1);
                words.push(len as i64);
                for i in 0..len {
                    words.push(self.memory.get(ptr + i).copied().unwrap_or(0));
                }
                let addr = self.heap_alloc_array(&words);
                self.regs[1] = addr as i64;
            }

            // ----------------------------------------------------------------
            // Float syscalls (210-220)
            // ----------------------------------------------------------------
            210 => {
                // itof: R1 = int → R1 = f64 bits
                let i = self.regs[1];
                self.regs[1] = (i as f64).to_bits() as i64;
            }
            211 => {
                // ftoi: R1 = f64 bits → R1 = int (truncate toward zero)
                let f = f64::from_bits(self.regs[1] as u64);
                self.regs[1] = f as i64;
            }
            212 => {
                // fadd: R1 + R2 → R1
                let a = f64::from_bits(self.regs[1] as u64);
                let b = f64::from_bits(self.regs[2] as u64);
                self.regs[1] = (a + b).to_bits() as i64;
            }
            213 => {
                // fsub: R1 - R2 → R1
                let a = f64::from_bits(self.regs[1] as u64);
                let b = f64::from_bits(self.regs[2] as u64);
                self.regs[1] = (a - b).to_bits() as i64;
            }
            214 => {
                // fmul: R1 * R2 → R1
                let a = f64::from_bits(self.regs[1] as u64);
                let b = f64::from_bits(self.regs[2] as u64);
                self.regs[1] = (a * b).to_bits() as i64;
            }
            215 => {
                // fdiv: R1 / R2 → R1
                let a = f64::from_bits(self.regs[1] as u64);
                let b = f64::from_bits(self.regs[2] as u64);
                self.regs[1] = (a / b).to_bits() as i64;
            }
            216 => {
                // fcmp: compare R1, R2 → R1 = +1/0/-1 (like TCMP for floats),
                // and R2 = 1 if the comparison is UNORDERED, else 0 (P20).
                //
                // Three values cannot express four outcomes. IEEE-754 says a
                // NaN compares false to everything including itself, but
                // `a > b` and `a < b` are both false for a NaN, so the
                // three-way result collapsed to 0 — "equal" — and T3 reported
                // `nan == nan` as TRUE where LLVM reported false. Every float
                // comparison against a NaN was wrong, in the direction that
                // makes a guard silently pass.
                //
                // The unordered bit rides in R2 rather than a second syscall.
                // R2 is an ABI register, never allocated, and it has already
                // been consumed as the right-hand operand by the time this
                // returns — so the caller can read it without a save.
                let a = f64::from_bits(self.regs[1] as u64);
                let b = f64::from_bits(self.regs[2] as u64);
                let unordered = a.is_nan() || b.is_nan();
                self.regs[1] = if a > b { 1 } else if a < b { -1 } else { 0 };
                self.regs[2] = i64::from(unordered);
            }
            218 => {
                // heap_alloc_words: R1 = word count → R1 = base address.
                //
                // Struct allocations go here rather than on the stack.  A stack
                // slot is scoped to its loop iteration, so a struct pointer that
                // outlives the iteration — `pcbs[i] = age_tick(p)` stores one
                // into an array — aliased the next iteration's allocation and
                // every element ended up reading the same buffer.  The LLVM
                // backend already mallocs struct allocas for the same reason.
                let n = self.regs[1].max(1) as usize;
                // This arm's own copy of the bound check became `heap_reserve`
                // (report.txt P39) so that the four allocators cannot drift:
                // it was the only one that HAD a check, and the other three
                // silently dropped the words that did not fit.
                let base = match self.heap_reserve(n) {
                    Some(b) => b,
                    None => return,
                };
                // Callers read fields before writing them (a partially
                // initialised struct literal), so hand back zeroed memory
                // rather than whatever the previous allocation left behind.
                for i in 0..n {
                    self.memory[base + i] = 0;
                }
                // P41: `heap_reserve` ADVANCES `heap_ptr` as well as checking
                // the bound, so the bump this arm used to do for itself is now
                // its second. Left behind by P39's extraction, it charged 2n
                // words for every n-word struct — the addresses stayed disjoint
                // (which is why only the arithmetic in the unit test could see
                // it) but the heap emptied at half its real capacity, and P39's
                // new bound check turns that into a trap rather than silence.
                self.regs[1] = base as i64;
            }
            219 => {
                // float_load: R1 = address → R1 = float bits at that address
                let addr = self.regs[1] as usize;
                if let Some(&bits) = self.float_data.get(&addr) {
                    self.regs[1] = bits;
                } else {
                    self.regs[1] = 0;
                }
            }
            217 => {
                // print_bool3: R1 = bool3 value → "true"/"false"/"unknown"
                // (same wording as the LLVM backend's __manit_print_bool3).
                let t = self.regs[1];
                let s = if t > 0 { "true" } else if t < 0 { "false" } else { "unknown" };
                self.push_out(s.to_string());
            }
            220 => {
                // fneg: R1 = -R1 (flip IEEE 754 sign bit, bit 63)
                self.regs[1] = self.regs[1] ^ (1i64 << 63);
            }
            221 => {
                // frem: R1 % R2 → R1 (report.txt P19)
                //
                // Rust's `%` on f64 is C's fmod — truncated toward zero, sign
                // of the dividend — which is what LLVM's `frem` computes, so
                // the two backends agree by construction.
                //
                // This existed nowhere until P19. `Rem` was missing from the
                // emitter's float list, so `x % y` on two floats fell through
                // to the INTEGER path and emitted `TMOD` against two IEEE-754
                // bit patterns: `7.5 % 2.0` returned 0 where LLVM returned 1.5.
                let a = f64::from_bits(self.regs[1] as u64);
                let b = f64::from_bits(self.regs[2] as u64);
                self.regs[1] = (a % b).to_bits() as i64;
            }

            // ----------------------------------------------------------------
            // fmt_format / fmt_show_* (127-132)
            // ----------------------------------------------------------------
            127 => {
                // fmt_format(template_ptr=R1, arg0=R2, arg1=R3, ...) → R1 = result str ptr
                let tmpl = self.get_string_r1();
                let placeholder_count = tmpl.matches("{}").count();
                let arg_strs: Vec<String> = (0..placeholder_count).map(|i| {
                    let val = if i + 2 < self.regs.len() { self.regs[i + 2] } else { 0 };
                    let addr = val as usize;
                    if self.string_data.contains_key(&addr) {
                        // A `str` argument. Rendered as TEXT because the
                        // template it lands in is text; P82 keeps the bytes
                        // exact everywhere the value is not being formatted.
                        self.str_at(val)
                    } else {
                        val.to_string()
                    }
                }).collect();
                let mut out = String::new();
                let mut arg_iter = arg_strs.iter();
                let mut chars = tmpl.chars().peekable();
                while let Some(c) = chars.next() {
                    if c == '{' && chars.peek() == Some(&'}') {
                        chars.next();
                        out.push_str(arg_iter.next().map(|s| s.as_str()).unwrap_or(""));
                    } else {
                        out.push(c);
                    }
                }
                let addr = self.heap_alloc_str(out);
                self.regs[1] = addr as i64;
            }
            128 => {
                // fmt_show_int(val=R1) → R1 = str ptr
                let n = self.regs[1];
                let s = n.to_string();
                let addr = self.heap_alloc_str(s);
                self.regs[1] = addr as i64;
            }
            129 => {
                // fmt_show_float(val=R1) → R1 = str ptr (val is f64 bits as i64)
                let bits = self.regs[1] as u64;
                let f = f64::from_bits(bits);
                let s = format!("{}", f);
                let addr = self.heap_alloc_str(s);
                self.regs[1] = addr as i64;
            }
            130 => {
                // fmt_show_bool(val=R1) → R1 = str ptr
                let b = self.regs[1];
                let s = if b != 0 { "true".to_string() } else { "false".to_string() };
                let addr = self.heap_alloc_str(s);
                self.regs[1] = addr as i64;
            }
            132 => {
                // fmt::align_left: R1=str_ptr, R21=width, R22=fill_char → returns new str ptr
                let str_addr = self.regs[1] as usize;
                let width = self.regs[21] as usize;
                let fill = char::from_u32(self.regs[22] as u32).unwrap_or(' ');
                let s = self.bytes_at(str_addr as i64);
                let padded = if s.len() < width {
                    let mut out = s.clone();
                    let mut fb = [0u8; 4];
                    let fs = fill.encode_utf8(&mut fb).as_bytes().to_vec();
                    for _ in 0..(width - s.len()) { out.extend_from_slice(&fs); }
                    out
                } else {
                    s
                };
                let addr = self.heap_alloc_str(padded);
                self.regs[1] = addr as i64;
            }

            // ----------------------------------------------------------------
            // Time (540) and env (550, 552-553)
            // ----------------------------------------------------------------
            540 => {
                // time_now() -> R1 (Unix timestamp as milliseconds, i64)
                let ms = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0);
                self.regs[1] = ms;
            }
            550 => {
                // env_exit(code: R1) — halt the machine with R1 as the exit
                // code.  (Halting instead of std::process::exit keeps buffered
                // output and lets the embedding process decide how to exit;
                // run-t3 propagates R1 as the process status.)
                self.halted = true;
            }
            552 => {
                // env_argc() -> R1
                //
                // `argv[0]` is the .t3b the emulator was handed, so argc is
                // never 0 — it matches the LLVM backend, where env_argc reads
                // /proc/self/cmdline and counts the binary path itself.
                self.regs[1] = self.argv.len() as i64;
            }
            553 => {
                // env_arg(R1) -> R1
                //
                // Out of range returns "", not a trap, byte-for-byte with
                // runtime/system.c's env_arg. The two implementations are the
                // one hand-written pair for this call, so they are kept
                // deliberately boring and identical.
                let idx = self.regs[1];
                let s = if idx < 0 || idx as usize >= self.argv.len() {
                    String::new()
                } else {
                    self.argv[idx as usize].clone()
                };
                let addr = self.heap_alloc_str(s);
                self.regs[1] = addr as i64;
            }

            // Unassigned numbers inside the claimed ranges (e.g. 218) take the
            // graceful TRAP path, not a panic.
            _ => self.trap_unknown_syscall(num),
        }
    }
}
