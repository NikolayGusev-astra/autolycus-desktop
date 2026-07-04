// src/components/views/FeedView.tsx
// The unified activity feed — the main "Command Center" screen. Renders cards
// from all connected sources (email/Telegram/Jira/...) pulled from the Hermes
// state.db via list_feed_cmd. Each card shows source icon, preview, time, and
// quick actions (open / reply / task / delegate).

import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Mail, Send, CheckSquare, Terminal, Bot, FileText,
  Loader, RefreshCw, Plus, ChevronRight,
} from "lucide-react";
import { useTranslation } from "../../hooks/useTranslation";

interface FeedItem {
  session_id: string;
  source: string;
  started_at: number;
  title: string | null;
  preview: string;
  message_count: number;
  model: string;
}

// Source → icon + color mapping.
const SOURCE_META: Record<string, { icon: typeof Mail; color: string; label: string }> = {
  telegram: { icon: Send, color: "#0088cc", label: "Telegram" },
  telethon: { icon: Send, color: "#0088cc", label: "Telegram" },
  email: { icon: Mail, color: "#ea4335", label: "Почта" },
  jira: { icon: CheckSquare, color: "#0052cc", label: "Jira" },
  api_server: { icon: Bot, color: "#6b7280", label: "API" },
  cli: { icon: Terminal, color: "#6b7280", label: "CLI" },
  tui: { icon: Terminal, color: "#6b7280", label: "TUI" },
};
const DEFAULT_META = { icon: FileText, color: "#6b7280", label: "Источник" };

function timeAgo(ts: number): string {
  const diff = Date.now() / 1000 - ts;
  if (diff < 60) return "только что";
  if (diff < 3600) return `${Math.floor(diff / 60)} мин назад`;
  if (diff < 86400) return `${Math.floor(diff / 3600)} ч назад`;
  return `${Math.floor(diff / 86400)} дн назад`;
}

