use std::io;
fn take_first<A, B>(a: A, b: B) -> A { a }
fn take_second<A, B>(a: A, b: B) -> B { b }
fn main() {
    io::println(take_second(1, "x"));
    io::print_int(take_first(2, "y")); io::newline();
    io::print_int(take_second("z", 3)); io::newline();
}
