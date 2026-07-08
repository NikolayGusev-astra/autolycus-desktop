// src/components/views/FeedView.tsx
// Command Center main screen. Dynamic columns by source (chat/email/TG/Jira),
// per-source briefings + a unified briefing, and quick actions on each card
// (open session, create task, summarize).

import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Mail, Send, CheckSquare, Terminal, Bot, FileText, Loader, RefreshCw,
  ChevronRight, ListChecks, Sparkles, Columns, ListPlus, MessageSquare,
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
  briefing_smart: { icon: Sparkles, color: "#10b981", label: "Smart Briefing" },
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

export function FeedView({ onNewTask, onOpenSession, onOpenChat }: {
  onNewTask?: () => void;
  onOpenSession?: (sessionId: string) => void;
  onOpenChat?: (sessionId: string) => void;
}) {
  const { t } = useTranslation();
  const [items, setItems] = useState<FeedItem[]>([]);
  const [sourcesConfig, setSourcesConfig] = useState<any>({ telegram: [], email: [], jira: [] });
  const [loading, setLoading] = useState(true);
  const [layout, setLayout] = useState<"columns" | "list">("columns");
  const [briefings, setBriefings] = useState<Record<string, string>>({});
  const [briefingLoading, setBriefingLoading] = useState<string | null>(null);
  const [actionStatus, setActionStatus] = useState("");
  const retryRef = useRef(0);

  const load = useCallback(async () => {
    try {
      const [itemsResult, sourcesResult] = await Promise.all([
        invoke<FeedItem[]>("list_feed_cmd", { limit: 80, profile: null }),
        invoke<any>("list_sources_cmd", { profile: null }),
      ]);
      setItems(itemsResult);
      setSourcesConfig(sourcesResult);
      // Retry once if empty (init timing).
      if (itemsResult.length === 0 && retryRef.current < 2) {
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

  // ── Smart briefing via Tauri + MCP (urgent/important/stale/personal) ────
  const generateSmartBriefing = useCallback(async () => {
    setBriefingLoading("smart");
    setActionStatus("Smart briefing: querying MCP...");
    try {
      const result = await invoke<any>("generate_smart_briefing_cmd", {
        days: 7,
        profile: null,
      });
      setBriefings((p) => ({ ...p, smart: result.formatted }));
      // Refresh feed to surface the new session row from state.db
      void load();
      setActionStatus(`Smart briefing: ${result.title}`);
    } catch (e: any) {
      console.error("smart briefing failed", e);
      setActionStatus("Smart briefing failed: " + (e?.message || String(e)));
    } finally {
      setBriefingLoading(null);
      setTimeout(() => setActionStatus(""), 4000);
    }
  }, [load]);

  // ── Briefing generation ──────────────────────────────────────────────────
  const generateBriefing = useCallback(async (source: string | null) => {
    const key = source || "unified";
    setBriefingLoading(key);
    try {
      // Get items for this source, filter by recent (last 7 days)
      const now = Date.now() / 1000;
      const weekAgo = now - 7 * 24 * 60 * 60;
      const sourceItems = source
        ? items.filter(i => i.source === source && i.started_at >= weekAgo)
        : items.filter(i => i.started_at >= weekAgo);

      // Bug B fix: also aggregate tasks / projects / goals so the briefing
      // isn't just "the last N conversations".
      const [tasksResult, projectsResult, goalsResult] = await Promise.all([
        invoke<any[]>("list_tasks_cmd", { profile: null }).catch(() => []),
        invoke<any[]>("list_projects_cmd", { profile: null }).catch(() => []),
        invoke<any[]>("list_goals_cmd", { profile: null }).catch(() => []),
      ]);
      const activeTasks = (tasksResult || [])
        .filter((t: any) => t.status !== "done" && t.status !== "completed")
        .slice(0, 25);
      const projects = (projectsResult || []).slice(0, 15);
      const goals = (goalsResult || []).slice(0, 10);

      if (sourceItems.length === 0 && activeTasks.length === 0) {
        setBriefings((p) => ({ ...p, [key]: "Нет недавней активности и открытых задач за последние 7 дней." }));
        return;
      }

      // Build context with source attribution
      const sessionContext = sourceItems
        .map(i => `[${i.source}] ${i.title || i.preview} (${new Date(i.started_at * 1000).toLocaleDateString("ru-RU")})`)
        .join("\n");
      const tasksContext = activeTasks.length
        ? activeTasks.map((t: any) => `- [задача #${t.id}] ${t.title}${t.project_id ? ` (проект #${t.project_id})` : ""}${t.assignee ? ` @${t.assignee}` : ""} [${t.status}]`).join("\n")
        : "—";
      const projectsContext = projects.length
        ? projects.map((p: any) => `- [проект #${p.id}] ${p.name}`).join("\n")
        : "—";
      const goalsContext = goals.length
        ? goals.map((g: any) => `- [цель #${g.id}] ${g.title}${g.progress != null ? ` (${g.progress}%)` : ""}`).join("\n")
        : "—";

      const prompt = source
        ? `Проанализируй недавние сообщения из источника "${source}" за последние 7 дней. Дай краткий структурированный брифинг с указанием источников.

Сессии:
${sessionContext}

Открытые задачи (контекст):
${tasksContext}

Формат ответа:
1. **Важное** - что требует внимания
2. **Действия** - что нужно сделать
3. **Риски** - потенциальные проблемы
Каждый пункт с указанием источника в скобках.`
        : `Сделай сводный брифинг по всем источникам за последние 7 дней. Учти сессии, открытые задачи, проекты и цели.

Сессии (последние 7 дней):
${sessionContext}

Открытые задачи:
${tasksContext}

Проекты:
${projectsContext}

Цели:
${goalsContext}

Формат ответа:
1. **Важное** - что требует внимания (источник/задача)
2. **Действия** - что нужно сделать (источник/задача)
3. **Риски** - потенциальные проблемы

Группируй по источникам/проектам, где возможно.`;

      // Bug A fix: deterministic session_id so briefing calls don't spawn new
      // desk-<uuid> sessions that would feed back into the feed (recursion).
      const result = await invoke<string>("send_message_cmd", {
        request: { text: prompt, session_id: `briefing:${key}`, history: null },
      });
      setBriefings((p) => ({ ...p, [key]: result }));
    } catch (e) {
      setBriefings((p) => ({ ...p, [key]: "Ошибка: " + String(e) }));
    } finally {
      setBriefingLoading(null);
    }
  }, [items]);

  // Auto-generate unified briefing once on mount (after items load).
  const autoBriefRef = useRef(false);
  useEffect(() => {
    if (!autoBriefRef.current && items.length > 0 && !briefings["unified"] && briefingLoading === null) {
      autoBriefRef.current = true;
      void generateBriefing(null);
    }
  }, [items, briefings, briefingLoading, generateBriefing]);

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

  // ── Delegate from a feed card (task + assignee) ──────────────────────────
  const delegateFromCard = async (item: FeedItem, assignee: string) => {
    const title = item.title || item.preview?.slice(0, 80) || `Из ${item.source}`;
    try {
      // Detect project from content using LLM
      let projectId: number | null = null;
      try {
        const projectPrompt = `Определи к какому проекту относится эта задача. Список проектов будет предоставлен. Ответь только ID проекта или "none".
Задача: ${title}
Источник: ${item.source}
Контекст: ${item.preview}`;
        const projectResult = await invoke<string>("send_message_cmd", {
          request: { text: projectPrompt, session_id: null, history: null },
        });
        const parsed = parseInt(projectResult.trim());
        if (!isNaN(parsed)) projectId = parsed;
      } catch {
        // ignore project detection errors
      }

      const id = await invoke<number>("create_task_cmd", { title, projectId, profile: null });
      if (assignee) {
        await invoke("update_task_cmd", { id, assignee, profile: null });
      }
      setActionStatus(`✓ Делегировано: ${assignee || "без исполнителя"}${projectId ? ` (проект #${projectId})` : ""}`);
      setTimeout(() => setActionStatus(""), 3000);
    } catch (e) {
      setActionStatus("✗ " + String(e));
    }
  };

  // ── Summarize from a feed card ────────────────────────────────────────────
  const summarizeFromCard = async (item: FeedItem) => {
    try {
      const prompt = `Сделай структурированное резюме (3-5 пунктов) по сессии ${item.session_id} из источника "${item.source}".
Тема: ${item.title || item.preview}.
Дата: ${new Date(item.started_at * 1000).toLocaleDateString("ru-RU")}.

Формат:
1. **Суть** - главная мысль
2. **Детали** - ключевые факты
3. **Действия** - что нужно сделать
4. **Источник** - ${item.source}

Ответь кратко, по пунктам.`;
      const result = await invoke<string>("send_message_cmd", {
        request: { text: prompt, session_id: null, history: null },
      });
      setActionStatus(`✓ Резюме: ${result.slice(0, 80)}...`);
      setTimeout(() => setActionStatus(""), 4000);
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
                <ListPlus className="w-4 h-4" /> {t("nav.tasks")}
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

        {/* Source Briefing Columns */}
        {((sourcesConfig.telegram?.filter((s: any) => s.enabled).length || 0) + (sourcesConfig.email?.filter((s: any) => s.enabled).length || 0) + (sourcesConfig.jira?.filter((s: any) => s.enabled).length || 0)) > 0 && (
          <div className="mb-5 p-4 rounded-lg border border-ac-brand-border bg-ac-brand-soft">
            <div className="flex items-center justify-between mb-3">
              <div className="flex items-center gap-2">
                <Sparkles className="w-4 h-4 text-ac-brand" />
                <span className="text-sm font-semibold text-ac-ink">{t("feed.sourceBriefings")}</span>
              </div>
              <button
                onClick={() => void generateBriefing(null)}
                disabled={briefingLoading === "unified"}
                className="ml-auto text-xs text-ac-brand hover:underline flex items-center gap-1"
              >
                {briefingLoading === "unified" ? <Loader className="w-3 h-3 animate-spin" /> : <Sparkles className="w-3 h-3" />}
                {briefings["unified"] ? t("feed.update") : t("feed.generate")}
              </button>
              <button
                onClick={() => void generateSmartBriefing()}
                disabled={briefingLoading === "smart"}
                className="ml-2 text-xs text-ac-brand hover:underline flex items-center gap-1"
                title="Smart briefing: urgent Jira + emails + sessions via MCP"
              >
                {briefingLoading === "smart" ? <Loader className="w-3 h-3 animate-spin" /> : <Sparkles className="w-3 h-3" />}
                {briefings["smart"] ? "Smart \u21bb" : "Smart briefing"}
              </button>
            </div>
            {briefings["smart"] && (
              <div className="mb-4 p-3 rounded border border-ac-brand-border bg-ac-card/50">
                <div className="text-xs uppercase tracking-wide text-ac-muted mb-2">Smart MCP Briefing</div>
                <p className="text-sm text-ac-ink-2 whitespace-pre-wrap leading-relaxed">{briefings["smart"]}</p>
              </div>
            )}
            {briefings["unified"] ? (
              <p className="text-sm text-ac-ink-2 whitespace-pre-wrap leading-relaxed mb-4">{briefings["unified"]}</p>
            ) : (
              <p className="text-xs text-ac-muted">{t("feed.briefingHint")}</p>
            )}
            
            {/* Per-source briefing columns */}
            <div className="flex gap-4 overflow-x-auto pb-2">
              {sourcesConfig.telegram?.filter((s: any) => s.enabled).map((s: any) => (
                <SourceBriefingColumn key={s.id} source={s} sourceType="telegram" onGenerate={() => generateBriefing(`telegram:${s.id}`)} />
              ))}
              {sourcesConfig.email?.filter((s: any) => s.enabled).map((s: any) => (
                <SourceBriefingColumn key={s.id} source={s} sourceType="email" onGenerate={() => generateBriefing(`email:${s.id}`)} />
              ))}
              {sourcesConfig.jira?.filter((s: any) => s.enabled).map((s: any) => (
                <SourceBriefingColumn key={s.id} source={s} sourceType="jira" onGenerate={() => generateBriefing(`jira:${s.id}`)} />
              ))}
            </div>
          </div>
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
                        onSummarize={() => void summarizeFromCard(item)}
                        onReply={() => onOpenChat?.(item.session_id)}
                        onDelegate={(a) => void delegateFromCard(item, a)}
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
                  onSummarize={() => void summarizeFromCard(item)}
                  onReply={() => onOpenChat?.(item.session_id)}
                  onDelegate={(a) => void delegateFromCard(item, a)}
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
  item, meta, onOpen, onCreateTask, onSummarize, onReply, onDelegate,
}: {
  item: FeedItem;
  meta: { icon: typeof Mail; color: string; label: string };
  onOpen: () => void;
  onCreateTask: () => void;
  onSummarize?: () => void;
  onReply?: () => void;
  onDelegate?: (assignee: string) => void;
}) {
  const Icon = meta.icon;
  const [showDelegate, setShowDelegate] = useState(false);
  const [assignee, setAssignee] = useState("");
  return (
    <div
      className="group p-3 rounded-lg border border-ac-border bg-ac-surface hover:border-ac-brand-border transition-colors cursor-pointer animate-fade-in"
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
      </div>
      {/* Generative-UI quick actions */}
      <div className="flex gap-1 mt-2 ml-10">
        <button
          onClick={(e) => { e.stopPropagation(); onCreateTask(); }}
          className="text-[10px] px-2 py-0.5 rounded text-ac-muted hover:text-ac-brand hover:bg-ac-bg border border-ac-border"
        >
          <ListChecks className="w-2.5 h-2.5 inline" /> {t_global("feed.toTask")}
        </button>
        {onSummarize && (
          <button
            onClick={(e) => { e.stopPropagation(); onSummarize(); }}
            className="text-[10px] px-2 py-0.5 rounded text-ac-muted hover:text-ac-brand hover:bg-ac-bg border border-ac-border"
          >
            <Sparkles className="w-2.5 h-2.5 inline" /> Резюме
          </button>
        )}
        {onReply && (
          <button
            onClick={(e) => { e.stopPropagation(); onReply(); }}
            className="text-[10px] px-2 py-0.5 rounded text-ac-muted hover:text-ac-brand hover:bg-ac-bg border border-ac-border"
          >
            <MessageSquare className="w-2.5 h-2.5 inline" /> Ответить
          </button>
        )}
        <button
          onClick={(e) => { e.stopPropagation(); onOpen(); }}
          className="text-[10px] px-2 py-0.5 rounded text-ac-muted hover:text-ac-brand hover:bg-ac-bg border border-ac-border"
        >
          <ChevronRight className="w-2.5 h-2.5 inline" /> Открыть
        </button>
        {onDelegate && (
          <button
            onClick={(e) => { e.stopPropagation(); setShowDelegate(!showDelegate); }}
            className="text-[10px] px-2 py-0.5 rounded text-ac-muted hover:text-ac-brand hover:bg-ac-bg border border-ac-border"
          >
            👤 Делегировать
          </button>
        )}
      </div>
      {/* Delegate inline form */}
      {showDelegate && onDelegate && (
        <div className="flex gap-1 mt-1.5 ml-10" onClick={(e) => e.stopPropagation()}>
          <input
            className="ac-input flex-1 px-2 py-1 text-xs"
            placeholder="Имя исполнителя"
            value={assignee}
            onChange={(e) => setAssignee(e.target.value)}
            onKeyDown={(e) => { if (e.key === "Enter") { onDelegate(assignee); setShowDelegate(false); setAssignee(""); } }}
          />
          <button
            onClick={() => { onDelegate(assignee); setShowDelegate(false); setAssignee(""); }}
            className="ac-btn px-2 py-1 text-xs"
          >OK</button>
        </div>
      )}
    </div>
  );
}

// Simple t() shim for card (avoids prop drilling).
function t_global(key: string): string {
  return key; // i18n keys fall through gracefully
}

// ── Source Briefing Column ────────────────────────────────────────────────
interface SourceBriefingColumnProps {
  source: any;
  sourceType: "telegram" | "email" | "jira";
  onGenerate: () => void;
}

function SourceBriefingColumn({ source, sourceType, onGenerate }: SourceBriefingColumnProps) {
  const [loading, setLoading] = useState(false);
  const color = sourceType === "telegram" ? "#0088cc" : sourceType === "email" ? "#ea4335" : "#0052cc";

  const handleGenerate = async () => {
    setLoading(true);
    try {
      onGenerate();
      await new Promise(r => setTimeout(r, 500));
    } catch (e) {
      console.error(e);
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="shrink-0 w-72 flex flex-col">
      <div className="flex items-center gap-2 mb-2 px-1">
        <div className="w-7 h-7 rounded-lg flex items-center justify-center" style={{ background: color + "18" }}>
          {sourceType === "telegram" && <Send className="w-3.5 h-3.5" style={{ color }} />}
          {sourceType === "email" && <Mail className="w-3.5 h-3.5" style={{ color }} />}
          {sourceType === "jira" && <CheckSquare className="w-3.5 h-3.5" style={{ color }} />}
        </div>
        <span className="text-sm font-medium text-ac-ink">{source.name}</span>
      </div>

      <button
        onClick={handleGenerate}
        disabled={loading}
        className="mb-2 text-[10px] text-ac-brand hover:underline text-left px-1 flex items-center gap-1"
      >
        {loading ? <Loader className="w-2.5 h-2.5 animate-spin" /> : <Sparkles className="w-2.5 h-2.5" />}
        {loading ? "Генерация..." : "Сгенерировать брифинг"}
      </button>

      {loading && (
        <div className="mb-2 p-2 rounded-md bg-ac-brand-soft border border-ac-brand-border text-[11px] text-ac-ink-2 whitespace-pre-wrap max-h-24 overflow-y-auto">
          <Loader className="w-2.5 h-2.5 animate-spin inline mr-1" />
          Генерация брифинга...
        </div>
      )}
    </div>
  );
}

export default FeedView;
