// src-tauri/src/chat.rs
// Chat streaming: sendMessage with SSE, API fallback, session management
// Ported from fathah/hermes-desktop src/main/hermes.ts (chat part)

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::ChildStdout;
use std::thread;

use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter};

use crate::config::{self, ModelConfig, SshConfig};
use crate::gateway::{self, GatewayState};
use crate::ssh::SshState;

// ── Chat Event (unified for all modes) ────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum ChatEvent {
    #[serde(rename = "token")]
    Token { content: String },
    #[serde(rename = "reasoning")]
    Reasoning { content: String },
    #[serde(rename = "tool_start")]
    ToolStart {
        name: String,
        tool_call_id: String,
    },
    #[serde(rename = "tool_complete")]
    ToolComplete {
        name: String,
        tool_call_id: String,
        output: String,
        duration_ms: u64,
    },
    #[serde(rename = "approval_request")]
    ApprovalRequest {
        request_id: String,
        tool_name: String,
        tool_input: String,
        action: String,
        command_class: String,
    },
    #[serde(rename = "pipeline_status")]
    PipelineStatus {
        backend: String,
        model: Option<String>,
        tokens_used: Option<u64>,
        tokens_limit: Option<u64>,
        cost_usd: Option<f64>,
    },
    #[serde(rename = "done")]
    Done {
        session_id: Option<String>,
    },
    #[serde(rename = "error")]
    Error { message: String },
    #[serde(rename = "status")]
    Status { status: String },
}

// ── Connection mode ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConnectionMode {
    #[serde(rename = "local")]
    Local,
    #[serde(rename = "remote")]
    Remote,
    #[serde(rename = "ssh")]
    Ssh,
}

// ── Send message request ─────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct SendMessageRequest {
    pub text: String,
    pub session_id: Option<String>,
    pub history: Option<Vec<HistoryItem>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HistoryItem {
    pub role: String,
    pub content: String,
}

// ── SSE Parser ────────────────────────────────────────────────────────────

pub struct SseParser {
    pub has_content: bool,
    pub last_error: String,
}

impl SseParser {
    pub fn new() -> Self {
        Self {
            has_content: false,
            last_error: String::new(),
        }
    }

    /// Process a single SSE data line
    pub fn process_data(&mut self, data: &str) -> Option<ChatEvent> {
        if data == "[DONE]" {
            return Some(ChatEvent::Done { session_id: None });
        }

        let parsed: Value = match serde_json::from_str(data) {
            Ok(v) => v,
            Err(_) => return None,
        };

        // Check for error
        if let Some(err) = parsed.get("error") {
            let msg = err
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown error");
            self.last_error = msg.to_string();
            return Some(ChatEvent::Error {
                message: msg.to_string(),
            });
        }

        // Extract delta
        let delta = parsed.get("choices").and_then(|c| c.get(0)).and_then(|c| c.get("delta"));

        // Extract usage
        if let Some(usage) = parsed.get("usage") {
            // Usage is typically in the final chunk
        }

        if let Some(delta) = delta {
            if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                if !content.is_empty() {
                    self.has_content = true;
                    return Some(ChatEvent::Token {
                        content: content.to_string(),
                    });
                }
            }
        }

        None
    }

    /// Parse a full SSE block (may contain event: and data: lines)
    pub fn parse_block(block: &str) -> Option<(String, String)> {
        let mut event_type = String::new();
        let mut data_line = String::new();

        for line in block.lines() {
            if line.starts_with("event: ") {
                event_type = line[7..].trim().to_string();
            } else if line.starts_with("data: ") {
                data_line = line[6..].to_string();
            }
        }

        if data_line.is_empty() {
            None
        } else {
            Some((event_type, data_line))
        }
    }
}

// ── API-based chat (remote mode) ──────────────────────────────────────────

