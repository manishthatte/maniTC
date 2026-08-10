/* gui.c — SDL2 GUI wrapper (full build) and stubs (MANIT_NO_GUI build).
 * Included by manit_runtime.c — do not compile independently.
 * Author: Manish Jagdish Thatte
 */

#ifndef MANIT_NO_GUI

#include <SDL2/SDL.h>
#include <SDL2/SDL_ttf.h>

/* ======================== SDL2 GUI wrapper ================================ */

static SDL_Window*   g_sdl_window   = NULL;
static SDL_Renderer* g_sdl_renderer = NULL;
static TTF_Font*     g_sdl_font     = NULL;
static TTF_Font*     g_sdl_font_lg  = NULL;  /* larger font for headings */

/* Last event state */
static int64_t g_gui_event_type    = 0;  /* 0=none 1=quit 2=keydown 3=mousemove 4=mbdown 5=mbup 6=textinput 7=mousewheel */
static int64_t g_gui_event_key     = 0;
static int64_t g_gui_mouse_x       = 0;
static int64_t g_gui_mouse_y       = 0;
static int64_t g_gui_mouse_btn     = 0;
static int64_t g_gui_text_char     = 0;
static int64_t g_gui_wheel_dy      = 0;
static char    g_gui_text_buf[32]  = {0};

static void gui_process_event(SDL_Event* ev) {
    switch (ev->type) {
    case SDL_QUIT:
        g_gui_event_type = 1;
        break;
    case SDL_KEYDOWN:
        g_gui_event_type = 2;
        g_gui_event_key  = (int64_t)ev->key.keysym.sym;
        break;
    case SDL_MOUSEMOTION:
        g_gui_event_type = 3;
        g_gui_mouse_x    = ev->motion.x;
        g_gui_mouse_y    = ev->motion.y;
        break;
    case SDL_MOUSEBUTTONDOWN:
        g_gui_event_type = 4;
        g_gui_mouse_x    = ev->button.x;
        g_gui_mouse_y    = ev->button.y;
        g_gui_mouse_btn  = ev->button.button;  /* 1=left 2=mid 3=right */
        break;
    case SDL_MOUSEBUTTONUP:
        g_gui_event_type = 5;
        g_gui_mouse_x    = ev->button.x;
        g_gui_mouse_y    = ev->button.y;
        g_gui_mouse_btn  = ev->button.button;
        break;
    case SDL_TEXTINPUT:
        g_gui_event_type  = 6;
        g_gui_text_char   = (int64_t)(unsigned char)ev->text.text[0];
        strncpy(g_gui_text_buf, ev->text.text, sizeof(g_gui_text_buf) - 1);
        g_gui_text_buf[sizeof(g_gui_text_buf) - 1] = '\0';
        break;
    case SDL_MOUSEWHEEL:
        g_gui_event_type = 7;
        g_gui_wheel_dy   = (int64_t)ev->wheel.y;   /* positive = scroll up */
        break;
    default:
        /* A11: leave the previous event state alone — see gui_poll_event. */
        g_gui_event_type = 0;
        break;
    }
}

/* True for SDL events gui_process_event actually maps to a ManiT event type.
   A11: gui_poll_event used to return 1 for *any* event while reporting type 0,
   so a window event (this window is resizable, so they are frequent) clobbered
   a real keypress or click that had not been read yet. */
static int gui_event_is_mapped(const SDL_Event* ev) {
    switch (ev->type) {
    case SDL_QUIT: case SDL_KEYDOWN: case SDL_MOUSEMOTION:
    case SDL_MOUSEBUTTONDOWN: case SDL_MOUSEBUTTONUP:
    case SDL_TEXTINPUT: case SDL_MOUSEWHEEL:
        return 1;
    default:
        return 0;
    }
}

/* Return the text-input string from the last SDL_TEXTINPUT event */
char* gui_event_text_str(void) { return strdup(g_gui_text_buf); }
int64_t gui_wheel_dy(void)     { return g_gui_wheel_dy; }

