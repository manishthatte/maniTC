use std::io;
struct Pair<T> { pub first: T, pub second: T }
fn swap<T>(p: Pair<T>) -> Pair<T> { Pair { first: p.second, second: p.first } }
fn main() { let p = Pair { first: 1, second: 2 }; let q = swap(p);
    io::print_int(q.first); io::print(" "); io::print_int(q.second); io::newline(); }
