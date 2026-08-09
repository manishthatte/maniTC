use std::io;

// classify: maps a balanced-ternary t27 word to its sign as a tryte.
//   Input  +1  →  tryte +1  (positive)
//   Input   0  →  tryte  0  (zero)
//   Input  -1  →  tryte -1  (negative)

fn classify(x: t27) -> tryte {
    // tif branches on a trit/bool3; `as trit` clamps a t27 to its sign
    // (docs/language-reference.md, "Type cast" and "tif").
    tif (x as trit) {
        + => { return 1 as tryte }
        0 => { return 0 as tryte }
        - => { return -1 as tryte }
    }
}

fn main() {
    let pos: t27 = 1 as t27;
    let zer: t27 = 0 as t27;
    let neg: t27 = -1 as t27;

    let r_pos = classify(pos);
    let r_zer = classify(zer);
    let r_neg = classify(neg);

    io::print("R1=+1 -> tryte="); io::print_int(r_pos as int); io::newline();
    io::print("R1=0  -> tryte="); io::print_int(r_zer as int); io::newline();
    io::print("R1=-1 -> tryte="); io::print_int(r_neg as int); io::newline();
}
