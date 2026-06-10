// src-tauri/src/lib.rs
// Autolycus Desktop v0.5.0 — Rust backend
// Ported from fathah/hermes-desktop (v0.5.8)

mod auth;
mod chat;
mod config;
mod config_health;
mod cronjobs;
mod discovery;
mod gateway;
mod kanban;
mod media;
mod memory;
mod model_discovery;
mod models;
mod mcp;
mod profiles;
mod provider_registry;
mod registry;
mod sessions;
mod skills;
mod ssh;
mod telegram;
mod terminal;
mod validation;

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
    pub auth: auth::AuthState,
}

use std::sync::Mutex;

impl AppState {
    fn new() -> Self {
        Self {
            gateway: GatewayState::new(),
            ssh: SshState::new(),
            hermes_home: Arc::new(Mutex::new(None)),
            auth: auth::AuthState::new(),
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

/// Detect local instances with version, gateway status, etc.
#[tauri::command]
async fn detect_local_instances_cmd() -> Result<Vec<discovery::DetectedInstance>, String> {
    Ok(discovery::detect_local_instances())
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

// ── Skills Commands ───────────────────────────────────────────────────────

/// List installed skills
#[tauri::command]
async fn list_installed_skills_cmd(
    state: State<'_, AppState>,
    profile: Option<String>,
) -> Result<Vec<skills::InstalledSkill>, String> {
    let hermes_home = state.hermes_home.lock().unwrap().clone()
        .ok_or("App not initialized")?;

    Ok(skills::list_installed_skills(&hermes_home, profile.as_deref()))
}

/// Get skill content
#[tauri::command]
async fn get_skill_content_cmd(
    skill_path: String,
) -> Result<String, String> {
    skills::get_skill_content(&skill_path)
}

/// Install skill
#[tauri::command]
async fn install_skill_cmd(
    state: State<'_, AppState>,
    identifier: String,
    profile: Option<String>,
) -> Result<(), String> {
    let hermes_home = state.hermes_home.lock().unwrap().clone()
        .ok_or("App not initialized")?;

    skills::install_skill(&hermes_home, profile.as_deref(), &identifier)
}

/// Uninstall skill
#[tauri::command]
async fn uninstall_skill_cmd(
    state: State<'_, AppState>,
    name: String,
    profile: Option<String>,
) -> Result<(), String> {
    let hermes_home = state.hermes_home.lock().unwrap().clone()
        .ok_or("App not initialized")?;

    skills::uninstall_skill(&hermes_home, profile.as_deref(), &name)
}

// ── Memory Commands ──────────────────────────────────────────────────────

/// Read memory (memory.md + user.md + stats)
#[tauri::command]
async fn read_memory_cmd(
    state: State<'_, AppState>,
    profile: Option<String>,
) -> Result<memory::MemoryReadResult, String> {
    let hermes_home = state.hermes_home.lock().unwrap().clone()
        .ok_or("App not initialized")?;
    Ok(memory::read_memory(&hermes_home, profile.as_deref()))
}

/// Write user profile (user.md)
#[tauri::command]
async fn write_user_profile_cmd(
    state: State<'_, AppState>,
    content: String,
    profile: Option<String>,
) -> Result<(), String> {
    let hermes_home = state.hermes_home.lock().unwrap().clone()
        .ok_or("App not initialized")?;
    memory::write_user_profile(&hermes_home, profile.as_deref(), &content)
}

/// Add memory entry
#[tauri::command]
async fn add_memory_entry_cmd(
    state: State<'_, AppState>,
    content: String,
    profile: Option<String>,
) -> Result<(), String> {
    let hermes_home = state.hermes_home.lock().unwrap().clone()
        .ok_or("App not initialized")?;
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
    let hermes_home = state.hermes_home.lock().unwrap().clone()
        .ok_or("App not initialized")?;
    memory::update_memory_entry(&hermes_home, profile.as_deref(), index, &content)
}

/// Remove memory entry
#[tauri::command]
async fn remove_memory_entry_cmd(
    state: State<'_, AppState>,
    index: usize,
    profile: Option<String>,
) -> Result<(), String> {
    let hermes_home = state.hermes_home.lock().unwrap().clone()
        .ok_or("App not initialized")?;
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
    state: State<'_, AppState>,
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
    let hermes_home = state.hermes_home.lock().unwrap().clone()
        .ok_or("App not initialized")?;
    telegram::save_config(&hermes_home, &config)
}

/// Load Telegram config
#[tauri::command]
async fn load_telegram_config_cmd(
    state: State<'_, AppState>,
) -> Result<telegram::TelegramConfig, String> {
    let hermes_home = state.hermes_home.lock().unwrap().clone()
        .ok_or("App not initialized")?;
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
    let hermes_home = state.hermes_home.lock().unwrap().clone()
        .ok_or("App not initialized")?;
    Ok(registry::get_installed(&hermes_home, profile.as_deref()))
}

/// Install from registry
#[tauri::command]
async fn install_from_registry_cmd(
    state: State<'_, AppState>,
    item: registry::RegistryItem,
    profile: Option<String>,
) -> Result<(), String> {
    let hermes_home = state.hermes_home.lock().unwrap().clone()
        .ok_or("App not initialized")?;
    registry::install_from_registry(&hermes_home, profile.as_deref(), &item)
}

// ── Kanban Commands ───────────────────────────────────────────────────────

/// List kanban boards
#[tauri::command]
async fn list_kanban_boards_cmd(
    state: State<'_, AppState>,
    profile: Option<String>,
) -> Result<Vec<kanban::KanbanBoard>, String> {
    let hermes_home = state.hermes_home.lock().unwrap().clone()
        .ok_or("App not initialized")?;
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
    let hermes_home = state.hermes_home.lock().unwrap().clone()
        .ok_or("App not initialized")?;
    kanban::create_board(&hermes_home, profile.as_deref(), &slug, &name, description.as_deref())
}

/// Delete a kanban board
#[tauri::command]
async fn delete_kanban_board_cmd(
    state: State<'_, AppState>,
    slug: String,
    profile: Option<String>,
) -> Result<bool, String> {
    let hermes_home = state.hermes_home.lock().unwrap().clone()
        .ok_or("App not initialized")?;
    kanban::delete_board(&hermes_home, profile.as_deref(), &slug)
}

/// List tasks for a board
#[tauri::command]
async fn list_kanban_tasks_cmd(
    state: State<'_, AppState>,
    board_slug: String,
    profile: Option<String>,
) -> Result<kanban::KanbanBoardView, String> {
    let hermes_home = state.hermes_home.lock().unwrap().clone()
        .ok_or("App not initialized")?;
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
    let hermes_home = state.hermes_home.lock().unwrap().clone()
        .ok_or("App not initialized")?;
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
    let hermes_home = state.hermes_home.lock().unwrap().clone()
        .ok_or("App not initialized")?;
    kanban::update_task(&hermes_home, profile.as_deref(), &task_id, &fields)
}

/// Delete a kanban task
#[tauri::command]
async fn delete_kanban_task_cmd(
    state: State<'_, AppState>,
    task_id: String,
    profile: Option<String>,
) -> Result<bool, String> {
    let hermes_home = state.hermes_home.lock().unwrap().clone()
        .ok_or("App not initialized")?;
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
    let hermes_home = state.hermes_home.lock().unwrap().clone()
        .ok_or("App not initialized")?;
    kanban::move_task(&hermes_home, profile.as_deref(), &task_id, &new_status)
}

// ── Config Health Commands ────────────────────────────────────────────────

/// Run config health check
#[tauri::command]
async fn config_health_check_cmd(
    state: State<'_, AppState>,
    profile: Option<String>,
) -> Result<config::ConfigHealthReport, String> {
    let hermes_home = state.hermes_home.lock().unwrap().clone()
        .ok_or("App not initialized")?;
    Ok(config::run_config_health_check(&hermes_home, profile.as_deref()))
}

/// Auto-fix a config health issue
#[tauri::command]
async fn auto_fix_config_cmd(
    state: State<'_, AppState>,
    code: String,
    profile: Option<String>,
) -> Result<String, String> {
    let hermes_home = state.hermes_home.lock().unwrap().clone()
        .ok_or("App not initialized")?;
    config_health::auto_fix_issue(&hermes_home, &code, profile.as_deref())
}

// ── Validation Commands ───────────────────────────────────────────────────

/// Validate chat readiness — pre-send check
#[tauri::command]
async fn validate_chat_readiness_cmd(
    state: State<'_, AppState>,
    profile: Option<String>,
) -> Result<validation::ChatReadiness, String> {
    let hermes_home = state.hermes_home.lock().unwrap().clone()
        .ok_or("App not initialized")?;
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
    let hermes_home = state.hermes_home.lock().unwrap().clone()
        .ok_or("App not initialized")?;
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
    let hermes_home = state.hermes_home.lock().unwrap().clone()
        .ok_or("App not initialized")?;
    cronjobs::create_cron_job(&hermes_home, profile.as_deref(), &schedule, prompt.as_deref(), name.as_deref(), deliver.as_deref())
}

/// Remove cron job
#[tauri::command]
async fn remove_cron_job_cmd(
    state: State<'_, AppState>,
    job_id: String,
    profile: Option<String>,
) -> Result<(), String> {
    let hermes_home = state.hermes_home.lock().unwrap().clone()
        .ok_or("App not initialized")?;
    cronjobs::remove_cron_job(&hermes_home, profile.as_deref(), &job_id)
}

/// Pause cron job
#[tauri::command]
async fn pause_cron_job_cmd(
    state: State<'_, AppState>,
    job_id: String,
    profile: Option<String>,
) -> Result<(), String> {
    let hermes_home = state.hermes_home.lock().unwrap().clone()
        .ok_or("App not initialized")?;
    cronjobs::pause_cron_job(&hermes_home, profile.as_deref(), &job_id)
}

/// Resume cron job
#[tauri::command]
async fn resume_cron_job_cmd(
    state: State<'_, AppState>,
    job_id: String,
    profile: Option<String>,
) -> Result<(), String> {
    let hermes_home = state.hermes_home.lock().unwrap().clone()
        .ok_or("App not initialized")?;
    cronjobs::resume_cron_job(&hermes_home, profile.as_deref(), &job_id)
}

/// Trigger cron job
#[tauri::command]
async fn trigger_cron_job_cmd(
    state: State<'_, AppState>,
    job_id: String,
    profile: Option<String>,
) -> Result<String, String> {
    let hermes_home = state.hermes_home.lock().unwrap().clone()
        .ok_or("App not initialized")?;
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
    let hermes_home = state.hermes_home.lock().unwrap().clone()
        .ok_or("App not initialized")?;

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
    let hermes_home = state.hermes_home.lock().unwrap().clone()
        .ok_or("App not initialized")?;
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
    let hermes_home = state.hermes_home.lock().unwrap().clone()
        .ok_or("App not initialized")?;
    auth::add_credential_pool_entry(&hermes_home, &provider, &key, &label).await
}

/// Set credential pool for a provider
#[tauri::command]
async fn set_credential_pool_cmd(
    state: tauri::State<'_, AppState>,
    provider: String,
    entries: Vec<auth::CredentialPoolEntry>,
) -> Result<(), String> {
    let hermes_home = state.hermes_home.lock().unwrap().clone()
        .ok_or("App not initialized")?;
    auth::set_credential_pool(&hermes_home, &provider, &entries).await
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
            detect_local_instances_cmd,
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
            // Telegram
            send_telegram_message_cmd,
            validate_telegram_bot_token_cmd,
            save_telegram_config_cmd,
            load_telegram_config_cmd,
            // Media
            get_media_info_cmd,
            read_media_data_url_cmd,
            list_media_files_cmd,
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
