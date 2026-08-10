// stdlib/std/time.mt
// Time and duration utilities for maniT.
//
// Provides wall-clock and monotonic timestamps, duration arithmetic, and
// timer utilities.  All durations are represented in integer nanoseconds
// internally; helper constructors accept more natural units.
//
// Usage:
//   use std::time;
//   let start = time::now();
//   // ... work ...
//   let elapsed = time::now().since(start);
//   io::println(elapsed.to_ms_str());

// ---------------------------------------------------------------------------
// Duration
// ---------------------------------------------------------------------------

// A span of time measured in nanoseconds.  Always non-negative.
struct Duration {
    // Number of nanoseconds.
    nanos: int,
}

impl Duration {
    // Construct from nanoseconds.
    fn from_nanos(ns: int) -> Duration ;  // native

    // Construct from microseconds.
    fn from_micros(us: int) -> Duration ;  // native

    // Construct from milliseconds.
    fn from_millis(ms: int) -> Duration ;  // native

    // Construct from whole seconds.
    fn from_secs(s: int) -> Duration ;  // native

    // Construct from minutes.
    fn from_mins(m: int) -> Duration ;  // native

    // Construct from hours.
    fn from_hours(h: int) -> Duration ;  // native

    // A zero-length Duration.
    fn zero() -> Duration ;  // native

    // Return the total nanoseconds.
    fn as_nanos(self) -> int ;  // native

    // Return total microseconds (truncated).
    fn as_micros(self) -> int ;  // native

    // Return total milliseconds (truncated).
    fn as_millis(self) -> int ;  // native

    // Return total whole seconds (truncated).
    fn as_secs(self) -> int ;  // native

    // Return total minutes (truncated).
    fn as_mins(self) -> int ;  // native

    // Return total hours (truncated).
    fn as_hours(self) -> int ;  // native

    // Return the sub-second nanosecond component (0–999_999_999).
    fn subsec_nanos(self) -> int ;  // native

    // Add two durations.
    fn add(self, other: Duration) -> Duration ;  // native

    // Subtract `other` from this duration.  Returns zero if other > self.
    fn sub(self, other: Duration) -> Duration ;  // native

    // Multiply duration by an integer factor.
    fn mul(self, n: int) -> Duration ;  // native

    // Divide duration by an integer divisor.
    fn div(self, n: int) -> Duration ;  // native

    // Return true if this duration is zero.
    fn is_zero(self) -> bool ;  // native

    // Compare: return + if self > other, 0 if equal, - if self < other.
    fn compare(self, other: Duration) -> trit ;  // native

    // Human-readable string: "2h 3m 5.123s", "450ms", "12µs", "900ns".
    fn to_str(self) -> str ;  // native

    // Format as a decimal millisecond count: "1234.567ms".
    fn to_ms_str(self) -> str ;  // native
}

// ---------------------------------------------------------------------------
// Instant — monotonic timestamp
// ---------------------------------------------------------------------------

// A point in time obtained from the system monotonic clock.
// Instants can only be compared with other Instants; they have no meaning
// as absolute wall-clock times (use DateTime for that).
struct Instant {
    // nanoseconds since an arbitrary, fixed epoch
    raw_nanos: int,
}

impl Instant {
    // Return the current instant from the monotonic clock.
    fn now() -> Instant ;  // native

    // Compute the duration elapsed since `earlier`.
    // Panics if `earlier` is later than `self`.
    fn since(self, earlier: Instant) -> Duration ;  // native

    // Compute the duration elapsed from `self` to `later`.
    // Panics if `later` is earlier than `self`.
    fn until(self, later: Instant) -> Duration ;  // native

    // Return how much time has elapsed since this instant was captured.
    fn elapsed(self) -> Duration ;  // native

    // Add a duration, returning a future Instant.
    fn add(self, d: Duration) -> Instant ;  // native

    // Subtract a duration, returning an earlier Instant.
    fn sub(self, d: Duration) -> Instant ;  // native

    // Compare two instants: + if self is later, 0 if equal, - if earlier.
    fn compare(self, other: Instant) -> trit ;  // native
}

// Return the current monotonic instant.  Shorthand for Instant::now().
fn now() -> Instant ;  // native

// ---------------------------------------------------------------------------
// DateTime — wall-clock calendar time
// ---------------------------------------------------------------------------

// A point in time expressed as a calendar date and time of day in UTC.
struct DateTime {
    year:   int,
    month:  int,   // 1–12
    day:    int,   // 1–31
    hour:   int,   // 0–23
    minute: int,   // 0–59
    second: int,   // 0–59
    nanos:  int,   // 0–999_999_999
}

impl DateTime {
    // Return the current UTC date and time.
    fn now_utc() -> DateTime ;  // native

    // Return the current local date and time.
    fn now_local() -> DateTime ;  // native

    // Construct from a Unix timestamp (seconds since 1970-01-01T00:00:00Z).
    fn from_unix(secs: int) -> DateTime ;  // native

    // Convert to a Unix timestamp.
    fn to_unix(self) -> int ;  // native

    // Format using a strftime-style pattern.
    // Supported tokens: %Y %m %d %H %M %S %f (nanoseconds) %z (UTC offset).
    fn format(self, pattern: str) -> str ;  // native

    // Parse a date-time string with the given strftime pattern.
    fn parse(s: str, pattern: str) -> DateTime ;  // native

    // ISO 8601 string: "2026-03-19T14:00:00Z".
    fn to_iso8601(self) -> str ;  // native

    // Compute the Duration between two DateTimes.
    fn since(self, earlier: DateTime) -> Duration ;  // native

    // Add a duration to this DateTime.
    fn add(self, d: Duration) -> DateTime ;  // native

    // Subtract a duration.
    fn sub(self, d: Duration) -> DateTime ;  // native
}

// ---------------------------------------------------------------------------
// Sleeping
// ---------------------------------------------------------------------------

// Block the current thread for `ms` milliseconds.
fn sleep(ms: int) ;  // native

// Block the current thread for the given Duration.
fn sleep_duration(d: Duration) ;  // native

// ---------------------------------------------------------------------------
// Timer — one-shot and repeating timers
// ---------------------------------------------------------------------------

// A timer that fires after a delay (one-shot) or repeatedly at an interval.
struct Timer {
    // native timer handle
}

impl Timer {
    // Create a timer that fires once after `ms` milliseconds.
    fn once(ms: int) -> Timer ;  // native

    // Create a timer that fires every `ms` milliseconds.
    fn repeating(ms: int) -> Timer ;  // native

    // Block until the timer fires.  For repeating timers, blocks until the
    // next tick and returns the number of ticks that have elapsed since the
    // last call (normally 1, but may be higher if the caller was delayed).
    fn wait(self) -> int ;  // native

    // Reset the timer.  For one-shot timers, allows them to fire again.
    fn reset(self) ;  // native

    // Cancel a repeating timer, preventing future ticks.
    fn cancel(self) ;  // native

    // Return true if the timer has already fired (non-blocking check).
    fn has_fired(self) -> bool ;  // native
}

// ---------------------------------------------------------------------------
// Benchmarking helper
// ---------------------------------------------------------------------------

// Measure the wall-clock time taken by `f` and return the Duration.
fn measure(f: fn()) -> Duration ;  // native
