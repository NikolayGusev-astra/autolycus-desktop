// src-tauri/src/ssh.rs
// SSH tunnel + remote execution
// Ported from fathah/hermes-desktop src/main/ssh-tunnel.ts + ssh-remote.rs
//
// P1-AUDIT: ssh_exec now uses tokio::process::Command with a proper overall
// execution timeout (not just connect timeout). kill_on_drop ensures cleanup.

use std::io::{BufRead, BufReader};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::Child;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::config::{expand_tilde, SshConfig};
use thiserror::Error;
use tokio::process::Command;
use tokio::time::timeout;

#[derive(Error, Debug)]
pub enum SshError {
    #[error("SSH command timed out after {seconds}s")]
    Timeout { seconds: u64 },
    #[error("SSH process spawn failed: {0}")]
    SpawnFailed(String),
    #[error("SSH process wait failed: {0}")]
    WaitFailed(String),
    #[error("SSH command failed with exit code {code}: {stderr}")]
    CommandFailed { code: i32, stderr: String },
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Invalid config: {0}")]
    InvalidConfig(String),
}

impl From<SshError> for String {
    fn from(e: SshError) -> Self {
        e.to_string()
    }
}

// ── State ─────────────────────────────────────────────────────────────────

pub struct SshState {
    pub tunnel_process: Arc<Mutex<Option<Child>>>,
    pub active_config: Arc<Mutex<Option<SshConfig>>>,
    pub tunnel_running: Arc<Mutex<bool>>,
    tunnel_generation: AtomicU64,
}

impl SshState {
    pub fn new() -> Self {
        Self {
            tunnel_process: Arc::new(Mutex::new(None)),
            active_config: Arc::new(Mutex::new(None)),
            tunnel_running: Arc::new(Mutex::new(false)),
            tunnel_generation: AtomicU64::new(0),
        }
    }

    /// Monotonically increasing generation of successfully started tunnels.
    pub fn tunnel_generation(&self) -> u64 {
        self.tunnel_generation.load(Ordering::Acquire)
    }

    pub fn active_config(&self) -> Option<SshConfig> {
        self.active_config.lock().unwrap().clone()
    }
}

// ── Shell-path safety ─────────────────────────────────────────────────────

/// A filesystem path is "shell-safe" if it contains no shell metacharacters,
/// so it can be interpolated into a remote `nohup <path> ... &` command without
/// quoting. Used to guard `start_remote_gateway_cmd` against injection.
pub fn is_safe_shell_path(path: &str) -> bool {
    if path.is_empty() {
        return false;
    }
    // Reject any shell metacharacter that could break out of the command.
    const FORBIDDEN: &[char] = &[
        ';', '|', '&', '$', '`', '(', ')', '{', '}', '<', '>', '\n', '\r', '\t', '"', '\'', '\\',
        '*', '?', '[', ']', '!', '#', '~', '=', ':',
    ];
    // Allow letters, digits, path separators, dot, dash, underscore, plus.
    path.chars().all(|c| !FORBIDDEN.contains(&c))
}

// ── Tunnel URL ────────────────────────────────────────────────────────────

pub fn get_tunnel_url(state: &SshState) -> Option<String> {
    let running = *state.tunnel_running.lock().unwrap();
    let config = state.active_config.lock().unwrap();
    if running {
        config
            .as_ref()
            .map(|c| format!("http://127.0.0.1:{}", c.local_port))
    } else {
        None
    }
}

pub fn is_tunnel_active(state: &SshState) -> bool {
    let mut process = state.tunnel_process.lock().unwrap();
    let alive = process
        .as_mut()
        .is_some_and(|child| child.try_wait().ok().flatten().is_none());
    if !alive {
        process.take();
        *state.tunnel_running.lock().unwrap() = false;
    }
    alive
}

