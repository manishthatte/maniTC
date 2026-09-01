//! P99 / concurrency step 3b — §11 on the LLVM backend.
//!
//! © Manish Jagdish Thatte
//!
//! `docs/semantics.md` §11 is normative and one specification. **The two
//! backends reach it by different lowerings**, which is what makes agreement
//! here evidence rather than a shared lowering agreeing with itself:
//!
//! - **T3 FORKS** (P89). Syscall 80 returns 0 in the child, so the body is
//!   reached by an ordinary branch and finds its enclosing locals because it IS
//!   the same frame, one copy later.
//! - **LLVM OUTLINES** (P99). A task's continuation there is a C call stack and
//!   a hosted process does not own its stack, so the body becomes a function
//!   and §11.2's copy of the store is passed explicitly in an env array.
//!
//! Every row runs the SAME program on both and asserts the SAME output.

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static N: AtomicUsize = AtomicUsize::new(0);

fn manitc_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_manitc"))
}

fn workdir() -> PathBuf {
    let slot = N.fetch_add(1, Ordering::Relaxed);
    let d = std::env::temp_dir()
        .join(format!("manitc_coop_{}", std::process::id()))
        .join(slot.to_string());
    std::fs::create_dir_all(&d).expect("temp dir");
    d
}

fn run_t3(src: &str) -> String {
    let d = workdir();
    let path = d.join("p.mt");
    std::fs::write(&path, src).expect("write");
    let base = path.with_extension("");
    let c = Command::new(manitc_bin())
        .args(["compile", path.to_str().unwrap(), "--target", "t3",
               "--sched", "cooperative", "-o", base.to_str().unwrap()])
        .output().expect("compile");
    assert!(c.status.success(), "T3 compile failed:\n{}{}",
            String::from_utf8_lossy(&c.stdout), String::from_utf8_lossy(&c.stderr));
    let r = Command::new(manitc_bin())
        .args(["run-t3", base.with_extension("t3b").to_str().unwrap()])
        .output().expect("run");
    let out: String = String::from_utf8_lossy(&r.stdout)
        .lines().filter(|l| !l.starts_with("[T3ISA]"))
        .map(|l| format!("{l}\n")).collect();
    out + &String::from_utf8_lossy(&r.stderr)
        .lines().filter(|l| l.starts_with("TRAP"))
        .map(|l| format!("{l}\n")).collect::<String>()
}

fn compile_llvm(src: &str, sched: Option<&str>) -> Option<PathBuf> {
    let d = workdir();
    let path = d.join("p.mt");
    std::fs::write(&path, src).expect("write");
    let bin = d.join("p.bin");
    let mut args = vec![
        "compile".to_string(), path.to_string_lossy().into_owned(),
        "--target".into(), "llvm".into(),
        "-o".into(), bin.to_string_lossy().into_owned(),
    ];
    if let Some(m) = sched {
        args.push("--sched".into());
        args.push(m.to_string());
    }
    Command::new(manitc_bin()).args(&args).output().expect("compile");
    bin.exists().then_some(bin)
}

/// Run under `--sched cooperative`; `None` only when clang is absent.
///
/// **THE CONTROL COMPILE IS WHAT SEPARATES THE TWO STATES, AND LEAVING IT OUT
/// IS A SILENT PASS.** Skipping whenever no binary appears looks right and is
/// P47's defect: a compiler that REFUSES `--sched cooperative` produces no
/// binary either, so six of these seven rows went green on the pre-change tree
/// while asserting nothing about LLVM at all. Compiling the same program
/// WITHOUT the flag answers the environment question on its own, so a failure
/// with the flag is a real failure.
///
/// Recorded at this length because it is the second time in one day: the same
/// separation was written into `scheduler_runtime_tests.rs` an hour earlier and
/// then not carried here. *The pattern does not stop when it is named* — P64.
fn run_llvm(src: &str) -> Option<String> {
    if compile_llvm(src, None).is_none() {
        return None; // no clang: an environment fact, not a result
    }
    let bin = compile_llvm(src, Some("cooperative")).expect(
        "this program compiles to LLVM without --sched, so failing WITH it is a \
         real failure — most likely the flag is still refused",
    );
    let r = Command::new(&bin).output().expect("run");
    Some(String::from_utf8_lossy(&r.stdout).into_owned()
         + &String::from_utf8_lossy(&r.stderr))
}

/// Assert both backends produce `want` under `--sched cooperative`.
fn both_coop(src: &str, want: &str, what: &str) {
    let t3 = run_t3(src);
    assert_eq!(t3, want, "{what}: T3 (fork lowering)");
    if let Some(ll) = run_llvm(src) {
        assert_eq!(ll, want, "{what}: LLVM (outlined lowering)");
    }
}

