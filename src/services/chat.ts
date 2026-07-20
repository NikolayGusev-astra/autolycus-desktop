/**
 * Chat Service - Typed wrapper for chat-related backend commands.
 * Components should import from here instead of calling invoke() directly.
 */
import { invoke } from "@tauri-apps/api/core";
import type { SendMessageRequest } from "@/lib/types";

export const chatService = {
  // Send a chat message
  async sendMessage(request: SendMessageRequest): Promise<string> {
    return invoke("send_message_cmd", request);
  },

  // Abort current message generation
  async abortMessage(): Promise<void> {
    return invoke("abort_message_cmd");
  },

  // List all sessions
  async listSessions(profile?: string): Promise<Array<{ id: string; title: string; updatedAt: number }>> {
    return invoke("list_sessions_cmd", { profile });
  },

  // Get messages for a session
  async getSessionMessages(sessionId: string, profile?: string): Promise<Array<{
    id: number;
    role: string;
    content: string;
    timestamp: number;
  }>> {
    return invoke("get_session_messages_cmd", { sessionId, profile });
  },

  // Search sessions
  async searchSessions(query: string, profile?: string): Promise<Array<{ id: string; title: string; snippet: string }>> {
    return invoke("search_sessions_cmd", { query, profile });
  },

  // Delete a session
  async deleteSession(sessionId: string, profile?: string): Promise<void> {
    return invoke("delete_session_cmd", { sessionId, profile });
  },

  // Get session stats
  async getSessionStats(sessionId: string, profile?: string): Promise<{
    messageCount: number;
    tokenCount: number;
    firstMessageAt: number;
    lastMessageAt: number;
  }> {
    return invoke("get_session_stats_cmd", { sessionId, profile });
  },
};