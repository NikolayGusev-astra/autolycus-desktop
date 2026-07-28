use std::sync::Arc;
use std::time::Duration;

use crate::chat::ConnectionMode;
use crate::product_domain::{
    ActionOutcome, ApprovalChoice, ConversationId, ConversationRepository, ConversationStatus,
    ProductCommandError, ProductConversation, ProductError,
};
use crate::session_registry::{
    ConversationId as RegistryConversationId, ProfileId, RuntimeKey, SessionState,
};
use crate::ws_transport::{
    call_rpc, create_session_on_connection, submit_prompt_on_connection, WsError,
};
use crate::{RuntimeError, RuntimeState, RuntimeSupervisor, SessionRegistry};

/// Product API for creating and using conversations. Session IDs remain an
/// infrastructure concern and are resolved through `SessionRegistry`.
#[derive(Clone)]
pub struct ConversationService {
    sessions: Arc<SessionRegistry>,
    local_supervisor: Arc<RuntimeSupervisor>,
    remote_supervisor: Arc<RuntimeSupervisor>,
    ssh_supervisor: Arc<RuntimeSupervisor>,
    conversations: Arc<dyn ConversationRepository>,
}

impl ConversationService {
    pub fn new(
        sessions: Arc<SessionRegistry>,
        local_supervisor: Arc<RuntimeSupervisor>,
        remote_supervisor: Arc<RuntimeSupervisor>,
        ssh_supervisor: Arc<RuntimeSupervisor>,
        conversations: Arc<dyn ConversationRepository>,
    ) -> Self {
        Self {
            sessions,
            local_supervisor,
            remote_supervisor,
            ssh_supervisor,
            conversations,
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
                runtime_key.clone(),
            )
            .await;

        self.conversations
            .create(ProductConversation {
                id: product_id.clone(),
                status: ConversationStatus::Active,
                title: None,
                runtime_key,
            })
            .await?;

        Ok(product_id)
    }

    pub async fn send_message(
        &self,
        conv_id: &ConversationId,
        text: &str,
    ) -> Result<(), ProductError> {
        let conversation = self
            .conversations
            .get(conv_id)
            .await
            .ok_or(ProductError::ConversationNotFound)?;
        let (supervisor, live_session_id) = self
            .resolve_live_session(&conversation)
            .await
            .map_err(product_error_from_command)?;
        submit_prompt_on_connection(&supervisor.client(), &live_session_id, text)
            .await
            .map_err(send_error)?;
        debug_assert_eq!(conversation.runtime_key, supervisor.runtime_key());
        Ok(())
    }

    pub async fn refresh_status(
        &self,
        conv_id: &ConversationId,
    ) -> Result<ConversationStatus, ProductError> {
        let conversation = self
            .conversations
            .get(conv_id)
            .await
            .ok_or(ProductError::ConversationNotFound)?;
        let runtime_state = self
            .supervisor_for_runtime_key(&conversation.runtime_key)?
            .state();
        let binding = self
            .sessions
            .get(&registry_id(conv_id), conversation.runtime_key)
            .await;
        let status = project_status(
            binding.as_ref().map(|binding| &binding.state),
            runtime_state,
        );
        if conversation.status != status {
            self.conversations
                .update_status(conv_id, status.clone())
                .await?;
        }
        Ok(status)
    }

    pub async fn list_conversations(&self) -> Vec<ProductConversation> {
        self.conversations.list().await
    }

    pub async fn abort_conversation(
        &self,
        conv_id: &ConversationId,
    ) -> Result<ActionOutcome, ProductCommandError> {
        self.call_action(conv_id, "session.interrupt", serde_json::json!({}))
            .await
    }

    pub async fn respond_approval(
        &self,
        conv_id: &ConversationId,
        request_id: &str,
        choice: ApprovalChoice,
        all: bool,
    ) -> Result<ActionOutcome, ProductCommandError> {
        self.call_action(conv_id, "approval.respond", serde_json::json!({"request_id": request_id, "choice": choice.as_wire_value(), "all": all})).await
    }

    pub async fn respond_clarification(
        &self,
        conv_id: &ConversationId,
        request_id: &str,
        answer: &str,
    ) -> Result<ActionOutcome, ProductCommandError> {
        self.call_action(
            conv_id,
            "clarify.respond",
            serde_json::json!({"request_id": request_id, "answer": answer}),
        )
        .await
    }

