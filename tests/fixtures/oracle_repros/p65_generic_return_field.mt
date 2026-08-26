use std::io;
struct Pair { pub first: int, pub second: int }
fn id<T>(x: T) -> T { x }
fn main() {
    let p = Pair { first: 1, second: 2 };
    let q = id(p);
    io::print_int(q.first); io::print(" "); io::print_int(q.second); io::newline();
}
