// src-tauri/src/provider_registry.rs
// Canonical inference base URLs for built-in providers.
// Ported from fathah/hermes-desktop src/main/provider-registry.ts

use std::collections::HashMap;

/// Look up the canonical inference base URL for a built-in provider id.
/// Returns None when the provider isn't in the registry (e.g. custom, auto).
pub fn canonical_base_url(provider: &str) -> Option<&'static str> {
    match provider.to_lowercase().as_str() {
        "openai" => Some("https://api.openai.com/v1"),
        "openrouter" => Some("https://openrouter.ai/api/v1"),
        "ollama-cloud" => Some("https://ollama.com/v1"),
        "deepseek" => Some("https://api.deepseek.com/v1"),
        "groq" => Some("https://api.groq.com/openai/v1"),
        "mistral" => Some("https://api.mistral.ai/v1"),
        "together" => Some("https://api.together.xyz/v1"),
        "fireworks" => Some("https://api.fireworks.ai/inference/v1"),
        "atlascloud" => Some("https://api.atlascloud.ai/v1"),
        "cerebras" => Some("https://api.cerebras.ai/v1"),
        "perplexity" => Some("https://api.perplexity.ai"),
        "huggingface" => Some("https://router.huggingface.co/v1"),
        "xiaomi" => Some("https://api.xiaomimimo.com/v1"),
        "zai" => Some("https://api.z.ai/api/paas/v4"),
        "anthropic" => Some("https://api.anthropic.com/v1"),
        "lmstudio" => Some("http://localhost:1234/v1"),
        "atomicchat" => Some("http://localhost:1337/v1"),
        "ollama" => Some("http://localhost:11434/v1"),
        "vllm" => Some("http://localhost:8000/v1"),
        "llamacpp" => Some("http://localhost:8080/v1"),
        "kilocode" | "kilo" => Some("https://api.kilo.ai/api/gateway"),
        "copilot" => Some("https://api.githubcopilot.com"),
        _ => None,
    }
}

/// Get all known provider IDs — single source of truth for the frontend provider list.
pub fn all_provider_ids() -> Vec<&'static str> {
    vec![
        "openai",
        "openrouter",
        "ollama-cloud",
        "deepseek",
        "groq",
        "mistral",
        "together",
        "fireworks",
        "atlascloud",
        "cerebras",
        "perplexity",
        "huggingface",
        "xiaomi",
        "zai",
        "anthropic",
        "lmstudio",
        "atomicchat",
        "ollama",
        "vllm",
        "llamacpp",
        "kilocode",
        "copilot",
    ]
}

/// Get all provider base URLs as a HashMap.
pub fn all_provider_urls() -> HashMap<String, String> {
    let mut map = HashMap::new();
    for (k, v) in [
        ("openai", "https://api.openai.com/v1"),
        ("openrouter", "https://openrouter.ai/api/v1"),
        ("ollama-cloud", "https://ollama.com/v1"),
        ("deepseek", "https://api.deepseek.com/v1"),
        ("groq", "https://api.groq.com/openai/v1"),
        ("mistral", "https://api.mistral.ai/v1"),
        ("together", "https://api.together.xyz/v1"),
        ("fireworks", "https://api.fireworks.ai/inference/v1"),
        ("atlascloud", "https://api.atlascloud.ai/v1"),
        ("cerebras", "https://api.cerebras.ai/v1"),
        ("perplexity", "https://api.perplexity.ai"),
        ("huggingface", "https://router.huggingface.co/v1"),
        ("xiaomi", "https://api.xiaomimimo.com/v1"),
        ("zai", "https://api.z.ai/api/paas/v4"),
        ("anthropic", "https://api.anthropic.com/v1"),
        ("lmstudio", "http://localhost:1234/v1"),
        ("atomicchat", "http://localhost:1337/v1"),
        ("ollama", "http://localhost:11434/v1"),
        ("vllm", "http://localhost:8000/v1"),
        ("llamacpp", "http://localhost:8080/v1"),
        ("kilocode", "https://api.kilo.ai/api/gateway"),
        ("copilot", "https://api.githubcopilot.com"),
    ] {
        map.insert(k.to_string(), v.to_string());
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_providers_have_urls() {
        assert_eq!(
            canonical_base_url("openai"),
            Some("https://api.openai.com/v1")
        );
        assert_eq!(
            canonical_base_url("anthropic"),
            Some("https://api.anthropic.com/v1")
        );
        assert_eq!(
            canonical_base_url("openrouter"),
            Some("https://openrouter.ai/api/v1")
        );
    }

    #[test]
    fn unknown_provider_returns_none() {
        assert_eq!(canonical_base_url("custom"), None);
        assert_eq!(canonical_base_url("auto"), None);
        assert_eq!(canonical_base_url("nonexistent"), None);
    }

    #[test]
    fn case_insensitive() {
        assert_eq!(canonical_base_url("OpenAI"), canonical_base_url("openai"));
        assert_eq!(canonical_base_url("GROQ"), canonical_base_url("groq"));
    }

    #[test]
    fn all_ids_resolve_to_urls() {
        // Cross-consistency: every id in all_provider_ids must have a URL in
        // canonical_base_url AND all_provider_urls. Guards against drift
        // between the three duplicated literals.
        let urls = all_provider_urls();
        for id in all_provider_ids() {
            assert!(
                canonical_base_url(id).is_some(),
                "Provider '{}' missing from canonical_base_url",
                id
            );
            assert!(
                urls.contains_key(id),
                "Provider '{}' missing from all_provider_urls",
                id
            );
        }
    }

    #[test]
    fn provider_count_is_stable() {
        // Lock the count so adding a provider forces a test update (and thus
        // a conscious check that the URL table is also updated).
        assert_eq!(all_provider_ids().len(), 22);
        assert_eq!(all_provider_urls().len(), 22);
    }
}
