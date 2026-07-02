// src-tauri/src/lib.rs
// Штурман Desktop v0.5.0 — Rust backend
// Ported from fathah/hermes-desktop (v0.5.8)

mod auth;
mod chat;
mod config;
mod config_health;
mod cronjobs;
mod discovery;
mod gateway;
mod install;
mod kanban;
mod media;
mod memory;
mod model_discovery;
mod models;
mod mcp;
mod profiles;
mod provider_registry;
mod registry;
mod secrets;
mod sessions;
mod skills;
mod ssh;
mod stt;
mod telegram;
mod terminal;
mod validation;

use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, State};

// ── Re-exports ───────────────────────────────────────────────────────────

pub use chat::{send_message, ChatEvent, ConnectionMode, SendMessageRequest};
pub use config::{ConnectionConfig, PublicConnectionConfig, SshConfig};
pub use gateway::{GatewayStartResult, GatewayState};
pub use models::SavedModel;
pub use profiles::ProfileInfo;
pub use sessions::{SessionMessage, SessionStats, SessionSummary};
pub use ssh::SshState;

// ── App State ─────────────────────────────────────────────────────────────

pub struct AppState {
    pub gateway: GatewayState,
    pub ssh: SshState,
    /// Resolved HERMES_HOME. Set once during init_app, read by every command.
    /// Lock-free (ArcSwap) so reads never block the async runtime and there is
    /// no mutex to poison.
    pub hermes_home: arc_swap::ArcSwapOption<PathBuf>,
    pub auth: auth::AuthState,
}

impl AppState {
    fn new() -> Self {
        Self {
            gateway: GatewayState::new(),
            ssh: SshState::new(),
            hermes_home: arc_swap::ArcSwapOption::from(None),
            auth: auth::AuthState::new(),
        }
    }

    /// Read the resolved HERMES_HOME. Returns `Err` if init_app has not run yet.
    /// Lock-free; safe to call from async commands without blocking the runtime.
    fn hermes_home(&self) -> Result<PathBuf, String> {
        // load_full() -> Option<Guard<Arc<PathBuf>>>. Guard derefs to
        // Arc<PathBuf>; as_ref() yields &Arc<PathBuf> -> &PathBuf via deref,
        // to_path_buf() clones out an owned PathBuf.
        self.hermes_home
            .load_full()
            .map(|guard| guard.as_ref().to_path_buf())
            .ok_or_else(|| "App not initialized".to_string())
    }
}

// ── Connection Info ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct ConnectionInfo {
    pub mode: String,
    pub instance: String,
    pub port: Option<u16>,
}

// ── Instance Info ─────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct InstanceInfo {
    pub path: String,
    pub instance: String,
    pub exists: bool,
}

#[derive(Debug, Serialize)]
pub struct RemoteInstanceInfo {
    pub path: String,
    pub instance: String,
    pub exists: bool,
}

// ── Tauri Commands ────────────────────────────────────────────────────────

