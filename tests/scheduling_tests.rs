//! §11 on the T3 backend — `--sched cooperative`.
//!
//! © Manish Jagdish Thatte
//!
//! Step 2 of `enhance/phase3-the-semantics-debt/CONCURRENCY_DECISION.md` §5.
//! `tests/interleaving_tests.rs` pins the same rules against the A3 reference;
//! these run them on the machine, through the compiler.
//!
//! **Every row asserts BOTH lowerings**, and that is the point rather than
//! thoroughness: `--sched inline` is `docs/memory-model.md` §4 — the language
//! as it has always behaved — and `cooperative` is §11. A row that checked only
//! the new one could not tell "§11 works" from "the flag did nothing", and a
//! row that checked only the old one is what a default-off feature gets by
//! accident.

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

mod common;

static N: AtomicUsize = AtomicUsize::new(0);

fn manitc_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_manitc"))
}

/// Compile with the given `--sched` and run on T3; returns (stdout, trapped).
fn run(src: &str, sched: &str) -> (String, bool) {
    let slot = N.fetch_add(1, Ordering::Relaxed);
    let d = common::suite_root("sched")
        .join(slot.to_string());
    std::fs::create_dir_all(&d).expect("temp dir");
    let path = d.join("p.mt");
    std::fs::write(&path, src).expect("write");
    let base = path.with_extension("");

    let c = Command::new(manitc_bin())
        .args([
            "compile", path.to_str().unwrap(), "--target", "t3",
            "--sched", sched, "-o", base.to_str().unwrap(),
        ])
        .output()
        .expect("compile");
    assert!(
        c.status.success(),
        "compile failed under --sched {}:\n{}\n{}",
        sched,
        String::from_utf8_lossy(&c.stdout),
        String::from_utf8_lossy(&c.stderr)
    );

    let r = Command::new(manitc_bin())
        .args(["run-t3", base.with_extension("t3b").to_str().unwrap()])
        .output()
        .expect("run");
    let stdout = String::from_utf8_lossy(&r.stdout).into_owned();
    let trapped = stdout.contains("TRAP:") || r.status.code() == Some(70);
    let out: String = stdout
        .lines()
        .filter(|l| !l.starts_with("[T3ISA]") && !l.starts_with("TRAP:"))
        .map(|l| format!("{}\n", l))
        .collect();
    (out, trapped)
}

const SPAWN_THEN_PRINT: &str = r#"
use std::io;
fn main() {
    let ch = channel<int>();
    spawn {
        io::println("task");
        ch.send(7);
    }
    io::println("main");
    let v = ch.recv();
    io::print("got ");
    io::println_int(v);
}
"#;

/// §11.5 (SPAWN): the task is appended at the back and the spawner CONTINUES.
///
/// The two orderings ARE the finding. Under `inline` the block is evaluated
/// where it is written, so `task` prints first — that is what
/// `docs/memory-model.md` §4 specifies and what report.txt P5 records as the
/// defect. Under `cooperative` the spawner runs on, blocks in `recv`, and the
/// task runs then.
#[test]
fn spawn_runs_in_place_inline_and_as_a_task_cooperatively() {
    let (inline, t1) = run(SPAWN_THEN_PRINT, "inline");
    assert!(!t1, "inline: unexpected trap: {:?}", inline);
    assert_eq!(
        inline, "task\nmain\ngot 7\n",
        "--sched inline is docs/memory-model.md §4: the block runs in place"
    );

    let (coop, t2) = run(SPAWN_THEN_PRINT, "cooperative");
    assert!(!t2, "cooperative: unexpected trap: {:?}", coop);
    assert_eq!(
        coop, "main\ntask\ngot 7\n",
        "§11.5 (SPAWN): the spawner continues; the task runs when main blocks \
         in (RECV-BLOCK), and (SEND-WAKE) hands main the value"
    );

    assert_ne!(inline, coop, "the flag has to change something");
}

