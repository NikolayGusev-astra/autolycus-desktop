// src-tauri/src/chat.rs
// Chat streaming: sendMessage with SSE, API fallback, session management
// Ported from fathah/hermes-desktop src/main/hermes.ts (chat part)

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::AppHandle;

use crate::config::{self, SshConfig};
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
    ToolStart { name: String, tool_call_id: String },
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
    Done { session_id: Option<String> },
    #[serde(rename = "error")]
    Error { message: String },
    #[serde(rename = "status")]
    Status { status: String },
    // Phase 1B additions
    #[serde(rename = "session_info")]
    SessionInfo {
        session_id: String,
        stored_session_id: String,
        running: bool,
        model: Option<String>,
        provider: Option<String>,
        tools: serde_json::Value,
        skills: serde_json::Value,
        usage: Option<serde_json::Value>,
        desktop_contract: Option<u32>,
    },
    #[serde(rename = "thinking")]
    Thinking { content: String },
    #[serde(rename = "tool_generating")]
    ToolGenerating {
        name: String,
        tool_call_id: Option<String>,
        content: Option<String>,
    },
    #[serde(rename = "clarify_request")]
    ClarifyRequest {
        request_id: String,
        question: String,
        choices: Vec<String>,
    },
    #[serde(rename = "sudo_request")]
    SudoRequest {
        request_id: String,
        reason: Option<String>,
        timeout_secs: Option<u64>,
    },
    #[serde(rename = "sudo_expire")]
    SudoExpire { request_id: String },
    #[serde(rename = "secret_request")]
    SecretRequest {
        request_id: String,
        prompt: String,
        env_var: String,
        metadata: Option<serde_json::Value>,
    },
    #[serde(rename = "secret_expire")]
    SecretExpire { request_id: String },
    #[serde(rename = "notification")]
    Notification {
        id: String,
        key: String,
        text: String,
        level: Option<String>,
        kind: Option<String>,
        ttl_ms: Option<u64>,
    },
    #[serde(rename = "notification.clear")]
    NotificationClear { key: String },
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
    /// Phase 1C.2: product/UI conversation ID. When present, the backend
    /// resolves the Hermes live session ID through the SessionRegistry instead
    /// of the global session_id cache. Falls back to session_id for backward
    /// compatibility with frontends that haven't migrated yet.
    #[serde(default)]
    pub conversation_id: Option<String>,
    pub history: Option<Vec<HistoryItem>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HistoryItem {
    pub role: String,
    pub content: String,
}

// ── SSE Parser ────────────────────────────────────────────────────────────
// REMOVED (P2.1 cleanup follow-up): SseParser was the HTTP /v1/chat/completions
// SSE-stream parser. With the WS migration (ADR-004/005) all transports use
// parse_ws_message / parse_gateway_event; SseParser had 0 callers.

// ── API-based chat (remote mode) ──────────────────────────────────────────
// REMOVED (ADR-004, P2.1): send_message_via_api() targeted the HTTP
// /v1/chat/completions endpoint, which does not exist on a real `hermes serve`
// backend. All three connection modes now use the WebSocket transport
// (send_message_via_ws). The SseParser below was its only consumer and is
// removed with it.

// ── TUI Gateway chat (local mode) ─────────────────────────────────────────
// NOTE (ADR-004, P2.3): the old send_message_via_gateway() WS-over-stdio stub
// was dead code (0 callers) and is now superseded by ws_transport.rs. Removed.
// parse_gateway_event() below is retained — it is the core of parse_ws_message
// (the live WS wire-envelope dispatcher).

// ── Payload extraction helpers (real wire format, server.py _emit) ────────
//
// The upstream backend stamps every event as:
//   {jsonrpc:"2.0", method:"event", params:{type, session_id, payload:{...}}}
// Content fields (text, name, tool_id, output) live INSIDE params.payload,
// not at the params top level. These helpers read payload.<field> with a
// fallback to params.<field> for backward compat with older event shapes.

/// Read the "text" field from an event — checks params.payload.text first
/// (real wire format), then params.text (legacy fallback).
fn payload_text(value: &Value) -> Option<&str> {
    payload_field(value, "text")
}

/// Read a named field from params.payload.<field>, falling back to params.<field>.
fn payload_field<'a>(value: &'a Value, field: &str) -> Option<&'a str> {
    value
        .get("params")
        .and_then(|p| p.get("payload"))
        .and_then(|pl| pl.get(field))
        .or_else(|| value.get("params").and_then(|p| p.get(field)))
        .and_then(|v| v.as_str())
}

