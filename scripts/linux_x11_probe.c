// Test-only interposer: verify XInitThreads precedes opening QuotaMate displays.
#define _GNU_SOURCE
#include <dlfcn.h>
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>

typedef struct _XDisplay Display;
extern char *program_invocation_short_name;
static int initialized;
static int is_app(void);
static void *traced_pet;
static unsigned suppressed_handlers;
static unsigned configure_callbacks_after_suppression;
static void *traced_clock;
static unsigned flush_count, resume_count, resize_check_count, paused_check_count;
static unsigned manual_check_count, paused_manual_check_count;
static int clock_paused, clock_report_registered;
static double last_flush, last_resume;
static unsigned paint_sources[64], paint_source_count;

static void trace_paint_sources(void) {
    void *(*find)(void *, unsigned) = dlsym(RTLD_NEXT, "g_main_context_find_source_by_id");
    int64_t (*ready_time)(void *) = dlsym(RTLD_NEXT, "g_source_get_ready_time");
    int64_t (*now)(void) = dlsym(RTLD_NEXT, "g_get_monotonic_time");
    if (!find || !ready_time || !now) return;
    for (unsigned index = 0; index < 64; index++) {
        if (!paint_sources[index]) continue;
        void *source = find(NULL, paint_sources[index]);
        if (source) fprintf(stderr, "QUOTAMATE_X11_PROBE paint-source id=%u ready-delta-us=%lld\n",
                            paint_sources[index], (long long)(ready_time(source) - now()));
    }
}

static double monotonic_seconds(void) {
    struct timespec now;
    clock_gettime(CLOCK_MONOTONIC, &now);
    return now.tv_sec + now.tv_nsec / 1000000000.0;
}

static void report_clock(void) {
    fprintf(stderr, "QUOTAMATE_X11_PROBE frame-clock-summary checks=%u paused-checks=%u manual-checks=%u paused-manual-checks=%u flush=%u resume=%u paused=%d last-flush=%.6f last-resume=%.6f\n",
            resize_check_count, paused_check_count, manual_check_count, paused_manual_check_count, flush_count, resume_count,
            clock_paused, last_flush, last_resume);
}

static void clock_event(void *clock, void *data) {
    if (clock != traced_clock) return;
    // Observe the flush/resume protocol after its normal handlers. This state
    // means flush has not yet been followed by resume; no private GTK memory is read.
    clock_paused = data != NULL;
    if (clock_paused) { flush_count++; last_flush = monotonic_seconds(); }
    else { resume_count++; last_resume = monotonic_seconds(); }
}

static void forget_clock(void *data, void *clock) {
    (void)data;
    if (traced_clock == clock) traced_clock = NULL;
}

static void observe_clock(void *window) {
    if (!initialized || !is_app() || traced_pet != window ||
        !getenv("QUOTAMATE_TEST_FRAME_CLOCK")) return;
    void *(*get_clock)(void *) = dlsym(RTLD_NEXT, "gtk_widget_get_frame_clock");
    typedef void (*Callback)(void);
    unsigned long (*connect_data)(void *, const char *, Callback, void *, void *, int) =
        dlsym(RTLD_NEXT, "g_signal_connect_data");
    void (*weak_ref)(void *, void (*)(void *, void *), void *) = dlsym(RTLD_NEXT, "g_object_weak_ref");
    if (!get_clock || !connect_data || !weak_ref) return;
    void *clock = get_clock(window);
    if (!clock || traced_clock == clock) return;
    traced_clock = clock;
    connect_data(clock, "flush-events", (Callback)clock_event, (void *)1, NULL, 1);
    connect_data(clock, "resume-events", (Callback)clock_event, NULL, NULL, 1);
    weak_ref(clock, forget_clock, NULL);
    if (!clock_report_registered) { atexit(report_clock); clock_report_registered = 1; }
}

static void report_suppression(void) {
    fprintf(stderr, "QUOTAMATE_X11_PROBE suppression-summary blocked=%u callbacks=%u\n",
            suppressed_handlers, configure_callbacks_after_suppression);
}

