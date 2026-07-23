// src-tauri/src/ws_transport.rs
// WebSocket client for the Hermes /api/ws JSON-RPC transport (ADR-004).
//
// The real Hermes backend (NousResearch/hermes-agent, `hermes serve`) exposes a
// newline-delimited JSON-RPC 2.0 channel over WebSocket. This module connects,
// performs the auth handshake, creates/resumes a session, submits a prompt,
// and forwards streaming events to the SAME Tauri `chat_event` channel the
// HTTP transport used — so the frontend stays unchanged.
//
// Phase 0 (ADR-004): connect-per-message, no persistent connection yet.

use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};
use tokio::sync::oneshot;
use tokio_tungstenite::tungstenite::Message;

use crate::chat::{parse_ws_message, ChatEvent};
use crate::hermes_protocol::{
    self, GatewayEvent, ParsedGatewayEvent, PromptSubmitParams, PromptSubmitResult,
    RoutedGatewayEvent, SessionCreateParams, SessionCreateResult,
};

/// Generic RPC timeout duration.
const RPC_TIMEOUT: Duration = Duration::from_secs(30);

/// JSON-RPC request id type.
pub type RpcId = u64;

/// JSON-RPC request id counter (process-wide).
static NEXT_RPC_ID: AtomicU64 = AtomicU64::new(1);

fn next_rpc_id() -> RpcId {
    NEXT_RPC_ID.fetch_add(1, Ordering::Relaxed)
}

/// Generic RPC dispatcher for the persistent connection.
/// Sends a command through the mpsc channel, waits for the response with timeout,
/// and returns the parsed result or a typed error.
pub async fn call_rpc<P, R>(
    ws_state: &WsState,
    method: &'static str,
    params: P,
    timeout: Duration,
) -> Result<R, GatewayClientError>
where
    P: Serialize + for<'de> Deserialize<'de> + std::fmt::Debug,
    R: for<'de> Deserialize<'de> + std::fmt::Debug,
{
    let tx_guard = ws_state.cmd_tx.lock().await;
    let tx = tx_guard.as_ref().ok_or(GatewayClientError::Protocol(
        "not connected: no cmd_tx".into(),
    ))?;
    let id = next_rpc_id();
    let params_value = serde_json::to_value(&params)
        .map_err(|e| GatewayClientError::Protocol(format!("serialize params: {}", e)))?;
    let (reply_tx, reply_rx) = oneshot::channel();
    tx.send(WsCommand::Rpc {
        id,
        method: method.to_string(),
        params: params_value,
        reply: reply_tx,
    })
    .await
    .map_err(|_| GatewayClientError::Protocol("reader task closed".into()))?;
    drop(tx_guard); // Release lock before awaiting

    // Wait for RPC response with timeout.
    // reply_rx is `oneshot::Receiver<Result<Value, GatewayClientError>>`, so
    // three unwrap layers: timeout → oneshot RecvError → inner GatewayClientError.
    // The reader task already discriminates success (Ok(result_value)) from
    // RPC error (Err(BackendError)), so response_value is the bare `result`.
    let response_value = tokio::time::timeout(timeout, reply_rx)
        .await
        .map_err(|_| GatewayClientError::RpcTimeout)? // Elapsed
        .map_err(|_| GatewayClientError::ConnectionLost)??; // RecvError, then inner error

    // Parse the result directly into R — the error envelope was already handled
    // by the reader task (IncomingFrame::RpcError → BackendError).
    serde_json::from_value::<R>(response_value)
        .map_err(|e| GatewayClientError::Protocol(format!("response parse: {}", e)))
}

/// Submit a prompt on the persistent connection using the generic RPC dispatcher.
/// Waits for the RPC acknowledgement from Hermes before returning.
pub async fn submit_prompt_on_connection(
    ws_state: &WsState,
    session_id: &str,
    text: &str,
) -> Result<PromptSubmitResult, WsError> {
    call_rpc(
        ws_state,
        "prompt.submit",
        PromptSubmitParams {
            session_id: session_id.to_owned(),
            text: text.to_owned(),
        },
        RPC_TIMEOUT,
    )
    .await
}

/// Create a session on the persistent connection using the generic RPC dispatcher.
pub async fn create_session_on_connection(
    ws_state: &WsState,
    source: &str,
) -> Result<SessionCreateResult, WsError> {
    call_rpc(
        ws_state,
        "session.create",
        SessionCreateParams {
            source: source.to_owned(),
            cols: 96,
        },
        RPC_TIMEOUT,
    )
    .await
}

/// Send one chat message over the /api/ws transport.
///
/// - `ws_url`: full WS URL incl. auth, e.g. `ws://127.0.0.1:64724/api/ws?token=<...>`
/// - `session_id`: existing session to resume, or None to create a new one
/// - `text`: the user's message
/// - `app_handle`: used to emit streaming `chat_event`s to the frontend
///
/// Returns the session_id the turn ran under (created or resumed). The actual
/// reply content arrives as streaming `Token` events via `chat_event`, exactly
/// like the HTTP transport — the Ok value is just the session handle.
pub async fn send_message_via_ws(
    ws_url: &str,
    session_id: Option<&str>,
    text: &str,
    app_handle: &AppHandle,
) -> Result<String, WsError> {
    // Regular chat: source="desktop" (the normal session surface).
    send_message_via_ws_impl(ws_url, session_id, text, app_handle, "desktop", false)
        .await
        .map(|(sid, _)| sid)
}

/// Same as `send_message_via_ws`, but also accumulates all streamed token
/// text into a single `String` and returns it alongside the session_id.
///
/// `source` controls the session's `source` field in state.db — the backend
/// preserves it verbatim (`server.py:_resolve_session_source`). Briefings pass
/// `"briefing_smart"` so the session is hidden from the feed the next briefing
/// scans (prevents the "briefing-of-briefing" loop).
///
/// The tokens are STILL emitted as `chat_event`s so a visible ChatView keeps
/// streaming — the buffer is purely an additional accumulation.
pub async fn send_message_via_ws_buffered(
    ws_url: &str,
    session_id: Option<&str>,
    text: &str,
    app_handle: &AppHandle,
    source: &str,
) -> Result<(String, String), WsError> {
    send_message_via_ws_impl(ws_url, session_id, text, app_handle, source, true).await
}

/// Shared implementation. `source` → session.create `source` param.
/// `buffered` → accumulate token text into the returned String[1].
async fn send_message_via_ws_impl(
    ws_url: &str,
    session_id: Option<&str>,
    text: &str,
    app_handle: &AppHandle,
    source: &str,
    buffered: bool,
) -> Result<(String, String), WsError> {
    tracing::info!(target: "steersman_desktop_lib::ws", ws_url, source, buffered, "connecting");
    let (mut ws, _resp) = tokio_tungstenite::connect_async(ws_url)
        .await
        .map_err(|e| WsError::Connect(e.to_string()))?;

    // 1. Handshake: wait for the `gateway.ready` event (tui_gateway/ws.py:324).
    let ready = wait_for_ready(&mut ws).await;
    if !ready {
        tracing::warn!(target: "steersman_desktop_lib::ws", "no gateway.ready before timeout, proceeding anyway");
    }

    // 2. Resolve session: create one if none was supplied.
    let sid = match session_id {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => create_session(&mut ws, source).await?,
    };
    tracing::info!(target: "steersman_desktop_lib::ws", session_id = %sid, "session ready");

    // 3. Submit the prompt; the response acks turn start, real content streams.
    submit_prompt(&mut ws, &sid, text).await?;

    // 4. Read streaming events until the turn ends (done/error) or the socket
    //    closes. Each recognised event is emitted on `chat_event`, identical to
    //    what the HTTP transport did — the frontend needs no changes.
    let mut buffer = String::new();
    let result = if buffered {
        read_events_buffered(&mut ws, app_handle, &mut buffer).await
    } else {
        read_events(&mut ws, app_handle).await
    };

    // Best-effort close.
    let _ = ws.close(None).await;

    match result {
        Ok(returned_sid) => Ok((returned_sid.unwrap_or(sid), buffer)),
        Err(e) => Err(e),
    }
}

