use std::io;
struct Two<A, B> { pub a: A, pub b: B }
impl<A, B> Two<A, B> {
    fn maxa(self, other: A) -> A { if self.a > other { self.a } else { other } }
    fn maxb(self, other: B) -> B { if self.b > other { self.b } else { other } }
}
fn main() {
    let t = Two { a: -1.5, b: -7 };
    io::print_float(t.maxa(-2.5)); io::newline();
    io::print_int(t.maxb(-9)); io::newline();
    let u = Two { a: -9, b: -1.5 };
    io::print_int(u.maxa(-7)); io::newline();
    io::print_float(u.maxb(-2.5)); io::newline();
}
