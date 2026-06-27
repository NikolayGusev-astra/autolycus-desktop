// src-tauri/src/telegram.rs
// Telegram Bot API integration for auto-delivery.
// Sends messages to a Telegram chat via Bot API.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramConfig {
    pub bot_token: String,
    pub chat_id: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct TelegramResult {
    pub success: bool,
    pub error: Option<String>,
}

/// Send a message to Telegram chat.
pub async fn send_message(
    bot_token: &str,
    chat_id: &str,
    text: &str,
) -> TelegramResult {
    if bot_token.is_empty() || chat_id.is_empty() {
        return TelegramResult {
            success: false,
            error: Some("Bot token and chat ID are required".to_string()),
        };
    }

    let url = format!(
        "https://api.telegram.org/bot{}/sendMessage",
        bot_token
    );

    let mut params = HashMap::new();
    params.insert("chat_id", chat_id.to_string());
    params.insert("text", text.to_string());
    params.insert("parse_mode", "HTML".to_string());

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return TelegramResult {
                success: false,
                error: Some(format!("HTTP client error: {}", e)),
            };
        }
    };

    match client.post(&url).json(&params).send().await {
        Ok(resp) => {
            if resp.status().is_success() {
                TelegramResult {
                    success: true,
                    error: None,
                }
            } else {
                TelegramResult {
                    success: false,
                    error: Some(format!("Telegram API error: HTTP {}", resp.status())),
                }
            }
        }
        Err(e) => TelegramResult {
            success: false,
            error: Some(format!("Request error: {}", e)),
        },
    }
}

/// Validate a bot token by calling getMe.
pub async fn validate_bot_token(bot_token: &str) -> TelegramResult {
    if bot_token.is_empty() {
        return TelegramResult {
            success: false,
            error: Some("Bot token is required".to_string()),
        };
    }

    let url = format!(
        "https://api.telegram.org/bot{}/getMe",
        bot_token
    );

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return TelegramResult {
                success: false,
                error: Some(format!("HTTP client error: {}", e)),
            };
        }
    };

    match client.get(&url).send().await {
        Ok(resp) => {
            if resp.status().is_success() {
                TelegramResult {
                    success: true,
                    error: None,
                }
            } else {
                TelegramResult {
                    success: false,
                    error: Some(format!("Invalid bot token: HTTP {}", resp.status())),
                }
            }
        }
        Err(e) => TelegramResult {
            success: false,
            error: Some(format!("Request error: {}", e)),
        },
    }
}

/// Save Telegram config.
///
/// The bot token is stored in the OS keyring; telegram.json keeps only
/// `chat_id`/`enabled` (no secret). A one-time plaintext token still present
/// on disk is migrated on load.
pub fn save_config(
    hermes_home: &std::path::Path,
    config: &TelegramConfig,
) -> Result<(), String> {
    // Token → keyring.
    crate::secrets::set(crate::secrets::account::TELEGRAM_BOT_TOKEN, &config.bot_token)?;

    // Non-secret fields → telegram.json (still 0600 for consistency).
    let on_disk = TelegramConfig {
        bot_token: String::new(),
        chat_id: config.chat_id.clone(),
        enabled: config.enabled,
    };
    let config_path = hermes_home.join("telegram.json");
    let json = serde_json::to_string_pretty(&on_disk)
        .map_err(|e| format!("Serialization error: {}", e))?;
    write_secret_file(&config_path, &json).map_err(|e| format!("Write error: {}", e))?;
    Ok(())
}

/// Write a file with mode 0600 on unix (owner-only).
fn write_secret_file(path: &std::path::Path, content: &str) -> std::io::Result<()> {
    std::fs::write(path, content)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

/// Load Telegram config.
///
/// The bot token is read from the keyring. If a plaintext token is still in
/// telegram.json (pre-migration), it is moved to the keyring and cleared on
/// disk on the next save.
pub fn load_config(hermes_home: &std::path::Path) -> TelegramConfig {
    let config_path = hermes_home.join("telegram.json");
    let mut on_disk = if !config_path.exists() {
        TelegramConfig {
            bot_token: String::new(),
            chat_id: String::new(),
            enabled: false,
        }
    } else {
        std::fs::read_to_string(&config_path)
            .ok()
            .and_then(|c| serde_json::from_str(&c).ok())
            .unwrap_or_else(|| TelegramConfig {
                bot_token: String::new(),
                chat_id: String::new(),
                enabled: false,
            })
    };

    // One-time migration of a plaintext token into the keyring.
    if !on_disk.bot_token.is_empty() {
        if crate::secrets::migrate(crate::secrets::account::TELEGRAM_BOT_TOKEN, &on_disk.bot_token) {
            on_disk.bot_token.clear();
            // Persist the cleaned config immediately so the token is gone.
            let cleaned = TelegramConfig {
                bot_token: String::new(),
                chat_id: on_disk.chat_id.clone(),
                enabled: on_disk.enabled,
            };
            let json = serde_json::to_string_pretty(&cleaned).unwrap_or_default();
            let _ = write_secret_file(&config_path, &json);
        }
    }

    // Resolve the token from the keyring (fallback to empty / plaintext).
    let bot_token = crate::secrets::get(crate::secrets::account::TELEGRAM_BOT_TOKEN)
        .ok()
        .flatten()
        .unwrap_or_default();

    TelegramConfig {
        bot_token,
        chat_id: on_disk.chat_id,
        enabled: on_disk.enabled,
    }
}
