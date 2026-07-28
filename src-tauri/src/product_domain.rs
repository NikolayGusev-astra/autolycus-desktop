use std::fmt;
use std::sync::Arc;

use tauri::AppHandle;

use crate::{ConversationService, RuntimeSupervisor, SessionRegistry};

/// Stable identity exposed by the product API. It is intentionally separate
/// from the transport registry's conversation key.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ConversationId(pub String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConversationStatus {
    Active,
    Suspended,
    Resuming,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductConversation {
    pub id: ConversationId,
    pub status: ConversationStatus,
    pub title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProductError {
    ConversationNotFound,
    RuntimeUnavailable(String),
    SendFailed(String),
    Timeout,
    Internal(String),
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

/// Product-facing dependencies. It deliberately owns no connection settings;
/// supervisors retain the endpoints and lifecycle configuration established by
/// the app's infrastructure layer.
#[derive(Clone)]
pub struct ProductContext {
    pub app_handle: AppHandle,
    pub sessions: Arc<SessionRegistry>,
    pub local_supervisor: Arc<RuntimeSupervisor>,
    pub remote_supervisor: Arc<RuntimeSupervisor>,
    pub ssh_supervisor: Arc<RuntimeSupervisor>,
}

impl ProductContext {
    pub fn new(
        app_handle: AppHandle,
        sessions: Arc<SessionRegistry>,
        local_supervisor: Arc<RuntimeSupervisor>,
        remote_supervisor: Arc<RuntimeSupervisor>,
        ssh_supervisor: Arc<RuntimeSupervisor>,
    ) -> Self {
        Self {
            app_handle,
            sessions,
            local_supervisor,
            remote_supervisor,
            ssh_supervisor,
        }
    }

    pub fn conversation_service(&self) -> ConversationService {
        ConversationService::new(
            Arc::clone(&self.sessions),
            Arc::clone(&self.local_supervisor),
            Arc::clone(&self.remote_supervisor),
            Arc::clone(&self.ssh_supervisor),
        )
    }
}
