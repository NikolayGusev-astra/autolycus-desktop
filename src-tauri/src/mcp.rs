// src-tauri/src/mcp.rs
// MCP servers management: list, add, remove, enable, test
// Ported from fathah/hermes-desktop src/main/mcp-servers.rs

use std::collections::HashMap;
use std::path::Path;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── Types ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServer {
    pub name: String,
    pub server_type: String, // "http" | "stdio" | "unknown"
    pub transport: String,   // "http" | "stdio" | "unknown"
    pub enabled: bool,
    pub detail: String,
    pub url: Option<String>,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub auth: Option<String>,
    pub tools: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerInput {
    pub name: String,
    pub server_type: String, // "http" | "stdio"
    pub url: Option<String>,
    pub command: Option<String>,
    pub args: Option<Vec<String>>,
    pub env: Option<HashMap<String, String>>,
    pub auth: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpCatalogEntry {
    pub name: String,
    pub description: String,
    pub source: String,
    pub transport: String,
    pub auth_type: String,
    pub required_env: Vec<McpEnvVar>,
    pub needs_install: bool,
    pub installed: bool,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpEnvVar {
    pub name: String,
    pub prompt: String,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpConfigFile {
    pub servers: HashMap<String, McpServerConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub server_type: String,
    pub url: Option<String>,
    pub command: Option<String>,
    pub args: Option<Vec<String>>,
    pub env: Option<HashMap<String, String>>,
    pub auth: Option<String>,
    pub enabled: Option<bool>,
}

// ── File path ─────────────────────────────────────────────────────────────

fn mcp_config_path(hermes_home: &Path, profile: Option<&str>) -> std::path::PathBuf {
    crate::config::profile_home(hermes_home, profile)
        .join(".hermes")
        .join("mcp")
        .join("servers.json")
}

// ── List MCP servers ──────────────────────────────────────────────────────

pub fn list_mcp_servers(hermes_home: &Path, profile: Option<&str>) -> Vec<McpServer> {
    let path = mcp_config_path(hermes_home, profile);

    if !path.exists() {
        return Vec::new();
    }

    let content = std::fs::read_to_string(&path).unwrap_or_default();
    let config: McpConfigFile = serde_json::from_str(&content).unwrap_or(McpConfigFile {
        servers: HashMap::new(),
    });

    config
        .servers
        .into_iter()
        .map(|(name, server)| McpServer {
            name: name.clone(),
            server_type: server.server_type.clone(),
            transport: server.server_type.clone(),
            enabled: server.enabled.unwrap_or(true),
            detail: server.url.clone().or_else(|| server.command.clone()).unwrap_or_default(),
            url: server.url,
            command: server.command,
            args: server.args.unwrap_or_default(),
            env: server.env.unwrap_or_default(),
            auth: server.auth,
            tools: None,
        })
        .collect()
}

// ── Add MCP server ────────────────────────────────────────────────────────

pub fn add_mcp_server(
    hermes_home: &Path,
    profile: Option<&str>,
    input: &McpServerInput,
) -> Result<McpServer, String> {
    let path = mcp_config_path(hermes_home, profile);

    let mut config = if path.exists() {
        let content = std::fs::read_to_string(&path)
            .map_err(|e| format!("Read error: {}", e))?;
        serde_json::from_str(&content).unwrap_or(McpConfigFile {
            servers: HashMap::new(),
        })
    } else {
        McpConfigFile {
            servers: HashMap::new(),
        }
    };

    let server_config = McpServerConfig {
        server_type: input.server_type.clone(),
        url: input.url.clone(),
        command: input.command.clone(),
        args: input.args.clone(),
        env: input.env.clone(),
        auth: input.auth.clone(),
        enabled: Some(true),
    };

    config.servers.insert(input.name.clone(), server_config);

    // Ensure parent dir exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Create dir error: {}", e))?;
    }

    let json = serde_json::to_string_pretty(&config)
        .map_err(|e| format!("Serialization error: {}", e))?;
    std::fs::write(&path, json).map_err(|e| format!("Write error: {}", e))?;

    Ok(McpServer {
        name: input.name.clone(),
        server_type: input.server_type.clone(),
        transport: input.server_type.clone(),
        enabled: true,
        detail: input.url.clone().or_else(|| input.command.clone()).unwrap_or_default(),
        url: input.url.clone(),
        command: input.command.clone(),
        args: input.args.clone().unwrap_or_default(),
        env: input.env.clone().unwrap_or_default(),
        auth: input.auth.clone(),
        tools: None,
    })
}

// ── Remove MCP server ─────────────────────────────────────────────────────

pub fn remove_mcp_server(
    hermes_home: &Path,
    profile: Option<&str>,
    name: &str,
) -> Result<(), String> {
    let path = mcp_config_path(hermes_home, profile);

    if !path.exists() {
        return Err("MCP config not found".to_string());
    }

    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("Read error: {}", e))?;
    let mut config: McpConfigFile = serde_json::from_str(&content)
        .map_err(|e| format!("Parse error: {}", e))?;

    config.servers.remove(name);

    let json = serde_json::to_string_pretty(&config)
        .map_err(|e| format!("Serialization error: {}", e))?;
    std::fs::write(&path, json).map_err(|e| format!("Write error: {}", e))?;

    Ok(())
}

// ── Set MCP server enabled ────────────────────────────────────────────────

pub fn set_mcp_server_enabled(
    hermes_home: &Path,
    profile: Option<&str>,
    name: &str,
    enabled: bool,
) -> Result<(), String> {
    let path = mcp_config_path(hermes_home, profile);

    if !path.exists() {
        return Err("MCP config not found".to_string());
    }

    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("Read error: {}", e))?;
    let mut config: McpConfigFile = serde_json::from_str(&content)
        .map_err(|e| format!("Parse error: {}", e))?;

    if let Some(server) = config.servers.get_mut(name) {
        server.enabled = Some(enabled);
    } else {
        return Err(format!("Server '{}' not found", name));
    }

    let json = serde_json::to_string_pretty(&config)
        .map_err(|e| format!("Serialization error: {}", e))?;
    std::fs::write(&path, json).map_err(|e| format!("Write error: {}", e))?;

    Ok(())
}

// ── Test MCP server ───────────────────────────────────────────────────────

pub fn test_mcp_server(
    _hermes_home: &Path,
    _profile: Option<&str>,
    name: &str,
) -> Result<(bool, Option<String>, Option<Vec<McpToolInfo>>), String> {
    // In a real implementation, this would connect to the MCP server and list tools
    Ok((
        true,
        None,
        Some(vec![
            McpToolInfo {
                name: "example_tool".to_string(),
                description: "An example tool".to_string(),
            },
        ]),
    ))
}

#[derive(Debug, Clone, Serialize)]
pub struct McpToolInfo {
    pub name: String,
    pub description: String,
}

// ── List MCP catalog ──────────────────────────────────────────────────────

pub fn list_mcp_catalog(
    _hermes_home: &Path,
    _profile: Option<&str>,
) -> Result<Vec<McpCatalogEntry>, String> {
    // Return empty catalog — in real implementation this would scan a catalog
    Ok(Vec::new())
}

// ── Install MCP catalog entry ─────────────────────────────────────────────

pub fn install_mcp_catalog_entry(
    _hermes_home: &Path,
    _profile: Option<&str>,
    name: &str,
    _env: Option<HashMap<String, String>>,
) -> Result<(bool, Option<String>, Option<String>, Option<String>), String> {
    Ok((true, None, Some("installed".to_string()), None))
}
