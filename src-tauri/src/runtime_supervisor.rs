//! Runtime ownership and lifecycle supervision for gateway WebSocket clients.

use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex as StdMutex,
    },
    time::{Duration, Instant},
};

use tokio::{
    sync::{Mutex, Notify},
    task::JoinHandle,
};

use crate::{
    session_registry::{RuntimeKey, SessionRegistry},
    ws_transport::{
        ConnectionState, EmitFn, EndpointSnapshot, GatewayClient, HealthStatus, WsCommand, WsError,
    },
};

const INITIAL_RECONNECT_DELAY: Duration = Duration::from_secs(1);
const MAX_RECONNECT_DELAY: Duration = Duration::from_secs(30);

/// Owns one runtime connection and the task that keeps it alive.
///
/// `lifecycle_lock` makes externally visible lifecycle transitions atomic. The
/// endpoint revision is still checked immediately before connecting so a retry
/// can never intentionally use a snapshot superseded while it was preparing.
#[derive(Clone)]
pub struct RuntimeSupervisor {
    client: Arc<GatewayClient>,
    runtime_key: RuntimeKey,
    endpoint: Arc<Mutex<Option<EndpointSnapshot>>>,
    endpoint_revision: Arc<AtomicU64>,
    sessions: Option<Arc<SessionRegistry>>,
    reconnect_delay: Arc<Mutex<Duration>>,
    max_reconnect_delay: Duration,
    last_connected: Arc<Mutex<Option<Instant>>>,
    degraded_count: Arc<Mutex<u64>>,
    emit_fn: Arc<Mutex<EmitFn>>,
    shutdown: Arc<AtomicBool>,
    shutdown_notify: Arc<Notify>,
    lifecycle_lock: Arc<Mutex<()>>,
    background_task: Arc<StdMutex<Option<JoinHandle<()>>>>,
}

impl RuntimeSupervisor {
    pub fn new(runtime_key: RuntimeKey, sessions: Option<Arc<SessionRegistry>>) -> Self {
        Self {
            client: Arc::new(GatewayClient::new(runtime_key.clone(), "", None)),
            runtime_key,
            endpoint: Arc::new(Mutex::new(None)),
            endpoint_revision: Arc::new(AtomicU64::new(0)),
            sessions,
            reconnect_delay: Arc::new(Mutex::new(INITIAL_RECONNECT_DELAY)),
            max_reconnect_delay: MAX_RECONNECT_DELAY,
            last_connected: Arc::new(Mutex::new(None)),
            degraded_count: Arc::new(Mutex::new(0)),
            // The loop is allowed to exist before the first UI emitter arrives.
            emit_fn: Arc::new(Mutex::new(noop_emitter())),
            shutdown: Arc::new(AtomicBool::new(false)),
            shutdown_notify: Arc::new(Notify::new()),
            lifecycle_lock: Arc::new(Mutex::new(())),
            background_task: Arc::new(StdMutex::new(None)),
        }
    }

    /// Configure the runtime and make one immediate connection attempt.
    /// The reconnect loop has already been started when this returns, including
    /// when the immediate attempt fails.
    pub async fn start(&self, endpoint: EndpointSnapshot, emit_fn: EmitFn) -> Result<(), WsError> {
        let _lifecycle = self.lifecycle_lock.lock().await;
        self.shutdown.store(false, Ordering::Release);
        *self.emit_fn.lock().await = emit_fn;
        self.store_endpoint(endpoint).await;
        self.spawn_background_reconnect();
        self.connect_current().await
    }

    pub async fn update_endpoint(&self, endpoint: EndpointSnapshot) -> Result<(), WsError> {
        let _lifecycle = self.lifecycle_lock.lock().await;
        self.store_endpoint(endpoint).await;
        // Endpoint changes deliberately rotate a healthy socket too.
        self.shutdown_current_connection().await;
        self.connect_current().await
    }

    pub async fn health_check(&self) -> HealthStatus {
        let runtime = self.client.runtime.lock().await;
        match runtime.state {
            ConnectionState::Connected => HealthStatus::Connected,
            ConnectionState::Degraded => {
                let reason = "connection reconciliation is degraded".to_string();
                drop(runtime);
                *self.degraded_count.lock().await += 1;
                HealthStatus::Degraded { reason }
            }
            ConnectionState::Connecting => HealthStatus::Degraded {
                reason: "connection is in progress".to_string(),
            },
            ConnectionState::Disconnected => HealthStatus::Disconnected {
                reason: "connection is disconnected".to_string(),
            },
        }
    }

