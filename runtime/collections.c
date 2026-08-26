/* collections.c — Vec, Map, Set, Deque, TernaryTrie, Result.
 * Included by manit_runtime.c — do not compile independently.
 * Author: Manish Jagdish Thatte
 */

/* ======================== Vec<T> ======================== */

struct ManitVec {
    int64_t* data;
    int64_t  len;
    int64_t  cap;
};

static ManitVec* Vec_new_internal(void) {
    ManitVec* v = malloc(sizeof(ManitVec));
    if (!v) return NULL;
    v->data = malloc(8 * sizeof(int64_t));
    if (!v->data) { free(v); return NULL; }
    v->len = 0;
    v->cap = 8;
    return v;
}

static void Vec_push_internal(ManitVec* v, int64_t x) {
    if (!v) return;
    if (v->len == v->cap) {
        int64_t ncap = v->cap * 2;
        int64_t* nd = realloc(v->data, (size_t)ncap * sizeof(int64_t));
        if (!nd) {
            fprintf(stderr, "manit: out of memory in Vec_push\n");
            exit(1);
        }
        v->data = nd;
        v->cap = ncap;
    }
    v->data[v->len++] = x;
}

ManitVec* Vec_new(void) { return Vec_new_internal(); }

void Vec_push(ManitVec* v, int64_t x) { Vec_push_internal(v, x); }

int64_t Vec_pop(ManitVec* v) {
    if (!v || v->len == 0) return 0;
    return v->data[--v->len];
}

int64_t Vec_get(ManitVec* v, int64_t i) {
    if (!v || i < 0 || i >= v->len) return 0;
    return v->data[i];
}

void Vec_set(ManitVec* v, int64_t i, int64_t x) {
    if (!v || i < 0 || i >= v->len) return;
    v->data[i] = x;
}

int64_t Vec_len(ManitVec* v) { return v ? v->len : 0; }
int     Vec_is_empty(ManitVec* v) { return !v || v->len == 0; }
void    Vec_clear(ManitVec* v) { if (v) v->len = 0; }

int Vec_contains(ManitVec* v, int64_t x) {
    if (!v) return 0;
    for (int64_t i = 0; i < v->len; i++)
        if (v->data[i] == x) return 1;
    return 0;
}

/* Returns the REMOVED ELEMENT (report.txt P59).
 *
 * This was `void`, while `Vec<T>::remove` is typed `T` by
 * semantic/analyzer/type_inference.rs. Every layer agreed with the C
 * signature and none with the type: the LLVM declaration was `declare void`,
 * the T3 emulator discarded what it removed, and the T3 emitter used the
 * no-result syscall helper -- which allocates the destination register and
 * then drops it, so the caller read whatever the PRECEDING operation had left
 * there. Out of range yields 0 on both backends, matching the old silent
 * return rather than inventing a trap. */
int64_t Vec_remove(ManitVec* v, int64_t i) {
    if (!v || i < 0 || i >= v->len) return 0;
    int64_t removed = v->data[i];
    memmove(v->data + i, v->data + i + 1,
            (size_t)(v->len - i - 1) * sizeof(int64_t));
    v->len--;
    return removed;
}

static int cmp_i64(const void* a, const void* b) {
    int64_t x = *(const int64_t*)a, y = *(const int64_t*)b;
    return (x > y) - (x < y);
}

void Vec_sort(ManitVec* v) {
    if (v && v->len > 0)
        qsort(v->data, (size_t)v->len, sizeof(int64_t), cmp_i64);
}

void Vec_reverse(ManitVec* v) {
    if (!v) return;
    for (int64_t i = 0, j = v->len - 1; i < j; i++, j--) {
        int64_t tmp = v->data[i]; v->data[i] = v->data[j]; v->data[j] = tmp;
    }
}

int64_t Vec_index_of(ManitVec* v, int64_t x) {
    if (!v) return -1;
    for (int64_t i = 0; i < v->len; i++)
        if (v->data[i] == x) return i;
    return -1;
}

