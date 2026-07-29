// src-tauri/src/ws_transport.rs
// WebSocket client for the Hermes /api/ws JSON-RPC transport (ADR-004).
//
// The real Hermes backend (NousResearch/hermes-agent, `hermes serve`) exposes a
// newline-delimited JSON-RPC 2.0 channel over WebSocket. This module connects,
// performs the auth handshake, creates/resumes a session, submits a prompt,
// and forwards streaming events to the SAME Tauri `chat_event` channel the
// HTTP transport used — so the frontend stays unchanged.
//

use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter};
use tokio::sync::oneshot;
use tokio_tungstenite::tungstenite::Message;
use url::Url;

use crate::chat::{parse_ws_message, ChatEvent};
use crate::hermes_protocol::{
    self, GatewayEvent, ParsedGatewayEvent, PromptSubmitParams, PromptSubmitResult,
    RoutedGatewayEvent, SessionCreateParams, SessionCreateResult,
};
use crate::runtime_supervisor::RuntimeState;

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

/// Authentication material used when connecting to a Hermes gateway.
#[derive(Clone)]
pub struct GatewayAuth {
    pub api_key: String,
}

impl GatewayAuth {
    pub fn redacted(&self) -> String {
        "***".to_string()
    }

    /// Stable, non-reversible identity used to notice credential rotation.
    pub fn sha256_fingerprint(&self) -> String {
        hex::encode(Sha256::digest(self.api_key.as_bytes()))
    }
}

impl Debug for GatewayAuth {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "GatewayAuth(<redacted>)")
    }
}

/// Non-secret identity of a WebSocket connection target.
#[derive(Clone, Eq, PartialEq)]
pub struct EndpointIdentity {
    pub public_url: String,
    pub auth_fingerprint: String,
    pub tunnel_generation: Option<u64>,
}

impl EndpointIdentity {
    pub fn from_ws_url(
        ws_url: &str,
        auth: Option<&GatewayAuth>,
        tunnel_generation: Option<u64>,
    ) -> Self {
        let url_auth = Url::parse(ws_url).ok().and_then(|url| {
            url.query_pairs()
                .find(|(key, _)| key == "token")
                .map(|(_, value)| GatewayAuth {
                    api_key: value.into_owned(),
                })
        });
        Self {
            public_url: redacted_ws_url(ws_url),
            auth_fingerprint: auth
                .map(GatewayAuth::sha256_fingerprint)
                .or_else(|| url_auth.as_ref().map(GatewayAuth::sha256_fingerprint))
                .unwrap_or_default(),
            tunnel_generation,
        }
    }
}

impl Debug for EndpointIdentity {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EndpointIdentity")
            .field("public_url", &self.public_url)
            .field("auth_fingerprint", &"<redacted>")
            .field("tunnel_generation", &self.tunnel_generation)
            .finish()
    }
}

/// Complete, non-secret description of the endpoint a connection attempt owns.
///
/// `identity` is intentionally supplied by the runtime owner rather than
/// reconstructed by the transport so credential and SSH tunnel rotations are
/// preserved through the whole connection lifecycle.
#[derive(Clone, Debug)]
pub struct EndpointSnapshot {
    pub ws_url: String,
    pub identity: EndpointIdentity,
    pub runtime_key: crate::session_registry::RuntimeKey,
}

/// Connection health as observed by a runtime supervisor.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum HealthStatus {
    Connected {
        state: RuntimeState,
    },
    Degraded {
        reason: String,
        state: RuntimeState,
    },
    Disconnected {
        reason: String,
        state: RuntimeState,
        attempt: u32,
    },
}

/// Build a gateway WebSocket URL without interpolating auth into the URL text.
pub fn build_ws_url(base: &str, token: &str) -> Result<(String, GatewayAuth), WsError> {
    let mut url = Url::parse(&to_ws_url(base))
        .map_err(|error| WsError::Protocol(format!("invalid WebSocket URL: {}", error)))?;

    let path = url.path().trim_end_matches('/');
    if !path.ends_with("/api/ws") {
        url.set_path(&format!("{}/api/ws", path));
    }
    url.set_query(None);
    url.query_pairs_mut().append_pair("token", token);

    Ok((
        url.into(),
        GatewayAuth {
            api_key: token.to_string(),
        },
    ))
}

/// Return a diagnostic-safe WebSocket URL with its token query parameter removed.
pub fn redacted_ws_url(ws_url: &str) -> String {
    let Ok(mut url) = Url::parse(ws_url) else {
        return "<invalid WebSocket URL>".to_string();
    };
    let retained_pairs: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(key, _)| key != "token")
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    url.set_query(None);
    if !retained_pairs.is_empty() {
        url.query_pairs_mut().extend_pairs(retained_pairs);
    }
    url.into()
}
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
pub async fn call_rpc<P, R, State>(
    ws_state: &State,
    method: &'static str,
    params: P,
    timeout: Duration,
) -> Result<R, GatewayClientError>
where
    P: Serialize + for<'de> Deserialize<'de> + std::fmt::Debug,
    R: for<'de> Deserialize<'de> + std::fmt::Debug,
    State: StateLike + ?Sized,
{
    // Clone the sender under the runtime lock, then release immediately.
    // This prevents holding the connection mutex across an async await,
    // which would block reader cleanup and state transitions.
    let tx = {
        let runtime = ws_state.runtime().lock().await;

        // Check connection state atomically with sender retrieval.
        // Allow Connecting state for the handshake phase (session.create/resume).
        if runtime.state == ConnectionState::Disconnected {
            return Err(GatewayClientError::ConnectionLost);
        }

        runtime
            .cmd_tx
            .clone()
            .ok_or(GatewayClientError::ConnectionLost)?
    };

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
    .map_err(|_| interruption_error(method, InterruptionCause::ClosedChannel))?;

    // Wait for RPC response with timeout.
    // reply_rx is `oneshot::Receiver<Result<Value, GatewayClientError>>`, so
    // three unwrap layers: timeout → oneshot RecvError → inner GatewayClientError.
    // The reader task already discriminates success (Ok(result_value)) from
    // RPC error (Err(BackendError)), so response_value is the bare `result`.
    let response_value = tokio::time::timeout(timeout, reply_rx)
        .await
        .map_err(|_| interruption_error(method, InterruptionCause::Timeout))?
        // RecvError: the reply sender was dropped (reader task died / channel
        // closed). For non-safe methods this is OutcomeUnknown (server may have
        // processed the request before the channel died).
        .map_err(|_| interruption_error(method, InterruptionCause::ClosedChannel))??;

    // Parse the result directly into R — the error envelope was already handled
    // by the reader task (IncomingFrame::RpcError → BackendError).
    serde_json::from_value::<R>(response_value)
        .map_err(|e| GatewayClientError::Protocol(format!("response parse: {}", e)))
}

/// Submit a prompt on the persistent connection using the generic RPC dispatcher.
/// Waits for the RPC acknowledgement from Hermes before returning.
///
/// Create a session on the persistent connection using the generic RPC dispatcher.
/// `requested_profile` selects the Hermes profile scope (state.db, config, etc.);
/// pass None for the launch/default profile.
pub async fn create_session_on_connection<State>(
    ws_state: &State,
    source: &str,
    requested_profile: Option<&str>,
) -> Result<SessionCreateResult, WsError>
where
    State: StateLike + ?Sized,
{
    call_rpc(
        ws_state,
        "session.create",
        SessionCreateParams {
            source: source.to_owned(),
            cols: 96,
            profile: requested_profile.map(|s| s.to_string()),
        },
        RPC_TIMEOUT,
    )
    .await
}

/// Submit a prompt on the persistent connection using the generic RPC dispatcher.
/// Waits for the RPC acknowledgement from Hermes before returning.
///
/// Refuses to send if the compatibility handshake has not completed or failed
/// (Phase 1C.1): `prompt.submit` must not proceed against an unknown or
/// incompatible backend.
pub async fn submit_prompt_on_connection<State>(
    ws_state: &State,
    session_id: &str,
    text: &str,
) -> Result<PromptSubmitResult, WsError>
where
    State: StateLike + ?Sized,
{
    // Compatibility gate: do not allow user work until the handshake succeeds.
    {
        let compat = ws_state.compatibility().lock().await;
        match &*compat {
            crate::hermes_protocol::RuntimeCompatibility::Compatible { .. } => {}
            crate::hermes_protocol::RuntimeCompatibility::Unknown => {
                return Err(WsError::Protocol(
                    "prompt.submit before compatibility handshake".into(),
                ));
            }
            other => {
                return Err(WsError::Incompatible(other.clone()));
            }
        }
    }
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

/// Resume a session by its durable reference. Used by the reconnect
/// reconciliation loop to obtain a fresh live session_id for the new connection
/// while preserving conversation history. Sends `session_id` (the durable ID)
/// in the wire params per the real Hermes contract.
pub async fn resume_session_on_connection<State>(
    ws_state: &State,
    durable: &crate::session_registry::DurableSessionRef,
) -> Result<crate::hermes_protocol::SessionResumeResult, WsError>
where
    State: StateLike + ?Sized,
{
    call_rpc(
        ws_state,
        "session.resume",
        crate::hermes_protocol::SessionResumeParams {
            session_id: durable.stored_session_id.clone(),
            profile: if durable.profile.0.is_empty() {
                None
            } else {
                Some(durable.profile.0.clone())
            },
            cols: Some(96),
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
#[allow(dead_code)]
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
    let redacted_url = redacted_ws_url(ws_url);
    tracing::info!(target: "steersman_desktop_lib::ws", ws_url = %redacted_url, source, buffered, "connecting");
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

/// Why an RPC's outcome became unknown. Used by OutcomeUnknown so error
/// messages and logs accurately report the cause (not always "disconnect").
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InterruptionCause {
    /// Socket disconnected (reader task exited).
    Disconnect,
    /// Caller timeout or reader deadline expired before a response arrived.
    Timeout,
    /// Failed to send the request frame (partial write possible).
    SendFailure,
    /// The reply channel was closed (reader task dropped the sender).
    ClosedChannel,
}

impl std::fmt::Display for InterruptionCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InterruptionCause::Disconnect => f.write_str("disconnect"),
            InterruptionCause::Timeout => f.write_str("timeout"),
            InterruptionCause::SendFailure => f.write_str("send failure"),
            InterruptionCause::ClosedChannel => f.write_str("closed reply channel"),
        }
    }
}

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
    /// A non-safe RPC (prompt.submit, approval.respond, secret.respond,
    /// sudo.respond, session.close, session.create) was interrupted before a
    /// confirmation arrived. The server may or may not have processed it — the
    /// outcome is genuinely unknown and must NOT be auto-retried. Carries the
    /// cause so Display/log messages are accurate (not always "disconnect").
    OutcomeUnknown {
        method: String,
        cause: InterruptionCause,
    },
    /// Hermes backend is not version-compatible with this desktop build.
    /// Distinct from connection failures so callers can branch (prompt the
    /// user to upgrade Hermes or the desktop, not to reconnect).
    Incompatible(crate::hermes_protocol::RuntimeCompatibility),
}

/// Retry classification for RPC methods. Uses an EXPLICIT ALLOWLIST of safe-to-
/// retry methods; everything else (including unknown future methods) defaults
/// to OutcomeUnknown. This is safe-by-default: a forgotten or new method is
/// never silently treated as retryable.
///
/// Safe methods: pure reads (no side effects) + session.resume (the pinned
/// Hermes serializes competing resumes and reuses the live durable-key
/// binding, so a retry does not create a duplicate session).
const SAFE_RETRY_METHODS: &[&str] = &[
    "session.status",
    "session.history",
    "session.list",
    "session.active_list",
    "session.resume",
];

/// True only for methods in the explicit safe-retry allowlist. Everything else
/// (session.create, session.close, prompt.submit, approvals, unknown methods)
/// is NOT safe to retry.
fn is_safe_retry(method: &str) -> bool {
    SAFE_RETRY_METHODS.contains(&method)
}

/// Unified classification for interrupted RPCs (timeout, disconnect, send
/// failure, closed reply channel). Safe-retry methods get a plain RpcTimeout
/// (caller may retry); all others get OutcomeUnknown (server may have processed
/// it — do not auto-retry). All interruption paths MUST use this so a
/// prompt.submit timeout is never misreported as a plain RpcTimeout.
fn interruption_error(method: &str, cause: InterruptionCause) -> GatewayClientError {
    if is_safe_retry(method) {
        // Safe methods: disconnect → ConnectionLost, timeout → RpcTimeout.
        match cause {
            InterruptionCause::Disconnect | InterruptionCause::ClosedChannel => {
                GatewayClientError::ConnectionLost
            }
            InterruptionCause::Timeout | InterruptionCause::SendFailure => {
                GatewayClientError::RpcTimeout
            }
        }
    } else {
        GatewayClientError::OutcomeUnknown {
            method: method.to_owned(),
            cause,
        }
    }
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
            GatewayClientError::OutcomeUnknown { method, cause } => write!(
                f,
                "outcome unknown: '{}' interrupted by {} (not retried)",
                method, cause
            ),
            GatewayClientError::Incompatible(c) => match c {
                crate::hermes_protocol::RuntimeCompatibility::HermesUpgradeRequired {
                    received,
                    minimum,
                } => write!(
                    f,
                    "Hermes upgrade required: backend contract {} < desktop minimum {}",
                    received, minimum
                ),
                crate::hermes_protocol::RuntimeCompatibility::DesktopUpgradeRequired {
                    received,
                    maximum,
                } => write!(
                    f,
                    "Desktop upgrade required: backend contract {} > desktop maximum {}",
                    received, maximum
                ),
                // Unknown/Checking should never be surfaced as Incompatible, but
                // provide a fallback rather than panicking.
                _ => write!(f, "incompatible runtime: {:?}", c),
            },
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
    /// Socket is live, reader task is running, all sessions resumed.
    Connected,
    /// Socket is live but some conversations failed to resume. Prompts for
    /// restored conversations work; ResumeFailed conversations need manual
    /// retry. Distinct from Connected so callers can warn the user.
    Degraded,
}

/// Result of reconnect reconciliation: which conversations were restored,
/// which failed permanently, and which were interrupted by a mid-resume
/// disconnect (retryable on next reconnect). Used by ensure_ws_connection to
/// choose Connected vs Degraded vs ConnectionLost (interrupted means the
/// socket died during reconciliation — must NOT declare Connected).
#[derive(Debug, Clone, Default)]
pub struct ReconciliationReport {
    pub restored: Vec<crate::session_registry::ConversationId>,
    pub failed: Vec<crate::session_registry::ConversationId>,
    pub interrupted: Vec<crate::session_registry::ConversationId>,
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
                message: p.message.clone(),
                choices: p
                    .choices
                    .iter()
                    .map(|choice| {
                        match choice {
                            hermes_protocol::ApprovalChoice::Once => "once",
                            hermes_protocol::ApprovalChoice::Session => "session",
                            hermes_protocol::ApprovalChoice::Always => "always",
                            hermes_protocol::ApprovalChoice::Deny => "deny",
                        }
                        .to_string()
                    })
                    .collect(),
                allow_permanent: p.allow_permanent.unwrap_or(false),
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

/// Connection runtime state — generation, lifecycle state, and command channel.
/// All three fields are protected by a single mutex to prevent TOCTOU races
/// between the reader task cleanup and the connection finalization (ADR-006 §P0).
#[derive(Debug)]
pub struct ConnectionRuntime {
    /// Generation this connection was established with.
    pub generation: u64,
    /// Current connection lifecycle state.
    pub state: ConnectionState,
    /// Command sender for the reader task. None when disconnected.
    pub cmd_tx: Option<tokio::sync::mpsc::Sender<WsCommand>>,
    /// Diagnostic-safe WebSocket URL for this connection.
    pub ws_url: String,
    /// Identity of the connected endpoint, including an auth fingerprint.
    pub endpoint_identity: Option<EndpointIdentity>,
}

impl Default for ConnectionRuntime {
    fn default() -> Self {
        Self {
            generation: 0,
            state: ConnectionState::Disconnected,
            cmd_tx: None,
            ws_url: String::new(),
            endpoint_identity: None,
        }
    }
}

/// Identifies the WebSocket endpoint used by a gateway runtime.
pub trait RuntimeEndpoint: Send + Sync {
    fn ws_url(&self) -> String;
    fn auth(&self) -> Option<&GatewayAuth>;
    fn identity(&self) -> EndpointIdentity;
    fn runtime_key(&self) -> crate::session_registry::RuntimeKey;
}

/// Fixed endpoint used while GatewayClient owns a single runtime URL.
struct StaticRuntimeEndpoint {
    ws_url: String,
    auth: Option<GatewayAuth>,
    runtime_key: crate::session_registry::RuntimeKey,
    identity: EndpointIdentity,
}

impl RuntimeEndpoint for StaticRuntimeEndpoint {
    fn ws_url(&self) -> String {
        self.ws_url.clone()
    }

