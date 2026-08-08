/* core.c — IO, fmt, math, str.
 * Included by manit_runtime.c — do not compile independently.
 * Author: Manish Jagdish Thatte
 */

/* ======================== IO ======================== */

void io_println(const char* s) { printf("%s\n", s); }
void io_print(const char* s) { printf("%s", s); }
void io_newline(void) { printf("\n"); }
void io_print_int(int64_t n) { printf("%ld", (long)n); }
void io_print_float(double f) { printf("%g", f); }
void io_print_char(int8_t c) { printf("%c", (char)c); }
void io_print_trit(int8_t t) {
    if (t > 0) printf("+");
    else if (t < 0) printf("-");
    else printf("0");
}
void io_print_bool3(int8_t t) {
    if (t > 0) printf("true");
    else if (t < 0) printf("false");
    else printf("unknown");
}
void io_print_tryte(int8_t t) { printf("%d", (int)t); }
void io_println_int(int64_t n) { printf("%ld\n", (long)n); }
void io_println_float(double f) { printf("%g\n", f); }

char* io_read_line(void) {
    char* buf = malloc(1024);
    if (!buf) return NULL;
    if (!fgets(buf, 1024, stdin)) { buf[0] = '\0'; return buf; }
    size_t len = strlen(buf);
    if (len > 0 && buf[len-1] == '\n') buf[len-1] = '\0';
    return buf;
}

int64_t io_read_int(void) {
    int64_t n = 0;
    scanf("%ld", (long*)&n);
    return n;
}

/* ======================== fmt ======================== */

char* fmt_int_to_str(int64_t n) {
    char* buf = malloc(32);
    if (!buf) return NULL;
    snprintf(buf, 32, "%ld", (long)n);
    return buf;
}

char* fmt_show_int(int64_t n) { return fmt_int_to_str(n); }

char* fmt_show_float(double f) {
    char* buf = malloc(32);
    if (!buf) return NULL;
    snprintf(buf, 32, "%g", f);
    return buf;
}

char* fmt_show_bool(int b) {
    return strdup(b ? "true" : "false");
}

char* fmt_show_trit(int8_t t) {
    if (t > 0) return strdup("+");
    if (t < 0) return strdup("-");
    return strdup("0");
}

char* fmt_show_bool3(int8_t t) {
    if (t > 0) return strdup("true");
    if (t < 0) return strdup("false");
    return strdup("unknown");
}

char* fmt_concat(const char* a, const char* b) {
    size_t la = strlen(a), lb = strlen(b);
    char* r = malloc(la + lb + 1);
    if (!r) return NULL;
    memcpy(r, a, la);
    memcpy(r + la, b, lb + 1);
    return r;
}

char* fmt_format(const char* fmt_str, ...) {
    char* out = malloc(4096);
    if (!out) return NULL;
    const char* p = fmt_str;
    char* q = out;
    char* end = out + 4095;
    va_list args;
    va_start(args, fmt_str);
    while (*p && q < end) {
        if (p[0] == '{' && p[1] == '}') {
            int64_t n = va_arg(args, int64_t);
            int written = snprintf(q, (size_t)(end - q), "%ld", (long)n);
            if (written > 0) q += written;
            p += 2;
        } else {
            *q++ = *p++;
        }
    }
    *q = '\0';
    va_end(args);
    return out;
}

char* fmt_pad_zeros(const char* s, int64_t width) {
    size_t slen = strlen(s);
    if ((int64_t)slen >= width) return strdup(s);
    size_t pad = (size_t)(width - (int64_t)slen);
    char* r = malloc(width + 1);
    if (!r) return NULL;
    memset(r, '0', pad);
    memcpy(r + pad, s, slen + 1);
    return r;
}

char* fmt_align_left(const char* s, int64_t width) {
    size_t slen = strlen(s);
    if ((int64_t)slen >= width) return strdup(s);
    char* r = malloc((size_t)width + 1);
    if (!r) return NULL;
    memcpy(r, s, slen);
    memset(r + slen, ' ', (size_t)(width - (int64_t)slen));
    r[width] = '\0';
    return r;
}

char* fmt_align_right(const char* s, int64_t width) {
    size_t slen = strlen(s);
    if ((int64_t)slen >= width) return strdup(s);
    size_t pad = (size_t)(width - (int64_t)slen);
    char* r = malloc((size_t)width + 1);
    if (!r) return NULL;
    memset(r, ' ', pad);
    memcpy(r + pad, s, slen + 1);
    return r;
}

char* fmt_to_upper(const char* s) {
    char* r = strdup(s);
    if (!r) return NULL;
    for (char* p = r; *p; p++)
        if (*p >= 'a' && *p <= 'z') *p -= 32;
    return r;
}

char* fmt_to_lower(const char* s) {
    char* r = strdup(s);
    if (!r) return NULL;
    for (char* p = r; *p; p++)
        if (*p >= 'A' && *p <= 'Z') *p += 32;
    return r;
}

/* ======================== math ======================== */

