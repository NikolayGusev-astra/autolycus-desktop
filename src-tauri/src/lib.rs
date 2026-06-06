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
fn start_backend(state: State<AppState>, app_handle: tauri::AppHandle) -> Result<u16, String> {
    if state.backend_process.lock().unwrap().is_some() {
        return Ok(*state.backend_port.lock().unwrap());
    }

    // Try to find Python backend
    // Priority: bundled > venv > system PATH
    let python_backend = find_python_backend(&app_handle)?;

    let mut child = Command::new(&python_backend)
        .arg("--mode=websocket")
        .arg("--port=0")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to start backend: {}", e))?;

    // Read WS_READY:PORT from stdout
    let stdout = child.stdout.take().ok_or("No stdout")?;
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    reader.read_line(&mut line).map_err(|e| format!("Read error: {}", e))?;

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

fn find_python_backend(app_handle: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    // 1. Check bundled resource dir (for AppImage)
    if let Ok(resource_dir) = app_handle.path().resource_dir() {
        let bundled = resource_dir.join("bin").join("autolycus-backend");
        if bundled.exists() {
            return Ok(bundled);
        }
    }

    // 2. Check common venv locations
    let home = std::env::var("HOME").map_err(|e| format!("No HOME: {}", e))?;
    let venv_paths = [
        format!("{}/autolycus/venv/bin/autolycus", home),
        format!("{}/autolycus/venv/bin/python", home),
        format!("{}/.autolycus/venv/bin/autolycus", home),
    ];

    for path in &venv_paths {
        let p = std::path::PathBuf::from(path);
        if p.exists() {
            return Ok(p);
        }
    }

    // 3. Check system PATH
    if let Ok(output) = Command::new("which").arg("autolycus").output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Ok(std::path::PathBuf::from(path));
            }
        }
    }

    // 4. Fallback: try python -m tui_gateway
    if let Ok(output) = Command::new("which").arg("python3").output() {
        if output.status.success() {
            return Ok(std::path::PathBuf::from("python3"));
        }
    }

    Err("Python backend not found. Install: pip install autolycus".to_string())
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
