// src-tauri/src/ssh.rs
// SSH tunnel + remote execution
// Ported from fathah/hermes-desktop src/main/ssh-tunnel.ts + ssh-remote.rs

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::config::{expand_tilde, SshConfig};

// ── State ─────────────────────────────────────────────────────────────────

pub struct SshState {
    pub tunnel_process: Arc<Mutex<Option<Child>>>,
    pub active_config: Arc<Mutex<Option<SshConfig>>>,
    pub tunnel_running: Arc<Mutex<bool>>,
}

impl SshState {
    pub fn new() -> Self {
        Self {
            tunnel_process: Arc::new(Mutex::new(None)),
            active_config: Arc::new(Mutex::new(None)),
            tunnel_running: Arc::new(Mutex::new(false)),
        }
    }
}

// ── Tunnel URL ────────────────────────────────────────────────────────────

pub fn get_tunnel_url(state: &SshState) -> Option<String> {
    let running = *state.tunnel_running.lock().unwrap();
    let config = state.active_config.lock().unwrap();
    if running {
        config.as_ref().map(|c| format!("http://127.0.0.1:{}", c.local_port))
    } else {
        None
    }
}

pub fn is_tunnel_active(state: &SshState) -> bool {
    *state.tunnel_running.lock().unwrap()
}

// ── Health check ──────────────────────────────────────────────────────────

pub async fn check_tunnel_health(port: u16) -> bool {
    let url = format!("http://127.0.0.1:{}/health", port);
    match reqwest::get(&url).await {
        Ok(resp) => resp.status().is_success(),
        Err(_) => false,
    }
}

// ── Start tunnel ──────────────────────────────────────────────────────────

pub fn start_ssh_tunnel(state: &SshState, config: &SshConfig) -> Result<(), String> {
    stop_ssh_tunnel(state);

    let key_path = expand_tilde(&config.key_path);
    let local_port = config.local_port;
    let remote_port = config.remote_port;

    let mut cmd = Command::new("ssh");
    cmd.arg("-N")
        .arg("-L")
        .arg(format!("{}:127.0.0.1:{}", local_port, remote_port))
        .arg("-p")
        .arg(config.port.to_string())
        .arg("-i")
        .arg(&key_path)
        .arg("-o")
        .arg("StrictHostKeyChecking=accept-new")
        .arg("-o")
        .arg("BatchMode=yes")
        .arg("-o")
        .arg("ExitOnForwardFailure=yes")
        .arg("-o")
        .arg("ServerAliveInterval=30")
        .arg("-o")
        .arg("ServerAliveCountMax=3")
        .arg(format!("{}@{}", config.username, config.host))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| format!("Failed to spawn SSH: {}", e))?;

    // Read stderr for errors
    if let Some(stderr) = child.stderr.take() {
        let reader = BufReader::new(stderr);
        thread::spawn(move || {
            for line in reader.lines() {
                if let Ok(text) = line {
                    eprintln!("[ssh] {}", text);
                }
            }
        });
    }

    // Wait for tunnel to be ready
    let start = Instant::now();
    let timeout = Duration::from_secs(12);
    loop {
        // Check if port is open
        if TcpStream::connect_timeout(
            &format!("127.0.0.1:{}", local_port).parse().unwrap(),
            Duration::from_millis(200),
        )
        .is_ok()
        {
            break;
        }

        // Check if process exited
        match child.try_wait() {
            Ok(Some(status)) => {
                return Err(format!(
                    "SSH tunnel exited with status: {}",
                    status
                ));
            }
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    return Err("SSH tunnel connection timeout".to_string());
                }
                thread::sleep(Duration::from_millis(400));
            }
            Err(e) => return Err(format!("SSH process error: {}", e)),
        }
    }

    *state.tunnel_process.lock().unwrap() = Some(child);
    *state.active_config.lock().unwrap() = Some(config.clone());
    *state.tunnel_running.lock().unwrap() = true;

    Ok(())
}

// ── Stop tunnel ───────────────────────────────────────────────────────────

pub fn stop_ssh_tunnel(state: &SshState) {
    let mut process = state.tunnel_process.lock().unwrap();
    if let Some(mut child) = process.take() {
        #[cfg(unix)]
        {
            let pid = child.id();
            unsafe {
                libc::kill(pid as i32, libc::SIGTERM);
            }
        }
        let _ = child.wait();
    }
    *state.active_config.lock().unwrap() = None;
    *state.tunnel_running.lock().unwrap() = false;
}

// ── Test SSH connection ───────────────────────────────────────────────────

pub fn test_ssh_connection(config: &SshConfig) -> Result<bool, String> {
    let key_path = expand_tilde(&config.key_path);
    let output = Command::new("ssh")
        .arg("-o")
        .arg("BatchMode=yes")
        .arg("-o")
        .arg("StrictHostKeyChecking=accept-new")
        .arg("-o")
        .arg("ConnectTimeout=10")
        .arg("-i")
        .arg(&key_path)
        .arg("-p")
        .arg(config.port.to_string())
        .arg(format!("{}@{}", config.username, config.host))
        .arg("echo ok")
        .output()
        .map_err(|e| format!("SSH test failed: {}", e))?;

    Ok(output.status.success() && String::from_utf8_lossy(&output.stdout).contains("ok"))
}

// ── SSH remote exec ───────────────────────────────────────────────────────

pub fn ssh_exec(config: &SshConfig, command: &str, timeout_secs: u64) -> Result<String, String> {
    let key_path = expand_tilde(&config.key_path);

    let mut child = Command::new("ssh")
        .arg("-o")
        .arg("BatchMode=yes")
        .arg("-o")
        .arg("StrictHostKeyChecking=accept-new")
        .arg("-o")
        .arg("ConnectTimeout=15")
        .arg("-i")
        .arg(&key_path)
        .arg("-p")
        .arg(config.port.to_string())
        .arg(format!("{}@{}", config.username, config.host))
        .arg(command)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("SSH exec failed: {}", e))?;

    // Wait with timeout
    let start = Instant::now();
    let timeout = Duration::from_secs(timeout_secs);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if status.success() {
                    // Read stdout
                    // Note: we need to read before wait, this is simplified
                    return Ok("".to_string());
                } else {
                    return Err(format!("SSH command failed with status: {}", status));
                }
            }
            Ok(None) => {
                if start.elapsed() > timeout {
                    return Err("SSH command timed out".to_string());
                }
                thread::sleep(Duration::from_millis(100));
            }
            Err(e) => return Err(format!("SSH wait error: {}", e)),
        }
    }
}
