/* core.c — IO, fmt, math, str.
 * Included by manit_runtime.c — do not compile independently.
 * Author: Manish Jagdish Thatte
 */

/* ======================== runtime faults ======================== */
/* A7/A2/E1: a single reporting path for runtime faults, so the LLVM backend
 * fails the way the T3 emulator already does (a named TRAP and a defined exit
 * status) instead of dying on a raw SIGFPE/SIGSEGV.
 *
 * stdout is flushed FIRST: it is block-buffered when redirected, so a program
 * killed by a signal lost everything it had printed, which made these faults
 * look like "the program produced no output at all".
 *
 * Exit status 70 matches T3_TRAP_EXIT on the T3 side. */
#define MANIT_TRAP_EXIT 70

/* (3^27 - 1) / 2 — the 27-trit word range.
 *
 * Defined here rather than beside the lane-wise helpers further down, because
 * the N5 guards below need it and they come first. There is ONE definition:
 * two would be two numbers to keep equal, and this file's whole subject is a
 * representation whose bounds are exact. */
#define MANIT_T3_MAX   3812798742493LL
#define MANIT_T3_MIN   (-3812798742493LL)

void manit_fault(const char* msg) {
    fflush(stdout);
    fprintf(stderr, "TRAP: %s\n", msg ? msg : "runtime fault");
    fflush(stderr);
    exit(MANIT_TRAP_EXIT);
}

/* Divisor guard: called immediately before an integer sdiv/srem. */
void manit_check_divisor(int64_t d) {
    if (d == 0) manit_fault("division by zero");
}

/* N5 (--lang v2): the 27-trit range guards.
 *
 * `int` is 27 trits on the T3 machine and was 64 bits here, so a value in
 * (3812798742493, 2^63-1] existed on one backend and not the other:
 * `let m: int = 3812798742493; m + 1` traps on T3 and answered 3812798742494
 * on LLVM. Under v2 `int` means 27 trits everywhere, which on a 64-bit machine
 * means checking. `trint` is the wider type for code that wants the machine
 * word, and is not checked.
 *
 * Called BEFORE the arithmetic, on the OPERANDS, which is the same shape as
 * manit_check_divisor above and is what makes the check exact: the true result
 * is computed here in __int128, so a product that overflows int64 is caught on
 * its real value rather than on a wrapped or saturated one. Doing it after the
 * fact in int64 would have missed exactly the multiplications that overflow
 * hardest.
 *
 * The cost is a call per `int` add/sub/mul, not a compare-and-branch: the LLVM
 * emitter builds one straight-line sequence per IR instruction and cannot open
 * a new basic block in the middle of one. It is the same cost the divisor
 * guard already pays on every integer division, and it is paid only by code
 * compiled with --lang v2. */
static void manit_t27_fault(const char* what, __int128 v) {
    /* No printf length modifier for __int128; the value is rendered by hand.
     * It is at most 39 digits plus a sign. */
    char digits[48];
    int n = 0;
    int neg = v < 0;
    unsigned __int128 m = neg ? (unsigned __int128)(-v) : (unsigned __int128)v;
    if (m == 0) digits[n++] = '0';
    while (m > 0) { digits[n++] = (char)('0' + (int)(m % 10)); m /= 10; }
    char num[52];
    int k = 0;
    if (neg) num[k++] = '-';
    while (n > 0) num[k++] = digits[--n];
    num[k] = '\0';

    char buf[192];
    snprintf(buf, sizeof(buf),
             "%s overflow: result %s is outside the 27-trit range "
             "[%lld, %lld]",
             what, num, (long long)MANIT_T3_MIN, (long long)MANIT_T3_MAX);
    manit_fault(buf);
}

void manit_check_t27_add(int64_t a, int64_t b) {
    __int128 r = (__int128)a + (__int128)b;
    if (r > MANIT_T3_MAX || r < MANIT_T3_MIN)
        manit_t27_fault("int addition", r);
}

