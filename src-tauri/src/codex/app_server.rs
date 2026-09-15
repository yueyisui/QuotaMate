use std::{collections::HashMap, path::PathBuf, process::Stdio, sync::Arc, time::Duration};

#[cfg(windows)]
use std::time::SystemTime;

use serde_json::Value;
use tauri::{AppHandle, Emitter};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines},
    process::{Child, ChildStdout, Command},
    sync::mpsc,
    time::{self, Instant},
};

use crate::{
    app_state::{AppState, CodexControl},
    codex::{
        protocol::{
            account_request, initialize_request, initialized_notification, rate_limits_request,
        },
        usage::{
            CodexUsage, parse_account_plan_response, parse_rate_limits_response,
            response_from_notification,
        },
    },
    tray,
};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const INITIALIZE_TIMEOUT: Duration = Duration::from_secs(12);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const RECONNECT_DELAY: Duration = Duration::from_secs(5);

enum ConnectionResult {
    Disconnected(String),
    Shutdown,
}

#[derive(Clone, Copy)]
enum PendingRequest {
    Account,
    RateLimits,
}

pub async fn start(app: AppHandle, state: Arc<AppState>) {
    let (sender, mut receiver) = mpsc::channel(16);
    *state.codex_control.write().await = Some(sender);

    loop {
        let binary = match resolve_codex_binary() {
            Ok(binary) => binary,
            Err(error) => {
                log::error!("Codex CLI unavailable: {error}");
                let mut status = state.runtime_status.write().await;
                status.cli_detected = false;
                status.cli_version = None;
                status.app_server_status = "unavailable".into();
                status.last_error = Some(error.clone());
                drop(status);
                let _ = app.emit(
                    "runtime-status-updated",
                    state.runtime_status.read().await.clone(),
                );
                publish_usage(&app, &state, CodexUsage::unavailable(error)).await;
                if wait_before_reconnect(&mut receiver).await {
                    break;
                }
                continue;
            }
        };
        match detect_codex_cli(&binary).await {
            Ok(version) => {
                log::info!("Codex CLI detected: {version}");
                let mut status = state.runtime_status.write().await;
                status.cli_detected = true;
                status.cli_version = Some(version);
                status.app_server_status = "connecting".into();
                status.last_error = None;
            }
            Err(error) => {
                log::error!("Codex CLI unavailable: {error}");
                let mut status = state.runtime_status.write().await;
                status.cli_detected = false;
                status.cli_version = None;
                status.app_server_status = "unavailable".into();
                status.last_error = Some(error.clone());
                drop(status);
                let _ = app.emit(
                    "runtime-status-updated",
                    state.runtime_status.read().await.clone(),
                );
                publish_usage(
                    &app,
                    &state,
                    CodexUsage::unavailable("Codex CLI was not found"),
                )
                .await;
                if wait_before_reconnect(&mut receiver).await {
                    break;
                }
                continue;
            }
        }

        match run_connection(&app, &state, &binary, &mut receiver).await {
            ConnectionResult::Shutdown => break,
            ConnectionResult::Disconnected(error) => {
                log::error!("Codex app-server disconnected: {error}");
                {
                    let mut status = state.runtime_status.write().await;
                    status.app_server_status = "reconnecting".into();
                    status.last_error = Some(error.clone());
                }
                let _ = app.emit(
                    "runtime-status-updated",
                    state.runtime_status.read().await.clone(),
                );
                publish_usage(&app, &state, CodexUsage::unavailable(error)).await;
                if wait_before_reconnect(&mut receiver).await {
                    break;
                }
            }
        }
    }

    log::info!("Codex app-server worker stopped");
}

async fn wait_before_reconnect(receiver: &mut mpsc::Receiver<CodexControl>) -> bool {
    tokio::select! {
        _ = time::sleep(RECONNECT_DELAY) => false,
        control = receiver.recv() => matches!(control, Some(CodexControl::Shutdown) | None),
    }
}

async fn detect_codex_cli(binary: &PathBuf) -> Result<String, String> {
    let mut command = Command::new(binary);
    command
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    hide_window(&mut command);
    let output = time::timeout(Duration::from_secs(8), command.output())
        .await
        .map_err(|_| "Codex version check timed out".to_string())?
        .map_err(|error| format!("Unable to launch Codex CLI: {error}"))?;
    if !output.status.success() {
        return Err(short_error(&output.stderr));
    }
    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if version.is_empty() {
        Err("Codex CLI returned an empty version".into())
    } else {
        Ok(version)
    }
}

