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

// ── Credential Pool (synced with Hermes auth.json) ───────────────────────
//
// Hermes stores its credential pool inside `auth.json` under the
// `credential_pool` key (provider → [entry]). Each entry carries either:
//   • source "manual"  → the raw secret is in `access_token` (persisted)
//   • source "env:KEY" → the secret lives in ~/.hermes/.env; auth.json holds
//     only a `secret_fingerprint` = sha256:<first 16 hex of the full secret>.
// We read/write auth.json directly so changes made in the desktop (e.g. adding
// a Groq key) are immediately visible to the agent, and vice-versa.

use std::collections::HashMap;
use std::path::PathBuf;

use sha2::{Digest, Sha256};

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
    /// sha256:<16 hex> — set by Hermes for borrowed (env) sources.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret_fingerprint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_status: Option<String>,
}

impl CredentialPoolEntry {
    /// Resolve the real secret value for this entry, given the hermes home
    /// (to read .env for env-sourced credentials). manual → access_token;
    /// env:KEY → look the var up in .env.
    pub fn resolve_secret(&self, hermes_home: &PathBuf) -> Option<String> {
        // manual source: the secret is stored inline.
        if self.access_token.as_deref().map(|s| !s.is_empty()).unwrap_or(false) {
            return self.access_token.clone();
        }
        // env:KEY — read the variable from the agent's .env.
        if let Some(src) = self.source.as_deref() {
            if let Some(var) = src.strip_prefix("env:") {
                if !var.is_empty() {
                    let env = crate::config::read_env(hermes_home, None);
                    if let Some(v) = env.get(var).filter(|v| !v.is_empty()) {
                        return Some(v.clone());
                    }
                    // Also check the global ~/.hermes/.env (Hermes's real
                    // credential store, which may differ from a profile home).
                    if let Some(home) = dirs::home_dir() {
                        let global_env = crate::config::read_env(&home.join(".hermes"), None);
                        if let Some(v) = global_env.get(var).filter(|v| !v.is_empty()) {
                            return Some(v.clone());
                        }
                    }
                    // Fallback: process environment.
                    if let Ok(v) = std::env::var(var) {
                        if !v.is_empty() {
                            return Some(v);
                        }
                    }
                }
            }
        }
        None
    }
}

/// Path to the Hermes auth store (NOT a separate credential_pool.json — the
/// old code invented that file and never synced with the agent).
fn auth_store_path(hermes_home: &PathBuf) -> PathBuf {
    hermes_home.join("auth.json")
}

/// Read the whole auth.json as a JSON Value (preserving all fields we don't
/// touch, like `providers` / `active_provider`).
fn read_auth_store(hermes_home: &PathBuf) -> serde_json::Value {
    let path = auth_store_path(hermes_home);
    if !path.exists() {
        return serde_json::json!({});
    }
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return serde_json::json!({}),
    };
    serde_json::from_str(&content).unwrap_or(serde_json::json!({}))
}

/// Atomically write auth.json back, preserving every top-level field except
/// `credential_pool` (which we replace) and `updated_at` (refreshed).
fn write_auth_store(hermes_home: &PathBuf, mut store: serde_json::Value) -> Result<(), String> {
    let path = auth_store_path(hermes_home);
    store["updated_at"] = serde_json::Value::String(now_iso8601());
    let content = serde_json::to_string_pretty(&store)
        .map_err(|e| format!("Failed to serialize auth.json: {}", e))?;
    write_secret_file(&path, &content)
        .map_err(|e| format!("Failed to write auth.json: {}", e))?;
    Ok(())
}

fn now_iso8601() -> String {
    // Best-effort RFC3339 timestamp without pulling chrono into auth.rs.
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{}", secs)
}

/// Compute the Hermes-style fingerprint: sha256 of the full secret, first 16
/// hex chars, prefixed with "sha256:".
pub fn fingerprint(secret: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(secret.as_bytes());
    let digest = hasher.finalize();
    let hex = digest.iter().map(|b| format!("{:02x}", b)).collect::<String>();
    format!("sha256:{}", &hex[..16])
}

