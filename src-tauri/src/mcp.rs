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
    /// Extra top-level fields on the server entry that we don't model explicitly
    /// (timeout, connect_timeout, headers, etc.) — shown read-only in the UI so
    /// the user sees the full picture and we never silently drop them on write.
    #[serde(default)]
    pub raw_fields: HashMap<String, String>,
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

// NOTE (T2): the legacy McpConfigFile wrapper ({servers: ...} JSON) was removed
// when storage migrated from servers.json to config.yaml's mcp_servers: block.
// McpServerConfig below is retained — it is the in-memory representation used by
// the yaml round-trip helpers.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub server_type: String,
    pub url: Option<String>,
    pub command: Option<String>,
    pub args: Option<Vec<String>>,
    pub env: Option<HashMap<String, String>>,
    pub auth: Option<String>,
    pub enabled: Option<bool>,
    #[serde(default)]
    pub raw_fields: HashMap<String, String>,
}

// ── File path ─────────────────────────────────────────────────────────────
//
// ADR-002 §Config.yaml schema + T2 (P-AUDIT #11): MCP servers are stored in
// config.yaml `mcp_servers:` block, NOT in a separate servers.json. The Hermes
// backend reads ONLY config.yaml, so servers in servers.json were invisible
// to the agent. This module now reads/writes the `mcp_servers:` hash in
// config.yaml via yaml-rust2 round-trip.

fn config_yaml_path(hermes_home: &Path, profile: Option<&str>) -> std::path::PathBuf {
    match profile {
        Some(p) if p != "default" && !p.is_empty() => {
            hermes_home.join("profiles").join(p).join("config.yaml")
        }
        _ => hermes_home.join("config.yaml"),
    }
}

/// Read the `mcp_servers:` block from config.yaml as a map of name → McpServerConfig.
pub(crate) fn read_mcp_servers_yaml(hermes_home: &Path, profile: Option<&str>) -> HashMap<String, McpServerConfig> {
    let json = match crate::config::read_config_yaml(hermes_home, profile) {
        Ok(v) => v,
        Err(_) => return HashMap::new(),
    };
    // read_config_yaml returns the full config; extract mcp_servers.
    let mcp_block = json.get("mcp_servers").cloned().unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
    let mut out = HashMap::new();
    if let serde_json::Value::Object(map) = mcp_block {
        for (name, val) in map {
            let server_type = if val.get("url").is_some() { "http" } else { "stdio" };
            // Collect extra fields we don't model explicitly (timeout,
            // connect_timeout, headers, etc.) so the UI can show them and we
            // never silently drop them.
            let known_keys = ["server_type", "url", "command", "args", "env", "auth", "enabled"];
            let mut raw_fields = HashMap::new();
            if let Some(obj) = val.as_object() {
                for (k, v) in obj {
                    if !known_keys.contains(&k.as_str()) {
                        let display = if let Some(s) = v.as_str() {
                            s.to_string()
                        } else {
                            v.to_string()
                        };
                        raw_fields.insert(k.clone(), display);
                    }
                }
            }
            let cfg = McpServerConfig {
                server_type: val
                    .get("server_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or(server_type)
                    .to_string(),
                url: val.get("url").and_then(|v| v.as_str()).map(|s| s.to_string()),
                command: val.get("command").and_then(|v| v.as_str()).map(|s| s.to_string()),
                args: val.get("args").and_then(|v| v.as_array()).map(|a| {
                    a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect()
                }),
                env: val.get("env").and_then(|v| v.as_object()).map(|o| {
                    o.iter()
                        .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                        .collect()
                }),
                auth: val.get("auth").and_then(|v| v.as_str()).map(|s| s.to_string()),
                enabled: val.get("enabled").and_then(|v| v.as_bool()),
                raw_fields,
            };
            out.insert(name, cfg);
        }
    }
    out
}

// ── List MCP servers ──────────────────────────────────────────────────────

pub fn list_mcp_servers(hermes_home: &Path, profile: Option<&str>) -> Vec<McpServer> {
    let servers = read_mcp_servers_yaml(hermes_home, profile);
    servers
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
            raw_fields: server.raw_fields,
        })
        .collect()
}

// ── Add MCP server ────────────────────────────────────────────────────────

