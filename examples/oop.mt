// examples/oop.mt
// Demonstrates user-defined impl blocks, traits, and generic functions.

use std::io;
use std::fmt;

// ---------------------------------------------------------------------------
// 1. Structs with impl methods
// ---------------------------------------------------------------------------

struct Point {
    x: float,
    y: float,
}

impl Point {
    fn new(x: float, y: float) -> Point {
        return Point { x: x, y: y };
    }

    fn zero() -> Point {
        return Point { x: 0.0, y: 0.0 };
    }

    fn add(self, other: Point) -> Point {
        return Point { x: self.x + other.x, y: self.y + other.y };
    }

    fn scale(self, factor: float) -> Point {
        return Point { x: self.x * factor, y: self.y * factor };
    }

    fn to_str(self) -> str {
        return fmt::format("({}, {})", [fmt::show_float(self.x), fmt::show_float(self.y)]);
    }
}

fn demo_point() {
    io::println("--- 1. Point struct with impl methods ---");

    let p1 = Point::new(3.0, 4.0);
    let p2 = Point::new(1.0, 2.0);

    let p3 = p1.add(p2);
    io::print("  p1 + p2 = ");
    io::println(p3.to_str());

    let p4 = p1.scale(2.0);
    io::print("  p1 * 2 = ");
    io::println(p4.to_str());

    let origin = Point::zero();
    io::print("  origin  = ");
    io::println(origin.to_str());
    io::println("");
}

// ---------------------------------------------------------------------------
// 2. Counter struct — mutable state via return values
// ---------------------------------------------------------------------------

struct Counter {
    value: int,
    step: int,
}

impl Counter {
    fn new(start: int, step: int) -> Counter {
        return Counter { value: start, step: step };
    }

    fn tick(self) -> Counter {
        return Counter { value: self.value + self.step, step: self.step };
    }

    fn reset(self) -> Counter {
        return Counter { value: 0, step: self.step };
    }

    fn get(self) -> int {
        return self.value;
    }
}

fn demo_counter() {
    io::println("--- 2. Counter with chained method calls ---");

    let c = Counter::new(0, 3);
    let c = c.tick();
    let c = c.tick();
    let c = c.tick();
    io::print("  After 3 ticks (step=3): ");
    io::println_int(c.get());    // 9

    let c = c.reset();
    io::print("  After reset: ");
    io::println_int(c.get());    // 0
    io::println("");
}

// ---------------------------------------------------------------------------
// 3. Trait definition and implementation
// ---------------------------------------------------------------------------

trait Describable {
    fn describe(self) -> str;
}

trait Scalable {
    fn scale_by(self, factor: int) -> Self;
    fn magnitude(self) -> int;
}

struct Rectangle {
    width: int,
    height: int,
}

impl Rectangle {
    fn new(w: int, h: int) -> Rectangle {
        return Rectangle { width: w, height: h };
    }

    fn area(self) -> int {
        return self.width * self.height;
    }

    fn perimeter(self) -> int {
        return (self.width + self.height) * 2;
    }
}

impl Describable for Rectangle {
    fn describe(self) -> str {
        return fmt::format("Rectangle({}x{})", [fmt::show_int(self.width), fmt::show_int(self.height)]);
    }
}

impl Scalable for Rectangle {
    fn scale_by(self, factor: int) -> Rectangle {
        return Rectangle { width: self.width * factor, height: self.height * factor };
    }

    fn magnitude(self) -> int {
        return self.width * self.height;
    }
}

struct Circle {
    radius: int,
}

impl Circle {
    fn new(r: int) -> Circle {
        return Circle { radius: r };
    }
}

impl Describable for Circle {
    fn describe(self) -> str {
        return fmt::format("Circle(r={})", [fmt::show_int(self.radius)]);
    }
}

impl Scalable for Circle {
    fn scale_by(self, factor: int) -> Circle {
        return Circle { radius: self.radius * factor };
    }

    fn magnitude(self) -> int {
        return self.radius * self.radius;
    }
}

fn demo_traits() {
    io::println("--- 3. Traits: Describable + Scalable ---");

    let rect = Rectangle::new(4, 3);
    io::print("  ");
    io::println(rect.describe());
    io::print("  area = ");
    io::println_int(rect.area());
    io::print("  perimeter = ");
    io::println_int(rect.perimeter());

    let big_rect = rect.scale_by(2);
    io::print("  scaled: ");
    io::println(big_rect.describe());
    io::print("  magnitude = ");
    io::println_int(big_rect.magnitude());

    let circle = Circle::new(5);
    io::print("  ");
    io::println(circle.describe());
    let big_circle = circle.scale_by(3);
    io::print("  scaled: ");
    io::println(big_circle.describe());
    io::print("  magnitude = ");
    io::println_int(big_circle.magnitude());
    io::println("");
}

// ---------------------------------------------------------------------------
// 4. Generic functions (type erasure — T treated as Unknown at compile time)
// ---------------------------------------------------------------------------

fn identity<T>(x: T) -> T {
    return x;
}

fn max_int(a: int, b: int) -> int {
    if a > b { return a; }
    return b;
}

fn clamp_int(val: int, lo: int, hi: int) -> int {
    if val < lo { return lo; }
    if val > hi { return hi; }
    return val;
}

fn demo_generic_fns() {
    io::println("--- 4. Generic and utility functions ---");

    let n = identity(42);
    io::print("  identity(42) = ");
    io::println_int(n);

    io::print("  max(7, 3)  = ");
    io::println_int(max_int(7, 3));

    io::print("  clamp(15, 0, 10) = ");
    io::println_int(clamp_int(15, 0, 10));
    io::println("");
}

// ---------------------------------------------------------------------------
// 5. Enum methods
// ---------------------------------------------------------------------------

enum Direction {
    North,
    South,
    East,
    West,
}

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
            Direction::North => "North",
            Direction::South => "South",
            Direction::East  => "East",
            Direction::West  => "West",
        }
    }
}

fn demo_enum_methods() {
    io::println("--- 5. Enum with impl methods ---");

    let d = Direction::North;
    io::print("  ");
    io::print(d.to_str());
    io::print(" -> opposite -> ");
    io::println(d.opposite().to_str());

    let d2 = Direction::East;
    io::print("  ");
    io::print(d2.to_str());
    io::print(" -> opposite -> ");
    io::println(d2.opposite().to_str());
    io::println("");
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() {
    io::println("maniT OOP Demo");
    io::println("==============");
    io::println("");

    demo_point();
    demo_counter();
    demo_traits();
    demo_generic_fns();
    demo_enum_methods();

    io::println("All demos complete.");
}