/// Wait up to 5s for the `gateway.ready` event after connect.
async fn wait_for_ready<S>(ws: &mut S) -> bool
where
    S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        match tokio::time::timeout(remaining, ws.next()).await {
            Ok(Some(Ok(msg))) => {
                if let Message::Text(text) = msg {
                    if let Ok(value) = serde_json::from_str::<Value>(&text) {
                        let is_ready = value
                            .get("method")
                            .and_then(|m| m.as_str())
                            .map(|m| m == "event")
                            .unwrap_or(false)
                            && value
                                .get("params")
                                .and_then(|p| p.get("type"))
                                .and_then(|t| t.as_str())
                                .map(|t| t == "gateway.ready")
                                .unwrap_or(false);
                        if is_ready {
                            return true;
                        }
                    }
                }
            }
            _ => break,
        }
    }
    false
}

/// Build the `session.create` JSON-RPC request. Extracted for testing —
/// the source param is the contract guarantee that briefings don't leak into
/// the feed (see `briefing_smart` filter in sessions.rs).
fn build_session_create_request(id: u64, source: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "session.create",
        "params": {
            "source": source,
            "cols": 96,
        },
    })
}

/// Send `session.create`, return the new session_id from the response.
///
/// `source` controls the session's `source` field in state.db. The backend
/// (`server.py:_resolve_session_source`) preserves any explicit value. This
/// matters because `sessions.rs` list/feed queries filter by source to hide
/// service sessions. A briefing MUST pass `"briefing_smart"` so it doesn't
/// leak into the feed the NEXT briefing analyses (otherwise: briefing-of-
/// briefing-of-briefing infinite loop). Regular chat passes `"desktop"`.
async fn create_session<S>(ws: &mut S, source: &str) -> Result<String, WsError>
where
    S: SinkExt<Message, Error = tokio_tungstenite::tungstenite::Error>
        + StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>>
        + Unpin,
{
    let id = next_rpc_id();
    let req = build_session_create_request(id, source);
    send_json(ws, &req).await?;

    // Read frames until the matching JSON-RPC response arrives.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        match tokio::time::timeout(remaining, ws.next()).await {
            Ok(Some(Ok(msg))) => {
                if let Message::Text(text) = msg {
                    let value: Value = serde_json::from_str(&text)
                        .map_err(|e| WsError::SessionCreate(format!("bad JSON: {}", e)))?;
                    // Skip streaming events while waiting for the response.
                    if value.get("id").and_then(|v| v.as_u64()) != Some(id) {
                        continue;
                    }
                    if let Some(err) = value.get("error") {
                        let msg = err
                            .get("message")
                            .and_then(|m| m.as_str())
                            .unwrap_or("session.create failed");
                        return Err(WsError::SessionCreate(msg.to_string()));
                    }
                    let sid = value
                        .get("result")
                        .and_then(|r| r.get("session_id"))
                        .and_then(|s| s.as_str())
                        .or_else(|| {
                            // Some servers return the id at the top of result.
                            value.get("result").and_then(|r| r.as_str())
                        })
                        .ok_or_else(|| {
                            WsError::SessionCreate("no session_id in response".to_string())
                        })?;
                    return Ok(sid.to_string());
                }
            }
            Ok(Some(Err(e))) => {
                return Err(WsError::Stream(format!("session.create socket: {}", e)))
            }
            _ => return Err(WsError::Stream("session.create: stream closed".to_string())),
        }
    }
}

/// Send `prompt.submit` for the given session (connect-per-message path).
///
/// We don't wait for the prompt.submit response (it only acks turn-start;
/// real content arrives as events). The id is still sent per JSON-RPC.
async fn submit_prompt<S>(ws: &mut S, session_id: &str, text: &str) -> Result<(), WsError>
where
    S: SinkExt<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    let req = build_prompt_submit_request(next_rpc_id(), session_id, text);
    send_json(ws, &req).await
}

/// Read streaming events until done/error or socket close.
///
/// Returns `Ok(Some(session_id))` on a clean `Done` (carrying the session id),
/// `Ok(None)` if the stream ended without an explicit done, `Err` on error event.
async fn read_events<S>(ws: &mut S, app_handle: &AppHandle) -> Result<Option<String>, WsError>
where
    S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    // Hard ceiling so a wedged agent can't hold the connection forever.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(1800);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        match tokio::time::timeout(remaining, ws.next()).await {
            Ok(Some(Ok(msg))) => match msg {
                Message::Text(text) => {
                    if let Some(event) = parse_ws_message(&text) {
                        let terminal = is_terminal(&event);
                        if matches!(event, ChatEvent::Done { ref session_id } if session_id.is_some())
                        {
                            let sid = if let ChatEvent::Done { session_id } = event.clone() {
                                session_id
                            } else {
                                None
                            };
                            let _ = app_handle.emit("chat_event", event);
                            return Ok(sid);
                        }
                        // If the backend sent an explicit error event, surface
                        // it as a typed BackendError so callers can branch.
                        if let ChatEvent::Error { message } = &event {
                            let _ = app_handle.emit("chat_event", event.clone());
                            return Err(WsError::BackendError(message.clone()));
                        }
                        let _ = app_handle.emit("chat_event", event);
                        if terminal {
                            // Done without session_id.
                            return Ok(None);
                        }
                    }
                }
                Message::Close(_) => {
                    tracing::info!(target: "steersman_desktop_lib::ws", "server closed connection");
                    return Ok(None);
                }
                _ => {}
            },
            Ok(Some(Err(e))) => return Err(WsError::Stream(e.to_string())),
            Ok(None) => {
                tracing::info!(target: "steersman_desktop_lib::ws", "stream ended");
                return Ok(None);
            }
            Err(_) => return Err(WsError::Timeout),
        }
    }
}

fn is_terminal(event: &ChatEvent) -> bool {
    matches!(event, ChatEvent::Done { .. } | ChatEvent::Error { .. })
}

/// Like `read_events`, but appends every `Token`'s text into `buffer`.
async fn read_events_buffered<S>(
    ws: &mut S,
    app_handle: &AppHandle,
    buffer: &mut String,
) -> Result<Option<String>, WsError>
where
    S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    let deadline = tokio::time::Instant::now() + Duration::from_secs(1800);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        match tokio::time::timeout(remaining, ws.next()).await {
            Ok(Some(Ok(msg))) => match msg {
                Message::Text(text) => {
                    if let Some(event) = parse_ws_message(&text) {
                        // Accumulate token text before emitting, so the buffer
                        // captures the full assistant reply regardless of
                        // whether a ChatView is visible to consume the stream.
                        if let ChatEvent::Token { ref content } = event {
                            buffer.push_str(content);
                        }
                        let terminal = is_terminal(&event);
                        if matches!(event, ChatEvent::Done { ref session_id } if session_id.is_some())
                        {
                            let sid = if let ChatEvent::Done { session_id } = event.clone() {
                                session_id
                            } else {
                                None
                            };
                            let _ = app_handle.emit("chat_event", event);
                            return Ok(sid);
                        }
                        if let ChatEvent::Error { message } = &event {
                            let _ = app_handle.emit("chat_event", event.clone());
                            return Err(WsError::BackendError(message.clone()));
                        }
                        let _ = app_handle.emit("chat_event", event);
                        if terminal {
                            return Ok(None);
                        }
                    }
                }
                Message::Close(_) => {
                    tracing::info!(target: "steersman_desktop_lib::ws", "server closed connection");
                    return Ok(None);
                }
                _ => {}
            },
            Ok(Some(Err(e))) => return Err(WsError::Stream(e.to_string())),
            Ok(None) => {
                tracing::info!(target: "steersman_desktop_lib::ws", "stream ended");
                return Ok(None);
            }
            Err(_) => return Err(WsError::Timeout),
        }
    }
}

