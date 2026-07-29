// src-tauri/src/hermes_protocol.rs
// Typed DTOs for the Hermes JSON-RPC WebSocket protocol (ADR-004 / Phase 0).
//
// This module provides strongly-typed request/response structures for the
// /api/ws JSON-RPC 2.0 channel. The wire format is:
//   Request:  {"jsonrpc":"2.0","id":N,"method":"...","params":{...}}
//   Response: {"jsonrpc":"2.0","id":N,"result":{...}} or {"error":{...}}
//   Event:    {"jsonrpc":"2.0","method":"event","params":{"type":"...","session_id":"...","payload":{...}}}
//
// All types use serde with snake_case to match Hermes wire format.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// JSON-RPC 2.0 version string (constant per spec).
pub const JSONRPC_VERSION: &str = "2.0";

/// Desktop contract version this build implements. Bumping the range is a
/// deliberate release decision: a lower value means "this desktop supports
/// Hermes back to contract N", a higher value means "requires Hermes with
/// at least contract N". The single supported version today is 4.
pub const MIN_HERMES_DESKTOP_CONTRACT: u32 = 4;
pub const MAX_HERMES_DESKTOP_CONTRACT: u32 = 4;

/// Result of a compatibility handshake against the connected Hermes backend.
///
/// Stored separately from the network [`ConnectionState`](crate::ws_transport::ConnectionState):
/// a live socket is not evidence of compatibility. `prompt.submit` must not
/// proceed while this is `Unknown` or an incompatible variant.
#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeCompatibility {
    /// Handshake not yet performed (fresh connection).
    Unknown,
    /// Handshake in progress.
    Checking,
    /// Backend contract is within the supported range.
    Compatible { contract: u32 },
    /// Backend is older than the minimum this desktop supports.
    HermesUpgradeRequired { received: u32, minimum: u32 },
    /// Backend is newer than the maximum this desktop supports.
    DesktopUpgradeRequired { received: u32, maximum: u32 },
}

impl RuntimeCompatibility {
    /// Evaluate a received desktop_contract against the supported range.
    pub fn evaluate(received: u32) -> Self {
        if received < MIN_HERMES_DESKTOP_CONTRACT {
            RuntimeCompatibility::HermesUpgradeRequired {
                received,
                minimum: MIN_HERMES_DESKTOP_CONTRACT,
            }
        } else if received > MAX_HERMES_DESKTOP_CONTRACT {
            RuntimeCompatibility::DesktopUpgradeRequired {
                received,
                maximum: MAX_HERMES_DESKTOP_CONTRACT,
            }
        } else {
            RuntimeCompatibility::Compatible { contract: received }
        }
    }

    /// True when the backend is compatible and user work may proceed.
    pub fn is_compatible(&self) -> bool {
        matches!(self, RuntimeCompatibility::Compatible { .. })
    }
}

/// Generate a fresh JSON-RPC request ID.
pub fn next_request_id() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// Protocol-level errors for event parsing.
#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
pub enum ProtocolError {
    #[error("Not an event envelope (method != \"event\")")]
    NotAnEvent,
    #[error("Missing or invalid params: {0}")]
    InvalidParams(String),
    #[error("Unknown event type: {0}")]
    UnknownType(String),
    #[error("Malformed known event '{event_type}': {error}")]
    MalformedKnown { event_type: String, error: String },
    #[error("JSON deserialization failed: {0}")]
    JsonError(String),
}

/// Result type for parsed gateway events.
pub type ParseResult = Result<Option<RoutedGatewayEvent>, ProtocolError>;

/// Event type returned by parse_gateway_event — includes session_id for routing.
#[derive(Debug, Clone, Deserialize)]
pub struct RoutedGatewayEvent {
    pub session_id: Option<String>,
    pub event: ParsedGatewayEvent,
}

/// Discriminated union of parsing outcomes.
/// - Known: successfully parsed a recognized event type
/// - UnknownType: forward-compatible unknown event
/// - MalformedKnown: recognized type but payload didn't match schema (log + telemetry!)
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum ParsedGatewayEvent {
    Known(GatewayEvent),
    UnknownType {
        event_type: String,
        session_id: Option<String>,
        payload: Value,
    },
    MalformedKnown {
        event_type: String,
        session_id: Option<String>,
        payload: Value,
        error: String,
    },
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

// ── session.create ───────────────────────────────────────────────────────────

/// Request params for `session.create`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SessionCreateParams {
    /// Session source: "desktop" (chat), "briefing_smart" (briefing), etc.
    /// Controls visibility in the feed (sessions.rs filters by source).
    pub source: String,
    /// Terminal columns for formatting (backend may use for table rendering).
    pub cols: u16,
    /// Optional profile scope. Hermes supports params.profile in session.create;
    /// for non-launch profiles this selects the state.db, config, skills, memory.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
}

/// Response result for `session.create`.
/// Matches real Hermes wire format with snake_case fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SessionCreateResult {
    /// The new session ID (UUID).
    pub session_id: String,
    /// The durable stored session ID.
    pub stored_session_id: String,
    /// Number of messages in the session.
    pub message_count: usize,
    /// Message history (empty on create).
    pub messages: Vec<serde_json::Value>,
    /// Session info including desktop_contract version.
    pub info: SessionCreateInfo,
}

/// Session creation info with desktop contract version.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SessionCreateInfo {
    pub desktop_contract: u32,
    /// The profile name the session was created under. Hermes returns this in
    /// session.create.info.profile_name; for the launch profile it may be absent.
    #[serde(default)]
    pub profile_name: Option<String>,
}

