// stdlib/std/sync.mt
// Synchronisation primitives for maniT.
//
// Provides mutually exclusive locks, reader-writer locks, channels, counting
// semaphores, barriers, and — unique to maniT — AtomicTrit for lock-free
// operations on single trit values.
//
// All types in this module are safe to share across threads spawned with
// `spawn { }`.  Ownership follows maniT's affine type rules.
//
// Usage:
//   use std::sync;
//   let m: Mutex<int> = Mutex::new(0);

// ---------------------------------------------------------------------------
// Mutex<T> — exclusive mutual exclusion lock
// ---------------------------------------------------------------------------

// A mutual-exclusion lock protecting a value of type T.
// Only one thread may hold the lock at a time.
struct Mutex<T> {
    // native OS or spinlock primitive + T slot
}

impl<T> Mutex<T> {
    // Create a new Mutex wrapping `val`.
    fn new(val: T) -> Mutex<T> { /* native */ }

    // Block until the lock can be acquired, then return a MutexGuard.
    // The guard releases the lock automatically when it goes out of scope.
    fn lock(self) -> MutexGuard<T> { /* native */ }

    // Attempt to acquire the lock without blocking.
    // Returns Ok(guard) if successful, Err("locked") if already held.
    fn try_lock(self) -> Result<MutexGuard<T>, str> { /* native */ }

    // Consume the Mutex and return the inner value (only valid when no guards
    // exist, i.e. called from the sole owner).
    fn into_inner(self) -> T { /* native */ }
}

// Guard returned by Mutex::lock.  Dereference to access the inner value.
struct MutexGuard<T> {
    // native — holds lock until dropped
}

impl<T> MutexGuard<T> {
    // Read the protected value.
    fn get(self) -> T { /* native */ }

    // Write a new value into the protected slot.
    fn set(self, val: T) { /* native */ }

    // Modify the protected value in place using a function.
    fn update(self, f: fn(T) -> T) { /* native */ }

    // Release the lock immediately (normally happens automatically on drop).
    fn unlock(self) { /* native */ }
}

// ---------------------------------------------------------------------------
// RwLock<T> — reader-writer lock
// ---------------------------------------------------------------------------

// A lock allowing many concurrent readers OR one exclusive writer.
struct RwLock<T> {
    // native RwLock primitive + T slot
}

impl<T> RwLock<T> {
    // Create a new RwLock wrapping `val`.
    fn new(val: T) -> RwLock<T> { /* native */ }

    // Acquire a shared read lock.  Multiple readers can hold this simultaneously.
    fn read(self) -> ReadGuard<T> { /* native */ }

    // Acquire an exclusive write lock.  Blocks until all readers/writers finish.
    fn write(self) -> WriteGuard<T> { /* native */ }

    // Non-blocking read attempt.
    fn try_read(self) -> Result<ReadGuard<T>, str> { /* native */ }

    // Non-blocking write attempt.
    fn try_write(self) -> Result<WriteGuard<T>, str> { /* native */ }
}

// Shared read guard — allows reading but not writing.
struct ReadGuard<T> {
    // native — holds read lock until dropped
}

impl<T> ReadGuard<T> {
    fn get(self) -> T { /* native */ }
    fn release(self) { /* native */ }
}

// Exclusive write guard — allows reading and writing.
struct WriteGuard<T> {
    // native — holds write lock until dropped
}

impl<T> WriteGuard<T> {
    fn get(self) -> T { /* native */ }
    fn set(self, val: T) { /* native */ }
    fn update(self, f: fn(T) -> T) { /* native */ }
    fn release(self) { /* native */ }
}

// ---------------------------------------------------------------------------
// Channel<T> — multi-producer, single-consumer message channel
// ---------------------------------------------------------------------------

// An unbounded MPSC channel.  Any number of senders can push values;
// the receiver end pops them in FIFO order.
//
// `channel<T>()` (a language built-in) returns a Channel<T>.
// This struct documents the methods available on channel values.
struct Channel<T> {
    // native queue + waker mechanism
}

impl<T> Channel<T> {
    // Create a new unbounded channel.
    fn new() -> Channel<T> { /* native */ }

    // Create a bounded channel that blocks senders when full.
    fn bounded(capacity: int) -> Channel<T> { /* native */ }

    // Send a value.  For bounded channels, blocks if the channel is full.
    fn send(self, val: T) { /* native */ }

    // Try to send without blocking.  Returns Err if the channel is full.
    fn try_send(self, val: T) -> Result<void, str> { /* native */ }

    // Receive the next value.  Blocks until a value is available.
    fn recv(self) -> T { /* native */ }

