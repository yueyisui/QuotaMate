mod app_state;
mod codex;
mod commands;
mod config;
mod scheduler;
mod tray;
mod windows;

use std::sync::Arc;

use app_state::AppState;
use tauri::Manager;
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_log::{Target, TargetKind};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .targets([
                    Target::new(TargetKind::LogDir {
                        file_name: Some("quotamate".into()),
                    }),
                    Target::new(TargetKind::Stdout),
                ])
                .build(),
        )
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            let _ = windows::show_main(app, None);
        }))
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::get_usage,
            commands::get_config,
            commands::get_runtime_status,
            commands::save_config,
            commands::refresh_usage,
            commands::show_window,
            commands::hide_menu_bar_panel,
            commands::show_floating_context_menu,
            commands::set_compact_expanded,
            commands::set_pet_expanded,
            commands::disable_display,
            commands::import_pet_image,
            commands::get_pet_image_data,
            commands::get_pet_image_history,
            commands::select_pet_image,
            commands::delete_pet_image,
            commands::open_log_folder,
            commands::app_version,
        ])
        .on_menu_event(tray::handle_context_menu_event)
        .setup(|app| {
            let config = config::load(app.handle());
            if let Err(error) = config::save(app.handle(), &config) {
                log::warn!("Unable to persist normalized configuration: {error}");
            }
            let state = Arc::new(AppState::new(config.clone()));
            app.manage(state.clone());

            if config.launch_at_startup {
                if let Err(error) = app.autolaunch().enable() {
                    log::warn!("Unable to restore startup registration: {error}");
                }
            }

            tray::create(app, &config)?;
            if let Some(window) = app.get_webview_window("main") {
                windows::attach_main_close_behavior(&window);
                let start_minimized = config.start_minimized
                    || std::env::args().any(|argument| argument == "--minimized");
                if start_minimized {
                    let _ = window.hide();
                }
            }
            if let Err(error) = windows::apply_display_config(app.handle(), &config) {
                log::warn!("Unable to restore display windows: {error}");
            }

            let app_handle = app.handle().clone();
            let codex_state = state.clone();
            tauri::async_runtime::spawn(async move {
                codex::app_server::start(app_handle, codex_state).await;
            });
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                scheduler::start(app_handle, state).await;
            });
            log::info!("QuotaMate {} started", env!("CARGO_PKG_VERSION"));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
