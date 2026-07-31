import { useRef, useEffect, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { invoke } from "@tauri-apps/api/core";
import { MessageBubble } from "./MessageBubble";
import { useGatewayStore } from "../../stores/gatewayStore";
import { useConversationStore, type ConversationMessage } from "../../stores/conversationStore";
import type { Message } from "../../lib/types";

const NO_MESSAGES: ConversationMessage[] = [];

/** Empty-state greeting derived from the agent's soul (soul.md). Falls back to
 * a sensible default. The user can customize this via onboarding/Settings. */
function SoulGreeting() {
  const [title, setTitle] = useState<string>("Штурман");
  const [subtitle, setSubtitle] = useState<string>("");

  useEffect(() => {
    (async () => {
      try {
        const soul = await invoke<string>("read_soul_cmd").catch(() => "");
        // Pull a name ("Your name is X") and a greeting line if present.
        const nameMatch = soul.match(/name is\s+(.+?)[.\n]/i);
        if (nameMatch) {
          const name = nameMatch[1].trim().replace(/^the\s+/i, "");
          setTitle(name.charAt(0).toUpperCase() + name.slice(1));
        }
        // Use the personality description as the subtitle if it's short.
        const lines = soul
          .split("\n")
          .map((l) => l.trim())
          .filter((l) => l && !l.startsWith("#") && !/^your name/i.test(l) && l.length < 160);
        if (lines.length) setSubtitle(lines[0]);
      } catch {
        /* keep defaults */
      }
    })();
  }, []);

  return (
    <div className="flex h-full items-center justify-center text-ac-faint">
      <div className="text-center max-w-md">
        <p className="text-lg mb-1 text-ac-ink">
          {title === "Штурман" ? "Привет, я Штурман" : `Привет, я ${title}`}
        </p>
        {subtitle ? (
          <p className="text-sm">{subtitle}</p>
        ) : (
          <p className="text-sm">Отправьте сообщение, чтобы начать</p>
        )}
      </div>
    </div>
  );
}

export function MessageList({ conversationId }: { conversationId: string | null }) {
  const parentRef = useRef<HTMLDivElement>(null);
  const legacyMessages = useGatewayStore((s) => s.messages);
  const productMessages = useConversationStore((state) =>
    conversationId ? state.messagesByConversation.get(conversationId) ?? NO_MESSAGES : NO_MESSAGES,
  );
  const messages: Message[] = conversationId
    ? productMessages.map((message) => ({
      id: message.id,
      role: message.role === "user" ? "user" : message.role === "tool" ? "tool" : "assistant",
      content: message.content,
      thinking: message.thinking,
      timestamp: 0,
    }))
    : legacyMessages;

  const virtualizer = useVirtualizer({
    count: messages.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 80,
    overscan: 5,
    // Dynamic measurement: real bubbles are 100-400px (markdown, code). Without
    // this the virtualizer assumed 80px each → messages overlapped and
    // disappeared on scroll.
    measureElement:
      typeof window !== "undefined" && !navigator.userAgent.includes("Firefox")
        ? (el) => el?.getBoundingClientRect().height ?? 80
        : undefined,
  });

  // Auto-scroll on new message OR content update
  const lastContent = messages.length > 0 ? messages[messages.length - 1].content : "";
  useEffect(() => {
    if (messages.length > 0) {
      virtualizer.scrollToIndex(messages.length - 1, { align: "end" });
    }
  }, [messages.length, lastContent]);

  if (messages.length === 0) {
    return <SoulGreeting />;
  }

  return (
    <div ref={parentRef} className="h-full overflow-y-auto px-4 py-4">
      <div style={{ height: virtualizer.getTotalSize() }} className="relative">
        {virtualizer.getVirtualItems().map((item) => (
          <div
            key={messages[item.index]?.id ?? item.key}
            ref={virtualizer.measureElement}
            data-index={item.index}
            className="absolute top-0 left-0 w-full"
            style={{ transform: `translateY(${item.start}px)` }}
          >
            <MessageBubble message={messages[item.index]} />
          </div>
        ))}
      </div>
    </div>
  );
}