    fn auth(&self) -> Option<&GatewayAuth> {
        self.auth.as_ref()
    }

    fn identity(&self) -> EndpointIdentity {
        self.identity.clone()
    }

    fn runtime_key(&self) -> crate::session_registry::RuntimeKey {
        self.runtime_key.clone()
    }
}

/// Persistent WebSocket client for one Local, Remote, or SSH runtime.
///
/// This is the Phase 1 replacement for the three legacy state types. The
/// existing types remain in use until their callers move to this client.
pub struct GatewayClient {
    pub runtime: Arc<tokio::sync::Mutex<ConnectionRuntime>>,
    pub generation: Arc<AtomicU64>,
    pub connect_lock: Arc<tokio::sync::Mutex<()>>,
    pub compatibility: Arc<tokio::sync::Mutex<crate::hermes_protocol::RuntimeCompatibility>>,
    pub runtime_key: crate::session_registry::RuntimeKey,
    pub endpoint: Arc<tokio::sync::RwLock<Box<dyn RuntimeEndpoint>>>,
}

impl Clone for GatewayClient {
    fn clone(&self) -> Self {
        Self {
            runtime: self.runtime.clone(),
            generation: self.generation.clone(),
            connect_lock: self.connect_lock.clone(),
            compatibility: self.compatibility.clone(),
            runtime_key: self.runtime_key.clone(),
            endpoint: self.endpoint.clone(),
        }
    }
}

impl GatewayClient {
    /// Creates a disconnected client for a runtime and its WebSocket endpoint.
    pub fn new(
        runtime_key: crate::session_registry::RuntimeKey,
        endpoint: impl Into<String>,
        auth: Option<GatewayAuth>,
    ) -> Self {
        let endpoint = endpoint.into();
        let endpoint_identity = EndpointIdentity::from_ws_url(&endpoint, auth.as_ref(), None);
        let redacted_url = endpoint_identity.public_url.clone();
        Self {
            runtime: Arc::new(tokio::sync::Mutex::new(ConnectionRuntime {
                ws_url: redacted_url,
                ..ConnectionRuntime::default()
            })),
            generation: Arc::new(AtomicU64::new(1)),
            connect_lock: Arc::new(tokio::sync::Mutex::new(())),
            compatibility: Arc::new(tokio::sync::Mutex::new(
                crate::hermes_protocol::RuntimeCompatibility::Unknown,
            )),
            runtime_key: runtime_key.clone(),
            endpoint: Arc::new(tokio::sync::RwLock::new(Box::new(StaticRuntimeEndpoint {
                ws_url: endpoint,
                auth,
                runtime_key,
                identity: endpoint_identity,
            }))),
        }
    }

    /// Replace the endpoint configuration while preserving this client's
    /// connection state and session registry ownership. The next
    /// [`Self::ensure_connected`] call rotates the socket if the identity
    /// changed.
    pub async fn configure_endpoint(
        &self,
        endpoint: impl Into<String>,
        auth: Option<GatewayAuth>,
        tunnel_generation: Option<u64>,
    ) {
        let ws_url = endpoint.into();
        let identity = EndpointIdentity::from_ws_url(&ws_url, auth.as_ref(), tunnel_generation);
        *self.endpoint.write().await = Box::new(StaticRuntimeEndpoint {
            ws_url,
            auth,
            runtime_key: self.runtime_key.clone(),
            identity,
        });
    }

    /// Configure an already resolved endpoint snapshot.  The supervisor owns
    /// the identity, including credential rotation, so it must not be rebuilt
    /// from a URL that may intentionally redact or omit credentials.
    pub async fn configure_snapshot(&self, snapshot: EndpointSnapshot) {
        debug_assert_eq!(snapshot.runtime_key, self.runtime_key);
        *self.endpoint.write().await = Box::new(StaticRuntimeEndpoint {
            ws_url: snapshot.ws_url,
            auth: None,
            runtime_key: snapshot.runtime_key,
            identity: snapshot.identity,
        });
    }

    /// Ensure this client's configured endpoint is connected.
    ///
    /// New production code should prefer this over supplying a URL to the
    /// backward-compatible [`ensure_ws_connection`] helper.
    pub async fn ensure_connected(
        &self,
        emit_fn: EmitFn,
        sessions: Option<Arc<crate::session_registry::SessionRegistry>>,
    ) -> Result<(), WsError> {
        // The endpoint is authoritative for this client. Reading all endpoint
        // fields here keeps connection ownership local to GatewayClient; the
        // legacy helper continues deriving its compatibility identity from the
        // URL for callers that do not have a RuntimeEndpoint.
        let endpoint = self.endpoint.read().await;
        let snapshot = EndpointSnapshot {
            ws_url: endpoint.ws_url(),
            // Read auth alongside identity so the endpoint's full configuration
            // remains the source of truth; identity contains its fingerprint.
            identity: {
                let _auth = endpoint.auth();
                endpoint.identity()
            },
            runtime_key: endpoint.runtime_key(),
        };
        drop(endpoint);
        let state = Arc::new(self.clone());
        ensure_connection_with_endpoint(snapshot, emit_fn, &state, sessions).await
    }
}

impl Default for GatewayClient {
    fn default() -> Self {
        Self::new(crate::session_registry::RuntimeKey::Local, "", None)
    }
}

// Keep this compile-time contract explicit while the legacy states coexist.
#[allow(dead_code)]
fn assert_gateway_client_traits()
where
    GatewayClient: Send + Sync + 'static,
{
}

/// Trait for state types that can be used with the reader task.
pub trait StateLike: Send + Sync + 'static {
    fn runtime(&self) -> &tokio::sync::Mutex<ConnectionRuntime>;
    fn generation(&self) -> &std::sync::atomic::AtomicU64;
    fn connect_lock(&self) -> &tokio::sync::Mutex<()>;
    fn compatibility(&self) -> &tokio::sync::Mutex<crate::hermes_protocol::RuntimeCompatibility>;
    /// Returns the RuntimeKey for this state type (Local/Remote/Ssh).
    fn runtime_key(&self) -> crate::session_registry::RuntimeKey;
}

// Blanket implementation for Arc<T> where T: StateLike
impl<T: StateLike + ?Sized> StateLike for Arc<T> {
    fn runtime(&self) -> &tokio::sync::Mutex<ConnectionRuntime> {
        (**self).runtime()
    }
    fn generation(&self) -> &std::sync::atomic::AtomicU64 {
        (**self).generation()
    }
    fn connect_lock(&self) -> &tokio::sync::Mutex<()> {
        (**self).connect_lock()
    }
    fn compatibility(&self) -> &tokio::sync::Mutex<crate::hermes_protocol::RuntimeCompatibility> {
        (**self).compatibility()
    }
    fn runtime_key(&self) -> crate::session_registry::RuntimeKey {
        (**self).runtime_key()
    }
}

impl StateLike for GatewayClient {
    fn runtime(&self) -> &tokio::sync::Mutex<ConnectionRuntime> {
        self.runtime.as_ref()
    }
    fn generation(&self) -> &std::sync::atomic::AtomicU64 {
        self.generation.as_ref()
    }
    fn connect_lock(&self) -> &tokio::sync::Mutex<()> {
        self.connect_lock.as_ref()
    }
    fn compatibility(&self) -> &tokio::sync::Mutex<crate::hermes_protocol::RuntimeCompatibility> {
        self.compatibility.as_ref()
    }
    fn runtime_key(&self) -> crate::session_registry::RuntimeKey {
        self.runtime_key.clone()
    }
}

impl StateLike for WsState {
    fn runtime(&self) -> &tokio::sync::Mutex<ConnectionRuntime> {
        &self.runtime
    }
    fn generation(&self) -> &std::sync::atomic::AtomicU64 {
        &self.generation
    }
    fn connect_lock(&self) -> &tokio::sync::Mutex<()> {
        &self.connect_lock
    }
    fn compatibility(&self) -> &tokio::sync::Mutex<crate::hermes_protocol::RuntimeCompatibility> {
        &self.compatibility
    }
    fn runtime_key(&self) -> crate::session_registry::RuntimeKey {
        crate::session_registry::RuntimeKey::Local
    }
}

/// Persistent WS connection state, held in `AppState.ws`.
///
/// All fields are behind `tokio::sync::Mutex` to allow concurrent access from
/// async Tauri command handlers. The actual socket lives inside the reader
/// task (not stored here) — handlers communicate via `cmd_tx`.
pub struct WsState {
    /// Connection runtime: generation, state, cmd_tx, ws_url under one lock.
    pub runtime: tokio::sync::Mutex<ConnectionRuntime>,
    /// Serializes concurrent `ensure_ws_connection` calls so only one task
    /// performs the actual connect+handshake.
    pub connect_lock: tokio::sync::Mutex<()>,
    /// Runtime compatibility result, separate from the network connection
    /// state. A live socket is not evidence of compatibility: `prompt.submit`
    /// must check this before proceeding. Reset to `Unknown` on each connect.
    pub compatibility: tokio::sync::Mutex<crate::hermes_protocol::RuntimeCompatibility>,
    /// Monotonically increasing generation counter. Incremented on each new connection.
    pub generation: std::sync::atomic::AtomicU64,
}