pub fn add_mcp_server(
    hermes_home: &Path,
    profile: Option<&str>,
    input: &McpServerInput,
) -> Result<McpServer, String> {
    // Check if the server already exists (read is safe, doesn't modify).
    let existing = read_mcp_servers_yaml(hermes_home, profile);
    if existing.contains_key(&input.name) {
        return Err(format!("Server '{}' already exists", input.name));
    }

    // Line-based insertion: build the new server block as YAML lines and
    // inject it right after the `mcp_servers:` header (or create the header).
    // This preserves all other content (comments, other servers, their extra
    // fields like headers/timeout that McpServerConfig doesn't model).
    insert_server_block_linebased(hermes_home, profile, input)?;

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
        raw_fields: HashMap::new(),
    })
}

/// Insert a new MCP server block into config.yaml using line-based editing.
/// Finds the `mcp_servers:` header and inserts the new server's lines right
/// after it (before any existing server entries), preserving everything else.
fn insert_server_block_linebased(
    hermes_home: &Path,
    profile: Option<&str>,
    input: &McpServerInput,
) -> Result<(), String> {
    let path = config_yaml_path(hermes_home, profile);
    let content = std::fs::read_to_string(&path).unwrap_or_default();
    let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();

    // Build the new server block lines (2-space indent for server name, 4 for fields).
    let mut block: Vec<String> = Vec::new();
    block.push(format!("  {}:", input.name));
    if let Some(url) = &input.url {
        if !url.is_empty() {
            block.push(format!("    url: \"{}\"", url));
        }
    }
    if let Some(cmd) = &input.command {
        if !cmd.is_empty() {
            block.push(format!("    command: \"{}\"", cmd));
        }
    }
    if let Some(args) = &input.args {
        if !args.is_empty() {
            let args_yaml = args
                .iter()
                .map(|a| format!("\"{}\"", a))
                .collect::<Vec<_>>()
                .join(", ");
            block.push(format!("    args: [{}]", args_yaml));
        }
    }
    if let Some(env) = &input.env {
        if !env.is_empty() {
            block.push("    env:".to_string());
            for (k, v) in env {
                block.push(format!("      {}: \"{}\"", k, v));
            }
        }
    }
    if let Some(auth) = &input.auth {
        if !auth.is_empty() {
            block.push(format!("    auth: \"{}\"", auth));
        }
    }

    // Find the `mcp_servers:` header line.
    let mcp_idx = lines.iter().position(|l| l.trim_start() == "mcp_servers:" || l.trim_start().starts_with("mcp_servers:"));

    match mcp_idx {
        Some(idx) => {
            // Insert right after the header. The new server goes first among
            // the entries (order doesn't matter semantically in YAML).
            for (offset, line) in block.iter().enumerate() {
                lines.insert(idx + 1 + offset, line.clone());
            }
        }
        None => {
            // No mcp_servers block yet — create one at the end.
            if !lines.is_empty() && !lines.last().map(|l| l.is_empty()).unwrap_or(true) {
                lines.push(String::new());
            }
            lines.push("mcp_servers:".to_string());
            for line in &block {
                lines.push(line.clone());
            }
        }
    }

    let out = lines.join("\n") + "\n";
    std::fs::write(&path, out).map_err(|e| format!("Write config.yaml error: {}", e))?;
    Ok(())
}

// ── Remove MCP server ─────────────────────────────────────────────────────

pub fn remove_mcp_server(
    hermes_home: &Path,
    profile: Option<&str>,
    name: &str,
) -> Result<(), String> {
    let path = config_yaml_path(hermes_home, profile);
    if !path.exists() {
        return Err("config.yaml not found".to_string());
    }

    // Line-based removal: find the `  <name>:` line under mcp_servers and
    // delete it plus all its indented children, preserving everything else.
    let content = std::fs::read_to_string(&path);
    let content = content.map_err(|e| format!("Read config.yaml error: {}", e))?;
    let lines: Vec<&str> = content.lines().collect();

    // Find the server block start: `  <name>:` at 2-space indent inside mcp_servers.
    let server_idx = lines.iter().position(|l| l.trim_start() == format!("{}:", name) && l.starts_with("  ") && !l.starts_with("   "));

    let server_idx = match server_idx {
        Some(i) => i,
        None => return Err(format!("Server '{}' not found", name)),
    };

    // Find where the block ends: next line at indent <= 2 that isn't empty.
    let mut end_idx = lines.len();
    for (i, line) in lines.iter().enumerate().skip(server_idx + 1) {
        if !line.trim().is_empty() {
            let indent = line.len() - line.trim_start().len();
            if indent <= 2 {
                end_idx = i;
                break;
            }
        }
    }

    // Rebuild without the removed block.
    let mut out_lines: Vec<&str> = lines[..server_idx].to_vec();
    out_lines.extend_from_slice(&lines[end_idx..]);
    let out = out_lines.join("\n") + "\n";
    std::fs::write(&path, out).map_err(|e| format!("Write config.yaml error: {}", e))?;
    Ok(())
}

