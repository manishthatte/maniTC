//! §11 of `docs/semantics.md` — cooperative scheduling for the T3 emulator.
//!
//! © Manish Jagdish Thatte
//!
//! Step 2 of `enhance/phase3-the-semantics-debt/CONCURRENCY_DECISION.md` §5.
//! Step 1 (P84) wrote §11 and gave the A3 reference interpreter a scheduler;
//! this is the same specification on the machine the language actually runs on.
//!
//! # The one design problem, and the answer
//!
//! §11.2 says a spawned task gets a **copy of its spawner's store**, sharing
//! only channels. On T3 the store is the register file and the stack, and the
//! obvious implementation — give each task its own slice of the stack region —
//! **does not work**, for a reason worth writing down: a copied frame can
//! contain the ADDRESS of a stack slot, and moving the copy to a different base
//! leaves that address pointing into the spawner's frame. The bug would be
//! silent, data-dependent, and it would look like a register-allocation fault.
//!
//! So the stack does not move. **Every task sees its stack at the same
//! addresses, and the emulator swaps the contents on a context switch** — the
//! live window `memory[sp .. STACK_BASE]`, which is a handful of words for a
//! task that spawns near the top of a frame. That is what the `Task` struct in
//! `profiler.rs` has described since it was written, dead, in August: `pc`,
//! `regs`, and a `stack` of its own.
//!
//! It also means **P77 stays declined** — the memory map does not move, no
//! stack is carved into slices, and a program that never spawns is untouched
//! down to the word.
//!
//! # What is shared and what is not
//!
//! Copied on spawn, swapped on switch: the registers, the program counter, the
//! live stack, the call stack and the call depth.
//!
//! Shared, deliberately: the heap (so a channel handle means the same channel
//! in every task — §11.3's "a channel value is a name, not its contents"), the
//! output trace (§11.3: one trace per program), module globals at 61,000, and
//! the scratch window at 62,000. **The last two are a divergence from §11.2
//! and are recorded rather than hidden**: the core §11 specifies has no
//! globals, so the document does not say what a task should see of one, and a
//! per-task copy would make a global mean something different from what it
//! means in every non-spawning program. The scratch window is safe for a
//! different reason — it lives only across the instructions of a single
//! tuple or `Result` return, and none of §11.4's three yield points can fall
//! inside one.

use std::collections::{HashMap, VecDeque};

use super::{Emulator, STACK_BASE};

/// §11.3. A task's name. Task 0 is `main`.
pub(crate) type TaskId = usize;

/// §11.3. A suspended task. The RUNNING task has no entry here — its state is
/// live in the emulator's own registers, PC and memory.
#[derive(Clone)]
pub(crate) struct SavedTask {
    pub pc: usize,
    pub regs: [i64; 27],
    /// `memory[regs[26] .. STACK_BASE]` — the live stack window, saved on the
    /// way out and written back on the way in, at the SAME addresses.
    pub stack: Vec<i64>,
    pub call_stack: Vec<usize>,
    pub call_depth: usize,
}

/// §11.12 `𝒯`. What a task handle names.
///
/// `Taken` is the only one of the three the one-shot-channel model does not
/// already give, and §11.12 keeps it for one reason: without it the second
/// `await` on a handle answers with (RECV-CLOSED)'s zero, silently. With it,
/// the second `await` is a trap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TaskVal {
    Running,
    Done(i64),
    Taken,
}