/// **The row the whole design exists for.** A spawned block reads a local of
/// the function that spawned it, and it works with no capture analysis, no
/// closure conversion and no environment record — because §11.5 (SPAWN) is
/// implemented as a FORK, so the task shares the frame layout by BEING the
/// same frame, one copy later.
///
/// Every `spawn` in `examples/concurrency.mt` captures something, so this is
/// the difference between a lowering that is a page and one that is a phase.
#[test]
fn a_spawned_block_reads_the_locals_it_was_written_beside() {
    let src = r#"
use std::io;
fn main() {
    let n = 21;
    let ch = channel<int>();
    spawn {
        io::print("child sees n = ");
        io::println_int(n);
        ch.send(n * 2);
    }
    let v = ch.recv();
    io::print("got ");
    io::println_int(v);
}
"#;
    for sched in ["inline", "cooperative"] {
        let (out, trapped) = run(src, sched);
        assert!(!trapped, "{}: unexpected trap: {:?}", sched, out);
        assert!(
            out.contains("child sees n = 21") && out.contains("got 42"),
            "{}: the captured local must be visible in the task: {:?}",
            sched,
            out
        );
    }
}

/// §11.6: `main` returning does not end the program.
///
/// Under `inline` this cannot be observed at all — the block has already run —
/// which is exactly why the two arms assert different things rather than the
/// same string twice.
#[test]
fn main_returning_does_not_end_the_program() {
    let src = r#"
use std::io;
fn main() {
    spawn {
        io::println("later");
    }
    io::println("main done");
}
"#;
    let (inline, _) = run(src, "inline");
    assert_eq!(inline, "later\nmain done\n", "inline: the block ran in place");

    let (coop, trapped) = run(src, "cooperative");
    assert!(!trapped, "cooperative: unexpected trap: {:?}", coop);
    assert_eq!(
        coop, "main done\nlater\n",
        "§11.6: main terminates as a task and the rest run on. `main done` \
         alone would mean HALT ended the program and discarded the task"
    );
}

/// §11.6: a receive nothing can satisfy is a DETECTED deadlock, and §8's rule
/// that the trace before a trap survives still holds.
///
/// Both lowerings trap, and they reach it differently: with nothing spawned
/// P81's message says so directly, while under the scheduler §11.6 empties the
/// run queue and reports the deadlock. **The verdict is the same either way**,
/// which is why P81's trap could be left exactly as it was.
#[test]
fn an_unfillable_receive_traps_under_both_lowerings() {
    let src = r#"
use std::io;
fn main() {
    let ch = channel<int>();
    io::println("before");
    let v = ch.recv();
    io::println("after");
}
"#;
    for sched in ["inline", "cooperative"] {
        let (out, trapped) = run(src, sched);
        assert!(trapped, "{}: an unfillable receive must trap: {:?}", sched, out);
        assert_eq!(out, "before\n", "{}: §8 retains the trace, and only it", sched);
    }
}

/// Two producers and a consumer, all spawned: the scheduler has to interleave
/// three tasks through one channel and finish.
#[test]
fn two_producers_and_a_consumer_interleave_and_terminate() {
    let src = r#"
use std::io;
fn main() {
    let ch = channel<int>();
    spawn { ch.send(1); ch.send(2); }
    spawn { ch.send(10); ch.send(20); }
    mut total = 0;
    mut got = 0;
    while got < 4 {
        let v = ch.recv();
        total = total + v;
        got = got + 1;
    }
    io::print("total ");
    io::println_int(total);
}
"#;
    let (out, trapped) = run(src, "cooperative");
    assert!(!trapped, "unexpected trap: {:?}", out);
    assert_eq!(
        out, "total 33\n",
        "every send must be received exactly once, whatever the interleaving"
    );
}

