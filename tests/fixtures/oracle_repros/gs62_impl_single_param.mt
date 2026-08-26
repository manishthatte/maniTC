use std::io;
struct Pair<T> { pub first: T, pub second: T }
impl<T> Pair<T> { fn swap(self) -> Pair<T> { Pair { first: self.second, second: self.first } } }
fn main() { let p = Pair { first: 1, second: 2 }; let q = p.swap();
    io::print_int(q.first); io::print(" "); io::print_int(q.second); io::newline(); }