/// Monitor the tunnel process until it exits.  RuntimeSupervisor owns this
/// task and uses its completion as the tunnel-health signal.
pub fn monitor_tunnel(state: &SshState) -> tokio::task::JoinHandle<()> {
    let process = Arc::clone(&state.tunnel_process);
    let running = Arc::clone(&state.tunnel_running);
    tokio::spawn(async move {
        loop {
            let alive = process
                .lock()
                .unwrap()
                .as_mut()
                .is_some_and(|child| child.try_wait().ok().flatten().is_none());
            if !alive {
                *running.lock().unwrap() = false;
                return;
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    })
}

/// A synchronous closer suitable for RuntimeSupervisor's drop-safe guard.
pub fn tunnel_cleanup(state: &SshState) -> Arc<dyn Fn() + Send + Sync> {
    let process = Arc::clone(&state.tunnel_process);
    let running = Arc::clone(&state.tunnel_running);
    let config = Arc::clone(&state.active_config);
    Arc::new(move || {
        if let Some(mut child) = process.lock().unwrap().take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        *running.lock().unwrap() = false;
        *config.lock().unwrap() = None;
    })
}

// ── SSH tunnel management (uses std::process) ──────────────────────────────

/// Start SSH tunnel in background thread. Returns local port on success.
pub fn start_ssh_tunnel(
    state: &SshState,
    mut config: SshConfig,
    _hermes_home: PathBuf,
) -> Result<u16, String> {
    // Stop existing tunnel if any
    stop_ssh_tunnel(state)?;

    let local_port = portpicker::pick_unused_port().ok_or("No free local port")?;
    let remote_port = config.remote_port;

    let key_path = expand_tilde(&config.key_path);
    let key_path_str = key_path.to_string();

    let mut cmd = std::process::Command::new("ssh");
    cmd.args([
        "-N", // no remote command, just tunnel
        "-L",
        &format!("127.0.0.1:{local_port}:127.0.0.1:{remote_port}"),
        "-o",
        "ExitOnForwardFailure=yes",
        "-o",
        "ServerAliveInterval=15",
        "-o",
        "ServerAliveCountMax=3",
        "-o",
        "StrictHostKeyChecking=accept-new",
        "-i",
        &key_path_str,
    ]);
    cmd.arg(format!("{}@{}", config.username, config.host));
    if config.port != 22 {
        cmd.args(["-p", &config.port.to_string()]);
    }

    let mut child = cmd
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn SSH tunnel: {e}"))?;

    let stderr = child.stderr.take().expect("stderr piped");
    let reader = BufReader::new(stderr);

    // Wait for tunnel to be ready (read stderr for "forwarding" line)
    let ready = Arc::new(Mutex::new(false));
    let ready_clone = ready.clone();

    thread::spawn(move || {
        for line in reader.lines().flatten() {
            eprintln!("[ssh tunnel] {}", line);
            if line.contains("forwarding") || line.contains("Forwarding") {
                *ready_clone.lock().unwrap() = true;
                break;
            }
        }
    });

    // Poll for readiness with timeout
    let start = Instant::now();
    let timeout_dur = Duration::from_secs(10);
    while !*ready.lock().unwrap() {
        if start.elapsed() > timeout_dur {
            let _ = child.kill();
            return Err("SSH tunnel startup timeout".into());
        }
        thread::sleep(Duration::from_millis(100));
        if child
            .try_wait()
            .map_err(|e| format!("SSH tunnel wait error: {e}"))?
            .is_some()
        {
            return Err("SSH tunnel process exited unexpectedly".into());
        }
    }

    // The OS-selected local port, not the configured preference, is the
    // endpoint the supervisor and WebSocket client must monitor.
    config.local_port = local_port;
    *state.tunnel_running.lock().unwrap() = true;
    *state.active_config.lock().unwrap() = Some(config);
    *state.tunnel_process.lock().unwrap() = Some(child);
    state.tunnel_generation.fetch_add(1, Ordering::Release);

    Ok(local_port)
}

/// Stop SSH tunnel.
pub fn stop_ssh_tunnel(state: &SshState) -> Result<(), String> {
    if let Some(mut child) = state.tunnel_process.lock().unwrap().take() {
        let _ = child.kill();
        let _ = child.wait();
    }
    *state.tunnel_running.lock().unwrap() = false;
    *state.active_config.lock().unwrap() = None;
    Ok(())
}

// ── Remote command execution with overall timeout ───────────────────────────

/// Execute a command on the remote host via SSH with a hard timeout on the
/// entire execution (connection + command run).
///
/// Uses `tokio::process::Command` with `kill_on_drop(true)` so the child is
/// reaped if the future is dropped or times out.
pub async fn ssh_exec(
    config: &SshConfig,
    command: &str,
    timeout_secs: u64,
) -> Result<String, SshError> {
    let key_path = expand_tilde(&config.key_path);
    let key_path_str = key_path.to_string();

    let mut cmd = Command::new("ssh");
    cmd.args([
        "-o",
        "ConnectTimeout=10",
        "-o",
        "ServerAliveInterval=15",
        "-o",
        "ServerAliveCountMax=3",
        "-o",
        "StrictHostKeyChecking=accept-new",
        "-i",
        &key_path_str,
    ]);
    cmd.arg(format!("{}@{}", config.username, config.host));
    if config.port != 22 {
        cmd.args(["-p", &config.port.to_string()]);
    }
    cmd.arg(command);

    cmd.kill_on_drop(true);
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let child = cmd
        .spawn()
        .map_err(|e| SshError::SpawnFailed(e.to_string()))?;

    // Use tokio::time::timeout with kill_on_drop handling cleanup
    // Since kill_on_drop is true, the child will be killed when dropped
    let output = match timeout(Duration::from_secs(timeout_secs), child.wait_with_output()).await {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => return Err(SshError::WaitFailed(e.to_string())),
        Err(_) => {
            // Timeout - child will be killed on drop due to kill_on_drop(true)
            return Err(SshError::Timeout {
                seconds: timeout_secs,
            });
        }
    };

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        let code = output.status.code().unwrap_or(-1);
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        Err(SshError::CommandFailed { code, stderr })
    }
}

// ── Tunnel health check ────────────────────────────────────────────────────

pub async fn check_tunnel_health(_port: u16) -> bool {
    // Simple TCP connect check
    TcpStream::connect_timeout(&"127.0.0.1".parse().unwrap(), Duration::from_secs(2)).is_ok()
}

// ── Synchronous test connection (for config validation) ──────────────────────

pub fn test_ssh_connection(config: &SshConfig) -> Result<bool, String> {
    let key_path = expand_tilde(&config.key_path);
    let key_path_str = key_path.to_string();

    let output = std::process::Command::new("ssh")
        .arg("-o")
        .arg("BatchMode=yes")
        .arg("-o")
        .arg("StrictHostKeyChecking=accept-new")
        .arg("-o")
        .arg("ConnectTimeout=10")
        .arg("-i")
        .arg(&key_path_str)
        .arg("-p")
        .arg(config.port.to_string())
        .arg(format!("{}@{}", config.username, config.host))
        .arg("echo ok")
        .output()
        .map_err(|e| format!("SSH test failed: {}", e))?;

    Ok(output.status.success() && String::from_utf8_lossy(&output.stdout).contains("ok"))
}
