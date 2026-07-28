//! Runtime ownership and lifecycle supervision for gateway WebSocket clients.

use std::{
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
        Arc, Mutex as StdMutex,
    },
    time::{Duration, Instant},
};

use tokio::{
    process::Child,
    sync::{watch, Mutex, Notify},
    task::JoinHandle,
};

use crate::{
    gateway::GatewayProcess,
    session_registry::{RuntimeKey, SessionRegistry},
    ws_transport::{
        ConnectionState, EmitFn, EndpointSnapshot, GatewayClient, HealthStatus, WsCommand, WsError,
    },
};

const INITIAL_RECONNECT_DELAY: Duration = Duration::from_secs(1);
const MAX_RECONNECT_DELAY: Duration = Duration::from_secs(30);

/// Recreates the runtime-owned resource and returns the endpoint it exposes.
/// Factories also transfer ownership of the new resource to their supervisor
/// before resolving the endpoint.
pub type ResourceFactory = Arc<
    dyn Fn() -> Pin<Box<dyn Future<Output = Result<EndpointSnapshot, RuntimeError>> + Send>>
        + Send
        + Sync,
>;

/// Named alias for callers that describe the factory by its supervisor role.
pub type RuntimeResourceFactory = ResourceFactory;

/// Full lifecycle of a gateway runtime, including process ownership and the
/// connection phases hidden beneath the WebSocket transport.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum RuntimeState {
    Stopped,
    Starting,
    AwaitingGateway,
    CheckingCompatibility,
    Reconciling,
    Ready,
    Degraded {
        reason: String,
    },
    Reconnecting {
        attempt: u32,
        #[serde(skip_serializing)]
        next_retry_at: Instant,
        reason: String,
    },
    Stopping,
    Failed {
        error: RuntimeError,
    },
    Incompatible {
        reason: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "error", rename_all = "snake_case")]
pub enum RuntimeError {
    GatewayStartFailed(String),
    GatewayCrashed(String),
    TunnelFailed(String),
    TunnelCrashed(String),
    CompatibilityRejected(String),
    WebSocket(String),
    ConnectionLost,
    Shutdown,
    Timeout,
}

/// A child process owned by a runtime.  Dropping the guard asks Tokio to kill
/// the child; callers that need deterministic cleanup should use `kill`.
pub struct ChildGuard {
    child: Child,
    local_endpoint: Option<LocalGatewayEndpoint>,
}

#[derive(Clone)]
struct LocalGatewayEndpoint {
    port: u16,
    session_token: String,
}

impl ChildGuard {
    pub fn new(child: Child) -> Self {
        Self {
            child,
            local_endpoint: None,
        }
    }

    fn from_gateway(process: GatewayProcess) -> Self {
        Self {
            child: process.child,
            local_endpoint: Some(LocalGatewayEndpoint {
                port: process.port,
                session_token: process.session_token,
            }),
        }
    }

