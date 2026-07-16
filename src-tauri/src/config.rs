// src-tauri/src/config.rs
// Configuration management: desktop.json, .env, config.yaml
// Ported from fathah/hermes-desktop src/main/config.rs

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

// ── Hermes Home ──────────────────────────────────────────────────────────

/// Resolve HERMES_HOME directory. Precedence:
/// 1. HERMES_HOME env var
/// 2. Override file in app data dir
/// 3. Platform default (~/.hermes on Linux, %LOCALAPPDATA%\hermes on Windows)
pub fn resolve_hermes_home() -> PathBuf {
    // 1. Env var
    if let Ok(val) = std::env::var("HERMES_HOME") {
        let p = PathBuf::from(&val);
        if p.exists() {
            return p;
        }
    }

    // 2. Override file (set by discovery's "adopt instance")
    if let Some(override_path) = read_override_file() {
        if override_path.exists() {
            return override_path;
        }
    }

    // 3a. Windows: %LOCALAPPDATA%\hermes (the real install location for a
    // uv-managed Hermes checkout). The old code only looked at ~/.hermes, so
    // sessions/memory/state.db under AppData\Local\hermes were invisible.
    if cfg!(windows) {
        if let Some(local) = dirs::data_local_dir() {
            let cand = local.join("hermes");
            if cand.is_dir() && looks_like_hermes_home(&cand) {
                return cand;
            }
        }
        // Some installs live under %APPDATA%\hermes
        if let Some(appdata) = dirs::data_dir() {
            let cand = appdata.join("hermes");
            if cand.is_dir() && looks_like_hermes_home(&cand) {
                return cand;
            }
        }
    }

    // 3b. Platform default ~/.hermes
    if let Some(home) = dirs::home_dir() {
        let default = home.join(".hermes");
        if default.exists() {
            return default;
        }
    }

    // Fallback: ~/.hermes
    dirs::home_dir()
        .map(|h| h.join(".hermes"))
        .unwrap_or_else(|| PathBuf::from("/tmp/.hermes"))
}

/// A directory is a Hermes home if it holds config.yaml/.env OR the data files
/// we actually want to read (state.db, sessions/, memories/). Distinguishes a
/// real home from a stale empty folder.
fn looks_like_hermes_home(dir: &Path) -> bool {
    for marker in ["config.yaml", "config.yml", ".env", "auth.json", "state.db"] {
        if dir.join(marker).exists() {
            return true;
        }
    }
    false
}

fn read_override_file() -> Option<PathBuf> {
    let data_dir = dirs::data_dir()?;
    let file = data_dir.join("steersman-desktop").join("hermes-home.json");
    if !file.exists() {
        return None;
    }
    let content = fs::read_to_string(&file).ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&content).ok()?;
    let path = parsed.get("hermesHome")?.as_str()?;
    let path = path.trim();
    if path.is_empty() {
        return None;
    }
    let expanded = expand_tilde(path);
    if PathBuf::from(&expanded).exists() {
        Some(PathBuf::from(expanded))
    } else {
        None
    }
}

pub fn expand_tilde(path: &str) -> String {
    if path.starts_with("~/") {
        if let Some(home) = dirs::home_dir() {
            return path.replacen("~", &home.to_string_lossy(), 1);
        }
    }
    path.to_string()
}

// ── Desktop Config (desktop.json) ────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SshConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub key_path: String,
    pub remote_port: u16,
    pub local_port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConnectionConfig {
    #[serde(rename = "connectionMode")]
    pub mode: String, // "local" | "remote" | "ssh"
    #[serde(rename = "remoteUrl")]
    pub remote_url: String,
    #[serde(rename = "remoteApiKey")]
    pub api_key: String,
    pub ssh: SshConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicConnectionConfig {
    pub mode: String,
    pub remote_url: String,
    pub has_api_key: bool,
    pub api_key_length: usize,
    pub ssh: SshConfig,
}

impl From<&ConnectionConfig> for PublicConnectionConfig {
    fn from(cfg: &ConnectionConfig) -> Self {
        // The API key lives in the keyring now; report its presence/length
        // from there rather than from the (always-empty) on-disk field.
        let key = get_remote_api_key();
        let (has, len) = if key.is_empty() {
            // Fallback for a not-yet-migrated plaintext key.
            (!cfg.api_key.is_empty(), cfg.api_key.len())
        } else {
            (true, key.len())
        };
        Self {
            mode: cfg.mode.clone(),
            remote_url: cfg.remote_url.clone(),
            has_api_key: has,
            api_key_length: len,
            ssh: cfg.ssh.clone(),
        }
    }
}

fn desktop_config_path(hermes_home: &Path) -> PathBuf {
    hermes_home.join("desktop.json")
}

pub fn read_desktop_config(hermes_home: &Path) -> ConnectionConfig {
    let path = desktop_config_path(hermes_home);
    if !path.exists() {
        return ConnectionConfig::default();
    }
    let mut cfg = match fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str::<ConnectionConfig>(&content).unwrap_or_default(),
        Err(_) => return ConnectionConfig::default(),
    };

    // One-time migration: if a plaintext API key is still in desktop.json,
    // move it into the OS keyring and clear the field on disk. Subsequent
    // reads pull the key from the keyring via get_remote_api_key().
    if !cfg.api_key.is_empty() {
        let migrated = crate::secrets::migrate(crate::secrets::account::REMOTE_API_KEY, &cfg.api_key);
        if migrated {
            // Rewrite the file without the plaintext key. If the rewrite
            // fails we keep the old file; the key is now ALSO in keyring,
            // so get_remote_api_key() still resolves it.
            cfg.api_key.clear();
            let _ = write_desktop_config(hermes_home, &cfg);
        }
    }

    cfg
}

/// Resolve the remote API key, preferring the OS keyring and falling back to
/// the (legacy) plaintext field in desktop.json.
pub fn get_remote_api_key() -> String {
    match crate::secrets::get(crate::secrets::account::REMOTE_API_KEY) {
        Ok(Some(k)) => k,
        // Fallback: a non-migrated plaintext key, or keyring unavailable.
        _ => String::new(),
    }
}