pub async fn send_message_via_api(
    api_url: &str,
    api_key: &str,
    model_config: &ModelConfig,
    message: &str,
    history: Option<&Vec<HistoryItem>>,
    session_id: Option<&str>,
    app_handle: &AppHandle,
) -> Result<String, String> {
    let client = Client::new();

    // Build messages array
    let mut messages: Vec<Value> = Vec::new();

    // Add history
    if let Some(hist) = history {
        for item in hist {
            messages.push(serde_json::json!({
                "role": if item.role == "agent" { "assistant" } else { &item.role },
                "content": &item.content,
            }));
        }
    }

    // Add current message
    messages.push(serde_json::json!({
        "role": "user",
        "content": message,
    }));

    // Generate session ID
    let sid = session_id
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("desk-{}", uuid::Uuid::new_v4()));

    let body = serde_json::json!({
        "model": if model_config.model.is_empty() { "hermes-agent" } else { &model_config.model },
        "messages": messages,
        "stream": true,
    });

    let url = format!("{}/v1/chat/completions", api_url.trim_end_matches('/'));

    let mut req = client
        .post(&url)
        .header("Content-Type", "application/json")
        .body(body.to_string());

    if !api_key.is_empty() {
        req = req.header("Authorization", format!("Bearer {}", api_key));
    }

    let response = req
        .send()
        .await
        .map_err(|e| format!("API request failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("API error {}: {}", status, body));
    }

    // Stream SSE response
    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    let mut parser = SseParser::new();
    let mut full_text = String::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("Stream error: {}", e))?;
        let text = String::from_utf8_lossy(&chunk);
        buffer.push_str(&text);

        // Process complete lines
        while let Some(pos) = buffer.find("\n\n") {
            let block = buffer[..pos].to_string();
            buffer = buffer[pos + 2..].to_string();

            if let Some((_, data)) = SseParser::parse_block(&block) {
                if let Some(event) = parser.process_data(&data) {
                    match &event {
                        ChatEvent::Token { content } => {
                            full_text.push_str(content);
                        }
                        ChatEvent::Done { .. } => {
                            let _ = app_handle.emit("chat_event", event);
                            return Ok(full_text);
                        }
                        ChatEvent::Error { .. } => {
                            let _ = app_handle.emit("chat_event", event.clone());
                            return Err(parser.last_error.clone());
                        }
                        _ => {}
                    }
                    let _ = app_handle.emit("chat_event", event);
                }
            }
        }
    }

    Ok(full_text)
}

// ── TUI Gateway chat (local mode) ─────────────────────────────────────────

pub fn send_message_via_gateway(
    gateway_stdout: BufReader<ChildStdout>,
    message: &str,
    session_id: Option<String>,
    app_handle: &AppHandle,
) -> Result<(), String> {
    // Parse gateway stdout for events
    let handle = app_handle.clone();

    thread::spawn(move || {
        let mut parser = SseParser::new();
        let mut buffer = String::new();

        for line in gateway_stdout.lines() {
            let line = match line {
                Ok(l) => l,
                Err(_) => break,
            };

            // Gateway events are newline-delimited JSON
            if line.trim().starts_with('{') {
                if let Ok(value) = serde_json::from_str::<Value>(&line) {
                    // Parse as gateway event
                    if let Some(event) = parse_gateway_event(&value) {
                        let _ = handle.emit("chat_event", event);
                    }
                }
            } else if line.contains("data:") {
                // SSE format
                buffer.push_str(&line);
                buffer.push('\n');

                if line.trim().is_empty() || line == "data: [DONE]" {
                    if let Some((_, data)) = SseParser::parse_block(&buffer) {
                        if let Some(event) = parser.process_data(&data) {
                            let _ = handle.emit("chat_event", event);
                        }
                    }
                    buffer.clear();
                }
            }
        }

        // Emit done
        let _ = handle.emit(
            "chat_event",
            ChatEvent::Done {
                session_id,
            },
        );
    });

    Ok(())
}

