use std::{path::Path, process::Stdio, time::Duration};

use serde_json::Value;
use tokio::{
    io::BufReader,
    process::Command,
    sync::mpsc,
    task::JoinHandle,
    time::{self, MissedTickBehavior},
};

use super::{
    app_server::{hide_window, json_rpc_error, wait_for_response, write_message},
    protocol::{account_request, initialize_request, initialized_notification},
};

pub const ACCOUNT_CHECK_INTERVAL: Duration = Duration::from_secs(5);
const PROBE_TIMEOUT: Duration = Duration::from_secs(12);

// Identity comes only from official account/read. No credential files or
// keychain tokens are read by QuotaMate. Full emails never leave this module.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountSnapshot {
    kind: Option<String>,
    email: Option<String>,
    account_id: Option<String>,
    pub plan_type: Option<String>,
}

impl AccountSnapshot {
    pub fn parse(result: &Value) -> Result<Self, String> {
        let account = result
            .get("account")
            .ok_or_else(|| "Account response is missing account information".to_string())?;
        if !account.is_null() && !account.is_object() {
            return Err("Account response has invalid account information".into());
        }
        if account.is_object() && account.get("type").and_then(Value::as_str).is_none() {
            return Err("Account response is missing its authentication type".into());
        }
        let string = |key| account.get(key).and_then(Value::as_str).map(str::to_owned);
        Ok(Self {
            kind: string("type"),
            email: string("email"),
            account_id: string("id").or_else(|| string("accountId")),
            plan_type: super::usage::parse_account_plan_response(result),
        })
    }

    pub fn supports_quota(&self) -> bool {
        self.kind.as_deref() == Some("chatgpt")
    }

    pub fn label(&self) -> Option<String> {
        self.email.as_deref().map(mask_email)
    }
}

fn mask_email(email: &str) -> String {
    let Some((local, domain)) = email.rsplit_once('@') else {
        return "***".into();
    };
    let first = local.chars().next().unwrap_or('*');
    format!("{first}***@{domain}")
}

