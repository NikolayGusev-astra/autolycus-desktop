use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

// ── Types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub mode: String,
    pub python_path: Option<String>,
    pub script_path: Option<String>,
    pub remote_host: Option<String>,
    pub remote_port: Option<u16>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentEvent {
    pub event_type: String,
    pub payload: serde_json::Value,
    pub session_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ConnectionInfo {
    pub port: u16,
    pub mode: String,
}

struct AgentState {
    tcp_stream: Mutex<Option<TcpStream>>,
    config: Mutex<Option<AgentConfig>>,
}

impl AgentState {
    fn new() -> Self {
        Self {
            tcp_stream: Mutex::new(None),
            config: Mutex::new(None),
        }
    }
}

// ── Helpers ────────────────────────────────────────────────────────────

fn parse_port_line(line: &str) -> Option<u16> {
    line.strip_prefix("READY:")?.parse::<u16>().ok()
}

fn find_python(config: &AgentConfig) -> Result<std::path::PathBuf, String> {
    if let Some(ref path) = config.python_path {
        let p = std::path::PathBuf::from(path);
        if p.exists() {
            return Ok(p);
        }
        return Err(format!("Python not found at: {}", path));
    }

    let home = dirs::home_dir().ok_or("Cannot determine home directory")?;
    let candidates = [
        home.join("autolycus/venv/bin/python"),
        home.join("autolycus/venv/bin/python3"),
        home.join(".autolycus/venv/bin/python"),
        home.join(".hermes/venv/bin/python"),
        home.join(".hermes/hermes-agent/venv/bin/python3"),
    ];

    for path in &candidates {
        if path.exists() {
            return Ok(path.clone());
        }
    }

    if let Ok(output) = Command::new("which").arg("python3").output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Ok(std::path::PathBuf::from(path));
            }
        }
    }

    Err("Python not found. Install Python 3.11+ or specify path in settings.".to_string())
}

fn find_script(config: &AgentConfig) -> Result<std::path::PathBuf, String> {
    if let Some(ref path) = config.script_path {
        let p = std::path::PathBuf::from(path);
        if p.exists() {
            return Ok(p);
        }
        return Err(format!("Script not found at: {}", path));
    }

    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()));

    let search_paths: Vec<std::path::PathBuf> = [
        // Bundled with the app
        exe_dir.as_ref().map(|d| d.join("python/tcp_server.py")),
        exe_dir.as_ref().map(|d| d.join("tui_gateway/tcp_server.py")),
        exe_dir.as_ref().map(|d| d.join("tui_gateway/entry.py")),
        // Development / system
        dirs::home_dir().map(|h| h.join("autolycus/tui_gateway/tcp_server.py")),
        dirs::home_dir().map(|h| h.join("autolycus/tui_gateway/entry.py")),
        dirs::home_dir().map(|h| h.join(".autolycus/tui_gateway/tcp_server.py")),
        Some(std::path::PathBuf::from(
            "/opt/autolycus/tui_gateway/tcp_server.py",
        )),
        Some(std::path::PathBuf::from(
            "/usr/local/lib/autolycus/tui_gateway/tcp_server.py",
        )),
    ]
    .iter()
    .filter_map(|p| p.clone())
    .collect();

    for path in &search_paths {
        if path.exists() {
            return Ok(path.clone());
        }
    }

    Err("Python backend script not found. Ensure autolycus is installed.".to_string())
}

// ── Event reader ───────────────────────────────────────────────────────

fn spawn_event_reader(
    app_handle: AppHandle,
    reader: BufReader<impl Read + Send + 'static>,
) {
    thread::spawn(move || {
        for line in reader.lines() {
            match line {
                Ok(text) => {
                    let trimmed = text.trim();
                    if trimmed.is_empty() {
                        continue;
                    }

                    match serde_json::from_str::<serde_json::Value>(trimmed) {
                        Ok(value) => {
                            let event_type = value
                                .get("params")
                                .and_then(|p| p.get("type"))
                                .and_then(|t| t.as_str())
                                .unwrap_or("unknown")
                                .to_string();

                            let session_id = value
                                .get("params")
                                .and_then(|p| p.get("session_id"))
                                .and_then(|s| s.as_str())
                                .map(|s| s.to_string());

                            let _ = app_handle.emit(
                                "agent_event",
                                AgentEvent {
                                    event_type,
                                    payload: value,
                                    session_id,
                                },
                            );
                        }
                        Err(_) => {
                            let _ = app_handle.emit(
                                "agent_event",
                                AgentEvent {
                                    event_type: "gateway.stderr".to_string(),
                                    payload: serde_json::json!({"line": trimmed}),
                                    session_id: None,
                                },
                            );
                        }
                    }
                }
                Err(_) => {
                    let _ = app_handle.emit(
                        "agent_event",
                        AgentEvent {
                            event_type: "gateway.exited".to_string(),
                            payload: serde_json::json!({"reason": "read error"}),
                            session_id: None,
                        },
                    );
                    break;
                }
            }
        }
    });
}