    /// Retry using the normal reconnect backoff.
    pub async fn reconnect(&self, emit_fn: EmitFn) -> Result<(), WsError> {
        {
            let _lifecycle = self.lifecycle_lock.lock().await;
            *self.emit_fn.lock().await = emit_fn;
        }
        // Do not hold lifecycle_lock during backoff: endpoint updates must be
        // able to supersede this attempt. The lock is reacquired for the actual
        // connection transition below.
        let delay = *self.reconnect_delay.lock().await;
        tokio::select! {
            _ = tokio::time::sleep(delay) => {}
            _ = self.shutdown_notify.notified() => {
                return Err(WsError::Protocol("runtime supervisor is stopped".into()));
            }
        }
        if self.shutdown.load(Ordering::Acquire) {
            return Err(WsError::Protocol("runtime supervisor is stopped".into()));
        }
        let _lifecycle = self.lifecycle_lock.lock().await;
        self.connect_current().await
    }

    /// Tear down even a healthy connection and reconnect immediately.
    pub async fn force_reconnect(&self) -> Result<(), WsError> {
        let _lifecycle = self.lifecycle_lock.lock().await;
        self.shutdown_current_connection().await;
        self.connect_current().await
    }

    /// Stop the reader and the supervisor loop, then leave the instance ready
    /// for a later `start` call.
    pub async fn stop(&self) {
        let task = {
            let _lifecycle = self.lifecycle_lock.lock().await;
            self.shutdown.store(true, Ordering::Release);
            self.shutdown_notify.notify_waiters();
            self.shutdown_current_connection().await;
            self.background_task
                .lock()
                .expect("background task mutex poisoned")
                .take()
        };

        if let Some(task) = task {
            if let Err(error) = task.await {
                tracing::warn!(runtime = ?self.runtime_key, %error, "runtime reconnect task failed");
            }
        }

        *self.reconnect_delay.lock().await = INITIAL_RECONNECT_DELAY;
        *self.last_connected.lock().await = None;
        self.shutdown.store(false, Ordering::Release);
    }

    /// Compatibility shim for existing callers. `start` starts this itself.
    pub fn spawn_background_reconnect(&self) {
        let mut task = self
            .background_task
            .lock()
            .expect("background task mutex poisoned");
        if task.is_some() {
            return;
        }

        let supervisor = self.clone();
        *task = Some(tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(1)) => {}
                    _ = supervisor.shutdown_notify.notified() => break,
                }
                if supervisor.shutdown.load(Ordering::Acquire) {
                    break;
                }
                if matches!(
                    supervisor.health_check().await,
                    HealthStatus::Disconnected { .. }
                ) {
                    tracing::info!(runtime = ?supervisor.runtime_key, "attempting runtime reconnect");
                    let emit_fn = supervisor.emit_fn.lock().await.clone();
                    if let Err(error) = supervisor.reconnect(emit_fn).await {
                        if !supervisor.shutdown.load(Ordering::Acquire) {
                            tracing::warn!(runtime = ?supervisor.runtime_key, %error, "runtime reconnect failed");
                        }
                    }
                }
            }
        }));
    }

    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Release);
        self.shutdown_notify.notify_waiters();
    }

    pub fn client(&self) -> &Arc<GatewayClient> {
        &self.client
    }
    pub fn runtime_key(&self) -> RuntimeKey {
        self.runtime_key.clone()
    }

    async fn store_endpoint(&self, endpoint: EndpointSnapshot) {
        assert_eq!(
            endpoint.runtime_key, self.runtime_key,
            "endpoint belongs to another runtime"
        );
        *self.endpoint.lock().await = Some(endpoint);
        self.endpoint_revision.fetch_add(1, Ordering::AcqRel);
    }

    /// Connect the current endpoint.  If an endpoint update wins the race with
    /// preparation, discard the old snapshot and restart from the new one.
    async fn connect_current(&self) -> Result<(), WsError> {
        loop {
            let revision = self.endpoint_revision.load(Ordering::Acquire);
            let endpoint = self.endpoint.lock().await.clone().ok_or_else(|| {
                WsError::Protocol("cannot reconnect before an endpoint is configured".into())
            })?;
            self.client.configure_snapshot(endpoint).await;
            if self.endpoint_revision.load(Ordering::Acquire) != revision {
                continue;
            }

            let emit_fn = self.emit_fn.lock().await.clone();
            let result = self
                .client
                .ensure_connected(emit_fn, self.sessions.clone())
                .await;
            if self.endpoint_revision.load(Ordering::Acquire) != revision {
                self.shutdown_current_connection().await;
                continue;
            }
            return self.record_connect_result(result).await;
        }
    }

    async fn record_connect_result(&self, result: Result<(), WsError>) -> Result<(), WsError> {
        match result {
            Ok(()) => {
                *self.reconnect_delay.lock().await = INITIAL_RECONNECT_DELAY;
                *self.last_connected.lock().await = Some(Instant::now());
                Ok(())
            }
            Err(error) => {
                let mut delay = self.reconnect_delay.lock().await;
                *delay = (*delay * 2).min(self.max_reconnect_delay);
                Err(error)
            }
        }
    }

    async fn shutdown_current_connection(&self) {
        let shutdown_tx = {
            let mut runtime = self.client.runtime.lock().await;
            runtime.state = ConnectionState::Disconnected;
            runtime.generation = self.client.generation.fetch_add(1, Ordering::Release) + 1;
            runtime.cmd_tx.take()
        };
        if let Some(tx) = shutdown_tx {
            let _ = tx.send(WsCommand::Shutdown).await;
        }
    }
}

