/* sync.c — Mutex, AtomicTrit, channel, ternary utils, spawn/join, Semaphore, Barrier, AtomicInt.
 * Included by manit_runtime.c — do not compile independently.
 * Author: Manish Jagdish Thatte
 */

/* ======================== sync / Mutex ======================== */

typedef struct {
    pthread_mutex_t lock;
    int64_t value;
} ManitMutex;

ManitMutex* Mutex_new(int64_t v) {
    ManitMutex* m = malloc(sizeof(*m));
    if (!m) return NULL;
    /* Recursive: the guard pattern (`let g = m.lock(); g.get(); ...`)
     * calls Mutex_get/Mutex_set while the lock is already held by the
     * same thread; a default mutex would self-deadlock there. */
    pthread_mutexattr_t attr;
    pthread_mutexattr_init(&attr);
    pthread_mutexattr_settype(&attr, PTHREAD_MUTEX_RECURSIVE);
    pthread_mutex_init(&m->lock, &attr);
    pthread_mutexattr_destroy(&attr);
    m->value = v;
    return m;
}

/* Returns the mutex itself: `let guard = m.lock()` binds the handle as the
 * guard, matching the T3 emulator (syscall 110 returns R1 unchanged). */
ManitMutex* Mutex_lock(ManitMutex* m) {
    if (m) pthread_mutex_lock(&m->lock);
    return m;
}

void Mutex_unlock(ManitMutex* m) {
    if (m) pthread_mutex_unlock(&m->lock);
}

int64_t Mutex_get(ManitMutex* m) {
    if (!m) return 0;
    pthread_mutex_lock(&m->lock);
    int64_t v = m->value;
    pthread_mutex_unlock(&m->lock);
    return v;
}

void Mutex_set(ManitMutex* m, int64_t v) {
    if (!m) return;
    pthread_mutex_lock(&m->lock);
    m->value = v;
    pthread_mutex_unlock(&m->lock);
}

/* ======================== AtomicTrit ======================== */

typedef struct { volatile int8_t val; pthread_mutex_t lock; } ManitAtomicTrit;

ManitAtomicTrit* AtomicTrit_new(int8_t v) {
    ManitAtomicTrit* a = malloc(sizeof(*a));
    if (!a) return NULL;
    a->val = v;
    pthread_mutex_init(&a->lock, NULL);
    return a;
}

int8_t AtomicTrit_get(ManitAtomicTrit* a) {
    if (!a) return 0;
    pthread_mutex_lock(&a->lock);
    int8_t v = a->val;
    pthread_mutex_unlock(&a->lock);
    return v;
}

void AtomicTrit_set(ManitAtomicTrit* a, int8_t v) {
    if (!a) return;
    pthread_mutex_lock(&a->lock);
    a->val = v;
    pthread_mutex_unlock(&a->lock);
}

int8_t AtomicTrit_swap(ManitAtomicTrit* a, int8_t v) {
    if (!a) return 0;
    pthread_mutex_lock(&a->lock);
    int8_t old = a->val;
    a->val = v;
    pthread_mutex_unlock(&a->lock);
    return old;
}

int AtomicTrit_compare_exchange(ManitAtomicTrit* a, int8_t expected, int8_t new_val) {
    if (!a) return 0;
    pthread_mutex_lock(&a->lock);
    int ok = a->val == expected;
    if (ok) a->val = new_val;
    pthread_mutex_unlock(&a->lock);
    return ok;
}

int8_t AtomicTrit_fetch_and(ManitAtomicTrit* a, int8_t v) {
    if (!a) return 0;
    pthread_mutex_lock(&a->lock);
    int8_t old = a->val;
    /* ternary AND = min */
    a->val = old < v ? old : v;
    pthread_mutex_unlock(&a->lock);
    return old;
}

int8_t AtomicTrit_fetch_or(ManitAtomicTrit* a, int8_t v) {
    if (!a) return 0;
    pthread_mutex_lock(&a->lock);
    int8_t old = a->val;
    /* ternary OR = max */
    a->val = old > v ? old : v;
    pthread_mutex_unlock(&a->lock);
    return old;
}

int8_t AtomicTrit_fetch_neg(ManitAtomicTrit* a) {
    if (!a) return 0;
    pthread_mutex_lock(&a->lock);
    int8_t old = a->val;
    a->val = -old;
    pthread_mutex_unlock(&a->lock);
    return old;
}

/* ======================== channel (blocking, thread-safe) ======================== */

#define CHAN_DEFAULT_CAP 256

