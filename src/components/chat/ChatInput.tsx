import { useState, useRef, useCallback, type KeyboardEvent } from "react";
import { Send, Paperclip, X } from "lucide-react";
import type { AgentStatus } from "../../lib/types";
import { useTranslation } from "../../hooks/useTranslation";

interface ChatInputProps {
  onSend: (message: string, attachments?: File[]) => void;
  disabled?: boolean;
  agentStatus?: AgentStatus;
}

const STATUS_PLACEHOLDER_KEY: Record<AgentStatus, string> = {
  idle: "chat_placeholder_idle",
  thinking: "chat_placeholder_thinking",
  streaming: "chat_placeholder_streaming",
  tool_calling: "chat_placeholder_tool_calling",
  error: "chat_placeholder_error",
};

export function ChatInput({ onSend, disabled, agentStatus = "idle" }: ChatInputProps) {
  const [text, setText] = useState("");
  const [isSending, setIsSending] = useState(false);
  const [attachments, setAttachments] = useState<File[]>([]);
  const [isDragging, setIsDragging] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const { t } = useTranslation();

  const isBlocked = disabled || isSending || agentStatus === "thinking" || agentStatus === "streaming" || agentStatus === "tool_calling";
  const placeholder = t(STATUS_PLACEHOLDER_KEY[agentStatus] || "chat_placeholder_idle");

  const handleSend = useCallback(async () => {
    if (text.trim() && !isBlocked) {
      setIsSending(true);
      try {
        await onSend(text.trim(), attachments.length > 0 ? attachments : undefined);
        setText("");
        setAttachments([]);
      } finally {
        setIsSending(false);
      }
    }
  }, [text, isBlocked, onSend, attachments]);

  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  const handleFileSelect = (files: FileList | null) => {
    if (!files) return;
    const valid = Array.from(files).filter((f) => f.size < 50 * 1024 * 1024); // 50MB
    setAttachments((prev) => [...prev, ...valid]);
  };

  const removeAttachment = (idx: number) => {
    setAttachments((prev) => prev.filter((_, i) => i !== idx));
  };

  const handleDrop = (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragging(false);
    handleFileSelect(e.dataTransfer.files);
  };

  const handleDragOver = (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragging(true);
  };

  const handleDragLeave = (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragging(false);
  };

  return (
    <div
      className={`flex flex-col gap-2 px-4 py-3 border-t border-ac-border ${isDragging ? "bg-ac-glow" : ""}`}
      onDrop={handleDrop}
      onDragOver={handleDragOver}
      onDragLeave={handleDragLeave}
    >
      {/* Attachments preview */}
      {attachments.length > 0 && (
        <div className="flex flex-wrap gap-2">
          {attachments.map((file, idx) => (
            <div
              key={idx}
              className="flex items-center gap-1.5 rounded-md bg-ac-surface px-2 py-1 text-xs text-ac-stone"
            >
              <Paperclip className="w-3 h-3" />
              <span className="max-w-32 truncate">{file.name}</span>
              <span className="text-ac-stone/50">{(file.size / 1024).toFixed(0)}KB</span>
              <button onClick={() => removeAttachment(idx)} className="hover:text-ac-red">
                <X className="w-3 h-3" />
              </button>
            </div>
          ))}
        </div>
      )}

      <div className="flex gap-2 items-center">
        {/* Attach button */}
        <input
          ref={fileInputRef}
          type="file"
          multiple
          onChange={(e) => handleFileSelect(e.target.files)}
          className="hidden"
        />
        <button
          onClick={() => fileInputRef.current?.click()}
          disabled={isBlocked}
          title={t("chat.attach_file") || "Прикрепить файл"}
          className="p-2 rounded-lg hover:bg-ac-surface transition-colors disabled:opacity-30"
        >
          <Paperclip className="w-4 h-4" />
        </button>

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
          disabled={(!text.trim() && attachments.length === 0) || isBlocked}
          className="ac-btn px-4 py-2 text-sm disabled:opacity-30 disabled:cursor-not-allowed"
        >
          <Send className="w-4 h-4" />
        </button>
      </div>
    </div>
  );
}
