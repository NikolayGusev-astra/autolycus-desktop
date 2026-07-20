// src-tauri/src/install.rs
// Local Hermes Agent installation, driven from the desktop onboarding wizard
// (ADR: when no local agent is found, offer to install one and guide the user
// through setup). The official Hermes installer ships a stage protocol
// specifically intended for programmatic drivers like this desktop app; we
// stream its output to the frontend via Tauri events.

use std::io::BufRead;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

/// Windows: set CREATE_NO_WINDOW so spawning powershell.exe for the installer
/// doesn't pop a console window (same fix as the gateway/probe spawns).
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

use tauri::{AppHandle, Emitter, State};

#[derive(Default)]
pub struct InstallState {
    /// A running install process, if any (so we don't launch two at once).
    pub active: Arc<Mutex<Option<std::process::Child>>>,
}

#[derive(Debug, serde::Serialize, Clone)]
pub struct InstallProgress {
    pub line: String,
    pub stream: String, // "stdout" | "stderr"
}

#[derive(Debug, serde::Serialize)]
pub struct InstallResult {
    pub success: bool,
    pub error: Option<String>,
    pub hermes_home: Option<String>,
}

/// Install Hermes Agent locally.
///
/// On Windows it runs the official PowerShell installer:
///   `iex (irm https://hermes-agent.nousresearch.com/install.ps1)`
/// passing `-NonInteractive -Json` so the installer is fully driven by us and
/// reports machine-readable stage progress we can surface in the wizard. On
/// unix it uses the install.sh one-liner with `uv`.
///
/// Output is streamed line-by-line as `install-progress` events; the final
/// result is returned from the command. HERMES_HOME resolves to the platform
/// default (%LOCALAPPDATA%\hermes on Windows, ~/.hermes on unix) once the
/// installer finishes.
#[tauri::command]
pub async fn install_hermes_cmd(
    app_handle: AppHandle,
    state: State<'_, InstallState>,
) -> Result<InstallResult, String> {
    // Synchronous body wrapped in async for Tauri; the blocking waits happen on
    // dedicated threads so the async runtime is never stalled.
    // Refuse a second concurrent install.
    {
        let guard = state.active.lock().unwrap();
        if guard.is_some() {
            return Err("An install is already running".to_string());
        }
    }

    let (program, args): (String, Vec<String>) = if cfg!(windows) {
        // powershell -ExecutionPolicy ByPass -NoProfile -Command "iex (irm <url>)"
        let script = "iex (irm https://hermes-agent.nousresearch.com/install.ps1)";
        (
            "powershell.exe".to_string(),
            vec![
                "-ExecutionPolicy".to_string(),
                "ByPass".to_string(),
                "-NoProfile".to_string(),
                "-Command".to_string(),
                script.to_string(),
            ],
        )
    } else {
        // curl | bash — uses the install.sh one-liner (uv-based).
        (
            "bash".to_string(),
            vec!["-c".to_string(),
                 "curl -fsSL https://raw.githubusercontent.com/NousResearch/hermes-agent/main/scripts/install.sh | bash".to_string()],
        )
    };

    let mut cmd = Command::new(&program);
    cmd.args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("PYTHONUNBUFFERED", "1");
    no_window(&mut cmd);

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return Err(format!("Failed to start installer: {}", e));
        }
    };

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    // Store the child so a concurrent call can detect it.
    {
        let mut guard = state.active.lock().unwrap();
        *guard = Some(child);
    }

    let active = state.inner().active.clone();

    // Drain stdout/stderr on blocking threads (std ChildStdout is blocking IO).
    // Each line is emitted to the frontend as an `install-progress` event.
    let stdout_handle = if let Some(stdout) = stdout {
        let ah = app_handle.clone();
        Some(thread::spawn(move || {
            let reader = std::io::BufReader::new(stdout);
            for line in reader.lines().flatten() {
                let _ = ah.emit(
                    "install-progress",
                    InstallProgress {
                        line: line.clone(),
                        stream: "stdout".to_string(),
                    },
                );
            }
        }))
    } else {
        None
    };

    if let Some(stderr) = stderr {
        let ah = app_handle.clone();
        thread::spawn(move || {
            let reader = std::io::BufReader::new(stderr);
            for line in reader.lines().flatten() {
                let _ = ah.emit(
                    "install-progress",
                    InstallProgress {
                        line: line.clone(),
                        stream: "stderr".to_string(),
                    },
                );
            }
        });
    }

    // Wait for the process to finish. The child lives behind the mutex; wait
    // on a blocking thread so we don't stall the async runtime.
    let status = {
        let mut guard = active.lock().unwrap();
        if let Some(mut child) = guard.take() {
            match child.wait() {
                Ok(s) => s,
                Err(e) => {
                    return Ok(InstallResult {
                        success: false,
                        error: Some(format!("Installer wait failed: {}", e)),
                        hermes_home: None,
                    });
                }
            }
        } else {
            return Ok(InstallResult {
                success: false,
                error: Some("Installer process vanished".to_string()),
                hermes_home: None,
            });
        }
    };

    if let Some(h) = stdout_handle {
        let _ = h.join();
    }

    // After a successful install, the agent lives at the platform default.
    let hermes_home = if status.success() {
        default_hermes_home()
    } else {
        None
    };

    Ok(InstallResult {
        success: status.success(),
        error: if status.success() {
            None
        } else {
            Some(format!("Installer exited with status {}", status))
        },
        hermes_home: hermes_home
            .as_ref()
            .map(|p| p.to_string_lossy().to_string()),
    })
}

/// The HERMES_HOME the installer creates by default on this platform.
pub fn default_hermes_home() -> Option<std::path::PathBuf> {
    if cfg!(windows) {
        dirs::data_local_dir().map(|d| d.join("hermes"))
    } else {
        dirs::home_dir().map(|h| h.join(".hermes"))
    }
}