/// Build a typed `session.create` request.
pub fn build_session_create_request(
    id: u64,
    source: &str,
    cols: u16,
    profile: Option<&str>,
) -> JsonRpcRequest<SessionCreateParams> {
    JsonRpcRequest::new(
        id,
        "session.create",
        SessionCreateParams {
            source: source.to_string(),
            cols,
            profile: profile.map(|s| s.to_string()),
        },
    )
}

// ── session.resume ──────────────────────────────────────────────────────────

/// Request params for `session.resume` (reconnect reconciliation).
///
/// IMPORTANT wire contract: Hermes reads `params.session_id` (the DURABLE
/// stored ID), NOT `stored_session_id`. The backend returns `4006:
/// session_id required` if the field is absent. This struct's field name is
/// `session_id` to match the wire format; semantically it carries the durable
/// stored session ID.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SessionResumeParams {
    /// The durable stored session ID. Named `session_id` to match the Hermes
    /// wire format (tui_gateway/server.py reads `params.get("session_id")`).
    pub session_id: String,
    /// Optional profile scope. Sessions are profile-scoped on the backend.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    /// Optional terminal columns.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cols: Option<u16>,
}

/// Response result for `session.resume`.
///
/// Wire contract: the NEW live ID is in `session_id`; the durable ID that was
/// resumed is in `resumed` (or `session_key`). There is NO `stored_session_id`
/// field in the real Hermes response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SessionResumeResult {
    /// The NEW live session ID valid for the current connection.
    pub session_id: String,
    /// The durable stored session ID that was resumed.
    #[serde(default)]
    pub resumed: String,
    /// Alternative field name some Hermes versions use for the durable ID.
    #[serde(default)]
    pub session_key: Option<String>,
    /// Number of messages in the resumed session.
    #[serde(default)]
    pub message_count: usize,
    /// Recent message history (may be empty or truncated by the backend).
    #[serde(default)]
    pub messages: Vec<serde_json::Value>,
    /// Session info (may be empty object).
    #[serde(default)]
    pub info: serde_json::Value,
}

impl SessionResumeResult {
    /// The durable stored session ID, preferring `resumed` then `session_key`.
    pub fn durable_id(&self) -> &str {
        if !self.resumed.is_empty() {
            &self.resumed
        } else if let Some(key) = &self.session_key {
            key
        } else {
            ""
        }
    }
}

// ── prompt.submit ────────────────────────────────────────────────────────────

/// Request params for `prompt.submit`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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

/// Response result for `prompt.submit` (async TUI Gateway model).
/// Hermes returns immediately with status; content streams via events.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PromptSubmitResult {
    pub status: PromptSubmitStatus,
    #[serde(default)]
    pub turn_isolation: bool,
    #[serde(default)]
    pub text: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PromptSubmitStatus {
    Streaming,
    Queued,
    Steered,
    Rejected,
}

// ── Streaming events (method == "event") ─────────────────────────────────────

/// Two-stage event parsing: envelope -> params -> specific payload.
/// Hermes event structure:
///   {"jsonrpc":"2.0","method":"event","params":{"type":"...","session_id":"...","payload":{...}}}

#[derive(Debug, Clone, Deserialize)]
pub struct GatewayEventEnvelope {
    pub jsonrpc: String,
    pub method: String,
    pub params: GatewayEventParams,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GatewayEventParams {
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub payload: Value,
}

/// Discriminated union of all event types the backend can emit.
/// The payload is already extracted from params.payload.
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

    /// Message start (beginning of assistant reply).
    #[serde(rename = "message.start")]
    MessageStart(MessageStartPayload),

    /// Reasoning / chain-of-thought delta.
    #[serde(rename = "reasoning.delta")]
    ReasoningDelta(ReasoningDeltaPayload),

    /// Thinking delta (alternative to reasoning).
    #[serde(rename = "thinking.delta")]
    ThinkingDelta(ThinkingDeltaPayload),

    /// Tool call started.
    #[serde(rename = "tool.start")]
    ToolStart(ToolStartPayload),

    /// Tool call generating (streaming tool output).
    #[serde(rename = "tool.generating")]
    ToolGenerating(ToolGeneratingPayload),

    /// Tool call completed.
    #[serde(rename = "tool.complete")]
    ToolComplete(ToolCompletePayload),

    /// Approval request for a tool action.
    #[serde(rename = "approval.request")]
    ApprovalRequest(ApprovalRequestPayload),

    /// Clarification request from agent.
    #[serde(rename = "clarify.request")]
    ClarifyRequest(ClarifyRequestPayload),

    /// Sudo request for elevated operations.
    #[serde(rename = "sudo.request")]
    SudoRequest(SudoRequestPayload),

    /// Sudo session expired.
    #[serde(rename = "sudo.expire")]
    SudoExpire(SudoExpirePayload),

    /// Secret request (API keys, tokens).
    #[serde(rename = "secret.request")]
    SecretRequest(SecretRequestPayload),

    /// Secret session expired.
    #[serde(rename = "secret.expire")]
    SecretExpire(SecretExpirePayload),

    /// Session info (running state, model/provider, tools, skills, usage, stored_session_id, desktop_contract).
    #[serde(rename = "session.info")]
    SessionInfo(SessionInfoPayload),

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

