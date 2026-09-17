use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

pub const CONFIG_VERSION: u32 = 5;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowPosition {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PetImageRecord {
    pub id: String,
    pub path: String,
    pub imported_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Trigger {
    pub id: String,
    pub enabled: bool,
    pub time: String,
    pub label: String,
    pub last_run: Option<i64>,
    pub last_run_date: Option<String>,
    pub last_status: Option<String>,
    pub last_result: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AppConfig {
    pub config_version: u32,
    pub language: String,
    pub launch_at_startup: bool,
    pub start_minimized: bool,
    pub minimize_to_tray: bool,
    pub run_in_background: bool,
    pub compact_widget_enabled: bool,
    pub compact_widget_locked: bool,
    pub compact_show_five_hour: bool,
    pub compact_show_weekly: bool,
    pub compact_position: Option<WindowPosition>,
    pub pet_enabled: bool,
    pub pet_image: Option<String>,
    pub pet_image_history: Vec<PetImageRecord>,
    pub pet_preset: String,
    pub pet_scale: f64,
    pub pet_position: Option<WindowPosition>,
    pub pet_always_on_top: bool,
    pub opacity: f64,
    pub refresh_interval: u64,
    pub show_five_hour: bool,
    pub show_weekly: bool,
    pub show_reset_countdown: bool,
    pub scheduler_enabled: bool,
    pub triggers: Vec<Trigger>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            config_version: CONFIG_VERSION,
            language: "system".into(),
            launch_at_startup: false,
            start_minimized: false,
            minimize_to_tray: true,
            run_in_background: true,
            compact_widget_enabled: false,
            compact_widget_locked: false,
            compact_show_five_hour: true,
            compact_show_weekly: true,
            compact_position: None,
            pet_enabled: false,
            pet_image: None,
            pet_image_history: Vec::new(),
            pet_preset: "cat".into(),
            pet_scale: 1.0,
            pet_position: None,
            pet_always_on_top: true,
            opacity: 0.92,
            refresh_interval: 60,
            show_five_hour: true,
            show_weekly: true,
            show_reset_countdown: true,
            scheduler_enabled: true,
            triggers: Vec::new(),
        }
    }
}

impl AppConfig {
    pub fn normalize(&mut self) {
        self.config_version = CONFIG_VERSION;
        if !matches!(self.language.as_str(), "system" | "zh-CN" | "en") {
            self.language = "system".into();
        }
        self.refresh_interval = self.refresh_interval.clamp(30, 3600);
        self.pet_scale = self.pet_scale.clamp(0.7, 1.5);
        self.opacity = self.opacity.clamp(0.35, 1.0);
        if !matches!(
            self.pet_preset.as_str(),
            "cat" | "dog" | "rocket" | "car" | "robot" | "tiga" | "custom"
        ) {
            self.pet_preset = "cat".into();
        }
        if let Some(path) = self.pet_image.clone()
            && !self.pet_image_history.iter().any(|item| item.path == path)
        {
            let id = std::path::Path::new(&path)
                .file_stem()
                .and_then(|value| value.to_str())
                .filter(|value| !value.is_empty())
                .unwrap_or("legacy")
                .to_string();
            self.pet_image_history.push(PetImageRecord {
                id,
                path,
                imported_at: 0,
            });
        }
        if self.compact_widget_enabled && self.pet_enabled {
            self.compact_widget_enabled = false;
        }
        if !self.compact_show_five_hour && !self.compact_show_weekly {
            self.compact_show_five_hour = true;
        }
        self.triggers.retain(|trigger| is_valid_time(&trigger.time));
        for trigger in &mut self.triggers {
            trigger.label = trigger.label.trim().chars().take(60).collect();
        }
    }
}

impl AppConfig {
    pub fn uses_chinese(&self) -> bool {
        self.language == "zh-CN" || (self.language == "system" && system_locale_is_chinese())
    }
}

#[cfg(windows)]
fn system_locale_is_chinese() -> bool {
    use windows_sys::Win32::Globalization::GetUserDefaultLocaleName;

    let mut buffer = [0u16; 85];
    let length = unsafe { GetUserDefaultLocaleName(buffer.as_mut_ptr(), buffer.len() as i32) };
    if length <= 1 {
        return false;
    }
    String::from_utf16_lossy(&buffer[..length as usize - 1])
        .to_ascii_lowercase()
        .starts_with("zh")
}

#[cfg(target_os = "macos")]
fn system_locale_is_chinese() -> bool {
    if std::env::var("LANG")
        .unwrap_or_default()
        .to_ascii_lowercase()
        .starts_with("zh")
    {
        return true;
    }
    std::process::Command::new("defaults")
        .args(["read", "-g", "AppleLanguages"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).to_ascii_lowercase())
        .is_some_and(|languages| languages.contains("zh-hans") || languages.contains("zh-cn"))
}

#[cfg(all(not(windows), not(target_os = "macos")))]
fn system_locale_is_chinese() -> bool {
    std::env::var("LANG")
        .unwrap_or_default()
        .to_ascii_lowercase()
        .starts_with("zh")
}

pub fn is_valid_time(value: &str) -> bool {
    let Some((hour, minute)) = value.split_once(':') else {
        return false;
    };
    hour.len() == 2
        && minute.len() == 2
        && hour.parse::<u8>().is_ok_and(|value| value < 24)
        && minute.parse::<u8>().is_ok_and(|value| value < 60)
}

pub fn config_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map_err(|error| format!("Cannot resolve the app configuration directory: {error}"))
}

