//! Definitional interpreter for the ManiT core.
//!
//! © Manish Jagdish Thatte
//!
//! This implements docs/semantics.md and nothing else. Where the two differ,
//! this file is wrong. It is written for auditability rather than speed — the
//! recommendations ask for "deliberately slow and obviously correct" — so every
//! rule appears once, in the order the document states it, with the document's
//! section number on it.
//!
//! Independence rule: see lex.rs.

use super::ast::*;
use std::collections::{HashMap, VecDeque};
use std::sync::{Condvar, Mutex, MutexGuard};

/// §3. T3_MAX = (3^27 - 1) / 2.
pub const T3_MAX: i64 = 3_812_798_742_493;

/// Which version of the language this account is evaluating (R2).
///
/// The reference keeps its OWN copy of this distinction rather than importing
/// the compiler's `LangVersion`, for the reason the whole module exists: a
/// reference implementation that shares a definition with the thing it checks
/// cannot witness a mistake in that definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    /// `/` truncates toward zero, `%` takes the dividend's sign.
    #[default]
    V1,
    /// C4: `/` rounds to nearest, ties away from zero; `%` is the balanced
    /// remainder that pairs with it.
    V2,
}

/// C4, written from `docs/semantics.md` §6.1 rather than from the compiler.
///
/// Deliberately the OBVIOUS transcription of the rule — widen, take absolute
/// values, compare `2|r|` against `|b|` — and not the negative-magnitude form
/// the compiler and both backends use. If the two forms disagree anywhere, the
/// conformance suite is what says so, and it can only say so while they are
/// written differently.
fn div_nearest_ref(a: i64, b: i64) -> i64 {
    let (x, y) = (a as i128, b as i128);
    let q = x / y;
    let r = x - q * y;
    if r == 0 {
        return q as i64;
    }
    if 2 * r.abs() >= y.abs() {
        // Ties away from zero: away from the sign the quotient itself has.
        if (x < 0) == (y < 0) { (q + 1) as i64 } else { (q - 1) as i64 }
    } else {
        q as i64
    }
}
const LANES: usize = 27;

/// §3. Four value forms. `Trit` and `Bool3` share a carrier and differ only in
/// type; `Bool` does not share it — that is the hazard §6.4 documents.
#[derive(Debug, Clone, PartialEq)]
pub enum Val {
    Int(i64),
    Trit(i8),
    Bool3(i8),
    Bool(bool),
    /// §6.8. A `Result<T, str>`.
    Res(Res),
    /// A string. In the core it exists only to be printed or carried as a
    /// `Result` message, so it has no operations of its own.
    Text(String),
    /// §11.3. A channel, as an index into the configuration's channel store
    /// `𝒞`. An index and not a pointer because a channel has IDENTITY and no
    /// contents of its own: `𝒞` and the blocked map `B` are indexed by it, and
    /// copying the value copies the name of the channel rather than the queue.
    /// That is what lets §11.2 hand a spawned task a COPY of its spawner's
    /// store and still have the two share the channel.
    Chan(usize),
}

/// §6.8. A `Result<T, str>` value.
///
/// One carrier for three outcomes, with the tag as a TRIT rather than a
/// boolean pair — which is the whole design claim: `Ok`, `Unknown` and `Err`
/// are three coordinate answers, not "success" and two flavours of failure.
/// `Unknown` carries a reason and is NOT a kind of `Err`.
#[derive(Debug, Clone, PartialEq)]
pub struct Res {
    /// +1 Ok, 0 Unknown, -1 Err.
    pub tag: i8,
    /// Present only when `tag == 1`.
    pub val: Option<Box<Val>>,
    /// The message carried by `Unknown` and `Err`; empty for `Ok`.
    pub msg: String,
}

impl Val {
    /// How `io::print_int` sees a value: the carrier, as an integer.
    fn as_print_int(&self) -> i64 {
        match self {
            Val::Int(n) => *n,
            Val::Trit(t) | Val::Bool3(t) => *t as i64,
            Val::Bool(b) => *b as i64,
            // §6.8: a Result's integer face is its TAG — the trit that says
            // which of the three outcomes it is. That is what `io::print_int`
            // shows and what `tif` dispatches on.
            Val::Res(r) => r.tag as i64,
            Val::Text(_) => 0,
            // A channel has no numeric face. §11 defines no cast to or from
            // one and no printer for one, so this is unreachable from a
            // program that parses; 0 rather than a panic because the core's
            // rule is that `cast` is total.
            Val::Chan(_) => 0,
        }
    }
}

/// §8. A trap ends the program; the trace produced so far is retained.
pub struct Trap(pub String);

/// Why an evaluation stopped early.
///
/// `?` is not a trap: it unwinds to the enclosing function and becomes that
/// function's return value (§6.9). Modelling both with one channel is what
/// keeps `?` out of the expression type — every expression would otherwise
/// have to return "a value or a propagation", and the rules in §5 would be
/// written twice.
pub enum Abort {
    Trap(Trap),
    /// The non-`Ok` `Result` being propagated out of the enclosing call.
    Propagate(Val),
    /// §11. The program has already ended under some other task — a trap
    /// there, or §11.6's deadlock — so this one stops where it stands.
    ///
    /// It is NOT a trap and contributes nothing to the observation: the event
    /// that ended the program is already recorded, and a cancelled task
    /// reporting a second one would make the outcome depend on which thread
    /// the operating system happened to wake.
    Cancelled,
}

type R<T> = Result<T, Abort>;

fn trap<T>(msg: impl Into<String>) -> R<T> {
    Err(Abort::Trap(Trap(msg.into())))
}

/// §7. `return` is not an expression, so it needs its own control-flow path.
enum Flow { Normal, Return(Option<Val>) }


// ---------------------------------------------------------------------------
// §11 The scheduler
// ---------------------------------------------------------------------------

/// §11.3. A task's name.
pub type TaskId = usize;

