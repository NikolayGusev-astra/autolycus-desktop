//! Runtime ownership and lifecycle supervision for gateway WebSocket clients.

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use tokio::sync::Mutex;

use crate::{
    session_registry::{RuntimeKey, SessionRegistry},
    ws_transport::{
        ConnectionState, EmitFn, EndpointSnapshot, GatewayClient, HealthStatus, WsError,
    },
};

const INITIAL_RECONNECT_DELAY: Duration = Duration::from_secs(1);
const MAX_RECONNECT_DELAY: Duration = Duration::from_secs(30);

pub struct RuntimeSupervisor {
    client: Arc<GatewayClient>,
    runtime_key: RuntimeKey,
    endpoint: Arc<Mutex<Option<EndpointSnapshot>>>,
    sessions: Option<Arc<SessionRegistry>>,
    reconnect_delay: Mutex<Duration>,
    max_reconnect_delay: Duration,
    last_connected: Mutex<Option<Instant>>,
    degraded_count: Mutex<u64>,
    emit_fn: Mutex<Option<EmitFn>>,
    shutdown: AtomicBool,
    background_started: AtomicBool,
}

impl RuntimeSupervisor {
    pub fn new(runtime_key: RuntimeKey, sessions: Option<Arc<SessionRegistry>>) -> Self {
        Self {
            client: Arc::new(GatewayClient::new(runtime_key.clone(), "", None)),
            runtime_key,
            endpoint: Arc::new(Mutex::new(None)),
            sessions,
            reconnect_delay: Mutex::new(INITIAL_RECONNECT_DELAY),
            max_reconnect_delay: MAX_RECONNECT_DELAY,
            last_connected: Mutex::new(None),
            degraded_count: Mutex::new(0),
            emit_fn: Mutex::new(None),
            shutdown: AtomicBool::new(false),
            background_started: AtomicBool::new(false),
        }
    }

    pub async fn start(&self, endpoint: EndpointSnapshot, emit_fn: EmitFn) -> Result<(), WsError> {
        self.shutdown.store(false, Ordering::Release);
        self.store_endpoint(endpoint).await;
        *self.emit_fn.lock().await = Some(emit_fn.clone());
        self.connect(emit_fn).await
    }

    pub async fn update_endpoint(&self, endpoint: EndpointSnapshot) -> Result<(), WsError> {
        self.store_endpoint(endpoint).await;
        let emit_fn = self
            .emit_fn
            .lock()
            .await
            .clone()
            .unwrap_or_else(noop_emitter);
        self.connect(emit_fn).await
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

    pub async fn reconnect(&self, emit_fn: EmitFn) -> Result<(), WsError> {
        let endpoint = self.endpoint.lock().await.clone().ok_or_else(|| {
            WsError::Protocol("cannot reconnect before an endpoint is configured".into())
        })?;
        self.client.configure_snapshot(endpoint).await;
        let delay = *self.reconnect_delay.lock().await;
        tokio::time::sleep(delay).await;
        *self.emit_fn.lock().await = Some(emit_fn.clone());
        self.connect(emit_fn).await
    }

    pub fn spawn_background_reconnect(self: &Arc<Self>, emit_fn: EmitFn) {
        if self.background_started.swap(true, Ordering::AcqRel) {
            return;
        }
        let supervisor = Arc::clone(self);
        tokio::spawn(async move {
            while !supervisor.shutdown.load(Ordering::Acquire) {
                tokio::time::sleep(Duration::from_secs(1)).await;
                if matches!(
                    supervisor.health_check().await,
                    HealthStatus::Disconnected { .. }
                ) {
                    tracing::info!(runtime = ?supervisor.runtime_key, "attempting runtime reconnect");
                    if let Err(error) = supervisor.reconnect(emit_fn.clone()).await {
                        tracing::warn!(runtime = ?supervisor.runtime_key, %error, "runtime reconnect failed");
                    }
                }
            }
        });
    }

    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Release);
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
        self.client.configure_snapshot(endpoint.clone()).await;
        *self.endpoint.lock().await = Some(endpoint);
    }

    async fn connect(&self, emit_fn: EmitFn) -> Result<(), WsError> {
        match self
            .client
            .ensure_connected(emit_fn, self.sessions.clone())
            .await
        {
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
}

fn noop_emitter() -> EmitFn {
    Arc::new(|_| {})
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ws_transport::EndpointIdentity;

    fn endpoint(url: &str) -> EndpointSnapshot {
        EndpointSnapshot {
            ws_url: url.into(),
            identity: EndpointIdentity::from_ws_url(url, None, None),
            runtime_key: RuntimeKey::Remote("test".into()),
        }
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
    async fn failed_connect_increases_backoff_and_remains_disconnected() {
        let supervisor = RuntimeSupervisor::new(RuntimeKey::Remote("test".into()), None);
        let emitter = noop_emitter();
        assert!(supervisor
            .start(endpoint("ws://127.0.0.1:1/api/ws"), emitter.clone())
            .await
            .is_err());
        assert_eq!(
            *supervisor.reconnect_delay.lock().await,
            Duration::from_secs(2)
        );
        assert!(supervisor.reconnect(emitter).await.is_err());
        assert_eq!(
            *supervisor.reconnect_delay.lock().await,
            Duration::from_secs(4)
        );
        assert!(matches!(
            supervisor.health_check().await,
            HealthStatus::Disconnected { .. }
        ));
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
    async fn disconnected_client_can_attempt_reconnect() {
        let supervisor = RuntimeSupervisor::new(RuntimeKey::Remote("test".into()), None);
        *supervisor.endpoint.lock().await = Some(endpoint("ws://127.0.0.1:1/api/ws"));
        supervisor
            .client
            .configure_snapshot(endpoint("ws://127.0.0.1:1/api/ws"))
            .await;
        assert!(supervisor.reconnect(noop_emitter()).await.is_err());
        assert!(matches!(
            supervisor.health_check().await,
            HealthStatus::Disconnected { .. }
        ));
    }
}
