//! §11.9 and §11.10 of `docs/semantics.md` — structured waiting, and closing
//! a channel, on both backends.
//!
//! © Manish Jagdish Thatte
//!
//! Step 4 of `enhance/phase3-the-semantics-debt/CONCURRENCY_DECISION.md` §5.
//! Steps 2 and 3 made tasks real; `Mutex`, `Semaphore` and `Barrier` were left
//! as they were, and their own comments said what they were: *"no-op in
//! sequential model"*, true when written and untouched by the two steps that
//! removed the premise.
//!
//! **§11.9 makes all three DERIVED forms**, so nothing here is testing a new
//! rule — it is testing that three implementations of a desugaring into
//! `send`/`recv` agree with the rules already pinned by
//! `interleaving_tests.rs`. The two backends reach them by different
//! mechanisms, which is what makes their agreement evidence rather than a
//! shared lowering agreeing with itself: **T3 re-executes the blocking
//! syscall** after a wake (the emulator rewinds the PC), while **LLVM returns
//! from `manit_sched_block_on` into a `while` loop** in the C runtime.
//!
//! Every expected trace was derived by hand from §11.5's rules before it was
//! run, as `interleaving_tests.rs` requires: a test whose expectation came
//! from the implementation records what the implementation does.

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

mod common;

static N: AtomicUsize = AtomicUsize::new(0);

fn manitc_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_manitc"))
}

fn workdir() -> PathBuf {
    let slot = N.fetch_add(1, Ordering::Relaxed);
    let d = common::suite_root("sw")
        .join(slot.to_string());
    std::fs::create_dir_all(&d).expect("temp dir");
    d
}

