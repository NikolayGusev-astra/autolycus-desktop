import { useState, useRef, useCallback, useMemo, useEffect, type KeyboardEvent } from "react";
import { Send, Paperclip, X, Mic, ChevronDown, Cpu, Brain, Square, Shield } from "lucide-react";
import type { AgentStatus } from "../../lib/types";
import { useTranslation } from "../../hooks/useTranslation";
import { VoiceInput } from "./VoiceInput";
import { invoke } from "@tauri-apps/api/core";
import { useSettingsStore } from "../../stores/settingsStore";

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
  /** Stop the current generation (abort SSE stream). */
  onStop?: () => void;
  disabled?: boolean;
  agentStatus?: AgentStatus;
}

/// Reasoning effort levels (GPT-5.6 guide practice 2).
const REASONING_LEVELS = ["none", "low", "medium", "high", "xhigh", "max"] as const;
type ReasoningLevel = (typeof REASONING_LEVELS)[number];

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

export function ChatInput({ onSend, onStop, disabled, agentStatus = "idle" }: ChatInputProps) {
  const [text, setText] = useState("");
  const [isSending, setIsSending] = useState(false);
  const [attachments, setAttachments] = useState<Attachment[]>([]);
  const [isDragging, setIsDragging] = useState(false);
  const [slashIdx, setSlashIdx] = useState(0);
  const [currentModel, setCurrentModel] = useState<string>("");
  const [reasoningEffort, setReasoningEffort] = useState<ReasoningLevel | null>(null);
  const [showReasoningMenu, setShowReasoningMenu] = useState(false);
  const [autonomyPolicy, setAutonomyPolicy] = useState<string | null>(null);
  const [showAutonomyMenu, setShowAutonomyMenu] = useState(false);
  const [showModelDropdown, setShowModelDropdown] = useState(false);
  const [savedModels, setSavedModels] = useState<any[]>([]);
  const [activeModelCaps, setActiveModelCaps] = useState<{
    supports_reasoning?: boolean;
    supports_vision?: boolean;
    supports_tools?: boolean;
    context_length?: number;
  }>({});
  const inputRef = useRef<HTMLInputElement>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const { t } = useTranslation();
  const modelConfig = useSettingsStore((s) => s.modelConfig);
  // Round-trip the existing proxy config so selecting a model in the chat
  // dropdown does not clobber the user's proxy settings. The previous code
  // sent `proxy: null`, which Rust deserialised as `None` and wiped the block.
  const prevProxy = (modelConfig as any)?.proxy ?? null;

  // Load saved models on mount + read the active model's steer settings
  useEffect(() => {
    invoke<any[]>("list_models_cmd")
      .then((models) => {
        setSavedModels(models);
        if (models.length > 0) setCurrentModel(models[0].id);
      })
      .catch(() => {});
    invoke<any>("get_model_config_cmd", { profile: null })
      .then((config) => {
        if (config?.model) setCurrentModel(`${config.provider}/${config.model}`);
        // Read steer settings from config so toggles reflect persisted state.
        if (config?.reasoning_effort) {
          setReasoningEffort(config.reasoning_effort as ReasoningLevel);
        }
        if (config?.autonomy_policy) {
          setAutonomyPolicy(config.autonomy_policy as string);
        }
      })
      .catch(() => {});
  }, []);

  // When the active model changes (via dropdown), update capability flags so
  // the reasoning toggle only shows for models that support it.
  useEffect(() => {
    const match = savedModels.find(
      (m) => `${m.provider}/${m.model}` === currentModel
    );
    if (match) {
      setActiveModelCaps({
        supports_reasoning: match.supports_reasoning ?? false,
        supports_vision: match.supports_vision ?? false,
        supports_tools: match.supports_tools ?? true,
        context_length: match.context_length,
      });
    }
  }, [currentModel, savedModels]);

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
      className={`flex flex-col gap-2 px-4 py-3 border-t border-ac-border ${isDragging ? "bg-ac-brand-soft" : ""}`}
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
          aria-label={t("chat.attach_file") || "Attach file"}
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

        {/* Model selector */}
        <div className="relative">
          <button
            onClick={() => setShowModelDropdown(!showModelDropdown)}
            className="flex items-center gap-1 px-2 py-1.5 text-[10px] text-ac-muted hover:text-ac-brand border border-ac-border rounded-md"
            title={t("chat.selectModel") || "Выбрать модель"}
            aria-label={t("chat.selectModel") || "Select model"}
          >
            <Cpu className="w-3 h-3" />
            <span className="max-w-24 truncate">{currentModel || t("chat.selectModel") || "Модель"}</span>
            <ChevronDown className="w-3 h-3" />
          </button>
          {showModelDropdown && (
            <div className="absolute bottom-full left-0 mb-1 w-64 rounded-lg border border-ac-border bg-ac-surface shadow-lg overflow-hidden z-20">
              {savedModels.map((m) => (
                <button
                  key={m.id}
                  type="button"
                  className={`w-full flex items-center justify-between px-3 py-2 text-left text-xs ${currentModel === m.name ? "bg-ac-brand-soft" : ""}`}
                  onClick={() => {
                    // Actually persist the selection so the next message uses
                    // this model. Round-trip proxy + steer settings.
                    invoke("set_model_config_cmd", {
                      provider: m.provider,
                      model: m.model,
                      baseUrl: m.base_url ?? "",
                      proxy: prevProxy,
                      profile: null,
                      reasoningEffort: reasoningEffort,
                      verbosity: (modelConfig as any)?.verbosity ?? null,
                      autonomyPolicy: autonomyPolicy,
                      promptCache: (modelConfig as any)?.prompt_cache ?? null,
                      reasoningContext: (modelConfig as any)?.reasoning_context ?? null,
                    })
                      .then(() => setCurrentModel(m.name))
                      .catch((err) => console.error("Failed to set model:", err));
                    setShowModelDropdown(false);
                  }}
                >
                  <span className="text-ac-ink">{m.name}</span>
                  <span className="text-ac-muted">{m.provider}/{m.model}</span>
                </button>
              ))}
              {savedModels.length === 0 && (
                <div className="px-3 py-2 text-xs text-ac-muted">{t("chat.noModels") || "Нет моделей"}</div>
              )}
            </div>
          )}
        </div>

        {/* Reasoning effort steer — only shown if the active model supports it.
            Dropdown with levels: none/low/medium/high/xhigh/max (practice 2).
            Writes to config so it persists across sessions and is sent in the
            chat request body by the Rust backend. */}
        {activeModelCaps.supports_reasoning && (
          <div className="relative">
            <button
              onClick={() => setShowReasoningMenu(!showReasoningMenu)}
              className={`flex items-center gap-1 px-2 py-1.5 text-[10px] border rounded-md ${
                reasoningEffort && reasoningEffort !== "none"
                  ? "bg-ac-brand/10 text-ac-brand border-ac-brand/30"
                  : "text-ac-muted border-ac-border"
              }`}
              title={t("chat.reasoningMode") || "Уровень рассуждений"}
            >
              <Brain className="w-3 h-3" />
              {reasoningEffort || "auto"}
              <ChevronDown className="w-3 h-3" />
            </button>
            {showReasoningMenu && (
              <div className="absolute bottom-full left-0 mb-1 w-32 rounded-lg border border-ac-border bg-ac-surface shadow-lg overflow-hidden z-20">
                {REASONING_LEVELS.map((level) => (
                  <button
                    key={level}
                    type="button"
                    onClick={() => {
                      setReasoningEffort(level);
                      setShowReasoningMenu(false);
                      // Persist to config. Round-trip all steer fields so changing
                      // one doesn't clobber the others (same fix as proxy).
                      invoke("set_model_config_cmd", {
                        provider: (modelConfig as any)?.provider ?? "",
                        model: (modelConfig as any)?.model ?? "",
                        baseUrl: (modelConfig as any)?.base_url ?? "",
                        proxy: prevProxy,
                        profile: null,
                        reasoningEffort: level,
                        verbosity: (modelConfig as any)?.verbosity ?? null,
                        autonomyPolicy: autonomyPolicy,
                        promptCache: (modelConfig as any)?.prompt_cache ?? null,
                        reasoningContext: (modelConfig as any)?.reasoning_context ?? null,
                      }).catch((err) =>
                        console.error("Failed to set reasoning effort:", err)
                      );
                    }}
                    className={`w-full px-3 py-1.5 text-left text-xs hover:bg-ac-surface-2 ${
                      reasoningEffort === level ? "text-ac-brand font-medium" : "text-ac-ink"
                    }`}
                  >
                    {level}
                  </button>
                ))}
              </div>
            )}
          </div>
        )}

        {/* Autonomy policy steer — practice 3. Compact policy beats long
            "how to behave" text. Three modes: readonly / local / confirm-external.
            Injected as a short system message by the Rust backend. */}
        <div className="relative">
          <button
            onClick={() => setShowAutonomyMenu(!showAutonomyMenu)}
            className={`flex items-center gap-1 px-2 py-1.5 text-[10px] border rounded-md ${
              autonomyPolicy && autonomyPolicy !== "readonly"
                ? "bg-ac-brand/10 text-ac-brand border-ac-brand/30"
                : "text-ac-muted border-ac-border"
            }`}
            title="Границы автономии"
          >
            <Shield className="w-3 h-3" />
            {autonomyPolicy === "readonly" ? "read" :
             autonomyPolicy === "local" ? "local" :
             autonomyPolicy === "confirm-external" ? "confirm" : "auto"}
            <ChevronDown className="w-3 h-3" />
          </button>
          {showAutonomyMenu && (
            <div className="absolute bottom-full left-0 mb-1 w-48 rounded-lg border border-ac-border bg-ac-surface shadow-lg overflow-hidden z-20">
              {([
                { val: null, label: "Auto (default)" },
                { val: "readonly", label: "Read-only: report only" },
                { val: "local", label: "Local: act + check" },
                { val: "confirm-external", label: "Confirm external" },
              ] as const).map((opt) => (
                <button
                  key={opt.label}
                  type="button"
                  onClick={() => {
                    setAutonomyPolicy(opt.val);
                    setShowAutonomyMenu(false);
                    invoke("set_model_config_cmd", {
                      provider: (modelConfig as any)?.provider ?? "",
                      model: (modelConfig as any)?.model ?? "",
                      baseUrl: (modelConfig as any)?.base_url ?? "",
                      proxy: prevProxy,
                      profile: null,
                      reasoningEffort: reasoningEffort,
                      verbosity: (modelConfig as any)?.verbosity ?? null,
                      autonomyPolicy: opt.val,
                      promptCache: (modelConfig as any)?.prompt_cache ?? null,
                      reasoningContext: (modelConfig as any)?.reasoning_context ?? null,
                    }).catch((err) =>
                      console.error("Failed to set autonomy policy:", err)
                    );
                  }}
                  className={`w-full px-3 py-1.5 text-left text-xs hover:bg-ac-surface-2 ${
                    autonomyPolicy === opt.val ? "text-ac-brand font-medium" : "text-ac-ink"
                  }`}
                >
                  {opt.label}
                </button>
              ))}
            </div>
          )}
        </div>

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
          aria-label={t("chat.send") || "Send message"}
          className="ac-btn px-4 py-2 text-sm disabled:opacity-30 disabled:cursor-not-allowed"
        >
          <Send className="w-4 h-4" />
        </button>

        {/* Stop button — shown when the agent is actively generating. Aborts
            the current SSE stream so the user doesn't have to wait or kill the
            app. Replaces the old "input fully locked during generation" UX. */}
        {isBlocked && onStop && (
          <button
            onClick={onStop}
            className="px-4 py-2 text-sm rounded-lg border border-ac-red/40 text-ac-red hover:bg-ac-red/10 transition-colors"
            title={t("chat.stop") || "Остановить генерацию"}
            aria-label={t("chat.stop") || "Stop generation"}
          >
            <Square className="w-4 h-4" />
          </button>
        )}
      </div>
    </div>
  );
}
