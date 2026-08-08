// examples/concurrency.mt
// Concurrency and async patterns in maniT.
//
// Demonstrates:
//   - spawn { } for lightweight tasks
//   - channel<T>() for message passing
//   - std::sync: Mutex<T>, AtomicTrit, Barrier, Semaphore
//   - async fn and await
//   - std::async: spawn_task, join_all, timeout, select
//   - A producer-consumer pipeline
//   - Fan-out / fan-in aggregation

use std::io;
use std::fmt;
use std::sync;
use std::async;
use std::time;

// ---------------------------------------------------------------------------
// 1. Basic channel and spawn
// ---------------------------------------------------------------------------
fn demo_basic_channel() {
    io::println("--- 1. Basic channel + spawn ---");

    // Create an unbounded channel.
    let ch = channel<int>();

    // Spawn a producer task that sends five values.
    spawn {
        mut i = 1;
        while i <= 5 {
            io::print("  producer sends: ");
            io::println_int(i);
            ch.send(i);
            i = i + 1;
        }
        ch.close();
    }

    // Receive in the main task until the channel is drained.
    mut sum = 0;
    loop {
        match ch.try_recv() {
            Ok(v)      => { io::print("  consumer got:  "); io::println_int(v); sum = sum + v; }
            Err("closed") => { break; }
            Err(_)     => { async::yield_now(); }  // channel empty but not yet closed
        }
    }

    io::print("  sum = ");
    io::println_int(sum);   // 15
    io::println("");
}

// ---------------------------------------------------------------------------
// 2. Producer-consumer pipeline
// ---------------------------------------------------------------------------
// Two producers each put N items onto a shared channel.
// One consumer reads all items and aggregates the total.
fn demo_producer_consumer() {
    io::println("--- 2. Producer-consumer pipeline ---");

    let work_ch  = channel<int>();
    let result_ch = channel<int>();

    let n_per_producer = 10;

    // Producer A: sends 1..n
    spawn {
        mut v = 1;
        while v <= n_per_producer {
            work_ch.send(v);
            v = v + 1;
        }
    }

    // Producer B: sends n+1..2n
    spawn {
        mut v = n_per_producer + 1;
        while v <= n_per_producer * 2 {
            work_ch.send(v);
            v = v + 1;
        }
    }

    // Consumer: collects 2*n items, then sends aggregate.
    let total_items = n_per_producer * 2;
    spawn {
        mut acc = 0;
        mut received = 0;
        while received < total_items {
            let item = work_ch.recv();
            acc = acc + item;
            received = received + 1;
        }
        result_ch.send(acc);
    }

    let total = result_ch.recv();
    io::print("  Sum of 1..");
    io::print_int(total_items);
    io::print(" = ");
    io::println_int(total);   // 210
    io::println("");
}

// ---------------------------------------------------------------------------
// 3. Mutex-protected shared counter
// ---------------------------------------------------------------------------
fn demo_mutex() {
    io::println("--- 3. Mutex<int> shared counter ---");

    let counter: Mutex<int> = Mutex::new(0);
    let done_ch = channel<void>();

    let n_threads = 9;   // ternary-friendly number

    mut t = 0;
    while t < n_threads {
        let tid = t;
        let counter_ref = counter;  // each task captures a reference
        spawn {
            mut j = 0;
            while j < 100 {
                let guard = counter_ref.lock();
                guard.update(fn(v: int) -> int => v + 1);
                guard.unlock();
                j = j + 1;
            }
            done_ch.send(());
        }
        t = t + 1;
    }

    // Wait for all threads to finish.
    mut finished = 0;
    while finished < n_threads {
        done_ch.recv();
        finished = finished + 1;
    }

    let final_count = counter.lock().get();
    io::print("  Final counter value (expected ");
    io::print_int(n_threads * 100);
    io::print("): ");
    io::println_int(final_count);
    io::println("");
}

// ---------------------------------------------------------------------------
// 4. AtomicTrit — lock-free ternary state flag
// ---------------------------------------------------------------------------
// Models a task lifecycle: - (not started) -> 0 (running) -> + (done).
fn demo_atomic_trit() {
    io::println("--- 4. AtomicTrit lifecycle flag ---");

    let status: AtomicTrit = AtomicTrit::new(-);   // not started

    io::print("  Initial status: ");
    io::println_trit(status.load());

    spawn {
        // Transition: not started -> running
        status.store(0);
        io::println("  [task] running...");
        time::sleep(1);   // simulate work (1 ms)
        // Transition: running -> done
        status.store(+);
        io::println("  [task] done.");
    }

    // Poll until done.
    loop {
        let s = status.load();
        tif s {
            + => { io::println("  [main] task completed."); break; }
            0 => { io::print("  [main] task is running... "); io::println_trit(s); async::yield_now(); }
            - => { io::println("  [main] task not yet started."); async::yield_now(); }
        }
    }
    io::println("");
}