/// A fresh process is intentional: an already running app-server can cache
/// authentication. This also detects changes when Codex uses OS keychain storage.
pub async fn read_fresh(binary: &Path) -> Result<AccountSnapshot, String> {
    let mut command = Command::new(binary);
    command
        .args(["app-server", "--stdio"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    hide_window(&mut command);
    let mut child = command
        .spawn()
        .map_err(|_| "Unable to start Codex account check".to_string())?;
    let result = time::timeout(PROBE_TIMEOUT, async {
        let mut stdin = child
            .stdin
            .take()
            .ok_or("Account check stdin unavailable")?;
        let stdout = child
            .stdout
            .take()
            .ok_or("Account check stdout unavailable")?;
        let mut lines = BufReader::new(stdout).lines();
        write_message(&mut stdin, &initialize_request(1)).await?;
        let initialized = wait_for_response(&mut lines, 1).await?;
        if initialized.get("error").is_some() {
            return Err(json_rpc_error(&initialized));
        }
        write_message(&mut stdin, &initialized_notification()).await?;
        write_message(&mut stdin, &account_request(2)).await?;
        let response = wait_for_response(&mut lines, 2).await?;
        if response.get("error").is_some() {
            return Err(json_rpc_error(&response));
        }
        AccountSnapshot::parse(response.get("result").unwrap_or(&Value::Null))
    })
    .await
    .unwrap_or_else(|_| Err("Codex account check timed out".into()));
    // Reap even on failed initialization or a timeout; never leave probe workers.
    let _ = child.start_kill();
    let _ = child.wait().await;
    result
}

use tokio::io::AsyncBufReadExt;

pub struct AccountWatcher(JoinHandle<()>);

impl AccountWatcher {
    pub fn start(binary: std::path::PathBuf) -> (Self, mpsc::Receiver<AccountSnapshot>) {
        let (sender, receiver) = mpsc::channel(1);
        let task = tokio::spawn(async move {
            let mut interval = time::interval(ACCOUNT_CHECK_INTERVAL);
            interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
            interval.tick().await; // First check is five seconds after connecting.
            loop {
                interval.tick().await;
                if sender.is_closed() {
                    break;
                }
                if let Ok(account) = read_fresh(&binary).await
                    && sender.send(account).await.is_err()
                {
                    break;
                }
            }
        });
        (Self(task), receiver)
    }
}

impl Drop for AccountWatcher {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn distinguishes_accounts_even_with_same_plan() {
        let a = AccountSnapshot::parse(&json!({"account": {
            "type": "chatgpt", "email": "alice@example.test", "planType": "plus"
        }}))
        .unwrap();
        let b = AccountSnapshot::parse(&json!({"account": {
            "type": "chatgpt", "email": "bob@example.test", "planType": "plus"
        }}))
        .unwrap();
        assert_ne!(a, b);
        assert_eq!(a.label().as_deref(), Some("a***@example.test"));
        assert!(!serde_json::to_string(&a.label()).unwrap().contains("alice"));
    }

    #[test]
    fn detects_logout_plan_and_workspace_changes() {
        let account = |id, plan| {
            AccountSnapshot::parse(&json!({"account": {
                "type": "chatgpt", "email": "same@example.test", "accountId": id, "planType": plan
            }}))
            .unwrap()
        };
        let a = account("a", "plus");
        assert_ne!(a, account("b", "plus"));
        assert_ne!(a, account("a", "pro"));
        let logout = AccountSnapshot::parse(&json!({"account": null})).unwrap();
        assert!(!logout.supports_quota());
        assert_eq!(logout.label(), None);
        assert_ne!(a, logout);
    }

    #[test]
    fn invalid_responses_are_not_treated_as_logout() {
        assert!(AccountSnapshot::parse(&json!({})).is_err());
        assert!(AccountSnapshot::parse(&json!({"account": {}})).is_err());
        assert!(AccountSnapshot::parse(&json!({"account": "bad"})).is_err());
        assert_eq!(mask_email("not-an-email"), "***");
        assert_eq!(mask_email("猫@example.test"), "猫***@example.test");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn fresh_process_observes_switch_and_logout_without_reading_credentials() {
        use std::{fs, os::unix::fs::PermissionsExt};
        let directory =
            std::env::temp_dir().join(format!("quotamate-probe-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&directory).unwrap();
        let binary = directory.join("codex");
        let state = directory.join("account-response.json");
        // A stand-in for the official RPC process, not a credential fixture.
        fs::write(&binary, concat!(
            "#!/bin/sh\n",
            "while IFS= read -r line; do\n",
            "case \"$line\" in\n",
            " *'\"method\":\"initialize\"'*) echo '{\"id\":1,\"result\":{}}' ;;\n",
            " *'\"method\":\"account/read\"'*) cat \"$(dirname \"$0\")/account-response.json\" ;;\n",
            "esac\n",
            "done\n",
        )).unwrap();
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
        let response = |email: &str| {
            json!({"id": 2, "result": {"account": {
                "type": "chatgpt", "email": email, "planType": "plus"
            }}})
            .to_string()
                + "\n"
        };
        fs::write(&state, response("alice@example.test")).unwrap();
        let a = read_fresh(&binary).await.unwrap();
        fs::write(&state, response("bob@example.test")).unwrap();
        let b = read_fresh(&binary).await.unwrap();
        assert_ne!(a, b);
        fs::write(&state, "{\"id\":2,\"result\":{\"account\":null}}\n").unwrap();
        assert!(!read_fresh(&binary).await.unwrap().supports_quota());
        fs::write(
            &state,
            "{\"id\":2,\"error\":{\"message\":\"fixture failure\"}}\n",
        )
        .unwrap();
        assert!(read_fresh(&binary).await.is_err());
        // The background watcher must detect changes independently of the
        // user-configurable (usually 60s) quota polling interval.
        fs::write(&state, response("alice@example.test")).unwrap();
        let (watcher, mut updates) = AccountWatcher::start(binary.clone());
        let watched_a = time::timeout(Duration::from_secs(9), updates.recv())
            .await
            .unwrap()
            .unwrap();
        fs::write(&state, response("bob@example.test")).unwrap();
        let watched_b = time::timeout(Duration::from_secs(9), updates.recv())
            .await
            .unwrap()
            .unwrap();
        assert_ne!(watched_a, watched_b);
        drop(watcher);
        fs::remove_dir_all(directory).unwrap();
    }
}
