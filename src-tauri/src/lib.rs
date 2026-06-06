use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Child, Stdio};
use std::sync::Mutex;
use std::thread;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

// ── Types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub mode: String,          // "local" | "remote"
    pub python_path: Option<String>,
    pub instance: Option<String>, // "hermes" | "autolycus" | "hermes-agent"
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
    pub mode: String,
    pub instance: String,
}

struct AgentState {
    child: Mutex<Option<Child>>,
    stdin_tx: Mutex<Option<std::sync::mpsc::Sender<String>>>,
    config: Mutex<Option<AgentConfig>>,
}

impl AgentState {
    fn new() -> Self {
        Self {
            child: Mutex::new(None),
            stdin_tx: Mutex::new(None),
            config: Mutex::new(None),
        }
    }
}

// ── Helpers ────────────────────────────────────────────────────────────

fn find_instance(config: &AgentConfig) -> Result<(String, String), String> {
    // If user specified python_path, use it directly
    if let Some(ref path) = config.python_path {
        let p = std::path::PathBuf::from(path);
        if p.exists() {
            let instance = detect_instance_type(&p);
            return Ok((path.clone(), instance));
        }
        return Err(format!("Python not found at: {}", path));
    }

    // Auto-detect installed instances
    let home = dirs::home_dir().ok_or("Cannot determine home directory")?;

    let candidates: Vec<(std::path::PathBuf, String)> = vec![
        // Autolycus venv
        (home.join("autolycus/venv/bin/python"), "autolycus".to_string()),
        (home.join("autolycus/venv/bin/python3"), "autolycus".to_string()),
        (home.join(".autolycus/venv/bin/python"), "autolycus".to_string()),
        // Hermes venv
        (home.join(".hermes/venv/bin/python"), "hermes".to_string()),
        (home.join(".hermes/hermes-agent/venv/bin/python"), "hermes-agent".to_string()),
        // System
        (std::path::PathBuf::from("/usr/local/bin/python3"), "system".to_string()),
    ];

    for (path, instance) in candidates {
        if path.exists() {
            return Ok((path.to_string_lossy().to_string(), instance));
        }
    }

    Err("No Python instance found. Install autolycus or hermes.".to_string())
}

fn detect_instance_type(python_path: &std::path::PathBuf) -> String {
    // Try to detect which instance this python belongs to
    let path_str = python_path.to_string_lossy();
    if path_str.contains("autolycus") {
        "autolycus".to_string()
    } else if path_str.contains("hermes-agent") {
        "hermes-agent".to_string()
    } else if path_str.contains("hermes") {
        "hermes".to_string()
    } else {
        "unknown".to_string()
    }
}

fn get_tui_gateway_module(instance: &str) -> &'static str {
    match instance {
        "autolycus" => "tui_gateway.entry",
        "hermes" => "tui_gateway.entry",
        "hermes-agent" => "tui_gateway.entry",
        _ => "tui_gateway.entry",
    }
}

// ── Event reader ──────────────────────────────────────────────────────

fn spawn_event_reader(app_handle: AppHandle, mut reader: BufReader<std::process::ChildStdout>) {
    thread::spawn(move || {
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => break, // EOF
                Ok(_) => {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }

                    // Parse JSON-RPC event
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
                        let event_type = value
                            .get("type")
                            .or_else(|| value.get("params").and_then(|p| p.get("type")))
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
                }
                Err(_) => break,
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
    // Stop existing
    stop_agent(state.clone()).await?;

    let (python_path, instance) = find_instance(&config)?;
    let module = get_tui_gateway_module(&instance);

    // Spawn child process: python -m tui_gateway.entry
    let mut child = Command::new(&python_path)
        .arg("-m")
        .arg(module)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("HERMES_PYTHON_SRC_ROOT", std::path::PathBuf::from(&python_path).parent().unwrap().parent().unwrap())
        .spawn()
        .map_err(|e| format!("Failed to spawn {}: {}", instance, e))?;

    // Set up stdout reader
    let stdout = child.stdout.take().ok_or("No stdout")?;
    let reader = BufReader::new(stdout);
    spawn_event_reader(app_handle, reader);

    // Set up stdin writer channel
    let stdin = child.stdin.take().ok_or("No stdin")?;
    let (tx, rx) = std::sync::mpsc::String::new();
    thread::spawn(move || {
        let mut stdin = stdin;
        while let Ok(line) = rx.recv() {
            if writeln!(stdin, "{}", line).is_err() {
                break;
            }
        }
    });

    *state.child.lock().unwrap() = Some(child);
    *state.stdin_tx.lock().unwrap() = Some(tx);
    *state.config.lock().unwrap() = Some(config);

    Ok(ConnectionInfo {
        mode: "local".to_string(),
        instance,
    })
}

#[tauri::command]
async fn stop_agent(state: State<'_, AgentState>) -> Result<(), String> {
    *state.stdin_tx.lock().unwrap() = None;
    if let Some(mut child) = state.child.lock().unwrap().take() {
        let _ = child.kill();
        let _ = child.wait();
    }
    *state.config.lock().unwrap() = None;
    Ok(())
}

#[tauri::command]
async fn send_rpc(
    state: State<'_, AgentState>,
    method: String,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let tx = state
        .stdin_tx
        .lock()
        .unwrap()
        .clone()
        .ok_or("Not connected to backend")?;

    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": format!("{}", std::process::id()),
        "method": method,
        "params": params,
    });

    let line = serde_json::to_string(&request)
        .map_err(|e| format!("Serialization error: {}", e))?;

    tx.send(line)
        .map_err(|e| format!("Failed to send: {}", e))?;

    Ok(serde_json::json!({"status": "sent"}))
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
