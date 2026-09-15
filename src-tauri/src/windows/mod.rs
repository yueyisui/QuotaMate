use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, Position, Rect, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, WindowEvent, window::Color,
};

use crate::{
    app_state::AppState,
    config::{self, AppConfig, WindowPosition},
};

pub fn show_main(app: &AppHandle, page: Option<&str>) -> Result<(), String> {
    hide_menu_bar_panel(app)?;
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "Main window is unavailable".to_string())?;
    window.show().map_err(|error| error.to_string())?;
    window.unminimize().map_err(|error| error.to_string())?;
    window.set_focus().map_err(|error| error.to_string())?;
    if let Some(page) = page {
        let _ = app.emit_to("main", "navigate", page);
    }
    Ok(())
}

#[cfg(target_os = "macos")]
pub fn toggle_menu_bar_panel(app: &AppHandle, tray_rect: Rect) -> Result<(), String> {
    const PANEL_WIDTH: f64 = 390.0;
    const PANEL_HEIGHT: f64 = 500.0;

    let panel = if let Some(window) = app.get_webview_window("menu-panel") {
        if window.is_visible().map_err(|error| error.to_string())? {
            return window.hide().map_err(|error| error.to_string());
        }
        window
    } else {
        let window = WebviewWindowBuilder::new(
            app,
            "menu-panel",
            WebviewUrl::App("index.html?window=menu-panel".into()),
        )
        .title("QuotaMate Usage")
        .inner_size(PANEL_WIDTH, PANEL_HEIGHT)
        .resizable(false)
        .decorations(false)
        .transparent(true)
        .background_color(Color(0, 0, 0, 0))
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .build()
        .map_err(|error| error.to_string())?;
        attach_menu_bar_panel_behavior(&window);
        window
    };

    let scale = panel.scale_factor().map_err(|error| error.to_string())?;
    let tray_position = tray_rect.position.to_physical::<i32>(scale);
    let tray_size = tray_rect.size.to_physical::<u32>(scale);
    let panel_size = panel.outer_size().map_err(|error| error.to_string())?;
    let requested = PhysicalPosition::new(
        tray_position.x + tray_size.width as i32 - panel_size.width as i32,
        tray_position.y + tray_size.height as i32 + (6.0 * scale).round() as i32,
    );
    let position = safe_initial_position(&panel, requested)?;
    panel
        .set_position(Position::Physical(position))
        .map_err(|error| error.to_string())?;
    panel.show().map_err(|error| error.to_string())?;
    panel.set_focus().map_err(|error| error.to_string())
}

#[cfg(not(target_os = "macos"))]
pub fn toggle_menu_bar_panel(app: &AppHandle, _tray_rect: Rect) -> Result<(), String> {
    show_main(app, Some("usage"))
}

pub fn hide_menu_bar_panel(app: &AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("menu-panel") {
        window.hide().map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub fn show_compact(app: &AppHandle, config: &AppConfig) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        // On macOS the compact mode is rendered as live text beside the menu
        // bar icon, so no desktop WebView window should be created.
        let _ = config;
        return hide_auxiliary(app, "compact");
    }

    #[cfg(not(target_os = "macos"))]
    {
        let (width, height) = compact_collapsed_size(config);
        let _ = app.emit_to("compact", "compact-collapse", ());
        let window = if let Some(window) = app.get_webview_window("compact") {
            window
                .set_size(tauri::LogicalSize::new(width, height))
                .map_err(|error| error.to_string())?;
            window
        } else {
            let window = WebviewWindowBuilder::new(
                app,
                "compact",
                WebviewUrl::App("index.html?window=compact".into()),
            )
            .title("QuotaMate Compact")
            .inner_size(width, height)
            .resizable(false)
            .decorations(false)
            .transparent(true)
            .always_on_top(true)
            .skip_taskbar(true)
            .visible(false)
            .build()
            .map_err(|error| error.to_string())?;
            attach_hide_on_close(&window);
            attach_position_persistence(&window, "compact");
            window
        };
        apply_position(&window, config.compact_position.as_ref(), 20, 82)?;
        window.show().map_err(|error| error.to_string())
    }
}

pub fn set_compact_expanded(
    app: &AppHandle,
    config: &AppConfig,
    expanded: bool,
) -> Result<(), String> {
    let Some(window) = app.get_webview_window("compact") else {
        return Ok(());
    };
    let (width, height) = if expanded {
        (410.0, 540.0)
    } else {
        compact_collapsed_size(config)
    };
    let scale = window.scale_factor().map_err(|error| error.to_string())?;
    let old_size = window.outer_size().map_err(|error| error.to_string())?;
    let old_position = window.outer_position().map_err(|error| error.to_string())?;
    let new_width = (width * scale).round() as i32;
    let new_height = (height * scale).round() as i32;
    let anchored_position = PhysicalPosition::new(
        old_position.x + old_size.width as i32 - new_width,
        old_position.y + old_size.height as i32 - new_height,
    );
    window
        .set_position(Position::Physical(anchored_position))
        .map_err(|error| error.to_string())?;
    window
        .set_size(tauri::LogicalSize::new(width, height))
        .map_err(|error| error.to_string())
}

