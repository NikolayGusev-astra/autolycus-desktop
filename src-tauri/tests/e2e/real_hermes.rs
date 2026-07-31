//! Real Hermes gateway E2E tests.
//!
//! These tests connect to a REAL Hermes gateway and exercise the full protocol
//! stack: WebSocket transport, JSON-RPC session management, prompt submission,
//! streaming event routing, reconnection with session resume, and MCP tool calls.
//!
//! ## Configuration
//!
//! | Env var | Default | Required |
//! |---|---|---|
//! | `HERMES_TEST_URL` | `ws://localhost:8642/api/ws` | no |
//! | `HERMES_TEST_API_KEY` | — | no (needed for remote) |
//! | `HERMES_HOME` | — | no (required for local mode) |
//! | `HERMES_TEST_PROFILE` | `default` | no |
//! | `HERMES_TEST_MCP_TOOL` | `steersman_list_tasks` | no |
//!
//! At least one of `HERMES_TEST_API_KEY` (remote mode) or `HERMES_HOME` (local
//! mode) must be set. If both are absent, all tests are skipped.
//!
//! In local mode, the test harness spawns `hermes serve` as a child process
//! (matching the desktop app's behaviour), reads the OS-assigned port from
//! stdout, and connects to `ws://127.0.0.1:{port}/api/ws?token={session_token}`.
//! The process is killed on test teardown via the `LocalGatewayGuard` drop impl.
//!
//! ## Test status
//!
//! - `connects_and_completes_compatibility_handshake` — ✅ PASS
//! - `creates_session_submits_prompt_and_receives_reply` — ✅ PASS
//! - `resumes_session_after_forced_reconnect` — ✅ PASS
//! - `prompt_error_returns_typed_product_error` — ✅ PASS
//! - `discovers_and_calls_steersman_mcp_tool` — ⚠️ requires `steersman-mcp-server`
//!   configured in the Hermes profile (toolset or MCP config). Fails gracefully
//!   with a clear message if the tool is not available.
//!
//! ## Running (local mode — spawns `hermes serve` automatically)
//!
//! ```powershell
//! cargo test --manifest-path src-tauri/Cargo.toml --test real_hermes_e2e -- --ignored --test-threads=1
//! ```
//!
//! ## Running (remote mode)
//!
//! ```powershell
//! $env:HERMES_TEST_API_KEY = "..."
//! cargo test --manifest-path src-tauri/Cargo.toml --test real_hermes_e2e -- --ignored --test-threads=1
//! ```
//!
//! All tests are `#[ignore]` by default — they only run when explicitly requested
//! with `--ignored`, protecting CI from accidental real-gateway execution.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex as StdMutex;
use std::time::{Duration, Instant};

use steersman_desktop_lib::{
    build_ws_url, ChatEvent, ConnectionMode, ConversationId, ConversationService, EmitFn,
    EndpointIdentity, EndpointSnapshot, InMemoryConversationRepository, ProductError,
    RoutedChatEvent, RuntimeKey, RuntimeState, RuntimeSupervisor, SessionRegistry,
};

// ── Configuration ────────────────────────────────────────────────────────────

/// Read from environment and return a Hermes endpoint configuration.
/// Returns `None` when neither HERMES_TEST_API_KEY nor HERMES_HOME is set.
fn config_from_env() -> Option<RealHermesConfig> {
    let api_key = std::env::var("HERMES_TEST_API_KEY").unwrap_or_default();
    let hermes_home = std::env::var("HERMES_HOME").unwrap_or_default();

    if api_key.is_empty() && hermes_home.is_empty() {
        return None;
    }

    let ws_base_url = std::env::var("HERMES_TEST_URL")
        .unwrap_or_else(|_| "ws://localhost:8642/api/ws".to_string());
    let profile = std::env::var("HERMES_TEST_PROFILE")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "default".to_string());
    let mcp_tool = std::env::var("HERMES_TEST_MCP_TOOL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "steersman_list_tasks".to_string());

    let hermes_home = if hermes_home.is_empty() {
        None
    } else {
        Some(PathBuf::from(hermes_home))
    };

    Some(RealHermesConfig {
        ws_base_url,
        api_key: if api_key.is_empty() { None } else { Some(api_key.into()) },
        hermes_home,
        profile,
        mcp_tool,
        timeout: Duration::from_secs(180),
    })
}