static void suppress_configure_handlers(void *window) {
    if (!initialized || !is_app() || traced_pet != window || suppressed_handlers) return;
    const char *marker = getenv("QUOTAMATE_TEST_SUPPRESS_CONFIGURE_MARKER");
    if (!marker || access(marker, F_OK) != 0) return;
    unsigned long (*widget_type)(void) = dlsym(RTLD_NEXT, "gtk_widget_get_type");
    unsigned (*lookup)(const char *, unsigned long) = dlsym(RTLD_NEXT, "g_signal_lookup");
    unsigned long (*find)(void *, unsigned, unsigned, unsigned, void *, void *, void *) =
        dlsym(RTLD_NEXT, "g_signal_handler_find");
    void (*block)(void *, unsigned long) = dlsym(RTLD_NEXT, "g_signal_handler_block");
    if (!widget_type || !lookup || !find || !block) return;
    unsigned signal = lookup("configure-event", widget_type());
    if (!signal) return;
    unsigned long handler;
    // MATCH_ID alone in block_matched requires GLib 2.78. Finding unblocked
    // handlers by ID also works on Ubuntu 22.04, preserving the class closure.
    while (suppressed_handlers < 128 &&
           (handler = find(window, (1u << 0) | (1u << 5), signal, 0, NULL, NULL, NULL))) {
        block(window, handler);
        suppressed_handlers++;
    }
    if (suppressed_handlers) {
        fprintf(stderr, "QUOTAMATE_X11_PROBE suppression-start blocked=%u\n", suppressed_handlers);
        atexit(report_suppression);
    }
}

// Public GdkEventConfigure ABI, kept local so the probe needs no GTK headers.
typedef struct {
    int type;
    void *window;
    signed char send_event;
    int x, y, width, height;
} ProbeConfigureEvent;

static void write_layout(void *window) {
    if (!initialized || !is_app() || traced_pet != window) return;
    const char *path = getenv("QUOTAMATE_TEST_LAYOUT_PATH");
    if (!path) return;
    void *(*get_window)(void *) = dlsym(RTLD_NEXT, "gtk_widget_get_window");
    void *(*get_child)(void *) = dlsym(RTLD_NEXT, "gtk_bin_get_child");
    int (*mapped)(void *) = dlsym(RTLD_NEXT, "gtk_widget_get_mapped");
    int (*allocated_width)(void *) = dlsym(RTLD_NEXT, "gtk_widget_get_allocated_width");
    int (*allocated_height)(void *) = dlsym(RTLD_NEXT, "gtk_widget_get_allocated_height");
    int (*cached_width)(void *) = dlsym(RTLD_NEXT, "gdk_window_get_width");
    int (*cached_height)(void *) = dlsym(RTLD_NEXT, "gdk_window_get_height");
    unsigned long (*xid)(void *) = dlsym(RTLD_NEXT, "gdk_x11_window_get_xid");
    void (*geometry)(void *, int *, int *, int *, int *) = dlsym(RTLD_NEXT, "gdk_window_get_geometry");
    int (*set_contents)(const char *, const char *, long, void **) = dlsym(RTLD_NEXT, "g_file_set_contents");
    if (!get_window || !get_child || !mapped || !allocated_width || !allocated_height ||
        !cached_width || !cached_height || !xid || !geometry || !set_contents) return;
    void *surface = get_window(window);
    if (!surface) return;
    void *child = get_child(window);
    int width = 0, height = 0;
    geometry(surface, NULL, NULL, &width, &height);
    char json[512];
    int length = snprintf(json, sizeof(json),
        "{\"xid\":%lu,\"mapped\":%d,\"native\":[%d,%d],\"gdkCached\":[%d,%d],"
        "\"windowAllocation\":[%d,%d],\"childAllocation\":[%d,%d]}\n",
        xid(surface), mapped(window), width, height, cached_width(surface), cached_height(surface),
        allocated_width(window), allocated_height(window),
        child ? allocated_width(child) : 0, child ? allocated_height(child) : 0);
    if (length > 0 && (size_t)length < sizeof(json)) set_contents(path, json, length, NULL);
}

