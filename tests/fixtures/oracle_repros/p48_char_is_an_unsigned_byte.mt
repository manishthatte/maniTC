use std::io;
use std::str;
// P48. `char as int` was the only divergence the report recorded. It is not the
// only one: a `char` was an UNSIGNED BYTE on T3 and a SIGNED i8 on LLVM, and
// every operation inherited it. Each line below answered differently on the two
// backends before 29 August 2026.
fn thru_fn(c: char) -> int { return c as int; }
fn main() {
    let s = "aéb";
    // 1. the recorded one: every byte of a multi-byte string
    let mut i: int = 0;
    while i < str::len(s) {
        io::print_int(str::char_at(s, i) as int); io::print(" ");
        i = i + 1;
    }
    io::newline();
    let c: char = str::char_at(s, 1);          // 0xC3
    // 2. NOT recorded: ORDERING. `c > 'a'` was 1 on T3 and 0 on LLVM, so every
    //    str:: function that compares characters answered differently.
    io::print_int(if c > 'a' { 1 } else { 0 });
    io::print_int(if c < 'a' { 1 } else { 0 });
    io::print_int(if c == c { 1 } else { 0 });
    io::newline();
    // 3. NOT recorded: `int as char` did not narrow AT ALL on T3 (300 stayed
    //    300) while LLVM truncated to 44. Both clamp now, as `as trit` does.
    let neg: int = 0 - 5;
    io::print_int(300 as char as int); io::print(" ");
    io::print_int(neg as char as int); io::print(" ");
    io::print_int(255 as char as int); io::print(" ");
    io::print_int(0 as char as int);
    io::newline();
    // 4. NOT recorded: `float as char` was not a conversion on T3 at all — it
    //    handed back the raw IEEE-754 bit pattern.
    let negf: float = 0.0 - 2.5;
    io::print_int(3.9 as char as int); io::print(" ");
    io::print_int(negf as char as int); io::print(" ");
    io::print_int(999.0 as char as int);
    io::newline();
    // 5. the value must survive a call boundary and an array slot
    io::print_int(thru_fn(c)); io::print(" ");
    let mut arr: [char; 2] = ['x', 'y'];
    arr[0] = c;
    io::print_int(arr[0] as int);
    io::newline();
    // 6. EVERY cast that touches a char, in both directions. These are here
    //    because giving `char` its own IR type silently removed it from every
    //    or-pattern that listed `I8` — five of them, and the compiler reported
    //    none, since each pattern stayed valid without it (report.txt P68's
    //    shape). `'Q' as trit` came out 81 and `true as char` came out 0.
    //    `'Q' as float` is here for a different reason: it was ALREADY wrong on
    //    T3 before any of this, and only probing the whole family found it.
    let t: trit = 0 - 1;
    let bt: bool = true;
    let b3: bool3 = 5 > 0;
    let y: tryte = 100;
    io::print_int(t as char as int); io::print(" ");
    io::print_int(bt as char as int); io::print(" ");
    io::print_int(b3 as char as int); io::print(" ");
    io::print_int(y as char as int);
    io::newline();
    let q: char = 'Q';
    io::print_int(q as int); io::print(" ");
    io::print_int(q as trit as int); io::print(" ");
    io::print_float(q as float); io::print(" ");
    io::print_int(if q as bool { 1 } else { 0 });
    io::newline();
}
