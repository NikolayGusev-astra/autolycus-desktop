// src-tauri/src/config_health.rs
// Configuration health audit — ported from fathah/hermes-desktop src/main/config-health.ts

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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
        Self { errors: 0, warnings: 0, infos: 0 }
    }
}

pub fn run_health_check(hermes_home: &PathBuf, profile: Option<&str>) -> Result<HealthReport, String> {
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

    // Check 4: Python virtual environment
    let venv_path = hermes_home.join("venv");
    if !venv_path.exists() {
        issues.push(HealthIssue {
            code: "VENV_MISSING".to_string(),
            severity: "error".to_string(),
            message: "Python virtual environment not found".to_string(),
            fix: Some("Run: python3 -m venv venv && source venv/bin/activate && pip install -r requirements.txt".to_string()),
        });
    }

    // Check 5: SQLite session database
    let db_path = hermes_home.join("sessions.db");
    if !db_path.exists() {
        issues.push(HealthIssue {
            code: "DB_MISSING".to_string(),
            severity: "warning".to_string(),
            message: "Sessions database not found — will be created automatically".to_string(),
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

/// Attempt to auto-fix a specific config health issue
pub fn auto_fix_issue(hermes_home: &PathBuf, code: &str, profile: Option<&str>) -> Result<String, String> {
    match code {
        "ENV_MISSING" => {
            let env_path = hermes_home.join(".env");
            std::fs::write(env_path, "# Autolycus environment variables\n")
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
            let default_config = "\
# Autolycus Configuration
model:
  provider: auto
  name: auto
gateway:
  port: 8000
  host: 127.0.0.1
";
            std::fs::write(&config_path, default_config)
                .map_err(|e| format!("Failed to create config: {}", e))?;
            Ok(format!("Created default config at {}", config_path.display()))
        }
        "HERMES_HOME_MISSING" => {
            std::fs::create_dir_all(hermes_home)
                .map_err(|e| format!("Failed to create hermes home: {}", e))?;
            Ok(format!("Created directory: {}", hermes_home.display()))
        }
        _ => Err(format!("No auto-fix available for issue: {}", code)),
    }
}