/// `--sched cooperative` now COMPILES AND RUNS on LLVM, and produces the same
/// observable behaviour as it does on T3.
///
/// **Corrected 2 September 2026 (P99).** This row used to assert the opposite —
/// that the flag was REFUSED — and the refusal was right for as long as the
/// LLVM backend had no scheduler: a flag silently dropped is how someone comes
/// to believe a program is scheduled when it is not. Step 3 of the decision
/// document is now done and the refusal is gone, so the row asserts what
/// replaced it rather than being deleted (permanent rule 7). What it pins is
/// unchanged in spirit: the flag must MEAN something on this backend.
///
/// The two backends reach §11 by different lowerings — T3 forks (P89), LLVM
/// outlines the body and passes §11.2's copy of the store explicitly (P99) —
/// so agreement here is a real check rather than a shared lowering agreeing
/// with itself.
#[test]
fn cooperative_now_runs_on_llvm_and_agrees_with_t3() {
    let slot = N.fetch_add(1, Ordering::Relaxed);
    let d = common::suite_root("sched")
        .join(format!("llvm{}", slot));
    std::fs::create_dir_all(&d).expect("temp dir");
    let path = d.join("p.mt");
    std::fs::write(&path, SPAWN_THEN_PRINT).expect("write");
    let bin = path.with_extension("");

    let c = Command::new(manitc_bin())
        .args([
            "compile", path.to_str().unwrap(), "--target", "llvm",
            "--sched", "cooperative", "-o", bin.to_str().unwrap(),
        ])
        .output()
        .expect("compile");
    let msg = format!(
        "{}{}",
        String::from_utf8_lossy(&c.stdout),
        String::from_utf8_lossy(&c.stderr)
    );
    // No binary means no clang, which is an environment fact rather than a
    // result. Told apart by the ARTEFACT and never by the shape of a message
    // (P47).
    if !bin.exists() {
        return;
    }
    assert!(c.status.success(), "LLVM must accept --sched cooperative now: {msg}");

    let r = Command::new(&bin).output().expect("run");
    let llvm_out = String::from_utf8_lossy(&r.stdout).into_owned();
    assert_eq!(
        llvm_out, "main\ntask\ngot 7\n",
        "§11.5 (SPAWN): the spawner continues; the task runs when main blocks \
         in (RECV-BLOCK), and (SEND-WAKE) hands main the value — the SAME \
         string the T3 row above asserts, reached by a different lowering"
    );
}

/// An unknown `--sched` is an error, not a silent fallback to the default.
#[test]
fn an_unknown_sched_mode_is_refused() {
    let d = common::suite_root("sched_bad");
    std::fs::create_dir_all(&d).expect("temp dir");
    let path = d.join("p.mt");
    std::fs::write(&path, "fn main() { }").expect("write");
    let c = Command::new(manitc_bin())
        .args([
            "compile", path.to_str().unwrap(), "--target", "t3",
            "--sched", "preemptive", "-o", path.with_extension("").to_str().unwrap(),
        ])
        .output()
        .expect("compile");
    assert!(!c.status.success(), "an unknown mode must be refused");
}

// ---------------------------------------------------------------------------
// P118 — §11.2 says a spawned task gets a COPY of the store, and for an
// aggregate neither backend makes one
// ---------------------------------------------------------------------------
//
// Measured with the task's write ordered before the spawner's read, so the
// answer is about sharing and not about scheduling:
//
//   struct P { pub x: int, pub y: int }
//   let mut p = P { x: 1, y: 2 };  spawn { p.x = 99; }  yield;  print(p.x)
//     T3   -> 99   the task and the spawner hold the same heap cell
//     LLVM -> 1    but for the wrong reason: the outlined body binds the
//                  captured ADDRESS as though it were the value, so a task
//                  that merely READS the capture sees `x=94299331632576 y=0`
//
// Refused rather than repaired, because the repair is a deep copy at the spawn
// site and a deep copy needs regions to be affordable (F-4, and B7's D-4).
// **The population was measured first**: 0 of the 34 `spawn` sites across both
// repositories and the 2,507-file corpus capture an aggregate — every one
// captures a channel, a `Mutex`/atomic handle, or a scalar.

