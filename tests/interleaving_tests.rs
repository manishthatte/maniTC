//! §11 of `docs/semantics.md` — interleaving, against the A3 reference.
//!
//! © Manish Jagdish Thatte
//!
//! **Corrected 2 September 2026.** This header read *"These rows are about a
//! section that is AHEAD of both backends"* and said the backends "still run
//! `spawn` inline". That was true when written and stopped being true when
//! steps 2 and 3 landed: under `--sched cooperative` both backends implement
//! §11, and `structured_waiting_tests.rs` now runs the same programs on them.
//! The claim survived six days and three commits because a file header is not
//! something a test can fail on — which is this repository's own argument for
//! pinning documented claims rather than reviewing prose (permanent rule 6).
//!
//! What these rows still are, and what makes them worth keeping separate: they
//! pin the **A3 reference** against the DOCUMENT, rule by rule, with every
//! expectation derived by hand from §11.5. That is a third account, independent
//! of both backends — and the reason it stays independent is that it is the
//! only one of the three that cannot be wrong in the same way as the other two.
//! The rows remain correct as written; only the header's claim about the
//! backends was stale.
//!
//! Every expected trace below was derived by hand from §11.5's rules before it
//! was run, because a test whose expectation came from the implementation is a
//! record of what the implementation does and not of what the document says.

use manitc::reference;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

mod common;

fn obs(src: &str) -> (String, Option<String>) {
    match reference::interpret(src) {
        Ok(o) => (o.out, o.trap),
        Err(e) => panic!("reference refused a §11 program: {}", e),
    }
}

fn out_of(src: &str) -> String {
    let (out, trap) = obs(src);
    assert!(trap.is_none(), "unexpected trap: {:?} (trace so far {:?})", trap, out);
    out
}

// ---------------------------------------------------------------------------
// §11.5 (SPAWN)
// ---------------------------------------------------------------------------

/// (SPAWN) appends the new task at the BACK and the spawning task CONTINUES.
///
/// This is the single row that would fail against every implementation ManiT
/// has today, and it is the whole of report.txt P5: `spawn { B }` currently
/// lowers to `B`, so "inline" prints `in` first.
#[test]
fn spawn_appends_and_the_spawner_continues() {
    let out = out_of(
        r#"
        fn main() -> void {
            spawn { io::println("in"); }
            io::println("after");
        }"#,
    );
    assert_eq!(
        out, "after\nin\n",
        "§11.5 (SPAWN): the spawner continues and the task runs later. \
         `in` first would mean the block was evaluated in place, which is \
         docs/memory-model.md §4's behaviour, not §11's"
    );
}

/// §11.6. `main` returning does not end the program: it terminates as a task
/// and the rest run. Kept as its own row because it is the COMPATIBLE choice
/// rather than the obvious one — every spawned block in every existing program
/// completes today, and ending at `main` would silently discard that work.
#[test]
fn main_returning_does_not_end_the_program() {
    let out = out_of(
        r#"
        fn main() -> void {
            spawn { io::println("later"); }
            io::println("main done");
        }"#,
    );
    assert_eq!(out, "main done\nlater\n", "§11.6: the remaining tasks run");
}

// ---------------------------------------------------------------------------
// §11.5 (YIELD)
// ---------------------------------------------------------------------------

/// (YIELD) moves the head to the back — so three tasks that each yield once
/// produce a round robin, and the ORDER is the specification's, not an
/// artefact of which thread the operating system happened to wake.
#[test]
fn yield_moves_the_head_to_the_back() {
    let out = out_of(
        r#"
        fn main() -> void {
            spawn { io::println("t1-a"); yield; io::println("t1-b"); }
            spawn { io::println("t2-a"); yield; io::println("t2-b"); }
            io::println("main-a");
            yield;
            io::println("main-b");
        }"#,
    );
    assert_eq!(
        out, "main-a\nt1-a\nt2-a\nmain-b\nt1-b\nt2-b\n",
        "§11.5 (YIELD): R = [main,t1,t2] -> [t1,t2,main] -> [t2,main,t1] -> ..."
    );
}

/// §11.4: the list of yield points is COMPLETE, and a call is not on it.
/// A task that calls a function runs the whole call before anyone else moves.
#[test]
fn a_call_is_not_a_yield_point() {
    let out = out_of(
        r#"
        fn noise() -> void {
            io::println("f1");
            io::println("f2");
        }
        fn main() -> void {
            spawn { io::println("other"); }
            noise();
            io::println("main");
        }"#,
    );
    assert_eq!(
        out, "f1\nf2\nmain\nother\n",
        "§11.4: switching at a call or a return would interleave `other` here"
    );
}

