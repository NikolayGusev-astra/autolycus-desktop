// src-tauri/src/hermes_protocol.rs
// Typed DTOs for the Hermes JSON-RPC WebSocket protocol (ADR-004 / Phase 0).
//
// This module provides strongly-typed request/response structures for the
// /api/ws JSON-RPC 2.0 channel. The wire format is:
//   Request:  {"jsonrpc":"2.0","id":N,"method":"...","params":{...}}
//   Response: {"jsonrpc":"2.0","id":N,"result":{...}} or {"error":{...}}
//   Event:    {"jsonrpc":"2.0","method":"event","params":{"type":"...","payload":{...}}}
//
// All types use serde with `rename_all = "camelCase"` to match the Python
// backend's Pydantic models (server.py). Enums use external tagging for
// discriminated unions where the backend uses a single field.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// JSON-RPC 2.0 version string (constant per spec).
pub const JSONRPC_VERSION: &str = "2.0";

/// Generate a fresh JSON-RPC request ID.
pub fn next_request_id() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

// ── Request / Response envelope ──────────────────────────────────────────────

/// Generic JSON-RPC 2.0 request.
#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcRequest<P> {
    pub jsonrpc: &'static str,
    pub id: u64,
    pub method: &'static str,
    pub params: P,
}

impl<P> JsonRpcRequest<P> {
    pub fn new(id: u64, method: &'static str, params: P) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION,
            id,
            method,
            params,
        }
    }
}

/// Generic JSON-RPC 2.0 response (success).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse<R> {
    pub jsonrpc: String,
    pub id: u64,
    pub result: R,
}

/// JSON-RPC 2.0 error object.
#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    pub data: Option<Value>,
}

/// Generic JSON-RPC 2.0 response (may be success or error).
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum JsonRpcResult<R> {
    Success(JsonRpcResponse<R>),
    Error {
        jsonrpc: String,
        id: u64,
        error: JsonRpcError,
    },
}

impl<R> JsonRpcResult<R> {
    /// Extract the result, converting error to a typed error.
    pub fn into_result(self) -> Result<R, JsonRpcError> {
        match self {
            JsonRpcResult::Success(r) => Ok(r.result),
            JsonRpcResult::Error { error, .. } => Err(error),
        }
    }
}

/// Streaming event envelope (method == "event").
#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcEvent<P> {
    pub jsonrpc: String,
    pub method: String, // always "event"
    pub params: P,
}

// ── session.create ───────────────────────────────────────────────────────────

/// Request params for `session.create`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionCreateParams {
    /// Session source: "desktop" (chat), "briefing_smart" (briefing), etc.
    /// Controls visibility in the feed (sessions.rs filters by source).
    pub source: String,
    /// Terminal columns for formatting (backend may use for table rendering).
    pub cols: u16,
}

/// Response result for `session.create`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionCreateResult {
    /// The new session ID (UUID).
    pub session_id: String,
}

/// Build a typed `session.create` request.
pub fn build_session_create_request(
    id: u64,
    source: &str,
    cols: u16,
) -> JsonRpcRequest<SessionCreateParams> {
    JsonRpcRequest::new(
        id,
        "session.create",
        SessionCreateParams {
            source: source.to_string(),
            cols,
        },
    )
}

// ── prompt.submit ────────────────────────────────────────────────────────────

/// Request params for `prompt.submit`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptSubmitParams {
    /// Session ID returned by `session.create`.
    pub session_id: String,
    /// User message text.
    pub text: String,
}

/// Build a typed `prompt.submit` request.
pub fn build_prompt_submit_request(
    id: u64,
    session_id: &str,
    text: &str,
) -> JsonRpcRequest<PromptSubmitParams> {
    JsonRpcRequest::new(
        id,
        "prompt.submit",
        PromptSubmitParams {
            session_id: session_id.to_string(),
            text: text.to_string(),
        },
    )
}

// ── Streaming events (method == "event") ─────────────────────────────────────

