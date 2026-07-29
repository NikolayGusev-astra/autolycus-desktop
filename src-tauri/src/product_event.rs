use serde::Serialize;

use crate::{ApprovalChoice, ChatEvent, ConversationId};

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
        choices: Vec<ApprovalChoice>,
        allow_permanent: bool,
    },
    ClarificationRequired {
        conversation_id: ConversationId,
        request_id: String,
        message: String,
        choices: Vec<String>,
    },
    SecretRequired {
        conversation_id: ConversationId,
        request_id: String,
        prompt: Option<String>,
        env_var: Option<String>,
    },
    InteractionExpired {
        conversation_id: ConversationId,
        request_id: String,
        kind: String,
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
            message,
            choices,
            allow_permanent,
            ..
        } => Some(ProductEvent::ApprovalRequired {
            conversation_id: conv_id,
            request_id,
            tool_id: tool_name,
            message: message.or_else(|| (!tool_input.is_empty()).then_some(tool_input)),
            choices: choices
                .into_iter()
                .filter_map(|choice| match choice.as_str() {
                    "once" => Some(ApprovalChoice::Once),
                    "session" => Some(ApprovalChoice::Session),
                    "always" => Some(ApprovalChoice::Always),
                    "deny" => Some(ApprovalChoice::Deny),
                    _ => None,
                })
                .collect(),
            allow_permanent,
        }),
        ChatEvent::ClarifyRequest {
            request_id,
            question,
            choices,
        } => Some(ProductEvent::ClarificationRequired {
            conversation_id: conv_id,
            request_id,
            message: question,
            choices,
        }),
        ChatEvent::SecretRequest {
            request_id,
            prompt,
            env_var,
            ..
        } => Some(ProductEvent::SecretRequired {
            conversation_id: conv_id,
            request_id,
            prompt: (!prompt.is_empty()).then_some(prompt),
            env_var: (!env_var.is_empty()).then_some(env_var),
        }),
        ChatEvent::SecretExpire { request_id } => Some(ProductEvent::InteractionExpired {
            conversation_id: conv_id,
            request_id,
            kind: "secret".into(),
        }),
        ChatEvent::SudoRequest {
            request_id, reason, ..
        } => Some(ProductEvent::PrivilegeRequired {
            conversation_id: conv_id,
            request_id,
            message: reason.unwrap_or_default(),
        }),
        ChatEvent::SudoExpire { request_id } => Some(ProductEvent::InteractionExpired {
            conversation_id: conv_id,
            request_id,
            kind: "sudo".into(),
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
                    message: Some("Approve shell command?".into()),
                    choices: vec!["once".into(), "always".into(), "deny".into()],
                    allow_permanent: true,
                },
                id.clone()
            ),
            Some(ProductEvent::ApprovalRequired {
                conversation_id, request_id, tool_id, message, choices, allow_permanent,
            }) if conversation_id == id && request_id == "request-1"
                && tool_id == "shell"
                && message.as_deref() == Some("Approve shell command?")
                && choices == vec![ApprovalChoice::Once, ApprovalChoice::Always, ApprovalChoice::Deny]
                && allow_permanent
        ));
    }

    #[test]
    fn preserves_clarification_secret_and_expiration_payloads() {
        let id = ConversationId("conversation-1".into());

        assert!(matches!(
            translate_hermes_to_product(
                ChatEvent::ClarifyRequest {
                    request_id: "clarify-1".into(),
                    question: "Which environment?".into(),
                    choices: vec!["development".into(), "production".into()],
                },
                id.clone(),
            ),
            Some(ProductEvent::ClarificationRequired {
                conversation_id, request_id, message, choices,
            }) if conversation_id == id && request_id == "clarify-1"
                && message == "Which environment?"
                && choices == vec!["development", "production"]
        ));

        assert!(matches!(
            translate_hermes_to_product(
                ChatEvent::SecretRequest {
                    request_id: "secret-1".into(),
                    prompt: "Enter API key".into(),
                    env_var: "API_KEY".into(),
                    metadata: None,
                },
                id.clone(),
            ),
            Some(ProductEvent::SecretRequired {
                conversation_id, request_id, prompt, env_var,
            }) if conversation_id == id && request_id == "secret-1"
                && prompt.as_deref() == Some("Enter API key")
                && env_var.as_deref() == Some("API_KEY")
        ));

        assert!(matches!(
            translate_hermes_to_product(ChatEvent::SecretExpire {
                request_id: "secret-1".into(),
            }, id.clone()),
            Some(ProductEvent::InteractionExpired { conversation_id, request_id, kind })
                if conversation_id == id && request_id == "secret-1" && kind == "secret"
        ));
        assert!(matches!(
            translate_hermes_to_product(ChatEvent::SudoExpire {
                request_id: "sudo-1".into(),
            }, id.clone()),
            Some(ProductEvent::InteractionExpired { conversation_id, request_id, kind })
                if conversation_id == id && request_id == "sudo-1" && kind == "sudo"
        ));
        assert!(matches!(
            translate_hermes_to_product(
                ChatEvent::SudoRequest {
                    request_id: "sudo-2".into(),
                    reason: Some("Administrative access required".into()),
                    timeout_secs: None,
                },
                id.clone(),
            ),
            Some(ProductEvent::PrivilegeRequired { conversation_id, request_id, .. })
                if conversation_id == id && request_id == "sudo-2"
        ));
    }
}
