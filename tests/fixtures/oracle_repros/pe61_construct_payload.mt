use std::io;
enum Shape { Circle(int) }
fn main() { let s = Shape::Circle(2); match s { Shape::Circle(r) => { io::print_int(r); io::newline(); } } }