    /// Notification show.
    #[serde(rename = "notification.show")]
    NotificationShow(NotificationShowPayload),

    /// Notification clear.
    #[serde(rename = "notification.clear")]
    NotificationClear(NotificationClearPayload),

    /// Unknown/forward-compatible event - preserves raw payload.
    Unknown {
        event_type: String,
        session_id: Option<String>,
        payload: Value,
    },
}

/// Payload for `gateway.ready` — Hermes sends skin inside payload.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct GatewayReadyPayload {
    pub skin: Option<String>,
}

/// Payload for `message.delta` (token stream).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MessageDeltaPayload {
    pub text: String,
}

/// Payload for `message.complete`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MessageCompletePayload {
    #[serde(default)]
    pub text: Option<String>,
}

/// Payload for `reasoning.delta`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ReasoningDeltaPayload {
    pub text: String,
}

/// Payload for `tool.start` — nested inside params.payload.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ToolStartPayload {
    pub tool_id: String,
    pub name: String,
    #[serde(default)]
    pub context: Value,
}

/// Payload for `tool.complete` — matches Hermes wire format.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ToolCompletePayload {
    pub tool_id: String,
    pub name: String,
    #[serde(default)]
    pub args: Value,
    #[serde(default)]
    pub result: Value,
    #[serde(default)]
    pub duration_s: Option<f64>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub result_text: Option<String>,
    #[serde(default)]
    pub inline_diff: Option<String>,
}

/// Payload for `approval.request` — nested inside params.payload.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ApprovalRequestPayload {
    pub request_id: String,
    #[allow(dead_code)]
    pub tool_id: String,
    pub name: String,
    #[allow(dead_code)]
    pub command: Option<String>,
    pub tool_input: Option<String>,
    pub action: Option<String>,
    pub message: Option<String>,
    pub command_class: Option<String>,
    #[allow(dead_code)]
    pub smart_denied: Option<bool>,
    pub allow_permanent: Option<bool>,
    #[serde(default)]
    pub choices: Vec<ApprovalChoice>,
}

/// Authoritative approval choices from Hermes (not derived from flags).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalChoice {
    Once,
    Session,
    Always,
    Deny,
}

/// Payload for `status.update` — nested inside params.payload.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct StatusUpdatePayload {
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
}

/// Payload for `pipeline.status` — nested inside params.payload.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PipelineStatusPayload {
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

/// Payload for `error` streaming event — nested inside params.payload.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ErrorPayload {
    pub message: String,
    #[serde(default)]
    pub code: Option<String>,
}

/// Payload for `message.end` — nested inside params.payload.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MessageEndPayload {
    #[serde(default)]
    pub text: Option<String>,
}

/// Payload for `message.start` — beginning of assistant reply.
/// Hermes emits this with empty/no payload (just `_emit("message.start", sid)`).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MessageStartPayload {
    #[serde(default)]
    pub text: Option<String>,
}

/// Payload for `thinking.delta` — alternative to reasoning.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ThinkingDeltaPayload {
    pub text: String,
}

/// Payload for `tool.generating` — streaming tool output.
/// Hermes sends only `name`, `tool_id` is optional.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ToolGeneratingPayload {
    pub name: String,
    #[serde(default)]
    pub tool_id: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
}

/// Payload for `clarify.request` — agent needs clarification.
/// Hermes uses `choices` field (not `options`).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ClarifyRequestPayload {
    pub request_id: String,
    pub question: String,
    #[serde(default)]
    pub choices: Vec<String>,
}

/// Payload for `sudo.request` — elevated operations.
/// Hermes sends only request_id (reason is optional).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SudoRequestPayload {
    pub request_id: String,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

/// Payload for `sudo.expire` — sudo session expired.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SudoExpirePayload {
    pub request_id: String,
}

/// Payload for `secret.request` — API keys, tokens.
/// Hermes sends: request_id, prompt, env_var, metadata.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SecretRequestPayload {
    pub request_id: String,
    pub prompt: String,
    pub env_var: String,
    #[serde(default)]
    pub metadata: Option<Value>,
}

/// Payload for `secret.expire` — secret session expired.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SecretExpirePayload {
    pub request_id: String,
}

/// Payload for `session.info` — running state, model/provider, tools, skills, usage, stored_session_id, desktop_contract.
/// Hermes uses `running: bool` (not `state`), tools/skills are objects grouped by category,
/// and includes extra fields like reasoning_effort, service_tier, approval_mode, etc.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SessionInfoPayload {
    #[serde(default)]
    pub stored_session_id: String,

    #[serde(default)]
    pub running: bool,

    #[serde(default)]
    pub model: Option<String>,

    #[serde(default)]
    pub provider: Option<String>,

    #[serde(default)]
    pub tools: Value,

    #[serde(default)]
    pub skills: Value,

    #[serde(default)]
    pub usage: Option<Value>,

    #[serde(default)]
    pub desktop_contract: Option<u32>,

    // Capture any additional fields Hermes may add
    #[serde(flatten)]
    pub extra: std::collections::HashMap<String, Value>,
}

/// Payload for `notification.show`.
/// Hermes sends: id, key, text, level, kind, ttl_ms.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct NotificationShowPayload {
    pub id: String,
    pub key: String,
    pub text: String,
    #[serde(default)]
    pub level: Option<String>, // "info", "warning", "error"
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub ttl_ms: Option<u64>,
}

