use std::sync::Arc;

use crate::chat::ConnectionMode;
use crate::product_domain::{ConversationId, ConversationStatus, ProductError};
use crate::session_registry::{
    ConversationId as RegistryConversationId, ProfileId, RuntimeKey, SessionState,
};
use crate::ws_transport::{create_session_on_connection, submit_prompt_on_connection, WsError};
use crate::{RuntimeError, RuntimeSupervisor, SessionRegistry};

/// Product API for creating and using conversations. Session IDs remain an
/// infrastructure concern and are resolved through `SessionRegistry`.
#[derive(Clone)]
pub struct ConversationService {
    sessions: Arc<SessionRegistry>,
    local_supervisor: Arc<RuntimeSupervisor>,
    remote_supervisor: Arc<RuntimeSupervisor>,
    ssh_supervisor: Arc<RuntimeSupervisor>,
}

impl ConversationService {
    pub fn new(
        sessions: Arc<SessionRegistry>,
        local_supervisor: Arc<RuntimeSupervisor>,
        remote_supervisor: Arc<RuntimeSupervisor>,
        ssh_supervisor: Arc<RuntimeSupervisor>,
    ) -> Self {
        Self {
            sessions,
            local_supervisor,
            remote_supervisor,
            ssh_supervisor,
        }
    }

    pub async fn create_conversation(
        &self,
        mode: ConnectionMode,
    ) -> Result<ConversationId, ProductError> {
        let supervisor = self.supervisor_for_mode(&mode);
        supervisor
            .ensure_started_configured()
            .await
            .map_err(runtime_error)?;

        let client = supervisor.client();
        let result = create_session_on_connection(&client, "desktop", None)
            .await
            .map_err(create_error)?;
        let product_id = ConversationId(uuid::Uuid::new_v4().to_string());
        let registry_id = registry_id(&product_id);
        let generation = client.runtime.lock().await.generation;
        let runtime_key = supervisor.runtime_key();
        let profile = result
            .info
            .profile_name
            .filter(|profile| !profile.is_empty())
            .map(ProfileId::new)
            .unwrap_or_default();

        self.sessions
            .set_live(
                registry_id,
                result.session_id,
                Some(result.stored_session_id),
                profile,
                generation,
                runtime_key,
            )
            .await;

        Ok(product_id)
    }

    pub async fn send_message(
        &self,
        conv_id: &ConversationId,
        text: &str,
    ) -> Result<(), ProductError> {
        let (supervisor, runtime_key, live_session_id) = self
            .lookup_conversation(conv_id)
            .await
            .ok_or(ProductError::ConversationNotFound)?;
        supervisor
            .ensure_started_configured()
            .await
            .map_err(runtime_error)?;
        submit_prompt_on_connection(&supervisor.client(), &live_session_id, text)
            .await
            .map_err(send_error)?;
        debug_assert_eq!(runtime_key, supervisor.runtime_key());
        Ok(())
    }

    pub async fn get_status(&self, conv_id: &ConversationId) -> Option<ConversationStatus> {
        let registry_id = registry_id(conv_id);
        for supervisor in self.supervisors() {
            if let Some(binding) = self
                .sessions
                .get(&registry_id, supervisor.runtime_key())
                .await
            {
                return Some(match binding.state {
                    SessionState::Active => ConversationStatus::Active,
                    SessionState::Suspended => ConversationStatus::Suspended,
                    SessionState::Resuming { .. } => ConversationStatus::Resuming,
                    SessionState::ResumeFailed => {
                        ConversationStatus::Failed("session resume failed".into())
                    }
                });
            }
        }
        None
    }

    fn supervisor_for_mode(&self, mode: &ConnectionMode) -> Arc<RuntimeSupervisor> {
        match mode {
            ConnectionMode::Local => Arc::clone(&self.local_supervisor),
            ConnectionMode::Remote => Arc::clone(&self.remote_supervisor),
            ConnectionMode::Ssh => Arc::clone(&self.ssh_supervisor),
        }
    }

    fn supervisors(&self) -> [&Arc<RuntimeSupervisor>; 3] {
        [
            &self.local_supervisor,
            &self.remote_supervisor,
            &self.ssh_supervisor,
        ]
    }

