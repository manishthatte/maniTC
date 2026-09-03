/* sched.c — the cooperative task scheduler for the LLVM backend.
 *
 * © Manish Jagdish Thatte
 *
 * Implements docs/semantics.md §11, which is normative. The T3 emulator's
 * `codegen_t3/emulator/sched.rs` implements the same section for that backend
 * and the two must agree observably; where they differ, §11 decides.
 *
 * WHY THREADS WITH A BATON, AND NOT ucontext.
 *
 * A task's continuation on this backend IS a C call stack, so switching tasks
 * means switching stacks. `ucontext` can do that, but §11.2 says a spawned task
 * gets a COPY of the spawning task's store — and copying a live C stack is the
 * thing P88 refused to do on T3 for a reason that applies here with more force:
 * a copied frame can hold the address of one of its own slots, and moving it
 * leaves that pointer aimed at the original. T3 could dodge this by keeping
 * every task's stack at the SAME addresses and swapping the live window,
 * because the emulator owns its memory map. A hosted process does not own its
 * stack.
 *
 * So a task gets a real stack of its own from the start, and the "copy of the
 * store" is passed to it explicitly — the compiler outlines the spawn body and
 * hands its captured values in `env`. That is closer to §11.2 than T3's fork
 * is: the fork copies the whole frame and relies on the child not writing what
 * the parent reads, while this copies exactly the values §11.2 says it should.
 *
 * DETERMINISM DOES NOT DEPEND ON THE THREADS BEING FAIR. Exactly one task holds
 * the baton, handoffs happen only at §11.4's three yield points, and the run
 * queue's insertion points are the ones §11.5 states. The OS scheduler never
 * gets to choose: a task that does not hold the baton is asleep on its own
 * condition variable. That is the same argument the A3 reference interpreter
 * makes for its own threads.
 */

#define MANIT_SCHED_MAX_TASKS 4096

typedef struct ManitTask2 {
    pthread_t       thread;
    pthread_cond_t  wake;
    int             started;      /* the pthread exists */
    int             finished;     /* body returned; §11.5 (DONE) */
    int             is_main;      /* task 0, which the process started on */
    int64_t         id;
    int64_t       (*body)(int64_t*);
    int64_t*        env;
    /* §11.12: the `ManitHandle` this task completes when it terminates.
     * `void*` because the handle type is declared below — the task does not
     * need to see inside it, and keeping it opaque is the same discipline the
     * `void**` waiter protocol uses. */
    void*           handle;
    struct ManitTask2* next;      /* run queue, or a channel's waiter list */
} ManitTask2;

static pthread_mutex_t manit_sched_lock = PTHREAD_MUTEX_INITIALIZER;
static ManitTask2* manit_run_head = NULL;   /* R, §11.3 */
static ManitTask2* manit_run_tail = NULL;
static ManitTask2* manit_current  = NULL;   /* the head of R is the running task */
static int64_t     manit_next_id  = 0;
static int         manit_sched_on = 0;
static int         manit_blocked_count = 0; /* |B|, for §11.6 */
/* F-4: how many tasks exist, main included. A region may reclaim only when
 * this is 1 — the allocator is shared, so with a second task alive an
 * allocation of ITS may sit above the mark. Declining to reclaim is a leak,
 * which is what happened before regions existed; reclaiming would be
 * corruption, and those are not the same size of mistake. */
static int         manit_live_tasks = 1;
int manit_live_task_count(void) { return manit_live_tasks; }
/* Broadcast when R empties. `__task_main_done` waits on it: the other tasks are
 * real threads, so returning from `main` would take the process down under
 * them. A separate variable from the per-task `wake` because a finished task is
 * no longer anyone's hand-off target. */
static pthread_cond_t manit_quiesce = PTHREAD_COND_INITIALIZER;

/* ---- run queue -------------------------------------------------------- */

static void manit_rq_push(ManitTask2* t) {
    t->next = NULL;
    if (manit_run_tail) manit_run_tail->next = t; else manit_run_head = t;
    manit_run_tail = t;
}

