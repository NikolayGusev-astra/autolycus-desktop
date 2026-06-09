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

/// Save Telegram config to desktop.json
pub fn save_config(
    hermes_home: &std::path::Path,
    config: &TelegramConfig,
) -> Result<(), String> {
    let config_path = hermes_home.join("telegram.json");
    let json = serde_json::to_string_pretty(config)
        .map_err(|e| format!("Serialization error: {}", e))?;
    std::fs::write(&config_path, json)
        .map_err(|e| format!("Write error: {}", e))?;
    Ok(())
}

/// Load Telegram config from desktop.json
pub fn load_config(hermes_home: &std::path::Path) -> TelegramConfig {
    let config_path = hermes_home.join("telegram.json");
    if !config_path.exists() {
        return TelegramConfig {
            bot_token: String::new(),
            chat_id: String::new(),
            enabled: false,
        };
    }

    match std::fs::read_to_string(&config_path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_else(|_| TelegramConfig {
            bot_token: String::new(),
            chat_id: String::new(),
            enabled: false,
        }),
        Err(_) => TelegramConfig {
            bot_token: String::new(),
            chat_id: String::new(),
            enabled: false,
        },
    }
}
