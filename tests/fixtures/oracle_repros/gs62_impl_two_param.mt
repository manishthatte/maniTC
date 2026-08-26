use std::io;
struct Pair<A, B> { pub first: A, pub second: B }
impl<A, B> Pair<A, B> { fn swap(self) -> Pair<B, A> { Pair { first: self.second, second: self.first } } }
fn main() { let p = Pair { first: 1, second: "x" }; let q = p.swap();
    io::println(q.first); io::print_int(q.second); io::newline(); }
