use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use tauri::State;

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

    // In production, the Python backend is bundled as part of the app
    // For dev, we assume it's available on the system PATH
    let python_backend = app_handle
        .path()
        .resource_dir()
        .map(|d| d.join("bin").join("autolycus-backend"))
        .filter(|p| p.exists())
        .unwrap_or_else(|_| std::path::PathBuf::from("autolycus-backend"));

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