/* ---- str elements compare as TEXT, not as addresses --------------------
   A str inside a collection is type-erased to an int64_t, which here is its
   pointer.  The three functions above therefore answer by identity: they match
   only a value that came from the same place, so a needle built at run time
   misses text that is already in the vector, and Vec_sort orders addresses.
   maniTC routes a Vec<str> to these instead (see STR_SENSITIVE in
   ir/lower/lower_expr.rs).  The T3 emulator has its own copies for the same
   reason — its int64_t is an intern id rather than a pointer, which is a
   different wrong answer to the same question. */

int Vec_contains_str(ManitVec* v, int64_t x) {
    if (!v || !x) return 0;
    for (int64_t i = 0; i < v->len; i++)
        if (v->data[i] && strcmp((const char*)v->data[i], (const char*)x) == 0) return 1;
    return 0;
}

int64_t Vec_index_of_str(ManitVec* v, int64_t x) {
    if (!v || !x) return -1;
    for (int64_t i = 0; i < v->len; i++)
        if (v->data[i] && strcmp((const char*)v->data[i], (const char*)x) == 0) return i;
    return -1;
}

static int cmp_str_ptr(const void* a, const void* b) {
    const char* x = (const char*)(*(const int64_t*)a);
    const char* y = (const char*)(*(const int64_t*)b);
    if (!x) return y ? -1 : 0;
    if (!y) return 1;
    return strcmp(x, y);
}

void Vec_sort_str(ManitVec* v) {
    if (v && v->len > 0)
        qsort(v->data, (size_t)v->len, sizeof(int64_t), cmp_str_ptr);
}

typedef int64_t (*ManitFn1)(int64_t);
typedef int64_t (*ManitFn2)(int64_t, int64_t);
/* `_Bool`, not `int`. A maniT `bool` is `i1` in the emitted LLVM IR, and the
 * x86-64 psABI leaves the bits ABOVE bit 0 of the return register UNDEFINED
 * for an i1 return. Declaring the callback as returning `int` therefore read
 * 31 bits of whatever the callee happened to leave in EAX.
 *
 * It was latent for as long as the generated code happened to leave those bits
 * clear — a truncating `x % 2` ends in `sub; sete %al` with a small value
 * already in RAX — and it stopped being latent the moment C4's rounding
 * sequence left -1 there instead: `sete %al` cleared AL and left
 * 0xFFFFFFFFFFFFFF00, so `if (f(...))` was true for every element and
 * `Vec::filter` returned the whole vector. Found by cross-backend parity on
 * examples/data_structures.mt under --lang v2; the T3 backend, whose syscall
 * ABI returns a whole word, was right throughout.
 *
 * `_Bool` is the type that matches an i1 return: clang reads only AL for it. */
typedef _Bool   (*ManitPred)(int64_t);

void Vec_for_each(ManitVec* v, ManitFn1 f) {
    if (!v || !f) return;
    for (int64_t i = 0; i < v->len; i++) f(v->data[i]);
}

ManitVec* Vec_map(ManitVec* v, ManitFn1 f) {
    ManitVec* r = Vec_new();
    if (!v || !f) return r;
    for (int64_t i = 0; i < v->len; i++) Vec_push_internal(r, f(v->data[i]));
    return r;
}

ManitVec* Vec_filter(ManitVec* v, ManitPred f) {
    ManitVec* r = Vec_new();
    if (!v || !f) return r;
    for (int64_t i = 0; i < v->len; i++)
        if (f(v->data[i])) Vec_push_internal(r, v->data[i]);
    return r;
}

int64_t Vec_fold(ManitVec* v, int64_t init, ManitFn2 f) {
    if (!v || !f) return init;
    int64_t acc = init;
    for (int64_t i = 0; i < v->len; i++) acc = f(acc, v->data[i]);
    return acc;
}

ManitVec* Vec_slice(ManitVec* v, int64_t start, int64_t end) {
    ManitVec* r = Vec_new();
    if (!v) return r;
    if (start < 0) start = 0;
    for (int64_t i = start; i < end && i < v->len; i++)
        Vec_push_internal(r, v->data[i]);
    return r;
}

