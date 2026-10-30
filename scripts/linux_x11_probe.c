// Test-only interposer: verify XInitThreads precedes opening QuotaMate displays.
#define _GNU_SOURCE
#include <dlfcn.h>
#include <stdio.h>
#include <string.h>
#include <time.h>

typedef struct _XDisplay Display;
extern char *program_invocation_short_name;
static int initialized;
static int is_app(void);
static void *traced_pet;

// Public GdkEventConfigure ABI, kept local so the probe needs no GTK headers.
typedef struct {
    int type;
    void *window;
    signed char send_event;
    int x, y, width, height;
} ProbeConfigureEvent;

static void trace_pet(void *widget, const char *event) {
    // Never initialize GTK types while observing pre-XInitThreads startup.
    if (!initialized || !is_app()) return;
    void *(*get_toplevel)(void *) = dlsym(RTLD_NEXT, "gtk_widget_get_toplevel");
    unsigned long (*window_type)(void) = dlsym(RTLD_NEXT, "gtk_window_get_type");
    unsigned long (*widget_type)(void) = dlsym(RTLD_NEXT, "gtk_widget_get_type");
    int (*is_type)(void *, unsigned long) = dlsym(RTLD_NEXT, "g_type_check_instance_is_a");
    const char *(*get_title)(void *) = dlsym(RTLD_NEXT, "gtk_window_get_title");
    if (!get_toplevel || !window_type || !widget_type || !is_type || !get_title ||
        !is_type(widget, widget_type())) return;
    void *window = get_toplevel(widget);
    if (!window || !is_type(window, window_type())) return;
    const char *title = get_title(window);
    if (!title || strcmp(title, "QuotaMate Pet") != 0) return;
    int (*get_visible)(void *) = dlsym(RTLD_NEXT, "gtk_widget_get_visible");
    int (*get_mapped)(void *) = dlsym(RTLD_NEXT, "gtk_widget_get_mapped");
    int (*get_realized)(void *) = dlsym(RTLD_NEXT, "gtk_widget_get_realized");
    void *(*get_window)(void *) = dlsym(RTLD_NEXT, "gtk_widget_get_window");
    unsigned long (*get_xid)(void *) = dlsym(RTLD_NEXT, "gdk_x11_window_get_xid");
    void (*get_size)(void *, int *, int *) = dlsym(RTLD_NEXT, "gtk_window_get_size");
    void (*get_position)(void *, int *, int *) = dlsym(RTLD_NEXT, "gtk_window_get_position");
    void (*get_geometry)(void *, int *, int *, int *, int *) = dlsym(RTLD_NEXT, "gdk_window_get_geometry");
    int (*get_origin)(void *, int *, int *) = dlsym(RTLD_NEXT, "gdk_window_get_origin");
    void *native = get_window ? get_window(window) : NULL;
    int x = 0, y = 0, width = 0, height = 0;
    if (get_size) get_size(window, &width, &height);
    if (get_position) get_position(window, &x, &y);
    int server_x = 0, server_y = 0, server_width = 0, server_height = 0;
    if (native && get_geometry) get_geometry(native, NULL, NULL, &server_width, &server_height);
    if (native && get_origin) get_origin(native, &server_x, &server_y);
    struct timespec now;
    clock_gettime(CLOCK_MONOTONIC, &now);
    fprintf(stderr, "QUOTAMATE_X11_PROBE pet event=%s time=%ld.%03ld visible=%d mapped=%d realized=%d xid=%lu geometry=%d,%d,%d,%d native=%d,%d,%d,%d\n",
            event, now.tv_sec, now.tv_nsec / 1000000,
            get_visible ? get_visible(window) : -1,
            get_mapped ? get_mapped(window) : -1,
            get_realized ? get_realized(window) : -1,
            native && get_xid ? get_xid(native) : 0, x, y, width, height,
            server_x, server_y, server_width, server_height);
}

static int configured(void *widget, const ProbeConfigureEvent *event, void *data) {
    (void)data;
    fprintf(stderr, "QUOTAMATE_X11_PROBE pet configure-payload=%d,%d,%d,%d send-event=%d\n",
            event->x, event->y, event->width, event->height, event->send_event);
    trace_pet(widget, "configure-event");
    return 0;
}

static void map_state(void *widget, void *data) {
    trace_pet(widget, data);
}

static int mapped_event(void *widget, void *event, void *data) {
    (void)event;
    trace_pet(widget, data);
    return 0;
}

static void forget_pet(void *data, void *object) {
    (void)data;
    if (traced_pet == object) traced_pet = NULL;
}

static void observe_pet(void *window, const char *title) {
    if (!initialized || !is_app() || !title || strcmp(title, "QuotaMate Pet") != 0 || traced_pet == window) return;
    typedef void (*Callback)(void);
    unsigned long (*connect_data)(void *, const char *, Callback, void *, void *, int) =
        dlsym(RTLD_NEXT, "g_signal_connect_data");
    void (*weak_ref)(void *, void (*)(void *, void *), void *) = dlsym(RTLD_NEXT, "g_object_weak_ref");
    if (!connect_data || !weak_ref) return;
    traced_pet = window;
    connect_data(window, "configure-event", (Callback)configured, NULL, NULL, 0);
    connect_data(window, "map", (Callback)map_state, "map-signal", NULL, 0);
    connect_data(window, "unmap", (Callback)map_state, "unmap-signal", NULL, 0);
    connect_data(window, "map-event", (Callback)mapped_event, "map-event", NULL, 0);
    connect_data(window, "unmap-event", (Callback)mapped_event, "unmap-event", NULL, 0);
    weak_ref(window, forget_pet, NULL);
}

static void trace_request(void *window, const char *event, int first, int second) {
    if (!initialized || !is_app()) return;
    const char *(*get_title)(void *) = dlsym(RTLD_NEXT, "gtk_window_get_title");
    const char *title = get_title ? get_title(window) : NULL;
    if (!title || strcmp(title, "QuotaMate Pet") != 0) return;
    fprintf(stderr, "QUOTAMATE_X11_PROBE pet request=%s values=%d,%d\n", event, first, second);
    trace_pet(window, event);
}

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

void gtk_window_set_title(void *window, const char *title) {
    void (*original)(void *, const char *) = dlsym(RTLD_NEXT, "gtk_window_set_title");
    original(window, title);
    observe_pet(window, title);
    trace_pet(window, "set-title");
}

void gtk_widget_show(void *widget) {
    void (*original)(void *) = dlsym(RTLD_NEXT, "gtk_widget_show");
    trace_pet(widget, "show-before");
    original(widget);
    trace_pet(widget, "show-after");
}

void gtk_widget_show_all(void *widget) {
    void (*original)(void *) = dlsym(RTLD_NEXT, "gtk_widget_show_all");
    trace_pet(widget, "show-all-before");
    original(widget);
    trace_pet(widget, "show-all-after");
}

void gtk_widget_hide(void *widget) {
    void (*original)(void *) = dlsym(RTLD_NEXT, "gtk_widget_hide");
    trace_pet(widget, "hide-before");
    original(widget);
    trace_pet(widget, "hide-after");
}

void gtk_window_resize(void *window, int width, int height) {
    void (*original)(void *, int, int) = dlsym(RTLD_NEXT, "gtk_window_resize");
    trace_request(window, "resize", width, height);
    original(window, width, height);
}

void gtk_window_move(void *window, int x, int y) {
    void (*original)(void *, int, int) = dlsym(RTLD_NEXT, "gtk_window_move");
    trace_request(window, "move", x, y);
    original(window, x, y);
}
