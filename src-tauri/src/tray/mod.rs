use std::{sync::Arc, time::Duration};

use tauri::{
    App, AppHandle, Emitter, Manager,
    menu::{ContextMenu, Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use tauri_plugin_autostart::ManagerExt;

use crate::{
    app_state::{AppState, CodexControl},
    codex::usage::CodexUsage,
    config::AppConfig,
    windows,
};

pub struct TrayMenuState {
    open: MenuItem<tauri::Wry>,
    display_header: MenuItem<tauri::Wry>,
    compact: MenuItem<tauri::Wry>,
    pet: MenuItem<tauri::Wry>,
    hidden: MenuItem<tauri::Wry>,
    startup: MenuItem<tauri::Wry>,
    refresh: MenuItem<tauri::Wry>,
    scheduler: MenuItem<tauri::Wry>,
    settings: MenuItem<tauri::Wry>,
    exit: MenuItem<tauri::Wry>,
}

pub fn create(app: &App, config: &AppConfig) -> tauri::Result<()> {
    let show_usage = MenuItem::with_id(app, "show_usage", "Open QuotaMate", true, None::<&str>)?;
    let display_header = MenuItem::with_id(
        app,
        "display_header",
        "Desktop display",
        false,
        None::<&str>,
    )?;
    let show_compact = MenuItem::with_id(app, "show_compact", "Compact mode", true, None::<&str>)?;
    let show_pet = MenuItem::with_id(app, "show_pet", "Desktop pet", true, None::<&str>)?;
    let hide_display = MenuItem::with_id(app, "hide_display", "Hidden", true, None::<&str>)?;
    let startup = MenuItem::with_id(
        app,
        "toggle_startup",
        "Launch at startup",
        true,
        None::<&str>,
    )?;
    let scheduler = MenuItem::with_id(app, "scheduler", "Scheduler", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let refresh = MenuItem::with_id(app, "refresh", "Refresh usage", true, None::<&str>)?;
    let exit = MenuItem::with_id(app, "exit", "Exit", true, None::<&str>)?;
    let separator_1 = PredefinedMenuItem::separator(app)?;
    let separator_2 = PredefinedMenuItem::separator(app)?;
    let separator_3 = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(
        app,
        &[
            &show_usage,
            &separator_1,
            &display_header,
            &show_compact,
            &show_pet,
            &hide_display,
            &separator_2,
            &startup,
            &refresh,
            &scheduler,
            &settings,
            &separator_3,
            &exit,
        ],
    )?;

    app.manage(TrayMenuState {
        open: show_usage,
        display_header,
        compact: show_compact,
        pet: show_pet,
        hidden: hide_display,
        startup,
        refresh,
        scheduler,
        settings,
        exit,
    });
    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or_else(|| tauri::Error::AssetNotFound("default window icon".into()))?;
    TrayIconBuilder::with_id("main-tray")
        .icon(icon)
        .icon_as_template(cfg!(target_os = "macos"))
        .tooltip("Codex\nConnecting…")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show_usage" => {
                let _ = windows::show_main(app, Some("usage"));
            }
            "show_compact" => {
                select_display(app, "compact");
            }
            "show_pet" => {
                select_display(app, "pet");
            }
            "hide_display" => {
                select_display(app, "hidden");
            }
            "scheduler" => {
                let _ = windows::show_main(app, Some("scheduler"));
            }
            "settings" => {
                let _ = windows::show_main(app, Some("settings"));
            }
            "refresh" => request_refresh(app),
            "toggle_startup" => toggle_startup(app),
            "exit" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    if let Some(sender) = app
                        .state::<Arc<AppState>>()
                        .codex_control
                        .read()
                        .await
                        .clone()
                    {
                        let _ = sender.send(CodexControl::Shutdown).await;
                    }
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    app.exit(0);
                });
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                rect,
                ..
            } = event
            {
                #[cfg(target_os = "macos")]
                let _ = windows::toggle_menu_bar_panel(tray.app_handle(), rect);
                #[cfg(not(target_os = "macos"))]
                let _ = windows::show_main(tray.app_handle(), Some("usage"));
            }
        })
        .build(app)?;
    sync_menu(app.handle(), config);
    Ok(())
}