async fn run_connection(
    app: &AppHandle,
    state: &Arc<AppState>,
    binary: &PathBuf,
    receiver: &mut mpsc::Receiver<CodexControl>,
) -> ConnectionResult {
    let mut command = Command::new(binary);
    command
        .args(["app-server", "--stdio"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    hide_window(&mut command);

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            return ConnectionResult::Disconnected(format!(
                "Unable to start codex app-server: {error}"
            ));
        }
    };
    log::info!("Codex app-server started");

    if let Some(stderr) = child.stderr.take() {
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if !line.trim().is_empty() {
                    log::warn!("Codex app-server: {}", sanitize(&line));
                }
            }
        });
    }

    let Some(mut stdin) = child.stdin.take() else {
        return stop_child(
            child,
            ConnectionResult::Disconnected("App Server stdin is unavailable".into()),
        )
        .await;
    };
    let Some(stdout) = child.stdout.take() else {
        return stop_child(
            child,
            ConnectionResult::Disconnected("App Server stdout is unavailable".into()),
        )
        .await;
    };
    let mut lines = BufReader::new(stdout).lines();

    if let Err(error) = write_message(&mut stdin, &initialize_request(1)).await {
        return stop_child(child, ConnectionResult::Disconnected(error)).await;
    }
    match time::timeout(INITIALIZE_TIMEOUT, wait_for_response(&mut lines, 1)).await {
        Ok(Ok(response)) if response.get("error").is_none() => {}
        Ok(Ok(response)) => {
            let error = json_rpc_error(&response);
            return stop_child(child, ConnectionResult::Disconnected(error)).await;
        }
        Ok(Err(error)) => {
            return stop_child(child, ConnectionResult::Disconnected(error)).await;
        }
        Err(_) => {
            return stop_child(
                child,
                ConnectionResult::Disconnected("App Server initialization timed out".into()),
            )
            .await;
        }
    }
    if let Err(error) = write_message(&mut stdin, &initialized_notification()).await {
        return stop_child(child, ConnectionResult::Disconnected(error)).await;
    }

    {
        let mut status = state.runtime_status.write().await;
        status.app_server_status = "connected".into();
        status.last_error = None;
    }
    let _ = app.emit(
        "runtime-status-updated",
        state.runtime_status.read().await.clone(),
    );

    let mut request_id = 2_u64;
    if let Err(error) = write_message(&mut stdin, &account_request(request_id)).await {
        return stop_child(child, ConnectionResult::Disconnected(error)).await;
    }
    let mut pending = HashMap::from([(request_id, (Instant::now(), PendingRequest::Account))]);
    request_id = request_id.saturating_add(1);
    if let Err(error) = write_message(&mut stdin, &rate_limits_request(request_id)).await {
        return stop_child(child, ConnectionResult::Disconnected(error)).await;
    }
    pending.insert(request_id, (Instant::now(), PendingRequest::RateLimits));
    let mut last_poll = Instant::now();
    let mut heartbeat = time::interval(Duration::from_secs(2));
    heartbeat.set_missed_tick_behavior(time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            control = receiver.recv() => {
                match control {
                    Some(CodexControl::Refresh) => {
                        request_id = request_id.saturating_add(1);
                        if let Err(error) = write_message(&mut stdin, &rate_limits_request(request_id)).await {
                            return stop_child(child, ConnectionResult::Disconnected(error)).await;
                        }
                        pending.insert(request_id, (Instant::now(), PendingRequest::RateLimits));
                        last_poll = Instant::now();
                    }
                    Some(CodexControl::Shutdown) | None => {
                        return stop_child(child, ConnectionResult::Shutdown).await;
                    }
                }
            }
            line = lines.next_line() => {
                let line = match line {
                    Ok(Some(line)) => line,
                    Ok(None) => return stop_child(child, ConnectionResult::Disconnected("App Server stdout closed".into())).await,
                    Err(error) => return stop_child(child, ConnectionResult::Disconnected(format!("Cannot read App Server output: {error}"))).await,
                };
                if line.trim().is_empty() { continue; }
                let message: Value = match serde_json::from_str(&line) {
                    Ok(message) => message,
                    Err(error) => {
                        log::warn!("Ignoring malformed App Server message: {error}");
                        continue;
                    }
                };
                if message.get("method").and_then(Value::as_str) == Some("account/rateLimits/updated") {
                    if let Some(params) = message.get("params") {
                        let is_codex = params
                            .get("rateLimits")
                            .and_then(|value| value.get("limitId"))
                            .and_then(Value::as_str)
                            .is_none_or(|limit_id| limit_id == "codex");
                        if is_codex {
                            let update = response_from_notification(params);
                            if let Ok(mut usage) = parse_rate_limits_response(&update) {
                                let current = state.usage.read().await.clone();
                                usage.five_hour = usage.five_hour.or(current.five_hour);
                                usage.weekly = usage.weekly.or(current.weekly);
                                usage.plan_type = usage.plan_type.or(current.plan_type);
                                usage.rate_limit_reset_credits = usage
                                    .rate_limit_reset_credits
                                    .or(current.rate_limit_reset_credits);
                                publish_usage(app, state, usage).await;
                            }
                        }
                    }
                    request_id = request_id.saturating_add(1);
                    if let Err(error) = write_message(&mut stdin, &rate_limits_request(request_id)).await {
                        return stop_child(child, ConnectionResult::Disconnected(error)).await;
                    }
                    pending.insert(request_id, (Instant::now(), PendingRequest::RateLimits));
                    last_poll = Instant::now();
                } else if let Some(response_id) = message.get("id").and_then(Value::as_u64) {
                    let Some((_, request_kind)) = pending.remove(&response_id) else {
                        continue;
                    };
                    if message.get("error").is_some() {
                        let error = json_rpc_error(&message);
                        match request_kind {
                            PendingRequest::Account => log::warn!("Account read failed: {error}"),
                            PendingRequest::RateLimits => {
                                log::error!("Rate-limit read failed: {error}");
                                publish_usage(app, state, CodexUsage::unavailable(error)).await;
                            }
                        }
                    } else if let Some(result) = message.get("result") {
                        match request_kind {
                            PendingRequest::Account => {
                                if let Some(plan_type) = parse_account_plan_response(result) {
                                    let mut usage = state.usage.read().await.clone();
                                    usage.plan_type = Some(plan_type);
                                    publish_usage(app, state, usage).await;
                                }
                            }
                            PendingRequest::RateLimits => {
                                match parse_rate_limits_response(result) {
                                    Ok(mut usage) => {
                                        let current = state.usage.read().await.clone();
                                        usage.plan_type = usage.plan_type.or(current.plan_type);
                                        publish_usage(app, state, usage).await;
                                    }
                                    Err(error) => {
                                        log::error!("Invalid rate-limit response: {error}");
                                        publish_usage(app, state, CodexUsage::unavailable(error)).await;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            _ = heartbeat.tick() => {
                match child.try_wait() {
                    Ok(Some(status)) => return ConnectionResult::Disconnected(format!("App Server exited with {status}")),
                    Err(error) => return stop_child(child, ConnectionResult::Disconnected(format!("Cannot inspect App Server process: {error}"))).await,
                    Ok(None) => {}
                }
                if pending.values().any(|(started, _)| started.elapsed() >= REQUEST_TIMEOUT) {
                    return stop_child(child, ConnectionResult::Disconnected("App Server rate-limit request timed out".into())).await;
                }
                let refresh_interval = state.config.read().await.refresh_interval;
                if pending.is_empty() && last_poll.elapsed() >= Duration::from_secs(refresh_interval) {
                    request_id = request_id.saturating_add(1);
                    if let Err(error) = write_message(&mut stdin, &rate_limits_request(request_id)).await {
                        return stop_child(child, ConnectionResult::Disconnected(error)).await;
                    }
                    pending.insert(request_id, (Instant::now(), PendingRequest::RateLimits));
                    last_poll = Instant::now();
                }
            }
        }
    }
}

async fn wait_for_response(
    lines: &mut Lines<BufReader<ChildStdout>>,
    expected_id: u64,
) -> Result<Value, String> {
    loop {
        let line = lines
            .next_line()
            .await
            .map_err(|error| format!("Cannot read App Server initialization: {error}"))?
            .ok_or_else(|| "App Server closed during initialization".to_string())?;
        let message: Value = serde_json::from_str(&line)
            .map_err(|error| format!("Invalid JSON during initialization: {error}"))?;
        if message.get("id").and_then(Value::as_u64) == Some(expected_id) {
            return Ok(message);
        }
    }
}

async fn write_message(
    stdin: &mut tokio::process::ChildStdin,
    message: &Value,
) -> Result<(), String> {
    let mut encoded = serde_json::to_vec(message)
        .map_err(|error| format!("Cannot encode App Server request: {error}"))?;
    encoded.push(b'\n');
    stdin
        .write_all(&encoded)
        .await
        .map_err(|error| format!("Cannot write to App Server: {error}"))?;
    stdin
        .flush()
        .await
        .map_err(|error| format!("Cannot flush App Server request: {error}"))
}

async fn publish_usage(app: &AppHandle, state: &Arc<AppState>, usage: CodexUsage) {
    if usage.status == "available" {
        let five_hour = usage
            .five_hour
            .as_ref()
            .map(|window| format!("{:.0}% left", window.remaining_percent))
            .unwrap_or_else(|| "unavailable".into());
        let weekly = usage
            .weekly
            .as_ref()
            .map(|window| format!("{:.0}% left", window.remaining_percent))
            .unwrap_or_else(|| "unavailable".into());
        log::info!("Rate limit snapshot updated: 5h {five_hour}, weekly {weekly}");
    }
    *state.usage.write().await = usage.clone();
    let config = state.config.read().await.clone();
    tray::update_usage(app, &usage, &config);
    let _ = app.emit("usage-updated", usage);
}

async fn stop_child(mut child: Child, result: ConnectionResult) -> ConnectionResult {
    if child.try_wait().ok().flatten().is_none() {
        let _ = child.start_kill();
        let _ = time::timeout(Duration::from_secs(3), child.wait()).await;
    }
    result
}

fn json_rpc_error(message: &Value) -> String {
    message
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .map(sanitize)
        .unwrap_or_else(|| "Unknown App Server error".into())
}

fn short_error(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let value = sanitize(text.trim());
    if value.is_empty() {
        "Codex command failed".into()
    } else {
        value.chars().take(300).collect()
    }
}

fn sanitize(value: &str) -> String {
    let lowercase = value.to_ascii_lowercase();
    if ["authorization", "access_token", "cookie", "bearer "]
        .iter()
        .any(|needle| lowercase.contains(needle))
    {
        "[redacted potentially sensitive Codex diagnostic]".into()
    } else {
        value.chars().take(500).collect()
    }
}

pub fn resolve_codex_binary() -> Result<PathBuf, String> {
    let executable = if cfg!(windows) { "codex.exe" } else { "codex" };
    if let Some(candidate) = std::env::var_os("CODEX_CLI_PATH").map(PathBuf::from)
        && candidate.is_file()
    {
        return Ok(candidate);
    }
    if let Some(path) = std::env::var_os("PATH") {
        for directory in std::env::split_paths(&path) {
            let candidate = directory.join(executable);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        let mut candidates = vec![
            PathBuf::from("/Applications/Codex.app/Contents/Resources/codex"),
            PathBuf::from("/Applications/ChatGPT.app/Contents/Resources/codex"),
            PathBuf::from("/opt/homebrew/bin/codex"),
            PathBuf::from("/usr/local/bin/codex"),
        ];
        if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
            candidates.extend([
                home.join("Applications/Codex.app/Contents/Resources/codex"),
                home.join("Applications/ChatGPT.app/Contents/Resources/codex"),
                home.join(".local/bin/codex"),
            ]);
            let nvm_root = home.join(".nvm/versions/node");
            if let Ok(entries) = std::fs::read_dir(nvm_root) {
                let mut nvm_candidates = entries
                    .filter_map(Result::ok)
                    .map(|entry| entry.path().join("bin/codex"))
                    .filter(|path| path.is_file())
                    .collect::<Vec<_>>();
                nvm_candidates.sort();
                nvm_candidates.reverse();
                candidates.extend(nvm_candidates);
            }
        }
        if let Some(candidate) = candidates.into_iter().find(|path| path.is_file()) {
            return Ok(candidate);
        }
    }

    #[cfg(windows)]
    if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
        let bin_root = PathBuf::from(local_app_data)
            .join("OpenAI")
            .join("Codex")
            .join("bin");
        if let Ok(entries) = std::fs::read_dir(bin_root) {
            let mut candidates = entries
                .filter_map(Result::ok)
                .map(|entry| entry.path().join("codex.exe"))
                .filter(|path| path.is_file())
                .collect::<Vec<_>>();
            candidates.sort_by_key(|path| {
                path.metadata()
                    .and_then(|metadata| metadata.modified())
                    .unwrap_or(SystemTime::UNIX_EPOCH)
            });
            if let Some(candidate) = candidates.pop() {
                return Ok(candidate);
            }
        }
    }
    Err("Codex CLI was not found in PATH or the Codex Desktop installation".into())
}

#[cfg(windows)]
fn hide_window(command: &mut Command) {
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_window(_command: &mut Command) {}