typedef struct {
    int64_t* buffer;
    int capacity;
    int count;
    int head;
    int tail;
    pthread_mutex_t lock;
    pthread_cond_t not_empty;
    pthread_cond_t not_full;
    int closed;
    /* §11.3's B(c): the tasks waiting to receive, LONGEST-WAITING FIRST.
     * Owned here because a channel is where a waiter waits, but ordered and
     * moved only by `sched.c`, which is the one place that knows what R is. */
    void* waiters;
    void* waiters_tail;
} ManitChan;

ManitChan* channel_bounded(int64_t capacity);

ManitChan* channel_new(void) {
    return channel_bounded(CHAN_DEFAULT_CAP);
}

ManitChan* channel_bounded(int64_t capacity) {
    ManitChan* c = malloc(sizeof(ManitChan));
    if (!c) return NULL;
    if (capacity < 1) capacity = 1;
    if (capacity > (int64_t)1 << 30) capacity = (int64_t)1 << 30;
    c->capacity = (int)capacity;
    c->buffer = malloc(sizeof(int64_t) * (size_t)c->capacity);
    if (!c->buffer) { free(c); return NULL; }
    c->count = 0;
    c->head = 0;
    c->tail = 0;
    c->closed = 0;
    c->waiters = NULL;
    c->waiters_tail = NULL;
    pthread_mutex_init(&c->lock, NULL);
    pthread_cond_init(&c->not_empty, NULL);
    pthread_cond_init(&c->not_full, NULL);
    return c;
}

void channel_send(ManitChan* c, int64_t v) {
    if (!c) return;
    pthread_mutex_lock(&c->lock);
    while (c->count == c->capacity && !c->closed) {
        pthread_cond_wait(&c->not_full, &c->lock);
    }
    if (c->closed) {
        pthread_mutex_unlock(&c->lock);
        fprintf(stderr, "manit: send on closed channel\n");
        return;
    }
    c->buffer[c->tail] = v;
    c->tail = (c->tail + 1) % c->capacity;
    c->count++;
    if (manit_sched_active()) {
        /* §11.5 (SEND) and (SEND-WAKE). The value is appended and AT MOST ONE
         * waiter is woken — the longest-waiting, to the BACK of R. `send` does
         * not yield (§11.4), because §11.1 leaves channels unbounded, so the
         * spawner simply continues. */
        pthread_mutex_unlock(&c->lock);
        manit_sched_wake_one(&c->waiters, &c->waiters_tail);
        return;
    }
    pthread_cond_signal(&c->not_empty);
    pthread_mutex_unlock(&c->lock);
}

int64_t channel_recv(ManitChan* c) {
    if (!c) return 0;
    pthread_mutex_lock(&c->lock);
    /* P81. An OPEN, EMPTY channel can never be filled under the current
     * contract: `spawn { B }` runs B in place and to completion, so there is no
     * other task and never will be (report.txt P5, docs/memory-model.md). The
     * wait below is therefore a guaranteed deadlock — and it deadlocked in the
     * worst available way, blocking with stdout unflushed so the program
     * printed NOTHING AT ALL, including everything it had produced before the
     * recv. T3 meanwhile returned 0 and carried on. One program, a wrong answer
     * on one backend and silence on the other.
     *
     * Faulting is not a decision about cooperative vs pre-emptive scheduling —
     * that is what P5 is parked on. It is the two backends agreeing about a
     * program that cannot make progress, and it stops being reachable the day
     * `spawn` starts a real task. A CLOSED empty channel is untouched: that is
     * the drain case, and it already returns 0 here without waiting. */
    if (manit_sched_active()) {
        /* §11.5 (RECV) and (RECV-BLOCK). An empty queue is the second of
         * §11.4's three yield points: the task leaves R and joins B(c). The
         * LOOP is the specification rather than defensive coding — §11.7 says
         * a woken receiver re-executes its `recv`, and that is exactly what
         * makes a wake-all bug invisible to a test that counts wakes.
         *
         * P81's fault below does not apply here and must not: it exists
         * because `spawn` ran its block in place, so an open empty channel
         * could never be filled. Under §11 there may be a task that has not
         * run yet, and the case where there genuinely is not is §11.6's
         * deadlock trap, raised by the scheduler when R empties. */
        while (c->count == 0 && !c->closed) {
            pthread_mutex_unlock(&c->lock);
            manit_sched_block_on(&c->waiters, &c->waiters_tail);
            pthread_mutex_lock(&c->lock);
        }
    } else if (c->count == 0 && !c->closed) {
        pthread_mutex_unlock(&c->lock);
        manit_fault("recv on an empty channel that is still open: nothing can "
                    "send to it, because `spawn` runs its block in place");
    }
    while (c->count == 0 && !c->closed) {
        pthread_cond_wait(&c->not_empty, &c->lock);
    }
    if (c->count == 0) {
        pthread_mutex_unlock(&c->lock);
        return 0;
    }
    int64_t v = c->buffer[c->head];
    c->head = (c->head + 1) % c->capacity;
    c->count--;
    pthread_cond_signal(&c->not_full);
    pthread_mutex_unlock(&c->lock);
    return v;
}

