// src-tauri/src/profiles.rs
// Profile management: per-profile CRUD
// Ported from fathah/hermes-desktop src/main/profiles.rs

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::config::{self, profile_home, read_env};

// ── Types ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct ProfileInfo {
    pub name: String,
    pub path: String,
    pub is_default: bool,
    pub is_active: bool,
    pub model: String,
    pub provider: String,
    pub has_env: bool,
    pub has_soul: bool,
    pub skill_count: usize,
    pub gateway_running: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileConfig {
    pub active_profile: Option<String>,
}

// ── List profiles ─────────────────────────────────────────────────────────

pub fn list_profiles(hermes_home: &Path, active_profile: Option<&str>) -> Vec<ProfileInfo> {
    let mut profiles = Vec::new();

    // Default profile
    let default_active = active_profile.unwrap_or("default");
    profiles.push(ProfileInfo {
        name: "default".to_string(),
        path: hermes_home.to_string_lossy().to_string(),
        is_default: true,
        is_active: default_active == "default",
        model: get_profile_model(hermes_home, None),
        provider: get_profile_provider(hermes_home, None),
        has_env: hermes_home.join(".env").exists(),
        has_soul: hermes_home.join("soul.md").exists(),
        skill_count: count_skills(hermes_home, None),
        gateway_running: false, // Will be set by caller
    });

    // Named profiles
    let profiles_dir = hermes_home.join("profiles");
    if profiles_dir.exists() {
        if let Ok(entries) = fs::read_dir(&profiles_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                    profiles.push(ProfileInfo {
                        name: name.clone(),
                        path: path.to_string_lossy().to_string(),
                        is_default: false,
                        is_active: default_active == name,
                        model: get_profile_model(hermes_home, Some(&name)),
                        provider: get_profile_provider(hermes_home, Some(&name)),
                        has_env: path.join(".env").exists(),
                        has_soul: path.join("soul.md").exists(),
                        skill_count: count_skills(hermes_home, Some(&name)),
                        gateway_running: false,
                    });
                }
            }
        }
    }

    profiles
}

fn get_profile_model(hermes_home: &Path, profile: Option<&str>) -> String {
    let env = read_env(hermes_home, profile);
    env.get("MODEL").cloned().unwrap_or_default()
}

fn get_profile_provider(hermes_home: &Path, profile: Option<&str>) -> String {
    let env = read_env(hermes_home, profile);
    env.get("PROVIDER").cloned().unwrap_or_default()
}

fn count_skills(hermes_home: &Path, profile: Option<&str>) -> usize {
    let skills_dir = profile_home(hermes_home, profile).join("skills");
    if !skills_dir.exists() {
        return 0;
    }
    fs::read_dir(&skills_dir)
        .map(|entries| entries.filter(|e| e.as_ref().map(|e| e.path().is_dir()).unwrap_or(false)).count())
        .unwrap_or(0)
}

// ── Create profile ────────────────────────────────────────────────────────

pub fn create_profile(hermes_home: &Path, name: &str, clone: bool) -> Result<(), String> {
    let profile_path = hermes_home.join("profiles").join(name);

    if profile_path.exists() {
        return Err(format!("Profile '{}' already exists", name));
    }

    fs::create_dir_all(&profile_path)
        .map_err(|e| format!("Failed to create profile dir: {}", e))?;

    if clone {
        // Copy from default profile
        let default_files = [".env", "config.yaml", "soul.md", "memory.md", "user.md"];
        for file in &default_files {
            let src = hermes_home.join(file);
            if src.exists() {
                let dst = profile_path.join(file);
                fs::copy(&src, &dst)
                    .map_err(|e| format!("Failed to copy {}: {}", file, e))?;
            }
        }

        // Copy skills
        let default_skills = hermes_home.join("skills");
        if default_skills.exists() {
            let profile_skills = profile_path.join("skills");
            copy_dir_all(&default_skills, &profile_skills)
                .map_err(|e| format!("Failed to copy skills: {}", e))?;
        }
    }

    Ok(())
}

fn copy_dir_all(src: &Path, dst: &Path) -> Result<(), std::io::Error> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        if path.is_dir() {
            copy_dir_all(&path, &dst.join(name))?;
        } else {
            fs::copy(&path, &dst.join(name))?;
        }
    }
    Ok(())
}

// ── Delete profile ────────────────────────────────────────────────────────

pub fn delete_profile(hermes_home: &Path, name: &str) -> Result<(), String> {
    if name == "default" {
        return Err("Cannot delete default profile".to_string());
    }

    let profile_path = hermes_home.join("profiles").join(name);

    if !profile_path.exists() {
        return Err(format!("Profile '{}' not found", name));
    }

    fs::remove_dir_all(&profile_path)
        .map_err(|e| format!("Failed to delete profile: {}", e))?;

    Ok(())
}

// ── Set active profile ────────────────────────────────────────────────────

pub fn set_active_profile(hermes_home: &Path, name: &str) -> Result<(), String> {
    let config_path = hermes_home.join("active_profile");

    if name == "default" || name.is_empty() {
        // Remove active profile file to use default
        if config_path.exists() {
            fs::remove_file(&config_path)
                .map_err(|e| format!("Failed to remove active_profile: {}", e))?;
        }
    } else {
        // Verify profile exists
        let profile_path = hermes_home.join("profiles").join(name);
        if !profile_path.exists() {
            return Err(format!("Profile '{}' not found", name));
        }

        fs::write(&config_path, name)
            .map_err(|e| format!("Failed to write active_profile: {}", e))?;
    }

    Ok(())
}

// ── Get active profile ────────────────────────────────────────────────────

pub fn get_active_profile(hermes_home: &Path) -> Option<String> {
    let config_path = hermes_home.join("active_profile");
    if config_path.exists() {
        fs::read_to_string(&config_path)
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    } else {
        None
    }
}