/// §11.3. The parts of a configuration that are shared between tasks: the run
/// queue `R`, the blocked map `B`, the channel store `𝒞`, and the trace `ω`.
///
/// One mutex covers all four, and §11.4 is the reason that is not a
/// bottleneck: at most one task is ever runnable, so the lock is never
/// contended. **Its job is to make "exactly one task runs at a time"
/// expressible in Rust, not to arbitrate between tasks that might otherwise
/// race** — under §11.2 they have nothing to race over.
struct Shared {
    /// §11.3 ω. One trace for the whole program: output is a program-level
    /// observable, and §11.3 says which task produced which part of it is not.
    out: String,
    /// The step budget of §4's note, shared rather than per-task: a
    /// non-terminating PROGRAM is what it exists to stop.
    budget: u64,
    /// §11.3 R. The head is the running task.
    run: VecDeque<TaskId>,
    /// §11.3 B, one queue per channel so (SEND-WAKE) can take the
    /// longest-waiting waiter rather than an arbitrary one.
    blocked: Vec<VecDeque<TaskId>>,
    /// §11.3 𝒞.
    chans: Vec<VecDeque<Val>>,
    /// §11.10 𝒦, the closed set. One flag per channel rather than a set,
    /// because channels are named by index here and the two are the same
    /// thing at this size.
    closed: Vec<bool>,
    /// §11.11: each channel's capacity, 0 meaning UNBOUNDED. Unbounded is the
    /// default and what every channel was before 0.7.
    caps: Vec<usize>,
    /// §11.11 `S`, the send-blocked map. A SECOND map and not an extension of
    /// `blocked`: a `recv` must wake a SENDER and a `send` must wake a
    /// RECEIVER, and one queue holding both lets either wake the wrong kind.
    blocked_send: Vec<VecDeque<TaskId>>,
    /// §8: the first trap ends the program, so later ones are not recorded.
    trap: Option<String>,
    /// The program is over — by a trap, or by §11.6's deadlock, or normally.
    /// A task waiting for its turn wakes, sees this, and unwinds.
    stopping: bool,
}

/// §11.3–§11.6, as an object. Every method is named for the rule it implements.
struct Sched {
    m: Mutex<Shared>,
    cv: Condvar,
    /// Ids handed out. Separate from `Shared` only because it is never read
    /// under the same lock as anything else.
    next_id: Mutex<TaskId>,
}

impl Sched {
    fn new(budget: u64) -> Self {
        Sched {
            m: Mutex::new(Shared {
                out: String::new(),
                budget,
                // §11.3: a program starts as ONE task.
                run: VecDeque::from([0usize]),
                blocked: Vec::new(),
                chans: Vec::new(),
                closed: Vec::new(),
                caps: Vec::new(),
                blocked_send: Vec::new(),
                trap: None,
                stopping: false,
            }),
            cv: Condvar::new(),
            next_id: Mutex::new(1),
        }
    }

    /// A poisoned lock cannot happen here — no task panics while holding it,
    /// and a ManiT trap is a value rather than a panic — but recovering rather
    /// than unwrapping keeps a bug in this file from becoming a hang in the
    /// harness.
    fn lock(&self) -> MutexGuard<'_, Shared> {
        self.m.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Block until `id` is the head of the run queue — §11.3's "the head of
    /// `R` is the running task" — or the program ends underneath it.
    fn await_turn(&self, id: TaskId) -> R<()> {
        let mut g = self.lock();
        loop {
            // Checked FIRST: a task that is at the head of a run queue nobody
            // will ever advance is still cancelled.
            if g.stopping {
                return Err(Abort::Cancelled);
            }
            if g.run.front() == Some(&id) {
                return Ok(());
            }
            g = self.cv.wait(g).unwrap_or_else(|e| e.into_inner());
        }
    }

    /// §11.3 ω.
    fn emit(&self, text: &str) {
        self.lock().out.push_str(text);
    }

    fn tick(&self) -> R<()> {
        let mut g = self.lock();
        if g.budget == 0 {
            return trap("step budget exhausted (non-terminating?)");
        }
        g.budget -= 1;
        Ok(())
    }

    /// A fresh channel: an entry in 𝒞 and its own queue in B.
    fn new_channel(&self, cap: usize) -> usize {
        let mut g = self.lock();
        g.chans.push(VecDeque::new());
        g.blocked.push(VecDeque::new());
        g.blocked_send.push(VecDeque::new());
        g.closed.push(false);
        g.caps.push(cap);
        g.chans.len() - 1
    }

    /// §11.5 (SPAWN). Appends at the back, and does **not** yield — §11.4
    /// records that as a choice: the spawning task's own code reads
    /// sequentially.
    fn enqueue_new_task(&self) -> TaskId {
        let mut n = self.next_id.lock().unwrap_or_else(|e| e.into_inner());
        let id = *n;
        *n += 1;
        self.lock().run.push_back(id);
        id
    }

    /// §11.5 (YIELD). The head goes to the back.
    ///
    /// When it is the only runnable task this is the identity, which is what
    /// the rule says: `R·⟨s,σ⟩` with `R` empty is `⟨s,σ⟩`.
    fn yield_now(&self, id: TaskId) -> R<()> {
        {
            let mut g = self.lock();
            debug_assert_eq!(g.run.front(), Some(&id), "§11.3: only the head runs");
            g.run.pop_front();
            g.run.push_back(id);
        }
        self.cv.notify_all();
        self.await_turn(id)
    }

    /// §11.5 (SEND) and (SEND-WAKE).
    ///
    /// Never blocks: §11.1 leaves channels unbounded, which §11.4 gives as the
    /// reason `send` is not a fourth yield point. Exactly one waiter is woken,
    /// and it is the longest-waiting.
    fn send(&self, id: TaskId, c: usize, v: Val) -> R<()> {
        // §11.11 (SEND-BLOCK). The LOOP is the specification, exactly as it is
        // for `recv`: a woken sender RE-EXECUTES its send, because a third
        // task may have taken the space it was woken for. An unbounded channel
        // is never full, so a program that asks for no capacity never enters
        // it and §11.4's original three yield points remain exactly its yield
        // points.
        loop {
            {
                let mut g = self.lock();
                // §11.10 (SEND-CLOSED). Tested FIRST: a sender woken by
                // (CLOSE) must trap rather than re-block.
                if g.closed[c] {
                    return trap("send on a closed channel — the value cannot be received");
                }
                let cap = g.caps[c];
                if cap == 0 || g.chans[c].len() < cap {
                    g.chans[c].push_back(v);
                    if let Some(w) = g.blocked[c].pop_front() {
                        g.run.push_back(w);
                    }
                    break;
                }
                debug_assert_eq!(g.run.front(), Some(&id), "§11.3: only the head runs");
                g.run.pop_front();
                g.blocked_send[c].push_back(id);
                self.check_end(&mut g);
            }
            self.cv.notify_all();
            self.await_turn(id)?;
        }
        self.cv.notify_all();
        Ok(())
    }

