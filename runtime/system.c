/* system.c — fs, env/argv, time, path utilities, aliases, dir listing, process spawn, terminal.
 * Included by manit_runtime.c — do not compile independently.
 * Author: Manish Jagdish Thatte
 */

#include <sys/wait.h>

/* ======================== fs (filesystem) ======================== */

int fs_exists(const char* path) {
    struct stat st;
    return stat(path, &st) == 0;
}

int fs_is_file(const char* path) {
    struct stat st;
    return stat(path, &st) == 0 && S_ISREG(st.st_mode);
}

int64_t fs_is_dir(const char* path) {
    struct stat st;
    return (stat(path, &st) == 0 && S_ISDIR(st.st_mode)) ? 1 : 0;
}

char* fs_read_file(const char* path) {
    FILE* f = fopen(path, "rb");
    if (!f) return strdup("");
    long sz = -1;
    if (fseek(f, 0, SEEK_END) == 0) {
        sz = ftell(f);
        if (fseek(f, 0, SEEK_SET) != 0) sz = -1;
    }
    if (sz > 0) {
        char* buf = malloc((size_t)sz + 1);
        if (!buf) { fclose(f); return strdup(""); }
        size_t rd = fread(buf, 1, (size_t)sz, f);
        buf[rd] = '\0';
        fclose(f);
        return buf;
    }
    size_t cap = 8192, len = 0;
    char* buf = malloc(cap);
    if (!buf) { fclose(f); return strdup(""); }
    for (;;) {
        size_t rd = fread(buf + len, 1, cap - len - 1, f);
        len += rd;
        if (rd == 0) break;
        if (len + 1 >= cap) {
            char* nb = realloc(buf, cap * 2);
            if (!nb) break;
            buf = nb;
            cap *= 2;
        }
    }
    buf[len] = '\0';
    fclose(f);
    return buf;
}

void fs_write_file(const char* path, const char* content) {
    FILE* f = fopen(path, "wb");
    if (!f) return;
    fwrite(content, 1, strlen(content), f);
    fclose(f);
}

void fs_append_file(const char* path, const char* content) {
    FILE* f = fopen(path, "ab");
    if (!f) return;
    fwrite(content, 1, strlen(content), f);
    fclose(f);
}

void fs_remove_file(const char* path) {
    remove(path);
}

/* fs_delete: remove file or empty directory */
int64_t fs_delete(const char* path) {
    if (remove(path) == 0) return 1;
    if (rmdir(path)  == 0) return 1;
    return 0;
}

void fs_copy_file(const char* src, const char* dst) {
    FILE* in = fopen(src, "rb");
    if (!in) return;
    FILE* out = fopen(dst, "wb");
    if (!out) { fclose(in); return; }
    char buf[8192];
    size_t n;
    while ((n = fread(buf, 1, sizeof(buf), in)) > 0)
        fwrite(buf, 1, n, out);
    fclose(in);
    fclose(out);
}

void fs_rename(const char* src, const char* dst) {
    rename(src, dst);
}

void fs_create_dir(const char* path) {
    mkdir(path, 0755);
}

void fs_create_dir_all(const char* path) {
    /* Simple recursive mkdir */
    char* tmp = strdup(path);
    if (!tmp) return;
    for (char* p = tmp + 1; *p; p++) {
        if (*p == '/') {
            *p = '\0';
            mkdir(tmp, 0755);
            *p = '/';
        }
    }
    mkdir(tmp, 0755);
    free(tmp);
}

void fs_remove_dir(const char* path) {
    rmdir(path);
}

int64_t fs_file_size(const char* path) {
    struct stat st;
    if (stat(path, &st) != 0) return -1;
    return (int64_t)st.st_size;
}

/* File handle operations */

typedef struct {
    FILE* fp;
    char* path;
} ManitFile;

ManitFile* fs_open(const char* path, const char* mode) {
    FILE* fp = fopen(path, mode);
    if (!fp) return NULL;
    ManitFile* f = malloc(sizeof(ManitFile));
    if (!f) { fclose(fp); return NULL; }
    f->fp = fp;
    f->path = strdup(path);
    return f;
}

char* fs_read(ManitFile* f, int64_t n) {
    if (!f || !f->fp || n < 0) return strdup("");
    char* buf = malloc((size_t)n + 1);
    if (!buf) return strdup("");
    size_t rd = fread(buf, 1, (size_t)n, f->fp);
    buf[rd] = '\0';
    return buf;
}

