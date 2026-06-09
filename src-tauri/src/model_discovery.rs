// src-tauri/src/model_discovery.rs
// Provider model discovery — fetch available models from provider's /models endpoint.
// Ported from fathah/hermes-desktop src/main/model-discovery.ts (simplified)

use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize)]
pub struct DiscoveredModel {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiscoveryResult {
    pub success: bool,
    pub models: Vec<DiscoveredModel>,
    pub error: Option<String>,
}

/// Providers whose /models we never call.
const NON_DISCOVERABLE: &[&str] = &[
    "auto", "custom", "google", "xai", "qwen", "minimax", "kimi-coding",
];

/// OAuth/subscription providers — no static-key /v1/models endpoint.
const OAUTH_PROVIDERS: &[&str] = &[
    "openai-codex", "xai-oauth", "qwen-oauth", "google-gemini-cli",
    "minimax-oauth", "nous",
];

/// Check if a provider supports model discovery.
pub fn is_discoverable(provider: &str) -> bool {
    let p = provider.to_lowercase();
    !NON_DISCOVERABLE.contains(&p.as_str()) && !OAUTH_PROVIDERS.contains(&p.as_str())
}

/// Get the base URL for a provider (uses provider_registry).
fn provider_base_url(provider: &str) -> Option<&'static str> {
    crate::provider_registry::canonical_base_url(provider)
}

/// Discover models from a provider's /models endpoint.
pub async fn discover_models(
    provider: &str,
    base_url: Option<&str>,
    api_key: Option<&str>,
) -> DiscoveryResult {
    let p = provider.to_lowercase();

    if NON_DISCOVERABLE.contains(&p.as_str()) {
        return DiscoveryResult {
            success: false,
            models: Vec::new(),
            error: Some(format!("Provider '{}' does not support model discovery", provider)),
        };
    }

    if OAUTH_PROVIDERS.contains(&p.as_str()) {
        return DiscoveryResult {
            success: false,
            models: Vec::new(),
            error: Some(format!("Provider '{}' requires OAuth for model discovery", provider)),
        };
    }

    let url = base_url.or_else(|| provider_base_url(provider));
    let url = match url {
        Some(u) => format!("{}/models", u.trim_end_matches('/')),
        None => {
            return DiscoveryResult {
                success: false,
                models: Vec::new(),
                error: Some(format!("No base URL for provider '{}'", provider)),
            };
        }
    };

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return DiscoveryResult {
                success: false,
                models: Vec::new(),
                error: Some(format!("HTTP client error: {}", e)),
            };
        }
    };

    let mut req = client.get(&url);
    if let Some(key) = api_key {
        if !key.is_empty() {
            req = req.bearer_auth(key);
        }
    }

    let response = match req.send().await {
        Ok(r) => r,
        Err(e) => {
            return DiscoveryResult {
                success: false,
                models: Vec::new(),
                error: Some(format!("Request error: {}", e)),
            };
        }
    };

    if !response.status().is_success() {
        return DiscoveryResult {
            success: false,
            models: Vec::new(),
            error: Some(format!("HTTP {}", response.status())),
        };
    }

    let json: serde_json::Value = match response.json().await {
        Ok(j) => j,
        Err(e) => {
            return DiscoveryResult {
                success: false,
                models: Vec::new(),
                error: Some(format!("JSON parse error: {}", e)),
            };
        }
    };

    // Parse OpenAI-compatible response: { "data": [ { "id": "...", "name": "?" } ] }
    let mut models = Vec::new();
    if let Some(data) = json.get("data").and_then(|d| d.as_array()) {
        for item in data {
            if let Some(id) = item.get("id").and_then(|i| i.as_str()) {
                let name = item
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or(id);
                models.push(DiscoveredModel {
                    id: id.to_string(),
                    name: name.to_string(),
                });
            }
        }
    }

    DiscoveryResult {
        success: true,
        models,
        error: None,
    }
}

/// Get hardcoded model list for OAuth providers.
pub fn get_oauth_models(provider: &str) -> Vec<DiscoveredModel> {
    match provider.to_lowercase().as_str() {
        "openai-codex" => vec![
            DiscoveredModel { id: "o3".to_string(), name: "o3".to_string() },
            DiscoveredModel { id: "o4-mini".to_string(), name: "o4-mini".to_string() },
            DiscoveredModel { id: "gpt-4o".to_string(), name: "GPT-4o".to_string() },
            DiscoveredModel { id: "gpt-4o-mini".to_string(), name: "GPT-4o-mini".to_string() },
        ],
        "xai-oauth" => vec![
            DiscoveredModel { id: "grok-3".to_string(), name: "Grok 3".to_string() },
            DiscoveredModel { id: "grok-3-mini".to_string(), name: "Grok 3 Mini".to_string() },
        ],
        "google-gemini-cli" => vec![
            DiscoveredModel { id: "gemini-2.5-pro".to_string(), name: "Gemini 2.5 Pro".to_string() },
            DiscoveredModel { id: "gemini-2.5-flash".to_string(), name: "Gemini 2.5 Flash".to_string() },
        ],
        _ => Vec::new(),
    }
}
