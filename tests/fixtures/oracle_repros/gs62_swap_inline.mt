use std::io;
struct Pair<A, B> { pub first: A, pub second: B }
fn main() { let p = Pair { first: 1, second: "x" };
    let q = Pair { first: p.second, second: p.first };
    io::println(q.first); io::print_int(q.second); io::newline(); }
