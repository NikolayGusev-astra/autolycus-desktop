import { useState, useRef, useCallback, type KeyboardEvent } from "react";
import { Send } from "lucide-react";

interface ChatInputProps {
  onSend: (message: string) => void;
  disabled?: boolean;
}

export function ChatInput({ onSend, disabled }: ChatInputProps) {
  const [text, setText] = useState("");
  const [isSending, setIsSending] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  const handleSend = useCallback(async () => {
    if (text.trim() && !disabled && !isSending) {
      setIsSending(true);
      try {
        await onSend(text.trim());
        setText("");
      } finally {
        setIsSending(false);
      }
    }
  }, [text, disabled, isSending, onSend]);

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
        placeholder="Сообщение..."
        disabled={disabled || isSending}
        className="ac-input flex-1 px-3.5 py-2 text-sm"
      />
      <button
        onClick={handleSend}
        disabled={!text.trim() || disabled || isSending}
        className="ac-btn px-4 py-2 text-sm disabled:opacity-30 disabled:cursor-not-allowed"
      >
        <Send className="w-4 h-4" />
      </button>
    </div>
  );
}