/// Persist the remote API key into the OS keyring. Also clears any stale
/// plaintext copy from desktop.json so the key lives in exactly one place.
pub fn set_remote_api_key(hermes_home: &Path, api_key: &str) -> Result<(), String> {
    if api_key.is_empty() {
        // Empty value = user cleared the key. Remove from keyring.
        crate::secrets::delete(crate::secrets::account::REMOTE_API_KEY)?;
    } else {
        crate::secrets::set(crate::secrets::account::REMOTE_API_KEY, api_key)?;
    }
    // Ensure desktop.json no longer carries the plaintext key.
    let mut cfg = read_desktop_config(hermes_home);
    if !cfg.api_key.is_empty() {
        cfg.api_key.clear();
        write_desktop_config(hermes_home, &cfg)?;
    }
    Ok(())
}

pub fn write_desktop_config(hermes_home: &Path, config: &ConnectionConfig) -> Result<(), String> {
    let path = desktop_config_path(hermes_home);
    let json = serde_json::to_string_pretty(config)
        .map_err(|e| format!("Serialization error: {}", e))?;
    // desktop.json no longer stores the API key (moved to keyring), but it may
    // still hold the SSH key path, so keep it owner-only.
    write_secret_file(&path, &json).map_err(|e| format!("Write error: {}", e))?;
    Ok(())
}

/// Write a file containing secrets with mode 0600 (owner read/write only).
/// Falls back to a plain write on platforms where the permission is not
/// applicable (e.g. Windows), where file ACLs are the relevant control.
fn write_secret_file(path: &Path, content: &str) -> std::io::Result<()> {
    fs::write(path, content)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

// ── .env file ─────────────────────────────────────────────────────────────

pub fn read_env(hermes_home: &Path, profile: Option<&str>) -> HashMap<String, String> {
    let env_path = profile_env_path(hermes_home, profile);
    parse_env_file(&env_path)
}

fn profile_env_path(hermes_home: &Path, profile: Option<&str>) -> PathBuf {
    match profile {
        Some(p) if p != "default" && !p.is_empty() => {
            hermes_home.join("profiles").join(p).join(".env")
        }
        _ => hermes_home.join(".env"),
    }
}

fn parse_env_file(path: &Path) -> HashMap<String, String> {
    let mut result = HashMap::new();
    if !path.exists() {
        return result;
    }
    if let Ok(content) = fs::read_to_string(path) {
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim().to_string();
                let value = value.trim().trim_matches('"').trim_matches('\'').to_string();
                result.insert(key, value);
            }
        }
    }
    result
}

pub fn write_env_value(
    hermes_home: &Path,
    profile: Option<&str>,
    key: &str,
    value: &str,
) -> Result<(), String> {
    let env_path = profile_env_path(hermes_home, profile);
    let mut env = parse_env_file(&env_path);

    // Read existing content to preserve formatting
    let mut lines = if env_path.exists() {
        fs::read_to_string(&env_path)
            .map(|c| c.lines().map(|l| l.to_string()).collect::<Vec<_>>())
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let mut found = false;
    for line in &mut lines {
        let trimmed = line.trim();
        if trimmed.starts_with(&format!("{}=", key)) || trimmed.starts_with(&format!("{} =", key)) {
            *line = format!("{}=\"{}\"", key, value);
            found = true;
            break;
        }
    }

    if !found {
        lines.push(format!("{}=\"{}\"", key, value));
    }

    // Ensure parent dir exists
    if let Some(parent) = env_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Create dir error: {}", e))?;
    }

    // .env holds provider API keys — restrict to the owner on unix.
    write_secret_file(&env_path, &(lines.join("\n") + "\n"))
        .map_err(|e| format!("Write error: {}", e))?;

    env.insert(key.to_string(), value.to_string());
    Ok(())
}

// ── config.yaml ───────────────────────────────────────────────────────────

pub fn read_config_yaml(hermes_home: &Path, profile: Option<&str>) -> Result<serde_json::Value, String> {
    let yaml_path = profile_config_path(hermes_home, profile);
    if !yaml_path.exists() {
        return Ok(serde_json::json!({}));
    }
    let content = fs::read_to_string(&yaml_path)
        .map_err(|e| format!("Read error: {}", e))?;
    let parsed: serde_json::Value = serde_yaml_to_json(&content)?;
    Ok(parsed)
}

fn profile_config_path(hermes_home: &Path, profile: Option<&str>) -> PathBuf {
    match profile {
        Some(p) if p != "default" && !p.is_empty() => {
            hermes_home.join("profiles").join(p).join("config.yaml")
        }
        _ => hermes_home.join("config.yaml"),
    }
}

fn serde_yaml_to_json(content: &str) -> Result<serde_json::Value, String> {
    // Simple YAML to JSON conversion using yaml-rust2
    let docs = yaml_rust2::YamlLoader::load_from_str(content)
        .map_err(|e| format!("YAML parse error: {}", e))?;
    if docs.is_empty() {
        return Ok(serde_json::json!({}));
    }
    yaml_value_to_json(&docs[0])
}

fn yaml_value_to_json(yaml: &yaml_rust2::Yaml) -> Result<serde_json::Value, String> {
    match yaml {
        yaml_rust2::Yaml::Null => Ok(serde_json::Value::Null),
        yaml_rust2::Yaml::Boolean(b) => Ok(serde_json::json!(b)),
        yaml_rust2::Yaml::Integer(i) => Ok(serde_json::json!(i)),
        yaml_rust2::Yaml::Real(f) => {
            if let Ok(v) = f.parse::<f64>() {
                Ok(serde_json::json!(v))
            } else {
                Ok(serde_json::json!(f))
            }
        }
        yaml_rust2::Yaml::String(s) => Ok(serde_json::json!(s)),
        yaml_rust2::Yaml::Array(arr) => {
            let items: Result<Vec<_>, _> = arr.iter().map(yaml_value_to_json).collect();
            Ok(serde_json::json!(items?))
        }
        yaml_rust2::Yaml::Hash(map) => {
            let mut obj = serde_json::Map::new();
            for (k, v) in map {
                let key = match k {
                    yaml_rust2::Yaml::String(s) => s.clone(),
                    yaml_rust2::Yaml::Integer(i) => i.to_string(),
                    _ => continue,
                };
                obj.insert(key, yaml_value_to_json(v)?);
            }
            Ok(serde_json::Value::Object(obj))
        }
        _ => Ok(serde_json::Value::Null),
    }
}

