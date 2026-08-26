use std::io;
fn max<T>(a: T, b: T) -> T {
    if a > b { a } else { b }
}
fn main() {
    io::print_int(max(3, 7)); io::newline();
    io::print_float(max(1.5, 2.5)); io::newline();
}
