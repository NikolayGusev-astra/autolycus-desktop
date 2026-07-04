// src/components/views/FeedView.tsx
// Command Center main screen. Dynamic columns by source (chat/email/TG/Jira),
// per-source briefings + a unified briefing, and quick actions on each card
// (open session, create task, summarize).

import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Mail, Send, CheckSquare, Terminal, Bot, FileText, Loader, RefreshCw,
  Plus, ChevronRight, ListChecks, Sparkles, Columns,
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

const SOURCE_META: Record<string, { icon: typeof Mail; color: string; label: string }> = {
  telegram: { icon: Send, color: "#0088cc", label: "Telegram" },
  telethon: { icon: Send, color: "#0088cc", label: "Telegram" },
  email: { icon: Mail, color: "#ea4335", label: "Почта" },
  jira: { icon: CheckSquare, color: "#0052cc", label: "Jira" },
  api_server: { icon: Bot, color: "#6b7280", label: "API" },
  cli: { icon: Terminal, color: "#6b7280", label: "CLI" },
  tui: { icon: Terminal, color: "#6b7280", label: "TUI" },
  mcp: { icon: FileText, color: "#9333ea", label: "MCP" },
};
const DEFAULT_META = { icon: FileText, color: "#6b7280", label: "Источник" };

function timeAgo(ts: number): string {
  if (!ts || ts <= 0) return "";
  const diff = Date.now() / 1000 - ts;
  if (diff < 0) return "только что";
  if (diff < 60) return "только что";
  if (diff < 3600) return `${Math.floor(diff / 60)} мин`;
  if (diff < 86400) return `${Math.floor(diff / 3600)} ч`;
  return `${Math.floor(diff / 86400)} дн`;
}

