// src-tauri/src/auth.rs
// OAuth device code flow for provider authentication.
// Ported from fathah/hermes-desktop src/main/hermes-auth.ts

use std::process::Stdio;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::Mutex;

use crate::config::expand_tilde;

// ── OAuth-capable providers ──────────────────────────────────────────────

/// Providers that support OAuth device-code flow.
/// Mirrors hermes-agent's `_OAUTH_CAPABLE_PROVIDERS`.
pub const OAUTH_LOGIN_PROVIDERS: &[&str] = &[
    "openai-codex",
    "xai-oauth",
    "qwen-oauth",
    "google-gemini-cli",
    "minimax-oauth",
    "nous",
];

pub fn is_oauth_login_provider(value: &str) -> bool {
    OAUTH_LOGIN_PROVIDERS.contains(&value)
}

// ── Types ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthLoginResult {
    pub success: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCodeInfo {
    pub url: String,
    pub code: String,
}

/// Shared state for the active OAuth login process.
/// Only one interactive login can run at a time.
#[derive(Clone)]
pub struct AuthState {
    pub active_proc: Arc<Mutex<Option<tokio::process::Child>>>,
}

impl AuthState {
    pub fn new() -> Self {
        Self {
            active_proc: Arc::new(Mutex::new(None)),
        }
    }
}

// ── Device code detection ────────────────────────────────────────────────

/// Parse a device-code login prompt out of the CLI's streamed output.
/// Returns `Some { url, code }` once both parts are present.
pub fn detect_device_code(text: &str) -> Option<DeviceCodeInfo> {
    let url_match = regex::Regex::new(
        r"Open this URL in your browser:[^\S\n]*\n[^\S\n]*(https://\S+)",
    )
    .ok()?
    .captures(text)?;
    let code_match = regex::Regex::new(
        r"Enter this code:[^\S\n]*\n[^\S\n]*(\S+)",
    )
    .ok()?
    .captures(text)?;
    Some(DeviceCodeInfo {
        url: url_match[1].to_string(),
        code: code_match[1].to_string(),
    })
}

// ── OAuth login subprocess ───────────────────────────────────────────────

/// Run `hermes auth add <provider> --type oauth` as a subprocess,
/// streaming stdout/stderr line-by-line to the frontend via Tauri events.
///
/// Emits `oauth-login-progress` events with the CLI output chunks.
/// Emits `oauth-login-complete` events with the final result.
/// Emits `oauth-device-code` events when a device code is detected.
pub async fn run_oauth_login(
    app_handle: AppHandle,
    hermes_home: std::path::PathBuf,
    hermes_python: std::path::PathBuf,
    provider: String,
    profile: Option<String>,
    auth_state: &AuthState,
) -> Result<OAuthLoginResult, String> {
    if !is_oauth_login_provider(&provider) {
        return Err(format!("Unsupported OAuth provider: {}", provider));
    }

    // Check if another login is already in progress
    let mut guard = auth_state.active_proc.lock().await;
    if guard.is_some() {
        return Err("Another sign-in is already in progress.".to_string());
    }

    // Build command args
    let mut args = vec!["auth", "add", &provider, "--type", "oauth"];
    let profile_owned;
    if let Some(ref p) = profile {
        if p != "default" {
            profile_owned = p.clone();
            // Insert -p and profile before auth
            args.insert(0, &profile_owned);
            args.insert(0, "-p");
        }
    }

    let mut cmd = Command::new(&hermes_python);
    cmd.args(&args)
        .current_dir(&hermes_home)
        .env("PYTHONUNBUFFERED", "1")
        .env("TERM", "dumb")
        .env("HOME", dirs::home_dir().unwrap_or_default())
        .env("HERMES_HOME", &hermes_home)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());

    let mut child = cmd.spawn().map_err(|e| format!("Failed to start sign-in: {}",e))?;

    let stdout = child.stdout.take().ok_or("Failed to capture stdout")?;
    let stderr = child.stderr.take().ok_or("Failed to capture stderr")?;

    *guard = Some(child);
    drop(guard);

    let app_handle_clone = app_handle.clone();
    let active_proc_clone = auth_state.active_proc.clone();

    // Spawn task to stream output
    tokio::spawn(async move {
        let mut stdout_reader = BufReader::new(stdout).lines();
        let mut stderr_reader = BufReader::new(stderr).lines();
        let mut full_output = String::new();

        loop {
            tokio::select! {
                Ok(Some(line)) = stdout_reader.next_line() => {
                    let chunk = format!("{}\n", line);
                    full_output.push_str(&chunk);
                    let _ = app_handle_clone.emit("oauth-login-progress", chunk.clone());

                    // Check for device code
                    if let Some(dc) = detect_device_code(&full_output) {
                        let _ = app_handle_clone.emit("oauth-device-code", dc);
                    }
                }
                Ok(Some(line)) = stderr_reader.next_line() => {
                    let chunk = format!("{}\n", line);
                    full_output.push_str(&chunk);
                    let _ = app_handle_clone.emit("oauth-login-progress", chunk.clone());
                }
                else => break,
            }
        }

        // Wait for process to finish
        let mut guard = active_proc_clone.lock().await;
        if let Some(mut child) = guard.take() {
            match child.wait().await {
                Ok(status) => {
                    let result = if status.success() {
                        OAuthLoginResult {
                            success: true,
                            error: None,
                        }
                    } else {
                        OAuthLoginResult {
                            success: false,
                            error: Some(format!(
                                "Sign-in exited with code {}.",
                                status.code().unwrap_or(-1)
                            )),
                        }
                    };
                    let _ = app_handle_clone.emit("oauth-login-complete", result);
                }
                Err(e) => {
                    let _ = app_handle_clone.emit(
                        "oauth-login-complete",
                        OAuthLoginResult {
                            success: false,
                            error: Some(format!("Sign-in failed: {}", e)),
                        },
                    );
                }
            }
        }
    });

    Ok(OAuthLoginResult {
        success: true,
        error: None,
    })
}