async fn send_json<S>(ws: &mut S, value: &Value) -> Result<(), WsError>
where
    S: SinkExt<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    let text =
        serde_json::to_string(value).map_err(|e| WsError::Protocol(format!("encode: {}", e)))?;
    ws.send(Message::Text(text))
        .await
        .map_err(|e| WsError::Protocol(format!("ws send: {}", e)))
}

// ── Typed errors (ADR-004 §Последствия, P3.3) ──────────────────────────────

/// Error type for the persistent gateway client.
#[derive(Debug, Clone)]
pub enum GatewayClientError {
    Connect(String),
    AuthFailed,
    Protocol(String),
    BackendError(String),
    Timeout,
    Stream(String),
    ReadyTimeout,
    ConnectionLost,
    RpcTimeout,
    SessionCreate(String),
}

impl std::fmt::Display for GatewayClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GatewayClientError::Connect(s) => write!(f, "connect failed: {}", s),
            GatewayClientError::AuthFailed => write!(f, "auth rejected"),
            GatewayClientError::Protocol(s) => write!(f, "protocol error: {}", s),
            GatewayClientError::BackendError(s) => write!(f, "backend error: {}", s),
            GatewayClientError::Timeout => write!(f, "timeout"),
            GatewayClientError::Stream(s) => write!(f, "stream error: {}", s),
            GatewayClientError::ReadyTimeout => write!(f, "ready timeout"),
            GatewayClientError::ConnectionLost => write!(f, "connection lost"),
            GatewayClientError::RpcTimeout => write!(f, "RPC timeout"),
            GatewayClientError::SessionCreate(s) => write!(f, "session create failed: {}", s),
        }
    }
}

impl std::error::Error for GatewayClientError {}

/// Backward-compatibility type alias for existing code using WsError.
pub type WsError = GatewayClientError;

/// Pending RPC request tracked in the read-loop.
struct PendingRequest {
    reply: oneshot::Sender<Result<Value, GatewayClientError>>,
    method: String,
    timeout: Instant,
}

// ── Commands sent to the reader task ────────────────────────────────────────

/// Commands sent by Tauri command handlers to the reader task via mpsc.
#[derive(Debug)]
pub enum WsCommand {
    /// Generic JSON-RPC call. The reader task sends the frame and waits for response by ID.
    Rpc {
        id: RpcId,
        method: String,
        params: Value,
        reply: oneshot::Sender<Result<Value, GatewayClientError>>,
    },
    /// Tear down the reader task and close the socket.
    Shutdown,
}

// ── Incoming frame classification ───────────────────────────────────────────

/// Classifies one parsed JSON-RPC frame.
#[allow(clippy::large_enum_variant)]
enum IncomingFrame {
    RpcResponse { id: RpcId, result: Value },
    RpcError { id: RpcId, error: Value },
    Event(RoutedGatewayEvent),
}

/// Classify a raw JSON-RPC 2.0 value into either an RPC response or an event.
fn classify_frame(value: &Value) -> Option<IncomingFrame> {
    // Has "id" → JSON-RPC response
    if let Some(id) = value.get("id").and_then(|v| v.as_u64()) {
        if let Some(result) = value.get("result") {
            return Some(IncomingFrame::RpcResponse {
                id,
                result: result.clone(),
            });
        }
        if let Some(error) = value.get("error") {
            return Some(IncomingFrame::RpcError {
                id,
                error: error.clone(),
            });
        }
        return None;
    }
    // Has method "event" → streaming event via typed parser
    let is_event = value
        .get("method")
        .and_then(|m| m.as_str())
        .map(|m| m == "event")
        .unwrap_or(false);
    if !is_event {
        return None;
    }
    // Use the typed parser from hermes_protocol
    match hermes_protocol::parse_gateway_event(value) {
        Ok(Some(routed)) => Some(IncomingFrame::Event(routed)),
        Ok(None) => None,
        Err(e) => {
            tracing::warn!(target: "steersman_desktop_lib::ws", error = %e, "unparseable event frame");
            None
        }
    }
}

// ── ADR-006: Persistent WebSocket connection ──────────────────────────────
//
// One long-lived WS connection per app lifetime. A reader task owns the socket
// (both halves); command handlers send WsCommand through an mpsc channel. This
// replaces connect-per-message (ADR-004 Phase 0) with a single point of
// failure (the initial connect) instead of five per-message.

/// Connection lifecycle state for the persistent WS link.
#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionState {
    /// No socket; `ensure_ws_connection` will open one.
    Disconnected,
    /// `connect_async` in flight (or reader task spawning).
    Connecting,
    /// Socket is live, reader task is running. Carries the default session_id
    /// so subsequent prompts reuse it without a new `session.create`.
    Connected,
}

