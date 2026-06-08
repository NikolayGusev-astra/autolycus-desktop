// src-tauri/src/lib.rs
// Autolycus Desktop v0.4.0 — Rust backend
// Ported from fathah/hermes-desktop (v0.5.8)

mod chat;
mod config;
mod gateway;
mod models;
mod profiles;
mod sessions;
mod ssh;

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

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
    pub hermes_home: Arc<Mutex<Option<PathBuf>>>,
}

use std::sync::Mutex;

impl AppState {
    fn new() -> Self {
        Self {
            gateway: GatewayState::new(),
            ssh: SshState::new(),
            hermes_home: Arc::new(Mutex::new(None)),
        }
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

// ── Tauri Commands ────────────────────────────────────────────────────────

/// Initialize app — resolve hermes home, detect instances
#[tauri::command]
async fn init_app(state: State<'_, AppState>) -> Result<InitResult, String> {
    let hermes_home = config::resolve_hermes_home();
    *state.hermes_home.lock().unwrap() = Some(hermes_home.clone());

    Ok(InitResult {
        hermes_home: hermes_home.to_string_lossy().to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

#[derive(Debug, Serialize)]
struct InitResult {
    hermes_home: String,
    version: String,
}

/// Detect available Python instances
#[tauri::command]
async fn detect_instances() -> Result<Vec<InstanceInfo>, String> {
    let home = dirs::home_dir().ok_or("Cannot determine home directory")?;

    let candidates: Vec<(PathBuf, &str)> = vec![
        (home.join("autolycus/venv/bin/python"), "autolycus"),
        (home.join("autolycus/venv/bin/python3"), "autolycus"),
        (home.join(".autolycus/venv/bin/python"), "autolycus"),
        (home.join(".hermes/venv/bin/python"), "hermes"),
        (home.join(".hermes/hermes-agent/venv/bin/python"), "hermes-agent"),
        (PathBuf::from("/usr/local/bin/python3"), "system"),
    ];

    let mut result = Vec::new();
    for (path, instance) in candidates {
        result.push(InstanceInfo {
            path: path.to_string_lossy().to_string(),
            instance: instance.to_string(),
            exists: path.exists(),
        });
    }

    Ok(result)
}

/// Check if a Python path exists
#[tauri::command]
async fn check_python_path(path: String) -> Result<bool, String> {
    let expanded = config::expand_tilde(&path);
    Ok(PathBuf::from(&expanded).exists())
}

/// Get app version
#[tauri::command]
fn get_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

// ── Connection Commands ───────────────────────────────────────────────────

/// Get connection config
#[tauri::command]
async fn get_connection_config(state: State<'_, AppState>) -> Result<PublicConnectionConfig, String> {
    let hermes_home = state.hermes_home.lock().unwrap().clone()
        .ok_or("App not initialized")?;
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
    let hermes_home = state.hermes_home.lock().unwrap().clone()
        .ok_or("App not initialized")?;

    let cfg = ConnectionConfig {
        mode,
        remote_url,
        api_key,
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
    state: State<'_, AppState>,
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
    let hermes_home = state.hermes_home.lock().unwrap().clone()
        .ok_or("App not initialized")?;

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

// ── Chat Commands ─────────────────────────────────────────────────────────

/// Send chat message
#[tauri::command]
async fn send_message_cmd(
    state: State<'_, AppState>,
    app_handle: AppHandle,
    request: SendMessageRequest,
) -> Result<String, String> {
    let hermes_home = state.hermes_home.lock().unwrap().clone()
        .ok_or("App not initialized")?;

    let conn_cfg = config::read_desktop_config(&hermes_home);

    let mode = match conn_cfg.mode.as_str() {
        "remote" => ConnectionMode::Remote,
        "ssh" => ConnectionMode::Ssh,
        _ => ConnectionMode::Local,
    };

    send_message(
        &state.gateway,
        &state.ssh,
        &hermes_home,
        &mode,
        &conn_cfg.remote_url,
        &conn_cfg.api_key,
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
    let hermes_home = state.hermes_home.lock().unwrap().clone()
        .ok_or("App not initialized")?;

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
    let hermes_home = state.hermes_home.lock().unwrap().clone()
        .ok_or("App not initialized")?;

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
    let hermes_home = state.hermes_home.lock().unwrap().clone()
        .ok_or("App not initialized")?;

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
    let hermes_home = state.hermes_home.lock().unwrap().clone()
        .ok_or("App not initialized")?;

    sessions::delete_session(&hermes_home, profile.as_deref(), &session_id)
        .map_err(|e| format!("SQLite error: {}", e))
}

/// Get session stats
#[tauri::command]
async fn get_session_stats_cmd(
    state: State<'_, AppState>,
    profile: Option<String>,
) -> Result<SessionStats, String> {
    let hermes_home = state.hermes_home.lock().unwrap().clone()
        .ok_or("App not initialized")?;

    sessions::get_session_stats(&hermes_home, profile.as_deref())
        .map_err(|e| format!("SQLite error: {}", e))
}

// ── Profile Commands ──────────────────────────────────────────────────────

/// List profiles
#[tauri::command]
async fn list_profiles_cmd(
    state: State<'_, AppState>,
) -> Result<Vec<ProfileInfo>, String> {
    let hermes_home = state.hermes_home.lock().unwrap().clone()
        .ok_or("App not initialized")?;

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
    let hermes_home = state.hermes_home.lock().unwrap().clone()
        .ok_or("App not initialized")?;

    profiles::create_profile(&hermes_home, &name, clone)
}

/// Delete profile
#[tauri::command]
async fn delete_profile_cmd(
    state: State<'_, AppState>,
    name: String,
) -> Result<(), String> {
    let hermes_home = state.hermes_home.lock().unwrap().clone()
        .ok_or("App not initialized")?;

    profiles::delete_profile(&hermes_home, &name)
}

/// Set active profile
#[tauri::command]
async fn set_active_profile_cmd(
    state: State<'_, AppState>,
    name: String,
) -> Result<(), String> {
    let hermes_home = state.hermes_home.lock().unwrap().clone()
        .ok_or("App not initialized")?;

    profiles::set_active_profile(&hermes_home, &name)
}

// ── Model Commands ────────────────────────────────────────────────────────

/// List models
#[tauri::command]
async fn list_models_cmd(
    state: State<'_, AppState>,
) -> Result<Vec<SavedModel>, String> {
    let hermes_home = state.hermes_home.lock().unwrap().clone()
        .ok_or("App not initialized")?;

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
    let hermes_home = state.hermes_home.lock().unwrap().clone()
        .ok_or("App not initialized")?;

    models::add_model(&hermes_home, &name, &provider, &model, &base_url)
}

/// Remove model
#[tauri::command]
async fn remove_model_cmd(
    state: State<'_, AppState>,
    id: String,
) -> Result<bool, String> {
    let hermes_home = state.hermes_home.lock().unwrap().clone()
        .ok_or("App not initialized")?;

    models::remove_model(&hermes_home, &id)
}

/// Update model
#[tauri::command]
async fn update_model_cmd(
    state: State<'_, AppState>,
    id: String,
    fields: std::collections::HashMap<String, String>,
) -> Result<bool, String> {
    let hermes_home = state.hermes_home.lock().unwrap().clone()
        .ok_or("App not initialized")?;

    models::update_model(&hermes_home, &id, &fields)
}

// ── Config Commands ───────────────────────────────────────────────────────

/// Get env vars
#[tauri::command]
async fn get_env_cmd(
    state: State<'_, AppState>,
    profile: Option<String>,
) -> Result<std::collections::HashMap<String, String>, String> {
    let hermes_home = state.hermes_home.lock().unwrap().clone()
        .ok_or("App not initialized")?;

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
    let hermes_home = state.hermes_home.lock().unwrap().clone()
        .ok_or("App not initialized")?;

    config::write_env_value(&hermes_home, profile.as_deref(), &key, &value)
}

/// Get model config
#[tauri::command]
async fn get_model_config_cmd(
    state: State<'_, AppState>,
    profile: Option<String>,
) -> Result<config::ModelConfig, String> {
    let hermes_home = state.hermes_home.lock().unwrap().clone()
        .ok_or("App not initialized")?;

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
    let hermes_home = state.hermes_home.lock().unwrap().clone()
        .ok_or("App not initialized")?;

    config::set_model_config(&hermes_home, profile.as_deref(), &provider, &model, &base_url)
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

// ── Entry Point ───────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::new())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            // App
            init_app,
            detect_instances,
            check_python_path,
            get_app_version,
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
            // SSH
            start_ssh_tunnel_cmd,
            stop_ssh_tunnel_cmd,
            ssh_tunnel_status_cmd,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
