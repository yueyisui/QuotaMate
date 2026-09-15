use std::{fs, path::PathBuf, sync::Arc};

use base64::{Engine, engine::general_purpose::STANDARD};
use chrono::Utc;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_autostart::ManagerExt;
use uuid::Uuid;

use crate::{
    app_state::{AppState, CodexControl, RuntimeStatus},
    codex::usage::CodexUsage,
    config::{self, AppConfig, PetImageRecord},
    windows,
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PetImageHistoryEntry {
    id: String,
    imported_at: i64,
    data_url: String,
    active: bool,
}

#[tauri::command]
pub async fn get_usage(state: State<'_, Arc<AppState>>) -> Result<CodexUsage, String> {
    Ok(state.usage.read().await.clone())
}

#[tauri::command]
pub async fn get_config(state: State<'_, Arc<AppState>>) -> Result<AppConfig, String> {
    Ok(state.config.read().await.clone())
}

#[tauri::command]
pub async fn get_runtime_status(state: State<'_, Arc<AppState>>) -> Result<RuntimeStatus, String> {
    Ok(state.runtime_status.read().await.clone())
}

#[tauri::command]
pub async fn save_config(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    mut next: AppConfig,
) -> Result<AppConfig, String> {
    let current = state.config.read().await.clone();
    if next.compact_widget_enabled && next.pet_enabled {
        if next.compact_widget_enabled && !current.compact_widget_enabled {
            next.pet_enabled = false;
        } else {
            next.compact_widget_enabled = false;
        }
    }
    next.normalize();

    // Preserve scheduler-owned run metadata if a UI save races with a trigger finishing.
    for trigger in &mut next.triggers {
        if let Some(existing) = current.triggers.iter().find(|item| item.id == trigger.id) {
            if existing.last_run.unwrap_or_default() > trigger.last_run.unwrap_or_default() {
                trigger.last_run = existing.last_run;
                trigger.last_run_date.clone_from(&existing.last_run_date);
                trigger.last_status.clone_from(&existing.last_status);
                trigger.last_result.clone_from(&existing.last_result);
            }
        }
    }

    if next.launch_at_startup != current.launch_at_startup {
        if next.launch_at_startup {
            app.autolaunch()
                .enable()
                .map_err(|error| error.to_string())?;
        } else {
            app.autolaunch()
                .disable()
                .map_err(|error| error.to_string())?;
        }
    }
    config::save(&app, &next)?;
    *state.config.write().await = next.clone();
    windows::apply_display_config(&app, &next)?;
    crate::tray::sync_menu(&app, &next);
    let _ = app.emit("config-updated", next.clone());
    Ok(next)
}

#[tauri::command]
pub async fn refresh_usage(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let sender = state
        .codex_control
        .read()
        .await
        .clone()
        .ok_or_else(|| "Codex worker is not ready yet".to_string())?;
    sender
        .send(CodexControl::Refresh)
        .await
        .map_err(|_| "Codex worker has stopped".to_string())
}

#[tauri::command]
pub fn show_window(app: AppHandle, window: String) -> Result<(), String> {
    match window.as_str() {
        "main" => windows::show_main(&app, None),
        "usage" => windows::show_main(&app, Some("usage")),
        "scheduler" | "settings" | "about" => windows::show_main(&app, Some(&window)),
        "compact" | "pet" => Err("Enable this display from Settings".into()),
        _ => Err("Unknown window".into()),
    }
}

#[tauri::command]
pub fn hide_menu_bar_panel(app: AppHandle) -> Result<(), String> {
    windows::hide_menu_bar_panel(&app)
}

#[tauri::command]
pub async fn show_floating_context_menu(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    window: String,
) -> Result<(), String> {
    let config = state.config.read().await.clone();
    crate::tray::show_floating_context_menu(&app, &config, &window)
}

#[tauri::command]
pub async fn set_compact_expanded(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    expanded: bool,
) -> Result<(), String> {
    let config = state.config.read().await.clone();
    windows::set_compact_expanded(&app, &config, expanded)
}

#[tauri::command]
pub async fn set_pet_expanded(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    expanded: bool,
) -> Result<(), String> {
    let config = state.config.read().await.clone();
    windows::set_pet_expanded(&app, &config, expanded)
}

#[tauri::command]
pub async fn disable_display(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    window: String,
) -> Result<(), String> {
    let snapshot = {
        let mut config = state.config.write().await;
        match window.as_str() {
            "compact" => config.compact_widget_enabled = false,
            "pet" => config.pet_enabled = false,
            _ => return Err("Only compact and pet displays can be disabled".into()),
        }
        config.clone()
    };
    config::save(&app, &snapshot)?;
    windows::apply_display_config(&app, &snapshot)?;
    crate::tray::sync_menu(&app, &snapshot);
    let _ = app.emit("config-updated", snapshot);
    Ok(())
}

#[tauri::command]
pub async fn import_pet_image(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    source: String,
) -> Result<AppConfig, String> {
    let source = PathBuf::from(source);
    let extension = source
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| "Pet image needs a PNG, WebP, or GIF extension".to_string())?;
    if !matches!(extension.as_str(), "png" | "webp" | "gif") {
        return Err("Only PNG, WebP, and GIF pet images are supported".into());
    }
    let metadata =
        fs::metadata(&source).map_err(|error| format!("Cannot read pet image: {error}"))?;
    if metadata.len() > 20 * 1024 * 1024 {
        return Err("Pet image must be smaller than 20 MB".into());
    }

    let pets_dir = config::config_dir(&app)?.join("pets");
    fs::create_dir_all(&pets_dir)
        .map_err(|error| format!("Cannot create pet image directory: {error}"))?;
    let id = Uuid::new_v4().to_string();
    let destination = pets_dir.join(format!("{id}.{extension}"));
    fs::copy(&source, &destination).map_err(|error| format!("Cannot copy pet image: {error}"))?;
    let destination_text = destination.to_string_lossy().into_owned();

    let snapshot = {
        let mut current = state.config.write().await;
        current.pet_image = Some(destination_text.clone());
        current.pet_image_history.push(PetImageRecord {
            id,
            path: destination_text,
            imported_at: Utc::now().timestamp(),
        });
        current.pet_preset = "custom".into();
        current.pet_enabled = true;
        current.compact_widget_enabled = false;
        current.clone()
    };
    config::save(&app, &snapshot)?;
    windows::apply_display_config(&app, &snapshot)?;
    crate::tray::sync_menu(&app, &snapshot);
    let _ = app.emit("config-updated", snapshot.clone());
    Ok(snapshot)
}

