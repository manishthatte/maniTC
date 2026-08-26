use std::io;
struct Box2<T> { pub a: T, pub b: T }
impl<T> Box2<T> { fn bigger(self) -> T { if self.a > self.b { self.a } else { self.b } } }
fn main() {
    let p = Box2 { a: 1.5, b: 2.5 };
    io::print_float(p.bigger()); io::newline();
    let n = Box2 { a: -1.5, b: -2.5 };
    io::print_float(n.bigger()); io::newline();
}
