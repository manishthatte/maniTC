use std::io;
// P71: `pick`'s body cannot be instantiated at `T = P` — `>` does not apply to
// a struct (P45) — so the instantiation is DISCARDED and the erased body runs.
// The DECLARATION still says `-> T`, so the call's type is `P`, and `q.y` must
// read slot 1. Before P71 the call was typed `<unknown>`, a field lookup on
// which finds no struct and takes slot 0: this printed `1 1`.
//
// Both arms return `self.a` on purpose. Which struct `>` picks is A4's open
// address comparison and differs between backends; the FIELD it then reads
// must not.
struct P { pub x: int, pub y: int }
fn pick<T>(a: T, b: T) -> T { if a > b { a } else { a } }
fn main() {
    let q = pick(P { x: 1, y: 2 }, P { x: 3, y: 4 });
    io::print_int(q.x); io::print(" "); io::print_int(q.y); io::newline();
}