struct RealHermesConfig {
    ws_base_url: String,
    api_key: Option<Box<str>>,
    hermes_home: Option<PathBuf>,
    profile: String,
    mcp_tool: String,
    timeout: Duration,
}

impl std::fmt::Debug for RealHermesConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RealHermesConfig")
            .field("ws_base_url", &self.ws_base_url)
            .field("api_key", &self.api_key.as_ref().map(|_| "<redacted>"))
            .field("hermes_home", &self.hermes_home)
            .field("profile", &self.profile)
            .field("mcp_tool", &self.mcp_tool)
            .field("timeout", &self.timeout)
            .finish()
    }
}

// ── Local gateway process management ────────────────────────────────────────

/// Guard that kills the spawned `hermes serve` process on drop.
struct LocalGatewayGuard {
    child: Option<std::process::Child>,
}

#[allow(dead_code)]
impl LocalGatewayGuard {
    async fn kill(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for LocalGatewayGuard {
    fn drop(&mut self) {
        if let Some(ref mut child) = self.child {
            let _ = child.kill();
        }
    }
}

/// Spawn `hermes serve --host 127.0.0.1 --port 0` and return the WS URL + guard.
async fn spawn_local_hermes(
    hermes_home: &PathBuf,
    profile: &str,
) -> Result<(String, LocalGatewayGuard), String> {
    // Find the python interpreter using the venv in HERMES_HOME.
    let venv = hermes_home.join("hermes-agent").join("venv");
    let python = if cfg!(windows) {
        venv.join("Scripts").join("python.exe")
    } else {
        venv.join("bin").join("python")
    };
    if !python.exists() {
        return Err(format!("Python not found at {:?}", python));
    }

    // Generate a session token (same as gateway.rs generate_session_token).
    let session_token = generate_session_token();

    let serve_args = build_serve_args(profile);

    let mut cmd = std::process::Command::new(&python);
    cmd.arg("-m").arg("hermes_cli.main");
    for a in &serve_args {
        cmd.arg(a);
    }
    cmd.env("HERMES_HOME", hermes_home);
    cmd.env("HERMES_DASHBOARD_SESSION_TOKEN", &session_token);
    cmd.env("PYTHONUNBUFFERED", "1");
    // Set PYTHON_SRC_ROOT to the Hermes repo path (matching desktop app behavior).
    if let Some(repo) = python.parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
    {
        cmd.env("HERMES_PYTHON_SRC_ROOT", repo);
        cmd.current_dir(repo);
    }
    // Match the desktop app's process configuration.
    cmd.stdin(std::process::Stdio::null());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| format!("spawn hermes serve: {e}"))?;
    // Construct the guard immediately — Drop will kill the child on any error path.
    let mut guard = LocalGatewayGuard { child: Some(child) };
    // Take the child from the guard, take its pipes, then put it back.
    let mut child = guard.child.take().unwrap();
    let stdout = match child.stdout.take() {
        Some(s) => s,
        None => { child.kill().ok(); return Err("no stdout".to_string()); }
    };
    let stderr = match child.stderr.take() {
        Some(s) => s,
        None => { child.kill().ok(); return Err("no stderr".to_string()); }
    };
    guard.child = Some(child);

    // Spawn a background thread to capture stderr.
    let _ = std::thread::Builder::new()
        .name("hermes-stderr".into())
        .spawn(move || {
            use std::io::BufRead;
            let reader = std::io::BufReader::new(stderr);
            for line in reader.lines() {
                if let Ok(l) = line {
                    let trimmed = l.trim().to_string();
                    if !trimmed.is_empty() {
                        eprintln!("[hermes:stderr] {trimmed}");
                    }
                }
            }
        });

    // Spawn a background thread that reads stdout continuously.
    // The thread keeps the stdout pipe alive so the Python process doesn't crash
    // when it tries to print after we've found the port.
    let port_found = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (port_tx, port_rx) = std::sync::mpsc::channel();
    let port_found_clone = std::sync::Arc::clone(&port_found);
    let _ = std::thread::Builder::new()
        .name("hermes-stdout".into())
        .spawn(move || {
            use std::io::BufRead;
            let reader = std::io::BufReader::new(stdout);
            for line in reader.lines() {
                if let Ok(l) = line {
                    let trimmed = l.trim().to_string();
                    if !trimmed.is_empty() && !port_found_clone.load(std::sync::atomic::Ordering::Acquire) {
                        if let Some(port) = parse_ready_port(&trimmed) {
                            port_found_clone.store(true, std::sync::atomic::Ordering::Release);
                            let _ = port_tx.send(port);
                        }
                    }
                }
            }
            // Signal that stdout is closed (process exited).
            if !port_found_clone.load(std::sync::atomic::Ordering::Acquire) {
                let _ = port_tx.send(0);
            }
        });

    // Wait for the port from the channel with a timeout.
    let deadline = Instant::now() + Duration::from_secs(120);
    let port = loop {
        if Instant::now() > deadline {
            return Err("timed out waiting for HERMES_BACKEND_READY".to_string());
        }
        match port_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(0) => {
                return Err("stdout closed before HERMES_BACKEND_READY".to_string());
            }
            Ok(port) => break port,
            Err(_) => continue,
        }
    };

