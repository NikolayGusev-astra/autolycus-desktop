import { useCallback } from "react";
import { MessageList } from "./MessageList";
import { ChatInput } from "./ChatInput";
import { useGatewayStore } from "../../stores/gatewayStore";

export function ChatView() {
  const { client, currentSessionId, addMessage } = useGatewayStore();

  const handleSend = useCallback(
    async (text: string) => {
      if (!client || !text.trim()) return;

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
        // Use prompt.submit — this is the correct method for chat in tui_gateway
        await client.call("prompt.submit", {
          text: text.trim(),
          session_id: currentSessionId,
        });
      } catch (err) {
        console.error("Failed to send message:", err);
      }
    },
    [client, currentSessionId, addMessage]
  );

  return (
    <div className="flex h-full flex-col bg-white dark:bg-gray-900">
      <div className="flex-1 overflow-hidden">
        <MessageList />
      </div>
      <ChatInput onSend={handleSend} />
    </div>
  );
}