/// Discriminated union of all event types the backend can emit.
/// The `type` field inside `params` determines the variant.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GatewayEvent {
    /// Backend is ready to accept requests (emitted once after connect).
    #[serde(rename = "gateway.ready")]
    GatewayReady(GatewayReadyPayload),

    /// Token stream (message delta).
    #[serde(rename = "message.delta")]
    MessageDelta(MessageDeltaPayload),

    /// Token stream complete.
    #[serde(rename = "message.complete")]
    MessageComplete(MessageCompletePayload),

    /// Reasoning / chain-of-thought delta.
    #[serde(rename = "reasoning.delta")]
    ReasoningDelta(ReasoningDeltaPayload),

    /// Tool call started.
    #[serde(rename = "tool.start")]
    ToolStart(ToolStartPayload),

    /// Tool call completed.
    #[serde(rename = "tool.complete")]
    ToolComplete(ToolCompletePayload),

    /// Approval request for a tool action.
    #[serde(rename = "approval.request")]
    ApprovalRequest(ApprovalRequestPayload),

    /// Status update (e.g., "thinking", "processing").
    #[serde(rename = "status.update")]
    StatusUpdate(StatusUpdatePayload),

    /// Pipeline status (tokens used, cost, etc.).
    #[serde(rename = "pipeline.status")]
    PipelineStatus(PipelineStatusPayload),

    /// Error during streaming.
    #[serde(rename = "error")]
    Error(ErrorPayload),

    /// Turn ended (may carry session_id for turn linking).
    #[serde(rename = "message.end")]
    MessageEnd(MessageEndPayload),
}

/// Payload for `gateway.ready`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayReadyPayload {
    pub version: Option<String>,
    pub backend: Option<String>,
}

/// Payload for `message.delta` (token stream).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageDeltaPayload {
    pub session_id: String,
    #[serde(default)]
    pub payload: Option<MessageDeltaInner>,
    // Fallback fields for older event shapes
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub delta: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageDeltaInner {
    pub text: String,
}

impl MessageDeltaPayload {
    /// Extract the token text, checking all known locations.
    pub fn token_text(&self) -> Option<&str> {
        self.payload
            .as_ref()
            .map(|p| p.text.as_str())
            .or(self.text.as_deref())
            .or(self.delta.as_deref())
    }
}

/// Payload for `message.complete`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageCompletePayload {
    pub session_id: String,
    // Some backends include the full text on complete
    #[serde(default)]
    pub text: Option<String>,
}

/// Payload for `reasoning.delta`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReasoningDeltaPayload {
    pub session_id: String,
    #[serde(default)]
    pub payload: Option<ReasoningDeltaInner>,
    #[serde(default)]
    pub text: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReasoningDeltaInner {
    pub text: String,
}

impl ReasoningDeltaPayload {
    pub fn reasoning_text(&self) -> Option<&str> {
        self.payload
            .as_ref()
            .map(|p| p.text.as_str())
            .or(self.text.as_deref())
    }
}

/// Payload for `tool.start`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolStartPayload {
    pub session_id: String,
    pub tool_id: String,
    pub name: String,
}

/// Payload for `tool.complete`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCompletePayload {
    pub session_id: String,
    pub tool_id: String,
    pub name: String,
    #[serde(default)]
    pub output: Option<String>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
}

/// Payload for `approval.request`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalRequestPayload {
    pub session_id: String,
    pub request_id: String,
    pub tool_id: String,
    pub name: String,
    pub command: Option<String>,
    pub tool_input: Option<String>,
    pub action: Option<String>,
    pub message: Option<String>,
    pub command_class: Option<String>,
    pub smart_denied: Option<bool>,
    pub allow_permanent: Option<bool>,
}

/// Payload for `status.update`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusUpdatePayload {
    pub session_id: String,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
}

/// Payload for `pipeline.status`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PipelineStatusPayload {
    pub session_id: String,
    pub backend: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub tokens_used: Option<u64>,
    #[serde(default)]
    pub tokens_limit: Option<u64>,
    #[serde(default)]
    pub cost_usd: Option<f64>,
}

/// Payload for `error` streaming event.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorPayload {
    pub session_id: Option<String>,
    pub message: String,
    #[serde(default)]
    pub code: Option<String>,
}

/// Payload for `message.end`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageEndPayload {
    pub session_id: String,
    #[serde(default)]
    pub text: Option<String>,
}

/// Parse a raw JSON-RPC event envelope into a GatewayEvent.
pub fn parse_gateway_event(value: &Value) -> Option<GatewayEvent> {
    // Must be an event envelope
    let is_event = value
        .get("method")
        .and_then(|m| m.as_str())
        .map(|m| m == "event")
        .unwrap_or(false);
    if !is_event {
        return None;
    }

    // Extract params and try to deserialize as GatewayEvent
    let params = value.get("params")?;
    serde_json::from_value(params.clone()).ok()
}

