use std::io;
struct Rec { pub n: int, pub s: str }
impl Rec { fn bump(self) -> Rec { Rec { n: self.n + 1, s: self.s } } }
fn main() { let r = Rec { n: 1, s: "x" }; let q = r.bump(); io::print_int(q.n); io::println(q.s); }
