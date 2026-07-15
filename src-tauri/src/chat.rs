// src-tauri/src/chat.rs
// Chat streaming: sendMessage with SSE, API fallback, session management
// Ported from fathah/hermes-desktop src/main/hermes.ts (chat part)

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter};

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
        if let Some(_usage) = parsed.get("usage") {
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
        "message.end" | "done" => {
            // upstream emits session_id in params on message.end; the frontend
            // pins currentSessionId from it (ChatView.tsx:201-203).
            let session_id = value
                .get("params")
                .and_then(|p| p.get("session_id"))
                .and_then(|s| s.as_str())
                .map(|s| s.to_string());
            Some(ChatEvent::Done { session_id })
        }
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
/// from `HERMES_DASHBOARD_SESSION_TOKEN` (Phase 0: the token is supplied by the
/// already-running `hermes serve` process; Phase 1 will have Steersman spawn
/// `hermes serve` itself and generate the token).
async fn send_via_ws_local(
    gateway_state: &GatewayState,
    request: &SendMessageRequest,
    app_handle: &AppHandle,
) -> Result<String, String> {
    let port = gateway::get_gateway_port(gateway_state, None)
        .ok_or("Gateway not available (no port)")?;
    // Token: prefer the one Steersman generated for the spawned process (P1);
    // fall back to HERMES_DASHBOARD_SESSION_TOKEN env for Phase 0 compatibility.
    let token = gateway::get_gateway_session_token(gateway_state, None)
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
    let ws_url = format!("ws://127.0.0.1:{}/api/ws?token={}", port, token);
    // send_message_via_ws returns Result<_, WsError> (typed); convert to String
    // at the Tauri-command boundary. Full Result<_,String> -> typed migration
    // across chat.rs is out of P3.3 scope (only the WS path is typed here).
    crate::ws_transport::send_message_via_ws(
        &ws_url,
        request.session_id.as_deref(),
        &request.text,
        app_handle,
    )
    .await
    .map_err(|e| e.to_string())
}

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
    // Note: model_config is no longer fetched here — the WS transport sends
    // only {session_id, text}; reasoning_effort/verbosity/etc. live in the
    // backend's config.yaml, not the per-request body (ADR-004).
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

            // ADR-004: the local backend is `hermes serve`, which exposes the
            // WebSocket /api/ws transport. There is no HTTP fallback — the
            // legacy /v1/chat/completions endpoint does not exist on a real
            // backend and has been removed (P2.1).
            send_via_ws_local(gateway_state, &request, app_handle).await
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
                crate::ssh::start_ssh_tunnel(ssh_state, ssh)
                    .map_err(|e| format!("SSH tunnel failed: {}", e))?;
            }

            let tunnel_url = crate::ssh::get_tunnel_url(ssh_state)
                .ok_or("SSH tunnel not available")?;

            // The remote backend's session token (same resolution as Remote).
            let tunneled_token = config::get_api_server_key(hermes_home, None)
                .or_else(|| {
                    dirs::home_dir().and_then(|h| {
                        config::get_api_server_key(&h.join(".hermes"), None)
                    })
                })
                .or_else(|| std::env::var("API_SERVER_KEY").ok().filter(|v| !v.is_empty()))
                .unwrap_or_default();

            send_via_ws_remote(&tunnel_url, &tunneled_token, &request, app_handle).await
        }
    }
}

/// Resolve an autonomy policy name to a compact system-prompt text (practice 3).
/// Returns None for unknown/empty policies so no system message is injected.
/// Each policy is ONE sentence — the GPT-5.6 guide warns against repetitive
/// "ask first" boilerplate that annoys users on safe actions.
pub fn autonomy_policy_text(policy: Option<&str>) -> Option<&'static str> {
    match policy? {
        "" | "auto" => None,
        "readonly" => Some(
            "You are in read-only mode: gather information and report. Do not create, modify, or delete anything.",
        ),
        "local" => Some(
            "You may make local changes and run checks without asking. External writes (messages, tickets, purchases, deletions) require user confirmation.",
        ),
        "confirm-external" => Some(
            "All actions that affect external systems require explicit user confirmation. Local analysis and reporting are autonomous.",
        ),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readonly_policy_produces_restriction() {
        let text = autonomy_policy_text(Some("readonly"));
        assert!(text.is_some());
        assert!(text.unwrap().contains("read-only mode"));
    }

    #[test]
    fn local_policy_allows_local_actions() {
        let text = autonomy_policy_text(Some("local"));
        assert!(text.unwrap().contains("local changes"));
    }

    #[test]
    fn confirm_external_requires_confirmation() {
        let text = autonomy_policy_text(Some("confirm-external"));
        assert!(text.unwrap().contains("explicit user confirmation"));
    }

    #[test]
    fn empty_policy_returns_none() {
        assert_eq!(autonomy_policy_text(None), None);
        assert_eq!(autonomy_policy_text(Some("")), None);
        assert_eq!(autonomy_policy_text(Some("auto")), None);
    }

    #[test]
    fn unknown_policy_returns_none() {
        assert_eq!(autonomy_policy_text(Some("bogus")), None);
    }

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

    // ── ADR-005: Remote/SSH scheme conversion (P3.2) ────────────────────────

    #[test]
    fn to_ws_url_converts_http_to_ws() {
        assert_eq!(to_ws_url("http://1.2.3.4:8642"), "ws://1.2.3.4:8642");
    }

    #[test]
    fn to_ws_url_converts_https_to_wss() {
        assert_eq!(to_ws_url("https://remote.example.com:9000"), "wss://remote.example.com:9000");
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
}