// ---------------------------------------------------------------------------
// §11.5 (SEND), (SEND-WAKE), (RECV), (RECV-BLOCK)
// ---------------------------------------------------------------------------

/// A send hands a blocked receiver its value, and the receiver resumes where
/// it blocked.
#[test]
fn recv_blocks_and_a_send_wakes_it() {
    let out = out_of(
        r#"
        fn main() -> void {
            let c: chan = channel();
            spawn {
                io::println("B1");
                c.send(7);
                io::println("B2");
            }
            io::println("A1");
            let v: int = c.recv();
            io::print("got ");
            io::println_int(v);
        }"#,
    );
    assert_eq!(
        out, "A1\nB1\nB2\ngot 7\n",
        "§11.5: main blocks in (RECV-BLOCK), the task runs to completion \
         because (SEND) is not a yield point, and main resumes after it"
    );
}

/// §11.4: `send` is not a yield point, because §11.1 leaves channels
/// unbounded. If it were, `got 1` would print before `after-send`.
#[test]
fn send_does_not_yield() {
    let out = out_of(
        r#"
        fn main() -> void {
            let c: chan = channel();
            spawn { let v: int = c.recv(); io::print("got "); io::println_int(v); }
            yield;
            c.send(1);
            io::println("after-send");
        }"#,
    );
    assert_eq!(
        out, "after-send\ngot 1\n",
        "§11.4: the sender keeps running; the woken task is appended at the back"
    );
}

/// (SEND-WAKE) wakes **exactly one** waiter, and it is the longest-waiting.
///
/// Two receivers block in order; one value is sent; the FIRST gets it and the
/// second is still waiting when the run queue empties — so the program ends in
/// §11.6's deadlock. Waking both would print two lines and end normally, and
/// waking the wrong one would print `B 1`.
#[test]
fn send_wakes_exactly_one_waiter_and_it_is_the_longest_waiting() {
    let (out, trap) = obs(
        r#"
        fn main() -> void {
            let c: chan = channel();
            spawn { let v: int = c.recv(); io::print("A "); io::println_int(v); }
            spawn { let v: int = c.recv(); io::print("B "); io::println_int(v); }
            yield;
            c.send(1);
            io::println("sent");
        }"#,
    );
    assert_eq!(out, "sent\nA 1\n", "§11.5 (SEND-WAKE): one waiter, the longest-waiting");
    assert!(
        trap.as_deref().unwrap_or("").starts_with("deadlock"),
        "§11.6: the second receiver is still blocked when R empties, got {:?}",
        trap
    );
}

/// The row above is NOT enough, and finding that out is worth more than the
/// row itself.
///
/// Waking **all** waiters instead of one passes every other test in this file,
/// because a spuriously woken receiver simply re-executes its `recv`, finds
/// nothing and blocks again — **producing no output while it does it**. The
/// difference between (SEND-WAKE) and a wake-all is therefore invisible in any
/// program where the woken tasks are already in the run queue in the same
/// order.
///
/// What the two do differ in is the ORDER OF `B`, so the program has to make
/// that observable: block A then B, wake one of them, steal the value out from
/// under it, and let it go back to the END of `B`. Under (SEND-WAKE) `B` is
/// then `[B, A]` and the next send wakes **B**; under a wake-all both were
/// already released and re-blocked in their original order, `[A, B]`, and the
/// next send wakes **A**.
///
/// The general shape, which is the reason this comment is longer than the
/// test: **a rule about which of several waiting things is chosen is only
/// testable once the choice changes what is PRINTED**, and a waiter that goes
/// straight back to waiting prints nothing. The obvious test for "exactly one"
/// checks the count, and the count is not what leaks.
#[test]
fn send_wake_is_the_longest_waiting_and_not_merely_one_of_them() {
    let (out, trap) = obs(
        r#"
        fn main() -> void {
            let c: chan = channel();
            spawn { let v: int = c.recv(); io::print("A "); io::println_int(v); }
            spawn { let v: int = c.recv(); io::print("B "); io::println_int(v); }
            yield;
            c.send(1);
            let mine: int = c.recv();
            io::print("main ");
            io::println_int(mine);
            yield;
            yield;
            c.send(2);
            io::println("sent2");
        }"#,
    );
    assert_eq!(
        out, "main 1\nsent2\nB 2\n",
        "§11.5 (SEND-WAKE): after A is woken, robbed and re-blocked, B is the \
         longest-waiting and the second send must go to B. `A 2` here means \
         the first send released both waiters"
    );
    assert!(
        trap.as_deref().unwrap_or("").starts_with("deadlock"),
        "§11.6: whichever receiver did not get a value is still blocked, {:?}",
        trap
    );
}