#[tauri::command]
pub async fn get_pet_image_data(state: State<'_, Arc<AppState>>) -> Result<Option<String>, String> {
    let Some(path) = state.config.read().await.pet_image.clone() else {
        return Ok(None);
    };
    Ok(Some(read_pet_image_data(&PathBuf::from(path))?))
}

#[tauri::command]
pub async fn get_pet_image_history(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<PetImageHistoryEntry>, String> {
    let config = state.config.read().await.clone();
    let mut history = Vec::new();
    for item in config.pet_image_history.iter().rev() {
        match read_pet_image_data(&PathBuf::from(&item.path)) {
            Ok(data_url) => history.push(PetImageHistoryEntry {
                id: item.id.clone(),
                imported_at: item.imported_at,
                data_url,
                active: config.pet_preset == "custom"
                    && config.pet_image.as_deref() == Some(item.path.as_str()),
            }),
            Err(error) => log::warn!("Skipping unavailable pet history image: {error}"),
        }
    }
    Ok(history)
}

#[tauri::command]
pub async fn select_pet_image(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Result<AppConfig, String> {
    let snapshot = {
        let mut current = state.config.write().await;
        let item = current
            .pet_image_history
            .iter()
            .find(|item| item.id == id)
            .cloned()
            .ok_or_else(|| "Pet history image was not found".to_string())?;
        fs::metadata(&item.path).map_err(|error| format!("Cannot read pet image: {error}"))?;
        current.pet_image = Some(item.path);
        current.pet_preset = "custom".into();
        current.pet_enabled = true;
        current.compact_widget_enabled = false;
        current.clone()
    };
    persist_pet_config(&app, &snapshot).await?;
    Ok(snapshot)
}

#[tauri::command]
pub async fn delete_pet_image(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Result<AppConfig, String> {
    let item = state
        .config
        .read()
        .await
        .pet_image_history
        .iter()
        .find(|item| item.id == id)
        .cloned()
        .ok_or_else(|| "Pet history image was not found".to_string())?;
    match fs::remove_file(&item.path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("Cannot delete pet image: {error}")),
    }
    let snapshot = {
        let mut current = state.config.write().await;
        current.pet_image_history.retain(|entry| entry.id != id);
        if current.pet_image.as_deref() == Some(item.path.as_str()) {
            if let Some(replacement) = current.pet_image_history.last() {
                current.pet_image = Some(replacement.path.clone());
                current.pet_preset = "custom".into();
            } else {
                current.pet_image = None;
                current.pet_preset = "cat".into();
            }
        }
        current.clone()
    };
    persist_pet_config(&app, &snapshot).await?;
    Ok(snapshot)
}

fn read_pet_image_data(path: &PathBuf) -> Result<String, String> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| "Stored pet image has no extension".to_string())?;
    let mime = match extension.as_str() {
        "png" => "image/png",
        "webp" => "image/webp",
        "gif" => "image/gif",
        _ => return Err("Stored pet image type is unsupported".into()),
    };
    let bytes = fs::read(path).map_err(|error| format!("Cannot load stored pet image: {error}"))?;
    Ok(format!("data:{mime};base64,{}", STANDARD.encode(bytes)))
}

async fn persist_pet_config(app: &AppHandle, snapshot: &AppConfig) -> Result<(), String> {
    config::save(app, snapshot)?;
    windows::apply_display_config(app, snapshot)?;
    crate::tray::sync_menu(app, snapshot);
    let _ = app.emit("config-updated", snapshot.clone());
    Ok(())
}

#[tauri::command]
pub fn open_log_folder(app: AppHandle) -> Result<(), String> {
    let directory = app
        .path()
        .app_log_dir()
        .map_err(|error| format!("Cannot resolve log directory: {error}"))?;
    fs::create_dir_all(&directory)
        .map_err(|error| format!("Cannot create log directory: {error}"))?;
    tauri_plugin_opener::open_path(&directory, None::<&str>)
        .map_err(|error| format!("Cannot open log directory: {error}"))?;
    Ok(())
}

#[tauri::command]
pub fn app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