/* str_split — returns a ManitVec of char* pointers */
ManitVec* str_split(const char* s, const char* sep) {
    ManitVec* v = Vec_new();
    if (!v || !s) return v;
    if (!sep || !*sep) {
        char* dup = strdup(s);
        if (dup) Vec_push_internal(v, (int64_t)(intptr_t)dup);
        return v;
    }
    size_t sl = strlen(sep);
    const char* p = s;
    for (;;) {
        const char* q = strstr(p, sep);
        size_t tl = q ? (size_t)(q - p) : strlen(p);
        char* tok = malloc(tl + 1);
        if (!tok) break;
        memcpy(tok, p, tl);
        tok[tl] = '\0';
        Vec_push_internal(v, (int64_t)(intptr_t)tok);
        if (!q) break;
        p = q + sl;
    }
    return v;
}

/* ======================== Map<K,V> ======================== */

#define MAP_INITIAL_CAP 64

typedef struct { int used; int64_t key; int64_t val; } ManitMapEntry;
typedef struct { ManitMapEntry* entries; int64_t count; int64_t cap;
                 int64_t* order; int64_t order_len; int64_t order_cap; } ManitMap;

/* Iteration order is INSERTION order and is part of the maniT language, not a
   property of the table it happens to be stored in.  Walking slots 0..cap
   gives hash order, which is a different sequence from the T3 emulator's, so
   the same program would print different things on the two backends.  These
   two helpers maintain the order array both Map and Set now carry alongside
   the table.  See src/codegen_t3/emulator/ordered.rs for the other half and
   for why insertion order is the only order both backends can agree on. */
static int order_push(int64_t** arr, int64_t* len, int64_t* cap, int64_t k) {
    if (*len >= *cap) {
        int64_t nc = *cap ? *cap * 2 : MAP_INITIAL_CAP;
        int64_t* na = realloc(*arr, (size_t)nc * sizeof(int64_t));
        if (!na) return 0;
        *arr = na; *cap = nc;
    }
    (*arr)[(*len)++] = k;
    return 1;
}

static void order_erase(int64_t* arr, int64_t* len, int64_t k) {
    for (int64_t i = 0; i < *len; i++) {
        if (arr[i] != k) continue;
        for (int64_t j = i; j + 1 < *len; j++) arr[j] = arr[j + 1];
        (*len)--;
        return;
    }
}

static uint64_t map_hash(int64_t key) {
    uint64_t h = (uint64_t)key;
    h ^= h >> 33;
    h *= 0xff51afd7ed558ccdULL;
    h ^= h >> 33;
    return h;
}

ManitMap* Map_new(void) {
    ManitMap* m = malloc(sizeof(ManitMap));
    if (!m) return NULL;
    m->entries = calloc(MAP_INITIAL_CAP, sizeof(ManitMapEntry));
    if (!m->entries) { free(m); return NULL; }
    m->count = 0;
    m->cap = MAP_INITIAL_CAP;
    m->order = NULL; m->order_len = 0; m->order_cap = 0;
    return m;
}

/* A13: probe at most `cap` slots. Map_insert normally holds the load factor at
   50%, but if map_grow fails under memory pressure the table can be driven to
   full, and an unbounded probe for an absent key would then spin forever.
   Returns -1 when the key is absent and there is no free slot. */
static int64_t map_slot(ManitMap* m, int64_t key) {
    int64_t mask = m->cap - 1;
    int64_t slot = (int64_t)(map_hash(key) & (uint64_t)mask);
    for (int64_t probes = 0; probes < m->cap; probes++) {
        if (!m->entries[slot].used || m->entries[slot].key == key) return slot;
        slot = (slot + 1) & mask;
    }
    return -1;
}

static int map_grow(ManitMap* m) {
    int64_t ocap = m->cap;
    ManitMapEntry* oe = m->entries;
    ManitMapEntry* ne = calloc((size_t)ocap * 2, sizeof(ManitMapEntry));
    if (!ne) return 0;
    m->entries = ne;
    m->cap = ocap * 2;
    for (int64_t i = 0; i < ocap; i++) {
        if (!oe[i].used) continue;
        int64_t s = map_slot(m, oe[i].key);
        if (s < 0) { /* cannot happen: the new table is twice as large */
            free(ne); m->entries = oe; m->cap = ocap; return 0;
        }
        m->entries[s] = oe[i];
    }
    free(oe);
    return 1;
}

