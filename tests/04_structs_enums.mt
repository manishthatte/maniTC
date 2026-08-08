// Test: structs, enums, impl blocks, traits, pattern matching on custom types
use std::io;
use std::fmt;

fn pass(label: str) { io::print("PASS "); io::println(label); }
fn fail(label: str) { io::print("FAIL "); io::println(label); }
fn check(label: str, cond: bool) { if cond { pass(label) } else { fail(label) } }
fn check_int(label: str, got: int, want: int) {
    if got == want { pass(label) }
    else {
        io::print("FAIL "); io::print(label);
        io::print(" got="); io::print_int(got);
        io::print(" want="); io::print_int(want);
        io::newline();
    }
}
fn check_str(label: str, got: str, want: str) {
    if got == want { pass(label) }
    else {
        io::print("FAIL "); io::print(label);
        io::print(" got="); io::print(got);
        io::print(" want="); io::println(want);
    }
}

// ---------------------------------------------------------------------------
// Basic struct
// ---------------------------------------------------------------------------

struct Point {
    pub x: int,
    pub y: int,
}

impl Point {
    fn new(x: int, y: int) -> Point {
        Point { x: x, y: y }
    }

    fn add(self, other: Point) -> Point {
        Point { x: self.x + other.x, y: self.y + other.y }
    }

    fn scale(self, factor: int) -> Point {
        Point { x: self.x * factor, y: self.y * factor }
    }

    fn manhattan(self) -> int {
        let ax = if self.x < 0 { -self.x } else { self.x };
        let ay = if self.y < 0 { -self.y } else { self.y };
        ax + ay
    }

    fn eq(self, other: Point) -> bool {
        self.x == other.x && self.y == other.y
    }

    fn to_str(self) -> str {
        fmt::format("({}, {})", self.x, self.y)
    }
}

fn test_struct_basic() {
    let p = Point { x: 3, y: 4 };
    check_int("struct: field x",   p.x, 3);
    check_int("struct: field y",   p.y, 4);
}

fn test_struct_new() {
    let p = Point::new(5, 6);
    check_int("struct-new: x=5",  p.x, 5);
    check_int("struct-new: y=6",  p.y, 6);
}

fn test_struct_methods() {
    let p1 = Point::new(1, 2);
    let p2 = Point::new(3, 4);
    let sum = p1.add(p2);
    check_int("method: add x",   sum.x, 4);
    check_int("method: add y",   sum.y, 6);

    let scaled = p1.scale(3);
    check_int("method: scale x", scaled.x, 3);
    check_int("method: scale y", scaled.y, 6);

    check_int("method: manhattan (3,4)=7", p2.manhattan(), 7);
    check_int("method: manhattan (-1,-1)=2", Point::new(-1, -1).manhattan(), 2);
}

fn test_struct_eq() {
    let p1 = Point::new(1, 2);
    let p2 = Point::new(1, 2);
    let p3 = Point::new(3, 4);
    check("struct-eq: same",    p1.eq(p2));
    check("struct-eq: diff",    !p1.eq(p3));
}

fn test_struct_to_str() {
    let p = Point::new(1, 2);
    check_str("struct-str: (1, 2)", p.to_str(), "(1, 2)");
}

// ---------------------------------------------------------------------------
// Struct mutation
// ---------------------------------------------------------------------------

struct Counter {
    pub value: int,
    pub step: int,
}

impl Counter {
    fn new(step: int) -> Counter {
        Counter { value: 0, step: step }
    }
    fn tick(self) -> Counter {
        Counter { value: self.value + self.step, step: self.step }
    }
    fn reset(self) -> Counter {
        Counter { value: 0, step: self.step }
    }
}

fn test_counter() {
    let c = Counter::new(3);
    check_int("counter: init=0",  c.value, 0);
    let c1 = c.tick();
    check_int("counter: tick1=3", c1.value, 3);
    let c2 = c1.tick();
    check_int("counter: tick2=6", c2.value, 6);
    let c3 = c2.reset();
    check_int("counter: reset=0", c3.value, 0);
    check_int("counter: step=3",  c3.step,  3);
}

