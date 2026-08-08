// examples/hello.mt
// The canonical first maniT program.
//
// Demonstrates:
//   - use declarations
//   - io::println for basic output
//   - trit literals and the tif three-way branch
//   - bool3 literals (True, Unknown, False)
//   - the Result<T,E> three-state type

use std::io;
use std::fmt;

fn main() {
    // Basic output.
    io::println("Hello from maniT!");
    io::println("Running on balanced ternary logic.");
    io::println("");

    // -----------------------------------------------------------------------
    // Trit literals and tif branching
    // -----------------------------------------------------------------------
    // A trit holds one of three values: + (+1), 0 (zero), - (-1).
    let t: trit = +;

    io::println("--- trit demo ---");
    tif t {
        + => io::println("Trit is positive (+1)"),
        0 => io::println("Trit is zero     ( 0)"),
        - => io::println("Trit is negative (-1)"),
    }

    // Try each trit value.
    let values: [trit] = [+, 0, -];
    for v in values {
        io::print("  trit ");
        io::print_trit(v);
        io::print(" -> ");
        tif v {
            + => io::println("positive"),
            0 => io::println("zero"),
            - => io::println("negative"),
        }
    }

    // -----------------------------------------------------------------------
    // bool3 — three-valued boolean
    // -----------------------------------------------------------------------
    io::println("");
    io::println("--- bool3 demo ---");

    let sensor_ok: bool3 = True;
    let sensor_unknown: bool3 = Unknown;
    let sensor_fail: bool3 = False;

    let readings: [bool3] = [sensor_ok, sensor_unknown, sensor_fail];
    let labels: [str] = ["sensor_ok", "sensor_unknown", "sensor_fail"];

    let mut i = 0;
    for r in readings {
        io::print("  ");
        io::print(labels[i]);
        io::print(" = ");
        io::println_bool3(r);
        i = i + 1;
    }

    // -----------------------------------------------------------------------
    // Balanced ternary literal (tryte)
    // -----------------------------------------------------------------------
    io::println("");
    io::println("--- balanced ternary literal ---");

    // 0t+0- means:  (+1)*9 + (0)*3 + (-1)*1  = 9 + 0 - 1 = 8
    let n: tryte = 0t+0-;
    io::print("  0t+0- as int = ");
    io::println_int(8);   // value is 8
    io::print("  tryte ");
    io::print_tryte(n);
    io::println(" = +0-");

    // -----------------------------------------------------------------------
    // Result<T, E> — three-state return value
    // -----------------------------------------------------------------------
    io::println("");
    io::println("--- Result demo ---");

    let r1: Result<int, str> = Ok(42);
    let r2: Result<int, str> = Unknown("not yet computed");
    let r3: Result<int, str> = Err("division by zero");

    let results: [Result<int, str>] = [r1, r2, r3];
    for res in results {
        match res {
            Ok(v)       => { io::print("  Ok("); io::print_int(v); io::println(")"); }
            Unknown(m)  => { io::print("  Unknown(\""); io::print(m); io::println("\")"); }
            Err(e)      => { io::print("  Err(\""); io::print(e); io::println("\")"); }
        }
    }

    io::println("");
    io::println("Done.");
}
