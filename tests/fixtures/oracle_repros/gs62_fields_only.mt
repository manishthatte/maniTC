use std::io;
struct Pair<A, B> { pub first: A, pub second: B }
fn main() { let p = Pair { first: 1, second: "x" }; io::print_int(p.first); io::println(p.second); }
