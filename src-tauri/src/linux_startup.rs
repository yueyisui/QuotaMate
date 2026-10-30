use std::path::Path;

#[link(name = "X11")]
unsafe extern "C" {
    fn XInitThreads() -> std::ffi::c_int;
}

// Call first in main, before GTK opens a display or Tauri starts worker threads.
// GTK, WebKit and Tao's input thread all use Xlib in a multi-window application.
pub fn initialize() {
    // SAFETY: No toolkit, display connection or application thread exists yet.
    // Xlib requires this call before every other Xlib function.
    if unsafe { XInitThreads() } == 0 {
        eprintln!("Unable to initialize X11 thread support");
        std::process::exit(1);
    }

    // Older proprietary NVIDIA drivers cannot reliably import WebKitGTK's
    // DMA-BUF textures, especially in transparent windows. Keep the fallback
    // local to NVIDIA systems and honor an explicitly configured renderer.
    if Path::new("/proc/driver/nvidia/version").exists()
        && std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none()
    {
        // SAFETY: This runs before GTK/WebKit initialization and any threads.
        unsafe { std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1") };
    }
}
