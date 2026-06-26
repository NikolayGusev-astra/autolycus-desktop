import { useState } from "react";
import { Copy, RefreshCw, Check, User } from "lucide-react";
import { AgentMarkdown } from "../AgentMarkdown";
import { useTranslation } from "../../hooks/useTranslation";

interface Message {
  id: string;
  role: string;
  content: string;
  timestamp: number;
  isStreaming?: boolean;
}

interface MessageBubbleProps {
  message: Message;
  onRegenerate?: () => void;
  canRegenerate?: boolean;
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
          isUser ? "bg-ac-amber/20" : "bg-ac-surface border border-ac-border"
        }`}
      >
        {isUser ? (
          <User className="w-4 h-4 text-ac-amber" />
        ) : (
          <span className="text-xs font-display font-bold text-ac-amber">Ш</span>
        )}
      </div>

      {/* Content */}
      <div className={`flex flex-col gap-1 max-w-[80%] ${isUser ? "items-end" : "items-start"}`}>
        <div
          className={`rounded-lg px-4 py-2 text-sm ${
            isUser
              ? "bg-ac-amber/15 text-ac-ivory"
              : "bg-ac-surface text-ac-ivory border border-ac-border"
          }`}
        >
          {isUser ? (
            <p className="whitespace-pre-wrap">{message.content}</p>
          ) : (
            <AgentMarkdown>{message.content}</AgentMarkdown>
          )}
          {message.isStreaming && (
            <span className="inline-block w-2 h-4 bg-ac-amber/60 animate-pulse ml-1" />
          )}
        </div>

        {/* Action buttons — visible on hover */}
        {!message.isStreaming && (
          <div className="flex gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
            <button
              onClick={handleCopy}
              title={t("btn.copy") || "Копировать"}
              className="p-1 rounded hover:bg-ac-surface text-ac-stone hover:text-ac-ivory transition-colors"
            >
              {copied ? <Check className="w-3 h-3 text-ac-green" /> : <Copy className="w-3 h-3" />}
            </button>
            {!isUser && canRegenerate && onRegenerate && (
              <button
                onClick={onRegenerate}
                title={t("btn.regenerate") || "Перегенерировать"}
                className="p-1 rounded hover:bg-ac-surface text-ac-stone hover:text-ac-ivory transition-colors"
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