/// Kill the in-flight login subprocess, if any.
pub async fn cancel_oauth_login(
    auth_state: &AuthState,
) -> Result<bool, String> {
    let mut guard = auth_state.active_proc.lock().await;
    if let Some(mut child) = guard.take() {
        child.kill().await.map_err(|e| format!("Failed to cancel: {}", e))?;
        Ok(true)
    } else {
        Ok(false)
    }
}

// ── Credential storage (keyring) ─────────────────────────────────────────

/// Store a credential in the OS keyring.
#[tauri::command]
pub async fn store_credential(
    service: String,
    account: String,
    password: String,
) -> Result<(), String> {
    let entry = keyring::Entry::new(&service, &account)
        .map_err(|e| format!("Keyring error: {}", e))?;
    entry
        .set_password(&password)
        .map_err(|e| format!("Failed to store credential: {}", e))?;
    Ok(())
}

/// Retrieve a credential from the OS keyring.
#[tauri::command]
pub async fn get_credential(
    service: String,
    account: String,
) -> Result<Option<String>, String> {
    let entry = keyring::Entry::new(&service, &account)
        .map_err(|e| format!("Keyring error: {}", e))?;
    match entry.get_password() {
        Ok(password) => Ok(Some(password)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(format!("Failed to get credential: {}", e)),
    }
}

/// Delete a credential from the OS keyring.
#[tauri::command]
pub async fn delete_credential(
    service: String,
    account: String,
) -> Result<(), String> {
    let entry = keyring::Entry::new(&service, &account)
        .map_err(|e| format!("Keyring error: {}", e))?;
    entry
        .delete_password()
        .map_err(|e| format!("Failed to delete credential: {}", e))?;
    Ok(())
}

// ── Credential Pool ───────────────────────────────────────────────────────

use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialPoolEntry {
    pub id: Option<String>,
    pub label: Option<String>,
    pub auth_type: Option<String>,
    pub priority: Option<i32>,
    pub source: Option<String>,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub request_count: Option<i32>,
    pub key: Option<String>,
}

fn pool_path(hermes_home: &PathBuf) -> PathBuf {
    hermes_home.join("credential_pool.json")
}

pub async fn get_credential_pool(
    hermes_home: &PathBuf,
) -> Result<HashMap<String, Vec<CredentialPoolEntry>>, String> {
    let path = pool_path(hermes_home);
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let content = tokio::fs::read_to_string(&path)
        .await
        .map_err(|e| format!("Failed to read pool: {}", e))?;
    let pool: HashMap<String, Vec<CredentialPoolEntry>> = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse pool: {}", e))?;
    Ok(pool)
}

pub async fn add_credential_pool_entry(
    hermes_home: &PathBuf,
    provider: &str,
    key: &str,
    label: &str,
) -> Result<Vec<CredentialPoolEntry>, String> {
    let mut pool = get_credential_pool(hermes_home).await?;
    let entries = pool.entry(provider.to_string()).or_default();
    entries.push(CredentialPoolEntry {
        id: Some(format!("{}-{}", provider, entries.len())),
        label: if label.is_empty() { None } else { Some(label.to_string()) },
        auth_type: Some("api_key".to_string()),
        priority: Some(entries.len() as i32),
        source: Some("manual".to_string()),
        access_token: Some(key.to_string()),
        refresh_token: None,
        api_key: None,
        base_url: None,
        request_count: Some(0),
        key: None,
    });
    let result = entries.clone();
    set_credential_pool(hermes_home, provider, &entries).await?;
    Ok(result)
}

pub async fn set_credential_pool(
    hermes_home: &PathBuf,
    provider: &str,
    entries: &[CredentialPoolEntry],
) -> Result<(), String> {
    let mut pool = get_credential_pool(hermes_home).await?;
    pool.insert(provider.to_string(), entries.to_vec());
    let content = serde_json::to_string_pretty(&pool)
        .map_err(|e| format!("Failed to serialize pool: {}", e))?;
    tokio::fs::write(pool_path(hermes_home), content)
        .await
        .map_err(|e| format!("Failed to write pool: {}", e))?;
    Ok(())
}