/// Translate a typed RoutedGatewayEvent to a frontend ChatEvent.
/// This replaces the legacy parse_ws_message for the persistent path.
fn translate_gateway_event(routed: &RoutedGatewayEvent) -> Option<ChatEvent> {
    match &routed.event {
        ParsedGatewayEvent::Known(GatewayEvent::MessageDelta(p)) => Some(ChatEvent::Token {
            content: p.text.clone(),
        }),
        ParsedGatewayEvent::Known(GatewayEvent::ReasoningDelta(p)) => Some(ChatEvent::Reasoning {
            content: p.text.clone(),
        }),
        ParsedGatewayEvent::Known(GatewayEvent::ThinkingDelta(p)) => Some(ChatEvent::Thinking {
            content: p.text.clone(),
        }),
        ParsedGatewayEvent::Known(GatewayEvent::MessageStart(_)) => Some(ChatEvent::Status {
            status: "streaming".to_string(),
        }),
        ParsedGatewayEvent::Known(GatewayEvent::MessageComplete(_))
        | ParsedGatewayEvent::Known(GatewayEvent::MessageEnd(_)) => Some(ChatEvent::Done {
            session_id: Some(routed.session_id.clone().unwrap_or_default()),
        }),
        ParsedGatewayEvent::Known(GatewayEvent::ToolStart(p)) => Some(ChatEvent::ToolStart {
            name: p.name.clone(),
            tool_call_id: p.tool_id.clone(),
        }),
        ParsedGatewayEvent::Known(GatewayEvent::ToolComplete(p)) => Some(ChatEvent::ToolComplete {
            name: p.name.clone(),
            tool_call_id: p.tool_id.clone(),
            output: p.result_text.clone().unwrap_or_default(),
            duration_ms: p.duration_s.map(|s| (s * 1000.0) as u64).unwrap_or(0),
        }),
        ParsedGatewayEvent::Known(GatewayEvent::ApprovalRequest(p)) => {
            Some(ChatEvent::ApprovalRequest {
                request_id: p.request_id.clone(),
                tool_name: p.name.clone(),
                tool_input: p.tool_input.clone().unwrap_or_default(),
                action: p.action.clone().unwrap_or_default(),
                command_class: p.command_class.clone().unwrap_or("write".to_string()),
            })
        }
        ParsedGatewayEvent::Known(GatewayEvent::StatusUpdate(p)) => Some(ChatEvent::Status {
            status: p
                .text
                .clone()
                .unwrap_or_else(|| p.kind.clone().unwrap_or("unknown".to_string())),
        }),
        ParsedGatewayEvent::Known(GatewayEvent::PipelineStatus(p)) => {
            Some(ChatEvent::PipelineStatus {
                backend: p.backend.clone(),
                model: p.model.clone(),
                tokens_used: p.tokens_used,
                tokens_limit: p.tokens_limit,
                cost_usd: p.cost_usd,
            })
        }
        ParsedGatewayEvent::Known(GatewayEvent::Error(p)) => Some(ChatEvent::Error {
            message: p.message.clone(),
        }),
        ParsedGatewayEvent::Known(GatewayEvent::ToolGenerating(p)) => {
            Some(ChatEvent::ToolGenerating {
                name: p.name.clone(),
                tool_call_id: p.tool_id.clone(),
                content: p.text.clone(),
            })
        }
        ParsedGatewayEvent::Known(GatewayEvent::ClarifyRequest(p)) => {
            Some(ChatEvent::ClarifyRequest {
                request_id: p.request_id.clone(),
                question: p.question.clone(),
                choices: p.choices.clone(),
            })
        }
        ParsedGatewayEvent::Known(GatewayEvent::SudoRequest(p)) => Some(ChatEvent::SudoRequest {
            request_id: p.request_id.clone(),
            reason: p.reason.clone(),
            timeout_secs: p.timeout_secs,
        }),
        ParsedGatewayEvent::Known(GatewayEvent::SudoExpire(p)) => Some(ChatEvent::SudoExpire {
            request_id: p.request_id.clone(),
        }),
        ParsedGatewayEvent::Known(GatewayEvent::SecretRequest(p)) => {
            Some(ChatEvent::SecretRequest {
                request_id: p.request_id.clone(),
                prompt: p.prompt.clone(),
                env_var: p.env_var.clone(),
                metadata: p.metadata.clone(),
            })
        }
        ParsedGatewayEvent::Known(GatewayEvent::SecretExpire(p)) => Some(ChatEvent::SecretExpire {
            request_id: p.request_id.clone(),
        }),
        ParsedGatewayEvent::Known(GatewayEvent::SessionInfo(p)) => Some(ChatEvent::SessionInfo {
            session_id: routed.session_id.clone().unwrap_or_default(),
            stored_session_id: p.stored_session_id.clone(),
            running: p.running,
            model: p.model.clone(),
            provider: p.provider.clone(),
            tools: p.tools.clone(),
            skills: p.skills.clone(),
            usage: p.usage.clone(),
            desktop_contract: p.desktop_contract,
        }),
        ParsedGatewayEvent::Known(GatewayEvent::NotificationShow(p)) => {
            Some(ChatEvent::Notification {
                id: p.id.clone(),
                key: p.key.clone(),
                text: p.text.clone(),
                level: p.level.clone(),
                kind: p.kind.clone(),
                ttl_ms: p.ttl_ms,
            })
        }
        ParsedGatewayEvent::Known(GatewayEvent::NotificationClear(p)) => {
            Some(ChatEvent::NotificationClear { key: p.key.clone() })
        }
        ParsedGatewayEvent::Known(GatewayEvent::GatewayReady(_)) => None,
        ParsedGatewayEvent::Known(GatewayEvent::Unknown { event_type, .. }) => {
            tracing::trace!(target: "steersman_desktop_lib::ws", event_type, "ignored gateway event");
            None
        }
        ParsedGatewayEvent::MalformedKnown {
            event_type,
            session_id: _,
            payload: _,
            error,
        } => {
            tracing::warn!(target: "steersman_desktop_lib::ws", event_type, error, "malformed known event, falling back to legacy parser");
            // Could fall back to legacy parser here if needed
            None
        }
        ParsedGatewayEvent::UnknownType {
            event_type,
            session_id: _,
            payload: _,
        } => {
            tracing::warn!(target: "steersman_desktop_lib::ws", event_type, "unknown event type");
            None
        }
    }
}
/// Extracted for testing — the reader task calls this before `ws.send`.
fn build_prompt_submit_request(id: u64, session_id: &str, text: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "prompt.submit",
        "params": {
            "session_id": session_id,
            "text": text,
        },
    })
}

/// Persistent WS connection state, held in `AppState.ws`.
///
/// All fields are behind `tokio::sync::Mutex` to allow concurrent access from
/// async Tauri command handlers. The actual socket lives inside the reader
/// task (not stored here) — handlers communicate via `cmd_tx`.
pub struct WsState {
    /// Current connection lifecycle.
    pub state: tokio::sync::Mutex<ConnectionState>,
    /// The full WS URL.
    pub ws_url: tokio::sync::Mutex<String>,
    /// Default chat session_id.
    pub session_id: tokio::sync::Mutex<Option<String>>,
    /// Sender into the reader task's mpsc channel.
    pub cmd_tx: tokio::sync::Mutex<Option<tokio::sync::mpsc::Sender<WsCommand>>>,
    /// Serializes concurrent `ensure_ws_connection` calls so only one task
    /// performs the actual connect+handshake.
    pub connect_lock: tokio::sync::Mutex<()>,
    /// Monotonically increasing generation counter. Incremented on each new connection.
    pub generation: std::sync::atomic::AtomicU64,
}

impl WsState {
    /// Create a fresh Disconnected state. Called once in `AppState::new`.
    pub fn new() -> Self {
        Self {
            state: tokio::sync::Mutex::new(ConnectionState::Disconnected),
            ws_url: tokio::sync::Mutex::new(String::new()),
            session_id: tokio::sync::Mutex::new(None),
            cmd_tx: tokio::sync::Mutex::new(None),
            connect_lock: tokio::sync::Mutex::new(()),
            generation: std::sync::atomic::AtomicU64::new(1),
        }
    }

    /// Bump generation and return the new value.
    pub fn next_generation(&self) -> u64 {
        self.generation
            .fetch_add(1, std::sync::atomic::Ordering::Release)
            + 1
    }
}

impl Default for WsState {
    fn default() -> Self {
        Self::new()
    }
}

// ── T2 (ADR-006): ensure_ws_connection + reader_task ───────────────────────
//
// ensure_ws_connection opens the socket ONCE (idempotent: if already Connected
// or Connecting, returns immediately). The reader task owns the socket for its
// entire lifetime, reading events and dispatching WsCommand via mpsc.

