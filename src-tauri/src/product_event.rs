use crate::{ChatEvent, ConversationId};

/// Stable event contract consumed by the product layer. Hermes-specific event
/// details remain in `ChatEvent` and can evolve without leaking into callers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProductEvent {
    Token {
        conversation_id: ConversationId,
        text: String,
    },
    MessageComplete {
        conversation_id: ConversationId,
    },
    ToolStart {
        conversation_id: ConversationId,
        name: String,
    },
    ToolComplete {
        conversation_id: ConversationId,
        name: String,
        result: String,
    },
    Error {
        conversation_id: ConversationId,
        message: String,
    },
    StatusUpdate {
        conversation_id: ConversationId,
        text: String,
    },
}

pub fn translate_hermes_to_product(
    chat_event: ChatEvent,
    conv_id: ConversationId,
) -> Option<ProductEvent> {
    match chat_event {
        ChatEvent::Token { content }
        | ChatEvent::Reasoning { content }
        | ChatEvent::Thinking { content } => Some(ProductEvent::Token {
            conversation_id: conv_id,
            text: content,
        }),
        ChatEvent::Done { .. } => Some(ProductEvent::MessageComplete {
            conversation_id: conv_id,
        }),
        ChatEvent::ToolStart { name, .. } | ChatEvent::ToolGenerating { name, .. } => {
            Some(ProductEvent::ToolStart {
                conversation_id: conv_id,
                name,
            })
        }
        ChatEvent::ToolComplete { name, output, .. } => Some(ProductEvent::ToolComplete {
            conversation_id: conv_id,
            name,
            result: output,
        }),
        ChatEvent::Error { message } => Some(ProductEvent::Error {
            conversation_id: conv_id,
            message,
        }),
        ChatEvent::Status { status } => Some(ProductEvent::StatusUpdate {
            conversation_id: conv_id,
            text: status,
        }),
        _ => None,
    }
}