/// Initialize app — resolve hermes home, detect instances
#[tauri::command]
async fn init_app(state: State<'_, AppState>) -> Result<InitResult, String> {
    let hermes_home = config::resolve_hermes_home();

    // Harden secret-bearing files: files created before the 0600 fix (PR #2)
    // may still be world/group-readable. Tighten them on every start so an
    // existing install is brought up to current policy. Missing files are
    // skipped; non-unix platforms are no-ops.
    harden_secret_files(&hermes_home);

    state.hermes_home.store(Some(std::sync::Arc::new(hermes_home.clone())));

    Ok(InitResult {
        hermes_home: hermes_home.to_string_lossy().to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

/// Ensure files that may hold secrets are owner-only (0600) on unix.
/// Idempotent and best-effort: failures are logged, never fatal.
fn harden_secret_files(hermes_home: &std::path::Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for name in ["desktop.json", "telegram.json", ".env", "auth.json"] {
            let path = hermes_home.join(name);
            let mode = match std::fs::metadata(&path) {
                Ok(m) => m.permissions().mode(),
                Err(_) => continue, // file absent — nothing to harden
            };
            // Only chmod if more permissive than 0600 (owner rw).
            if mode & 0o077 != 0 {
                if let Err(e) = std::fs::set_permissions(
                    &path,
                    std::fs::Permissions::from_mode(0o600),
                ) {
                    eprintln!(
                        "[steersman] warning: could not tighten permissions on {}: {}",
                        path.display(),
                        e
                    );
                } else {
                    eprintln!(
                        "[steersman] hardened '{}' to 0600 (was {:o})",
                        path.display(),
                        mode & 0o777
                    );
                }
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = hermes_home;
    }
}

#[derive(Debug, Serialize)]
struct InitResult {
    hermes_home: String,
    version: String,
}

/// Detect available Python instances.
///
/// Thin adapter over the unified cross-platform discovery
/// (`discovery::detect_local_instances`, ADR-001). The legacy function kept a
/// hard-coded unix-only candidate list that found nothing on Windows; we now
/// surface whatever the real discovery detects, mapped to the legacy
/// `{path, instance, exists}` shape so the existing frontend (ConnectScreen)
/// keeps working unchanged.
#[tauri::command]
async fn detect_instances() -> Result<Vec<InstanceInfo>, String> {
    let detected = discovery::detect_local_instances();
    let result = detected
        .into_iter()
        .map(|inst| InstanceInfo {
            path: inst.path,
            instance: inst.instance_type,
            exists: true, // discovery only returns existing interpreters
        })
        .collect();
    Ok(result)
}

/// Check if a Python path exists
#[tauri::command]
async fn check_python_path(path: String) -> Result<bool, String> {
    let expanded = config::expand_tilde(&path);
    Ok(PathBuf::from(&expanded).exists())
}

/// Detect local instances with version, gateway status, etc.
#[tauri::command]
async fn detect_local_instances_cmd() -> Result<Vec<discovery::DetectedInstance>, String> {
    Ok(discovery::detect_local_instances())
}

/// Connect to a detected agent instance — adopt its environment.
///
/// The user picks an instance from the discovery list; we set HERMES_HOME to
/// that instance's home dir (falling back to a resolved one) so the rest of
/// the app reuses its profiles, skills, and secrets instead of bootstrapping
/// a new environment. Returns the resolved home dir.
#[tauri::command]
async fn connect_to_instance(
    state: State<'_, AppState>,
    instance: discovery::DetectedInstance,
) -> Result<String, String> {
    // Prefer the instance's reported home dir; otherwise resolve the default.
    // But if the inferred home lacks state.db (the real data dir), fall back to
    // resolve_hermes_home() which reliably finds %LOCALAPPDATA%\hermes on
    // Windows. This keeps sessions/memory readable.
    let resolved_default = config::resolve_hermes_home();
    let home = match &instance.home_dir {
        Some(h) if !h.is_empty()
            && PathBuf::from(h).is_dir()
            && PathBuf::from(h).join("state.db").exists() =>
        {
            PathBuf::from(h)
        }
        _ => resolved_default,
    };

    // Sanity-check: a real agent home should be a directory that exists.
    if !home.is_dir() {
        return Err(format!(
            "Instance home '{}' does not exist or is not a directory",
            home.display()
        ));
    }

    state.hermes_home.store(Some(std::sync::Arc::new(home.clone())));

    Ok(home.to_string_lossy().to_string())
}

/// Auto-discover and adopt the local Hermes instance in one shot (ADR-003).
///
/// Runs discovery; if an instance is found, adopts its HERMES_HOME and starts
/// the gateway, returning the resolved home + whether the gateway came up.
/// The frontend uses this on startup to go straight to the chat screen when a
/// local agent exists (the shturman.ai "Подключен" experience), without ever
/// showing the manual connection screen.
#[tauri::command]
async fn auto_connect_local_cmd(
    state: State<'_, AppState>,
) -> Result<AutoConnectResult, String> {
    let instance = match discovery::primary_instance() {
        Some(i) => i,
        None => {
            return Ok(AutoConnectResult {
                found: false,
                hermes_home: None,
                gateway_running: false,
                label: None,
                error: Some("No local Hermes installation detected".to_string()),
            });
        }
    };

    // Adopt the instance's home dir, but ONLY if it actually holds state.db
    // (the real data dir with sessions/memory). Otherwise fall back to
    // resolve_hermes_home(), which on Windows finds %LOCALAPPDATA%\hermes. This
    // avoids clobbering the correct home with a discovery-inferred venv/checkout
    // dir that has no data — which previously left sessions/memory empty.
    let resolved_default = config::resolve_hermes_home();
    let home = match &instance.home_dir {
        Some(h) if !h.is_empty()
            && PathBuf::from(h).is_dir()
            && PathBuf::from(h).join("state.db").exists() =>
        {
            PathBuf::from(h)
        }
        Some(h) if !h.is_empty() && PathBuf::from(h).is_dir() => {
            // home_dir exists but lacks state.db — prefer the resolved default
            // if it has state.db, else use the inferred home as a last resort.
            if resolved_default.join("state.db").exists() {
                resolved_default
            } else {
                PathBuf::from(h)
            }
        }
        _ => {
            if resolved_default.is_dir() {
                resolved_default
            } else {
                return Ok(AutoConnectResult {
                    found: true,
                    hermes_home: None,
                    gateway_running: instance.gateway_running,
                    label: instance.label.clone(),
                    error: Some("Detected instance but could not resolve its home directory".to_string()),
                });
            }
        }
    };

    state
        .hermes_home
        .store(Some(std::sync::Arc::new(home.clone())));

    // Start the gateway if it isn't already up. primary_instance prefers a
    // running gateway; if it's already running we just report that.
    let gateway_running = if instance.gateway_running {
        true
    } else {
        let result = gateway::start_gateway(&state.gateway, &home, None);
        result.success
    };

    Ok(AutoConnectResult {
        found: true,
        hermes_home: Some(home.to_string_lossy().to_string()),
        gateway_running,
        label: instance.label.clone(),
        error: if gateway_running {
            None
        } else {
            Some("Instance adopted but gateway failed to start".to_string())
        },
    })
}

/// Result of an auto-connect attempt.
#[derive(Debug, Serialize)]
struct AutoConnectResult {
    found: bool,
    hermes_home: Option<String>,
    gateway_running: bool,
    label: Option<String>,
    error: Option<String>,
}

// ── Soul / Personality ────────────────────────────────────────────────────
//
// The agent's identity lives in two places: the free-form `soul.md` file
// (personality text the user can edit) and the `personalities` map in
// config.yaml with a `display.personality` pointer selecting the active one.
// These commands expose both so the onboarding wizard and the Settings → Soul
// tab can read/customize the agent's "soul".

#[derive(Debug, Serialize, Clone)]
struct Personality {
    id: String,
    description: String,
}

/// Read the agent's soul.md (persona text). Empty string if absent.
#[tauri::command]
fn read_soul_cmd(state: State<'_, AppState>) -> Result<String, String> {
    let hermes_home = state.hermes_home()?;
    Ok(memory::read_soul(&hermes_home, None))
}

/// Write the agent's soul.md (persona text).
#[tauri::command]
fn write_soul_cmd(state: State<'_, AppState>, content: String) -> Result<(), String> {
    let hermes_home = state.hermes_home()?;
    memory::write_soul(&hermes_home, None, &content)
}

/// Reset soul.md to the default and return the new content.
#[tauri::command]
fn reset_soul_cmd(state: State<'_, AppState>) -> Result<String, String> {
    let hermes_home = state.hermes_home()?;
    Ok(memory::reset_soul(&hermes_home, None))
}

/// List the available personalities from config.yaml (the `personalities` map).
#[tauri::command]
fn get_personalities_cmd(state: State<'_, AppState>) -> Result<Vec<Personality>, String> {
    let hermes_home = state.hermes_home()?;
    let yaml = config::read_config_yaml(&hermes_home, None).unwrap_or_default();
    let mut out = Vec::new();
    if let Some(personalities) = yaml.get("personalities").and_then(|v| v.as_object()) {
        for (id, desc) in personalities {
            let description = desc
                .as_str()
                .map(|s| s.to_string())
                .unwrap_or_else(|| desc.to_string());
            out.push(Personality {
                id: id.clone(),
                description,
            });
        }
    }
    // Guarantee a sensible default is always offered even if the install
    // shipped a trimmed config.
    if out.is_empty() {
        out.push(Personality {
            id: "helpful".to_string(),
            description: "You are a helpful, friendly AI assistant.".to_string(),
        });
    }
    Ok(out)
}

/// Read the active personality id (`display.personality` in config.yaml).
#[tauri::command]
fn get_personality_cmd(state: State<'_, AppState>) -> Result<String, String> {
    let hermes_home = state.hermes_home()?;
    let yaml = config::read_config_yaml(&hermes_home, None).unwrap_or_default();
    let active = yaml
        .get("display")
        .and_then(|d| d.get("personality"))
        .and_then(|p| p.as_str())
        .unwrap_or("helpful")
        .to_string();
    Ok(active)
}

/// Set the active personality (`display.personality`). Done via a targeted
/// text rewrite of config.yaml rather than a full re-serialize, to preserve
/// comments and unrelated keys.
#[tauri::command]
fn set_personality_cmd(state: State<'_, AppState>, personality: String) -> Result<(), String> {
    let hermes_home = state.hermes_home()?;
    set_config_scalar(&hermes_home, None, "personality", &personality)
}

/// Generic helper: set a scalar leaf under a one-level parent in config.yaml
/// (e.g. `display.personality`). Used because we don't want to rewrite the
/// whole YAML (which would lose comments/ordering).
fn set_config_scalar(
    hermes_home: &std::path::Path,
    profile: Option<&str>,
    key: &str,
    value: &str,
) -> Result<(), String> {
    let path = profile_config_yaml_path(hermes_home, profile);
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("Read config.yaml error: {}", e))?;

    // Look for an existing `display:` block and update the key inside it; or
    // append a fresh block if absent. We operate on lines so formatting is
    // preserved.
    let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
    let mut in_display = false;
    let mut display_indent: Option<usize> = None;
    let mut replaced = false;

    for line in &mut lines {
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        if trimmed.starts_with("display:") {
            in_display = true;
            display_indent = Some(indent);
            continue;
        }
        if in_display {
            // A line indented less-or-equal to the `display:` header (and not
            // blank) means we left the block.
            if !trimmed.is_empty() && indent <= display_indent.unwrap_or(0) {
                in_display = false;
                continue;
            }
            // Same indent as expected children → candidate for our key.
            if trimmed.starts_with(&format!("{}:", key)) {
                *line = format!(
                    "{}{}: {}",
                    " ".repeat(display_indent.unwrap_or(0) + 2),
                    key,
                    value
                );
                replaced = true;
            }
        }
    }

    if !replaced {
        // Either no display block or no such key: append a clean block.
        let block = format!(
            "display:\n  {}: {}\n",
            key, value
        );
        lines.push(block);
    }

    let new_content = lines.join("\n");
    std::fs::write(&path, new_content).map_err(|e| format!("Write config.yaml error: {}", e))?;
    Ok(())
}

fn profile_config_yaml_path(hermes_home: &std::path::Path, profile: Option<&str>) -> std::path::PathBuf {
    match profile {
        Some(p) if p != "default" && !p.is_empty() => {
            hermes_home.join("profiles").join(p).join("config.yaml")
        }
        _ => hermes_home.join("config.yaml"),
    }
}

/// Save a provider API key to the instance's `.env` (where Hermes reads it) and
/// optionally remember the provider/model. Used by the onboarding wizard.
#[tauri::command]
fn save_provider_key_cmd(
    state: State<'_, AppState>,
    env_key: String,
    api_key: String,
    provider: Option<String>,
    model: Option<String>,
    base_url: Option<String>,
) -> Result<(), String> {
    let hermes_home = state.hermes_home()?;
    config::write_env_value(&hermes_home, None, &env_key, &api_key)?;
    // Persist the active provider/model if supplied, so the freshly-installed
    // agent picks them up immediately.
    if let (Some(p), Some(m)) = (provider, model) {
        let b = base_url.unwrap_or_default();
        config::set_model_config(&hermes_home, None, &p, &m, &b);
    }
    Ok(())
}

/// Detect Python/steersman instances on a remote machine via SSH
#[tauri::command]
async fn detect_remote_instances_cmd(ssh_config: SshConfig) -> Result<Vec<RemoteInstanceInfo>, String> {
    let candidates: Vec<(&str, &str)> = vec![
        ("~/steersman/venv/bin/python3", "steersman"),
        ("~/steersman/venv/bin/python", "steersman"),
        ("~/.steersman/venv/bin/python", "steersman"),
        ("~/.hermes/venv/bin/python", "hermes"),
        ("~/.hermes/hermes-agent/venv/bin/python", "hermes-agent"),
        ("/usr/local/bin/python3", "system"),
    ];

    let mut result = Vec::new();
    for (path, instance) in candidates {
        let exists = ssh::ssh_exec(&ssh_config, &format!("test -f {}", path), 15).is_ok();
        result.push(RemoteInstanceInfo {
            path: path.to_string(),
            instance: instance.to_string(),
            exists,
        });
    }

    Ok(result)
}

/// Get app version
#[tauri::command]
fn get_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Get all version info
#[tauri::command]
fn get_versions_cmd() -> VersionsInfo {
    VersionsInfo {
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        tauri_version: tauri::VERSION.to_string(),
        rust_version: option_env!("CARGO_PKG_RUST_VERSION").unwrap_or("unknown").to_string(),
        node_version: std::env::var("npm_config_node_version").unwrap_or_else(|_| "unknown".into()),
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
    }
}

#[derive(Debug, Serialize)]
struct VersionsInfo {
    app_version: String,
    tauri_version: String,
    rust_version: String,
    node_version: String,
    os: String,
    arch: String,
}

// ── Connection Commands ───────────────────────────────────────────────────

/// Get connection config
#[tauri::command]
async fn get_connection_config(state: State<'_, AppState>) -> Result<PublicConnectionConfig, String> {
    let hermes_home = state.hermes_home()?;
    let cfg = config::read_desktop_config(&hermes_home);
    Ok(PublicConnectionConfig::from(&cfg))
}

/// Set connection config
#[tauri::command]
async fn set_connection_config(
    state: State<'_, AppState>,
    mode: String,
    remote_url: String,
    api_key: String,
    ssh_host: String,
    ssh_port: u16,
    ssh_username: String,
    ssh_key_path: String,
    ssh_remote_port: u16,
    ssh_local_port: u16,
) -> Result<bool, String> {
    let hermes_home = state.hermes_home()?;

    // The API key goes to the OS keyring, not into desktop.json. The on-disk
    // config keeps an empty api_key field (legacy compat) — see set_remote_api_key.
    config::set_remote_api_key(&hermes_home, &api_key)?;

    let cfg = ConnectionConfig {
        mode,
        remote_url,
        api_key: String::new(),
        ssh: SshConfig {
            host: ssh_host,
            port: ssh_port,
            username: ssh_username,
            key_path: ssh_key_path,
            remote_port: ssh_remote_port,
            local_port: ssh_local_port,
        },
    };

    config::write_desktop_config(&hermes_home, &cfg)?;
    Ok(true)
}

/// Test connection (ping remote or check SSH)
#[tauri::command]
async fn test_connection(
    _state: State<'_, AppState>,
    mode: String,
    url: String,
    ssh_config: Option<SshConfig>,
) -> Result<bool, String> {
    match mode.as_str() {
        "local" => {
            // Check if hermes is installed
            Ok(gateway::find_hermes_python().is_ok())
        }
        "remote" => {
            // Try to reach the API
            let api_url = format!("{}/health", url.trim_end_matches('/'));
            match reqwest::get(&api_url).await {
                Ok(resp) => Ok(resp.status().is_success()),
                Err(_) => Ok(false),
            }
        }
        "ssh" => {
            let ssh = ssh_config.ok_or("SSH config required")?;
            ssh::test_ssh_connection(&ssh)
        }
        _ => Err(format!("Unknown mode: {}", mode)),
    }
}

// ── Gateway Commands ──────────────────────────────────────────────────────

/// Start gateway
#[tauri::command]
async fn start_gateway_cmd(
    state: State<'_, AppState>,
    profile: Option<String>,
) -> Result<GatewayStartResult, String> {
    let hermes_home = state.hermes_home()?;

    Ok(gateway::start_gateway(
        &state.gateway,
        &hermes_home,
        profile.as_deref(),
    ))
}

/// Stop gateway
#[tauri::command]
async fn stop_gateway_cmd(
    state: State<'_, AppState>,
    profile: Option<String>,
) -> Result<bool, String> {
    gateway::stop_gateway(&state.gateway, profile.as_deref())?;
    Ok(true)
}

/// Check gateway status
#[tauri::command]
async fn gateway_status_cmd(
    state: State<'_, AppState>,
    profile: Option<String>,
) -> Result<bool, String> {
    Ok(gateway::is_gateway_running(&state.gateway, profile.as_deref()))
}

/// Get gateway port
#[tauri::command]
async fn get_gateway_port_cmd(
    state: State<'_, AppState>,
    profile: Option<String>,
) -> Result<Option<u16>, String> {
    Ok(gateway::get_gateway_port(&state.gateway, profile.as_deref()))
}

/// Start gateway on remote machine via SSH
#[tauri::command]
async fn start_remote_gateway_cmd(
    ssh_config: SshConfig,
    python_path: String,
) -> Result<GatewayStartResult, String> {
    // Defense against shell injection: python_path is interpolated into a
    // remote shell command, so it must be a plain filesystem path with no
    // shell metacharacters.
    if !ssh::is_safe_shell_path(&python_path) {
        return Err(format!(
            "Refused remote gateway start: python path '{}' is not a safe shell path",
            python_path
        ));
    }

    let remote_port = ssh_config.remote_port;

    // Start remote gateway via SSH in background (ADR-002). The Hermes gateway
    // is a `hermes gateway` subcommand, not a `python -m hermes` module, and it
    // does not take a `--port` flag (the port comes from API_SERVER_PORT or
    // config.yaml). Prefer the `hermes` launcher on PATH, else fall back to
    // invoking the CLI module through the interpreter. remote_port (u16) is
    // safe to interpolate.
    let cmd = format!(
        "nohup sh -c 'API_SERVER_PORT={} hermes gateway 2>/dev/null || API_SERVER_PORT={} {} -m hermes_cli.main gateway' > /tmp/gateway.log 2>&1 &",
        remote_port, remote_port, python_path
    );

    ssh::ssh_exec(&ssh_config, &cmd, 10)?;

    // Wait for gateway to start (async, does not block the Tauri runtime).
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    // Check if gateway is running on remote
    let check_cmd = format!("curl -sf http://127.0.0.1:{}/health 2>/dev/null || echo fail", remote_port);
    let output = ssh::ssh_exec(&ssh_config, &check_cmd, 5)?;

    let success = !output.trim().is_empty() && !output.contains("fail");

    Ok(GatewayStartResult {
        success,
        running: success,
        already_running: None,
        error: if success { None } else { Some("Remote gateway health check failed".to_string()) },
        log_path: Some("/tmp/gateway.log".to_string()),
    })
}

// ── Chat Commands ─────────────────────────────────────────────────────────

/// Send chat message
#[tauri::command]
async fn send_message_cmd(
    state: State<'_, AppState>,
    app_handle: AppHandle,
    request: SendMessageRequest,
) -> Result<String, String> {
    let hermes_home = state.hermes_home()?;

    let conn_cfg = config::read_desktop_config(&hermes_home);

    let mode = match conn_cfg.mode.as_str() {
        "remote" => ConnectionMode::Remote,
        "ssh" => ConnectionMode::Ssh,
        _ => ConnectionMode::Local,
    };

    // API key now lives in the OS keyring (migrated out of desktop.json).
    let remote_api_key = config::get_remote_api_key();

    send_message(
        &state.gateway,
        &state.ssh,
        &hermes_home,
        &mode,
        &conn_cfg.remote_url,
        &remote_api_key,
        &Some(conn_cfg.ssh),
        request,
        &app_handle,
    )
    .await
}

// ── Session Commands ──────────────────────────────────────────────────────

/// List sessions
#[tauri::command]
async fn list_sessions_cmd(
    state: State<'_, AppState>,
    profile: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Vec<SessionSummary>, String> {
    let hermes_home = state.hermes_home()?;

    sessions::list_sessions(
        &hermes_home,
        profile.as_deref(),
        limit.unwrap_or(50),
        offset.unwrap_or(0),
    )
    .map_err(|e| format!("SQLite error: {}", e))
}

/// Get session messages
#[tauri::command]
async fn get_session_messages_cmd(
    state: State<'_, AppState>,
    session_id: String,
    profile: Option<String>,
) -> Result<Vec<SessionMessage>, String> {
    let hermes_home = state.hermes_home()?;

    sessions::get_session_messages(&hermes_home, profile.as_deref(), &session_id)
        .map_err(|e| format!("SQLite error: {}", e))
}

/// Search sessions
#[tauri::command]
async fn search_sessions_cmd(
    state: State<'_, AppState>,
    query: String,
    limit: Option<i64>,
    profile: Option<String>,
) -> Result<Vec<sessions::SearchResult>, String> {
    let hermes_home = state.hermes_home()?;

    sessions::search_sessions(
        &hermes_home,
        profile.as_deref(),
        &query,
        limit.unwrap_or(20),
    )
    .map_err(|e| format!("SQLite error: {}", e))
}

/// Delete session
#[tauri::command]
async fn delete_session_cmd(
    state: State<'_, AppState>,
    session_id: String,
    profile: Option<String>,
) -> Result<(), String> {
    let hermes_home = state.hermes_home()?;

    sessions::delete_session(&hermes_home, profile.as_deref(), &session_id)
        .map_err(|e| format!("SQLite error: {}", e))
}

/// Get session stats
#[tauri::command]
async fn get_session_stats_cmd(
    state: State<'_, AppState>,
    profile: Option<String>,
) -> Result<SessionStats, String> {
    let hermes_home = state.hermes_home()?;

    sessions::get_session_stats(&hermes_home, profile.as_deref())
        .map_err(|e| format!("SQLite error: {}", e))
}

// ── Profile Commands ──────────────────────────────────────────────────────

/// List profiles
#[tauri::command]
async fn list_profiles_cmd(
    state: State<'_, AppState>,
) -> Result<Vec<ProfileInfo>, String> {
    let hermes_home = state.hermes_home()?;

    let active = profiles::get_active_profile(&hermes_home);
    Ok(profiles::list_profiles(&hermes_home, active.as_deref()))
}

/// Create profile
#[tauri::command]
async fn create_profile_cmd(
    state: State<'_, AppState>,
    name: String,
    clone: bool,
) -> Result<(), String> {
    let hermes_home = state.hermes_home()?;

    profiles::create_profile(&hermes_home, &name, clone)
}

/// Delete profile
#[tauri::command]
async fn delete_profile_cmd(
    state: State<'_, AppState>,
    name: String,
) -> Result<(), String> {
    let hermes_home = state.hermes_home()?;

    profiles::delete_profile(&hermes_home, &name)
}

/// Set active profile
#[tauri::command]
async fn set_active_profile_cmd(
    state: State<'_, AppState>,
    name: String,
) -> Result<(), String> {
    let hermes_home = state.hermes_home()?;

    profiles::set_active_profile(&hermes_home, &name)
}

// ── Model Commands ────────────────────────────────────────────────────────

/// List models
#[tauri::command]
async fn list_models_cmd(
    state: State<'_, AppState>,
) -> Result<Vec<SavedModel>, String> {
    let hermes_home = state.hermes_home()?;

    Ok(models::list_models(&hermes_home))
}

/// Add model
#[tauri::command]
async fn add_model_cmd(
    state: State<'_, AppState>,
    name: String,
    provider: String,
    model: String,
    base_url: String,
) -> Result<SavedModel, String> {
    let hermes_home = state.hermes_home()?;

    models::add_model(&hermes_home, &name, &provider, &model, &base_url)
}

/// Remove model
#[tauri::command]
async fn remove_model_cmd(
    state: State<'_, AppState>,
    id: String,
) -> Result<bool, String> {
    let hermes_home = state.hermes_home()?;

    models::remove_model(&hermes_home, &id)
}

/// Update model
#[tauri::command]
async fn update_model_cmd(
    state: State<'_, AppState>,
    id: String,
    fields: std::collections::HashMap<String, String>,
) -> Result<bool, String> {
    let hermes_home = state.hermes_home()?;

    models::update_model(&hermes_home, &id, &fields)
}

// ── Config Commands ───────────────────────────────────────────────────────

/// Get env vars
#[tauri::command]
async fn get_env_cmd(
    state: State<'_, AppState>,
    profile: Option<String>,
) -> Result<std::collections::HashMap<String, String>, String> {
    let hermes_home = state.hermes_home()?;

    Ok(config::read_env(&hermes_home, profile.as_deref()))
}

/// Set env var
#[tauri::command]
async fn set_env_cmd(
    state: State<'_, AppState>,
    key: String,
    value: String,
    profile: Option<String>,
) -> Result<(), String> {
    let hermes_home = state.hermes_home()?;

    config::write_env_value(&hermes_home, profile.as_deref(), &key, &value)
}

/// Get model config
#[tauri::command]
async fn get_model_config_cmd(
    state: State<'_, AppState>,
    profile: Option<String>,
) -> Result<config::ModelConfig, String> {
    let hermes_home = state.hermes_home()?;

    Ok(config::get_model_config(&hermes_home, profile.as_deref()))
}

/// Set model config
#[tauri::command]
async fn set_model_config_cmd(
    state: State<'_, AppState>,
    provider: String,
    model: String,
    base_url: String,
    profile: Option<String>,
) -> Result<(), String> {
    let hermes_home = state.hermes_home()?;

    config::set_model_config(&hermes_home, profile.as_deref(), &provider, &model, &base_url)
}

// ── Skills Commands ───────────────────────────────────────────────────────

/// List installed skills
#[tauri::command]
async fn list_installed_skills_cmd(
    state: State<'_, AppState>,
    profile: Option<String>,
) -> Result<Vec<skills::InstalledSkill>, String> {
    let hermes_home = state.hermes_home()?;

    Ok(skills::list_installed_skills(&hermes_home, profile.as_deref()))
}

/// Get skill content
#[tauri::command]
async fn get_skill_content_cmd(
    state: State<'_, AppState>,
    skill_name: String,
    profile: Option<String>,
) -> Result<String, String> {
    let hermes_home = state.hermes_home()?;
    skills::get_skill_content(&hermes_home, profile.as_deref(), &skill_name)
}

/// Install skill
#[tauri::command]
async fn install_skill_cmd(
    state: State<'_, AppState>,
    identifier: String,
    profile: Option<String>,
) -> Result<(), String> {
    let hermes_home = state.hermes_home()?;

    skills::install_skill(&hermes_home, profile.as_deref(), &identifier)
}

/// Uninstall skill
#[tauri::command]
async fn uninstall_skill_cmd(
    state: State<'_, AppState>,
    name: String,
    profile: Option<String>,
) -> Result<(), String> {
    let hermes_home = state.hermes_home()?;

    skills::uninstall_skill(&hermes_home, profile.as_deref(), &name)
}

// ── Memory Commands ──────────────────────────────────────────────────────

/// Read memory (memory.md + user.md + stats)
#[tauri::command]
async fn read_memory_cmd(
    state: State<'_, AppState>,
    profile: Option<String>,
) -> Result<memory::MemoryReadResult, String> {
    let hermes_home = state.hermes_home()?;
    Ok(memory::read_memory(&hermes_home, profile.as_deref()))
}

/// Write user profile (user.md)
#[tauri::command]
async fn write_user_profile_cmd(
    state: State<'_, AppState>,
    content: String,
    profile: Option<String>,
) -> Result<(), String> {
    let hermes_home = state.hermes_home()?;
    memory::write_user_profile(&hermes_home, profile.as_deref(), &content)
}

/// Add memory entry
#[tauri::command]
async fn add_memory_entry_cmd(
    state: State<'_, AppState>,
    content: String,
    profile: Option<String>,
) -> Result<(), String> {
    let hermes_home = state.hermes_home()?;
    memory::add_memory_entry(&hermes_home, profile.as_deref(), &content)
}

/// Update memory entry
#[tauri::command]
async fn update_memory_entry_cmd(
    state: State<'_, AppState>,
    index: usize,
    content: String,
    profile: Option<String>,
) -> Result<(), String> {
    let hermes_home = state.hermes_home()?;
    memory::update_memory_entry(&hermes_home, profile.as_deref(), index, &content)
}

/// Remove memory entry
#[tauri::command]
async fn remove_memory_entry_cmd(
    state: State<'_, AppState>,
    index: usize,
    profile: Option<String>,
) -> Result<(), String> {
    let hermes_home = state.hermes_home()?;
    memory::remove_memory_entry(&hermes_home, profile.as_deref(), index)
}

// ── SSH Commands ──────────────────────────────────────────────────────────

/// Start SSH tunnel
#[tauri::command]
async fn start_ssh_tunnel_cmd(
    state: State<'_, AppState>,
    ssh_config: SshConfig,
) -> Result<(), String> {
    ssh::start_ssh_tunnel(&state.ssh, &ssh_config)
}

/// Stop SSH tunnel
#[tauri::command]
async fn stop_ssh_tunnel_cmd(
    state: State<'_, AppState>,
) -> Result<(), String> {
    ssh::stop_ssh_tunnel(&state.ssh);
    Ok(())
}

/// Check SSH tunnel status
#[tauri::command]
async fn ssh_tunnel_status_cmd(
    state: State<'_, AppState>,
) -> Result<bool, String> {
    Ok(ssh::is_tunnel_active(&state.ssh))
}

// ── Telegram Commands ─────────────────────────────────────────────────────

/// Send Telegram message
#[tauri::command]
async fn send_telegram_message_cmd(
    _state: State<'_, AppState>,
    bot_token: String,
    chat_id: String,
    text: String,
) -> Result<telegram::TelegramResult, String> {
    Ok(telegram::send_message(&bot_token, &chat_id, &text).await)
}

/// Validate Telegram bot token
#[tauri::command]
async fn validate_telegram_bot_token_cmd(
    bot_token: String,
) -> Result<telegram::TelegramResult, String> {
    Ok(telegram::validate_bot_token(&bot_token).await)
}

/// Save Telegram config
#[tauri::command]
async fn save_telegram_config_cmd(
    state: State<'_, AppState>,
    config: telegram::TelegramConfig,
) -> Result<(), String> {
    let hermes_home = state.hermes_home()?;
    telegram::save_config(&hermes_home, &config)
}

/// Load Telegram config
#[tauri::command]
async fn load_telegram_config_cmd(
    state: State<'_, AppState>,
) -> Result<telegram::TelegramConfig, String> {
    let hermes_home = state.hermes_home()?;
    Ok(telegram::load_config(&hermes_home))
}

// ── Media Commands ────────────────────────────────────────────────────────

/// Get media info for a file
#[tauri::command]
async fn get_media_info_cmd(path: String) -> Result<Option<media::MediaInfo>, String> {
    Ok(media::get_media_info(&path))
}

/// Read file as base64 data URL
#[tauri::command]
async fn read_media_data_url_cmd(path: String) -> Result<Option<String>, String> {
    Ok(media::read_as_data_url(&path))
}

/// List media files in directory
#[tauri::command]
async fn list_media_files_cmd(dir: String) -> Result<Vec<media::MediaInfo>, String> {
    Ok(media::list_media_files(&dir))
}

/// Save a media blob (voice clip or attachment) to the instance media cache.
/// Accepts raw bytes + extension, returns the saved file path.
#[tauri::command]
async fn save_media_blob_cmd(
    state: State<'_, AppState>,
    data: Vec<u8>,
    ext: String,
) -> Result<String, String> {
    let hermes_home = state.hermes_home()?;
    let path = media::save_media_blob(&hermes_home, &data, &ext)?;
    Ok(path.to_string_lossy().to_string())
}

// ── Model Discovery Commands ──────────────────────────────────────────────

/// Discover models from provider
#[tauri::command]
async fn discover_models_cmd(
    provider: String,
    base_url: Option<String>,
    api_key: Option<String>,
) -> Result<model_discovery::DiscoveryResult, String> {
    Ok(model_discovery::discover_models(&provider, base_url.as_deref(), api_key.as_deref()).await)
}

/// Check if provider supports model discovery
#[tauri::command]
async fn is_discoverable_cmd(provider: String) -> Result<bool, String> {
    Ok(model_discovery::is_discoverable(&provider))
}

/// Get OAuth provider models
#[tauri::command]
async fn get_oauth_models_cmd(provider: String) -> Result<Vec<model_discovery::DiscoveredModel>, String> {
    Ok(model_discovery::get_oauth_models(&provider))
}

// ── Terminal Commands ─────────────────────────────────────────────────────

/// Open terminal in directory
#[tauri::command]
async fn open_terminal_cmd(cwd: String) -> Result<terminal::TerminalResult, String> {
    Ok(terminal::open_terminal(&cwd))
}

// ── Provider Registry Commands ────────────────────────────────────────────

/// Get canonical base URL for provider
#[tauri::command]
async fn get_provider_base_url_cmd(provider: String) -> Result<Option<String>, String> {
    Ok(provider_registry::canonical_base_url(&provider).map(|s| s.to_string()))
}

/// Get all provider base URLs
#[tauri::command]
async fn get_all_provider_urls_cmd() -> Result<std::collections::HashMap<String, String>, String> {
    Ok(provider_registry::all_provider_urls())
}

// ── Registry Commands ─────────────────────────────────────────────────────

/// Fetch registry catalog from GitHub
#[tauri::command]
async fn fetch_registry_catalog_cmd() -> Result<registry::RegistryCatalog, String> {
    registry::fetch_catalog().await
}

/// Get installed registry items
#[tauri::command]
async fn get_installed_registry_cmd(
    state: State<'_, AppState>,
    profile: Option<String>,
) -> Result<registry::InstalledRegistry, String> {
    let hermes_home = state.hermes_home()?;
    Ok(registry::get_installed(&hermes_home, profile.as_deref()))
}

/// Install from registry
#[tauri::command]
async fn install_from_registry_cmd(
    state: State<'_, AppState>,
    item: registry::RegistryItem,
    profile: Option<String>,
) -> Result<(), String> {
    let hermes_home = state.hermes_home()?;
    registry::install_from_registry(&hermes_home, profile.as_deref(), &item)
}

// ── Kanban Commands ───────────────────────────────────────────────────────

/// List kanban boards
#[tauri::command]
async fn list_kanban_boards_cmd(
    state: State<'_, AppState>,
    profile: Option<String>,
) -> Result<Vec<kanban::KanbanBoard>, String> {
    let hermes_home = state.hermes_home()?;
    kanban::list_boards(&hermes_home, profile.as_deref())
}

/// Create a kanban board
#[tauri::command]
async fn create_kanban_board_cmd(
    state: State<'_, AppState>,
    slug: String,
    name: String,
    description: Option<String>,
    profile: Option<String>,
) -> Result<kanban::KanbanBoard, String> {
    let hermes_home = state.hermes_home()?;
    kanban::create_board(&hermes_home, profile.as_deref(), &slug, &name, description.as_deref())
}

/// Delete a kanban board
#[tauri::command]
async fn delete_kanban_board_cmd(
    state: State<'_, AppState>,
    slug: String,
    profile: Option<String>,
) -> Result<bool, String> {
    let hermes_home = state.hermes_home()?;
    kanban::delete_board(&hermes_home, profile.as_deref(), &slug)
}

/// List tasks for a board
#[tauri::command]
async fn list_kanban_tasks_cmd(
    state: State<'_, AppState>,
    board_slug: String,
    profile: Option<String>,
) -> Result<kanban::KanbanBoardView, String> {
    let hermes_home = state.hermes_home()?;
    kanban::list_tasks(&hermes_home, profile.as_deref(), &board_slug)
}

/// Create a kanban task
#[tauri::command]
async fn create_kanban_task_cmd(
    state: State<'_, AppState>,
    board_slug: String,
    title: String,
    body: Option<String>,
    status: String,
    profile: Option<String>,
) -> Result<kanban::KanbanTask, String> {
    let hermes_home = state.hermes_home()?;
    kanban::create_task(&hermes_home, profile.as_deref(), &board_slug, &title, body.as_deref(), &status)
}

/// Update a kanban task
#[tauri::command]
async fn update_kanban_task_cmd(
    state: State<'_, AppState>,
    task_id: String,
    fields: std::collections::HashMap<String, String>,
    profile: Option<String>,
) -> Result<bool, String> {
    let hermes_home = state.hermes_home()?;
    kanban::update_task(&hermes_home, profile.as_deref(), &task_id, &fields)
}

/// Delete a kanban task
#[tauri::command]
async fn delete_kanban_task_cmd(
    state: State<'_, AppState>,
    task_id: String,
    profile: Option<String>,
) -> Result<bool, String> {
    let hermes_home = state.hermes_home()?;
    kanban::delete_task(&hermes_home, profile.as_deref(), &task_id)
}

/// Move a task to a different status
#[tauri::command]
async fn move_kanban_task_cmd(
    state: State<'_, AppState>,
    task_id: String,
    new_status: String,
    profile: Option<String>,
) -> Result<bool, String> {
    let hermes_home = state.hermes_home()?;
    kanban::move_task(&hermes_home, profile.as_deref(), &task_id, &new_status)
}

// ── Config Health Commands ────────────────────────────────────────────────

/// Run config health check
#[tauri::command]
async fn config_health_check_cmd(
    state: State<'_, AppState>,
    profile: Option<String>,
) -> Result<config::ConfigHealthReport, String> {
    let hermes_home = state.hermes_home()?;
    Ok(config::run_config_health_check(&hermes_home, profile.as_deref()))
}

/// Auto-fix a config health issue
#[tauri::command]
async fn auto_fix_config_cmd(
    state: State<'_, AppState>,
    code: String,
    profile: Option<String>,
) -> Result<String, String> {
    let hermes_home = state.hermes_home()?;
    config_health::auto_fix_issue(&hermes_home, &code, profile.as_deref())
}

// ── Validation Commands ───────────────────────────────────────────────────

/// Validate chat readiness — pre-send check
#[tauri::command]
async fn validate_chat_readiness_cmd(
    state: State<'_, AppState>,
    profile: Option<String>,
) -> Result<validation::ChatReadiness, String> {
    let hermes_home = state.hermes_home()?;
    Ok(validation::validate_chat_readiness(&hermes_home, profile.as_deref()))
}

// ── Cron Jobs Commands ───────────────────────────────────────────────────

/// List cron jobs
#[tauri::command]
async fn list_cron_jobs_cmd(
    state: State<'_, AppState>,
    include_disabled: Option<bool>,
    profile: Option<String>,
) -> Result<Vec<cronjobs::CronJob>, String> {
    let hermes_home = state.hermes_home()?;
    Ok(cronjobs::list_cron_jobs(&hermes_home, profile.as_deref(), include_disabled.unwrap_or(true)))
}

/// Create cron job
#[tauri::command]
async fn create_cron_job_cmd(
    state: State<'_, AppState>,
    schedule: String,
    prompt: Option<String>,
    name: Option<String>,
    deliver: Option<String>,
    profile: Option<String>,
) -> Result<cronjobs::CronJob, String> {
    let hermes_home = state.hermes_home()?;
    cronjobs::create_cron_job(&hermes_home, profile.as_deref(), &schedule, prompt.as_deref(), name.as_deref(), deliver.as_deref())
}

/// Remove cron job
#[tauri::command]
async fn remove_cron_job_cmd(
    state: State<'_, AppState>,
    job_id: String,
    profile: Option<String>,
) -> Result<(), String> {
    let hermes_home = state.hermes_home()?;
    cronjobs::remove_cron_job(&hermes_home, profile.as_deref(), &job_id)
}

/// Pause cron job
#[tauri::command]
async fn pause_cron_job_cmd(
    state: State<'_, AppState>,
    job_id: String,
    profile: Option<String>,
) -> Result<(), String> {
    let hermes_home = state.hermes_home()?;
    cronjobs::pause_cron_job(&hermes_home, profile.as_deref(), &job_id)
}

/// Resume cron job
#[tauri::command]
async fn resume_cron_job_cmd(
    state: State<'_, AppState>,
    job_id: String,
    profile: Option<String>,
) -> Result<(), String> {
    let hermes_home = state.hermes_home()?;
    cronjobs::resume_cron_job(&hermes_home, profile.as_deref(), &job_id)
}

/// Trigger cron job
#[tauri::command]
async fn trigger_cron_job_cmd(
    state: State<'_, AppState>,
    job_id: String,
    profile: Option<String>,
) -> Result<String, String> {
    let hermes_home = state.hermes_home()?;
    cronjobs::trigger_cron_job(&hermes_home, profile.as_deref(), &job_id)
}

// ── Auth Commands ─────────────────────────────────────────────────────────

/// Start OAuth login flow for a provider
#[tauri::command]
async fn auth_login_cmd(
    state: tauri::State<'_, AppState>,
    app_handle: AppHandle,
    provider: String,
    profile: Option<String>,
) -> Result<auth::OAuthLoginResult, String> {
    let hermes_home = state.hermes_home()?;

    let (hermes_python, _hermes_repo) = gateway::find_hermes_python()
        .map_err(|e| format!("Hermes Python not found: {}", e))?;

    auth::run_oauth_login(
        app_handle,
        hermes_home,
        hermes_python,
        provider,
        profile,
        &state.auth,
    )
    .await
}

/// Cancel in-flight OAuth login
#[tauri::command]
async fn auth_cancel_cmd(
    state: tauri::State<'_, AppState>,
) -> Result<bool, String> {
    auth::cancel_oauth_login(&state.auth).await
}

/// Store credential in OS keyring
#[tauri::command]
async fn store_credential_cmd(
    service: String,
    account: String,
    password: String,
) -> Result<(), String> {
    auth::store_credential(service, account, password).await
}

/// Get credential from OS keyring
#[tauri::command]
async fn get_credential_cmd(
    service: String,
    account: String,
) -> Result<Option<String>, String> {
    auth::get_credential(service, account).await
}

/// Delete credential from OS keyring
#[tauri::command]
async fn delete_credential_cmd(
    service: String,
    account: String,
) -> Result<(), String> {
    auth::delete_credential(service, account).await
}

// ── Credential Pool Commands ──────────────────────────────────────────────

/// Get credential pool for all providers
#[tauri::command]
async fn get_credential_pool_cmd(
    state: tauri::State<'_, AppState>,
) -> Result<std::collections::HashMap<String, Vec<auth::CredentialPoolEntry>>, String> {
    let hermes_home = state.hermes_home()?;
    auth::get_credential_pool(&hermes_home).await
}

/// Add credential pool entry
#[tauri::command]
async fn add_credential_pool_entry_cmd(
    state: tauri::State<'_, AppState>,
    provider: String,
    key: String,
    label: String,
) -> Result<Vec<auth::CredentialPoolEntry>, String> {
    let hermes_home = state.hermes_home()?;
    auth::add_credential_pool_entry(&hermes_home, &provider, &key, &label).await
}

/// Set credential pool for a provider
#[tauri::command]
async fn set_credential_pool_cmd(
    state: tauri::State<'_, AppState>,
    provider: String,
    entries: Vec<auth::CredentialPoolEntry>,
) -> Result<(), String> {
    let hermes_home = state.hermes_home()?;
    auth::set_credential_pool(&hermes_home, &provider, &entries).await
}

// ── Entry Point ───────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Single-instance: a second launch focuses the existing window instead
        // of starting a duplicate process (which would also fail to register
        // the global hotkey a second time and panic).
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            // get_webview_window comes from the Manager trait.
            use tauri::Manager;
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .manage(AppState::new())
        .manage(install::InstallState::default())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .setup(|app| {
            // ── Tray icon (best-effort) ───────────────────────────────
            // On some Linux DEs (notably Astra Linux / older libayatana-
            // appindicator), building the tray icon fails with a raw-data
            // size mismatch ("wrong data size, expected 4096 got 8192").
            // The tray is a convenience, not a requirement — if it cannot be
            // created, the app must still start. So we swallow the error
            // instead of propagating it via `?` (which would abort startup).
            use tauri::menu::{Menu, MenuItem};
            use tauri::tray::TrayIconBuilder;
            use tauri::Manager;

            let tray_result = (|| -> Result<(), tauri::Error> {
                let show_item = MenuItem::with_id(app, "show", "Показать", true, None::<&str>)?;
                let hide_item = MenuItem::with_id(app, "hide", "Скрыть", true, None::<&str>)?;
                let quit_item = MenuItem::with_id(app, "quit", "Выход", true, None::<&str>)?;
                let menu = Menu::with_items(app, &[&show_item, &hide_item, &quit_item])?;

                let icon = match app.default_window_icon() {
                    Some(i) => i.clone(),
                    None => {
                        return Err(tauri::Error::AssetNotFound(
                            "default window icon".to_string(),
                        ))
                    }
                };
                let builder = TrayIconBuilder::new()
                    .icon(icon)
                    .tooltip("Штурман")
                    .menu(&menu)
                    .on_menu_event(|app, event| match event.id.as_ref() {
                        "show" => {
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                        "hide" => {
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.hide();
                            }
                        }
                        "quit" => {
                            app.exit(0);
                        }
                        _ => {}
                    })
                    .on_tray_icon_event(|tray, event| {
                        if let tauri::tray::TrayIconEvent::Click { button: tauri::tray::MouseButton::Left, .. } = event {
                            let app = tray.app_handle();
                            if let Some(window) = app.get_webview_window("main") {
                                if window.is_visible().unwrap_or(false) {
                                    let _ = window.hide();
                                } else {
                                    let _ = window.show();
                                    let _ = window.set_focus();
                                }
                            }
                        }
                    });
                builder.build(app)?;
                Ok(())
            })();

            if let Err(e) = tray_result {
                eprintln!("[steersman] warning: tray icon unavailable on this platform, continuing without it: {}", e);
            }

            // ── Global shortcut: Ctrl+Shift+S (best-effort) ────────────
            // If the shortcut is already taken (e.g. by another app or a stale
            // instance), registration fails — but that must not abort startup.
            use tauri_plugin_global_shortcut::GlobalShortcutExt;
            if let Err(e) = app.global_shortcut().on_shortcut(
                "Control+Shift+S",
                move |app, _shortcut, _event| {
                    if let Some(window) = app.get_webview_window("main") {
                        if window.is_visible().unwrap_or(false) {
                            let _ = window.hide();
                        } else {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                },
            ) {
                eprintln!(
                    "[steersman] warning: global shortcut Ctrl+Shift+S unavailable, continuing without it: {}",
                    e
                );
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // App
            init_app,
            detect_instances,
            check_python_path,
            detect_local_instances_cmd,
            connect_to_instance,
            auto_connect_local_cmd,
            // Soul / personality / provider-key (onboarding + Settings → Soul)
            read_soul_cmd,
            write_soul_cmd,
            reset_soul_cmd,
            get_personalities_cmd,
            get_personality_cmd,
            set_personality_cmd,
            save_provider_key_cmd,
            // Local Hermes install (onboarding wizard)
            install::install_hermes_cmd,
            // On-the-fly voice transcription (Groq/OpenAI Whisper)
            stt::transcribe_audio_cmd,
            detect_remote_instances_cmd,
            get_app_version,
            get_versions_cmd,
            // Connection
            get_connection_config,
            set_connection_config,
            test_connection,
            // Gateway
            start_gateway_cmd,
            stop_gateway_cmd,
            gateway_status_cmd,
            get_gateway_port_cmd,
            // Chat
            send_message_cmd,
            // Sessions
            list_sessions_cmd,
            get_session_messages_cmd,
            search_sessions_cmd,
            delete_session_cmd,
            get_session_stats_cmd,
            // Profiles
            list_profiles_cmd,
            create_profile_cmd,
            delete_profile_cmd,
            set_active_profile_cmd,
            // Models
            list_models_cmd,
            add_model_cmd,
            remove_model_cmd,
            update_model_cmd,
            // Config
            get_env_cmd,
            set_env_cmd,
            get_model_config_cmd,
            set_model_config_cmd,
            // Skills
            list_installed_skills_cmd,
            get_skill_content_cmd,
            install_skill_cmd,
            uninstall_skill_cmd,
            // Memory
            read_memory_cmd,
            write_user_profile_cmd,
            add_memory_entry_cmd,
            update_memory_entry_cmd,
            remove_memory_entry_cmd,
            // SSH
            start_ssh_tunnel_cmd,
            stop_ssh_tunnel_cmd,
            ssh_tunnel_status_cmd,
            start_remote_gateway_cmd,
            // Telegram
            send_telegram_message_cmd,
            validate_telegram_bot_token_cmd,
            save_telegram_config_cmd,
            load_telegram_config_cmd,
            // Media
            get_media_info_cmd,
            read_media_data_url_cmd,
            list_media_files_cmd,
            save_media_blob_cmd,
            // Model Discovery
            discover_models_cmd,
            is_discoverable_cmd,
            get_oauth_models_cmd,
            // Terminal
            open_terminal_cmd,
            // Provider Registry
            get_provider_base_url_cmd,
            get_all_provider_urls_cmd,
            // Registry
            fetch_registry_catalog_cmd,
            get_installed_registry_cmd,
            install_from_registry_cmd,
            // Kanban
            list_kanban_boards_cmd,
            create_kanban_board_cmd,
            delete_kanban_board_cmd,
            list_kanban_tasks_cmd,
            create_kanban_task_cmd,
            update_kanban_task_cmd,
            delete_kanban_task_cmd,
            move_kanban_task_cmd,
            // Validation
            validate_chat_readiness_cmd,
            // Cron Jobs
            list_cron_jobs_cmd,
            create_cron_job_cmd,
            remove_cron_job_cmd,
            pause_cron_job_cmd,
            resume_cron_job_cmd,
            trigger_cron_job_cmd,
            // Config Health
            config_health_check_cmd,
            auto_fix_config_cmd,
            // Auth
            auth_login_cmd,
            auth_cancel_cmd,
            store_credential_cmd,
            get_credential_cmd,
            delete_credential_cmd,
            // Credential Pool
            get_credential_pool_cmd,
            add_credential_pool_entry_cmd,
            set_credential_pool_cmd,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Verify credential pool sync: add a Groq key (as the UI would) → it lands
    /// in auth.json credential_pool → STT can resolve it.
    ///   cargo test --lib credential_sync_groq -- --nocapture --ignored
    #[tokio::test]
    #[ignore]
    async fn credential_sync_groq() {
        let hh: PathBuf = r"C:\Users\n.gusev\AppData\Local\hermes".into();
        let key = "gsk_test_credential_sync_placeholder";

        println!("=== before ===");
        let pool = auth::get_credential_pool(&hh).await.unwrap();
        println!("groq entries: {:?}", pool.get("groq").map(|v| v.len()));

        auth::add_credential_pool_entry(&hh, "groq", key, "GROQ_API_KEY")
            .await
            .unwrap();

        println!("=== after add ===");
        let pool = auth::get_credential_pool(&hh).await.unwrap();
        let groq = pool.get("groq").unwrap();
        println!("groq entries now: {}", groq.len());
        for e in groq {
            println!(
                "  label={:?} source={:?} fingerprint={:?} resolved={:?}",
                e.label,
                e.source,
                e.secret_fingerprint,
                e.resolve_secret(&hh).is_some()
            );
        }

        // Cleanup: remove the test entry so we don't leave junk in auth.json.
        let remaining: Vec<auth::CredentialPoolEntry> = groq
            .iter()
            .filter(|e| e.label.as_deref() != Some("GROQ_API_KEY") || e.source.as_deref() == Some("manual"))
            .cloned()
            .collect();
        // Keep only non-test entries.
        let real: Vec<_> = remaining
            .into_iter()
            .filter(|e| e.access_token.as_deref() != Some(key))
            .collect();
        auth::set_credential_pool(&hh, "groq", &real).await.unwrap();
        println!("cleaned up test entry");
    }
}