// ---------------------------------------------------------------------------
// §11.6 deadlock
// ---------------------------------------------------------------------------

/// The finding that makes cooperative scheduling worth its cost, and the one
/// P5.1 measured the absence of: a deadlock is DETECTED, and the trace
/// produced before it survives.
///
/// On LLVM today the same shape blocks in `pthread_cond_wait` with stdout
/// unflushed and prints **nothing at all** — the answer and the trace are both
/// lost. §8 requires the trace to be retained, and this is that requirement
/// applied to §11.6's trap.
#[test]
fn deadlock_is_a_trap_and_the_trace_survives_it() {
    let (out, trap) = obs(
        r#"
        fn main() -> void {
            let c: chan = channel();
            io::println("before");
            let v: int = c.recv();
            io::println("after");
        }"#,
    );
    assert_eq!(out, "before\n", "§8: the trace up to the trap is retained");
    let t = trap.expect("§11.6: R empty and B non-empty is a trap");
    assert!(t.starts_with("deadlock"), "the message names the situation: {:?}", t);
    assert!(
        t.contains("no runnable task can fill"),
        "§11.6: it names WHY, not just that something stopped: {:?}",
        t
    );
}

/// The other end condition: `R` empty with `B` empty is a normal end, not a
/// trap. Kept beside its neighbour because the two are one line apart in
/// §11.6 and telling them apart is the whole of that section.
#[test]
fn an_empty_run_queue_with_nothing_blocked_ends_normally() {
    let (out, trap) = obs(
        r#"
        fn main() -> void {
            let c: chan = channel();
            spawn { c.send(3); }
            let v: int = c.recv();
            io::println_int(v);
        }"#,
    );
    assert_eq!(out, "3\n");
    assert_eq!(trap, None, "§11.6: every task finished, so the program ends normally");
}

// ---------------------------------------------------------------------------
// §11.2 tasks do not share a store
// ---------------------------------------------------------------------------

/// A spawned task gets a COPY of its spawner's store. This is what makes a
/// data race unreachable rather than undefined — with nothing shared, there is
/// nothing to race over even at a yield point.
#[test]
fn a_spawned_task_gets_a_copy_of_the_store() {
    let out = out_of(
        r#"
        fn main() -> void {
            let mut n: int = 1;
            spawn { n = 99; io::print("in "); io::println_int(n); }
            yield;
            io::print("out ");
            io::println_int(n);
        }"#,
    );
    assert_eq!(
        out, "in 99\nout 1\n",
        "§11.2: the task's write is its own; `out 99` would mean a shared store"
    );
}

/// ...and the channel survives the copy still naming the same channel, which
/// is why `Val::Chan` is an INDEX. If a channel were copied by value the two
/// tasks would hold different queues and this would deadlock.
#[test]
fn a_channel_survives_the_store_copy() {
    let out = out_of(
        r#"
        fn main() -> void {
            let c: chan = channel();
            spawn { c.send(42); }
            let v: int = c.recv();
            io::println_int(v);
        }"#,
    );
    assert_eq!(out, "42\n", "§11.3: a channel value is a name, not its contents");
}

// ---------------------------------------------------------------------------
// §11.7 determinism
// ---------------------------------------------------------------------------

/// §11.7's proposition, checked rather than asserted: the observation is a
/// function of the program.
///
/// This is the row that would catch the scheduler being implemented with real
/// concurrency — threads that actually run at once produce a different
/// interleaving now and then, and "now and then" is exactly what a single run
/// cannot see. The program is built to be maximally sensitive to it: three
/// tasks, two channels and interleaved yields.
#[test]
fn the_observation_is_deterministic() {
    let src = r#"
        fn main() -> void {
            let a: chan = channel();
            let b: chan = channel();
            spawn {
                io::println("p1");
                a.send(1);
                yield;
                io::println("p2");
                b.send(2);
            }
            spawn {
                let x: int = a.recv();
                io::print("c1 ");
                io::println_int(x);
                yield;
                let y: int = b.recv();
                io::print("c2 ");
                io::println_int(y);
            }
            io::println("m1");
            yield;
            io::println("m2");
        }"#;
    let first = obs(src);
    for i in 1..200 {
        let again = obs(src);
        assert_eq!(
            first, again,
            "§11.7: run {} disagreed with run 0 — the observation is not a \
             function of the program",
            i
        );
    }
}

