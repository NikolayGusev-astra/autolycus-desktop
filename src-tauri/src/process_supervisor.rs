// src-tauri/src/process_supervisor.rs
// Unified async process supervision for gateway / ssh / mcp children.
//
// P1-AUDIT: replaces the previous pattern of `std::process::Command` + OS
// threads + `thread::sleep` polling (which blocked Tokio worker threads and
// made cancellation/stop races hard). All children are now spawned via
// `tokio::process::Command`, streamed via `tokio::io::BufReader`, and readiness
// is awaited with `tokio::time::timeout` + a `CancellationToken`.

use std::time::Duration;

/// Parse `HERMES_BACKEND_READY port=N` (or any `<marker> port=N`) out of a line.
fn parse_port_marker(line: &str, marker: &str) -> Option<u16> {
    let idx = line.find(marker)?;
    let tail = &line[idx..];
    let port_idx = tail.find("port=")?;
    let after = &tail[port_idx + "port=".len()..];
    let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse::<u16>().ok()
}

/// Run a oneshot WS readiness probe (connect, wait up to 5s for gateway.ready).
/// Real implementation uses the existing `ws_transport::check_ws_ready`.
pub async fn probe_ws_ready(ws_url: &str) -> bool {
    use futures::StreamExt;
    use tokio_tungstenite::tungstenite::Message;

    let (mut ws, _resp) = match tokio_tungstenite::connect_async(ws_url).await {
        Ok(p) => p,
        Err(_) => return false,
    };
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        match tokio::time::timeout(remaining, ws.next()).await {
            Ok(Some(Ok(Message::Text(text)))) => {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                    let is_ready = v
                        .get("method")
                        .and_then(|m| m.as_str())
                        .map(|m| m == "event")
                        .unwrap_or(false)
                        && v.get("params")
                            .and_then(|p| p.get("type"))
                            .and_then(|t| t.as_str())
                            .map(|t| t == "gateway.ready")
                            .unwrap_or(false);
                    if is_ready {
                        let _ = ws.close(None).await;
                        return true;
                    }
                }
            }
            Ok(Some(Ok(_))) => continue,
            _ => break,
        }
    }
    let _ = ws.close(None).await;
    false
}