static ManitTask2* manit_rq_pop(void) {
    ManitTask2* t = manit_run_head;
    if (!t) return NULL;
    manit_run_head = t->next;
    if (!manit_run_head) manit_run_tail = NULL;
    t->next = NULL;
    return t;
}

/* §11.6. Called with the lock held and `manit_current` already removed from R.
 *
 * Hands the baton to the new head of R and blocks until it comes back. If R is
 * empty the program is over, and WHICH ending it is depends on B: empty is a
 * normal end, non-empty is the deadlock the decision document's §3 exists to
 * detect. A pthread runtime can only suffer that; a scheduler that owns the
 * whole runnable set can name it.
 */
static int manit_anyone_awaiting(void);

static void manit_hand_off(ManitTask2* self, int self_requeued) {
    if (!manit_run_head) {
        if (manit_blocked_count > 0) {
            /* §11.12's own §11.6 clause, checked FIRST and in the same order
             * the T3 emulator checks it. A task blocked in `await` is in B, so
             * §11.6 already covers it; what it must not do is report a CHANNEL
             * nobody is waiting on. The message names which kind of wait it
             * is, for §11.11's reason — the wrong word sends the reader
             * looking for a missing sender when the problem is a task that
             * cannot finish.
             *
             * Derived by WALKING the handle list rather than kept as a second
             * counter: `manit_sched_block_on` and `manit_sched_wake_all` are
             * shared with channels and do not know which kind of queue they
             * were handed, so a counter would have to be maintained at call
             * sites that cannot see the distinction — and a counter that can
             * drift from the thing it counts is what P41 was. */
            int awaiting = manit_anyone_awaiting();
            pthread_mutex_unlock(&manit_sched_lock);
            if (awaiting) {
                manit_fault("deadlock — every task is blocked awaiting a task "
                            "that cannot finish");
            } else {
                manit_fault("deadlock — every task is blocked on a channel that no "
                            "runnable task can fill");
            }
            return; /* not reached */
        }
        /* R is empty and nothing is blocked: the program is over (§11.6). */
        manit_current = self_requeued ? self : NULL;
        pthread_cond_broadcast(&manit_quiesce);
        return;
    }
    ManitTask2* next = manit_run_head;
    manit_current = next;
    pthread_cond_signal(&next->wake);
    if (!self_requeued) return;              /* the caller is finishing */
    while (manit_current != self) {
        pthread_cond_wait(&self->wake, &manit_sched_lock);
    }
}

/* ---- §11.12: `Task<T>` and `await` ------------------------------------
 *
 * A task handle is a ONE-SHOT CHANNEL OF CAPACITY ONE that the task sends to
 * when it terminates, and `await` is its `recv` — which is why §11.4's list of
 * yield points does not grow: an unfinished task is an empty queue, so
 * awaiting one is point 2.
 *
 * `taken` is the only state the channel model does not already give. Without
 * it a second `await` answers with (RECV-CLOSED)'s zero, silently; with it,
 * the second `await` is a trap. §11.12 decision 2 states the rule on the
 * VALUE rather than as a restriction on the handle, deliberately, so that when
 * affine types land `await` consuming its handle makes the second one a
 * COMPILE error and this trap becomes unreachable.
 */
/* Forward-declared: both live below, next to the channel protocol they were
 * written for, and §11.12 reuses that protocol rather than growing a second. */
void manit_sched_block_on(void** head, void** tail);
void manit_sched_wake_all(void** head, void** tail);

typedef enum { MT_RUNNING, MT_DONE, MT_TAKEN } ManitTaskState;

typedef struct ManitHandle {
    int64_t id;
    ManitTaskState state;
    int64_t value;
    /* B(h). The same `void**` protocol a channel uses, so the
     * "longest-waiting" ordering stays enforced in one place. */
    void* awaiters_head;
    void* awaiters_tail;
    struct ManitHandle* next;
} ManitHandle;

static ManitHandle* manit_handles = NULL;
static int64_t manit_next_handle = 1;