void manit_check_t27_sub(int64_t a, int64_t b) {
    __int128 r = (__int128)a - (__int128)b;
    if (r > MANIT_T3_MAX || r < MANIT_T3_MIN)
        manit_t27_fault("int subtraction", r);
}

void manit_check_t27_mul(int64_t a, int64_t b) {
    __int128 r = (__int128)a * (__int128)b;
    if (r > MANIT_T3_MAX || r < MANIT_T3_MIN)
        manit_t27_fault("int multiplication", r);
}

/* Bounds guard: called before indexing a fixed-length array. */
void manit_check_index(int64_t idx, int64_t len) {
    if (idx < 0 || idx >= len) {
        char buf[128];
        snprintf(buf, sizeof(buf),
                 "index %lld is out of bounds for an array of length %lld",
                 (long long)idx, (long long)len);
        manit_fault(buf);
    }
}

/* Result guard: called by `.unwrap()` before word 1 is read.
 *
 * The tag is a trit — +1 Ok, 0 Unknown, -1 Err — so a Result has three
 * outcomes and `unwrap` names only one of them. The other two fault here.
 * The message must stay byte-identical to SYSCALL #561 in the T3 emulator
 * (src/codegen_t3/emulator/syscalls.rs): they are the one pair of hand-written
 * twins in the whole Result implementation, and a divergence between them is
 * exactly what the two-backend tests are watching for. */
void manit_check_result_ok(int64_t tag) {
    if (tag == 1) return;
    manit_fault(tag == 0
        ? "unwrap on a Result that is Unknown"
        : "unwrap on a Result that is Err");
}

/* ==================== float rendering (shortest round-trip) ====================
 *
 * The LLVM backend printed floats with "%g" and the T3 emulator with Rust's
 * `format!("{}", f)`. Those are not two spellings of one thing, they are two
 * different numbers on screen:
 *
 *      value              %g              Rust Display
 *      3.14159265358979   3.14159         3.14159265358979
 *      1234567.0          1.23457e+06     1234567
 *      1e300              1e+300          1 followed by 300 zeros
 *      2.0/3.0            0.666667        0.6666666666666666
 *
 * "%g" gives SIX significant figures and switches to scientific notation. Eleven
 * digits of a double were being silently dropped on one backend and kept on the
 * other, so no float-valued program could be cross-checked — which is precisely
 * the check the oracle exists to perform.
 *
 * T3 is the side that was right, so C moves to match it. Rust's Display for f64
 * is: the SHORTEST decimal string that reads back as the identical double,
 * rendered POSITIONALLY — never in scientific notation, and never padded to the
 * exact binary value. That last point matters: 1e300 prints as 1-and-300-zeros,
 * not as the 301-digit integer the double actually equals, which is what
 * "%.0f" would have produced.
 *
 * Two steps. Find the shortest precision that round-trips through strtod (glibc's
 * strtod is correctly rounded, so equality here is exact, not approximate), then
 * shift the decimal point by hand. Doing the second step with printf is not
 * possible: "%.*f" renders the true binary value, and "%.*e" keeps the exponent.
 */

#define MANIT_FLOAT_BUF 512   /* worst case: 0. + 323 zeros + 17 digits + sign + NUL */

/* Split a "%e" rendering — [-]D[.DDD]e(+|-)XX — into its digit string (no
 * decimal point), its decimal exponent, and its sign. Returns the digit count. */
static int manit_split_e(const char* s, char* digits, size_t cap, int* exp10, int* neg) {
    *neg = 0;
    if (*s == '-') { *neg = 1; s++; }
    int nd = 0;
    while (*s && *s != 'e' && *s != 'E') {
        if (*s >= '0' && *s <= '9' && nd + 1 < (int)cap) digits[nd++] = *s;
        s++;
    }
    digits[nd] = '\0';
    *exp10 = (*s == 'e' || *s == 'E') ? atoi(s + 1) : 0;
    return nd;
}

static int manit_shortest_prec(double f) {
    char probe[64];
    for (int p = 1; p <= 17; p++) {
        snprintf(probe, sizeof probe, "%.*e", p - 1, f);
        if (strtod(probe, NULL) == f) return p;
    }
    return 17;
}