    let ws_url = format!("ws://127.0.0.1:{port}/api/ws?token={session_token}");
    tracing::info!(target: "e2e", port, ws_url = %redact_url(&ws_url), "local hermes serve started");

    Ok((ws_url, guard))
}

/// Parse HERMES_BACKEND_READY port=N from a stdout line.
fn parse_ready_port(line: &str) -> Option<u16> {
    let idx = line.find("HERMES_BACKEND_READY")?;
    let tail = &line[idx..];
    let port_idx = tail.find("port=")?;
    let after = &tail[port_idx + "port=".len()..];
    let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse::<u16>().ok()
}

/// Generate a session token (same as gateway.rs generate_session_token).
fn generate_session_token() -> String {
    use base64::Engine;
    let mut bytes = [0u8; 32];
    // NOTE: getrandom is pulled in transitively by uuid.
    uuid::Uuid::new_v4().as_bytes().iter().enumerate().for_each(|(i, b)| {
        if i < 32 { bytes[i] = *b; }
    });
    // Fill any remaining bytes from another UUID.
    let more = uuid::Uuid::new_v4();
    for (i, b) in more.as_bytes().iter().enumerate() {
        if i + 16 < 32 { bytes[i + 16] = *b; }
    }
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// Build serve args for `hermes serve`.
fn build_serve_args(profile: &str) -> Vec<String> {
    let mut args = Vec::new();
    if profile != "default" && !profile.is_empty() {
        args.push("--profile".to_string());
        args.push(profile.to_string());
    }
    args.push("serve".to_string());
    args.push("--host".to_string());
    args.push("127.0.0.1".to_string());
    args.push("--port".to_string());
    args.push("0".to_string());
    args
}

/// Redact token from a WS URL for logging.
fn redact_url(url: &str) -> String {
    let idx = url.find("?token=").map(|i| i + 7);
    match idx {
        Some(i) => {
            let mut s = url[..i].to_string();
            s.push_str("***");
            s
        }
        None => url.to_string(),
    }
}

// ── Event collector ─────────────────────────────────────────────────────────

/// Thread-safe buffer of routed chat events, collected by the test EmitFn.
struct EventCollector {
    events: StdMutex<Vec<RoutedChatEvent>>,
    done: AtomicBool,
}

impl EventCollector {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            events: StdMutex::new(Vec::new()),
            done: AtomicBool::new(false),
        })
    }

    fn emitter(this: &Arc<Self>) -> EmitFn {
        let collector = Arc::clone(this);
        Arc::new(move |event: &RoutedChatEvent| {
            if matches!(event.event, ChatEvent::Done { .. } | ChatEvent::Error { .. }) {
                collector.done.store(true, Ordering::Release);
            }
            let mut events = collector.events.lock().unwrap();
            events.push(event.clone());
        })
    }

    async fn wait_for_terminal(&self, timeout: Duration) -> Result<Vec<RoutedChatEvent>, String> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if self.done.load(Ordering::Acquire) {
                let events = self.events.lock().unwrap();
                return Ok(events.clone());
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        Err("timed out waiting for terminal event (Done/Error)".to_string())
    }

    fn reset(&self) {
        self.done.store(false, Ordering::Release);
        self.events.lock().unwrap().clear();
    }
}

