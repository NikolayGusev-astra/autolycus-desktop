// src-tauri/src/gateway.rs
// Gateway lifecycle management: start, stop, restart, health polling
// Ported from fathah/hermes-desktop src/main/hermes.ts (gateway part)

use std::collections::HashMap;
use std::io::BufRead;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::AppHandle;

use crate::config::{self, expand_tilde, profile_home, read_env, ModelConfig};

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
    pub hermes_home: Arc<Mutex<Option<PathBuf>>>,
}

impl GatewayState {
    pub fn new() -> Self {
        Self {
            processes: Arc::new(Mutex::new(HashMap::new())),
            api_server_available: Arc::new(Mutex::new(None)),
            hermes_home: Arc::new(Mutex::new(None)),
        }
    }
}

static GATEWAY_PORT_BASE: AtomicU64 = AtomicU64::new(8642);

// ── Hermes paths ──────────────────────────────────────────────────────────

pub fn find_hermes_python() -> Result<(PathBuf, String), String> {
    let home = dirs::home_dir().ok_or("Cannot determine home directory")?;

    let candidates: Vec<(PathBuf, &str)> = vec![
        (home.join("autolycus/venv/bin/python"), "autolycus"),
        (home.join("autolycus/venv/bin/python3"), "autolycus"),
        (home.join(".autolycus/venv/bin/python"), "autolycus"),
        (home.join(".hermes/venv/bin/python"), "hermes"),
        (home.join(".hermes/hermes-agent/venv/bin/python"), "hermes-agent"),
        (PathBuf::from("/usr/local/bin/python3"), "system"),
    ];

    for (path, instance) in candidates {
        if path.exists() {
            return Ok((path, instance.to_string()));
        }
    }

    Err("No Python instance found. Install autolycus or hermes.".to_string())
}

pub fn find_hermes_repo(python_path: &PathBuf) -> Option<PathBuf> {
    // Go up from venv/bin/python to find the repo root
    python_path
        .parent() // bin/
        .and_then(|p| p.parent()) // venv/
        .and_then(|p| p.parent()) // repo root
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
        let processes = state.processes.lock().unwrap();
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

    // Build command: python -m hermes gateway
    let mut cmd = Command::new(&python_path);
    cmd.arg("-m")
        .arg("hermes")
        .arg("gateway")
        .arg("--port")
        .arg(port.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    // Set environment
    if let Some(repo) = &repo_path {
        cmd.env("HERMES_PYTHON_SRC_ROOT", repo);
    }
    cmd.env("HERMES_HOME", hermes_home);
    cmd.env("PYTHONUNBUFFERED", "1");

    // Profile-specific env
    let profile_home_path = profile_home(hermes_home, profile);
    if profile_home_path != *hermes_home {
        cmd.env("HERMES_PROFILE_HOME", &profile_home_path);
    }

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

    // Read stdout for READY signal or error
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    // Wait for gateway to be ready (up to 10 seconds)
    let start = Instant::now();
    let mut ready = false;
    let reader = std::io::BufReader::new(stdout);
    let stderr_reader = std::io::BufReader::new(stderr);

    // Read stderr in background
    let profile_key_clone = profile_key.clone();
    thread::spawn(move || {
        for line in stderr_reader.lines() {
            if let Ok(text) = line {
                eprintln!("[gateway:{}] {}", profile_key_clone, text);
            }
        }
    });

    // Check stdout for ready signal
    for line in reader.lines() {
        if let Ok(text) = line {
            if text.contains("READY") || text.contains("running") || text.contains("started") {
                ready = true;
                break;
            }
            if start.elapsed() > Duration::from_secs(10) {
                break;
            }
        }
    }

    if !ready {
        // Gateway might still be starting, give it more time
        thread::sleep(Duration::from_secs(2));
    }

    // Store process
    {
        let mut processes = state.processes.lock().unwrap();
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

    // Mark API as available
    {
        let mut api = state.api_server_available.lock().unwrap();
        *api = Some(true);
    }

    GatewayStartResult {
        success: true,
        running: true,
        already_running: Some(false),
        error: None,
        log_path: Some(format!("{}/logs/gateway.log", hermes_home.display())),
    }
}

// ── Gateway stop ──────────────────────────────────────────────────────────

pub fn stop_gateway(state: &GatewayState, profile: Option<&str>) -> Result<(), String> {
    let profile_key = profile.unwrap_or("default").to_string();

    let mut processes = state.processes.lock().unwrap();
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

    let mut processes = state.processes.lock().unwrap();
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
    let processes = state.processes.lock().unwrap();
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
    stop_gateway(state, profile);
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