// ── Model Config ──────────────────────────────────────────────────────────

/// Per-connector / global SOCKS5 proxy settings.
/// IPC contract matches the frontend `ProxySettings` type: { use_proxy, proxy_url }.
/// NOTE: defaults are intentionally OFF — a desktop app must never silently route
/// user traffic through a SOCKS5 endpoint the user did not configure. The previous
/// default (`socks5://127.0.0.1:12334`) was a developer-machine value that broke
/// every other user's Remote/SSH/Telegram/discovery calls.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxySettings {
    #[serde(default)]
    pub use_proxy: bool,
    #[serde(default)]
    pub proxy_url: String,
}

impl Default for ProxySettings {
    fn default() -> Self {
        Self {
            use_proxy: false,
            proxy_url: String::new(),
        }
    }
}

impl ProxySettings {
    /// Resolve effective proxy URL. Returns `""` (=> direct connection) when:
    ///   - `use_proxy == false`, OR
    ///   - no explicit URL is configured AND no HTTP_PROXY/HTTPS_PROXY env var
    ///     AND no system proxy is detected.
    /// Callers already gate on `use_proxy` before calling reqwest::Proxy::all(),
    /// so the empty-string return is the safe default.
    pub fn resolve_url(&self) -> String {
        if !self.use_proxy {
            return String::new();
        }
        if !self.proxy_url.is_empty() {
            return self.proxy_url.clone();
        }
        // Check process env vars (HTTP_PROXY / HTTPS_PROXY).
        if let Ok(env_p) = std::env::var("HTTP_PROXY").or_else(|_| std::env::var("HTTPS_PROXY")) {
            if !env_p.is_empty() {
                return env_p;
            }
        }
        // Check system proxy settings (Windows registry, macOS system config,
        // Linux gsettings). This is the TUN/system-proxy case.
        if let Some(sys_proxy) = detect_system_proxy() {
            return sys_proxy;
        }
        // No proxy configured and no env/system override => direct connection.
        String::new()
    }
}

/// Universal proxy resolver: checks ALL possible sources in priority order and
/// returns the first non-empty proxy URL. Used by both gateway.rs (to propagate
/// to the Hermes child process) and chat.rs (for direct LLM calls).
///
/// Priority (ADR-002 §Proxy, ADR-003 §Proxy):
///   1. Desktop ProxySettings (config.yaml `proxy:` block with use_proxy=true)
///      — our desktop-only feature for explicit proxy config
///   2. HTTPS_PROXY → HTTP_PROXY → ALL_PROXY in .env
///      — upstream Hermes Agent chain (case-insensitive)
///   3. HTTPS_PROXY → HTTP_PROXY → ALL_PROXY in process env
///   4. OS system proxy (Windows registry / macOS scutil / Linux gsettings)
///
/// NOTE: config.yaml keys `500-network.proxy`, top-level `proxy:`, `model.proxy`
/// are NOT checked — they do NOT exist in upstream Hermes Agent (ADR-002 §Config).
/// They were fork-only inventions that caused false-positive proxy detection.
pub fn resolve_effective_proxy(hermes_home: &Path, profile: Option<&str>) -> String {
    // Source 1: desktop ProxySettings (our feature)
    let model_config = get_model_config(hermes_home, profile);
    if model_config.proxy.use_proxy {
        let url = model_config.proxy.resolve_url();
        if !url.is_empty() {
            return url;
        }
    }
    // Source 2: .env — upstream Hermes Agent proxy chain
    let env_map = read_env(hermes_home, profile);
    for var in &["HTTPS_PROXY", "HTTP_PROXY", "ALL_PROXY",
                 "https_proxy", "http_proxy", "all_proxy"] {
        if let Some(url) = env_map.get(*var) {
            if !url.is_empty() {
                return url.clone();
            }
        }
    }
    // Source 3: process env — same chain (inherited from shell)
    for var in &["HTTPS_PROXY", "HTTP_PROXY", "ALL_PROXY",
                 "https_proxy", "http_proxy", "all_proxy"] {
        if let Ok(url) = std::env::var(var) {
            if !url.is_empty() {
                return url;
            }
        }
    }
    // Source 4: OS system proxy
    if let Some(sys_proxy) = detect_system_proxy() {
        return sys_proxy;
    }
    String::new()
}

/// Detect the system-level proxy configuration. This handles cases the user
/// did NOT explicitly configure in the app but their OS is set to use:
///   - Windows: Internet Settings (registry `ProxyServer` / `ProxyEnable`)
///   - macOS: `scutil --proxy` or `networksetup`
///   - Linux: GNOME `org.gnome.system.proxy` via gsettings
///
/// TUN-mode proxies (e.g. Clash TUN, v2ray tun2socks) route at the network
/// layer and don't need env vars — they are transparent. This function only
/// returns proxies that require explicit configuration (HTTP/SOCKS), so TUN
/// mode is correctly a "no proxy URL needed, traffic flows" case → returns "".
pub fn detect_system_proxy() -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        detect_windows_proxy()
    }
    #[cfg(target_os = "macos")]
    {
        detect_macos_proxy()
    }
    #[cfg(target_os = "linux")]
    {
        detect_linux_proxy()
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        None
    }
}