char* fs_read_line(ManitFile* f) {
    if (!f || !f->fp) return strdup("");
    char* buf = malloc(4096);
    if (!buf) return strdup("");
    if (!fgets(buf, 4096, f->fp)) { buf[0] = '\0'; }
    return buf;
}

void fs_write(ManitFile* f, const char* s) {
    if (f && f->fp) fwrite(s, 1, strlen(s), f->fp);
}

void fs_close(ManitFile* f) {
    if (!f) return;
    if (f->fp) fclose(f->fp);
    free(f->path);
    free(f);
}

void fs_flush(ManitFile* f) {
    if (f && f->fp) fflush(f->fp);
}

void fs_seek(ManitFile* f, int64_t pos) {
    if (f && f->fp) fseek(f->fp, (long)pos, SEEK_SET);
}

int64_t fs_tell(ManitFile* f) {
    if (!f || !f->fp) return -1;
    return (int64_t)ftell(f->fp);
}

/* ======================== env (environment) ======================== */

char* env_get_env(const char* key) {
    const char* v = getenv(key);
    return v ? strdup(v) : strdup("");
}

char* env_get_env_or(const char* key, const char* def) {
    const char* v = getenv(key);
    return strdup(v ? v : def);
}

void env_set_env(const char* key, const char* value) {
    setenv(key, value, 1);
}

void env_unset_env(const char* key) {
    unsetenv(key);
}

int env_has_env(const char* key) {
    return getenv(key) != NULL;
}

/* ── argv access via /proc/self/cmdline ──────────────────────────────────── */
static int   g_argc_parsed = 0;
static int   g_argc_val    = 0;
static char* g_argv_ptrs[128];
static char  g_cmdline_buf[8192];

static void parse_cmdline_once(void) {
    if (g_argc_parsed) return;
    g_argc_parsed = 1;
    FILE* f = fopen("/proc/self/cmdline", "rb");
    if (!f) return;
    size_t n = fread(g_cmdline_buf, 1, sizeof(g_cmdline_buf) - 1, f);
    fclose(f);
    g_cmdline_buf[n] = '\0';
    g_argc_val = 0;
    size_t i = 0;
    while (i < n && g_argc_val < 127) {
        g_argv_ptrs[g_argc_val++] = &g_cmdline_buf[i];
        while (i < n && g_cmdline_buf[i] != '\0') i++;
        i++;
    }
}

int64_t env_argc(void) {
    parse_cmdline_once();
    return (int64_t)g_argc_val;
}

char* env_argv(int64_t idx) {
    parse_cmdline_once();
    if (idx < 0 || idx >= g_argc_val) return strdup("");
    return strdup(g_argv_ptrs[idx]);
}

char* env_cwd(void) {
    char buf[4096];
    if (getcwd(buf, sizeof(buf))) return strdup(buf);
    return strdup("");
}

/* A12: chdir's result was discarded, so env::set_cwd("/nonexistent") reported
   nothing and the program carried on in the old directory. The ABI is void
   (declared `void @env_set_cwd(ptr)` for the LLVM backend), so report on
   stderr; a status-returning variant would be an API change. */
void env_set_cwd(const char* path) {
    if (chdir(path) != 0)
        fprintf(stderr, "manit: env::set_cwd(\"%s\") failed: %s\n",
                path ? path : "(null)", strerror(errno));
}

int64_t env_pid(void) { return (int64_t)getpid(); }
int64_t env_ppid(void) { return (int64_t)getppid(); }

char* env_os_name(void) {
#if defined(__linux__)
    return strdup("linux");
#elif defined(__APPLE__)
    return strdup("macos");
#else
    return strdup("unknown");
#endif
}

char* env_arch(void) {
#if defined(__x86_64__)
    return strdup("x86_64");
#elif defined(__aarch64__)
    return strdup("aarch64");
#else
    return strdup("unknown");
#endif
}

int64_t env_cpu_count(void) {
    return (int64_t)sysconf(_SC_NPROCESSORS_ONLN);
}

char* env_home_dir(void) {
    const char* h = getenv("HOME");
    return strdup(h ? h : "");
}

char* env_temp_dir(void) {
    const char* t = getenv("TMPDIR");
    return strdup(t ? t : "/tmp");
}

void env_exit(int64_t code) {
    exit((int)code);
}

void env_abort(const char* msg) {
    fprintf(stderr, "abort: %s\n", msg);
    exit(1);
}