    pub async fn respond_secret(
        &self,
        conv_id: &ConversationId,
        request_id: &str,
        secret: &str,
    ) -> Result<ActionOutcome, ProductCommandError> {
        self.call_action(
            conv_id,
            "secret.respond",
            serde_json::json!({"request_id": request_id, "value": secret}),
        )
        .await
    }

    pub async fn respond_sudo(
        &self,
        conv_id: &ConversationId,
        request_id: &str,
        password: &str,
    ) -> Result<ActionOutcome, ProductCommandError> {
        self.call_action(
            conv_id,
            "sudo.respond",
            serde_json::json!({"request_id": request_id, "password": password}),
        )
        .await
    }

    async fn call_action(
        &self,
        conv_id: &ConversationId,
        method: &'static str,
        mut params: serde_json::Value,
    ) -> Result<ActionOutcome, ProductCommandError> {
        let conversation = self
            .conversations
            .get(conv_id)
            .await
            .ok_or(ProductCommandError::ConversationNotFound)?;
        let (supervisor, live_session_id) = self.resolve_live_session(&conversation).await?;
        params["session_id"] = serde_json::Value::String(live_session_id);
        call_rpc::<serde_json::Value, serde_json::Value, _>(
            &*supervisor.client(),
            method,
            params,
            Duration::from_secs(30),
        )
        .await
        .map(|result| ActionOutcome::from_rpc_result(&result))
        .map_err(send_error)
        .map_err(ProductCommandError::from)
    }

    async fn resolve_live_session(
        &self,
        conversation: &ProductConversation,
    ) -> Result<(Arc<RuntimeSupervisor>, String), ProductCommandError> {
        let runtime_key = conversation.runtime_key.clone();
        let supervisor = self
            .supervisor_for_runtime_key(&runtime_key)
            .map_err(ProductCommandError::from)?;
        let registry_id = registry_id(&conversation.id);
        let binding = self.sessions.get(&registry_id, runtime_key.clone()).await;
        if binding
            .as_ref()
            .and_then(|binding| binding.live_session_id.as_ref())
            .is_none()
        {
            supervisor
                .ensure_started_configured()
                .await
                .map_err(runtime_error)
                .map_err(ProductCommandError::from)?;
            // A suspended binding can coexist with a healthy socket. Rotate
            // that socket so reconnect reconciliation resumes the durable
            // session and installs a fresh live ID.
            if self
                .sessions
                .get_live(&registry_id, runtime_key.clone())
                .await
                .is_none()
            {
                supervisor.force_reconnect().await.map_err(|error| {
                    ProductCommandError::RuntimeUnavailable {
                        message: error.to_string(),
                    }
                })?;
            }
        }
        let live_session_id = self
            .sessions
            .get_live(&registry_id, runtime_key)
            .await
            .ok_or_else(|| ProductCommandError::RuntimeUnavailable {
                message: "conversation has no live session".into(),
            })?;
        Ok((supervisor, live_session_id))
    }

