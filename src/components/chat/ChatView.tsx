// src/components/chat/ChatView.tsx
// v0.4.0: Uses new send_message_cmd Tauri command with chat_event listener

import { useCallback, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { MessageList } from "./MessageList";
import { ChatInput } from "./ChatInput";
import { useGatewayStore } from "../../stores/gatewayStore";

export function ChatView() {
  const { currentSessionId, addMessage, agentStatus, setAgentStatus } = useGatewayStore();

  // Listen for chat events from Rust backend
  useEffect(() => {
    const unlisten = listen<{ type: string; content?: string; message?: string; name?: string; tool_call_id?: string; output?: string; duration_ms?: number; backend?: string; model?: string; tokens_used?: number; tokens_limit?: number; cost_usd?: number; session_id?: string; status?: string }>("chat_event", (event) => {
      const payload = event.payload;
      const eventType = payload.type;

      switch (eventType) {
        case "token":
          if (payload.content) {
            const messages = useGatewayStore.getState().messages;
            const lastIdx = messages.length - 1;
            const lastMsg = messages[lastIdx];
            if (lastMsg && lastMsg.isStreaming && lastMsg.role === "assistant") {
              const updated = [...messages];
              updated[lastIdx] = { ...lastMsg, content: lastMsg.content + payload.content };
              useGatewayStore.setState({ messages: updated });
            } else {
              addMessage({
                id: `msg-${Date.now()}`,
                role: "assistant",
                content: payload.content,
                timestamp: Date.now(),
                isStreaming: true,
              });
            }
          }
          break;

        case "reasoning":
          if (payload.content) {
            setAgentStatus("thinking");
            addMessage({
              id: `thinking-${Date.now()}`,
              role: "assistant",
              content: payload.content,
              timestamp: Date.now(),
              isStreaming: true,
            });
          }
          break;

        case "tool_start":
          setAgentStatus("tool_calling");
          break;

        case "tool_complete":
          setAgentStatus("idle");
          break;

        case "done":
          const msgs = useGatewayStore.getState().messages;
          const lastIdx = msgs.length - 1;
          const lastMsg = msgs[lastIdx];
          if (lastMsg && lastMsg.isStreaming) {
            const updated = [...msgs];
            updated[lastIdx] = { ...lastMsg, isStreaming: false };
            useGatewayStore.setState({ messages: updated });
          }
          setAgentStatus("idle");
          break;

        case "error":
          setAgentStatus("error");
          break;

        case "status":
          if (payload.status) {
            setAgentStatus(payload.status as any);
          }
          break;

        default:
          break;
      }
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, [addMessage, setAgentStatus]);

  const handleSend = useCallback(
    async (text: string) => {
      if (!text.trim()) return;

      // Clear previous messages for new conversation
      useGatewayStore.setState({ messages: [] });

      // Add user message to UI immediately
      const userMsg = {
        id: `user-${Date.now()}`,
        role: "user" as const,
        content: text.trim(),
        timestamp: Date.now(),
      };
      addMessage(userMsg);

      try {
        // Use new send_message_cmd Tauri command
        await invoke<string>("send_message_cmd", {
          request: {
            text: text.trim(),
            session_id: currentSessionId,
            history: null,
          },
        });
      } catch (err) {
        console.error("Failed to send message:", err);
        setAgentStatus("error");
      }
    },
    [currentSessionId, addMessage, setAgentStatus]
  );

  return (
    <div className="flex h-full flex-col bg-white dark:bg-gray-900">
      <div className="flex-1 overflow-hidden">
        <MessageList />
      </div>
      <ChatInput onSend={handleSend} agentStatus={agentStatus} />
    </div>
  );
}
