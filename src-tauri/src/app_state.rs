use tokio::sync::{RwLock, mpsc};

use crate::{codex::usage::CodexUsage, config::AppConfig};

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStatus {
    pub cli_detected: bool,
    pub cli_version: Option<String>,
    pub app_server_status: String,
    pub last_error: Option<String>,
}

impl Default for RuntimeStatus {
    fn default() -> Self {
        Self {
            cli_detected: false,
            cli_version: None,
            app_server_status: "starting".into(),
            last_error: None,
        }
    }
}

#[derive(Debug)]
pub enum CodexControl {
    Refresh,
    Shutdown,
}

pub struct AppState {
    pub config: RwLock<AppConfig>,
    pub usage: RwLock<CodexUsage>,
    pub runtime_status: RwLock<RuntimeStatus>,
    pub codex_control: RwLock<Option<mpsc::Sender<CodexControl>>>,
}

impl AppState {
    pub fn new(config: AppConfig) -> Self {
        Self {
            config: RwLock::new(config),
            usage: RwLock::new(CodexUsage::connecting()),
            runtime_status: RwLock::new(RuntimeStatus::default()),
            codex_control: RwLock::new(None),
        }
    }
}