/* Resolve the one case where glibc and Rust legitimately disagree.
 *
 * It is not a rounding *error* on either side, it is a tie-break convention. When
 * a double sits exactly halfway between two p-significant-digit decimals, glibc's
 * printf rounds the tie to EVEN and Rust's f64 Display rounds it AWAY FROM ZERO.
 * So 1059438285926254.25 prints as ...254.2 from C and ...254.3 from Rust, and
 * both read back as the identical double. It bites about 1 double in 4,000.
 *
 * Every double has a FINITE exact decimal expansion — at most 767 significant
 * digits, for the smallest subnormal — and glibc renders it exactly when asked
 * for enough. So a tie is decidable, not estimable: digit `nd` of the exact
 * expansion is '5' and every digit after it is '0'.
 *
 * On a tie the digits are re-derived by TRUNCATING the exact expansion and
 * incrementing. Rounding glibc's output a second time would double-round: for
 * 276804372109801.375 glibc has already carried 7 to 8, and incrementing that
 * again gives ...801.39 where both conventions agree on ...801.38.
 *
 * Returns 1 and rewrites `digits`/`exp10` on a tie, 0 otherwise.
 */
static int manit_tie_break(double f, int nd, char* digits, int* exp10) {
    char ex[1100], d[1100];
    int e10, neg;
    snprintf(ex, sizeof ex, "%.*e", 800, f);   /* 801 sig digits > any double needs */
    int exnd = manit_split_e(ex, d, sizeof d, &e10, &neg);
    if (nd >= exnd || d[nd] != '5') return 0;
    for (int i = nd + 1; i < exnd; i++) if (d[i] != '0') return 0;

    for (int i = 0; i < nd; i++) digits[i] = d[i];
    digits[nd] = '\0';
    int i = nd - 1;
    while (i >= 0) {
        if (digits[i] != '9') { digits[i]++; break; }
        digits[i] = '0';
        i--;
    }
    if (i < 0) {
        /* 9.99…95 carried past the top: the digits become 1 and the value gains
         * a decimal place. */
        digits[0] = '1';
        for (int k = 1; k < nd; k++) digits[k] = '0';
        e10 += 1;
    }
    *exp10 = e10;
    return 1;
}

/* Render `f` into `out` exactly as Rust's `format!("{}", f)` would. */
static void manit_format_float(double f, char* out, size_t n) {
    if (n == 0) return;
    if (isnan(f)) { snprintf(out, n, "NaN"); return; }
    if (isinf(f)) { snprintf(out, n, f < 0 ? "-inf" : "inf"); return; }
    /* Rust prints negative zero as "-0". Go through signbit, not a comparison:
     * -0.0 == 0.0 is true. */
    if (f == 0.0) { snprintf(out, n, signbit(f) ? "-0" : "0"); return; }

    int prec = manit_shortest_prec(f);
    char sci[64], digits[40];
    int exp10, neg;
    snprintf(sci, sizeof sci, "%.*e", prec - 1, f);
    int nd = manit_split_e(sci, digits, sizeof digits, &exp10, &neg);

    /* Adopt Rust's tie-break where the two conventions part company. */
    manit_tie_break(f, nd, digits, &exp10);

    /* `point` = how many digits belong before the decimal point. It may be <= 0
     * (leading "0.000…") or > nd (trailing zeros before the point). */
    int point = exp10 + 1;

    size_t w = 0;
    #define PUTC(ch) do { if (w + 1 < n) out[w] = (ch); w++; } while (0)
    if (neg) PUTC('-');
    if (point <= 0) {
        PUTC('0'); PUTC('.');
        for (int i = 0; i < -point; i++) PUTC('0');
        for (int i = 0; i < nd; i++) PUTC(digits[i]);
    } else if (point >= nd) {
        for (int i = 0; i < nd; i++) PUTC(digits[i]);
        for (int i = 0; i < point - nd; i++) PUTC('0');
    } else {
        for (int i = 0; i < point; i++) PUTC(digits[i]);
        PUTC('.');
        for (int i = point; i < nd; i++) PUTC(digits[i]);
    }
    #undef PUTC
    out[w < n ? w : n - 1] = '\0';
}