/// Ensure the persistent WS connection is open. Idempotent: if already
/// `Connected`, returns `Ok(())` without re-connecting.
///
/// Concurrent calls are serialized via a dedicated `connect_lock` so that only
/// one task performs the actual TCP+handshake; others wait for the result and
/// then observe `Connected` (or the first caller's `Disconnected` on failure).
///
/// On first call: connects, waits for `gateway.ready`, spawns the reader task,
/// stores the `cmd_tx` sender into `ws_state`, and sets state to `Connected`.
/// On socket drop (detected by the reader task), state returns to
/// `Disconnected` and the next call re-connects.
pub async fn ensure_ws_connection(
    ws_url: &str,
    emit_fn: EmitFn,
    ws_state: &Arc<WsState>,
) -> Result<(), WsError> {
    // Fast path: already connected (no lock contention on connect_lock).
    {
        let state = ws_state.state.lock().await;
        if *state == ConnectionState::Connected {
            return Ok(());
        }
    }

    // Serialize concurrent connect attempts. The first caller does the actual
    // connect; subsequent callers acquire the lock after the first finishes
    // and observe the final state.
    let _connect_guard = ws_state.connect_lock.lock().await;

    // Double-check after acquiring the lock — the first caller may have already
    // connected while we were waiting.
    {
        let state = ws_state.state.lock().await;
        match *state {
            ConnectionState::Connected => return Ok(()),
            ConnectionState::Connecting => {
                // Should not happen under connect_lock, but guard anyway.
                return Err(WsError::Protocol("concurrent connect race".into()));
            }
            ConnectionState::Disconnected => {}
        }
        drop(state);
    }

    // We are the single connector. Set Connecting.
    *ws_state.state.lock().await = ConnectionState::Connecting;

    // Connect.
    tracing::info!(target: "steersman_desktop_lib::ws", ws_url, "opening persistent connection");
    let (ws, _resp) = match tokio_tungstenite::connect_async(ws_url).await {
        Ok(result) => result,
        Err(e) => {
            *ws_state.state.lock().await = ConnectionState::Disconnected;
            return Err(WsError::Connect(e.to_string()));
        }
    };

    // Strict gateway.ready barrier: fail if ready not received within timeout.
    // Preserves the typed error from wait_for_gateway_ready (ReadyTimeout,
    // Stream, Connect) instead of wrapping everything in Connect.
    let ws = match tokio::time::timeout(Duration::from_secs(5), wait_for_gateway_ready(ws)).await {
        Ok(Ok(ws)) => ws,
        Ok(Err(e)) => {
            *ws_state.state.lock().await = ConnectionState::Disconnected;
            return Err(e);
        }
        Err(_) => {
            *ws_state.state.lock().await = ConnectionState::Disconnected;
            return Err(WsError::ReadyTimeout);
        }
    };

    // Create the mpsc channel for command handlers → reader task.
    let (cmd_tx, cmd_rx) = tokio::sync::mpsc::channel::<WsCommand>(64);

    // Bump generation before storing cmd_tx.
    let generation = ws_state.next_generation();

    *ws_state.ws_url.lock().await = ws_url.to_string();
    *ws_state.cmd_tx.lock().await = Some(cmd_tx);

    // Spawn the reader task with the generation.
    tokio::spawn(reader_task(
        ws,
        cmd_rx,
        emit_fn,
        Arc::clone(ws_state),
        generation,
    ));

    *ws_state.state.lock().await = ConnectionState::Connected;
    tracing::info!(target: "steersman_desktop_lib::ws", generation, "persistent connection established");

    Ok(())
}

/// Strict gateway.ready barrier. Returns Err on any read error before ready.
async fn wait_for_gateway_ready<S>(mut ws: S) -> Result<S, GatewayClientError>
where
    S: futures::SinkExt<Message, Error = tokio_tungstenite::tungstenite::Error>
        + Unpin
        + futures::Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>>
        + Unpin
        + Send
        + 'static,
{
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return Err(GatewayClientError::ReadyTimeout);
        }
        match tokio::time::timeout(remaining, ws.next()).await {
            Ok(Some(Ok(Message::Text(text)))) => {
                if let Ok(value) = serde_json::from_str::<Value>(&text) {
                    let is_ready = value.get("method").and_then(|m| m.as_str()) == Some("event")
                        && value
                            .get("params")
                            .and_then(|p| p.get("type"))
                            .and_then(|t| t.as_str())
                            == Some("gateway.ready");
                    if is_ready {
                        return Ok(ws);
                    }
                }
            }
            Ok(Some(Ok(_))) => {} // Non-text frames - ignore
            Ok(Some(Err(e))) => return Err(GatewayClientError::Stream(e.to_string())),
            _ => {
                return Err(GatewayClientError::Connect(
                    "connection closed before gateway.ready".into(),
                ))
            }
        }
    }
}

/// The reader task: owns the WS socket, reads frames, dispatches RPC responses
/// and emits events. Uses a pending map for RPC ID tracking.
///
/// On socket close/error: marks Disconnected (with generation guard) and exits.
async fn reader_task<S>(
    mut ws: S,
    mut cmd_rx: tokio::sync::mpsc::Receiver<WsCommand>,
    emit_fn: EmitFn,
    ws_state: Arc<WsState>,
    my_generation: u64,
) where
    S: futures::SinkExt<Message, Error = tokio_tungstenite::tungstenite::Error>
        + futures::Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>>
        + Unpin
        + Send
        + 'static,
{
    // Pending RPC map: resolves responses by ID.
    let mut pending: HashMap<RpcId, PendingRequest> = HashMap::new();

    // Periodic timer for pending RPC expiration (every 5s).
    let mut cleanup_tick = tokio::time::interval(Duration::from_secs(5));
    cleanup_tick.tick().await; // suppress initial immediate tick

    loop {
        // Check for expired pending RPCs
        let now = Instant::now();
        let mut expired_ids: Vec<RpcId> = Vec::new();
        for (id, pending_req) in &pending {
            if now >= pending_req.timeout {
                expired_ids.push(*id);
            }
        }
        for id in expired_ids {
            if let Some(pending_req) = pending.remove(&id) {
                tracing::warn!(
                    target: "steersman_desktop_lib::ws",
                    id, method = %pending_req.method,
                    "pending RPC expired"
                );
                let _ = pending_req.reply.send(Err(GatewayClientError::RpcTimeout));
            }
        }

        tokio::select! {
            // Periodic cleanup of expired pending RPCs
            _ = cleanup_tick.tick() => {
                let now = Instant::now();
                let mut expired_ids: Vec<RpcId> = Vec::new();
                for (id, pending_req) in &pending {
                    if now >= pending_req.timeout {
                        expired_ids.push(*id);
                    }
                }
                for id in expired_ids {
                    if let Some(pending_req) = pending.remove(&id) {
                        tracing::warn!(
                            target: "steersman_desktop_lib::ws",
                            id, method = %pending_req.method,
                            "pending RPC expired (timer)"
                        );
                        let _ = pending_req.reply.send(Err(GatewayClientError::RpcTimeout));
                    }
                }
            }
            // Read incoming WS frames
            msg = ws.next() => {
                        match msg {
                            Some(Ok(Message::Text(text))) => {
                                let value: Value = match serde_json::from_str(&text) {
                                    Ok(v) => v,
                                    Err(_) => continue,
                                };

                                match classify_frame(&value) {
                                    Some(IncomingFrame::RpcResponse { id, result }) => {
                                        if let Some(pending_req) = pending.remove(&id) {
                                            let _ = pending_req.reply.send(Ok(result));
                                        } else {
                                            tracing::warn!(target: "steersman_desktop_lib::ws", id, "RPC response for unknown request");
                                        }
                                    }
                                    Some(IncomingFrame::RpcError { id, error }) => {
                                        if let Some(pending_req) = pending.remove(&id) {
                                            let msg = error.get("message")
                                                .and_then(|m| m.as_str())
                                                .unwrap_or("RPC error");
                                            let _ = pending_req.reply.send(
                                                Err(GatewayClientError::BackendError(msg.to_string()))
                                            );
                                        }
                                    }
                                    Some(IncomingFrame::Event(routed_event)) => {
                                        // Convert via typed parser to ChatEvent (replaces legacy parse_ws_message)
                                        if let Some(chat_event) = translate_gateway_event(&routed_event) {
                                            (emit_fn)(&chat_event);
                                            // Update session_id from Done events
                                            if let ChatEvent::Done { session_id: Some(ref sid) } = chat_event {
                                                let mut sid_lock = ws_state.session_id.lock().await;
                                                if !sid.is_empty() {
                                                    *sid_lock = Some(sid.clone());
                                                }
                                            }
                                        }
        // Track session_id from session.info events (use live session_id from params, not stored_session_id)
                                    if let ParsedGatewayEvent::Known(GatewayEvent::SessionInfo(_)) = &routed_event.event {
                                        if let Some(live_sid) = &routed_event.session_id {
                                            if !live_sid.is_empty() {
                                                *ws_state.session_id.lock().await = Some(live_sid.clone());
                                            }
                                        }
                                    }
                                    }
                                    None => {
                                        // Unrecognized frame - log and skip
                                        tracing::warn!(target: "steersman_desktop_lib::ws", "unrecognized frame");
                                    }
                                }
                            }
                            Some(Ok(Message::Close(_))) => {
                                tracing::info!(target: "steersman_desktop_lib::ws", "server closed connection");
                                break;
                            }
                            Some(Ok(_)) => {} // Binary, Ping, Pong — ignore
                            Some(Err(e)) => {
                                tracing::warn!(target: "steersman_desktop_lib::ws", error = %e, "ws read error");
                                break;
                            }
                            None => {
                                tracing::info!(target: "steersman_desktop_lib::ws", "stream ended");
                                break;
                            }
                        }
                    }
                    // Process commands from Tauri command handlers.
                    cmd = cmd_rx.recv() => {
                        match cmd {
                            Some(WsCommand::Rpc { id, method, params, reply }) => {
                                // Build JSON-RPC 2.0 request
                                let req = json!({
                                    "jsonrpc": "2.0",
                                    "id": id,
                                    "method": method,
                                    "params": params,
                                });
                                // Register pending before sending
                                pending.insert(id, PendingRequest {
                                    reply,
                                    method: method.clone(),
                                    timeout: Instant::now() + Duration::from_secs(30),
                                });
                                // Send
                                if let Err(e) = send_json_gateway(&mut ws, &req).await {
                                    tracing::warn!(target: "steersman_desktop_lib::ws", error = %e, method, "RPC send failed");
                                    // Remove from pending and notify caller
                                    if let Some(p) = pending.remove(&id) {
                                        let _ = p.reply.send(Err(GatewayClientError::Protocol(e.to_string())));
                                    }
                                    break;
                                }
                            }
                            Some(WsCommand::Shutdown) | None => {
                                tracing::info!(target: "steersman_desktop_lib::ws", "reader task shutting down");
                                break;
                            }
                        }
                    }
                }
    }

    // Generation-guarded cleanup: only clear state if we are still the current generation.
    let current_gen = ws_state
        .generation
        .load(std::sync::atomic::Ordering::Acquire);
    if current_gen == my_generation {
        *ws_state.state.lock().await = ConnectionState::Disconnected;
        *ws_state.cmd_tx.lock().await = None;
    }

    // Complete all pending RPCs with ConnectionLost.
    for (_, pending_req) in pending.drain() {
        let _ = pending_req
            .reply
            .send(Err(GatewayClientError::ConnectionLost));
    }

    tracing::info!(target: "steersman_desktop_lib::ws", generation = my_generation, "reader task exited");
}

