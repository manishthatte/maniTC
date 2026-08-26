use std::io;
fn largest<T: Ord>(a: T, b: T) -> T { if a > b { a } else { b } }
fn main() {
    io::print_int(largest(3, 9)); io::newline();
    io::print_float(largest(1.5, 2.5)); io::newline();
    io::print_float(largest(-1.5, -2.5)); io::newline();
    io::print_int(largest(7, 2)); io::newline();
}