void Map_insert(ManitMap* m, int64_t k, int64_t v) {
    if (!m) return;
    if (m->count * 2 >= m->cap && !map_grow(m) && m->count + 1 >= m->cap)
        return;
    int64_t s = map_slot(m, k);
    if (s < 0) return;              /* table full and key absent (A13) */
    if (!m->entries[s].used) {
        m->count++;
        order_push(&m->order, &m->order_len, &m->order_cap, k);
    }
    m->entries[s].used = 1; m->entries[s].key = k; m->entries[s].val = v;
}

int64_t Map_get(ManitMap* m, int64_t k) {
    if (!m) return 0;
    int64_t s = map_slot(m, k);
    return (s >= 0 && m->entries[s].used) ? m->entries[s].val : 0;
}

int64_t Map_get_or(ManitMap* m, int64_t k, int64_t def) {
    if (!m) return def;
    int64_t s = map_slot(m, k);
    return (s >= 0 && m->entries[s].used) ? m->entries[s].val : def;
}

int Map_contains_key(ManitMap* m, int64_t k) {
    if (!m) return 0;
    int64_t s = map_slot(m, k);
    return s >= 0 && m->entries[s].used;
}

void Map_remove(ManitMap* m, int64_t k) {
    if (!m) return;
    int64_t s = map_slot(m, k);
    if (s < 0 || !m->entries[s].used) return;
    m->entries[s].used = 0;
    m->count--;
    order_erase(m->order, &m->order_len, k);
    int64_t mask = m->cap - 1;
    int64_t hole = s;
    int64_t j = (s + 1) & mask;
    while (m->entries[j].used) {
        int64_t home = (int64_t)(map_hash(m->entries[j].key) & (uint64_t)mask);
        if (((j - home) & mask) >= ((j - hole) & mask)) {
            m->entries[hole] = m->entries[j];
            m->entries[j].used = 0;
            hole = j;
        }
        j = (j + 1) & mask;
    }
}

int64_t Map_len(ManitMap* m) { return m ? m->count : 0; }
int     Map_is_empty(ManitMap* m) { return !m || m->count == 0; }

/* ---- str keys ----------------------------------------------------------
   Map and Set hash and compare the type-erased int64_t, which for a str is its
   pointer.  That made a map keyed by strings answer by identity: it matched
   only literals the C compiler had already merged, a key built at run time
   missed its own entry, and inserting the same text twice made two entries.
   The T3 emulator does not have this bug — it interns every str key by content
   inside the syscall — so this was a real divergence, and T3 was the correct
   side.

   Interning here makes the native backend agree, and it is the same mechanism:
   one canonical address per distinct text, so identity BECOMES equality and
   everything downstream (including the set algebra, which compares stored
   entries) is correct without further change. */

/* An open-addressed table, hashed on the TEXT, that grows.  A fixed cap with a
   fall back to identity beyond it would put a silent cliff in the middle of the
   correctness this is here to provide: a program would be right for the first N
   distinct strings and quietly wrong afterwards, with nothing to see.  A linear
   scan would have been simpler and is O(n) per lookup, which is O(n^2) to fill
   a map — hashing keeps insert and lookup flat. */
static char**  g_intern = NULL;
static int64_t g_intern_cap = 0;   /* always a power of two, or 0 */
static int64_t g_intern_len = 0;

static uint64_t str_hash(const char* s) {
    uint64_t h = 1469598103934665603ULL;          /* FNV-1a */
    while (*s) { h ^= (unsigned char)*s++; h *= 1099511628211ULL; }
    return h;
}

/* Place an already-owned string; used both by the public entry point and by
   growth, which must not re-copy. */
