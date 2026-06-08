// src-tauri/src/skills.rs
// Skills management: list, install, uninstall
// Ported from fathah/hermes-desktop src/main/skills.rs

use std::fs;
use std::path::Path;

use serde::Serialize;

use crate::config::profile_home;

// ── Types ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct InstalledSkill {
    pub name: String,
    pub category: String,
    pub description: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct BundledSkill {
    pub name: String,
    pub description: String,
    pub category: String,
    pub source: String,
    pub installed: bool,
}

// ── List installed skills ─────────────────────────────────────────────────

pub fn list_installed_skills(hermes_home: &Path, profile: Option<&str>) -> Vec<InstalledSkill> {
    let skills_dir = profile_home(hermes_home, profile).join("skills");

    if !skills_dir.exists() {
        return Vec::new();
    }

    let mut skills = Vec::new();

    if let Ok(entries) = fs::read_dir(&skills_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
            let mut description = String::new();
            let mut category = String::new();

            // Read SKILL.md for metadata
            let skill_md = path.join("SKILL.md");
            if skill_md.exists() {
                if let Ok(content) = fs::read_to_string(&skill_md) {
                    // Parse YAML frontmatter
                    if content.starts_with("---") {
                        if let Some(end) = content[3..].find("---") {
                            let frontmatter = &content[3..end + 3];
                            for line in frontmatter.lines() {
                                let line = line.trim();
                                if line.starts_with("description:") {
                                    description = line["description:".len()..].trim().trim_matches('"').trim_matches('\'').to_string();
                                } else if line.starts_with("category:") {
                                    category = line["category:".len()..].trim().trim_matches('"').trim_matches('\'').to_string();
                                }
                            }
                        }
                    }

                    // Fallback: first non-empty, non-heading line
                    if description.is_empty() {
                        for line in content.lines() {
                            let trimmed = line.trim();
                            if !trimmed.is_empty() && !trimmed.starts_with('#') && !trimmed.starts_with("---") {
                                description = trimmed.chars().take(120).collect();
                                break;
                            }
                        }
                    }
                }
            }

            skills.push(InstalledSkill {
                name,
                category,
                description,
                path: path.to_string_lossy().to_string(),
            });
        }
    }

    skills
}

// ── Get skill content ─────────────────────────────────────────────────────

pub fn get_skill_content(skill_path: &str) -> Result<String, String> {
    let path = Path::new(skill_path).join("SKILL.md");
    if !path.exists() {
        return Err("SKILL.md not found".to_string());
    }
    fs::read_to_string(&path).map_err(|e| format!("Read error: {}", e))
}

// ── Install skill (copy from bundled) ─────────────────────────────────────

pub fn install_skill(
    hermes_home: &Path,
    profile: Option<&str>,
    identifier: &str,
) -> Result<(), String> {
    let skills_dir = profile_home(hermes_home, profile).join("skills");
    fs::create_dir_all(&skills_dir).map_err(|e| format!("Create dir error: {}", e))?;

    let dest = skills_dir.join(identifier);
    if dest.exists() {
        return Err(format!("Skill '{}' already installed", identifier));
    }

    // Try to find in bundled skills
    let bundled_paths = [
        Path::new("/usr/share/autolycus/skills").join(identifier),
        hermes_home.join("..").join("skills").join(identifier),
        Path::new("skills").join(identifier),
    ];

    for src in &bundled_paths {
        if src.exists() && src.is_dir() {
            copy_dir_all(src, &dest).map_err(|e| format!("Copy error: {}", e))?;
            return Ok(());
        }
    }

    // Create minimal skill directory
    fs::create_dir_all(&dest).map_err(|e| format!("Create dir error: {}", e))?;
    let skill_md = dest.join("SKILL.md");
    fs::write(&skill_md, format!("---\nname: {}\ndescription: {}\n---\n", identifier, identifier))
        .map_err(|e| format!("Write error: {}", e))?;

    Ok(())
}

// ── Uninstall skill ───────────────────────────────────────────────────────

pub fn uninstall_skill(
    hermes_home: &Path,
    profile: Option<&str>,
    name: &str,
) -> Result<(), String> {
    let skills_dir = profile_home(hermes_home, profile).join("skills");
    let skill_path = skills_dir.join(name);

    if !skill_path.exists() {
        return Err(format!("Skill '{}' not found", name));
    }

    fs::remove_dir_all(&skill_path).map_err(|e| format!("Remove error: {}", e))?;
    Ok(())
}

// ── Helper: copy directory ────────────────────────────────────────────────

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
