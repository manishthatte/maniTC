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

/// §11.3's configuration, less the parts the emulator already owns.
pub(crate) struct Sched {
    /// §11.3 `R`. The head is the running task; there is never more than one.
    pub run: VecDeque<TaskId>,
    /// §11.3 `B`, one queue per channel handle so (SEND-WAKE) can take the
    /// longest-waiting waiter rather than an arbitrary one.
    pub blocked: HashMap<usize, VecDeque<TaskId>>,
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

    /// Is any task waiting on a channel? §11.6's second end condition.
    pub fn anyone_blocked(&self) -> bool {
        self.blocked.values().any(|q| !q.is_empty())
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
        if self.sched.anyone_blocked() {
            self.trap(
                "TRAP: deadlock — every task is blocked on a channel that no \
                 runnable task can fill",
            );
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
}