/* Allocated under the scheduler lock: `__task_spawn` runs with tasks live. */
static ManitHandle* manit_handle_new_locked(ManitTaskState st, int64_t v) {
    ManitHandle* h = (ManitHandle*)calloc(1, sizeof(ManitHandle));
    h->id = manit_next_handle++;
    h->state = st;
    h->value = v;
    h->next = manit_handles;
    manit_handles = h;
    return h;
}

static ManitHandle* manit_handle_find(int64_t id) {
    for (ManitHandle* h = manit_handles; h; h = h->next) {
        if (h->id == id) return h;
    }
    return NULL;
}

/* §11.12 decision 1 and 3, and `--sched inline`'s whole implementation: a
 * value that never had a task gets a handle already in `done(v)`. A handle
 * whose task finished long ago is the ordinary case, not a special one. */
int64_t __task_done_value(int64_t v) {
    pthread_mutex_lock(&manit_sched_lock);
    ManitHandle* h = manit_handle_new_locked(MT_DONE, v);
    int64_t id = h->id;
    pthread_mutex_unlock(&manit_sched_lock);
    return id;
}

/* §11.12 (DONE-T): 𝒯[h ↦ done(v)], and EVERY waiter is woken.
 *
 * All of them, and it is (CLOSE)'s reason rather than (SEND-WAKE)'s: a `send`
 * produces one value so a second waiter would find nothing and block again,
 * while termination is a PERMANENT FACT — every awaiting task can proceed, and
 * one left queued is stranded forever because nothing finishes a task twice. */
static void manit_handle_complete(ManitHandle* h, int64_t v) {
    if (!h) return;
    pthread_mutex_lock(&manit_sched_lock);
    h->state = MT_DONE;
    h->value = v;
    pthread_mutex_unlock(&manit_sched_lock);
    manit_sched_wake_all(&h->awaiters_head, &h->awaiters_tail);
}

/* Is any task queued on a task handle? §11.6 counts an awaiter exactly as it
 * counts a channel waiter. */
static int manit_anyone_awaiting(void) {
    for (ManitHandle* h = manit_handles; h; h = h->next) {
        if (h->awaiters_head) return 1;
    }
    return 0;
}

/* §11.12 (AWAIT) and (AWAIT-BLOCK). */
int64_t __task_await(int64_t id) {
    for (;;) {
        pthread_mutex_lock(&manit_sched_lock);
        ManitHandle* h = manit_handle_find(id);
        if (!h) {
            pthread_mutex_unlock(&manit_sched_lock);
            manit_fault("await on a value that is not a task handle");
            return 0;
        }
        if (h->state == MT_TAKEN) {
            pthread_mutex_unlock(&manit_sched_lock);
            manit_fault("await on a task whose value has already been taken");
            return 0;
        }
        if (h->state == MT_DONE) {
            /* (AWAIT) does not touch R: a finished task is awaited without
             * yielding. */
            h->state = MT_TAKEN;
            int64_t v = h->value;
            pthread_mutex_unlock(&manit_sched_lock);
            return v;
        }
        pthread_mutex_unlock(&manit_sched_lock);
        if (!manit_sched_on) {
            /* Unscheduled and still running is unreachable — every handle is
             * born `done` under `--sched inline` — so saying so beats hanging
             * in a wait nothing can end (P5.1's signature). */
            manit_fault("await on a task that cannot finish");
            return 0;
        }
        /* (AWAIT-BLOCK). The loop is the specification: being woken means
         * re-executing the await, exactly as (RECV-BLOCK) does, so an
         * intervening `await` by a third task behaves as the rules say. */
        manit_sched_block_on(&h->awaiters_head, &h->awaiters_tail);
    }
}

/* ---- the entry point every task thread runs --------------------------- */