/* ======================== IO ======================== */

void io_println(const char* s) { printf("%s\n", s); }
void io_print(const char* s) { printf("%s", s); }
void io_newline(void) { printf("\n"); }
void io_print_int(int64_t n) { printf("%ld", (long)n); }
void io_print_float(double f) {
    char buf[MANIT_FLOAT_BUF];
    manit_format_float(f, buf, sizeof buf);
    printf("%s", buf);
}
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
void io_println_float(double f) {
    char buf[MANIT_FLOAT_BUF];
    manit_format_float(f, buf, sizeof buf);
    printf("%s\n", buf);
}

char* io_read_line(void) {
    char* buf = malloc(1024);
    if (!buf) return NULL;
    if (!fgets(buf, 1024, stdin)) { buf[0] = '\0'; return buf; }
    size_t len = strlen(buf);
    if (len > 0 && buf[len-1] == '\n') buf[len-1] = '\0';
    return buf;
}

int64_t io_read_int(void) {
    long long n = 0;
    if (scanf("%lld", &n) != 1) {
        int ch;
        while ((ch = getchar()) != EOF && ch != '\n') {}
        return 0;
    }
    return (int64_t)n;
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
    /* 32 bytes was never enough: 1e300 alone needs 302. */
    char* buf = malloc(MANIT_FLOAT_BUF);
    if (!buf) return NULL;
    manit_format_float(f, buf, MANIT_FLOAT_BUF);
    return buf;
}

char* fmt_show_bool(int b) {
    return strdup(b ? "true" : "false");
}

/* fmt_show_trit and fmt_show_bool3 were REMOVED on 20 August 2026. They now
 * live in stdlib/fmt.mt as three-armed `tif` expressions, which is one body for
 * both backends instead of a C function that only the LLVM target could reach —
 * fmt::show_trit had no T3 intercept at all, so the same source built on one
 * backend and not the other.
 *
 * Deleting the C rather than leaving it unused is the point. The ManiT
 * definition mangles to the same symbol, so keeping both is a duplicate
 * definition at link time; and an unused copy is exactly how fmt_align_left
 * came to disagree with T3 for months.
 *
 * fmt_show_bool3 also had the wrong strings. It returned "true"/"false"/
 * "unknown" while stdlib/fmt.mt, stdlib/str.mt and stdlib/io.mt all document
 * "True"/"Unknown"/"False". The ManiT implementation follows the documented
 * contract, so LLVM output changes case here — deliberately. */

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
            /* The language-level API is fmt::format(str, [str]): every
             * substitution argument is already a string (via fmt::show_*). */
            const char* s = va_arg(args, const char*);
            int avail = (int)(end - q);
            int written = snprintf(q, (size_t)avail, "%s", s ? s : "(null)");
            if (written > 0) q += written < avail ? written : avail - 1;
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

/* fmt_align_left / fmt_align_right were REMOVED on 20 August 2026, one day
 * after being repaired, because repairing them was treating the symptom.
 *
 * The bug: the `pad` parameter was missing from this file while stdlib/fmt.mt
 * declared `align_left(s, width, pad: char)` and the LLVM call site passed
 * three arguments. codegen_llvm/helpers.rs declared only two, clang accepted a
 * 3-argument call to a 2-parameter function, the pad char was dropped, and this
 * code hardcoded a space. T3 (syscalls #15/#132) honoured it all along, so
 * `align_left("ab", 5, '.')` printed "ab..." on T3 and "ab   " on LLVM. Nothing
 * caught it because native call arguments are never type-checked.
 *
 * The cause was not the missing parameter but the existence of two independent
 * implementations behind one name. Both are gone: fmt::align_left and
 * align_right are now one line of ManiT each over str::pad_right/pad_left, so
 * there is nothing left to drift. */

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

