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

void Vec_remove(ManitVec* v, int64_t i) {
    if (!v || i < 0 || i >= v->len) return;
    memmove(v->data + i, v->data + i + 1,
            (size_t)(v->len - i - 1) * sizeof(int64_t));
    v->len--;
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

typedef int64_t (*ManitFn1)(int64_t);
typedef int64_t (*ManitFn2)(int64_t, int64_t);
typedef int     (*ManitPred)(int64_t);

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
typedef struct { ManitMapEntry* entries; int64_t count; int64_t cap; } ManitMap;

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
    return m;
}

static int64_t map_slot(ManitMap* m, int64_t key) {
    int64_t mask = m->cap - 1;
    int64_t slot = (int64_t)(map_hash(key) & (uint64_t)mask);
    while (m->entries[slot].used && m->entries[slot].key != key)
        slot = (slot + 1) & mask;
    return slot;
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
    if (!m->entries[s].used) m->count++;
    m->entries[s].used = 1; m->entries[s].key = k; m->entries[s].val = v;
}

int64_t Map_get(ManitMap* m, int64_t k) {
    if (!m) return 0;
    int64_t s = map_slot(m, k);
    return m->entries[s].used ? m->entries[s].val : 0;
}

int64_t Map_get_or(ManitMap* m, int64_t k, int64_t def) {
    if (!m) return def;
    int64_t s = map_slot(m, k);
    return m->entries[s].used ? m->entries[s].val : def;
}

int Map_contains_key(ManitMap* m, int64_t k) {
    if (!m) return 0;
    int64_t s = map_slot(m, k);
    return m->entries[s].used;
}

void Map_remove(ManitMap* m, int64_t k) {
    if (!m) return;
    int64_t s = map_slot(m, k);
    if (!m->entries[s].used) return;
    m->entries[s].used = 0;
    m->count--;
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

ManitVec* Map_keys(ManitMap* m) {
    ManitVec* v = Vec_new();
    if (!m) return v;
    for (int64_t i = 0; i < m->cap; i++)
        if (m->entries[i].used) Vec_push_internal(v, m->entries[i].key);
    return v;
}

ManitVec* Map_values(ManitMap* m) {
    ManitVec* v = Vec_new();
    if (!m) return v;
    for (int64_t i = 0; i < m->cap; i++)
        if (m->entries[i].used) Vec_push_internal(v, m->entries[i].val);
    return v;
}

/* ======================== Set<T> ======================== */

typedef struct { int used; int64_t key; } ManitSetEntry;
typedef struct { ManitSetEntry* entries; int64_t count; int64_t cap; } ManitSet;

ManitSet* Set_new(void) {
    ManitSet* s = malloc(sizeof(ManitSet));
    if (!s) return NULL;
    s->entries = calloc(MAP_INITIAL_CAP, sizeof(ManitSetEntry));
    if (!s->entries) { free(s); return NULL; }
    s->count = 0;
    s->cap = MAP_INITIAL_CAP;
    return s;
}

static int64_t set_slot(ManitSet* s, int64_t key) {
    int64_t mask = s->cap - 1;
    int64_t slot = (int64_t)(map_hash(key) & (uint64_t)mask);
    while (s->entries[slot].used && s->entries[slot].key != key)
        slot = (slot + 1) & mask;
    return slot;
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
    if (!s->entries[slot].used) s->count++;
    s->entries[slot].used = 1; s->entries[slot].key = x;
}

int Set_contains(ManitSet* s, int64_t x) {
    if (!s) return 0;
    int64_t slot = set_slot(s, x);
    return s->entries[slot].used;
}

void Set_remove(ManitSet* s, int64_t x) {
    if (!s) return;
    int64_t slot = set_slot(s, x);
    if (!s->entries[slot].used) return;
    s->entries[slot].used = 0;
    s->count--;
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
    for (int64_t i = 0; i < s->cap; i++)
        if (s->entries[i].used) f(s->entries[i].key);
}

ManitSet* Set_intersection(ManitSet* a, ManitSet* b) {
    ManitSet* r = Set_new();
    if (!r || !a || !b) return r;
    for (int64_t i = 0; i < a->cap; i++)
        if (a->entries[i].used && Set_contains(b, a->entries[i].key))
            Set_insert(r, a->entries[i].key);
    return r;
}

ManitSet* Set_union(ManitSet* a, ManitSet* b) {
    ManitSet* r = Set_new();
    if (!r || !a || !b) return r;
    for (int64_t i = 0; i < a->cap; i++)
        if (a->entries[i].used) Set_insert(r, a->entries[i].key);
    for (int64_t i = 0; i < b->cap; i++)
        if (b->entries[i].used) Set_insert(r, b->entries[i].key);
    return r;
}

ManitSet* Set_difference(ManitSet* a, ManitSet* b) {
    ManitSet* r = Set_new();
    if (!r || !a || !b) return r;
    for (int64_t i = 0; i < a->cap; i++)
        if (a->entries[i].used && !Set_contains(b, a->entries[i].key))
            Set_insert(r, a->entries[i].key);
    return r;
}

int Set_is_subset(ManitSet* a, ManitSet* b) {
    if (!a || !b) return 0;
    for (int64_t i = 0; i < a->cap; i++)
        if (a->entries[i].used && !Set_contains(b, a->entries[i].key)) return 0;
    return 1;
}

int Set_is_superset(ManitSet* a, ManitSet* b) { return Set_is_subset(b, a); }

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
