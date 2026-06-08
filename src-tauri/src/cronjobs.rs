// src-tauri/src/cronjobs.rs
// Cron job management: list, create, remove, pause, resume, trigger
// Ported from fathah/hermes-desktop src/main/cronjobs.rs

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── Types ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronJob {
    pub id: String,
    pub name: String,
    pub schedule: String,
    pub prompt: String,
    pub state: String, // "active" | "paused" | "completed"
    pub enabled: bool,
    pub next_run_at: Option<String>,
    pub last_run_at: Option<String>,
    pub last_status: Option<String>,
    pub last_error: Option<String>,
    pub repeat: Option<CronRepeat>,
    pub deliver: Vec<String>,
    pub skills: Vec<String>,
    pub script: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronRepeat {
    pub times: Option<i64>,
    pub completed: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronConfigFile {
    pub jobs: HashMap<String, CronJobConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronJobConfig {
    pub schedule: String,
    pub prompt: Option<String>,
    pub enabled: Option<bool>,
    pub name: Option<String>,
    pub deliver: Option<Vec<String>>,
    pub skills: Option<Vec<String>>,
    pub script: Option<String>,
}

// ── File path ─────────────────────────────────────────────────────────────

fn cron_config_path(hermes_home: &Path, profile: Option<&str>) -> std::path::PathBuf {
    crate::config::profile_home(hermes_home, profile)
        .join(".hermes")
        .join("cron")
        .join("jobs.json")
}

// ── List cron jobs ────────────────────────────────────────────────────────

pub fn list_cron_jobs(
    hermes_home: &Path,
    profile: Option<&str>,
    include_disabled: bool,
) -> Vec<CronJob> {
    let path = cron_config_path(hermes_home, profile);

    if !path.exists() {
        return Vec::new();
    }

    let content = std::fs::read_to_string(&path).unwrap_or_default();
    let config: CronConfigFile = serde_json::from_str(&content).unwrap_or(CronConfigFile {
        jobs: HashMap::new(),
    });

    config
        .jobs
        .into_iter()
        .filter(|(_, job)| include_disabled || job.enabled != Some(false))
        .map(|(id, job)| CronJob {
            id: id.clone(),
            name: job.name.unwrap_or(id),
            schedule: job.schedule,
            prompt: job.prompt.unwrap_or_default(),
            state: "active".to_string(),
            enabled: job.enabled.unwrap_or(true),
            next_run_at: None,
            last_run_at: None,
            last_status: None,
            last_error: None,
            repeat: None,
            deliver: job.deliver.unwrap_or_default(),
            skills: job.skills.unwrap_or_default(),
            script: job.script,
        })
        .collect()
}

// ── Create cron job ────────────────────────────────────────────────────────

pub fn create_cron_job(
    hermes_home: &Path,
    profile: Option<&str>,
    schedule: &str,
    prompt: Option<&str>,
    name: Option<&str>,
    deliver: Option<&str>,
) -> Result<CronJob, String> {
    let path = cron_config_path(hermes_home, profile);

    let mut config = if path.exists() {
        let content = std::fs::read_to_string(&path)
            .map_err(|e| format!("Read error: {}", e))?;
        serde_json::from_str(&content).unwrap_or(CronConfigFile {
            jobs: HashMap::new(),
        })
    } else {
        CronConfigFile {
            jobs: HashMap::new(),
        }
    };

    let id = Uuid::new_v4().to_string();
    let job_name = name.map(|s| s.to_string()).unwrap_or_else(|| id.clone());

    let job = CronJobConfig {
        schedule: schedule.to_string(),
        prompt: prompt.map(|s| s.to_string()),
        enabled: Some(true),
        name: Some(job_name.to_string()),
        deliver: deliver.map(|d| vec![d.to_string()]),
        skills: None,
        script: None,
    };

    config.jobs.insert(id.clone(), job);

    // Ensure parent dir exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Create dir error: {}", e))?;
    }

    let json = serde_json::to_string_pretty(&config)
        .map_err(|e| format!("Serialization error: {}", e))?;
    std::fs::write(&path, json).map_err(|e| format!("Write error: {}", e))?;

    Ok(CronJob {
        id,
        name: job_name.to_string(),
        schedule: schedule.to_string(),
        prompt: prompt.unwrap_or("").to_string(),
        state: "active".to_string(),
        enabled: true,
        next_run_at: None,
        last_run_at: None,
        last_status: None,
        last_error: None,
        repeat: None,
        deliver: deliver.map(|d| vec![d.to_string()]).unwrap_or_default(),
        skills: Vec::new(),
        script: None,
    })
}

// ── Remove cron job ────────────────────────────────────────────────────────

pub fn remove_cron_job(
    hermes_home: &Path,
    profile: Option<&str>,
    job_id: &str,
) -> Result<(), String> {
    let path = cron_config_path(hermes_home, profile);

    if !path.exists() {
        return Err("Cron config not found".to_string());
    }

    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("Read error: {}", e))?;
    let mut config: CronConfigFile = serde_json::from_str(&content)
        .map_err(|e| format!("Parse error: {}", e))?;

    config.jobs.remove(job_id);

    let json = serde_json::to_string_pretty(&config)
        .map_err(|e| format!("Serialization error: {}", e))?;
    std::fs::write(&path, json).map_err(|e| format!("Write error: {}", e))?;

    Ok(())
}

// ── Pause cron job ─────────────────────────────────────────────────────────

pub fn pause_cron_job(
    hermes_home: &Path,
    profile: Option<&str>,
    job_id: &str,
) -> Result<(), String> {
    update_cron_job(hermes_home, profile, job_id, |job| {
        job.enabled = Some(false);
    })
}

// ── Resume cron job ────────────────────────────────────────────────────────

pub fn resume_cron_job(
    hermes_home: &Path,
    profile: Option<&str>,
    job_id: &str,
) -> Result<(), String> {
    update_cron_job(hermes_home, profile, job_id, |job| {
        job.enabled = Some(true);
    })
}

// ── Trigger cron job ───────────────────────────────────────────────────────

pub fn trigger_cron_job(
    _hermes_home: &Path,
    _profile: Option<&str>,
    job_id: &str,
) -> Result<String, String> {
    // In a real implementation, this would trigger the cron job via the gateway
    Ok(format!("Triggered job: {}", job_id))
}

// ── Helper: update cron job ────────────────────────────────────────────────

fn update_cron_job<F>(
    hermes_home: &Path,
    profile: Option<&str>,
    job_id: &str,
    updater: F,
) -> Result<(), String>
where
    F: FnOnce(&mut CronJobConfig),
{
    let path = cron_config_path(hermes_home, profile);

    if !path.exists() {
        return Err("Cron config not found".to_string());
    }

    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("Read error: {}", e))?;
    let mut config: CronConfigFile = serde_json::from_str(&content)
        .map_err(|e| format!("Parse error: {}", e))?;

    if let Some(job) = config.jobs.get_mut(job_id) {
        updater(job);
    } else {
        return Err(format!("Job '{}' not found", job_id));
    }

    let json = serde_json::to_string_pretty(&config)
        .map_err(|e| format!("Serialization error: {}", e))?;
    std::fs::write(&path, json).map_err(|e| format!("Write error: {}", e))?;

    Ok(())
}