    pub async fn migrate_conversations(
        &self,
        old_runtime_key: RuntimeKey,
        new_runtime_key: RuntimeKey,
    ) -> Result<(), ProductError> {
        for mut conversation in self.conversations.list().await {
            if conversation.runtime_key != old_runtime_key {
                continue;
            }
            self.sessions
                .migrate_runtime(
                    &registry_id(&conversation.id),
                    old_runtime_key.clone(),
                    new_runtime_key.clone(),
                )
                .await;
            conversation.runtime_key = new_runtime_key.clone();
            conversation.status = ConversationStatus::Suspended;
            self.conversations.create(conversation).await?;
        }
        Ok(())
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

    fn supervisor_for_runtime_key(
        &self,
        runtime_key: &RuntimeKey,
    ) -> Result<Arc<RuntimeSupervisor>, ProductError> {
        self.supervisors()
            .into_iter()
            .find(|supervisor| supervisor.runtime_key() == *runtime_key)
            .cloned()
            .ok_or_else(|| {
                ProductError::RuntimeUnavailable("conversation runtime is unavailable".into())
            })
    }
}

fn project_status(
    session_state: Option<&SessionState>,
    runtime_state: RuntimeState,
) -> ConversationStatus {
    match runtime_state {
        RuntimeState::Failed { error } => ConversationStatus::Failed(error.to_string()),
        RuntimeState::Incompatible { reason } => ConversationStatus::Failed(reason),
        RuntimeState::Stopped | RuntimeState::Stopping => ConversationStatus::Suspended,
        _ => match session_state {
            Some(SessionState::Active) => ConversationStatus::Active,
            Some(SessionState::Resuming { .. }) => ConversationStatus::Resuming,
            Some(SessionState::ResumeFailed) => {
                ConversationStatus::Failed("session resume failed".into())
            }
            Some(SessionState::Suspended) | None => ConversationStatus::Suspended,
        },
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

fn product_error_from_command(error: ProductCommandError) -> ProductError {
    match error {
        ProductCommandError::ConversationNotFound => ProductError::ConversationNotFound,
        ProductCommandError::RuntimeUnavailable { message } => {
            ProductError::RuntimeUnavailable(message)
        }
        ProductCommandError::SendFailed { message } => ProductError::SendFailed(message),
        ProductCommandError::Timeout => ProductError::Timeout,
        ProductCommandError::Internal { message } => ProductError::Internal(message),
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
        start_mock_backend_with_connections(1).await
    }

    async fn start_reconnect_mock_backend() -> (String, Arc<Mutex<Vec<Value>>>) {
        start_mock_backend_with_connections(2).await
    }

    async fn start_mock_backend_with_connections(
        connection_count: usize,
    ) -> (String, Arc<Mutex<Vec<Value>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let received = Arc::new(Mutex::new(Vec::new()));
        let received_for_task = Arc::clone(&received);

        tokio::spawn(async move {
            for _ in 0..connection_count {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                let received_for_connection = Arc::clone(&received_for_task);
                tokio::spawn(async move {
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
                        received_for_connection.lock().await.push(request.clone());
                        let id = request["id"].as_u64().unwrap();
                        let response = match request["method"].as_str() {
                            Some("session.create") => json!({
                                "jsonrpc":"2.0", "id": id,
                                "result": {"session_id":"mock-session", "stored_session_id":"mock-session",
                                    "message_count":0, "messages":[], "info":{"desktop_contract":4}}
                            }),
                            Some("session.resume") => {
                                let stored_session_id = request["params"]["session_id"]
                                    .as_str()
                                    .expect("session.resume must include the durable session ID");
                                json!({
                                    "jsonrpc":"2.0", "id": id,
                                    "result": {"session_id":"resumed-session", "resumed": stored_session_id,
                                        "message_count":0, "messages":[], "info":{}}
                                })
                            }
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
        let service = ConversationService::new(
            Arc::clone(&sessions),
            local.clone(),
            remote,
            ssh,
            crate::InMemoryConversationRepository::new(),
        );

        let conversation_id = service
            .create_conversation(ConnectionMode::Local)
            .await
            .unwrap();
        assert_eq!(
            service.refresh_status(&conversation_id).await.unwrap(),
            ConversationStatus::Active
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

    #[tokio::test]
    async fn action_commands_send_their_rpc_with_the_live_session() {
        let (url, received) = start_mock_backend().await;
        let sessions = SessionRegistry::new();
        let local = Arc::new(RuntimeSupervisor::new(
            RuntimeKey::Local,
            Some(Arc::clone(&sessions)),
        ));
        local
            .start(
                EndpointSnapshot {
                    identity: EndpointIdentity::from_ws_url(&url, None, None),
                    ws_url: url,
                    runtime_key: RuntimeKey::Local,
                },
                Arc::new(|_| {}),
            )
            .await
            .unwrap();
        let service = ConversationService::new(
            Arc::clone(&sessions),
            Arc::clone(&local),
            Arc::new(RuntimeSupervisor::new(
                RuntimeKey::Remote("test".into()),
                None,
            )),
            Arc::new(RuntimeSupervisor::new(RuntimeKey::Ssh("test".into()), None)),
            crate::InMemoryConversationRepository::new(),
        );
        let id = service
            .create_conversation(ConnectionMode::Local)
            .await
            .unwrap();

        service.abort_conversation(&id).await.unwrap();
        service
            .respond_approval(&id, "approval-1", ApprovalChoice::Always, true)
            .await
            .unwrap();
        service
            .respond_clarification(&id, "clarify-1", "yes")
            .await
            .unwrap();
        service
            .respond_secret(&id, "secret-1", "token")
            .await
            .unwrap();
        service
            .respond_sudo(&id, "sudo-1", "password")
            .await
            .unwrap();

        let requests = received.lock().await;
        let request = requests
            .iter()
            .find(|request| request["method"] == "session.interrupt")
            .unwrap();
        assert_eq!(request["params"], json!({"session_id": "mock-session"}));
        let request = requests
            .iter()
            .find(|request| request["method"] == "approval.respond")
            .unwrap();
        assert_eq!(
            request["params"],
            json!({"request_id": "approval-1", "choice": "always", "all": true, "session_id": "mock-session"})
        );
        let request = requests
            .iter()
            .find(|request| request["method"] == "clarify.respond")
            .unwrap();
        assert_eq!(
            request["params"],
            json!({"request_id": "clarify-1", "answer": "yes", "session_id": "mock-session"})
        );
        let request = requests
            .iter()
            .find(|request| request["method"] == "secret.respond")
            .unwrap();
        assert_eq!(
            request["params"],
            json!({"request_id": "secret-1", "value": "token", "session_id": "mock-session"})
        );
        let request = requests
            .iter()
            .find(|request| request["method"] == "sudo.respond")
            .unwrap();
        assert_eq!(
            request["params"],
            json!({"request_id": "sudo-1", "password": "password", "session_id": "mock-session"})
        );
        local.stop().await;
    }

    #[tokio::test]
    async fn action_after_disconnect_reconnects_and_succeeds() {
        let (url, received) = start_reconnect_mock_backend().await;
        let sessions = SessionRegistry::new();
        let local = Arc::new(RuntimeSupervisor::new(
            RuntimeKey::Local,
            Some(Arc::clone(&sessions)),
        ));
        local
            .start(
                EndpointSnapshot {
                    identity: EndpointIdentity::from_ws_url(&url, None, None),
                    ws_url: url,
                    runtime_key: RuntimeKey::Local,
                },
                Arc::new(|_| {}),
            )
            .await
            .unwrap();
        let service = ConversationService::new(
            Arc::clone(&sessions),
            Arc::clone(&local),
            Arc::new(RuntimeSupervisor::new(
                RuntimeKey::Remote("test".into()),
                None,
            )),
            Arc::new(RuntimeSupervisor::new(RuntimeKey::Ssh("test".into()), None)),
            crate::InMemoryConversationRepository::new(),
        );
        let id = service
            .create_conversation(ConnectionMode::Local)
            .await
            .unwrap();

        // Simulate a disconnected generation: the durable binding is kept,
        // but its live ID is stale until the action resolves it.
        sessions
            .mark_stale_for_generation(2, RuntimeKey::Local)
            .await;
        assert_eq!(
            service.abort_conversation(&id).await.unwrap(),
            ActionOutcome::Applied
        );

        let requests = received.lock().await;
        assert!(requests.iter().any(|request| {
            request["method"] == "session.resume"
                && request["params"]["session_id"] == "mock-session"
        }));
        assert!(requests.iter().any(|request| {
            request["method"] == "session.interrupt"
                && request["params"]["session_id"] == "resumed-session"
        }));
        local.stop().await;
    }

    #[tokio::test]
    async fn refresh_status_projects_a_suspended_binding() {
        let sessions = SessionRegistry::new();
        let repository = crate::InMemoryConversationRepository::new();
        let id = ConversationId("suspended-conversation".into());
        repository
            .create(ProductConversation {
                id: id.clone(),
                status: ConversationStatus::Active,
                title: None,
                runtime_key: RuntimeKey::Local,
            })
            .await
            .unwrap();
        sessions
            .set_live(
                registry_id(&id),
                "live-session".into(),
                Some("stored-session".into()),
                ProfileId::empty(),
                1,
                RuntimeKey::Local,
            )
            .await;
        sessions.suspend_all(RuntimeKey::Local).await;
        let service = ConversationService::new(
            sessions,
            Arc::new(RuntimeSupervisor::new(RuntimeKey::Local, None)),
            Arc::new(RuntimeSupervisor::new(
                RuntimeKey::Remote("test".into()),
                None,
            )),
            Arc::new(RuntimeSupervisor::new(RuntimeKey::Ssh("test".into()), None)),
            Arc::clone(&repository) as Arc<dyn ConversationRepository>,
        );

        assert_eq!(
            service.refresh_status(&id).await.unwrap(),
            ConversationStatus::Suspended
        );
        assert_eq!(
            repository.get(&id).await.unwrap().status,
            ConversationStatus::Suspended
        );
    }

    #[tokio::test]
    async fn list_conversations_returns_persisted_conversations() {
        let repository = crate::InMemoryConversationRepository::new();
        repository
            .create(ProductConversation {
                id: ConversationId("persisted-conversation".into()),
                status: ConversationStatus::Active,
                title: Some("Persisted".into()),
                runtime_key: RuntimeKey::Local,
            })
            .await
            .unwrap();
        let service = ConversationService::new(
            SessionRegistry::new(),
            Arc::new(RuntimeSupervisor::new(RuntimeKey::Local, None)),
            Arc::new(RuntimeSupervisor::new(
                RuntimeKey::Remote("test".into()),
                None,
            )),
            Arc::new(RuntimeSupervisor::new(RuntimeKey::Ssh("test".into()), None)),
            repository,
        );

        let conversations = service.list_conversations().await;
        assert_eq!(conversations.len(), 1);
        assert_eq!(conversations[0].id.0, "persisted-conversation");
    }

    #[tokio::test]
    async fn migration_moves_product_and_session_bindings_to_new_runtime() {
        let sessions = SessionRegistry::new();
        let repository = crate::InMemoryConversationRepository::new();
        let local = Arc::new(RuntimeSupervisor::new(RuntimeKey::Local, None));
        let remote = Arc::new(RuntimeSupervisor::new(
            RuntimeKey::Remote("old".into()),
            None,
        ));
        let ssh = Arc::new(RuntimeSupervisor::new(RuntimeKey::Ssh("ssh".into()), None));
        let service = ConversationService::new(
            Arc::clone(&sessions),
            local,
            remote,
            ssh,
            Arc::clone(&repository) as Arc<dyn ConversationRepository>,
        );
        let id = ConversationId("conversation-1".into());
        repository
            .create(ProductConversation {
                id: id.clone(),
                status: ConversationStatus::Active,
                title: None,
                runtime_key: RuntimeKey::Remote("old".into()),
            })
            .await
            .unwrap();
        sessions
            .set_live(
                registry_id(&id),
                "live-1".into(),
                Some("stored-1".into()),
                ProfileId::default(),
                1,
                RuntimeKey::Remote("old".into()),
            )
            .await;

        service
            .migrate_conversations(
                RuntimeKey::Remote("old".into()),
                RuntimeKey::Remote("new".into()),
            )
            .await
            .unwrap();

        assert_eq!(
            repository.get(&id).await.unwrap().runtime_key,
            RuntimeKey::Remote("new".into())
        );
        assert!(sessions
            .get_live(&registry_id(&id), RuntimeKey::Remote("new".into()))
            .await
            .is_none());
    }

    #[tokio::test]
    async fn suspended_persisted_conversation_is_not_reported_as_missing() {
        let sessions = SessionRegistry::new();
        let repository = crate::InMemoryConversationRepository::new();
        let service = ConversationService::new(
            sessions,
            Arc::new(RuntimeSupervisor::new(RuntimeKey::Local, None)),
            Arc::new(RuntimeSupervisor::new(
                RuntimeKey::Remote("remote".into()),
                None,
            )),
            Arc::new(RuntimeSupervisor::new(RuntimeKey::Ssh("ssh".into()), None)),
            Arc::clone(&repository) as Arc<dyn ConversationRepository>,
        );
        let id = ConversationId("suspended-conversation".into());
        repository
            .create(ProductConversation {
                id: id.clone(),
                status: ConversationStatus::Suspended,
                title: None,
                runtime_key: RuntimeKey::Local,
            })
            .await
            .unwrap();

        assert!(matches!(
            service.send_message(&id, "hello").await,
            Err(ProductError::RuntimeUnavailable(_))
        ));
    }
}
