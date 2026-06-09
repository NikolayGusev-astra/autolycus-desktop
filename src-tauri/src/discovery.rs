// src-tauri/src/discovery.rs
// Local instance discovery: find hermes/autolycus installations,
// determine version, gateway status, and active profile.
// v0.5.0 — Phase 3: Autodetect instances

use std::net::TcpStream;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use serde::Serialize;

// ── DetectedInstance ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct DetectedInstance {
    pub path: String,
    pub instance_type: String, // "autolycus", "hermes", "hermes-agent", "system"
    pub version: String,
    pub gateway_running: bool,
    pub gateway_port: Option<u16>,
    pub active_profile: String,
}

// ── Version detection ──────────────────────────────────────────────────────

/// Run `{python_path} -m hermes_cli.main --version` and parse the output.
/// Falls back to `{python_path} --version` if hermes_cli is not available.
pub fn get_instance_version(python_path: &str) -> String {
    // Try hermes CLI first
    let output = Command::new(python_path)
        .args(["-m", "hermes_cli.main", "--version"])
        .output();

    if let Ok(out) = output {
        if out.status.success() {
            let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !stdout.is_empty() {
                return stdout;
            }
        }
    }

    // Try simpler hermes --version
    let output = Command::new(python_path)
        .args(["-m", "hermes", "--version"])
        .output();

    if let Ok(out) = output {
        if out.status.success() {
            let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !stdout.is_empty() {
                return stdout;
            }
        }
    }

    // Fallback: get Python version itself
    let output = Command::new(python_path)
        .arg("--version")
        .output();

    if let Ok(out) = output {
        if out.status.success() {
            let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !stdout.is_empty() {
                return stdout;
            }
        }
    }

    "unknown".to_string()
}

// ── Gateway status ─────────────────────────────────────────────────────────

/// Check if a hermes gateway process is running for this python installation.
/// Checks common gateway ports (8642-8645) and also scans `ps aux`.
pub fn check_gateway_status(python_path: &str) -> (bool, Option<u16>) {
    // Check common gateway ports
    let common_ports: [u16; 4] = [8642, 8643, 8644, 8645];
    for &port in &common_ports {
        let addr = format!("127.0.0.1:{}", port);
        if let Ok(parsed) = addr.parse() {
            if TcpStream::connect_timeout(&parsed, Duration::from_millis(200)).is_ok() {
                return (true, Some(port));
            }
        }
    }

    // Fallback: check ps aux for hermes process matching this python
    if cfg!(unix) {
        let output = Command::new("ps").args(["aux"]).output();
        if let Ok(out) = output {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let python_bin = std::path::Path::new(python_path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            for line in stdout.lines() {
                if line.contains(&python_bin)
                    && line.contains("hermes")
                    && line.contains("gateway")
                {
                    return (true, Some(8642));
                }
            }
        }
    }

    (false, None)
}

// ── Detect local instances ────────────────────────────────────────────────

/// Scan for all local hermes/autolycus installations and return detailed info.
pub fn detect_local_instances() -> Vec<DetectedInstance> {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return Vec::new(),
    };

    // Candidate python paths (matching find_hermes_python logic)
    let candidates: Vec<(PathBuf, &str)> = vec![
        (home.join("autolycus/venv/bin/python"), "autolycus"),
        (home.join("autolycus/venv/bin/python3"), "autolycus"),
        (home.join(".autolycus/venv/bin/python"), "autolycus"),
        (home.join(".hermes/venv/bin/python"), "hermes"),
        (
            home.join(".hermes/hermes-agent/venv/bin/python"),
            "hermes-agent",
        ),
        (PathBuf::from("/usr/local/bin/python3"), "system"),
    ];

    let mut results = Vec::new();
    let mut seen_paths = std::collections::HashSet::new();

    for (path, instance_type) in candidates {
        if !path.exists() {
            continue;
        }

        let path_str = path.to_string_lossy().to_string();

        // Skip duplicates (same path already added)
        if seen_paths.contains(&path_str) {
            continue;
        }
        seen_paths.insert(path_str.clone());

        let version = get_instance_version(&path_str);
        let (gateway_running, gateway_port) = check_gateway_status(&path_str);
        let active_profile = detect_active_profile(&path_str);

        results.push(DetectedInstance {
            path: path_str,
            instance_type: instance_type.to_string(),
            version,
            gateway_running,
            gateway_port,
            active_profile,
        });
    }

    // If nothing found, try `which python3` as system fallback
    if results.is_empty() {
        if let Ok(output) = Command::new("which").arg("python3").output() {
            if output.status.success() {
                let which_path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !which_path.is_empty() {
                    let version = get_instance_version(&which_path);
                    let (gateway_running, gateway_port) =
                        check_gateway_status(&which_path);
                    results.push(DetectedInstance {
                        path: which_path,
                        instance_type: "system".to_string(),
                        version,
                        gateway_running,
                        gateway_port,
                        active_profile: "default".to_string(),
                    });
                }
            }
        }
    }

    results
}

// ── Active profile detection ──────────────────────────────────────────────

/// Try to determine the active profile for a given python installation.
/// Checks HERMES_PROFILE or looks at the config files in ~/.hermes/
fn detect_active_profile(_python_path: &str) -> String {
    // Try via environment
    if let Ok(profile) = std::env::var("HERMES_PROFILE") {
        if !profile.is_empty() {
            return profile;
        }
    }

    // Check for active_profile file in common locations
    let home = dirs::home_dir();
    for base in &[
        home.as_ref().map(|h| h.join(".hermes")),
        home.as_ref().map(|h| h.join(".autolycus")),
    ] {
        if let Some(base_path) = base {
            let active_file = base_path.join("active_profile");
            if active_file.exists() {
                if let Ok(content) = std::fs::read_to_string(&active_file) {
                    let profile = content.trim().to_string();
                    if !profile.is_empty() {
                        return profile;
                    }
                }
            }
        }
    }

    "default".to_string()
}