/* ======================== time ======================== */

int64_t time_now_ms(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (int64_t)ts.tv_sec * 1000 + (int64_t)ts.tv_nsec / 1000000;
}

int64_t time_now_nanos(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (int64_t)ts.tv_sec * 1000000000LL + (int64_t)ts.tv_nsec;
}

void time_sleep_ms(int64_t ms) {
    struct timespec ts;
    ts.tv_sec = ms / 1000;
    ts.tv_nsec = (ms % 1000) * 1000000;
    nanosleep(&ts, NULL);
}

int64_t time_unix_secs(void) {
    return (int64_t)time(NULL);
}

char* time_format_iso8601(int64_t unix_secs) {
    time_t t = (time_t)unix_secs;
    struct tm tm;
    gmtime_r(&t, &tm);
    char* buf = malloc(32);
    if (!buf) return strdup("");
    strftime(buf, 32, "%Y-%m-%dT%H:%M:%SZ", &tm);
    return buf;
}

/* ======================== path utilities ======================== */

char* path_join(const char* base, const char* child) {
    size_t bl = strlen(base);
    size_t cl = strlen(child);
    int need_sep = bl > 0 && base[bl - 1] != '/';
    char* r = malloc(bl + cl + 2);
    if (!r) return strdup("");
    memcpy(r, base, bl);
    if (need_sep) r[bl++] = '/';
    memcpy(r + bl, child, cl + 1);
    return r;
}

char* path_file_name(const char* p) {
    const char* s = strrchr(p, '/');
    return strdup(s ? s + 1 : p);
}

char* path_extension(const char* p) {
    const char* name = strrchr(p, '/');
    if (!name) name = p; else name++;
    const char* dot = strrchr(name, '.');
    return strdup(dot && dot != name ? dot + 1 : "");
}

char* path_parent(const char* p) {
    const char* s = strrchr(p, '/');
    if (!s || s == p) return strdup(s == p ? "/" : ".");
    return str_substr(p, 0, (int64_t)(s - p));
}

/* ---- Aliases for LLVM codegen name mangling ---- */
/* The LLVM emitter mangles some names differently from the C definitions */

/* Channel aliases (LLVM uses Channel_send, C has channel_send) */
void Channel_send(ManitChan* c, int64_t v) { channel_send(c, v); }
int64_t Channel_recv(ManitChan* c) { return channel_recv(c); }
ManitChan* Channel_new(void) { return channel_new(); }
ManitChan* Channel_bounded(int64_t capacity) { return channel_bounded(capacity); }
int64_t Channel_len(ManitChan* c) { return channel_len(c); }
int Channel_is_empty(ManitChan* c) { return channel_is_empty(c); }
void Channel_close(ManitChan* c) { channel_close(c); }
int Channel_is_closed(ManitChan* c) { return channel_is_closed(c); }
ManitChan* channel(void) { return channel_new(); }

/* Result aliases (LLVM uses Ok/Err/Unknown, C has Ok_new/Err_new/Unknown_new) */
ManitResult* Ok(int64_t v) { return Ok_new(v); }
ManitResult* Err(const char* m) { return Err_new(m); }
ManitResult* Unknown(const char* m) { return Unknown_new(m); }

/* String aliases */
char* int_to_str(int64_t n) { return str_from_int(n); }
int64_t str_to_int(const char* s) { return str_parse_int(s); }
char* str_slice(const char* s, int64_t start, int64_t end) {
    /* ManiT slice uses (start, end) indices, not (start, length) */
    int64_t len = end - start;
    if (len < 0) len = 0;
    return str_substr(s, start, len);
}

/* Math/ternary aliases */
char* math_to_balanced_ternary(int64_t n) { return ternary_to_balanced_ternary(n); }
int64_t math_from_balanced_ternary(const char* s) { return ternary_from_balanced_ternary(s); }

int8_t ternary_trit_median(int8_t a, int8_t b, int8_t c) {
    /* Median of three trits: sort and pick middle */
    int8_t arr[3] = {a, b, c};
    for (int i = 0; i < 2; i++)
        for (int j = i + 1; j < 3; j++)
            if (arr[j] < arr[i]) { int8_t t = arr[i]; arr[i] = arr[j]; arr[j] = t; }
    return arr[1];
}

int64_t ternary_t27_shift_left(int64_t val, int64_t n) {
    for (int64_t i = 0; i < n; i++) val *= 3;
    return val;
}

