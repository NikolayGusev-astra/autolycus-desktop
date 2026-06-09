// src-tauri/src/validation.rs
// Pre-send chat readiness validation.
// Ported from fathah/hermes-desktop src/main/validation.ts

use std::path::Path;

use serde::Serialize;

use crate::config;

/// Readiness check result code.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ChatReadinessCode {
    NoActiveModel,
    NoProvider,
    NoBaseUrl,
    MissingApiKey,
    GatewayDown,
}

/// Where to send the user to fix the issue.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FixLocation {
    Providers,
    Models,
    Gateway,
    Setup,
}

/// Chat readiness check result.
#[derive(Debug, Clone, Serialize)]
pub struct ChatReadiness {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<ChatReadinessCode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix_location: Option<FixLocation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_env_key: Option<String>,
}

impl Default for ChatReadiness {
    fn default() -> Self {
        Self {
            ok: true,
            code: None,
            message: None,
            fix_location: None,
            expected_env_key: None,
        }
    }
}

/// Validate chat readiness — synchronous check against config, no network calls.
/// Fail open: any error returns ok=true.
pub fn validate_chat_readiness(
    hermes_home: &Path,
    profile: Option<&str>,
) -> ChatReadiness {
    let mc = config::get_model_config(hermes_home, profile);
    let provider = mc.provider.trim().to_lowercase();
    let model = mc.model.trim();
    let base_url = mc.base_url.trim();

    // Auto provider — skip check
    if provider.is_empty() || provider == "auto" {
        return ChatReadiness::default();
    }

    // No model selected
    if model.is_empty() {
        return ChatReadiness {
            ok: false,
            code: Some(ChatReadinessCode::NoActiveModel),
            message: Some("No model selected. Pick one in Models or the Chat picker.".to_string()),
            fix_location: Some(FixLocation::Models),
            ..Default::default()
        };
    }

    // Check API key
    if !config::has_api_key_for_provider(hermes_home, profile, &provider, base_url) {
        let expected_key = config::expected_env_key_for_url(base_url);
        return ChatReadiness {
            ok: false,
            code: Some(ChatReadinessCode::MissingApiKey),
            message: Some(format!(
                "Missing {} for {}. Set it in Providers.",
                expected_key, provider
            )),
            fix_location: Some(FixLocation::Providers),
            expected_env_key: Some(expected_key.to_string()),
        };
    }

    ChatReadiness::default()
}