static void allocated(void *widget, void *allocation, void *data) {
    (void)allocation;
    (void)data;
    write_layout(widget);
}

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
    int (*allocated_width)(void *) = dlsym(RTLD_NEXT, "gtk_widget_get_allocated_width");
    int (*allocated_height)(void *) = dlsym(RTLD_NEXT, "gtk_widget_get_allocated_height");
    void *(*get_child)(void *) = dlsym(RTLD_NEXT, "gtk_bin_get_child");
    void *child = get_child ? get_child(window) : NULL;
    void *native = get_window ? get_window(window) : NULL;
    int x = 0, y = 0, width = 0, height = 0;
    if (get_size) get_size(window, &width, &height);
    if (get_position) get_position(window, &x, &y);
    int server_x = 0, server_y = 0, server_width = 0, server_height = 0;
    if (native && get_geometry) get_geometry(native, NULL, NULL, &server_width, &server_height);
    if (native && get_origin) get_origin(native, &server_x, &server_y);
    struct timespec now;
    clock_gettime(CLOCK_MONOTONIC, &now);
    fprintf(stderr, "QUOTAMATE_X11_PROBE pet event=%s time=%ld.%03ld visible=%d mapped=%d realized=%d xid=%lu geometry=%d,%d,%d,%d native=%d,%d,%d,%d allocation=%d,%d child=%d,%d\n",
            event, now.tv_sec, now.tv_nsec / 1000000,
            get_visible ? get_visible(window) : -1,
            get_mapped ? get_mapped(window) : -1,
            get_realized ? get_realized(window) : -1,
            native && get_xid ? get_xid(native) : 0, x, y, width, height,
            server_x, server_y, server_width, server_height,
            allocated_width ? allocated_width(window) : -1,
            allocated_height ? allocated_height(window) : -1,
            child && allocated_width ? allocated_width(child) : -1,
            child && allocated_height ? allocated_height(child) : -1);
    write_layout(window);
    observe_clock(window);
}

static int configured(void *widget, const ProbeConfigureEvent *event, void *data) {
    (void)data;
    if (suppressed_handlers) configure_callbacks_after_suppression++;
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
    // Read allocation after GTK's class handler has laid out its child. No
    // polling source, layout request or event injection is added by the probe.
    connect_data(window, "size-allocate", (Callback)allocated, NULL, NULL, 1);
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
    suppress_configure_handlers(window);
    trace_request(window, "resize", width, height);
    original(window, width, height);
}

void gtk_window_move(void *window, int x, int y) {
    void (*original)(void *, int, int) = dlsym(RTLD_NEXT, "gtk_window_move");
    trace_request(window, "move", x, y);
    original(window, x, y);
}

void gtk_container_check_resize(void *container) {
    void (*original)(void *) = dlsym(RTLD_NEXT, "gtk_container_check_resize");
    int diagnostic = initialized && is_app() && traced_pet == container &&
        getenv("QUOTAMATE_TEST_FRAME_CLOCK") != NULL;
    const char *caller = "unknown";
    if (diagnostic) {
        Dl_info information = {0};
        if (dladdr(__builtin_return_address(0), &information) && information.dli_fname) {
            const char *slash = strrchr(information.dli_fname, '/');
            caller = slash ? slash + 1 : information.dli_fname;
        }
        observe_clock(container);
        resize_check_count++;
        if (clock_paused) paused_check_count++;
        if (strcmp(caller, "quotamate") == 0) {
            manual_check_count++;
            if (clock_paused) paused_manual_check_count++;
        }
        fprintf(stderr, "QUOTAMATE_X11_PROBE check-resize-before caller=%s paused=%d flush=%u resume=%u last-flush=%.6f last-resume=%.6f\n",
                caller, clock_paused, flush_count, resume_count, last_flush, last_resume);
        trace_paint_sources();
        trace_pet(container, "check-resize-before");
    }
    original(container);
    if (diagnostic) {
        fprintf(stderr, "QUOTAMATE_X11_PROBE check-resize-after caller=%s paused=%d flush=%u resume=%u last-flush=%.6f last-resume=%.6f\n",
                caller, clock_paused, flush_count, resume_count, last_flush, last_resume);
        trace_pet(container, "check-resize-after");
    }
}

void g_source_set_name_by_id(unsigned tag, const char *name) {
    void (*original)(unsigned, const char *) = dlsym(RTLD_NEXT, "g_source_set_name_by_id");
    original(tag, name);
    if (initialized && is_app() && getenv("QUOTAMATE_TEST_FRAME_CLOCK") && name &&
        strcmp(name, "[gtk+] gdk_frame_clock_paint_idle") == 0) {
        paint_sources[paint_source_count++ % 64] = tag;
    }
}
