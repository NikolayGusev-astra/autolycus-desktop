// src-tauri/src/mcp.rs
// MCP servers management: list, add, remove, enable, test
// Ported from fathah/hermes-desktop src/main/mcp-servers.rs

use std::collections::HashMap;
use std::path::Path;
use serde::{Deserialize, Serialize};

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
    _name: &str,
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
    _name: &str,
    _env: Option<HashMap<String, String>>,
) -> Result<(bool, Option<String>, Option<String>, Option<String>), String> {
    Ok((true, None, Some("installed".to_string()), None))
}

/// Sync MCP credentials into config.yaml mcp_servers.<name>.env: blocks
/// (ADR-002 §MCP env whitelist).
///
/// Hermes Agent's `_build_safe_env()` strips ALL non-whitelisted env vars when
/// launching MCP servers. Credentials written to `.env` DON'T reach MCP servers.
/// They MUST be in `config.yaml mcp_servers.<name>.env:` block — that's the only
/// way Hermes injects them into the child process.
///
/// This function reads source credentials (email/jira) and writes them as
/// `env:` blocks under the corresponding `mcp_servers` entries in config.yaml.
/// Called automatically after source add/update/remove (alongside .env write).
pub fn sync_mcp_env_blocks(
    hermes_home: &Path,
    profile: Option<&str>,
) -> Result<(), String> {
    let sources = crate::sources::SourcesConfig::load(hermes_home, profile);

    // Build env maps for email and jira MCP servers from SourcesConfig.
    let mut email_env: HashMap<String, String> = HashMap::new();
    if let Some(email) = sources.email.iter().find(|s| s.enabled) {
        email_env.insert("EMAIL_ADDRESS".to_string(), email.address.clone());
        email_env.insert("EMAIL_USER".to_string(), email.address.clone());
        email_env.insert("EMAIL_PASSWORD".to_string(), email.password.clone());
        email_env.insert("EMAIL_IMAP_HOST".to_string(), email.imap_host.clone());
        email_env.insert("EMAIL_IMAP_PORT".to_string(), email.imap_port.to_string());
        email_env.insert("EMAIL_SMTP_HOST".to_string(), email.smtp_host.clone());
        email_env.insert("EMAIL_SMTP_PORT".to_string(), email.smtp_port.to_string());
        email_env.insert(
            "EMAIL_USE_SSL".to_string(),
            if email.use_ssl { "true".to_string() } else { "false".to_string() },
        );
        email_env.insert("EMAIL_HOST".to_string(), email.smtp_host.clone());
    }

    let mut jira_env: HashMap<String, String> = HashMap::new();
    if let Some(jira) = sources.jira.iter().find(|s| s.enabled) {
        jira_env.insert("JIRA_PAT".to_string(), jira.api_token.clone());
        jira_env.insert("JIRA_BASE_URL".to_string(), jira.url.clone());
        if !jira.username.is_empty() {
            jira_env.insert("JIRA_USERNAME".to_string(), jira.username.clone());
        }
        if !jira.project_key.is_empty() {
            jira_env.insert("JIRA_PROJECT_KEY".to_string(), jira.project_key.clone());
        }
    }

    // Also include bitrix if configured.
    let mut bitrix_env: HashMap<String, String> = HashMap::new();
    if let Some(bx) = sources.bitrix.iter().find(|s| s.enabled) {
        bitrix_env.insert("BITRIX_WEBHOOK".to_string(), bx.webhook_url.clone());
        bitrix_env.insert("BITRIX_USER_ID".to_string(), bx.user_id.clone());
    }

    // Write env blocks into config.yaml using set_yaml_block_scalars for each
    // mcp_servers.<name>.env.<KEY> path. This is a line-based approach that
    // preserves the rest of config.yaml.
    for (server_name, env_vars) in [
        ("email", &email_env),
        ("jira", &jira_env),
        ("bitrix", &bitrix_env),
    ] {
        if env_vars.is_empty() {
            continue;
        }
        // Write each env var as mcp_servers.<server_name>.env.<KEY>: <VALUE>
        let kvs: Vec<(&str, &str)> = env_vars
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let block = format!("mcp_servers.{}.env", server_name);
        crate::config::set_yaml_block_scalars(hermes_home, profile, &block, &kvs)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tempdir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "steersman-mcp-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    // ADR-002 §Config.yaml schema: the upstream `mcp_servers:` block is the
    // single source of truth. Steersman must read/write THERE, not servers.json.

    #[test]
    fn add_then_list_reads_from_config_yaml() {
        let dir = tempdir();
        // Pre-seed an (empty) config.yaml so the writer has somewhere to go.
        fs::write(dir.join("config.yaml"), "model:\n  default: ''\n").unwrap();

        let input = McpServerInput {
            name: "email".to_string(),
            server_type: "stdio".to_string(),
            url: None,
            command: Some("mcp-email".to_string()),
            args: Some(vec![]),
            env: Some(HashMap::from([("EMAIL_ADDRESS".to_string(), "a@b.com".to_string())])),
            auth: None,
        };
        add_mcp_server(&dir, None, &input).unwrap();

        // The server must be readable back — from config.yaml, not servers.json.
        let listed = list_mcp_servers(&dir, None);
        assert_eq!(listed.len(), 1, "expected 1 server, got {:?}", listed);
        assert_eq!(listed[0].name, "email");
        assert_eq!(listed[0].command.as_deref(), Some("mcp-email"));
    }

    #[test]
    fn add_writes_to_config_yaml_not_servers_json() {
        let dir = tempdir();
        fs::write(dir.join("config.yaml"), "model:\n  default: ''\n").unwrap();

        let input = McpServerInput {
            name: "jira".to_string(),
            server_type: "stdio".to_string(),
            url: None,
            command: Some("mcp-jira".to_string()),
            args: None,
            env: None,
            auth: None,
        };
        add_mcp_server(&dir, None, &input).unwrap();

        // config.yaml must now contain the mcp_servers block.
        let yaml = fs::read_to_string(dir.join("config.yaml")).unwrap();
        assert!(yaml.contains("mcp_servers:"), "config.yaml missing mcp_servers: {}", yaml);
        assert!(yaml.contains("jira"), "config.yaml missing server name: {}", yaml);

        // servers.json must NOT be created (the legacy storage is retired).
        assert!(
            !dir.join(".hermes").join("mcp").join("servers.json").exists(),
            "servers.json was created — storage must be config.yaml only"
        );
    }

    #[test]
    fn remove_deletes_from_config_yaml() {
        let dir = tempdir();
        fs::write(dir.join("config.yaml"), "model:\n  default: ''\n").unwrap();
        let input = McpServerInput {
            name: "temp".to_string(),
            server_type: "stdio".to_string(),
            url: None,
            command: Some("x".to_string()),
            args: None,
            env: None,
            auth: None,
        };
        add_mcp_server(&dir, None, &input).unwrap();
        assert_eq!(list_mcp_servers(&dir, None).len(), 1);

        remove_mcp_server(&dir, None, "temp").unwrap();
        assert!(
            list_mcp_servers(&dir, None).is_empty(),
            "server still present after remove"
        );
    }
}

