use std::io;
struct Pair<A, B> { pub first: A, pub second: B }
impl<A, B> Pair<A, B> { fn same(self) -> Pair<A, B> { Pair { first: self.first, second: self.second } } }
fn main() { let p = Pair { first: 1, second: "x" }; let q = p.same();
    io::print_int(q.first); io::println(q.second); }