// ---------------------------------------------------------------------------
// The regression guard
// ---------------------------------------------------------------------------

/// §11.3 makes every program a task pool, including the ones with no `spawn`
/// in them — a program that never spawns is the pool with one task in it, and
/// there is deliberately no second code path for it.
///
/// So the whole sequential language now runs through the scheduler, and this
/// row is here to say that costs it nothing. `conformance_tests.rs` is the
/// real evidence for that, over every core program it has; this is the cheap
/// one that names the reason.
#[test]
fn a_program_that_never_spawns_is_the_pool_with_one_task() {
    let (out, trap) = obs(
        r#"
        fn fib(n: int) -> int {
            if n < 2 { return n; }
            return fib(n - 1) + fib(n - 2);
        }
        fn main() -> void {
            io::println_int(fib(12));
            let x: int = 7 / 2;
            io::println_int(x);
        }"#,
    );
    assert_eq!(out, "144\n3\n");
    assert_eq!(trap, None);
}

/// A trap inside a task ends the whole program (§8), and the trace survives.
#[test]
fn a_trap_in_a_task_ends_the_program() {
    let (out, trap) = obs(
        r#"
        fn main() -> void {
            spawn {
                io::println("task");
                let z: int = 0;
                let boom: int = 1 / z;
                io::println("unreachable");
            }
            io::println("main");
        }"#,
    );
    assert_eq!(out, "main\ntask\n", "§8: the trace up to the trap is retained");
    assert!(trap.is_some(), "§8: a trap in any task ends the program");
    assert!(
        !out.contains("unreachable"),
        "the trapping task stopped where it trapped"
    );
}

// ---------------------------------------------------------------------------
// §1.2 — the gap, asserted rather than left silent
// ---------------------------------------------------------------------------

static N: AtomicUsize = AtomicUsize::new(0);

fn t3_trace(src: &str) -> String {
    let slot = N.fetch_add(1, Ordering::Relaxed);
    let d = common::suite_root("il")
        .join(slot.to_string());
    std::fs::create_dir_all(&d).expect("temp dir");
    let path = d.join("gap.mt");
    std::fs::write(&path, src).expect("write");
    let base = path.with_extension("");
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_manitc"));

    let c = Command::new(&bin)
        .args(["compile", path.to_str().unwrap(), "--target", "t3",
               "-o", base.to_str().unwrap()])
        .output()
        .expect("compile");
    assert!(
        c.status.success(),
        "T3 refused a §11 program — the gap this row asserts has CHANGED SHAPE. \
         It used to accept `spawn` and run it inline.\n{}\n{}",
        String::from_utf8_lossy(&c.stdout),
        String::from_utf8_lossy(&c.stderr)
    );
    let r = Command::new(&bin)
        .args(["run-t3", base.with_extension("t3b").to_str().unwrap()])
        .output()
        .expect("run");
    String::from_utf8_lossy(&r.stdout)
        .lines()
        .filter(|l| !l.starts_with("[T3ISA]") && !l.starts_with("TRAP:"))
        .map(|l| format!("{}\n", l))
        .collect()
}