int64_t gui_init(int64_t width, int64_t height, const char* title) {
    if (SDL_Init(SDL_INIT_VIDEO | SDL_INIT_EVENTS) != 0) return -1;
    if (TTF_Init() != 0) { SDL_Quit(); return -2; }

    g_sdl_window = SDL_CreateWindow(
        title,
        SDL_WINDOWPOS_CENTERED, SDL_WINDOWPOS_CENTERED,
        (int)width, (int)height,
        SDL_WINDOW_SHOWN | SDL_WINDOW_RESIZABLE);
    if (!g_sdl_window) { TTF_Quit(); SDL_Quit(); return -3; }

    g_sdl_renderer = SDL_CreateRenderer(
        g_sdl_window, -1,
        SDL_RENDERER_ACCELERATED | SDL_RENDERER_PRESENTVSYNC);
    if (!g_sdl_renderer) {
        SDL_DestroyWindow(g_sdl_window); TTF_Quit(); SDL_Quit(); return -4;
    }
    SDL_SetRenderDrawBlendMode(g_sdl_renderer, SDL_BLENDMODE_BLEND);

    /* find a monospace font */
    const char* font_paths[] = {
        "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationMono-Regular.ttf",
        "/usr/share/fonts/truetype/freefont/FreeMono.ttf",
        "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
        "/usr/share/fonts/dejavu/DejaVuSansMono.ttf",
        NULL
    };
    for (int i = 0; font_paths[i]; i++) {
        g_sdl_font = TTF_OpenFont(font_paths[i], 15);
        if (g_sdl_font) {
            g_sdl_font_lg = TTF_OpenFont(font_paths[i], 18);
            break;
        }
    }
    if (!g_sdl_font) {
        /* fallback: try any system font */
        g_sdl_font = TTF_OpenFont("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", 15);
        if (g_sdl_font)
            g_sdl_font_lg = TTF_OpenFont("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", 18);
    }

    SDL_StartTextInput();
    /* A10: returning 0 here with g_sdl_font == NULL produced a window that
       opened, painted its rectangles and showed no text at all, with no error
       anywhere — the first-run experience on any system with SDL2 but no
       DejaVu/Liberation fonts. Report it distinctly instead. */
    if (!g_sdl_font) {
        fprintf(stderr, "manit: gui_init: no usable font found "
                        "(install fonts-dejavu-core or fonts-liberation)\n");
        return -5;
    }
    return 0;
}

int64_t gui_quit(void) {
    SDL_StopTextInput();
    if (g_sdl_font)    { TTF_CloseFont(g_sdl_font);    g_sdl_font = NULL; }
    if (g_sdl_font_lg) { TTF_CloseFont(g_sdl_font_lg); g_sdl_font_lg = NULL; }
    if (g_sdl_renderer){ SDL_DestroyRenderer(g_sdl_renderer); g_sdl_renderer = NULL; }
    if (g_sdl_window)  { SDL_DestroyWindow(g_sdl_window);     g_sdl_window = NULL; }
    TTF_Quit();
    SDL_Quit();
    return 0;
}

int64_t gui_clear(void) {
    SDL_RenderClear(g_sdl_renderer);
    return 0;
}

int64_t gui_present(void) {
    SDL_RenderPresent(g_sdl_renderer);
    return 0;
}

int64_t gui_set_color(int64_t r, int64_t g, int64_t b, int64_t a) {
    SDL_SetRenderDrawColor(g_sdl_renderer,
        (Uint8)r, (Uint8)g, (Uint8)b, (Uint8)a);
    return 0;
}

int64_t gui_fill_rect(int64_t x, int64_t y, int64_t w, int64_t h) {
    SDL_Rect rect = {(int)x, (int)y, (int)w, (int)h};
    SDL_RenderFillRect(g_sdl_renderer, &rect);
    return 0;
}

int64_t gui_draw_rect(int64_t x, int64_t y, int64_t w, int64_t h) {
    SDL_Rect rect = {(int)x, (int)y, (int)w, (int)h};
    SDL_RenderDrawRect(g_sdl_renderer, &rect);
    return 0;
}

int64_t gui_draw_line(int64_t x1, int64_t y1, int64_t x2, int64_t y2) {
    SDL_RenderDrawLine(g_sdl_renderer,
        (int)x1, (int)y1, (int)x2, (int)y2);
    return 0;
}

int64_t gui_draw_text(const char* text, int64_t x, int64_t y) {
    if (!g_sdl_font || !text || !*text) return 0;
    Uint8 r, g, b, a;
    SDL_GetRenderDrawColor(g_sdl_renderer, &r, &g, &b, &a);
    SDL_Color col = {r, g, b, a};
    SDL_Surface* surf = TTF_RenderUTF8_Blended(g_sdl_font, text, col);
    if (!surf) return -1;
    SDL_Texture* tex = SDL_CreateTextureFromSurface(g_sdl_renderer, surf);
    if (tex) {
        SDL_Rect dst = {(int)x, (int)y, surf->w, surf->h};
        SDL_RenderCopy(g_sdl_renderer, tex, NULL, &dst);
        SDL_DestroyTexture(tex);
    }
    SDL_FreeSurface(surf);
    return 0;
}

