import { useCallback } from "react";
import { MessageList } from "./MessageList";
import { ChatInput } from "./ChatInput";
import { useGatewayStore } from "../../stores/gatewayStore";

export function ChatView() {
  const { client, currentSessionId } = useGatewayStore();

  const handleSend = useCallback(
    async (text: string) => {
      if (!client || !text.trim()) return;
      try {
        await client.call("chat", {
          message: text.trim(),
          session_id: currentSessionId,
        });
      } catch (err) {
        console.error("Failed to send message:", err);
      }
    },
    [client, currentSessionId]
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