/// **This row asserts a DISAGREEMENT, and it is meant to.**
///
/// §1.2: §11 is ahead of every implementation, and a specification's lead over
/// its implementations has to be recorded somewhere that cannot go quiet. The
/// alternatives are both worse — a conformance row that simply fails reads as
/// a regression, and one that is skipped says nothing at all.
///
/// So: the same program, to the reference and to T3, with the answers pinned
/// **in both directions**. Today T3 runs `spawn` inline
/// (`ir/lower/lower_expr.rs`, `TypedExprKind::Spawn` → `lower_block`), which is
/// exactly what `docs/memory-model.md` §4 says the language does and what
/// report.txt P5 records as the defect.
///
/// **Corrected 2 September 2026.** This paragraph used to read *"When step 2 of
/// `CONCURRENCY_DECISION.md` §5 lands, this row goes red — and that is the
/// signal it exists to give."* **Step 2 landed on 30 August and the row did not
/// go red**, because it compiles WITHOUT `--sched cooperative` and so measures
/// the DEFAULT mode, which still runs `spawn { B }` in place and still will.
///
/// The assertion was right the whole time; only the prediction about it was
/// wrong. The row is not a temporary gap-marker after all — it is the pin on
/// `--sched inline`'s semantics, which `docs/memory-model.md` §4 still governs
/// and which every program compiled without the flag still gets. It goes red
/// the day cooperative becomes the DEFAULT, and that is the signal it now
/// gives.
///
/// *A test that predicts when it will fail is making a claim like any other,
/// and this one was falsified without anybody noticing, because being wrong
/// looked exactly like still being right.*
#[test]
fn t3_does_not_implement_11_yet_and_this_says_so() {
    let src = r#"
        fn main() -> void {
            spawn { io::println("in"); }
            io::println("after");
        }"#;

    let reference = out_of(src);
    assert_eq!(
        reference, "after\nin\n",
        "§11.5 (SPAWN): the reference implements the specification"
    );

    let t3 = t3_trace(src);
    assert_eq!(
        t3, "in\nafter\n",
        "T3 runs the spawned block IN PLACE — docs/memory-model.md §4, \
         report.txt P5. If this now reads `after\\nin\\n`, T3 has learned §11: \
         move this program into conformance_tests.rs, where three-way \
         agreement is the assertion, and delete this row."
    );

    assert_ne!(
        reference, t3,
        "the gap §1.2 requires to be recorded has closed on one side only"
    );
}

// ---------------------------------------------------------------------------
// §11.10 — closing a channel, against the A3 reference
// ---------------------------------------------------------------------------
//
// Every expectation below is derived by hand from §11.10's rules, as this
// file requires. The reference is the account that had to CATCH UP here — both
// backends implemented `close` before §11 was written — which is the reverse
// of §11.9 and the reason these rows exist at all.

/// (CLOSE) wakes EVERY waiter, in `B(c)`'s own order.
///
/// The one place in §11 where all are woken rather than one. Derived: two
/// SPAWNs append A then B; (YIELD) sends main to the back; A blocks, then B
/// blocks, leaving `B(ch) = [A, B]`; main's close appends both to `R` in that
/// order, then main terminates by (DONE).
///
/// **Against a wake-one close this row does not merely print the wrong order —
/// it deadlock-traps**, because the second waiter is stranded and no `send`
/// can ever reach it. That is the opposite of (SEND-WAKE), where §11.7 records
/// that a wake-all is nearly invisible; here a wake-one is loud.
#[test]
fn s11_10_close_wakes_every_waiter() {
    let out = out_of(
        "fn main() -> void {\n\
         \x20  let ch = channel();\n\
         \x20  spawn { io::println(\"A\"); let a = ch.recv(); io::println(\"A done\"); }\n\
         \x20  spawn { io::println(\"B\"); let b = ch.recv(); io::println(\"B done\"); }\n\
         \x20  yield;\n\
         \x20  io::println(\"close\");\n\
         \x20  ch.close();\n\
         }",
    );
    assert_eq!(out, "A\nB\nclose\nA done\nB done\n", "§11.10 (CLOSE)");
}

/// (RECV) still drains a closed channel, then (RECV-CLOSED) yields zero
/// without blocking.
#[test]
fn s11_10_a_closed_channel_drains_then_yields_zero() {
    let out = out_of(
        "fn main() -> void {\n\
         \x20  let ch = channel();\n\
         \x20  ch.send(1); ch.send(2);\n\
         \x20  ch.close();\n\
         \x20  io::println_int(ch.recv());\n\
         \x20  io::println_int(ch.recv());\n\
         \x20  io::println_int(ch.recv());\n\
         }",
    );
    assert_eq!(out, "1\n2\n0\n", "§11.10 (RECV) then (RECV-CLOSED)");
}

/// (SEND-CLOSED) traps, and the trace produced up to that point is retained,
/// exactly as §8 requires.
#[test]
fn s11_10_send_on_a_closed_channel_traps() {
    let (out, trap) = obs(
        "fn main() -> void {\n\
         \x20  let ch = channel();\n\
         \x20  ch.close();\n\
         \x20  io::println(\"before\");\n\
         \x20  ch.send(1);\n\
         \x20  io::println(\"after\");\n\
         }",
    );
    assert_eq!(out, "before\n", "the trace before the trap must be retained");
    assert_eq!(
        trap.as_deref(),
        Some("send on a closed channel — the value cannot be received"),
        "§11.10 (SEND-CLOSED)"
    );
}

