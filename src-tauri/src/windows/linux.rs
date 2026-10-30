use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};

use gtk::{
    cairo::{ImageSurface, Operator, Surface},
    glib::{self, ControlFlow, Propagation, SourceId},
    prelude::*,
};
use tauri::WebviewWindow;
use webkit2gtk::{
    HardwareAccelerationPolicy, SettingsExt, SnapshotOptions, SnapshotRegion, WebView, WebViewExt,
    gio::{self, prelude::CancellableExt},
};

const FRAME_INTERVAL: Duration = Duration::from_millis(33);

#[derive(Clone, Copy, PartialEq, Eq)]
struct Geometry {
    width: i32,
    height: i32,
    scale: i32,
}

impl Geometry {
    fn of(view: &WebView) -> Self {
        Self {
            width: view.allocated_width(),
            height: view.allocated_height(),
            scale: view.scale_factor(),
        }
    }
}

struct Frame {
    surface: Surface,
    geometry: Geometry,
}

struct Renderer {
    frame: Option<Frame>,
    geometry: Geometry,
    generation: u64,
    pending: Option<gio::Cancellable>,
    timer: Option<SourceId>,
    destroyed: bool,
    failures: u32,
    next_attempt: Instant,
    last_warning: Option<Instant>,
}

impl Renderer {
    fn invalidate(&mut self) -> Option<gio::Cancellable> {
        self.generation = self.generation.wrapping_add(1);
        self.frame = None;
        // Keep the pending slot occupied until its callback completes. Cancelling
        // a request must never allow a second request to run alongside it.
        self.pending.clone()
    }

    fn update_geometry(&mut self, geometry: Geometry) -> Option<gio::Cancellable> {
        if self.geometry == geometry {
            return None;
        }
        self.geometry = geometry;
        self.invalidate()
    }
}

pub(super) fn configure_pet(window: &WebviewWindow) -> Result<(), String> {
    window
        .with_webview(|webview| {
            let view = webview.inner();
            if let Some(settings) = WebViewExt::settings(&view) {
                settings.set_hardware_acceleration_policy(HardwareAccelerationPolicy::Never);
            }
            // The fix was backported to stable 2.52.6 and is in 2.54+.
            // Keep the normal, cheaper drawing path on those engines. Earlier
            // 2.53 development builds still have the bug, so do not use a
            // simple lexicographic >= 2.52.6 check.
            // SAFETY: These public C getters only read immutable library
            // version constants and require no pointers or owned resources.
            let (major, minor, micro) = unsafe {
                (
                    webkit2gtk::ffi::webkit_get_major_version(),
                    webkit2gtk::ffi::webkit_get_minor_version(),
                    webkit2gtk::ffi::webkit_get_micro_version(),
                )
            };
            let native_renderer = has_transparent_copy_fix(major, minor, micro);
            log::info!(
                "Pet renderer: WebKitGTK {major}.{minor}.{micro}, {}",
                if native_renderer {
                    "native"
                } else {
                    "transparent-frame compatibility"
                }
            );
            if !native_renderer {
                install_renderer(&view);
            }
        })
        .map_err(|error| error.to_string())
}

fn has_transparent_copy_fix(major: u32, minor: u32, micro: u32) -> bool {
    // https://github.com/WebKit/WebKit/commit/33a866ae9ae3e6adb219377d3b16bacfc87a9c36
    major > 2 || (major == 2 && (minor >= 54 || (minor == 52 && micro >= 6)))
}

#[cfg(test)]
mod tests {
    use super::has_transparent_copy_fix;

    #[test]
    fn follows_verified_stable_backports_without_skipping_affected_development_builds() {
        for version in [(2, 46, 0), (2, 50, 6), (2, 52, 5), (2, 53, 4)] {
            assert!(!has_transparent_copy_fix(version.0, version.1, version.2));
        }
        for version in [(2, 52, 6), (2, 54, 0), (2, 54, 2)] {
            assert!(has_transparent_copy_fix(version.0, version.1, version.2));
        }
    }
}

