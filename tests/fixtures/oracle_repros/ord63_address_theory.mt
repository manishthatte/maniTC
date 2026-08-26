use std::io;
fn largest<T: Ord>(a: T, b: T) -> T { if a > b { a } else { b } }
fn main() {
    let hi: str = "zzz"; let lo: str = "aaa";
    io::println(largest(hi, lo));
    let lo2: str = "aaa"; let hi2: str = "zzz";
    io::println(largest(hi2, lo2));
}