    /// §11.10 (CLOSE). Marks the channel closed and wakes **every** task
    /// waiting on it, in `B(c)`'s own order.
    ///
    /// The one place in §11 where all waiters are woken rather than one, and
    /// not an inconsistency with (SEND-WAKE): a `send` produces exactly one
    /// value so only one waiter can proceed, while a `close` produces no value
    /// but makes a PERMANENT fact true of the channel, and every waiter's
    /// `recv` can now complete with (RECV-CLOSED)'s zero. A waiter left on
    /// `B(c)` after a close is stranded forever, because no `send` will ever
    /// wake it.
    ///
    /// Idempotent: a second close finds `B(c)` already empty.
    fn close(&self, c: usize) {
        {
            let mut g = self.lock();
            g.closed[c] = true;
            while let Some(w) = g.blocked[c].pop_front() {
                g.run.push_back(w);
            }
            // §11.11: the SENDERS too. Each re-executes its send, finds the
            // channel closed and traps by (SEND-CLOSED).
            while let Some(w) = g.blocked_send[c].pop_front() {
                g.run.push_back(w);
            }
        }
        self.cv.notify_all();
    }

    /// §11.5 (RECV) and (RECV-BLOCK).
    ///
    /// **The loop is the specification, not an optimisation.** (RECV-BLOCK)
    /// puts the task into `B` with its `recv` still in front of it, so being
    /// woken means re-executing the receive rather than being handed a value.
    /// That is what makes an intervening `recv` by a third task — which may
    /// take the very value this one was woken for — behave here exactly as the
    /// rules say, instead of by whatever the implementation found convenient.
    fn recv(&self, id: TaskId, c: usize) -> R<Val> {
        loop {
            {
                let mut g = self.lock();
                if let Some(v) = g.chans[c].pop_front() {
                    // §11.11 (RECV-WAKE): a receive frees exactly one slot, so
                    // exactly one sender is woken — the longest-waiting.
                    if let Some(w) = g.blocked_send[c].pop_front() {
                        g.run.push_back(w);
                    }
                    drop(g);
                    self.cv.notify_all();
                    return Ok(v);
                }
                // §11.10 (RECV-CLOSED) takes precedence over (RECV-BLOCK): a
                // drained, CLOSED channel completes the receive with the zero
                // value instead of blocking. The zero is a real value and
                // cannot be told from a sent one — that is the cost of the
                // rule, and why `try_recv` exists.
                if g.closed[c] {
                    return Ok(Val::Int(0));
                }
                debug_assert_eq!(g.run.front(), Some(&id), "§11.3: only the head runs");
                g.run.pop_front();
                g.blocked[c].push_back(id);
                self.check_end(&mut g);
            }
            self.cv.notify_all();
            self.await_turn(id)?;
        }
    }

    /// §11.5 (DONE).
    fn finish(&self, id: TaskId) {
        {
            let mut g = self.lock();
            if let Some(at) = g.run.iter().position(|t| *t == id) {
                g.run.remove(at);
            }
            self.check_end(&mut g);
        }
        self.cv.notify_all();
    }

    /// §11.6, the two end conditions, read off the configuration itself.
    ///
    /// Called wherever a task leaves the run queue, which is the only way `R`
    /// can become empty. If anything is still waiting on a channel, no
    /// runnable task can ever fill it — that is not an inference about the
    /// future but a fact about the present configuration, and it is the
    /// property a pthread runtime cannot compute.
    fn check_end(&self, g: &mut Shared) {
        if !g.run.is_empty() || g.stopping {
            return;
        }
        // §11.11: `S` counts as much as `B`, and the message names which —
        // "fill" for a channel nobody will send to, "drain" for one nobody
        // will receive from. The wrong word sends the reader looking for a
        // missing sender when the problem is a missing receiver.
        if g.blocked_send.iter().any(|q| !q.is_empty()) {
            g.trap = Some(
                "deadlock — every task is blocked on a channel that no runnable \
                 task can drain"
                    .into(),
            );
        } else if g.blocked.iter().any(|q| !q.is_empty()) {
            g.trap = Some(
                "deadlock — every task is blocked on a channel that no runnable \
                 task can fill"
                    .into(),
            );
        }
        g.stopping = true;
    }

    /// §8 through §11: the first trap ends the whole program.
    fn record_trap(&self, why: String) {
        {
            let mut g = self.lock();
            if g.trap.is_none() {
                g.trap = Some(why);
            }
            g.stopping = true;
        }
        self.cv.notify_all();
    }
}

