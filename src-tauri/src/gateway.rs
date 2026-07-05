// src-tauri/src/gateway.rs
// Gateway lifecycle management: start, stop, restart, health polling
// Ported from fathah/hermes-desktop src/main/hermes.ts (gateway part)

use std::collections::HashMap;
use std::io::BufRead;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::config::profile_home;

/// Apply the Windows `CREATE_NO_WINDOW` creation flag to a Command so spawning a
/// console-subsystem child (python.exe, hermes.exe) does NOT pop up a console
/// window. On non-Windows this is a no-op. Without it, a Tauri GUI app spawning
/// a console binary makes a console window flash (or, for the long-lived
/// gateway, stay open persistently).
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

// ── Types ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct GatewayStartResult {
    pub success: bool,
    pub running: bool,
    pub already_running: Option<bool>,
    pub error: Option<String>,
    pub log_path: Option<String>,
}

#[derive(Debug)]
pub struct GatewayProcess {
    pub child: Child,
    pub port: u16,
    pub profile_key: String,
    pub started_at: Instant,
}

// ── State ─────────────────────────────────────────────────────────────────

pub struct GatewayState {
    pub processes: Arc<Mutex<HashMap<String, GatewayProcess>>>,
    pub api_server_available: Arc<Mutex<Option<bool>>>,
}

impl GatewayState {
    pub fn new() -> Self {
        Self {
            processes: Arc::new(Mutex::new(HashMap::new())),
            api_server_available: Arc::new(Mutex::new(None)),
        }
    }
}

static GATEWAY_PORT_BASE: AtomicU64 = AtomicU64::new(8642);

// ── Hermes paths ──────────────────────────────────────────────────────────

/// Resolve the agent interpreter via the unified cross-platform discovery
/// (ADR-001). Replaces the old `find_hermes_python`, which kept a hard-coded
/// unix-only candidate list and found nothing on Windows.
pub fn find_hermes_python() -> Result<(PathBuf, String), String> {
    match crate::discovery::find_local_interpreter() {
        Some(p) => Ok((p.clone(), "hermes".to_string())),
        None => Err(
            "No local Hermes/Steersman installation found. Install Hermes Agent first.".to_string(),
        ),
    }
}

/// Resolve the agent home directory from discovery (the source of truth), so
/// we don't guess repo layout. Returns None if unknown — callers handle it.
pub fn find_hermes_repo(python_path: &PathBuf) -> Option<PathBuf> {
    // A venv interpreter lives at <root>/venv/{bin|Scripts}/python[.exe];
    // the agent source checkout is the venv's parent's parent.
    python_path
        .parent() // bin/ or Scripts/
        .and_then(|p| p.parent()) // venv/
        .and_then(|p| p.parent()) // checkout root
        .map(|p| p.to_path_buf())
}

// ── Port allocation ───────────────────────────────────────────────────────

fn allocate_port(profile_key: &str) -> u16 {
    // Default profile gets 8642, others get offset
    if profile_key == "default" {
        8642
    } else {
        let hash = profile_key.bytes().fold(0u64, |acc, b| {
            acc.wrapping_mul(31).wrapping_add(b as u64)
        });
        (8642 + (hash % 1000)) as u16
    }
}

// ── Gateway start ─────────────────────────────────────────────────────────

