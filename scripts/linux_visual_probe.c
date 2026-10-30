// Test-only WebKit interposer. Never linked into or bundled with QuotaMate.
// The visual harness uses an isolated config and injects the same public IPC
// calls as the settings UI, without adding a production automation endpoint.
#define _GNU_SOURCE
#include <dlfcn.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <gdk/gdkx.h>
#include <webkit2/webkit2.h>

extern char *program_invocation_short_name;
static GList *views;
static gchar *last_command;
static gboolean polling;
static gulong pet_xid;

static void write_reply(const gchar *reply) {
    const gchar *command_path = getenv("QUOTAMATE_VISUAL_COMMAND");
    if (!command_path) return;
    gchar *reply_path = g_strconcat(command_path, ".ack", NULL);
    GError *error = NULL;
    if (!g_file_set_contents(reply_path, reply, -1, &error)) {
        fprintf(stderr, "QUOTAMATE_VISUAL_PROBE reply failed: %s\n", error->message);
        g_error_free(error);
    }
    g_free(reply_path);
}

static void script_reply(WebKitUserContentManager *manager,
                         WebKitJavascriptResult *result, gpointer data) {
    (void)manager;
    (void)data;
    JSCValue *value = webkit_javascript_result_get_js_value(result);
    gchar *reply = jsc_value_to_string(value);
    write_reply(reply);
    g_free(reply);
}

static void script_finished(GObject *object, GAsyncResult *result, gpointer data) {
    guint64 *request = data;
    GError *error = NULL;
    WebKitJavascriptResult *value = webkit_web_view_run_javascript_finish(
        WEBKIT_WEB_VIEW(object), result, &error);
    if (value) webkit_javascript_result_unref(value);
    if (error) {
        gchar *escaped = g_strescape(error->message, NULL);
        gchar *reply = g_strdup_printf(
            "{\"id\":%" G_GUINT64_FORMAT ",\"ok\":false,\"error\":\"%s\"}",
            *request, escaped);
        write_reply(reply);
        g_free(reply);
        g_free(escaped);
        g_error_free(error);
    }
    g_free(request);
}

static gboolean poll_command(gpointer data) {
    (void)data;
    // Export the exact test WebView's native window. A real user's existing
    // pet can have the same title on a physical display and must be untouched.
    for (GList *item = views; item; item = item->next) {
        WebKitWebView *view = item->data;
        const gchar *uri = webkit_web_view_get_uri(view);
        if (!uri || !strstr(uri, "window=pet")) continue;
        GtkWidget *toplevel = gtk_widget_get_toplevel(GTK_WIDGET(view));
        if (!gtk_widget_get_mapped(toplevel)) continue;
        GdkWindow *window = gtk_widget_get_window(toplevel);
        if (!window || !GDK_IS_X11_WINDOW(window)) continue;
        gulong xid = gdk_x11_window_get_xid(window);
        if (xid == pet_xid) continue;
        pet_xid = xid;
        gchar *path = g_strconcat(getenv("QUOTAMATE_VISUAL_COMMAND"), ".window", NULL);
        gchar *value = g_strdup_printf("%lu\n", xid);
        g_file_set_contents(path, value, -1, NULL);
        g_free(value);
        g_free(path);
    }
    const gchar *policy = getenv("QUOTAMATE_VISUAL_HARDWARE_POLICY");
    if (last_command && policy) {
        for (GList *item = views; item; item = item->next) {
            WebKitWebView *view = item->data;
            const gchar *uri = webkit_web_view_get_uri(view);
            if (!uri || !strstr(uri, "window=pet") ||
                g_object_get_data(G_OBJECT(view), "quotamate-visual-policy-applied")) continue;
            WebKitHardwareAccelerationPolicy next = strcmp(policy, "always") == 0
                ? WEBKIT_HARDWARE_ACCELERATION_POLICY_ALWAYS
                : WEBKIT_HARDWARE_ACCELERATION_POLICY_ON_DEMAND;
            webkit_settings_set_hardware_acceleration_policy(webkit_web_view_get_settings(view), next);
            g_object_set_data(G_OBJECT(view), "quotamate-visual-policy-applied", GINT_TO_POINTER(1));
            fprintf(stderr, "QUOTAMATE_VISUAL_PROBE hardware policy=%s\n", policy);
        }
    }
    // Optional diagnostic only, never part of a normal regression run. It can
    // distinguish missing GTK damage from stale pixels inside WebKit's store.
    if (last_command && getenv("QUOTAMATE_VISUAL_FORCE_REDRAW")) {
        for (GList *item = views; item; item = item->next) {
            WebKitWebView *view = item->data;
            const gchar *uri = webkit_web_view_get_uri(view);
            if (uri && strstr(uri, "window=pet")) gtk_widget_queue_draw(GTK_WIDGET(view));
        }
    }
    gchar *command = NULL;
    const gchar *path = getenv("QUOTAMATE_VISUAL_COMMAND");
    if (!path || !g_file_get_contents(path, &command, NULL, NULL)) return G_SOURCE_CONTINUE;
    if (g_strcmp0(last_command, command) == 0) {
        g_free(command);
        return G_SOURCE_CONTINUE;
    }
    for (GList *item = views; item; item = item->next) {
        WebKitWebView *view = item->data;
        const gchar *uri = webkit_web_view_get_uri(view);
        if (!uri || !strstr(uri, "window=pet") || webkit_web_view_is_loading(view)) continue;
        guint64 *request = g_new0(guint64, 1);
        if (sscanf(command, "// quotamate-visual-request: %" G_GUINT64_FORMAT, request) != 1) {
            g_free(request);
            break;
        }
        g_free(last_command);
        last_command = g_strdup(command);
        webkit_web_view_run_javascript(view, command, NULL, script_finished, request);
        break;
    }
    g_free(command);
    return G_SOURCE_CONTINUE;
}

static void forget_view(gpointer data, GObject *object) {
    (void)data;
    views = g_list_remove(views, object);
}

WebKitSettings *webkit_web_view_get_settings(WebKitWebView *view) {
    WebKitSettings *(*original)(WebKitWebView *) = dlsym(RTLD_NEXT, "webkit_web_view_get_settings");
    WebKitSettings *settings = original(view);
    if (!getenv("QUOTAMATE_VISUAL_COMMAND") ||
        strcmp(program_invocation_short_name, "quotamate") != 0) return settings;
    if (!g_list_find(views, view)) {
        WebKitUserContentManager *manager = webkit_web_view_get_user_content_manager(view);
        g_signal_connect(manager, "script-message-received::quotamateVisual",
                         G_CALLBACK(script_reply), NULL);
        if (!webkit_user_content_manager_register_script_message_handler(manager, "quotamateVisual")) {
            fprintf(stderr, "QUOTAMATE_VISUAL_PROBE handler registration failed\n");
        }
        views = g_list_append(views, view);
        g_object_weak_ref(G_OBJECT(view), forget_view, NULL);
        fprintf(stderr, "QUOTAMATE_VISUAL_PROBE observing WebView\n");
    }
    if (!polling) {
        polling = TRUE;
        g_timeout_add(75, poll_command, NULL);
    }
    return settings;
}