fn install_renderer(view: &WebView) {
    // Affected Skia versions of WebKitGTK blend transparent SHM updates into the
    // previous backing store (WebKit bug 319864). Clearing the GTK draw target
    // cannot repair that already-corrupted image. Fresh public-API snapshots
    // bypass the retained store and preserve SVG, GIF, CSS animation and input.
    // https://bugs.webkit.org/show_bug.cgi?id=319864
    let renderer = Rc::new(RefCell::new(Renderer {
        frame: None,
        geometry: Geometry::of(view),
        generation: 0,
        pending: None,
        timer: None,
        destroyed: false,
        failures: 0,
        next_attempt: Instant::now(),
        last_warning: None,
    }));

    let drawing = renderer.clone();
    view.connect_draw(move |view, context| {
        if context.save().is_ok() {
            context.set_operator(Operator::Clear);
            let _ = context.paint();
            let state = drawing.borrow();
            if let Some(frame) = &state.frame
                && frame.geometry == Geometry::of(view)
            {
                context.set_operator(Operator::Source);
                if context.set_source_surface(&frame.surface, 0.0, 0.0).is_ok() {
                    let _ = context.paint();
                }
            }
            let _ = context.restore();
        }
        // The default handler would paint WebKit's stale backing store again.
        Propagation::Stop
    });

    let resizing = renderer.clone();
    view.connect_size_allocate(move |view, _| {
        let cancelled = resizing.borrow_mut().update_geometry(Geometry::of(view));
        if let Some(cancelled) = cancelled {
            cancelled.cancel();
        }
    });
    let scaling = renderer.clone();
    view.connect_scale_factor_notify(move |view| {
        let cancelled = scaling.borrow_mut().update_geometry(Geometry::of(view));
        if let Some(cancelled) = cancelled {
            cancelled.cancel();
        }
    });
    let unmapping = renderer.clone();
    view.connect_unmap(move |_| {
        let cancelled = unmapping.borrow_mut().invalidate();
        if let Some(cancelled) = cancelled {
            cancelled.cancel();
        }
    });

    let ticking = renderer.clone();
    let weak_view = view.downgrade();
    let timer = glib::timeout_add_local(FRAME_INTERVAL, move || {
        let Some(view) = weak_view.upgrade() else {
            return ControlFlow::Break;
        };
        if ticking.borrow().destroyed {
            return ControlFlow::Break;
        }
        if !view.is_mapped() || view.is_loading() {
            return ControlFlow::Continue;
        }
        let geometry = Geometry::of(&view);
        let cancelled = ticking.borrow_mut().update_geometry(geometry);
        if let Some(cancelled) = cancelled {
            cancelled.cancel();
        }
        let (generation, cancellable) = {
            let mut state = ticking.borrow_mut();
            if geometry.width <= 0
                || geometry.height <= 0
                || state.pending.is_some()
                || Instant::now() < state.next_attempt
            {
                return ControlFlow::Continue;
            }
            let cancellable = gio::Cancellable::new();
            state.pending = Some(cancellable.clone());
            (state.generation, cancellable)
        };
        let completed = ticking.clone();
        let weak_view = view.downgrade();
        view.snapshot(
            SnapshotRegion::Visible,
            SnapshotOptions::TRANSPARENT_BACKGROUND,
            Some(&cancellable),
            move |result| {
                let mut state = completed.borrow_mut();
                state.pending = None;
                let Some(view) = weak_view.upgrade() else {
                    return;
                };
                if state.destroyed
                    || state.generation != generation
                    || !view.is_mapped()
                    || Geometry::of(&view) != geometry
                {
                    return;
                }
                match result {
                    Ok(surface) => {
                        // Snapshot pixels can be device-scaled. Fit their actual
                        // dimensions to this exact logical GTK allocation.
                        if let Ok(image) = ImageSurface::try_from(surface.clone()) {
                            surface.set_device_scale(
                                f64::from(image.width()) / f64::from(geometry.width),
                                f64::from(image.height()) / f64::from(geometry.height),
                            );
                        }
                        state.frame = Some(Frame { surface, geometry });
                        state.failures = 0;
                        state.next_attempt = Instant::now();
                        drop(state);
                        // Full damage removes pixels outside a moving/shrinking
                        // SVG as well as pixels that became fully transparent.
                        view.queue_draw();
                    }
                    Err(error) => {
                        state.failures = state.failures.saturating_add(1);
                        let delay = 250_u64 * (1_u64 << state.failures.min(4));
                        let now = Instant::now();
                        state.next_attempt = now + Duration::from_millis(delay);
                        if state
                            .last_warning
                            .is_none_or(|last| now.duration_since(last) >= Duration::from_secs(60))
                        {
                            log::warn!("Unable to render Linux pet snapshot: {error}");
                            state.last_warning = Some(now);
                        }
                    }
                }
            },
        );
        ControlFlow::Continue
    });
    renderer.borrow_mut().timer = Some(timer);

    view.connect_destroy(move |_| {
        let (cancelled, timer) = {
            let mut state = renderer.borrow_mut();
            state.destroyed = true;
            let cancelled = state.invalidate();
            (cancelled, state.timer.take())
        };
        if let Some(cancelled) = cancelled {
            cancelled.cancel();
        }
        if let Some(timer) = timer {
            timer.remove();
        }
    });
}