int64_t gui_draw_text_lg(const char* text, int64_t x, int64_t y) {
    TTF_Font* f = g_sdl_font_lg ? g_sdl_font_lg : g_sdl_font;
    if (!f || !text || !*text) return 0;
    Uint8 r, g, b, a;
    SDL_GetRenderDrawColor(g_sdl_renderer, &r, &g, &b, &a);
    SDL_Color col = {r, g, b, a};
    SDL_Surface* surf = TTF_RenderUTF8_Blended(f, text, col);
    if (!surf) return -1;
    SDL_Texture* tex = SDL_CreateTextureFromSurface(g_sdl_renderer, surf);
    if (tex) {
        SDL_Rect dst = {(int)x, (int)y, surf->w, surf->h};
        SDL_RenderCopy(g_sdl_renderer, tex, NULL, &dst);
        SDL_DestroyTexture(tex);
    }
    SDL_FreeSurface(surf);
    return 0;
}

int64_t gui_text_width(const char* text) {
    if (!g_sdl_font || !text) return 0;
    int w = 0, h = 0;
    TTF_SizeUTF8(g_sdl_font, text, &w, &h);
    return (int64_t)w;
}

int64_t gui_font_height(void) {
    if (!g_sdl_font) return 16;
    return (int64_t)TTF_FontHeight(g_sdl_font);
}

int64_t gui_window_width(void) {
    if (!g_sdl_window) return 800;
    int w = 800, h = 600;
    SDL_GetWindowSize(g_sdl_window, &w, &h);
    return (int64_t)w;
}

int64_t gui_window_height(void) {
    if (!g_sdl_window) return 600;
    int w = 800, h = 600;
    SDL_GetWindowSize(g_sdl_window, &w, &h);
    return (int64_t)h;
}

int64_t gui_poll_event(void) {
    SDL_Event ev;
    /* Drain unmapped events rather than surfacing them as type 0 (A11). */
    while (SDL_PollEvent(&ev)) {
        if (gui_event_is_mapped(&ev)) { gui_process_event(&ev); return 1; }
    }
    g_gui_event_type = 0;
    return 0;
}

int64_t gui_wait_event(int64_t timeout_ms) {
    SDL_Event ev;
    while (SDL_WaitEventTimeout(&ev, (int)timeout_ms)) {
        if (gui_event_is_mapped(&ev)) { gui_process_event(&ev); return 1; }
    }
    g_gui_event_type = 0;
    return 0;
}

int64_t gui_event_type(void)     { return g_gui_event_type; }
int64_t gui_event_key(void)      { return g_gui_event_key; }
int64_t gui_mouse_x(void)        { return g_gui_mouse_x; }
int64_t gui_mouse_y(void)        { return g_gui_mouse_y; }
int64_t gui_mouse_btn(void)      { return g_gui_mouse_btn; }
int64_t gui_event_text_char(void){ return g_gui_text_char; }
int64_t gui_ticks(void)          { return (int64_t)SDL_GetTicks(); }
int64_t gui_delay(int64_t ms)    { SDL_Delay((Uint32)ms); return 0; }

