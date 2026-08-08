/*
 * manit_runtime.c — C runtime library for the ManiT balanced ternary language.
 * This file is the aggregator; implementation is split into sub-modules.
 *
 * Compile (full, with SDL2 + curl):
 *   clang -c manit_runtime.c -o manit_runtime.o `sdl2-config --cflags` -lSDL2 -lSDL2_ttf -lcurl -lm -lpthread
 *
 * Compile (minimal, no SDL2/curl — for shell/TUI binaries):
 *   clang -c -DMANIT_NO_GUI manit_runtime.c -o manit_runtime_minimal.o -lm -lpthread
 *
 * Author: Manish Jagdish Thatte
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <math.h>
#include <stdint.h>
#include <stdarg.h>
#include <pthread.h>
#include <unistd.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <sys/ioctl.h>
#include <dirent.h>
#include <time.h>
#include <errno.h>
#include <termios.h>

/* Forward declarations (used across sub-modules) */
typedef struct ManitVec ManitVec;
static ManitVec* Vec_new_internal(void);
static void Vec_push_internal(ManitVec* v, int64_t x);

#include "core.c"
#include "collections.c"
#include "sync.c"
#include "system.c"
#include "net.c"
#include "gui.c"