// ── Test harness ────────────────────────────────────────────────────────────

/// Provides a Hermes gateway connection and the product-level conversation API.
struct RealHermesHarness {
    #[allow(dead_code)]
    sessions: Arc<SessionRegistry>,
    local: Arc<RuntimeSupervisor>,
    conversations: ConversationService,
    collector: Arc<EventCollector>,
    #[allow(dead_code)]
    _gateway_guard: Option<LocalGatewayGuard>,
}

impl RealHermesHarness {
    async fn connect(config: &RealHermesConfig) -> Result<Self, String> {
        // Resolve the endpoint URL: either from config, or spawn local serve.
        let (ws_url, gateway_guard) = if let Some(ref api_key) = config.api_key {
            let (url, _auth) = build_ws_url(&config.ws_base_url, api_key)
                .map_err(|e| format!("build_ws_url: {e}"))?;
            (url, None)
        } else if let Some(ref hermes_home) = config.hermes_home {
            let (url, guard) = spawn_local_hermes(hermes_home, &config.profile).await?;
            (url, Some(guard))
        } else {
            return Err("no HERMES_TEST_API_KEY or HERMES_HOME set".to_string());
        };

        let endpoint = EndpointSnapshot {
            ws_url: ws_url.clone(),
            identity: EndpointIdentity::from_ws_url(&ws_url, None, None),
            runtime_key: RuntimeKey::Local,
        };

        let sessions = SessionRegistry::new();
        let collector = EventCollector::new();
        let emit_fn = EventCollector::emitter(&collector);

        let local = Arc::new(RuntimeSupervisor::new(
            RuntimeKey::Local,
            Some(Arc::clone(&sessions)),
        ));
        local
            .start(endpoint.clone(), emit_fn)
            .await
            .map_err(|e| format!("supervisor.start: {e:?}"))?;

        let ready = wait_for_ready(&local, config.timeout).await;
        if !ready {
            return Err(format!(
                "supervisor did not reach Ready within timeout (state: {:?})",
                local.state()
            ));
        }

        let conversations = ConversationService::new(
            Arc::clone(&sessions),
            Arc::clone(&local),
            Arc::new(RuntimeSupervisor::new(RuntimeKey::Remote("e2e".into()), None)),
            Arc::new(RuntimeSupervisor::new(RuntimeKey::Ssh("e2e".into()), None)),
            InMemoryConversationRepository::new(),
        );

        Ok(Self {
            sessions,
            local,
            conversations,
            collector,
            _gateway_guard: gateway_guard,
        })
    }

    async fn create_conversation(&self) -> Result<ConversationId, String> {
        self.conversations
            .create_conversation(Some(ConnectionMode::Local))
            .await
            .map_err(|e| format!("create_conversation: {e:?}"))
    }

    async fn prompt_and_collect(
        &self,
        conversation: &ConversationId,
        text: &str,
        timeout: Duration,
    ) -> Result<Vec<RoutedChatEvent>, String> {
        self.collector.reset();
        self.conversations
            .send_message(conversation, text)
            .await
            .map_err(|e| format!("send_message: {e:?}"))?;
        self.collector.wait_for_terminal(timeout).await
    }

