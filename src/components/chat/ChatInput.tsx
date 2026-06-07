import { useState, useRef, useCallback, type KeyboardEvent } from "react";
import { Send } from "lucide-react";
import type { AgentStatus } from "../../lib/types";

interface ChatInputProps {
  onSend: (message: string) => void;
  disabled?: boolean;
  agentStatus?: AgentStatus;
}

const STATUS_PLACEHOLDER: Record<AgentStatus, string> = {
  idle: "Сообщение...",
  thinking: "Агент думает...",
  streaming: "Агент отвечает...",
  tool_calling: "Выполняет команду...",
  error: "Ошибка — попробуйте ещё раз",
};

export function ChatInput({ onSend, disabled, agentStatus = "idle" }: ChatInputProps) {
  const [text, setText] = useState("");
  const [isSending, setIsSending] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  const isBlocked = disabled || isSending || agentStatus === "thinking" || agentStatus === "streaming" || agentStatus === "tool_calling";
  const placeholder = STATUS_PLACEHOLDER[agentStatus] || "Сообщение...";

  const handleSend = useCallback(async () => {
    if (text.trim() && !isBlocked) {
      setIsSending(true);
      try {
        await onSend(text.trim());
        setText("");
      } finally {
        setIsSending(false);
      }
    }
  }, [text, isBlocked, onSend]);

  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  return (
    <div className="flex gap-2 px-4 py-3 border-t border-ac-border">
      <input
        ref={inputRef}
        type="text"
        value={text}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={handleKeyDown}
        placeholder={placeholder}
        disabled={isBlocked}
        className="ac-input flex-1 px-3.5 py-2 text-sm"
      />
      <button
        onClick={handleSend}
        disabled={!text.trim() || isBlocked}
        className="ac-btn px-4 py-2 text-sm disabled:opacity-30 disabled:cursor-not-allowed"
      >
        <Send className="w-4 h-4" />
      </button>
    </div>
  );
}