/* SDL2 key symbol constants (SDLK_* values) */
int64_t gui_key_return(void)    { return SDLK_RETURN; }
int64_t gui_key_escape(void)    { return SDLK_ESCAPE; }
int64_t gui_key_backspace(void) { return SDLK_BACKSPACE; }
int64_t gui_key_delete(void)    { return SDLK_DELETE; }
int64_t gui_key_up(void)        { return SDLK_UP; }
int64_t gui_key_down(void)      { return SDLK_DOWN; }
int64_t gui_key_left(void)      { return SDLK_LEFT; }
int64_t gui_key_right(void)     { return SDLK_RIGHT; }
int64_t gui_key_home(void)      { return SDLK_HOME; }
int64_t gui_key_end(void)       { return SDLK_END; }
int64_t gui_key_pageup(void)    { return SDLK_PAGEUP; }
int64_t gui_key_pagedown(void)  { return SDLK_PAGEDOWN; }
int64_t gui_key_enter(void)     { return SDLK_RETURN; }
int64_t gui_key_pgup(void)      { return SDLK_PAGEUP; }
int64_t gui_key_pgdn(void)      { return SDLK_PAGEDOWN; }
int64_t gui_key_f1(void)        { return SDLK_F1; }
int64_t gui_key_f2(void)        { return SDLK_F2; }
int64_t gui_key_f3(void)        { return SDLK_F3; }
int64_t gui_key_f4(void)        { return SDLK_F4; }
int64_t gui_key_f5(void)        { return SDLK_F5; }
int64_t gui_key_f6(void)        { return SDLK_F6; }
int64_t gui_key_f7(void)        { return SDLK_F7; }
int64_t gui_key_f8(void)        { return SDLK_F8; }
int64_t gui_key_f9(void)        { return SDLK_F9; }
int64_t gui_key_f10(void)       { return SDLK_F10; }
int64_t gui_key_f11(void)       { return SDLK_F11; }
int64_t gui_key_f12(void)       { return SDLK_F12; }
int64_t gui_key_tab(void)       { return SDLK_TAB; }
int64_t gui_key_space(void)     { return SDLK_SPACE; }
/* Letter keys (SDLK_a–z = ASCII 97–122) */
int64_t gui_key_a(void) { return SDLK_a; }
int64_t gui_key_b(void) { return SDLK_b; }
int64_t gui_key_c(void) { return SDLK_c; }
int64_t gui_key_d(void) { return SDLK_d; }
int64_t gui_key_e(void) { return SDLK_e; }
int64_t gui_key_f(void) { return SDLK_f; }
int64_t gui_key_g(void) { return SDLK_g; }
int64_t gui_key_h(void) { return SDLK_h; }
int64_t gui_key_i(void) { return SDLK_i; }
int64_t gui_key_j(void) { return SDLK_j; }
int64_t gui_key_k(void) { return SDLK_k; }
int64_t gui_key_l(void) { return SDLK_l; }
int64_t gui_key_m(void) { return SDLK_m; }
int64_t gui_key_n(void) { return SDLK_n; }
int64_t gui_key_o(void) { return SDLK_o; }
int64_t gui_key_p(void) { return SDLK_p; }
int64_t gui_key_q(void) { return SDLK_q; }
int64_t gui_key_r(void) { return SDLK_r; }
int64_t gui_key_s(void) { return SDLK_s; }
int64_t gui_key_t(void) { return SDLK_t; }
int64_t gui_key_u(void) { return SDLK_u; }
int64_t gui_key_v(void) { return SDLK_v; }
int64_t gui_key_w(void) { return SDLK_w; }
int64_t gui_key_x(void) { return SDLK_x; }
int64_t gui_key_y(void) { return SDLK_y; }
int64_t gui_key_z(void) { return SDLK_z; }
int64_t gui_key_mod_ctrl(void)  { return (SDL_GetModState() & KMOD_CTRL)  ? 1 : 0; }
int64_t gui_key_mod_shift(void) { return (SDL_GetModState() & KMOD_SHIFT) ? 1 : 0; }
int64_t gui_key_mod_alt(void)   { return (SDL_GetModState() & KMOD_ALT)   ? 1 : 0; }

char*   gui_clipboard_get(void) {
    char* t = SDL_GetClipboardText();
    if (!t) return strdup("");
    char* copy = strdup(t);
    SDL_free(t);
    return copy;
}
void    gui_clipboard_set(const char* text) { SDL_SetClipboardText(text); }

#else  /* MANIT_NO_GUI — stub net + gui symbols */