// ── Tauri Commands ─────────────────────────────────────────────────────

#[tauri::command]
async fn start_agent(
    state: State<'_, AgentState>,
    app_handle: tauri::AppHandle,
    config: AgentConfig,
) -> Result<ConnectionInfo, String> {
    stop_agent(state.clone()).await?;

    match config.mode.as_str() {
        "local" => start_local(state, app_handle, config).await,
        "remote" => start_remote(state, app_handle, config).await,
        _ => Err(format!("Unknown mode: {}", config.mode)),
    }
}

async fn start_local(
    state: State<'_, AgentState>,
    app_handle: tauri::AppHandle,
    config: AgentConfig,
) -> Result<ConnectionInfo, String> {
    let python = find_python(&config)?;
    let script = find_script(&config)?;

    let mut child = Command::new(&python)
        .arg(&script)
        .arg("--port=0")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to start Python backend: {}", e))?;

    // Read READY:PORT
    let stdout = child.stdout.take().ok_or("No stdout")?;
    let mut stdout_reader = BufReader::new(stdout);
    let mut line = String::new();
    stdout_reader
        .read_line(&mut line)
        .map_err(|e| format!("Failed to read port: {}", e))?;

    let port = parse_port_line(line.trim())
        .ok_or_else(|| format!("Invalid READY line: {}", line.trim()))?;

    // Connect TCP
    let addr = format!("127.0.0.1:{}", port);
    let stream = TcpStream::connect(&addr)
        .map_err(|e| format!("Failed to connect to backend: {}", e))?;

    stream
        .set_read_timeout(Some(Duration::from_secs(60)))
        .map_err(|e| format!("Failed to set timeout: {}", e))?;

    let write_stream = stream.try_clone().map_err(|e| format!("Failed to clone: {}", e))?;

    *state.tcp_stream.lock().unwrap() = Some(write_stream);
    *state.config.lock().unwrap() = Some(config.clone());

    // Event reader
    spawn_event_reader(app_handle.clone(), stdout_reader);

    // Stderr reader
    if let Some(stderr) = child.stderr.take() {
        let stderr_reader = BufReader::new(stderr);
        let app = app_handle.clone();
        thread::spawn(move || {
            for line in stderr_reader.lines() {
                if let Ok(text) = line {
                    if !text.trim().is_empty() {
                        let _ = app.emit("agent_event", AgentEvent {
                            event_type: "gateway.stderr".to_string(),
                            payload: serde_json::json!({"line": text}),
                            session_id: None,
                        });
                    }
                } else {
                    break;
                }
            }
        });
    }

    // NOTE: child handle leaked. Proper: store in Mutex<Option<Child>>.
    std::mem::forget(child);

    Ok(ConnectionInfo { port, mode: "local".to_string() })
}

async fn start_remote(
    state: State<'_, AgentState>,
    app_handle: tauri::AppHandle,
    config: AgentConfig,
) -> Result<ConnectionInfo, String> {
    let host = config.remote_host.as_ref().ok_or("Remote host not specified")?;
    let port = config.remote_port.ok_or("Remote port not specified")?;

    let addr = format!("{}:{}", host, port);
    let stream = TcpStream::connect(&addr)
        .map_err(|e| format!("Failed to connect to {}: {}", addr, e))?;

    stream
        .set_read_timeout(Some(Duration::from_secs(60)))
        .map_err(|e| format!("Failed to set timeout: {}", e))?;

    let write_stream = stream.try_clone().map_err(|e| format!("Failed to clone: {}", e))?;

    *state.tcp_stream.lock().unwrap() = Some(write_stream);
    *state.config.lock().unwrap() = Some(config.clone());

    let reader = BufReader::new(stream);
    spawn_event_reader(app_handle, reader);

    Ok(ConnectionInfo { port, mode: "remote".to_string() })
}

#[tauri::command]
async fn stop_agent(state: State<'_, AgentState>) -> Result<(), String> {
    *state.tcp_stream.lock().unwrap() = None;
    *state.config.lock().unwrap() = None;
    Ok(())
}

#[tauri::command]
async fn send_rpc(
    state: State<'_, AgentState>,
    method: String,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let mut guard = state.tcp_stream.lock().unwrap();

    match guard.as_mut() {
        Some(stream) => {
            let request = serde_json::json!({
                "jsonrpc": "2.0",
                "id": format!("{}", std::process::id()),
                "method": method,
                "params": params,
            });

            let line = format!(
                "{}\n",
                serde_json::to_string(&request)
                    .map_err(|e| format!("Serialization error: {}", e))?
            );

            stream
                .write_all(line.as_bytes())
                .map_err(|e| format!("Write error: {}", e))?;
            stream.flush().map_err(|e| format!("Flush error: {}", e))?;

            Ok(serde_json::json!({"status": "sent"}))
        }
        None => Err("Not connected to backend".to_string()),
    }
}

#[tauri::command]
fn get_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

// ── Entry point ────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AgentState::new())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            start_agent,
            stop_agent,
            send_rpc,
            get_app_version,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