pub fn config_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(config_dir(app)?.join("config.json"))
}

pub fn load(app: &AppHandle) -> AppConfig {
    let Ok(path) = config_path(app) else {
        log::error!("Unable to resolve configuration path");
        return AppConfig::default();
    };

    let mut config = match fs::read_to_string(&path) {
        Ok(contents) => serde_json::from_str(&contents).unwrap_or_else(|error| {
            log::error!("Unable to parse configuration: {error}");
            AppConfig::default()
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => AppConfig::default(),
        Err(error) => {
            log::error!("Unable to read configuration: {error}");
            AppConfig::default()
        }
    };
    config.normalize();
    config
}

pub fn save(app: &AppHandle, config: &AppConfig) -> Result<(), String> {
    let path = config_path(app)?;
    let parent = path
        .parent()
        .ok_or_else(|| "Configuration path has no parent directory".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Cannot create configuration directory: {error}"))?;
    let json = serde_json::to_string_pretty(config)
        .map_err(|error| format!("Cannot serialize configuration: {error}"))?;
    fs::write(path, json).map_err(|error| format!("Cannot save configuration: {error}"))
}

#[cfg(test)]
mod tests {
    use super::{AppConfig, is_valid_time};

    #[test]
    fn validates_24_hour_times() {
        assert!(is_valid_time("00:00"));
        assert!(is_valid_time("23:59"));
        assert!(!is_valid_time("24:00"));
        assert!(!is_valid_time("9:00"));
    }

    #[test]
    fn desktop_display_modes_are_mutually_exclusive() {
        let mut config = AppConfig {
            compact_widget_enabled: true,
            pet_enabled: true,
            ..AppConfig::default()
        };
        config.normalize();
        assert!(!config.compact_widget_enabled);
        assert!(config.pet_enabled);
    }

    #[test]
    fn compact_mode_always_has_visible_content() {
        let mut config = AppConfig {
            compact_show_five_hour: false,
            compact_show_weekly: false,
            ..AppConfig::default()
        };
        config.normalize();
        assert!(config.compact_show_five_hour);
        assert!(!config.compact_show_weekly);
    }

    #[test]
    fn invalid_language_falls_back_to_system() {
        let mut config = AppConfig {
            language: "invalid".into(),
            ..AppConfig::default()
        };
        config.normalize();
        assert_eq!(config.language, "system");
    }

    #[test]
    fn built_in_tiga_preset_is_preserved() {
        let mut config = AppConfig {
            pet_preset: "tiga".into(),
            ..AppConfig::default()
        };
        config.normalize();
        assert_eq!(config.pet_preset, "tiga");
    }

    #[test]
    fn migrates_existing_custom_pet_into_history() {
        let mut config = AppConfig {
            pet_image: Some("C:\\pets\\legacy.png".into()),
            pet_preset: "custom".into(),
            ..AppConfig::default()
        };
        config.normalize();
        assert_eq!(config.pet_image_history.len(), 1);
        assert_eq!(config.pet_image_history[0].path, "C:\\pets\\legacy.png");
    }
}
