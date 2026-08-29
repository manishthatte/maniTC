use std::io;
// P71, the impl-method half. Same shape as p71_failed_inst_freefn, reached
// through a generic `impl<T>` method rather than a free function — the two
// call sites gate the return type separately and both had to be split.
struct P { pub x: int, pub y: int }
struct Box2<T> { pub a: T, pub b: T }
impl<T> Box2<T> { fn first(self) -> T { if self.a > self.b { self.a } else { self.a } } }
fn main() {
    let b = Box2 { a: P { x: 1, y: 2 }, b: P { x: 3, y: 4 } };
    let q = b.first();
    io::print_int(q.x); io::print(" "); io::print_int(q.y); io::newline();
}
