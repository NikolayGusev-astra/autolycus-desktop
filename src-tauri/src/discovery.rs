// src-tauri/src/discovery.rs
// Local instance discovery: find hermes/autolycus/steersman installations,
// determine version, gateway status, active profile, and the agent home dir
// so the desktop app can adopt an existing environment instead of creating
// a fresh one.
//
// Cross-platform: works on Linux, macOS, and Windows. Platform differences
// (venv layout, process listing, binary lookup) are isolated behind small
// helpers so the discovery logic above them stays uniform.

use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde::{Deserialize, Serialize};

// ── DetectedInstance ───────────────────────────────────────────────────────

/// Deserialize is required so the type can be passed as an argument to a
/// Tauri command (the frontend sends it as JSON over IPC).
#[derive(Debug, Clone, Serialize, Deserialize)]
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

// ── Platform helpers ───────────────────────────────────────────────────────

/// The interpreter binary name on the current platform.
fn python_bin() -> &'static str {
    if cfg!(windows) {
        "python.exe"
    } else {
        "python"
    }
}

/// The directory inside a venv that holds the interpreter on this platform.
fn venv_bin_dir() -> &'static str {
    if cfg!(windows) {
        "Scripts"
    } else {
        "bin"
    }
}

/// Build a venv interpreter path: `<venv_root>/{bin|Scripts}/{python|python.exe}`.
fn venv_interpreter(venv_root: &Path) -> PathBuf {
    venv_root.join(venv_bin_dir()).join(python_bin())
}

/// The agent binary name (without extension) used for pip-style installs.
const AGENT_BINS: &[&str] = &["hermes", "steersman", "autolycus"];

/// On Windows, agent bins get an `.exe`; pip/shims on unix have no suffix.
fn agent_bin(name: &str) -> String {
    if cfg!(windows) {
        format!("{}.exe", name)
    } else {
        name.to_string()
    }
}

/// Windows: set CREATE_NO_WINDOW on a Command so probing python/where/tasklist
/// doesn't flash a console window. Without this, discovery probes dozens of
/// interpreters (3 version calls each) → dozens of flashing windows at launch.
fn no_window(cmd: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    {
        let _ = cmd;
    }
}

/// Resolve a plain command name to its full path via the OS lookup tool
/// (`which` on unix, `where` on Windows). Returns the first match.
fn lookup_on_path(cmd: &str) -> Option<PathBuf> {
    let (tool, flag) = if cfg!(windows) {
        ("where", "")
    } else {
        ("which", "")
    };
    let mut builder = Command::new(tool);
    no_window(&mut builder);
    if !flag.is_empty() {
        builder.arg(flag);
    }
    builder.arg(cmd);
    let out = builder.output().ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .next()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
}

// ── Version detection ──────────────────────────────────────────────────────

