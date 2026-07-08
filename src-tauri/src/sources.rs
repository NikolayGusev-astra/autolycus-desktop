// src-tauri/src/sources.rs
// Multiple sources management (Telegram bots, Email accounts, etc.)
// Each source is a connector instance with its own config.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

use crate::config::profile_home;

/// Default for the `use_proxy` per-connector flag: on by default, matching the
/// requirement that every connector can be toggled through a proxy.
fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramSource {
    pub id: String,
    pub name: String,
    pub bot_token: String,
    pub chat_id: String,
    pub allowed_users: String,
    pub home_channel: String,
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub use_proxy: bool,
    #[serde(default)]
    pub proxy_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailSource {
    pub id: String,
    pub name: String,
    pub address: String,
    pub password: String,
    pub smtp_host: String,
    pub smtp_port: u16,
    pub imap_host: String,
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub use_proxy: bool,
    #[serde(default)]
    pub proxy_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JiraSource {
    pub id: String,
    pub name: String,
    pub url: String,
    pub username: String,
    pub api_token: String,
    pub project_key: String,
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub use_proxy: bool,
    #[serde(default)]
    pub proxy_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Source {
    Telegram(TelegramSource),
    Email(EmailSource),
    Jira(JiraSource),
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SourcesConfig {
    pub telegram: Vec<TelegramSource>,
    pub email: Vec<EmailSource>,
    pub jira: Vec<JiraSource>,
}

impl SourcesConfig {
    pub fn load(hermes_home: &Path, profile: Option<&str>) -> Self {
        let path = sources_path(hermes_home, profile);
        if !path.exists() {
            return Self::default();
        }
        let content = std::fs::read_to_string(&path).ok().unwrap_or_default();
        serde_json::from_str(&content).unwrap_or_default()
    }

    pub fn save(&self, hermes_home: &Path, profile: Option<&str>) -> Result<(), String> {
        let path = sources_path(hermes_home, profile);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("mkdir: {}", e))?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| format!("serialize: {}", e))?;
        write_secret_file(&path, &json).map_err(|e| format!("write: {}", e))?;
        Ok(())
    }

    pub fn add_telegram(&mut self, source: TelegramSource) {
        self.telegram.push(source);
    }

    pub fn remove_telegram(&mut self, id: &str) -> bool {
        let len = self.telegram.len();
        self.telegram.retain(|s| s.id != id);
        self.telegram.len() != len
    }

    pub fn update_telegram(&mut self, id: &str, update: TelegramSource) -> bool {
        if let Some(idx) = self.telegram.iter().position(|s| s.id == id) {
            self.telegram[idx] = update;
            true
        } else {
            false
        }
    }

    pub fn add_email(&mut self, source: EmailSource) {
        self.email.push(source);
    }

    pub fn remove_email(&mut self, id: &str) -> bool {
        let len = self.email.len();
        self.email.retain(|s| s.id != id);
        self.email.len() != len
    }

    pub fn update_email(&mut self, id: &str, update: EmailSource) -> bool {
        if let Some(idx) = self.email.iter().position(|s| s.id == id) {
            self.email[idx] = update;
            true
        } else {
            false
        }
    }

    pub fn add_jira(&mut self, source: JiraSource) {
        self.jira.push(source);
    }

    pub fn remove_jira(&mut self, id: &str) -> bool {
        let len = self.jira.len();
        self.jira.retain(|s| s.id != id);
        self.jira.len() != len
    }

    pub fn update_jira(&mut self, id: &str, update: JiraSource) -> bool {
        if let Some(idx) = self.jira.iter().position(|s| s.id == id) {
            self.jira[idx] = update;
            true
        } else {
            false
        }
    }
}

fn sources_path(hermes_home: &Path, profile: Option<&str>) -> std::path::PathBuf {
    profile_home(hermes_home, profile).join("sources.json")
}

fn write_secret_file(path: &std::path::Path, content: &str) -> std::io::Result<()> {
    std::fs::write(path, content)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

// Generate env vars for Hermes from sources config
impl SourcesConfig {
    pub fn to_env_vars(&self) -> HashMap<String, String> {
        let mut env = HashMap::new();

        // For backwards compatibility, use the first enabled source as primary
        if let Some(tg) = self.telegram.iter().find(|s| s.enabled) {
            env.insert("TELEGRAM_BOT_TOKEN".to_string(), tg.bot_token.clone());
            env.insert("TELEGRAM_CHAT_ID".to_string(), tg.chat_id.clone());
            if !tg.allowed_users.is_empty() {
                env.insert("TELEGRAM_ALLOWED_USERS".to_string(), tg.allowed_users.clone());
            }
            if !tg.home_channel.is_empty() {
                env.insert("TELEGRAM_HOME_CHANNEL".to_string(), tg.home_channel.clone());
            }
        }

        if let Some(email) = self.email.iter().find(|s| s.enabled) {
            env.insert("EMAIL_ADDRESS".to_string(), email.address.clone());
            env.insert("EMAIL_PASSWORD".to_string(), email.password.clone());
            env.insert("EMAIL_SMTP_HOST".to_string(), email.smtp_host.clone());
            env.insert("EMAIL_SMTP_PORT".to_string(), email.smtp_port.to_string());
            env.insert("EMAIL_IMAP_HOST".to_string(), email.imap_host.clone());
        }

        // TODO: Add Jira env vars when Hermes supports them

        env
    }
}