/// §11.3's configuration, less the parts the emulator already owns.
pub(crate) struct Sched {
    /// §11.3 `R`. The head is the running task; there is never more than one.
    pub run: VecDeque<TaskId>,
    /// §11.3 `B`, one queue per channel handle so (SEND-WAKE) can take the
    /// longest-waiting waiter rather than an arbitrary one.
    pub blocked: HashMap<usize, VecDeque<TaskId>>,
    /// §11.11 `S`, the SEND-blocked map — tasks waiting for room on a bounded
    /// channel.
    ///
    /// **A second map and not an extension of `B`, which is the point.** A
    /// `recv` must wake a SENDER and a `send` must wake a RECEIVER; one queue
    /// holding both would let either wake the wrong kind, and the woken task
    /// would re-check, find nothing changed for it, and block again — visible
    /// only as an ordering difference, which is (SEND-WAKE)'s bug wearing a
    /// new hat (§11.7).
    pub blocked_send: HashMap<usize, VecDeque<TaskId>>,
    /// §11.11: the capacity of each BOUNDED channel. A handle absent here is
    /// unbounded, which is `channel<T>()` and every channel that existed
    /// before 0.7.
    pub chan_cap: HashMap<usize, usize>,
    /// §11.12 `𝒯`, keyed by task handle. A handle is a task id, and inline
    /// handles (below) draw from the same counter so the two can never
    /// collide.
    pub tstate: HashMap<TaskId, TaskVal>,
    /// §11.12 (AWAIT-BLOCK): tasks waiting on a handle.
    ///
    /// **A third map, and separate from `blocked` for §11.11's reason.** A
    /// channel's (SEND-WAKE) must not wake an awaiter and a task's (DONE-T)
    /// must not wake a receiver; one queue holding both lets either wake the
    /// wrong kind, and the woken task re-checks, finds nothing changed for it
    /// and blocks again — visible only as an ordering difference.
    pub blocked_await: HashMap<TaskId, VecDeque<TaskId>>,
    /// Every task except the running one.
    pub saved: HashMap<TaskId, SavedTask>,
    pub next_id: TaskId,
    /// Task 0's `R1` at the moment it exited. §11.6 lets `main` finish while
    /// other tasks run on, so the process exit status has to be captured when
    /// task 0 ends rather than when the program does.
    pub main_exit: Option<i64>,
    /// True once anything has spawned. A program that never spawns must be
    /// byte-identical to one compiled before any of this existed, so every
    /// scheduler operation is a no-op until this flips.
    pub active: bool,
}

impl Default for Sched {
    fn default() -> Self {
        Sched {
            run: VecDeque::from([0usize]),
            blocked: HashMap::new(),
            blocked_send: HashMap::new(),
            chan_cap: HashMap::new(),
            tstate: HashMap::new(),
            blocked_await: HashMap::new(),
            saved: HashMap::new(),
            next_id: 1,
            main_exit: None,
            active: false,
        }
    }
}

impl Sched {
    /// The running task — §11.3's head of `R`.
    pub fn current(&self) -> Option<TaskId> {
        self.run.front().copied()
    }

    /// **F-4**: how many tasks exist at all — running, runnable or blocked.
    ///
    /// A region may reclaim only when this is 1, because the bump pointer is
    /// shared: with a second task alive, an allocation of ITS may sit above
    /// the mark, and resetting would hand out memory that task still holds.
    /// Blocked tasks count — a task waiting on a channel still owns whatever
    /// it allocated before it blocked.
    pub fn live_task_count(&self) -> usize {
        // `saved` holds every task except the running one, blocked ones
        // included, so this is the whole population and not just the queue.
        self.saved.len() + self.run.len().min(1)
    }

    /// Is any task waiting on a channel? §11.6's second end condition.
    /// Is anybody waiting on a task handle? §11.6 counts an awaiter exactly
    /// as it counts a channel waiter: if `R` empties while one is queued, no
    /// runnable task can ever finish the awaited task.
    pub fn anyone_awaiting(&self) -> bool {
        self.blocked_await.values().any(|q| !q.is_empty())
    }

    pub fn anyone_blocked(&self) -> bool {
        // §11.11: `S` counts as much as `B`. A task waiting for room on a
        // channel nothing will drain is as deadlocked as one waiting for a
        // value on a channel nothing will fill, and §11.6 must see both.
        self.blocked.values().any(|q| !q.is_empty())
            || self.blocked_send.values().any(|q| !q.is_empty())
    }

}

// ---------------------------------------------------------------------------
// The operations, one per rule of §11.5
// ---------------------------------------------------------------------------

