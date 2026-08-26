use std::io;
struct B<T> { pub a: T, pub b: T }
impl<T> B<T> { fn big(self) -> T { if self.a > self.b { self.a } else { self.b } } }
fn mid<T>(x: B<T>) -> T { x.big() }
fn outer<T>(x: B<T>) -> T { mid(x) }
fn main() {
    let f = B { a: -1.5, b: -2.5 };
    io::print_float(outer(f)); io::newline();
    let g = B { a: -2.5, b: -1.5 };
    io::print_float(outer(g)); io::newline();
}
