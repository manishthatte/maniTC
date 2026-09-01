//! Concurrency step 3a — the cooperative scheduler in the C runtime.
//!
//! © Manish Jagdish Thatte
//!
//! `docs/semantics.md` §11 is normative and the T3 emulator already implements
//! it (`codegen_t3/emulator/sched.rs`, P88/P89). This is the same section for
//! the LLVM backend, where a task's continuation IS a C call stack.
//!
//! **Nothing in ManiT reaches it yet**, which is why these rows drive the C ABI
//! directly. `--sched cooperative --target llvm` is still refused (P89) and the
//! scheduler is inert until `__task_bootstrap` is called: measured, not
//! assumed — the whole example set is byte-identical on both backends with the
//! scheduler present. The lowering that will call it is step 3b.
//!
//! **Why threads with a baton and not `ucontext`.** §11.2 gives a spawned task
//! a COPY of the spawning task's store, and copying a live C stack is what P88
//! refused to do on T3 for a reason that applies here with more force: a copied
//! frame can hold the address of one of its own slots. T3 could dodge it by
//! keeping every stack at the SAME addresses and swapping the live window,
//! because the emulator owns its memory map; a hosted process does not own its
//! stack. So each task gets a real stack from the start and the captured store
//! is passed explicitly in `env`. Determinism does not depend on the threads
//! being fair — exactly one task holds the baton, and handoffs happen only at
//! §11.4's three yield points.

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static N: AtomicUsize = AtomicUsize::new(0);

fn workdir() -> PathBuf {
    let slot = N.fetch_add(1, Ordering::Relaxed);
    let d = std::env::temp_dir()
        .join(format!("manitc_sched_{}", std::process::id()))
        .join(slot.to_string());
    std::fs::create_dir_all(&d).expect("temp dir");
    d
}

fn runtime_dir() -> Option<PathBuf> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("runtime");
    p.join("manit_runtime.c").exists().then_some(p)
}

fn cc() -> &'static str {
    if Command::new("cc").arg("--version").output().is_ok() { "cc" } else { "gcc" }
}

/// Compile one C program against the runtime; `Some(path)` iff a binary appeared.
fn build(body: &str) -> Option<PathBuf> {
    let rt = runtime_dir()?;
    let d = workdir();
    let src = d.join("t.c");
    std::fs::write(&src, format!("#define MANIT_NO_GUI 1\n#include \"manit_runtime.c\"\n{body}"))
        .expect("write");
    let bin = d.join("t");
    let _ = Command::new(cc())
        .args(["-I", rt.to_str().unwrap(), "-DMANIT_NO_GUI", "-o", bin.to_str().unwrap()])
        .arg(&src)
        .args(["-lm", "-lpthread"])
        .output();
    bin.exists().then_some(bin)
}

/// Whether this environment can build against the runtime AT ALL.
///
/// **THE CONTROL COMPILE IS THE WHOLE POINT OF THIS FUNCTION.** The first
/// version of these rows skipped whenever no binary appeared, which is P47's
/// defect exactly: a compiler that lacks the scheduler produces no binary
/// either, so all five rows went GREEN on the pre-change tree in 0.05 seconds,
/// having compiled nothing. Telling the two states apart needs a program that
/// uses only what the runtime has ALWAYS had — if that builds, a failure to
/// build the harness is a real failure and not an absent toolchain.
fn toolchain_present() -> bool {
    build("int main(void){ return 0; }").is_some()
}

/// Build and run, asserting the build succeeded when the toolchain works.
fn build_and_run(body: &str) -> Option<(String, i32)> {
    if !toolchain_present() {
        return None;
    }
    let bin = build(body).expect(
        "the C toolchain builds against this runtime, so failing to build the \
         harness is a REAL failure — most likely the scheduler ABI is missing",
    );
    let r = Command::new(&bin).output().expect("run");
    Some((
        String::from_utf8_lossy(&r.stdout).into_owned()
            + &String::from_utf8_lossy(&r.stderr),
        r.status.code().unwrap_or(-1),
    ))
}

const TASKS: &str = r#"
static ManitChan* ch;
static int64_t a(int64_t* e){ (void)e; puts("A1"); __task_yield(); puts("A2"); return 0; }
static int64_t b(int64_t* e){ (void)e; puts("B1"); __task_yield(); puts("B2"); return 0; }
"#;

