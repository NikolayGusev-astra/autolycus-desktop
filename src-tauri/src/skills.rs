// src-tauri/src/skills.rs
// Skills management: list, install, uninstall
// Ported from fathah/hermes-desktop src/main/skills.rs

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::config::profile_home;

// ── Path safety ──────────────────────────────────────────────────────────

/// Reject identifiers that could escape the skills directory.
/// A skill identifier must be a single path component (no separators,
/// no parent traversal) and must not be a reserved name.
fn validate_skill_identifier(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Skill name must not be empty".to_string());
    }
    if name.contains('/') || name.contains('\\') {
        return Err(format!(
            "Skill name must not contain path separators: '{}'",
            name
        ));
    }
    if name == "." || name == ".." {
        return Err(format!("Invalid skill name: '{}'", name));
    }
    if name.contains('\0') {
        return Err("Skill name must not contain NUL".to_string());
    }
    Ok(())
}

/// Canonicalize a path and assert it remains inside `base`.
/// Defends against symlinks and traversal: the resolved real path
/// must start with the canonicalized base.
fn ensure_inside(base: &Path, target: &Path) -> Result<PathBuf, String> {
    // Materialize the candidate without requiring it to exist yet:
    // join the base with the normalized target. We canonicalize the base
    // (which must exist) and check the lexical containment of target.
    let canon_base = base
        .canonicalize()
        .map_err(|e| format!("Cannot canonicalize base {}: {}", base.display(), e))?;

    // If the target already exists, canonicalize it and compare real paths.
    if let Ok(canon_target) = target.canonicalize() {
        if canon_target.starts_with(&canon_base) {
            return Ok(canon_target);
        }
        return Err(format!(
            "Path '{}' escapes the skills directory",
            target.display()
        ));
    }

    // Target doesn't exist yet (install path). Walk up to the first existing
    // ancestor, canonicalize that, and verify the chain stays within base.
    let mut ancestor = target.to_path_buf();
    let mut suffix: Vec<PathBuf> = Vec::new();
    loop {
        if ancestor.exists() {
            break;
        }
        if ancestor.parent().is_none() {
            return Err("Cannot resolve skill path".to_string());
        }
        let next = ancestor.parent().unwrap().to_path_buf();
        let name = ancestor
            .strip_prefix(&next)
            .map_err(|_| "path error".to_string())?
            .to_path_buf();
        suffix.push(name);
        ancestor = next;
    }
    let canon_ancestor = ancestor
        .canonicalize()
        .map_err(|e| format!("Cannot canonicalize path: {}", e))?;
    if !canon_ancestor.starts_with(&canon_base) {
        return Err(format!(
            "Path '{}' escapes the skills directory",
            target.display()
        ));
    }
    let mut resolved = canon_ancestor;
    for part in suffix.into_iter().rev() {
        resolved = resolved.join(part);
    }
    Ok(resolved)
}

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

            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            // Skip hidden (dot-prefixed) and known-junk directories that are not
            // skills: .curator_backups, .git, ._apple macOS resource-fork
            // artifacts, node_modules, etc. Previously these were listed as
            // skills, leading to "SKILL.md not found" on click.
            if name.starts_with('.')
                || matches!(
                    name.as_str(),
                    "node_modules" | "__pycache__" | "venv" | ".venv"
                )
            {
                continue;
            }

            // Only treat a directory as a skill if it has a SKILL.md marker.
            // This is the single fix for the "SKILL.md not found" error.
            let skill_md = path.join("SKILL.md");
            if !skill_md.exists() {
                continue;
            }

            let mut description = String::new();
            let mut category = String::new();

            // Read SKILL.md for metadata
            if let Ok(content) = fs::read_to_string(&skill_md) {
                // Parse YAML frontmatter
                if content.starts_with("---") {
                    if let Some(end) = content[3..].find("---") {
                        let frontmatter = &content[3..end + 3];
                        for line in frontmatter.lines() {
                            let line = line.trim();
                            if line.starts_with("description:") {
                                description = line["description:".len()..]
                                    .trim()
                                    .trim_matches('"')
                                    .trim_matches('\'')
                                    .to_string();
                            } else if line.starts_with("category:") {
                                category = line["category:".len()..]
                                    .trim()
                                    .trim_matches('"')
                                    .trim_matches('\'')
                                    .to_string();
                            }
                        }
                    }
                }

                // Fallback: first non-empty, non-heading line
                if description.is_empty() {
                    for line in content.lines() {
                        let trimmed = line.trim();
                        if !trimmed.is_empty()
                            && !trimmed.starts_with('#')
                            && !trimmed.starts_with("---")
                        {
                            description = trimmed.chars().take(120).collect();
                            break;
                        }
                    }
                }
            }

            // Category fallback: use the skill name if none declared.
            if category.is_empty() {
                category = "uncategorized".to_string();
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

/// Read a skill's SKILL.md. The skill is addressed by name and profile so
/// the caller can never read an arbitrary filesystem path.
pub fn get_skill_content(
    hermes_home: &Path,
    profile: Option<&str>,
    skill_name: &str,
) -> Result<String, String> {
    validate_skill_identifier(skill_name)?;
    let skills_dir = profile_home(hermes_home, profile).join("skills");
    let skill_path = skills_dir.join(skill_name);
    let safe = ensure_inside(&skills_dir, &skill_path)?;
    let md = safe.join("SKILL.md");
    if !md.exists() {
        return Err("SKILL.md not found".to_string());
    }
    fs::read_to_string(&md).map_err(|e| format!("Read error: {}", e))
}

// ── Install skill (copy from bundled) ─────────────────────────────────────

pub fn install_skill(
    hermes_home: &Path,
    profile: Option<&str>,
    identifier: &str,
) -> Result<(), String> {
    validate_skill_identifier(identifier)?;

    let skills_dir = profile_home(hermes_home, profile).join("skills");
    fs::create_dir_all(&skills_dir).map_err(|e| format!("Create dir error: {}", e))?;

    let dest = skills_dir.join(identifier);
    if dest.exists() {
        return Err(format!("Skill '{}' already installed", identifier));
    }

    // Verify destination stays within the skills directory (defense against
    // symlinks created on the parent chain after install).
    let safe_dest = ensure_inside(&skills_dir, &dest)?;
    if safe_dest != dest {
        return Err(format!("Resolved skill path escapes the skills directory"));
    }

    // Try to find in bundled skills
    let bundled_paths = [
        Path::new("/usr/share/steersman/skills").join(identifier),
        hermes_home.join("..").join("skills").join(identifier),
        Path::new("skills").join(identifier),
    ];

    for src in &bundled_paths {
        if src.exists() && src.is_dir() {
            copy_dir_all(src, &safe_dest).map_err(|e| format!("Copy error: {}", e))?;
            return Ok(());
        }
    }

    // Create minimal skill directory
    fs::create_dir_all(&safe_dest).map_err(|e| format!("Create dir error: {}", e))?;
    let skill_md = safe_dest.join("SKILL.md");
    fs::write(
        &skill_md,
        format!(
            "---\nname: {}\ndescription: {}\n---\n",
            identifier, identifier
        ),
    )
    .map_err(|e| format!("Write error: {}", e))?;

    Ok(())
}

// ── Uninstall skill ───────────────────────────────────────────────────────

pub fn uninstall_skill(
    hermes_home: &Path,
    profile: Option<&str>,
    name: &str,
) -> Result<(), String> {
    validate_skill_identifier(name)?;
    let skills_dir = profile_home(hermes_home, profile).join("skills");
    let skill_path = skills_dir.join(name);

    // Resolve the real path and confirm it is inside the skills directory.
    // Blocks traversal and symlink-based escapes before the destructive
    // remove_dir_all.
    let safe = ensure_inside(&skills_dir, &skill_path)?;
    if !safe.exists() {
        return Err(format!("Skill '{}' not found", name));
    }

    fs::remove_dir_all(&safe).map_err(|e| format!("Remove error: {}", e))?;
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

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_identifiers() {
        assert!(validate_skill_identifier("my-skill").is_ok());
        assert!(validate_skill_identifier("My_Skill.1").is_ok());
        assert!(validate_skill_identifier("a").is_ok());
    }

    #[test]
    fn rejects_traversal_in_identifier() {
        assert!(validate_skill_identifier("../etc").is_err());
        assert!(validate_skill_identifier("a/b").is_err());
        assert!(validate_skill_identifier("a\\b").is_err());
        assert!(validate_skill_identifier(".").is_err());
        assert!(validate_skill_identifier("..").is_err());
    }

    #[test]
    fn rejects_empty_and_nul() {
        assert!(validate_skill_identifier("").is_err());
        assert!(validate_skill_identifier("a\0b").is_err());
    }

    #[test]
    fn ensure_inside_allows_child() {
        let tmp = std::env::temp_dir().join(format!("skills-test-{}", std::process::id()));
        let skills = tmp.join("skills");
        fs::create_dir_all(skills.join("real")).unwrap();
        let target = skills.join("real");
        assert!(ensure_inside(&skills, &target).is_ok());
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn ensure_inside_rejects_escape() {
        let tmp = std::env::temp_dir().join(format!("skills-test-esc-{}", std::process::id()));
        let skills = tmp.join("skills");
        fs::create_dir_all(&skills).unwrap();
        // ../etc resolves outside the skills directory
        let target = skills.join("..").join("etc");
        assert!(ensure_inside(&skills, &target).is_err());
        let _ = fs::remove_dir_all(&tmp);
    }
}