// ---------------------------------------------------------------------------
// Enum — unit variants
// ---------------------------------------------------------------------------

enum Color { Red, Green, Blue }

impl Color {
    fn to_str(self) -> str {
        match self {
            Color::Red   => "red",
            Color::Green => "green",
            Color::Blue  => "blue",
        }
    }
    fn is_primary(self) -> bool { true }
    fn index(self) -> int {
        match self {
            Color::Red   => 0,
            Color::Green => 1,
            Color::Blue  => 2,
        }
    }
}

fn test_enum_basic() {
    let r = Color::Red;
    let g = Color::Green;
    let b = Color::Blue;
    check_str("enum: red",   Color::Red.to_str(),   "red");
    check_str("enum: green", Color::Green.to_str(), "green");
    check_str("enum: blue",  Color::Blue.to_str(),  "blue");
    check_int("enum: idx red",   r.index(), 0);
    check_int("enum: idx green", g.index(), 1);
    check_int("enum: idx blue",  b.index(), 2);
}

fn test_enum_match() {
    let c = Color::Green;
    let name = match c {
        Color::Red   => "r",
        Color::Green => "g",
        Color::Blue  => "b",
    };
    check_str("enum-match: green=g", name, "g");
}

// ---------------------------------------------------------------------------
// Enum — four-way direction
// ---------------------------------------------------------------------------

enum Direction { North, South, East, West }

impl Direction {
    fn opposite(self) -> Direction {
        match self {
            Direction::North => Direction::South,
            Direction::South => Direction::North,
            Direction::East  => Direction::West,
            Direction::West  => Direction::East,
        }
    }
    fn to_str(self) -> str {
        match self {
            Direction::North => "N",
            Direction::South => "S",
            Direction::East  => "E",
            Direction::West  => "W",
        }
    }
    fn dx(self) -> int {
        match self { Direction::East => 1, Direction::West => -1, _ => 0 }
    }
    fn dy(self) -> int {
        match self { Direction::North => 1, Direction::South => -1, _ => 0 }
    }
}

fn test_direction() {
    check_str("dir: N.to_str=N",    Direction::North.to_str(), "N");
    check_str("dir: S.to_str=S",    Direction::South.to_str(), "S");
    check_str("dir: opposite(N)=S", Direction::North.opposite().to_str(), "S");
    check_str("dir: opposite(E)=W", Direction::East.opposite().to_str(),  "W");
    check_int("dir: dx(E)=1",       Direction::East.dx(),   1);
    check_int("dir: dx(W)=-1",      Direction::West.dx(),  -1);
    check_int("dir: dy(N)=1",       Direction::North.dy(),  1);
    check_int("dir: dy(S)=-1",      Direction::South.dy(), -1);
}

fn test_direction_chain() {
    let d = Direction::North;
    let s = d.opposite().to_str();
    check_str("dir-chain: N.opp.to_str=S", s, "S");
}

// ---------------------------------------------------------------------------
// Traits
// ---------------------------------------------------------------------------

trait Area {
    fn area(self) -> int;
}

trait Perimeter {
    fn perimeter(self) -> int;
}

trait Describe {
    fn describe(self) -> str;
}

struct Rectangle {
    pub width: int,
    pub height: int,
}

struct Square {
    pub side: int,
}

impl Area for Rectangle {
    fn area(self) -> int { self.width * self.height }
}

impl Perimeter for Rectangle {
    fn perimeter(self) -> int { 2 * (self.width + self.height) }
}

impl Describe for Rectangle {
    fn describe(self) -> str { fmt::format("Rect({}x{})", self.width, self.height) }
}

impl Area for Square {
    fn area(self) -> int { self.side * self.side }
}

