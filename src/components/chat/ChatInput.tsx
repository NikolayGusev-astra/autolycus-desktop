import { useState, useRef, useCallback, useMemo, type KeyboardEvent } from "react";
import { Send, Paperclip, X, Mic } from "lucide-react";
import type { AgentStatus } from "../../lib/types";
import { useTranslation } from "../../hooks/useTranslation";
import { VoiceInput } from "./VoiceInput";

/** A user attachment. Either a browser File (will be saved on send) or a
 * media clip already saved to disk by the backend (e.g. a voice recording). */
export interface Attachment {
  file?: File;
  /** For saved clips (voice). */
  path?: string;
  kind: "image" | "audio" | "video" | "file";
  mime?: string;
  name: string;
  size?: number;
}

interface ChatInputProps {
  onSend: (message: string, attachments?: Attachment[]) => void;
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

/** Heuristic kind from a file's mime/extension. */
function kindOf(file: File): Attachment["kind"] {
  const t = file.type.toLowerCase();
  if (t.startsWith("image/")) return "image";
  if (t.startsWith("audio/")) return "audio";
  if (t.startsWith("video/")) return "video";
  return "file";
}

const URL_RE = /https?:\/\/[^\s]+/;

// Slash commands — like Hermes CLI/Telegram. Each has a trigger, label, and
// optional handler that returns the text to send (or undefined to skip send).
const SLASH_COMMANDS: { cmd: string; label: string; desc: string }[] = [
  { cmd: "/model", label: "/model", desc: "Сменить модель" },
  { cmd: "/clear", label: "/clear", desc: "Очистить контекст" },
  { cmd: "/compact", label: "/compact", desc: "Сжать диалог" },
  { cmd: "/profile", label: "/profile", desc: "Сменить профиль" },
  { cmd: "/help", label: "/help", desc: "Помощь" },
  { cmd: "/tasks", label: "/tasks", desc: "Извлечь задачи" },
  { cmd: "/skills", label: "/skills", desc: "Список навыков" },
  { cmd: "/cost", label: "/cost", desc: "Стоимость сессии" },
];

export function ChatInput({ onSend, disabled, agentStatus = "idle" }: ChatInputProps) {
  const [text, setText] = useState("");
  const [isSending, setIsSending] = useState(false);
  const [attachments, setAttachments] = useState<Attachment[]>([]);
  const [isDragging, setIsDragging] = useState(false);
  const [slashIdx, setSlashIdx] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const { t } = useTranslation();

  // Slash menu: show when the input starts with "/".
  const slashOpen = text.startsWith("/") && !text.includes(" ");
  const slashMatches = useMemo(
    () => (slashOpen ? SLASH_COMMANDS.filter((c) => c.cmd.startsWith(text)) : []),
    [slashOpen, text]
  );

  const isBlocked =
    disabled ||
    isSending ||
    agentStatus === "thinking" ||
    agentStatus === "streaming" ||
    agentStatus === "tool_calling";
  const placeholder = t(STATUS_PLACEHOLDER_KEY[agentStatus] || "chat_placeholder_idle");

  const handleSend = useCallback(async () => {
    if ((text.trim() || attachments.length > 0) && !isBlocked) {
      // Snapshot what we're sending, then clear the input IMMEDIATELY — before
      // awaiting onSend. The previous code awaited onSend (which blocks until
      // the agent fully responds), leaving the typed text stuck in the field
      // for the whole turn.
      const msg = text.trim();
      const atts = attachments.length > 0 ? attachments : undefined;
      setText("");
      setAttachments([]);
      setIsSending(true);
      try {
        await onSend(msg, atts);
      } finally {
        setIsSending(false);
      }
    }
  }, [text, isBlocked, onSend, attachments]);

  const handleKeyDown = (e: KeyboardEvent) => {
    // Slash-menu navigation: ArrowUp/Down to move, Enter/Tab to select, Esc to close.
    if (slashMatches.length > 0) {
      if (e.key === "ArrowDown") { e.preventDefault(); setSlashIdx((i) => (i + 1) % slashMatches.length); return; }
      if (e.key === "ArrowUp") { e.preventDefault(); setSlashIdx((i) => (i - 1 + slashMatches.length) % slashMatches.length); return; }
      if (e.key === "Enter" || e.key === "Tab") {
        e.preventDefault();
        const chosen = slashMatches[slashIdx];
        if (chosen) { setText(chosen.cmd + " "); inputRef.current?.focus(); }
        return;
      }
      if (e.key === "Escape") { e.preventDefault(); setText(""); return; }
    }
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  const handleFileSelect = (files: FileList | null) => {
    if (!files) return;
    const valid = Array.from(files)
      .filter((f) => f.size < 50 * 1024 * 1024) // 50MB
      .map<Attachment>((f) => ({
        file: f,
        kind: kindOf(f),
        mime: f.type,
        name: f.name,
        size: f.size,
      }));
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

  // Detect a pasted/typed URL — surface it as an attachment chip the agent can
  // fetch/transcribe (mirrors shturman.ai URL handling).
  const handlePaste = (e: React.ClipboardEvent) => {
    const pasted = e.clipboardData.getData("text");
    const url = pasted.trim().match(URL_RE)?.[0];
    if (url) {
      e.preventDefault();
      setAttachments((prev) => [
        ...prev,
        { kind: "file", name: url, mime: "text/uri-list" },
      ]);
      // Also keep the URL in the text so it's visible/editable.
      setText((prev) => (prev ? `${prev} ${url}` : url));
    }
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
          {attachments.map((att, idx) => (
            <div
              key={idx}
              className="flex items-center gap-1.5 rounded-md bg-ac-surface px-2 py-1 text-xs text-ac-muted"
            >
              {att.kind === "audio" ? (
                <Mic className="w-3 h-3" />
              ) : (
                <Paperclip className="w-3 h-3" />
              )}
              <span className="max-w-40 truncate">{att.name}</span>
              {att.size ? (
                <span className="text-ac-faint">{(att.size / 1024).toFixed(0)}KB</span>
              ) : null}
              <button onClick={() => removeAttachment(idx)} className="hover:text-ac-red">
                <X className="w-3 h-3" />
              </button>
            </div>
          ))}
        </div>
      )}

      <div className="flex gap-2 items-center relative">
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
          className="p-2 rounded-lg hover:bg-ac-surface transition-colors disabled:opacity-30 text-ac-muted"
        >
          <Paperclip className="w-4 h-4" />
        </button>

        {/* Voice input — two modes: on-the-fly transcription (text into the
            input box) or voice-note attachment. */}
        <VoiceInput
          onTranscribed={(text) => {
            // Append recognized text to the current input (with a separator).
            setText((prev) => (prev.trim() ? `${prev.trimEnd()} ${text}` : text));
            inputRef.current?.focus();
          }}
          onRecorded={(att) =>
            setAttachments((prev) => [...prev, att])
          }
        />

        <div className="flex-1 relative">
          {slashMatches.length > 0 && (
            <div className="absolute bottom-full left-0 mb-1 w-64 rounded-lg border border-ac-border bg-ac-surface shadow-lg overflow-hidden z-20">
              {slashMatches.map((c, i) => (
                <button
                  key={c.cmd}
                  type="button"
                  onMouseEnter={() => setSlashIdx(i)}
                  onClick={() => { setText(c.cmd + " "); inputRef.current?.focus(); }}
                  className={`w-full flex items-center justify-between px-3 py-2 text-left text-xs ${i === slashIdx ? "bg-ac-brand-soft" : ""}`}
                >
                  <span className="font-mono text-ac-brand">{c.cmd}</span>
                  <span className="text-ac-muted">{c.desc}</span>
                </button>
              ))}
            </div>
          )}
          <input
            ref={inputRef}
            type="text"
            value={text}
            onChange={(e) => { setText(e.target.value); setSlashIdx(0); }}
            onKeyDown={handleKeyDown}
            onPaste={handlePaste}
            placeholder={placeholder}
            disabled={isBlocked}
            className="ac-input w-full px-3.5 py-2 text-sm"
          />
        </div>

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