/// What a task runs. `main` is a call and therefore has a return value to
/// discard; a spawned block is a statement sequence over a store copied from
/// its spawner (§11.2).
enum Work<'a> {
    Main(&'a Fn),
    Spawned(&'a [Stmt], Vec<HashMap<String, (Val, bool)>>),
}

/// One task, start to finish, on its own thread.
///
/// The recursive evaluator's own call stack **is** the task's continuation —
/// which is why there is a thread here rather than a state machine. A `yield`
/// can occur inside a loop inside a called function, so resuming means
/// resuming a Rust call stack, and an OS thread is the cheapest way to have
/// several of those. Determinism is not weakened by it and does not depend on
/// the scheduler being fair: §11.3's run queue admits exactly one runnable
/// task, so the threads never race, and every switch is at one of §11.4's
/// three points.
fn run_task<'a, 'sc>(
    sched: &'sc Sched,
    fns: &'sc HashMap<String, &'a Fn>,
    lang: Lang,
    scope: &'sc std::thread::Scope<'sc, 'a>,
    id: TaskId,
    work: Work<'a>,
) where
    'a: 'sc,
{
    let mut it = Interp { fns, sched, scope, id, lang };
    if it.sched.await_turn(id).is_err() {
        return; // cancelled before it ever ran
    }
    let is_main = matches!(work, Work::Main(_));
    let outcome = match work {
        Work::Main(f) => it.call_body(f, Vec::new()).map(|_| ()),
        // §11: a spawned block is the task's top level, so `return` there ends
        // the TASK. There is no enclosing call for it to return from — the
        // function that wrote the `spawn` may have returned long ago.
        Work::Spawned(body, mut env) => it.stmts(body, &mut env).map(|_| ()),
    };
    match outcome {
        Ok(()) => sched.finish(id),
        Err(Abort::Cancelled) => {}
        Err(Abort::Trap(Trap(why))) => sched.record_trap(why),
        // §6.9's `?` has no enclosing call at a task's top level. Reported
        // rather than swallowed, for the reason §6.9 gives for `?` existing at
        // all: a non-`Ok` that vanishes is the failure mode being avoided.
        Err(Abort::Propagate(_)) => sched.record_trap(
            if is_main { "`?` propagated out of main" }
            else { "`?` propagated out of a spawned task" }
                .to_string(),
        ),
    }
}

pub struct Interp<'a, 'sc> {
    fns: &'sc HashMap<String, &'a Fn>,
    /// §11.3. The trace, the budget, the run queue, the blocked map and the
    /// channel store all live here, because all five are shared between tasks.
    /// A program that never spawns still runs through it, as the one task
    /// §11.3 says a program starts as — one code path and not two.
    sched: &'sc Sched,
    /// §11.5 (SPAWN) needs to create a task, and a task is a thread.
    scope: &'sc std::thread::Scope<'sc, 'a>,
    /// Which task this evaluator is. Read only by the scheduler operations.
    id: TaskId,
    /// R2: the language version being evaluated. Read in exactly one place —
    /// the `/` and `%` rule of §6.1.
    lang: Lang,
}

pub struct Observation {
    pub out: String,
    pub trap: Option<String>,
}

pub fn run(program: &[Fn]) -> Result<Observation, String> {
    run_with(program, Lang::default())
}

pub fn run_with(program: &[Fn], lang: Lang) -> Result<Observation, String> {
    let mut fns = HashMap::new();
    for f in program {
        if fns.insert(f.name.clone(), f).is_some() {
            return Err(format!("duplicate function `{}`", f.name));
        }
    }
    if !fns.contains_key("main") {
        return Err("no `main`".into());
    }
    let sched = Sched::new(200_000_000);
    let main = fns["main"];

    // §11.3: a program starts as `⟨ ⟨main's body, ∅⟩ , ∅ , ∅ , ε ⟩` — one
    // task. A program with no `spawn` in it never leaves that state, so the
    // sequential language runs down this path too and there is no second
    // implementation of it to disagree with.
    std::thread::scope(|scope| {
        run_task(&sched, &fns, lang, scope, 0, Work::Main(main));
    });

    let g = sched.lock();
    Ok(Observation { out: g.out.clone(), trap: g.trap.clone() })
}

// ---------------------------------------------------------------------------
// §6.6 lane decomposition
// ---------------------------------------------------------------------------

fn lanes(w: i64) -> [i8; LANES] {
    // Repeated balanced division. `w` is always in range here: §8 traps before
    // an out-of-range value can reach a lane operation.
    let mut out = [0i8; LANES];
    let mut n = w;
    for slot in out.iter_mut() {
        let mut r = n % 3;
        n /= 3;
        if r == 2 { r = -1; n += 1; }
        if r == -2 { r = 1; n -= 1; }
        *slot = r as i8;
    }
    out
}

fn from_lanes(l: &[i8; LANES]) -> i64 {
    let mut v = 0i64;
    let mut p = 1i64;
    for d in l.iter() {
        v += *d as i64 * p;
        p *= 3;
    }
    v
}

// ---------------------------------------------------------------------------
// §6.4, §6.5 per-trit connectives
// ---------------------------------------------------------------------------

fn t_and(a: i8, b: i8) -> i8 { a.min(b) }
fn t_or(a: i8, b: i8) -> i8 { a.max(b) }
/// §6.5. Written as the table the document prints, not as modular arithmetic.
fn t_xor(a: i8, b: i8) -> i8 {
    match (a, b) {
        (-1, -1) => 1, (-1, 0) => -1, (-1, 1) => 0,
        (0, -1) => -1, (0, 0) => 0, (0, 1) => 1,
        (1, -1) => 0, (1, 0) => 1, (1, 1) => -1,
        _ => unreachable!("not a trit: {} {}", a, b),
    }
}
/// §6.4. Łukasiewicz: the (0,0) cell is +1, which is what makes `a timp a` a
/// tautology. Kleene's `max(-a, b)` gives 0 there.
fn t_imp(a: i8, b: i8) -> i8 { (1 - a + b).min(1) }
fn t_con(a: i8, b: i8) -> i8 {
    if a == 1 && b == 1 { 1 } else if a == -1 && b == -1 { -1 } else { 0 }
}
fn t_any(a: i8, b: i8) -> i8 {
    if a == 1 || b == 1 { 1 } else if a == -1 || b == -1 { -1 } else { 0 }
}
fn t_cmp(a: i8, b: i8) -> i8 { if a > b { 1 } else if a < b { -1 } else { 0 } }

impl<'a, 'sc> Interp<'a, 'sc>
where
    'a: 'sc,
{
    fn tick(&mut self) -> R<()> {
        self.sched.tick()
    }

    /// §11.3 ω. One trace for the whole program.
    fn emit(&self, text: &str) {
        self.sched.emit(text);
    }

    // ---- §8 range check ---------------------------------------------------

    fn checked(&self, v: i128, what: &str) -> R<Val> {
        if v > T3_MAX as i128 || v < -(T3_MAX as i128) {
            return trap(format!(
                "{} overflow: result {} is outside the 27-trit range [{}, {}]",
                what, v, -T3_MAX, T3_MAX
            ));
        }
        Ok(Val::Int(v as i64))
    }

    // ---- §6.4 operand coercion -------------------------------------------

    /// §6.4. A `Bool` operand of a three-valued operator converts by
    /// `b -> 2b - 1`, so `false` becomes -1. An `Int` operand is NOT accepted:
    /// applying a three-valued connective to a ternary NUMBER is undefined
    /// (report.txt P1).
    fn trit_operand(&self, v: Val, op: &str) -> R<i8> {
        Ok(match v {
            Val::Trit(t) | Val::Bool3(t) => t,
            Val::Bool(b) => if b { 1 } else { -1 },
            // A `Result`'s tag IS a trit (§6.8), so `tif r.tag()` and
            // `tif r` dispatch the same way. That is the design claim made
            // operational rather than a convenience.
            Val::Res(ref r) => r.tag,
            Val::Int(_) => return trap(format!(
                "`{}` applied to an int: a three-valued operator takes trit or bool3 (semantics.md §6.4)", op)),
            Val::Text(_) => return trap(format!(
                "`{}` applied to a str", op)),
            Val::Chan(_) => return trap(format!(
                "`{}` applied to a channel", op)),
        })
    }

    /// §6.6. A lane-wise operand is a word.
    fn word_operand(&self, v: Val, op: &str) -> R<i64> {
        match v {
            Val::Int(n) => Ok(n),
            _ => trap(format!(
                "`{}` applied to a non-word: lane-wise operators take int (semantics.md §6.6)", op)),
        }
    }

    // ---- calls ------------------------------------------------------------

    fn call_body(&mut self, f: &'a Fn, args: Vec<Val>) -> R<Option<Val>> {
        let mut env: Vec<HashMap<String, (Val, bool)>> = vec![HashMap::new()];
        for ((name, ty), v) in f.params.iter().zip(args) {
            let v = coerce_decl(v, ty.clone());
            env[0].insert(name.clone(), (v, false));
        }
        // §6.9. A `?` inside this body unwinds to HERE and becomes the call's
        // return value. That is the whole of what `?` does: it is a return, not
        // an error, which is why `Unknown` survives it intact.
        match self.stmts(&f.body, &mut env) {
            Ok(Flow::Return(v)) => Ok(v),
            Ok(Flow::Normal) => Ok(None),
            Err(Abort::Propagate(v)) => Ok(Some(v)),
            Err(other) => Err(other),
        }
    }

    fn stmts(
        &mut self,
        body: &'a [Stmt],
        env: &mut Vec<HashMap<String, (Val, bool)>>,
    ) -> R<Flow> {
        env.push(HashMap::new());
        let r = self.stmts_inner(body, env);
        env.pop();
        r
    }

    fn stmts_inner(
        &mut self,
        body: &'a [Stmt],
        env: &mut Vec<HashMap<String, (Val, bool)>>,
    ) -> R<Flow> {
        for s in body {
            self.tick()?;
            match s {
                Stmt::Let { name, mutable, ty, init } => {
                    let v = self.expr(init, env)?;
                    let v = match ty { Some(t) => coerce_decl(v, t.clone()), None => v };
                    env.last_mut().unwrap().insert(name.clone(), (v, *mutable));
                }
                Stmt::Assign { name, val } => {
                    let v = self.expr(val, env)?;
                    let mut done = false;
                    for scope in env.iter_mut().rev() {
                        if let Some((slot, mutable)) = scope.get_mut(name) {
                            if !*mutable {
                                return trap(format!(
                                    "cannot assign to immutable binding `{}`", name));
                            }
                            // Assignment preserves the binding's value FORM, so
                            // `let mut t: trit = +; t = 5;` clamps as §6.7 says.
                            let form = slot.clone();
                            *slot = reshape(v, form);
                            done = true;
                            break;
                        }
                    }
                    if !done {
                        return trap(format!("assignment to unbound `{}`", name));
                    }
                }
                Stmt::If { arms, els } => {
                    let mut taken = false;
                    for (c, b) in arms {
                        let cv = self.expr(c, env)?;
                        if truthy(cv) {
                            if let Flow::Return(v) = self.stmts(b, env)? {
                                return Ok(Flow::Return(v));
                            }
                            taken = true;
                            break;
                        }
                    }
                    if !taken {
                        if let Some(b) = els {
                            if let Flow::Return(v) = self.stmts(b, env)? {
                                return Ok(Flow::Return(v));
                            }
                        }
                    }
                }
                Stmt::Tif { scrutinee, pos, zero, neg } => {
                    // §7. Three arms, dispatched on the sign of the carrier.
                    let v = self.expr(scrutinee, env)?;
                    let t = self.trit_operand(v, "tif")?;
                    let arm = if t > 0 { pos } else if t == 0 { zero } else { neg };
                    if let Flow::Return(v) = self.stmts(arm, env)? {
                        return Ok(Flow::Return(v));
                    }
                }
                // §11.5 (SPAWN). The new task is appended at the back and
                // this one CONTINUES — §11.4 records not yielding here as a
                // choice, and it is what lets a task's own code read
                // sequentially.
                //
                // §11.2: the task gets a COPY of this store, so the two share
                // nothing but channels — whose value is an index
                // (`Val::Chan`), so it survives the copy still naming the same
                // channel.
                Stmt::Spawn(body) => {
                    let id = self.sched.enqueue_new_task();
                    let copy = env.clone();
                    let (sched, fns, lang, scope) =
                        (self.sched, self.fns, self.lang, self.scope);
                    self.scope.spawn(move || {
                        run_task(sched, fns, lang, scope, id, Work::Spawned(body, copy));
                    });
                }

                // §11.5 (YIELD).
                Stmt::Yield => self.sched.yield_now(self.id)?,

                Stmt::While { cond, body } => loop {
                    self.tick()?;
                    let c = self.expr(cond, env)?;
                    if !truthy(c) { break; }
                    if let Flow::Return(v) = self.stmts(body, env)? {
                        return Ok(Flow::Return(v));
                    }
                },
                Stmt::Return(e) => {
                    let v = match e { Some(e) => Some(self.expr(e, env)?), None => None };
                    return Ok(Flow::Return(v));
                }
                Stmt::Expr(e) => { self.expr(e, env)?; }
            }
        }
        Ok(Flow::Normal)
    }

    // ---- expressions ------------------------------------------------------

    fn expr(
        &mut self,
        e: &'a Expr,
        env: &mut Vec<HashMap<String, (Val, bool)>>,
    ) -> R<Val> {
        self.tick()?;
        match e {
            Expr::Int(n) => self.checked(*n as i128, "literal"),
            Expr::TritLit(t) => Ok(Val::Trit(*t)),
            Expr::BoolLit(b) => Ok(Val::Bool(*b)),
            Expr::Bool3Lit(t) => Ok(Val::Bool3(*t)),
            Expr::Str(t) => Ok(Val::Text(t.clone())),
            Expr::Var(name) => {
                for scope in env.iter().rev() {
                    if let Some((v, _)) = scope.get(name) { return Ok(v.clone()); }
                }
                trap(format!("unbound identifier `{}`", name))
            }
            Expr::Cast(inner, ty) => {
                let v = self.expr(inner, env)?;
                Ok(cast(v, ty.clone()))
            }
            Expr::Un(op, inner) => {
                let v = self.expr(inner, env)?;
                match op {
                    Un::Neg => {
                        let n = v.as_print_int() as i128;
                        self.checked(-n, "negation")
                    }
                    // §6.4
                    Un::Tnot => {
                        let t = self.trit_operand(v.clone(), "tnot")?;
                        Ok(match v { Val::Bool3(_) => Val::Bool3(-t), _ => Val::Trit(-t) })
                    }
                    Un::Tposs => Ok(Val::Bool(self.trit_operand(v, "tposs")? >= 0)),
                    Un::Tnec => Ok(Val::Bool(self.trit_operand(v, "tnec")? == 1)),
                    // §6.6. tnotw a = -a, because negating a balanced-ternary
                    // number negates every trit.
                    Un::Tnotw => {
                        let w = self.word_operand(v, "tnotw")?;
                        self.checked(-(w as i128), "tnotw")
                    }
                }
            }
            Expr::Bin(op, l, r) => self.binop(*op, l, r, env),
            Expr::Call(name, args) => self.call(name, args, env),

            // §6.8. The three constructors. `Ok` carries a value; `Unknown`
            // and `Err` carry a message. `Unknown` is NOT a kind of `Err` and
            // the two are never merged here.
            Expr::Method(recv, name, args) => {
                let r = self.expr(recv, env)?;
                // §11.5 (SEND), (SEND-WAKE), (RECV), (RECV-BLOCK). Dispatched
                // on the RECEIVER's value form and not on the method NAME, so
                // a `Result` carrying a method spelled `send` is still a
                // `Result`: the core has no overloading and this keeps it so.
                if let Val::Chan(c) = r {
                    return self.chan_method(c, name, args, env);
                }
                self.result_method(r, name, args, env)
            }

            // §6.9. `?` — propagate the whole non-Ok Result out of the
            // enclosing function; evaluate to the payload on Ok.
            //
            // The propagated value is the ORIGINAL Result, message intact, so
            // `Unknown("why")` arrives at the caller still saying why and
            // still saying Unknown. Collapsing it to Err here would be the
            // exact mistake the type exists to prevent.
            Expr::Try(inner) => {
                let v = self.expr(inner, env)?;
                match v {
                    Val::Res(r) if r.tag == 1 => {
                        Ok(*r.val.clone().unwrap_or(Box::new(Val::Int(0))))
                    }
                    Val::Res(r) => Err(Abort::Propagate(Val::Res(r))),
                    other => trap(format!("`?` applied to a non-Result: {:?}", other)),
                }
            }

            // §6.10. `match` on a Result. Exhaustiveness is enforced by the
            // parser, so by here one arm always applies.
            Expr::Match(scrut, arms) => {
                let v = self.expr(scrut, env)?;
                let r = match v {
                    Val::Res(r) => r,
                    other => return trap(format!(
                        "the core only matches on a Result, got {:?}", other)),
                };
                let want = match r.tag { 1 => "Ok", 0 => "Unknown", _ => "Err" };
                let arm = arms.iter().find(|a| a.variant == want)
                    .or_else(|| arms.iter().find(|a| a.variant == "_"))
                    .ok_or_else(|| Abort::Trap(Trap(format!(
                        "no arm for `{}` — the parser should have refused this", want))))?;
                let bound = match r.tag {
                    1 => *r.val.clone().unwrap_or(Box::new(Val::Int(0))),
                    _ => Val::Text(r.msg.clone()),
                };
                env.push(HashMap::new());
                if let Some(b) = &arm.binding {
                    env.last_mut().unwrap().insert(b.clone(), (bound, false));
                }
                let flow = self.stmts_inner(&arm.body, env);
                env.pop();
                match flow? {
                    Flow::Return(v) => Err(Abort::Propagate(v.unwrap_or(Val::Int(0)))),
                    Flow::Normal => Ok(Val::Int(0)),
                }
            }
        }
    }

    fn binop(
        &mut self,
        op: Bin,
        l: &'a Expr,
        r: &'a Expr,
        env: &mut Vec<HashMap<String, (Val, bool)>>,
    ) -> R<Val> {
        // §6.3. The two exceptions to left-to-right-then-apply: the right
        // operand is not evaluated at all, so its output is not produced.
        if matches!(op, Bin::AndAnd | Bin::OrOr) {
            let lv = truthy(self.expr(l, env)?);
            return match op {
                Bin::AndAnd => if !lv { Ok(Val::Bool(false)) }
                               else { Ok(Val::Bool(truthy(self.expr(r, env)?))) },
                _ => if lv { Ok(Val::Bool(true)) }
                     else { Ok(Val::Bool(truthy(self.expr(r, env)?))) },
            };
        }

        // §5. Left to right, fully.
        let a = self.expr(l, env)?;
        let b = self.expr(r, env)?;

        match op {
            // §6.1
            Bin::Add => self.checked(a.as_print_int() as i128 + b.as_print_int() as i128, "add"),
            Bin::Sub => self.checked(a.as_print_int() as i128 - b.as_print_int() as i128, "sub"),
            Bin::Mul => self.checked(a.as_print_int() as i128 * b.as_print_int() as i128, "mul"),
            Bin::Div | Bin::Rem => {
                let (x, y) = (a.as_print_int(), b.as_print_int());
                if y == 0 {
                    return trap(format!(
                        "division by zero: {} {} 0", x, if op == Bin::Div { "/" } else { "%" }));
                }
                match self.lang {
                    // Truncating toward zero, remainder taking the dividend's
                    // sign. Rust's `/` and `%` are already defined that way,
                    // which is why this is not spelled out further.
                    Lang::V1 => Ok(Val::Int(if op == Bin::Div { x / y } else { x % y })),
                    // C4. `%` is DEFINED from `/` here, not given a rule of its
                    // own — that is what makes `(a / b) * b + (a % b) == a`
                    // hold, and stating the remainder separately would be
                    // stating the identity twice and inviting the two
                    // statements to disagree.
                    Lang::V2 => {
                        let q = div_nearest_ref(x, y);
                        if op == Bin::Div {
                            // The quotient can leave the word: `T3_MIN / -1` is
                            // in range, but nothing else is, and the check
                            // belongs here rather than being assumed.
                            self.checked(q as i128, "div")
                        } else {
                            self.checked(x as i128 - (q as i128) * (y as i128), "rem")
                        }
                    }
                }
            }
            // §6.2
            Bin::Eq | Bin::Ne | Bin::Lt | Bin::Gt | Bin::Le | Bin::Ge => {
                let (x, y) = (a.as_print_int(), b.as_print_int());
                Ok(Val::Bool(match op {
                    Bin::Eq => x == y, Bin::Ne => x != y,
                    Bin::Lt => x < y, Bin::Gt => x > y,
                    Bin::Le => x <= y, _ => x >= y,
                }))
            }
            // §6.4
            Bin::Tand | Bin::Tor | Bin::Txor | Bin::Tcon | Bin::Tany
            | Bin::Timp | Bin::Teq => {
                let name = tlogic_name(op);
                let (x, y) = (self.trit_operand(a.clone(), name)?, self.trit_operand(b.clone(), name)?);
                let z = match op {
                    Bin::Tand => t_and(x, y),
                    Bin::Tor => t_or(x, y),
                    Bin::Txor => t_xor(x, y),
                    Bin::Tcon => t_con(x, y),
                    Bin::Tany => t_any(x, y),
                    Bin::Timp => t_imp(x, y),
                    _ => t_and(t_imp(x, y), t_imp(y, x)),
                };
                // §6.4 result typing. tand/tor/tany/timp/teq are closed on
                // {-1,+1}, so two Bools give a Bool; txor and tcon are not.
                let closed = matches!(op,
                    Bin::Tand | Bin::Tor | Bin::Tany | Bin::Timp | Bin::Teq);
                Ok(match (a, b) {
                    (Val::Bool(_), Val::Bool(_)) if closed => Val::Bool(z > 0),
                    (Val::Bool(_), Val::Bool(_))
                    | (Val::Bool3(_), Val::Bool3(_))
                    | (Val::Bool(_), Val::Bool3(_))
                    | (Val::Bool3(_), Val::Bool(_)) => Val::Bool3(z),
                    _ => Val::Trit(z),
                })
            }
            // §6.6
            Bin::Tandw | Bin::Torw | Bin::Txorw | Bin::Timpw | Bin::Tcmpw => {
                let name = tlogic_name(op);
                let (x, y) = (self.word_operand(a, name)?, self.word_operand(b, name)?);
                let (lx, ly) = (lanes(x), lanes(y));
                let mut out = [0i8; LANES];
                for i in 0..LANES {
                    out[i] = match op {
                        Bin::Tandw => t_and(lx[i], ly[i]),
                        Bin::Torw => t_or(lx[i], ly[i]),
                        Bin::Txorw => t_xor(lx[i], ly[i]),
                        Bin::Timpw => t_imp(lx[i], ly[i]),
                        _ => t_cmp(lx[i], ly[i]),
                    };
                }
                Ok(Val::Int(from_lanes(&out)))
            }
            Bin::AndAnd | Bin::OrOr => unreachable!("handled above"),
        }
    }

    /// §11.5. The two channel operations.
    fn chan_method(
        &mut self,
        c: usize,
        name: &str,
        args: &'a [Expr],
        env: &mut Vec<HashMap<String, (Val, bool)>>,
    ) -> R<Val> {
        match name {
            "send" => {
                let v = match args {
                    [a] => self.expr(a, env)?,
                    _ => return trap("`send` takes exactly one argument"),
                };
                // §6.7: the channel carries `int`, so the value is cast on the
                // way IN. Casting on the way out would make what a receiver
                // sees depend on the binding it happens to be stored in.
                self.sched.send(self.id, c, cast(v, Ty::Int))?;
                Ok(Val::Int(0))
            }
            "recv" => {
                if !args.is_empty() {
                    return trap("`recv` takes no arguments");
                }
                self.sched.recv(self.id, c)
            }
            // §11.10 (CLOSE). Added in 0.6 — the operation had been on BOTH
            // BACKENDS since before §11 was written, and this account is the
            // one that had to catch up, which is the reverse of §11.9.
            "close" => {
                if !args.is_empty() {
                    return trap("`close` takes no arguments");
                }
                self.sched.close(c);
                Ok(Val::Int(0))
            }
            other => trap(format!("`{}` is not a channel operation", other)),
        }
    }

    fn call(
        &mut self,
        name: &str,
        args: &'a [Expr],
        env: &mut Vec<HashMap<String, (Val, bool)>>,
    ) -> R<Val> {
        // §6.8. The three `Result` constructors.
        match name {
            "Ok" | "Unknown" | "Err" => {
                let payload = match args.first() {
                    Some(a) => self.expr(a, env)?,
                    None => Val::Int(0),
                };
                return Ok(Val::Res(match name {
                    "Ok" => Res { tag: 1, val: Some(Box::new(payload)), msg: String::new() },
                    "Unknown" => Res {
                        tag: 0, val: None,
                        msg: match payload { Val::Text(t) => t, o => o.as_print_int().to_string() },
                    },
                    _ => Res {
                        tag: -1, val: None,
                        msg: match payload { Val::Text(t) => t, o => o.as_print_int().to_string() },
                    },
                }));
            }
            _ => {}
        }

        // §11.1. A channel constructor. `channel()` rather than a literal
        // because the core has no type arguments to write: §11's channel
        // carries `int`, which is all that specifying INTERLEAVING needs.
        if name == "channel" {
            if !args.is_empty() {
                return trap("`channel()` takes no arguments");
            }
            return Ok(Val::Chan(self.sched.new_channel(0)));
        }

        // §11.11. `channel<T>(n)` in the surface language, which the core
        // spells as a second constructor — see the note in `parser/exprs.rs`
        // for why two names rather than one with a sentinel capacity.
        if name == "channel_bounded" {
            let cap = match args {
                [a] => self.expr(a, env)?.as_print_int(),
                _ => return trap("`channel_bounded` takes exactly one argument"),
            };
            // A capacity below 1 traps rather than clamping: a zero-capacity
            // channel can never hold a value, so every send on it blocks
            // forever.
            if cap < 1 {
                return trap("a channel capacity must be at least 1");
            }
            return Ok(Val::Chan(self.sched.new_channel(cap as usize)));
        }

        // §1. The core's only library surface: the four printers.
        match name {
            "io::print" | "io::println" => {
                for a in args {
                    if let Expr::Str(s) = a {
                        self.emit(s);
                    } else {
                        match self.expr(a, env)? {
                            Val::Text(t) => self.emit(&t),
                            v => self.emit(&v.as_print_int().to_string()),
                        }
                    }
                }
                if name == "io::println" { self.emit("\n"); }
                return Ok(Val::Int(0));
            }
            "io::print_int" | "io::println_int" => {
                for a in args {
                    match self.expr(a, env)? {
                        Val::Text(t) => self.emit(&t),
                        v => self.emit(&v.as_print_int().to_string()),
                    }
                }
                if name == "io::println_int" { self.emit("\n"); }
                return Ok(Val::Int(0));
            }
            _ => {}
        }

        let f = *self.fns.get(name).ok_or_else(|| Abort::Trap(Trap(format!(
                "call to `{}`, which is not in the core and not defined here", name))))?;
        // §5. Arguments left to right, before the call.
        let mut vals = Vec::new();
        for a in args {
            vals.push(self.expr(a, env)?);
        }
        if vals.len() != f.params.len() {
            return trap(format!(
                "`{}` takes {} argument(s), given {}", name, f.params.len(), vals.len()));
        }
        let ret = self.call_body(f, vals)?;
        Ok(match (ret, f.ret.clone()) {
            (Some(v), t) => coerce_decl(v, t),
            (None, _) => Val::Int(0),
        })
    }
}

impl<'a, 'sc> Interp<'a, 'sc>
where
    'a: 'sc,
{
    /// §6.8. The six `Result` accessors the reference documents.
    ///
    /// `tag()` is the primitive one — it hands back the trit, which is what
    /// makes `tif r.tag()` a single three-way dispatch. `is_ok`/`is_unknown`/
    /// `is_err` are that same question asked one yes-or-no at a time.
    fn result_method(
        &mut self,
        recv: Val,
        name: &str,
        args: &'a [Expr],
        env: &mut Vec<HashMap<String, (Val, bool)>>,
    ) -> R<Val> {
        let r = match recv {
            Val::Res(r) => r,
            other => return trap(format!("`.{}()` on a non-Result: {:?}", name, other)),
        };
        match name {
            "tag" => Ok(Val::Trit(r.tag)),
            "is_ok" => Ok(Val::Bool(r.tag == 1)),
            "is_unknown" => Ok(Val::Bool(r.tag == 0)),
            "is_err" => Ok(Val::Bool(r.tag == -1)),
            // §8: `unwrap` names ONE of three outcomes, so the other two trap.
            // The two messages differ, because "it failed" and "we do not know"
            // are different facts and a shared message would hide which.
            "unwrap" => match r.tag {
                1 => Ok(*r.val.clone().unwrap_or(Box::new(Val::Int(0)))),
                0 => trap("unwrap on a Result that is Unknown"),
                _ => trap("unwrap on a Result that is Err"),
            },
            // The default is evaluated either way — the reference says so, and
            // it is observable when the argument prints.
            "unwrap_or" => {
                let d = match args.first() {
                    Some(a) => self.expr(a, env)?,
                    None => return trap("unwrap_or takes one argument"),
                };
                Ok(if r.tag == 1 {
                    *r.val.clone().unwrap_or(Box::new(Val::Int(0)))
                } else {
                    d
                })
            }
            other => trap(format!("`.{}()` is not a core Result method", other)),
        }
    }
}

fn tlogic_name(op: Bin) -> &'static str {
    match op {
        Bin::Tand => "tand", Bin::Tor => "tor", Bin::Txor => "txor",
        Bin::Tcon => "tcon", Bin::Tany => "tany", Bin::Timp => "timp",
        Bin::Teq => "teq", Bin::Tandw => "tandw", Bin::Torw => "torw",
        Bin::Txorw => "txorw", Bin::Timpw => "timpw", Bin::Tcmpw => "tcmpw",
        _ => "?",
    }
}

