// src-tauri/src/stt.rs
// Speech-to-text for on-the-fly voice transcription in the chat input.
//
// Mirrors the approach proven on kanban.gen-ii.ru: Groq's Whisper-Large-V3
// endpoint (OpenAI-compatible, ~50x real-time, free tier) is the primary
// transcriber, with OpenAI Whisper as a fallback. Audio is sent as multipart
// form data. The result text is returned to the frontend and inserted into the
// input box — so the user sees the recognized text immediately, like shturman.ai.

use std::path::Path;

use crate::config;

/// Transcribe an audio file to text.
///
/// Provider priority:
///   1. Groq (`GROQ_API_KEY`) — fast, free; whisper-large-v3
///   2. OpenAI (`OPENAI_API_KEY`) — whisper-1
///
/// `audio_path` points at a file the desktop saved (webm/wav/mp3…). Groq/OpenAI
/// accept most common formats.
pub async fn transcribe_audio(
    hermes_home: &Path,
    audio_path: &str,
) -> Result<String, String> {
    let env = config::read_env(hermes_home, None);

    // Prefer Groq if a key is present.
    if let Some(key) = env.get("GROQ_API_KEY").filter(|k| !k.is_empty()) {
        return transcribe_via_groq(audio_path, key).await;
    }
    if let Some(key) = env.get("OPENAI_API_KEY").filter(|k| !k.is_empty()) {
        return transcribe_via_openai(audio_path, key).await;
    }
    Err(
        "No STT provider configured. Set GROQ_API_KEY (recommended, free) or OPENAI_API_KEY in the agent .env."
            .to_string(),
    )
}

async fn transcribe_via_groq(audio_path: &str, api_key: &str) -> Result<String, String> {
    transcribe_openai_compat(
        "https://api.groq.com/openai/v1/audio/transcriptions",
        "whisper-large-v3",
        audio_path,
        api_key,
    )
    .await
}

async fn transcribe_via_openai(audio_path: &str, api_key: &str) -> Result<String, String> {
    transcribe_openai_compat(
        "https://api.openai.com/v1/audio/transcriptions",
        "whisper-1",
        audio_path,
        api_key,
    )
    .await
}

/// POST a multipart audio file to an OpenAI-compatible /audio/transcriptions
/// endpoint and return the transcript text. Uses a hand-built multipart body
/// (no extra multipart dependency).
async fn transcribe_openai_compat(
    url: &str,
    model: &str,
    audio_path: &str,
    api_key: &str,
) -> Result<String, String> {
    let bytes = std::fs::read(audio_path).map_err(|e| format!("Read audio error: {}", e))?;
    let filename = std::path::Path::new(audio_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("audio.webm")
        .to_string();
    // Guess a content type from the extension.
    let ext = std::path::Path::new(&filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("webm")
        .to_lowercase();
    let mime = match ext.as_str() {
        "wav" => "audio/wav",
        "mp3" => "audio/mpeg",
        "m4a" => "audio/mp4",
        "ogg" => "audio/ogg",
        _ => "audio/webm",
    };

    let boundary = "----shturman-stt-boundary-7d3e9f1a";
    let body = build_multipart(boundary, &bytes, &filename, mime, model);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let resp = client
        .post(url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header(
            "Content-Type",
            format!("multipart/form-data; boundary={}", boundary),
        )
        .body(body)
        .send()
        .await
        .map_err(|e| format!("STT request failed: {}", e))?;

    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(format!("STT HTTP {}: {}", status.as_u16(), text));
    }

    // Response shape: { "text": "..." }
    let v: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("Parse STT response error: {}", e))?;
    v.get("text")
        .and_then(|t| t.as_str())
        .map(|s| s.trim().to_string())
        .ok_or_else(|| format!("STT response missing 'text': {}", text))
}

/// Build a minimal multipart/form-data body carrying one file field ("file")
/// and a "model" field.
fn build_multipart(
    boundary: &str,
    bytes: &[u8],
    filename: &str,
    mime: &str,
    model: &str,
) -> Vec<u8> {
    let mut out = Vec::new();
    let push = |out: &mut Vec<u8>, s: &str| out.extend_from_slice(s.as_bytes());

    // model field
    push(&mut out, &format!("--{}\r\n", boundary));
    push(&mut out, "Content-Disposition: form-data; name=\"model\"\r\n\r\n");
    push(&mut out, model);
    push(&mut out, "\r\n");

    // file field
    push(&mut out, &format!("--{}\r\n", boundary));
    push(
        &mut out,
        &format!(
            "Content-Disposition: form-data; name=\"file\"; filename=\"{}\"\r\n",
            filename
        ),
    );
    push(&mut out, &format!("Content-Type: {}\r\n\r\n", mime));
    out.extend_from_slice(bytes);
    push(&mut out, "\r\n");

    // closing boundary
    push(&mut out, &format!("--{}--\r\n", boundary));
    out
}

/// Tauri command wrapper: transcribe a saved audio clip to text.
#[tauri::command]
pub async fn transcribe_audio_cmd(
    state: tauri::State<'_, crate::AppState>,
    audio_path: String,
) -> Result<String, String> {
    let hermes_home = state.hermes_home()?;
    transcribe_audio(&hermes_home, &audio_path).await
}
