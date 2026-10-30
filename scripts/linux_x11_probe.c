// Test-only interposer: verify XInitThreads precedes opening QuotaMate displays.
#define _GNU_SOURCE
#include <dlfcn.h>
#include <stdio.h>
#include <string.h>

typedef struct _XDisplay Display;
extern char *program_invocation_short_name;
static int initialized;

static int is_app(void) {
    return strcmp(program_invocation_short_name, "quotamate") == 0;
}

int XInitThreads(void) {
    int (*original)(void) = dlsym(RTLD_NEXT, "XInitThreads");
    initialized = original();
    if (is_app()) fprintf(stderr, "QUOTAMATE_X11_PROBE init=%d\n", initialized);
    return initialized;
}

Display *XOpenDisplay(const char *name) {
    Display *(*original)(const char *) = dlsym(RTLD_NEXT, "XOpenDisplay");
    if (is_app()) fprintf(stderr, "QUOTAMATE_X11_PROBE open initialized=%d\n", initialized);
    return original(name);
}
