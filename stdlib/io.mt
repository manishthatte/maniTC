// stdlib/std/io.mt
// Standard I/O module for maniT.
//
// Provides console I/O, stdin/stdout/stderr handles, and ternary-aware
// printing utilities.  All functions in this module are native builtins;
// the bodies shown here are stubs that document calling conventions.
//
// Usage:
//   use std::io;
//   io::println("hello");

// ---------------------------------------------------------------------------
// Basic text output
// ---------------------------------------------------------------------------

// Print a string followed by a newline to stdout.
fn println(s: str) ;  // native

// Print a string without a trailing newline to stdout.
fn print(s: str) ;  // native

// Print a string followed by a newline to stderr.
fn eprintln(s: str) ;  // native

// Print a string without a trailing newline to stderr.
fn eprint(s: str) ;  // native

// ---------------------------------------------------------------------------
// Basic text input
// ---------------------------------------------------------------------------

// Read a single line from stdin (newline stripped).
// Blocks until the user presses Enter.
fn read_line() -> str ;  // native

// Read a line from stdin and parse it as an int.
// Panics if the input is not a valid integer.
fn read_int() -> int ;  // native

// Read a line from stdin and parse it as a float.
// Panics if the input is not a valid float.
fn read_float() -> float ;  // native

// ---------------------------------------------------------------------------
// Ternary-aware output
// ---------------------------------------------------------------------------

// Print a single trit value: displays "+", "0", or "-".
fn print_trit(t: trit) ;  // native

// Print a trit value followed by a newline.
fn println_trit(t: trit) ;  // native

// Print a bool3 value: displays "True", "Unknown", or "False".
fn print_bool3(b: bool3) ;  // native

// Print a bool3 value followed by a newline.
fn println_bool3(b: bool3) ;  // native

// Print a t27 (27-trit) integer in balanced ternary notation.
// Example output: "+0-+0" for the value 72.
fn print_ternary(n: t27) ;  // native

// Print a t27 value in balanced ternary followed by a newline.
fn println_ternary(n: t27) ;  // native

// Print a tryte (3-trit) in balanced ternary notation.
fn print_tryte(t: tryte) ;  // native

// Print a tryte value followed by a newline.
fn println_tryte(t: tryte) ;  // native

// Print a t9 (9-trit) value in balanced ternary notation.
fn print_t9(n: t9) ;  // native

// Print a t9 value followed by a newline.
fn println_t9(n: t9) ;  // native

// ---------------------------------------------------------------------------
// Stream handles
// ---------------------------------------------------------------------------

// Opaque handle to the process standard input stream.
struct Stdin {
    // native handle — do not construct directly; use stdin()
}

// Opaque handle to the process standard output stream.
struct Stdout {
    // native handle — do not construct directly; use stdout()
}

// Opaque handle to the process standard error stream.
struct Stderr {
    // native handle — do not construct directly; use stderr()
}

impl Stdin {
    // Read one line from this stream (newline stripped).
    fn read_line(self) -> str ;  // native

    // Read one line and parse as int.
    fn read_int(self) -> int ;  // native

    // Read one line and parse as float.
    fn read_float(self) -> float ;  // native

    // Read until EOF, returning all content as a single str.
    fn read_all(self) -> str ;  // native
}

impl Stdout {
    // Write a string to this stream.
    fn write(self, s: str) ;  // native

    // Write a string followed by a newline.
    fn writeln(self, s: str) ;  // native

    // Flush any buffered data to the underlying OS stream.
    fn flush(self) ;  // native
}

impl Stderr {
    // Write a string to this stream.
    fn write(self, s: str) ;  // native

    // Write a string followed by a newline.
    fn writeln(self, s: str) ;  // native

    // Flush any buffered data to the underlying OS stream.
    fn flush(self) ;  // native
}

// Return the process stdin handle.
fn stdin() -> Stdin ;  // native

// Return the process stdout handle.
fn stdout() -> Stdout ;  // native

// Return the process stderr handle.
fn stderr() -> Stderr ;  // native

// ---------------------------------------------------------------------------
// Formatted output helpers (thin wrappers — see std::fmt for richer API)
// ---------------------------------------------------------------------------

// Print an integer in decimal followed by a newline.
fn println_int(n: int) ;  // native

// Print a float in decimal followed by a newline.
fn println_float(f: float) ;  // native

// Print a bool (true/false) followed by a newline.
fn println_bool(b: bool) ;  // native
