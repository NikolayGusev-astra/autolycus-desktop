// src-tauri/src/config_health.rs
// Configuration health audit — ported from fathah/hermes-desktop src/main/config-health.ts

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthIssue {
    pub code: String,
    pub severity: String, // "error" | "warning" | "info"
    pub message: String,
    pub fix: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthReport {
    pub profile: Option<String>,
    pub issues: Vec<HealthIssue>,
    pub summary: HealthSummary,
    pub ran_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthSummary {
    pub errors: usize,
    pub warnings: usize,
    pub infos: usize,
}

impl Default for HealthSummary {
    fn default() -> Self {
        Self {
            errors: 0,
            warnings: 0,
            infos: 0,
        }
    }
}

pub fn run_health_check(
    hermes_home: &PathBuf,
    profile: Option<&str>,
) -> Result<HealthReport, String> {
    let mut issues = Vec::new();

    // Check 1: Hermes home directory exists
    if !hermes_home.exists() {
        issues.push(HealthIssue {
            code: "HERMES_HOME_MISSING".to_string(),
            severity: "error".to_string(),
            message: format!("Hermes home directory not found: {}", hermes_home.display()),
            fix: Some("Create the directory or set HERMES_HOME environment variable".to_string()),
        });
    }

    // Check 2: config.yaml exists
    let config_path = if let Some(ref p) = profile {
        hermes_home.join("profiles").join(p).join("config.yaml")
    } else {
        hermes_home.join("config.yaml")
    };

    if !config_path.exists() {
        issues.push(HealthIssue {
            code: "CONFIG_MISSING".to_string(),
            severity: "warning".to_string(),
            message: format!("Config file not found: {}", config_path.display()),
            fix: Some("Run initial setup or copy a template config.yaml".to_string()),
        });
    }

    // Check 3: .env file exists
    let env_path = hermes_home.join(".env");
    if !env_path.exists() {
        issues.push(HealthIssue {
            code: "ENV_MISSING".to_string(),
            severity: "info".to_string(),
            message: ".env file not found — API keys may not be configured".to_string(),
            fix: Some("Create .env file with API keys".to_string()),
        });
    }

    // Check 4: Python virtual environment (ADR-002: real path is hermes-agent/venv)
    let venv_path = hermes_home.join("hermes-agent").join("venv");
    if !venv_path.exists() {
        // Also check the legacy venv/ path (older installs).
        let legacy_venv = hermes_home.join("venv");
        if !legacy_venv.exists() {
            issues.push(HealthIssue {
                code: "VENV_MISSING".to_string(),
                severity: "error".to_string(),
                message: "Python virtual environment not found".to_string(),
                fix: Some(
                    "Run: hermes setup (installs hermes-agent/venv automatically)".to_string(),
                ),
            });
        }
    }

    // Check 5: SQLite session database (ADR-002: real name is state.db, not sessions.db)
    let db_path = hermes_home.join("state.db");
    if !db_path.exists() {
        issues.push(HealthIssue {
            code: "DB_MISSING".to_string(),
            severity: "warning".to_string(),
            message: "State database not found — will be created automatically on first chat"
                .to_string(),
            fix: None,
        });
    }

    // Calculate summary
    let mut summary = HealthSummary::default();
    for issue in &issues {
        match issue.severity.as_str() {
            "error" => summary.errors += 1,
            "warning" => summary.warnings += 1,
            "info" => summary.infos += 1,
            _ => {}
        }
    }

    Ok(HealthReport {
        profile: profile.map(String::from),
        issues,
        summary,
        ran_at: chrono::Utc::now().timestamp(),
    })
}

/// Default config.yaml template written by the CONFIG_MISSING auto-fix.
///
/// NOTE (ADR-004): there is intentionally NO `platforms.api_server.port` here.
/// The real backend is `hermes serve --port 0`, which has the OS assign a free
/// port — hardcoding 8642 (the legacy ADR-002 HTTP API Server port) pointed
/// users at an endpoint that is never listened on.
const DEFAULT_CONFIG_TEMPLATE: &str = "\
# Штурман Configuration
# The backend is launched via `hermes serve` (ADR-004); its port is assigned
# by the OS, so no platforms.api_server.port is hardcoded here.
model:
  default: ''
  provider: ''
";

/// Attempt to auto-fix a specific config health issue
pub fn auto_fix_issue(
    hermes_home: &PathBuf,
    code: &str,
    profile: Option<&str>,
) -> Result<String, String> {
    match code {
        "ENV_MISSING" => {
            let env_path = hermes_home.join(".env");
            std::fs::write(env_path, "# Штурман environment variables\n")
                .map_err(|e| format!("Failed to create .env: {}", e))?;
            Ok("Created .env file".to_string())
        }
        "CONFIG_MISSING" => {
            let config_path = if let Some(p) = profile {
                hermes_home.join("profiles").join(p).join("config.yaml")
            } else {
                hermes_home.join("config.yaml")
            };
            if let Some(parent) = config_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create config directory: {}", e))?;
            }
            std::fs::write(&config_path, DEFAULT_CONFIG_TEMPLATE)
                .map_err(|e| format!("Failed to create config: {}", e))?;
            Ok(format!(
                "Created default config at {}",
                config_path.display()
            ))
        }
        "HERMES_HOME_MISSING" => {
            std::fs::create_dir_all(hermes_home)
                .map_err(|e| format!("Failed to create hermes home: {}", e))?;
            Ok(format!("Created directory: {}", hermes_home.display()))
        }
        _ => Err(format!("No auto-fix available for issue: {}", code)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ADR-004: the default config must NOT hardcode the legacy 8642 port.
    // The real backend is `hermes serve --port 0` (OS-assigned); 8642 pointed
    // at an endpoint that is never listened on and confused users.

    #[test]
    fn default_config_has_no_legacy_8642_port() {
        assert!(
            !DEFAULT_CONFIG_TEMPLATE.contains("8642"),
            "default config template must not reference the legacy 8642 port"
        );
    }

    #[test]
    fn default_config_has_no_api_server_block() {
        // platforms.api_server was an ADR-002 concept; hermes serve does not
        // read it. Check for the actual YAML key (indented, colon) rather than
        // the bare word, which legitimately appears in this explanatory comment.
        assert!(
            !DEFAULT_CONFIG_TEMPLATE.contains("\n  api_server:")
                && !DEFAULT_CONFIG_TEMPLATE.contains("\napi_server:"),
            "default config template must not define an api_server YAML block"
        );
    }

    #[test]
    fn default_config_has_model_block() {
        // The one thing the backend DOES read from config.yaml is model.default.
        assert!(DEFAULT_CONFIG_TEMPLATE.contains("model:"));
    }
}
