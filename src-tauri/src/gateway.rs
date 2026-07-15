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
    /// Dashboard session token (ADR-004 §2). Used to build the WS ?token= URL
    /// and also injected into the child env as HERMES_DASHBOARD_SESSION_TOKEN.
    pub session_token: String,
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

    let repo_path = find_hermes_repo(&python_path);

    // ── Build `hermes serve` command (ADR-004) ────────────────────────────
    // The real Hermes backend is launched via `hermes serve --host 127.0.0.1
    // --port 0`. With --port 0 the OS assigns a free port, printed to stdout
    // as HERMES_BACKEND_READY port=N. Auth is a session token (ADR-004 §2).
    let hermes_launcher = python_path
        .parent() // bin/ or Scripts/
        .map(|dir| dir.join(if cfg!(windows) { "hermes.exe" } else { "hermes" }));

    // Windows: prefer pythonw.exe (no console). Fallback: bare python + CREATE_NO_WINDOW.
    #[cfg(windows)]
    let python_for_gateway = {
        let pythonw = python_path.with_file_name("pythonw.exe");
        if pythonw.exists() { pythonw } else { python_path.clone() }
    };
    #[cfg(not(windows))]
    let python_for_gateway = python_path.clone();

    // serve_args mirrors the desktop invocation (build_serve_args, unit-tested).
    let serve_args = build_serve_args(profile);

    let mut cmd = if let Some(launcher) = hermes_launcher.as_ref().filter(|p| p.exists()) {
        let mut c = Command::new(launcher);
        for a in &serve_args { c.arg(a); }
        c
    } else {
        // Fallback: invoke the CLI module directly with the interpreter.
        let mut c = Command::new(&python_for_gateway);
        c.arg("-m").arg("hermes_cli.main");
        for a in &serve_args { c.arg(a); }
        c
    };

    // cwd: set to HERMES_REPO (ADR-003 #14 — still valid in ADR-004).
    if let Some(repo) = &repo_path {
        cmd.current_dir(repo);
    }

    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    // ── Environment (ADR-004 §2: session token, not API_SERVER_*) ─────────
    // Generate a random dashboard session token and inject it into the child
    // env. The same token is later used as the WS ?token= value.
    let session_token = generate_session_token();
    if let Some(repo) = &repo_path {
        cmd.env("HERMES_PYTHON_SRC_ROOT", repo);
    }
    cmd.env("HERMES_HOME", hermes_home);
    cmd.env("PYTHONUNBUFFERED", "1");
    cmd.env("HERMES_DASHBOARD_SESSION_TOKEN", &session_token);

    // ── .env bridge (ADR-003 #4 — still valid): inject ALL .env keys ──────
    let env_map = crate::config::read_env(hermes_home, profile);
    for (key, value) in &env_map {
        // Don't override keys we set explicitly above.
        if key != "HERMES_HOME"
            && key != "PYTHONUNBUFFERED"
            && key != "HERMES_DASHBOARD_SESSION_TOKEN"
        {
            cmd.env(key, value);
        }
    }

    // ── Proxy (ADR-003: env chain, not config.yaml keys) ──────────────────
    let proxy_url = crate::config::resolve_effective_proxy(hermes_home, profile);

    if !proxy_url.is_empty() {
        cmd.env("HTTP_PROXY", &proxy_url);
        cmd.env("HTTPS_PROXY", &proxy_url);
        cmd.env("ALL_PROXY", &proxy_url);
        cmd.env("http_proxy", &proxy_url);
        cmd.env("https_proxy", &proxy_url);
        cmd.env("all_proxy", &proxy_url);
        tracing::info!(target: "steersman_desktop_lib::gateway", proxy = %proxy_url, "proxy enabled");
    } else {
        tracing::info!(target: "steersman_desktop_lib::gateway", "no proxy detected (direct or TUN mode)");
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

    // Drain stdout/stderr in the background. The stdout thread ALSO parses the
    // HERMES_BACKEND_READY port=N line (ADR-004 §1) and stores it in a shared
    // cell so the main thread can pick up the OS-assigned port.
    let port_cell: Arc<Mutex<Option<u16>>> = Arc::new(Mutex::new(None));
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let profile_key_clone = profile_key.clone();
    let port_cell_stdout = Arc::clone(&port_cell);
    thread::spawn(move || {
        let reader = std::io::BufReader::new(stdout);
        for line in reader.lines().flatten() {
            eprintln!("[gateway:{}] {}", profile_key_clone, line);
            // Capture the OS-assigned port the first time we see the ready line.
            if let Some(p) = parse_ready_port(&line) {
                let mut cell = port_cell_stdout.lock().unwrap_or_else(|e| e.into_inner());
                if cell.is_none() {
                    *cell = Some(p);
                }
            }
        }
    });
    let profile_key_clone2 = profile_key.clone();
    thread::spawn(move || {
        let stderr_reader = std::io::BufReader::new(stderr);
        for line in stderr_reader.lines().flatten() {
            eprintln!("[gateway:{}] {}", profile_key_clone2, line);
        }
    });

    // ── Readiness (ADR-004 §3): wait for the backend to print its port, then
    // confirm a WS handshake completes (gateway.ready event). The legacy TCP
    // /health poll is gone — hermes serve has no /health on the WS server.
    let mut ready = false;
    let mut port: u16 = 0;
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        // Bail early if the child already exited.
        if let Ok(Some(_status)) = child.try_wait() {
            break;
        }
        // Has the stdout thread captured the port yet?
        let captured = {
            let cell = port_cell.lock().unwrap_or_else(|e| e.into_inner());
            *cell
        };
        if let Some(p) = captured {
            port = p;
            // Probe readiness with a WS handshake. spawn a one-shot tokio task
            // (start_gateway itself is sync, so we use a fresh runtime here).
            let ws_url = format!("ws://127.0.0.1:{}/api/ws?token={}", port, session_token);
            let ws_ok = tokio::task::block_in_place(|| {
                tokio::runtime::Handle::try_current()
                    .ok()
                    .map(|h| h.block_on(check_ws_ready(&ws_url)))
                    .unwrap_or(false)
            });
            if ws_ok {
                ready = true;
                break;
            }
        }
        thread::sleep(Duration::from_millis(500));
    }
    tracing::info!(
        target: "steersman_desktop_lib::gateway",
        profile = %profile_key, ready, port,
        "WS handshake complete"
    );

    // If we never captured a port, fall back to 0 (caller treats as not-ready).
    if port == 0 {
        ready = false;
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
                session_token: session_token.clone(),
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
                "Backend process started but did not become ready via WS handshake within 30s. \
                 Check that Hermes Agent is installed and `hermes serve` runs. Captured port: {}.",
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

/// Return the dashboard session token for the spawned gateway (ADR-004 §2).
/// Used to build the WS ?token= URL. Falls back to None if no process is held.
pub fn get_gateway_session_token(state: &GatewayState, profile: Option<&str>) -> Option<String> {
    let profile_key = profile.unwrap_or("default").to_string();
    let processes = state.processes.lock().unwrap_or_else(|p| p.into_inner());
    processes.get(&profile_key).map(|gw| gw.session_token.clone())
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

// ADR-004: hermes serve spawn helpers (P1)
//
// The real Hermes backend is launched via: hermes serve --host 127.0.0.1 --port 0
// With --port 0 the OS assigns a free port, which the backend prints to stdout
// as HERMES_BACKEND_READY port=N. Auth between Steersman and the backend is a
// random session token (secrets.token_urlsafe equivalent) passed to the child
// via HERMES_DASHBOARD_SESSION_TOKEN and echoed back in the WS URL ?token=.
// These helpers keep the spawn logic unit-testable by extracting the pure
// pieces (arg construction, port parsing, token generation).

// Build the argv for hermes serve, in the shape the upstream CLI expects.
// profile None or "default" means no --profile flag (launch profile).
// Otherwise --profile <name> is inserted before the subcommand, mirroring
// the desktop invocation: [--profile, p, serve, --host, 127.0.0.1, --port, 0].
pub fn build_serve_args(profile: Option<&str>) -> Vec<String> {
    let mut args = Vec::new();
    if let Some(p) = profile {
        if p != "default" && !p.is_empty() {
            args.push("--profile".to_string());
            args.push(p.to_string());
        }
    }
    args.push("serve".to_string());
    args.push("--host".to_string());
    args.push("127.0.0.1".to_string());
    args.push("--port".to_string());
    args.push("0".to_string());
    args
}

// Parse the OS-assigned port out of a backend stdout line.
// The backend emits HERMES_BACKEND_READY port=N once it is listening.
// Returns None for lines that do not match (caller keeps scanning stdout).
pub fn parse_ready_port(line: &str) -> Option<u16> {
    // Tolerate surrounding whitespace / ANSI / log prefixes.
    let idx = line.find("HERMES_BACKEND_READY")?;
    let tail = &line[idx..];
    let port_idx = tail.find("port=")?;
    let after = &tail[port_idx + "port=".len()..];
    // Take leading digits only.
    let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse::<u16>().ok()
}

// Generate a fresh dashboard session token.
// Mirrors upstream secrets.token_urlsafe(32): 32 random bytes encoded
// base64url (URL-safe, no padding). Used both as the WS ?token= value and
// as the HERMES_DASHBOARD_SESSION_TOKEN env injected into the child.
pub fn generate_session_token() -> String {
    use base64::Engine;
    let mut bytes = [0u8; 32];
    // getrandom is pulled in transitively by uuid; use its fallback-safe path.
    fill_random(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

// Best-effort cryptographically-secure random fill (getrandom via uuid dep).
fn fill_random(buf: &mut [u8]) {
    // uuid::Uuid::new_v4() internally calls getrandom(); reuse it to fill buf.
    let mut offset = 0;
    while offset < buf.len() {
        let u = uuid::Uuid::new_v4();
        let bytes = u.as_bytes();
        let take = (buf.len() - offset).min(bytes.len());
        buf[offset..offset + take].copy_from_slice(&bytes[..take]);
        offset += take;
    }
}

// One-shot WS readiness probe (ADR-004 §3). Connects, waits up to 5s for the
// gateway.ready event, returns true on success. Used by start_gateway in place
// of the legacy TCP /health poll. The connection is closed immediately after.
async fn check_ws_ready(ws_url: &str) -> bool {
    use futures::StreamExt;
    use tokio_tungstenite::tungstenite::Message;

    let (mut ws, _resp) = match tokio_tungstenite::connect_async(ws_url).await {
        Ok(pair) => pair,
        Err(_) => return false,
    };
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        match tokio::time::timeout(remaining, ws.next()).await {
            Ok(Some(Ok(Message::Text(text)))) => {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                    let is_ready = v.get("method").and_then(|m| m.as_str()) == Some("event")
                        && v.get("params")
                            .and_then(|p| p.get("type"))
                            .and_then(|t| t.as_str())
                            == Some("gateway.ready");
                    if is_ready {
                        let _ = ws.close(None).await;
                        return true;
                    }
                }
            }
            _ => break,
        }
    }
    let _ = ws.close(None).await;
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    // build_serve_args

    #[test]
    fn serve_args_default_profile_no_profile_flag() {
        let args = build_serve_args(None);
        assert_eq!(args, vec!["serve", "--host", "127.0.0.1", "--port", "0"]);
    }

    #[test]
    fn serve_args_named_profile_inserts_profile_flag() {
        let args = build_serve_args(Some("architect"));
        assert_eq!(
            args,
            vec!["--profile", "architect", "serve", "--host", "127.0.0.1", "--port", "0"]
        );
    }

    #[test]
    fn serve_args_default_string_treated_as_no_profile() {
        // "default" is the launch profile; must NOT emit --profile default.
        let args = build_serve_args(Some("default"));
        assert_eq!(args, vec!["serve", "--host", "127.0.0.1", "--port", "0"]);
        assert!(!args.iter().any(|a| a == "--profile"));
    }

    // parse_ready_port

    #[test]
    fn parse_ready_port_extracts_port() {
        assert_eq!(parse_ready_port("HERMES_BACKEND_READY port=9420"), Some(9420));
    }

    #[test]
    fn parse_ready_port_with_log_prefix() {
        assert_eq!(
            parse_ready_port("[2026-07-15 20:00:01] HERMES_BACKEND_READY port=64724"),
            Some(64724)
        );
    }

    #[test]
    fn parse_ready_port_rejects_non_matching_lines() {
        assert_eq!(parse_ready_port("Starting hermes..."), None);
        assert_eq!(parse_ready_port("listening on 8642"), None);
        assert_eq!(parse_ready_port(""), None);
    }

    #[test]
    fn parse_ready_port_invalid_number_returns_none() {
        // port=99999 overflows u16.
        assert_eq!(parse_ready_port("HERMES_BACKEND_READY port=99999"), None);
    }

    // generate_session_token

    #[test]
    fn session_token_is_base64url_no_pad_43_chars() {
        // 32 bytes -> base64url no-pad -> exactly 43 chars, alphabet [A-Za-z0-9_-].
        let t = generate_session_token();
        assert_eq!(t.len(), 43);
        assert!(t.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
    }

    #[test]
    fn session_token_is_unique() {
        let first = generate_session_token();
        let second = generate_session_token();
        assert!(first != second);
    }

    #[test]
    fn session_token_no_padding_char() {
        let t = generate_session_token();
        // base64url-no-pad must never contain the padding char.
        assert_eq!(t.contains('='), false);
    }
}
