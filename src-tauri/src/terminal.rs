// src-tauri/src/terminal.rs
// Terminal launcher — open system terminal in a directory.
// Ported from fathah/hermes-desktop src/main/terminal-launcher.ts (simplified)

use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub struct TerminalCommand {
    pub command: String,
    pub args: Vec<String>,
    pub cwd: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TerminalResult {
    pub success: bool,
    pub error: Option<String>,
}

/// Open a terminal in the given directory.
pub fn open_terminal(cwd: &str) -> TerminalResult {
    let path = Path::new(cwd);
    let cwd = if path.exists() && path.is_dir() {
        cwd.to_string()
    } else {
        match std::env::current_dir() {
            Ok(d) => d.to_string_lossy().to_string(),
            Err(_) => return TerminalResult {
                success: false,
                error: Some("Cannot determine current directory".to_string()),
            },
        }
    };

    let (command, args) = get_terminal_command(&cwd);

    match std::process::Command::new(&command)
        .args(&args)
        .current_dir(&cwd)
        .spawn()
    {
        Ok(_) => TerminalResult {
            success: true,
            error: None,
        },
        Err(e) => TerminalResult {
            success: false,
            error: Some(format!("Failed to spawn terminal '{}': {}", command, e)),
        },
    }
}

/// Get the terminal command for the current platform.
fn get_terminal_command(cwd: &str) -> (String, Vec<String>) {
    if cfg!(target_os = "windows") {
        // Windows: use cmd.exe or PowerShell
        if std::path::Path::new("C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe").exists() {
            (
                "powershell.exe".to_string(),
                vec![
                    "-NoExit".to_string(),
                    "-Command".to_string(),
                    format!("Set-Location '{}'", cwd),
                ],
            )
        } else {
            (
                "cmd.exe".to_string(),
                vec!["/K".to_string(), format!("cd /d {}", cwd)],
            )
        }
    } else if cfg!(target_os = "macos") {
        // macOS: use Terminal.app
        (
            "open".to_string(),
            vec!["-a".to_string(), "Terminal".to_string(), cwd.to_string()],
        )
    } else {
        // Linux: try common terminals
        let terminals: Vec<(String, Vec<String>)> = vec![
            ("x-terminal-emulator".to_string(), vec!["--working-directory".to_string(), cwd.to_string()]),
            ("gnome-terminal".to_string(), vec!["--working-directory".to_string(), cwd.to_string()]),
            ("konsole".to_string(), vec!["--workdir".to_string(), cwd.to_string()]),
            ("xfce4-terminal".to_string(), vec!["--working-directory".to_string(), cwd.to_string()]),
            ("mate-terminal".to_string(), vec!["--working-directory".to_string(), cwd.to_string()]),
            ("xterm".to_string(), vec!["-e".to_string(), format!("cd {} && exec $SHELL", cwd)]),
        ];

        for (cmd, args) in &terminals {
            if which(cmd) {
                return (cmd.clone(), args.clone());
            }
        }

        // Fallback: xterm
        ("xterm".to_string(), vec!["-e".to_string(), format!("cd {} && exec $SHELL", cwd)])
    }
}

/// Check if a command exists in PATH.
fn which(cmd: &str) -> bool {
    std::process::Command::new("which")
        .arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}