export function FeedView({ onOpenChat, onNewTask, onOpenSession }: { onOpenChat?: () => void; onNewTask?: () => void; onOpenSession?: (sessionId: string) => void }) {
  const { t } = useTranslation();
  const [items, setItems] = useState<FeedItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [filter, setFilter] = useState<string>("all");
  const [briefing, setBriefing] = useState<string | null>(null);
  const [briefingLoading, setBriefingLoading] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const result = await invoke<FeedItem[]>("list_feed_cmd", { limit: 50, profile: null });
      setItems(result);
    } catch (e) {
      console.error("feed load failed", e);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { void load(); }, [load]);

  const generateBriefing = useCallback(async () => {
    setBriefingLoading(true);
    try {
      const result = await invoke<string>("send_message_cmd", {
        request: {
          text: "Дай краткий брифинг: проанализируй последние активности и скажи, что требует моего внимания сегодня. 3 главных пункта. Будь краток.",
          session_id: null,
          history: null,
        },
      });
      setBriefing(result);
    } catch (e) {
      setBriefing("Не удалось сгенерировать брифинг: " + String(e));
    } finally {
      setBriefingLoading(false);
    }
  }, []);

  // Unique sources for filter chips.
  const sources = Array.from(new Set(items.map((i) => i.source)));
  const filtered = filter === "all" ? items : items.filter((i) => i.source === filter);

  return (
    <div className="p-6 max-w-3xl mx-auto">
      {/* Header */}
      <div className="flex items-center justify-between mb-5">
        <div>
          <h2 className="text-lg font-semibold text-ac-ink">{t("feed.title")}</h2>
          <p className="text-xs text-ac-muted mt-0.5">{t("feed.subtitle")}</p>
        </div>
        <div className="flex gap-2">
          <button onClick={() => void load()} className="p-2 rounded-md border border-ac-border text-ac-muted hover:text-ac-brand" title={t("btn.refresh")}>
            {loading ? <Loader className="w-4 h-4 animate-spin" /> : <RefreshCw className="w-4 h-4" />}
          </button>
          {onNewTask && (
            <button onClick={onNewTask} className="ac-btn px-3 py-2 text-sm flex items-center gap-1.5">
              <Plus className="w-4 h-4" /> {t("dash.newTask")}
            </button>
          )}
        </div>
      </div>

      {/* AI Briefing — hero feature */}
      <div className="mb-5 p-4 rounded-lg border border-ac-brand-border bg-ac-brand-soft">
        <div className="flex items-center justify-between mb-2">
          <div className="flex items-center gap-2">
            <Bot className="w-4 h-4 text-ac-brand" />
            <span className="text-sm font-semibold text-ac-ink">{t("feed.briefing")}</span>
          </div>
          <button
            onClick={() => void generateBriefing()}
            disabled={briefingLoading}
            className="text-xs text-ac-brand hover:underline flex items-center gap-1"
          >
            {briefingLoading ? <Loader className="w-3 h-3 animate-spin" /> : <RefreshCw className="w-3 h-3" />}
            {briefing ? t("feed.updateBriefing") : t("feed.generateBriefing")}
          </button>
        </div>
        {briefing ? (
          <p className="text-sm text-ac-ink-2 whitespace-pre-wrap leading-relaxed">{briefing}</p>
        ) : (
          <p className="text-xs text-ac-muted">{t("feed.briefingHint")}</p>
        )}
      </div>

      {/* Source filters */}
      {sources.length > 1 && (
        <div className="flex gap-1.5 mb-4 flex-wrap">
          <button
            onClick={() => setFilter("all")}
            className={`px-2.5 py-1 text-xs rounded-full ${filter === "all" ? "bg-ac-brand text-white" : "bg-ac-surface text-ac-muted border border-ac-border"}`}
          >
            {t("feed.all")}
          </button>
          {sources.map((s) => {
            const meta = SOURCE_META[s] || DEFAULT_META;
            return (
              <button
                key={s}
                onClick={() => setFilter(s)}
                className={`px-2.5 py-1 text-xs rounded-full flex items-center gap-1 ${filter === s ? "text-white" : "bg-ac-surface text-ac-muted border border-ac-border"}`}
                style={filter === s ? { background: meta.color } : {}}
              >
                <meta.icon className="w-3 h-3" /> {meta.label}
              </button>
            );
          })}
        </div>
      )}

      {/* Feed cards */}
      {loading ? (
        <div className="flex justify-center py-12"><Loader className="w-6 h-6 animate-spin text-ac-muted" /></div>
      ) : filtered.length === 0 ? (
        <div className="text-center py-16">
          <p className="text-sm text-ac-muted">{t("feed.empty")}</p>
          {onOpenChat && (
            <button onClick={onOpenChat} className="mt-3 text-sm text-ac-brand hover:underline">
              {t("feed.askAssistant")}
            </button>
          )}
        </div>
      ) : (
        <div className="space-y-2">
          {filtered.map((item) => {
            const meta = SOURCE_META[item.source] || DEFAULT_META;
            const Icon = meta.icon;
            return (
              <div
                key={item.session_id}
                className="group p-3.5 rounded-lg border border-ac-border bg-ac-surface hover:border-ac-brand-border transition-colors cursor-pointer"
                onClick={() => onOpenSession ? onOpenSession(item.session_id) : onOpenChat?.()}
              >
                <div className="flex items-start gap-3">
                  {/* Source icon */}
                  <div className="w-9 h-9 rounded-lg flex items-center justify-center shrink-0" style={{ background: meta.color + "18" }}>
                    <Icon className="w-4 h-4" style={{ color: meta.color }} />
                  </div>
                  {/* Content */}
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2 mb-0.5">
                      <span className="text-[10px] font-medium uppercase tracking-wide" style={{ color: meta.color }}>{meta.label}</span>
                      <span className="text-[10px] text-ac-faint">{timeAgo(item.started_at)}</span>
                      <span className="text-[10px] text-ac-faint">· {item.message_count} {t("feed.msgs")}</span>
                    </div>
                    <p className="text-sm text-ac-ink truncate">
                      {item.title || item.preview || t("feed.untitled")}
                    </p>
                    {item.preview && item.title && (
                      <p className="text-xs text-ac-muted truncate mt-0.5">{item.preview}</p>
                    )}
                  </div>
                  {/* Chevron */}
                  <ChevronRight className="w-4 h-4 text-ac-faint shrink-0 mt-2 opacity-0 group-hover:opacity-100 transition-opacity" />
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

export default FeedView;