// ---------------------------------------------------------------------------
// 5. Barrier — synchronise N tasks at a checkpoint
// ---------------------------------------------------------------------------
fn demo_barrier() {
    io::println("--- 5. Barrier (3-thread synchronisation) ---");

    let n = 3;
    let barrier = Barrier::new(n);
    let done_ch  = channel<int>();

    mut i = 0;
    while i < n {
        let tid = i;
        spawn {
            io::print("  thread ");
            io::print_int(tid);
            io::println(" before barrier");

            // All three threads meet here before any proceeds.
            let is_leader = barrier.wait();

            if is_leader {
                io::print("  thread ");
                io::print_int(tid);
                io::println(" is the barrier leader");
            }

            io::print("  thread ");
            io::print_int(tid);
            io::println(" after barrier");
            done_ch.send(tid);
        }
        i = i + 1;
    }

    mut finished = 0;
    while finished < n {
        done_ch.recv();
        finished = finished + 1;
    }
    io::println("  All threads passed the barrier.");
    io::println("");
}

// ---------------------------------------------------------------------------
// 6. async fn + await
// ---------------------------------------------------------------------------
async fn fetch_data(id: int) -> str {
    // Simulate a network fetch with a small delay.
    async::sleep(id * 2);
    let msg = fmt::format("data-{}", [fmt::show_int(id)]);
    return msg;
}

async fn process(id: int) -> int {
    let data = fetch_data(id).await;
    io::print("  processed: ");
    io::println(data);
    return id * id;
}

fn demo_async_await() {
    io::println("--- 6. async/await fan-out ---");

    // Launch 5 async tasks concurrently.
    let mut handles: Vec<Task<int>> = Vec::new();
    mut n = 1;
    while n <= 5 {
        handles.push(async::spawn_task(process(n)));
        n = n + 1;
    }

    // Wait for all tasks; collect results.
    let mut results: Vec<int> = Vec::new();
    mut k = 0;
    while k < handles.len() {
        let val = handles.get(k).join();
        results.push(val);
        k = k + 1;
    }

    io::print("  Squares: ");
    mut r = 0;
    while r < results.len() {
        if r > 0 { io::print(", "); }
        io::print_int(results.get(r));
        r = r + 1;
    }
    io::println("");
    io::println("");
}

// ---------------------------------------------------------------------------
// 7. Semaphore — rate-limit concurrent workers
// ---------------------------------------------------------------------------
fn demo_semaphore() {
    io::println("--- 7. Semaphore (max 3 concurrent workers) ---");

    // Allow at most 3 tasks to run simultaneously.
    let sem = Semaphore::new(3);
    let done_ch = channel<int>();
    let n_workers = 9;

    mut w = 0;
    while w < n_workers {
        let wid = w;
        let sem_ref = sem;
        spawn {
            sem_ref.acquire();
            io::print("  worker ");
            io::print_int(wid);
            io::println(" acquired semaphore");
            time::sleep(5);   // simulate work
            io::print("  worker ");
            io::print_int(wid);
            io::println(" releasing semaphore");
            sem_ref.release();
            done_ch.send(wid);
        }
        w = w + 1;
    }

    mut done_count = 0;
    while done_count < n_workers {
        let wid = done_ch.recv();
        done_count = done_count + 1;
    }
    io::println("  All workers finished.");
    io::println("");
}

// ---------------------------------------------------------------------------
// 8. select — first of two async operations
// ---------------------------------------------------------------------------
async fn slow_task(label: str, delay_ms: int) -> str {
    async::sleep(delay_ms);
    return label;
}

fn demo_select() {
    io::println("--- 8. select (first-to-complete wins) ---");

    let fut_a = slow_task("task-A (100ms)", 100);
    let fut_b = slow_task("task-B  (10ms)",  10);

    let futs: Vec<Future<str>> = Vec::new();
    futs.push(fut_a);
    futs.push(fut_b);

    let (winner_idx, winner_val) = async::select(futs).block_on();
    io::print("  Winner: index=");
    io::print_int(winner_idx);
    io::print(", value=\"");
    io::print(winner_val);
    io::println("\"");
    io::println("");
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------
fn main() {
    io::println("maniT Concurrency Demo");
    io::println("======================");
    io::println("");

    demo_basic_channel();
    demo_producer_consumer();
    demo_mutex();
    demo_atomic_trit();
    demo_barrier();
    demo_async_await();
    demo_semaphore();
    demo_select();

    io::println("All demos complete.");
}