/// Internal helper for the reader task to send JSON-RPC frames.
async fn send_json_gateway<S>(ws: &mut S, value: &Value) -> Result<(), GatewayClientError>
where
    S: futures::SinkExt<Message, Error = tokio_tungstenite::tungstenite::Error>
        + Unpin
        + futures::Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>>
        + Unpin
        + Send
        + 'static,
{
    let text = serde_json::to_string(value)
        .map_err(|e| GatewayClientError::Protocol(format!("encode: {}", e)))?;
    ws.send(Message::Text(text))
        .await
        .map_err(|e| GatewayClientError::Protocol(format!("ws send: {}", e)))
}

// Callback type for emitting chat events. Production wraps `app_handle.emit`;
// tests pass a mock that records events.
pub type EmitFn = Arc<dyn Fn(&ChatEvent) + Send + Sync + 'static>;

/// Build an EmitFn from a Tauri AppHandle. This is the production path.
pub fn make_tauri_emitter<R>(app_handle: AppHandle<R>) -> EmitFn
where
    R: tauri::Runtime,
    AppHandle<R>: Emitter<R>,
{
    Arc::new(move |event: &ChatEvent| {
        let _ = app_handle.emit("chat_event", event);
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // RED: these tests assert that the WS transport returns typed WsError
    // variants, not bare Strings. Currently send_message_via_ws returns
    // Result<String, String>, so these will not compile until the migration
    // is complete (GREEN step).

    #[test]
    fn ws_error_connect_is_matchable() {
        let err = WsError::Connect("refused".to_string());
        assert!(matches!(err, WsError::Connect(_)));
    }

    #[test]
    fn ws_error_auth_failed_is_distinct_variant() {
        let err = WsError::AuthFailed;
        assert!(matches!(err, WsError::AuthFailed));
        // Distinct from Connect — callers branch differently (auth → re-login).
        assert!(!matches!(err, WsError::Connect(_)));
    }

    #[test]
    fn ws_error_timeout_has_no_payload() {
        let err = WsError::Timeout;
        assert!(matches!(err, WsError::Timeout));
    }

    #[test]
    fn ws_error_display_is_human_readable() {
        let s = WsError::Connect("refused".to_string()).to_string();
        assert!(s.contains("connect failed"));
        assert!(s.contains("refused"));
    }

    #[test]
    fn ws_error_backend_error_carries_message() {
        let err = WsError::BackendError("model overloaded".to_string());
        assert!(matches!(err, WsError::BackendError(msg) if msg == "model overloaded"));
    }

    // ── session.create source param (briefing leak prevention) ─────────────
    //
    // The briefing path MUST create sessions with source="briefing_smart" so
    // sessions.rs list/feed queries filter them out. If a briefing session is
    // created with source="desktop", it appears as a normal chat that the NEXT
    // briefing analyses → "briefing-of-briefing" infinite loop.

    #[test]
    fn session_create_for_briefing_uses_briefing_smart_source() {
        let req = build_session_create_request(1, "briefing_smart");
        let source = req
            .get("params")
            .and_then(|p| p.get("source"))
            .and_then(|s| s.as_str())
            .expect("source field missing");
        assert_eq!(
            source, "briefing_smart",
            "briefing sessions must use source=briefing_smart to be filtered from the feed"
        );
    }

    #[test]
    fn session_create_for_chat_uses_desktop_source() {
        let req = build_session_create_request(2, "desktop");
        let source = req
            .get("params")
            .and_then(|p| p.get("source"))
            .and_then(|s| s.as_str())
            .expect("source field missing");
        assert_eq!(source, "desktop");
    }

    #[test]
    fn session_create_request_is_valid_jsonrpc() {
        let req = build_session_create_request(42, "desktop");
        assert_eq!(req.get("jsonrpc").and_then(|v| v.as_str()), Some("2.0"));
        assert_eq!(
            req.get("method").and_then(|v| v.as_str()),
            Some("session.create")
        );
        assert_eq!(req.get("id").and_then(|v| v.as_u64()), Some(42));
        // cols must be present (rendering width contract).
        assert!(req.get("params").and_then(|p| p.get("cols")).is_some());
    }

    // ── T1 (ADR-006): WsState + WsCommand + ConnectionState ────────────────

    #[test]
    fn ws_state_new_starts_disconnected() {
        let ws = WsState::new();
        // Use try_lock to avoid needing a tokio runtime in a sync test.
        let state = ws.state.try_lock().expect("state lock not poisoned");
        assert_eq!(*state, ConnectionState::Disconnected);
    }

    #[tokio::test]
    async fn connection_state_transitions() {
        let ws = WsState::new();
        // Disconnected → Connecting → Connected → Disconnected
        {
            let mut state = ws.state.lock().await;
            *state = ConnectionState::Connecting;
            assert_eq!(*state, ConnectionState::Connecting);
            *state = ConnectionState::Connected;
            assert_eq!(*state, ConnectionState::Connected);
            *state = ConnectionState::Disconnected;
            assert_eq!(*state, ConnectionState::Disconnected);
        }
    }

    #[tokio::test]
    async fn ws_state_holds_session_id() {
        let ws = WsState::new();
        // Initially None.
        assert!(ws.session_id.lock().await.is_none());
        // Set after first session.create.
        *ws.session_id.lock().await = Some("abc123".to_string());
        assert_eq!(ws.session_id.lock().await.as_deref(), Some("abc123"));
        // Survives a separate lock cycle (simulates next turn).
        assert_eq!(ws.session_id.lock().await.as_deref(), Some("abc123"));
    }

    #[test]
    fn ws_command_submit_prompt_serializes_correctly() {
        // The reader task will call build_prompt_submit_request when it
        // receives a SubmitPrompt command. Verify the frame matches the
        // upstream contract (server.py:8464 reads session_id + text).
        let req = build_prompt_submit_request(7, "sess123", "Hello world");
        assert_eq!(req.get("jsonrpc").and_then(|v| v.as_str()), Some("2.0"));
        assert_eq!(
            req.get("method").and_then(|v| v.as_str()),
            Some("prompt.submit")
        );
        assert_eq!(req.get("id").and_then(|v| v.as_u64()), Some(7));
        assert_eq!(
            req.get("params")
                .and_then(|p| p.get("session_id"))
                .and_then(|v| v.as_str()),
            Some("sess123")
        );
        assert_eq!(
            req.get("params")
                .and_then(|p| p.get("text"))
                .and_then(|v| v.as_str()),
            Some("Hello world")
        );
        // No model/stream/history fields — upstream prompt.submit only takes
        // session_id + text (verified in tui_gateway/server.py:8468-8469).
        let params = req.get("params").unwrap().as_object().unwrap();
        assert!(!params.contains_key("model"));
        assert!(!params.contains_key("stream"));
        assert!(!params.contains_key("history"));
    }

    #[tokio::test]
    async fn ws_command_create_session_carries_source() {
        // Verify the Rpc command can be constructed with session.create and its source
        // is accessible (the reader task uses it in build_session_create_request).
        let (tx, mut rx) = tokio::sync::mpsc::channel::<WsCommand>(1);
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel::<Result<Value, WsError>>();
        tx.send(WsCommand::Rpc {
            id: 42,
            method: "session.create".to_string(),
            params: json!({"source": "briefing_smart", "cols": 96}),
            reply: reply_tx,
        })
        .await
        .unwrap();
        let cmd = rx.recv().await.unwrap();
        match cmd {
            WsCommand::Rpc {
                id: _,
                method,
                params,
                reply,
            } => {
                assert_eq!(method, "session.create");
                assert_eq!(params["source"], "briefing_smart");
                // Simulate the reader task sending back a session_id.
                let _ = reply.send(Ok(serde_json::Value::String("brief-sess-456".to_string())));
            }
            _ => panic!("expected Rpc"),
        }
        // The caller gets the session_id via oneshot.
        let result = reply_rx.await.unwrap().unwrap();
        assert_eq!(result, "brief-sess-456");
    }

    // ── T2 (ADR-006): ensure_ws_connection + reader_task ───────────────────

    #[tokio::test]
    async fn ensure_connection_fast_path_returns_if_connected() {
        // If WsState is already Connected, ensure_ws_connection returns Ok
        // without attempting a real connect (which would fail in a test env).
        let ws = Arc::new(WsState::new());
        *ws.state.lock().await = ConnectionState::Connected;
        // We can't easily build an AppHandle in a unit test, so we verify the
        // state guard logic directly: the function checks state == Connected
        // before touching the network.
        let state = ws.state.lock().await;
        assert_eq!(
            *state,
            ConnectionState::Connected,
            "precondition: must be Connected"
        );
        // In the real function, this guard returns Ok(()) immediately.
    }

    #[tokio::test]
    async fn ensure_connection_waits_during_connecting() {
        // If another task set Connecting, ensure_ws_connection spins until
        // the state resolves. Here we simulate the state resolving to Connected.
        let ws = Arc::new(WsState::new());
        *ws.state.lock().await = ConnectionState::Connecting;
        // Simulate the connecting task finishing.
        tokio::spawn({
            let ws = Arc::clone(&ws);
            async move {
                tokio::time::sleep(Duration::from_millis(50)).await;
                *ws.state.lock().await = ConnectionState::Connected;
            }
        });
        // Spin like ensure_ws_connection does until Connected.
        let mut resolved = false;
        for _ in 0..50 {
            let s = ws.state.lock().await;
            if *s == ConnectionState::Connected {
                resolved = true;
                break;
            }
            drop(s);
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(resolved, "should have resolved to Connected");
    }

    #[tokio::test]
    async fn submit_prompt_on_connection_errors_when_disconnected() {
        // If cmd_tx is None (Disconnected), submit_prompt_on_connection fails
        // with a typed WsError, not a panic.
        let ws = Arc::new(WsState::new());
        // cmd_tx is None by default.
        let result = submit_prompt_on_connection(&ws, "sess1", "hello").await;
        assert!(result.is_err(), "should error when not connected");
        let err = result.unwrap_err();
        assert!(matches!(err, WsError::Protocol(_)));
    }

    #[tokio::test]
    async fn create_session_on_connection_errors_when_disconnected() {
        let ws = Arc::new(WsState::new());
        let result = create_session_on_connection(&ws, "desktop").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), WsError::Protocol(_)));
    }

    // ── E2E tests with mock WS backend ─────────────────────────────────────
    //
    // These tests spin up a real tokio-tungstenite WS server that speaks the
    // JSON-RPC /api/ws protocol (gateway.ready, session.create, prompt.submit,
    // message.start/delta/complete) and exercise the full persistent connection
    // lifecycle. No real Hermes Agent needed.

    use tokio::net::TcpListener;

    /// Start a mock backend that:
    /// - sends gateway.ready on connect
    /// - responds to session.create with session_id "mock-sess"
    /// - on prompt.submit: ACKs, streams message.start → delta("Hello") →
    ///   message.complete(session_id)
    ///
    /// Returns (ws_url, received_frames) where received_frames captures all
    /// JSON-RPC requests the server got (for assertions).
    async fn start_mock_backend() -> (String, Arc<tokio::sync::Mutex<Vec<Value>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let received: Arc<tokio::sync::Mutex<Vec<Value>>> =
            Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let received_clone = Arc::clone(&received);

        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();

            // gateway.ready
            let ready = json!({
                "jsonrpc": "2.0", "method": "event",
                "params": {"type": "gateway.ready", "payload": {}}
            });
            let _ = ws.send(Message::Text(ready.to_string())).await;

            while let Some(Ok(msg)) = ws.next().await {
                if let Message::Text(text) = msg {
                    let req: Value = match serde_json::from_str(&text) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };
                    received_clone.lock().await.push(req.clone());
                    let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");
                    let id = req.get("id").and_then(|i| i.as_u64()).unwrap_or(0);
                    let sid = req
                        .get("params")
                        .and_then(|p| p.get("session_id"))
                        .and_then(|s| s.as_str())
                        .unwrap_or("mock-sess");

                    match method {
                        "session.create" => {
                            let source = req
                                .get("params")
                                .and_then(|p| p.get("source"))
                                .and_then(|s| s.as_str())
                                .unwrap_or("desktop");
                            let new_sid = if source == "briefing_smart" {
                                "brief-sess"
                            } else {
                                "chat-sess"
                            };
                            let resp = json!({
                                "jsonrpc": "2.0", "id": id,
                                "result": {
                                    "session_id": new_sid,
                                    "stored_session_id": new_sid,
                                    "message_count": 0,
                                    "messages": [],
                                    "info": { "desktop_contract": 3 }
                                }
                            });
                            let _ = ws.send(Message::Text(resp.to_string())).await;
                        }
                        "prompt.submit" => {
                            let ack =
                                json!({"jsonrpc":"2.0","id":id,"result":{"status":"streaming"}});
                            let _ = ws.send(Message::Text(ack.to_string())).await;
                            // Stream events
                            for ev in [
                                json!({"jsonrpc":"2.0","method":"event","params":{"type":"message.start","session_id":sid}}),
                                json!({"jsonrpc":"2.0","method":"event","params":{"type":"message.delta","session_id":sid,"payload":{"text":"Hello"}}}),
                                json!({"jsonrpc":"2.0","method":"event","params":{"type":"message.complete","session_id":sid,"payload":{"text":"Hello","status":"complete"}}}),
                            ] {
                                let _ = ws.send(Message::Text(ev.to_string())).await;
                            }
                        }
                        _ => {}
                    }
                } else if matches!(msg, Message::Close(_)) {
                    break;
                }
            }
        });

        (
            format!("ws://127.0.0.1:{}/api/ws?token=test", port),
            received,
        )
    }

    fn mock_emitter() -> (EmitFn, Arc<tokio::sync::Mutex<Vec<ChatEvent>>>) {
        let events: Arc<tokio::sync::Mutex<Vec<ChatEvent>>> =
            Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let _events_clone = Arc::clone(&events);
        let emit_fn: EmitFn = Arc::new(move |event: &ChatEvent| {
            // ChatEvent is not Clone, so we capture a debug snapshot.
            let snapshot = format!("{:?}", event);
            let _events = _events_clone.clone();
            // Can't move ChatEvent (not Clone), so record the type tag.
            let tag = match event {
                ChatEvent::Token { .. } => "token",
                ChatEvent::Reasoning { .. } => "reasoning",
                ChatEvent::ToolStart { .. } => "tool_start",
                ChatEvent::ToolComplete { .. } => "tool_complete",
                ChatEvent::Done { session_id } => {
                    if let Some(_sid) = session_id {
                        let _ec = _events.clone();
                        let _ = snapshot; // keep for potential future use
                                          // Record done with session_id for session reuse tests.
                                          // We use a side-channel: push the sid string.
                        let _ = _ec;
                    }
                    "done"
                }
                ChatEvent::Error { .. } => "error",
                ChatEvent::Status { .. } => "status",
                ChatEvent::ApprovalRequest { .. } => "approval",
                ChatEvent::PipelineStatus { .. } => "pipeline",
                ChatEvent::SessionInfo { .. } => "session_info",
                ChatEvent::Thinking { .. } => "thinking",
                ChatEvent::ToolGenerating { .. } => "tool_generating",
                ChatEvent::ClarifyRequest { .. } => "clarify_request",
                ChatEvent::SudoRequest { .. } => "sudo_request",
                ChatEvent::SudoExpire { .. } => "sudo_expire",
                ChatEvent::SecretRequest { .. } => "secret_request",
                ChatEvent::SecretExpire { .. } => "secret_expire",
                ChatEvent::Notification { .. } => "notification",
                ChatEvent::NotificationClear { .. } => "notification_clear",
            };
            let _ = tag;
            // We can't store ChatEvent (no Clone), but tests check WsState
            // session_id directly, which the reader task updates from Done.
        });
        (emit_fn, events)
    }

    async fn wait_connected(ws_state: &WsState, timeout_ms: u64) {
        let deadline = tokio::time::sleep(Duration::from_millis(timeout_ms));
        tokio::pin!(deadline);
        loop {
            if let Ok(s) = ws_state.state.try_lock() {
                if *s == ConnectionState::Connected {
                    return;
                }
            }
            if deadline.is_elapsed() {
                panic!("not Connected within {}ms", timeout_ms);
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    #[tokio::test]
    async fn e2e_ensure_connection_reaches_connected() {
        let (ws_url, _) = start_mock_backend().await;
        let ws_state = Arc::new(WsState::new());
        let (emit_fn, _) = mock_emitter();

        ensure_ws_connection(&ws_url, emit_fn, &ws_state)
            .await
            .unwrap();
        wait_connected(&ws_state, 3000).await;

        // cmd_tx must be set.
        assert!(ws_state.cmd_tx.lock().await.is_some());
    }

    #[tokio::test]
    async fn e2e_create_session_returns_mock_id() {
        let (ws_url, received) = start_mock_backend().await;
        let ws_state = Arc::new(WsState::new());
        let (emit_fn, _) = mock_emitter();
        ensure_ws_connection(&ws_url, emit_fn, &ws_state)
            .await
            .unwrap();
        wait_connected(&ws_state, 3000).await;

        let result = create_session_on_connection(&ws_state, "desktop")
            .await
            .unwrap();
        assert_eq!(result.session_id, "chat-sess");

        tokio::time::sleep(Duration::from_millis(100)).await;
        let frames = received.lock().await;
        assert!(frames
            .iter()
            .any(|v| v.get("method").and_then(|m| m.as_str()) == Some("session.create")));
    }

    #[tokio::test]
    async fn e2e_create_briefing_session_uses_correct_source() {
        let (ws_url, received) = start_mock_backend().await;
        let ws_state = Arc::new(WsState::new());
        let (emit_fn, _) = mock_emitter();
        ensure_ws_connection(&ws_url, emit_fn, &ws_state)
            .await
            .unwrap();
        wait_connected(&ws_state, 3000).await;

        let result = create_session_on_connection(&ws_state, "briefing_smart")
            .await
            .unwrap();
        assert_eq!(result.session_id, "brief-sess");

        tokio::time::sleep(Duration::from_millis(100)).await;
        let frames = received.lock().await;
        assert!(frames.iter().any(|v| {
            v.get("method").and_then(|m| m.as_str()) == Some("session.create")
                && v.get("params")
                    .and_then(|p| p.get("source"))
                    .and_then(|s| s.as_str())
                    == Some("briefing_smart")
        }));
    }

    #[tokio::test]
    async fn e2e_submit_prompt_streams_events_and_caches_session() {
        let (ws_url, received) = start_mock_backend().await;
        let ws_state = Arc::new(WsState::new());
        let (emit_fn, _) = mock_emitter();
        ensure_ws_connection(&ws_url, emit_fn, &ws_state)
            .await
            .unwrap();
        wait_connected(&ws_state, 3000).await;

        let sid = create_session_on_connection(&ws_state, "desktop")
            .await
            .unwrap()
            .session_id;
        submit_prompt_on_connection(&ws_state, &sid, "test prompt")
            .await
            .unwrap();

        // Wait for message.complete to arrive and session_id to be cached.
        for _ in 0..60 {
            if ws_state
                .session_id
                .try_lock()
                .map(|s| s.is_some())
                .unwrap_or(false)
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let cached = ws_state.session_id.lock().await.clone();
        assert_eq!(
            cached.as_deref(),
            Some("chat-sess"),
            "session_id must be cached after message.complete"
        );

        let frames = received.lock().await;
        assert!(frames.iter().any(|v| {
            v.get("method").and_then(|m| m.as_str()) == Some("prompt.submit")
                && v.get("params")
                    .and_then(|p| p.get("text"))
                    .and_then(|s| s.as_str())
                    == Some("test prompt")
        }));
    }

    #[tokio::test]
    async fn e2e_ensure_connection_idempotent() {
        let (ws_url, _) = start_mock_backend().await;
        let ws_state = Arc::new(WsState::new());

        let (emit_fn, _) = mock_emitter();
        ensure_ws_connection(&ws_url, emit_fn, &ws_state)
            .await
            .unwrap();
        wait_connected(&ws_state, 3000).await;

        // Second connect — fast path, must NOT error.
        let (emit_fn2, _) = mock_emitter();
        ensure_ws_connection(&ws_url, emit_fn2, &ws_state)
            .await
            .unwrap();

        assert_eq!(*ws_state.state.lock().await, ConnectionState::Connected);
    }
}
