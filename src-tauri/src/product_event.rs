use serde::Serialize;

use crate::{ChatEvent, ConversationId};

/// Stable event contract consumed by the product layer. Hermes-specific event
/// details remain in `ChatEvent` and can evolve without leaking into callers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProductEvent {
    MessageDelta {
        conversation_id: ConversationId,
        text: String,
    },
    MessageCompleted {
        conversation_id: ConversationId,
    },
    Reasoning {
        conversation_id: ConversationId,
        text: String,
    },
    Thinking {
        conversation_id: ConversationId,
        text: String,
    },
    ToolStarted {
        conversation_id: ConversationId,
        name: String,
    },
    ToolCompleted {
        conversation_id: ConversationId,
        name: String,
        result: String,
    },
    ApprovalRequired {
        conversation_id: ConversationId,
        request_id: String,
        tool_id: String,
        message: Option<String>,
    },
    ClarificationRequired {
        conversation_id: ConversationId,
        request_id: String,
        message: String,
    },
    SecretRequired {
        conversation_id: ConversationId,
        request_id: String,
    },
    PrivilegeRequired {
        conversation_id: ConversationId,
        request_id: String,
        message: String,
    },
    Error {
        conversation_id: ConversationId,
        message: String,
    },
    StatusUpdate {
        conversation_id: ConversationId,
        text: String,
    },
    Progress {
        conversation_id: ConversationId,
        text: String,
    },
}

pub fn translate_hermes_to_product(
    chat_event: ChatEvent,
    conv_id: ConversationId,
) -> Option<ProductEvent> {
    match chat_event {
        ChatEvent::Token { content } => Some(ProductEvent::MessageDelta {
            conversation_id: conv_id,
            text: content,
        }),
        ChatEvent::Reasoning { content } => Some(ProductEvent::Reasoning {
            conversation_id: conv_id,
            text: content,
        }),
        ChatEvent::Thinking { content } => Some(ProductEvent::Thinking {
            conversation_id: conv_id,
            text: content,
        }),
        ChatEvent::Done { .. } => Some(ProductEvent::MessageCompleted {
            conversation_id: conv_id,
        }),
        ChatEvent::ToolStart { name, .. } | ChatEvent::ToolGenerating { name, .. } => {
            Some(ProductEvent::ToolStarted {
                conversation_id: conv_id,
                name,
            })
        }
        ChatEvent::ToolComplete { name, output, .. } => Some(ProductEvent::ToolCompleted {
            conversation_id: conv_id,
            name,
            result: output,
        }),
        ChatEvent::ApprovalRequest {
            request_id,
            tool_name,
            tool_input,
            ..
        } => Some(ProductEvent::ApprovalRequired {
            conversation_id: conv_id,
            request_id,
            tool_id: tool_name,
            message: (!tool_input.is_empty()).then_some(tool_input),
        }),
        ChatEvent::ClarifyRequest {
            request_id,
            question,
            ..
        } => Some(ProductEvent::ClarificationRequired {
            conversation_id: conv_id,
            request_id,
            message: question,
        }),
        ChatEvent::SecretRequest { request_id, .. } | ChatEvent::SecretExpire { request_id } => {
            Some(ProductEvent::SecretRequired {
                conversation_id: conv_id,
                request_id,
            })
        }
        ChatEvent::SudoRequest {
            request_id, reason, ..
        } => Some(ProductEvent::PrivilegeRequired {
            conversation_id: conv_id,
            request_id,
            message: reason.unwrap_or_default(),
        }),
        ChatEvent::SudoExpire { request_id } => Some(ProductEvent::PrivilegeRequired {
            conversation_id: conv_id,
            request_id,
            message: "privilege request expired".into(),
        }),
        ChatEvent::Error { message } => Some(ProductEvent::Error {
            conversation_id: conv_id,
            message,
        }),
        ChatEvent::Status { status } => Some(ProductEvent::StatusUpdate {
            conversation_id: conv_id,
            text: status,
        }),
        ChatEvent::PipelineStatus { backend, .. } => Some(ProductEvent::Progress {
            conversation_id: conv_id,
            text: backend,
        }),
        ChatEvent::SessionInfo { .. } => Some(ProductEvent::StatusUpdate {
            conversation_id: conv_id,
            text: "session information updated".into(),
        }),
        ChatEvent::Notification { text, .. } => Some(ProductEvent::Progress {
            conversation_id: conv_id,
            text,
        }),
        ChatEvent::NotificationClear { key } => Some(ProductEvent::Progress {
            conversation_id: conv_id,
            text: format!("notification cleared: {key}"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_reasoning_and_maps_interaction_requests() {
        let id = ConversationId("conversation-1".into());
        assert!(matches!(
            translate_hermes_to_product(
                ChatEvent::Reasoning {
                    content: "because".into()
                },
                id.clone()
            ),
            Some(ProductEvent::Reasoning { text, .. }) if text == "because"
        ));
        assert!(matches!(
            translate_hermes_to_product(
                ChatEvent::ApprovalRequest {
                    request_id: "request-1".into(),
                    tool_name: "shell".into(),
                    tool_input: "ls".into(),
                    action: "run".into(),
                    command_class: "read".into(),
                },
                id
            ),
            Some(ProductEvent::ApprovalRequired { request_id, tool_id, .. })
                if request_id == "request-1" && tool_id == "shell"
        ));
    }
}
