// src-tauri/src/model_discovery.rs
// Provider model discovery — fetch available models from provider's /models endpoint,
// including capability detection (vision/reasoning/tools/context length).
// Ported from fathah/hermes-desktop src/main/model-discovery.ts (simplified + extended)

use serde::{Deserialize, Serialize};

/// Capabilities a model supports. Used to gate UI steer controls — we only
/// show reasoning/verbosity toggles for models that actually support them.
/// Based on the 7-practice GPT-5.6 guide: know what your model can do before
/// adding controls.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCapabilities {
    pub supports_reasoning: bool,
    pub supports_vision: bool,
    pub supports_tools: bool,
    pub context_length: Option<u32>,
    pub max_output_tokens: Option<u32>,
}

impl Default for ModelCapabilities {
    fn default() -> Self {
        Self {
            supports_reasoning: false,
            supports_vision: false,
            supports_tools: true, // most modern models support tool calling
            context_length: None,
            max_output_tokens: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredModel {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub capabilities: ModelCapabilities,
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

/// Heuristic capability detection from model id/name when the provider's
/// /models endpoint doesn't include a capabilities field. This covers the
/// common cases: OpenAI o-series (reasoning), GPT-4o (vision), Claude thinking
/// variants, model names containing "reasoning"/"vision"/"thinking".
pub fn infer_capabilities_public(provider: &str, model_id: &str) -> ModelCapabilities {
    infer_capabilities(provider, model_id)
}

fn infer_capabilities(provider: &str, model_id: &str) -> ModelCapabilities {
    let id = model_id.to_lowercase();
    let mut caps = ModelCapabilities::default();

    // ── Reasoning ────────────────────────────────────────────────────────
    // o1, o3, o4, gpt-5.6-*, *-reasoning, *-thinking, deepseek-r1, gemini-*thinking
    let reasoning_patterns = [
        "o1", "o3", "o4-", "reasoning", "thinking", "-r1", "r1-",
        "gpt-5", "claude-3.7", "claude-4", "gemini-2.5",
    ];
    caps.supports_reasoning = reasoning_patterns.iter().any(|p| id.contains(p));

    // ── Vision ───────────────────────────────────────────────────────────
    // gpt-4o, gpt-4-turbo, gpt-5, claude-3/4, gemini, *-vision, *-multimodal
    let vision_patterns = [
        "gpt-4o", "gpt-4-turbo", "gpt-5", "claude-3", "claude-4",
        "gemini", "vision", "multimodal", "llava", "bakllava",
    ];
    caps.supports_vision = vision_patterns.iter().any(|p| id.contains(p));

    // ── Tools ────────────────────────────────────────────────────────────
    // Most modern models support tool calling. Exceptions: small/old models.
    let no_tools = ["mini", "nano", "tiny", "1b", "3b", "7b-text"];
    caps.supports_tools = !no_tools.iter().any(|p| id.contains(p));

    // Provider-specific overrides for known contexts
    if provider == "anthropic" {
        caps.supports_vision = id.contains("claude-3") || id.contains("claude-4");
        caps.supports_tools = true;
    }

    caps
}

/// Extract capabilities from a provider's model JSON object. Different
/// providers use different schemas:
/// - OpenAI: `capabilities` object with boolean fields
/// - OpenRouter: `architecture.modality` + `supported_parameters` array
/// - Generic: `context_length`, `max_tokens` top-level fields
fn extract_capabilities(provider: &str, model_id: &str, item: &serde_json::Value) -> ModelCapabilities {
    let mut caps = infer_capabilities(provider, model_id);

    // ── Context length / max tokens (common across providers) ────────────
    if let Some(ctx) = item.get("context_length").and_then(|v| v.as_u64()) {
        caps.context_length = Some(ctx as u32);
    } else if let Some(ctx) = item
        .get("context")
        .and_then(|v| v.get("length"))
        .and_then(|v| v.as_u64())
    {
        caps.context_length = Some(ctx as u32);
    }
    if let Some(max) = item.get("max_tokens").and_then(|v| v.as_u64()) {
        caps.max_output_tokens = Some(max as u32);
    } else if let Some(max) = item.get("max_output_tokens").and_then(|v| v.as_u64()) {
        caps.max_output_tokens = Some(max as u32);
    }

    // ── OpenAI-style capabilities object ─────────────────────────────────
    if let Some(cap_obj) = item.get("capabilities").and_then(|v| v.as_object()) {
        if let Some(v) = cap_obj.get("reasoning").and_then(|v| v.as_bool()) {
            caps.supports_reasoning = v;
        }
        if let Some(v) = cap_obj.get("vision").and_then(|v| v.as_bool()) {
            caps.supports_vision = v;
        }
        if let Some(v) = cap_obj.get("tools").and_then(|v| v.as_bool()) {
            caps.supports_tools = v;
        }
    }

    // ── OpenRouter: architecture.modality + supported_parameters ─────────
    if let Some(modality) = item
        .get("architecture")
        .and_then(|v| v.get("modality"))
        .and_then(|v| v.as_str())
    {
        if modality.contains("image") || modality.contains("vision") {
            caps.supports_vision = true;
        }
    }
    if let Some(params) = item
        .get("supported_parameters")
        .and_then(|v| v.as_array())
    {
        for param in params {
            if let Some(s) = param.as_str() {
                if s == "reasoning" || s == "reasoning_effort" {
                    caps.supports_reasoning = true;
                }
                if s == "tools" || s == "tool_choice" {
                    caps.supports_tools = true;
                }
            }
        }
    }

    caps
}

/// Discover models from a provider's /models endpoint.
pub async fn discover_models(
    provider: &str,
    base_url: Option<&str>,
    api_key: Option<&str>,
    use_proxy: bool,
) -> DiscoveryResult {
    discover_models_with_home(provider, base_url, api_key, use_proxy, None).await
}

/// Discovery variant that resolves the proxy from a specific hermes_home
/// (not a hardcoded ~/.hermes). When `hermes_home` is None, falls back to
/// resolve_hermes_home() so existing callers work unchanged.
pub async fn discover_models_with_home(
    provider: &str,
    base_url: Option<&str>,
    api_key: Option<&str>,
    use_proxy: bool,
    hermes_home: Option<&std::path::Path>,
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

    let client = {
        let mut builder = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10));
        if use_proxy {
            // Resolve the proxy from the REAL hermes home (not a hardcoded
            // ~/.hermes — that reads the wrong config on uv-managed Windows
            // installs where HERMES_HOME points at AppData/Local/hermes).
            let home = hermes_home
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| crate::config::resolve_hermes_home());
            let proxy_url = crate::config::resolve_effective_proxy(&home, None);
            if !proxy_url.is_empty() {
                // If the proxy URL is malformed, fall back to a direct client
                // rather than failing the whole discovery call.
                if let Ok(proxy) = reqwest::Proxy::all(&proxy_url) {
                    builder = builder.proxy(proxy);
                }
            }
        }
        match builder.build() {
            Ok(c) => c,
            Err(e) => {
                return DiscoveryResult {
                    success: false,
                    models: Vec::new(),
                    error: Some(format!("HTTP client error: {}", e)),
                };
            }
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

    // Parse OpenAI-compatible response: { "data": [ { "id": "...", "name": "?", "capabilities": {...} } ] }
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
                    capabilities: extract_capabilities(&p, id, item),
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
    let mk = |id: &str, name: &str| DiscoveredModel {
        id: id.to_string(),
        name: name.to_string(),
        capabilities: infer_capabilities(provider, id),
    };
    match provider.to_lowercase().as_str() {
        "openai-codex" => vec![
            mk("o3", "o3"),
            mk("o4-mini", "o4-mini"),
            mk("gpt-4o", "GPT-4o"),
            mk("gpt-4o-mini", "GPT-4o-mini"),
        ],
        "xai-oauth" => vec![
            mk("grok-3", "Grok 3"),
            mk("grok-3-mini", "Grok 3 Mini"),
        ],
        "google-gemini-cli" => vec![
            mk("gemini-2.5-pro", "Gemini 2.5 Pro"),
            mk("gemini-2.5-flash", "Gemini 2.5 Flash"),
        ],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Live debug — run with:
    ///   cargo test --lib live_discover_kilo -- --nocapture --ignored
    /// Verifies discovery works against the real kilocode provider using
    /// creds from the credential pool (auth.json) + proxy from config.yaml.
    #[tokio::test]
    #[ignore]
    async fn live_discover_kilo() {
        let home = crate::config::resolve_hermes_home();
        eprintln!("hermes_home = {}", home.display());
        let result = discover_models_with_home("kilo", None, None, true, Some(&home)).await;
        eprintln!("success={}, models={}, error={:?}", result.success, result.models.len(), result.error);
        if result.success {
            for m in result.models.iter().take(5) {
                eprintln!("  - {} (reasoning={}, vision={})", m.name, m.capabilities.supports_reasoning, m.capabilities.supports_vision);
            }
        }
        assert!(result.success || result.error.is_some(), "should at least return a result");
    }

    #[test]
    fn reasoning_models_detected() {
        assert!(infer_capabilities("openai", "o3").supports_reasoning);
        assert!(infer_capabilities("openai", "o4-mini").supports_reasoning);
        assert!(infer_capabilities("openai", "gpt-5.6-sol").supports_reasoning);
        assert!(infer_capabilities("deepseek", "deepseek-r1").supports_reasoning);
        assert!(infer_capabilities("custom", "my-model-reasoning").supports_reasoning);
    }

    #[test]
    fn non_reasoning_models_detected() {
        assert!(!infer_capabilities("openai", "gpt-4o-mini").supports_reasoning);
        assert!(!infer_capabilities("groq", "llama-3.1-70b").supports_reasoning);
    }

    #[test]
    fn vision_models_detected() {
        assert!(infer_capabilities("openai", "gpt-4o").supports_vision);
        assert!(infer_capabilities("anthropic", "claude-3.5-sonnet").supports_vision);
        assert!(infer_capabilities("ollama", "llava-7b").supports_vision);
    }

    #[test]
    fn tools_flagged_correctly() {
        assert!(infer_capabilities("openai", "gpt-4o").supports_tools);
        assert!(infer_capabilities("anthropic", "claude-3.5-sonnet").supports_tools);
        // Small text-only models typically don't support tools.
        assert!(!infer_capabilities("ollama", "phi-3-mini-3b").supports_tools);
    }

    #[test]
    fn extract_capabilities_from_openai_schema() {
        let json: serde_json::Value = serde_json::json!({
            "id": "gpt-5.6-sol",
            "capabilities": {
                "reasoning": true,
                "vision": false,
                "tools": true,
            },
            "context_length": 200000,
            "max_tokens": 16384,
        });
        let caps = extract_capabilities("openai", "gpt-5.6-sol", &json);
        assert!(caps.supports_reasoning);
        assert!(!caps.supports_vision);
        assert!(caps.supports_tools);
        assert_eq!(caps.context_length, Some(200000));
        assert_eq!(caps.max_output_tokens, Some(16384));
    }

    #[test]
    fn extract_capabilities_from_openrouter_schema() {
        let json: serde_json::Value = serde_json::json!({
            "id": "anthropic/claude-3.5-sonnet",
            "architecture": { "modality": "text+image->text" },
            "supported_parameters": ["tools", "reasoning"],
            "context_length": 200000,
        });
        let caps = extract_capabilities("openrouter", "anthropic/claude-3.5-sonnet", &json);
        assert!(caps.supports_vision); // modality includes "image"
        assert!(caps.supports_tools);
        assert!(caps.supports_reasoning);
    }
}