/// §7. `if` and `while` take a `Bool`. A `Bool` is 0/1, and a nonzero carrier
/// of any other form is true — which is what the compiler's `Int -> Bool` cast
/// does (§6.7).
fn truthy(v: Val) -> bool {
    match v {
        Val::Bool(b) => b,
        other => other.as_print_int() != 0,
    }
}

/// §6.7 casts.
fn cast(v: Val, to: Ty) -> Val {
    let carrier = v.as_print_int();
    match to {
        Ty::Int => Val::Int(carrier),
        // Int -> Trit CLAMPS. Not a truncation: `5 as trit` is +1.
        Ty::Trit => Val::Trit(carrier.clamp(-1, 1) as i8),
        Ty::Bool3 => Val::Bool3(carrier.clamp(-1, 1) as i8),
        Ty::Bool => Val::Bool(carrier != 0),
        Ty::Void => Val::Int(0),
        // A `Result` and a `str` are not scalars and the core defines no cast
        // to either: a declared type of that shape leaves the value alone.
        // §11.3: a channel is a name, not a number. There is no cast to or
        // from one, so a declared `chan` leaves the value alone — the same
        // rule `str` and `Result` get, and for the same reason.
        Ty::Str | Ty::Result(_) | Ty::Chan => v,
    }
}

/// A declared type on a `let`, a parameter or a return applies the §6.7 cast.
fn coerce_decl(v: Val, ty: Ty) -> Val {
    match ty { Ty::Void => v, t => cast(v, t) }
}

/// Assignment keeps the binding's existing value form (§7): a `trit` binding
/// stays a trit, so the assigned value is cast to it.
fn reshape(v: Val, like: Val) -> Val {
    match like {
        Val::Int(_) => cast(v, Ty::Int),
        Val::Trit(_) => cast(v, Ty::Trit),
        Val::Bool3(_) => cast(v, Ty::Bool3),
        Val::Bool(_) => cast(v, Ty::Bool),
        // Assigning into a Result- or str-shaped binding replaces it wholesale;
        // there is no narrowing to do.
        Val::Res(_) | Val::Text(_) | Val::Chan(_) => v,
    }
}
