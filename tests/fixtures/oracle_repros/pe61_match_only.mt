use std::io;
enum Shape { Circle(int), Rect(int,int) }
fn area(s: Shape) -> int { match s { Shape::Circle(r) => r, Shape::Rect(w,h) => w*h, } }
fn main() { io::print_int(2); io::newline(); }
