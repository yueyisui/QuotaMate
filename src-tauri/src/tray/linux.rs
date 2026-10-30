use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use tauri::{
    App, AppHandle, Manager,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIcon,
};
use tokio::sync::{Mutex, MutexGuard};

use crate::{app_state::AppState, codex::usage::CodexUsage, config::AppConfig};

struct DisplayUpdates {
    generation: AtomicU64,
    commit: Mutex<()>,
}

impl DisplayUpdates {
    const fn new() -> Self {
        Self {
            generation: AtomicU64::new(0),
            commit: Mutex::const_new(()),
        }
    }

    fn next(&self) -> u64 {
        self.generation.fetch_add(1, Ordering::SeqCst) + 1
    }

    async fn lock_current(&self, generation: u64) -> Option<MutexGuard<'_, ()>> {
        let guard = self.commit.lock().await;
        (generation == self.generation.load(Ordering::SeqCst)).then_some(guard)
    }
}

static DISPLAY_UPDATES: DisplayUpdates = DisplayUpdates::new();

pub(super) fn next_display_selection() -> u64 {
    DISPLAY_UPDATES.next()
}

pub(super) async fn lock_display_selection(generation: u64) -> Option<MutexGuard<'static, ()>> {
    DISPLAY_UPDATES.lock_current(generation).await
}

pub(super) fn refresh_usage(app: &AppHandle) {
    let generation = DISPLAY_UPDATES.generation.load(Ordering::SeqCst);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let Some(_guard) = lock_display_selection(generation).await else {
            return;
        };
        let Some(state) = app.try_state::<Arc<AppState>>() else {
            return;
        };
        // Read after entering the same commit section as display selection.
        // An older quota/menu refresh must not restore a previous panel label.
        let usage = state.usage.read().await.clone();
        let config = state.config.read().await.clone();
        if let Some(tray) = app.tray_by_id("main-tray") {
            update_usage(&app, &tray, &usage, &config);
        }
    });
}

struct UsageMenu(MenuItem<tauri::Wry>);

pub(super) fn add_usage_menu(app: &App, menu: &Menu<tauri::Wry>) -> tauri::Result<()> {
    // AppIndicator does not deliver tray mouse events or tooltips. Keep an
    // actionable quota row even on desktops that do not render panel labels.
    let usage = MenuItem::with_id(app, "linux_usage", "Codex —", true, None::<&str>)?;
    menu.prepend(&PredefinedMenuItem::separator(app)?)?;
    menu.prepend(&usage)?;
    app.manage(UsageMenu(usage));
    Ok(())
}

fn update_usage(app: &AppHandle, tray: &TrayIcon, usage: &CodexUsage, config: &AppConfig) {
    let summary = usage_summary(usage, config);
    // An empty string explicitly clears a previous AppIndicator label when
    // switching to a pet or hidden mode. The icon/menu remain available.
    let title = if config.compact_widget_enabled {
        summary.as_str()
    } else {
        ""
    };
    let _ = tray.set_title(Some(title));
    if let Some(menu) = app.try_state::<UsageMenu>() {
        let _ = menu.0.set_text(format!("Codex · {summary}"));
    }
}

fn usage_summary(usage: &CodexUsage, config: &AppConfig) -> String {
    let values: Vec<_> = usage
        .visible_windows(
            !config.compact_widget_enabled || config.compact_show_five_hour,
            !config.compact_widget_enabled || config.compact_show_weekly,
        )
        .into_iter()
        .map(super::quota_value)
        .collect();
    if !values.is_empty() {
        return values.join(" · ");
    }
    match (usage.status.as_str(), config.uses_chinese()) {
        ("connecting", true) => "连接中…",
        ("connecting", false) => "Connecting…",
        ("available", true) => "暂无额度数据",
        ("available", false) => "No quota reported",
        (_, true) => "暂不可用",
        (_, false) => "Unavailable",
    }
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codex::usage::parse_rate_limits_response;
    use serde_json::json;

    #[tokio::test]
    async fn delayed_older_selection_cannot_follow_the_latest_commit() {
        let updates = Arc::new(DisplayUpdates::new());
        let old_generation = updates.next();
        let (start_old, old_ready) = tokio::sync::oneshot::channel();
        let old_updates = updates.clone();
        let old = tokio::spawn(async move {
            old_ready.await.unwrap();
            old_updates.lock_current(old_generation).await.is_some()
        });

        let latest_generation = updates.next();
        let latest_commit = updates.lock_current(latest_generation).await.unwrap();
        start_old.send(()).unwrap();
        // The old task starts after the latest task and must still be stale
        // when it acquires the commit lock, regardless of runtime scheduling.
        drop(latest_commit);
        assert!(!old.await.unwrap());
    }

    #[test]
    fn panel_uses_selected_windows_and_adapts_to_weekly_only() {
        let mut config = AppConfig::default();
        config.language = "en".into();
        config.compact_widget_enabled = true;
        let both = parse_rate_limits_response(&json!({"rateLimits": {
            "primary": {"windowDurationMins": 300, "usedPercent": 20},
            "secondary": {"windowDurationMins": 10080, "usedPercent": 35}
        }}))
        .unwrap();
        assert_eq!(usage_summary(&both, &config), "5h 80% · W 65%");
        config.compact_show_weekly = false;
        assert_eq!(usage_summary(&both, &config), "5h 80%");
        let weekly = parse_rate_limits_response(&json!({"rateLimits": {
            "primary": {"windowDurationMins": 10080, "usedPercent": 35}
        }}))
        .unwrap();
        assert_eq!(usage_summary(&weekly, &config), "W 65%");
        config.compact_widget_enabled = false;
        assert_eq!(usage_summary(&both, &config), "5h 80% · W 65%");
    }

    #[test]
    fn missing_usage_and_disconnect_do_not_become_a_percentage() {
        let mut config = AppConfig::default();
        config.language = "en".into();
        assert_eq!(
            usage_summary(&CodexUsage::connecting(), &config),
            "Connecting…"
        );
        assert_eq!(
            usage_summary(&CodexUsage::unavailable("offline"), &config),
            "Unavailable"
        );
        let empty = parse_rate_limits_response(&json!({"rateLimits": {}})).unwrap();
        assert_eq!(usage_summary(&empty, &config), "No quota reported");
        config.language = "zh-CN".into();
        assert_eq!(usage_summary(&empty, &config), "暂无额度数据");
    }
}
