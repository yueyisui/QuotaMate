use std::{path::Path, process::Stdio, time::Duration};

use tokio::{process::Command, time};

use crate::codex::app_server::resolve_codex_binary;
use crate::config::Trigger;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const TRIGGER_PROMPT: &str = "Reply with OK only.";
const TRIGGER_TIMEOUT: Duration = Duration::from_secs(120);

pub struct TriggerResult {
    pub success: bool,
    pub status: String,
    pub summary: String,
}

pub async fn execute(trigger: &Trigger, runtime_dir: &Path) -> TriggerResult {
    log::info!("Executing Codex trigger: {}", trigger.label);
    if let Err(error) = std::fs::create_dir_all(runtime_dir) {
        return TriggerResult {
            success: false,
            status: "failed".into(),
            summary: format!("Cannot create runtime directory: {error}"),
        };
    }

    let binary = match resolve_codex_binary() {
        Ok(binary) => binary,
        Err(error) => {
            return TriggerResult {
                success: false,
                status: "failed".into(),
                summary: error,
            };
        }
    };
    let mut command = Command::new(binary);
    command
        .args([
            "--ask-for-approval",
            "never",
            "--sandbox",
            "read-only",
            "--cd",
        ])
        .arg(runtime_dir)
        .args([
            "exec",
            "--ephemeral",
            "--skip-git-repo-check",
            "--ignore-rules",
            "--color",
            "never",
            TRIGGER_PROMPT,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    hide_window(&mut command);

    let child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            return TriggerResult {
                success: false,
                status: "failed".into(),
                summary: format!("Unable to start Codex: {error}"),
            };
        }
    };

    match time::timeout(TRIGGER_TIMEOUT, child.wait_with_output()).await {
        Err(_) => TriggerResult {
            success: false,
            status: "timed_out".into(),
            summary: "Codex trigger exceeded the 120 second timeout".into(),
        },
        Ok(Err(error)) => TriggerResult {
            success: false,
            status: "failed".into(),
            summary: format!("Unable to wait for Codex: {error}"),
        },
        Ok(Ok(output)) if output.status.success() => TriggerResult {
            success: true,
            status: "success".into(),
            summary: format!(
                "Completed with exit code {}",
                output.status.code().unwrap_or(0)
            ),
        },
        Ok(Ok(output)) => {
            let code = output.status.code().unwrap_or(-1);
            let error = safe_summary(&output.stderr);
            TriggerResult {
                success: false,
                status: "failed".into(),
                summary: if error.is_empty() {
                    format!("Codex exited with code {code}")
                } else {
                    format!("Codex exited with code {code}: {error}")
                },
            }
        }
    }
}

fn safe_summary(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    let lowercase = text.to_ascii_lowercase();
    if ["authorization", "access_token", "cookie", "bearer "]
        .iter()
        .any(|needle| lowercase.contains(needle))
    {
        "Potentially sensitive diagnostic was redacted".into()
    } else {
        text.lines()
            .rev()
            .find(|line| !line.trim().is_empty())
            .unwrap_or_default()
            .trim()
            .chars()
            .take(300)
            .collect()
    }
}

#[cfg(windows)]
fn hide_window(command: &mut Command) {
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_window(_command: &mut Command) {}
