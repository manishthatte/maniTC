// EXPECT_ERROR: non-exhaustive match
enum Color {
    Red,
    Green,
    Blue,
}

fn main() {
    let c = Color::Red;
    let name = match c {
        Color::Red => "red",
        Color::Green => "green",
    };
}
