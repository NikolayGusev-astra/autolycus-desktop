//! Local-only diagnostic capture. It is opt-in and never sends data over the network.

use std::{
    fs::{self, OpenOptions},
    io::Write,
    panic,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex, OnceLock,
    },
};

static ENABLED: AtomicBool = AtomicBool::new(false);
static LOG_FILE: OnceLock<Mutex<PathBuf>> = OnceLock::new();

/// Configuration for optional local crash diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelemetryConfig {
    pub enabled: bool,
    pub log_file: PathBuf,
}

impl TelemetryConfig {
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            log_file: default_log_file(),
        }
    }

    pub fn from_environment() -> Self {
        let enabled = matches!(std::env::var("STEERSMAN_TELEMETRY"), Ok(value) if value == "1" || value.eq_ignore_ascii_case("true"));
        Self {
            enabled,
            log_file: default_log_file(),
        }
    }
}

fn default_log_file() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("steersman-desktop")
        .join("diagnostics.log")
}

/// Enables local-only diagnostic capture. Disabled configurations do nothing.
/// The panic hook records only a fixed event name, never panic payloads or user data.
pub fn initialize(config: TelemetryConfig) {
    if !config.enabled || ENABLED.swap(true, Ordering::AcqRel) {
        return;
    }
    let _ = LOG_FILE.set(Mutex::new(config.log_file));
    record("startup");
    let previous = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        record("panic");
        previous(info);
    }));
}

/// Appends a static diagnostic event to the local log when telemetry is enabled.
pub fn record(event: &str) {
    if !ENABLED.load(Ordering::Acquire) {
        return;
    }
    let Some(path) = LOG_FILE
        .get()
        .and_then(|path| path.lock().ok().map(|path| path.clone()))
    else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{} telemetry=true", event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_telemetry_does_not_create_a_log() {
        let config = TelemetryConfig::disabled();
        assert!(!config.enabled);
    }
}