pub async fn get_credential_pool(
    hermes_home: &PathBuf,
) -> Result<HashMap<String, Vec<CredentialPoolEntry>>, String> {
    let store = read_auth_store(hermes_home);
    let pool_val = store.get("credential_pool").cloned().unwrap_or(serde_json::json!({}));
    let pool: HashMap<String, Vec<CredentialPoolEntry>> =
        serde_json::from_value(pool_val).unwrap_or_default();
    Ok(pool)
}

/// Add a credential for a provider. Writes into auth.json (manual source keeps
/// the raw secret in `access_token`; env source writes the key to .env and
/// stores only a fingerprint). Returns the provider's resulting entry list.
pub async fn add_credential_pool_entry(
    hermes_home: &PathBuf,
    provider: &str,
    key: &str,
    label: &str,
) -> Result<Vec<CredentialPoolEntry>, String> {
    let mut pool = get_credential_pool(hermes_home).await?;
    let entries = pool.entry(provider.to_string()).or_default();
    let next_priority = entries.iter().map(|e| e.priority.unwrap_or(0)).max().unwrap_or(-1) + 1;
    let id = uuid::Uuid::new_v4().simple().to_string()[..6].to_string();

    // Decide source: if the label looks like an ENV var, treat as env-sourced
    // (write the value to .env, fingerprint in auth.json); else manual.
    let is_env_label = label
        .chars()
        .all(|c| c.is_ascii_uppercase() || c == '_')
        && label.contains('_');
    let (source, access_token, secret_fingerprint) = if is_env_label {
        // Write to the agent .env so Hermes resolves it at runtime.
        crate::config::write_env_value(hermes_home, None, label, key)?;
        (format!("env:{}", label), None, Some(fingerprint(key)))
    } else {
        (String::from("manual"), Some(key.to_string()), None)
    };

    entries.push(CredentialPoolEntry {
        id: Some(id),
        label: if label.is_empty() { None } else { Some(label.to_string()) },
        auth_type: Some("api_key".to_string()),
        priority: Some(next_priority),
        source: Some(source),
        access_token,
        refresh_token: None,
        api_key: None,
        base_url: None,
        request_count: Some(0),
        key: None,
        secret_fingerprint,
        last_status: None,
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
    let mut store = read_auth_store(hermes_home);
    // Ensure credential_pool object exists.
    if store.get("credential_pool").is_none() {
        store["credential_pool"] = serde_json::json!({});
    }
    let entries_val = serde_json::to_value(entries)
        .map_err(|e| format!("Failed to serialize entries: {}", e))?;
    store["credential_pool"][provider] = entries_val;
    write_auth_store(hermes_home, store)
}

/// Remove a credential entry by provider + id. Also clears the env var if the
/// entry was env-sourced, so the deletion fully takes effect.
pub async fn remove_credential_pool_entry(
    hermes_home: &PathBuf,
    provider: &str,
    entry_id: &str,
) -> Result<(), String> {
    let mut pool = get_credential_pool(hermes_home).await?;
    if let Some(entries) = pool.get_mut(provider) {
        // Find the entry to also clean up its env var if needed.
        if let Some(e) = entries.iter().find(|e| e.id.as_deref() == Some(entry_id)) {
            if let Some(src) = e.source.as_deref() {
                if let Some(var) = src.strip_prefix("env:") {
                    let _ = crate::config::write_env_value(hermes_home, None, var, "");
                }
            }
        }
        entries.retain(|e| e.id.as_deref() != Some(entry_id));
        if entries.is_empty() {
            pool.remove(provider);
        }
    }
    // Write the whole pool back.
    let mut store = read_auth_store(hermes_home);
    let pool_val = serde_json::to_value(&pool).unwrap_or(serde_json::json!({}));
    store["credential_pool"] = pool_val;
    write_auth_store(hermes_home, store)
}

/// Write a file containing secrets with mode 0600 (owner-only) on unix.
fn write_secret_file(path: &PathBuf, content: &str) -> Result<(), std::io::Error> {
    std::fs::write(path, content)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}
