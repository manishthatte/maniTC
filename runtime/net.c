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

static size_t curl_write_cb(void* ptr, size_t size, size_t nmemb, void* ud) {
    struct curl_resp_buf* b = (struct curl_resp_buf*)ud;
    size_t bytes = size * nmemb;
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

char* net_http_get(const char* url) {
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
    curl_easy_setopt(curl, CURLOPT_TIMEOUT, 30L);
    curl_easy_setopt(curl, CURLOPT_USERAGENT, "THATTEOS/0.3 browser.mt/1.0");
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