#[test]
fn s11_spawn_appends_and_does_not_yield() {
    // §11.4: "`spawn` does not yield. The spawning task continues; the new task
    // is appended at the back." So `main-after-spawn` must precede A1, and the
    // round then runs A, B, main in the order they entered R.
    let Some((out, _)) = build_and_run(&format!("{TASKS}
int main(void){{ __task_bootstrap();
  __task_spawn(a, NULL); __task_spawn(b, NULL);
  puts(\"main-after-spawn\"); __task_yield(); puts(\"main-2\");
  __task_main_done(); return 0; }}")) else { return };
    let want = "main-after-spawn\nA1\nB1\nmain-2\nA2\nB2\n";
    assert!(out.starts_with(want), "§11.4/§11.5: wrong interleaving.\ngot:\n{out}\nwant prefix:\n{want}");
}

#[test]
fn s11_recv_blocks_and_send_wakes_the_waiter() {
    // §11.5 (RECV-BLOCK) then (SEND-WAKE). The receiver is spawned FIRST and
    // blocks; each send appends a value and wakes at most one waiter.
    let Some((out, code)) = build_and_run(r#"
static ManitChan* ch;
static int64_t cons(int64_t* e){ (void)e;
  for (int i=0;i<2;i++) printf("got %lld\n",(long long)channel_recv(ch)); return 0; }
static int64_t prod(int64_t* e){ printf("send %lld\n",(long long)e[0]);
  channel_send(ch, e[0]); return 0; }
static int64_t* one(int64_t v){ int64_t* p = malloc(sizeof(int64_t)); p[0]=v; return p; }
int main(void){ __task_bootstrap(); ch = channel_new();
  __task_spawn(cons, NULL); __task_spawn(prod, one(11)); __task_spawn(prod, one(22));
  __task_main_done(); puts("end"); return 0; }
"#) else { return };
    assert_eq!(code, 0, "expected a normal end, got {code}:\n{out}");
    for want in ["send 11", "send 22", "got 11", "got 22", "end"] {
        assert!(out.contains(want), "§11.5: missing {want:?} in:\n{out}");
    }
    assert!(out.find("got 11").unwrap() < out.find("got 22").unwrap(),
            "§11.5: the channel is a QUEUE; 11 was sent first:\n{out}");
}

#[test]
fn s11_deadlock_is_a_trap_that_keeps_the_trace() {
    // §11.6: R empty and B non-empty is a TRAP, not a hang — "the scheduler
    // knows the whole runnable set, so it can detect a deadlock a pthread
    // runtime can only suffer".
    //
    // THE RETAINED TRACE IS THE POINT, and it is what P5.1 could not do: on
    // LLVM the same shape blocked in `pthread_cond_wait` with stdout unflushed
    // and printed NOTHING AT ALL, losing the trace along with the answer. So
    // this row asserts the output BEFORE the trap, not only the trap.
    let Some((out, code)) = build_and_run(r#"
static ManitChan* ch;
static int64_t w(int64_t* e){ (void)e; puts("waiting"); channel_recv(ch); puts("never"); return 0; }
int main(void){ __task_bootstrap(); ch = channel_new();
  puts("before"); __task_spawn(w, NULL); __task_main_done(); puts("unreachable"); return 0; }
"#) else { return };
    assert!(out.contains("deadlock"), "§11.6: expected a deadlock trap:\n{out}");
    assert!(out.contains("before") && out.contains("waiting"),
            "§11.6: the trace before the trap was lost — this is P5.1:\n{out}");
    assert!(!out.contains("unreachable") && !out.contains("never"),
            "§11.6: execution continued past the trap:\n{out}");
    assert_ne!(code, 0, "§11.6: a trap must not exit 0:\n{out}");
}

#[test]
fn s11_the_schedule_is_deterministic() {
    // §11.7: `→` is a partial function, so a core program has exactly ONE
    // observable behaviour. These are real OS threads and determinism does not
    // depend on them being fair — it comes from the baton. Pinned by repetition
    // for the same reason the reference interpreter pins its own.
    let prog = format!("{TASKS}
static int64_t* one(int64_t v){{ int64_t* p = malloc(sizeof(int64_t)); p[0]=v; return p; }}
static int64_t prod(int64_t* e){{ channel_send(ch, e[0]); return 0; }}
static int64_t cons(int64_t* e){{ (void)e;
  for (int i=0;i<2;i++) printf(\"got %lld\\n\",(long long)channel_recv(ch)); return 0; }}
int main(void){{ __task_bootstrap(); ch = channel_new();
  __task_spawn(a, NULL); __task_spawn(b, NULL); __task_spawn(cons, NULL);
  __task_spawn(prod, one(1)); __task_spawn(prod, one(2));
  __task_yield(); __task_main_done(); return 0; }}");
    let Some((first, _)) = build_and_run(&prog) else { return };
    for i in 0..40 {
        let (again, _) = build_and_run(&prog).expect("it built once already");
        assert_eq!(first, again, "§11.7: run {i} differed:\n{first}\n---\n{again}");
    }
}

#[test]
fn s11_the_scheduler_is_inert_until_bootstrap() {
    // The whole reason this can ship before the lowering does. Without
    // `__task_bootstrap`, `__task_spawn` runs the body IN PLACE — which is
    // `docs/memory-model.md` §4, what every ManiT program has always done —
    // and a channel keeps its condition-variable behaviour.
    let Some((out, code)) = build_and_run(r#"
static int64_t body(int64_t* e){ printf("body %lld\n",(long long)e[0]); return 0; }
int main(void){
  int64_t* p = malloc(sizeof(int64_t)); p[0]=7;
  puts("before"); __task_spawn(body, p); puts("after"); return 0; }
"#) else { return };
    assert_eq!(code, 0, "expected a normal end:\n{out}");
    assert!(out.starts_with("before\nbody 7\nafter\n"),
            "without bootstrap the body must run IN PLACE, as §4 always did:\n{out}");
}
