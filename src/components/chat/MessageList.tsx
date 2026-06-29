import { useRef, useEffect, useMemo, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { invoke } from "@tauri-apps/api/core";
import { MessageBubble } from "./MessageBubble";
import { useGatewayStore } from "../../stores/gatewayStore";
import type { Message } from "../../lib/types";

function groupMessages(messages: Message[]): Message[] {
  // Simple pass-through for now; grouping logic can be added later
  return messages;
}

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

  // Auto-scroll on new message OR content update
  const lastContent = grouped.length > 0 ? grouped[grouped.length - 1].content : "";
  useEffect(() => {
    if (grouped.length > 0) {
      virtualizer.scrollToIndex(grouped.length - 1, { align: "end" });
    }
  }, [grouped.length, lastContent]);

  if (grouped.length === 0) {
    return <SoulGreeting />;
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