int64_t ternary_t27_shift_right(int64_t val, int64_t n) {
    for (int64_t i = 0; i < n; i++) {
        /* Balanced ternary division: round toward zero */
        if (val >= 0) val = val / 3;
        else val = -((-val) / 3);
    }
    return val;
}

/* TernaryTrie aliases */
int TernaryTrie_contains_key(ManitTrie* t, ManitVec* key) {
    return TernaryTrie_contains(t, key);
}

void TernaryTrie_remove(ManitTrie* t, ManitVec* key) {
    if (!t || !key) return;
    ManitTrieNode* n = t->root;
    int64_t len = Vec_len(key);
    for (int64_t i = 0; i < len && n; i++) {
        int idx = trit_to_idx(Vec_get(key, i));
        n = n->children[idx];
    }
    if (n && n->used) { n->used = 0; t->count--; }
}

ManitVec* TernaryTrie_keys(ManitTrie* t) {
    return TernaryTrie_keys_with_prefix(t, NULL);
}

/* ======================== directory listing ======================== */
/* Stateful single-directory listing for ManiT shells.
   Call fs_list_dir_open(path) to load entries, then fs_list_dir_entry(i)
   to retrieve each filename.  Not thread-safe (shell is single-threaded). */

static char**  g_dir_entries = NULL;
static int64_t g_dir_count   = 0;

int64_t fs_list_dir_open(const char* path) {
    /* Free previous listing */
    if (g_dir_entries) {
        for (int64_t i = 0; i < g_dir_count; i++) free(g_dir_entries[i]);
        free(g_dir_entries);
        g_dir_entries = NULL;
        g_dir_count   = 0;
    }
    DIR* dir = opendir(path);
    if (!dir) return 0;

    /* Count non-dot entries */
    int64_t count = 0;
    struct dirent* e;
    while ((e = readdir(dir)) != NULL) {
        if (e->d_name[0] == '.') continue;
        count++;
    }
    rewinddir(dir);

    g_dir_entries = (char**)malloc((size_t)count * sizeof(char*));
    if (!g_dir_entries) { closedir(dir); return 0; }

    int64_t idx = 0;
    while ((e = readdir(dir)) != NULL && idx < count) {
        if (e->d_name[0] == '.') continue;
        char* name = strdup(e->d_name);
        if (name) g_dir_entries[idx++] = name;
    }
    closedir(dir);
    g_dir_count = idx;

    /* Sort entries alphabetically */
    for (int64_t i = 0; i < g_dir_count - 1; i++) {
        for (int64_t j = i + 1; j < g_dir_count; j++) {
            if (strcmp(g_dir_entries[i], g_dir_entries[j]) > 0) {
                char* tmp = g_dir_entries[i];
                g_dir_entries[i] = g_dir_entries[j];
                g_dir_entries[j] = tmp;
            }
        }
    }
    return g_dir_count;
}

char* fs_list_dir_entry(int64_t idx) {
    if (!g_dir_entries || idx < 0 || idx >= g_dir_count) return strdup("");
    return strdup(g_dir_entries[idx]);
}

/* ======================== process spawn ======================== */
/* Fork + exec a compiled binary.  Waits for child to exit.
   Returns child exit code, or -1 on error. */

int64_t process_spawn(const char* path) {
    pid_t pid = fork();
    if (pid < 0) return -1;
    if (pid == 0) {
        /* child */
        execl(path, path, (char*)NULL);
        _exit(127);  /* exec failed */
    }
    /* parent — wait */
    int status = 0;
    if (waitpid(pid, &status, 0) < 0) return -1;
    if (WIFEXITED(status))   return (int64_t)WEXITSTATUS(status);
    if (WIFSIGNALED(status)) return (int64_t)(128 + WTERMSIG(status));
    return 0;
}

/* ======================== shell_exec ======================== */
/* Run a shell command via /bin/sh, capture stdout, return as string.
   Max 65535 bytes captured; caller owns the returned memory. */
char* shell_exec(const char* cmd) {
    FILE* f = popen(cmd, "r");
    if (!f) return strdup("");
    char   buf[65536];
    size_t n = fread(buf, 1, sizeof(buf) - 1, f);
    pclose(f);
    buf[n] = '\0';
    return strdup(buf);
}

/* ======================== fs copy / move (return status) ======================== */

