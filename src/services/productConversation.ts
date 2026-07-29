import { invoke } from "@tauri-apps/api/core";

export interface ProductConversationDto {
  id: string;
  title: string | null;
  status: "Active" | "Suspended" | "Resuming" | "ResumeFailed" | "Failed";
  connection_mode: "local" | "remote" | "ssh";
}

export interface ProductEvent {
  type:
    | "MessageDelta"
    | "MessageCompleted"
    | "Reasoning"
    | "Thinking"
    | "ToolStarted"
    | "ToolCompleted"
    | "ApprovalRequired"
    | "ClarificationRequired"
    | "SecretRequired"
    | "PrivilegeRequired"
    | "Error"
    | "StatusUpdate"
    | "Progress"
    | "InteractionExpired";
  conversation_id: string;
  [key: string]: unknown;
}

export const productConversationService = {
  async createConversation(mode: string): Promise<string> {
    return invoke("create_conversation_cmd", { mode });
  },

  async sendMessage(conversationId: string, text: string): Promise<void> {
    return invoke("send_message_cmd_v2", { conversationId, text });
  },

  async getConversations(): Promise<ProductConversationDto[]> {
    return invoke("get_conversations_cmd");
  },

  async getConversationStatus(conversationId: string): Promise<string> {
    return invoke("get_conversation_status_cmd", { conversationId });
  },

  async abortConversation(conversationId: string): Promise<string> {
    return invoke("abort_conversation_cmd", { conversationId });
  },

  async respondApproval(
    conversationId: string,
    requestId: string,
    choice: string,
    all: boolean,
  ): Promise<string> {
    return invoke("respond_approval_cmd", { conversationId, requestId, choice, all });
  },

  async respondClarification(conversationId: string, requestId: string, answer: string): Promise<string> {
    return invoke("respond_clarification_cmd", { conversationId, requestId, answer });
  },

  async respondSecret(conversationId: string, requestId: string, secret: string): Promise<string> {
    return invoke("respond_secret_cmd", { conversationId, requestId, secret });
  },

  async respondSudo(conversationId: string, requestId: string, password: string): Promise<string> {
    return invoke("respond_sudo_cmd", { conversationId, requestId, password });
  },
};