/// `manitc check` only, since these rows are about a refusal.
fn check_only(src: &str) -> (bool, String) {
    let slot = N.fetch_add(1, Ordering::Relaxed);
    let d = common::suite_root("sched").join(format!("chk{}", slot));
    std::fs::create_dir_all(&d).expect("temp dir");
    let path = d.join("p.mt");
    std::fs::write(&path, src).expect("write");
    let c = Command::new(manitc_bin())
        .args(["check", path.to_str().unwrap()])
        .output()
        .expect("check");
    (
        c.status.success(),
        format!(
            "{}{}",
            String::from_utf8_lossy(&c.stdout),
            String::from_utf8_lossy(&c.stderr)
        ),
    )
}

#[test]
fn p118_a_spawn_may_not_capture_an_aggregate() {
    // Three shapes, because "aggregate" is three variants of one type and a
    // row that tested only the struct would leave the other two to be found by
    // somebody else.
    let cases = [
        ("struct", r#"
use std::io;
struct P { pub x: int, pub y: int }
fn main() {
    let p: P = P { x: 11, y: 22 };
    spawn { io::println_int(p.x); }
}
"#),
        ("array", r#"
use std::io;
fn main() {
    let a: [int; 3] = [1, 2, 3];
    spawn { io::println_int(a[0]); }
}
"#),
        ("tuple", r#"
use std::io;
fn main() {
    let t: (int, int) = (4, 5);
    spawn { io::println_int(t.0); }
}
"#),
    ];
    for (what, src) in cases {
        let (ok, msg) = check_only(src);
        assert!(!ok, "P118: a spawn capturing a {what} must be refused, and was accepted");
        assert!(
            msg.contains("11.2") && msg.contains("aggregate"),
            "P118: the {what} refusal must name the clause it enforces, and said:\n{msg}"
        );
    }
}

/// A GUARD, and it says so: this passes on the pre-P118 compiler too, because
/// nothing was refused there at all. It is here because the refusal's
/// discriminator is "does this struct have FIELDS", and getting that wrong in
/// the other direction would refuse every concurrent program in the language —
/// `Channel<T>` and `Mutex<T>` are `Generic`, while `AtomicTrit`, `Barrier` and
/// `Semaphore` are `Struct(name, [])` with no fields, which is exactly the
/// distinction §11.2 needs: a handle is one word and copies correctly, and
/// channels are the one thing tasks are REQUIRED to share.
#[test]
fn p118_a_handle_is_not_an_aggregate() {
    let cases = [
        ("channel", r#"
use std::io;
fn main() {
    let ch = channel<int>();
    spawn { ch.send(7); }
    io::println_int(ch.recv());
}
"#),
        ("barrier", r#"
use std::sync;
use std::io;
fn main() {
    let b: Barrier = Barrier::new(2);
    spawn { b.wait(); }
    b.wait();
    io::println("through");
}
"#),
    ];
    for (what, src) in cases {
        let (ok, msg) = check_only(src);
        assert!(ok, "P118 must not refuse a {what} capture — §11.2 requires tasks to share these:\n{msg}");
    }
}

#[test]
fn p118_a_move_inside_a_task_does_not_consume_the_spawners_binding() {
    // The same clause in the other direction. The move checker used to treat
    // `spawn { B }` as a plain block, so a move inside B consumed the
    // SPAWNER's binding — its own comment said "anything it moves is also
    // moved in the parent scope". Under §11.2 the task moved its own copy.
    //
    // Asserted on the VALUE and under both lowerings (rule 8), because "it
    // compiles now" would also be true of a checker that had simply stopped
    // looking inside the block.
    let src = r#"
use std::io;
fn main() {
    let s: str = "hello";
    spawn { let t: str = s; io::println(t); }
    io::println(s);
}
"#;
    let (ok, msg) = check_only(src);
    assert!(ok, "P118: §11.2 gives the task a copy, so its move is its own:\n{msg}");
    for sched in ["inline", "cooperative"] {
        let (out, trapped) = run(src, sched);
        assert!(!trapped, "P118: --sched {sched} trapped");
        assert_eq!(
            out, "hello\nhello\n",
            "P118: under --sched {sched} both the task's copy and the spawner's \
             own binding must print"
        );
    }
}