/// (CLOSE) is idempotent: a second close finds `B(c)` already empty and adds a
/// member the set already has.
///
/// This row exists because the T3 emulator got it wrong in a way nothing else
/// would have caught — `heap_objs.remove` took the entry out unconditionally
/// and the following `if let` matched only an OPEN channel, so a second close
/// destroyed the channel and its queued values. `send(7); close(); close();
/// recv()` gave 0 on T3 and 7 on LLVM (report.txt P106).
#[test]
fn s11_10_close_is_idempotent() {
    let out = out_of(
        "fn main() -> void {\n\
         \x20  let ch = channel();\n\
         \x20  ch.send(7);\n\
         \x20  ch.close();\n\
         \x20  ch.close();\n\
         \x20  io::println_int(ch.recv());\n\
         }",
    );
    assert_eq!(out, "7\n", "§11.10: close is idempotent");
}

/// §11.6's deadlock trap must NOT fire for a task whose channel was closed.
///
/// Before (CLOSE) woke waiters, this program reported *"deadlock — every task
/// is blocked on a channel that no runnable task can fill"*, which was true and
/// useless: the close had already established that none ever would.
#[test]
fn s11_10_a_closed_channel_is_not_a_deadlock() {
    let (out, trap) = obs(
        "fn main() -> void {\n\
         \x20  let ch = channel();\n\
         \x20  spawn { let v = ch.recv(); io::println(\"woke\"); }\n\
         \x20  yield;\n\
         \x20  ch.close();\n\
         }",
    );
    assert!(trap.is_none(), "a closed channel must not deadlock: {trap:?}");
    assert_eq!(out, "woke\n");
}

// ---------------------------------------------------------------------------
// §11.11 — bounded channels, and the fourth yield point
// ---------------------------------------------------------------------------

/// (SEND-BLOCK) and (RECV-WAKE): a producer stops at the bound and resumes
/// when a receive frees a slot.
///
/// Derived by hand: capacity 2, so `send 1` and `send 2` fill it; `send 3`
/// finds `|𝒞(c)| = cap(c)` and joins `S(c)`, which empties `R` of everything
/// but main; each `recv` takes a value and wakes the one sender.
#[test]
fn s11_11_a_full_send_blocks_and_a_recv_wakes_it() {
    let out = out_of(
        "fn main() -> void {\n\
         \x20  let ch = channel_bounded(2);\n\
         \x20  spawn {\n\
         \x20    let mut i = 1;\n\
         \x20    while i <= 4 { ch.send(i); io::println(\"sent\"); i = i + 1; }\n\
         \x20  }\n\
         \x20  yield;\n\
         \x20  let mut n = 0;\n\
         \x20  while n < 4 { let v = ch.recv(); io::println(\"got\"); n = n + 1; }\n\
         }",
    );
    assert_eq!(out, "sent\nsent\ngot\ngot\nsent\nsent\ngot\ngot\n", "§11.11");
}

/// An UNBOUNDED channel is never full, so §11.4's original three yield points
/// are still exactly its yield points.
///
/// GUARD ROW: it passes before §11.11 too, and that is its job — it is the
/// reason the fourth yield point moves no existing program.
#[test]
fn s11_11_an_unbounded_send_still_does_not_yield() {
    let out = out_of(
        "fn main() -> void {\n\
         \x20  let ch = channel();\n\
         \x20  let mut i = 0;\n\
         \x20  while i < 300 { ch.send(i); i = i + 1; }\n\
         \x20  io::println(\"sent 300\");\n\
         \x20  let mut s = 0; let mut j = 0;\n\
         \x20  while j < 300 { s = s + ch.recv(); j = j + 1; }\n\
         \x20  io::println_int(s);\n\
         }",
    );
    assert_eq!(out, "sent 300\n44850\n", "§11.1: unbounded means unbounded");
}