/* math_abs, math_min, math_max, math_clamp and math_pow were here until
 * 20 August 2026. They are ManiT source in stdlib/math.mt now, and the merged
 * bodies mangle to these exact symbols, so keeping the C copies was a
 * `multiple definition` at link.
 *
 * math_pow is the one worth pausing on: it was `double math_pow(double, double)`
 * while ManiT has always declared `pow(base: int, exp: int) -> int`. A declare
 * and a body both existed and the call still failed — the declared-vs-defined
 * class again. Deleting it retires the mismatch rather than repairing it.
 *
 * The float functions below stay native for now; they are the open half of the
 * module. */
/* math_sqrt/log/log2/log3/floor/ceil/round/sin/cos were defined here and
 * are gone. They were the only nine of math::'s 34 float functions that
 * existed at all on LLVM, and NONE of them existed on T3 — the T3 emulator
 * has no float-math syscalls, only arithmetic (212-215), comparison (216),
 * conversion (210-211) and load (219). So `math::sqrt` was an undefined
 * label on T3 while working fine on LLVM: the worst possible split, because
 * it looks correct on the backend most people build with.
 *
 * All 34 are now written in ManiT and shared by both backends from one body.
 * Keeping a C definition alongside the ManiT one would shadow it and
 * reintroduce exactly the divergence this closes. */
double  math_abs_float(double f) { return fabs(f); }
int64_t math_trit_count(int64_t n) {
    /* Count number of trits needed to represent n in balanced ternary.
       trit_count(0) = 1 (the single trit '0').
       Uses balanced ternary decomposition: digits are {-1, 0, +1}. */
    if (n == 0) return 1;
    int64_t count = 0;
    uint64_t v = n < 0 ? 0 - (uint64_t)n : (uint64_t)n;
    while (v) {
        uint64_t rem = v % 3;
        if (rem == 2) { v = v / 3 + 1; } /* carry: 2 → -1 + carry */
        else { v = v / 3; }
        count++;
    }
    return count;
}

/* ======================== str ======================== */

int64_t str_len(const char* s) { return (int64_t)strlen(s); }
int8_t  str_char_at(const char* s, int64_t i) {
    if (!s || i < 0 || i >= (int64_t)strlen(s)) return 0;
    return (int8_t)s[i];
}
char*   str_concat(const char* a, const char* b) { return fmt_concat(a, b); }

/* str_substr: superseded 19 August 2026 — now ManiT source in stdlib/str.mt,
 * so both backends share one definition. The C copy had to go: the merged
 * ManiT function mangles to the same symbol and the linker rejected it. */

/* str_starts_with: superseded 19 August 2026 — now ManiT source in stdlib/str.mt,
 * so both backends share one definition. The C copy had to go: the merged
 * ManiT function mangles to the same symbol and the linker rejected it. */

/* str_ends_with: superseded 19 August 2026 — now ManiT source in stdlib/str.mt,
 * so both backends share one definition. The C copy had to go: the merged
 * ManiT function mangles to the same symbol and the linker rejected it. */

int str_contains(const char* s, const char* sub) {
    return strstr(s, sub) != NULL;
}

int64_t str_find(const char* s, const char* sub) {
    const char* p = strstr(s, sub);
    return p ? (int64_t)(p - s) : -1LL;
}

char* str_replace(const char* s, const char* from, const char* to) {
    size_t fl = strlen(from), tl = strlen(to);
    if (fl == 0) return strdup(s);
    size_t count = 0;
    for (const char* p = strstr(s, from); p; p = strstr(p + fl, from)) count++;
    size_t sl = strlen(s);
    char* r = malloc(sl - count * fl + count * tl + 1);
    if (!r) return NULL;
    const char* p = s;
    char* q = r;
    while (*p) {
        if (strncmp(p, from, fl) == 0) {
            memcpy(q, to, tl);
            q += tl;
            p += fl;
        } else {
            *q++ = *p++;
        }
    }
    *q = '\0';
    return r;
}

