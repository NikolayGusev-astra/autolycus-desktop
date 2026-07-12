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
    #[serde(default = "default_imap_port")]
    pub imap_port: u16,
    #[serde(default = "default_use_ssl")]
    pub use_ssl: bool,
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub use_proxy: bool,
    #[serde(default)]
    pub proxy_url: String,
}

/// Default IMAP port (993 for SSL, which is the common case).
fn default_imap_port() -> u16 {
    993
}

/// Default to SSL=true since most providers require it.
fn default_use_ssl() -> bool {
    true
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
pub struct BitrixSource {
    pub id: String,
    pub name: String,
    pub webhook_url: String,
    pub user_id: String,
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
    Bitrix(BitrixSource),
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SourcesConfig {
    pub telegram: Vec<TelegramSource>,
    pub email: Vec<EmailSource>,
    pub jira: Vec<JiraSource>,
    #[serde(default)]
    pub bitrix: Vec<BitrixSource>,
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

    pub fn add_bitrix(&mut self, source: BitrixSource) {
        self.bitrix.push(source);
    }

    pub fn remove_bitrix(&mut self, id: &str) -> bool {
        let len = self.bitrix.len();
        self.bitrix.retain(|s| s.id != id);
        self.bitrix.len() != len
    }

    pub fn update_bitrix(&mut self, id: &str, update: BitrixSource) -> bool {
        if let Some(idx) = self.bitrix.iter().position(|s| s.id == id) {
            self.bitrix[idx] = update;
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

// Generate env vars for Hermes from sources config.
// These must match what Hermes's email/jira MCP servers actually read.
// See briefing.rs::call_smart_briefing_mcp for the canonical env-var contract.
impl SourcesConfig {
    pub fn to_env_vars(&self) -> HashMap<String, String> {
        let mut env = HashMap::new();

        // Telegram: first enabled source.
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

        // Email: first enabled source. Write BOTH EMAIL_ADDRESS and EMAIL_USER
        // because different consumers read different keys:
        //   - mcp-email-life/server.py reads EMAIL_ADDRESS (line 52)
        //   - Hermes agent core reads EMAIL_USER
        // Writing only one breaks the other. Both = same address value.
        if let Some(email) = self.email.iter().find(|s| s.enabled) {
            env.insert("EMAIL_ADDRESS".to_string(), email.address.clone());
            env.insert("EMAIL_USER".to_string(), email.address.clone());
            env.insert("EMAIL_PASSWORD".to_string(), email.password.clone());
            env.insert("EMAIL_HOST".to_string(), email.smtp_host.clone());
            env.insert("EMAIL_SMTP_HOST".to_string(), email.smtp_host.clone());
            env.insert("EMAIL_SMTP_PORT".to_string(), email.smtp_port.to_string());
            env.insert("EMAIL_IMAP_HOST".to_string(), email.imap_host.clone());
            env.insert("EMAIL_IMAP_PORT".to_string(), email.imap_port.to_string());
            env.insert(
                "EMAIL_USE_SSL".to_string(),
                if email.use_ssl { "true" } else { "false" }.to_string(),
            );
        }

        // Jira: first enabled source. Uses JIRA_PAT + JIRA_BASE_URL
        // (matching briefing.rs env-var contract).
        if let Some(jira) = self.jira.iter().find(|s| s.enabled) {
            env.insert("JIRA_PAT".to_string(), jira.api_token.clone());
            env.insert("JIRA_BASE_URL".to_string(), jira.url.clone());
            if !jira.username.is_empty() {
                env.insert("JIRA_USERNAME".to_string(), jira.username.clone());
            }
            if !jira.project_key.is_empty() {
                env.insert("JIRA_PROJECT_KEY".to_string(), jira.project_key.clone());
            }
        }

        // Bitrix: first enabled source.
        if let Some(bx) = self.bitrix.iter().find(|s| s.enabled) {
            env.insert("BITRIX_WEBHOOK".to_string(), bx.webhook_url.clone());
            env.insert("BITRIX_USER_ID".to_string(), bx.user_id.clone());
        }

        env
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_email() -> EmailSource {
        EmailSource {
            id: "test-1".into(),
            name: "Test".into(),
            address: "user@example.com".into(),
            password: "secret".into(),
            smtp_host: "smtp.example.com".into(),
            smtp_port: 587,
            imap_host: "imap.example.com".into(),
            imap_port: 993,
            use_ssl: true,
            enabled: true,
            use_proxy: false,
            proxy_url: String::new(),
        }
    }

    fn make_telegram() -> TelegramSource {
        TelegramSource {
            id: "tg-1".into(),
            name: "Test TG".into(),
            bot_token: "123:ABC".into(),
            chat_id: "-100123".into(),
            allowed_users: "42".into(),
            home_channel: String::new(),
            enabled: true,
            use_proxy: false,
            proxy_url: String::new(),
        }
    }

    fn make_jira() -> JiraSource {
        JiraSource {
            id: "jira-1".into(),
            name: "Test Jira".into(),
            url: "https://company.atlassian.net".into(),
            username: "me".into(),
            api_token: "token123".into(),
            project_key: "PROJ".into(),
            enabled: true,
            use_proxy: false,
            proxy_url: String::new(),
        }
    }

    fn make_bitrix() -> BitrixSource {
        BitrixSource {
            id: "bx-1".into(),
            name: "Test BX".into(),
            webhook_url: "https://company.bitrix24.ru/rest/1/xxx/".into(),
            user_id: "1".into(),
            enabled: true,
            use_proxy: false,
            proxy_url: String::new(),
        }
    }

    #[test]
    fn email_uses_correct_env_var_names() {
        // Both EMAIL_ADDRESS and EMAIL_USER must be emitted — different consumers
        // read different keys:
        //   - mcp-email-life/server.py reads EMAIL_ADDRESS (line 52)
        //   - Hermes agent core reads EMAIL_USER
        // Both must have the same address value.
        let config = SourcesConfig {
            telegram: vec![],
            email: vec![make_email()],
            jira: vec![],
            bitrix: vec![],
        };
        let env = config.to_env_vars();
        assert!(env.contains_key("EMAIL_USER"), "must emit EMAIL_USER");
        assert_eq!(env.get("EMAIL_USER").unwrap(), "user@example.com");
        assert!(env.contains_key("EMAIL_ADDRESS"), "must emit EMAIL_ADDRESS");
        assert_eq!(env.get("EMAIL_ADDRESS").unwrap(), "user@example.com");
        assert_eq!(env.get("EMAIL_IMAP_PORT").unwrap(), "993");
        assert_eq!(env.get("EMAIL_SMTP_PORT").unwrap(), "587");
        assert_eq!(env.get("EMAIL_USE_SSL").unwrap(), "true");
    }

    #[test]
    fn telegram_env_vars() {
        let config = SourcesConfig {
            telegram: vec![make_telegram()],
            email: vec![],
            jira: vec![],
            bitrix: vec![],
        };
        let env = config.to_env_vars();
        assert_eq!(env.get("TELEGRAM_BOT_TOKEN").unwrap(), "123:ABC");
        assert_eq!(env.get("TELEGRAM_CHAT_ID").unwrap(), "-100123");
        assert_eq!(env.get("TELEGRAM_ALLOWED_USERS").unwrap(), "42");
    }

    #[test]
    fn jira_env_vars() {
        let config = SourcesConfig {
            telegram: vec![],
            email: vec![],
            jira: vec![make_jira()],
            bitrix: vec![],
        };
        let env = config.to_env_vars();
        assert_eq!(env.get("JIRA_PAT").unwrap(), "token123");
        assert_eq!(
            env.get("JIRA_BASE_URL").unwrap(),
            "https://company.atlassian.net"
        );
    }

    #[test]
    fn bitrix_env_vars() {
        let config = SourcesConfig {
            telegram: vec![],
            email: vec![],
            jira: vec![],
            bitrix: vec![make_bitrix()],
        };
        let env = config.to_env_vars();
        assert_eq!(
            env.get("BITRIX_WEBHOOK").unwrap(),
            "https://company.bitrix24.ru/rest/1/xxx/"
        );
        assert_eq!(env.get("BITRIX_USER_ID").unwrap(), "1");
    }

    #[test]
    fn only_first_enabled_source_used() {
        let config = SourcesConfig {
            telegram: vec![],
            email: vec![
                EmailSource {
                    id: "first".into(),
                    name: "First".into(),
                    address: "first@example.com".into(),
                    password: "p1".into(),
                    smtp_host: "s1".into(),
                    smtp_port: 587,
                    imap_host: "i1".into(),
                    imap_port: 993,
                    use_ssl: true,
                    enabled: true,
                    use_proxy: false,
                    proxy_url: String::new(),
                },
                EmailSource {
                    id: "second".into(),
                    name: "Second".into(),
                    address: "second@example.com".into(),
                    password: "p2".into(),
                    smtp_host: "s2".into(),
                    smtp_port: 465,
                    imap_host: "i2".into(),
                    imap_port: 993,
                    use_ssl: true,
                    enabled: true,
                    use_proxy: false,
                    proxy_url: String::new(),
                },
            ],
            jira: vec![],
            bitrix: vec![],
        };
        let env = config.to_env_vars();
        assert_eq!(env.get("EMAIL_USER").unwrap(), "first@example.com");
    }

    #[test]
    fn disabled_source_not_emitted() {
        let mut email = make_email();
        email.enabled = false;
        let config = SourcesConfig {
            telegram: vec![],
            email: vec![email],
            jira: vec![],
            bitrix: vec![],
        };
        let env = config.to_env_vars();
        assert!(!env.contains_key("EMAIL_USER"));
    }

    #[test]
    fn bitrix_crud_roundtrip() {
        let mut config = SourcesConfig::default();
        let bx = make_bitrix();
        config.add_bitrix(bx.clone());
        assert_eq!(config.bitrix.len(), 1);

        let mut updated = bx.clone();
        updated.name = "Updated".into();
        assert!(config.update_bitrix(&bx.id, updated));
        assert_eq!(config.bitrix[0].name, "Updated");

        assert!(config.remove_bitrix(&bx.id));
        assert_eq!(config.bitrix.len(), 0);
    }
}