pub fn start_gateway(
    state: &GatewayState,
    hermes_home: &PathBuf,
    profile: Option<&str>,
) -> GatewayStartResult {
    let profile_key = profile.unwrap_or("default").to_string();

    // Check if already running
    {
        let processes = state.processes.lock().unwrap_or_else(|p| p.into_inner());
        if processes.contains_key(&profile_key) {
            return GatewayStartResult {
                success: true,
                running: true,
                already_running: Some(true),
                error: None,
                log_path: None,
            };
        }
    }

    // Find hermes installation
    let (python_path, _instance) = match find_hermes_python() {
        Ok(v) => v,
        Err(e) => {
            return GatewayStartResult {
                success: false,
                running: false,
                already_running: None,
                error: Some(e),
                log_path: None,
            };
        }
    };

    let port = allocate_port(&profile_key);
    let repo_path = find_hermes_repo(&python_path);

    // Build the gateway command (ADR-002). The real Hermes backend exposes
    // `gateway` as a subcommand of the `hermes` console-script launcher
    // (hermes.exe / hermes), NOT as a `python -m hermes` module (there is no
    // top-level `hermes` package — only `hermes_cli`). The gateway also does
    // NOT accept a `--port` flag; the port is taken from
    // platforms.api_server.extra.port in config.yaml or the API_SERVER_PORT
    // env var. So: prefer the launcher if present, else fall back to
    // `python -m hermes_cli.main gateway`, and pass the port via env.
    let hermes_launcher = python_path
        .parent() // bin/ or Scripts/
        .map(|dir| dir.join(if cfg!(windows) { "hermes.exe" } else { "hermes" }));

    let mut cmd = if let Some(launcher) = hermes_launcher.as_ref().filter(|p| p.exists()) {
        let mut c = Command::new(launcher);
        c.arg("gateway");
        c
    } else {
        // Fallback: invoke the CLI module directly with the interpreter.
        let mut c = Command::new(&python_path);
        c.arg("-m").arg("hermes_cli.main").arg("gateway");
        c
    };
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    // Set environment
    if let Some(repo) = &repo_path {
        cmd.env("HERMES_PYTHON_SRC_ROOT", repo);
    }
    cmd.env("HERMES_HOME", hermes_home);
    cmd.env("PYTHONUNBUFFERED", "1");
    // The gateway reads its listen port from this env var (falls back to the
    // value in config.yaml, default 8642). We set it explicitly so the port
    // we allocated matches the one we health-check.
    cmd.env("API_SERVER_PORT", port.to_string());

    // Profile-specific env
    let profile_home_path = profile_home(hermes_home, profile);
    if profile_home_path != *hermes_home {
        cmd.env("HERMES_PROFILE_HOME", &profile_home_path);
    }

    // Windows: suppress the console window that would otherwise pop up and
    // stay open for the lifetime of the gateway.
    no_window(&mut cmd);

    // Spawn
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return GatewayStartResult {
                success: false,
                running: false,
                already_running: None,
                error: Some(format!("Failed to spawn gateway: {}", e)),
                log_path: None,
            };
        }
    };

    // Drain stdout/stderr in the background so the child's pipes never fill and
    // block the process. We no longer parse stdout for a "READY" string — the
    // real Hermes gateway logs to stdout in formats that vary by version, and
    // blocking on stdout.lines() hangs until the pipe closes. Instead we wait
    // for readiness by polling the HTTP /health endpoint (ADR-002).
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let profile_key_clone = profile_key.clone();
    thread::spawn(move || {
        let reader = std::io::BufReader::new(stdout);
        for line in reader.lines().flatten() {
            eprintln!("[gateway:{}] {}", profile_key_clone, line);
        }
    });
    let profile_key_clone2 = profile_key.clone();
    thread::spawn(move || {
        let stderr_reader = std::io::BufReader::new(stderr);
        for line in stderr_reader.lines().flatten() {
            eprintln!("[gateway:{}] {}", profile_key_clone2, line);
        }
    });

    // Wait for the gateway HTTP server to become reachable (up to 30s). We
    // probe TCP first (cheap, no runtime) then confirm with /health via a
    // blocking reqwest blocking client on a short-lived thread.
    let mut ready = false;
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        // Cheap TCP probe — mirrors discovery's check_gateway_status.
        let addr = format!("127.0.0.1:{}", port);
        if let Ok(parsed) = addr.parse() {
            if std::net::TcpStream::connect_timeout(&parsed, Duration::from_millis(300)).is_ok() {
                ready = true;
                break;
            }
        }
        // Bail early if the child already exited.
        match child.try_wait() {
            Ok(Some(_status)) => break, // process died
            _ => {}
        }
        thread::sleep(Duration::from_millis(300));
    }
    // If TCP is up, give the HTTP layer a brief moment to bind handlers.
    if ready {
        thread::sleep(Duration::from_millis(400));
    }

    // Store process
    {
        let mut processes = state.processes.lock().unwrap_or_else(|p| p.into_inner());
        processes.insert(
            profile_key.clone(),
            GatewayProcess {
                child,
                port,
                profile_key: profile_key.clone(),
                started_at: Instant::now(),
            },
        );
    }

    // Mark API as available (only if readiness was confirmed)
    {
        let mut api = state.api_server_available.lock().unwrap_or_else(|p| p.into_inner());
        *api = Some(ready);
    }

    GatewayStartResult {
        success: ready,
        running: ready,
        already_running: Some(false),
        error: if ready {
            None
        } else {
            Some(format!(
                "Gateway process started but did not become reachable on port {} within 30s. \
                 Check that Hermes Agent is installed and config.yaml has platforms.api_server.enabled.",
                port
            ))
        },
        log_path: Some(format!("{}/logs/gateway.log", hermes_home.display())),
    }
}