static void* manit_task_trampoline(void* arg) {
    ManitTask2* self = (ManitTask2*)arg;
    pthread_mutex_lock(&manit_sched_lock);
    while (manit_current != self) {
        pthread_cond_wait(&self->wake, &manit_sched_lock);
    }
    pthread_mutex_unlock(&manit_sched_lock);

    int64_t result = self->body ? self->body(self->env) : 0;
    /* THE RUNTIME OWNS `env`, ALWAYS. Both paths free it and no body may:
     * the two disagreed at first — the unscheduled path freed and the
     * trampoline leaked — so a body written to free its own captures
     * double-freed under one mode and was correct under the other. One owner,
     * stated here and in `__task_spawn`, because generated code should never
     * have to emit a free. */
    free(self->env);
    self->env = NULL;

    /* §11.12 (DONE-T) BEFORE §11.5 (DONE), and the order matters: completing
     * the handle moves every awaiter back onto R, so it has to happen while
     * this task is still the head and can hand off to them. Doing it after the
     * hand-off would leave them queued behind a task that has gone. */
    manit_handle_complete((ManitHandle*)self->handle, result);

    /* §11.5 (DONE): the task is removed. It is already the head of R. */
    pthread_mutex_lock(&manit_sched_lock);
    self->finished = 1;
    manit_live_tasks--;                       /* F-4 */
    ManitTask2* me = manit_rq_pop();
    (void)me;
    manit_hand_off(self, 0);
    pthread_mutex_unlock(&manit_sched_lock);
    return NULL;
}

/* ---- the ABI the compiler emits --------------------------------------- */

/* Register the process's own stack as task 0. Emitted at the top of `main`
 * when `--sched cooperative` is in force. */
void __task_bootstrap(void) {
    pthread_mutex_lock(&manit_sched_lock);
    if (manit_sched_on) { pthread_mutex_unlock(&manit_sched_lock); return; }
    ManitTask2* t = (ManitTask2*)calloc(1, sizeof(ManitTask2));
    pthread_cond_init(&t->wake, NULL);
    t->id = manit_next_id++;
    t->is_main = 1;
    t->started = 1;
    manit_rq_push(t);
    manit_current = t;
    manit_sched_on = 1;
    pthread_mutex_unlock(&manit_sched_lock);
}

/* §11.5 (SPAWN): the new task is appended at the BACK of R and the spawner
 * CONTINUES. `env` is the captured store §11.2 gives the child, packed by the
 * compiler; the task owns it and frees it when the body returns. */
int64_t __task_spawn(int64_t (*body)(int64_t*), int64_t* env) {
    if (!manit_sched_on) {
        /* Not scheduled: run it in place, as §4 always did — and §11.12
         * decision 1 makes the result an ordinary finished task rather than a
         * special case, so it gets a handle already in `done(v)`. The return
         * value used to be the BODY's result, which is not a handle and could
         * not be awaited. */
        int64_t r = body ? body(env) : 0;
        free(env);
        return __task_done_value(r);
    }
    pthread_mutex_lock(&manit_sched_lock);
    manit_live_tasks++;                       /* F-4 */
    ManitTask2* t = (ManitTask2*)calloc(1, sizeof(ManitTask2));
    pthread_cond_init(&t->wake, NULL);
    t->id = manit_next_id++;
    t->body = body;
    t->env = env;
    /* §11.12 (SPAWN-T): 𝒯[h ↦ running], recorded for EVERY spawn. Nothing
     * upstream knows whether the handle will be awaited, and a handle created
     * only on demand could not exist by the time the task finished. */
    ManitHandle* h = manit_handle_new_locked(MT_RUNNING, 0);
    t->handle = h;
    manit_rq_push(t);
    int64_t id = h->id;
    pthread_mutex_unlock(&manit_sched_lock);

    if (pthread_create(&t->thread, NULL, manit_task_trampoline, t) != 0) {
        manit_fault("could not create a task thread");
    }
    t->started = 1;
    return id;
}

/* §11.5 (YIELD): the running task goes to the BACK of R. */
void __task_yield(void) {
    if (!manit_sched_on) return;
    pthread_mutex_lock(&manit_sched_lock);
    ManitTask2* self = manit_current;
    if (self && manit_run_head == self && manit_run_head->next != NULL) {
        manit_rq_pop();
        manit_rq_push(self);
        manit_hand_off(self, 1);
    }
    pthread_mutex_unlock(&manit_sched_lock);
}