impl Emulator {
    /// Save the running task's state so another can run.
    ///
    /// The stack window is `[sp, STACK_BASE)`. Words BELOW `sp` are not saved
    /// and must not be: they are whatever the previously-running task left
    /// there, this task cannot read them without first pushing over them, and
    /// copying them would make one task's dead frames follow another around.
    fn save_current(&mut self) -> SavedTask {
        let sp = (self.regs[26] as usize).min(STACK_BASE);
        SavedTask {
            pc: self.pc,
            regs: self.regs,
            stack: self.memory[sp..STACK_BASE].to_vec(),
            call_stack: self.call_stack.clone(),
            call_depth: self.call_depth,
        }
    }

    /// Make `t` the running task: its stack window goes back at the SAME
    /// addresses it was saved from, which is the whole reason a task can hold
    /// the address of one of its own stack slots.
    fn restore(&mut self, t: SavedTask) {
        let sp = (t.regs[26] as usize).min(STACK_BASE);
        debug_assert_eq!(
            t.stack.len(),
            STACK_BASE - sp,
            "a saved stack window must match the stack pointer it was saved at"
        );
        self.memory[sp..STACK_BASE].copy_from_slice(&t.stack);
        self.regs = t.regs;
        self.pc = t.pc;
        self.call_stack = t.call_stack;
        self.call_depth = t.call_depth;
    }

    /// Give the processor to whichever task is now at the head of `R`.
    ///
    /// Called after any operation that changes the head. If the head is
    /// unchanged this is deliberately nothing at all — §11.5 (YIELD) with one
    /// runnable task is the identity, and doing a save/restore round trip
    /// anyway would be a slow no-op that could still go wrong.
    fn switch_if_needed(&mut self, was: TaskId) {
        let now = match self.sched.current() {
            Some(id) => id,
            // §11.6: nothing runnable. `end_of_program` has already decided
            // whether that is a normal end or a deadlock.
            None => return,
        };
        if now == was {
            return;
        }
        let saved = self.save_current();
        self.sched.saved.insert(was, saved);
        if let Some(t) = self.sched.saved.remove(&now) {
            self.restore(t);
        }
    }

    /// As `switch_if_needed`, for a task that is NOT coming back: its state is
    /// dropped rather than saved.
    fn switch_away_forever(&mut self) {
        if let Some(now) = self.sched.current() {
            if let Some(t) = self.sched.saved.remove(&now) {
                self.restore(t);
            }
        }
    }

    /// §11.6, the two end conditions, read off the configuration.
    ///
    /// Returns true when the program is over. A deadlock is DETECTED here
    /// rather than suffered: the scheduler holds the whole runnable set, so
    /// "no task can ever fill this channel" is a fact about the present
    /// configuration and not a guess about the future. That is the property
    /// P5.1 measured the absence of — on LLVM the same shape blocked in
    /// `pthread_cond_wait` with stdout unflushed and printed nothing at all.
    fn end_of_program(&mut self) -> bool {
        if self.sched.current().is_some() {
            return false;
        }
        if self.sched.anyone_awaiting() {
            // §11.12's own §11.6 clause. Reachable only when the awaited task
            // is itself blocked, and the message says so rather than naming a
            // channel that has nothing to do with it.
            self.trap(
                "TRAP: deadlock — every task is blocked awaiting a task that \
                 cannot finish",
            );
            return true;
        }
        if self.sched.anyone_blocked() {
            // §11.11: name which of the two it is. A task waiting for ROOM and
            // one waiting for a VALUE are both deadlocked when R empties, and
            // the message that says "fill" for a full channel sends the reader
            // looking for a missing sender when the problem is a missing
            // receiver.
            let waiting_to_send =
                self.sched.blocked_send.values().any(|q| !q.is_empty());
            if waiting_to_send {
                self.trap(
                    "TRAP: deadlock — every task is blocked on a channel that \
                     no runnable task can drain",
                );
            } else {
                self.trap(
                    "TRAP: deadlock — every task is blocked on a channel that \
                     no runnable task can fill",
                );
            }
        } else {
            // §11.6: every task finished. The status is main's, captured when
            // task 0 ended (§11.6 lets main finish while others run on).
            if let Some(code) = self.sched.main_exit {
                self.regs[1] = code;
            }
            self.halted = true;
        }
        true
    }