// ── Fixtures for testing (Phase 0) ───────────────────────────────────────────

/// Test fixtures mirroring real upstream wire formats.
/// These are used in unit tests to validate parsing without a live backend.
pub mod fixtures {
    use super::*;
    use serde_json::json;

    /// A `session.create` response.
    pub fn session_create_response(id: u64, session_id: &str) -> Value {
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": { "sessionId": session_id }
        })
    }

    /// A `prompt.submit` response (ack only; real content streams as events).
    pub fn prompt_submit_response(id: u64) -> Value {
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {}
        })
    }

    /// `gateway.ready` event.
    pub fn gateway_ready_event() -> Value {
        json!({
            "jsonrpc": "2.0",
            "method": "event",
            "params": {
                "type": "gateway.ready",
                "version": "0.5.8",
                "backend": "hermes-agent"
            }
        })
    }

    /// `message.delta` (token) event — real wire format with payload.text.
    pub fn message_delta_event(session_id: &str, token: &str) -> Value {
        json!({
            "jsonrpc": "2.0",
            "method": "event",
            "params": {
                "type": "message.delta",
                "sessionId": session_id,
                "payload": { "text": token }
            }
        })
    }

    /// `message.delta` event with legacy text field (fallback).
    pub fn message_delta_event_legacy(session_id: &str, token: &str) -> Value {
        json!({
            "jsonrpc": "2.0",
            "method": "event",
            "params": {
                "type": "message.delta",
                "sessionId": session_id,
                "text": token
            }
        })
    }

    /// `reasoning.delta` event.
    pub fn reasoning_delta_event(session_id: &str, text: &str) -> Value {
        json!({
            "jsonrpc": "2.0",
            "method": "event",
            "params": {
                "type": "reasoning.delta",
                "sessionId": session_id,
                "payload": { "text": text }
            }
        })
    }

    /// `tool.start` event.
    pub fn tool_start_event(session_id: &str, tool_id: &str, name: &str) -> Value {
        json!({
            "jsonrpc": "2.0",
            "method": "event",
            "params": {
                "type": "tool.start",
                "sessionId": session_id,
                "toolId": tool_id,
                "name": name
            }
        })
    }

    /// `tool.complete` event.
    pub fn tool_complete_event(session_id: &str, tool_id: &str, name: &str, output: &str) -> Value {
        json!({
            "jsonrpc": "2.0",
            "method": "event",
            "params": {
                "type": "tool.complete",
                "sessionId": session_id,
                "toolId": tool_id,
                "name": name,
                "output": output,
                "durationMs": 42
            }
        })
    }

    /// `approval.request` event.
    pub fn approval_request_event(
        session_id: &str,
        request_id: &str,
        tool_id: &str,
        name: &str,
        command: &str,
    ) -> Value {
        json!({
            "jsonrpc": "2.0",
            "method": "event",
            "params": {
                "type": "approval.request",
                "sessionId": session_id,
                "requestId": request_id,
                "toolId": tool_id,
                "name": name,
                "command": command,
                "action": "execute",
                "commandClass": "write"
            }
        })
    }

    /// `status.update` event.
    pub fn status_update_event(session_id: &str, text: &str) -> Value {
        json!({
            "jsonrpc": "2.0",
            "method": "event",
            "params": {
                "type": "status.update",
                "sessionId": session_id,
                "text": text
            }
        })
    }

    /// `pipeline.status` event.
    pub fn pipeline_status_event(
        session_id: &str,
        backend: &str,
        model: &str,
        tokens_used: u64,
        tokens_limit: u64,
        cost_usd: f64,
    ) -> Value {
        json!({
            "jsonrpc": "2.0",
            "method": "event",
            "params": {
                "type": "pipeline.status",
                "sessionId": session_id,
                "backend": backend,
                "model": model,
                "tokensUsed": tokens_used,
                "tokensLimit": tokens_limit,
                "costUsd": cost_usd
            }
        })
    }

    /// `error` streaming event.
    pub fn error_event(session_id: Option<&str>, message: &str) -> Value {
        let mut params = json!({
            "type": "error",
            "message": message
        });
        if let Some(sid) = session_id {
            params["sessionId"] = json!(sid);
        }
        json!({
            "jsonrpc": "2.0",
            "method": "event",
            "params": params
        })
    }

    /// `message.end` event.
    pub fn message_end_event(session_id: &str) -> Value {
        json!({
            "jsonrpc": "2.0",
            "method": "event",
            "params": {
                "type": "message.end",
                "sessionId": session_id
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn session_create_request_serializes_correctly() {
        let req = build_session_create_request(1, "desktop", 96);
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["jsonrpc"], "2.0");
        assert_eq!(json["id"], 1);
        assert_eq!(json["method"], "session.create");
        assert_eq!(json["params"]["source"], "desktop");
        assert_eq!(json["params"]["cols"], 96);
    }

    #[test]
    fn prompt_submit_request_serializes_correctly() {
        let req = build_prompt_submit_request(2, "sess-123", "Hello world");
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["method"], "prompt.submit");
        assert_eq!(json["params"]["sessionId"], "sess-123");
        assert_eq!(json["params"]["text"], "Hello world");
    }

    #[test]
    fn parse_gateway_ready_event() {
        let raw = fixtures::gateway_ready_event();
        let event = parse_gateway_event(&raw).unwrap();
        match event {
            GatewayEvent::GatewayReady(payload) => {
                assert_eq!(payload.version, Some("0.5.8".to_string()));
            }
            _ => panic!("expected GatewayReady"),
        }
    }

    #[test]
    fn parse_message_delta_event_real_format() {
        let raw = fixtures::message_delta_event("sess-123", "Hello");
        let event = parse_gateway_event(&raw).unwrap();
        match event {
            GatewayEvent::MessageDelta(payload) => {
                assert_eq!(payload.token_text(), Some("Hello"));
            }
            _ => panic!("expected MessageDelta"),
        }
    }

    #[test]
    fn parse_message_delta_event_legacy_format() {
        let raw = fixtures::message_delta_event_legacy("sess-123", "Hello");
        let event = parse_gateway_event(&raw).unwrap();
        match event {
            GatewayEvent::MessageDelta(payload) => {
                assert_eq!(payload.token_text(), Some("Hello"));
            }
            _ => panic!("expected MessageDelta"),
        }
    }

    #[test]
    fn parse_reasoning_delta_event() {
        let raw = fixtures::reasoning_delta_event("sess-123", "thinking...");
        let event = parse_gateway_event(&raw).unwrap();
        match event {
            GatewayEvent::ReasoningDelta(payload) => {
                assert_eq!(payload.reasoning_text(), Some("thinking..."));
            }
            _ => panic!("expected ReasoningDelta"),
        }
    }

    #[test]
    fn parse_tool_start_then_complete() {
        let start = fixtures::tool_start_event("sess-123", "t1", "read_file");
        let event = parse_gateway_event(&start).unwrap();
        match event {
            GatewayEvent::ToolStart(p) => {
                assert_eq!(p.name, "read_file");
                assert_eq!(p.tool_id, "t1");
            }
            _ => panic!("expected ToolStart"),
        }

        let complete = fixtures::tool_complete_event("sess-123", "t1", "read_file", "file content");
        let event = parse_gateway_event(&complete).unwrap();
        match event {
            GatewayEvent::ToolComplete(p) => {
                assert_eq!(p.name, "read_file");
                assert_eq!(p.output, Some("file content".to_string()));
            }
            _ => panic!("expected ToolComplete"),
        }
    }

    #[test]
    fn parse_approval_request() {
        let raw =
            fixtures::approval_request_event("sess-123", "req-1", "t1", "write_file", "echo hi");
        let event = parse_gateway_event(&raw).unwrap();
        match event {
            GatewayEvent::ApprovalRequest(p) => {
                assert_eq!(p.request_id, "req-1");
                assert_eq!(p.name, "write_file");
                assert_eq!(p.command, Some("echo hi".to_string()));
            }
            _ => panic!("expected ApprovalRequest"),
        }
    }

    #[test]
    fn parse_message_end_carries_session_id() {
        let raw = fixtures::message_end_event("desk-123-abc");
        let event = parse_gateway_event(&raw).unwrap();
        match event {
            GatewayEvent::MessageEnd(p) => {
                assert_eq!(p.session_id, "desk-123-abc");
            }
            _ => panic!("expected MessageEnd"),
        }
    }

    #[test]
    fn json_rpc_response_roundtrip() {
        let result = SessionCreateResult {
            session_id: "test-123".to_string(),
        };
        let response = JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: 1,
            result: result.clone(),
        };
        let json = serde_json::to_value(&response).unwrap();
        let parsed: JsonRpcResponse<SessionCreateResult> = serde_json::from_value(json).unwrap();
        assert_eq!(parsed.result.session_id, "test-123");
    }
}