fn run_t3(src: &str, sched: &str) -> String {
    let d = workdir();
    let path = d.join("p.mt");
    std::fs::write(&path, src).expect("write");
    let base = path.with_extension("");
    let c = Command::new(manitc_bin())
        .args(["compile", path.to_str().unwrap(), "--target", "t3",
               "--sched", sched, "-o", base.to_str().unwrap()])
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

/// `None` only when clang is absent.
///
/// **The control compile is what separates the two states.** Skipping whenever
/// no binary appears is P47's defect and it has now bitten this repository
/// three times, twice in the scheduler's own rows: a compiler that REFUSES the
/// flag, or one whose runtime does not link, produces no binary either.
/// Compiling the same program WITHOUT the flag answers the environment
/// question on its own, so a failure WITH it is a real failure.
fn run_llvm(src: &str, sched: &str) -> Option<String> {
    if compile_llvm(src, None).is_none() {
        return None; // no clang: an environment fact, not a result
    }
    let bin = compile_llvm(src, Some(sched)).expect(
        "this program compiles to LLVM without --sched, so failing WITH it is a \
         real failure — most likely the runtime no longer links",
    );
    let r = Command::new(&bin).output().expect("run");
    Some(String::from_utf8_lossy(&r.stdout).into_owned()
         + &String::from_utf8_lossy(&r.stderr))
}

/// Assert both backends produce `want`. The backends are asserted against the
/// SAME string rather than against each other, so a row cannot pass by the two
/// being wrong together — which is what the parity matrix does and why it
/// reported nothing about the barrier for a week.
fn both_coop(src: &str, want: &str, what: &str) {
    let t3 = run_t3(src, "cooperative");
    assert_eq!(t3, want, "{what}: T3 (blocking syscall, re-executed on wake)");
    if let Some(ll) = run_llvm(src, "cooperative") {
        assert_eq!(ll, want, "{what}: LLVM (C runtime, baton handed back)");
    }
}

/// `manitc check` on a source, returning (success, combined output).
fn check(src: &str, extra: &[&str]) -> (bool, String) {
    let d = workdir();
    let path = d.join("p.mt");
    std::fs::write(&path, src).expect("write");
    let mut args = vec!["check".to_string(), path.to_string_lossy().into_owned()];
    args.extend(extra.iter().map(|s| s.to_string()));
    let o = Command::new(manitc_bin()).args(&args).output().expect("check");
    (o.status.success(),
     String::from_utf8_lossy(&o.stdout).into_owned() + &String::from_utf8_lossy(&o.stderr))
}

// ---------------------------------------------------------------------------
// §11.9's observable contract, clause by clause
// ---------------------------------------------------------------------------

/// §11.9 contract 1: at most one task holds a `Mutex`.
///
/// Derived from §11.5: main takes the token, so A's `lock` finds the slot
/// empty and joins B(m) (RECV-BLOCK). main's `unlock` is (SEND-WAKE) and puts
/// A back on R, so "A locked" cannot appear before "main unlocking".
///
/// On the pre-step-4 compiler this printed `A locked` BEFORE `main unlocking`
/// on T3 — both tasks holding one mutex — and printed **nothing at all** on
/// LLVM, where the acquiring task blocked on a condition variable while
/// holding the baton needed to signal it.
#[test]
fn s11_9_a_mutex_excludes() {
    both_coop("
use std::io;
use std::sync;
use std::async;
fn main() {
    let m: Mutex<int> = Mutex::new(0);
    let g = m.lock();
    spawn { io::println(\"A locking\"); let a = m.lock(); io::println(\"A locked\"); a.unlock(); }
    io::println(\"main locked\");
    async::yield_now();
    io::println(\"main unlocking\");
    g.unlock();
}
", "main locked\nA locking\nmain unlocking\nA locked\n", "§11.9 contract 1");
}

/// §11.9 contract 2: a `Semaphore(n)` admits at most n holders.
///
/// One permit, two takers. The second `acquire` is `recv` on an empty channel.
/// Before step 4 this printed `A got it` while main still held the only
/// permit, on T3, and hung on LLVM.
#[test]
fn s11_9_a_one_permit_semaphore_admits_one() {
    both_coop("
use std::io;
use std::sync;
use std::async;
fn main() {
    let s = Semaphore::new(1);
    s.acquire();
    spawn { io::println(\"A acquiring\"); s.acquire(); io::println(\"A got it\"); s.release(); }
    io::println(\"main got it\");
    async::yield_now();
    io::println(\"main releasing\");
    s.release();
}
", "main got it\nA acquiring\nmain releasing\nA got it\n", "§11.9 contract 2");
}

/// §11.9 contract 3: a `Barrier(n)` releases nobody until n have arrived.
///
/// **This is the row the parity matrix could not have produced.** Before step
/// 4 both backends printed `main after` before `A before` — a two-party
/// barrier releasing one party alone — and they AGREED, so a cross-backend
/// comparison reported no divergence. The barrier had three implementations
/// (T3 emulator, C runtime, and one the LLVM backend emitted into every
/// module) and only the emitted one was reachable on LLVM.
#[test]
fn s11_9_a_barrier_releases_nobody_until_all_arrive() {
    both_coop("
use std::io;
use std::sync;
fn main() {
    let b = Barrier::new(2);
    spawn { io::println(\"A before\"); b.wait(); io::println(\"A after\"); }
    io::println(\"main before\");
    b.wait();
    io::println(\"main after\");
}
", "main before\nA before\nA after\nmain after\n", "§11.9 contract 3");
}

/// §11.9 contract 3, second half: exactly one task is the leader, and it is
/// the LAST to arrive.
///
/// Asserting the VALUE and not just the ordering (permanent rule 8): a barrier
/// that released correctly but returned `true` to everyone, or to nobody,
/// passes the row above and fails this one.
#[test]
fn s11_9_the_barrier_leader_is_the_last_to_arrive() {
    both_coop("
use std::io;
use std::sync;
fn main() {
    let b = Barrier::new(2);
    spawn {
        let lead = b.wait();
        io::print(\"A leader=\"); if lead { io::println(\"1\"); } else { io::println(\"0\"); }
    }
    let lead = b.wait();
    io::print(\"main leader=\"); if lead { io::println(\"1\"); } else { io::println(\"0\"); }
}
", "A leader=1\nmain leader=0\n", "§11.9 contract 3, the leader");
}

/// §11.9 contract 4: a release wakes AT MOST ONE waiter, the longest-waiting.
///
/// **§11.7 is explicit that this clause is nearly untestable**, because a
/// spuriously woken task re-checks, finds the mutex still held, and blocks
/// again WHILE PRINTING NOTHING — so counting wakes cannot see a wake-all. It
/// is observable only where the CHOICE changes what is printed, which needs
/// two waiters queued in a known order and a trace that says which ran first.
///
/// Derived by hand: R = [main]; two SPAWNs append A then B; (YIELD) sends main
/// to the back, so A runs and blocks, then B runs and blocks, leaving
/// B(m) = [A, B]. main's unlock takes the FRONT of that queue. A LIFO wake, or
/// a wake-all followed by whichever task the run queue happens to reach first,
/// prints `B got` before `A got`.
#[test]
fn s11_9_a_release_wakes_the_longest_waiting() {
    both_coop("
use std::io;
use std::sync;
use std::async;
fn main() {
    let m: Mutex<int> = Mutex::new(0);
    let g = m.lock();
    spawn { io::println(\"A wants\"); let a = m.lock(); io::println(\"A got\"); a.unlock(); }
    spawn { io::println(\"B wants\"); let b = m.lock(); io::println(\"B got\"); b.unlock(); }
    async::yield_now();
    io::println(\"main unlocking\");
    g.unlock();
}
", "A wants\nB wants\nmain unlocking\nA got\nB got\n", "§11.9 contract 4 (SEND-WAKE)");
}

/// §11.9 contract 5, and the compatibility guarantee: **the default
/// `--sched inline` mode is unchanged.**
///
/// `docs/memory-model.md` §4 is still the account of that mode — execution is
/// sequential and `spawn { B }` evaluates B in place — so with one task a lock
/// that does nothing IS mutual exclusion and a barrier is meaningless. Every
/// blocking path added by step 4 is gated on the scheduler being active, and
/// this row is what says the gate is real rather than intended.
///
/// GUARD ROW: it passes on the pre-step-4 compiler too, by construction, and
/// that is the point of it.
#[test]
fn s11_9_the_default_mode_is_unchanged() {
    let src = "
use std::io;
use std::sync;
fn main() {
    let b = Barrier::new(2);
    spawn { io::println(\"A before\"); b.wait(); io::println(\"A after\"); }
    io::println(\"main before\");
    b.wait();
    io::println(\"main after\");
}
";
    // Inline: the spawn runs in place, so A completes before main's own wait.
    // Both arrivals happen; nobody ever blocks.
    let want = "A before\nA after\nmain before\nmain after\n";
    assert_eq!(run_t3(src, "inline"), want, "T3 default mode moved");
    if let Some(ll) = run_llvm(src, "inline") {
        assert_eq!(ll, want, "LLVM default mode moved");
    }
}

// ---------------------------------------------------------------------------
// The barrier's three implementations, and the one that survives
// ---------------------------------------------------------------------------

/// The LLVM backend must not DEFINE `Barrier_wait`.
///
/// It used to, with a comment saying why — *"Spawn blocks execute inline, so a
/// real pthread barrier would block the single thread forever"* — which was
/// true when written. That `define internal` shadowed the C runtime's, so the
/// runtime's scheduler-aware barrier was dead code on LLVM and fixing it there
/// alone changed nothing observable.
///
/// Pinned on the EMITTED IR rather than on the source string, because what
/// matters is which implementation the module links against.
#[test]
fn the_barrier_has_one_implementation() {
    let d = workdir();
    let path = d.join("p.mt");
    std::fs::write(&path, "
use std::sync;
fn main() { let b = Barrier::new(2); b.wait(); }
").expect("write");
    let ll = d.join("p.ll");
    Command::new(manitc_bin())
        .args(["compile", path.to_str().unwrap(), "--target", "llvm",
               "-o", d.join("p.bin").to_str().unwrap()])
        .output().expect("compile");
    let text = std::fs::read_to_string(&ll)
        .or_else(|_| std::fs::read_to_string(path.with_extension("ll")))
        .expect("the backend writes a .ll beside its output");
    assert!(
        !text.contains("define internal i1 @Barrier_wait"),
        "the LLVM backend is defining Barrier_wait again, which shadows \
         runtime/sync.c and makes the scheduler-aware barrier unreachable"
    );
    assert!(
        text.contains("declare i1 @Barrier_wait"),
        "Barrier_wait must be DECLARED so the module links against the one \
         implementation in runtime/sync.c; found neither a declare nor a define"
    );
}

// ---------------------------------------------------------------------------
// `AtomicTrit` is deprecated — CONCURRENCY_DECISION.md §2
// ---------------------------------------------------------------------------

/// Constructing an `AtomicTrit` warns, naming the replacement.
///
/// The decision document deprecates it because an atomic exists to make an
/// operation indivisible against a PRE-EMPTIVE scheduler and ManiT's is
/// cooperative: between two of §11.4's yield points every sequence is already
/// indivisible, so it guarantees nothing a plain `trit` does not.
///
/// The clause is on the CONSTRUCTOR and that is sufficient rather than
/// partial — an `AtomicTrit` cannot be obtained any other way.
#[test]
fn atomic_trit_construction_is_deprecated() {
    let (ok, out) = check("
use std::sync;
fn main() { let a: AtomicTrit = AtomicTrit::new(0); a.store(+); }
", &[]);
    assert!(ok, "deprecation is a warning, not an error:\n{out}");
    assert!(out.contains("deprecated-native"), "no deprecation lint fired:\n{out}");
    assert!(out.contains("AtomicTrit::new"), "the warning must name the constructor:\n{out}");
    assert!(out.contains("cooperative"), "the warning must say WHY:\n{out}");
}

/// ...and `lint allow(deprecated-native);` restores the previous compiler
/// exactly, which is this repository's standing requirement for a new
/// diagnostic.
///
/// The row asserts the allow RELATIVE to the warning — both halves, same
/// program — because asserting only that nothing is reported is satisfied by a
/// compiler that reports nothing, which is exactly the compiler this change
/// replaces. Written the one-sided way first, it passed on the control.
#[test]
fn the_atomic_trit_deprecation_can_be_silenced() {
    const BODY: &str = "
use std::sync;
fn main() { let a: AtomicTrit = AtomicTrit::new(0); a.store(+); }
";
    let (ok_warn, warned) = check(BODY, &[]);
    assert!(ok_warn, "{warned}");
    assert!(
        warned.contains("deprecated"),
        "nothing to silence — the deprecation is not firing at all:\n{warned}"
    );
    let (ok, out) = check(&format!("lint allow(deprecated-native);{BODY}"), &[]);
    assert!(ok, "{out}");
    assert!(!out.contains("deprecated"), "the allow did not silence it:\n{out}");
}

/// `Mutex`, `Barrier` and `Semaphore` are NOT deprecated.
///
/// The decision document keeps them, with a different job: not exclusion
/// against pre-emption but structured waiting. A row that only asserted the
/// warning FIRES would pass a change that deprecated the whole module.
#[test]
fn the_waiting_primitives_are_not_deprecated() {
    let (ok, out) = check("
use std::sync;
fn main() {
    let m: Mutex<int> = Mutex::new(0);
    let s = Semaphore::new(1);
    let b = Barrier::new(1);
    m.lock().unlock(); s.acquire(); s.release(); b.wait();
}
", &[]);
    assert!(ok, "{out}");
    assert!(!out.contains("deprecated"), "a waiting primitive was deprecated:\n{out}");
}

// ---------------------------------------------------------------------------
// The mechanism the deprecation needed, which did not work
// ---------------------------------------------------------------------------

/// An `extern "c" fn` declaration written in a STDLIB module must take effect.
///
/// It did not, and the failure was silent: `Item::ExternDecl` fell into the
/// `_ => {}` arm of `stdlib_expand`'s merge loop — report.txt P61's shape, one
/// item kind later — and, for a native-only module like `sync`, never reached
/// that loop at all, because only `SOURCE_MODULES` is merged. So the A1
/// mechanism could not be used BY the standard library, which is the one place
/// a native declaration is most worth having.
///
/// **Population zero before this**: no stdlib module had ever contained a
/// declaration, so nothing could have reported it. *A defect with a population
/// of zero is not absent, it is unwritten.* Found by writing step 4's
/// `deprecated(...)` clause and measuring that no warning appeared.
#[test]
fn a_stdlib_module_may_carry_an_a1_declaration() {
    // `sync` is native-only and not in SOURCE_MODULES, so this exercises the
    // `register_native_module_sigs` half.
    let (ok, out) = check("
use std::sync;
fn main() { let a: AtomicTrit = AtomicTrit::new(0); a.store(+); }
", &[]);
    assert!(ok, "{out}");
    assert!(
        out.contains("deprecated"),
        "a declaration in stdlib/sync.mt had no effect — the A1 mechanism is \
         unreachable from the standard library again:\n{out}"
    );
}

/// A program's OWN declaration of a native the stdlib also declares must win,
/// not collide.
///
/// The stdlib scan runs BEFORE `collect_declarations`, so without this
/// `stdlib/sync.mt` could not type-check as a program: the scan registered the
/// file's own declaration and the file then collided with itself, reporting
/// "already declared at line 257" AT line 257. Told apart by P80's
/// `Span::module`, which the scan stamps and a user file leaves as `None`.
#[test]
fn a_host_declaration_overrides_the_stdlib_one() {
    // The stdlib's clause must be firing first, or "no warning" below is
    // satisfied by there being no warning anywhere — the control's answer.
    let (_, baseline) = check("
use std::sync;
fn main() { let a: AtomicTrit = AtomicTrit::new(0); a.store(+); }
", &[]);
    assert!(
        baseline.contains("deprecated"),
        "nothing to override — the stdlib declaration is not in force:\n{baseline}"
    );
    let (ok, out) = check("
use std::sync;
extern \"c\" fn AtomicTrit::new(val: trit) -> AtomicTrit available(llvm, t3);
fn main() { let a: AtomicTrit = AtomicTrit::new(0); a.store(+); }
", &[]);
    assert!(ok, "a host declaration must replace the stdlib's, not collide:\n{out}");
    assert!(
        !out.contains("already declared"),
        "the stdlib scan collided with the program's own declaration:\n{out}"
    );
    // The host's declaration carries no `deprecated` clause, so the warning
    // must be GONE — which is what proves the override replaced rather than
    // merely tolerated the stdlib entry.
    assert!(
        !out.contains("deprecated"),
        "the stdlib's clause survived a host declaration that omits it:\n{out}"
    );
}

// ---------------------------------------------------------------------------
// §11.10 — closing a channel
// ---------------------------------------------------------------------------
//
// The reverse of §11.9's situation: `close` was implemented on BOTH backends
// before §11 was written, and `examples/concurrency.mt` depended on it, so the
// SPECIFICATION was the thing that lagged. Writing the rules down found two
// defects, both present on both backends and therefore invisible to the parity
// matrix.

/// §11.10 (CLOSE) wakes a task already blocked on the channel.
///
/// Before this, a receiver blocked in `recv` when another task closed the
/// channel was never woken, and the program hit §11.6's deadlock trap —
/// reporting that no runnable task could fill the channel, which was TRUE and
/// USELESS, because the close had already established that none ever would.
///
/// The `yield` is load-bearing: without it the spawned task has not reached
/// its `recv` when the close happens, so nothing is ever on `B(c)` and the row
/// passes against a `close` that wakes nobody. *The first draft of this row
/// did exactly that.*
#[test]
fn s11_10_close_wakes_a_blocked_receiver() {
    both_coop("
use std::io;
use std::async;
fn main() {
    let ch = channel<int>();
    spawn { io::println(\"A waiting\"); let v = ch.recv(); io::print(\"A got \"); io::println_int(v); }
    async::yield_now();
    io::println(\"main closing\");
    ch.close();
    io::println(\"main done\");
}
", "A waiting\nmain closing\nmain done\nA got 0\n", "§11.10 (CLOSE) wakes a waiter");
}

/// §11.10 (CLOSE) wakes **every** waiter — the one place in §11 where all are
/// woken rather than one.
///
/// Not an inconsistency with (SEND-WAKE), and the row is what makes the
/// difference observable: a `send` produces one value, so a second waiter
/// would find nothing and block again (§11.7's invisible bug); a `close`
/// produces no value but makes a PERMANENT fact true, so every waiter's `recv`
/// can complete. **Against a wake-one `close`, `B` is stranded and the program
/// deadlock-traps** — so this row fails loudly rather than silently, which is
/// the opposite of the (SEND-WAKE) case and worth knowing.
///
/// Derived by hand: two SPAWNs append A then B; (YIELD) sends main to the
/// back; A blocks, then B blocks, leaving B(ch) = [A, B]; main's close appends
/// both to R in that order and main then terminates.
#[test]
fn s11_10_close_wakes_every_waiter_not_one() {
    both_coop("
use std::io;
use std::async;
fn main() {
    let ch = channel<int>();
    spawn { io::println(\"A waiting\"); let v = ch.recv(); io::print(\"A got \"); io::println_int(v); }
    spawn { io::println(\"B waiting\"); let v = ch.recv(); io::print(\"B got \"); io::println_int(v); }
    async::yield_now();
    io::println(\"main closing\");
    ch.close();
    io::println(\"main done\");
}
",
    "A waiting\nB waiting\nmain closing\nmain done\nA got 0\nB got 0\n",
    "§11.10 (CLOSE) wakes ALL waiters, in B(c)'s order");
}

/// A closed channel still DRAINS — (RECV) is unchanged — and then
/// (RECV-CLOSED) yields the zero value without blocking.
///
/// `examples/concurrency.mt`'s consumer loop depends on both halves.
///
/// GUARD ROW: this passes on the pre-§11.10 compiler too, and that is the
/// point of it. Both backends already behaved this way — §11.10 wrote the rule
/// down rather than changing it, and the two defects that section found were
/// elsewhere. Without this row a later "simplification" of (RECV-CLOSED) into
/// a trap or a block would pass everything else in this file.
#[test]
fn s11_10_a_closed_channel_drains_then_yields_zero() {
    both_coop("
use std::io;
fn main() {
    let ch = channel<int>();
    ch.send(1); ch.send(2);
    ch.close();
    io::println_int(ch.recv());
    io::println_int(ch.recv());
    io::println_int(ch.recv());
}
", "1\n2\n0\n", "§11.10 (RECV) drains, then (RECV-CLOSED)");
}

/// The zero of (RECV-CLOSED) cannot be told from a sent zero, which is the
/// cost of that rule and the reason `try_recv` exists.
///
/// Pinned because it is a documented LIMIT rather than a defect: a row that
/// only checked `recv` would leave a reader thinking the two are
/// distinguishable.
#[test]
fn s11_10_try_recv_is_how_a_receiver_tells_closed_from_zero() {
    both_coop("
use std::io;
fn main() {
    let ch = channel<int>();
    ch.send(0);
    ch.close();
    match ch.try_recv() { Ok(v) => { io::print(\"ok \"); io::println_int(v); } Err(m) => { io::print(\"err \"); io::println(m); } }
    match ch.try_recv() { Ok(v) => { io::print(\"ok \"); io::println_int(v); } Err(m) => { io::print(\"err \"); io::println(m); } }
}
", "ok 0\nerr closed\n", "§11.10: try_recv distinguishes, recv cannot");
}

/// §11.10 (SEND-CLOSED): a send on a closed channel TRAPS, with one message on
/// both backends.
///
/// T3 used to drop the value in SILENCE while LLVM dropped it and wrote
/// `manit: send on closed channel` to stderr — both lost it, and they
/// disagreed about whether a program could tell. Trapping is P81's precedent:
/// the value has nowhere to go, and discarding it is data loss with no
/// diagnostic.
#[test]
fn s11_10_send_on_a_closed_channel_traps() {
    let src = "
use std::io;
fn main() {
    let ch = channel<int>();
    ch.close();
    ch.send(9);
    io::println(\"unreachable\");
}
";
    let t3 = run_t3(src, "inline");
    assert!(t3.contains("TRAP: send on a closed channel"),
            "T3 must trap on a send to a closed channel, got {t3:?}");
    assert!(!t3.contains("unreachable"), "execution continued past the trap: {t3:?}");
    if let Some(ll) = run_llvm(src, "inline") {
        assert!(ll.contains("TRAP: send on a closed channel"),
                "LLVM must trap identically, got {ll:?}");
        assert!(!ll.contains("unreachable"), "execution continued past the trap: {ll:?}");
    }
}

/// Closing twice is idempotent — (CLOSE) on an already-closed channel finds
/// `B(c) = ε` and adds a member the set already has.
#[test]
fn s11_10_close_is_idempotent() {
    both_coop("
use std::io;
fn main() {
    let ch = channel<int>();
    ch.send(7);
    ch.close();
    ch.close();
    io::println_int(ch.recv());
    io::println(\"ok\");
}
", "7\nok\n", "§11.10: close is idempotent");
}

// ---------------------------------------------------------------------------
// §11.11 — bounded channels
// ---------------------------------------------------------------------------

/// (SEND-BLOCK) and (RECV-WAKE) on both backends: a producer stops at the
/// bound and resumes when a receive frees a slot.
///
/// The two backends reach it differently — T3 re-executes the blocking
/// syscall after a PC rewind, LLVM returns from `manit_sched_block_on` into a
/// `while` loop in the C runtime — so agreeing on this trace is evidence
/// rather than a shared lowering agreeing with itself.
#[test]
fn s11_11_a_full_send_blocks_and_a_recv_wakes_it() {
    both_coop("
use std::io;
use std::async;
fn main() {
    let ch = channel<int>(2);
    spawn { mut i = 1; while i <= 4 { ch.send(i); io::println(\"sent\"); i = i + 1; } }
    async::yield_now();
    mut n = 0;
    while n < 4 { let v = ch.recv(); io::println(\"got\"); n = n + 1; }
    io::println(\"done\");
}
", "sent\nsent\ngot\ngot\nsent\nsent\ngot\ngot\ndone\n", "§11.11 (SEND-BLOCK)/(RECV-WAKE)");
}

/// P107: an UNBOUNDED channel is unbounded, and §11.4's original three yield
/// points still hold for it.
///
/// **This is the row that would have caught P107.** `channel_new` allocated a
/// 256-slot ring on LLVM and a 257th send blocked on a condition variable
/// nothing could signal: the program printed NOTHING AT ALL, while T3 grew its
/// queue and answered. §11.1 said channels are unbounded and §11.4 gave that
/// as the reason `send` is not a yield point — so the clause justifying the
/// list of three was the one the implementation contradicted.
#[test]
fn p107_an_unbounded_channel_holds_more_than_256() {
    let src = "
use std::io;
fn main() {
    let ch = channel<int>();
    mut i = 0;
    while i < 300 { ch.send(i); i = i + 1; }
    io::println(\"sent 300\");
    mut s = 0; mut j = 0;
    while j < 300 { s = s + ch.recv(); j = j + 1; }
    io::println_int(s);
}
";
    let want = "sent 300\n44850\n";
    assert_eq!(run_t3(src, "inline"), want, "T3");
    if let Some(ll) = run_llvm(src, "inline") {
        assert_eq!(ll, want, "LLVM: `channel_new` is bounded again");
    }
}

/// A capacity below 1 traps rather than clamping, on both backends.
#[test]
fn s11_11_a_capacity_below_one_traps() {
    let src = "
use std::io;
fn main() { let ch = channel<int>(0); io::println(\"unreachable\"); }
";
    let t3 = run_t3(src, "inline");
    assert!(t3.contains("TRAP: a channel capacity must be at least 1"), "T3: {t3:?}");
    assert!(!t3.contains("unreachable"), "execution continued: {t3:?}");
    if let Some(ll) = run_llvm(src, "inline") {
        assert!(ll.contains("a channel capacity must be at least 1"), "LLVM: {ll:?}");
        assert!(!ll.contains("unreachable"), "execution continued: {ll:?}");
    }
}

/// §11.6 counts `S` as well as `B`, and says **drain** rather than **fill**.
///
/// The row asserts the WORD: a task waiting for ROOM and one waiting for a
/// VALUE are both deadlocked, and the wrong word sends the reader looking for
/// a missing sender when the problem is a missing receiver.
#[test]
fn s11_11_a_full_channel_deadlock_says_drain() {
    let out = run_t3("
use std::io;
use std::async;
fn main() {
    let ch = channel<int>(1);
    spawn { ch.send(1); ch.send(2); io::println(\"sender done\"); }
    async::yield_now();
    io::println(\"main done\");
}
", "cooperative");
    assert!(
        out.contains("no runnable task can drain"),
        "§11.6 must say `drain` for a channel nobody will receive from: {out:?}"
    );
    assert!(!out.contains("sender done"), "the sender must not complete: {out:?}");
}


// ---------------------------------------------------------------------------
// §11.12 — `Task<T>` and `await`, on both backends and in BOTH scheduling modes
//
// The two backends reach these by different mechanisms, which is what makes
// their agreement evidence rather than a shared lowering agreeing with itself:
// **T3 forks** — the parent's `__task_fork` result is the handle and the child
// exits through syscall 139 carrying the block's value — while **LLVM
// outlines** the body into `__spawn_body_N(ptr env)` and its trampoline
// completes the handle from the body's return value.
//
// And a third path with neither: under `--sched inline`, which is still the
// default, `spawn { B }` runs `B` in place and the handle is born `done(v)`.
// §11.12's decision 1 is what makes that a legal task rather than a special
// case, and every row below runs in both modes for exactly that reason.
// ---------------------------------------------------------------------------

/// Both backends, both scheduling modes, one expected trace.
fn all_four(src: &str, want: &str, what: &str) {
    for sched in ["inline", "cooperative"] {
        assert_eq!(run_t3(src, sched), want, "{what}: T3 under --sched {sched}");
        if let Some(ll) = run_llvm(src, sched) {
            assert_eq!(ll, want, "{what}: LLVM under --sched {sched}");
        }
    }
}

#[test]
fn s11_12_await_returns_the_blocks_value() {
    all_four(
        "fn main() {\n    let t: Task<int> = spawn { 42 };\n\
         \x20   io::print(\"v=\"); io::print_int(await t); io::println(\"\");\n}\n",
        "v=42\n",
        "§11.12: `spawn { B } : Task<T>` and `await` is its `recv`",
    );
}

#[test]
fn s11_12_a_float_survives_the_handle_unconverted() {
    // The value travels as a BIT PATTERN through one machine word. This is the
    // row that fails if anything on the path CONVERTS instead of
    // reinterpreting — 1.5 would arrive as 1, which is P65's erasure and P92's
    // float-in-a-word one construct later.
    //
    // It is also the row that caught the first design: the word was produced by
    // storing at the value's type and loading at `i64` through one `alloca`,
    // and `codegen_llvm` re-types a load to whatever was stored (precisely so a
    // store/load pair cannot disagree), so the outlined body emitted
    // `ret i64 %t1` for a `double` and clang refused the module.
    all_four(
        "fn main() {\n    let t: Task<float> = spawn { 1.5 };\n\
         \x20   io::print(\"f=\"); io::print_float(await t); io::println(\"\");\n}\n",
        "f=1.5\n",
        "§11.12: a float payload is reinterpreted, not converted",
    );
}

#[test]
fn s11_12_awaiting_twice_traps() {
    // §11.12 decision 2, and the message is the document's own words. Returning
    // the value again is available and rejected: a program awaiting one handle
    // twice has almost certainly confused two handles, and a detected error
    // beats a plausible continuation.
    //
    // **The value is asserted BEFORE the trap**, not just the trap: a row that
    // checked only for the message would pass on an implementation that trapped
    // on the FIRST await too.
    let src = "fn main() {\n    let t: Task<int> = spawn { 7 };\n\
               \x20   io::print_int(await t);\n    io::print_int(await t);\n}\n";
    for sched in ["inline", "cooperative"] {
        let t3 = run_t3(src, sched);
        assert!(t3.starts_with('7'), "the FIRST await must succeed: {t3:?}");
        assert!(
            t3.contains("await on a task whose value has already been taken"),
            "§11.12 decision 2, T3 under --sched {sched}: {t3:?}"
        );
        if let Some(ll) = run_llvm(src, sched) {
            assert!(ll.starts_with('7'), "the FIRST await must succeed: {ll:?}");
            assert!(
                ll.contains("await on a task whose value has already been taken"),
                "§11.12 decision 2, LLVM under --sched {sched}: {ll:?}"
            );
        }
    }
}

#[test]
fn s11_12_await_blocks_on_a_task_that_has_not_finished() {
    // (AWAIT-BLOCK): an unfinished task is an empty one-shot channel, so this
    // is §11.4's point 2 and the list of yield points does not grow. Cooperative
    // only — under `--sched inline` the block has already run, which is the
    // whole of decision 1.
    both_coop(
        "fn main() {\n    let ch: Channel<int> = channel<int>();\n\
         \x20   let t: Task<int> = spawn { let x: int = ch.recv(); x * 2 };\n\
         \x20   ch.send(21);\n\
         \x20   io::print(\"v=\"); io::print_int(await t); io::println(\"\");\n}\n",
        "v=42\n",
        "§11.12 (AWAIT-BLOCK) then (DONE-T)",
    );
}

#[test]
fn s11_12_spawn_as_a_statement_is_unmoved() {
    // §11.12's "what this costs": **nothing, for programs that exist**. The
    // handle is a return value rather than a change to the form, so a `spawn`
    // used as a statement discards it exactly as any expression statement does.
    //
    // **This row PASSES on the compiler without §11.12**, and saying so is the
    // honest half of permanent rule 9: it records the BOUNDARY the change is
    // drawn along rather than the change itself. The other five §11.12 rows are
    // red on the control; this one cannot be, because what it asserts is that
    // nothing moved.
    all_four(
        "fn main() {\n    let ch: Channel<int> = channel<int>();\n\
         \x20   spawn { ch.send(3); };\n\
         \x20   io::print(\"got=\"); io::print_int(ch.recv()); io::println(\"\");\n}\n",
        "got=3\n",
        "§11.12: `spawn` as a statement is unchanged",
    );
}

#[test]
fn s11_12_deadlock_names_the_await_and_not_a_channel() {
    // §11.12's §11.6 clause. A task blocked in `await` is in B, so §11.6
    // already covers it — what it must not do is report a CHANNEL nobody is
    // waiting on. The wrong word sends the reader looking for a missing sender
    // when the problem is a task that cannot finish, which is §11.11's lesson
    // about "fill" and "drain" one construct along.
    //
    // The backends compute this differently and that is the point: T3 asks its
    // `blocked_await` map, LLVM walks its handle list. Both had the channel
    // message first, and LLVM kept it until this row was written.
    let src = "fn main() {\n    let ch: Channel<int> = channel<int>();\n\
               \x20   let t: Task<int> = spawn { let x: int = ch.recv(); x };\n\
               \x20   io::print_int(await t);\n}\n";
    let t3 = run_t3(src, "cooperative");
    assert!(
        t3.contains("blocked awaiting a task that cannot finish"),
        "§11.6 with tasks, T3: {t3:?}"
    );
    if let Some(ll) = run_llvm(src, "cooperative") {
        assert!(
            ll.contains("blocked awaiting a task that cannot finish"),
            "§11.6 with tasks, LLVM: {ll:?}"
        );
    }
}