impl Perimeter for Square {
    fn perimeter(self) -> int { 4 * self.side }
}

impl Describe for Square {
    fn describe(self) -> str { fmt::format("Square({})", self.side) }
}

fn test_traits() {
    let r = Rectangle { width: 4, height: 3 };
    let s = Square { side: 5 };

    check_int("trait: rect area=12",    r.area(),      12);
    check_int("trait: rect perim=14",   r.perimeter(), 14);
    check_str("trait: rect describe",   r.describe(),  "Rect(4x3)");

    check_int("trait: sq area=25",      s.area(),      25);
    check_int("trait: sq perim=20",     s.perimeter(), 20);
    check_str("trait: sq describe",     s.describe(),  "Square(5)");
}

// ---------------------------------------------------------------------------
// Nested struct usage
// ---------------------------------------------------------------------------

struct Segment {
    pub start: Point,
    pub end: Point,
}

impl Segment {
    fn dx(self) -> int { self.end.x - self.start.x }
    fn dy(self) -> int { self.end.y - self.start.y }
    fn manhattan_len(self) -> int {
        let adx = if self.dx() < 0 { -self.dx() } else { self.dx() };
        let ady = if self.dy() < 0 { -self.dy() } else { self.dy() };
        adx + ady
    }
}

fn test_nested_struct() {
    let s = Segment {
        start: Point { x: 1, y: 1 },
        end:   Point { x: 4, y: 5 },
    };
    check_int("nested: dx=3",            s.dx(),            3);
    check_int("nested: dy=4",            s.dy(),            4);
    check_int("nested: manhattan_len=7", s.manhattan_len(), 7);
    check_int("nested: start.x=1",       s.start.x,         1);
    check_int("nested: end.y=5",         s.end.y,           5);
}

// ---------------------------------------------------------------------------
// Multiple traits on one type
// ---------------------------------------------------------------------------

trait Named {
    fn name(self) -> str;
}

impl Named for Rectangle {
    fn name(self) -> str { "rectangle" }
}

impl Named for Square {
    fn name(self) -> str { "square" }
}

fn test_multi_trait() {
    let r = Rectangle { width: 2, height: 3 };
    let s = Square { side: 4 };
    check_str("multi-trait: rect name",  r.name(), "rectangle");
    check_str("multi-trait: sq name",    s.name(), "square");
    check_int("multi-trait: rect area",  r.area(), 6);
    check_int("multi-trait: sq area",    s.area(), 16);
}

// ---------------------------------------------------------------------------
// Enum with wildcard in match
// ---------------------------------------------------------------------------

enum Weekday { Mon, Tue, Wed, Thu, Fri, Sat, Sun }

fn is_weekend(d: Weekday) -> bool {
    match d {
        Weekday::Sat | Weekday::Sun => true,
        _ => false,
    }
}

fn test_enum_wildcard() {
    check("weekend: Sat",   is_weekend(Weekday::Sat));
    check("weekend: Sun",   is_weekend(Weekday::Sun));
    check("weekend: !Mon",  !is_weekend(Weekday::Mon));
    check("weekend: !Wed",  !is_weekend(Weekday::Wed));
    check("weekend: !Fri",  !is_weekend(Weekday::Fri));
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() {
    io::println("=== 04 Structs & Enums ===");

    io::println("-- struct basic --");
    test_struct_basic();
    test_struct_new();
    test_struct_methods();
    test_struct_eq();
    test_struct_to_str();

    io::println("-- counter --");
    test_counter();

    io::println("-- enum Color --");
    test_enum_basic();
    test_enum_match();

    io::println("-- enum Direction --");
    test_direction();
    test_direction_chain();

    io::println("-- traits --");
    test_traits();

    io::println("-- nested struct --");
    test_nested_struct();

    io::println("-- multi-trait --");
    test_multi_trait();

    io::println("-- enum wildcard --");
    test_enum_wildcard();

    io::println("Done.");
}