int64_t fs_copy(const char* src, const char* dst) {
    FILE* in = fopen(src, "rb");
    if (!in) return 0;
    FILE* out = fopen(dst, "wb");
    if (!out) { fclose(in); return 0; }
    char buf[65536];
    size_t n;
    while ((n = fread(buf, 1, sizeof(buf), in)) > 0) {
        if (fwrite(buf, 1, n, out) != n) {
            fclose(in); fclose(out);
            return 0;
        }
    }
    int read_err = ferror(in);
    fclose(in);
    if (fclose(out) != 0 || read_err) return 0;
    return 1;
}

int64_t fs_move(const char* src, const char* dst) {
    if (rename(src, dst) == 0) return 1;
    /* rename may fail across filesystems — fall back to copy+delete */
    if (fs_copy(src, dst)) { remove(src); return 1; }
    return 0;
}

/* ======================== time / date ======================== */

char* env_timestamp(void) {
    time_t t = time(NULL);
    struct tm tm;
    localtime_r(&t, &tm);
    char* buf = (char*)malloc(32);
    if (!buf) return strdup("");
    strftime(buf, 32, "%Y-%m-%d %H:%M:%S", &tm);
    return buf;
}

char* env_date(void) {
    time_t t = time(NULL);
    struct tm tm;
    localtime_r(&t, &tm);
    char* buf = (char*)malloc(16);
    if (!buf) return strdup("");
    strftime(buf, 16, "%Y-%m-%d", &tm);
    return buf;
}

char* env_time(void) {
    time_t t = time(NULL);
    struct tm tm;
    localtime_r(&t, &tm);
    char* buf = (char*)malloc(12);
    if (!buf) return strdup("");
    strftime(buf, 12, "%H:%M:%S", &tm);
    return buf;
}

/* ======================== terminal control (TUI) ========================== */

static struct termios g_saved_termios;
static int            g_raw_mode = 0;

/* A15: a TUI that exits via env::exit, aborts, or is killed used to leave the
   terminal in raw mode — no echo, no line editing, no Ctrl-C. Restore on every
   ordinary exit path, and on the fatal signals, before re-raising. */
static void terminal_restore_atexit(void) {
    if (g_raw_mode) {
        tcsetattr(STDIN_FILENO, TCSAFLUSH, &g_saved_termios);
        g_raw_mode = 0;
    }
}

static void terminal_restore_on_signal(int sig) {
    terminal_restore_atexit();
    signal(sig, SIG_DFL);
    raise(sig);
}

int64_t terminal_set_raw(void) {
    struct termios raw;
    if (tcgetattr(STDIN_FILENO, &g_saved_termios) != 0) return -1;
    raw = g_saved_termios;
    raw.c_lflag &= ~(ECHO | ICANON | ISIG | IEXTEN);
    raw.c_iflag &= ~(IXON | ICRNL | BRKINT | INPCK | ISTRIP);
    raw.c_oflag &= ~OPOST;
    raw.c_cflag |= CS8;
    raw.c_cc[VMIN]  = 1;
    raw.c_cc[VTIME] = 0;
    if (tcsetattr(STDIN_FILENO, TCSAFLUSH, &raw) != 0) return -1;
    g_raw_mode = 1;
    static int handlers_installed = 0;
    if (!handlers_installed) {
        handlers_installed = 1;
        atexit(terminal_restore_atexit);
        signal(SIGINT,  terminal_restore_on_signal);
        signal(SIGTERM, terminal_restore_on_signal);
        signal(SIGSEGV, terminal_restore_on_signal);
        signal(SIGABRT, terminal_restore_on_signal);
        signal(SIGFPE,  terminal_restore_on_signal);
    }
    return 0;
}

int64_t terminal_set_cooked(void) {
    if (g_raw_mode) {
        tcsetattr(STDIN_FILENO, TCSAFLUSH, &g_saved_termios);
        g_raw_mode = 0;
    }
    return 0;
}

int64_t terminal_get_rows(void) {
    struct winsize ws;
    if (ioctl(STDOUT_FILENO, TIOCGWINSZ, &ws) == 0 && ws.ws_row > 0)
        return (int64_t)ws.ws_row;
    return 24;
}

int64_t terminal_get_cols(void) {
    struct winsize ws;
    if (ioctl(STDOUT_FILENO, TIOCGWINSZ, &ws) == 0 && ws.ws_col > 0)
        return (int64_t)ws.ws_col;
    return 80;
}

