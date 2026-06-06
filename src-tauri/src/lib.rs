use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use tauri::{Manager, State};

struct AppState {
    backend_process: Mutex<Option<std::process::Child>>,
    backend_port: Mutex<u16>,
}

#[tauri::command]
fn get_backend_port(state: State<AppState>) -> u16 {
    *state.backend_port.lock().unwrap()
}

#[tauri::command]
fn start_backend(
    state: State<AppState>,
    app_handle: tauri::AppHandle,
    python_path: Option<String>,
) -> Result<u16, String> {
    if state.backend_process.lock().unwrap().is_some() {
        return Ok(*state.backend_port.lock().unwrap());
    }

    // Find the ws_server.py script
    let ws_server = find_ws_server(&app_handle)?;

    // Find Python interpreter
    let python = python_path
        .map(std::path::PathBuf::from)
        .or_else(|| find_python())
        .ok_or("Python not found. Install Python 3.11+ or specify path in settings.")?;

    let mut child = Command::new(&python)
        .arg(&ws_server)
        .arg("--port=0")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to start backend: {}", e))?;

    // Read WS_READY:PORT from stdout
    let stdout = child.stdout.take().ok_or("No stdout")?;
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|e| format!("Read error: {}", e))?;

    let port: u16 = line
        .trim()
        .strip_prefix("WS_READY:")
        .ok_or("Invalid WS_READY format")?
        .parse()
        .map_err(|e| format!("Invalid port: {}", e))?;

    *state.backend_process.lock().unwrap() = Some(child);
    *state.backend_port.lock().unwrap() = port;

    Ok(port)
}

fn find_ws_server(_app_handle: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    // Check common locations relative to the executable
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()));

    let search_paths = [
        // Bundled with AppImage
        exe_dir
            .as_ref()
            .map(|d| d.join("tui_gateway").join("ws_server.py")),
        // Development: autolycus repo
        dirs::home_dir().map(|h| {
            h.join("autolycus")
                .join("tui_gateway")
                .join("ws_server.py")
        }),
        dirs::home_dir().map(|h| {
            h.join(".autolycus")
                .join("tui_gateway")
                .join("ws_server.py")
        }),
        // System-wide
        Some(std::path::PathBuf::from(
            "/usr/local/lib/autolycus/tui_gateway/ws_server.py",
        )),
    ];

    for path in search_paths.iter().flatten() {
        if path.exists() {
            return Ok(path.clone());
        }
    }

    Err("ws_server.py not found. Ensure autolycus is installed.".to_string())
}

fn find_python() -> Option<std::path::PathBuf> {
    // Check common venv locations
    let home = std::env::var("HOME").ok()?;
    let candidates = [
        format!("{}/autolycus/venv/bin/python", home),
        format!("{}/autolycus/venv/bin/python3", home),
        format!("{}/.autolycus/venv/bin/python", home),
        format!("{}/.hermes/venv/bin/python", home),
    ];

    for path in &candidates {
        let p = std::path::PathBuf::from(path);
        if p.exists() {
            return Some(p);
        }
    }

    // Check system PATH
    if let Ok(output) = Command::new("which").arg("python3").output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Some(std::path::PathBuf::from(path));
            }
        }
    }

    None
}

#[tauri::command]
fn stop_backend(state: State<AppState>) -> Result<(), String> {
    if let Some(mut child) = state.backend_process.lock().unwrap().take() {
        let _ = child.kill();
    }
    *state.backend_port.lock().unwrap() = 0;
    Ok(())
}

#[tauri::command]
fn get_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState {
            backend_process: Mutex::new(None),
            backend_port: Mutex::new(0),
        })
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            start_backend,
            stop_backend,
            get_backend_port,
            get_app_version,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