int64_t channel_len(ManitChan* c) {
    if (!c) return 0;
    pthread_mutex_lock(&c->lock);
    int64_t n = c->count;
    pthread_mutex_unlock(&c->lock);
    return n;
}

int channel_is_empty(ManitChan* c) {
    return channel_len(c) == 0;
}

void channel_close(ManitChan* c) {
    if (!c) return;
    pthread_mutex_lock(&c->lock);
    c->closed = 1;
    pthread_cond_broadcast(&c->not_empty);
    pthread_cond_broadcast(&c->not_full);
    pthread_mutex_unlock(&c->lock);
}

int channel_is_closed(ManitChan* c) {
    if (!c) return 1;
    pthread_mutex_lock(&c->lock);
    int r = c->closed && c->count == 0;
    pthread_mutex_unlock(&c->lock);
    return r;
}

/* P99 / §11.4: `yield_now` is a YIELD POINT and must hand the baton to the next
 * task, not to the next OS thread.
 *
 * `sched_yield(3)` — POSIX, which is what this used to call unconditionally —
 * yields the THREAD. Under the cooperative scheduler exactly one task runs at a
 * time and the running one holds the baton, so yielding the thread hands it
 * straight back and the caller spins forever holding the very thing the other
 * tasks are waiting for. `examples/concurrency.mt`'s first demo does exactly
 * that: its consumer loops on `try_recv` and calls `yield_now` when the channel
 * is empty, so the producer could never run.
 *
 * The two backends' definitions LOOKED equivalent and were not. T3 compiles
 * `async::yield_now` to `SYSCALL #81`, whose comment still reads "no-op"
 * because it was one when it was written — P88 made it a real task yield, and
 * T3 inherited the correct behaviour without the line changing. LLVM's
 * `sched_yield()` kept the name it always had and stopped meaning the same
 * thing. */
void async_yield_now(void) {
    if (manit_sched_active()) { __task_yield(); return; }
    sched_yield();
}

/* ======================== ternary utils ======================== */

int64_t ternary_trit_to_int(int8_t t) { return (int64_t)t; }
/* Superseded 19 August 2026: this is now ManiT source in
 * stdlib/ternary.mt, so both backends share one definition. Leaving the C
 * copy here made the linker reject every program that called it -- the
 * merged ManiT function mangles to the same symbol.
 * The sign-clamp below is what the ManiT version does, which is why
 * int_to_trit was DIVERGENT: the T3 emitter treated it as an identity
 * move, so int_to_trit(5) returned 5 there and +1 here. */
char* ternary_t27_to_str(int64_t n); /* forward declaration */
char* ternary_to_balanced_ternary(int64_t n) { return ternary_t27_to_str(n); }
int64_t ternary_from_balanced_ternary(const char* s) {
    if (!s) return 0;
    int64_t result = 0;
    for (int i = 0; s[i]; i++) {
        result *= 3;
        if (s[i] == '+')      result += 1;
        else if (s[i] == '-') result -= 1;
        /* '0' adds nothing */
    }
    return result;
}

char* ternary_t27_to_str(int64_t n) {
    if (n == 0) {
        char* r = malloc(2); if (r) { r[0] = '0'; r[1] = '\0'; } return r;
    }
    char buf[64];
    int idx = 63;
    buf[idx] = '\0';
    int negative = n < 0;
    uint64_t v = negative ? 0 - (uint64_t)n : (uint64_t)n;
    while (v && idx > 0) {
        uint64_t rem = v % 3;
        v /= 3;
        if (rem == 0) {
            buf[--idx] = '0';
        } else if (rem == 1) {
            buf[--idx] = negative ? '-' : '+';
        } else { /* rem == 2: borrow */
            buf[--idx] = negative ? '+' : '-';
            v++;
        }
    }
    return strdup(buf + idx);
}

/* ======================== spawn / join (pthreads) ======================== */

typedef struct {
    pthread_t thread;
    int64_t (*fn_ptr)(int64_t);
    int64_t arg;
    int64_t result;
} ManitTask;

static void* _manit_task_runner(void* arg) {
    ManitTask* task = (ManitTask*)arg;
    task->result = task->fn_ptr(task->arg);
    return NULL;
}

