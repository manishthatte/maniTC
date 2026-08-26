use std::io;
struct Box2<T> { pub a: T, pub b: T }
impl<T> Box2<T> { fn bigger(self) -> T { if self.a > self.b { self.a } else { self.b } } }
fn main() {
    let f = Box2 { a: 1.5, b: 2.5 };
    io::print_float(f.bigger()); io::newline();
    let g = Box2 { a: -1.5, b: -2.5 };
    io::print_float(g.bigger()); io::newline();
    let i = Box2 { a: 7, b: 3 };
    io::print_int(i.bigger()); io::newline();
    let j = Box2 { a: -7, b: -3 };
    io::print_int(j.bigger()); io::newline();
}
