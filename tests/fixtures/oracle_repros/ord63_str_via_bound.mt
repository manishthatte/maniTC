use std::io;
fn largest<T: Ord>(a: T, b: T) -> T { if a > b { a } else { b } }
fn main() { io::println(largest("mm", "aa")); io::println(largest("aa", "mm")); io::println(largest("zz", "ab")); }