pub fn show_pet(app: &AppHandle, config: &AppConfig) -> Result<(), String> {
    let (width, height) = pet_collapsed_size(config);
    let _ = app.emit_to("pet", "pet-collapse", ());
    let window = if let Some(window) = app.get_webview_window("pet") {
        window
            .set_size(tauri::LogicalSize::new(width, height))
            .map_err(|error| error.to_string())?;
        window
    } else {
        let window =
            WebviewWindowBuilder::new(app, "pet", WebviewUrl::App("index.html?window=pet".into()))
                .title("QuotaMate Pet")
                .inner_size(width, height)
                .min_inner_size(140.0, 170.0)
                .resizable(false)
                .decorations(false)
                .transparent(true)
                .background_color(Color(0, 0, 0, 0))
                .shadow(false)
                .always_on_top(config.pet_always_on_top)
                .skip_taskbar(true)
                .visible(false)
                .build()
                .map_err(|error| error.to_string())?;
        attach_hide_on_close(&window);
        attach_position_persistence(&window, "pet");
        window
    };
    window
        .set_always_on_top(config.pet_always_on_top)
        .map_err(|error| error.to_string())?;
    window
        .set_background_color(Some(Color(0, 0, 0, 0)))
        .map_err(|error| error.to_string())?;
    window
        .set_shadow(false)
        .map_err(|error| error.to_string())?;
    apply_position(&window, config.pet_position.as_ref(), 28, 90)?;
    window.show().map_err(|error| error.to_string())
}

pub fn set_pet_expanded(app: &AppHandle, config: &AppConfig, expanded: bool) -> Result<(), String> {
    let Some(window) = app.get_webview_window("pet") else {
        return Ok(());
    };
    let (width, height) = if expanded {
        (310.0, 390.0)
    } else {
        pet_collapsed_size(config)
    };
    resize_from_bottom_right(&window, width, height)
}

pub fn apply_display_config(app: &AppHandle, config: &AppConfig) -> Result<(), String> {
    if config.compact_widget_enabled {
        show_compact(app, config)?;
    } else {
        hide_auxiliary(app, "compact")?;
    }

    if config.pet_enabled {
        show_pet(app, config)?;
    } else {
        hide_auxiliary(app, "pet")?;
    }
    Ok(())
}

fn attach_menu_bar_panel_behavior(window: &WebviewWindow) {
    let panel = window.clone();
    window.clone().on_window_event(move |event| match event {
        WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            let _ = panel.hide();
        }
        WindowEvent::Focused(false) => {
            let _ = panel.hide();
        }
        _ => {}
    });
}

pub fn attach_main_close_behavior(window: &WebviewWindow) {
    let window = window.clone();
    window.clone().on_window_event(move |event| match event {
        WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            let _ = window.hide();
        }
        _ => {}
    });
}

fn compact_collapsed_size(config: &AppConfig) -> (f64, f64) {
    let visible_values =
        usize::from(config.compact_show_five_hour) + usize::from(config.compact_show_weekly);
    (if visible_values > 1 { 232.0 } else { 150.0 }, 58.0)
}

fn pet_collapsed_size(config: &AppConfig) -> (f64, f64) {
    (220.0 * config.pet_scale, 270.0 * config.pet_scale)
}

fn resize_from_bottom_right(window: &WebviewWindow, width: f64, height: f64) -> Result<(), String> {
    let scale = window.scale_factor().map_err(|error| error.to_string())?;
    let old_size = window.outer_size().map_err(|error| error.to_string())?;
    let old_position = window.outer_position().map_err(|error| error.to_string())?;
    let new_width = (width * scale).round() as i32;
    let new_height = (height * scale).round() as i32;
    window
        .set_position(Position::Physical(PhysicalPosition::new(
            old_position.x + old_size.width as i32 - new_width,
            old_position.y + old_size.height as i32 - new_height,
        )))
        .map_err(|error| error.to_string())?;
    window
        .set_size(tauri::LogicalSize::new(width, height))
        .map_err(|error| error.to_string())
}

fn attach_hide_on_close(window: &WebviewWindow) {
    let window = window.clone();
    window.clone().on_window_event(move |event| {
        if let WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            let _ = window.hide();
        }
    });
}

