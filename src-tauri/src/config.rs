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