    // Try to receive without blocking.  Returns Err("empty") if no value
    // is immediately available.
    fn try_recv(self) -> Result<T, str> { /* native */ }

    // Receive with a timeout of `ms` milliseconds.
    // Returns Err("timeout") if no value arrives within the deadline.
    fn recv_timeout(self, ms: int) -> Result<T, str> { /* native */ }

    // Return the current number of values waiting in the channel.
    fn len(self) -> int { /* native */ }

    // Return true if the channel currently holds no values.
    fn is_empty(self) -> bool { /* native */ }

    // Close the channel.  Subsequent sends panic; recv drains remaining items.
    fn close(self) { /* native */ }

    // Return true if the channel has been closed and drained.
    fn is_closed(self) -> bool { /* native */ }
}

// ---------------------------------------------------------------------------
// Semaphore — counting semaphore
// ---------------------------------------------------------------------------

// A counting semaphore that controls access to a pool of `n` permits.
struct Semaphore {
    // native semaphore counter
}

impl Semaphore {
    // Create a semaphore with `permits` initial permits.
    fn new(permits: int) -> Semaphore { /* native */ }

    // Acquire one permit, blocking if none are available.
    fn acquire(self) { /* native */ }

    // Acquire `n` permits, blocking until all are available.
    fn acquire_n(self, n: int) { /* native */ }

    // Release one permit back to the pool.
    fn release(self) { /* native */ }

    // Release `n` permits back to the pool.
    fn release_n(self, n: int) { /* native */ }

    // Try to acquire one permit without blocking.
    fn try_acquire(self) -> bool { /* native */ }

    // Return the current permit count.
    fn available(self) -> int { /* native */ }
}

// ---------------------------------------------------------------------------
// Barrier — synchronise a fixed number of threads at a rendezvous point
// ---------------------------------------------------------------------------

// All threads must call wait() before any of them are allowed to proceed.
struct Barrier {
    // native barrier implementation
}

impl Barrier {
    // Create a Barrier for `n` threads.
    fn new(n: int) -> Barrier { /* native */ }

    // Block until `n` threads have all called wait().
    // Returns true in exactly one of the threads (the "leader"), false in all
    // others — useful for performing once-only setup after the barrier.
    fn wait(self) -> bool { /* native */ }

    // Return the number of threads this barrier is waiting for.
    fn count(self) -> int { /* native */ }
}

// ---------------------------------------------------------------------------
// AtomicTrit — lock-free trit (unique to maniT)
// ---------------------------------------------------------------------------

// A trit value that can be read and written atomically across threads.
// Uses CPU atomic instructions where available, otherwise a spinlock.
//
// AtomicTrit is the ternary analogue of AtomicBool / AtomicInt.
// It is particularly useful for shared state flags that naturally have three
// states (e.g. Unstarted / Running / Done mapped to - / 0 / +).
struct AtomicTrit {
    // native atomic storage
}

impl AtomicTrit {
    // Create an AtomicTrit initialised to `val`.
    fn new(val: trit) -> AtomicTrit { /* native */ }

    // Load the current value with sequentially-consistent ordering.
    fn load(self) -> trit { /* native */ }

    // Store `val` with sequentially-consistent ordering.
    fn store(self, val: trit) { /* native */ }

    // Atomically swap the value, returning the old value.
    fn swap(self, val: trit) -> trit { /* native */ }

    // Compare-and-swap: if the current value equals `expected`, store `new_val`
    // and return true.  Otherwise return false without modifying the value.
    fn compare_exchange(self, expected: trit, new_val: trit) -> bool { /* native */ }

    // Atomically apply ternary AND (min) and return the old value.
    fn fetch_and(self, val: trit) -> trit { /* native */ }

    // Atomically apply ternary OR (max) and return the old value.
    fn fetch_or(self, val: trit) -> trit { /* native */ }

    // Atomically negate the trit (flip sign) and return the old value.
    fn fetch_neg(self) -> trit { /* native */ }
}

// ---------------------------------------------------------------------------
// AtomicInt — lock-free integer
// ---------------------------------------------------------------------------

// A plain integer that can be read/written atomically.
struct AtomicInt {
    // native atomic storage
}

impl AtomicInt {
    fn new(val: int) -> AtomicInt { /* native */ }
    fn load(self) -> int { /* native */ }
    fn store(self, val: int) { /* native */ }
    fn swap(self, val: int) -> int { /* native */ }
    fn fetch_add(self, val: int) -> int { /* native */ }
    fn fetch_sub(self, val: int) -> int { /* native */ }
    fn compare_exchange(self, expected: int, new_val: int) -> bool { /* native */ }
}