/// Run `{python_path} -m hermes_cli.main --version` and parse the output.
/// Falls back to `{python_path} --version` if hermes_cli is not available.
pub fn get_instance_version(python_path: &str) -> String {
    // Try hermes CLI first
    let mut c1 = Command::new(python_path);
    no_window(&mut c1);
    let output = c1.args(["-m", "hermes_cli.main", "--version"]).output();

    if let Ok(out) = output {
        if out.status.success() {
            let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !stdout.is_empty() {
                return stdout;
            }
        }
    }

    // Try simpler hermes --version
    let mut c2 = Command::new(python_path);
    no_window(&mut c2);
    let output = c2.args(["-m", "hermes", "--version"]).output();

    if let Ok(out) = output {
        if out.status.success() {
            let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !stdout.is_empty() {
                return stdout;
            }
        }
    }

    // Fallback: get Python version itself
    let mut c3 = Command::new(python_path);
    no_window(&mut c3);
    let output = c3.arg("--version").output();

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
/// Checks common gateway ports (8642-8645) and, as a fallback, scans the
/// process list (ps aux on unix, tasklist on Windows).
pub fn check_gateway_status(python_path: &str) -> (bool, Option<u16>) {
    // Check common gateway ports (works identically on every platform).
    let common_ports: [u16; 4] = [8642, 8643, 8644, 8645];
    for &port in &common_ports {
        let addr = format!("127.0.0.1:{}", port);
        if let Ok(parsed) = addr.parse() {
            if TcpStream::connect_timeout(&parsed, Duration::from_millis(200)).is_ok() {
                return (true, Some(port));
            }
        }
    }

    // Fallback: scan the process list for a hermes gateway matching this python.
    if cfg!(unix) {
        if let Ok(out) = Command::new("ps").args(["aux"]).output() {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let python_bin_name = Path::new(python_path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            for line in stdout.lines() {
                if line.contains(&python_bin_name)
                    && line.contains("hermes")
                    && line.contains("gateway")
                {
                    return (true, Some(8642));
                }
            }
        }
    } else if cfg!(windows) {
        // `tasklist` lists image names; match the interpreter + a hermes
        // python module run (the command line isn't in tasklist by default,
        // so this is a coarse signal — the port check above is authoritative).
        let python_exe = Path::new(python_path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if !python_exe.is_empty() {
            let mut tc = Command::new("tasklist");
            no_window(&mut tc);
            if let Ok(out) = tc
                .args([
                    "/FI",
                    &format!("IMAGENAME eq {}", python_exe),
                    "/FO",
                    "CSV",
                    "/NH",
                ])
                .output()
            {
                let stdout = String::from_utf8_lossy(&out.stdout);
                if !stdout.trim().is_empty() && !stdout.contains("INFO: No tasks") {
                    // Coarse: interpreter is running. Combined with the port
                    // probe failing, we can't confirm a gateway here, so keep
                    // (false, None) — the port check is the source of truth.
                    let _ = stdout;
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

    let mut candidates: Vec<(PathBuf, &str)> = Vec::new();

    // ── Conventional venv installs (platform-aware venv layout) ──
    for (sub, kind) in [
        ("steersman", "steersman"),
        ("steersman/venv", "steersman"),
        (".steersman", "steersman"),
        (".steersman/venv", "steersman"),
        (".hermes", "hermes"),
        (".hermes/venv", "hermes"),
        (".hermes/hermes-agent", "hermes-agent"),
        (".hermes/hermes-agent/venv", "hermes-agent"),
        (".autolycus", "autolycus"),
        (".autolycus/venv", "autolycus"),
        ("autolycus", "autolycus"),
    ] {
        let py = venv_interpreter(&home.join(sub));
        candidates.push((py, kind));
    }

    // ── /opt project installs (linux only) ──
    if cfg!(target_os = "linux") {
        for sub in ["hermes", "steersman", "autolycus"] {
            let kind = match sub {
                "autolycus" => "autolycus",
                "steersman" => "steersman",
                _ => "hermes",
            };
            candidates.push((venv_interpreter(Path::new(&format!("/opt/{}", sub))), kind));
        }
    }

    // ── Application bundles / common macOS install locations ──
    if cfg!(target_os = "macos") {
        // Homebrew Cellar venvs, /Applications bundles, ~/Library
        for sub in [
            "Library/Штурман",
            "Library/Steersman",
            "Library/Hermes",
            "Library/Autolycus",
        ] {
            let py = venv_interpreter(&home.join(sub));
            candidates.push((py, "hermes"));
        }
        // Homebrew python with the agent package
        for p in ["/opt/homebrew/bin/python3", "/usr/local/bin/python3"] {
            candidates.push((PathBuf::from(p), "system"));
        }
    }

    // ── Windows install locations ──
    if cfg!(windows) {
        // %APPDATA% and %LOCALAPPDATA% installs
        let appdata = dirs::data_dir(); // %APPDATA% on Windows via dirs
        let localappdata = dirs::data_local_dir(); // %LOCALAPPDATA%
        for base in [appdata, localappdata] {
            if let Some(base) = base {
                for (sub, kind) in [
                    // Conventional flat installs.
                    ("Steersman", "steersman"),
                    ("Hermes", "hermes"),
                    ("Autolycus", "autolycus"),
                    // Real-world Hermes layout: <base>\hermes\hermes-agent\venv
                    // (uv-managed checkout). The venv interpreter lives at
                    // ...\hermes-agent\venv\Scripts\python.exe — match it here so
                    // detection finds an actual installation instead of nothing.
                    ("hermes\\hermes-agent\\venv", "hermes"),
                    ("hermes\\hermes-agent", "hermes"),
                    ("Hermes\\hermes-agent\\venv", "hermes"),
                    ("Steersman\\venv", "steersman"),
                    ("Autolycus\\venv", "autolycus"),
                ] {
                    let py = venv_interpreter(&base.join(sub));
                    candidates.push((py, kind));
                }
            }
        }
        // ~/.hermes and ~/.steersman checkout layouts (venv inside the checkout)
        for (sub, kind) in [
            (".hermes\\hermes-agent\\venv", "hermes"),
            (".steersman\\venv", "steersman"),
            (".autolycus\\venv", "autolycus"),
        ] {
            let py = venv_interpreter(&home.join(sub));
            candidates.push((py, kind));
        }
        // Program Files installs
        for pf in [
            PathBuf::from("C:\\Program Files\\Steersman\\venv"),
            PathBuf::from("C:\\Program Files\\Hermes\\hermes-agent\\venv"),
            PathBuf::from("C:\\Program Files\\Autolycus\\venv"),
        ] {
            candidates.push((venv_interpreter(&pf), "hermes"));
        }
    }

    // ── pip-installed agent binaries (resolved to a real interpreter) ──
    for bin in AGENT_BINS {
        let local_bin = home
            .join(if cfg!(windows) {
                "AppData\\Local\\Programs"
            } else {
                ".local/bin"
            })
            .join(agent_bin(bin));
        if local_bin.exists() {
            candidates.push((local_bin, bin));
        }
        // And anywhere on PATH
        if let Some(found) = lookup_on_path(bin) {
            candidates.push((found, bin));
        }
    }

    // ── System python fallbacks ──
    if cfg!(windows) {
        candidates.push((PathBuf::from("python.exe"), "system"));
    } else {
        candidates.push((PathBuf::from("/usr/local/bin/python3"), "system"));
        candidates.push((PathBuf::from("/usr/bin/python3"), "system"));
    }

    // ── Project-local venvs (cross-platform: scan common project roots) ──
    for parent in &[
        home.join("hermes"),
        home.join("autolycus"),
        home.join("projects"),
        home.join("src"),
        home.join("Documents"), // common on macOS/Windows
    ] {
        if let Ok(entries) = std::fs::read_dir(parent) {
            for entry in entries.flatten() {
                let p = entry.path();
                let venv_py = venv_interpreter(&p);
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

        // pip-style agent bins are launchers, not interpreters; resolve the
        // underlying python so version/gateway probes make sense.
        let probe_path = resolve_interpreter(&path);

        let version = get_instance_version(&probe_path.to_string_lossy());
        let (gateway_running, gateway_port) = check_gateway_status(&probe_path.to_string_lossy());
        let active_profile = detect_active_profile(&probe_path.to_string_lossy());
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

    // If nothing found, fall back to a system python on PATH.
    if results.is_empty() {
        if let Some(sys_py) = lookup_on_path(if cfg!(windows) { "python" } else { "python3" }) {
            let path_str = sys_py.to_string_lossy().to_string();
            let version = get_instance_version(&path_str);
            let (gateway_running, gateway_port) = check_gateway_status(&path_str);
            let (home_dir, label) = infer_environment(&sys_py, "system");
            results.push(DetectedInstance {
                path: path_str,
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

    results
}

/// If `path` is a pip-style agent launcher (hermes/steersman/autolycus),
/// resolve the underlying python interpreter it was installed by, so that
/// version/gateway probes target a real interpreter. Falls back to `path`.
fn resolve_interpreter(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let stem = name.trim_end_matches(".exe");
    if AGENT_BINS.contains(&stem) {
        // pip-installed agents are accompanied by a python in the same env.
        // Look for a sibling python/python.exe in the same dir, else on PATH.
        if let Some(dir) = path.parent() {
            let sibling = dir.join(python_bin());
            if sibling.exists() {
                return sibling;
            }
            // venv: ../bin/python or ..\Scripts\python.exe
            if dir
                .file_name()
                .map(|n| n == venv_bin_dir())
                .unwrap_or(false)
            {
                if let Some(venv_root) = dir.parent() {
                    let py = venv_interpreter(venv_root);
                    if py.exists() {
                        return py;
                    }
                }
            }
        }
        if let Some(found) = lookup_on_path(if cfg!(windows) { "python" } else { "python3" }) {
            return found;
        }
    }
    path.to_path_buf()
}

/// Infer the agent kind from a project directory by its characteristic files.
fn infer_kind_from_dir(dir: &Path) -> &'static str {
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

/// A directory looks like an agent home if it holds a config / env file. The
/// characteristic subdirs (skills/profiles/...) alone are NOT enough: a source
/// checkout (e.g. `hermes-agent/`) also contains `skills/` as part of the repo,
/// so matching on it alone would wrongly treat the checkout as the home dir.
/// Requiring a config file resolves the real home one level up.
fn looks_like_agent_home(dir: &Path) -> bool {
    // config files indicate the home root; state.db is the real data dir (it's
    // where sessions/memory live and what resolve_hermes_home keys on). Include
    // it so discovery doesn't infer a venv/checkout dir that lacks the data.
    for marker in ["config.yaml", "config.yml", ".env", "auth.json", "state.db"] {
        if dir.join(marker).exists() {
            return true;
        }
    }
    false
}

// ── Active profile detection ──────────────────────────────────────────────

/// Try to determine the active profile for a given python installation.
/// Checks HERMES_PROFILE or looks at the config files in ~/.hermes/
fn detect_active_profile(_python_path: &str) -> String {
    if let Ok(profile) = std::env::var("HERMES_PROFILE") {
        if !profile.is_empty() {
            return profile;
        }
    }

    let home = dirs::home_dir();
    for base in &[
        home.as_ref().map(|h| h.join(".hermes")),
        home.as_ref().map(|h| h.join(".steersman")),
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

// ── Public discovery helpers ───────────────────────────────────────────────

/// Find a usable local agent interpreter by running the full discovery scan
/// and returning the interpreter path of the first detected instance.
///
/// This is the single source of truth for "where is the local Hermes Python?"
/// — `gateway.rs` (gateway start) and `test_connection` use it instead of
/// keeping their own hard-coded candidate lists (ADR-001).
pub fn find_local_interpreter() -> Option<PathBuf> {
    let instances = detect_local_instances();
    instances.first().map(|inst| PathBuf::from(&inst.path))
}

/// The best instance to auto-connect to: prefer one whose gateway is already
/// running, else the first instance that has a known home_dir. Returns None if
/// no instance is detected at all. Used by the auto-adopt startup flow
/// (ADR-003).
pub fn primary_instance() -> Option<DetectedInstance> {
    let instances = detect_local_instances();
    if instances.is_empty() {
        return None;
    }
    // Prefer a running gateway (zero-touch: just adopt it).
    if let Some(running) = instances.iter().find(|i| i.gateway_running) {
        return Some(running.clone());
    }
    // Else prefer one with a known home dir.
    if let Some(with_home) = instances.iter().find(|i| i.home_dir.is_some()) {
        return Some(with_home.clone());
    }
    instances.first().cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Integration check that runs against the real machine: discovery should
    /// surface any locally-installed Hermes/Steersman interpreter. Run with
    ///     cargo test --lib detect_real_machine -- --nocapture --ignored
    #[test]
    #[ignore]
    fn detect_real_machine() {
        let instances = detect_local_instances();
        println!("discovery found {} instance(s):", instances.len());
        for inst in &instances {
            println!(
                "  - kind={} ver={} gw_running={} port={:?} home={:?} label={:?}\n    path={}",
                inst.instance_type,
                inst.version,
                inst.gateway_running,
                inst.gateway_port,
                inst.home_dir,
                inst.label,
                inst.path
            );
        }
        let primary = primary_instance();
        println!("primary_instance: {:?}", primary.is_some());
        let interp = find_local_interpreter();
        println!("find_local_interpreter: {:?}", interp);
    }
}
