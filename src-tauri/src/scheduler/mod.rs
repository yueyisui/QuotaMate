pub mod trigger;

use std::{sync::Arc, time::Duration};

use chrono::{Local, Timelike};
use tauri::{AppHandle, Emitter, Manager};
use tokio::time;

use crate::{app_state::AppState, config};

pub async fn start(app: AppHandle, state: Arc<AppState>) {
    let mut ticker = time::interval(Duration::from_secs(15));
    ticker.set_missed_tick_behavior(time::MissedTickBehavior::Skip);

    loop {
        ticker.tick().await;
        let now = Local::now();
        let local_date = now.format("%Y-%m-%d").to_string();
        let current_time = format!("{:02}:{:02}", now.hour(), now.minute());

        let due_ids = {
            let app_config = state.config.read().await;
            if !app_config.scheduler_enabled {
                Vec::new()
            } else {
                app_config
                    .triggers
                    .iter()
                    .filter(|trigger| is_due(trigger, &current_time, &local_date))
                    .map(|trigger| trigger.id.clone())
                    .collect::<Vec<_>>()
            }
        };

        for trigger_id in due_ids {
            let triggered_at = Local::now().timestamp();
            let trigger_to_run = {
                let mut app_config = state.config.write().await;
                let Some(trigger) = app_config
                    .triggers
                    .iter_mut()
                    .find(|trigger| trigger.id == trigger_id)
                else {
                    continue;
                };
                if trigger.last_run_date.as_deref() == Some(local_date.as_str()) {
                    continue;
                }
                trigger.last_run_date = Some(local_date.clone());
                trigger.last_run = Some(triggered_at);
                trigger.last_status = Some("running".into());
                trigger.last_result = Some("Codex session is running".into());
                let clone = trigger.clone();
                let config_snapshot = app_config.clone();
                drop(app_config);
                if let Err(error) = config::save(&app, &config_snapshot) {
                    log::error!("Unable to persist trigger start: {error}");
                }
                clone
            };
            let _ = app.emit("config-updated", state.config.read().await.clone());

            let runtime_dir = match app.path().app_local_data_dir() {
                Ok(path) => path.join("runtime"),
                Err(error) => {
                    finish_trigger(
                        &app,
                        &state,
                        &trigger_id,
                        "failed",
                        &format!("Cannot resolve runtime directory: {error}"),
                    )
                    .await;
                    continue;
                }
            };
            let result = trigger::execute(&trigger_to_run, &runtime_dir).await;
            if result.success {
                log::info!("Trigger {} executed successfully", trigger_to_run.label);
            } else {
                log::warn!(
                    "Trigger {} failed: {}",
                    trigger_to_run.label,
                    result.summary
                );
            }
            finish_trigger(&app, &state, &trigger_id, &result.status, &result.summary).await;
        }
    }
}

fn is_due(trigger: &config::Trigger, current_time: &str, local_date: &str) -> bool {
    trigger.enabled
        && trigger.time == current_time
        && trigger.last_run_date.as_deref() != Some(local_date)
}

async fn finish_trigger(
    app: &AppHandle,
    state: &Arc<AppState>,
    trigger_id: &str,
    status: &str,
    summary: &str,
) {
    let snapshot = {
        let mut app_config = state.config.write().await;
        if let Some(trigger) = app_config
            .triggers
            .iter_mut()
            .find(|trigger| trigger.id == trigger_id)
        {
            trigger.last_status = Some(status.to_string());
            trigger.last_result = Some(summary.chars().take(400).collect());
        }
        app_config.clone()
    };
    if let Err(error) = config::save(app, &snapshot) {
        log::error!("Unable to persist trigger result: {error}");
    }
    let _ = app.emit("config-updated", snapshot);
}

#[cfg(test)]
mod tests {
    use super::is_due;
    use crate::config::Trigger;

    fn trigger(last_run_date: Option<&str>) -> Trigger {
        Trigger {
            id: "morning".into(),
            enabled: true,
            time: "05:00".into(),
            label: "Morning".into(),
            last_run: None,
            last_run_date: last_run_date.map(str::to_string),
            last_status: None,
            last_result: None,
        }
    }

    #[test]
    fn runs_only_once_per_local_date() {
        assert!(is_due(&trigger(None), "05:00", "2026-09-13"));
        assert!(!is_due(&trigger(Some("2026-09-13")), "05:00", "2026-09-13"));
    }

    #[test]
    fn does_not_run_missed_time() {
        assert!(!is_due(&trigger(None), "09:00", "2026-09-13"));
    }
}