    /// §11.5 (SPAWN), as a **fork**: the child is a copy of this task that
    /// resumes at the same instruction, and the two are told apart by the
    /// value they get back — 0 in the child, the new task's id in the parent.
    ///
    /// Appends at the back and **does not yield**: §11.4 records that as a
    /// choice, and it is what lets the spawning task's own code read
    /// sequentially.
    ///
    /// # Why a fork rather than an entry address
    ///
    /// The first version of this took the ADDRESS of the spawned block, which
    /// works and costs a great deal upstream: a block nothing branches to has
    /// no CFG predecessor, so it is unreachable to every pass that walks the
    /// graph — `remove_unreachable_blocks` deletes it outright, and dominance,
    /// `mem2reg`, CSE and `--verify-ssa` all compute the wrong answer for it.
    /// Teaching them about an edge that is not a terminator's is a change to
    /// every consumer of the CFG, and P72 is the record of what that costs:
    /// the compiler names the matches that stop COMPILING and says nothing
    /// about the ones that merely stop being TRUE.
    ///
    /// A fork needs none of it. The parent branches on the returned id, so the
    /// body is an ordinary block on an ordinary edge that both parties really
    /// take — the child by getting 0 and the parent by not — and every pass
    /// works unchanged because nothing about the graph is new. **The whole
    /// upstream problem was created by the answer, not by the question.**
    ///
    /// §11.2 comes free with it: a fork IS a copy of the store, so a spawned
    /// block reaches the locals it was written beside because it shares the
    /// frame layout, being the same frame.
    pub(crate) fn sched_fork(&mut self) -> TaskId {
        self.sched.active = true;
        let id = self.sched.next_id;
        self.sched.next_id += 1;

        // `step` has already advanced the PC past the syscall, so the child
        // resumes at the instruction after it — the same one the parent is
        // about to run, which is the whole point of a fork.
        let mut child = self.save_current();
        child.regs[1] = 0;
        self.sched.saved.insert(id, child);
        self.sched.run.push_back(id);
        // §11.12 (SPAWN-T): 𝒯[h ↦ running]. Recorded for EVERY spawn, not
        // only for one whose handle is used — the handle is a return value and
        // nothing upstream knows whether it will be awaited.
        self.sched.tstate.insert(id, TaskVal::Running);
        id
    }

    /// §11.5 (YIELD). The head goes to the back.
    pub(crate) fn sched_yield(&mut self) {
        if !self.sched.active {
            return;
        }
        let Some(was) = self.sched.current() else { return };
        self.sched.run.pop_front();
        self.sched.run.push_back(was);
        self.switch_if_needed(was);
    }

    /// §11.5 (DONE) for a spawned task, and §11.6's "`main` returning does not
    /// end the program" for task 0.
    pub(crate) fn sched_task_exit(&mut self) {
        if !self.sched.active {
            self.halted = true;
            return;
        }
        let Some(me) = self.sched.current() else {
            self.halted = true;
            return;
        };
        if me == 0 {
            // §11.6. Captured now because the program may outlive main.
            self.sched.main_exit = Some(self.regs[1]);
        }
        self.sched.run.pop_front();
        if self.end_of_program() {
            return;
        }
        self.switch_away_forever();
    }

    /// §11.12 (DONE-T). A spawned task terminates WITH A VALUE: `𝒯[h ↦
    /// done(v)]`, and every waiter on `h` is woken.
    ///
    /// **All of them, not one, and it is (CLOSE)'s reason rather than
    /// (SEND-WAKE)'s.** A `send` produces one value, so a second waiter would
    /// find nothing and block again. Termination is a PERMANENT FACT: every
    /// awaiting task can now proceed, and one left queued is stranded forever
    /// because nothing will ever finish this task twice.
    pub(crate) fn sched_task_exit_value(&mut self, v: i64) {
        if let Some(me) = self.sched.current() {
            self.sched.tstate.insert(me, TaskVal::Done(v));
            if let Some(q) = self.sched.blocked_await.get_mut(&me) {
                while let Some(w) = q.pop_front() {
                    self.sched.run.push_back(w);
                }
            }
        }
        self.sched_task_exit();
    }

