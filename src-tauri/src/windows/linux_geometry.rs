use std::{cell::RefCell, collections::HashMap, rc::Rc, time::Duration};

use gtk::{glib, prelude::*};
use tauri::{PhysicalPosition, PhysicalSize, WebviewWindow};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Geometry {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

impl Geometry {
    fn read(window: &gtk::Window) -> Option<Self> {
        let surface = window.window()?;
        let (_, x, y) = surface.origin();
        let (_, _, width, height) = surface.geometry();
        Some(Self {
            x,
            y,
            width,
            height,
        })
    }

    fn same_size(self, other: Self) -> bool {
        self.width == other.width && self.height == other.height
    }
}

#[derive(Clone, Copy)]
struct Request {
    width: i32,
    height: i32,
    position: Option<(i32, i32)>,
    force_resize: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Transition {
    Resizing(Geometry),
    Moving(Geometry),
}

#[derive(Default)]
struct Controller {
    active: Option<Transition>,
    queued: Option<Request>,
    watchdog: Option<glib::SourceId>,
    idle: Option<glib::SourceId>,
    observer: Option<glib::SourceId>,
    ready: bool,
    force_move: bool,
    active_recovery: bool,
    last_stable: Option<(Geometry, i32)>,
}

#[derive(Debug, PartialEq, Eq)]
enum Step {
    Wait,
    Next,
    Apply(Transition),
}

impl Controller {
    fn step(&mut self, current: Geometry, target: impl FnOnce(Request) -> Geometry) -> Step {
        if !self.ready {
            return Step::Wait;
        }
        match self.active {
            Some(Transition::Resizing(target)) => {
                if !current.same_size(target) {
                    return Step::Wait;
                }
                self.active = Some(Transition::Moving(target));
                Step::Apply(Transition::Moving(target))
            }
            Some(Transition::Moving(target)) => {
                if current != target {
                    return Step::Wait;
                }
                self.active = None;
                self.force_move = false;
                Step::Next
            }
            None => {
                let Some(request) = self.queued.take() else {
                    return Step::Wait;
                };
                self.force_move = request.position.is_some();
                self.active_recovery = request.force_resize;
                let target = target(request);
                let transition = if !request.force_resize && current.same_size(target) {
                    Transition::Moving(target)
                } else {
                    Transition::Resizing(target)
                };
                self.active = Some(transition);
                Step::Apply(transition)
            }
        }
    }

    fn recover(&mut self, current: Option<Geometry>) -> Option<Transition> {
        let active = self.active.take()?;
        let (target, resizing) = match active {
            Transition::Resizing(target) => (target, true),
            Transition::Moving(target) => (target, false),
        };
        // Trust an intervening drag only while the acknowledged shape remains.
        // A late resize can otherwise combine the new position with an old
        // size, which must not become the anchor for the queued request.
        let anchor = current
            .filter(|geometry| !resizing && geometry.same_size(target))
            .unwrap_or(target);
        let request = self.queued.take().or_else(|| {
            (resizing && !self.active_recovery).then_some(Request {
                width: target.width,
                height: target.height,
                position: Some((target.x, target.y)),
                force_resize: false,
            })
        });
        self.force_move = false;
        if let Some(mut request) = request {
            request.position = Some(request.position.unwrap_or((
                anchor.x + anchor.width - request.width,
                anchor.y + anchor.height - request.height,
            )));
            // A queued collapse may already equal native geometry while an old
            // GTK expansion remains pending. Explicitly replace that request.
            request.force_resize = true;
            self.queued = Some(request);
        }
        Some(active)
    }
}

type SharedController = Rc<RefCell<Controller>>;

thread_local! {
    // Every access runs on GTK's main thread through with_webview. Entries are
    // removed on destroy; none of these states retains the native window.
    static CONTROLLERS: RefCell<HashMap<usize, SharedController>> = RefCell::new(HashMap::new());
}

fn with_native(
    window: &WebviewWindow,
    callback: impl FnOnce(gtk::Window, SharedController) + Send + 'static,
) -> Result<(), String> {
    window
        .with_webview(move |webview| {
            let Some(native) = webview.inner().toplevel().and_downcast::<gtk::Window>() else {
                return;
            };
            let key = native.as_ptr() as usize;
            let controller =
                CONTROLLERS.with(|controllers| controllers.borrow().get(&key).cloned());
            if let Some(controller) = controller {
                callback(native, controller);
            }
        })
        .map_err(|error| error.to_string())
}

pub(super) fn configure(window: &WebviewWindow) -> Result<(), String> {
    window
        .with_webview(|webview| {
            let Some(native) = webview.inner().toplevel().and_downcast::<gtk::Window>() else {
                return;
            };
            let key = native.as_ptr() as usize;
            let controller = Rc::new(RefCell::new(Controller::default()));
            CONTROLLERS
                .with(|controllers| controllers.borrow_mut().insert(key, controller.clone()));
            let configuring = controller.clone();
            native.connect_configure_event(move |native, _| {
                schedule_flush(native, &configuring);
                false
            });
            let mapping = controller.clone();
            native.connect_map_event(move |native, _| {
                if !native.is_mapped() {
                    return glib::Propagation::Proceed;
                }
                // GtkWidget::map only sends the mapping request. The WM can
                // still place a reopened window after that local signal. GDK's
                // map-event is the server MapNotify, after WM placement.
                mapping.borrow_mut().ready = true;
                schedule_flush(native, &mapping);
                glib::Propagation::Proceed
            });
            native.connect_unmap(move |native| {
                let mut state = controller.borrow_mut();
                if state.ready && state.active.is_none() {
                    if let Some(geometry) = Geometry::read(native) {
                        state.last_stable = Some((geometry, native.scale_factor()));
                    }
                }
                state.ready = false;
                state.active = None;
                state.force_move = false;
                drop(state);
                cancel_flush(&controller);
                cancel_observer(&controller);
                cancel_watchdog(&controller);
            });
            native.connect_destroy(move |_| {
                if let Some(controller) =
                    CONTROLLERS.with(|controllers| controllers.borrow_mut().remove(&key))
                {
                    cancel_flush(&controller);
                    cancel_observer(&controller);
                    cancel_watchdog(&controller);
                }
            });
        })
        .map_err(|error| error.to_string())
}

pub(super) fn resize_from_bottom_right(
    window: &WebviewWindow,
    width: f64,
    height: f64,
) -> Result<(), String> {
    submit(window, width, height, None)
}

pub(super) fn place(
    window: &WebviewWindow,
    width: f64,
    height: f64,
    position: PhysicalPosition<i32>,
) -> Result<(), String> {
    submit(window, width, height, Some(position))
}

fn submit(
    window: &WebviewWindow,
    width: f64,
    height: f64,
    position: Option<PhysicalPosition<i32>>,
) -> Result<(), String> {
    with_native(window, move |native, controller| {
        let scale = f64::from(native.scale_factor());
        let request = Request {
            width: width.round() as i32,
            height: height.round() as i32,
            force_resize: false,
            position: position.map(|point| {
                (
                    (f64::from(point.x) / scale).round() as i32,
                    (f64::from(point.y) / scale).round() as i32,
                )
            }),
        };
        controller.borrow_mut().queued = Some(request);
        if !native.is_mapped()
            && let Some((x, y)) = request.position
        {
            // Preserve the GTK user-position hint before MapRequest, as the
            // original Tauri set_position path did. Confirm it after MapNotify.
            native.move_(x, y);
        }
        schedule_flush(&native, &controller);
    })
}

fn target(window: &gtk::Window, current: Geometry, request: Request) -> Geometry {
    let (x, y) = request.position.unwrap_or((
        current.x + current.width - request.width,
        current.y + current.height - request.height,
    ));
    let mut result = Geometry {
        x,
        y,
        width: request.width,
        height: request.height,
    };
    let display = window.display();
    let area = (0..display.n_monitors())
        .filter_map(|index| display.monitor(index))
        .map(|monitor| monitor.workarea())
        .max_by_key(|area| {
            let width = ((result.x + result.width).min(area.x() + area.width())
                - result.x.max(area.x()))
            .max(0);
            let height = ((result.y + result.height).min(area.y() + area.height())
                - result.y.max(area.y()))
            .max(0);
            i64::from(width) * i64::from(height)
        });
    if let Some(area) = area {
        result.x = result.x.clamp(
            area.x(),
            (area.x() + area.width() - result.width).max(area.x()),
        );
        result.y = result.y.clamp(
            area.y(),
            (area.y() + area.height() - result.height).max(area.y()),
        );
    }
    result
}

fn cancel_flush(controller: &SharedController) {
    let idle = controller.borrow_mut().idle.take();
    if let Some(idle) = idle {
        idle.remove();
    }
}

fn schedule_flush(window: &gtk::Window, controller: &SharedController) {
    if !window.is_mapped() || !controller.borrow().ready || controller.borrow().idle.is_some() {
        return;
    }
    let weak_window = window.downgrade();
    let flushing = controller.clone();
    let idle = glib::idle_add_local_once(move || {
        // Leave GTK's Configure default handler before consuming its ACK or
        // submitting another size. Coalesce bursts into one bounded idle.
        flushing.borrow_mut().idle = None;
        let Some(window) = weak_window.upgrade() else {
            return;
        };
        if !window.is_mapped() || !flushing.borrow().ready {
            return;
        }
        let active = flushing.borrow().active;
        let replacement = match active {
            Some(Transition::Resizing(target)) | Some(Transition::Moving(target)) => {
                Geometry::read(&window)
                    .filter(|current| !current.same_size(target))
                    .map(|_| (target.width, target.height))
            }
            None => flushing
                .borrow()
                .queued
                .filter(|request| request.force_resize)
                .map(|request| (request.width, request.height)),
        };
        if let Some((width, height)) = replacement {
            // GTK clears its explicit size after layout; a late Configure can
            // restore an obsolete allocation. Reassert our unacknowledged size
            // before flushing, without restarting its acknowledgement timeout.
            window.resize(width, height);
        }
        // GTK's first pass can consume configure_notify_received and requeue
        // a changed request without sending it. The second submits that request
        // even when a covered window's frame clock does not run layout.
        window.check_resize();
        window.check_resize();
        advance(&window, &flushing);
    });
    controller.borrow_mut().idle = Some(idle);
}

fn advance(window: &gtk::Window, controller: &SharedController) {
    if !window.is_mapped() || !controller.borrow().ready {
        return;
    }
    // Never combine Tao's independently updated position and size caches.
    // Keep one target until GTK acknowledges BOTH halves, then start the
    // latest queued request from that complete native geometry.
    loop {
        let Some(current) = Geometry::read(window) else {
            return;
        };
        let step = controller
            .borrow_mut()
            .step(current, |request| target(window, current, request));
        if step == Step::Next || (step == Step::Wait && controller.borrow().active.is_none()) {
            controller.borrow_mut().last_stable = Some((current, window.scale_factor()));
        }
        match step {
            Step::Wait => return,
            Step::Next => {
                cancel_observer(controller);
                cancel_watchdog(controller);
                continue;
            }
            Step::Apply(Transition::Resizing(target)) => {
                // Grow before moving so expansion keeps the pointer inside.
                arm_watchdog(window, controller);
                window.resize(target.width, target.height);
                // resize() queues frame-clock layout. Flush it now so an
                // occluded window submits its request before the ACK timeout.
                window.check_resize();
                return;
            }
            Step::Apply(Transition::Moving(target)) => {
                let force_move = controller.borrow_mut().force_move;
                controller.borrow_mut().force_move = false;
                if force_move || current.x != target.x || current.y != target.y {
                    arm_watchdog(window, controller);
                    window.move_(target.x, target.y);
                    return;
                }
            }
        }
    }
}

fn cancel_watchdog(controller: &SharedController) {
    let watchdog = controller.borrow_mut().watchdog.take();
    if let Some(watchdog) = watchdog {
        watchdog.remove();
    }
}

fn cancel_observer(controller: &SharedController) {
    let observer = controller.borrow_mut().observer.take();
    if let Some(observer) = observer {
        observer.remove();
    }
}

fn arm_observer(window: &gtk::Window, controller: &SharedController) {
    if controller.borrow().observer.is_some() {
        return;
    }
    let weak_window = window.downgrade();
    let observing = controller.clone();
    let observer = glib::timeout_add_local(Duration::from_millis(30), move || {
        let Some(window) = weak_window.upgrade() else {
            observing.borrow_mut().observer = None;
            return glib::ControlFlow::Break;
        };
        let active = {
            let state = observing.borrow();
            state.ready && state.active.is_some()
        };
        if !window.is_mapped() || !active {
            observing.borrow_mut().observer = None;
            return glib::ControlFlow::Break;
        }
        // Native geometry can acknowledge a request without GTK delivering a
        // Configure signal. Observe only during the existing bounded timeout.
        schedule_flush(&window, &observing);
        glib::ControlFlow::Continue
    });
    controller.borrow_mut().observer = Some(observer);
}

fn arm_watchdog(window: &gtk::Window, controller: &SharedController) {
    cancel_watchdog(controller);
    arm_observer(window, controller);
    let weak_window = window.downgrade();
    let recovering = controller.clone();
    let watchdog = glib::timeout_add_local_once(Duration::from_secs(2), move || {
        // A WM can constrain a size or a user can drag before move ack. Do not
        // leave every later request blocked on an unreachable exact target.
        // This is one bounded resync, never an automatic retry loop.
        recovering.borrow_mut().watchdog = None;
        cancel_observer(&recovering);
        if let Some(window) = weak_window.upgrade() {
            let current = Geometry::read(&window);
            let acknowledged = current.is_some_and(|current| match recovering.borrow().active {
                Some(Transition::Resizing(target)) => current.same_size(target),
                Some(Transition::Moving(target)) => current == target,
                None => false,
            });
            if acknowledged {
                advance(&window, &recovering);
                return;
            }
            let active = {
                let mut state = recovering.borrow_mut();
                state.recover(current)
            };
            if active.is_some() {
                log::warn!(
                    "Linux pet geometry acknowledgement timed out: target={active:?}, native={:?}",
                    current
                );
            }
            schedule_flush(&window, &recovering);
        }
    });
    controller.borrow_mut().watchdog = Some(watchdog);
}

pub(super) async fn stable_geometry(
    window: &WebviewWindow,
) -> Option<(PhysicalPosition<i32>, PhysicalSize<u32>, f64)> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    with_native(window, move |native, controller| {
        let state = controller.borrow();
        // A collapse queued while hidden has not changed native geometry.
        // Preserve a drag even if hiding happened before its debounce elapsed.
        // An absolute placement queued by show_pet must finish first.
        let geometry = if state.active.is_none()
            && state
                .queued
                .is_none_or(|request| request.position.is_none())
        {
            let geometry = if state.ready && native.is_mapped() {
                Geometry::read(&native).map(|geometry| (geometry, native.scale_factor()))
            } else {
                // GTK can report (0,0) while hidden. Keep the last acknowledged
                // anchor so a quick hide does not defeat drag persistence.
                state.last_stable
            };
            geometry.map(|(geometry, scale)| {
                (
                    PhysicalPosition::new(geometry.x * scale, geometry.y * scale),
                    PhysicalSize::new(
                        (geometry.width * scale) as u32,
                        (geometry.height * scale) as u32,
                    ),
                    f64::from(scale),
                )
            })
        } else {
            None
        };
        let _ = sender.send(geometry);
    })
    .ok()?;
    receiver.await.ok().flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anchored(current: Geometry, request: Request) -> Geometry {
        let (x, y) = request.position.unwrap_or((
            current.x + current.width - request.width,
            current.y + current.height - request.height,
        ));
        Geometry {
            x,
            y,
            width: request.width,
            height: request.height,
        }
    }

    #[test]
    fn collapse_waits_for_complete_expansion_instead_of_using_hybrid_geometry() {
        let collapsed = Geometry {
            x: 400,
            y: 400,
            width: 200,
            height: 200,
        };
        let expanded = Geometry {
            x: 290,
            y: 210,
            width: 310,
            height: 390,
        };
        let mut state = Controller {
            ready: true,
            queued: Some(Request {
                width: 310,
                height: 390,
                position: None,
                force_resize: false,
            }),
            ..Controller::default()
        };
        assert_eq!(
            state.step(collapsed, |request| anchored(collapsed, request)),
            Step::Apply(Transition::Resizing(expanded))
        );
        state.queued = Some(Request {
            width: 200,
            height: 200,
            position: None,
            force_resize: false,
        });
        // Reproduces the cached shape/position combination seen under CPU load.
        let hybrid = Geometry {
            x: 290,
            y: 210,
            ..collapsed
        };
        assert_eq!(
            state.step(hybrid, |_| panic!("queued collapse ran too early")),
            Step::Wait
        );
        let size_ack = Geometry {
            x: 400,
            y: 400,
            ..expanded
        };
        assert_eq!(
            state.step(size_ack, |_| unreachable!()),
            Step::Apply(Transition::Moving(expanded))
        );
        assert_eq!(state.step(expanded, |_| unreachable!()), Step::Next);
        assert_eq!(
            state.step(expanded, |request| anchored(expanded, request)),
            Step::Apply(Transition::Resizing(collapsed))
        );
    }

    #[test]
    fn a_settled_drag_becomes_the_anchor_of_the_next_request() {
        let dragged = Geometry {
            x: 100,
            y: 120,
            width: 310,
            height: 390,
        };
        let mut state = Controller {
            ready: true,
            queued: Some(Request {
                width: 200,
                height: 200,
                position: None,
                force_resize: false,
            }),
            ..Controller::default()
        };
        assert_eq!(
            state.step(dragged, |request| anchored(dragged, request)),
            Step::Apply(Transition::Resizing(Geometry {
                x: 210,
                y: 310,
                width: 200,
                height: 200
            }))
        );
    }

    #[test]
    fn reopening_cannot_acknowledge_the_old_position_before_server_map_notify() {
        let saved = Geometry {
            x: 600,
            y: 500,
            width: 200,
            height: 200,
        };
        let mut state = Controller {
            queued: Some(Request {
                width: 200,
                height: 200,
                position: Some((600, 500)),
                force_resize: false,
            }),
            ..Controller::default()
        };
        // GtkWidget::map can still expose the old server coordinates before
        // the WM processes MapRequest and applies its own placement.
        assert_eq!(
            state.step(saved, |_| panic!("placement consumed before MapNotify")),
            Step::Wait
        );
        assert!(state.queued.is_some());
        state.ready = true;
        assert_eq!(
            state.step(saved, |_| saved),
            Step::Apply(Transition::Moving(saved))
        );
        // Even matching coordinates require an explicit post-map move request.
        assert!(state.force_move);
    }

    #[test]
    fn timeout_replaces_late_expansion_even_when_queued_collapse_matches_native_size() {
        let collapsed = Geometry {
            x: 400,
            y: 400,
            width: 200,
            height: 200,
        };
        let expanded = Geometry {
            x: 290,
            y: 210,
            width: 310,
            height: 390,
        };
        let mut state = Controller {
            ready: true,
            active: Some(Transition::Resizing(expanded)),
            queued: Some(Request {
                width: 200,
                height: 200,
                position: None,
                force_resize: false,
            }),
            ..Controller::default()
        };
        state.recover(Some(collapsed));
        assert!(state.queued.unwrap().force_resize);
        assert_eq!(state.queued.unwrap().position, Some((400, 400)));
        assert_eq!(
            state.step(collapsed, |request| anchored(collapsed, request)),
            Step::Apply(Transition::Resizing(collapsed))
        );
        // A recovery attempt without a new user request never retries itself.
        assert!(state.active_recovery);
        state.recover(Some(collapsed));
        assert!(state.active.is_none());
        assert!(state.queued.is_none());
    }

    #[test]
    fn late_resize_during_move_does_not_shift_the_queued_collapse_anchor() {
        let expanded = Geometry {
            x: 290,
            y: 110,
            width: 310,
            height: 390,
        };
        let late_collapse = Geometry {
            width: 200,
            height: 200,
            ..expanded
        };
        let expected = Geometry {
            x: 400,
            y: 300,
            ..late_collapse
        };
        let mut state = Controller {
            ready: true,
            active: Some(Transition::Moving(expanded)),
            queued: Some(Request {
                width: 200,
                height: 200,
                position: None,
                force_resize: false,
            }),
            ..Controller::default()
        };
        state.recover(Some(late_collapse));
        assert_eq!(state.queued.unwrap().position, Some((400, 300)));
        assert_eq!(
            state.step(late_collapse, |request| anchored(late_collapse, request)),
            Step::Apply(Transition::Resizing(expected))
        );
    }
}
