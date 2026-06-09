// src-tauri/src/registry.rs
// Community marketplace registry — fetch catalog from GitHub.
// Ported from fathah/hermes-desktop src/main/registry.rs (simplified)

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

// ── Types ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryItem {
    pub id: String,
    pub name: String,
    pub description: String,
    pub author: Option<String>,
    pub category: Option<String>,
    pub tags: Vec<String>,
    pub homepage: Option<String>,
    pub version: Option<String>,
    pub license: Option<String>,
    pub platforms: Vec<String>,
    pub path: Option<String>,
    pub source: Option<String>,
    pub kind: String, // "skill" | "mcp" | "agent" | "workflow"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryCatalog {
    pub skills: Vec<RegistryItem>,
    pub mcps: Vec<RegistryItem>,
    pub agents: Vec<RegistryItem>,
    pub workflows: Vec<RegistryItem>,
}

impl Default for RegistryCatalog {
    fn default() -> Self {
        Self {
            skills: Vec::new(),
            mcps: Vec::new(),
            agents: Vec::new(),
            workflows: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryDetail {
    pub markdown: Option<String>,
    pub description: Option<String>,
    pub rows: Vec<RegistryDetailRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryDetailRow {
    pub label: String,
    pub value: Option<String>,
    pub mono: bool,
    pub chips: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledRegistry {
    pub skills: Vec<String>,
    pub mcps: Vec<String>,
    pub workflows: Vec<String>,
}

// ── Index entry from registry repo ────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct IndexEntry {
    id: String,
    #[serde(rename = "type")]
    entry_type: String,
    category: Option<String>,
    name: String,
    version: Option<String>,
    description: Option<String>,
    tags: Option<Vec<String>>,
    author: Option<serde_json::Value>,
    license: Option<String>,
    platforms: Option<Vec<String>>,
    path: Option<String>,
}

// ── Registry client ──────────────────────────────────────────────────────

const REGISTRY_REPO: &str = "fathah/hermes-registry";
const REGISTRY_BRANCH: &str = "main";
const INDEX_URL: &str = "https://raw.githubusercontent.com/fathah/hermes-registry/refs/heads/main/index.json";

/// Fetch the registry catalog from GitHub.
pub async fn fetch_catalog() -> Result<RegistryCatalog, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let response = client
        .get(INDEX_URL)
        .send()
        .await
        .map_err(|e| format!("Registry fetch error: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Registry returned status: {}", response.status()));
    }

    let entries: Vec<IndexEntry> = response
        .json()
        .await
        .map_err(|e| format!("Registry parse error: {}", e))?;

    let mut catalog = RegistryCatalog::default();

    for entry in entries {
        let item = RegistryItem {
            id: entry.id.clone(),
            name: entry.name,
            description: entry.description.unwrap_or_default(),
            author: match entry.author {
                Some(serde_json::Value::String(s)) => Some(s),
                Some(serde_json::Value::Object(obj)) => obj
                    .get("name")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                _ => None,
            },
            category: entry.category,
            tags: entry.tags.unwrap_or_default(),
            homepage: Some(format!(
                "https://github.com/{}/tree/{}",
                REGISTRY_REPO, REGISTRY_BRANCH
            )),
            version: entry.version,
            license: entry.license,
            platforms: entry.platforms.unwrap_or_default(),
            path: entry.path.clone(),
            source: entry.path,
            kind: entry.entry_type.clone(),
        };

        match entry.entry_type.as_str() {
            "skill" => catalog.skills.push(item),
            "mcp" => catalog.mcps.push(item),
            "agent" => catalog.agents.push(item),
            "workflow" => catalog.workflows.push(item),
            _ => {}
        }
    }

    Ok(catalog)
}

/// Get installed items (skills, mcps, workflows).
pub fn get_installed(hermes_home: &Path, profile: Option<&str>) -> InstalledRegistry {
    let skills = crate::skills::list_installed_skills(hermes_home, profile)
        .iter()
        .map(|s| s.name.clone())
        .collect();

    let mcps = crate::mcp::list_mcp_servers(hermes_home, profile)
        .iter()
        .map(|m| m.name.clone())
        .collect();

    InstalledRegistry {
        skills,
        mcps,
        workflows: Vec::new(), // workflows not yet implemented
    }
}

/// Install a skill from the registry.
pub fn install_from_registry(
    hermes_home: &Path,
    profile: Option<&str>,
    item: &RegistryItem,
) -> Result<(), String> {
    let source = item.source.as_ref().ok_or("No source path for this item")?;
    crate::skills::install_skill(hermes_home, profile, source)
}