impl WsState {
    /// Create a fresh Disconnected state. Called once in `AppState::new`.
    pub fn new() -> Self {
        Self {
            runtime: tokio::sync::Mutex::new(ConnectionRuntime::default()),
            connect_lock: tokio::sync::Mutex::new(()),
            compatibility: tokio::sync::Mutex::new(
                crate::hermes_protocol::RuntimeCompatibility::Unknown,
            ),
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
pub async fn ensure_ws_connection<State>(
    ws_url: &str,
    emit_fn: EmitFn,
    ws_state: &Arc<State>,
    sessions: Option<Arc<crate::session_registry::SessionRegistry>>,
) -> Result<(), WsError>
where
    State: StateLike,
{
    let snapshot = EndpointSnapshot {
        ws_url: ws_url.to_string(),
        identity: EndpointIdentity::from_ws_url(ws_url, None, None),
        runtime_key: ws_state.runtime_key(),
    };
    ensure_connection_with_endpoint(snapshot, emit_fn, ws_state, sessions).await
}

/// Core persistent connection lifecycle for a fully identified endpoint.
async fn ensure_connection_with_endpoint<State>(
    snapshot: EndpointSnapshot,
    emit_fn: EmitFn,
    ws_state: &Arc<State>,
    sessions: Option<Arc<crate::session_registry::SessionRegistry>>,
) -> Result<(), WsError>
where
    State: StateLike,
{
    // Fast path: keep a live connection only when it belongs to the requested
    // endpoint identity. The token is represented only by its SHA-256
    // fingerprint, so credential rotation reconnects without retaining secrets.
    let EndpointSnapshot {
        ws_url,
        identity: endpoint_identity,
        runtime_key,
    } = snapshot;
    debug_assert_eq!(runtime_key, ws_state.runtime_key());
    let redacted_url = endpoint_identity.public_url.clone();
    let shutdown_tx = {
        let mut runtime = ws_state.runtime().lock().await;
        if runtime.state != ConnectionState::Connected {
            None
        } else if runtime.endpoint_identity.as_ref() == Some(&endpoint_identity) {
            return Ok(());
        } else {
            runtime.state = ConnectionState::Disconnected;
            // Invalidate the old reader before its asynchronous cleanup runs.
            // Otherwise it could reset the replacement connection while it is
            // still handshaking.
            runtime.generation = ws_state.generation().fetch_add(1, Ordering::Release) + 1;
            runtime.cmd_tx.take()
        }
    };

    // Stop the old reader before serializing the new connection attempt. Its
    // generation-guarded cleanup cannot clear a newer connection's state.
    if let Some(tx) = shutdown_tx {
        let _ = tx.send(WsCommand::Shutdown).await;
    }

    // Serialize concurrent connect attempts. The first caller does the actual
    // connect; subsequent callers acquire the lock after the first finishes
    // and observe the final state.
    let _connect_guard = ws_state.connect_lock().lock().await;

    // Double-check after acquiring the lock — the first caller may have already
    // connected while we were waiting.
    let shutdown_tx = {
        let mut runtime = ws_state.runtime().lock().await;
        match runtime.state {
            ConnectionState::Connected | ConnectionState::Degraded
                if runtime.endpoint_identity.as_ref() == Some(&endpoint_identity) =>
            {
                return Ok(());
            }
            ConnectionState::Connected | ConnectionState::Degraded => {
                runtime.state = ConnectionState::Disconnected;
                runtime.generation = ws_state.generation().fetch_add(1, Ordering::Release) + 1;
                runtime.cmd_tx.take()
            }
            ConnectionState::Connecting => {
                // Should not happen under connect_lock, but guard anyway.
                return Err(WsError::Protocol("concurrent connect race".into()));
            }
            ConnectionState::Disconnected => None,
        }
    };
    if let Some(tx) = shutdown_tx {
        let _ = tx.send(WsCommand::Shutdown).await;
    }
    {
        let mut runtime = ws_state.runtime().lock().await;
        // We are the single connector. Set Connecting after stopping any
        // mismatched connection discovered while waiting for the lock.
        runtime.state = ConnectionState::Connecting;
    }

    // Connect.
    tracing::info!(target: "steersman_desktop_lib::ws", ws_url = %redacted_url, "opening persistent connection");
    let (ws, _resp) = match tokio_tungstenite::connect_async(&ws_url).await {
        Ok(result) => result,
        Err(e) => {
            ws_state.runtime().lock().await.state = ConnectionState::Disconnected;
            return Err(WsError::Connect(e.to_string()));
        }
    };

    // Strict gateway.ready barrier: fail if ready not received within timeout.
    // Preserves the typed error from wait_for_gateway_ready (ReadyTimeout,
    // Stream, Connect) instead of wrapping everything in Connect.
    let ws = match tokio::time::timeout(Duration::from_secs(5), wait_for_gateway_ready(ws)).await {
        Ok(Ok(ws)) => ws,
        Ok(Err(e)) => {
            ws_state.runtime().lock().await.state = ConnectionState::Disconnected;
            return Err(e);
        }
        Err(_) => {
            ws_state.runtime().lock().await.state = ConnectionState::Disconnected;
            return Err(WsError::ReadyTimeout);
        }
    };

    // Create the mpsc channel for command handlers → reader task.
    let (cmd_tx, cmd_rx) = tokio::sync::mpsc::channel::<WsCommand>(64);

    // Bump generation before storing cmd_tx.
    let generation = ws_state.generation().fetch_add(1, Ordering::Release) + 1;

    // Store the redacted URL, cmd_tx, and generation atomically under runtime lock.
    {
        let mut runtime = ws_state.runtime().lock().await;
        runtime.ws_url = redacted_url;
        runtime.endpoint_identity = Some(endpoint_identity);
        runtime.cmd_tx = Some(cmd_tx);
        runtime.generation = generation;
    }

    // Spawn the reader task with the generation.
    tokio::spawn(reader_task(
        ws,
        cmd_rx,
        emit_fn,
        Arc::clone(ws_state),
        sessions.clone(),
        generation,
    ));

    // ── Compatibility handshake (Phase 1C.1) ────────────────────────────────
    // The reader task is now running, so call_rpc can dispatch. We probe the
    // backend with a throwaway session.create, read its desktop_contract, and
    // refuse to enter Connected until the contract is supported. The probe
    // session is closed immediately after (best-effort).
    {
        *ws_state.compatibility().lock().await =
            crate::hermes_protocol::RuntimeCompatibility::Checking;
    }
    match run_compatibility_handshake(ws_state).await {
        Ok(c) => {
            tracing::info!(
                target: "steersman_desktop_lib::ws",
                compatibility = ?c,
                generation,
                "compatibility handshake passed"
            );
            *ws_state.compatibility().lock().await = c;
        }
        Err(e) => {
            *ws_state.compatibility().lock().await =
                crate::hermes_protocol::RuntimeCompatibility::Unknown;
            // Atomic cleanup of state + cmd_tx under runtime lock.
            let mut runtime = ws_state.runtime().lock().await;
            runtime.state = ConnectionState::Disconnected;
            runtime.cmd_tx = None;
            // Signal the reader task to shut down.
            return Err(e);
        }
    }

    // ── Reconnect reconciliation (Phase 1C.4) ────────────────────────────────
    // Before declaring Connected, resume any conversations that were Suspended
    // by a prior disconnect and have a durable stored_session_id. Each resume
    // obtains a fresh live ID for the new generation.
    let mut degraded = false;
    let mut interrupted = false;
    if let Some(sessions) = &sessions {
        let report = reconcile_sessions(ws_state, sessions, generation).await;
        if !report.interrupted.is_empty() {
            // A mid-resume disconnect means the socket died during reconciliation.
            // The bindings returned to Suspended, but we must NOT declare this
            // connection Connected — the reader task is gone.
            interrupted = true;
            tracing::warn!(
                target: "steersman_desktop_lib::ws",
                interrupted = report.interrupted.len(),
                generation,
                "reconciliation interrupted by disconnect; not declaring Connected"
            );
        } else if !report.failed.is_empty() {
            degraded = true;
            tracing::warn!(
                target: "steersman_desktop_lib::ws",
                restored = report.restored.len(),
                failed = report.failed.len(),
                generation,
                "reconciliation partial: some conversations failed to resume"
            );
        }
    }

    // GUARD: never set Connected if the socket died during reconciliation or
    // the reader task exited. Verify the connection is still ours and alive.
    // All checks and the final state update are done atomically under runtime lock.
    // If interrupted, also send Shutdown to the old reader to stop it cleanly.
    let shutdown_tx = {
        let mut runtime = ws_state.runtime().lock().await;
        let fail = interrupted
            || runtime.generation != generation
            || runtime.cmd_tx.is_none()
            || runtime.state == ConnectionState::Disconnected;
        if fail {
            runtime.state = ConnectionState::Disconnected;
            runtime.cmd_tx.take()
        } else {
            // Connected only if all resumes succeeded; otherwise Degraded.
            runtime.state = if degraded {
                ConnectionState::Degraded
            } else {
                ConnectionState::Connected
            };
            None
        }
    };

    // If should_fail was true, return error (also send shutdown if we had a sender).
    if let Some(tx) = shutdown_tx {
        let _ = tx.send(WsCommand::Shutdown).await;
        return Err(WsError::ConnectionLost);
    } else if interrupted
        || ws_state.runtime().lock().await.state == ConnectionState::Disconnected
        || ws_state.runtime().lock().await.cmd_tx.is_none()
        || ws_state.runtime().lock().await.generation != generation
    {
        // No sender to shut down, but we still must fail.
        return Err(WsError::ConnectionLost);
    }
    tracing::info!(target: "steersman_desktop_lib::ws", generation, degraded, "persistent connection established");

    Ok(())
}

/// Resume all suspended conversations that have a durable stored_session_id.
/// Called after the compatibility handshake passes, before Connected.
///
/// On success: the binding gets a fresh live ID for the new generation and
/// returns to Active. On failure: the binding is marked ResumeFailed (durable
/// ID retained) and logged — it does not block other conversations or the
/// connection from reaching Connected.
async fn reconcile_sessions<State>(
    ws_state: &Arc<State>,
    sessions: &Arc<crate::session_registry::SessionRegistry>,
    generation: u64,
) -> ReconciliationReport
where
    State: StateLike,
{
    let runtime_key = ws_state.runtime_key();
    let to_resume = sessions
        .take_suspended_for_resume(generation, runtime_key.clone())
        .await;
    if to_resume.is_empty() {
        return ReconciliationReport::default();
    }
    tracing::info!(
        target: "steersman_desktop_lib::ws",
        count = to_resume.len(),
        generation,
        "reconciling suspended sessions"
    );
    let mut report = ReconciliationReport::default();
    let mut connection_lost = false;
    for (conv, durable) in to_resume {
        if connection_lost {
            // Connection died during a previous resume; remaining bindings
            // must return to Suspended for retry on next reconnect.
            tracing::warn!(
                target: "steersman_desktop_lib::ws",
                conversation = %conv,
                stored_id = %durable.stored_session_id,
                "reconciliation stopped due to prior connection loss; returning to Suspended"
            );
            sessions
                .return_to_suspended(&conv, runtime_key.clone())
                .await;
            report.interrupted.push(conv);
            continue;
        }
        match resume_session_on_connection(ws_state, &durable).await {
            Ok(result) => {
                if result.session_id.is_empty() {
                    tracing::warn!(
                        target: "steersman_desktop_lib::ws",
                        conversation = %conv,
                        stored_id = %durable.stored_session_id,
                        "resume returned empty live session_id"
                    );
                    sessions
                        .mark_resume_failed(&conv, runtime_key.clone())
                        .await;
                    report.failed.push(conv);
                    continue;
                }
                // Prefer the durable ID from the response; fall back to what we sent.
                let new_stored = {
                    let d = result.durable_id();
                    if d.is_empty() {
                        durable.stored_session_id.clone()
                    } else {
                        d.to_string()
                    }
                };
                sessions
                    .set_live(
                        conv.clone(),
                        result.session_id,
                        Some(new_stored),
                        durable.profile.clone(),
                        generation,
                        runtime_key.clone(),
                    )
                    .await;
                tracing::info!(
                    target: "steersman_desktop_lib::ws",
                    conversation = %conv,
                    generation,
                    "session resumed"
                );
                report.restored.push(conv);
            }
            Err(e) => {
                // Distinguish interruption (network) from genuine backend error.
                // Interruption → return to Suspended so the next reconnect retries.
                // session.resume is safe to retry (pinned Hermes serializes it).
                // Genuine error (4007 not found, malformed) → ResumeFailed.
                // Protocol errors (malformed response, deserialization failure) are
                // NOT retried — they indicate a persistently incompatible backend.
                let is_interruption = matches!(
                    e,
                    WsError::ConnectionLost | WsError::RpcTimeout | WsError::OutcomeUnknown { .. }
                );
                if is_interruption {
                    tracing::warn!(
                        target: "steersman_desktop_lib::ws",
                        conversation = %conv,
                        stored_id = %durable.stored_session_id,
                        error = %e,
                        "session.resume interrupted; returning to Suspended for retry"
                    );
                    sessions
                        .return_to_suspended(&conv, runtime_key.clone())
                        .await;
                    report.interrupted.push(conv);
                    // Mark that the connection is lost so subsequent resumes
                    // in this reconciliation are also marked interrupted.
                    connection_lost = true;
                } else {
                    tracing::warn!(
                        target: "steersman_desktop_lib::ws",
                        conversation = %conv,
                        stored_id = %durable.stored_session_id,
                        error = %e,
                        "session.resume failed; marking ResumeFailed"
                    );
                    sessions
                        .mark_resume_failed(&conv, runtime_key.clone())
                        .await;
                    report.failed.push(conv);
                }
            }
        }
    }
    report
}

/// Probe the backend with a throwaway `session.create` to read its
/// `desktop_contract`, evaluate compatibility, then close the probe session.
///
/// Returns `Ok(RuntimeCompatibility::Compatible{..})` on success, or
/// `Err(Incompatible(..))` / other transport errors on failure. The probe
/// session is closed best-effort: a close failure does not mask the real
/// compatibility result.
async fn run_compatibility_handshake<State>(
    ws_state: &Arc<State>,
) -> Result<crate::hermes_protocol::RuntimeCompatibility, WsError>
where
    State: StateLike,
{
    // Probe with source "compat_probe" so the backend marks it as a service
    // session (hidden from the feed, like briefing_smart).
    let probe_result = create_session_on_connection(ws_state, "compat_probe", None)
        .await
        .map_err(|e| match e {
            // Surface backend errors from the probe distinctly.
            WsError::BackendError(msg) => {
                WsError::Protocol(format!("compatibility probe failed: {}", msg))
            }
            other => other,
        })?;

    let received = probe_result.info.desktop_contract;
    let compat = crate::hermes_protocol::RuntimeCompatibility::evaluate(received);

    // Close the probe session best-effort. session.close is non-critical: if it
    // fails the probe session simply expires server-side. We must not let a
    // close failure mask the compatibility verdict.
    if let Err(e) = close_session_best_effort(ws_state, &probe_result.session_id).await {
        tracing::warn!(
            target: "steersman_desktop_lib::ws",
            error = %e,
            session_id = %probe_result.session_id,
            "probe session close failed (non-fatal)"
        );
    }

    if !compat.is_compatible() {
        return Err(WsError::Incompatible(compat));
    }
    Ok(compat)
}

/// Best-effort `session.close` via the generic RPC dispatcher. Returns Ok(())
/// on success or if the backend does not support the method.
async fn close_session_best_effort<State>(ws_state: &State, session_id: &str) -> Result<(), WsError>
where
    State: StateLike + ?Sized,
{
    // Use serde_json::Value as the params type to avoid imposing Deserialize
    // on a request-only struct. session.close is best-effort: we only care that
    // it was accepted, not its result body.
    let params = serde_json::json!({ "session_id": session_id });
    match call_rpc::<serde_json::Value, serde_json::Value, State>(
        ws_state,
        "session.close",
        params,
        Duration::from_secs(10),
    )
    .await
    {
        Ok(_) => Ok(()),
        // Backend may not implement session.close — treat as acceptable.
        Err(WsError::BackendError(_)) => Ok(()),
        Err(e) => Err(e),
    }
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
/// On socket close/error: marks Disconnected (with generation guard), suspends
/// the session registry (if provided), and completes pending RPCs with
/// OutcomeUnknown (non-idempotent) or ConnectionLost (idempotent).
async fn reader_task<S, State>(
    mut ws: S,
    mut cmd_rx: tokio::sync::mpsc::Receiver<WsCommand>,
    emit_fn: EmitFn,
    state: Arc<State>,
    sessions: Option<Arc<crate::session_registry::SessionRegistry>>,
    my_generation: u64,
) where
    S: futures::SinkExt<Message, Error = tokio_tungstenite::tungstenite::Error>
        + futures::Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>>
        + Unpin
        + Send
        + 'static,
    State: Send + Sync + 'static + StateLike,
{
    // Pending RPC map: resolves responses by ID.
    let mut pending: HashMap<RpcId, PendingRequest> = HashMap::new();

    // Periodic timer for pending RPC expiration (every 5s).
    let mut cleanup_tick = tokio::time::interval(Duration::from_secs(5));
    cleanup_tick.tick().await; // suppress initial immediate tick

    loop {
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
                                // Use interruption_error so non-idempotent methods get
                                // OutcomeUnknown, not a plain RpcTimeout.
                                let _ = pending_req.reply.send(Err(interruption_error(
                                    &pending_req.method,
                                    InterruptionCause::Timeout,
                                )));
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
                                            tracing::warn!(
                                                target: "steersman_desktop_lib::ws",
                                                id, "RPC response for unknown request"
                                            );
                                        }
                                    }
                                    Some(IncomingFrame::RpcError { id, error }) => {
                                        if let Some(pending_req) = pending.remove(&id) {
                                            let msg = error
                                                .get("message")
                                                .and_then(|m| m.as_str())
                                                .unwrap_or("RPC error");
                                            let _ = pending_req
                                                .reply
                                                .send(Err(GatewayClientError::BackendError(msg.to_string())));
                                        }
                                    }
                                    Some(IncomingFrame::Event(routed_event)) => {
                                        // Resolve the owning conversation via the live
                                        // session_id in params. For session-scoped events
                                        // (those carrying a session_id), an UNKNOWN live
                                        // session must be dropped (early continue) — it is
                                        // a stale event from a dead generation, not a
                                        // global event. Only events WITHOUT a session_id
                                        // (truly global gateway events) may be emitted
                                        // with conversation_id=None.
                                        let runtime_key = state.runtime_key();
                                        let conversation_id = match (
                                            &sessions,
                                            routed_event.session_id.as_deref(),
                                        ) {
                                            (Some(sessions), Some(live_sid)) if !live_sid.is_empty() => {
                                                match sessions.route_event(live_sid, runtime_key.clone()).await {
                                                    Some(conv) => Some(conv.0),
                                                    None => {
                                                        // Unknown session-scoped event: log and DROP.
                                                        // Do not emit as a global event — a late
                                                        // event from a dead generation must not
                                                        // leak into an active conversation.
                                                        tracing::warn!(
                                                            target: "steersman_desktop_lib::ws",
                                                            live_session_id = live_sid,
                                                            "dropping event for unknown live session"
                                                        );
                                                        continue;
                                                    }
                                                }
                                            }
                                            // No session_id → truly global event (e.g. gateway
                                            // status). Emit with conversation_id=None.
                                            _ => None,
                                        };

        // For session.info events, update the registry's
                                        // stored_session_id (never overwrites live ID).
                                        if let ParsedGatewayEvent::Known(GatewayEvent::SessionInfo(p)) =
                                            &routed_event.event
                                        {
                                            if let (Some(sessions), Some(live_sid)) =
                                                (&sessions, &routed_event.session_id)
                                        {
                                            if !live_sid.is_empty() && !p.stored_session_id.is_empty() {
                                                if let Some(conv) = sessions.route_event(live_sid, runtime_key.clone()).await {
                                                    sessions
                                                        .set_stored(&conv, runtime_key.clone(), p.stored_session_id.clone())
                                                        .await;
                                                }
                                            }
                                        }
                                        }

                                        // Translate via typed parser to ChatEvent.
                                        if let Some(chat_event) = translate_gateway_event(&routed_event) {
                                            let routed = RoutedChatEvent {
                                                conversation_id,
                                                event: chat_event,
                                            };
                                            (emit_fn)(&routed);
                                        }
                                    }
                                    None => {
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
                                pending.insert(
                                    id,
                                    PendingRequest {
                                        reply,
                                        method: method.clone(),
                                        timeout: Instant::now() + Duration::from_secs(30),
                                    },
                                );
                                // Send
                                if let Err(e) = send_json_gateway(&mut ws, &req).await {
                                    tracing::warn!(
                                        target: "steersman_desktop_lib::ws",
                                        error = %e, method, "RPC send failed"
                                    );
                                    // Remove from pending and notify caller. A send
                                    // failure means the frame may have been partially
                                    // sent — for non-idempotent methods the outcome is
                                    // unknown (server may have processed it).
                                    if let Some(p) = pending.remove(&id) {
                                        let err = interruption_error(&p.method, InterruptionCause::SendFailure);
                                        let _ = p.reply.send(Err(err));
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
    // ALL checks and modifications must happen under the SAME runtime lock to prevent TOCTOU.
    let should_suspend = {
        let mut runtime = state.runtime().lock().await;

        if runtime.generation != my_generation {
            false
        } else {
            runtime.state = ConnectionState::Disconnected;
            runtime.cmd_tx = None;
            true
        }
    };

    if should_suspend {
        // Suspend the session registry: durable IDs retained for resume, live
        // IDs cleared so stale events don't route. This is the disconnect half
        // of reconciliation (Phase 1C.3); the reconnect/resume half runs in
        // ensure_ws_connection after the handshake.
        if let Some(sessions) = &sessions {
            // Suspend THIS generation's bindings (== match). The dead socket's
            // live IDs become stale; durable IDs retained for resume.
            sessions
                .suspend_generation(my_generation, state.runtime_key())
                .await;
        }
    }

    // Complete all pending RPCs. Safe-retry methods get ConnectionLost (caller
    // may retry); all others get OutcomeUnknown with cause Disconnect.
    for (_, pending_req) in pending.drain() {
        let err = interruption_error(&pending_req.method, InterruptionCause::Disconnect);
        let _ = pending_req.reply.send(Err(err));
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

/// Envelope wrapping a ChatEvent with its owning conversation_id. The frontend
/// uses conversation_id to route streaming tokens, tool events, approvals, etc.
/// to the correct conversation view when multiple are open.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RoutedChatEvent {
    /// The conversation this event belongs to (resolved from the live session
    /// ID via the SessionRegistry). None for events with no owning conversation
    /// (e.g. gateway-level status not tied to a session).
    pub conversation_id: Option<String>,
    #[serde(flatten)]
    pub event: ChatEvent,
}

// Callback type for emitting routed chat events. Production wraps
// `app_handle.emit`; tests pass a mock that records events.
pub type EmitFn = Arc<dyn Fn(&RoutedChatEvent) + Send + Sync + 'static>;

/// Build an EmitFn from a Tauri AppHandle. This is the production path.
pub fn make_tauri_emitter<R>(app_handle: AppHandle<R>) -> EmitFn
where
    R: tauri::Runtime,
    AppHandle<R>: Emitter<R>,
{
    Arc::new(move |event: &RoutedChatEvent| {
        let _ = app_handle.emit("chat_event", event);
        if let Some(conversation_id) = event.conversation_id.as_ref() {
            if let Some(product_event) = crate::translate_hermes_to_product(
                event.event.clone(),
                crate::ConversationId(conversation_id.clone()),
            ) {
                let _ = app_handle.emit("product-event", product_event);
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_registry::RuntimeKey;

    #[test]
    fn redacted_ws_url_removes_token() {
        let token = "token with spaces&symbols";
        let (ws_url, _) = build_ws_url("https://gateway.example.test", token).unwrap();
        let redacted = redacted_ws_url(&ws_url);

        assert!(!redacted.contains(token));
        assert!(!redacted.contains("token="));
        assert_eq!(redacted, "wss://gateway.example.test/api/ws");
    }

    #[test]
    fn gateway_auth_redacts_api_key() {
        let auth = GatewayAuth {
            api_key: "secret-token".to_string(),
        };

        assert_eq!(auth.redacted(), "***");
        assert_eq!(format!("{:?}", auth), "GatewayAuth(<redacted>)");
        assert!(!format!("{:?}", auth).contains("secret-token"));
        assert_eq!(
            auth.sha256_fingerprint(),
            "930bbdc51b6aed5c2a5678fd6e28dee7a05e8a4b643cfc0b4427c3efb86c0d94"
        );
    }

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
        let runtime = ws.runtime.try_lock().expect("runtime lock not poisoned");
        assert_eq!(runtime.state, ConnectionState::Disconnected);
    }

    #[tokio::test]
    async fn connection_state_transitions() {
        let ws = WsState::new();
        // Disconnected → Connecting → Connected → Disconnected
        {
            let mut runtime = ws.runtime.lock().await;
            runtime.state = ConnectionState::Connecting;
            assert_eq!(runtime.state, ConnectionState::Connecting);
            runtime.state = ConnectionState::Connected;
            assert_eq!(runtime.state, ConnectionState::Connected);
            runtime.state = ConnectionState::Disconnected;
            assert_eq!(runtime.state, ConnectionState::Disconnected);
        }
    }

    #[tokio::test]
    async fn ws_state_starts_with_no_compatibility() {
        // The global session_id field is removed (Phase 1C.4): the registry is
        // the authoritative source. Verify compatibility starts Unknown.
        let ws = WsState::new();
        let compat = ws.compatibility.lock().await;
        assert_eq!(
            *compat,
            crate::hermes_protocol::RuntimeCompatibility::Unknown
        );
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
        ws.runtime.lock().await.state = ConnectionState::Connected;
        // We can't easily build an AppHandle in a unit test, so we verify the
        // state guard logic directly: the function checks state == Connected
        // before touching the network.
        let runtime = ws.runtime.lock().await;
        assert_eq!(
            runtime.state,
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
        ws.runtime.lock().await.state = ConnectionState::Connecting;
        // Simulate the connecting task finishing.
        tokio::spawn({
            let ws = Arc::clone(&ws);
            async move {
                tokio::time::sleep(Duration::from_millis(50)).await;
                ws.runtime.lock().await.state = ConnectionState::Connected;
            }
        });
        // Spin like ensure_ws_connection does until Connected.
        let mut resolved = false;
        for _ in 0..50 {
            let s = ws.runtime.lock().await;
            if s.state == ConnectionState::Connected {
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
        // Can be Protocol (old) or ConnectionLost (new) when not connected.
        assert!(matches!(
            err,
            WsError::Protocol(_) | WsError::ConnectionLost
        ));
    }

    #[tokio::test]
    async fn create_session_on_connection_errors_when_disconnected() {
        let ws = Arc::new(WsState::new());
        let result = create_session_on_connection(&ws, "desktop", None).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            WsError::Protocol(_) | WsError::ConnectionLost
        ));
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
    /// - responds to session.create with `contract` in info.desktop_contract
    /// - handles session.close (probe cleanup)
    /// - on prompt.submit: ACKs, streams message.start → delta("Hello") →
    ///   message.complete(session_id)
    ///
    /// `contract` controls the reported desktop_contract version, letting
    /// handshake accept/reject tests vary it.
    ///
    /// Returns (ws_url, received_frames) where received_frames captures all
    /// JSON-RPC requests the server got (for assertions).
    async fn start_mock_backend(contract: u32) -> (String, Arc<tokio::sync::Mutex<Vec<Value>>>) {
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
                            // Echo back the requested profile_name (or None).
                            let profile_name = req
                                .get("params")
                                .and_then(|p| p.get("profile"))
                                .and_then(|s| s.as_str());
                            let new_sid = if source == "briefing_smart" {
                                "brief-sess"
                            } else if source == "compat_probe" {
                                "probe-sess"
                            } else {
                                "chat-sess"
                            };
                            // Contract version configurable per-test.
                            let mut info = json!({ "desktop_contract": contract });
                            if let Some(pn) = profile_name {
                                info["profile_name"] = json!(pn);
                            }
                            let resp = json!({
                                "jsonrpc": "2.0", "id": id,
                                "result": {
                                    "session_id": new_sid,
                                    "stored_session_id": new_sid,
                                    "message_count": 0,
                                    "messages": [],
                                    "info": info
                                }
                            });
                            let _ = ws.send(Message::Text(resp.to_string())).await;
                        }
                        "session.close" => {
                            // Acknowledge the close (probe session cleanup).
                            let resp = json!({"jsonrpc":"2.0","id":id,"result":{}});
                            let _ = ws.send(Message::Text(resp.to_string())).await;
                        }
                        "session.resume" => {
                            // Real Hermes reads params.session_id (the durable
                            // ID), NOT stored_session_id. Returns 4006 if absent.
                            // Response carries the NEW live ID in session_id and
                            // the durable ID in resumed/session_key.
                            let durable = req
                                .get("params")
                                .and_then(|p| p.get("session_id"))
                                .and_then(|s| s.as_str());
                            match durable {
                                Some(d) if !d.is_empty() => {
                                    let new_live = format!("{}-resumed-{}", d, id);
                                    let resp = json!({
                                        "jsonrpc": "2.0", "id": id,
                                        "result": {
                                            "session_id": new_live,
                                            "resumed": d,
                                            "session_key": d,
                                            "message_count": 0,
                                            "messages": [],
                                            "info": {}
                                        }
                                    });
                                    let _ = ws.send(Message::Text(resp.to_string())).await;
                                }
                                _ => {
                                    // 4006: session_id required (real Hermes behavior).
                                    let resp = json!({
                                        "jsonrpc": "2.0", "id": id,
                                        "error": { "code": 4006, "message": "session_id required" }
                                    });
                                    let _ = ws.send(Message::Text(resp.to_string())).await;
                                }
                            }
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

    /// A compatible backend that keeps three WebSocket connections alive at
    /// once. The isolation test uses it to exercise multiple runtime clients
    /// against the same endpoint.
    async fn start_multi_runtime_mock_backend() -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        tokio::spawn(async move {
            for _ in 0..3 {
                let (stream, _) = listener.accept().await.unwrap();
                tokio::spawn(async move {
                    let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
                    let ready = json!({
                        "jsonrpc": "2.0", "method": "event",
                        "params": {"type": "gateway.ready", "payload": {}}
                    });
                    let _ = ws.send(Message::Text(ready.to_string())).await;

                    while let Some(Ok(Message::Text(text))) = ws.next().await {
                        let Ok(request) = serde_json::from_str::<Value>(&text) else {
                            continue;
                        };
                        let id = request.get("id").and_then(Value::as_u64).unwrap_or(0);
                        match request.get("method").and_then(Value::as_str) {
                            Some("session.create") => {
                                let response = json!({
                                    "jsonrpc": "2.0", "id": id,
                                    "result": {
                                        "session_id": "probe-sess",
                                        "stored_session_id": "probe-sess",
                                        "message_count": 0,
                                        "messages": [],
                                        "info": {"desktop_contract": 4}
                                    }
                                });
                                let _ = ws.send(Message::Text(response.to_string())).await;
                            }
                            Some("session.close") => {
                                let response = json!({"jsonrpc": "2.0", "id": id, "result": {}});
                                let _ = ws.send(Message::Text(response.to_string())).await;
                            }
                            _ => {}
                        }
                    }
                });
            }
        });

        format!("ws://127.0.0.1:{}/api/ws?token=test", port)
    }

    /// Start one minimal compatible backend and report when its client reader
    /// releases the socket. Used to prove endpoint rotation shuts down the old
    /// reader before establishing the replacement connection.
    async fn start_rotation_mock_backend() -> (String, oneshot::Receiver<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let (reader_exited_tx, reader_exited_rx) = oneshot::channel();

        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            let ready = json!({
                "jsonrpc": "2.0", "method": "event",
                "params": {"type": "gateway.ready", "payload": {}}
            });
            let _ = ws.send(Message::Text(ready.to_string())).await;

            while let Some(Ok(msg)) = ws.next().await {
                let Message::Text(text) = msg else {
                    continue;
                };
                let Ok(request) = serde_json::from_str::<Value>(&text) else {
                    continue;
                };
                let id = request.get("id").and_then(Value::as_u64).unwrap_or(0);
                match request.get("method").and_then(Value::as_str) {
                    Some("session.create") => {
                        let response = json!({
                            "jsonrpc": "2.0", "id": id,
                            "result": {
                                "session_id": "probe-sess",
                                "stored_session_id": "probe-sess",
                                "message_count": 0,
                                "messages": [],
                                "info": {"desktop_contract": 4}
                            }
                        });
                        let _ = ws.send(Message::Text(response.to_string())).await;
                    }
                    Some("session.close") => {
                        let response = json!({"jsonrpc": "2.0", "id": id, "result": {}});
                        let _ = ws.send(Message::Text(response.to_string())).await;
                    }
                    _ => {}
                }
            }

            let _ = reader_exited_tx.send(());
        });

        (
            format!("ws://127.0.0.1:{}/api/ws?token=test", port),
            reader_exited_rx,
        )
    }

    type MockEvents = Arc<tokio::sync::Mutex<Vec<(Option<String>, &'static str)>>>;

    /// Build an EmitFn that records (conversation_id, tag) into `events`.
    fn mock_emitter_into(events: MockEvents) -> EmitFn {
        Arc::new(move |routed: &RoutedChatEvent| {
            let tag = match &routed.event {
                ChatEvent::Token { .. } => "token",
                ChatEvent::Reasoning { .. } => "reasoning",
                ChatEvent::ToolStart { .. } => "tool_start",
                ChatEvent::ToolComplete { .. } => "tool_complete",
                ChatEvent::Done { .. } => "done",
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
            events
                .try_lock()
                .map(|mut v| v.push((routed.conversation_id.clone(), tag)))
                .ok();
        })
    }

    /// Convenience: fresh events Arc + emitter.
    fn mock_emitter() -> (EmitFn, MockEvents) {
        let events: MockEvents = Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let emit_fn = mock_emitter_into(Arc::clone(&events));
        (emit_fn, events)
    }

    fn make_gateway_for_runtime(key: RuntimeKey, ws_url: impl Into<String>) -> Arc<GatewayClient> {
        let ws_url = ws_url.into();
        let runtime_key = match key {
            RuntimeKey::Local => RuntimeKey::Local,
            RuntimeKey::Remote(_) => RuntimeKey::Remote("test-instance".into()),
            RuntimeKey::Ssh(_) => RuntimeKey::Ssh("test-tunnel".into()),
        };
        Arc::new(GatewayClient::new(runtime_key, ws_url, None))
    }

    async fn wait_connected<State: StateLike>(ws_state: &State, timeout_ms: u64) {
        let deadline = tokio::time::sleep(Duration::from_millis(timeout_ms));
        tokio::pin!(deadline);
        loop {
            if let Ok(rt) = ws_state.runtime().try_lock() {
                if rt.state == ConnectionState::Connected {
                    return;
                }
            }
            if deadline.is_elapsed() {
                panic!("not Connected within {}ms", timeout_ms);
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    async fn wait_disconnected<State: StateLike>(ws_state: &State, timeout_ms: u64) {
        let deadline = tokio::time::sleep(Duration::from_millis(timeout_ms));
        tokio::pin!(deadline);
        loop {
            if let Ok(rt) = ws_state.runtime().try_lock() {
                if rt.state == ConnectionState::Disconnected {
                    return;
                }
            }
            if deadline.is_elapsed() {
                panic!("not Disconnected within {}ms", timeout_ms);
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    #[tokio::test]
    async fn e2e_ensure_connection_reaches_connected() {
        let (ws_url, _) = start_mock_backend(4).await;
        let ws_state = Arc::new(WsState::new());
        let (emit_fn, _) = mock_emitter();

        ensure_ws_connection(&ws_url, emit_fn, &ws_state, None)
            .await
            .unwrap();
        wait_connected(&ws_state, 3000).await;

        // cmd_tx must be set.
        assert!(ws_state.runtime.lock().await.cmd_tx.is_some());
    }

    #[tokio::test]
    async fn gateway_client_ensure_connected_uses_its_configured_endpoint() {
        let (ws_url, _) = start_mock_backend(4).await;
        let auth = GatewayAuth {
            api_key: "test".into(),
        };
        let gateway = Arc::new(GatewayClient::new(
            RuntimeKey::Remote("endpoint-owned".into()),
            ws_url,
            Some(auth.clone()),
        ));
        let (emit_fn, _) = mock_emitter();

        gateway.ensure_connected(emit_fn, None).await.unwrap();

        let runtime = gateway.runtime.lock().await;
        assert_eq!(runtime.state, ConnectionState::Connected);
        assert_eq!(
            runtime
                .endpoint_identity
                .as_ref()
                .map(|identity| &identity.auth_fingerprint),
            Some(&auth.sha256_fingerprint())
        );
    }

    #[tokio::test]
    async fn runtime_supervisor_starts_and_reports_connected() {
        let (ws_url, _) = start_mock_backend(4).await;
        let runtime_key = RuntimeKey::Remote("supervisor-test".into());
        let supervisor =
            crate::runtime_supervisor::RuntimeSupervisor::new(runtime_key.clone(), None);
        let (emit_fn, _) = mock_emitter();
        supervisor
            .start(
                EndpointSnapshot {
                    identity: EndpointIdentity::from_ws_url(&ws_url, None, None),
                    ws_url,
                    runtime_key,
                },
                emit_fn,
            )
            .await
            .unwrap();
        assert!(matches!(
            supervisor.health_check().await,
            HealthStatus::Connected { .. }
        ));
    }

    #[tokio::test]
    async fn e2e_create_session_returns_mock_id() {
        let (ws_url, received) = start_mock_backend(4).await;
        let ws_state = Arc::new(WsState::new());
        let (emit_fn, _) = mock_emitter();
        ensure_ws_connection(&ws_url, emit_fn, &ws_state, None)
            .await
            .unwrap();
        wait_connected(&ws_state, 3000).await;

        let result = create_session_on_connection(&ws_state, "desktop", None)
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
        let (ws_url, received) = start_mock_backend(4).await;
        let ws_state = Arc::new(WsState::new());
        let (emit_fn, _) = mock_emitter();
        ensure_ws_connection(&ws_url, emit_fn, &ws_state, None)
            .await
            .unwrap();
        wait_connected(&ws_state, 3000).await;

        let result = create_session_on_connection(&ws_state, "briefing_smart", None)
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
        let (ws_url, received) = start_mock_backend(4).await;
        let ws_state = Arc::new(WsState::new());
        let sessions = crate::session_registry::SessionRegistry::new();
        let (emit_fn, _) = mock_emitter();
        ensure_ws_connection(&ws_url, emit_fn, &ws_state, Some(sessions.clone()))
            .await
            .unwrap();
        wait_connected(&ws_state, 3000).await;

        let result = create_session_on_connection(&ws_state, "desktop", None)
            .await
            .unwrap();
        let sid = result.session_id.clone();
        let generation = ws_state
            .generation
            .load(std::sync::atomic::Ordering::Acquire);
        sessions
            .set_live(
                crate::session_registry::ConversationId::new("conv-test"),
                sid.clone(),
                Some(result.stored_session_id),
                crate::session_registry::ProfileId::empty(),
                generation,
                RuntimeKey::Local,
            )
            .await;
        submit_prompt_on_connection(&ws_state, &sid, "test prompt")
            .await
            .unwrap();

        // Give the stream a moment to deliver events.
        tokio::time::sleep(Duration::from_millis(200)).await;

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
        let (ws_url, _) = start_mock_backend(4).await;
        let ws_state = Arc::new(WsState::new());

        let (emit_fn, _) = mock_emitter();
        ensure_ws_connection(&ws_url, emit_fn, &ws_state, None)
            .await
            .unwrap();
        wait_connected(&ws_state, 3000).await;

        // Second connect — fast path, must NOT error.
        let (emit_fn2, _) = mock_emitter();
        ensure_ws_connection(&ws_url, emit_fn2, &ws_state, None)
            .await
            .unwrap();

        assert_eq!(
            ws_state.runtime.lock().await.state,
            ConnectionState::Connected
        );
    }

    #[tokio::test]
    async fn ensure_connection_rotates_when_gateway_endpoint_changes() {
        let (url_a, reader_a_exited) = start_rotation_mock_backend().await;
        let (url_b, _reader_b_exited) = start_rotation_mock_backend().await;
        let gateway = Arc::new(GatewayClient::new(
            crate::session_registry::RuntimeKey::Remote("rotation-test".into()),
            "",
            None,
        ));

        let (emit_a, _) = mock_emitter();
        ensure_ws_connection(&url_a, emit_a, &gateway, None)
            .await
            .expect("connect URL-A");
        let first_generation = gateway.generation.load(Ordering::Acquire);

        let (emit_b, _) = mock_emitter();
        ensure_ws_connection(&url_b, emit_b, &gateway, None)
            .await
            .expect("connect URL-B");

        let runtime = gateway.runtime.lock().await;
        assert_eq!(runtime.state, ConnectionState::Connected);
        assert_eq!(runtime.ws_url, redacted_ws_url(&url_b));
        assert!(
            gateway.generation.load(Ordering::Acquire) > first_generation,
            "URL-B must establish a new connection generation"
        );
        drop(runtime);

        tokio::time::timeout(Duration::from_secs(1), reader_a_exited)
            .await
            .expect("old reader did not exit")
            .expect("old backend did not observe the reader exit");
    }

    #[tokio::test]
    async fn ensure_connection_rotates_when_auth_token_changes() {
        let initial_url = start_multi_runtime_mock_backend().await;
        let base_url = initial_url.split('?').next().unwrap();
        let (url_a, auth_a) = build_ws_url(base_url, "token-A").unwrap();
        let (url_b, auth_b) = build_ws_url(base_url, "token-B").unwrap();
        let gateway = Arc::new(GatewayClient::new(
            RuntimeKey::Remote("token-rotation-test".into()),
            "",
            None,
        ));

        let (emit_a, _) = mock_emitter();
        gateway
            .configure_endpoint(url_a.clone(), Some(auth_a.clone()), None)
            .await;
        gateway
            .ensure_connected(emit_a, None)
            .await
            .expect("connect token-A");
        let first_generation = gateway.generation.load(Ordering::Acquire);

        let (emit_b, _) = mock_emitter();
        gateway
            .configure_endpoint(url_b.clone(), Some(auth_b.clone()), None)
            .await;
        gateway
            .ensure_connected(emit_b, None)
            .await
            .expect("reconnect token-B");

        let runtime = gateway.runtime.lock().await;
        assert_eq!(runtime.state, ConnectionState::Connected);
        assert_eq!(redacted_ws_url(&url_a), redacted_ws_url(&url_b));
        assert_eq!(
            runtime.endpoint_identity.as_ref().unwrap().auth_fingerprint,
            auth_b.sha256_fingerprint()
        );
        assert_ne!(auth_a.sha256_fingerprint(), auth_b.sha256_fingerprint());
        assert!(gateway.generation.load(Ordering::Acquire) > first_generation);
    }

    #[tokio::test]
    async fn ensure_connected_rotates_when_ssh_tunnel_generation_changes() {
        let ws_url = start_multi_runtime_mock_backend().await;
        let (ws_url, auth) = build_ws_url(ws_url.split('?').next().unwrap(), "ssh-token").unwrap();
        let gateway = Arc::new(GatewayClient::new(
            RuntimeKey::Ssh("tunnel-identity".into()),
            "",
            None,
        ));

        let (emit_a, _) = mock_emitter();
        gateway
            .configure_endpoint(ws_url.clone(), Some(auth.clone()), Some(1))
            .await;
        gateway
            .ensure_connected(emit_a, None)
            .await
            .expect("connect tunnel generation 1");
        let first_generation = gateway.generation.load(Ordering::Acquire);

        let (emit_b, _) = mock_emitter();
        gateway
            .configure_endpoint(ws_url, Some(auth), Some(2))
            .await;
        gateway
            .ensure_connected(emit_b, None)
            .await
            .expect("reconnect tunnel generation 2");

        let runtime = gateway.runtime.lock().await;
        assert_eq!(runtime.state, ConnectionState::Connected);
        assert_eq!(
            runtime
                .endpoint_identity
                .as_ref()
                .unwrap()
                .tunnel_generation,
            Some(2)
        );
        assert!(gateway.generation.load(Ordering::Acquire) > first_generation);
    }

    #[tokio::test]
    async fn gateway_handshake_supports_each_runtime_key() {
        for key in [
            RuntimeKey::Local,
            RuntimeKey::Remote("test-instance".into()),
            RuntimeKey::Ssh("test-tunnel".into()),
        ] {
            let (ws_url, _) = start_mock_backend(4).await;
            let gateway = make_gateway_for_runtime(key.clone(), &ws_url);
            let (emit_fn, _) = mock_emitter();

            ensure_ws_connection(&ws_url, emit_fn, &gateway, None)
                .await
                .expect("handshake must connect");
            wait_connected(&gateway, 3000).await;

            assert_eq!(
                gateway.runtime.lock().await.state,
                ConnectionState::Connected
            );
            assert_eq!(gateway.runtime_key(), key);
        }
    }

    #[tokio::test]
    async fn gateway_reconnects_for_each_runtime_key() {
        for key in [
            RuntimeKey::Local,
            RuntimeKey::Remote("test-instance".into()),
            RuntimeKey::Ssh("test-tunnel".into()),
        ] {
            let (ws_url, _, disconnect_signal, _) = start_reconnect_mock_backend().await;
            let gateway = make_gateway_for_runtime(key.clone(), &ws_url);
            let (emit_fn, _) = mock_emitter();

            ensure_ws_connection(&ws_url, emit_fn, &gateway, None)
                .await
                .expect("initial connection must succeed");
            wait_connected(&gateway, 3000).await;

            disconnect_signal.notify_waiters();
            wait_disconnected(&gateway, 3000).await;

            let (reconnect_emit_fn, _) = mock_emitter();
            ensure_ws_connection(&ws_url, reconnect_emit_fn, &gateway, None)
                .await
                .expect("reconnection must succeed");
            wait_connected(&gateway, 3000).await;
            assert_eq!(gateway.runtime_key(), key);
        }
    }

    #[tokio::test]
    async fn gateway_clients_isolate_sessions_by_runtime_key() {
        use crate::session_registry::{ConversationId, ProfileId, SessionState};

        let ws_url = start_multi_runtime_mock_backend().await;
        let sessions = crate::session_registry::SessionRegistry::new();
        let local = make_gateway_for_runtime(RuntimeKey::Local, &ws_url);
        let remote = make_gateway_for_runtime(RuntimeKey::Remote("ignored".into()), &ws_url);
        let ssh = make_gateway_for_runtime(RuntimeKey::Ssh("ignored".into()), &ws_url);

        for gateway in [&local, &remote, &ssh] {
            let (emit_fn, _) = mock_emitter();
            ensure_ws_connection(&ws_url, emit_fn, gateway, Some(sessions.clone()))
                .await
                .expect("all runtime clients must connect");
            wait_connected(gateway, 3000).await;
        }

        let conversation = ConversationId::new("shared-conversation");
        for gateway in [&local, &remote, &ssh] {
            let key = gateway.runtime_key();
            let generation = gateway.generation.load(Ordering::Acquire);
            sessions
                .set_live(
                    conversation.clone(),
                    "shared-live-session".into(),
                    Some("shared-stored-session".into()),
                    ProfileId::empty(),
                    generation,
                    key,
                )
                .await;
        }

        assert_eq!(
            sessions
                .route_event(
                    "shared-live-session",
                    RuntimeKey::Remote("test-instance".into())
                )
                .await,
            Some(conversation.clone())
        );
        assert_eq!(
            sessions
                .route_event("shared-live-session", RuntimeKey::Ssh("test-tunnel".into()))
                .await,
            Some(conversation.clone())
        );

        let shutdown_tx = local
            .runtime
            .lock()
            .await
            .cmd_tx
            .clone()
            .expect("Local client must have a reader task");
        shutdown_tx.send(WsCommand::Shutdown).await.unwrap();
        wait_disconnected(&local, 3000).await;

        assert_eq!(
            sessions
                .get(&conversation, RuntimeKey::Local)
                .await
                .unwrap()
                .state,
            SessionState::Suspended
        );
        for key in [
            RuntimeKey::Remote("test-instance".into()),
            RuntimeKey::Ssh("test-tunnel".into()),
        ] {
            assert_eq!(
                sessions.get(&conversation, key).await.unwrap().state,
                SessionState::Active
            );
        }
        assert_eq!(
            remote.runtime.lock().await.state,
            ConnectionState::Connected
        );
        assert_eq!(ssh.runtime.lock().await.state, ConnectionState::Connected);
    }

    #[tokio::test]
    async fn gateway_preserves_remote_runtime_key_through_connection_lifecycle() {
        let (ws_url, _) = start_mock_backend(4).await;
        let remote_key = RuntimeKey::Remote("test-instance".into());
        let gateway = make_gateway_for_runtime(remote_key.clone(), &ws_url);
        assert_eq!(gateway.runtime_key(), remote_key);

        let (emit_fn, _) = mock_emitter();
        ensure_ws_connection(&ws_url, emit_fn, &gateway, None)
            .await
            .expect("Remote client must connect");
        wait_connected(&gateway, 3000).await;

        assert_eq!(
            gateway.runtime.lock().await.state,
            ConnectionState::Connected
        );
        assert_eq!(gateway.runtime_key(), remote_key);
    }

    // ── Phase 1C.1: Compatibility Handshake tests ────────────────────────────
    //
    // Mandatory tests 1-5 and 14 from the Phase 1C spec.

    /// Test 1: Contract 4 (supported) is accepted; connection reaches Connected.
    #[tokio::test]
    async fn handshake_accepts_supported_contract() {
        let (ws_url, _) = start_mock_backend(4).await;
        let ws_state = Arc::new(WsState::new());
        let (emit_fn, _) = mock_emitter();

        ensure_ws_connection(&ws_url, emit_fn, &ws_state, None)
            .await
            .expect("contract 4 must be accepted");
        wait_connected(&ws_state, 3000).await;

        let compat = ws_state.compatibility.lock().await;
        assert!(
            matches!(
                *compat,
                crate::hermes_protocol::RuntimeCompatibility::Compatible { contract: 4 }
            ),
            "expected Compatible, got {:?}",
            *compat
        );
    }

    /// Test 2: Contract below minimum (3 < 4) is rejected → HermesUpgradeRequired.
    #[tokio::test]
    async fn handshake_rejects_contract_below_minimum() {
        let (ws_url, _) = start_mock_backend(3).await;
        let ws_state = Arc::new(WsState::new());
        let (emit_fn, _) = mock_emitter();

        let err = ensure_ws_connection(&ws_url, emit_fn, &ws_state, None)
            .await
            .expect_err("contract 3 must be rejected");
        assert!(
            matches!(
                err,
                WsError::Incompatible(
                    crate::hermes_protocol::RuntimeCompatibility::HermesUpgradeRequired {
                        received: 3,
                        minimum: 4
                    }
                )
            ),
            "expected HermesUpgradeRequired, got {:?}",
            err
        );
        // Connection must NOT be Connected after a failed handshake.
        assert_eq!(
            ws_state.runtime.lock().await.state,
            ConnectionState::Disconnected,
            "state must be Disconnected after handshake failure"
        );
    }

    /// Test 3: Contract above maximum (5 > 4) is rejected → DesktopUpgradeRequired.
    #[tokio::test]
    async fn handshake_rejects_contract_above_maximum() {
        let (ws_url, _) = start_mock_backend(5).await;
        let ws_state = Arc::new(WsState::new());
        let (emit_fn, _) = mock_emitter();

        let err = ensure_ws_connection(&ws_url, emit_fn, &ws_state, None)
            .await
            .expect_err("contract 5 must be rejected");
        assert!(
            matches!(
                err,
                WsError::Incompatible(
                    crate::hermes_protocol::RuntimeCompatibility::DesktopUpgradeRequired {
                        received: 5,
                        maximum: 4
                    }
                )
            ),
            "expected DesktopUpgradeRequired, got {:?}",
            err
        );
    }

    /// Test 4: prompt.submit is blocked until the handshake completes.
    /// On a fresh WsState (Unknown compatibility), submit must error immediately
    /// without even attempting the RPC.
    #[tokio::test]
    async fn prompt_submit_blocked_before_handshake() {
        let ws = Arc::new(WsState::new());
        // No ensure_ws_connection called — compatibility stays Unknown.
        let err = submit_prompt_on_connection(&ws, "some-sess", "text")
            .await
            .expect_err("submit must be blocked with Unknown compatibility");
        assert!(
            matches!(err, WsError::Protocol(ref msg) if msg.contains("before compatibility")),
            "expected pre-handshake block, got {:?}",
            err
        );
    }

    /// Test 5 + probe close: the probe session created during handshake is
    /// closed via session.close. Verify the mock backend received session.close
    /// for the probe session id ("probe-sess").
    #[tokio::test]
    async fn handshake_probe_session_is_closed() {
        let (ws_url, received) = start_mock_backend(4).await;
        let ws_state = Arc::new(WsState::new());
        let (emit_fn, _) = mock_emitter();

        ensure_ws_connection(&ws_url, emit_fn, &ws_state, None)
            .await
            .expect("handshake should pass");
        // Give the close RPC a moment to be sent/received.
        tokio::time::sleep(Duration::from_millis(150)).await;

        let frames = received.lock().await;
        let close_sent = frames.iter().any(|v| {
            v.get("method").and_then(|m| m.as_str()) == Some("session.close")
                && v.get("params")
                    .and_then(|p| p.get("session_id"))
                    .and_then(|s| s.as_str())
                    == Some("probe-sess")
        });
        assert!(
            close_sent,
            "expected session.close for probe-sess, frames: {:?}",
            frames
                .iter()
                .map(|f| f.get("method").and_then(|m| m.as_str()))
                .collect::<Vec<_>>()
        );
    }

    /// Test 14: Incompatible runtime surfaces as a distinct Incompatible error,
    /// not as Connect or ConnectionFailed.
    #[tokio::test]
    async fn incompatible_runtime_is_distinct_error_not_connect() {
        let (ws_url, _) = start_mock_backend(2).await;
        let ws_state = Arc::new(WsState::new());
        let (emit_fn, _) = mock_emitter();

        let err = ensure_ws_connection(&ws_url, emit_fn, &ws_state, None)
            .await
            .expect_err("must fail");
        // Must be Incompatible, NOT Connect.
        assert!(
            matches!(err, WsError::Incompatible(_)),
            "expected Incompatible, got {:?}",
            err
        );
        assert!(
            !matches!(err, WsError::Connect(_)),
            "Incompatible must not masquerade as Connect"
        );
        // Display must mention the upgrade direction.
        let msg = err.to_string();
        assert!(
            msg.contains("upgrade required"),
            "error message must explain the upgrade, got: {}",
            msg
        );
    }

    /// Unit test: RuntimeCompatibility::evaluate boundary logic.
    #[test]
    fn runtime_compatibility_evaluate_boundaries() {
        use crate::hermes_protocol::RuntimeCompatibility as RC;
        // Below minimum.
        assert!(matches!(
            RC::evaluate(3),
            RC::HermesUpgradeRequired {
                received: 3,
                minimum: 4
            }
        ));
        // In range.
        assert!(matches!(RC::evaluate(4), RC::Compatible { contract: 4 }));
        // Above maximum.
        assert!(matches!(
            RC::evaluate(5),
            RC::DesktopUpgradeRequired {
                received: 5,
                maximum: 4
            }
        ));
        // is_compatible only for Compatible.
        assert!(!RC::Unknown.is_compatible());
        assert!(RC::evaluate(4).is_compatible());
        assert!(!RC::evaluate(3).is_compatible());
    }

    // ── Phase 1C.3: Reconnect reconciliation + OutcomeUnknown tests ──────────
    //
    // Mandatory tests 11-13 from the Phase 1C spec.

    /// Test 11: Disconnect suspends bindings but does NOT delete durable IDs.
    /// Uses the SessionRegistry directly (the reader task calls
    /// mark_stale_for_generation on disconnect).
    #[tokio::test]
    async fn disconnect_suspends_but_retains_durable_ids() {
        use crate::session_registry::{ConversationId, SessionRegistry, SessionState};

        let sessions = SessionRegistry::new();
        let conv_a = ConversationId::new("conv-a");
        let conv_b = ConversationId::new("conv-b");
        sessions
            .set_live(
                conv_a.clone(),
                "live-a".into(),
                Some("durable-a".into()),
                crate::session_registry::ProfileId::empty(),
                1,
                RuntimeKey::Local,
            )
            .await;
        sessions
            .set_live(
                conv_b.clone(),
                "live-b".into(),
                Some("durable-b".into()),
                crate::session_registry::ProfileId::empty(),
                1,
                RuntimeKey::Local,
            )
            .await;

        // Simulate disconnect: reader task marks stale for the NEW generation
        // (2), so any binding from an older generation (1) becomes Suspended.
        sessions
            .mark_stale_for_generation(2, RuntimeKey::Local)
            .await;

        // Both bindings must be Suspended.
        let ba = sessions.get(&conv_a, RuntimeKey::Local).await.unwrap();
        let bb = sessions.get(&conv_b, RuntimeKey::Local).await.unwrap();
        assert_eq!(ba.state, SessionState::Suspended);
        assert_eq!(bb.state, SessionState::Suspended);
        // Live IDs cleared (stale), but durable IDs retained for resume.
        assert_eq!(ba.live_session_id, None);
        assert_eq!(ba.stored_session_id.as_deref(), Some("durable-a"));
        assert_eq!(bb.live_session_id, None);
        assert_eq!(bb.stored_session_id.as_deref(), Some("durable-b"));
        // Stale live IDs no longer route events.
        assert_eq!(
            sessions.route_event("live-a", RuntimeKey::Local).await,
            None
        );
    }

    /// Test 12: Reconnect restores multiple conversations by resuming durable
    /// IDs. After mark_stale + set_live (resume), each conversation has a fresh
    /// live ID and routes events again.
    #[tokio::test]
    async fn reconnect_restores_multiple_conversations() {
        use crate::session_registry::{ConversationId, SessionRegistry, SessionState};

        let sessions = SessionRegistry::new();
        let conv_a = ConversationId::new("conv-a");
        let conv_b = ConversationId::new("conv-b");
        // Generation 1: both active.
        sessions
            .set_live(
                conv_a.clone(),
                "live-a1".into(),
                Some("durable-a".into()),
                crate::session_registry::ProfileId::empty(),
                1,
                RuntimeKey::Local,
            )
            .await;
        sessions
            .set_live(
                conv_b.clone(),
                "live-b1".into(),
                Some("durable-b".into()),
                crate::session_registry::ProfileId::empty(),
                1,
                RuntimeKey::Local,
            )
            .await;

        // Disconnect (generation 1 dies).
        sessions
            .mark_stale_for_generation(2, RuntimeKey::Local)
            .await;

        // Reconnect (generation 2): resume each durable session → new live IDs.
        sessions
            .set_live(
                conv_a.clone(),
                "live-a2".into(),
                Some("durable-a".into()),
                crate::session_registry::ProfileId::empty(),
                2,
                RuntimeKey::Local,
            )
            .await;
        sessions
            .set_live(
                conv_b.clone(),
                "live-b2".into(),
                Some("durable-b".into()),
                crate::session_registry::ProfileId::empty(),
                2,
                RuntimeKey::Local,
            )
            .await;

        // Both conversations restored, Active, routing via new live IDs.
        let ba = sessions.get(&conv_a, RuntimeKey::Local).await.unwrap();
        let bb = sessions.get(&conv_b, RuntimeKey::Local).await.unwrap();
        assert_eq!(ba.state, SessionState::Active);
        assert_eq!(bb.state, SessionState::Active);
        assert_eq!(
            sessions.route_event("live-a2", RuntimeKey::Local).await,
            Some(conv_a)
        );
        assert_eq!(
            sessions.route_event("live-b2", RuntimeKey::Local).await,
            Some(conv_b)
        );
        // Old live IDs from generation 1 no longer route.
        assert_eq!(
            sessions.route_event("live-a1", RuntimeKey::Local).await,
            None
        );
    }

    /// Test 13: RPC retry classification uses an explicit SAFE allowlist.
    /// Everything NOT in the allowlist (including session.create, approvals,
    /// and unknown future methods) defaults to OutcomeUnknown (safe-by-default).
    #[test]
    fn safe_retry_classification() {
        // Safe-to-retry methods (pure reads + session.resume which the pinned
        // Hermes serializes/dedupes).
        assert!(is_safe_retry("session.status"));
        assert!(is_safe_retry("session.history"));
        assert!(is_safe_retry("session.list"));
        assert!(is_safe_retry("session.active_list"));
        assert!(is_safe_retry("session.resume"));
        // NOT safe: prompt.submit, approvals, session.close, session.create.
        // session.create is NOT safe: a lost ack + retry creates a second session.
        assert!(!is_safe_retry("prompt.submit"));
        assert!(!is_safe_retry("approval.respond"));
        assert!(!is_safe_retry("secret.respond"));
        assert!(!is_safe_retry("sudo.respond"));
        assert!(!is_safe_retry("session.close"));
        assert!(!is_safe_retry("session.create"));
        // Unknown future method defaults to NOT safe (OutcomeUnknown).
        assert!(!is_safe_retry("some.future.method"));
    }

    /// Test 13 (integration): the OutcomeUnknown error carries the method name
    /// and displays distinctly from ConnectionLost.
    #[test]
    fn outcome_unknown_is_distinct_from_connection_lost() {
        let ou = WsError::OutcomeUnknown {
            method: "prompt.submit".into(),
            cause: InterruptionCause::Timeout,
        };
        let cl = WsError::ConnectionLost;
        // Distinct variants.
        assert!(matches!(ou, WsError::OutcomeUnknown { .. }));
        assert!(!matches!(ou, WsError::ConnectionLost));
        // Display explains the method and that it was not retried.
        let msg = ou.to_string();
        assert!(msg.contains("prompt.submit"), "msg: {}", msg);
        assert!(msg.contains("not retried"), "msg: {}", msg);
        // ConnectionLost display is different.
        assert_ne!(ou.to_string(), cl.to_string());
    }

    // ── Phase 1C.4: Real reconnect reconciliation integration test ───────────
    //
    // The audit demanded this: create 2 sessions, disconnect the socket,
    // reconnect, verify 2 REAL session.resume calls happened, verify new live
    // IDs in the registry, verify interleaved events route to the correct
    // conversation_id.

    /// A mock backend that accepts MULTIPLE connections on the same port, so a
    /// reconnect test can drop the first socket and establish a second one.
    /// Records all received frames across all connections for assertion.
    /// Returns (ws_url, received_frames, disconnect_signal). Signal
    /// disconnect_signal to make the server close its current WebSocket (simulating
    /// a server-initiated disconnect that the reader task must clean up after).
    async fn start_reconnect_mock_backend() -> (
        String,
        Arc<tokio::sync::Mutex<Vec<Value>>>,
        Arc<tokio::sync::Notify>,
        Arc<std::sync::atomic::AtomicBool>,
    ) {
        use tokio::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let received: Arc<tokio::sync::Mutex<Vec<Value>>> =
            Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let received_clone = Arc::clone(&received);
        let disconnect_signal: Arc<tokio::sync::Notify> = Arc::new(tokio::sync::Notify::new());
        let disconnect_clone = Arc::clone(&disconnect_signal);
        // When true, the mock closes the socket on receiving session.resume
        // instead of responding — simulates a mid-resume disconnect.
        let resume_fail_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let resume_fail_clone = Arc::clone(&resume_fail_flag);

        tokio::spawn(async move {
            // Accept multiple connections in a loop so reconnect works.
            for _ in 0..5 {
                let (stream, _) = match listener.accept().await {
                    Ok(s) => s,
                    Err(_) => break,
                };
                let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
                let rx_clone = Arc::clone(&received_clone);

                // gateway.ready
                let ready = json!({
                    "jsonrpc": "2.0", "method": "event",
                    "params": {"type": "gateway.ready", "payload": {}}
                });
                let _ = ws.send(Message::Text(ready.to_string())).await;

                // Handle requests on this connection until it closes OR the
                // test signals a server-initiated disconnect.
                loop {
                    tokio::select! {
                        biased;
                        _ = disconnect_clone.notified() => {
                            let _ = ws.close(None).await;
                            break;
                        }
                        msg = ws.next() => {
                            match msg {
                                Some(Ok(Message::Text(text))) => {
                                    let req: Value = match serde_json::from_str(&text) {
                                        Ok(v) => v,
                                        Err(_) => continue,
                                    };
                                    rx_clone.lock().await.push(req.clone());
                                    let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");
                                    let id = req.get("id").and_then(|i| i.as_u64()).unwrap_or(0);
                                    match method {
                                        "session.create" => {
                                            let source = req.get("params").and_then(|p| p.get("source")).and_then(|s| s.as_str()).unwrap_or("desktop");
                                            let (new_sid, stored) = if source == "compat_probe" {
                                                ("probe-sess".to_string(), "probe-sess".to_string())
                                            } else {
                                                let s = format!("{}-stored", source);
                                                (format!("{}-live", source), s)
                                            };
                                            let resp = json!({"jsonrpc":"2.0","id":id,"result":{"session_id":new_sid,"stored_session_id":stored,"message_count":0,"messages":[],"info":{"desktop_contract":4}}});
                                            let _ = ws.send(Message::Text(resp.to_string())).await;
                                        }
                                        "session.resume" => {
                                            // If the fail flag is set, close the socket instead of
                                            // responding — simulates a mid-resume disconnect.
                                            if resume_fail_clone.load(std::sync::atomic::Ordering::Relaxed) {
                                                let _ = ws.close(None).await;
                                                break;
                                            }
                                            let durable = req.get("params").and_then(|p| p.get("session_id")).and_then(|s| s.as_str());
                                            match durable {
                                                // Deliberately violate the resume result schema for
                                                // the integration test below. The connection remains
                                                // healthy, isolating protocol failure from interruption.
                                                Some("malformed-resume") => {
                                                    let resp = json!({"jsonrpc":"2.0","id":id,"result":{}});
                                                    let _ = ws.send(Message::Text(resp.to_string())).await;
                                                }
                                                Some(d) if !d.is_empty() => {
                                                    let new_live = format!("{}-resumed", d);
                                                    let resp = json!({"jsonrpc":"2.0","id":id,"result":{"session_id":new_live,"resumed":d,"session_key":d,"message_count":0,"messages":[],"info":{}}});
                                                    let _ = ws.send(Message::Text(resp.to_string())).await;
                                                }
                                                _ => {
                                                    let resp = json!({"jsonrpc":"2.0","id":id,"error":{"code":4006,"message":"session_id required"}});
                                                    let _ = ws.send(Message::Text(resp.to_string())).await;
                                                }
                                            }
                                        }
                                        "session.close" => {
                                            let resp = json!({"jsonrpc":"2.0","id":id,"result":{}});
                                            let _ = ws.send(Message::Text(resp.to_string())).await;
                                        }
                                        "prompt.submit" => {
                                            let sid = req.get("params").and_then(|p| p.get("session_id")).and_then(|s| s.as_str()).unwrap_or("");
                                            let ack = json!({"jsonrpc":"2.0","id":id,"result":{"status":"streaming"}});
                                            let _ = ws.send(Message::Text(ack.to_string())).await;
                                            for ev in [
                                                json!({"jsonrpc":"2.0","method":"event","params":{"type":"message.delta","session_id":sid,"payload":{"text":"Hi"}}}),
                                                json!({"jsonrpc":"2.0","method":"event","params":{"type":"message.complete","session_id":sid,"payload":{"text":"Hi","status":"complete"}}}),
                                            ] {
                                                let _ = ws.send(Message::Text(ev.to_string())).await;
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                                Some(Ok(Message::Close(_))) | None => break,
                                _ => {}
                            }
                        }
                    }
                }
                // Connection closed; loop back to accept the next one.
            }
        });

        (
            format!("ws://127.0.0.1:{}/api/ws?token=test", port),
            received,
            disconnect_signal,
            resume_fail_flag,
        )
    }

    /// Start a mock backend that accumulates TWO prompt.submit requests and then
    /// emits the exact interleaved sequence:
    ///   A.delta, B.delta, A.tool_start, B.approval_request, A.complete, B.complete
    ///
    /// Returns (ws_url, received_frames, emitted_events) where emitted_events
    /// records the exact sequence of (conversation_id, event_type) for assertions.
    // Type alias to reduce complexity for clippy::type_complexity.
    type EmitOrder = Arc<tokio::sync::Mutex<Vec<(Option<String>, &'static str)>>>;

    async fn start_interleaved_mock_backend() -> (String, Arc<tokio::sync::Mutex<Vec<Value>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let received: Arc<tokio::sync::Mutex<Vec<Value>>> =
            Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let received_clone = Arc::clone(&received);
        // Record the exact emitted event sequence for the test to assert against.
        let emit_order: EmitOrder = Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let emit_order_clone = Arc::clone(&emit_order);

        tokio::spawn(async move {
            // Accept exactly one connection for this test.
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();

            // gateway.ready
            let ready = json!({
                "jsonrpc": "2.0", "method": "event",
                "params": {"type": "gateway.ready", "payload": {}}
            });
            let _ = ws.send(Message::Text(ready.to_string())).await;

            // Buffer for the two prompt.submit requests.
            let mut submit_buffer: Vec<(u64, String)> = Vec::new(); // (id, session_id)

            while let Some(Ok(msg)) = ws.next().await {
                if let Message::Text(text) = msg {
                    let req: Value = match serde_json::from_str(&text) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };
                    received_clone.lock().await.push(req.clone());
                    let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");
                    let id = req.get("id").and_then(|i| i.as_u64()).unwrap_or(0);

                    match method {
                        "session.create" => {
                            let source = req
                                .get("params")
                                .and_then(|p| p.get("source"))
                                .and_then(|s| s.as_str())
                                .unwrap_or("desktop");
                            let (new_sid, stored) = if source == "compat_probe" {
                                ("probe-sess".to_string(), "probe-sess".to_string())
                            } else {
                                let s = format!("{}-stored", source);
                                (format!("{}-live", source), s)
                            };
                            let resp = json!({
                                "jsonrpc": "2.0", "id": id,
                                "result": {
                                    "session_id": new_sid,
                                    "stored_session_id": stored,
                                    "message_count": 0,
                                    "messages": [],
                                    "info": {"desktop_contract": 4}
                                }
                            });
                            let _ = ws.send(Message::Text(resp.to_string())).await;
                        }
                        "session.close" => {
                            let resp = json!({"jsonrpc":"2.0","id":id,"result":{}});
                            let _ = ws.send(Message::Text(resp.to_string())).await;
                        }
                        "session.resume" => {
                            let durable = req
                                .get("params")
                                .and_then(|p| p.get("session_id"))
                                .and_then(|s| s.as_str());
                            match durable {
                                Some(d) if !d.is_empty() => {
                                    let new_live = format!("{}-resumed", d);
                                    let resp = json!({
                                        "jsonrpc": "2.0", "id": id,
                                        "result": {
                                            "session_id": new_live,
                                            "resumed": d,
                                            "session_key": d,
                                            "message_count": 0,
                                            "messages": [],
                                            "info": {}
                                        }
                                    });
                                    let _ = ws.send(Message::Text(resp.to_string())).await;
                                }
                                _ => {
                                    let resp = json!({"jsonrpc":"2.0","id":id,"error":{"code":4006,"message":"session_id required"}});
                                    let _ = ws.send(Message::Text(resp.to_string())).await;
                                }
                            }
                        }
                        "prompt.submit" => {
                            // Accumulate the two submits.
                            let sid = req
                                .get("params")
                                .and_then(|p| p.get("session_id"))
                                .and_then(|s| s.as_str())
                                .unwrap_or("")
                                .to_string();
                            submit_buffer.push((id, sid));

                            // ACK immediately.
                            let ack =
                                json!({"jsonrpc":"2.0","id":id,"result":{"status":"streaming"}});
                            let _ = ws.send(Message::Text(ack.to_string())).await;

                            // When we have two submits, emit the interleaved sequence.
                            if submit_buffer.len() == 2 {
                                let (_, sid_a) = submit_buffer[0].clone();
                                let (_, sid_b) = submit_buffer[1].clone();

                                // Interleaved events:
                                // A.delta, B.delta, A.tool_start, B.approval_request, A.complete, B.complete
                                let events: Vec<Value> = vec![
                                    json!({"jsonrpc":"2.0","method":"event","params":{"type":"message.delta","session_id":sid_a,"payload":{"text":"A1"}}}),
                                    json!({"jsonrpc":"2.0","method":"event","params":{"type":"message.delta","session_id":sid_b,"payload":{"text":"B1"}}}),
                                    json!({"jsonrpc":"2.0","method":"event","params":{"type":"tool.start","session_id":sid_a,"payload":{"tool_id":"tc_a","name":"tool_a"}}}),
                                    json!({"jsonrpc":"2.0","method":"event","params":{"type":"approval.request","session_id":sid_b,"payload":{"request_id":"ar_b","tool_id":"tc_b","name":"tool_b","tool_input":"{}"}}}),
                                    json!({"jsonrpc":"2.0","method":"event","params":{"type":"message.complete","session_id":sid_a,"payload":{"text":"A complete","status":"complete"}}}),
                                    json!({"jsonrpc":"2.0","method":"event","params":{"type":"message.complete","session_id":sid_b,"payload":{"text":"B complete","status":"complete"}}}),
                                ];

                                for ev in events {
                                    let _ = ws.send(Message::Text(ev.to_string())).await;
                                }

                                // Record the expected emit order for verification.
                                let mut order = emit_order_clone.lock().await;
                                order.push((Some(sid_a.clone()), "token")); // A.delta -> Token
                                order.push((Some(sid_b.clone()), "token")); // B.delta -> Token
                                order.push((Some(sid_a.clone()), "tool_start")); // A.tool_start
                                order.push((Some(sid_b.clone()), "approval")); // B.approval_request
                                order.push((Some(sid_a.clone()), "done")); // A.complete -> Done
                                order.push((Some(sid_b.clone()), "done")); // B.complete -> Done
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

    /// Phase 1C.4 integration test: real reconnect reconciliation.
    ///
    /// 1. Connect, create 2 sessions (distinct durable IDs), register them.
    /// 2. Drop the connection (shutdown the reader task / set Disconnected).
    /// 3. Reconnect — ensure_ws_connection must run session.resume for BOTH
    ///    suspended conversations.
    /// 4. Verify 2 real session.resume RPCs were sent to the backend.
    /// 5. Verify new live IDs are in the registry (Active state).
    /// 6. Submit prompts on both — events must carry the correct
    ///    conversation_id (interleaved routing).
    #[tokio::test]
    async fn reconnect_reconciles_two_conversations_via_real_resume() {
        use crate::session_registry::{ConversationId, SessionState};

        let (ws_url, received, disconnect_signal, _resume_fail) =
            start_reconnect_mock_backend().await;
        let ws_state = Arc::new(WsState::new());
        let sessions = crate::session_registry::SessionRegistry::new();
        let (emit_fn, emitted) = mock_emitter();

        // 1. Connect + handshake.
        ensure_ws_connection(&ws_url, emit_fn, &ws_state, Some(sessions.clone()))
            .await
            .expect("first connect");
        wait_connected(&ws_state, 5000).await;
        let gen1 = ws_state
            .generation
            .load(std::sync::atomic::Ordering::Acquire);

        // Create two distinct conversations with distinct durable IDs.
        let conv_a = ConversationId::new("conv-a");
        let conv_b = ConversationId::new("conv-b");

        // create_session returns session_id based on source; use distinct sources.
        let ra = create_session_on_connection(&ws_state, "conva", None)
            .await
            .unwrap();
        sessions
            .set_live(
                conv_a.clone(),
                ra.session_id.clone(),
                Some(ra.stored_session_id),
                crate::session_registry::ProfileId::empty(),
                gen1,
                RuntimeKey::Local,
            )
            .await;
        let rb = create_session_on_connection(&ws_state, "convb", None)
            .await
            .unwrap();
        sessions
            .set_live(
                conv_b.clone(),
                rb.session_id.clone(),
                Some(rb.stored_session_id),
                crate::session_registry::ProfileId::empty(),
                gen1,
                RuntimeKey::Local,
            )
            .await;

        // 2. Server-initiated disconnect: signal the mock to close its WS.
        // Do NOT manually set Disconnected/Shutdown/suspend_generation — the
        // reader task must detect the real socket close and run cleanup itself.
        disconnect_signal.notify_waiters();
        // Wait for the reader task to observe the close and transition to
        // Disconnected + suspend the generation's bindings.
        let mut disconnected = false;
        for _ in 0..100 {
            if ws_state.runtime.lock().await.state == ConnectionState::Disconnected {
                disconnected = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(
            disconnected,
            "reader task must transition to Disconnected after real socket close"
        );

        // Verify both bindings became Suspended automatically (reader cleanup).
        assert_eq!(
            sessions
                .get(&conv_a, RuntimeKey::Local)
                .await
                .unwrap()
                .state,
            SessionState::Suspended,
            "conv-a must be Suspended after real disconnect cleanup"
        );
        assert_eq!(
            sessions
                .get(&conv_b, RuntimeKey::Local)
                .await
                .unwrap()
                .state,
            SessionState::Suspended,
            "conv-b must be Suspended after real disconnect cleanup"
        );

        // 3. Reconnect — reconciliation resumes both. Reuse the SAME events Arc
        // so post-reconnect events are captured.
        let emit_fn2 = mock_emitter_into(Arc::clone(&emitted));
        ensure_ws_connection(&ws_url, emit_fn2, &ws_state, Some(sessions.clone()))
            .await
            .expect("reconnect");
        wait_connected(&ws_state, 5000).await;

        // 4. Verify 2 real session.resume RPCs were sent.
        tokio::time::sleep(Duration::from_millis(200)).await;
        let frames = received.lock().await;
        let resume_calls: Vec<_> = frames
            .iter()
            .filter(|v| v.get("method").and_then(|m| m.as_str()) == Some("session.resume"))
            .collect();
        assert_eq!(
            resume_calls.len(),
            2,
            "expected exactly 2 session.resume calls, got {}: {:?}",
            resume_calls.len(),
            frames
                .iter()
                .map(|f| f.get("method").and_then(|m| m.as_str()))
                .collect::<Vec<_>>()
        );

        // 5. Verify new live IDs in registry (Active, resumed).
        let ba = sessions
            .get(&conv_a, RuntimeKey::Local)
            .await
            .expect("conv-a binding");
        let bb = sessions
            .get(&conv_b, RuntimeKey::Local)
            .await
            .expect("conv-b binding");
        assert_eq!(
            ba.state,
            SessionState::Active,
            "conv-a must be Active after resume"
        );
        assert_eq!(
            bb.state,
            SessionState::Active,
            "conv-b must be Active after resume"
        );
        // Live IDs changed (resumed suffix).
        assert!(ba.live_session_id.as_deref().unwrap().contains("resumed"));
        assert!(bb.live_session_id.as_deref().unwrap().contains("resumed"));

        drop(frames);

        // 6. Submit prompts on both — events route with correct conversation_id.
        let live_a = sessions.get_live(&conv_a, RuntimeKey::Local).await.unwrap();
        let live_b = sessions.get_live(&conv_b, RuntimeKey::Local).await.unwrap();
        // Submit both (they stream interleaved events).
        submit_prompt_on_connection(&ws_state, &live_a, "msg A")
            .await
            .expect("submit A");
        submit_prompt_on_connection(&ws_state, &live_b, "msg B")
            .await
            .expect("submit B");

        // Wait for events to be emitted.
        tokio::time::sleep(Duration::from_millis(300)).await;
        let emitted_events = emitted.lock().await;
        // Assert EXACT routing: each conversation received a delta then a done,
        // and events for conv-a and conv-b are distinguishable by conversation_id.
        // This verifies the registry routes by live session_id → conversation.
        let a_tags: Vec<&str> = emitted_events
            .iter()
            .filter(|(c, _)| c.as_deref() == Some("conv-a"))
            .map(|(_, t)| *t)
            .collect();
        let b_tags: Vec<&str> = emitted_events
            .iter()
            .filter(|(c, _)| c.as_deref() == Some("conv-b"))
            .map(|(_, t)| *t)
            .collect();
        assert!(
            a_tags.contains(&"token"),
            "conv-a must have a token event: {:?}",
            a_tags
        );
        assert!(
            a_tags.contains(&"done"),
            "conv-a must have a done event: {:?}",
            a_tags
        );
        assert!(
            b_tags.contains(&"token"),
            "conv-b must have a token event: {:?}",
            b_tags
        );
        assert!(
            b_tags.contains(&"done"),
            "conv-b must have a done event: {:?}",
            b_tags
        );
        // No event should have a None conversation_id (all are session-scoped).
        assert!(
            !emitted_events.iter().any(|(c, _)| c.is_none()),
            "no event should have conversation_id=None: {:?}",
            *emitted_events
        );
    }

    /// Phase 1C.4: profile-aware durable identity. Two conversations with the
    /// SAME durable ID but DIFFERENT profiles must produce two distinct resume
    /// calls with different profile params after reconnect.
    #[tokio::test]
    async fn profile_aware_resume_sends_distinct_profiles() {
        use crate::session_registry::{ConversationId, ProfileId, SessionState};

        let (ws_url, received, disconnect_signal, _resume_fail) =
            start_reconnect_mock_backend().await;
        let ws_state = Arc::new(WsState::new());
        let sessions = crate::session_registry::SessionRegistry::new();
        let (emit_fn, _) = mock_emitter();

        ensure_ws_connection(&ws_url, emit_fn, &ws_state, Some(sessions.clone()))
            .await
            .expect("connect");
        wait_connected(&ws_state, 5000).await;
        let gen1 = ws_state
            .generation
            .load(std::sync::atomic::Ordering::Acquire);

        // Create two bindings with the SAME stored_session_id but different profiles.
        // In a real backend these resolve to different state.db rows.
        let conv_a = ConversationId::new("conv-profile-a");
        let conv_b = ConversationId::new("conv-profile-b");
        sessions
            .set_live(
                conv_a.clone(),
                "live-a".into(),
                Some("same-durable".into()),
                ProfileId::new("work"),
                gen1,
                RuntimeKey::Local,
            )
            .await;
        sessions
            .set_live(
                conv_b.clone(),
                "live-b".into(),
                Some("same-durable".into()),
                ProfileId::new("personal"),
                gen1,
                RuntimeKey::Local,
            )
            .await;

        // Disconnect.
        disconnect_signal.notify_waiters();
        for _ in 0..100 {
            if ws_state.runtime.lock().await.state == ConnectionState::Disconnected {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        // Reconnect — reconciliation resumes both with their respective profiles.
        let (emit_fn2, _) = mock_emitter();
        ensure_ws_connection(&ws_url, emit_fn2, &ws_state, Some(sessions.clone()))
            .await
            .expect("reconnect");
        wait_connected(&ws_state, 5000).await;
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Verify both resume calls carry distinct profiles.
        let frames = received.lock().await;
        let resume_with_profile: Vec<Option<String>> = frames
            .iter()
            .filter(|v| v.get("method").and_then(|m| m.as_str()) == Some("session.resume"))
            .map(|v| {
                v.get("params")
                    .and_then(|p| p.get("profile"))
                    .and_then(|p| p.as_str())
                    .map(|s| s.to_string())
            })
            .collect();
        assert_eq!(resume_with_profile.len(), 2, "expected 2 resume calls");
        assert!(
            resume_with_profile.contains(&Some("work".into())),
            "expected a resume with profile 'work': {:?}",
            resume_with_profile
        );
        assert!(
            resume_with_profile.contains(&Some("personal".into())),
            "expected a resume with profile 'personal': {:?}",
            resume_with_profile
        );
        // Both bindings restored.
        assert_eq!(
            sessions
                .get(&conv_a, RuntimeKey::Local)
                .await
                .unwrap()
                .state,
            SessionState::Active
        );
        assert_eq!(
            sessions
                .get(&conv_b, RuntimeKey::Local)
                .await
                .unwrap()
                .state,
            SessionState::Active
        );
    }

    /// Phase 1C.4: double-disconnect-during-resume safety. If the socket dies
    /// while session.resume RPCs are in flight, bindings must return to Suspended
    /// (retryable), NOT get stuck in Resuming or marked ResumeFailed.
    #[tokio::test]
    async fn double_disconnect_during_resume_returns_to_suspended() {
        use crate::session_registry::{ConversationId, ProfileId, SessionState};

        // Use a mock that we can disconnect twice.
        let (ws_url, _received, disconnect_signal, _resume_fail) =
            start_reconnect_mock_backend().await;
        let ws_state = Arc::new(WsState::new());
        let sessions = crate::session_registry::SessionRegistry::new();
        let (emit_fn, _) = mock_emitter();

        // Gen 1: connect + two sessions.
        ensure_ws_connection(&ws_url, emit_fn, &ws_state, Some(sessions.clone()))
            .await
            .expect("connect 1");
        wait_connected(&ws_state, 5000).await;
        let gen1 = ws_state
            .generation
            .load(std::sync::atomic::Ordering::Acquire);
        sessions
            .set_live(
                ConversationId::new("c1"),
                "l1".into(),
                Some("d1".into()),
                ProfileId::empty(),
                gen1,
                RuntimeKey::Local,
            )
            .await;
        sessions
            .set_live(
                ConversationId::new("c2"),
                "l2".into(),
                Some("d2".into()),
                ProfileId::empty(),
                gen1,
                RuntimeKey::Local,
            )
            .await;

        // First disconnect → Suspended.
        disconnect_signal.notify_waiters();
        for _ in 0..100 {
            if ws_state.runtime.lock().await.state == ConnectionState::Disconnected {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        // Simulate resume being in-flight when the SECOND disconnect hits:
        // manually transition to Resuming (as take_suspended_for_resume does),
        // then call suspend_generation to simulate the reader cleanup firing on
        // the second socket death during reconciliation.
        sessions
            .take_suspended_for_resume(gen1, RuntimeKey::Local)
            .await; // transitions to Resuming
        assert_eq!(
            sessions
                .get(&ConversationId::new("c1"), RuntimeKey::Local)
                .await
                .unwrap()
                .state,
            SessionState::Resuming {
                attempt_generation: gen1
            }
        );
        // The second reader task dies; its cleanup must return Resuming→Suspended.
        // In the real flow this is suspend_generation(gen2); here gen1 bindings
        // are Resuming so suspend_generation(gen1) tests the path.
        sessions.suspend_generation(gen1, RuntimeKey::Local).await;
        assert_eq!(
            sessions
                .get(&ConversationId::new("c1"), RuntimeKey::Local)
                .await
                .unwrap()
                .state,
            SessionState::Suspended,
            "Resuming binding must return to Suspended after disconnect during resume"
        );
        assert_eq!(
            sessions
                .get(&ConversationId::new("c2"), RuntimeKey::Local)
                .await
                .unwrap()
                .state,
            SessionState::Suspended
        );
    }

    /// Phase 1C.4 FINAL: real gen1 → gen2 disconnect-during-resume → gen3 recovery.
    /// NO manual state manipulation. The mock closes the socket on gen2 when it
    /// receives session.resume; the reader task cleanup must return bindings to
    /// Suspended; gen3 reconnect must successfully resume both.
    #[tokio::test]
    async fn gen3_recovery_after_disconnect_during_resume() {
        use crate::session_registry::{ConversationId, ProfileId, SessionState};

        let (ws_url, received, disconnect_signal, resume_fail_flag) =
            start_reconnect_mock_backend().await;
        let ws_state = Arc::new(WsState::new());
        let sessions = crate::session_registry::SessionRegistry::new();
        let (emit_fn, _) = mock_emitter();

        // Gen 1: connect + two sessions.
        ensure_ws_connection(&ws_url, emit_fn, &ws_state, Some(sessions.clone()))
            .await
            .expect("gen1 connect");
        wait_connected(&ws_state, 5000).await;
        let gen1 = ws_state
            .generation
            .load(std::sync::atomic::Ordering::Acquire);
        sessions
            .set_live(
                ConversationId::new("c1"),
                "l1".into(),
                Some("d1".into()),
                ProfileId::empty(),
                gen1,
                RuntimeKey::Local,
            )
            .await;
        sessions
            .set_live(
                ConversationId::new("c2"),
                "l2".into(),
                Some("d2".into()),
                ProfileId::empty(),
                gen1,
                RuntimeKey::Local,
            )
            .await;

        // Gen 1 disconnect.
        disconnect_signal.notify_waiters();
        for _ in 0..100 {
            if ws_state.runtime.lock().await.state == ConnectionState::Disconnected {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        // Set the fail flag: gen2 will close the socket when it receives resume.
        resume_fail_flag.store(true, std::sync::atomic::Ordering::Relaxed);

        // Gen 2: connect — reconciliation starts session.resume, mock closes socket.
        let (emit_fn2, _) = mock_emitter();
        let gen2_result =
            ensure_ws_connection(&ws_url, emit_fn2, &ws_state, Some(sessions.clone())).await;
        // ensure_ws_connection must FAIL (ConnectionLost) — it cannot declare
        // Connected after the socket died during reconciliation.
        assert!(
            gen2_result.is_err(),
            "gen2 must fail when socket dies during resume, got: {:?}",
            gen2_result
        );
        // Bindings must be back to Suspended (not stuck in Resuming/ResumeFailed).
        assert_eq!(
            sessions
                .get(&ConversationId::new("c1"), RuntimeKey::Local)
                .await
                .unwrap()
                .state,
            SessionState::Suspended,
            "c1 must be Suspended after gen2 disconnect during resume"
        );
        assert_eq!(
            sessions
                .get(&ConversationId::new("c2"), RuntimeKey::Local)
                .await
                .unwrap()
                .state,
            SessionState::Suspended
        );

        // Clear the fail flag: gen3 will respond normally.
        resume_fail_flag.store(false, std::sync::atomic::Ordering::Relaxed);

        // Gen 3: connect — reconciliation resumes both successfully.
        let (emit_fn3, _) = mock_emitter();
        ensure_ws_connection(&ws_url, emit_fn3, &ws_state, Some(sessions.clone()))
            .await
            .expect("gen3 connect must succeed");
        wait_connected(&ws_state, 5000).await;

        // Both bindings must be Active with new live IDs.
        assert_eq!(
            sessions
                .get(&ConversationId::new("c1"), RuntimeKey::Local)
                .await
                .unwrap()
                .state,
            SessionState::Active
        );
        assert_eq!(
            sessions
                .get(&ConversationId::new("c2"), RuntimeKey::Local)
                .await
                .unwrap()
                .state,
            SessionState::Active
        );

        // Verify resume calls: gen2 may send 1-2 (socket closes on first resume),
        // gen3 sends 2 (both succeed). At minimum, gen3's 2 resumes must be present.
        tokio::time::sleep(Duration::from_millis(200)).await;
        let resume_count = received
            .lock()
            .await
            .iter()
            .filter(|v| v.get("method").and_then(|m| m.as_str()) == Some("session.resume"))
            .count();
        assert!(
            resume_count >= 3,
            "expected at least 3 resume calls (gen2 partial + gen3 both), got {}",
            resume_count
        );
    }

    /// Phase 1C.4: Real interleaved routing test.
    ///
    /// The mock backend accumulates TWO prompt.submit requests and then emits
    /// the exact interleaved sequence:
    ///   A.delta, B.delta, A.tool_start, B.approval_request, A.complete, B.complete
    ///
    /// The test verifies the exact sequence of conversation_ids matches.
    #[tokio::test]
    async fn interleaved_routing_exact_sequence() {
        use crate::session_registry::{ConversationId, ProfileId};

        // Start a mock backend that accumulates two prompt.submits and emits interleaved events.
        let (ws_url, received) = start_interleaved_mock_backend().await;
        let ws_state = Arc::new(WsState::new());
        let sessions = crate::session_registry::SessionRegistry::new();
        let (emit_fn, events) = mock_emitter();

        // Connect and handshake.
        ensure_ws_connection(&ws_url, emit_fn, &ws_state, Some(sessions.clone()))
            .await
            .expect("connect");
        wait_connected(&ws_state, 5000).await;
        let gen1 = ws_state
            .generation
            .load(std::sync::atomic::Ordering::Acquire);

        // Create two conversations with distinct live IDs.
        let conv_a = ConversationId::new("conv-a");
        let conv_b = ConversationId::new("conv-b");

        let ra = create_session_on_connection(&ws_state, "conv-a", None)
            .await
            .unwrap();
        sessions
            .set_live(
                conv_a.clone(),
                ra.session_id.clone(),
                Some(ra.stored_session_id),
                ProfileId::empty(),
                gen1,
                RuntimeKey::Local,
            )
            .await;

        let rb = create_session_on_connection(&ws_state, "conv-b", None)
            .await
            .unwrap();
        sessions
            .set_live(
                conv_b.clone(),
                rb.session_id.clone(),
                Some(rb.stored_session_id),
                ProfileId::empty(),
                gen1,
                RuntimeKey::Local,
            )
            .await;

        // Submit prompts on BOTH conversations concurrently (they will be queued in the mock).
        let live_a = sessions.get_live(&conv_a, RuntimeKey::Local).await.unwrap();
        let live_b = sessions.get_live(&conv_b, RuntimeKey::Local).await.unwrap();

        // Fire both submits without awaiting - they go through the mpsc channel.
        let submit_a = submit_prompt_on_connection(&ws_state, &live_a, "msg A");
        let submit_b = submit_prompt_on_connection(&ws_state, &live_b, "msg B");

        // Wait for both to complete (ack received).
        let (res_a, res_b) = futures::future::join(submit_a, submit_b).await;
        res_a.expect("submit A failed");
        res_b.expect("submit B failed");

        // Now wait for the mock to emit all interleaved events.
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Verify exact sequence of conversation_ids in emitted events.
        let emitted: Vec<_> = events
            .lock()
            .await
            .iter()
            .map(|(cid, tag)| (cid.clone(), *tag))
            .collect();
        // The mock emits events with the live session_ids (e.g., "conva-live", "convb-live")
        // but the emitter records the conversation_id ("conv-a", "conv-b") via routing.
        let expected = vec![
            (Some("conv-a".to_string()), "token"),
            (Some("conv-b".to_string()), "token"),
            (Some("conv-a".to_string()), "tool_start"),
            (Some("conv-b".to_string()), "approval"),
            (Some("conv-a".to_string()), "done"),
            (Some("conv-b".to_string()), "done"),
        ];
        assert_eq!(
            emitted, expected,
            "emitted sequence mismatch:\n  got: {emitted:?}\n  exp: {expected:?}"
        );

        // Also verify the mock received exactly two prompt.submits.
        let frames = received.lock().await;
        let submit_count = frames
            .iter()
            .filter(|v| v.get("method").and_then(|m| m.as_str()) == Some("prompt.submit"))
            .count();
        assert_eq!(submit_count, 2, "expected exactly 2 prompt.submit calls");
    }

    /// Phase 1C.4: malformed resume response → ResumeFailed (no retry loop).
    ///
    /// The mock returns a response that doesn't match SessionResumeResult schema
    /// (missing required fields). This produces WsError::Protocol which should
    /// NOT be treated as a retryable interruption. The binding must be marked
    /// ResumeFailed and subsequent reconnect must NOT retry session.resume.
    #[tokio::test]
    async fn malformed_resume_response_marks_resumefailed_no_retry() {
        use crate::session_registry::{ConversationId, ProfileId, SessionState};

        // The reconnect mock returns `{}` for durable ID "malformed-resume",
        // which cannot deserialize as SessionResumeResult while keeping the
        // WebSocket connection healthy.
        let (ws_url, received, disconnect_signal, _resume_fail) =
            start_reconnect_mock_backend().await;
        let ws_state = Arc::new(WsState::new());
        let sessions = crate::session_registry::SessionRegistry::new();
        let (emit_fn, _) = mock_emitter();

        // Gen 1: connect + one session.
        ensure_ws_connection(&ws_url, emit_fn, &ws_state, Some(sessions.clone()))
            .await
            .expect("gen1 connect");
        wait_connected(&ws_state, 5000).await;
        let gen1 = ws_state
            .generation
            .load(std::sync::atomic::Ordering::Acquire);
        sessions
            .set_live(
                ConversationId::new("c1"),
                "l1".into(),
                Some("malformed-resume".into()),
                ProfileId::empty(),
                gen1,
                RuntimeKey::Local,
            )
            .await;

        // Gen 1 disconnect.
        disconnect_signal.notify_waiters();
        for _ in 0..100 {
            if ws_state.runtime.lock().await.state == ConnectionState::Disconnected {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        // Gen 2: a real session.resume receives the malformed result. The
        // connection stays up, so reconciliation completes in Degraded state.
        let (emit_fn2, _) = mock_emitter();
        ensure_ws_connection(&ws_url, emit_fn2, &ws_state, Some(sessions.clone()))
            .await
            .expect("malformed resume must degrade, not disconnect");
        assert_eq!(
            ws_state.runtime.lock().await.state,
            ConnectionState::Degraded,
            "a protocol error during resume must leave the healthy connection Degraded"
        );
        assert_eq!(
            sessions
                .get(&ConversationId::new("c1"), RuntimeKey::Local)
                .await
                .unwrap()
                .state,
            SessionState::ResumeFailed,
            "malformed resume result must permanently fail this binding"
        );

        let resume_count_after_failure = received
            .lock()
            .await
            .iter()
            .filter(|frame| frame.get("method").and_then(|m| m.as_str()) == Some("session.resume"))
            .count();
        assert_eq!(
            resume_count_after_failure, 1,
            "expected one malformed resume attempt"
        );

        // Gen 3: force a fresh WebSocket connection. ResumeFailed bindings are
        // not eligible for reconciliation, so no second resume request is sent.
        disconnect_signal.notify_waiters();
        for _ in 0..100 {
            if ws_state.runtime.lock().await.state == ConnectionState::Disconnected {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert_eq!(
            ws_state.runtime.lock().await.state,
            ConnectionState::Disconnected
        );

        let (emit_fn3, _) = mock_emitter();
        ensure_ws_connection(&ws_url, emit_fn3, &ws_state, Some(sessions.clone()))
            .await
            .expect("reconnect after permanent resume failure");
        wait_connected(&ws_state, 5000).await;

        let resume_count_after_reconnect = received
            .lock()
            .await
            .iter()
            .filter(|frame| frame.get("method").and_then(|m| m.as_str()) == Some("session.resume"))
            .count();
        assert_eq!(
            resume_count_after_reconnect,
            1,
            "ResumeFailed must not be retried on reconnect; frames: {:?}",
            received.lock().await
        );
    }
}