    async fn lookup_conversation(
        &self,
        conv_id: &ConversationId,
    ) -> Option<(Arc<RuntimeSupervisor>, RuntimeKey, String)> {
        let registry_id = registry_id(conv_id);
        for supervisor in self.supervisors() {
            let runtime_key = supervisor.runtime_key();
            if let Some(live_session_id) = self
                .sessions
                .get_live(&registry_id, runtime_key.clone())
                .await
            {
                return Some((Arc::clone(supervisor), runtime_key, live_session_id));
            }
        }
        None
    }
}

fn registry_id(id: &ConversationId) -> RegistryConversationId {
    RegistryConversationId::new(id.0.clone())
}

fn runtime_error(error: RuntimeError) -> ProductError {
    match error {
        RuntimeError::Timeout => ProductError::Timeout,
        other => ProductError::RuntimeUnavailable(other.to_string()),
    }
}

fn create_error(error: WsError) -> ProductError {
    match error {
        WsError::Timeout | WsError::RpcTimeout | WsError::ReadyTimeout => ProductError::Timeout,
        other => ProductError::RuntimeUnavailable(other.to_string()),
    }
}

fn send_error(error: WsError) -> ProductError {
    match error {
        WsError::Timeout | WsError::RpcTimeout | WsError::ReadyTimeout => ProductError::Timeout,
        other => ProductError::SendFailed(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use futures::{SinkExt, StreamExt};
    use serde_json::{json, Value};
    use tokio::net::TcpListener;
    use tokio::sync::Mutex;
    use tokio_tungstenite::tungstenite::Message;

    use super::*;
    use crate::ws_transport::{EmitFn, EndpointIdentity, EndpointSnapshot};

    async fn start_mock_backend() -> (String, Arc<Mutex<Vec<Value>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let received = Arc::new(Mutex::new(Vec::new()));
        let received_for_task = Arc::clone(&received);

        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
            socket
                .send(Message::Text(
                    json!({"jsonrpc":"2.0", "method":"event", "params":{"type":"gateway.ready", "payload":{}}})
                        .to_string(),
                ))
                .await
                .unwrap();

            while let Some(Ok(Message::Text(text))) = socket.next().await {
                let request: Value = serde_json::from_str(&text).unwrap();
                received_for_task.lock().await.push(request.clone());
                let id = request["id"].as_u64().unwrap();
                let response = match request["method"].as_str() {
                    Some("session.create") => json!({
                        "jsonrpc":"2.0", "id": id,
                        "result": {"session_id":"mock-session", "stored_session_id":"mock-session",
                            "message_count":0, "messages":[], "info":{"desktop_contract":4}}
                    }),
                    Some("prompt.submit") => json!({
                        "jsonrpc":"2.0", "id": id,
                        "result": {"status":"streaming"}
                    }),
                    _ => json!({"jsonrpc":"2.0", "id": id, "result": {}}),
                };
                socket
                    .send(Message::Text(response.to_string()))
                    .await
                    .unwrap();
            }
        });

        (format!("ws://127.0.0.1:{port}/api/ws?token=test"), received)
    }

    #[tokio::test]
    async fn product_api_creates_and_sends_with_mock_backend() {
        let (url, received) = start_mock_backend().await;
        let sessions = SessionRegistry::new();
        let local = Arc::new(RuntimeSupervisor::new(
            RuntimeKey::Local,
            Some(Arc::clone(&sessions)),
        ));
        let no_events: EmitFn = Arc::new(|_| {});
        let endpoint = EndpointSnapshot {
            identity: EndpointIdentity::from_ws_url(&url, None, None),
            ws_url: url,
            runtime_key: RuntimeKey::Local,
        };
        local.start(endpoint, no_events).await.unwrap();
        let remote = Arc::new(RuntimeSupervisor::new(
            RuntimeKey::Remote("test".into()),
            None,
        ));
        let ssh = Arc::new(RuntimeSupervisor::new(RuntimeKey::Ssh("test".into()), None));
        let service = ConversationService::new(sessions, local.clone(), remote, ssh);

        let conversation_id = service
            .create_conversation(ConnectionMode::Local)
            .await
            .unwrap();
        assert_eq!(
            service.get_status(&conversation_id).await,
            Some(ConversationStatus::Active)
        );
        service
            .send_message(&conversation_id, "hello")
            .await
            .unwrap();

        assert!(received.lock().await.iter().any(|request| {
            request["method"] == "prompt.submit"
                && request["params"]["session_id"] == "mock-session"
                && request["params"]["text"] == "hello"
        }));
        local.stop().await;
    }
}
