// EXPECT_ERROR: non-exhaustive match on trit
fn main() {
    let t: trit = +;
    let val = match t {
        + => 1,
        0 => 0,
    };
}