char*   net_http_get(const char* url) { (void)url; return strdup("(no network support)"); }
int64_t gui_init(int64_t w, int64_t h, const char* t) { (void)w;(void)h;(void)t; return -1; }
int64_t gui_quit(void)          { return 0; }
int64_t gui_clear(void)         { return 0; }
int64_t gui_present(void)       { return 0; }
int64_t gui_set_color(int64_t r,int64_t g,int64_t b,int64_t a) { (void)r;(void)g;(void)b;(void)a; return 0; }
int64_t gui_fill_rect(int64_t x,int64_t y,int64_t w,int64_t h) { (void)x;(void)y;(void)w;(void)h; return 0; }
int64_t gui_draw_rect(int64_t x,int64_t y,int64_t w,int64_t h) { (void)x;(void)y;(void)w;(void)h; return 0; }
int64_t gui_draw_line(int64_t x1,int64_t y1,int64_t x2,int64_t y2) { (void)x1;(void)y1;(void)x2;(void)y2; return 0; }
int64_t gui_draw_text(const char* s,int64_t x,int64_t y) { (void)s;(void)x;(void)y; return 0; }
int64_t gui_draw_text_lg(const char* s,int64_t x,int64_t y) { (void)s;(void)x;(void)y; return 0; }
int64_t gui_text_width(const char* s) { (void)s; return 0; }
int64_t gui_font_height(void)   { return 16; }
int64_t gui_window_width(void)  { return 800; }
int64_t gui_window_height(void) { return 600; }
int64_t gui_poll_event(void)    { return 0; }
int64_t gui_wait_event(int64_t ms) { (void)ms; return 0; }
int64_t gui_event_type(void)    { return 1; }
int64_t gui_event_key(void)     { return 0; }
char*   gui_event_text_str(void) { return strdup(""); }
int64_t gui_mouse_x(void)       { return 0; }
int64_t gui_mouse_y(void)       { return 0; }
int64_t gui_mouse_btn(void)     { return 0; }
int64_t gui_wheel_dy(void)      { return 0; }
int64_t gui_ticks(void)         { return 0; }
int64_t gui_delay(int64_t ms)   { (void)ms; return 0; }
int64_t gui_key_up(void)        { return 273; }
int64_t gui_key_down(void)      { return 274; }
int64_t gui_key_left(void)      { return 276; }
int64_t gui_key_right(void)     { return 275; }
int64_t gui_key_enter(void)     { return 13; }
int64_t gui_key_backspace(void) { return 8; }
int64_t gui_key_delete(void)    { return 127; }
int64_t gui_key_escape(void)    { return 27; }
int64_t gui_key_f1(void)        { return 282; }
int64_t gui_key_f2(void)        { return 283; }
int64_t gui_key_f3(void)        { return 284; }
int64_t gui_key_f4(void)        { return 285; }
int64_t gui_key_f5(void)        { return 286; }
int64_t gui_key_f6(void)        { return 287; }
int64_t gui_key_f7(void)        { return 288; }
int64_t gui_key_f8(void)        { return 289; }
int64_t gui_key_f9(void)        { return 290; }
int64_t gui_key_f10(void)       { return 291; }
int64_t gui_key_f11(void)       { return 292; }
int64_t gui_key_f12(void)       { return 293; }
int64_t gui_key_home(void)      { return 278; }
int64_t gui_key_end(void)       { return 279; }
int64_t gui_key_pgup(void)      { return 280; }
int64_t gui_key_pgdn(void)      { return 281; }
int64_t gui_key_pageup(void)    { return 280; }
int64_t gui_key_pagedown(void)  { return 281; }
int64_t gui_key_return(void)    { return 13; }
int64_t gui_key_tab(void)       { return 9; }
int64_t gui_key_space(void)     { return 32; }
int64_t gui_key_a(void) { return 97; }
int64_t gui_key_b(void) { return 98; }
int64_t gui_key_c(void) { return 99; }
int64_t gui_key_d(void) { return 100; }
int64_t gui_key_e(void) { return 101; }
int64_t gui_key_f(void) { return 102; }
int64_t gui_key_g(void) { return 103; }
int64_t gui_key_h(void) { return 104; }
int64_t gui_key_i(void) { return 105; }
int64_t gui_key_j(void) { return 106; }
int64_t gui_key_k(void) { return 107; }
int64_t gui_key_l(void) { return 108; }
int64_t gui_key_m(void) { return 109; }
int64_t gui_key_n(void) { return 110; }
int64_t gui_key_o(void) { return 111; }
int64_t gui_key_p(void) { return 112; }
int64_t gui_key_q(void) { return 113; }
int64_t gui_key_r(void) { return 114; }
int64_t gui_key_s(void) { return 115; }
int64_t gui_key_t(void) { return 116; }
int64_t gui_key_u(void) { return 117; }
int64_t gui_key_v(void) { return 118; }
int64_t gui_key_w(void) { return 119; }
int64_t gui_key_x(void) { return 120; }
int64_t gui_key_y(void) { return 121; }
int64_t gui_key_z(void) { return 122; }
int64_t gui_key_mod_ctrl(void)  { return 0; }
int64_t gui_key_mod_shift(void) { return 0; }
int64_t gui_key_mod_alt(void)   { return 0; }
char*   gui_clipboard_get(void) { return strdup(""); }
void    gui_clipboard_set(const char* text) { (void)text; }

#endif /* MANIT_NO_GUI */

/* ======================== fs_mkdir ======================== */

int64_t fs_mkdir(const char* path) {
    return (mkdir(path, 0755) == 0) ? 1 : 0;
}
