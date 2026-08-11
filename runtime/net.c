/* net.c — HTTP client via libcurl.
 * Included by manit_runtime.c — do not compile independently.
 * Author: Manish Jagdish Thatte
 */

/* ======================== http (libcurl) ================================= */
#ifndef MANIT_NO_GUI

#include <curl/curl.h>

struct curl_resp_buf {
    char*  data;
    size_t len;
    size_t cap;
};

/* A14: cap the response body. The buffer used to double without limit, so the
   size was entirely under the remote server's control and an endless body would
   exhaust memory — reachable from thatteos userspace/browser.mt. */
#define NET_MAX_RESPONSE_BYTES ((size_t)64 * 1024 * 1024)

static size_t curl_write_cb(void* ptr, size_t size, size_t nmemb, void* ud) {
    struct curl_resp_buf* b = (struct curl_resp_buf*)ud;
    size_t bytes = size * nmemb;
    if (size != 0 && nmemb > (size_t)-1 / size) return 0;   /* overflow guard */
    if (b->len + bytes + 1 > NET_MAX_RESPONSE_BYTES) return 0;  /* abort transfer */
    while (b->len + bytes + 1 > b->cap) {
        size_t ncap = b->cap ? b->cap * 2 : 65536;
        char* nd = (char*)realloc(b->data, ncap);
        if (!nd) return 0;
        b->data = nd;
        b->cap = ncap;
    }
    memcpy(b->data + b->len, ptr, bytes);
    b->len += bytes;
    b->data[b->len] = '\0';
    return bytes;
}

/* A14: curl_easy_init performs lazy global initialisation, which libcurl
   documents as NOT thread-safe. ManiT has real pthread spawn, so two threads
   calling net::http_get concurrently was a documented race. Initialise once,
   under a mutex, before any easy handle exists. */
static pthread_mutex_t g_curl_init_lock = PTHREAD_MUTEX_INITIALIZER;
static int             g_curl_ready     = 0;

static void net_global_init_once(void) {
    pthread_mutex_lock(&g_curl_init_lock);
    if (!g_curl_ready) {
        curl_global_init(CURL_GLOBAL_DEFAULT);
        g_curl_ready = 1;
    }
    pthread_mutex_unlock(&g_curl_init_lock);
}

char* net_http_get(const char* url) {
    net_global_init_once();
    CURL* curl = curl_easy_init();
    if (!curl) return strdup("(curl init failed)");

    struct curl_resp_buf buf = {0};
    buf.data = (char*)malloc(65536);
    if (!buf.data) { curl_easy_cleanup(curl); return strdup("(out of memory)"); }
    buf.cap  = 65536;
    buf.data[0] = '\0';

    curl_easy_setopt(curl, CURLOPT_URL, url);
    curl_easy_setopt(curl, CURLOPT_WRITEFUNCTION, curl_write_cb);
    curl_easy_setopt(curl, CURLOPT_WRITEDATA, &buf);
    curl_easy_setopt(curl, CURLOPT_FOLLOWLOCATION, 1L);
    /* A14: FOLLOWLOCATION with libcurl's default (unlimited) redirect count let
       a redirect loop run until the 30 s timeout. */
    curl_easy_setopt(curl, CURLOPT_MAXREDIRS, 10L);
    curl_easy_setopt(curl, CURLOPT_TIMEOUT, 30L);
    curl_easy_setopt(curl, CURLOPT_USERAGENT, "thatteOS/0.3 browser.mt/1.0");
    curl_easy_setopt(curl, CURLOPT_SSL_VERIFYPEER, 1L);

    CURLcode res = curl_easy_perform(curl);
    curl_easy_cleanup(curl);

    if (res != CURLE_OK) {
        free(buf.data);
        char* err = (char*)malloc(128);
        if (!err) return strdup("(curl error)");
        snprintf(err, 128, "(curl error: %s)", curl_easy_strerror(res));
        return err;
    }
    return buf.data;
}

#endif /* !MANIT_NO_GUI */