int64_t math_abs(int64_t n) { return n < 0 ? -n : n; }
double  math_abs_float(double f) { return fabs(f); }
double  math_sqrt(double x) { return sqrt(x); }
double  math_pow(double x, double y) { return pow(x, y); }
double  math_log(double x) { return log(x); }
double  math_log2(double x) { return log2(x); }
double  math_log3(double x) { return log(x) / log(3.0); }
double  math_floor(double x) { return floor(x); }
double  math_ceil(double x) { return ceil(x); }
double  math_round(double x) { return round(x); }
int64_t math_min(int64_t a, int64_t b) { return a < b ? a : b; }
int64_t math_max(int64_t a, int64_t b) { return a > b ? a : b; }
int64_t math_clamp(int64_t v, int64_t lo, int64_t hi) {
    return v < lo ? lo : (v > hi ? hi : v);
}
double  math_sin(double x) { return sin(x); }
double  math_cos(double x) { return cos(x); }
int64_t math_trit_count(int64_t n) {
    /* Count number of trits needed to represent n in balanced ternary.
       trit_count(0) = 1 (the single trit '0').
       Uses balanced ternary decomposition: digits are {-1, 0, +1}. */
    if (n == 0) return 1;
    int64_t count = 0;
    int64_t v = n < 0 ? -n : n;
    while (v) {
        int64_t rem = v % 3;
        if (rem == 2) { v = v / 3 + 1; } /* carry: 2 → -1 + carry */
        else { v = v / 3; }
        count++;
    }
    return count;
}

/* ======================== str ======================== */

int64_t str_len(const char* s) { return (int64_t)strlen(s); }
int8_t  str_char_at(const char* s, int64_t i) { return (int8_t)s[i]; }
char*   str_concat(const char* a, const char* b) { return fmt_concat(a, b); }

char* str_substr(const char* s, int64_t start, int64_t len) {
    if (len < 0) len = 0;
    char* r = malloc((size_t)len + 1);
    if (!r) return NULL;
    strncpy(r, s + start, (size_t)len);
    r[len] = '\0';
    return r;
}

int str_starts_with(const char* s, const char* prefix) {
    return strncmp(s, prefix, strlen(prefix)) == 0;
}

int str_ends_with(const char* s, const char* suffix) {
    size_t sl = strlen(s), pl = strlen(suffix);
    return sl >= pl && strcmp(s + sl - pl, suffix) == 0;
}

int str_contains(const char* s, const char* sub) {
    return strstr(s, sub) != NULL;
}

int64_t str_find(const char* s, const char* sub) {
    const char* p = strstr(s, sub);
    return p ? (int64_t)(p - s) : -1LL;
}

char* str_replace(const char* s, const char* from, const char* to) {
    size_t fl = strlen(from), tl = strlen(to);
    size_t bufsz = 4096;
    char* r = malloc(bufsz);
    if (!r) return NULL;
    const char* p = s;
    char* q = r;
    char* rend = r + bufsz - 1;
    while (*p && q < rend) {
        if (fl > 0 && strncmp(p, from, fl) == 0) {
            size_t space = (size_t)(rend - q);
            size_t copy = tl < space ? tl : space;
            memcpy(q, to, copy);
            q += copy;
            p += fl;
        } else {
            *q++ = *p++;
        }
    }
    *q = '\0';
    return r;
}

char* str_trim(const char* s) {
    while (*s == ' ' || *s == '\t' || *s == '\n' || *s == '\r') s++;
    size_t len = strlen(s);
    while (len > 0 && (s[len-1] == ' ' || s[len-1] == '\t' ||
                        s[len-1] == '\n' || s[len-1] == '\r')) len--;
    return str_substr(s, 0, (int64_t)len);
}

/* str_split_head — everything before the first occurrence of sep.
   Returns the whole string if sep is not found. */
char* str_split_head(const char* s, const char* sep) {
    const char* p = strstr(s, sep);
    if (!p) return str_substr(s, 0, (int64_t)strlen(s));
    return str_substr(s, 0, (int64_t)(p - s));
}

/* str_split_tail — everything after the first occurrence of sep.
   Returns empty string if sep is not found. */
char* str_split_tail(const char* s, const char* sep) {
    const char* p = strstr(s, sep);
    if (!p) return str_substr(s, 0, 0);
    size_t sep_len = strlen(sep);
    size_t tail_start = (size_t)(p - s) + sep_len;
    return str_substr(s, (int64_t)tail_start, (int64_t)(strlen(s) - tail_start));
}

int64_t str_parse_int(const char* s) { return (int64_t)atoll(s); }
double  str_parse_float(const char* s) { return atof(s); }
char*   str_from_int(int64_t n) { return fmt_int_to_str(n); }

char* str_from_char(int64_t c) {
    char* r = malloc(2);
    if (!r) return NULL;
    r[0] = (char)(c & 0x7F); r[1] = '\0';
    return r;
}

char* str_to_upper(const char* s) { return fmt_to_upper(s); }
char* str_to_lower(const char* s) { return fmt_to_lower(s); }
int   str_eq(const char* a, const char* b) { return strcmp(a, b) == 0; }
