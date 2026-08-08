// stdlib/std/async.mt
// Asynchronous programming primitives for maniT.
//
// maniT uses a cooperative, work-stealing async runtime based on lightweight
// green threads (tasks).  The `async fn` keyword transforms a function into
// one that returns a Future<T>.  `await` suspends the current task until the
// Future resolves.
//
// The runtime is started automatically by `main()`.  For fine-grained control
// you can construct a Runtime explicitly and drive it manually.
//
// Usage:
//   use std::async;
//   let handle = async::spawn_task(my_async_fn());
//   let result = handle.await;

// ---------------------------------------------------------------------------
// Future<T>
// ---------------------------------------------------------------------------

// Represents a value of type T that will be produced asynchronously.
// A Future is "lazy": it makes no progress until awaited or polled by a task.
//
// Futures are single-use; once resolved the value is consumed.
struct Future<T> {
    // native poll state machine
}

impl<T> Future<T> {
    // Create an already-resolved Future wrapping `val`.
    fn ready(val: T) -> Future<T> { /* native */ }

    // Create a Future that never resolves (useful for select! timeouts).
    fn pending() -> Future<T> { /* native */ }

    // Block the current OS thread until the Future resolves and return its
    // value.  Do not call this inside an async context — use await instead.
    fn block_on(self) -> T { /* native */ }

    // Map the resolved value through `f`, returning a new Future.
    fn map<U>(self, f: fn(T) -> U) -> Future<U> { /* native */ }

    // Chain: when this Future resolves, pass the value to `f` to produce the
    // next Future (monadic bind / flat_map).
    fn then<U>(self, f: fn(T) -> Future<U>) -> Future<U> { /* native */ }

    // Return true if the Future has already resolved (non-blocking check).
    fn is_ready(self) -> bool { /* native */ }
}

// ---------------------------------------------------------------------------
// Task<T>
// ---------------------------------------------------------------------------

// A handle to a spawned asynchronous task.  Awaiting the handle blocks until
// the task completes and yields the task's return value.
struct Task<T> {
    // native task ID + result cell
}

impl<T> Task<T> {
    // Await the task's completion, returning its value.
    // This is equivalent to using the `await` keyword on the Task.
    fn join(self) -> T { /* native */ }

    // Attempt to get the result without suspending.
    // Returns Ok(val) if done, Err("pending") otherwise.
    fn try_join(self) -> Result<T, str> { /* native */ }

    // Cancel the task.  The task will be stopped at the next suspension point.
    fn cancel(self) { /* native */ }

    // Return true if the task has finished.
    fn is_done(self) -> bool { /* native */ }

    // Return the unique task identifier.
    fn id(self) -> int { /* native */ }
}

// ---------------------------------------------------------------------------
// Runtime
// ---------------------------------------------------------------------------

// The async executor.  Manages the thread pool and task scheduler.
struct Runtime {
    // native event loop + work-stealing queues
}

impl Runtime {
    // Create a Runtime using all available CPU cores.
    fn new() -> Runtime { /* native */ }

    // Create a Runtime with an explicit number of worker threads.
    fn with_threads(n: int) -> Runtime { /* native */ }

    // Create a single-threaded Runtime (useful for testing).
    fn single_threaded() -> Runtime { /* native */ }

    // Run `fut` to completion on this runtime, blocking the current thread.
    // Returns the value produced by `fut`.
    fn block_on<T>(self, fut: Future<T>) -> T { /* native */ }

    // Spawn a task on this runtime.  Returns a Task handle.
    fn spawn<T>(self, fut: Future<T>) -> Task<T> { /* native */ }

    // Shut down the runtime, waiting for all tasks to complete.
    fn shutdown(self) { /* native */ }

    // Shut down the runtime, cancelling all pending tasks immediately.
    fn shutdown_now(self) { /* native */ }

    // Return the number of worker threads in this runtime.
    fn thread_count(self) -> int { /* native */ }

    // Return the number of tasks currently queued or running.
    fn task_count(self) -> int { /* native */ }
}

// ---------------------------------------------------------------------------
// Free functions
// ---------------------------------------------------------------------------

// Spawn `fut` on the global runtime.  The global runtime is started the first
// time this function (or any async fn) is called.
fn spawn_task<T>(fut: Future<T>) -> Task<T> { /* native */ }

// Spawn multiple futures concurrently and await all of them.
// Returns a Vec of results in the same order as the input Vec.
fn join_all<T>(futs: Vec<Future<T>>) -> Vec<T> { /* native */ }

// Await two futures concurrently; return as soon as either completes.
// The other future is cancelled.
fn race<T>(a: Future<T>, b: Future<T>) -> T { /* native */ }

// Suspend the current task for `ms` milliseconds, then resume.
async fn sleep(ms: int) { /* native */ }

// Yield control back to the scheduler, allowing other tasks to run.
// Resumes on the next scheduler tick.
async fn yield_now() { /* native */ }

// Run `fut` with a timeout.  Returns Ok(val) if it finishes within `ms`
// milliseconds, or Err("timeout") if it does not.
async fn timeout<T>(ms: int, fut: Future<T>) -> Result<T, str> { /* native */ }

// Select the first future to complete from a Vec, returning its index and
// value.  All other futures are cancelled.
async fn select<T>(futs: Vec<Future<T>>) -> (int, T) { /* native */ }

// ---------------------------------------------------------------------------
// Async I/O utilities
// ---------------------------------------------------------------------------

// Read from an async reader into a buffer string, awaiting data arrival.
async fn read_async(reader: Stdin, n: int) -> str { /* native */ }

// Write a string to an async writer, awaiting completion.
async fn write_async(writer: Stdout, s: str) { /* native */ }