/* §11.6: `main` returning does not end the program. The remaining tasks run;
 * the process exits when R is empty, or traps if B is not. */
void __task_main_done(void) {
    if (!manit_sched_on) return;
    pthread_mutex_lock(&manit_sched_lock);
    ManitTask2* self = manit_current;
    if (self) {
        self->finished = 1;
        manit_rq_pop();
        manit_hand_off(self, 0);
        /* Wait for the whole system to quiesce before the process exits: the
         * other tasks are real threads and returning from `main` would take
         * the process down under them. */
        while (manit_run_head != NULL) {
            pthread_cond_wait(&manit_quiesce, &manit_sched_lock);
        }
    }
    pthread_mutex_unlock(&manit_sched_lock);
}

/* ---- what a channel needs from the scheduler --------------------------
 *
 * The waiter list B(c) lives on the channel, in `sync.c`, because that is where
 * a channel is; the scheduler only needs to move tasks on and off it. The two
 * halves talk through `void**` rather than sharing `ManitTask2`, so a channel
 * cannot reach inside a task and the "longest-waiting" ordering §11.3 requires
 * is enforced in exactly one place — here.
 */

int manit_sched_active(void) { return manit_sched_on; }

/* §11.5 (RECV-BLOCK): the running task leaves R and is appended to B(c).
 * Returns once someone has woken it. */
void manit_sched_block_on(void** head, void** tail) {
    pthread_mutex_lock(&manit_sched_lock);
    ManitTask2* self = manit_current;
    if (!self) { pthread_mutex_unlock(&manit_sched_lock); return; }
    manit_rq_pop();                       /* out of R */
    self->next = NULL;
    if (*tail) ((ManitTask2*)*tail)->next = self; else *head = self;
    *tail = self;
    manit_blocked_count++;
    manit_hand_off(self, 0);
    /* Woken by (SEND-WAKE), which put us back on R. */
    while (manit_current != self) {
        pthread_cond_wait(&self->wake, &manit_sched_lock);
    }
    pthread_mutex_unlock(&manit_sched_lock);
}

/* §11.5 (SEND-WAKE): wake AT MOST ONE waiter, the LONGEST-WAITING, and append
 * it to the BACK of R.
 *
 * §11.7 singles this clause out: waking all of them survives every obvious
 * test, because a spuriously woken receiver re-executes its `recv`, finds
 * nothing and blocks again WHILE PRINTING NOTHING. It is observable only where
 * it changes the ORDER of B. `tests/interleaving_tests.rs` carries the program
 * that can see it; the obvious test — counting how many waiters woke — cannot.
 */
/* §11.10 (CLOSE): wake EVERY waiter, in B(c)'s own order.
 *
 * The one place in §11 where all are woken rather than one, and not an
 * inconsistency with (SEND-WAKE): a `send` produces one value so only one
 * waiter can proceed, while a `close` produces no value but makes a PERMANENT
 * FACT true, and every waiter's `recv` can now complete with (RECV-CLOSED)'s
 * zero. A waiter left on B(c) after a close is stranded forever, because no
 * `send` will ever wake it — which is exactly what used to happen, on both
 * backends, ending in §11.6's deadlock trap. */
void manit_sched_wake_all(void** head, void** tail) {
    pthread_mutex_lock(&manit_sched_lock);
    ManitTask2* t = (ManitTask2*)*head;
    while (t) {
        ManitTask2* next = t->next;
        t->next = NULL;
        manit_blocked_count--;
        manit_rq_push(t);
        t = next;
    }
    *head = NULL;
    *tail = NULL;
    pthread_mutex_unlock(&manit_sched_lock);
}

void manit_sched_wake_one(void** head, void** tail) {
    pthread_mutex_lock(&manit_sched_lock);
    ManitTask2* t = (ManitTask2*)*head;
    if (t) {
        *head = t->next;
        if (!*head) *tail = NULL;
        t->next = NULL;
        manit_blocked_count--;
        manit_rq_push(t);
    }
    pthread_mutex_unlock(&manit_sched_lock);
}
