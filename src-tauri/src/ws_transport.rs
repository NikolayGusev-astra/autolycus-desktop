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

use std::time::Duration;

use futures::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};
use tokio_tungstenite::tungstenite::Message;

use crate::chat::{parse_ws_message, ChatEvent};

/// JSON-RPC request id counter (process-wide; requests are sequential within a
/// single short-lived WS connection, so collision across connections is fine).
static NEXT_RPC_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn next_rpc_id() -> u64 {
    NEXT_RPC_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
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
    tracing::info!(target: "steersman_desktop_lib::ws", ws_url, "connecting");
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
        _ => create_session(&mut ws).await?,
    };
    tracing::info!(target: "steersman_desktop_lib::ws", session_id = %sid, "session ready");

    // 3. Submit the prompt; the response acks turn start, real content streams.
    submit_prompt(&mut ws, &sid, text).await?;

    // 4. Read streaming events until the turn ends (done/error) or the socket
    //    closes. Each recognised event is emitted on `chat_event`, identical to
    //    what the HTTP transport did — the frontend needs no changes.
    let result = read_events(&mut ws, app_handle).await;

    // Best-effort close.
    let _ = ws.close(None).await;

    match result {
        Ok(returned_sid) => Ok(returned_sid.unwrap_or(sid)),
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
                    if let Some(value) = serde_json::from_str::<Value>(&text).ok() {
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

/// Send `session.create`, return the new session_id from the response.
async fn create_session<S>(ws: &mut S) -> Result<String, WsError>
where
    S: SinkExt<Message, Error = tokio_tungstenite::tungstenite::Error>
        + StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>>
        + Unpin,
{
    let id = next_rpc_id();
    let req = json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "session.create",
        "params": {},
    });
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

/// Send `prompt.submit` for the given session.
async fn submit_prompt<S>(ws: &mut S, session_id: &str, text: &str) -> Result<(), WsError>
where
    S: SinkExt<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    // We don't wait for the prompt.submit response (it only acks turn-start;
    // real content arrives as events). The id is still sent per JSON-RPC.
    let req = json!({
        "jsonrpc": "2.0",
        "id": next_rpc_id(),
        "method": "prompt.submit",
        "params": {
            "session_id": session_id,
            "text": text,
        },
    });
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
                        if matches!(event, ChatEvent::Done { ref session_id } if session_id.is_some()) {
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
    matches!(
        event,
        ChatEvent::Done { .. } | ChatEvent::Error { .. }
    )
}

async fn send_json<S>(ws: &mut S, value: &Value) -> Result<(), WsError>
where
    S: SinkExt<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    let text = serde_json::to_string(value).map_err(|e| WsError::Protocol(format!("encode: {}", e)))?;
    ws.send(Message::Text(text))
        .await
        .map_err(|e| WsError::Protocol(format!("ws send: {}", e)))
}

// ── Typed errors (ADR-004 §Последствия, P3.3) ──────────────────────────────
//
// The WS transport previously returned Result<T, String> everywhere, which
// forced the frontend to display a generic message and made it impossible to
// branch on error KIND in calling code (e.g. retry on Connect, surface auth
// on AuthFailed, give up on Protocol). WsError replaces String across the WS
// path. Tauri serializes the variant via thiserror's Display → JSON string,
// and callers can match on the variant.

/// Error type for the WebSocket transport (ws_transport.rs).
///
/// Scope (P3.3): the WS path only. gateway.rs still returns GatewayStartResult
/// (a struct, not Result) and the legacy HTTP path still uses String — both are
/// out of scope here and tracked separately.
#[derive(Debug)]
pub enum WsError {
    /// WebSocket connect or upgrade failed (network down, wrong port, TLS).
    Connect(String),
    /// Auth rejected by the backend (bad/expired session token) → 401/403 on /api/ws.
    AuthFailed,
    /// session.create RPC failed or returned no session_id.
    SessionCreate(String),
    /// JSON-RPC send/encode failure (should be rare; protocol-level).
    Protocol(String),
    /// A streaming turn ended with an `error` event from the backend.
    BackendError(String),
    /// The turn did not complete within the deadline.
    Timeout,
    /// Underlying socket error mid-stream.
    Stream(String),
}

impl std::fmt::Display for WsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WsError::Connect(s) => write!(f, "WS connect failed: {}", s),
            WsError::AuthFailed => write!(f, "WS auth rejected (bad/expired session token)"),
            WsError::SessionCreate(s) => write!(f, "session.create failed: {}", s),
            WsError::Protocol(s) => write!(f, "WS protocol error: {}", s),
            WsError::BackendError(s) => write!(f, "backend error event: {}", s),
            WsError::Timeout => write!(f, "WS turn timed out (1800s)"),
            WsError::Stream(s) => write!(f, "WS stream error: {}", s),
        }
    }
}

impl std::error::Error for WsError {}

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
        assert!(s.contains("WS connect failed"));
        assert!(s.contains("refused"));
    }

    #[test]
    fn ws_error_backend_error_carries_message() {
        let err = WsError::BackendError("model overloaded".to_string());
        assert!(matches!(err, WsError::BackendError(msg) if msg == "model overloaded"));
    }
}
