use std::io;
// P73, which P69 recorded as a limit: `Box2::bigger(b)` — the PATH form of a
// generic `impl<T>` method call — bound nothing, so it kept the erased body and
// compared two floats as integer bit patterns.
//
// THE NEGATIVE PAIR IS THE TEST. Positive doubles order the same way as their
// bit patterns, so `(1.5, 2.5)` answers 2.5 whether the comparison is a float
// compare or an integer one — the same trap P68 recorded, one finding later.
// Both forms are here so a reader can see which one moved.
struct Box2<T> { pub a: T, pub b: T }
impl<T> Box2<T> { fn bigger(self) -> T { if self.a > self.b { self.a } else { self.b } } }
fn main() {
    let p = Box2 { a: 1.5, b: 2.5 };
    io::print_float(Box2::bigger(p)); io::newline();
    let n = Box2 { a: 0.0 - 1.5, b: 0.0 - 2.5 };
    io::print_float(Box2::bigger(n)); io::newline();
    let m = Box2 { a: 0.0 - 1.5, b: 0.0 - 2.5 };
    io::print_float(m.bigger()); io::newline();
    let i = Box2 { a: 3, b: 7 };
    io::print_int(Box2::bigger(i)); io::newline();
}