/// Read tool name + tool_id from payload (or fallback to params top-level).
fn payload_tool_id(value: &Value) -> (&str, &str) {
    let name = payload_field(value, "name").unwrap_or("tool");
    let tool_id = payload_field(value, "tool_id").unwrap_or("");
    (name, tool_id)
}

fn parse_gateway_event(value: &Value) -> Option<ChatEvent> {
    let event_type = value
        .get("params")
        .and_then(|p| p.get("type"))
        .or_else(|| value.get("method"))
        .and_then(|t| t.as_str())?;

    match event_type {
        "message.chunk" | "token" | "message.delta" => {
            // Real wire format (server.py _emit): params.payload.text
            // Legacy/fallback: params.text or params.delta
            let content = payload_text(value).unwrap_or("");
            if !content.is_empty() {
                Some(ChatEvent::Token {
                    content: content.to_string(),
                })
            } else {
                None
            }
        }
        "reasoning.delta" => {
            let content = payload_text(value).unwrap_or("");
            if !content.is_empty() {
                Some(ChatEvent::Reasoning {
                    content: content.to_string(),
                })
            } else {
                None
            }
        }
        "tool.start" => {
            let (name, tool_id) = payload_tool_id(value);
            Some(ChatEvent::ToolStart {
                name: name.to_string(),
                tool_call_id: tool_id.to_string(),
            })
        }
        "tool.complete" => {
            let (name, tool_id) = payload_tool_id(value);
            let output = payload_field(value, "output").unwrap_or("");
            // Backend (server.py:3695) emits `duration_s` as a float (seconds).
            // The frontend ChatEvent contract expects `duration_ms`. Convert
            // seconds → milliseconds; fall back to 0 if absent.
            let duration_ms = value
                .get("params")
                .and_then(|p| p.get("payload"))
                .and_then(|pl| pl.get("duration_s"))
                .or_else(|| value.get("params").and_then(|p| p.get("duration_s")))
                .and_then(|d| d.as_f64())
                .map(|secs| (secs * 1000.0) as u64)
                .unwrap_or(0);
            Some(ChatEvent::ToolComplete {
                name: name.to_string(),
                tool_call_id: tool_id.to_string(),
                output: output.to_string(),
                duration_ms,
            })
        }
        "message.end" | "message.complete" | "message.done" | "done" => {
            // upstream emits session_id in params on message.complete (the real
            // terminal event); the frontend pins currentSessionId from it.
            let session_id = value
                .get("params")
                .and_then(|p| p.get("session_id"))
                .and_then(|s| s.as_str())
                .map(|s| s.to_string());
            Some(ChatEvent::Done { session_id })
        }
        "message.start" => Some(ChatEvent::Status {
            status: "streaming".to_string(),
        }),
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
        "approval.request" => {
            // Backend (_emit_approval_request, server.py:1147-1166) emits a
            // payload with: command (redacted), choices, smart_denied,
            // allow_permanent, and the tool context. Map to ChatEvent fields
            // the frontend ChatView.tsx:229 expects.
            let request_id = payload_field(value, "request_id")
                .or_else(|| payload_field(value, "tool_id"))
                .unwrap_or("");
            let tool_name = payload_field(value, "name")
                .or_else(|| payload_field(value, "tool_name"))
                .unwrap_or("tool");
            let tool_input = payload_field(value, "command")
                .or_else(|| payload_field(value, "tool_input"))
                .unwrap_or("");
            let action = payload_field(value, "action")
                .or_else(|| payload_field(value, "message"))
                .unwrap_or("");
            let command_class = payload_field(value, "command_class").unwrap_or("write");
            Some(ChatEvent::ApprovalRequest {
                request_id: request_id.to_string(),
                tool_name: tool_name.to_string(),
                tool_input: tool_input.to_string(),
                action: action.to_string(),
                command_class: command_class.to_string(),
            })
        }
        "session.info" => {
            let _session_id = payload_field(value, "session_id").unwrap_or("");
            let stored_session_id = payload_field(value, "stored_session_id").unwrap_or("");
            let running = value
                .get("params")
                .and_then(|p| p.get("payload"))
                .and_then(|pl| pl.get("running"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let model = payload_field(value, "model").map(|s| s.to_string());
            let provider = payload_field(value, "provider").map(|s| s.to_string());
            let tools = value
                .get("params")
                .and_then(|p| p.get("payload"))
                .and_then(|pl| pl.get("tools"))
                .cloned()
                .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
            let skills = value
                .get("params")
                .and_then(|p| p.get("payload"))
                .and_then(|pl| pl.get("skills"))
                .cloned()
                .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
            let usage = value
                .get("params")
                .and_then(|p| p.get("payload"))
                .and_then(|pl| pl.get("usage"))
                .cloned();
            let desktop_contract = value
                .get("params")
                .and_then(|p| p.get("payload"))
                .and_then(|pl| pl.get("desktop_contract"))
                .and_then(|v| v.as_u64())
                .map(|v| v as u32);
            let session_id_param = value
                .get("params")
                .and_then(|p| p.get("session_id"))
                .and_then(|s| s.as_str())
                .map(|s| s.to_string());

            Some(ChatEvent::SessionInfo {
                session_id: session_id_param.unwrap_or_default(),
                stored_session_id: stored_session_id.to_string(),
                running,
                model,
                provider,
                tools,
                skills,
                usage,
                desktop_contract,
            })
        }
        "thinking.delta" => {
            let content = payload_text(value).unwrap_or("");
            if !content.is_empty() {
                Some(ChatEvent::Thinking {
                    content: content.to_string(),
                })
            } else {
                None
            }
        }
        "tool.generating" => {
            let name = payload_field(value, "name").unwrap_or("tool");
            let tool_id = payload_field(value, "tool_id").map(|s| s.to_string());
            let content = payload_text(value).map(|s| s.to_string());
            Some(ChatEvent::ToolGenerating {
                name: name.to_string(),
                tool_call_id: tool_id,
                content,
            })
        }
        "clarify.request" => {
            let request_id = payload_field(value, "request_id").unwrap_or("");
            let question = payload_field(value, "question").unwrap_or("");
            let choices = value
                .get("params")
                .and_then(|p| p.get("payload"))
                .and_then(|pl| pl.get("choices"))
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            Some(ChatEvent::ClarifyRequest {
                request_id: request_id.to_string(),
                question: question.to_string(),
                choices,
            })
        }
        "sudo.request" => {
            let request_id = payload_field(value, "request_id").unwrap_or("");
            let reason = payload_field(value, "reason").map(|s| s.to_string());
            let timeout_secs = value
                .get("params")
                .and_then(|p| p.get("payload"))
                .and_then(|pl| pl.get("timeout_secs"))
                .and_then(|v| v.as_u64());
            Some(ChatEvent::SudoRequest {
                request_id: request_id.to_string(),
                reason,
                timeout_secs,
            })
        }
        "sudo.expire" => {
            let request_id = payload_field(value, "request_id").unwrap_or("");
            Some(ChatEvent::SudoExpire {
                request_id: request_id.to_string(),
            })
        }
        "secret.request" => {
            let request_id = payload_field(value, "request_id").unwrap_or("");
            let prompt = payload_field(value, "prompt").unwrap_or("");
            let env_var = payload_field(value, "env_var").unwrap_or("");
            let metadata = value
                .get("params")
                .and_then(|p| p.get("payload"))
                .and_then(|pl| pl.get("metadata"))
                .cloned();
            Some(ChatEvent::SecretRequest {
                request_id: request_id.to_string(),
                prompt: prompt.to_string(),
                env_var: env_var.to_string(),
                metadata,
            })
        }
        "secret.expire" => {
            let request_id = payload_field(value, "request_id").unwrap_or("");
            Some(ChatEvent::SecretExpire {
                request_id: request_id.to_string(),
            })
        }
        "notification.show" => {
            let id = payload_field(value, "id").unwrap_or("");
            let key = payload_field(value, "key").unwrap_or("");
            let text = payload_field(value, "text").unwrap_or("");
            let level = payload_field(value, "level").map(|s| s.to_string());
            let kind = payload_field(value, "kind").map(|s| s.to_string());
            let ttl_ms = value
                .get("params")
                .and_then(|p| p.get("payload"))
                .and_then(|pl| pl.get("ttl_ms"))
                .and_then(|v| v.as_u64());
            Some(ChatEvent::Notification {
                id: id.to_string(),
                key: key.to_string(),
                text: text.to_string(),
                level,
                kind,
                ttl_ms,
            })
        }
        "notification.clear" => {
            let key = payload_field(value, "key").unwrap_or("");
            Some(ChatEvent::Notification {
                id: "".to_string(),
                key: key.to_string(),
                text: "".to_string(),
                level: None,
                kind: None,
                ttl_ms: None,
            })
        }
        _ => None,
    }
}

// ── WebSocket wire-envelope dispatcher (ADR-004) ───────────────────────────
//
// The /api/ws JSON-RPC channel sends newline-delimited messages in BOTH
// directions (tui_gateway/ws.py:11). A streaming event is wrapped as:
//   {"jsonrpc":"2.0","method":"event","params":{"type":"<event>", ...}}
// while a JSON-RPC *response* (to session.create / prompt.submit) carries an
// "id" + "result"/"error" and is NOT a streaming event. This thin dispatcher
// unwraps the envelope and delegates the params payload to
// `parse_gateway_event`, which already knows the upstream event vocabulary.

/// Parse one raw WS wire line into a streaming ChatEvent, if it is one.
///
/// Returns None for:
/// - non-JSON / empty lines (heartbeats, partial frames)
/// - JSON-RPC responses (have `id` + `result`/`error`) — handled by the caller
/// - envelopes whose `method != "event"`
/// - events `parse_gateway_event` doesn't recognise
pub fn parse_ws_message(raw: &str) -> Option<ChatEvent> {
    let raw = raw.trim();
    if raw.is_empty() || !raw.starts_with('{') {
        return None;
    }
    let value: Value = serde_json::from_str(raw).ok()?;
    // A JSON-RPC response carries an "id" — it is not a streaming event.
    if value.get("id").is_some() {
        return None;
    }
    // Only "event" envelopes carry streaming events.
    let is_event = value
        .get("method")
        .and_then(|m| m.as_str())
        .map(|m| m == "event")
        .unwrap_or(false);
    if !is_event {
        return None;
    }
    parse_gateway_event(&value)
}

// ── Capability detection + Runs/Chat-Completions HTTP transports ──────────
// REMOVED (ADR-004/005, P2.1): supports_runs_transport(), send_message_via_runs(),
// parse_runs_event(), and send_via_best_transport() targeted the HTTP
// /v1/capabilities, /v1/runs, and /v1/chat/completions endpoints, none of which
// exist on a real `hermes serve` backend. All connection modes now use the
// WebSocket transport (send_message_via_ws). This block was ~300 lines.

// ── Unified send message ──────────────────────────────────────────────────

// ── Unified send message ──────────────────────────────────────────────────

/// Convert an HTTP(S) URL to a WebSocket URL (ADR-005).
///
/// `http://` → `ws://`, `https://` → `wss://`. Already-ws URLs pass through
/// unchanged. Used by the Remote and SSH branches to talk to a remote
/// `hermes serve` over the same /api/ws transport as Local.
pub fn to_ws_url(url: &str) -> String {
    let url = url.trim();
    if let Some(rest) = url.strip_prefix("https://") {
        format!("wss://{}", rest)
    } else if let Some(rest) = url.strip_prefix("http://") {
        format!("ws://{}", rest)
    } else {
        // Already ws:// / wss:// or some other scheme — pass through unchanged.
        url.to_string()
    }
}

/// Remote/SSH WebSocket transport (ADR-005, P3.2).
///
/// Same `send_message_via_ws` as Local, but the URL/token come from the
/// remote backend (or SSH tunnel). `base_url` is an http(s) URL and is
/// converted to ws(s); `token` is the remote backend's session token.
async fn send_via_ws_remote(
    base_url: &str,
    token: &str,
    request: &SendMessageRequest,
    app_handle: &AppHandle,
) -> Result<String, String> {
    if token.is_empty() {
        return Err("Remote session token is empty. Set it in Settings → Connection.".to_string());
    }
    // Strip any trailing path so we control the /api/ws suffix cleanly.
    let base = base_url.trim_end_matches('/').trim_end_matches("/api/ws");
    let ws_url = format!("{}/api/ws?token={}", to_ws_url(base), token);
    crate::ws_transport::send_message_via_ws(
        &ws_url,
        request.session_id.as_deref(),
        &request.text,
        app_handle,
    )
    .await
    .map_err(|e| e.to_string())
}

/// Local-mode WebSocket transport (ADR-004, Phase 0).
///
/// Builds the WS URL from the spawned gateway's port and reads the auth token
/// from `HERMES_DASHBOARD_SESSION_TOKEN`. Returns the URL + None if the gateway
/// is not yet running.
async fn build_local_ws_url(gateway_state: &GatewayState) -> Result<String, String> {
    let port = gateway::get_gateway_port(gateway_state, None)
        .await
        .ok_or("Gateway not available (no port)")?;
    // Token: prefer the one Steersman generated for the spawned process (P1);
    // fall back to HERMES_DASHBOARD_SESSION_TOKEN env for Phase 0 compatibility.
    let token = gateway::get_gateway_session_token(gateway_state, None)
        .await
        .or_else(|| {
            std::env::var("HERMES_DASHBOARD_SESSION_TOKEN")
                .ok()
                .filter(|v| !v.is_empty())
        })
        .ok_or_else(|| {
            "No dashboard session token: gateway not spawned by Steersman, \
             and HERMES_DASHBOARD_SESSION_TOKEN env is unset."
                .to_string()
        })?;
    // The session token is base64url (secrets.token_urlsafe in upstream),
    // whose alphabet [A-Za-z0-9_-] needs no percent-encoding in a query string.
    Ok(format!("ws://127.0.0.1:{}/api/ws?token={}", port, token))
}

/// ADR-006: Send a chat message over the PERSISTENT local WS connection.
///
/// Ensures the connection is open (connecting once, lazily), resolves the
/// session_id (creating a session if the frontend didn't pass one), and
/// submits the prompt via the reader task's mpsc channel. Streaming events
/// flow back as `chat_event` Tauri events — the frontend contract is unchanged.
pub async fn send_via_ws_persistent_local(
    gateway_state: &GatewayState,
    ws_state: &std::sync::Arc<crate::ws_transport::WsState>,
    sessions: &std::sync::Arc<crate::session_registry::SessionRegistry>,
    request: &SendMessageRequest,
    app_handle: &AppHandle,
) -> Result<String, String> {
    let ws_url = build_local_ws_url(gateway_state).await?;

    // Ensure the persistent connection is open (idempotent).
    let emit_fn = crate::ws_transport::make_tauri_emitter(app_handle.clone());
    crate::ws_transport::ensure_ws_connection(&ws_url, emit_fn, ws_state, Some(sessions.clone()))
        .await
        .map_err(|e| e.to_string())?;

    // Phase 1C.2: resolve the Hermes live session ID via the SessionRegistry.
    // The product layer identifies conversations by conversation_id; the
    // transport layer maps it to a live Hermes session ID. If no live ID
    // exists yet, create a session and register the binding.
    //
    // session_id in the request is kept as a backward-compatibility fallback
    // for frontends that still pass the Hermes UUID directly.
    let session_id = if let Some(sid) = request.session_id.as_deref().filter(|s| !s.is_empty()) {
        sid.to_string()
    } else if let Some(conv_str) = request.conversation_id.as_deref().filter(|s| !s.is_empty()) {
        let conv = crate::session_registry::ConversationId::new(conv_str);
        match sessions.get_live(&conv).await {
            Some(live) => live,
            None => {
                // No live ID for this conversation yet — create one.
                let result = crate::ws_transport::create_session_on_connection(ws_state, "desktop")
                    .await
                    .map_err(|e| format!("{:?}", e))?;
                sessions
                    .set_live(
                        conv,
                        result.session_id.clone(),
                        Some(result.stored_session_id),
                        ws_state
                            .generation
                            .load(std::sync::atomic::Ordering::Acquire),
                    )
                    .await;
                result.session_id
            }
        }
    } else {
        // Legacy fallback: no conversation_id and no session_id. Create a
        // new session. This path will go away once the frontend migrates to
        // conversation_id (tracked separately).
        let result = crate::ws_transport::create_session_on_connection(ws_state, "desktop")
            .await
            .map_err(|e| format!("{:?}", e))?;
        result.session_id
    };

    // Submit the prompt — events stream back via chat_event.
    crate::ws_transport::submit_prompt_on_connection(ws_state, &session_id, &request.text)
        .await
        .map_err(|e| format!("{:?}", e))?;

    Ok(session_id)
}

/// Tauri command boundary: inherent parameter fan-out across the three
/// connection modes. Collapse into a context struct in Phase 3 product API.
#[allow(clippy::too_many_arguments)]
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
    ws_state: &std::sync::Arc<crate::ws_transport::WsState>,
    sessions: &std::sync::Arc<crate::session_registry::SessionRegistry>,
) -> Result<String, String> {
    // Note: model_config is no longer fetched here — the WS transport sends
    // only {session_id, text}; reasoning_effort/verbosity/etc. live in the
    // backend's config.yaml, not the per-request body (ADR-004).
    match connection_mode {
        ConnectionMode::Local => {
            // Check if gateway is running
            if !gateway::is_gateway_running(gateway_state, None).await {
                // Try to start gateway
                let result = gateway::start_gateway(gateway_state, hermes_home, None).await;
                if !result.success {
                    return Err(result
                        .error
                        .unwrap_or("Failed to start gateway".to_string()));
                }
            }

            // ADR-006: Local mode uses the PERSISTENT WS connection (one
            // socket for the app lifetime). Remote/SSH still use connect-per-
            // message (separate migration, out of scope for ADR-006).
            send_via_ws_persistent_local(gateway_state, ws_state, sessions, &request, app_handle)
                .await
        }
        ConnectionMode::Remote => {
            // ADR-005: Remote talks to a remote `hermes serve` over the same
            // WS /api/ws transport. remote_api_key is interpreted as the remote
            // backend's session token (UI label rename is a separate task).
            send_via_ws_remote(remote_url, remote_api_key, &request, app_handle).await
        }
        ConnectionMode::Ssh => {
            let ssh = ssh_config.as_ref().ok_or("SSH config not provided")?;

            // Ensure tunnel is active. The SSH tunnel is generic TCP forwarding
            // (-L local:remote), so WebSocket frames pass through unmodified
            // (ADR-005). Only the URL scheme must be ws://, not http://.
            if !crate::ssh::is_tunnel_active(ssh_state) {
                crate::ssh::start_ssh_tunnel(ssh_state, ssh.clone(), hermes_home.clone())
                    .map_err(|e| format!("SSH tunnel failed: {}", e))?;
            }

            let tunnel_url =
                crate::ssh::get_tunnel_url(ssh_state).ok_or("SSH tunnel not available")?;

            // The remote backend's session token (same resolution as Remote).
            let tunneled_token = config::get_api_server_key(hermes_home, None)
                .or_else(|| {
                    dirs::home_dir()
                        .and_then(|h| config::get_api_server_key(&h.join(".hermes"), None))
                })
                .or_else(|| {
                    std::env::var("API_SERVER_KEY")
                        .ok()
                        .filter(|v| !v.is_empty())
                })
                .unwrap_or_default();

            send_via_ws_remote(&tunnel_url, &tunneled_token, &request, app_handle).await
        }
    }
}

// REMOVED (P2.1 cleanup follow-up): autonomy_policy_text() injected an
// autonomy-policy system message into the HTTP /v1/chat/completions body.
// The WS transport sends only {session_id, text}; steer settings (reasoning
// effort, autonomy policy, verbosity) live in the backend's config.yaml
// (ADR-004). 0 callers in production code; its 5 unit tests removed with it.

#[cfg(test)]
mod tests {
    use super::*;

    // ── ADR-004: WebSocket transport — parser tests (TDD) ──────────────────
    // Fixtures mirror the real events emitted by the upstream tui_gateway
    // (tui_gateway/server.py:3648-3921) over the /api/ws JSON-RPC channel.

    /// Helper: parse a raw JSON-RPC wire line into a ChatEvent.
    fn ws_event(raw: &str) -> Option<ChatEvent> {
        parse_ws_message(raw)
    }

    // ── parse_ws_message: the wire-envelope dispatcher (full JSON-RPC) ──────

    #[test]
    fn ws_token_event_from_message_chunk_envelope() {
        // Real upstream wire format: {"jsonrpc":"2.0","method":"event",
        //   "params":{"type":"message.chunk","text":"Hi"}}
        let ev = ws_event(
            r#"{"jsonrpc":"2.0","method":"event","params":{"type":"message.chunk","text":"Hi"}}"#,
        );
        match ev {
            Some(ChatEvent::Token { content }) => assert_eq!(content, "Hi"),
            other => panic!("expected Token, got {:?}", other),
        }
    }

    #[test]
    fn ws_reasoning_event_from_reasoning_delta() {
        let ev = ws_event(
            r#"{"jsonrpc":"2.0","method":"event","params":{"type":"reasoning.delta","text":"thinking..."}}"#,
        );
        match ev {
            Some(ChatEvent::Reasoning { content }) => assert_eq!(content, "thinking..."),
            other => panic!("expected Reasoning, got {:?}", other),
        }
    }

    #[test]
    fn ws_tool_start_then_complete() {
        let start = ws_event(
            r#"{"jsonrpc":"2.0","method":"event","params":{"type":"tool.start","name":"read_file","tool_id":"t1"}}"#,
        );
        match start {
            Some(ChatEvent::ToolStart { name, tool_call_id }) => {
                assert_eq!(name, "read_file");
                assert_eq!(tool_call_id, "t1");
            }
            other => panic!("expected ToolStart, got {:?}", other),
        }
        let complete = ws_event(
            r#"{"jsonrpc":"2.0","method":"event","params":{"type":"tool.complete","name":"read_file","tool_id":"t1","output":"42 chars"}}"#,
        );
        match complete {
            Some(ChatEvent::ToolComplete { name, output, .. }) => {
                assert_eq!(name, "read_file");
                assert_eq!(output, "42 chars");
            }
            other => panic!("expected ToolComplete, got {:?}", other),
        }
    }

    #[test]
    fn ws_done_event_carries_session_id() {
        // upstream message.end emits the session_id in params; the frontend
        // relies on it to pin currentSessionId (ChatView.tsx:201-203).
        let ev = ws_event(
            r#"{"jsonrpc":"2.0","method":"event","params":{"type":"message.end","session_id":"desk-123-abc"}}"#,
        );
        match ev {
            Some(ChatEvent::Done { session_id }) => {
                assert_eq!(session_id.as_deref(), Some("desk-123-abc"));
            }
            other => panic!("expected Done with session_id, got {:?}", other),
        }
    }

    #[test]
    fn ws_error_event_extracts_message() {
        let ev = ws_event(
            r#"{"jsonrpc":"2.0","method":"event","params":{"type":"error","message":"model overloaded"}}"#,
        );
        match ev {
            Some(ChatEvent::Error { message }) => assert_eq!(message, "model overloaded"),
            other => panic!("expected Error, got {:?}", other),
        }
    }

    #[test]
    fn ws_status_update_event() {
        let ev = ws_event(
            r#"{"jsonrpc":"2.0","method":"event","params":{"type":"status.update","text":"thinking"}}"#,
        );
        match ev {
            Some(ChatEvent::Status { status }) => assert_eq!(status, "thinking"),
            other => panic!("expected Status, got {:?}", other),
        }
    }

    #[test]
    fn ws_rpc_response_is_not_an_event() {
        // A JSON-RPC *response* (result/error with an id) is not a streaming
        // event and must not be misinterpreted as one.
        let ev = ws_event(r#"{"jsonrpc":"2.0","id":1,"result":{"session_id":"s1"}}"#);
        assert!(ev.is_none());
    }

    #[test]
    fn ws_garbage_line_returns_none() {
        assert!(ws_event("not json at all").is_none());
        assert!(ws_event("").is_none());
    }

    // ── Real upstream event vocabulary (discovered via live WS probe) ───────
    // The backend emits message.start/message.delta/message.complete (not
    // message.chunk/message.end as the old parser assumed). message.delta is
    // the real token stream; message.complete is the terminal event.

    #[test]
    fn ws_message_delta_is_token() {
        // Real wire format: params.payload.text (legacy params.delta fallback still works)
        let ev = ws_event(
            r#"{"jsonrpc":"2.0","method":"event","params":{"type":"message.delta","payload":{"text":"Hi there"}}}"#,
        );
        match ev {
            Some(ChatEvent::Token { content }) => assert_eq!(content, "Hi there"),
            other => panic!("expected Token, got {:?}", other),
        }
    }

    #[test]
    fn ws_message_delta_with_text_field() {
        // Some events carry the content in "text" instead of "delta".
        let ev = ws_event(
            r#"{"jsonrpc":"2.0","method":"event","params":{"type":"message.delta","text":"alt"}}"#,
        );
        match ev {
            Some(ChatEvent::Token { content }) => assert_eq!(content, "alt"),
            other => panic!("expected Token, got {:?}", other),
        }
    }

    // ── Real wire format: params.payload.text (server.py _emit, confirmed) ──
    // The actual backend frame is:
    //   {"jsonrpc":"2.0","method":"event","params":{"type":"message.delta","session_id":"s1","payload":{"text":"<token>"}}}
    // Text lives in params.payload.text — NOT params.text, NOT params.delta.

    #[test]
    fn ws_message_delta_real_wire_format_payload_text() {
        let ev = ws_event(
            r#"{"jsonrpc":"2.0","method":"event","params":{"type":"message.delta","session_id":"s1","payload":{"text":"Hello world"}}}"#,
        );
        match ev {
            Some(ChatEvent::Token { content }) => assert_eq!(content, "Hello world"),
            other => panic!("expected Token with payload.text, got {:?}", other),
        }
    }

    #[test]
    fn ws_reasoning_delta_real_wire_format_payload_text() {
        let ev = ws_event(
            r#"{"jsonrpc":"2.0","method":"event","params":{"type":"reasoning.delta","session_id":"s1","payload":{"text":"thinking..."}}}"#,
        );
        match ev {
            Some(ChatEvent::Reasoning { content }) => assert_eq!(content, "thinking..."),
            other => panic!("expected Reasoning with payload.text, got {:?}", other),
        }
    }

    #[test]
    fn ws_message_complete_real_wire_format() {
        let ev = ws_event(
            r#"{"jsonrpc":"2.0","method":"event","params":{"type":"message.complete","session_id":"s1","payload":{"text":"full response","usage":{}}}}"#,
        );
        match ev {
            Some(ChatEvent::Done { session_id }) => assert_eq!(session_id.as_deref(), Some("s1")),
            other => panic!("expected Done, got {:?}", other),
        }
    }

    #[test]
    fn ws_message_complete_is_terminal_done() {
        let ev = ws_event(
            r#"{"jsonrpc":"2.0","method":"event","params":{"type":"message.complete","session_id":"s42"}}"#,
        );
        match ev {
            Some(ChatEvent::Done { session_id }) => assert_eq!(session_id.as_deref(), Some("s42")),
            other => panic!("expected Done, got {:?}", other),
        }
    }

    #[test]
    fn ws_message_start_is_status() {
        let ev =
            ws_event(r#"{"jsonrpc":"2.0","method":"event","params":{"type":"message.start"}}"#);
        assert!(matches!(ev, Some(ChatEvent::Status { .. })));
    }

    #[test]
    fn ws_session_info_is_parsed() {
        // session.info is now parsed as SessionInfo event.
        let ev = ws_event(r#"{"jsonrpc":"2.0","method":"event","params":{"type":"session.info"}}"#);
        assert!(matches!(ev, Some(ChatEvent::SessionInfo { .. })));
    }

    // ── ADR-005: Remote/SSH scheme conversion (P3.2) ────────────────────────

    #[test]
    fn to_ws_url_converts_http_to_ws() {
        assert_eq!(to_ws_url("http://1.2.3.4:8642"), "ws://1.2.3.4:8642");
    }

    #[test]
    fn to_ws_url_converts_https_to_wss() {
        assert_eq!(
            to_ws_url("https://remote.example.com:9000"),
            "wss://remote.example.com:9000"
        );
    }

    #[test]
    fn to_ws_url_preserves_path_and_query() {
        assert_eq!(
            to_ws_url("http://127.0.0.1:9421/api/ws?token=abc"),
            "ws://127.0.0.1:9421/api/ws?token=abc"
        );
    }

    #[test]
    fn to_ws_url_idempotent_if_already_ws() {
        // Already-ws URLs pass through unchanged (no double-conversion).
        assert_eq!(to_ws_url("ws://host:1/path"), "ws://host:1/path");
        assert_eq!(to_ws_url("wss://host:1/path"), "wss://host:1/path");
    }

    #[test]
    fn to_ws_url_ssh_tunnel_localhost() {
        // SSH tunnel exposes a loopback http URL; must become ws://.
        assert_eq!(to_ws_url("http://127.0.0.1:18642"), "ws://127.0.0.1:18642");
    }

    // ── T5 (ADR-006 audit): event vocabulary fixes ──────────────────────────

    #[test]
    fn tool_complete_parses_duration_seconds() {
        // Backend (server.py:3695) emits duration_s as a float (seconds), but
        // the frontend contract expects duration_ms. The parser must convert.
        let raw = r#"{"jsonrpc":"2.0","method":"event","params":{"type":"tool.complete","session_id":"s1","payload":{"tool_id":"tc1","name":"read_file","duration_s":1.5,"result":"ok"}}}"#;
        match parse_ws_message(raw) {
            Some(ChatEvent::ToolComplete {
                name, duration_ms, ..
            }) => {
                assert_eq!(name, "read_file");
                // 1.5s → 1500ms
                assert_eq!(duration_ms, 1500, "1.5s must convert to 1500ms");
            }
            other => panic!("expected ToolComplete, got {:?}", other),
        }
    }

    #[test]
    fn tool_complete_defaults_duration_to_zero_when_absent() {
        let raw = r#"{"jsonrpc":"2.0","method":"event","params":{"type":"tool.complete","session_id":"s1","payload":{"tool_id":"tc1","name":"search"}}}"#;
        match parse_ws_message(raw) {
            Some(ChatEvent::ToolComplete { duration_ms, .. }) => {
                assert_eq!(duration_ms, 0, "missing duration_s must default to 0");
            }
            other => panic!("expected ToolComplete, got {:?}", other),
        }
    }

    #[test]
    fn approval_request_event_parsed() {
        // Wire format from _emit_approval_request (server.py:1147-1166).
        let raw = r#"{"jsonrpc":"2.0","method":"event","params":{"type":"approval.request","session_id":"s1","payload":{"request_id":"apr1","name":"bash","command":"rm -rf /tmp","command_class":"dangerous","choices":["once","deny"]}}}"#;
        match parse_ws_message(raw) {
            Some(ChatEvent::ApprovalRequest {
                request_id,
                tool_name,
                tool_input,
                command_class,
                ..
            }) => {
                assert_eq!(request_id, "apr1");
                assert_eq!(tool_name, "bash");
                assert_eq!(tool_input, "rm -rf /tmp");
                assert_eq!(command_class, "dangerous");
            }
            other => panic!("expected ApprovalRequest, got {:?}", other),
        }
    }
}