static void intern_place(char* owned) {
    int64_t mask = g_intern_cap - 1;
    int64_t i = (int64_t)(str_hash(owned) & (uint64_t)mask);
    while (g_intern[i]) i = (i + 1) & mask;
    g_intern[i] = owned;
}

static int intern_grow(void) {
    int64_t ocap = g_intern_cap;
    char** old = g_intern;
    int64_t ncap = ocap ? ocap * 2 : 256;
    char** ne = calloc((size_t)ncap, sizeof(char*));
    if (!ne) return 0;
    g_intern = ne; g_intern_cap = ncap;
    for (int64_t i = 0; i < ocap; i++)
        if (old[i]) intern_place(old[i]);
    free(old);
    return 1;
}

int64_t manit_intern_str(int64_t p) {
    const char* s = (const char*)p;
    if (!s) return p;
    if (g_intern_len * 2 >= g_intern_cap && !intern_grow()) return p;  /* OOM only */
    int64_t mask = g_intern_cap - 1;
    int64_t i = (int64_t)(str_hash(s) & (uint64_t)mask);
    while (g_intern[i]) {
        if (strcmp(g_intern[i], s) == 0) return (int64_t)g_intern[i];
        i = (i + 1) & mask;
    }
    size_t n = strlen(s) + 1;
    char* copy = malloc(n);
    if (!copy) return p;
    memcpy(copy, s, n);
    g_intern[i] = copy;
    g_intern_len++;
    return (int64_t)copy;
}

void    Map_insert_str(ManitMap* m, int64_t k, int64_t v) { Map_insert(m, manit_intern_str(k), v); }
int64_t Map_get_str(ManitMap* m, int64_t k)               { return Map_get(m, manit_intern_str(k)); }
int64_t Map_get_or_str(ManitMap* m, int64_t k, int64_t d) { return Map_get_or(m, manit_intern_str(k), d); }
int     Map_contains_key_str(ManitMap* m, int64_t k)      { return Map_contains_key(m, manit_intern_str(k)); }
void    Map_remove_str(ManitMap* m, int64_t k)            { Map_remove(m, manit_intern_str(k)); }

ManitVec* Map_keys(ManitMap* m) {
    ManitVec* v = Vec_new();
    if (!m) return v;
    for (int64_t i = 0; i < m->order_len; i++)
        Vec_push_internal(v, m->order[i]);
    return v;
}

/* Same order as Map_keys, so the two can be paired by index. */
ManitVec* Map_values(ManitMap* m) {
    ManitVec* v = Vec_new();
    if (!m) return v;
    for (int64_t i = 0; i < m->order_len; i++)
        Vec_push_internal(v, Map_get(m, m->order[i]));
    return v;
}

/* ======================== Set<T> ======================== */

typedef struct { int used; int64_t key; } ManitSetEntry;
typedef struct { ManitSetEntry* entries; int64_t count; int64_t cap;
                 int64_t* order; int64_t order_len; int64_t order_cap; } ManitSet;

ManitSet* Set_new(void) {
    ManitSet* s = malloc(sizeof(ManitSet));
    if (!s) return NULL;
    s->entries = calloc(MAP_INITIAL_CAP, sizeof(ManitSetEntry));
    if (!s->entries) { free(s); return NULL; }
    s->count = 0;
    s->cap = MAP_INITIAL_CAP;
    s->order = NULL; s->order_len = 0; s->order_cap = 0;
    return s;
}

/* A13: bounded probe, as for map_slot. -1 = absent and no free slot. */
static int64_t set_slot(ManitSet* s, int64_t key) {
    int64_t mask = s->cap - 1;
    int64_t slot = (int64_t)(map_hash(key) & (uint64_t)mask);
    for (int64_t probes = 0; probes < s->cap; probes++) {
        if (!s->entries[slot].used || s->entries[slot].key == key) return slot;
        slot = (slot + 1) & mask;
    }
    return -1;
}