    /// A handle for a value that was computed WITHOUT a task.
    ///
    /// `--sched inline` is still the default and still evaluates `spawn { B }`
    /// in place (`docs/memory-model.md` §4), so its handle is born `done`.
    /// §11.12's decision 1 — "awaiting a finished task returns immediately" —
    /// is what makes that a legal task rather than a special case: a handle
    /// whose task finished long ago is the ordinary one.
    pub(crate) fn sched_done_value(&mut self, v: i64) -> TaskId {
        let h = self.sched.next_id;
        self.sched.next_id += 1;
        self.sched.tstate.insert(h, TaskVal::Done(v));
        h
    }

    /// §11.12 (AWAIT). Returns `Some(v)` when the value is available, and
    /// `None` when the task blocked or the program ended — in which case the
    /// caller must not write a result register.
    pub(crate) fn sched_await(&mut self, h: usize) -> Option<i64> {
        match self.sched.tstate.get(&h).copied() {
            // (AWAIT): 𝒯(h) = done(v). Does not touch `R` — a finished task
            // is awaited without yielding.
            Some(TaskVal::Done(v)) => {
                self.sched.tstate.insert(h, TaskVal::Taken);
                Some(v)
            }
            // Decision 2. The alternative — answering with the value again —
            // is available and rejected: a program awaiting one handle twice
            // has almost certainly confused two handles, and a detected error
            // beats a plausible continuation.
            Some(TaskVal::Taken) => {
                self.trap(
                    "TRAP: await on a task whose value has already been taken",
                );
                None
            }
            // (AWAIT-BLOCK). An unfinished task is an empty one-shot channel,
            // so this is §11.4's point 2 and the list of yield points does not
            // grow.
            Some(TaskVal::Running) => {
                let Some(me) = self.sched.current() else { return None };
                self.sched.run.pop_front();
                self.sched.blocked_await.entry(h).or_default().push_back(me);
                if self.end_of_program() {
                    return None;
                }
                // As (RECV-BLOCK): the task resumes with its `await` still in
                // front of it, so rewind past the SYSCALL `step` already
                // advanced over and re-execute.
                self.pc -= 1;
                let saved = self.save_current();
                self.sched.saved.insert(me, saved);
                if let Some(t) = self.sched.saved.remove(&self.sched.run[0]) {
                    self.restore(t);
                }
                None
            }
            // A handle naming no task at all. Under `--sched inline` every
            // handle is created `done`, and under a scheduler every fork
            // records `running`, so this is a handle the program invented.
            None => {
                self.trap("TRAP: await on a value that is not a task handle");
                None
            }
        }
    }

    /// §11.5 (RECV-BLOCK): the task leaves `R` for `B(c)` with its `recv`
    /// still in front of it.
    ///
    /// Returns false when the program ended here — the caller must not go on
    /// to write a result register.
    pub(crate) fn sched_block_on(&mut self, chan: usize) -> bool {
        let Some(me) = self.sched.current() else {
            return false;
        };
        self.sched.run.pop_front();
        self.sched.blocked.entry(chan).or_default().push_back(me);

        if self.end_of_program() {
            return false;
        }
        // (RECV-BLOCK) puts the task back "with its `recv` still in front of
        // it", so resuming must RE-EXECUTE the receive rather than continue
        // past it. `step` advances the PC before it executes an opcode, so at
        // this point it already points past the SYSCALL: rewind one word.
        //
        // Re-executing is not an optimisation to be tidied away later — it is
        // what makes an intervening receive by a third task (which may take
        // the very value this task was woken for) behave the way §11.5 says
        // instead of the way the implementation found convenient.
        self.pc -= 1;
        let saved = self.save_current();
        self.sched.saved.insert(me, saved);
        if let Some(t) = self.sched.saved.remove(&self.sched.run[0]) {
            self.restore(t);
        }
        true
    }