/// Windows: read the Internet Settings registry key.
/// HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings
///   ProxyEnable (DWORD): 1 = proxy on
///   ProxyServer (REG_SZ): "host:port" or "http=host:port;https=host:port"
#[cfg(target_os = "windows")]
fn detect_windows_proxy() -> Option<String> {
    // Use reg.exe query — no winreg crate dependency, works on all Windows.
    let output = std::process::Command::new("reg")
        .args(["query", r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings", "/v", "ProxyEnable"])
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Look for "ProxyEnable    REG_DWORD    0x1"
    let enabled = stdout.lines().any(|line| {
        line.contains("ProxyEnable") && line.contains("0x1")
    });
    if !enabled {
        return None;
    }

    // Read ProxyServer value.
    let output = std::process::Command::new("reg")
        .args(["query", r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings", "/v", "ProxyServer"])
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.contains("ProxyServer") {
            // Extract the value after "REG_SZ"
            if let Some(idx) = line.find("REG_SZ") {
                let raw = line[idx + 6..].trim();
                if raw.is_empty() {
                    continue;
                }
                // ProxyServer can be "host:port" or "http=host:port;https=host:port;..."
                // Extract the http= entry first, else fall back to the first entry.
                for part in raw.split(';') {
                    let part = part.trim();
                    if let Some(addr) = part.strip_prefix("http=") {
                        return Some(format!("http://{}", addr));
                    }
                    if let Some(addr) = part.strip_prefix("https=") {
                        return Some(format!("http://{}", addr));
                    }
                    if let Some(addr) = part.strip_prefix("socks=") {
                        return Some(format!("socks5://{}", addr));
                    }
                }
                // No protocol prefix — assume HTTP.
                if raw.contains(':') && !raw.contains('=') {
                    return Some(format!("http://{}", raw));
                }
            }
        }
    }
    None
}

/// macOS: use `scutil --proxy` to read system proxy settings.
#[cfg(target_os = "macos")]
fn detect_macos_proxy() -> Option<String> {
    let output = std::process::Command::new("scutil")
        .arg("--proxy")
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut enabled = false;
    let mut host = String::new();
    let mut port = String::new();
    for line in stdout.lines() {
        let line = line.trim();
        if line.starts_with("HTTPEnable : 1") {
            enabled = true;
        }
        if let Some(val) = line.strip_prefix("HTTPProxy : ") {
            host = val.trim().to_string();
        }
        if let Some(val) = line.strip_prefix("HTTPPort : ") {
            port = val.trim().to_string();
        }
    }
    if enabled && !host.is_empty() && !port.is_empty() {
        return Some(format!("http://{}:{}", host, port));
    }
    None
}

/// Linux: check GNOME gsettings for system proxy.
#[cfg(target_os = "linux")]
fn detect_linux_proxy() -> Option<String> {
    // Try gsettings (GNOME/Ubuntu/Fedora).
    let mode = std::process::Command::new("gsettings")
        .args(["get", "org.gnome.system.proxy", "mode"])
        .output()
        .ok()?;
    let mode_str = String::from_utf8_lossy(&mode.stdout).trim().trim_matches('\'').to_string();
    if mode_str != "manual" {
        return None;
    }
    let host = std::process::Command::new("gsettings")
        .args(["get", "org.gnome.system.proxy.http", "host"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().trim_matches('\'').to_string())
        .unwrap_or_default();
    let port = std::process::Command::new("gsettings")
        .args(["get", "org.gnome.system.proxy.http", "port"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    if !host.is_empty() && !port.is_empty() {
        return Some(format!("http://{}:{}", host, port));
    }
    None
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelConfig {
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub proxy: ProxySettings,
    // ── Steer fields (GPT-5.6 guide practices 2, 3, 5, 6, 7) ─────────────
    /// reasoning.effort: none|low|medium|high|xhigh|max (practice 2).
    #[serde(default)]
    pub reasoning_effort: Option<String>,
    /// text.verbosity: low|medium|high (practice 5).
    #[serde(default)]
    pub verbosity: Option<String>,
    /// Autonomy policy: readonly|local|confirm-external (practice 3).
    #[serde(default)]
    pub autonomy_policy: Option<String>,
    /// Enable prompt caching for stable prefixes (practice 6).
    #[serde(default)]
    pub prompt_cache: Option<bool>,
    /// reasoning.context: all_turns|current_turn (practice 7).
    #[serde(default)]
    pub reasoning_context: Option<String>,
    /// Multi-model routing: map task type → model id (practice 1).
    /// e.g. {"routine": "gpt-5.6-luna", "complex": "gpt-5.6-sol"}
    #[serde(default)]
    pub model_routing: Option<std::collections::HashMap<String, String>>,
}

pub fn get_model_config(hermes_home: &Path, profile: Option<&str>) -> ModelConfig {
    // config.yaml `model:` block is Hermes's source of truth (the same keys
    // `hermes setup` writes). Read provider/default/base_url from there first.
    if let Ok(yaml) = read_config_yaml(hermes_home, profile) {
        if let Some(model_block) = yaml.get("model").and_then(|m| m.as_object()) {
            let provider = model_block.get("provider").and_then(|v| v.as_str()).unwrap_or("");
            let default = model_block.get("default").and_then(|v| v.as_str()).unwrap_or("");
            let base_url = model_block.get("base_url").and_then(|v| v.as_str()).unwrap_or("");
            // proxy: read from a top-level `proxy:` block (desktop-managed) or
            // from `model.proxy.*` for backward compatibility.
            let mut proxy = ProxySettings::default();
            if let Some(pb) = yaml.get("proxy").and_then(|m| m.as_object()) {
                proxy.use_proxy = pb.get("use_proxy").and_then(|v| v.as_bool()).unwrap_or(false);
                proxy.proxy_url = pb
                    .get("proxy_url")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
            } else if let Some(pb) = model_block.get("proxy").and_then(|m| m.as_object()) {
                proxy.use_proxy = pb.get("use_proxy").and_then(|v| v.as_bool()).unwrap_or(false);
                proxy.proxy_url = pb
                    .get("proxy_url")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
            }
            if !provider.is_empty() || !default.is_empty() {
                // Parse model_routing map if present (practice 1).
                let model_routing = model_block
                    .get("model_routing")
                    .and_then(|v| v.as_object())
                    .map(|obj| {
                        obj.iter()
                            .filter_map(|(k, v)| {
                                v.as_str().map(|s| (k.clone(), s.to_string()))
                            })
                            .collect::<std::collections::HashMap<String, String>>()
                    });
                return ModelConfig {
                    provider: provider.to_string(),
                    model: default.to_string(),
                    base_url: base_url.to_string(),
                    proxy,
                    reasoning_effort: model_block
                        .get("reasoning_effort")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    verbosity: model_block
                        .get("verbosity")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    autonomy_policy: model_block
                        .get("autonomy")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    prompt_cache: model_block
                        .get("prompt_cache")
                        .and_then(|v| v.as_bool()),
                    reasoning_context: model_block
                        .get("reasoning_context")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    model_routing,
                };
            }
        }
    }

    // Fallback: read from .env
    let env = read_env(hermes_home, profile);
    ModelConfig {
        provider: env.get("PROVIDER").cloned().unwrap_or_default(),
        model: env.get("MODEL").cloned().unwrap_or_default(),
        base_url: env.get("BASE_URL").cloned().unwrap_or_default(),
        proxy: ProxySettings::default(),
        reasoning_effort: None,
        verbosity: None,
        autonomy_policy: None,
        prompt_cache: None,
        reasoning_context: None,
        model_routing: None,
    }
}

pub fn set_model_config(
    hermes_home: &Path,
    profile: Option<&str>,
    provider: &str,
    model: &str,
    base_url: &str,
    proxy: Option<ProxySettings>,
    reasoning_effort: Option<&str>,
    verbosity: Option<&str>,
    autonomy_policy: Option<&str>,
    prompt_cache: Option<bool>,
    reasoning_context: Option<&str>,
) -> Result<(), String> {
    // Write to config.yaml's `model:` block — this is the source of truth that
    // `hermes setup`/Hermes itself reads (aligning the desktop with the agent,
    // instead of maintaining a divergent .env overlay). We also keep the .env
    // writes for backward compatibility with any code that still reads there.
    let mut kvs: Vec<(&str, &str)> = vec![
        ("provider", provider),
        ("default", model),
        ("base_url", base_url),
    ];
    // Steer fields — only write non-empty values (practice 2, 3, 5, 6, 7).
    if let Some(effort) = reasoning_effort {
        if !effort.is_empty() {
            kvs.push(("reasoning_effort", effort));
        }
    }
    if let Some(verb) = verbosity {
        if !verb.is_empty() {
            kvs.push(("verbosity", verb));
        }
    }
    if let Some(autonomy) = autonomy_policy {
        if !autonomy.is_empty() {
            kvs.push(("autonomy", autonomy));
        }
    }
    if let Some(cache) = prompt_cache {
        kvs.push(("prompt_cache", if cache { "true" } else { "false" }));
    }
    if let Some(ctx) = reasoning_context {
        if !ctx.is_empty() {
            kvs.push(("reasoning_context", ctx));
        }
    }
    if let Some(p) = &proxy {
        // Persist proxy in a dedicated top-level `proxy:` block (desktop-managed),
        // separate from Hermes's `model:` block, so we don't fight `hermes setup`
        // over the same keys. get_model_config reads it back from here.
        set_yaml_block_scalars(
            hermes_home,
            profile,
            "proxy",
            &[
                ("use_proxy", if p.use_proxy { "true" } else { "false" }),
                ("proxy_url", p.proxy_url.as_str()),
            ],
        )?;
    }
    set_yaml_block_scalars(hermes_home, profile, "model", &kvs)?;
    write_env_value(hermes_home, profile, "PROVIDER", provider)?;
    write_env_value(hermes_home, profile, "MODEL", model)?;
    write_env_value(hermes_home, profile, "BASE_URL", base_url)?;
    Ok(())
}

/// Set one or more scalar leaves under a top-level YAML block (e.g. `model:`).
/// Operates on lines so comments/formatting elsewhere are preserved. Creates
/// the block if absent. Used to keep the desktop's config writes aligned with
/// Hermes's own config.yaml (rather than only writing .env).
pub fn set_yaml_block_scalars(
    hermes_home: &Path,
    profile: Option<&str>,
    block: &str,
    kvs: &[(&str, &str)],
) -> Result<(), String> {
    let path = match profile {
        Some(p) if p != "default" && !p.is_empty() => {
            hermes_home.join("profiles").join(p).join("config.yaml")
        }
        _ => hermes_home.join("config.yaml"),
    };
    let content = fs::read_to_string(&path).unwrap_or_default();
    let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();

    let block_header = format!("{}:", block);
    let mut in_block = false;
    let mut block_indent: Option<usize> = None;
    // Track which keys we set so we can append missing ones.
    let mut set_keys: std::collections::HashSet<String> = std::collections::HashSet::new();

    let header_prefix = format!("{}:", block_header);
    for line in lines.iter_mut() {
        // Compute everything we need as owned values FIRST, so the immutable
        // borrow of `line` (via trim_start) ends before we mutate `*line`.
        let trimmed: String = line.trim_start().to_string();
        let indent = line.len() - line.trim_start().len();
        let is_header = trimmed == block_header || trimmed.starts_with(&header_prefix);
        let leaves_block = in_block && !trimmed.is_empty() && indent <= block_indent.unwrap_or(0);

        if is_header {
            in_block = true;
            block_indent = Some(indent);
            continue;
        }
        if leaves_block {
            in_block = false;
            continue;
        }
        if in_block {
            // Find the first matching key (owned) before mutating.
            let replacement: Option<(String, String)> = kvs
                .iter()
                .find(|(k, _)| trimmed.starts_with(&format!("{}:", k)))
                .map(|(k, v)| ((*k).to_string(), (*v).to_string()));
            if let Some((k, v)) = replacement {
                *line = format!(
                    "{}{}: {}",
                    " ".repeat(block_indent.unwrap_or(0) + 2),
                    k,
                    v
                );
                set_keys.insert(k);
            }
        }
    }

    // Append any keys that weren't found in the existing block.
    let missing: Vec<(&str, &str)> = kvs
        .iter()
        .filter(|(k, _)| !set_keys.contains(*k))
        .copied()
        .collect();
    if !missing.is_empty() {
        // Ensure the block header exists.
        if !lines.iter().any(|l| l.trim_start().starts_with(&block_header)) {
            lines.push(block_header.clone());
        }
        let indent = " ".repeat(2);
        for (k, v) in missing {
            lines.push(format!("{}{}: {}", indent, k, v));
        }
    }

    if lines.is_empty() {
        lines.push(block_header);
        let indent = " ".repeat(2);
        for (k, v) in kvs {
            lines.push(format!("{}{}: {}", indent, k, v));
        }
    }

    let new_content = lines.join("\n");
    fs::write(&path, new_content).map_err(|e| format!("Write config.yaml error: {}", e))?;
    Ok(())
}

// ── API Server Key ────────────────────────────────────────────────────────

pub fn get_api_server_key(hermes_home: &Path, profile: Option<&str>) -> Option<String> {
    let env = read_env(hermes_home, profile);
    env.get("API_SERVER_KEY").cloned()
}

// REMOVED (P2.1 cleanup follow-up): generate_api_server_key() generated a
// UUID for the legacy HTTP API_SERVER_KEY auth. WS transport uses
// HERMES_DASHBOARD_SESSION_TOKEN (generate_session_token in gateway.rs).
// 0 callers.

// ── Profile paths ─────────────────────────────────────────────────────────

pub fn profile_home(hermes_home: &Path, profile: Option<&str>) -> PathBuf {
    match profile {
        Some(p) if p != "default" && !p.is_empty() => {
            hermes_home.join("profiles").join(p)
        }
        _ => hermes_home.to_path_buf(),
    }
}

// ── URL → Env Key mapping ─────────────────────────────────────────────────

/// Given a base URL, return the expected env var name for the API key.
/// Falls back to CUSTOM_API_KEY for unknown URLs.
pub fn expected_env_key_for_url(url: &str) -> &str {
    let url_lower = url.to_lowercase();
    if url_lower.contains("openrouter.ai") { return "OPENROUTER_API_KEY"; }
    if url_lower.contains("anthropic.com") { return "ANTHROPIC_API_KEY"; }
    if url_lower.contains("openai.com") { return "OPENAI_API_KEY"; }
    if url_lower.contains("ollama.com") { return "OLLAMA_API_KEY"; }
    if url_lower.contains("huggingface.co") { return "HF_TOKEN"; }
    if url_lower.contains("api.groq.com") { return "GROQ_API_KEY"; }
    if url_lower.contains("api.deepseek.com") { return "DEEPSEEK_API_KEY"; }
    if url_lower.contains("api.together.xyz") { return "TOGETHER_API_KEY"; }
    if url_lower.contains("api.fireworks.ai") { return "FIREWORKS_API_KEY"; }
    if url_lower.contains("api.cerebras.ai") { return "CEREBRAS_API_KEY"; }
    if url_lower.contains("api.mistral.ai") { return "MISTRAL_API_KEY"; }
    if url_lower.contains("api.perplexity.ai") { return "PERPLEXITY_API_KEY"; }
    if url_lower.contains("api.xiaomimimo.com") { return "XIAOMI_API_KEY"; }
    "CUSTOM_API_KEY"
}

/// Returns true if the URL points to a local/private host.
pub fn is_local_base_url(url: &str) -> bool {
    let url_lower = url.to_lowercase();
    url_lower.starts_with("http://localhost")
        || url_lower.starts_with("http://127.0.0.1")
        || url_lower.starts_with("http://0.0.0.0")
        || url_lower.starts_with("http://[::1]")
        || url_lower.starts_with("http://192.168.")
        || url_lower.starts_with("http://10.")
        || url_lower.starts_with("http://172.16.")
        || url_lower.starts_with("http://172.17.")
        || url_lower.starts_with("http://172.18.")
        || url_lower.starts_with("http://172.19.")
        || url_lower.starts_with("http://172.2")
        || url_lower.starts_with("http://172.30.")
        || url_lower.starts_with("http://172.31.")
}

/// Provider IDs that authenticate via OAuth only (no API key variant).
const OAUTH_PROVIDERS: &[&str] = &[
    "openai-codex",
    "xai-oauth",
    "qwen-oauth",
    "google-gemini-cli",
    "minimax-oauth",
    "kimi-coding",
];

/// Provider IDs that don't need an API key at all.
const NO_KEY_PROVIDERS: &[&str] = &["auto"];

/// Check if the env var for the given provider+URL is set.
/// Returns true if the key is present or if the check is not applicable.
pub fn has_api_key_for_provider(
    hermes_home: &Path,
    profile: Option<&str>,
    provider: &str,
    base_url: &str,
) -> bool {
    let provider_lower = provider.to_lowercase();

    // Auto provider — skip check
    if NO_KEY_PROVIDERS.contains(&provider_lower.as_str()) {
        return true;
    }

    // OAuth-only providers — skip check
    if OAUTH_PROVIDERS.contains(&provider_lower.as_str()) {
        return true;
    }

    // Local URLs — skip check
    if is_local_base_url(base_url) {
        return true;
    }

    // Check the expected env var
    let expected_key = expected_env_key_for_url(base_url);
    if expected_key.is_empty() || expected_key == "CUSTOM_API_KEY" {
        // Unknown provider+URL — fail open
        return true;
    }

    let env = read_env(hermes_home, profile);
    let value = env.get(expected_key).map(|v| v.trim().to_string()).unwrap_or_default();
    if !value.is_empty() {
        return true;
    }

    // Fallback: check common alternative keys
    let fallback_keys = ["OPENAI_API_KEY", "CUSTOM_API_KEY"];
    for key in &fallback_keys {
        if let Some(v) = env.get(*key) {
            if !v.trim().is_empty() {
                return true;
            }
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proxy_default_is_off() {
        let p = ProxySettings::default();
        assert!(!p.use_proxy, "proxy must default to OFF (safe default)");
        assert!(p.proxy_url.is_empty());
        assert!(p.resolve_url().is_empty(), "OFF proxy must resolve to empty");
    }

    #[test]
    fn proxy_explicit_url() {
        let p = ProxySettings {
            use_proxy: true,
            proxy_url: "http://proxy.local:3128".into(),
        };
        assert_eq!(p.resolve_url(), "http://proxy.local:3128");
    }

    #[test]
    fn proxy_off_returns_empty_even_with_url() {
        // If use_proxy is false, resolve_url must return empty even if proxy_url
        // is set — the flag is the authoritative gate.
        let p = ProxySettings {
            use_proxy: false,
            proxy_url: "http://proxy.local:3128".into(),
        };
        assert_eq!(p.resolve_url(), "");
    }

    #[test]
    fn detect_system_proxy_does_not_panic() {
        // This must work on all platforms (Windows/macOS/Linux) without error.
        // It may return None (no system proxy configured) — that's fine.
        let _ = detect_system_proxy();
    }

    /// Verify resolve_hermes_home points at the real Hermes install (the one
    /// holding state.db), not a stale ~/.hermes. Run with:
    ///   cargo test --lib resolve_real_home -- --nocapture --ignored
    #[test]
    #[ignore]
    fn resolve_real_home() {
        let home = resolve_hermes_home();
        println!("resolved hermes_home = {}", home.display());
        let db = home.join("state.db");
        println!("state.db exists = {}", db.exists());
        if db.exists() {
            let conn = rusqlite::Connection::open(&db).unwrap();
            let n: i64 = conn
                .query_row("SELECT count(*) FROM sessions", [], |r| r.get(0))
                .unwrap();
            println!("sessions count = {}", n);
            assert!(n > 0, "expected sessions in the resolved home");
        }
    }

    // ── T3: YAML round-trip editor (P-AUDIT #19) ────────────────────────────

    use std::fs;
    use std::path::Path;

    /// Write a fixture config.yaml to a temp hermes_home for round-trip tests.
    fn write_fixture(home: &Path, yaml: &str) {
        fs::write(home.join("config.yaml"), yaml).unwrap();
    }

    #[test]
    fn yaml_roundtrip_updates_top_level_block() {
        let dir = tempdir();
        write_fixture(&dir, "model:\n  default: old-model\n  provider: old\n");
        set_yaml_block_scalars(&dir, None, "model", &[("default", "new-model")]).unwrap();
        let updated = fs::read_to_string(dir.join("config.yaml")).unwrap();
        assert!(updated.contains("new-model"), "updated content: {}", updated);
        assert!(!updated.contains("old-model"));
    }

    #[test]
    fn yaml_roundtrip_updates_nested_dotted_path() {
        // The real callers use dotted paths like "mcp_servers.email.env".
        let dir = tempdir();
        write_fixture(
            &dir,
            "mcp_servers:\n  email:\n    command: foo\n    env:\n      EMAIL_ADDRESS: old@example.com\n",
        );
        // Note: set_yaml_block_scalars treats block as a flat key search within
        // the located block; nested dotted paths exercise the line-walker.
        set_yaml_block_scalars(&dir, None, "mcp_servers.email.env", &[("EMAIL_ADDRESS", "new@example.com")]).unwrap();
        let updated = fs::read_to_string(dir.join("config.yaml")).unwrap();
        assert!(
            updated.contains("new@example.com"),
            "nested env update failed. content: {}",
            updated
        );
    }

    #[test]
    fn yaml_roundtrip_preserves_unrelated_keys() {
        let dir = tempdir();
        write_fixture(
            &dir,
            "model:\n  default: m1\n  provider: p1\nagent:\n  max_turns: 60\n",
        );
        set_yaml_block_scalars(&dir, None, "model", &[("default", "m2")]).unwrap();
        let updated = fs::read_to_string(dir.join("config.yaml")).unwrap();
        // Unrelated agent block must survive.
        assert!(updated.contains("max_turns: 60"), "unrelated key dropped: {}", updated);
        // Untouched sibling key in the same block survives.
        assert!(updated.contains("provider: p1"), "sibling key dropped: {}", updated);
    }

    #[test]
    fn yaml_roundtrip_preserves_comments() {
        // The audit flagged comment loss as a key fragility of the line-based
        // editor (P-AUDIT #19). A safe round-trip must keep user comments.
        let dir = tempdir();
        write_fixture(
            &dir,
            "# user comment: do not delete\nmodel:\n  default: m1\n  provider: p1\n",
        );
        set_yaml_block_scalars(&dir, None, "model", &[("default", "m2")]).unwrap();
        let updated = fs::read_to_string(dir.join("config.yaml")).unwrap();
        assert!(
            updated.contains("# user comment: do not delete"),
            "comment was dropped by round-trip. content: {}",
            updated
        );
    }

    #[test]
    fn yaml_roundtrip_updates_correct_instance_of_repeated_key() {
        // Fragility: when the same key name appears at multiple nesting levels
        // (e.g. `env:` under two different mcp_servers), the line-walker must
        // update only the targeted block, not all matches.
        let dir = tempdir();
        write_fixture(
            &dir,
            "mcp_servers:\n  email:\n    command: x\n    env:\n      KEY: email-val\n  jira:\n    command: y\n    env:\n      KEY: jira-val\n",
        );
        set_yaml_block_scalars(&dir, None, "mcp_servers.email.env", &[("KEY", "updated-email")]).unwrap();
        let updated = fs::read_to_string(dir.join("config.yaml")).unwrap();
        assert!(updated.contains("updated-email"), "target not updated: {}", updated);
        // The jira env must be untouched.
        assert!(
            updated.contains("jira-val"),
            "wrong block was modified (jira clobbered). content: {}",
            updated
        );
    }

    /// Minimal tempdir helper (avoids pulling in the tempfile crate).
    fn tempdir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "steersman-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }
}

// ── Config Health Check ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct ConfigHealthIssue {
    pub code: String,
    pub severity: String, // "error" | "warning" | "info"
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub locations: Vec<String>,
    pub auto_fixable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix_description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix_location: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConfigHealthReport {
    pub ran_at: i64,
    pub profile: String,
    pub issues: Vec<ConfigHealthIssue>,
    pub summary: ConfigHealthSummary,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConfigHealthSummary {
    pub errors: usize,
    pub warnings: usize,
    pub infos: usize,
}

impl Default for ConfigHealthSummary {
    fn default() -> Self {
        Self { errors: 0, warnings: 0, infos: 0 }
    }
}

/// Run config health check — returns a report of issues found.
/// Never throws; returns empty report on total failure.
pub fn run_config_health_check(
    hermes_home: &Path,
    profile: Option<&str>,
) -> ConfigHealthReport {
    let profile_name = profile.unwrap_or("default").to_string();
    let mut report = ConfigHealthReport {
        ran_at: chrono::Utc::now().timestamp(),
        profile: profile_name,
        issues: Vec::new(),
        summary: ConfigHealthSummary::default(),
    };

    // Check 1: Active model key presence
    if let Some(issue) = check_model_key_presence(hermes_home, profile) {
        report.issues.push(issue);
    }

    // Check 2: Non-ASCII credentials
    if let Some(issue) = check_non_ascii_credentials(hermes_home, profile) {
        report.issues.push(issue);
    }

    // Check 3: Empty API server key
    if let Some(issue) = check_api_server_key(hermes_home, profile) {
        report.issues.push(issue);
    }

    // Summarize
    for issue in &report.issues {
        match issue.severity.as_str() {
            "error" => report.summary.errors += 1,
            "warning" => report.summary.warnings += 1,
            _ => report.summary.infos += 1,
        }
    }

    report
}

/// Check: active model is configured but its expected provider key isn't in .env.
fn check_model_key_presence(
    hermes_home: &Path,
    profile: Option<&str>,
) -> Option<ConfigHealthIssue> {
    let mc = get_model_config(hermes_home, profile);
    let provider = mc.provider.trim().to_lowercase();
    let model = mc.model.trim();
    let base_url = mc.base_url.trim();

    if provider.is_empty() || provider == "auto" {
        return None;
    }
    if model.is_empty() {
        return None;
    }
    if is_local_base_url(base_url) {
        return None;
    }

    let expected_key = expected_env_key_for_url(base_url);
    if expected_key == "CUSTOM_API_KEY" {
        return None;
    }

    let env = read_env(hermes_home, profile);
    let value = env.get(expected_key).map(|v| v.trim().to_string()).unwrap_or_default();
    if !value.is_empty() {
        return None;
    }

    // Check fallback keys
    for fallback in &["OPENAI_API_KEY", "CUSTOM_API_KEY"] {
        if let Some(v) = env.get(*fallback) {
            if !v.trim().is_empty() {
                return None;
            }
        }
    }

    let env_path = profile_env_path(hermes_home, profile);
    Some(ConfigHealthIssue {
        code: "MODEL_KEY_MISSING".to_string(),
        severity: "warning".to_string(),
        message: format!(
            "Active model uses {} but {} is not set in .env.",
            provider, expected_key
        ),
        detail: Some(
            "Chat will fail with an upstream auth error until the key is configured.".to_string(),
        ),
        locations: vec![env_path.to_string_lossy().to_string()],
        auto_fixable: false,
        fix_description: None,
        fix_location: Some("providers".to_string()),
    })
}

/// Check: non-ASCII characters in credential values.
fn check_non_ascii_credentials(
    hermes_home: &Path,
    profile: Option<&str>,
) -> Option<ConfigHealthIssue> {
    let env = read_env(hermes_home, profile);
    let mut offenders = Vec::new();

    for (key, value) in &env {
        if !key.chars().all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit()) {
            continue;
        }
        if !key.ends_with("_API_KEY") && !key.ends_with("_TOKEN") && key != "API_SERVER_KEY" {
            continue;
        }
        if value.is_empty() {
            continue;
        }
        if value.chars().any(|c| !c.is_ascii() || (c as u32) < 0x20 || (c as u32) > 0x7e) {
            offenders.push(key.clone());
        }
    }

    if offenders.is_empty() {
        return None;
    }

    let env_path = profile_env_path(hermes_home, profile);
    Some(ConfigHealthIssue {
        code: "NON_ASCII_CREDENTIAL".to_string(),
        severity: "info".to_string(),
        message: format!("Non-ASCII characters detected in: {}.", offenders.join(", ")),
        detail: Some(
            "Common cause: a smart-quote or trailing newline from a paste.".to_string(),
        ),
        locations: vec![env_path.to_string_lossy().to_string()],
        auto_fixable: true,
        fix_description: Some("Strip non-ASCII characters from the values.".to_string()),
        fix_location: Some(".env".to_string()),
    })
}

/// Check: API server key presence.
fn check_api_server_key(
    hermes_home: &Path,
    profile: Option<&str>,
) -> Option<ConfigHealthIssue> {
    let env = read_env(hermes_home, profile);
    let env_key = env.get("API_SERVER_KEY").map(|v| v.trim().to_string()).unwrap_or_default();

    if !env_key.is_empty() {
        return None;
    }

    // Check if config.yaml exists — if not, this is a fresh install, skip
    let config_path = profile_config_path(hermes_home, profile);
    if !config_path.exists() {
        return None;
    }

    let env_path = profile_env_path(hermes_home, profile);
    Some(ConfigHealthIssue {
        code: "EMPTY_API_SERVER_KEY".to_string(),
        severity: "warning".to_string(),
        message: "No API_SERVER_KEY is set — chat will fail because the Hermes gateway requires auth.".to_string(),
        detail: Some(
            "API_SERVER_KEY is mandatory for Hermes API access. Set it in .env.".to_string(),
        ),
        locations: vec![env_path.to_string_lossy().to_string()],
        auto_fixable: false,
        fix_description: None,
        fix_location: Some("setup".to_string()),
    })
}
