// Test: tcon and tany operators — complete 3×3 truth tables
// tcon(a,b) = consensus: +1 if both +1, -1 if both -1, else 0
// tany(a,b) = any:       +1 if either +1, -1 if either -1, else 0
use std::io;

fn pass(label: str) { io::print("PASS "); io::println(label); }
fn fail(label: str) { io::print("FAIL "); io::println(label); }

fn trit_val(t: trit) -> int {
    if t > 0 { 1 } elif t == 0 { 0 } else { -1 }
}

fn check_trit(label: str, t: trit, want: int) {
    let g = trit_val(t);
    if g == want { pass(label); }
    else {
        io::print("FAIL "); io::print(label);
        io::print(" got="); io::print_int(g);
        io::print(" want="); io::print_int(want);
        io::newline();
    }
}

fn test_tcon() {
    let p: trit = +;
    let z: trit = 0;
    let n: trit = -;

    // tcon truth table (consensus)
    check_trit("tcon: +,+=+",  p tcon p,  1);
    check_trit("tcon: +,0=0",  p tcon z,  0);
    check_trit("tcon: +,-=0",  p tcon n,  0);
    check_trit("tcon: 0,+=0",  z tcon p,  0);
    check_trit("tcon: 0,0=0",  z tcon z,  0);
    check_trit("tcon: 0,-=0",  z tcon n,  0);
    check_trit("tcon: -,+=0",  n tcon p,  0);
    check_trit("tcon: -,0=0",  n tcon z,  0);
    check_trit("tcon: -,-=-",  n tcon n, -1);
}

fn test_tany() {
    let p: trit = +;
    let z: trit = 0;
    let n: trit = -;

    // tany truth table (any)
    check_trit("tany: +,+=+",  p tany p,  1);
    check_trit("tany: +,0=+",  p tany z,  1);
    check_trit("tany: +,-=+",  p tany n,  1);
    check_trit("tany: 0,+=+",  z tany p,  1);
    check_trit("tany: 0,0=0",  z tany z,  0);
    check_trit("tany: 0,-=-",  z tany n, -1);
    check_trit("tany: -,+= +",  n tany p,  1);  // positive wins when both signs present
    check_trit("tany: -,0=-",  n tany z, -1);
    check_trit("tany: -,-=-",  n tany n, -1);
}

fn test_tcon_laws() {
    let p: trit = +;
    let z: trit = 0;
    let n: trit = -;

    // Idempotent: tcon(x,x) = x
    check_trit("tcon-idem: +tcon+=+", p tcon p,  1);
    check_trit("tcon-idem: 0tcon0=0", z tcon z,  0);
    check_trit("tcon-idem: -tcon-=-", n tcon n, -1);

    // Commutative: tcon(a,b) = tcon(b,a)
    check_trit("tcon-comm: +tcon-=0", p tcon n,  0);
    check_trit("tcon-comm: -tcon+=0", n tcon p,  0);

    // tcon(x,0) = 0 for all x (zero annihilates)
    check_trit("tcon-zero: +tcon0=0", p tcon z,  0);
    check_trit("tcon-zero: -tcon0=0", n tcon z,  0);
}

fn test_tany_laws() {
    let p: trit = +;
    let z: trit = 0;
    let n: trit = -;

    // Idempotent: tany(x,x) = x
    check_trit("tany-idem: +tany+=+", p tany p,  1);
    check_trit("tany-idem: 0tany0=0", z tany z,  0);
    check_trit("tany-idem: -tany-=-", n tany n, -1);

    // tany(x,0) = x (zero is identity)
    check_trit("tany-id: +tany0=+", p tany z,  1);
    check_trit("tany-id: -tany0=-", n tany z, -1);
    check_trit("tany-id: 0tany0=0", z tany z,  0);
}

fn main() {
    io::println("=== 17 tcon/tany operators ===");

    io::println("-- tcon truth table --");
    test_tcon();

    io::println("-- tany truth table --");
    test_tany();

    io::println("-- tcon laws --");
    test_tcon_laws();

    io::println("-- tany laws --");
    test_tany_laws();

    io::println("Done.");
}