// ── Set MCP server enabled ────────────────────────────────────────────────

pub fn set_mcp_server_enabled(
    hermes_home: &Path,
    profile: Option<&str>,
    name: &str,
    enabled: bool,
) -> Result<(), String> {
    // Line-based: set the `enabled` field on the specific server block.
    // This preserves all other fields, comments, and unknown keys.
    let block = format!("mcp_servers.{}", name);
    crate::config::set_yaml_block_scalars(hermes_home, profile, &block, &[("enabled", if enabled { "true" } else { "false" })])
}

// ── Update MCP server env ─────────────────────────────────────────────────

/// Update environment variables for an MCP server in config.yaml using
/// line-based editing. Only the provided keys are written/updated; all other
/// env keys and server fields are preserved. This is the generic mechanism
/// the UI uses to configure credentials for ANY MCP server.
pub fn update_mcp_server_env(
    hermes_home: &Path,
    profile: Option<&str>,
    server_name: &str,
    env: &HashMap<String, String>,
) -> Result<(), String> {
    // Verify the server exists.
    let servers = read_mcp_servers_yaml(hermes_home, profile);
    if !servers.contains_key(server_name) {
        return Err(format!("Server '{}' not found", server_name));
    }

    let path = config_yaml_path(hermes_home, profile);
    update_server_env_linebased(&path, server_name, env)
}