/// Payload for `notification.clear`.
/// Hermes sends: key.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct NotificationClearPayload {
    pub key: String,
}

/// Parse a raw JSON-RPC value into a RoutedGatewayEvent using two-stage parsing.
/// First extracts envelope.params, then dispatches on params.event_type.
/// Returns ParsedGatewayEvent with Known/UnknownType/MalformedKnown discrimination.
pub fn parse_gateway_event(value: &Value) -> ParseResult {
    // Must be an event envelope
    let is_event = value
        .get("method")
        .and_then(|m| m.as_str())
        .map(|m| m == "event")
        .unwrap_or(false);
    if !is_event {
        return Ok(None);
    }

    // Extract params and deserialize as GatewayEventParams
    let params = value
        .get("params")
        .ok_or_else(|| ProtocolError::InvalidParams("missing params".into()))?;
    let params: GatewayEventParams = serde_json::from_value(params.clone())
        .map_err(|e| ProtocolError::JsonError(e.to_string()))?;

    // Dispatch based on event type
    let event_type = params.event_type.as_str();
    let session_id = params.session_id;
    let payload = params.payload;

    let parsed_event = match event_type {
        "gateway.ready" => match serde_json::from_value(payload.clone()) {
            Ok(p) => ParsedGatewayEvent::Known(GatewayEvent::GatewayReady(p)),
            Err(e) => ParsedGatewayEvent::MalformedKnown {
                event_type: event_type.to_string(),
                session_id: session_id.clone(),
                payload: payload.clone(),
                error: e.to_string(),
            },
        },
        "message.delta" => match serde_json::from_value(payload.clone()) {
            Ok(p) => ParsedGatewayEvent::Known(GatewayEvent::MessageDelta(p)),
            Err(e) => ParsedGatewayEvent::MalformedKnown {
                event_type: event_type.to_string(),
                session_id: session_id.clone(),
                payload: payload.clone(),
                error: e.to_string(),
            },
        },
        "message.complete" => match serde_json::from_value(payload.clone()) {
            Ok(p) => ParsedGatewayEvent::Known(GatewayEvent::MessageComplete(p)),
            Err(e) => ParsedGatewayEvent::MalformedKnown {
                event_type: event_type.to_string(),
                session_id: session_id.clone(),
                payload: payload.clone(),
                error: e.to_string(),
            },
        },
        "message.start" => {
            // Hermes emits message.start with NO payload (null/missing)
            // Use empty object as default
            let payload_for_parse = if payload.is_null()
                || payload.as_object().map(|o| o.is_empty()).unwrap_or(false)
            {
                serde_json::Value::Object(serde_json::Map::new())
            } else {
                payload.clone()
            };
            match serde_json::from_value(payload_for_parse) {
                Ok(p) => ParsedGatewayEvent::Known(GatewayEvent::MessageStart(p)),
                Err(e) => ParsedGatewayEvent::MalformedKnown {
                    event_type: event_type.to_string(),
                    session_id: session_id.clone(),
                    payload: payload.clone(),
                    error: e.to_string(),
                },
            }
        }
        "reasoning.delta" => match serde_json::from_value(payload.clone()) {
            Ok(p) => ParsedGatewayEvent::Known(GatewayEvent::ReasoningDelta(p)),
            Err(e) => ParsedGatewayEvent::MalformedKnown {
                event_type: event_type.to_string(),
                session_id: session_id.clone(),
                payload: payload.clone(),
                error: e.to_string(),
            },
        },
        "thinking.delta" => match serde_json::from_value(payload.clone()) {
            Ok(p) => ParsedGatewayEvent::Known(GatewayEvent::ThinkingDelta(p)),
            Err(e) => ParsedGatewayEvent::MalformedKnown {
                event_type: event_type.to_string(),
                session_id: session_id.clone(),
                payload: payload.clone(),
                error: e.to_string(),
            },
        },
        "tool.start" => match serde_json::from_value(payload.clone()) {
            Ok(p) => ParsedGatewayEvent::Known(GatewayEvent::ToolStart(p)),
            Err(e) => ParsedGatewayEvent::MalformedKnown {
                event_type: event_type.to_string(),
                session_id: session_id.clone(),
                payload: payload.clone(),
                error: e.to_string(),
            },
        },
        "tool.generating" => match serde_json::from_value(payload.clone()) {
            Ok(p) => ParsedGatewayEvent::Known(GatewayEvent::ToolGenerating(p)),
            Err(e) => ParsedGatewayEvent::MalformedKnown {
                event_type: event_type.to_string(),
                session_id: session_id.clone(),
                payload: payload.clone(),
                error: e.to_string(),
            },
        },
        "tool.complete" => match serde_json::from_value(payload.clone()) {
            Ok(p) => ParsedGatewayEvent::Known(GatewayEvent::ToolComplete(p)),
            Err(e) => ParsedGatewayEvent::MalformedKnown {
                event_type: event_type.to_string(),
                session_id: session_id.clone(),
                payload: payload.clone(),
                error: e.to_string(),
            },
        },
        "approval.request" => match serde_json::from_value(payload.clone()) {
            Ok(p) => ParsedGatewayEvent::Known(GatewayEvent::ApprovalRequest(p)),
            Err(e) => ParsedGatewayEvent::MalformedKnown {
                event_type: event_type.to_string(),
                session_id: session_id.clone(),
                payload: payload.clone(),
                error: e.to_string(),
            },
        },
        "clarify.request" => match serde_json::from_value(payload.clone()) {
            Ok(p) => ParsedGatewayEvent::Known(GatewayEvent::ClarifyRequest(p)),
            Err(e) => ParsedGatewayEvent::MalformedKnown {
                event_type: event_type.to_string(),
                session_id: session_id.clone(),
                payload: payload.clone(),
                error: e.to_string(),
            },
        },
        "sudo.request" => match serde_json::from_value(payload.clone()) {
            Ok(p) => ParsedGatewayEvent::Known(GatewayEvent::SudoRequest(p)),
            Err(e) => ParsedGatewayEvent::MalformedKnown {
                event_type: event_type.to_string(),
                session_id: session_id.clone(),
                payload: payload.clone(),
                error: e.to_string(),
            },
        },
        "sudo.expire" => match serde_json::from_value(payload.clone()) {
            Ok(p) => ParsedGatewayEvent::Known(GatewayEvent::SudoExpire(p)),
            Err(e) => ParsedGatewayEvent::MalformedKnown {
                event_type: event_type.to_string(),
                session_id: session_id.clone(),
                payload: payload.clone(),
                error: e.to_string(),
            },
        },
        "secret.request" => match serde_json::from_value(payload.clone()) {
            Ok(p) => ParsedGatewayEvent::Known(GatewayEvent::SecretRequest(p)),
            Err(e) => ParsedGatewayEvent::MalformedKnown {
                event_type: event_type.to_string(),
                session_id: session_id.clone(),
                payload: payload.clone(),
                error: e.to_string(),
            },
        },
        "secret.expire" => match serde_json::from_value(payload.clone()) {
            Ok(p) => ParsedGatewayEvent::Known(GatewayEvent::SecretExpire(p)),
            Err(e) => ParsedGatewayEvent::MalformedKnown {
                event_type: event_type.to_string(),
                session_id: session_id.clone(),
                payload: payload.clone(),
                error: e.to_string(),
            },
        },
        "session.info" => match serde_json::from_value(payload.clone()) {
            Ok(p) => ParsedGatewayEvent::Known(GatewayEvent::SessionInfo(p)),
            Err(e) => ParsedGatewayEvent::MalformedKnown {
                event_type: event_type.to_string(),
                session_id: session_id.clone(),
                payload: payload.clone(),
                error: e.to_string(),
            },
        },
        "status.update" => match serde_json::from_value(payload.clone()) {
            Ok(p) => ParsedGatewayEvent::Known(GatewayEvent::StatusUpdate(p)),
            Err(e) => ParsedGatewayEvent::MalformedKnown {
                event_type: event_type.to_string(),
                session_id: session_id.clone(),
                payload: payload.clone(),
                error: e.to_string(),
            },
        },
        "pipeline.status" => match serde_json::from_value(payload.clone()) {
            Ok(p) => ParsedGatewayEvent::Known(GatewayEvent::PipelineStatus(p)),
            Err(e) => ParsedGatewayEvent::MalformedKnown {
                event_type: event_type.to_string(),
                session_id: session_id.clone(),
                payload: payload.clone(),
                error: e.to_string(),
            },
        },
        "error" => match serde_json::from_value(payload.clone()) {
            Ok(p) => ParsedGatewayEvent::Known(GatewayEvent::Error(p)),
            Err(e) => ParsedGatewayEvent::MalformedKnown {
                event_type: event_type.to_string(),
                session_id: session_id.clone(),
                payload: payload.clone(),
                error: e.to_string(),
            },
        },
        "message.end" => match serde_json::from_value(payload.clone()) {
            Ok(p) => ParsedGatewayEvent::Known(GatewayEvent::MessageEnd(p)),
            Err(e) => ParsedGatewayEvent::MalformedKnown {
                event_type: event_type.to_string(),
                session_id: session_id.clone(),
                payload: payload.clone(),
                error: e.to_string(),
            },
        },
        "notification.show" => match serde_json::from_value(payload.clone()) {
            Ok(p) => ParsedGatewayEvent::Known(GatewayEvent::NotificationShow(p)),
            Err(e) => ParsedGatewayEvent::MalformedKnown {
                event_type: event_type.to_string(),
                session_id: session_id.clone(),
                payload: payload.clone(),
                error: e.to_string(),
            },
        },
        "notification.clear" => match serde_json::from_value(payload.clone()) {
            Ok(p) => ParsedGatewayEvent::Known(GatewayEvent::NotificationClear(p)),
            Err(e) => ParsedGatewayEvent::MalformedKnown {
                event_type: event_type.to_string(),
                session_id: session_id.clone(),
                payload: payload.clone(),
                error: e.to_string(),
            },
        },
        // Unknown event type - forward compatible
        _ => ParsedGatewayEvent::UnknownType {
            event_type: event_type.to_string(),
            session_id: session_id.clone(),
            payload: payload.clone(),
        },
    };

    Ok(Some(RoutedGatewayEvent {
        session_id,
        event: parsed_event,
    }))
}

