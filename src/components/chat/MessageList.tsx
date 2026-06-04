import { useRef, useEffect, useMemo } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { MessageBubble } from "./MessageBubble";
import { useGatewayStore } from "../../stores/gatewayStore";
import type { Message } from "../../lib/types";

function groupMessages(messages: Message[]): Message[] {
  // Simple pass-through for now; grouping logic can be added later
  return messages;
}

export function MessageList() {
  const parentRef = useRef<HTMLDivElement>(null);
  const messages = useGatewayStore((s) => s.messages);
  const grouped = useMemo(() => groupMessages(messages), [messages]);

  const virtualizer = useVirtualizer({
    count: grouped.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 80,
    overscan: 5,
  });

  useEffect(() => {
    if (grouped.length > 0) {
      virtualizer.scrollToIndex(grouped.length - 1, { align: "end" });
    }
  }, [grouped.length]);

  if (grouped.length === 0) {
    return (
      <div className="flex h-full items-center justify-center text-gray-400">
        <div className="text-center">
          <p className="text-lg mb-1">Welcome to Autolycus</p>
          <p className="text-sm">Send a message to start chatting</p>
        </div>
      </div>
    );
  }

  return (
    <div ref={parentRef} className="h-full overflow-y-auto px-4 py-4">
      <div style={{ height: virtualizer.getTotalSize() }} className="relative">
        {virtualizer.getVirtualItems().map((item) => (
          <div
            key={item.key}
            className="absolute top-0 left-0 w-full"
            style={{ transform: `translateY(${item.start}px)` }}
          >
            <MessageBubble message={grouped[item.index]} />
          </div>
        ))}
      </div>
    </div>
  );
}