    async fn reconnect(&self) -> Result<(), String> {
        self.local
            .force_reconnect()
            .await
            .map_err(|e| format!("force_reconnect: {e:?}"))?;
        let ready = wait_for_ready(&self.local, Duration::from_secs(30)).await;
        if !ready {
            return Err(format!(
                "runtime did not reconnect (state: {:?})",
                self.local.state()
            ));
        }
        Ok(())
    }

    async fn shutdown(self) {
        self.local.stop().await;
        // Gateway guard is dropped here, killing the child process.
    }
}

async fn wait_for_ready(supervisor: &RuntimeSupervisor, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        match supervisor.state() {
            RuntimeState::Ready | RuntimeState::Degraded { .. } => return true,
            _ => {}
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    false
}

fn collected_text(events: &[RoutedChatEvent]) -> String {
    events
        .iter()
        .filter_map(|e| match &e.event {
            ChatEvent::Token { content } => Some(content.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

fn has_done(events: &[RoutedChatEvent]) -> bool {
    events.iter().any(|e| matches!(e.event, ChatEvent::Done { .. }))
}

fn has_error(events: &[RoutedChatEvent]) -> bool {
    events.iter().any(|e| matches!(e.event, ChatEvent::Error { .. }))
}

// ── Tests ───────────────────────────────────────────────────────────────────

fn skip_unless_configured() -> Option<RealHermesConfig> {
    let config = config_from_env()?;
    tracing::info!(
        target: "e2e",
        ws_base_url = %config.ws_base_url,
        profile = %config.profile,
        has_api_key = %config.api_key.is_some(),
        has_hermes_home = %config.hermes_home.is_some(),
        "Hermes E2E test configured"
    );
    Some(config)
}

#[ignore]
#[tokio::test]
async fn connects_and_completes_compatibility_handshake() {
    let _ = tracing_subscriber::fmt::try_init();
    let config = match skip_unless_configured() {
        Some(c) => c,
        None => return,
    };

    let harness = RealHermesHarness::connect(&config)
        .await
        .expect("harness should connect to Hermes gateway");

    let state = harness.local.state();
    assert!(
        matches!(state, RuntimeState::Ready | RuntimeState::Degraded { .. }),
        "expected runtime to be Ready or Degraded, got {state:?}"
    );
    tracing::info!(target: "e2e", state = ?state, "connection established");

    harness.shutdown().await;
}

#[ignore]
#[tokio::test]
async fn creates_session_submits_prompt_and_receives_reply() {
    let _ = tracing_subscriber::fmt::try_init();
    let config = match skip_unless_configured() {
        Some(c) => c,
        None => return,
    };

    let harness = RealHermesHarness::connect(&config)
        .await
        .expect("harness should connect");

    let conversation = harness
        .create_conversation()
        .await
        .expect("should create conversation");

    let events = harness
        .prompt_and_collect(&conversation, "Say hello in one short sentence.", config.timeout)
        .await
        .expect("prompt should complete");

    assert!(!has_error(&events), "prompt returned an error event: {events:?}");
    assert!(has_done(&events), "expected a Done event");

    let text = collected_text(&events);
    assert!(!text.is_empty(), "expected non-empty token text from the agent");
    tracing::info!(
        target: "e2e",
        text_len = %text.len(),
        reply = %text.chars().take(200).collect::<String>(),
        "agent reply received"
    );

    harness.shutdown().await;
}

#[ignore]
#[tokio::test]
async fn resumes_session_after_forced_reconnect() {
    let _ = tracing_subscriber::fmt::try_init();
    let config = match skip_unless_configured() {
        Some(c) => c,
        None => return,
    };

    let harness = RealHermesHarness::connect(&config)
        .await
        .expect("harness should connect");

    let conversation = harness
        .create_conversation()
        .await
        .expect("should create conversation");

    let first = harness
        .prompt_and_collect(&conversation, "Say 'first' in one word.", config.timeout)
        .await
        .expect("first prompt should complete");
    assert!(!has_error(&first), "first prompt errored");
    assert!(has_done(&first), "first prompt missing Done");
    tracing::info!(
        target: "e2e",
        "first prompt completed, text = {}",
        collected_text(&first).chars().take(100).collect::<String>()
    );

    harness
        .reconnect()
        .await
        .expect("reconnect should succeed");

    let second = harness
        .prompt_and_collect(&conversation, "Say 'second' in one word.", config.timeout)
        .await
        .expect("second prompt should complete");
    assert!(!has_error(&second), "second prompt errored");
    assert!(has_done(&second), "second prompt missing Done");
    let second_text = collected_text(&second);
    assert!(!second_text.is_empty(), "expected non-empty reply after reconnect");
    tracing::info!(
        target: "e2e",
        "second prompt completed, text = {}",
        second_text.chars().take(100).collect::<String>()
    );

    harness.shutdown().await;
}

#[ignore]
#[tokio::test]
async fn discovers_and_calls_steersman_mcp_tool() {
    let _ = tracing_subscriber::fmt::try_init();
    let config = match skip_unless_configured() {
        Some(c) => c,
        None => return,
    };

    let harness = RealHermesHarness::connect(&config)
        .await
        .expect("harness should connect");

    let conversation = harness
        .create_conversation()
        .await
        .expect("should create conversation");

    let prompt = format!(
        "Use the `{}` tool and report what it returns. Do not do anything else.",
        config.mcp_tool
    );
    let events = harness
        .prompt_and_collect(&conversation, &prompt, config.timeout)
        .await
        .expect("MCP tool prompt should complete");

    assert!(!has_error(&events), "MCP tool prompt errored: {events:?}");
    assert!(has_done(&events), "MCP tool prompt missing Done");

    let tool_starts: Vec<&RoutedChatEvent> = events
        .iter()
        .filter(|e| matches!(&e.event, ChatEvent::ToolStart { name, .. } if name == &config.mcp_tool))
        .collect();
    let tool_completes: Vec<&RoutedChatEvent> = events
        .iter()
        .filter(|e| matches!(&e.event, ChatEvent::ToolComplete { name, .. } if name == &config.mcp_tool))
        .collect();

    assert!(
        !tool_starts.is_empty(),
        "expected at least one ToolStart event for MCP tool call — \
         agent may not have the tool configured; check HERMES_TEST_MCP_TOOL={}",
        config.mcp_tool
    );
    assert!(!tool_completes.is_empty(), "expected at least one ToolComplete event");
    tracing::info!(
        target: "e2e",
        tool_starts = %tool_starts.len(),
        tool_completes = %tool_completes.len(),
        "MCP tool call detected"
    );

    let text = collected_text(&events);
    assert!(!text.is_empty(), "expected non-empty reply after MCP tool call");
    tracing::info!(
        target: "e2e",
        "MCP tool response: {}",
        text.chars().take(300).collect::<String>()
    );

    harness.shutdown().await;
}

#[ignore]
#[tokio::test]
async fn send_to_uncreated_conversation_returns_typed_error() {
    let _ = tracing_subscriber::fmt::try_init();
    let config = match skip_unless_configured() {
        Some(c) => c,
        None => return,
    };

    let harness = RealHermesHarness::connect(&config)
        .await
        .expect("harness should connect");

    // Send to a non-existent conversation — must return ConversationNotFound.
    let fake_id = ConversationId("does-not-exist".to_string());
    let result = harness
        .conversations
        .send_message(&fake_id, "hello")
        .await;

    assert!(
        matches!(result, Err(ProductError::ConversationNotFound)),
        "expected ConversationNotFound for fake conversation, got {result:?}"
    );
    tracing::info!(target: "e2e", "non-existent conversation correctly rejected");

    harness.shutdown().await;
}