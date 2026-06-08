// src-tauri/src/models.rs
// Model management: models.json CRUD
// Ported from fathah/hermes-desktop src/main/models.rs

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

// ── Types ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedModel {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub model: String,
    pub base_url: String,
    pub api_mode: Option<String>,
    pub created_at: i64,
}

// ── Models file path ──────────────────────────────────────────────────────

fn models_file_path(hermes_home: &Path) -> std::path::PathBuf {
    hermes_home.join("models.json")
}

// ── Read models ───────────────────────────────────────────────────────────

pub fn list_models(hermes_home: &Path) -> Vec<SavedModel> {
    let path = models_file_path(hermes_home);
    if !path.exists() {
        return Vec::new();
    }

    match fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

// ── Add model ─────────────────────────────────────────────────────────────

pub fn add_model(
    hermes_home: &Path,
    name: &str,
    provider: &str,
    model: &str,
    base_url: &str,
) -> Result<SavedModel, String> {
    let mut models = list_models(hermes_home);

    let new_model = SavedModel {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.to_string(),
        provider: provider.to_string(),
        model: model.to_string(),
        base_url: base_url.to_string(),
        api_mode: None,
        created_at: chrono::Utc::now().timestamp(),
    };

    models.push(new_model.clone());
    write_models(hermes_home, &models)?;

    Ok(new_model)
}

// ── Remove model ──────────────────────────────────────────────────────────

pub fn remove_model(hermes_home: &Path, id: &str) -> Result<bool, String> {
    let mut models = list_models(hermes_home);
    let len_before = models.len();
    models.retain(|m| m.id != id);

    if models.len() == len_before {
        return Ok(false); // Not found
    }

    write_models(hermes_home, &models)?;
    Ok(true)
}

// ── Update model ──────────────────────────────────────────────────────────

pub fn update_model(
    hermes_home: &Path,
    id: &str,
    fields: &std::collections::HashMap<String, String>,
) -> Result<bool, String> {
    let mut models = list_models(hermes_home);

    for model in &mut models {
        if model.id == id {
            if let Some(name) = fields.get("name") {
                model.name = name.clone();
            }
            if let Some(provider) = fields.get("provider") {
                model.provider = provider.clone();
            }
            if let Some(m) = fields.get("model") {
                model.model = m.clone();
            }
            if let Some(base_url) = fields.get("base_url") {
                model.base_url = base_url.clone();
            }
            if let Some(api_mode) = fields.get("api_mode") {
                model.api_mode = Some(api_mode.clone());
            }

            write_models(hermes_home, &models)?;
            return Ok(true);
        }
    }

    Ok(false)
}

// ── Write models ──────────────────────────────────────────────────────────

fn write_models(hermes_home: &Path, models: &[SavedModel]) -> Result<(), String> {
    let path = models_file_path(hermes_home);
    let json = serde_json::to_string_pretty(models)
        .map_err(|e| format!("Serialization error: {}", e))?;
    fs::write(&path, json).map_err(|e| format!("Write error: {}", e))?;
    Ok(())
}