fn noop_emitter() -> EmitFn {
    Arc::new(|_| {})
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ws_transport::EndpointIdentity;
    use futures::{SinkExt, StreamExt};
    use serde_json::{json, Value};
    use tokio::{net::TcpListener, sync::oneshot};
    use tokio_tungstenite::tungstenite::Message;

    fn endpoint(url: &str) -> EndpointSnapshot {
        EndpointSnapshot {
            ws_url: url.into(),
            identity: EndpointIdentity::from_ws_url(url, None, None),
            runtime_key: RuntimeKey::Remote("test".into()),
        }
    }

    async fn start_mock_backend() -> (String, oneshot::Receiver<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!(
            "ws://127.0.0.1:{}/api/ws",
            listener.local_addr().unwrap().port()
        );
        let (closed_tx, closed_rx) = oneshot::channel();
        let closed_tx = Arc::new(StdMutex::new(Some(closed_tx)));
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let closed_tx = Arc::clone(&closed_tx);
                tokio::spawn(async move {
                    let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
                    ws.send(Message::Text(json!({"jsonrpc":"2.0","method":"event","params":{"type":"gateway.ready","payload":{}}}).to_string())).await.unwrap();
                    while let Some(Ok(Message::Text(text))) = ws.next().await {
                        let request: Value = serde_json::from_str(&text).unwrap();
                        let id = request["id"].as_u64().unwrap_or_default();
                        let result = match request["method"].as_str() {
                            Some("session.create") => {
                                json!({"session_id":"probe","stored_session_id":"probe","message_count":0,"messages":[],"info":{"desktop_contract":4}})
                            }
                            Some("session.close") => json!({}),
                            _ => continue,
                        };
                        ws.send(Message::Text(
                            json!({"jsonrpc":"2.0","id":id,"result":result}).to_string(),
                        ))
                        .await
                        .unwrap();
                    }
                    if let Some(tx) = closed_tx.lock().unwrap().take() {
                        let _ = tx.send(());
                    }
                });
            }
        });
        (url, closed_rx)
    }

    async fn wait_for_connected(supervisor: &RuntimeSupervisor) {
        tokio::time::timeout(Duration::from_secs(4), async {
            loop {
                if supervisor.health_check().await == HealthStatus::Connected {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("supervisor did not reconnect");
    }

    #[tokio::test]
    async fn supervisor_creates_disconnected_client_and_reports_health() {
        let supervisor = RuntimeSupervisor::new(RuntimeKey::Remote("test".into()), None);
        assert!(matches!(
            supervisor.health_check().await,
            HealthStatus::Disconnected { .. }
        ));
        assert_eq!(supervisor.runtime_key(), RuntimeKey::Remote("test".into()));
    }

    #[tokio::test]
    async fn failed_start_is_retried_by_background_loop() {
        let supervisor = RuntimeSupervisor::new(RuntimeKey::Remote("test".into()), None);
        let reserved = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!(
            "ws://127.0.0.1:{}/api/ws",
            reserved.local_addr().unwrap().port()
        );
        drop(reserved);
        assert!(supervisor
            .start(endpoint(&url), noop_emitter())
            .await
            .is_err());
        assert!(supervisor.background_task.lock().unwrap().is_some());
        // Publishing a server on the same endpoint lets the already-running
        // loop, rather than a second start call, make the successful retry.
        let listener =
            TcpListener::bind(url.trim_start_matches("ws://").split('/').next().unwrap())
                .await
                .unwrap();
        let (closed_tx, closed_rx) = oneshot::channel();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            ws.send(Message::Text(json!({"jsonrpc":"2.0","method":"event","params":{"type":"gateway.ready","payload":{}}}).to_string())).await.unwrap();
            while let Some(Ok(Message::Text(text))) = ws.next().await {
                let request: Value = serde_json::from_str(&text).unwrap();
                let id = request["id"].as_u64().unwrap_or_default();
                if request["method"] == "session.create" {
                    ws.send(Message::Text(json!({"jsonrpc":"2.0","id":id,"result":{"session_id":"probe","stored_session_id":"probe","message_count":0,"messages":[],"info":{"desktop_contract":4}}}).to_string())).await.unwrap();
                } else if request["method"] == "session.close" {
                    ws.send(Message::Text(
                        json!({"jsonrpc":"2.0","id":id,"result":{}}).to_string(),
                    ))
                    .await
                    .unwrap();
                }
            }
            let _ = closed_tx.send(());
        });
        wait_for_connected(&supervisor).await;
        supervisor.stop().await;
        let _ = closed_rx.await;
    }

    #[tokio::test]
    async fn endpoint_update_replaces_the_client_endpoint() {
        let supervisor = RuntimeSupervisor::new(RuntimeKey::Remote("test".into()), None);
        let updated = endpoint("ws://127.0.0.1:1/updated");
        assert!(supervisor.update_endpoint(updated.clone()).await.is_err());
        let configured = supervisor.client.endpoint.read().await;
        assert_eq!(configured.identity(), updated.identity);
        assert_eq!(configured.ws_url(), updated.ws_url);
    }

    #[tokio::test]
    async fn endpoint_update_during_reconnect_uses_the_new_revision() {
        let supervisor = RuntimeSupervisor::new(RuntimeKey::Remote("test".into()), None);
        assert!(supervisor
            .start(endpoint("ws://127.0.0.1:1/api/ws"), noop_emitter())
            .await
            .is_err());
        let retry = tokio::spawn({
            let supervisor = supervisor.clone();
            async move { supervisor.reconnect(noop_emitter()).await }
        });
        tokio::time::sleep(Duration::from_millis(20)).await;
        let (url, _closed) = start_mock_backend().await;
        supervisor.update_endpoint(endpoint(&url)).await.unwrap();
        assert_eq!(supervisor.endpoint_revision.load(Ordering::Acquire), 2);
        wait_for_connected(&supervisor).await;
        retry.await.unwrap().unwrap();
        supervisor.stop().await;
    }

    #[tokio::test]
    async fn force_reconnect_rotates_a_connected_socket() {
        let supervisor = RuntimeSupervisor::new(RuntimeKey::Remote("test".into()), None);
        let (url, _closed) = start_mock_backend().await;
        supervisor
            .start(endpoint(&url), noop_emitter())
            .await
            .unwrap();
        supervisor.force_reconnect().await.unwrap();
        assert_eq!(supervisor.health_check().await, HealthStatus::Connected);
        supervisor.stop().await;
    }

    #[tokio::test]
    async fn stop_sends_shutdown_waits_and_allows_restart() {
        let supervisor = RuntimeSupervisor::new(RuntimeKey::Remote("test".into()), None);
        let (url, reader_closed) = start_mock_backend().await;
        supervisor
            .start(endpoint(&url), noop_emitter())
            .await
            .unwrap();
        supervisor.stop().await;
        tokio::time::timeout(Duration::from_secs(1), reader_closed)
            .await
            .expect("reader did not receive shutdown")
            .unwrap();
        assert!(supervisor.background_task.lock().unwrap().is_none());
        assert!(supervisor
            .start(endpoint(&url), noop_emitter())
            .await
            .is_ok());
        assert_eq!(supervisor.health_check().await, HealthStatus::Connected);
        assert!(supervisor.background_task.lock().unwrap().is_some());
        supervisor.stop().await;
    }
}