#[test]
fn s11_spawn_does_not_yield_and_recv_blocks() {
    both_coop("
use std::io;
fn main() {
    let ch = channel<int>();
    spawn { io::println(\"task\"); ch.send(7); }
    io::println(\"main\");
    // The recv is bound BEFORE anything is printed, deliberately: written the
    // other way round the `print(\"got \")` happens first and the task's line
    // lands between the two halves of one output statement — `main`, `got `,
    // `task`, `7`. That is CORRECT under §11 and it makes the row about output
    // buffering rather than about scheduling.
    let v = ch.recv();
    io::print(\"got \"); io::println_int(v);
}
", "main\ntask\ngot 7\n", "§11.4/§11.5 SPAWN then RECV-BLOCK then SEND-WAKE");
}

#[test]
fn s11_a_spawned_block_reads_a_local_of_its_spawner() {
    // §11.2's copy of the store, and the row where the two lowerings differ
    // most. T3 gets it free — the child IS the frame. LLVM has to compute the
    // captures, pack them and read them back, so this is the row that fails if
    // the capture analysis misses a name or the env layout drifts between the
    // packing site and the outlined body.
    both_coop("
use std::io;
fn main() {
    let ch = channel<int>();
    let n = 21;
    spawn { ch.send(n * 2); }
    io::print(\"got \"); io::println_int(ch.recv());
}
", "got 42\n", "§11.2: the task gets a copy of the store");
}

#[test]
fn s11_yield_is_a_yield_point_on_both_backends() {
    // §11.4's FIRST yield point, and it did not exist in the language at all
    // until P99 — specified in §11.1 and §11.5, with syscall 81 sitting in the
    // T3 emulator since P88 and nothing able to reach it. Round-robin over
    // three tasks is what distinguishes a real yield from a no-op.
    both_coop("
use std::io;
fn main() {
    spawn { io::println(\"A1\"); yield; io::println(\"A2\"); }
    spawn { io::println(\"B1\"); yield; io::println(\"B2\"); }
    io::println(\"main-after-spawn\");
    yield;
    io::println(\"main-2\");
}
", "main-after-spawn\nA1\nB1\nmain-2\nA2\nB2\n", "§11.4/§11.5 YIELD");
}

#[test]
fn s11_main_returning_does_not_end_the_program() {
    // §11.6, and the COMPATIBLE choice rather than the obvious one: because
    // `spawn { B }` ran B inline before §11, every spawned block in every
    // existing program had already completed by the time `main` returned.
    // Ending at `main` would silently discard work those programs do.
    both_coop("
use std::io;
fn main() {
    spawn { io::println(\"task runs after main\"); }
    io::println(\"main ends\");
}
", "main ends\ntask runs after main\n", "§11.6: main is a task like any other");
}

#[test]
fn s11_deadlock_is_a_trap_and_the_trace_survives() {
    // §11.6, and P5.1's exact shape resolved. Both backends now name the
    // situation and keep everything printed before it. On the pthread runtime
    // this blocked in `pthread_cond_wait` with stdout unflushed and printed
    // NOTHING AT ALL, losing the trace along with the answer.
    let src = "
use std::io;
fn main() {
    let ch = channel<int>();
    io::println(\"before\");
    spawn { io::println(\"waiting\"); io::println_int(ch.recv()); }
    io::println(\"main done\");
}
";
    let t3 = run_t3(src);
    assert!(t3.contains("before") && t3.contains("waiting") && t3.contains("deadlock"),
            "T3: expected a deadlock trap with its trace:\n{t3}");
    assert!(!t3.contains("unreachable"), "T3: ran past the trap:\n{t3}");
    if let Some(ll) = run_llvm(src) {
        assert_eq!(ll, t3, "the two backends must agree about a deadlock, message included");
    }
}

#[test]
fn s11_the_acceptance_example_agrees_on_both_backends() {
    // `examples/concurrency.mt` is what §11.8 names as the program whose output
    // this section moves. It exercises channels, spawn, `try_recv`, Mutex,
    // AtomicTrit, Barrier, Semaphore and `async::yield_now` across eight demos.
    //
    // It is here because it caught a defect no small row did: `async::yield_now`
    // compiled to POSIX `sched_yield(3)` on LLVM, which yields the OS THREAD.
    // Under the baton the running task holds it, so a consumer looping on
    // `try_recv` and yielding could never let its producer run — the program
    // printed its first header and hung. T3 was correct by accident: it compiles
    // `yield_now` to SYSCALL #81, whose comment still said "no-op" from before
    // P88 made it a real task yield.
    let src = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/concurrency.mt"),
    ).expect("read the example");
    let t3 = run_t3(&src);
    assert!(t3.contains("All demos complete."), "T3 did not finish:\n{t3}");
    if let Some(ll) = run_llvm(&src) {
        assert_eq!(ll, t3, "the acceptance example must agree on both backends");
    }
}

#[test]
fn inline_remains_the_default_and_is_unchanged() {
    // The whole surface still defaults to `docs/memory-model.md` §4, where
    // `spawn { B }` runs B in place. §11.8 records that step 2 MOVES the output
    // of every program that spawns, which is exactly why it is opt-in.
    let d = workdir();
    let path = d.join("p.mt");
    std::fs::write(&path, "
use std::io;
fn main() {
    let ch = channel<int>();
    spawn { io::println(\"task\"); ch.send(7); }
    io::println(\"main\");
    io::print(\"got \"); io::println_int(ch.recv());
}
").expect("write");
    let base = path.with_extension("");
    assert!(Command::new(manitc_bin())
        .args(["compile", path.to_str().unwrap(), "--target", "t3",
               "-o", base.to_str().unwrap()])
        .output().expect("compile").status.success());
    let r = Command::new(manitc_bin())
        .args(["run-t3", base.with_extension("t3b").to_str().unwrap()])
        .output().expect("run");
    let out: String = String::from_utf8_lossy(&r.stdout)
        .lines().filter(|l| !l.starts_with("[T3ISA]"))
        .map(|l| format!("{l}\n")).collect();
    assert_eq!(out, "task\nmain\ngot 7\n",
               "without --sched the block still runs IN PLACE");
}
