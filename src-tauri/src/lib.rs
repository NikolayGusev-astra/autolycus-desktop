use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Child, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;

static REQUEST_ID: AtomicU64 = AtomicU64::new(1);

fn next_request_id() -> String {
    REQUEST_ID.fetch_add(1, Ordering::SeqCst).to_string()
}

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

// ── Types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub mode: String,
    pub python_path: Option<String>,
    pub instance: Option<String>,
    pub remote_host: Option<String>,
    pub remote_port: Option<u16>,
}

/// Typed stream events from backend (adapted from hermes-express)
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum StreamEvent {
    /// Text token delta from the model
    #[serde(rename = "token")]
    Token { content: String },
    /// Reasoning/thinking delta
    #[serde(rename = "reasoning")]
    Reasoning { content: String },
    /// Tool status update
    #[serde(rename = "tool_progress")]
    ToolProgress {
        tool: String,
        status: String,
        emoji: String,
        label: String,
        #[serde(rename = "toolCallId", skip_serializing_if = "Option::is_none")]
        tool_call_id: Option<String>,
    },
    /// Stream complete
    #[serde(rename = "done")]
    Done,
    /// Error during streaming
    #[serde(rename = "error")]
    Error { content: String },
    /// Status update
    #[serde(rename = "status")]
    Status { status: String },
    /// Raw JSON event (fallback)
    #[serde(rename = "raw")]
    Raw { payload: serde_json::Value },
}

/// Legacy AgentEvent for backward compatibility
#[derive(Debug, Clone, Serialize)]
pub struct AgentEvent {
    pub event_type: String,
    pub payload: serde_json::Value,
    pub session_id: Option<String>,
}