/* Read one raw byte (blocking). Returns 0-255. */
int64_t io_read_char(void) {
    unsigned char c;
    if (read(STDIN_FILENO, &c, 1) != 1) return -1;
    return (int64_t)c;
}

/*
 * Read a key, translating ESC sequences to special codes:
 *   1000 = Up       1001 = Down    1002 = Right   1003 = Left
 *   1004 = Page-Up  1005 = Page-Dn 1006 = Home    1007 = End
 *   1008 = Delete   1009 = Insert
 *   All other keys returned as their ASCII value (ESC alone = 27).
 */
int64_t io_read_key(void) {
    unsigned char c;
    if (read(STDIN_FILENO, &c, 1) != 1) return -1;
    if (c != 27) return (int64_t)c;          /* ordinary character */

    /* ESC — try to read the rest of the sequence with a short timeout */
    unsigned char seq[8];
    int n = 0;
    /* peek at next char; use non-blocking read via select with 50ms timeout */
    fd_set fds; struct timeval tv;
    FD_ZERO(&fds); FD_SET(STDIN_FILENO, &fds);
    tv.tv_sec = 0; tv.tv_usec = 50000;
    if (select(STDIN_FILENO + 1, &fds, NULL, NULL, &tv) <= 0) return 27; /* lone ESC */

    if (read(STDIN_FILENO, seq, 1) != 1) return 27;
    if (seq[0] != '[' && seq[0] != 'O') return 27;

    FD_ZERO(&fds); FD_SET(STDIN_FILENO, &fds);
    tv.tv_sec = 0; tv.tv_usec = 50000;
    if (select(STDIN_FILENO + 1, &fds, NULL, NULL, &tv) <= 0) return 27;
    if (read(STDIN_FILENO, seq + 1, 1) != 1) return 27;
    n = 2;

    if (seq[0] == '[') {
        if (seq[1] >= 'A' && seq[1] <= 'D') {
            switch (seq[1]) {
                case 'A': return 1000; /* up    */
                case 'B': return 1001; /* down  */
                case 'C': return 1002; /* right */
                case 'D': return 1003; /* left  */
            }
        }
        if (seq[1] >= '0' && seq[1] <= '9') {
            /* extended: ESC [ N ~ */
            unsigned char tilde;
            FD_ZERO(&fds); FD_SET(STDIN_FILENO, &fds);
            tv.tv_sec = 0; tv.tv_usec = 50000;
            if (select(STDIN_FILENO + 1, &fds, NULL, NULL, &tv) > 0)
                (void)!read(STDIN_FILENO, &tilde, 1);
            switch (seq[1]) {
                case '1': return 1006; /* Home   */
                case '2': return 1009; /* Insert */
                case '3': return 1008; /* Delete */
                case '4': return 1007; /* End    */
                case '5': return 1004; /* PgUp   */
                case '6': return 1005; /* PgDn   */
                case '7': return 1006; /* Home   */
                case '8': return 1007; /* End    */
            }
        }
        if (seq[1] == 'H') return 1006; /* Home */
        if (seq[1] == 'F') return 1007; /* End  */
    }
    if (seq[0] == 'O') {
        switch (seq[1]) {
            case 'A': return 1000;
            case 'B': return 1001;
            case 'C': return 1002;
            case 'D': return 1003;
            case 'H': return 1006;
            case 'F': return 1007;
        }
    }
    (void)n;
    return 27;
}

int64_t io_clear_screen(void) {
    (void)!write(STDOUT_FILENO, "\033[2J\033[H", 7);
    return 0;
}

/* row, col are 1-based */
int64_t io_move_cursor(int64_t row, int64_t col) {
    char buf[32];
    int len = snprintf(buf, sizeof(buf), "\033[%lld;%lldH",
                       (long long)row, (long long)col);
    if (len < 0) return 0;
    if (len >= (int)sizeof(buf)) len = (int)sizeof(buf) - 1;
    (void)!write(STDOUT_FILENO, buf, (size_t)len);
    return 0;
}

int64_t io_set_reverse(void) {
    (void)!write(STDOUT_FILENO, "\033[7m", 4);
    return 0;
}

int64_t io_reset_attr(void) {
    (void)!write(STDOUT_FILENO, "\033[0m", 4);
    return 0;
}

int64_t io_set_bold(void) {
    (void)!write(STDOUT_FILENO, "\033[1m", 4);
    return 0;
}