fn hide_auxiliary(app: &AppHandle, label: &str) -> Result<(), String> {
    if label == "compact" {
        let _ = app.emit_to("compact", "compact-collapse", ());
    } else if label == "pet" {
        let _ = app.emit_to("pet", "pet-collapse", ());
    }
    if let Some(window) = app.get_webview_window(label) {
        window.hide().map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn attach_position_persistence(window: &WebviewWindow, kind: &'static str) {
    let app = window.app_handle().clone();
    let tracked_window = window.clone();
    let move_generation = Arc::new(AtomicU64::new(0));
    window.on_window_event(move |event| {
        if !matches!(event, WindowEvent::Moved(_)) {
            return;
        }
        // Do not perform I/O or reposition the window while the OS is delivering
        // drag events. Persist only the final position after movement settles.
        let generation = move_generation.fetch_add(1, Ordering::Relaxed) + 1;
        let move_generation = move_generation.clone();
        let app = app.clone();
        let tracked_window = tracked_window.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_millis(280)).await;
            if move_generation.load(Ordering::Relaxed) != generation {
                return;
            }
            let Ok(current_position) = tracked_window.outer_position() else {
                return;
            };
            let outer_size = tracked_window.outer_size().ok();
            let scale_factor = tracked_window.scale_factor().ok();
            let state = app.state::<Arc<AppState>>();
            let snapshot = {
                let mut current = state.config.write().await;
                let mut position = WindowPosition {
                    x: current_position.x,
                    y: current_position.y,
                };
                match kind {
                    "compact" => {
                        if let (Some(size), Some(scale)) = (outer_size, scale_factor) {
                            let (collapsed_width, collapsed_height) =
                                compact_collapsed_size(&current);
                            position.x +=
                                size.width as i32 - (collapsed_width * scale).round() as i32;
                            position.y +=
                                size.height as i32 - (collapsed_height * scale).round() as i32;
                        }
                        current.compact_position = Some(position);
                    }
                    "pet" => {
                        if let (Some(size), Some(scale)) = (outer_size, scale_factor) {
                            let (collapsed_width, collapsed_height) = pet_collapsed_size(&current);
                            position.x +=
                                size.width as i32 - (collapsed_width * scale).round() as i32;
                            position.y +=
                                size.height as i32 - (collapsed_height * scale).round() as i32;
                        }
                        current.pet_position = Some(position);
                    }
                    _ => return,
                }
                current.clone()
            };
            if let Err(error) = config::save(&app, &snapshot) {
                log::warn!("Unable to save {kind} window position: {error}");
            }
        });
    });
}

fn apply_position(
    window: &WebviewWindow,
    saved: Option<&WindowPosition>,
    right_margin: i32,
    bottom_margin: i32,
) -> Result<(), String> {
    if let Some(position) = saved {
        let requested = PhysicalPosition::new(position.x, position.y);
        let safe = safe_initial_position(window, requested)?;
        window
            .set_position(Position::Physical(safe))
            .map_err(|error| error.to_string())
    } else {
        place_bottom_right(window, right_margin, bottom_margin)
    }
}

fn place_bottom_right(
    window: &WebviewWindow,
    right_margin: i32,
    bottom_margin: i32,
) -> Result<(), String> {
    let Some(monitor) = window
        .primary_monitor()
        .map_err(|error| error.to_string())?
    else {
        return Ok(());
    };
    let work_area = monitor.work_area();
    let monitor_position = work_area.position;
    let monitor_size = work_area.size;
    let window_size = window.outer_size().map_err(|error| error.to_string())?;
    let x = clamp_axis(
        monitor_position.x + monitor_size.width as i32 - window_size.width as i32 - right_margin,
        monitor_position.x,
        monitor_size.width,
        window_size.width,
    );
    let y = clamp_axis(
        monitor_position.y + monitor_size.height as i32 - window_size.height as i32 - bottom_margin,
        monitor_position.y,
        monitor_size.height,
        window_size.height,
    );
    window
        .set_position(Position::Physical(PhysicalPosition::new(x, y)))
        .map_err(|error| error.to_string())
}

fn safe_initial_position(
    window: &WebviewWindow,
    requested: PhysicalPosition<i32>,
) -> Result<PhysicalPosition<i32>, String> {
    let size = window.outer_size().map_err(|error| error.to_string())?;
    let monitors = window
        .available_monitors()
        .map_err(|error| error.to_string())?;
    let requested_right = requested.x.saturating_add(size.width as i32);
    let requested_bottom = requested.y.saturating_add(size.height as i32);
    let best = monitors.iter().max_by_key(|monitor| {
        let area = monitor.work_area();
        let right = area.position.x.saturating_add(area.size.width as i32);
        let bottom = area.position.y.saturating_add(area.size.height as i32);
        let overlap_width = (requested_right.min(right) - requested.x.max(area.position.x)).max(0);
        let overlap_height =
            (requested_bottom.min(bottom) - requested.y.max(area.position.y)).max(0);
        overlap_width as i64 * overlap_height as i64
    });
    let monitor = best
        .cloned()
        .or_else(|| window.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        return Ok(requested);
    };
    let area = monitor.work_area();
    Ok(PhysicalPosition::new(
        clamp_axis(requested.x, area.position.x, area.size.width, size.width),
        clamp_axis(requested.y, area.position.y, area.size.height, size.height),
    ))
}

fn clamp_axis(value: i32, origin: i32, available: u32, window: u32) -> i32 {
    let maximum = (origin + available as i32 - window as i32).max(origin);
    value.clamp(origin, maximum)
}

#[cfg(test)]
mod tests {
    use super::clamp_axis;

    #[test]
    fn clamps_windows_inside_positive_and_negative_monitor_bounds() {
        assert_eq!(clamp_axis(3941, 0, 2560, 232), 2328);
        assert_eq!(clamp_axis(-2500, -1920, 1920, 300), -1920);
        assert_eq!(clamp_axis(-400, -1920, 1920, 300), -400);
    }
}