/* Internal substring helper.
 *
 * This was `str_substr` until 19 August 2026, when str::substr became ManiT
 * source. The exported symbol had to go — the merged ManiT function mangles to
 * the same name — but str_slice, str_trim and path_parent are C primitives that
 * the ManiT layer is built ON, so they cannot call back into it. Hence an
 * internal name: same code, no exported symbol to collide.
 */
char* manit_substr(const char* s, int64_t start, int64_t len) {
    if (!s) return NULL;
    int64_t sl = (int64_t)strlen(s);
    if (start < 0) start = 0;
    if (start > sl) start = sl;
    if (len < 0) len = 0;
    if (start + len > sl) len = sl - start;
    char* r = malloc((size_t)len + 1);
    if (!r) return NULL;
    memcpy(r, s + start, (size_t)len);
    r[len] = '\0';
    return r;
}

char* str_trim(const char* s) {
    while (*s == ' ' || *s == '\t' || *s == '\n' || *s == '\r') s++;
    size_t len = strlen(s);
    while (len > 0 && (s[len-1] == ' ' || s[len-1] == '\t' ||
                        s[len-1] == '\n' || s[len-1] == '\r')) len--;
    return manit_substr(s, 0, (int64_t)len);
}

/* str_split_head — everything before the first occurrence of sep.
   Returns the whole string if sep is not found. */
/* str_split_head: superseded 19 August 2026 — now ManiT source in stdlib/str.mt,
 * so both backends share one definition. The C copy had to go: the merged
 * ManiT function mangles to the same symbol and the linker rejected it. */

/* str_split_tail — everything after the first occurrence of sep.
   Returns empty string if sep is not found. */
/* str_split_tail: superseded 19 August 2026 — now ManiT source in stdlib/str.mt,
 * so both backends share one definition. The C copy had to go: the merged
 * ManiT function mangles to the same symbol and the linker rejected it. */

/* str_parse_int: superseded 19 August 2026 — now ManiT source in stdlib/str.mt,
 * so both backends share one definition. The C copy had to go: the merged
 * ManiT function mangles to the same symbol and the linker rejected it. */

/* str_to_upper / str_to_lower: superseded 19 August 2026 — now ManiT source in
 * stdlib/str.mt, built on str_char_at + str_from_char, so both backends share
 * one definition. (fmt_to_upper / fmt_to_lower stay: they serve the fmt
 * module and are what these two used to wrap.) */

/* str_from_char(c) — build a one-character string.
 *
 * Added 19 August 2026. It was DECLARED in codegen_llvm/helpers.rs but defined
 * nowhere, which was a latent link error of the same class as str_from_int:
 * harmless only for as long as nothing called it. It is now a load-bearing
 * primitive — every char-dependent str:: function is written in ManiT on top
 * of this and str_char_at. */
char* str_from_char(int64_t c) {
    char* out = (char*)malloc(2);
    if (!out) return NULL;
    out[0] = (char)(c & 0xFF);
    out[1] = '\0';
    return out;
}
int   str_eq(const char* a, const char* b) { return strcmp(a, b) == 0; }

/* ------------------------------------------------------------------------
 * T3ISA v1.5 — lane-wise ternary logic (recommendation C2)
 * ------------------------------------------------------------------------
 *
 * On T3ISA each of these is ONE instruction (TANDW, TORW, TXORW, TIMPW,
 * TCMPW, TPOPC). On a binary machine there is no representation of a
 * balanced trit to extract with a mask, so each is a loop over 27 digits.
 * That gap is the performance argument for the ISA rather than a defect in
 * this file: the binary target pays what binary hardware has to pay.
 *
 * Every lane result is in {-1, 0, +1} by construction, so the reassembled
 * word is always in range and none of these can overflow.
 */

#define MANIT_T3_LANES 27
/* MANIT_T3_MAX / MANIT_T3_MIN are defined at the top of this file, beside the
 * N5 guards that also need them. */

