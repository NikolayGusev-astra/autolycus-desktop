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

    // 2. Override file
    if let Some(override_path) = read_override_file() {
        if override_path.exists() {
            return override_path;
        }
    }

    // 3. Platform default
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

fn read_override_file() -> Option<PathBuf> {
    let data_dir = dirs::data_dir()?;
    let file = data_dir.join("autolycus-desktop").join("hermes-home.json");
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
        Self {
            mode: cfg.mode.clone(),
            remote_url: cfg.remote_url.clone(),
            has_api_key: !cfg.api_key.is_empty(),
            api_key_length: cfg.api_key.len(),
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
    match fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => ConnectionConfig::default(),
    }
}

pub fn write_desktop_config(hermes_home: &Path, config: &ConnectionConfig) -> Result<(), String> {
    let path = desktop_config_path(hermes_home);
    let json = serde_json::to_string_pretty(config)
        .map_err(|e| format!("Serialization error: {}", e))?;
    fs::write(&path, json).map_err(|e| format!("Write error: {}", e))?;
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

    fs::write(&env_path, lines.join("\n") + "\n")
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub provider: String,
    pub model: String,
    pub base_url: String,
}

pub fn get_model_config(hermes_home: &Path, profile: Option<&str>) -> ModelConfig {
    // Try config.yaml first
    if let Ok(yaml) = read_config_yaml(hermes_home, profile) {
        if let Some(providers) = yaml.get("providers").and_then(|p| p.as_array()) {
            for provider in providers {
                if let Some(provider_name) = provider.get("name").and_then(|n| n.as_str()) {
                    if let Some(model) = provider.get("model").and_then(|m| m.as_str()) {
                        if let Some(base_url) = provider.get("base_url").and_then(|u| u.as_str()) {
                            return ModelConfig {
                                provider: provider_name.to_string(),
                                model: model.to_string(),
                                base_url: base_url.to_string(),
                            };
                        }
                    }
                }
            }
        }
    }

    // Fallback: read from .env
    let env = read_env(hermes_home, profile);
    ModelConfig {
        provider: env.get("PROVIDER").cloned().unwrap_or_default(),
        model: env.get("MODEL").cloned().unwrap_or_default(),
        base_url: env.get("BASE_URL").cloned().unwrap_or_default(),
    }
}

pub fn set_model_config(
    hermes_home: &Path,
    profile: Option<&str>,
    provider: &str,
    model: &str,
    base_url: &str,
) -> Result<(), String> {
    // Write to .env for simplicity
    write_env_value(hermes_home, profile, "PROVIDER", provider)?;
    write_env_value(hermes_home, profile, "MODEL", model)?;
    write_env_value(hermes_home, profile, "BASE_URL", base_url)?;
    Ok(())
}

// ── API Server Key ────────────────────────────────────────────────────────

pub fn get_api_server_key(hermes_home: &Path, profile: Option<&str>) -> Option<String> {
    let env = read_env(hermes_home, profile);
    env.get("API_SERVER_KEY").cloned()
}

pub fn generate_api_server_key() -> String {
    uuid::Uuid::new_v4().to_string()
}

// ── Profile paths ─────────────────────────────────────────────────────────

pub fn profile_home(hermes_home: &Path, profile: Option<&str>) -> PathBuf {
    match profile {
        Some(p) if p != "default" && !p.is_empty() => {
            hermes_home.join("profiles").join(p)
        }
        _ => hermes_home.to_path_buf(),
    }
}

pub fn looks_like_hermes_home(dir: &Path) -> bool {
    if !dir.exists() {
        return false;
    }
    dir.join("hermes-agent").exists()
        || dir.join("gateway.pid").exists()
        || dir.join("config.yaml").exists()
        || dir.join("active_profile").exists()
        || dir.join(".env").exists()
}
