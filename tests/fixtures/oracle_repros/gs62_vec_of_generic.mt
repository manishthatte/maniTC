use std::io;
struct Pair<A, B> { pub first: A, pub second: B }
fn main() {
    let mut v: Vec<Pair<int, str>> = Vec::new();
    v.push(Pair { first: 1, second: "a" });
    v.push(Pair { first: 2, second: "b" });
    for i in 0..v.len() { let p: Pair<int, str> = v.get(i);
        io::print_int(p.first); io::print(" "); io::println(p.second); }
}
