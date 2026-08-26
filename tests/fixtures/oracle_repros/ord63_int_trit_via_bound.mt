use std::io;
fn largest<T: Ord>(a: T, b: T) -> T { if a > b { a } else { b } }
fn glyph(t: trit) -> str { tif t { + => "+", 0 => "0", - => "-" } }
fn main() {
    io::print_int(largest(3, 9)); io::newline();
    io::println(glyph(largest(+, -)));
    io::println(glyph(largest(0, -)));
}