/// §11.6 counts `S` as well as `B`, and the message names which.
///
/// A task waiting for ROOM on a channel nothing will drain is as deadlocked as
/// one waiting for a VALUE on a channel nothing will fill. The wrong word
/// sends the reader looking for a missing sender when the problem is a missing
/// receiver, so the row asserts the WORD and not merely that it trapped.
#[test]
fn s11_11_blocking_on_a_full_channel_is_a_deadlock_that_says_drain() {
    let (out, trap) = obs(
        "fn main() -> void {\n\
         \x20  let ch = channel_bounded(1);\n\
         \x20  ch.send(1);\n\
         \x20  io::println(\"one\");\n\
         \x20  ch.send(2);\n\
         \x20  io::println(\"two\");\n\
         }",
    );
    assert_eq!(out, "one\n", "the trace before the trap must be retained");
    assert_eq!(
        trap.as_deref(),
        Some("deadlock — every task is blocked on a channel that no runnable task can drain"),
        "§11.6 must say `drain`, not `fill`"
    );
}

/// A capacity below 1 traps rather than clamping.
///
/// A zero-capacity channel can never hold a value, so every send on it blocks
/// forever; rounding up to 1 turns a program that cannot work into one that
/// quietly does something else.
#[test]
fn s11_11_a_capacity_below_one_traps() {
    let (_, trap) = obs("fn main() -> void { let ch = channel_bounded(0); }");
    assert_eq!(trap.as_deref(), Some("a channel capacity must be at least 1"));
}

/// (CLOSE) wakes blocked SENDERS too, and each then traps by (SEND-CLOSED).
///
/// The alternative is a task parked forever on a channel nothing will drain,
/// so the trap is the right outcome rather than an accident of ordering.
#[test]
fn s11_11_close_wakes_a_blocked_sender_which_then_traps() {
    let (out, trap) = obs(
        "fn main() -> void {\n\
         \x20  let ch = channel_bounded(1);\n\
         \x20  spawn { ch.send(1); ch.send(2); io::println(\"sender done\"); }\n\
         \x20  yield;\n\
         \x20  io::println(\"closing\");\n\
         \x20  ch.close();\n\
         }",
    );
    assert_eq!(out, "closing\n", "the sender must not complete");
    assert_eq!(
        trap.as_deref(),
        Some("send on a closed channel — the value cannot be received"),
        "a woken sender re-executes its send and meets (SEND-CLOSED)"
    );
}

// ---------------------------------------------------------------------------
// §11.12 — `Task<T>` and `await`: the gap, asserted in both directions
// ---------------------------------------------------------------------------

/// **§11.12 is now implemented BY BOTH BACKENDS and not by the A3 reference,
/// so §1.2's obligation has moved rather than gone.**
///
/// This row replaces `s11_12_task_and_await_are_specified_and_unimplemented`,
/// whose message said to do exactly this when the section landed. What it
/// pinned — that `spawn` binds `void` and `await` does not parse — is false
/// now: `spawn { B }` has type `Task<T>`, `await h` parses in prefix position,
/// and `structured_waiting_tests.rs` runs the behaviour on both backends.
///
/// **The gap that remains is the REFERENCE**, and it is a real one rather than
/// an oversight. §11.12 says `spawn { B } : Task<T>` "where `T` is the value of
/// block `B`", and §2's core grammar has no block value at all — a block is
/// `"{" stmt* "}"` and nothing gives it one. So the reference cannot implement
/// §11.12 as written without a grammar extension the section does not supply,
/// and §11.12's own first line claiming *"The A3 reference implements it"* was
/// false the day it was written.
///
/// Asserted in BOTH directions, as §1.2 requires: that the reference still
/// refuses `await`, AND that `spawn` is still a statement there. A row
/// asserting only the first would call the second done.
#[test]
fn s11_12_the_reference_does_not_have_task_and_await() {
    // Direction 1: `await` is not a token the core lexer knows.
    let refused = reference::interpret(
        "fn main() -> int { let t = spawn { 1 }; let v = await t; return v; }",
    );
    assert!(
        refused.is_err(),
        "§11.12 has landed in the A3 reference — delete this row, move the \
         program into a rule-by-rule row above, and correct §11.12's first \
         line, which claims the reference already implements it"
    );

    // Direction 2: `spawn` is a STATEMENT in the core, so it cannot be bound.
    // `src/reference/ast.rs` says so in as many words, and until §2 gains a
    // block value there is nothing for `Task<T>` to carry.
    let as_expr = reference::interpret(
        "fn main() -> int { let t = spawn { 1 }; return 0; }",
    );
    assert!(
        as_expr.is_err(),
        "`spawn` has become an expression in the A3 reference — §2's core \
         grammar must have gained a block value, and §11.12's status line \
         needs correcting with it"
    );
}
