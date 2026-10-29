use tauri::{
    App, AppHandle, Manager,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIcon,
};

use crate::{codex::usage::CodexUsage, config::AppConfig};

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

pub(super) fn update_usage(
    app: &AppHandle,
    tray: &TrayIcon,
    usage: &CodexUsage,
    config: &AppConfig,
) {
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