static int set_grow(ManitSet* s) {
    int64_t ocap = s->cap;
    ManitSetEntry* oe = s->entries;
    ManitSetEntry* ne = calloc((size_t)ocap * 2, sizeof(ManitSetEntry));
    if (!ne) return 0;
    s->entries = ne;
    s->cap = ocap * 2;
    for (int64_t i = 0; i < ocap; i++) {
        if (!oe[i].used) continue;
        int64_t slot = set_slot(s, oe[i].key);
        if (slot < 0) { /* cannot happen: the new table is twice as large */
            free(ne); s->entries = oe; s->cap = ocap; return 0;
        }
        s->entries[slot] = oe[i];
    }
    free(oe);
    return 1;
}

void Set_insert(ManitSet* s, int64_t x) {
    if (!s) return;
    if (s->count * 2 >= s->cap && !set_grow(s) && s->count + 1 >= s->cap)
        return;
    int64_t slot = set_slot(s, x);
    if (slot < 0) return;           /* table full and key absent (A13) */
    if (!s->entries[slot].used) {
        s->count++;
        order_push(&s->order, &s->order_len, &s->order_cap, x);
    }
    s->entries[slot].used = 1; s->entries[slot].key = x;
}

int Set_contains(ManitSet* s, int64_t x) {
    if (!s) return 0;
    int64_t slot = set_slot(s, x);
    return slot >= 0 && s->entries[slot].used;
}

void Set_remove(ManitSet* s, int64_t x) {
    if (!s) return;
    int64_t slot = set_slot(s, x);
    if (slot < 0 || !s->entries[slot].used) return;
    s->entries[slot].used = 0;
    s->count--;
    order_erase(s->order, &s->order_len, x);
    int64_t mask = s->cap - 1;
    int64_t hole = slot;
    int64_t j = (slot + 1) & mask;
    while (s->entries[j].used) {
        int64_t home = (int64_t)(map_hash(s->entries[j].key) & (uint64_t)mask);
        if (((j - home) & mask) >= ((j - hole) & mask)) {
            s->entries[hole] = s->entries[j];
            s->entries[j].used = 0;
            hole = j;
        }
        j = (j + 1) & mask;
    }
}

int64_t Set_len(ManitSet* s) { return s ? s->count : 0; }

void Set_for_each(ManitSet* s, ManitFn1 f) {
    if (!s || !f) return;
    for (int64_t i = 0; i < s->order_len; i++) f(s->order[i]);
}

ManitSet* Set_intersection(ManitSet* a, ManitSet* b) {
    ManitSet* r = Set_new();
    if (!r || !a || !b) return r;
    for (int64_t i = 0; i < a->order_len; i++)
        if (Set_contains(b, a->order[i])) Set_insert(r, a->order[i]);
    return r;
}

ManitSet* Set_union(ManitSet* a, ManitSet* b) {
    ManitSet* r = Set_new();
    if (!r || !a || !b) return r;
    for (int64_t i = 0; i < a->order_len; i++) Set_insert(r, a->order[i]);
    for (int64_t i = 0; i < b->order_len; i++) Set_insert(r, b->order[i]);
    return r;
}

ManitSet* Set_difference(ManitSet* a, ManitSet* b) {
    ManitSet* r = Set_new();
    if (!r || !a || !b) return r;
    for (int64_t i = 0; i < a->order_len; i++)
        if (!Set_contains(b, a->order[i])) Set_insert(r, a->order[i]);
    return r;
}

int Set_is_subset(ManitSet* a, ManitSet* b) {
    if (!a || !b) return 0;
    for (int64_t i = 0; i < a->cap; i++)
        if (a->entries[i].used && !Set_contains(b, a->entries[i].key)) return 0;
    return 1;
}

int Set_is_superset(ManitSet* a, ManitSet* b) { return Set_is_subset(b, a); }

/* str elements — see manit_intern_str above.  Only the three entry points that
   take an element need it; once every stored element is canonical, the set
   algebra compares them correctly on its own. */
void Set_insert_str(ManitSet* s, int64_t x)   { Set_insert(s, manit_intern_str(x)); }
int  Set_contains_str(ManitSet* s, int64_t x) { return Set_contains(s, manit_intern_str(x)); }
void Set_remove_str(ManitSet* s, int64_t x)   { Set_remove(s, manit_intern_str(x)); }

