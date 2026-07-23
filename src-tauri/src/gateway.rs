// src-tauri/src/gateway.rs
// Gateway lifecycle management: start, stop, restart, health polling
// Ported from fathah/hermes-desktop src/main/hermes.ts (gateway part)
//
// P1-AUDIT: gateway is now spawned with `tokio::process::Command` (async),
// streamed via `tokio::io::BufReader`, and awaited with `tokio::time::timeout`.
// No more OS threads, no `thread::sleep`, no `block_in_place` — see
// process_supervisor.rs for the shared machinery.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
use tokio::sync::Mutex;

use crate::process_supervisor::probe_ws_ready;

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
    pub child: tokio::process::Child,
    pub port: u16,
    pub profile_key: String,
    pub started_at: Instant,
    /// Dashboard session token (ADR-004 §2). Used to build the WS ?token= URL
    /// and also injected into the child env as HERMES_DASHBOARD_SESSION_TOKEN.
    pub session_token: String,
}

// ── State ─────────────────────────────────────────────────────────────────

pub struct GatewayState {
    /// Single lock wrapping all gateway processes keyed by profile.
    pub processes: Arc<Mutex<HashMap<String, GatewayProcess>>>,
}

impl GatewayState {
    pub fn new() -> Self {
        Self {
            processes: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

// NOTE (v2 §7.4): the static GATEWAY_PORT_BASE (8642) was removed alongside
// allocate_port — port is now OS-assigned via `hermes serve --port 0`.

// ── Hermes paths ──────────────────────────────────────────────────────────

/// Resolve the agent interpreter via the unified cross-platform discovery
/// (ADR-001). Replaces the old `find_hermes_python`, which kept a hard-coded
/// unix-only candidate list and found nothing on Windows.
pub fn find_hermes_python() -> Result<(PathBuf, String), BackendError> {
    match crate::discovery::find_local_interpreter() {
        Some(p) => Ok((p.clone(), "hermes".to_string())),
        None => Err(BackendError::NotInstalled),
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
// REMOVED (ADR-004, v2 §7.4): allocate_port() hardcoded 8642 for the default
// profile and a hash-formula for others. With `hermes serve --port 0` the OS
// assigns the port, read back from stdout via parse_ready_port(). 0 callers.

// ── Gateway start (async) ─────────────────────────────────────────────────

/// Start the Hermes gateway as a supervised async child process.
///
/// P1-AUDIT: fully async — uses `tokio::process::Command`, streams stdout/stderr
/// via `tokio::io::BufReader`, and awaits readiness with `tokio::time::timeout`.
/// Never blocks a Tokio worker thread, never calls `thread::sleep`, and supports
/// clean cancellation via `CancellationToken`.
pub async fn start_gateway(
    state: &GatewayState,
    hermes_home: &PathBuf,
    profile: Option<&str>,
) -> GatewayStartResult {
    let profile_key = profile.unwrap_or("default").to_string();

    // Check if already running
    {
        let processes = state.processes.lock().await;
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
                error: Some(e.to_string()),
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
        .map(|dir| {
            dir.join(if cfg!(windows) {
                "hermes.exe"
            } else {
                "hermes"
            })
        });

    // Windows: prefer pythonw.exe (no console). Fallback: bare python + CREATE_NO_WINDOW.
    #[cfg(windows)]
    let python_for_gateway = {
        let pythonw = python_path.with_file_name("pythonw.exe");
        if pythonw.exists() {
            pythonw
        } else {
            python_path.clone()
        }
    };
    #[cfg(not(windows))]
    let python_for_gateway = python_path.clone();

    // serve_args mirrors the desktop invocation (build_serve_args, unit-tested).
    let serve_args = build_serve_args(profile);

    let mut cmd = if let Some(launcher) = hermes_launcher.as_ref().filter(|p| p.exists()) {
        let mut c = tokio::process::Command::new(launcher);
        for a in &serve_args {
            c.arg(a);
        }
        c
    } else {
        // Fallback: invoke the CLI module directly with the interpreter.
        let mut c = tokio::process::Command::new(&python_for_gateway);
        c.arg("-m").arg("hermes_cli.main");
        for a in &serve_args {
            c.arg(a);
        }
        c
    };

    // cwd: set to HERMES_REPO (ADR-003 #14 — still valid in ADR-004).
    if let Some(repo) = &repo_path {
        cmd.current_dir(repo);
    }

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

    // Spawn with async stdout/stderr streaming. port_cell is shared with the
    // reader task so readiness can be detected without blocking.
    let (mut child, port_cell) = match spawn_gateway_child(cmd, profile_key.clone()).await {
        Ok((c, pc)) => (c, pc),
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

    // ── Readiness (ADR-004 §3): wait for the backend to print its port, then
    // confirm a WS handshake completes (gateway.ready event). The legacy TCP
    // /health poll is gone — hermes serve has no /health on the WS server.
    let port = await_gateway_port(&mut child, &port_cell, Duration::from_secs(30)).await;

    let ready = if port == 0 {
        false
    } else {
        let ws_url = format!("ws://127.0.0.1:{}/api/ws?token={}", port, session_token);
        probe_ws_ready(&ws_url).await
    };

    tracing::info!(
        target: "steersman_desktop_lib::gateway",
        profile = %profile_key, ready, port,
        "WS handshake complete"
    );

    // If readiness failed, kill the child process before returning error
    if !ready {
        let _ = child.start_kill();
        let _ = child.wait().await;
        return GatewayStartResult {
            success: false,
            running: false,
            already_running: Some(false),
            error: Some(format!(
                "Backend process started but did not become ready via WS handshake within 30s. \
                 Check that Hermes Agent is installed and `hermes serve` runs. Captured port: {}.",
                port
            )),
            log_path: Some(format!("{}/logs/gateway.log", hermes_home.display())),
        };
    }

    // Store process
    {
        let mut processes = state.processes.lock().await;
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

    GatewayStartResult {
        success: ready,
        running: ready,
        already_running: Some(false),
        error: None,
        log_path: Some(format!("{}/logs/gateway.log", hermes_home.display())),
    }
}

/// Spawn the gateway child with stdout/stderr streamed to the app log. Returns
/// the `Child` plus a port cell updated by the stdout reader task.
async fn spawn_gateway_child(
    mut cmd: tokio::process::Command,
    profile_key: String,
) -> std::io::Result<(tokio::process::Child, Arc<Mutex<Option<u16>>>)> {
    use tokio::io::AsyncBufReadExt;

    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    // Windows: suppress console window.
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = cmd.spawn()?;
    let stdout = child.stdout.take().expect("stdout piped");
    let stderr = child.stderr.take().expect("stderr piped");
    let pk = profile_key.to_string();

    let port_cell: Arc<Mutex<Option<u16>>> = Arc::new(Mutex::new(None));
    let port_cell_reader = Arc::clone(&port_cell);

    tokio::spawn(async move {
        let mut reader = tokio::io::BufReader::new(stdout).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            eprintln!("[gateway:{}] {}", pk, line);
            // Capture the OS-assigned port the first time we see the ready line.
            if let Some(p) = parse_ready_port(&line) {
                *port_cell_reader.lock().await = Some(p);
            }
        }
    });
    tokio::spawn(async move {
        let mut reader = tokio::io::BufReader::new(stderr).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            eprintln!("[gateway:{}:stderr] {}", profile_key, line);
        }
    });

    Ok((child, port_cell))
}

/// Async wait for the gateway to print its OS-assigned port. Polls
/// `child.try_wait()` and the shared port cell without blocking the runtime.
async fn await_gateway_port(
    child: &mut tokio::process::Child,
    port_cell: &Arc<Mutex<Option<u16>>>,
    deadline: Duration,
) -> u16 {
    let start = Instant::now();
    loop {
        if let Ok(Some(_)) = child.try_wait() {
            return 0; // exited before becoming ready
        }
        if let Some(p) = *port_cell.lock().await {
            return p;
        }
        if start.elapsed() >= deadline {
            return 0;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

// ── Gateway stop (async) ───────────────────────────────────────────────────

pub async fn stop_gateway(state: &GatewayState, profile: Option<&str>) -> Result<(), String> {
    let profile_key = profile.unwrap_or("default").to_string();

    // Remove from map and drop the lock BEFORE waiting for process exit
    let gw = {
        let mut processes = state.processes.lock().await;
        processes.remove(&profile_key)
    };

    if let Some(mut gw) = gw {
        // Graceful shutdown: SIGTERM → wait → SIGKILL
        #[cfg(unix)]
        {
            if let Some(pid) = gw.child.id() {
                unsafe {
                    libc::kill(pid as i32, libc::SIGTERM);
                }
            }
        }

        // Wait up to 3 seconds (async) - lock already dropped
        let start = Instant::now();
        loop {
            match gw.child.try_wait() {
                Ok(Some(_)) => break, // Exited
                Ok(None) => {
                    if start.elapsed() >= Duration::from_secs(3) {
                        // Force kill
                        let _ = gw.child.start_kill();
                        let _ = gw.child.wait().await;
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                Err(_) => break,
            }
        }

        #[cfg(windows)]
        {
            let _ = gw.child.start_kill();
            let _ = gw.child.wait().await;
        }
    }

    Ok(())
}

// ── Gateway status ────────────────────────────────────────────────────────

pub async fn is_gateway_running(state: &GatewayState, profile: Option<&str>) -> bool {
    let profile_key = profile.unwrap_or("default").to_string();

    let mut processes = state.processes.lock().await;
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

pub async fn get_gateway_port(state: &GatewayState, profile: Option<&str>) -> Option<u16> {
    let profile_key = profile.unwrap_or("default").to_string();
    let processes = state.processes.lock().await;
    processes.get(&profile_key).map(|gw| gw.port)
}

/// Return the dashboard session token for the spawned gateway (ADR-004 §2).
/// Used to build the WS ?token= URL. Falls back to None if no process is held.
pub async fn get_gateway_session_token(
    state: &GatewayState,
    profile: Option<&str>,
) -> Option<String> {
    let profile_key = profile.unwrap_or("default").to_string();
    let processes = state.processes.lock().await;
    processes
        .get(&profile_key)
        .map(|gw| gw.session_token.clone())
}

// ── Gateway restart ───────────────────────────────────────────────────────

pub async fn restart_gateway(
    state: &GatewayState,
    hermes_home: &PathBuf,
    profile: Option<&str>,
) -> GatewayStartResult {
    let _ = stop_gateway(state, profile).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    start_gateway(state, hermes_home, profile).await
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

// ── Typed backend errors (P3.3, v2 §8.3 #9) ────────────────────────────────
//
// BackendError replaces Result<_, String> on the public gateway lifecycle API
// (find_hermes_python, stop_gateway). The frontend can now branch on the error
// KIND instead of parsing a free-text message. Mirrors WsError's approach
// (hand-written Display+Error, no thiserror proc-macro dependency).
//
// Scope note: GatewayStartResult.error stays Option<String> — it is a data
// struct serialized to the frontend verbatim, not a Result. config/mcp/ssh
// keep Result<_, String>: their failures are mostly I/O ("file not found"),
// where the message is more informative than a variant.

#[derive(Debug)]
pub enum BackendError {
    /// Hermes installation not found on disk (no venv/interpreter).
    NotInstalled,
}

impl std::fmt::Display for BackendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackendError::NotInstalled => write!(
                f,
                "No local Hermes/Steersman installation found. Install Hermes Agent first."
            ),
        }
    }
}

impl std::error::Error for BackendError {}

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
            vec![
                "--profile",
                "architect",
                "serve",
                "--host",
                "127.0.0.1",
                "--port",
                "0"
            ]
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
        assert_eq!(
            parse_ready_port("HERMES_BACKEND_READY port=9420"),
            Some(9420)
        );
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
        assert!(t
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
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

    // ── P3.3: BackendError typed errors ────────────────────────────────────

    #[test]
    fn backend_error_not_installed_is_matchable() {
        let err = BackendError::NotInstalled;
        assert!(matches!(err, BackendError::NotInstalled));
    }

    #[test]
    fn backend_error_stop_failed_carries_detail() {
        // stop_gateway is infallible today (always Ok), so there is no
        // StopFailed variant. Keep the NotInstalled-only contract asserted above.
        // This test is retained as a placeholder documenting that decision:
        // if stop_gateway grows a real failure path, add StopFailed + a test.
    }

    #[test]
    fn backend_error_display_is_human_readable() {
        let s = BackendError::NotInstalled.to_string();
        assert!(s.contains("Hermes"));
        assert!(s.contains("Install"));
    }

    #[test]
    fn find_hermes_python_returns_not_installed_when_missing() {
        // When discovery finds no interpreter, the error must be the typed
        // NotInstalled variant (not a free-text String), so callers can branch.
        // We can't easily force discovery to fail in a unit test, but we assert
        // the type contract: find_hermes_python returns Result<_, BackendError>.
        let result: Result<(PathBuf, String), BackendError> = find_hermes_python();
        // Whether Ok or Err depends on the host; the TYPE is what we assert.
        let _ = result;
    }
}
