// src-tauri/src/memory.rs
// Memory management: memory.md, user.md, soul.md read/write
// Ported from fathah/hermes-desktop src/main/memory.rs

use std::fs;
use std::path::Path;

use serde::Serialize;

use crate::config::profile_home;

// ── Types ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct MemoryInfo {
    pub content: String,
    pub exists: bool,
    pub last_modified: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UserProfile {
    pub content: String,
    pub exists: bool,
    pub last_modified: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryReadResult {
    pub memory: MemoryInfo,
    pub user: UserProfile,
    pub stats: MemoryStats,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryStats {
    pub total_sessions: i64,
    pub total_messages: i64,
}

// ── Read memory ───────────────────────────────────────────────────────────

pub fn read_memory(hermes_home: &Path, profile: Option<&str>) -> MemoryReadResult {
    let home = profile_home(hermes_home, profile);

    let (mem_content, mem_exists, mem_modified) = read_file_info(&home.join("memory.md"));
    let (user_content, user_exists, user_modified) = read_file_info(&home.join("user.md"));

    MemoryReadResult {
        memory: MemoryInfo {
            content: mem_content,
            exists: mem_exists,
            last_modified: mem_modified,
        },
        user: UserProfile {
            content: user_content,
            exists: user_exists,
            last_modified: user_modified,
        },
        stats: MemoryStats {
            total_sessions: 0,
            total_messages: 0,
        },
    }
}

fn read_file_info(path: &Path) -> (String, bool, Option<u64>) {
    if !path.exists() {
        return (String::new(), false, None);
    }

    let content = fs::read_to_string(path).unwrap_or_default();
    let last_modified = fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs());

    (content, true, last_modified)
}

// ── Add memory entry ──────────────────────────────────────────────────────

pub fn add_memory_entry(
    hermes_home: &Path,
    profile: Option<&str>,
    content: &str,
) -> Result<(), String> {
    let home = profile_home(hermes_home, profile);
    let memory_path = home.join("memory.md");

    let mut existing = String::new();
    if memory_path.exists() {
        existing = fs::read_to_string(&memory_path)
            .map_err(|e| format!("Read error: {}", e))?;
    }

    let entry = if existing.is_empty() {
        format!("- {}", content)
    } else {
        format!("{}\n- {}", existing, content)
    };

    fs::write(&memory_path, entry)
        .map_err(|e| format!("Write error: {}", e))?;

    Ok(())
}

// ── Update memory entry ───────────────────────────────────────────────────

pub fn update_memory_entry(
    hermes_home: &Path,
    profile: Option<&str>,
    index: usize,
    content: &str,
) -> Result<(), String> {
    let home = profile_home(hermes_home, profile);
    let memory_path = home.join("memory.md");

    if !memory_path.exists() {
        return Err("memory.md not found".to_string());
    }

    let existing = fs::read_to_string(&memory_path)
        .map_err(|e| format!("Read error: {}", e))?;

    let mut lines: Vec<String> = existing.lines().map(|s| s.to_string()).collect();

    if index < lines.len() {
        lines[index] = format!("- {}", content);
    }

    fs::write(&memory_path, lines.join("\n"))
        .map_err(|e| format!("Write error: {}", e))?;

    Ok(())
}

// ── Remove memory entry ───────────────────────────────────────────────────

pub fn remove_memory_entry(
    hermes_home: &Path,
    profile: Option<&str>,
    index: usize,
) -> Result<(), String> {
    let home = profile_home(hermes_home, profile);
    let memory_path = home.join("memory.md");

    if !memory_path.exists() {
        return Err("memory.md not found".to_string());
    }

    let existing = fs::read_to_string(&memory_path)
        .map_err(|e| format!("Read error: {}", e))?;

    let lines: Vec<&str> = existing.lines().enumerate()
        .filter(|(i, _)| *i != index)
        .map(|(_, l)| l)
        .collect();

    fs::write(&memory_path, lines.join("\n"))
        .map_err(|e| format!("Write error: {}", e))?;

    Ok(())
}

// ── Write user profile ────────────────────────────────────────────────────

pub fn write_user_profile(
    hermes_home: &Path,
    profile: Option<&str>,
    content: &str,
) -> Result<(), String> {
    let home = profile_home(hermes_home, profile);
    fs::write(&home.join("user.md"), content)
        .map_err(|e| format!("Write error: {}", e))?;
    Ok(())
}

// ── Read soul ─────────────────────────────────────────────────────────────

pub fn read_soul(hermes_home: &Path, profile: Option<&str>) -> String {
    let home = profile_home(hermes_home, profile);
    let soul_path = home.join("soul.md");

    if !soul_path.exists() {
        return String::new();
    }

    fs::read_to_string(&soul_path).unwrap_or_default()
}

// ── Write soul ────────────────────────────────────────────────────────────

pub fn write_soul(
    hermes_home: &Path,
    profile: Option<&str>,
    content: &str,
) -> Result<(), String> {
    let home = profile_home(hermes_home, profile);
    fs::write(&home.join("soul.md"), content)
        .map_err(|e| format!("Write error: {}", e))?;
    Ok(())
}

// ── Reset soul ────────────────────────────────────────────────────────────

pub fn reset_soul(hermes_home: &Path, profile: Option<&str>) -> String {
    let home = profile_home(hermes_home, profile);
    let soul_path = home.join("soul.md");

    let default_soul = "# Soul\n\nYou are a helpful AI assistant.\n";

    if let Err(e) = fs::write(&soul_path, default_soul) {
        return format!("Failed to reset soul: {}", e);
    }

    default_soul.to_string()
}