int Set_is_disjoint(ManitSet* a, ManitSet* b) {
    if (!a || !b) return 1;
    for (int64_t i = 0; i < a->cap; i++)
        if (a->entries[i].used && Set_contains(b, a->entries[i].key)) return 0;
    return 1;
}

/* ======================== Deque<T> ======================== */

typedef struct ManitDequeNode {
    int64_t val;
    struct ManitDequeNode* prev;
    struct ManitDequeNode* next;
} ManitDequeNode;

typedef struct {
    ManitDequeNode* head;
    ManitDequeNode* tail;
    int64_t len;
} ManitDeque;

ManitDeque* Deque_new(void) { return calloc(1, sizeof(ManitDeque)); }

void Deque_push_front(ManitDeque* d, int64_t x) {
    if (!d) return;
    ManitDequeNode* n = malloc(sizeof(ManitDequeNode));
    if (!n) return;
    n->val = x; n->prev = NULL; n->next = d->head;
    if (d->head) d->head->prev = n;
    d->head = n;
    if (!d->tail) d->tail = n;
    d->len++;
}

void Deque_push_back(ManitDeque* d, int64_t x) {
    if (!d) return;
    ManitDequeNode* n = malloc(sizeof(ManitDequeNode));
    if (!n) return;
    n->val = x; n->next = NULL; n->prev = d->tail;
    if (d->tail) d->tail->next = n;
    d->tail = n;
    if (!d->head) d->head = n;
    d->len++;
}

int64_t Deque_pop_front(ManitDeque* d) {
    if (!d || !d->head) return 0;
    ManitDequeNode* n = d->head;
    int64_t v = n->val;
    d->head = n->next;
    if (d->head) d->head->prev = NULL;
    else d->tail = NULL;
    free(n); d->len--;
    return v;
}

int64_t Deque_pop_back(ManitDeque* d) {
    if (!d || !d->tail) return 0;
    ManitDequeNode* n = d->tail;
    int64_t v = n->val;
    d->tail = n->prev;
    if (d->tail) d->tail->next = NULL;
    else d->head = NULL;
    free(n); d->len--;
    return v;
}

int64_t Deque_front(ManitDeque* d) { return d && d->head ? d->head->val : 0; }
int64_t Deque_back(ManitDeque* d) { return d && d->tail ? d->tail->val : 0; }
int64_t Deque_len(ManitDeque* d) { return d ? d->len : 0; }
int     Deque_is_empty(ManitDeque* d) { return !d || d->len == 0; }

int Deque_contains(ManitDeque* d, int64_t x) {
    if (!d) return 0;
    ManitDequeNode* cur = d->head;
    while (cur) { if (cur->val == x) return 1; cur = cur->next; }
    return 0;
}

/* ======================== TernaryTrie ======================== */
/* Keys are Vec<int> (ManitVec*) where each element is a trit value (-1, 0, +1).
   Internally we use a 3-child trie: children[0]=minus, children[1]=zero, children[2]=plus. */

typedef struct ManitTrieNode {
    int used;
    int64_t val;
    struct ManitTrieNode* children[3]; /* [0]=-, [1]=0, [2]=+ */
} ManitTrieNode;

typedef struct {
    ManitTrieNode* root;
    int64_t count;
} ManitTrie;

/* Map trit value (-1,0,+1) to child index (0,1,2) */
static inline int trit_to_idx(int64_t t) {
    if (t < 0) return 0;
    if (t > 0) return 2;
    return 1;
}

ManitTrie* TernaryTrie_new(void) {
    ManitTrie* t = calloc(1, sizeof(ManitTrie));
    if (!t) return NULL;
    t->root = calloc(1, sizeof(ManitTrieNode));
    if (!t->root) { free(t); return NULL; }
    return t;
}

void TernaryTrie_insert(ManitTrie* t, ManitVec* key, int64_t val) {
    if (!t || !key) return;
    ManitTrieNode* n = t->root;
    int64_t len = Vec_len(key);
    for (int64_t i = 0; i < len; i++) {
        int idx = trit_to_idx(Vec_get(key, i));
        if (!n->children[idx]) {
            n->children[idx] = calloc(1, sizeof(ManitTrieNode));
            if (!n->children[idx]) return;
        }
        n = n->children[idx];
    }
    if (!n->used) t->count++;
    n->used = 1; n->val = val;
}