// ── Fixtures for testing (Phase 0) ───────────────────────────────────────────

/// Test fixtures mirroring real upstream wire formats.
/// These are used in unit tests to validate parsing without a live backend.
pub mod fixtures {
    use super::*;
    use serde_json::json;

    /// Create a minimal event with just type and session_id for testing.
    fn event_with_payload(event_type: &str, session_id: &str, payload: Value) -> Value {
        json!({
            "jsonrpc": "2.0",
            "method": "event",
            "params": {
                "type": event_type,
                "session_id": session_id,
                "payload": payload
            }
        })
    }

    /// A `session.create` response matching real Hermes wire format.
    pub fn session_create_response(id: u64, session_id: &str) -> Value {
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "session_id": session_id,
                "stored_session_id": format!("stored-{}", session_id),
                "message_count": 0,
                "messages": [],
                "info": {
                    "desktop_contract": 4
                }
            }
        })
    }

    /// A `prompt.submit` response (ack only; real content streams as events).
    pub fn prompt_submit_response(id: u64) -> Value {
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "status": "streaming",
                "turn_isolation": true
            }
        })
    }

    /// `gateway.ready` event — Hermes sends skin inside payload, NO session_id in params.
    pub fn gateway_ready_event() -> Value {
        json!({
            "jsonrpc": "2.0",
            "method": "event",
            "params": {
                "type": "gateway.ready",
                "payload": { "skin": "default" }
            }
        })
    }

    /// `message.delta` (token) event — real wire format with payload.text.
    pub fn message_delta_event(session_id: &str, token: &str) -> Value {
        event_with_payload("message.delta", session_id, json!({ "text": token }))
    }

    /// `message.delta` event with legacy text field (fallback - no payload).
    pub fn message_delta_event_legacy(session_id: &str, token: &str) -> Value {
        json!({
            "jsonrpc": "2.0",
            "method": "event",
            "params": {
                "type": "message.delta",
                "session_id": session_id,
                "text": token
            }
        })
    }

    /// `reasoning.delta` event.
    pub fn reasoning_delta_event(session_id: &str, text: &str) -> Value {
        event_with_payload("reasoning.delta", session_id, json!({ "text": text }))
    }

    /// `tool.start` event.
    pub fn tool_start_event(session_id: &str, tool_id: &str, name: &str) -> Value {
        event_with_payload(
            "tool.start",
            session_id,
            json!({
                "tool_id": tool_id,
                "name": name
            }),
        )
    }

    /// `tool.complete` event — matches Hermes wire format.
    pub fn tool_complete_event(
        session_id: &str,
        tool_id: &str,
        name: &str,
        result_text: &str,
    ) -> Value {
        event_with_payload(
            "tool.complete",
            session_id,
            json!({
                "tool_id": tool_id,
                "name": name,
                "args": {},
                "result": { "text": result_text },
                "duration_s": 0.042,
                "summary": "completed",
                "result_text": result_text,
                "inline_diff": null
            }),
        )
    }

    /// `approval.request` event.
    pub fn approval_request_event(
        session_id: &str,
        request_id: &str,
        tool_id: &str,
        name: &str,
        command: &str,
    ) -> Value {
        event_with_payload(
            "approval.request",
            session_id,
            json!({
                "request_id": request_id,
                "tool_id": tool_id,
                "name": name,
                "command": command,
                "action": "execute",
                "command_class": "write"
            }),
        )
    }

    /// `status.update` event.
    pub fn status_update_event(session_id: &str, text: &str) -> Value {
        event_with_payload("status.update", session_id, json!({ "text": text }))
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
        event_with_payload(
            "pipeline.status",
            session_id,
            json!({
                "backend": backend,
                "model": model,
                "tokens_used": tokens_used,
                "tokens_limit": tokens_limit,
                "cost_usd": cost_usd
            }),
        )
    }

    /// `error` streaming event.
    pub fn error_event(session_id: Option<&str>, message: &str) -> Value {
        let mut payload = json!({ "message": message });
        let sid = session_id.unwrap_or("");
        event_with_payload("error", sid, payload)
    }

    /// `message.end` event.
    pub fn message_end_event(session_id: &str) -> Value {
        event_with_payload("message.end", session_id, json!({}))
    }

    /// `message.start` event — Hermes emits with NO payload.
    pub fn message_start_event(session_id: &str) -> Value {
        json!({
            "jsonrpc": "2.0",
            "method": "event",
            "params": {
                "type": "message.start",
                "session_id": session_id
            }
        })
    }

    /// `tool.generating` event — only name required, tool_id optional.
    pub fn tool_generating_event(session_id: &str, name: &str, tool_id: Option<&str>) -> Value {
        let mut payload = json!({ "name": name });
        if let Some(tid) = tool_id {
            payload["tool_id"] = json!(tid);
        }
        event_with_payload("tool.generating", session_id, payload)
    }

    /// `clarify.request` event — uses choices (not options).
    pub fn clarify_request_event(
        session_id: &str,
        request_id: &str,
        question: &str,
        choices: Vec<&str>,
    ) -> Value {
        event_with_payload(
            "clarify.request",
            session_id,
            json!({
                "request_id": request_id,
                "question": question,
                "choices": choices
            }),
        )
    }

    /// `sudo.request` event — reason optional.
    pub fn sudo_request_event(session_id: &str, request_id: &str, reason: Option<&str>) -> Value {
        let mut payload = json!({ "request_id": request_id });
        if let Some(r) = reason {
            payload["reason"] = json!(r);
        }
        event_with_payload("sudo.request", session_id, payload)
    }

    /// `secret.request` event — Hermes format with env_var/prompt/metadata.
    pub fn secret_request_event(
        session_id: &str,
        request_id: &str,
        prompt: &str,
        env_var: &str,
    ) -> Value {
        event_with_payload(
            "secret.request",
            session_id,
            json!({
                "request_id": request_id,
                "prompt": prompt,
                "env_var": env_var
            }),
        )
    }

    /// `session.info` event — Hermes format with running bool and tools/skills as objects.
    pub fn session_info_event(session_id: &str, running: bool, model: Option<&str>) -> Value {
        event_with_payload(
            "session.info",
            session_id,
            json!({
                "running": running,
                "model": model,
                "tools": {},
                "skills": {}
            }),
        )
    }

    /// `notification.show` event — Hermes format with id/key/text/level/kind/ttl_ms.
    pub fn notification_show_event(session_id: &str, id: &str, key: &str, text: &str) -> Value {
        event_with_payload(
            "notification.show",
            session_id,
            json!({
                "id": id,
                "key": key,
                "text": text
            }),
        )
    }

    /// `notification.clear` event — Hermes format with key only.
    pub fn notification_clear_event(session_id: &str, key: &str) -> Value {
        event_with_payload("notification.clear", session_id, json!({ "key": key }))
    }

    /// Unknown event type for forward-compatibility testing.
    pub fn unknown_event(event_type: &str, session_id: &str) -> Value {
        event_with_payload(event_type, session_id, json!({ "custom": "data" }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn session_create_request_serializes_correctly() {
        let req = build_session_create_request(1, "desktop", 96, None);
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["jsonrpc"], "2.0");
        assert_eq!(json["id"], 1);
        assert_eq!(json["method"], "session.create");
        assert_eq!(json["params"]["source"], "desktop");
        assert_eq!(json["params"]["cols"], 96);
    }

    #[test]
    fn session_create_result_deserializes_real_wire_format() {
        // Real Hermes wire format has snake_case fields
        let json = r#"{
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "session_id": "abc-123",
                "stored_session_id": "stored-456",
                "message_count": 0,
                "messages": [],
                "info": {
                    "desktop_contract": 4
                }
            }
        }"#;

        let envelope: JsonRpcResponse<serde_json::Value> = serde_json::from_str(json).unwrap();
        let result: SessionCreateResult = serde_json::from_value(envelope.result).unwrap();

        assert_eq!(result.session_id, "abc-123");
        assert_eq!(result.stored_session_id, "stored-456");
        assert_eq!(result.message_count, 0);
        assert_eq!(result.messages.len(), 0);
        assert_eq!(result.info.desktop_contract, 4);
    }

    #[test]
    fn prompt_submit_result_deserializes_real_wire_format() {
        let json = r#"{
            "jsonrpc": "2.0",
            "id": 2,
            "result": {
                "status": "streaming",
                "turn_isolation": true
            }
        }"#;

        let envelope: JsonRpcResponse<serde_json::Value> = serde_json::from_str(json).unwrap();
        let result: PromptSubmitResult = serde_json::from_value(envelope.result).unwrap();

        assert_eq!(result.status, PromptSubmitStatus::Streaming);
        assert_eq!(result.turn_isolation, true);
    }

    #[test]
    fn prompt_submit_request_serializes_correctly() {
        let req = build_prompt_submit_request(2, "sess-123", "Hello world");
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["method"], "prompt.submit");
        assert_eq!(json["params"]["session_id"], "sess-123");
        assert_eq!(json["params"]["text"], "Hello world");
    }

    #[test]
    fn parse_gateway_ready_event() {
        let raw = fixtures::gateway_ready_event();
        let result = parse_gateway_event(&raw).unwrap();
        let routed = result.unwrap();
        match routed.event {
            ParsedGatewayEvent::Known(GatewayEvent::GatewayReady(payload)) => {
                assert_eq!(payload.skin, Some("default".to_string()));
            }
            _ => panic!("expected GatewayReady"),
        }
        assert_eq!(routed.session_id, None); // gateway.ready has no session_id
    }

    #[test]
    fn gateway_event_deserializes_snake_case_payload() {
        // Real Hermes wire format has snake_case payload fields
        let json = r#"{
            "jsonrpc": "2.0",
            "method": "event",
            "params": {
                "type": "tool.start",
                "session_id": "sess-123",
                "payload": {
                    "tool_id": "t1",
                    "name": "read_file"
                }
            }
        }"#;

        let result =
            parse_gateway_event(&serde_json::from_str::<serde_json::Value>(json).unwrap()).unwrap();
        let routed = result.unwrap();
        match routed.event {
            ParsedGatewayEvent::Known(GatewayEvent::ToolStart(payload)) => {
                assert_eq!(payload.tool_id, "t1");
                assert_eq!(payload.name, "read_file");
            }
            _ => panic!("expected ToolStart"),
        }
        assert_eq!(routed.session_id, Some("sess-123".to_string()));
    }

    #[test]
    fn parse_message_delta_event_real_format() {
        let raw = fixtures::message_delta_event("sess-123", "Hello");
        let result = parse_gateway_event(&raw).unwrap();
        let routed = result.unwrap();
        match routed.event {
            ParsedGatewayEvent::Known(GatewayEvent::MessageDelta(payload)) => {
                assert_eq!(payload.text, "Hello");
            }
            _ => panic!("expected MessageDelta"),
        }
        assert_eq!(routed.session_id, Some("sess-123".to_string()));
    }

    #[test]
    fn parse_message_delta_event_legacy_format() {
        let raw = fixtures::message_delta_event_legacy("sess-123", "Hello");
        let result = parse_gateway_event(&raw).unwrap();
        let routed = result.unwrap();
        match routed.event {
            // Legacy format has "text" at params level (no payload wrapper),
            // so params.payload is null/missing, fails to deserialize MessageDeltaPayload,
            // and falls through to MalformedKnown
            ParsedGatewayEvent::MalformedKnown {
                event_type,
                session_id,
                payload,
                error: _,
            } => {
                assert_eq!(event_type, "message.delta");
                assert_eq!(session_id, Some("sess-123".to_string()));
                // In legacy format, payload is null (missing), text is at params level
                assert!(payload.is_null(), "payload should be null in legacy format");
            }
            _ => panic!("expected MalformedKnown for legacy format"),
        }
    }

    #[test]
    fn parse_reasoning_delta_event() {
        let raw = fixtures::reasoning_delta_event("sess-123", "thinking...");
        let result = parse_gateway_event(&raw).unwrap();
        let routed = result.unwrap();
        match routed.event {
            ParsedGatewayEvent::Known(GatewayEvent::ReasoningDelta(payload)) => {
                assert_eq!(payload.text, "thinking...");
            }
            _ => panic!("expected ReasoningDelta"),
        }
        assert_eq!(routed.session_id, Some("sess-123".to_string()));
    }

    #[test]
    fn parse_tool_start_then_complete() {
        let start = fixtures::tool_start_event("sess-123", "t1", "read_file");
        let result = parse_gateway_event(&start).unwrap();
        let routed = result.unwrap();
        match routed.event {
            ParsedGatewayEvent::Known(GatewayEvent::ToolStart(p)) => {
                assert_eq!(p.name, "read_file");
                assert_eq!(p.tool_id, "t1");
            }
            _ => panic!("expected ToolStart"),
        }
        assert_eq!(routed.session_id, Some("sess-123".to_string()));

        let complete = fixtures::tool_complete_event("sess-123", "t1", "read_file", "file content");
        let result = parse_gateway_event(&complete).unwrap();
        let routed = result.unwrap();
        match routed.event {
            ParsedGatewayEvent::Known(GatewayEvent::ToolComplete(p)) => {
                assert_eq!(p.name, "read_file");
                assert_eq!(p.result_text, Some("file content".to_string()));
            }
            _ => panic!("expected ToolComplete"),
        }
        assert_eq!(routed.session_id, Some("sess-123".to_string()));
    }

    #[test]
    fn parse_approval_request() {
        let raw =
            fixtures::approval_request_event("sess-123", "req-1", "t1", "write_file", "echo hi");
        let result = parse_gateway_event(&raw).unwrap();
        let routed = result.unwrap();
        match routed.event {
            ParsedGatewayEvent::Known(GatewayEvent::ApprovalRequest(p)) => {
                assert_eq!(p.request_id, "req-1");
                assert_eq!(p.name, "write_file");
                assert_eq!(p.command, Some("echo hi".to_string()));
                // choices should be empty by default in fixture
                assert!(p.choices.is_empty());
            }
            _ => panic!("expected ApprovalRequest"),
        }
        assert_eq!(routed.session_id, Some("sess-123".to_string()));
    }

    #[test]
    fn parse_message_end_carries_session_id() {
        let raw = fixtures::message_end_event("desk-123-abc");
        let result = parse_gateway_event(&raw).unwrap();
        let routed = result.unwrap();
        match routed.event {
            ParsedGatewayEvent::Known(GatewayEvent::MessageEnd(_p)) => {
                // session_id is in GatewayEventParams, not in payload
                // This test just verifies the event parses correctly
                assert!(true);
            }
            _ => panic!("expected MessageEnd"),
        }
        assert_eq!(routed.session_id, Some("desk-123-abc".to_string()));
    }

    #[test]
    fn parse_unknown_event_preserved() {
        let raw = fixtures::unknown_event("custom.event", "sess-123");
        let result = parse_gateway_event(&raw).unwrap();
        let routed = result.unwrap();
        match routed.event {
            ParsedGatewayEvent::UnknownType {
                event_type,
                session_id,
                payload: _,
            } => {
                assert_eq!(event_type, "custom.event");
                assert_eq!(session_id, Some("sess-123".to_string()));
            }
            _ => panic!("expected Unknown event"),
        }
        assert_eq!(routed.session_id, Some("sess-123".to_string()));
    }

    #[test]
    fn parse_malformed_known_event_falls_to_malformed() {
        // tool.start with missing required field "name"
        let json = r#"{
            "jsonrpc": "2.0",
            "method": "event",
            "params": {
                "type": "tool.start",
                "session_id": "sess-123",
                "payload": { "tool_id": "t1" }
            }
        }"#;

        let result =
            parse_gateway_event(&serde_json::from_str::<serde_json::Value>(json).unwrap()).unwrap();
        let routed = result.unwrap();
        match routed.event {
            ParsedGatewayEvent::MalformedKnown {
                event_type, error, ..
            } => {
                assert_eq!(event_type, "tool.start");
                assert!(error.contains("missing field") || error.contains("name"));
            }
            _ => panic!("expected MalformedKnown, got {:?}", routed.event),
        }
        assert_eq!(routed.session_id, Some("sess-123".to_string()));
    }

    #[test]
    fn json_rpc_response_roundtrip() {
        let result = SessionCreateResult {
            session_id: "test-123".to_string(),
            stored_session_id: "stored-test-123".to_string(),
            message_count: 0,
            messages: vec![],
            info: SessionCreateInfo {
                desktop_contract: 4,
                profile_name: None,
            },
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