pub fn sync_menu(app: &AppHandle, config: &AppConfig) {
    let Some(items) = app.try_state::<TrayMenuState>() else {
        return;
    };
    let chinese = config.uses_chinese();
    let marker = |active| if active { "●" } else { "○" };
    let hidden = !config.compact_widget_enabled && !config.pet_enabled;
    let compact_label = if cfg!(target_os = "macos") {
        if chinese {
            "菜单栏额度"
        } else {
            "Menu bar quota"
        }
    } else if chinese {
        "简洁模式"
    } else {
        "Compact mode"
    };
    let display_label = if cfg!(target_os = "macos") {
        if chinese {
            "菜单栏与桌面显示"
        } else {
            "Menu bar & desktop display"
        }
    } else if chinese {
        "桌面显示模式"
    } else {
        "Desktop display"
    };
    let labels = if chinese {
        (
            "打开 QuotaMate",
            display_label,
            compact_label,
            "桌面宠物",
            "不显示",
            "开机自启动",
            "刷新额度",
            "计划任务",
            "设置",
            "退出",
        )
    } else {
        (
            "Open QuotaMate",
            display_label,
            compact_label,
            "Desktop pet",
            "Hidden",
            "Launch at startup",
            "Refresh usage",
            "Scheduler",
            "Settings",
            "Exit",
        )
    };
    let _ = items.open.set_text(labels.0);
    let _ = items.display_header.set_text(labels.1);
    let _ = items.compact.set_text(format!(
        "{} {}",
        marker(config.compact_widget_enabled),
        labels.2
    ));
    let _ = items
        .pet
        .set_text(format!("{} {}", marker(config.pet_enabled), labels.3));
    let _ = items
        .hidden
        .set_text(format!("{} {}", marker(hidden), labels.4));
    let _ = items
        .startup
        .set_text(format!("{} {}", marker(config.launch_at_startup), labels.5));
    let _ = items.refresh.set_text(labels.6);
    let _ = items.scheduler.set_text(labels.7);
    let _ = items.settings.set_text(labels.8);
    let _ = items.exit.set_text(labels.9);

    // The macOS compact display lives in the menu bar. Refresh its text
    // immediately when the user changes which quota windows are visible.
    #[cfg(target_os = "macos")]
    {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let Some(state) = app.try_state::<Arc<AppState>>() else {
                return;
            };
            let usage = state.usage.read().await.clone();
            let config = state.config.read().await.clone();
            update_usage(&app, &usage, &config);
        });
    }
}

pub fn show_floating_context_menu(
    app: &AppHandle,
    config: &AppConfig,
    window_label: &str,
) -> Result<(), String> {
    if !matches!(window_label, "compact" | "pet") {
        return Err("Context menu is only available for floating displays".into());
    }
    let chinese = config.uses_chinese();
    let marker = |active| if active { "●" } else { "○" };
    let compact_label = if cfg!(target_os = "macos") {
        if chinese {
            "菜单栏额度"
        } else {
            "Menu bar quota"
        }
    } else if chinese {
        "简洁模式"
    } else {
        "Compact mode"
    };
    let labels = if chinese {
        ("打开主界面", compact_label, "桌面宠物", "关闭浮窗")
    } else {
        (
            "Open QuotaMate",
            compact_label,
            "Desktop pet",
            "Close floating display",
        )
    };
    let open = MenuItem::with_id(app, "floating_open_main", labels.0, true, None::<&str>)
        .map_err(|error| error.to_string())?;
    let compact = MenuItem::with_id(
        app,
        "floating_compact",
        format!("{} {}", marker(config.compact_widget_enabled), labels.1),
        true,
        None::<&str>,
    )
    .map_err(|error| error.to_string())?;
    let pet = MenuItem::with_id(
        app,
        "floating_pet",
        format!("{} {}", marker(config.pet_enabled), labels.2),
        true,
        None::<&str>,
    )
    .map_err(|error| error.to_string())?;
    let close = MenuItem::with_id(app, "floating_close", labels.3, true, None::<&str>)
        .map_err(|error| error.to_string())?;
    let separator = PredefinedMenuItem::separator(app).map_err(|error| error.to_string())?;
    let menu = Menu::with_items(app, &[&open, &separator, &compact, &pet, &close])
        .map_err(|error| error.to_string())?;
    let window = app
        .get_webview_window(window_label)
        .ok_or_else(|| "Floating display is unavailable".to_string())?;
    menu.popup(window.as_ref().window())
        .map_err(|error| error.to_string())
}