int64_t TernaryTrie_get(ManitTrie* t, ManitVec* key) {
    if (!t || !key) return 0;
    ManitTrieNode* n = t->root;
    int64_t len = Vec_len(key);
    for (int64_t i = 0; i < len && n; i++) {
        int idx = trit_to_idx(Vec_get(key, i));
        n = n->children[idx];
    }
    return n && n->used ? n->val : 0;
}

int TernaryTrie_contains(ManitTrie* t, ManitVec* key) {
    if (!t || !key) return 0;
    ManitTrieNode* n = t->root;
    int64_t len = Vec_len(key);
    for (int64_t i = 0; i < len && n; i++) {
        int idx = trit_to_idx(Vec_get(key, i));
        n = n->children[idx];
    }
    return n && n->used;
}

int64_t TernaryTrie_len(ManitTrie* t) { return t ? t->count : 0; }

/* Collect all keys as Vec<Vec<int>> under a given node.
   prefix_vec holds the current path; we clone it when a node is marked used. */
static void trie_collect_keys(ManitTrieNode* n, ManitVec* prefix_vec, ManitVec* result) {
    if (n->used) {
        /* Clone the prefix vector */
        ManitVec* clone = Vec_new();
        int64_t plen = Vec_len(prefix_vec);
        for (int64_t i = 0; i < plen; i++) Vec_push(clone, Vec_get(prefix_vec, i));
        Vec_push(result, (int64_t)(intptr_t)clone);
    }
    static const int64_t trit_vals[3] = {-1, 0, 1};
    for (int c = 0; c < 3; c++) {
        if (n->children[c]) {
            Vec_push(prefix_vec, trit_vals[c]);
            trie_collect_keys(n->children[c], prefix_vec, result);
            Vec_pop(prefix_vec); /* backtrack */
        }
    }
}

ManitVec* TernaryTrie_keys_with_prefix(ManitTrie* t, ManitVec* prefix) {
    ManitVec* v = Vec_new();
    if (!t) return v;
    ManitTrieNode* n = t->root;
    if (prefix) {
        int64_t plen = Vec_len(prefix);
        for (int64_t i = 0; i < plen && n; i++) {
            int idx = trit_to_idx(Vec_get(prefix, i));
            n = n->children[idx];
        }
    }
    if (n) {
        ManitVec* pv = Vec_new();
        if (prefix) {
            int64_t plen = Vec_len(prefix);
            for (int64_t i = 0; i < plen; i++) Vec_push(pv, Vec_get(prefix, i));
        }
        trie_collect_keys(n, pv, v);
    }
    return v;
}

/* ======================== Result types ======================== */

typedef struct { int64_t tag; int64_t val; const char* msg; } ManitResult;
/* tag: 1=Ok, -1=Err, 0=Unknown */

ManitResult* Ok_new(int64_t v) {
    ManitResult* r = malloc(sizeof(*r));
    if (!r) return NULL;
    r->tag = 1; r->val = v; r->msg = NULL;
    return r;
}

ManitResult* Err_new(const char* m) {
    ManitResult* r = malloc(sizeof(*r));
    if (!r) return NULL;
    r->tag = -1; r->val = 0; r->msg = m ? strdup(m) : NULL;
    return r;
}

ManitResult* Unknown_new(const char* m) {
    ManitResult* r = malloc(sizeof(*r));
    if (!r) return NULL;
    r->tag = 0; r->val = 0; r->msg = m ? strdup(m) : NULL;
    return r;
}

int result_is_ok(ManitResult* r) { return r && r->tag == 1; }
int result_is_err(ManitResult* r) { return r && r->tag == -1; }
int result_is_unknown(ManitResult* r) { return r && r->tag == 0; }
int64_t result_unwrap(ManitResult* r) { return r ? r->val : 0; }