export function FeedView({ onNewTask, onOpenSession }: {
  onNewTask?: () => void;
  onOpenSession?: (sessionId: string) => void;
}) {
  const { t } = useTranslation();
  const [items, setItems] = useState<FeedItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [layout, setLayout] = useState<"columns" | "list">("columns");
  const [briefings, setBriefings] = useState<Record<string, string>>({});
  const [briefingLoading, setBriefingLoading] = useState<string | null>(null);
  const [actionStatus, setActionStatus] = useState("");
  const retryRef = useRef(0);

  const load = useCallback(async () => {
    try {
      const result = await invoke<FeedItem[]>("list_feed_cmd", { limit: 80, profile: null });
      setItems(result);
      // Retry once if empty (init timing).
      if (result.length === 0 && retryRef.current < 2) {
        retryRef.current++;
        setTimeout(() => void load(), 1500);
        return;
      }
    } catch (e) {
      console.error("feed load failed", e);
      if (retryRef.current < 2) {
        retryRef.current++;
        setTimeout(() => void load(), 1500);
        return;
      }
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { void load(); }, [load]);

  // Group items by source for columns.
  const sources = Array.from(new Set(items.map((i) => i.source)));
  const grouped: Record<string, FeedItem[]> = {};
  for (const s of sources) {
    grouped[s] = items.filter((i) => i.source === s);
  }

  // ── Briefing generation ──────────────────────────────────────────────────
  const generateBriefing = useCallback(async (source: string | null) => {
    const key = source || "unified";
    setBriefingLoading(key);
    try {
      const prompt = source
        ? `Проанализируй последние сообщения из источника "${source}" и дай краткий брифинг: что важно, что требует действий? 3 главных пункта.`
        : "Дай краткий брифинг по всем источникам: что требует моего внимания сегодня? 3 главных пункта.";
      const result = await invoke<string>("send_message_cmd", {
        request: { text: prompt, session_id: null, history: null },
      });
      setBriefings((p) => ({ ...p, [key]: result }));
    } catch (e) {
      setBriefings((p) => ({ ...p, [key]: "Ошибка: " + String(e) }));
    } finally {
      setBriefingLoading(null);
    }
  }, []);

  // ── Create task from a feed card ─────────────────────────────────────────
  const createTaskFromCard = async (item: FeedItem) => {
    const title = item.title || item.preview?.slice(0, 80) || `Из ${item.source}`;
    try {
      await invoke("create_task_cmd", { title, profile: null });
      setActionStatus(`✓ Задача создана: ${title.slice(0, 40)}`);
      setTimeout(() => setActionStatus(""), 3000);
    } catch (e) {
      setActionStatus("✗ " + String(e));
    }
  };

  // ── Render ───────────────────────────────────────────────────────────────
  if (loading) {
    return (
      <div className="flex justify-center py-20">
        <Loader className="w-6 h-6 animate-spin text-ac-muted" />
      </div>
    );
  }

  const isEmpty = items.length === 0;

  return (
    <div className="h-full overflow-y-auto">
      <div className="p-6 max-w-full">
        {/* Header */}
        <div className="flex items-center justify-between mb-5">
          <div>
            <h2 className="text-lg font-semibold text-ac-ink">{t("feed.title")}</h2>
            <p className="text-xs text-ac-muted mt-0.5">{t("feed.subtitle")}</p>
          </div>
          <div className="flex gap-2">
            {/* Layout toggle */}
            <button
              onClick={() => setLayout(layout === "columns" ? "list" : "columns")}
              className="p-2 rounded-md border border-ac-border text-ac-muted hover:text-ac-brand"
              title={layout === "columns" ? t("feed.listView") : t("feed.columnsView")}
            >
              <Columns className="w-4 h-4" />
            </button>
            <button onClick={() => void load()} className="p-2 rounded-md border border-ac-border text-ac-muted hover:text-ac-brand" title="Обновить">
              <RefreshCw className="w-4 h-4" />
            </button>
            {onNewTask && (
              <button onClick={onNewTask} className="ac-btn px-3 py-2 text-sm flex items-center gap-1.5">
                <Plus className="w-4 h-4" /> {t("dash.newTask")}
              </button>
            )}
          </div>
        </div>

        {/* Unified Briefing */}
        <div className="mb-5 p-4 rounded-lg border border-ac-brand-border bg-ac-brand-soft">
          <div className="flex items-center justify-between mb-2">
            <div className="flex items-center gap-2">
              <Sparkles className="w-4 h-4 text-ac-brand" />
              <span className="text-sm font-semibold text-ac-ink">{t("feed.unifiedBriefing")}</span>
            </div>
            <button
              onClick={() => void generateBriefing(null)}
              disabled={briefingLoading === "unified"}
              className="text-xs text-ac-brand hover:underline flex items-center gap-1"
            >
              {briefingLoading === "unified" ? <Loader className="w-3 h-3 animate-spin" /> : <Sparkles className="w-3 h-3" />}
              {briefings["unified"] ? t("feed.update") : t("feed.generate")}
            </button>
          </div>
          {briefings["unified"] ? (
            <p className="text-sm text-ac-ink-2 whitespace-pre-wrap leading-relaxed">{briefings["unified"]}</p>
          ) : (
            <p className="text-xs text-ac-muted">{t("feed.briefingHint")}</p>
          )}
        </div>

        {actionStatus && (
          <div className={`mb-3 text-xs ${actionStatus.startsWith("✓") ? "text-green-500" : "text-ac-red"}`}>{actionStatus}</div>
        )}

        {/* Empty state */}
        {isEmpty ? (
          <div className="text-center py-16">
            <p className="text-sm text-ac-muted mb-2">{t("feed.empty")}</p>
            <p className="text-xs text-ac-faint">{t("feed.emptyHint")}</p>
          </div>
        ) : layout === "columns" ? (
          /* ── Dynamic Columns by Source ── */
          <div className="flex gap-4 overflow-x-auto pb-4">
            {sources.map((src) => {
              const meta = SOURCE_META[src] || DEFAULT_META;
              const Icon = meta.icon;
              const srcItems = grouped[src] || [];
              const briefingKey = src;
              return (
                <div key={src} className="shrink-0 w-80 flex flex-col">
                  {/* Column header */}
                  <div className="flex items-center gap-2 mb-2 px-1">
                    <div className="w-7 h-7 rounded-lg flex items-center justify-center" style={{ background: meta.color + "18" }}>
                      <Icon className="w-3.5 h-3.5" style={{ color: meta.color }} />
                    </div>
                    <span className="text-sm font-medium text-ac-ink">{meta.label}</span>
                    <span className="text-[10px] text-ac-faint ml-auto">{srcItems.length}</span>
                  </div>

                  {/* Per-source briefing */}
                  <button
                    onClick={() => void generateBriefing(src)}
                    disabled={briefingLoading === briefingKey}
                    className="mb-2 text-[10px] text-ac-brand hover:underline text-left px-1 flex items-center gap-1"
                  >
                    {briefingLoading === briefingKey ? <Loader className="w-2.5 h-2.5 animate-spin" /> : <Sparkles className="w-2.5 h-2.5" />}
                    {briefings[briefingKey] ? t("feed.updateBriefing") : t("feed.briefSource")}
                  </button>
                  {briefings[briefingKey] && (
                    <div className="mb-2 p-2 rounded-md bg-ac-brand-soft border border-ac-brand-border text-[11px] text-ac-ink-2 whitespace-pre-wrap max-h-24 overflow-y-auto">
                      {briefings[briefingKey]}
                    </div>
                  )}

                  {/* Cards */}
                  <div className="space-y-1.5 overflow-y-auto flex-1">
                    {srcItems.map((item) => (
                      <FeedCard
                        key={item.session_id}
                        item={item}
                        meta={meta}
                        onOpen={() => onOpenSession?.(item.session_id)}
                        onCreateTask={() => void createTaskFromCard(item)}
                      />
                    ))}
                  </div>
                </div>
              );
            })}
          </div>
        ) : (
          /* ── Flat List Layout ── */
          <div className="space-y-1.5 max-w-3xl">
            {items.map((item) => {
              const meta = SOURCE_META[item.source] || DEFAULT_META;
              return (
                <FeedCard
                  key={item.session_id}
                  item={item}
                  meta={meta}
                  onOpen={() => onOpenSession?.(item.session_id)}
                  onCreateTask={() => void createTaskFromCard(item)}
                />
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}

// ── Feed Card ──────────────────────────────────────────────────────────────
function FeedCard({
  item, meta, onOpen, onCreateTask,
}: {
  item: FeedItem;
  meta: { icon: typeof Mail; color: string; label: string };
  onOpen: () => void;
  onCreateTask: () => void;
}) {
  const Icon = meta.icon;
  return (
    <div
      className="group p-3 rounded-lg border border-ac-border bg-ac-surface hover:border-ac-brand-border transition-colors cursor-pointer"
      onClick={onOpen}
    >
      <div className="flex items-start gap-2.5">
        <div className="w-8 h-8 rounded-lg flex items-center justify-center shrink-0" style={{ background: meta.color + "18" }}>
          <Icon className="w-3.5 h-3.5" style={{ color: meta.color }} />
        </div>
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-1.5 mb-0.5">
            <span className="text-[9px] font-medium uppercase tracking-wide" style={{ color: meta.color }}>{meta.label}</span>
            <span className="text-[9px] text-ac-faint">{timeAgo(item.started_at)}</span>
            <span className="text-[9px] text-ac-faint">· {item.message_count}</span>
          </div>
          <p className="text-xs text-ac-ink truncate">{item.title || item.preview || "Без названия"}</p>
          {item.preview && item.title && (
            <p className="text-[11px] text-ac-muted truncate mt-0.5">{item.preview}</p>
          )}
        </div>
        {/* Quick actions */}
        <div className="flex flex-col gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
          <button
            onClick={(e) => { e.stopPropagation(); onCreateTask(); }}
            className="p-1 rounded text-ac-faint hover:text-ac-brand"
            title={t_global("feed.toTask")}
          >
            <ListChecks className="w-3 h-3" />
          </button>
          <ChevronRight className="w-3 h-3 text-ac-faint mt-1" />
        </div>
      </div>
    </div>
  );
}

// Simple t() shim for card (avoids prop drilling).
function t_global(key: string): string {
  return key; // i18n keys fall through gracefully
}

export default FeedView;
