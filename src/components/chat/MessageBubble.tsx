import { useState, useEffect } from "react";
import { Copy, RefreshCw, Check, User, Mic, FileText, Link as LinkIcon, ListChecks } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { AgentMarkdown } from "../AgentMarkdown";
import { useTranslation } from "../../hooks/useTranslation";
import { useUIStore } from "../../stores/uiStore";
import type { MessageAttachment } from "../../lib/types";

interface Message {
  id: string;
  role: string;
  content: string;
  timestamp: number;
  isStreaming?: boolean;
  attachments?: MessageAttachment[];
}

interface MessageBubbleProps {
  message: Message;
  onRegenerate?: () => void;
  canRegenerate?: boolean;
}

/** Renders a single attachment as an inline preview. Images load their data URL
 * lazily via the media backend; audio shows a native player once resolved;
 * other files show a chip. */
function AttachmentView({ att }: { att: MessageAttachment }) {
  const [dataUrl, setDataUrl] = useState<string | null>(att.dataUrl ?? null);

  useEffect(() => {
    if (att.kind === "image" && att.path && !dataUrl) {
      invoke<string | null>("read_media_data_url_cmd", { path: att.path })
        .then((u) => u && setDataUrl(u))
        .catch(() => {});
    }
  }, [att.kind, att.path, dataUrl]);

  if (att.kind === "url" || (!att.path && att.mime === "text/uri-list")) {
    return (
      <span className="inline-flex items-center gap-1 text-xs text-ac-brand break-all">
        <LinkIcon className="w-3 h-3 shrink-0" />
        {att.name}
      </span>
    );
  }

  if (att.kind === "image") {
    return dataUrl ? (
      <img src={dataUrl} alt={att.name} className="max-h-40 rounded-md border border-ac-border" />
    ) : (
      <span className="text-xs text-ac-muted">{att.name}</span>
    );
  }

  if (att.kind === "audio") {
    return (
      <span className="inline-flex items-center gap-1.5 text-xs bg-ac-bg rounded-md px-2 py-1 border border-ac-border">
        <Mic className="w-3.5 h-3.5 text-ac-brand" />
        <span className="text-ac-muted">{att.name}</span>
      </span>
    );
  }

  // Generic file
  return (
    <span className="inline-flex items-center gap-1.5 text-xs bg-ac-bg rounded-md px-2 py-1 border border-ac-border">
      <FileText className="w-3.5 h-3.5 text-ac-muted" />
      <span className="text-ac-muted truncate max-w-48">{att.name}</span>
    </span>
  );
}

export function MessageBubble({ message, onRegenerate, canRegenerate }: MessageBubbleProps) {
  const [copied, setCopied] = useState(false);
  const { t } = useTranslation();
  const isUser = message.role === "user";

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(message.content);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // clipboard may not be available
    }
  };

  return (
    <div className={`group flex gap-3 px-4 py-3 ${isUser ? "flex-row-reverse" : "flex-row"}`}>
      {/* Avatar */}
      <div
        className={`flex-shrink-0 w-8 h-8 rounded-full flex items-center justify-center ${
          isUser ? "bg-ac-brand/20" : "bg-ac-surface border border-ac-border"
        }`}
        aria-hidden="true"
      >
        {isUser ? (
          <User className="w-4 h-4 text-ac-brand" />
        ) : (
          <span className="text-xs font-display font-bold text-ac-brand">Ш</span>
        )}
      </div>

      {/* Content */}
      <div className={`flex flex-col gap-1 max-w-[80%] ${isUser ? "items-end" : "items-start"}`}>
        {/* Sender name + timestamp */}
        <div className={`flex items-center gap-2 text-[10px] text-ac-faint ${isUser ? "flex-row-reverse" : ""}`}>
          <span className="font-medium text-ac-muted">{isUser ? t("chat.you") || "Вы" : "Штурман"}</span>
          {message.timestamp && (
            <span>
              {new Date(message.timestamp).toLocaleTimeString(
                useUIStore.getState().language === "ru" ? "ru-RU" : "en-US",
                { hour: "2-digit", minute: "2-digit" }
              )}
            </span>
          )}
        </div>

        {/* Attachment previews (above the text) */}
        {message.attachments && message.attachments.length > 0 && (
          <div className="flex flex-wrap gap-1.5 mb-1">
            {message.attachments.map((att, i) => (
              <AttachmentView key={i} att={att} />
            ))}
          </div>
        )}

        <div
          className={`rounded-lg px-4 py-2 text-sm ${
            isUser
              ? "bg-ac-brand/15 text-ac-ink"
              : "bg-ac-surface text-ac-ink border border-ac-border"
          }`}
        >
          {isUser ? (
            message.content ? (
              <p className="whitespace-pre-wrap">{message.content}</p>
            ) : null
          ) : (
            <AgentMarkdown>{message.content}</AgentMarkdown>
          )}
          {message.isStreaming && (
            <span className="inline-block w-2 h-4 bg-ac-brand/60 animate-pulse ml-1" />
          )}
        </div>

        {/* Action buttons — visible on hover AND focus-within (a11y) */}
        {!message.isStreaming && message.content && (
          <div className="flex gap-1 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 transition-opacity">
            <button
              onClick={handleCopy}
              title={t("btn.copy") || "Копировать"}
              aria-label={t("btn.copy") || "Copy message"}
              className="p-1 rounded hover:bg-ac-surface text-ac-muted hover:text-ac-ink transition-colors"
            >
              {copied ? <Check className="w-3 h-3 text-ac-green" /> : <Copy className="w-3 h-3" />}
            </button>
            {!isUser && (
              <button
                onClick={async () => {
                  try {
                    // Ask the agent to structure tasks from this reply as
                    // JSON, then create each in the desktop's task store.
                    const result = await invoke<string>("send_message_cmd", {
                      request: {
                        text: `Извлеки задачи из следующего текста и верни ТОЛЬКО JSON-массив: [{"title":"..."}]. Текст:\n\n${message.content}`,
                        session_id: null,
                        history: null,
                      },
                    });
                    const match = result.match(/\[[\s\S]*\]/);
                    if (match) {
                      const tasks = JSON.parse(match[0]) as { title: string }[];
                      for (const tk of tasks.slice(0, 10)) {
                        if (tk.title) await invoke("create_task_cmd", { title: tk.title, profile: null });
                      }
                    }
                  } catch (e) {
                    console.error("extract tasks failed", e);
                  }
                }}
                title={t("tasks.extract") || "Извлечь задачи"}
                className="p-1 rounded hover:bg-ac-surface text-ac-muted hover:text-ac-ink transition-colors"
              >
                <ListChecks className="w-3 h-3" />
              </button>
            )}
            {!isUser && canRegenerate && onRegenerate && (
              <button
                onClick={onRegenerate}
                title={t("btn.regenerate") || "Перегенерировать"}
                className="p-1 rounded hover:bg-ac-surface text-ac-muted hover:text-ac-ink transition-colors"
              >
                <RefreshCw className="w-3 h-3" />
              </button>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