pub fn handle_context_menu_event(app: &AppHandle, event: MenuEvent) {
    match event.id().as_ref() {
        "floating_open_main" => {
            let _ = windows::show_main(app, Some("usage"));
        }
        "floating_compact" => select_display(app, "compact"),
        "floating_pet" => select_display(app, "pet"),
        "floating_close" => select_display(app, "hidden"),
        _ => {}
    }
}

pub fn update_usage(app: &AppHandle, usage: &CodexUsage, config: &AppConfig) {
    let five_hour = usage
        .five_hour
        .as_ref()
        .map(|window| format!("{:.0}%", window.remaining_percent))
        .unwrap_or_else(|| "—".into());
    let weekly = usage
        .weekly
        .as_ref()
        .map(|window| format!("{:.0}%", window.remaining_percent))
        .unwrap_or_else(|| "—".into());
    if let Some(tray) = app.tray_by_id("main-tray") {
        let _ = tray.set_tooltip(Some(format!("Codex\n5h: {five_hour}\nW: {weekly}")));
        #[cfg(target_os = "macos")]
        {
            let title = compact_menu_bar_title(config, &five_hour, &weekly);
            // Passing None means "leave the native title unset" and can retain
            // the previous title on macOS. An explicit empty title clears it.
            let _ = tray.set_title(Some(title));
        }
    }
}

#[cfg(target_os = "macos")]
fn compact_menu_bar_title(config: &AppConfig, five_hour: &str, weekly: &str) -> String {
    if !config.compact_widget_enabled {
        return String::new();
    }
    let mut values = Vec::with_capacity(2);
    if config.compact_show_five_hour {
        values.push(format!("5h {five_hour}"));
    }
    if config.compact_show_weekly {
        values.push(format!("W {weekly}"));
    }
    values.join(" · ")
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::compact_menu_bar_title;
    use crate::config::AppConfig;

    #[test]
    fn menu_bar_title_only_appears_in_compact_mode() {
        let mut config = AppConfig::default();
        config.compact_widget_enabled = true;
        config.pet_enabled = false;
        assert_eq!(
            compact_menu_bar_title(&config, "80%", "65%"),
            "5h 80% · W 65%"
        );

        config.compact_widget_enabled = false;
        config.pet_enabled = true;
        assert_eq!(compact_menu_bar_title(&config, "80%", "65%"), "");

        config.pet_enabled = false;
        assert_eq!(compact_menu_bar_title(&config, "80%", "65%"), "");
    }
}

fn request_refresh(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Some(sender) = app
            .state::<Arc<AppState>>()
            .codex_control
            .read()
            .await
            .clone()
        {
            let _ = sender.send(CodexControl::Refresh).await;
        }
    });
}

fn select_display(app: &AppHandle, mode: &'static str) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<Arc<AppState>>();
        let snapshot = {
            let mut config = state.config.write().await;
            config.compact_widget_enabled = mode == "compact";
            config.pet_enabled = mode == "pet";
            config.clone()
        };
        if let Err(error) = crate::config::save(&app, &snapshot) {
            log::warn!("Unable to save desktop display mode: {error}");
        }
        if let Err(error) = windows::apply_display_config(&app, &snapshot) {
            log::warn!("Unable to apply desktop display mode: {error}");
        }
        sync_menu(&app, &snapshot);
        let _ = app.emit("config-updated", snapshot);
    });
}

fn toggle_startup(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<Arc<AppState>>();
        let enabled = !state.config.read().await.launch_at_startup;
        let result = if enabled {
            app.autolaunch().enable()
        } else {
            app.autolaunch().disable()
        };
        if let Err(error) = result {
            log::warn!("Unable to update startup registration: {error}");
            return;
        }
        let snapshot = {
            let mut config = state.config.write().await;
            config.launch_at_startup = enabled;
            config.clone()
        };
        if let Err(error) = crate::config::save(&app, &snapshot) {
            log::warn!("Unable to save startup preference: {error}");
        }
        sync_menu(&app, &snapshot);
        let _ = app.emit("config-updated", snapshot);
    });
}
