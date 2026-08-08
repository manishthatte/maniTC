// Test: concurrency — channels, spawn, AtomicTrit, Mutex,
//        producer-consumer, barrier synchronisation patterns
//        (cooperative single-threaded scheduler)
use std::io;
use std::sync;
use std::async;

fn pass(label: str) { io::print("PASS "); io::println(label); }
fn fail(label: str) { io::print("FAIL "); io::println(label); }
fn check(label: str, cond: bool) { if cond { pass(label) } else { fail(label) } }
fn check_int(label: str, got: int, want: int) {
    if got == want { pass(label); }
    else {
        io::print("FAIL "); io::print(label);
        io::print(" got="); io::print_int(got);
        io::print(" want="); io::print_int(want);
        io::newline();
    }
}

// ---------------------------------------------------------------------------
// 1. Basic channel send/receive
// ---------------------------------------------------------------------------

fn test_channel_basic() {
    let ch = channel<int>();

    let producer = spawn {
        ch.send(42);
    };

    let consumer = spawn {
        let v = ch.recv();
        check_int("chan-basic: received 42", v, 42);
    };

    producer.await;
    consumer.await;
    pass("chan-basic: completed");
}

// ---------------------------------------------------------------------------
// 2. Multiple sends
// ---------------------------------------------------------------------------

fn test_channel_multi_send() {
    let ch = channel<int>();

    let prod = spawn {
        let mut i: int = 0;
        while i < 5 {
            ch.send(i * i);
            i = i + 1;
        }
    };

    let cons = spawn {
        let mut sum: int = 0;
        let mut i: int = 0;
        while i < 5 {
            sum = sum + ch.recv();
            i = i + 1;
        }
        // 0+1+4+9+16 = 30
        check_int("chan-multi: sum of squares=30", sum, 30);
    };

    prod.await;
    cons.await;
}

// ---------------------------------------------------------------------------
// 3. AtomicTrit: lifecycle flag
// ---------------------------------------------------------------------------

fn test_atomic_trit() {
    let status: AtomicTrit = AtomicTrit::new(0);   // 0 = idle

    let worker = spawn {
        status.set(-1);   // -1 = starting
        async::yield_now();
        status.set(0);    // 0 = working
        async::yield_now();
        status.set(1);    // +1 = done
    };

    worker.await;

    let final_status = status.get();
    check_int("atomic-trit: done=+1", final_status, 1);
}

fn test_atomic_trit_all_states() {
    let a: AtomicTrit = AtomicTrit::new(-1);
    check_int("atomic-trit: init -1", a.get(), -1);

    a.set(0);
    check_int("atomic-trit: set 0", a.get(), 0);

    a.set(1);
    check_int("atomic-trit: set +1", a.get(), 1);

    a.set(-1);
    check_int("atomic-trit: set -1 again", a.get(), -1);
}

// ---------------------------------------------------------------------------
// 4. Mutex: shared counter
// ---------------------------------------------------------------------------

fn test_mutex_counter() {
    let counter: Mutex<int> = Mutex::new(0);

    let t1 = spawn {
        let mut i: int = 0;
        while i < 5 {
            counter.lock();
            let v = counter.get();
            counter.set(v + 1);
            counter.unlock();
            async::yield_now();
            i = i + 1;
        }
    };

    let t2 = spawn {
        let mut i: int = 0;
        while i < 5 {
            counter.lock();
            let v = counter.get();
            counter.set(v + 1);
            counter.unlock();
            async::yield_now();
            i = i + 1;
        }
    };

    t1.await;
    t2.await;

    counter.lock();
    let final_val = counter.get();
    counter.unlock();
    check_int("mutex: counter=10 after 2x5 increments", final_val, 10);
}

// ---------------------------------------------------------------------------
// 5. Producer-consumer with accumulation
// ---------------------------------------------------------------------------

fn test_producer_consumer() {
    let ch = channel<int>();
    let result: Mutex<int> = Mutex::new(0);

    let producer = spawn {
        let mut n: int = 1;
        while n <= 10 {
            ch.send(n);
            async::yield_now();
            n = n + 1;
        }
    };

    let consumer = spawn {
        let mut i: int = 0;
        while i < 10 {
            let v = ch.recv();
            result.lock();
            let cur = result.get();
            result.set(cur + v);
            result.unlock();
            i = i + 1;
        }
    };

    producer.await;
    consumer.await;

    result.lock();
    let total = result.get();
    result.unlock();
    // 1+2+...+10 = 55
    check_int("prod-cons: sum 1..10=55", total, 55);
}

// ---------------------------------------------------------------------------
// 6. Spawn with no communication (fire-and-forget)
// ---------------------------------------------------------------------------

fn test_spawn_no_comm() {
    let done: AtomicTrit = AtomicTrit::new(-1);

    let task = spawn {
        let mut i: int = 0;
        while i < 100 {
            i = i + 1;
        }
        done.set(1);
    };

    task.await;
    check_int("spawn-no-comm: task completed", done.get(), 1);
}

// ---------------------------------------------------------------------------
// 7. Channel: pass string messages
// ---------------------------------------------------------------------------

fn test_channel_strings() {
    let ch = channel<str>();

    let prod = spawn {
        ch.send("hello");
        ch.send("world");
        ch.send("done");
    };

    let cons = spawn {
        let s1 = ch.recv();
        let s2 = ch.recv();
        let s3 = ch.recv();
        check("chan-str: s1=hello",  s1 == "hello");
        check("chan-str: s2=world",  s2 == "world");
        check("chan-str: s3=done",   s3 == "done");
    };

    prod.await;
    cons.await;
}

// ---------------------------------------------------------------------------
// 8. Mutex: protect a Vec
// ---------------------------------------------------------------------------

fn test_mutex_vec() {
    let shared: Mutex<Vec<int>> = Mutex::new(Vec::new());

    let t1 = spawn {
        let mut i: int = 0;
        while i < 3 {
            shared.lock();
            let v = shared.get();
            v.push(i);
            shared.unlock();
            async::yield_now();
            i = i + 1;
        }
    };

    let t2 = spawn {
        let mut i: int = 10;
        while i < 13 {
            shared.lock();
            let v = shared.get();
            v.push(i);
            shared.unlock();
            async::yield_now();
            i = i + 1;
        }
    };

    t1.await;
    t2.await;

    shared.lock();
    let v = shared.get();
    let len = v.len();
    shared.unlock();
    check_int("mutex-vec: 6 items added", len, 6);
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() {
    io::println("=== 16 Concurrency Basic ===");

    io::println("-- channel basic --");
    test_channel_basic();

    io::println("-- channel multi-send --");
    test_channel_multi_send();

    io::println("-- AtomicTrit --");
    test_atomic_trit();
    test_atomic_trit_all_states();

    io::println("-- Mutex counter --");
    test_mutex_counter();

    io::println("-- producer-consumer --");
    test_producer_consumer();

    io::println("-- spawn no-comm --");
    test_spawn_no_comm();

    io::println("-- channel strings --");
    test_channel_strings();

    io::println("-- mutex vec --");
    test_mutex_vec();

    io::println("Done.");
}