    /// §11.5 (SEND-WAKE). Exactly one waiter, and it is the longest-waiting.
    ///
    /// Waking all of them would pass almost every test — a spuriously woken
    /// receiver re-executes its `recv`, finds nothing and blocks again while
    /// PRINTING NOTHING, so the extra wake leaves no trace. It is visible only
    /// through the order of `B`. That cost a whole round of the reference
    /// implementation's tests to notice (report.txt P84), and it is why this
    /// takes the front and not the lot.
    pub(crate) fn sched_wake_one(&mut self, chan: usize) {
        if !self.sched.active {
            return;
        }
        if let Some(q) = self.sched.blocked.get_mut(&chan) {
            if let Some(w) = q.pop_front() {
                self.sched.run.push_back(w);
            }
        }
    }

    /// §11.10 (CLOSE). Wake EVERY task waiting on this channel, in `B(c)`'s
    /// own order — longest-waiting first, appended to the back of `R`.
    ///
    /// **This is the one place in §11 where all waiters are woken rather than
    /// one, and it is not an inconsistency with (SEND-WAKE).** A `send`
    /// produces exactly one value, so exactly one waiter can proceed and a
    /// second would find nothing and block again — §11.7's invisible bug. A
    /// `close` produces no value but makes a PERMANENT FACT true of the
    /// channel, and every waiter's `recv` can now complete with the zero of
    /// (RECV-CLOSED). Leaving any of them on `B(c)` strands it forever,
    /// because after a close no `send` will ever wake it.
    ///
    /// That is exactly what happened before this existed: a task blocked in
    /// `recv` when another closed the channel was never woken, and the program
    /// hit §11.6's deadlock trap — reporting that no runnable task could fill
    /// the channel, which was true and useless, since the close had already
    /// established that none ever would. Identically on both backends, so the
    /// parity matrix reported nothing.
    pub(crate) fn sched_wake_all(&mut self, chan: usize) {
        if !self.sched.active {
            return;
        }
        if let Some(q) = self.sched.blocked.get_mut(&chan) {
            while let Some(w) = q.pop_front() {
                self.sched.run.push_back(w);
            }
        }
        // §11.11: (CLOSE) wakes the SENDERS too. Each re-executes its send,
        // finds the channel closed and traps by §11.10 (SEND-CLOSED) — which
        // is the right outcome and not an accident: its value has nowhere to
        // go, and the alternative is a task parked forever on a channel
        // nothing will ever drain.
        if let Some(q) = self.sched.blocked_send.get_mut(&chan) {
            while let Some(w) = q.pop_front() {
                self.sched.run.push_back(w);
            }
        }
    }

    /// §11.11 (SEND-BLOCK): the task leaves `R` for `S(c)` with its `send`
    /// still in front of it. Mirrors `sched_block_on`, including the PC
    /// rewind, so a woken sender RE-EXECUTES the send rather than being
    /// credited with it — a third task may have taken the space meanwhile.
    pub(crate) fn sched_block_on_send(&mut self, chan: usize) -> bool {
        let Some(me) = self.sched.current() else {
            return false;
        };
        self.sched.run.pop_front();
        self.sched.blocked_send.entry(chan).or_default().push_back(me);
        if self.end_of_program() {
            return false;
        }
        self.pc -= 1;
        let saved = self.save_current();
        self.sched.saved.insert(me, saved);
        if let Some(t) = self.sched.saved.remove(&self.sched.run[0]) {
            self.restore(t);
        }
        true
    }

    /// §11.11 (RECV-WAKE): a receive frees exactly one slot, so exactly one
    /// sender is woken — the longest-waiting.
    pub(crate) fn sched_wake_one_sender(&mut self, chan: usize) {
        if !self.sched.active {
            return;
        }
        if let Some(q) = self.sched.blocked_send.get_mut(&chan) {
            if let Some(w) = q.pop_front() {
                self.sched.run.push_back(w);
            }
        }
    }
}