ManitTask* manit_spawn(int64_t (*fn_ptr)(int64_t), int64_t arg) {
    ManitTask* task = malloc(sizeof(ManitTask));
    if (!task) return NULL;
    task->fn_ptr = fn_ptr;
    task->arg = arg;
    task->result = 0;
    if (pthread_create(&task->thread, NULL, _manit_task_runner, task) != 0) {
        free(task);
        return NULL;
    }
    return task;
}

int64_t manit_join(ManitTask* task) {
    if (!task) return 0;
    pthread_join(task->thread, NULL);
    int64_t r = task->result;
    free(task);
    return r;
}

/* ======================== Semaphore ======================== */

typedef struct {
    pthread_mutex_t lock;
    pthread_cond_t cond;
    int64_t permits;
} ManitSemaphore;

ManitSemaphore* Semaphore_new(int64_t permits) {
    ManitSemaphore* s = malloc(sizeof(*s));
    if (!s) return NULL;
    pthread_mutex_init(&s->lock, NULL);
    pthread_cond_init(&s->cond, NULL);
    s->permits = permits;
    return s;
}

void Semaphore_acquire(ManitSemaphore* s) {
    if (!s) return;
    pthread_mutex_lock(&s->lock);
    while (s->permits <= 0)
        pthread_cond_wait(&s->cond, &s->lock);
    s->permits--;
    pthread_mutex_unlock(&s->lock);
}

void Semaphore_release(ManitSemaphore* s) {
    if (!s) return;
    pthread_mutex_lock(&s->lock);
    s->permits++;
    pthread_cond_signal(&s->cond);
    pthread_mutex_unlock(&s->lock);
}

int Semaphore_try_acquire(ManitSemaphore* s) {
    if (!s) return 0;
    pthread_mutex_lock(&s->lock);
    int ok = s->permits > 0;
    if (ok) s->permits--;
    pthread_mutex_unlock(&s->lock);
    return ok;
}

int64_t Semaphore_available(ManitSemaphore* s) {
    if (!s) return 0;
    pthread_mutex_lock(&s->lock);
    int64_t n = s->permits;
    pthread_mutex_unlock(&s->lock);
    return n;
}

/* ======================== Barrier ======================== */

typedef struct {
    pthread_barrier_t barrier;
    int count;
} ManitBarrier;

ManitBarrier* Barrier_new(int64_t n) {
    if (n < 1 || n > 0x7fffffff) return NULL;
    ManitBarrier* b = malloc(sizeof(*b));
    if (!b) return NULL;
    b->count = (int)n;
    if (pthread_barrier_init(&b->barrier, NULL, (unsigned)n) != 0) {
        free(b);
        return NULL;
    }
    return b;
}

int Barrier_wait(ManitBarrier* b) {
    if (!b) return 0;
    int rc = pthread_barrier_wait(&b->barrier);
    return rc == PTHREAD_BARRIER_SERIAL_THREAD ? 1 : 0;
}

int64_t Barrier_count(ManitBarrier* b) {
    return b ? b->count : 0;
}

/* ======================== AtomicInt ======================== */

typedef struct { volatile int64_t val; } ManitAtomicInt;

ManitAtomicInt* AtomicInt_new(int64_t v) {
    ManitAtomicInt* a = malloc(sizeof(*a));
    if (!a) return NULL;
    a->val = v;
    return a;
}

int64_t AtomicInt_load(ManitAtomicInt* a) {
    if (!a) return 0;
    return __atomic_load_n(&a->val, __ATOMIC_SEQ_CST);
}

void AtomicInt_store(ManitAtomicInt* a, int64_t v) {
    if (a) __atomic_store_n(&a->val, v, __ATOMIC_SEQ_CST);
}

int64_t AtomicInt_swap(ManitAtomicInt* a, int64_t v) {
    if (!a) return 0;
    return __atomic_exchange_n(&a->val, v, __ATOMIC_SEQ_CST);
}

int64_t AtomicInt_fetch_add(ManitAtomicInt* a, int64_t v) {
    if (!a) return 0;
    return __atomic_fetch_add(&a->val, v, __ATOMIC_SEQ_CST);
}

int64_t AtomicInt_fetch_sub(ManitAtomicInt* a, int64_t v) {
    if (!a) return 0;
    return __atomic_fetch_sub(&a->val, v, __ATOMIC_SEQ_CST);
}

int AtomicInt_compare_exchange(ManitAtomicInt* a, int64_t expected, int64_t new_val) {
    if (!a) return 0;
    return __atomic_compare_exchange_n(&a->val, &expected, new_val,
                                        0, __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST);
}
