use std::io;
struct Pair<T> {
    pub first: T,
    pub second: T,
}
fn main() {
    let p = Pair { first: 1, second: 2 };
    io::print_int(p.first); io::newline();
    let q = Pair { first: 1.5, second: 2.5 };
    if q.first > q.second { io::println("first"); } else { io::println("second"); }
}