fn parse_gateway_event(value: &Value) -> Option<ChatEvent> {
    let event_type = value
        .get("params")
        .and_then(|p| p.get("type"))
        .or_else(|| value.get("method"))
        .and_then(|t| t.as_str())?;

    match event_type {
        "message.chunk" | "token" => {
            let content = value
                .get("params")
                .and_then(|p| p.get("text"))
                .and_then(|t| t.as_str())
                .unwrap_or("");
            if !content.is_empty() {
                Some(ChatEvent::Token {
                    content: content.to_string(),
                })
            } else {
                None
            }
        }
        "reasoning.delta" | "thinking.delta" => {
            let content = value
                .get("params")
                .and_then(|p| p.get("text"))
                .and_then(|t| t.as_str())
                .unwrap_or("");
            if !content.is_empty() {
                Some(ChatEvent::Reasoning {
                    content: content.to_string(),
                })
            } else {
                None
            }
        }
        "tool.start" => {
            let name = value
                .get("params")
                .and_then(|p| p.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("tool");
            let tool_id = value
                .get("params")
                .and_then(|p| p.get("tool_id"))
                .and_then(|id| id.as_str())
                .unwrap_or("");
            Some(ChatEvent::ToolStart {
                name: name.to_string(),
                tool_call_id: tool_id.to_string(),
            })
        }
        "tool.complete" => {
            let name = value
                .get("params")
                .and_then(|p| p.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("tool");
            let tool_id = value
                .get("params")
                .and_then(|p| p.get("tool_id"))
                .and_then(|id| id.as_str())
                .unwrap_or("");
            let output = value
                .get("params")
                .and_then(|p| p.get("output"))
                .and_then(|o| o.as_str())
                .unwrap_or("");
            Some(ChatEvent::ToolComplete {
                name: name.to_string(),
                tool_call_id: tool_id.to_string(),
                output: output.to_string(),
                duration_ms: 0,
            })
        }
        "message.end" | "done" => Some(ChatEvent::Done { session_id: None }),
        "error" => {
            let msg = value
                .get("params")
                .and_then(|p| p.get("message"))
                .or_else(|| value.get("error").and_then(|e| e.get("message")))
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown error");
            Some(ChatEvent::Error {
                message: msg.to_string(),
            })
        }
        "status.update" => {
            let status = value
                .get("params")
                .and_then(|p| p.get("text"))
                .or_else(|| value.get("params").and_then(|p| p.get("kind")))
                .and_then(|s| s.as_str())
                .unwrap_or("unknown");
            Some(ChatEvent::Status {
                status: status.to_string(),
            })
        }
        _ => None,
    }
}

// ── Unified send message ──────────────────────────────────────────────────

pub async fn send_message(
    gateway_state: &GatewayState,
    ssh_state: &SshState,
    hermes_home: &PathBuf,
    connection_mode: &ConnectionMode,
    remote_url: &str,
    remote_api_key: &str,
    ssh_config: &Option<SshConfig>,
    request: SendMessageRequest,
    app_handle: &AppHandle,
) -> Result<String, String> {
    let model_config = config::get_model_config(hermes_home, None);

    match connection_mode {
        ConnectionMode::Local => {
            // Check if gateway is running
            if !gateway::is_gateway_running(gateway_state, None) {
                // Try to start gateway
                let result = gateway::start_gateway(gateway_state, hermes_home, None);
                if !result.success {
                    return Err(result.error.unwrap_or("Failed to start gateway".to_string()));
                }
            }

            // Get API URL
            let api_url = gateway::get_api_url(gateway_state, None)
                .ok_or("Gateway not available")?;

            // Use API mode even for local (gateway exposes HTTP API)
            send_message_via_api(
                &api_url,
                "", // Local gateway doesn't need API key
                &model_config,
                &request.text,
                request.history.as_ref(),
                request.session_id.as_deref(),
                app_handle,
            )
            .await
        }
        ConnectionMode::Remote => {
            send_message_via_api(
                remote_url,
                remote_api_key,
                &model_config,
                &request.text,
                request.history.as_ref(),
                request.session_id.as_deref(),
                app_handle,
            )
            .await
        }
        ConnectionMode::Ssh => {
            let ssh = ssh_config.as_ref().ok_or("SSH config not provided")?;

            // Ensure tunnel is active
            if !crate::ssh::is_tunnel_active(ssh_state) {
                crate::ssh::start_ssh_tunnel(ssh_state, ssh)
                    .map_err(|e| format!("SSH tunnel failed: {}", e))?;
            }

            let tunnel_url = crate::ssh::get_tunnel_url(ssh_state)
                .ok_or("SSH tunnel not available")?;

            send_message_via_api(
                &tunnel_url,
                "", // Remote gateway may not need key over tunnel
                &model_config,
                &request.text,
                request.history.as_ref(),
                request.session_id.as_deref(),
                app_handle,
            )
            .await
        }
    }
}
