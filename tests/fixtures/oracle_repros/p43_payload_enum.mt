use std::io;
// P43. Every line here fails to COMPILE before 29 August 2026 — `Undefined
// label: Shape::Circle` — so the whole file is the finding. What it pins
// beyond that:
//
//   * `Rect(w, h)` binds TWO fields. Both pattern binders read every field
//     from word 1, so `w * h` was `w * w`; invisible while nothing could
//     construct a Rect.
//   * `Dot` is a plain variant of an enum that HAS payload variants, so it is a
//     cell too. One representation per enum, not per variant: the tag test runs
//     before the variant is known.
//   * the value crosses a function boundary, lives in a variable, and is
//     matched directly as a temporary.
enum Shape { Circle(int), Rect(int, int), Dot }
fn area(s: Shape) -> int {
    match s {
        Shape::Circle(r) => r * r,
        Shape::Rect(w, h) => w * h,
        Shape::Dot => 0,
    }
}
fn main() {
    io::print_int(area(Shape::Circle(3))); io::print(" ");
    io::print_int(area(Shape::Rect(4, 5))); io::print(" ");
    io::print_int(area(Shape::Dot)); io::newline();
    let r = Shape::Rect(6, 7);
    io::print_int(area(r)); io::newline();
    match Shape::Rect(2, 9) {
        Shape::Rect(w, h) => { io::print_int(w); io::print(" "); io::print_int(h); io::newline(); },
        Shape::Circle(x) => { io::print_int(x); io::newline(); },
        Shape::Dot => { io::print_int(0); io::newline(); },
    }
}
