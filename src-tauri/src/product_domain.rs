use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tokio::sync::Mutex;

use crate::session_registry::RuntimeKey;
use crate::{ConversationService, ProductEvent, RuntimeSupervisor, SessionRegistry};

/// Stable identity exposed by the product API. It is intentionally separate
/// from the transport registry's conversation key.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct ConversationId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum ConversationStatus {
    Active,
    Suspended,
    Resuming,
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalChoice {
    Once,
    Session,
    Always,
    Deny,
}

impl ApprovalChoice {
    pub fn as_wire_value(&self) -> &'static str {
        match self {
            Self::Once => "once",
            Self::Session => "session",
            Self::Always => "always",
            Self::Deny => "deny",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionOutcome {
    Applied,
    Expired,
    AlreadyResolved,
}

impl ActionOutcome {
    pub(crate) fn from_rpc_result(result: &serde_json::Value) -> Self {
        match result.get("status").and_then(serde_json::Value::as_str) {
            Some("expired") => Self::Expired,
            Some("already_resolved" | "already-resolved") => Self::AlreadyResolved,
            _ if result.get("resolved") == Some(&serde_json::Value::Bool(false)) => {
                Self::AlreadyResolved
            }
            _ => Self::Applied,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProductConversation {
    pub id: ConversationId,
    pub status: ConversationStatus,
    pub title: Option<String>,
    pub runtime_key: RuntimeKey,
}

/// Conversation representation safe to expose through the product API.
#[derive(serde::Serialize)]
pub struct ProductConversationDto {
    pub id: String,
    pub title: Option<String>,
    pub status: ConversationStatus,
    pub connection_mode: String,
}

impl From<&ProductConversation> for ProductConversationDto {
    fn from(conversation: &ProductConversation) -> Self {
        Self {
            id: conversation.id.0.clone(),
            title: conversation.title.clone(),
            status: conversation.status.clone(),
            connection_mode: match &conversation.runtime_key {
                RuntimeKey::Local => "local",
                RuntimeKey::Remote(_) => "remote",
                RuntimeKey::Ssh(_) => "ssh",
            }
            .to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProductError {
    ConversationNotFound,
    RuntimeUnavailable(String),
    SendFailed(String),
    Timeout,
    Internal(String),
}

#[derive(Debug, serde::Serialize)]
#[serde(tag = "code", content = "details")]
pub enum ProductCommandError {
    ConversationNotFound,
    RuntimeUnavailable { message: String },
    SendFailed { message: String },
    Timeout,
    Internal { message: String },
}

impl From<ProductError> for ProductCommandError {
    fn from(error: ProductError) -> Self {
        match error {
            ProductError::ConversationNotFound => Self::ConversationNotFound,
            ProductError::RuntimeUnavailable(message) => Self::RuntimeUnavailable { message },
            ProductError::SendFailed(message) => Self::SendFailed { message },
            ProductError::Timeout => Self::Timeout,
            ProductError::Internal(message) => Self::Internal { message },
        }
    }
}

impl fmt::Display for ProductError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ConversationNotFound => f.write_str("conversation not found"),
            Self::RuntimeUnavailable(message) => write!(f, "runtime unavailable: {message}"),
            Self::SendFailed(message) => write!(f, "message send failed: {message}"),
            Self::Timeout => f.write_str("operation timed out"),
            Self::Internal(message) => write!(f, "internal product error: {message}"),
        }
    }
}

impl std::error::Error for ProductError {}

#[async_trait::async_trait]
pub trait ConversationRepository: Send + Sync {
    async fn create(&self, conversation: ProductConversation) -> Result<(), ProductError>;
    async fn get(&self, id: &ConversationId) -> Option<ProductConversation>;
    async fn update_status(
        &self,
        id: &ConversationId,
        status: ConversationStatus,
    ) -> Result<(), ProductError>;
    async fn list(&self) -> Vec<ProductConversation>;
}

#[derive(Default)]
pub struct InMemoryConversationRepository {
    conversations: Mutex<HashMap<ConversationId, ProductConversation>>,
}

impl InMemoryConversationRepository {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }
}

#[async_trait::async_trait]
impl ConversationRepository for InMemoryConversationRepository {
    async fn create(&self, conversation: ProductConversation) -> Result<(), ProductError> {
        self.conversations
            .lock()
            .await
            .insert(conversation.id.clone(), conversation);
        Ok(())
    }

    async fn get(&self, id: &ConversationId) -> Option<ProductConversation> {
        self.conversations.lock().await.get(id).cloned()
    }

    async fn update_status(
        &self,
        id: &ConversationId,
        status: ConversationStatus,
    ) -> Result<(), ProductError> {
        let mut conversations = self.conversations.lock().await;
        let conversation = conversations
            .get_mut(id)
            .ok_or(ProductError::ConversationNotFound)?;
        conversation.status = status;
        Ok(())
    }

    async fn list(&self) -> Vec<ProductConversation> {
        self.conversations.lock().await.values().cloned().collect()
    }
}

/// Product-facing dependencies. It deliberately owns no connection settings;
/// supervisors retain the endpoints and lifecycle configuration established by
/// the app's infrastructure layer.
#[derive(Clone)]
pub struct ProductContext {
    app_handle: Option<AppHandle>,
    sessions: Arc<SessionRegistry>,
    local_supervisor: Arc<RuntimeSupervisor>,
    remote_supervisor: Arc<RuntimeSupervisor>,
    ssh_supervisor: Arc<RuntimeSupervisor>,
    conversations: Arc<dyn ConversationRepository>,
}

impl ProductContext {
    pub fn new(
        app_handle: AppHandle,
        sessions: Arc<SessionRegistry>,
        local_supervisor: Arc<RuntimeSupervisor>,
        remote_supervisor: Arc<RuntimeSupervisor>,
        ssh_supervisor: Arc<RuntimeSupervisor>,
        conversations: Arc<dyn ConversationRepository>,
    ) -> Self {
        Self {
            app_handle: Some(app_handle),
            sessions,
            local_supervisor,
            remote_supervisor,
            ssh_supervisor,
            conversations,
        }
    }

    /// Test-only product context. Product events are deliberately disabled
    /// because Tauri's mock runtime has a different `AppHandle` type.
    pub fn new_for_test(
        sessions: Arc<SessionRegistry>,
        local_supervisor: Arc<RuntimeSupervisor>,
        remote_supervisor: Arc<RuntimeSupervisor>,
        ssh_supervisor: Arc<RuntimeSupervisor>,
        conversations: Arc<dyn ConversationRepository>,
    ) -> Self {
        Self {
            app_handle: None,
            sessions,
            local_supervisor,
            remote_supervisor,
            ssh_supervisor,
            conversations,
        }
    }

    pub fn conversation_service(&self) -> ConversationService {
        ConversationService::new(
            Arc::clone(&self.sessions),
            Arc::clone(&self.local_supervisor),
            Arc::clone(&self.remote_supervisor),
            Arc::clone(&self.ssh_supervisor),
            Arc::clone(&self.conversations),
        )
    }

    pub fn emit_product_event(&self, event: &ProductEvent) {
        use tauri::Emitter;

        if let Some(app_handle) = &self.app_handle {
            let _ = app_handle.emit("product-event", event);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn in_memory_repository_persists_and_updates_a_conversation() {
        let repository = InMemoryConversationRepository::new();
        let id = ConversationId("conversation-1".into());
        repository
            .create(ProductConversation {
                id: id.clone(),
                status: ConversationStatus::Active,
                title: Some("Test".into()),
                runtime_key: RuntimeKey::Local,
            })
            .await
            .unwrap();

        repository
            .update_status(&id, ConversationStatus::Suspended)
            .await
            .unwrap();

        assert_eq!(repository.list().await.len(), 1);
        assert_eq!(
            repository.get(&id).await.unwrap().status,
            ConversationStatus::Suspended
        );
    }

    #[test]
    fn conversation_dto_hides_runtime_key_details() {
        let dto = ProductConversationDto::from(&ProductConversation {
            id: ConversationId("conversation-1".into()),
            status: ConversationStatus::Active,
            title: Some("Test".into()),
            runtime_key: RuntimeKey::Remote("private-instance-id".into()),
        });

        assert_eq!(dto.id, "conversation-1");
        assert_eq!(dto.connection_mode, "remote");
        assert_eq!(
            serde_json::to_value(dto).unwrap()["connection_mode"],
            "remote"
        );
    }

    #[test]
    fn action_outcomes_are_derived_from_the_hermes_result_contract() {
        assert_eq!(
            ActionOutcome::from_rpc_result(&serde_json::json!({"resolved": true})),
            ActionOutcome::Applied
        );
        assert_eq!(
            ActionOutcome::from_rpc_result(&serde_json::json!({"resolved": false})),
            ActionOutcome::AlreadyResolved
        );
        assert_eq!(
            ActionOutcome::from_rpc_result(&serde_json::json!({"status": "expired"})),
            ActionOutcome::Expired
        );
        assert_eq!(
            ActionOutcome::from_rpc_result(&serde_json::json!({"status": "already_resolved"})),
            ActionOutcome::AlreadyResolved
        );
    }

    #[test]
    fn approval_choices_match_hermes_wire_values() {
        assert_eq!(ApprovalChoice::Once.as_wire_value(), "once");
        assert_eq!(ApprovalChoice::Session.as_wire_value(), "session");
        assert_eq!(ApprovalChoice::Always.as_wire_value(), "always");
        assert_eq!(ApprovalChoice::Deny.as_wire_value(), "deny");
    }
}
