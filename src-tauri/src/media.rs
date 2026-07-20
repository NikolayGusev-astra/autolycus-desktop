// src-tauri/src/media.rs
// Media file management — read, save, cleanup.
// Ported from fathah/hermes-desktop src/main/media.ts (simplified)

use base64::Engine;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
pub struct MediaInfo {
    pub path: String,
    pub mime: String,
    pub size: u64,
}

/// MIME type by extension.
fn mime_by_ext(ext: &str) -> Option<&'static str> {
    match ext.to_lowercase().as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "svg" => Some("image/svg+xml"),
        "bmp" => Some("image/bmp"),
        "avif" => Some("image/avif"),
        "mp4" => Some("video/mp4"),
        "webm" => Some("video/webm"),
        "mp3" => Some("audio/mpeg"),
        "wav" => Some("audio/wav"),
        "pdf" => Some("application/pdf"),
        "txt" => Some("text/plain"),
        "md" => Some("text/markdown"),
        "json" => Some("application/json"),
        "yaml" | "yml" => Some("application/yaml"),
        _ => None,
    }
}

/// Get media info for a file.
pub fn get_media_info(path: &str) -> Option<MediaInfo> {
    let p = Path::new(path);
    if !p.exists() || !p.is_file() {
        return None;
    }

    let ext = p.extension()?.to_str()?;
    let mime = mime_by_ext(ext)?;
    let size = fs::metadata(p).ok()?.len();

    Some(MediaInfo {
        path: path.to_string(),
        mime: mime.to_string(),
        size,
    })
}

/// Read a file and return as base64 data URL.
pub fn read_as_data_url(path: &str) -> Option<String> {
    let info = get_media_info(path)?;
    let bytes = fs::read(path).ok()?;
    let base64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Some(format!("data:{};base64,{}", info.mime, base64))
}

/// List media files in a directory.
pub fn list_media_files(dir: &str) -> Vec<MediaInfo> {
    let p = Path::new(dir);
    if !p.exists() || !p.is_dir() {
        return Vec::new();
    }

    let mut result = Vec::new();
    if let Ok(entries) = fs::read_dir(p) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(info) = get_media_info(path.to_str().unwrap_or("")) {
                    result.push(info);
                }
            }
        }
    }

    // Sort by name
    result.sort_by(|a, b| a.path.cmp(&b.path));
    result
}

/// Get the media cache directory.
pub fn media_cache_dir(hermes_home: &Path) -> PathBuf {
    hermes_home.join("media")
}

/// Ensure the media cache directory exists.
pub fn ensure_media_dir(hermes_home: &Path) -> Result<(), String> {
    let dir = media_cache_dir(hermes_home);
    fs::create_dir_all(&dir).map_err(|e| format!("Create media dir error: {}", e))?;
    Ok(())
}

/// Persist a blob of media bytes (e.g. a recorded voice clip, or an uploaded
/// attachment) into the instance's media cache dir and return its absolute
/// path. The caller chooses the extension; we generate a unique filename.
pub fn save_media_blob(hermes_home: &Path, bytes: &[u8], ext: &str) -> Result<PathBuf, String> {
    ensure_media_dir(hermes_home)?;
    let dir = media_cache_dir(hermes_home);
    let name = format!(
        "{}-{}.{}",
        chrono::Utc::now().format("%Y%m%dT%H%M%S"),
        uuid::Uuid::new_v4().simple(),
        ext.trim_start_matches('.')
    );
    let path = dir.join(name);
    fs::write(&path, bytes).map_err(|e| format!("Write media error: {}", e))?;
    Ok(path)
}

/// MIME type for a file extension (thin wrapper around the existing map).
pub fn mime_for_ext(ext: &str) -> Option<&'static str> {
    mime_by_ext(ext)
}