impl From<StreamEvent> for AgentEvent {
    fn from(event: StreamEvent) -> Self {
        let (event_type, payload) = match &event {
            StreamEvent::Token { content } => ("token".to_string(), serde_json::json!({ "content": content })),
            StreamEvent::Reasoning { content } => ("reasoning".to_string(), serde_json::json!({ "content": content })),
            StreamEvent::ToolProgress { tool, status, emoji, label, tool_call_id } => (
                "tool_progress".to_string(),
                serde_json::json!({ "tool": tool, "status": status, "emoji": emoji, "label": label, "toolCallId": tool_call_id }),
            ),
            StreamEvent::Done => ("done".to_string(), serde_json::json!({})),
            StreamEvent::Error { content } => ("error".to_string(), serde_json::json!({ "content": content })),
            StreamEvent::Status { status } => ("status".to_string(), serde_json::json!({ "status": status })),
            StreamEvent::Raw { payload } => ("raw".to_string(), payload.clone()),
        };
        AgentEvent {
            event_type,
            payload,
            session_id: None,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ConnectionInfo {
    pub mode: String,
    pub instance: String,
}

#[derive(Debug, Serialize)]
pub struct InstanceInfo {
    pub path: String,
    pub instance: String,
    pub exists: bool,
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

fn expand_tilde(path: &str) -> String {
    if path.starts_with("~/") {
        if let Some(home) = dirs::home_dir() {
            return path.replacen("~", &home.to_string_lossy(), 1);
        }
    }
    path.to_string()
}

fn find_instance(config: &AgentConfig) -> Result<(String, String), String> {
    if let Some(ref path) = config.python_path {
        let expanded = expand_tilde(path);
        let p = std::path::PathBuf::from(&expanded);
        if p.exists() {
            let instance = detect_instance_type(&p);
            return Ok((expanded, instance));
        }
        return Err(format!("Python not found at: {}", expanded));
    }

    let home = dirs::home_dir().ok_or("Cannot determine home directory")?;

    let candidates: Vec<(std::path::PathBuf, String)> = vec![
        (home.join("autolycus/venv/bin/python"), "autolycus".to_string()),
        (home.join("autolycus/venv/bin/python3"), "autolycus".to_string()),
        (home.join(".autolycus/venv/bin/python"), "autolycus".to_string()),
        (home.join(".hermes/venv/bin/python"), "hermes".to_string()),
        (home.join(".hermes/hermes-agent/venv/bin/python"), "hermes-agent".to_string()),
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

fn get_tui_gateway_module(_instance: &str) -> &'static str {
    "tui_gateway.entry"
}

/// Check if a line is a JSON-RPC message (not a log line)
fn is_json_rpc(line: &str) -> bool {
    let trimmed = line.trim();
    if !trimmed.starts_with('{') {
        return false;
    }
    // JSON-RPC responses: {"jsonrpc":"2.0","id":"...","result":{...}}
    // JSON-RPC events: {"method":"event","params":{...}}
    trimmed.contains("\"jsonrpc\"") || trimmed.contains("\"method\"")
}

/// Parse a JSON-RPC line into a StreamEvent with session_id
fn parse_stream_event(value: &serde_json::Value) -> Option<(StreamEvent, Option<String>)> {
    let event_type = value
        .get("params")
        .and_then(|p| p.get("type"))
        .or_else(|| value.get("method"))
        .and_then(|t| t.as_str())?;

    let session_id = value
        .get("params")
        .and_then(|p| p.get("session_id"))
        .and_then(|s| s.as_str())
        .map(|s| s.to_string());

    let event = match event_type {
        "gateway.ready" => StreamEvent::Status { status: "ready".to_string() },
        "status.update" => {
            let status = value
                .get("params")
                .and_then(|p| p.get("text"))
                .or_else(|| value.get("params").and_then(|p| p.get("kind")))
                .and_then(|s| s.as_str())
                .unwrap_or("unknown")
                .to_string();
            StreamEvent::Status { status }
        }
        "message.start" => StreamEvent::Status { status: "streaming".to_string() },
        "message.chunk" | "thinking.delta" | "reasoning.delta" => {
            let content = value
                .get("params")
                .and_then(|p| p.get("text"))
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .to_string();
            if event_type.starts_with("reasoning") || event_type.starts_with("thinking") {
                StreamEvent::Reasoning { content }
            } else {
                StreamEvent::Token { content }
            }
        }
        "message.end" => StreamEvent::Done,
        "tool.start" | "tool.progress" | "tool.generating" => {
            let name = value
                .get("params")
                .and_then(|p| p.get("name"))
                .and_then(|s| s.as_str())
                .unwrap_or("unknown")
                .to_string();
            StreamEvent::ToolProgress {
                tool: name,
                status: "running".to_string(),
                emoji: "🔧".to_string(),
                label: "Executing...".to_string(),
                tool_call_id: None,
            }
        }
        "tool.complete" => {
            let name = value
                .get("params")
                .and_then(|p| p.get("name"))
                .and_then(|s| s.as_str())
                .unwrap_or("unknown")
                .to_string();
            StreamEvent::ToolProgress {
                tool: name,
                status: "done".to_string(),
                emoji: "✅".to_string(),
                label: "Complete".to_string(),
                tool_call_id: None,
            }
        }
        "error" => {
            let content = value
                .get("params")
                .and_then(|p| p.get("message"))
                .or_else(|| value.get("error").and_then(|e| e.get("message")))
                .and_then(|s| s.as_str())
                .unwrap_or("Unknown error")
                .to_string();
            StreamEvent::Error { content }
        }
        "gateway.stderr" => {
            let line = value
                .get("params")
                .and_then(|p| p.get("line"))
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .to_string();
            StreamEvent::Error { content: line }
        }
        "gateway.exited" => StreamEvent::Error { content: "Backend process exited".to_string() },
        _ => return None,
    };

    Some((event, session_id))
}

// ── Event reader ───────────────────────────────────────────────────────

fn spawn_event_reader(app_handle: AppHandle, mut reader: BufReader<std::process::ChildStdout>) {
    thread::spawn(move || {
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => {
                    // EOF — backend exited
                    let _ = app_handle.emit("agent_event", AgentEvent {
                        event_type: "gateway.exited".to_string(),
                        payload: serde_json::json!({ "reason": "EOF" }),
                        session_id: None,
                    });
                    break;
                }
                Ok(_) => {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }

                    // Only parse JSON-RPC messages (skip logs, errors, etc.)
                    if !is_json_rpc(trimmed) {
                        continue;
                    }

                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
                        // Try to parse as typed StreamEvent with session_id
                        if let Some((stream_event, session_id)) = parse_stream_event(&value) {
                            let mut agent_event: AgentEvent = stream_event.into();
                            agent_event.session_id = session_id;
                            let _ = app_handle.emit("agent_event", agent_event);
                        } else {
                            // Fallback: emit as raw event
                            let event_type = value
                                .get("params")
                                .and_then(|p| p.get("type"))
                                .or_else(|| value.get("method"))
                                .and_then(|t| t.as_str())
                                .unwrap_or("unknown")
                                .to_string();

                            let session_id = value
                                .get("params")
                                .and_then(|p| p.get("session_id"))
                                .and_then(|s| s.as_str())
                                .map(|s| s.to_string());

                            let _ = app_handle.emit("agent_event", AgentEvent {
                                event_type,
                                payload: value,
                                session_id,
                            });
                        }
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

    // Determine HERMES_PYTHON_SRC_ROOT from python path
    let src_root = std::path::PathBuf::from(&python_path)
        .parent() // bin/
        .and_then(|p| p.parent()) // venv/
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    // Spawn child process: python -m tui_gateway.entry
    let mut child = Command::new(&python_path)
        .arg("-m")
        .arg(module)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("HERMES_PYTHON_SRC_ROOT", &src_root)
        .env("PYTHONUNBUFFERED", "1")
        .spawn()
        .map_err(|e| format!("Failed to spawn {}: {}", instance, e))?;

    // Set up stdout reader
    let stdout = child.stdout.take().ok_or("No stdout")?;
    let reader = BufReader::new(stdout);
    spawn_event_reader(app_handle.clone(), reader);

    // Set up stderr reader — emit errors as events
    let stderr = child.stderr.take().ok_or("No stderr")?;
    let stderr_reader = BufReader::new(stderr);
    let app_handle_stderr = app_handle.clone();
    thread::spawn(move || {
        for line in stderr_reader.lines() {
            if let Ok(text) = line {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    let _ = app_handle_stderr.emit("agent_event", AgentEvent {
                        event_type: "gateway.stderr".to_string(),
                        payload: serde_json::json!({"line": trimmed}),
                        session_id: None,
                    });
                }
            } else {
                break;
            }
        }
    });

    // Set up stdin writer channel
    let stdin = child.stdin.take().ok_or("No stdin")?;
    let (tx, rx) = std::sync::mpsc::channel::<String>();
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
        #[cfg(unix)]
        {
            let pid = child.id();
            // Send SIGTERM first
            let _ = std::process::Command::new("kill")
                .arg("-TERM")
                .arg(pid.to_string())
                .output();
            // Wait with timeout (3 seconds)
            let start = std::time::Instant::now();
            loop {
                match child.try_wait() {
                    Ok(Some(_)) => break, // Process exited
                    Ok(None) => {
                        if start.elapsed().as_secs() >= 3 {
                            // Timeout — force kill
                            let _ = child.kill();
                            let _ = child.wait();
                            break;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    }
                    Err(_) => break,
                }
            }
        }
        #[cfg(not(unix))]
        {
            let _ = child.kill();
            let _ = child.wait();
        }
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

    let id = next_request_id();
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
    });

    let line = serde_json::to_string(&request)
        .map_err(|e| format!("Serialization error: {}", e))?;

    tx.send(line)
        .map_err(|e| format!("Failed to send: {}", e))?;

    // Response will come through the event reader
    Ok(serde_json::json!({"status": "sent"}))
}

#[tauri::command]
async fn check_python_path(path: String) -> Result<bool, String> {
    let expanded = expand_tilde(&path);
    Ok(std::path::PathBuf::from(&expanded).exists())
}

#[tauri::command]
async fn detect_instances() -> Result<Vec<InstanceInfo>, String> {
    let home = dirs::home_dir().ok_or("Cannot determine home directory")?;

    let candidates: Vec<(std::path::PathBuf, String)> = vec![
        (home.join("autolycus/venv/bin/python"), "autolycus".to_string()),
        (home.join("autolycus/venv/bin/python3"), "autolycus".to_string()),
        (home.join(".autolycus/venv/bin/python"), "autolycus".to_string()),
        (home.join(".hermes/venv/bin/python"), "hermes".to_string()),
        (home.join(".hermes/hermes-agent/venv/bin/python"), "hermes-agent".to_string()),
        (std::path::PathBuf::from("/usr/local/bin/python3"), "system".to_string()),
    ];

    let mut result = Vec::new();
    for (path, instance) in candidates {
        result.push(InstanceInfo {
            path: path.to_string_lossy().to_string(),
            instance,
            exists: path.exists(),
        });
    }

    Ok(result)
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
            check_python_path,
            detect_instances,
            get_app_version,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
