// src-tauri/src/discovery.rs
// Local instance discovery: find hermes/autolycus/steersman installations,
// determine version, gateway status, active profile, and the agent home dir
// so the desktop app can adopt an existing environment instead of creating
// a fresh one.

use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde::Serialize;

// ── DetectedInstance ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct DetectedInstance {
    pub path: String,
    pub instance_type: String, // "steersman", "hermes", "autolycus", "system", ...
    pub version: String,
    pub gateway_running: bool,
    pub gateway_port: Option<u16>,
    pub active_profile: String,
    /// The agent home directory (HERMES_HOME) this instance uses, if known.
    /// When set, the desktop app can connect to this instance and reuse its
    /// profiles/skills/secrets instead of bootstrapping a new environment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub home_dir: Option<String>,
    /// A short, human-friendly label for the instance (e.g. project name).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
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

/// Scan for all local hermes/autolycus/steersman installations and return
/// detailed info, including the agent home dir so the desktop app can adopt
/// an existing environment.
pub fn detect_local_instances() -> Vec<DetectedInstance> {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return Vec::new(),
    };

    // Candidate python interpreters + their inferred kind. Ordered so the
    // most specific (project venvs) come first; dedup by path later.
    let mut candidates: Vec<(PathBuf, &str)> = vec![
        (home.join("steersman/venv/bin/python"), "steersman"),
        (home.join("steersman/venv/bin/python3"), "steersman"),
        (home.join(".steersman/venv/bin/python"), "steersman"),
        (home.join(".hermes/venv/bin/python"), "hermes"),
        (
            home.join(".hermes/hermes-agent/venv/bin/python"),
            "hermes-agent",
        ),
        (home.join(".autolycus/venv/bin/python"), "autolycus"),
        (home.join("autolycus/venv/bin/python"), "autolycus"),
        // pip-installed agent binaries — resolved later to a real interpreter
        (home.join(".local/bin/hermes"), "hermes"),
        (home.join(".local/bin/steersman"), "steersman"),
        (home.join(".local/bin/autolycus"), "autolycus"),
        // /opt project installs
        (PathBuf::from("/opt/hermes/venv/bin/python"), "hermes"),
        (PathBuf::from("/opt/steersman/venv/bin/python"), "steersman"),
        (PathBuf::from("/opt/autolycus/venv/bin/python"), "autolycus"),
        (PathBuf::from("/usr/local/bin/python3"), "system"),
        (PathBuf::from("/usr/bin/python3"), "system"),
    ];

    // Add project-local venvs that contain an agent package. These are the
    // richest signal: a developer cloned hermes/autolycus and built a venv.
    for parent in &[
        home.join("hermes"),
        home.join("autolycus"),
        home.join("projects"),
        home.join("src"),
    ] {
        if let Ok(entries) = std::fs::read_dir(parent) {
            for entry in entries.flatten() {
                let p = entry.path();
                let venv_py = p.join("venv/bin/python");
                if venv_py.exists() {
                    let kind = infer_kind_from_dir(&p);
                    candidates.push((venv_py, kind));
                }
            }
        }
    }

    let mut results = Vec::new();
    let mut seen_paths = std::collections::HashSet::new();

    for (path, instance_type) in candidates {
        if !path.exists() {
            continue;
        }

        let path_str = path.to_string_lossy().to_string();
        if seen_paths.contains(&path_str) {
            continue;
        }
        seen_paths.insert(path_str.clone());

        let version = get_instance_version(&path_str);
        let (gateway_running, gateway_port) = check_gateway_status(&path_str);
        let active_profile = detect_active_profile(&path_str);
        let (home_dir, label) = infer_environment(&path, instance_type);

        results.push(DetectedInstance {
            path: path_str,
            instance_type: instance_type.to_string(),
            version,
            gateway_running,
            gateway_port,
            active_profile,
            home_dir,
            label,
        });
    }

    // If nothing found, try `which python3` as system fallback.
    if results.is_empty() {
        if let Ok(output) = Command::new("which").arg("python3").output() {
            if output.status.success() {
                let which_path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !which_path.is_empty() {
                    let version = get_instance_version(&which_path);
                    let (gateway_running, gateway_port) = check_gateway_status(&which_path);
                    let (home_dir, label) = infer_environment(Path::new(&which_path), "system");
                    results.push(DetectedInstance {
                        path: which_path,
                        instance_type: "system".to_string(),
                        version,
                        gateway_running,
                        gateway_port,
                        active_profile: "default".to_string(),
                        home_dir,
                        label,
                    });
                }
            }
        }
    }

    results
}

/// Infer the agent kind from a project directory by its characteristic files.
fn infer_kind_from_dir(dir: &Path) -> &'static str {
    // pyproject/setup naming is the strongest signal.
    let stem = dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if stem.contains("autolycus") {
        return "autolycus";
    }
    if stem.contains("steersman") {
        return "steersman";
    }
    if stem.contains("hermes") {
        return "hermes";
    }
    // Fall back to package metadata if present.
    if dir.join("autolycus").exists() || dir.join("src/autolycus").exists() {
        return "autolycus";
    }
    if dir.join("steersman").exists() || dir.join("src/steersman").exists() {
        return "steersman";
    }
    "hermes"
}

/// Given an interpreter/binary path and its kind, infer the agent home
/// directory (HERMES_HOME) and a human label, by walking characteristic
/// markers: config.yaml, skills/, profiles/, .env next to the venv.
fn infer_environment(interp: &Path, kind: &str) -> (Option<String>, Option<String>) {
    // Walk up from the interpreter to find a directory that "looks like" an
    // agent home: contains config.yaml/.env OR skills/ OR profiles/.
    let mut cursor = interp.parent();
    let mut found_home: Option<PathBuf> = None;
    for _ in 0..5 {
        if let Some(dir) = cursor {
            if looks_like_agent_home(dir) {
                found_home = Some(dir.to_path_buf());
                break;
            }
            cursor = dir.parent();
        } else {
            break;
        }
    }

    // Fallback to the conventional ~/.hermes / ~/.autolycus / ~/.steersman.
    let home_dir = found_home.or_else(|| {
        dirs::home_dir().and_then(|h| {
            for sub in [".hermes", ".autolycus", ".steersman"] {
                let p = h.join(sub);
                if p.is_dir() && looks_like_agent_home(&p) {
                    return Some(p);
                }
            }
            None
        })
    });

    let home_str = home_dir.as_ref().map(|p| p.to_string_lossy().to_string());
    let label = home_dir
        .as_ref()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .map(|n| match kind {
            "autolycus" => format!("Autolycus — {}", n),
            "steersman" => format!("Штурман — {}", n),
            "hermes" | "hermes-agent" => format!("Hermes — {}", n),
            _ => n.to_string(),
        });

    (home_str, label)
}

/// A directory looks like an agent home if it holds a config / env file or
/// one of the characteristic subdirs the agents create.
fn looks_like_agent_home(dir: &Path) -> bool {
    for marker in ["config.yaml", "config.yml", ".env", "auth.json"] {
        if dir.join(marker).exists() {
            return true;
        }
    }
    for sub in ["skills", "profiles", "sessions", "memory"] {
        if dir.join(sub).is_dir() {
            return true;
        }
    }
    false
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
        home.as_ref().map(|h| h.join(".steersman")),
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