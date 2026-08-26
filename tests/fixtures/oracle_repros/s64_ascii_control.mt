use std::io;
fn main() {
    io::println(str::reverse("abcde"));
    io::print_int(str::len("abc")); io::newline();
    io::print_int(str::char_at("A", 0) as int); io::newline();
}