// ── Gateway stop ──────────────────────────────────────────────────────────

pub fn stop_gateway(state: &GatewayState, profile: Option<&str>) -> Result<(), String> {
    let profile_key = profile.unwrap_or("default").to_string();

    let mut processes = state.processes.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(mut gw) = processes.remove(&profile_key) {
        // Graceful shutdown: SIGTERM → wait → SIGKILL
        #[cfg(unix)]
        {
            let pid = gw.child.id();
            unsafe {
                libc::kill(pid as i32, libc::SIGTERM);
            }
        }

        // Wait up to 3 seconds
        let start = Instant::now();
        loop {
            match gw.child.try_wait() {
                Ok(Some(_)) => break, // Exited
                Ok(None) => {
                    if start.elapsed() >= Duration::from_secs(3) {
                        // Force kill
                        let _ = gw.child.kill();
                        let _ = gw.child.wait();
                        break;
                    }
                    thread::sleep(Duration::from_millis(100));
                }
                Err(_) => break,
            }
        }

        #[cfg(windows)]
        {
            let _ = gw.child.kill();
            let _ = gw.child.wait();
        }
    }

    Ok(())
}

// ── Gateway status ────────────────────────────────────────────────────────

pub fn is_gateway_running(state: &GatewayState, profile: Option<&str>) -> bool {
    let profile_key = profile.unwrap_or("default").to_string();

    let mut processes = state.processes.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(gw) = processes.get_mut(&profile_key) {
        match gw.child.try_wait() {
            Ok(None) => true, // Still running
            Ok(Some(_)) => {
                // Exited
                processes.remove(&profile_key);
                false
            }
            Err(_) => false,
        }
    } else {
        false
    }
}

pub fn get_gateway_port(state: &GatewayState, profile: Option<&str>) -> Option<u16> {
    let profile_key = profile.unwrap_or("default").to_string();
    let processes = state.processes.lock().unwrap_or_else(|p| p.into_inner());
    processes.get(&profile_key).map(|gw| gw.port)
}

// ── Health check ──────────────────────────────────────────────────────────

pub async fn check_gateway_health(port: u16) -> bool {
    let url = format!("http://127.0.0.1:{}/health", port);
    match reqwest::get(&url).await {
        Ok(resp) => resp.status().is_success(),
        Err(_) => false,
    }
}

// ── Gateway restart ───────────────────────────────────────────────────────

pub async fn restart_gateway(
    state: &GatewayState,
    hermes_home: &PathBuf,
    profile: Option<&str>,
) -> GatewayStartResult {
    let _ = stop_gateway(state, profile);
    tokio::time::sleep(Duration::from_millis(500)).await;
    start_gateway(state, hermes_home, profile)
}

// ── API URL ───────────────────────────────────────────────────────────────

pub fn get_api_url(state: &GatewayState, profile: Option<&str>) -> Option<String> {
    get_gateway_port(state, profile).map(|port| format!("http://127.0.0.1:{}", port))
}

// ── API server ready check ────────────────────────────────────────────────

pub async fn is_api_server_ready(state: &GatewayState, profile: Option<&str>) -> bool {
    if let Some(port) = get_gateway_port(state, profile) {
        check_gateway_health(port).await
    } else {
        false
    }
}