    async fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    async fn kill(&mut self) {
        if self.is_alive().await {
            let _ = self.child.start_kill();
            let _ = self.child.wait().await;
        }
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

/// Ownership token for an SSH tunnel.  The actual SSH process remains owned
/// by the SSH layer; its monitor completes when that tunnel is no longer
/// usable.  Dropping the guard aborts the monitor.
#[allow(dead_code)]
pub struct SshTunnelGuard {
    #[allow(dead_code)]
    port: u16,
    monitor: Option<JoinHandle<()>>,
    close: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl SshTunnelGuard {
    pub fn new(port: u16, monitor: JoinHandle<()>) -> Self {
        Self {
            port,
            monitor: Some(monitor),
            close: None,
        }
    }

    fn is_alive(&self) -> bool {
        self.monitor
            .as_ref()
            .is_some_and(|monitor| !monitor.is_finished())
    }

    fn close(&mut self) {
        if let Some(monitor) = self.monitor.take() {
            monitor.abort();
        }
        if let Some(close) = self.close.take() {
            close();
        }
    }

    #[allow(dead_code)]
    pub fn port(&self) -> u16 {
        self.port
    }
}

impl Drop for SshTunnelGuard {
    fn drop(&mut self) {
        self.close();
    }
}

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
    reconnect_attempt: Arc<AtomicU32>,
    max_reconnect_delay: Duration,
    last_connected: Arc<Mutex<Option<Instant>>>,
    emit_fn: Arc<Mutex<EmitFn>>,
    current_state: watch::Receiver<RuntimeState>,
    state_tx: watch::Sender<RuntimeState>,
    shutdown: Arc<AtomicBool>,
    shutdown_notify: Arc<Notify>,
    lifecycle_lock: Arc<Mutex<()>>,
    lifecycle_epoch: Arc<AtomicU64>,
    background_task: Arc<StdMutex<Option<JoinHandle<()>>>>,
    resource_factory: Arc<StdMutex<Option<ResourceFactory>>>,
    resource_recovery_failed: Arc<AtomicBool>,
    /// Optional local Hermes process owned by this runtime.
    local_gateway_handle: Arc<Mutex<Option<ChildGuard>>>,
    /// Optional SSH tunnel monitor owned by this runtime.
    ssh_tunnel_handle: Arc<Mutex<Option<SshTunnelGuard>>>,
}

impl RuntimeSupervisor {
    pub fn new(runtime_key: RuntimeKey, sessions: Option<Arc<SessionRegistry>>) -> Self {
        let (state_tx, current_state) = watch::channel(RuntimeState::Stopped);
        Self {
            client: Arc::new(GatewayClient::new(runtime_key.clone(), "", None)),
            runtime_key,
            endpoint: Arc::new(Mutex::new(None)),
            endpoint_revision: Arc::new(AtomicU64::new(0)),
            sessions,
            reconnect_delay: Arc::new(Mutex::new(INITIAL_RECONNECT_DELAY)),
            reconnect_attempt: Arc::new(AtomicU32::new(0)),
            max_reconnect_delay: MAX_RECONNECT_DELAY,
            last_connected: Arc::new(Mutex::new(None)),
            // The loop is allowed to exist before the first UI emitter arrives.
            emit_fn: Arc::new(Mutex::new(noop_emitter())),
            current_state,
            state_tx,
            shutdown: Arc::new(AtomicBool::new(false)),
            shutdown_notify: Arc::new(Notify::new()),
            lifecycle_lock: Arc::new(Mutex::new(())),
            lifecycle_epoch: Arc::new(AtomicU64::new(0)),
            background_task: Arc::new(StdMutex::new(None)),
            resource_factory: Arc::new(StdMutex::new(None)),
            resource_recovery_failed: Arc::new(AtomicBool::new(false)),
            local_gateway_handle: Arc::new(Mutex::new(None)),
            ssh_tunnel_handle: Arc::new(Mutex::new(None)),
        }
    }

    /// Configure the runtime and make one immediate connection attempt.
    /// The reconnect loop has already been started when this returns, including
    /// when the immediate attempt fails.
    pub async fn start(&self, endpoint: EndpointSnapshot, emit_fn: EmitFn) -> Result<(), WsError> {
        let _lifecycle = self.lifecycle_lock.lock().await;
        self.lifecycle_epoch.fetch_add(1, Ordering::AcqRel);
        self.start_locked(endpoint, emit_fn).await
    }

    async fn start_locked(
        &self,
        endpoint: EndpointSnapshot,
        emit_fn: EmitFn,
    ) -> Result<(), WsError> {
        self.transition(RuntimeState::Starting);
        self.shutdown.store(false, Ordering::Release);
        self.resource_recovery_failed
            .store(false, Ordering::Release);
        *self.emit_fn.lock().await = emit_fn.clone();
        self.store_endpoint(endpoint).await;
        self.spawn_background_reconnect();
        self.transition(RuntimeState::AwaitingGateway);
        self.connect_current().await
    }

    pub async fn update_endpoint(&self, endpoint: EndpointSnapshot) -> Result<(), WsError> {
        let _lifecycle = self.lifecycle_lock.lock().await;
        self.lifecycle_epoch.fetch_add(1, Ordering::AcqRel);
        self.update_endpoint_locked(endpoint).await
    }

    async fn update_endpoint_locked(&self, endpoint: EndpointSnapshot) -> Result<(), WsError> {
        self.store_endpoint(endpoint).await;
        // Endpoint changes deliberately rotate a healthy socket too.
        self.shutdown_current_connection().await;
        self.connect_current().await
    }

    /// Start only when needed. A ready runtime keeps its socket when the
    /// endpoint identity is unchanged, and rotates it when it has changed.
    pub async fn ensure_started(
        &self,
        endpoint: EndpointSnapshot,
        emit_fn: EmitFn,
    ) -> Result<(), WsError> {
        let _lifecycle = self.lifecycle_lock.lock().await;
        *self.emit_fn.lock().await = emit_fn.clone();
        let same_endpoint = self
            .endpoint
            .lock()
            .await
            .as_ref()
            .is_some_and(|current| current.identity == endpoint.identity);
        match self.state() {
            RuntimeState::Ready | RuntimeState::Degraded { .. } if same_endpoint => Ok(()),
            RuntimeState::Ready | RuntimeState::Degraded { .. } => {
                self.lifecycle_epoch.fetch_add(1, Ordering::AcqRel);
                self.update_endpoint_locked(endpoint).await
            }
            _ => {
                self.lifecycle_epoch.fetch_add(1, Ordering::AcqRel);
                self.start_locked(endpoint, emit_fn).await
            }
        }
    }

    pub fn set_resource_factory(&self, factory: ResourceFactory) {
        *self
            .resource_factory
            .lock()
            .expect("resource factory mutex poisoned") = Some(factory);
    }

    pub async fn health_check(&self) -> HealthStatus {
        if !self.process_health().await {
            let error = if self.local_gateway_handle.lock().await.is_some() {
                RuntimeError::GatewayCrashed("owned gateway process stopped".to_string())
            } else {
                RuntimeError::TunnelCrashed("owned SSH tunnel stopped".to_string())
            };
            self.mark_disconnected().await;
            self.transition(RuntimeState::Failed { error });
        } else {
            let connection_state = self.client.runtime.lock().await.state.clone();
            if connection_state == ConnectionState::Disconnected
                && matches!(
                    self.current_state.borrow().clone(),
                    RuntimeState::Ready | RuntimeState::Degraded { .. }
                )
            {
                self.transition(RuntimeState::Failed {
                    error: RuntimeError::ConnectionLost,
                });
            }
        }
        self.health_from_state(self.current_state.borrow().clone())
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
        let attempt = self.reconnect_attempt.fetch_add(1, Ordering::AcqRel) + 1;
        self.transition(RuntimeState::Reconnecting {
            attempt,
            next_retry_at: Instant::now() + delay,
            reason: "connection is disconnected".to_string(),
        });
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
        let attempt = self.reconnect_attempt.fetch_add(1, Ordering::AcqRel) + 1;
        self.transition(RuntimeState::Reconnecting {
            attempt,
            next_retry_at: Instant::now(),
            reason: "reconnect requested".to_string(),
        });
        self.shutdown_current_connection().await;
        self.connect_current().await
    }

    /// Stop the reader and the supervisor loop, then leave the instance ready
    /// for a later `start` call.
    pub async fn stop(&self) {
        let (epoch, task) = {
            let _lifecycle = self.lifecycle_lock.lock().await;
            let epoch = self.lifecycle_epoch.fetch_add(1, Ordering::AcqRel) + 1;
            self.transition(RuntimeState::Stopping);
            self.shutdown.store(true, Ordering::Release);
            self.shutdown_notify.notify_waiters();
            self.stop_owned_resources().await;
            self.shutdown_current_connection().await;
            let task = self
                .background_task
                .lock()
                .expect("background task mutex poisoned")
                .take();
            (epoch, task)
        };

        if let Some(task) = task {
            if let Err(error) = task.await {
                tracing::warn!(runtime = ?self.runtime_key, %error, "runtime reconnect task failed");
            }
        }

        let _lifecycle = self.lifecycle_lock.lock().await;
        if self.lifecycle_epoch.load(Ordering::Acquire) == epoch {
            *self.reconnect_delay.lock().await = INITIAL_RECONNECT_DELAY;
            self.reconnect_attempt.store(0, Ordering::Release);
            *self.last_connected.lock().await = None;
            self.shutdown.store(false, Ordering::Release);
            self.resource_recovery_failed
                .store(false, Ordering::Release);
            self.transition(RuntimeState::Stopped);
        }
    }

    /// Compatibility shim for existing callers. `start` starts this itself.
    pub fn spawn_background_reconnect(&self) {
        let mut task = self
            .background_task
            .lock()
            .expect("background task mutex poisoned");
        if let Some(previous) = task.take() {
            previous.abort();
        }

        let supervisor = self.clone();
        let epoch = self.lifecycle_epoch.load(Ordering::Acquire);
        *task = Some(tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(1)) => {}
                    _ = supervisor.shutdown_notify.notified() => break,
                }
                if supervisor.shutdown.load(Ordering::Acquire)
                    || supervisor.lifecycle_epoch.load(Ordering::Acquire) != epoch
                {
                    break;
                }
                if !supervisor.process_health().await {
                    supervisor.recover_resource(epoch).await;
                    continue;
                }
                if matches!(
                    supervisor.health_check().await,
                    HealthStatus::Disconnected {
                        state: RuntimeState::Failed { .. },
                        ..
                    }
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
        self.transition(RuntimeState::Stopping);
        self.shutdown.store(true, Ordering::Release);
        self.shutdown_notify.notify_waiters();
    }

    pub fn client(&self) -> &Arc<GatewayClient> {
        &self.client
    }
    pub fn runtime_key(&self) -> RuntimeKey {
        self.runtime_key.clone()
    }

    /// Subscribe before an operation to receive every lifecycle transition.
    pub fn subscribe_state(&self) -> watch::Receiver<RuntimeState> {
        self.current_state.clone()
    }

    pub fn state(&self) -> RuntimeState {
        self.current_state.borrow().clone()
    }

    /// Transfer ownership of a local Hermes child to this supervisor.
    pub async fn set_local_gateway(&self, child: Child) {
        let mut handle = self.local_gateway_handle.lock().await;
        if let Some(mut previous) = handle.replace(ChildGuard::new(child)) {
            previous.kill().await;
        }
        self.resource_recovery_failed
            .store(false, Ordering::Release);
    }

    /// Adopt a ready Hermes process that was spawned by the compatibility
    /// gateway launcher, including the endpoint data needed to reconnect.
    pub async fn set_local_gateway_process(&self, process: GatewayProcess) {
        let mut handle = self.local_gateway_handle.lock().await;
        if let Some(mut previous) = handle.replace(ChildGuard::from_gateway(process)) {
            previous.kill().await;
        }
        self.resource_recovery_failed
            .store(false, Ordering::Release);
    }

    pub async fn local_gateway_endpoint(&self) -> Option<(u16, String)> {
        self.local_gateway_handle
            .lock()
            .await
            .as_ref()
            .and_then(|guard| guard.local_endpoint.as_ref())
            .map(|endpoint| (endpoint.port, endpoint.session_token.clone()))
    }

    pub async fn has_ssh_tunnel(&self) -> bool {
        self.ssh_tunnel_handle.lock().await.is_some()
    }

    pub async fn ssh_tunnel_healthy(&self) -> bool {
        self.ssh_tunnel_handle
            .lock()
            .await
            .as_ref()
            .is_some_and(SshTunnelGuard::is_alive)
    }

    /// Transfer ownership of an SSH-tunnel monitor to this supervisor.
    pub async fn set_ssh_tunnel(&self, port: u16, monitor: JoinHandle<()>) {
        let mut handle = self.ssh_tunnel_handle.lock().await;
        if let Some(mut previous) = handle.replace(SshTunnelGuard::new(port, monitor)) {
            previous.close();
        }
        self.resource_recovery_failed
            .store(false, Ordering::Release);
    }

    /// Like `set_ssh_tunnel`, with the SSH-layer close operation invoked when
    /// the supervisor is stopped or the guard is replaced.
    pub async fn set_ssh_tunnel_with_cleanup(
        &self,
        port: u16,
        monitor: JoinHandle<()>,
        close: Arc<dyn Fn() + Send + Sync>,
    ) {
        let mut handle = self.ssh_tunnel_handle.lock().await;
        let guard = SshTunnelGuard {
            port,
            monitor: Some(monitor),
            close: Some(close),
        };
        if let Some(mut previous) = handle.replace(guard) {
            previous.close();
        }
        self.resource_recovery_failed
            .store(false, Ordering::Release);
    }

    /// Whether every resource owned by this supervisor is still alive.
    pub async fn process_health(&self) -> bool {
        if let Some(child) = self.local_gateway_handle.lock().await.as_mut() {
            if !child.is_alive().await {
                return false;
            }
        }
        if let Some(tunnel) = self.ssh_tunnel_handle.lock().await.as_ref() {
            if !tunnel.is_alive() {
                return false;
            }
        }
        true
    }

    async fn stop_owned_resources(&self) {
        if let Some(mut child) = self.local_gateway_handle.lock().await.take() {
            child.kill().await;
        }
        if let Some(mut tunnel) = self.ssh_tunnel_handle.lock().await.take() {
            tunnel.close();
        }
    }

    async fn recover_resource(&self, expected_epoch: u64) {
        if self.resource_recovery_failed.load(Ordering::Acquire) {
            return;
        }
        let Some(factory) = self
            .resource_factory
            .lock()
            .expect("resource factory mutex poisoned")
            .clone()
        else {
            return;
        };

        let result = factory().await;
        let _lifecycle = self.lifecycle_lock.lock().await;
        if self.shutdown.load(Ordering::Acquire)
            || self.lifecycle_epoch.load(Ordering::Acquire) != expected_epoch
        {
            return;
        }
        match result {
            Ok(endpoint) => {
                self.resource_recovery_failed
                    .store(false, Ordering::Release);
                if let Err(error) = self.update_endpoint_locked(endpoint).await {
                    tracing::warn!(runtime = ?self.runtime_key, %error, "resource recovered but WebSocket reconnect failed");
                }
            }
            Err(error) => {
                self.resource_recovery_failed.store(true, Ordering::Release);
                self.transition(RuntimeState::Failed { error });
            }
        }
    }

    async fn mark_disconnected(&self) {
        let mut runtime = self.client.runtime.lock().await;
        runtime.state = ConnectionState::Disconnected;
    }

    fn transition(&self, new_state: RuntimeState) {
        let previous = self.current_state.borrow().clone();
        tracing::info!(runtime = ?self.runtime_key, from = ?previous, to = ?new_state, "runtime state transition");
        self.state_tx.send_replace(new_state);
    }

    fn health_from_state(&self, state: RuntimeState) -> HealthStatus {
        match state.clone() {
            RuntimeState::Ready => HealthStatus::Connected { state },
            RuntimeState::Degraded { reason } => HealthStatus::Degraded { reason, state },
            RuntimeState::Reconnecting {
                attempt, reason, ..
            } => HealthStatus::Disconnected {
                reason,
                state,
                attempt,
            },
            RuntimeState::Failed { error } => HealthStatus::Disconnected {
                reason: format!("{error:?}"),
                state,
                attempt: self.reconnect_attempt.load(Ordering::Acquire),
            },
            RuntimeState::Incompatible { reason } => HealthStatus::Disconnected {
                reason,
                state,
                attempt: 0,
            },
            _ => HealthStatus::Disconnected {
                reason: "runtime is not ready".to_string(),
                state,
                attempt: self.reconnect_attempt.load(Ordering::Acquire),
            },
        }
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
            self.transition(RuntimeState::CheckingCompatibility);
            self.transition(RuntimeState::Reconciling);
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
                self.reconnect_attempt.store(0, Ordering::Release);
                *self.last_connected.lock().await = Some(Instant::now());
                let connection_state = self.client.runtime.lock().await.state.clone();
                match connection_state {
                    ConnectionState::Degraded => self.transition(RuntimeState::Degraded {
                        reason: "connection reconciliation is degraded".to_string(),
                    }),
                    _ => self.transition(RuntimeState::Ready),
                }
                Ok(())
            }
            Err(error) => {
                let mut delay = self.reconnect_delay.lock().await;
                *delay = (*delay * 2).min(self.max_reconnect_delay);
                match &error {
                    WsError::Incompatible(reason) => self.transition(RuntimeState::Incompatible {
                        reason: format!("{reason:?}"),
                    }),
                    WsError::ConnectionLost => self.transition(RuntimeState::Failed {
                        error: RuntimeError::ConnectionLost,
                    }),
                    WsError::Timeout | WsError::RpcTimeout | WsError::ReadyTimeout => self
                        .transition(RuntimeState::Failed {
                            error: RuntimeError::Timeout,
                        }),
                    _ => self.transition(RuntimeState::Failed {
                        error: RuntimeError::WebSocket(error.to_string()),
                    }),
                }
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
        endpoint_for(url, RuntimeKey::Remote("test".into()))
    }

    fn endpoint_for(url: &str, runtime_key: RuntimeKey) -> EndpointSnapshot {
        EndpointSnapshot {
            ws_url: url.into(),
            identity: EndpointIdentity::from_ws_url(url, None, None),
            runtime_key,
        }
    }

    async fn start_mock_backend() -> (String, oneshot::Receiver<()>) {
        start_mock_backend_with_contract(4).await
    }

    async fn start_mock_backend_with_contract(contract: u32) -> (String, oneshot::Receiver<()>) {
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
                                json!({"session_id":"probe","stored_session_id":"probe","message_count":0,"messages":[],"info":{"desktop_contract":contract}})
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
                if matches!(
                    supervisor.health_check().await,
                    HealthStatus::Connected { .. }
                ) {
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
        assert!(matches!(
            supervisor.health_check().await,
            HealthStatus::Connected { .. }
        ));
        supervisor.stop().await;
    }

    #[tokio::test]
    async fn successful_start_reaches_ready() {
        let supervisor = RuntimeSupervisor::new(RuntimeKey::Remote("test".into()), None);
        let (url, _) = start_mock_backend().await;
        supervisor
            .start(endpoint(&url), noop_emitter())
            .await
            .unwrap();
        assert_eq!(supervisor.state(), RuntimeState::Ready);
        supervisor.stop().await;
    }

    #[tokio::test]
    async fn ensure_started_is_a_noop_for_a_ready_matching_endpoint() {
        let supervisor = RuntimeSupervisor::new(RuntimeKey::Remote("test".into()), None);
        let (url, _) = start_mock_backend().await;
        let endpoint = endpoint(&url);
        supervisor
            .start(endpoint.clone(), noop_emitter())
            .await
            .unwrap();
        let revision = supervisor.endpoint_revision.load(Ordering::Acquire);
        let generation = supervisor.client.runtime.lock().await.generation;

        supervisor
            .ensure_started(endpoint, noop_emitter())
            .await
            .unwrap();

        assert_eq!(
            supervisor.endpoint_revision.load(Ordering::Acquire),
            revision
        );
        assert_eq!(
            supervisor.client.runtime.lock().await.generation,
            generation
        );
        supervisor.stop().await;
    }

    #[tokio::test]
    async fn ensure_started_updates_a_ready_runtime_when_endpoint_changes() {
        let supervisor = RuntimeSupervisor::new(RuntimeKey::Remote("test".into()), None);
        let (first_url, _) = start_mock_backend().await;
        let (second_url, _) = start_mock_backend().await;
        supervisor
            .start(endpoint(&first_url), noop_emitter())
            .await
            .unwrap();

        supervisor
            .ensure_started(endpoint(&second_url), noop_emitter())
            .await
            .unwrap();

        assert_eq!(supervisor.endpoint_revision.load(Ordering::Acquire), 2);
        assert_eq!(
            supervisor.client.endpoint.read().await.identity(),
            endpoint(&second_url).identity
        );
        supervisor.stop().await;
    }

    #[tokio::test]
    async fn reconnect_enters_reconnecting_state() {
        let supervisor = RuntimeSupervisor::new(RuntimeKey::Remote("test".into()), None);
        assert!(supervisor
            .start(endpoint("ws://127.0.0.1:1/api/ws"), noop_emitter())
            .await
            .is_err());
        let reconnect = tokio::spawn({
            let supervisor = supervisor.clone();
            async move { supervisor.reconnect(noop_emitter()).await }
        });
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(matches!(
            supervisor.state(),
            RuntimeState::Reconnecting { .. }
        ));
        supervisor.stop().await;
        assert!(reconnect.await.unwrap().is_err());
    }

    #[tokio::test]
    async fn compatibility_rejection_sets_incompatible_state() {
        let supervisor = RuntimeSupervisor::new(RuntimeKey::Remote("test".into()), None);
        let (url, _) = start_mock_backend_with_contract(999).await;
        assert!(supervisor
            .start(endpoint(&url), noop_emitter())
            .await
            .is_err());
        assert!(matches!(
            supervisor.state(),
            RuntimeState::Incompatible { .. }
        ));
        supervisor.stop().await;
    }

    #[tokio::test]
    async fn watch_channel_delivers_transition_events() {
        let supervisor = RuntimeSupervisor::new(RuntimeKey::Remote("test".into()), None);
        let mut states = supervisor.subscribe_state();
        supervisor.transition(RuntimeState::Starting);
        states.changed().await.unwrap();
        assert_eq!(*states.borrow(), RuntimeState::Starting);
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
        assert!(matches!(
            supervisor.health_check().await,
            HealthStatus::Connected { .. }
        ));
        assert!(supervisor.background_task.lock().unwrap().is_some());
        supervisor.stop().await;
    }

    #[tokio::test]
    async fn concurrent_stop_and_start_cannot_end_stopped_with_live_socket() {
        let supervisor = Arc::new(RuntimeSupervisor::new(
            RuntimeKey::Remote("test".into()),
            None,
        ));
        let (url, _) = start_mock_backend().await;
        supervisor
            .start(endpoint(&url), noop_emitter())
            .await
            .unwrap();
        if let Some(task) = supervisor.background_task.lock().unwrap().take() {
            task.abort();
        }
        *supervisor.background_task.lock().unwrap() = Some(tokio::spawn(async {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }));

        let stopping = tokio::spawn({
            let supervisor = Arc::clone(&supervisor);
            async move { supervisor.stop().await }
        });
        tokio::time::sleep(Duration::from_millis(20)).await;
        supervisor
            .start(endpoint(&url), noop_emitter())
            .await
            .unwrap();
        stopping.await.unwrap();

        assert!(!matches!(supervisor.state(), RuntimeState::Stopped));
        assert!(matches!(
            supervisor.health_check().await,
            HealthStatus::Connected { .. } | HealthStatus::Degraded { .. }
        ));
        supervisor.stop().await;
    }

    fn short_lived_child() -> Child {
        #[cfg(windows)]
        let mut command = tokio::process::Command::new("cmd");
        #[cfg(windows)]
        command.args(["/C", "exit 0"]);
        #[cfg(not(windows))]
        let mut command = tokio::process::Command::new("sh");
        #[cfg(not(windows))]
        command.args(["-c", "exit 0"]);
        command.spawn().unwrap()
    }

    fn long_lived_child() -> Child {
        #[cfg(windows)]
        let mut command = tokio::process::Command::new("cmd");
        #[cfg(windows)]
        command.args(["/C", "ping -n 60 127.0.0.1 > nul"]);
        #[cfg(not(windows))]
        let mut command = tokio::process::Command::new("sh");
        #[cfg(not(windows))]
        command.args(["-c", "sleep 60"]);
        command.spawn().unwrap()
    }

    #[tokio::test]
    async fn local_process_death_marks_runtime_failed() {
        let supervisor = RuntimeSupervisor::new(RuntimeKey::Local, None);
        supervisor.set_local_gateway(short_lived_child()).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!supervisor.process_health().await);
        assert!(matches!(
            supervisor.health_check().await,
            HealthStatus::Disconnected {
                state: RuntimeState::Failed {
                    error: RuntimeError::GatewayCrashed(_)
                },
                ..
            }
        ));
    }

    #[tokio::test]
    async fn ssh_tunnel_monitor_death_marks_runtime_failed() {
        let supervisor = RuntimeSupervisor::new(RuntimeKey::Ssh("test".into()), None);
        supervisor
            .set_ssh_tunnel(12345, tokio::spawn(async {}))
            .await;
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert!(!supervisor.process_health().await);
        assert!(matches!(
            supervisor.health_check().await,
            HealthStatus::Disconnected {
                state: RuntimeState::Failed {
                    error: RuntimeError::TunnelCrashed(_)
                },
                ..
            }
        ));
    }

    #[tokio::test]
    async fn gateway_crash_factory_respawns_and_returns_ready() {
        let supervisor = Arc::new(RuntimeSupervisor::new(RuntimeKey::Local, None));
        let (url, _) = start_mock_backend().await;
        let factory_url = url.clone();
        let factory_calls = Arc::new(AtomicU32::new(0));
        let factory_supervisor = Arc::downgrade(&supervisor);
        let factory_calls_clone = Arc::clone(&factory_calls);
        supervisor.set_resource_factory(Arc::new(move || {
            let supervisor = factory_supervisor.clone();
            let url = factory_url.clone();
            let factory_calls = Arc::clone(&factory_calls_clone);
            Box::pin(async move {
                factory_calls.fetch_add(1, Ordering::AcqRel);
                let supervisor = supervisor.upgrade().ok_or_else(|| {
                    RuntimeError::GatewayStartFailed("supervisor dropped".to_string())
                })?;
                supervisor.set_local_gateway(long_lived_child()).await;
                Ok(endpoint_for(&url, RuntimeKey::Local))
            })
        }));
        supervisor.set_local_gateway(short_lived_child()).await;
        supervisor
            .start(endpoint_for(&url, RuntimeKey::Local), noop_emitter())
            .await
            .unwrap();

        tokio::time::timeout(Duration::from_secs(4), async {
            while factory_calls.load(Ordering::Acquire) == 0
                || !matches!(supervisor.state(), RuntimeState::Ready)
            {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("gateway factory did not restore the runtime");
        supervisor.stop().await;
    }

    #[tokio::test]
    async fn tunnel_crash_factory_respawns_and_returns_ready() {
        let supervisor = Arc::new(RuntimeSupervisor::new(RuntimeKey::Ssh("test".into()), None));
        let (url, _) = start_mock_backend().await;
        let factory_url = url.clone();
        let factory_calls = Arc::new(AtomicU32::new(0));
        let factory_supervisor = Arc::downgrade(&supervisor);
        let factory_calls_clone = Arc::clone(&factory_calls);
        supervisor.set_resource_factory(Arc::new(move || {
            let supervisor = factory_supervisor.clone();
            let url = factory_url.clone();
            let factory_calls = Arc::clone(&factory_calls_clone);
            Box::pin(async move {
                factory_calls.fetch_add(1, Ordering::AcqRel);
                let supervisor = supervisor
                    .upgrade()
                    .ok_or_else(|| RuntimeError::TunnelFailed("supervisor dropped".to_string()))?;
                supervisor
                    .set_ssh_tunnel(
                        12345,
                        tokio::spawn(async { futures::future::pending::<()>().await }),
                    )
                    .await;
                Ok(endpoint_for(&url, RuntimeKey::Ssh("test".into())))
            })
        }));
        supervisor
            .set_ssh_tunnel(12345, tokio::spawn(async {}))
            .await;
        supervisor
            .start(
                endpoint_for(&url, RuntimeKey::Ssh("test".into())),
                noop_emitter(),
            )
            .await
            .unwrap();

        tokio::time::timeout(Duration::from_secs(4), async {
            while factory_calls.load(Ordering::Acquire) == 0
                || !matches!(supervisor.state(), RuntimeState::Ready)
            {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("tunnel factory did not restore the runtime");
        supervisor.stop().await;
    }

    #[tokio::test]
    async fn stop_kills_owned_process_and_tunnel() {
        let supervisor = RuntimeSupervisor::new(RuntimeKey::Local, None);
        supervisor.set_local_gateway(long_lived_child()).await;
        let monitor = tokio::spawn(async { futures::future::pending::<()>().await });
        supervisor.set_ssh_tunnel(12345, monitor).await;
        supervisor.stop().await;
        assert!(supervisor.local_gateway_handle.lock().await.is_none());
        assert!(supervisor.ssh_tunnel_handle.lock().await.is_none());
        assert_eq!(supervisor.state(), RuntimeState::Stopped);
    }

    #[tokio::test]
    async fn resources_can_be_reowned_after_stop() {
        let supervisor = RuntimeSupervisor::new(RuntimeKey::Local, None);
        supervisor.set_local_gateway(long_lived_child()).await;
        supervisor.stop().await;
        supervisor.set_local_gateway(long_lived_child()).await;
        assert!(supervisor.process_health().await);
        supervisor.stop().await;
    }
}
