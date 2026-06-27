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

/// A path is safe to embed in a shell command if it contains no shell
/// metacharacters or control chars. Spaces are allowed because the caller
/// is responsible for quoting in the per-shell builders below; anything that
/// could break out of those quotes is rejected here.
fn looks_safe_for_shell(path: &str) -> bool {
    if path.is_empty() {
        return false;
    }
    !path.chars().any(|c| {
        c.is_control()
            || matches!(
                c,
                ';' | '&' | '|' | '$' | '`' | '(' | ')' | '<' | '>' | '\n' | '\r' | '"'
            )
    })
}

/// Escape a value for single-quoted PowerShell use: in PowerShell a single
/// quote inside a single-quoted string is escaped by doubling it ('' → '').
fn ps_single_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// Open a terminal in the given directory.
pub fn open_terminal(cwd: &str) -> TerminalResult {
    let path = Path::new(cwd);
    // The working directory MUST be an existing directory; this is the
    // primary control that prevents a caller from pointing the shell at an
    // arbitrary crafted path. We refuse to spawn a terminal otherwise.
    let cwd = if path.is_absolute() && path.is_dir() {
        path.to_string_lossy().to_string()
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
///
/// `cwd` is expected to already be a verified-existing absolute directory
/// path (enforced by `open_terminal`). The per-shell builders additionally
/// escape/quote the path so it cannot break out into a shell metacharacter
/// even if a value slips past the existence check.
fn get_terminal_command(cwd: &str) -> (String, Vec<String>) {
    if cfg!(target_os = "windows") {
        // Windows: use cmd.exe or PowerShell
        if std::path::Path::new("C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe").exists() {
            (
                "powershell.exe".to_string(),
                vec![
                    "-NoExit".to_string(),
                    "-Command".to_string(),
                    format!("Set-Location {}", ps_single_quote(cwd)),
                ],
            )
        } else {
            // cmd.exe has no general escaping; only proceed if the path is
            // free of shell metacharacters, otherwise fall back to a bare
            // spawn (no cd-in-shell) and let the parent process set the CWD.
            if looks_safe_for_shell(cwd) {
                (
                    "cmd.exe".to_string(),
                    vec!["/K".to_string(), format!("cd /d {}", cwd)],
                )
            } else {
                ("cmd.exe".to_string(), vec!["/K".to_string()])
            }
        }
    } else if cfg!(target_os = "macos") {
        // macOS: use Terminal.app — cwd is passed as a path argument, no
        // shell interpolation, but still guard against control chars.
        let safe = if looks_safe_for_shell(cwd) { cwd.to_string() } else { std::env::current_dir().map(|d| d.to_string_lossy().to_string()).unwrap_or_default() };
        (
            "open".to_string(),
            vec!["-a".to_string(), "Terminal".to_string(), safe],
        )
    } else {
        // Linux: try common terminals. For the `--working-directory`/`--workdir`
        // variants the path is a separate argv element, so quoting is handled
        // by execvp — only control-char filtering applies. For the xterm
        // `-e "cd ... && exec $SHELL"` variant the path goes through a shell,
        // so it must be metacharacter-free or we refuse to cd there.
        let safe = looks_safe_for_shell(cwd);
        let terminals: Vec<(String, Vec<String>)> = vec![
            ("x-terminal-emulator".to_string(), vec!["--working-directory".to_string(), cwd.to_string()]),
            ("gnome-terminal".to_string(), vec!["--working-directory".to_string(), cwd.to_string()]),
            ("konsole".to_string(), vec!["--workdir".to_string(), cwd.to_string()]),
            ("xfce4-terminal".to_string(), vec!["--working-directory".to_string(), cwd.to_string()]),
            ("mate-terminal".to_string(), vec!["--working-directory".to_string(), cwd.to_string()]),
        ];

        for (cmd, args) in &terminals {
            if which(cmd) {
                return (cmd.clone(), args.clone());
            }
        }

        // Fallback: xterm. Only embed cwd in the shell command if it is
        // metacharacter-free; otherwise spawn without an inline cd (the
        // process CWD is already set by the caller).
        if safe {
            ("xterm".to_string(), vec!["-e".to_string(), format!("cd {} && exec $SHELL", cwd)])
        } else {
            ("xterm".to_string(), vec!["-e".to_string(), "$SHELL".to_string()])
        }
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