static int64_t manit_clamp27(int64_t v) {
    if (v > MANIT_T3_MAX) return MANIT_T3_MAX;
    if (v < MANIT_T3_MIN) return MANIT_T3_MIN;
    return v;
}

/* Split a word into balanced-ternary trits, least significant first.
 *
 * Floor division throughout. C's `/` truncates toward zero, which disagrees
 * with a non-negative remainder for negative operands and silently yields the
 * wrong digits — the Rust side had exactly this defect and -8 decomposed to
 * +4. The digit and the carry must come from the SAME division. */
static void manit_trits27(int64_t v, signed char* out) {
    int64_t n = manit_clamp27(v);
    for (int i = 0; i < MANIT_T3_LANES; i++) {
        int64_t r = n % 3;
        int64_t q = n / 3;
        if (r < 0) { r += 3; q -= 1; }   /* make it a floor division */
        if (r == 2) { r = -1; q += 1; }  /* re-centre: balanced, not base-3 */
        out[i] = (signed char)r;
        n = q;
    }
}

static int64_t manit_from_trits27(const signed char* lanes) {
    int64_t v = 0;
    for (int i = MANIT_T3_LANES - 1; i >= 0; i--) {
        v = v * 3 + lanes[i];
    }
    return manit_clamp27(v);
}

static int64_t manit_lanewise2(int64_t a, int64_t b,
                               signed char (*f)(signed char, signed char)) {
    signed char la[MANIT_T3_LANES], lb[MANIT_T3_LANES], out[MANIT_T3_LANES];
    manit_trits27(a, la);
    manit_trits27(b, lb);
    for (int i = 0; i < MANIT_T3_LANES; i++) out[i] = f(la[i], lb[i]);
    return manit_from_trits27(out);
}

static signed char manit_trit_min(signed char a, signed char b) { return a < b ? a : b; }
static signed char manit_trit_max(signed char a, signed char b) { return a > b ? a : b; }

/* Balanced sum mod 3. Not an involution: 3k = 0 (mod 3), so three
 * applications are needed to recover the original, not two. */
static signed char manit_trit_xor(signed char a, signed char b) {
    int s = (a + b) % 3;
    if (s < 0) s += 3;
    return (signed char)(s == 2 ? -1 : s);
}

/* Lukasiewicz implication, min(+1, 1 - a + b). The a = b = 0 cell gives +1
 * where Kleene's max(-a, b) gives 0, and that single cell is what makes the
 * logic L3 rather than K3. */
static signed char manit_trit_imp(signed char a, signed char b) {
    int v = 1 - a + b;
    return (signed char)(v > 1 ? 1 : v);
}

static signed char manit_trit_cmp(signed char a, signed char b) {
    return (signed char)(a > b ? 1 : (a < b ? -1 : 0));
}

int64_t manit_lane_and(int64_t a, int64_t b) { return manit_lanewise2(a, b, manit_trit_min); }
int64_t manit_lane_or(int64_t a, int64_t b)  { return manit_lanewise2(a, b, manit_trit_max); }
int64_t manit_lane_xor(int64_t a, int64_t b) { return manit_lanewise2(a, b, manit_trit_xor); }
int64_t manit_lane_imp(int64_t a, int64_t b) { return manit_lanewise2(a, b, manit_trit_imp); }
int64_t manit_lane_cmp(int64_t a, int64_t b) { return manit_lanewise2(a, b, manit_trit_cmp); }

/* Count the lanes of `x` equal to the trit `k`.
 *
 * `k` is clamped into {-1, 0, +1}: a "count of lanes equal to 7" has no
 * meaning, and silently answering zero would hide the mistake rather than
 * report it. */
int64_t manit_lane_popcount(int64_t x, int64_t k) {
    signed char lanes[MANIT_T3_LANES];
    signed char want = (signed char)(k > 1 ? 1 : (k < -1 ? -1 : k));
    int64_t n = 0;
    manit_trits27(x, lanes);
    for (int i = 0; i < MANIT_T3_LANES; i++) if (lanes[i] == want) n++;
    return n;
}