/// Line-based editor for the `env:` block of a specific MCP server.
///
/// Walks the YAML lines to find:
///   1. The `mcp_servers:` header
///   2. The `  <server_name>:` entry (2-space indent)
///   3. The `    env:` sub-block (4-space indent)
///
/// Then replaces/adds keys within that env block. Everything else in the file
/// (comments, other servers, unknown fields like timeout/headers) is untouched.
fn update_server_env_linebased(
    path: &Path,
    server_name: &str,
    env: &HashMap<String, String>,
) -> Result<(), String> {
    let content = std::fs::read_to_string(path).unwrap_or_default();
    let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();

    // Phase 1: locate the `  <server_name>:` line inside mcp_servers.
    let server_idx = lines.iter().position(|l| l.trim_start() == format!("{}:", server_name) && l.starts_with("  ") && !l.starts_with("   "));

    let server_idx = match server_idx {
        Some(i) => i,
        None => {
            // Server block doesn't exist — can't add env to nothing.
            return Err(format!("Server '{}' not found in config.yaml", server_name));
        }
    };

    // Phase 2: find the `    env:` line within the server block (before the
    // next sibling server at indent <= 2).
    let mut env_idx: Option<usize> = None;
    let mut env_indent: usize = 4;
    for i in (server_idx + 1)..lines.len() {
        let line = &lines[i];
        if line.trim().is_empty() {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        if indent <= 2 {
            break; // left the server block
        }
        if line.trim_start() == "env:" || line.trim_start().starts_with("env:") {
            env_idx = Some(i);
            env_indent = indent;
            break;
        }
    }

    // Phase 3: replace existing keys / track what we've set.
    let mut set_keys: std::collections::HashSet<String> = std::collections::HashSet::new();

    if let Some(ei) = env_idx {
        // Walk the env block children (indent > env_indent) and replace matching keys.
        for i in (ei + 1)..lines.len() {
            let line = &lines[i];
            if line.trim().is_empty() {
                continue;
            }
            let indent = line.len() - line.trim_start().len();
            if indent <= env_indent {
                break; // left the env block
            }
            let trimmed = line.trim_start().to_string();
            for (k, v) in env {
                if trimmed.starts_with(&format!("{}:", k)) {
                    lines[i] = format!("{}{}: \"{}\"", " ".repeat(env_indent + 2), k, v);
                    set_keys.insert(k.clone());
                    break;
                }
            }
        }
    }

    // Phase 4: append missing keys.
    let missing: Vec<(&String, &String)> = env.iter().filter(|(k, _)| !set_keys.contains(*k)).collect();
    if !missing.is_empty() {
        if env_idx.is_none() {
            // No env: line — insert one after the server header.
            let insert_at = server_idx + 1;
            lines.insert(insert_at, format!("{}env:", " ".repeat(4)));
            env_idx = Some(insert_at);
        }
        // Append missing keys right after the env: line (or after existing keys).
        let mut insert_at = env_idx.unwrap() + 1;
        // Skip past existing env children to append at the end of the block.
        while insert_at < lines.len() {
            let line = &lines[insert_at];
            if line.trim().is_empty() {
                break;
            }
            let indent = line.len() - line.trim_start().len();
            if indent <= 4 {
                break;
            }
            insert_at += 1;
        }
        for (k, v) in &missing {
            lines.insert(insert_at, format!("      {}: \"{}\"", k, v));
            insert_at += 1;
        }
    }

    let out = lines.join("\n") + "\n";
    std::fs::write(path, out).map_err(|e| format!("Write config.yaml error: {}", e))?;
    Ok(())
}

// ── Test MCP server ───────────────────────────────────────────────────────

pub fn test_mcp_server(
    _hermes_home: &Path,
    _profile: Option<&str>,
    _name: &str,
) -> Result<(bool, Option<String>, Option<Vec<McpToolInfo>>), String> {
    // Not yet implemented — real probe would:
    // 1. spawn/connect to MCP server
    // 2. send initialize request
    // 3. send notifications/initialized
    // 4. send tools/list
    // 5. controlled shutdown with timeout
    // Using McpStdioClient as the transport layer.
    Err("NotImplemented: MCP server test probe not yet implemented".to_string())
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
    // (local registry, remote index, or embedded manifest) and return available
    // MCP servers with metadata.
    // For now, return empty to avoid fake data.
    Ok(Vec::new())
}

// ── Install MCP catalog entry ─────────────────────────────────────────────

pub fn install_mcp_catalog_entry(
    _hermes_home: &Path,
    _profile: Option<&str>,
    _name: &str,
    _env: Option<HashMap<String, String>>,
) -> Result<(bool, Option<String>, Option<String>, Option<String>), String> {
    // Not yet implemented — real implementation would:
    // 1. Fetch catalog entry metadata
    // 2. Install dependencies (npm, pip, binary)
    // 3. Write config.yaml mcp_servers entry with env
    // 4. Return installation status, command path, and any post-install instructions
    Err("NotImplemented: MCP catalog install not yet implemented".to_string())
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

    // ── Round-trip preservation tests ───────────────────────────────────────
    //
    // The line-based operations preserve everything: comments, unknown fields
    // (headers, timeout, connect_timeout), and other servers.

    /// A realistic config.yaml fixture with comments + unknown fields (headers,
    /// timeout, connect_timeout) that McpServerConfig doesn't model.
    const REALISTIC_FIXTURE: &str = "\
# Hermes Agent configuration
model:
  default: some-model

# MCP servers — managed by Steersman Desktop
mcp_servers:
  # Email connector (IMAP/SMTP via himalaya)
  email:
    command: \"python\"
    args: [\"server.py\"]
    env:
      EMAIL_ADDRESS: \"user@example.com\"
      EMAIL_PASSWORD: \"secret123\"
      NO_PROXY: \"*\"
    timeout: 60          # seconds
    connect_timeout: 30
  lodestone:
    url: \"https://lodestone.example.com/mcp/\"
    headers:
      Authorization: \"Bearer lst_token_here\"
    env:
      NO_PROXY: \"*\"
    timeout: 60
    connect_timeout: 30
";

    #[test]
    fn update_env_preserves_unknown_fields_and_comments() {
        let dir = tempdir();
        fs::write(dir.join("config.yaml"), REALISTIC_FIXTURE).unwrap();

        // Update one env var on the email server.
        let new_env = HashMap::from([
            ("EMAIL_PASSWORD".to_string(), "newpass456".to_string()),
        ]);
        update_mcp_server_env(&dir, None, "email", &new_env).unwrap();

        let after = fs::read_to_string(dir.join("config.yaml")).unwrap();

        // The updated value must be present.
        assert!(after.contains("newpass456"), "new env value missing: {}", after);
        // The old value must be gone.
        assert!(!after.contains("secret123"), "old value not replaced: {}", after);
        // Comments must survive (the whole point of line-based editing).
        assert!(after.contains("# Hermes Agent configuration"), "comment dropped: {}", after);
        assert!(after.contains("# Email connector"), "server comment dropped: {}", after);
        // Unknown fields (timeout, connect_timeout) must survive.
        assert!(after.contains("timeout: 60"), "timeout field dropped: {}", after);
        assert!(after.contains("connect_timeout: 30"), "connect_timeout dropped: {}", after);
        // The OTHER server (lodestone) must be fully intact.
        assert!(after.contains("lodestone"), "other server dropped: {}", after);
        assert!(after.contains("Bearer lst_token_here"), "lodestone header dropped: {}", after);
        // Other env vars on email must survive (only EMAIL_PASSWORD changed).
        assert!(after.contains("EMAIL_ADDRESS: \"user@example.com\""), "sibling env var dropped: {}", after);
        assert!(after.contains("NO_PROXY"), "NO_PROXY dropped: {}", after);
    }

    #[test]
    fn set_enabled_preserves_unknown_fields() {
        let dir = tempdir();
        fs::write(dir.join("config.yaml"), REALISTIC_FIXTURE).unwrap();

        set_mcp_server_enabled(&dir, None, "email", false).unwrap();

        let after = fs::read_to_string(dir.join("config.yaml")).unwrap();

        // enabled: false must be present on the email server.
        assert!(after.contains("enabled: false"), "enabled not set: {}", after);
        // Unknown fields must survive.
        assert!(after.contains("timeout: 60"), "timeout dropped after set_enabled: {}", after);
        assert!(after.contains("connect_timeout: 30"), "connect_timeout dropped: {}", after);
        // Lodestone must be untouched.
        assert!(after.contains("Bearer lst_token_here"), "lodestone clobbered: {}", after);
        // Comments survive.
        assert!(after.contains("# Email connector"), "comment dropped: {}", after);
    }

    #[test]
    fn remove_preserves_other_servers_and_comments() {
        let dir = tempdir();
        fs::write(dir.join("config.yaml"), REALISTIC_FIXTURE).unwrap();

        remove_mcp_server(&dir, None, "email").unwrap();

        let after = fs::read_to_string(dir.join("config.yaml")).unwrap();

        // Email server must be gone.
        assert!(!after.contains("EMAIL_PASSWORD"), "removed server env still present: {}", after);
        assert!(!after.contains("command: \"python\""), "removed server command still present: {}", after);
        // Lodestone must be fully intact.
        assert!(after.contains("lodestone"), "other server dropped: {}", after);
        assert!(after.contains("Bearer lst_token_here"), "lodestone header dropped: {}", after);
        assert!(after.contains("timeout: 60"), "lodestone timeout dropped: {}", after);
        // Comments survive.
        assert!(after.contains("# Hermes Agent configuration"), "comment dropped: {}", after);
    }

    #[test]
    fn add_preserves_existing_servers_and_comments() {
        let dir = tempdir();
        fs::write(dir.join("config.yaml"), REALISTIC_FIXTURE).unwrap();

        let input = McpServerInput {
            name: "newserver".to_string(),
            server_type: "stdio".to_string(),
            url: None,
            command: Some("newcmd".to_string()),
            args: None,
            env: Some(HashMap::from([("API_KEY".to_string(), "xyz".to_string())])),
            auth: None,
        };
        add_mcp_server(&dir, None, &input).unwrap();

        let after = fs::read_to_string(dir.join("config.yaml")).unwrap();

        // New server must be present.
        assert!(after.contains("newserver"), "new server not added: {}", after);
        assert!(after.contains("newcmd"), "new command not added: {}", after);
        assert!(after.contains("API_KEY: \"xyz\""), "new env not added: {}", after);
        // Existing servers must be fully intact.
        assert!(after.contains("email"), "existing server dropped: {}", after);
        assert!(after.contains("lodestone"), "existing server dropped: {}", after);
        assert!(after.contains("Bearer lst_token_here"), "lodestone header dropped: {}", after);
        // Comments survive.
        assert!(after.contains("# Hermes Agent configuration"), "comment dropped: {}", after);
        assert!(after.contains("# Email connector"), "comment dropped: {}", after);
    }

    #[test]
    fn list_returns_raw_fields_for_display() {
        let dir = tempdir();
        fs::write(dir.join("config.yaml"), REALISTIC_FIXTURE).unwrap();

        let listed = list_mcp_servers(&dir, None);
        let email = listed.iter().find(|s| s.name == "email").unwrap();

        // raw_fields must contain timeout and connect_timeout.
        assert!(email.raw_fields.contains_key("timeout"), "timeout not in raw_fields: {:?}", email.raw_fields);
        assert!(email.raw_fields.contains_key("connect_timeout"), "connect_timeout not in raw_fields: {:?}", email.raw_fields);
        assert_eq!(email.raw_fields.get("timeout").map(|s| s.as_str()), Some("60"));
    }

    #[test]
    fn update_env_returns_error_for_unknown_server() {
        let dir = tempdir();
        fs::write(dir.join("config.yaml"), REALISTIC_FIXTURE).unwrap();

        let env = HashMap::from([("KEY".to_string(), "val".to_string())]);
        let result = update_mcp_server_env(&dir, None, "nonexistent", &env);
        assert!(result.is_err(), "should error for unknown server");
        assert!(result.unwrap_err().contains("not found"));
    }
}

