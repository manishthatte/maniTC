use std::io;
struct Box2<T> { pub a: T, pub b: T }
fn main() {
    let p = Box2 { a: 1.5, b: 2.5 };
    io::print_float(p.a); io::print(" "); io::print_float(p.b); io::newline();
    if p.a > p.b { io::println("a"); } else { io::println("b"); }
    let n = Box2 { a: -1.5, b: -2.5 };
    if n.a > n.b { io::println("a"); } else { io::println("b"); }
    let i = Box2 { a: 3, b: 9 };
    io::print_int(i.a); io::print(" "); io::print_int(i.b); io::newline();
}
