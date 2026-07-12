// src/components/views/FeedView.tsx
// Bento-grid dashboard — replaces the old 3-column "мешанина чатов".
// Tiles: Briefing (cached) | Metrics | My Tasks | Channel preview | Quick ask.
// No auto-LLM on mount: briefing is read from cache, refreshed only on click.

import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Sparkles,
  RefreshCw,
  Send,
  Mail,
  CheckSquare,
  SquareCheckBig,
  Clock,
  AlertTriangle,
  TrendingUp,
  Loader,
  ChevronRight,
  ArrowRight,
  MessageSquare,
} from "lucide-react";
import { useTranslation } from "../../hooks/useTranslation";
import type { WorkTab } from "./WorkView";

interface FeedItem {
  session_id: string;
  source: string;
  started_at: number;
  title: string | null;
  preview: string;
  message_count: number;
  model: string;
}

interface CachedBriefing {
  text: string | null;
  generated_at: number | null;
  stale: boolean;
  title: string | null;
}

interface DashStats {
  tasks_total: number;
  tasks_done: number;
  tasks_today: number;
  active_tasks: number;
  overdue_tasks: number;
  goals_total: number;
  projects_total: number;
  protocols: number;
}

const SOURCE_ICON: Record<string, typeof Send> = {
  telegram: Send,
  telethon: Send,
  email: Mail,
  jira: CheckSquare,
  cli: SquareCheckBig,
  api_server: SquareCheckBig,
};

function timeAgo(ts: number): string {
  if (!ts || ts <= 0) return "";
  const diff = Date.now() / 1000 - ts;
  if (diff < 60) return "только что";
  if (diff < 3600) return `${Math.floor(diff / 60)} мин назад`;
  if (diff < 86400) return `${Math.floor(diff / 3600)} ч назад`;
  return `${Math.floor(diff / 86400)} дн назад`;
}

export function FeedView({
  onNewTask,
  onOpenSession,
  onOpenChat,
  onOpenWork,
}: {
  onNewTask?: () => void;
  onOpenSession?: (sessionId: string) => void;
  onOpenChat?: (sessionId: string) => void;
  onOpenWork?: (tab: WorkTab) => void;
}) {
  const { t } = useTranslation();
  const [channelItems, setChannelItems] = useState<FeedItem[]>([]);
  const [briefing, setBriefing] = useState<CachedBriefing | null>(null);
  const [stats, setStats] = useState<DashStats | null>(null);
  const [tasks, setTasks] = useState<any[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(false);
  const [briefingLoading, setBriefingLoading] = useState(false);
  const [quickAsk, setQuickAsk] = useState("");

  // ── Load: cached briefing + channel feed + stats + tasks ──────────────
  // No LLM call here — briefing comes from the DB cache.
  const load = useCallback(async () => {
    setError(false);
    try {
      const [channels, cached, dashStats, taskList] = await Promise.all([
        invoke<FeedItem[]>("list_feed_channels_cmd", {
          limitPerSource: 5,
          profile: null,
        }).catch(() => [] as FeedItem[]),
        invoke<CachedBriefing>("get_cached_briefing_cmd", {
          maxAgeSecs: 1800,
          profile: null,
        }).catch(() => null),
        invoke<DashStats>("dash_stats_cmd", { profile: null }).catch(
          () => null
        ),
        invoke<any[]>("list_tasks_cmd", { profile: null }).catch(() => []),
      ]);
      setChannelItems(channels);
      setBriefing(cached);
      setStats(dashStats);
      // Show only active, high-priority tasks (limit 5).
      setTasks(
        (taskList || [])
          .filter(
            (t: any) => t.status !== "done" && t.status !== "completed"
          )
          .sort((a: any, b: any) => (b.priority || 0) - (a.priority || 0))
          .slice(0, 5)
      );
    } catch (e) {
      console.error("dashboard load failed", e);
      setError(true);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  // ── Refresh briefing (manual — by click, not auto) ────────────────────
  const refreshBriefing = useCallback(async () => {
    setBriefingLoading(true);
    try {
      const result = await invoke<any>("generate_smart_briefing_cmd", {
        days: 7,
        profile: null,
      });
      setBriefing({
        text: result.formatted,
        generated_at: Date.now() / 1000,
        stale: false,
        title: result.title,
      });
      // Also refresh channel feed in case new sessions appeared.
      void load();
    } catch (e) {
      console.error("briefing refresh failed", e);
    } finally {
      setBriefingLoading(false);
    }
  }, [load]);

  // ── Quick ask: send to assistant and open chat ────────────────────────
  const handleQuickAsk = useCallback(async () => {
    if (!quickAsk.trim()) return;
    try {
      await invoke("send_message_cmd", {
        request: { text: quickAsk, session_id: null, history: null },
      });
    } catch (e) {
      console.error("quick ask failed", e);
    }
    setQuickAsk("");
    onOpenChat?.("");
  }, [quickAsk, onOpenChat]);

  // ── Loading skeleton ──────────────────────────────────────────────────
  if (loading) {
    return (
      <div className="p-6 max-w-6xl mx-auto">
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
          {[1, 2, 3, 4, 5, 6].map((i) => (
            <div
              key={i}
              className="h-40 rounded-lg border border-ac-border bg-ac-surface animate-pulse"
            />
          ))}
        </div>
      </div>
    );
  }

  // ── Error state ───────────────────────────────────────────────────────
  if (error) {
    return (
      <div className="p-6 max-w-6xl mx-auto">
        <div className="rounded-lg border border-red-300 bg-red-50 dark:bg-red-950/20 dark:border-red-800 p-6 text-center">
          <AlertTriangle className="w-8 h-8 text-ac-red mx-auto mb-3" />
          <p className="text-sm text-ac-ink mb-4">
            {t("feed.errorTitle")}
          </p>
          <button
            onClick={() => {
              setLoading(true);
              void load();
            }}
            className="ac-btn px-4 py-2 text-sm inline-flex items-center gap-2"
          >
            <RefreshCw className="w-4 h-4" /> {t("feed.retry")}
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="h-full overflow-y-auto">
      <div className="p-6 max-w-6xl mx-auto">
        {/* Header */}
        <div className="flex items-center justify-between mb-5">
          <div>
            <h2 className="text-lg font-semibold text-ac-ink">
              {t("feed.title")}
            </h2>
            <p className="text-xs text-ac-muted mt-0.5">
              {t("feed.subtitle")}
            </p>
          </div>
          <button
            onClick={() => void load()}
            className="p-2 rounded-md border border-ac-border text-ac-muted hover:text-ac-brand"
            title={t("feed.refresh")}
          >
            <RefreshCw className="w-4 h-4" />
          </button>
        </div>

        {/* Bento grid */}
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
          {/* ── Briefing tile (span 2 on large screens) ─────────────────── */}
          <div className="md:col-span-2 rounded-lg border border-ac-brand-border bg-ac-brand-soft p-4">
            <div className="flex items-center justify-between mb-2">
              <div className="flex items-center gap-2">
                <Sparkles className="w-4 h-4 text-ac-brand" />
                <span className="text-sm font-semibold text-ac-ink">
                  {t("feed.briefing")}
                </span>
                {briefing?.generated_at && (
                  <span className="text-[10px] text-ac-muted">
                    {timeAgo(briefing.generated_at)}
                  </span>
                )}
                {briefing?.stale && (
                  <span className="text-[10px] text-amber-600 dark:text-amber-400">
                    {t("feed.stale")}
                  </span>
                )}
              </div>
              <button
                onClick={() => void refreshBriefing()}
                disabled={briefingLoading}
                className="text-xs text-ac-brand hover:underline flex items-center gap-1 disabled:opacity-50"
              >
                {briefingLoading ? (
                  <Loader className="w-3 h-3 animate-spin" />
                ) : (
                  <RefreshCw className="w-3 h-3" />
                )}
                {briefing?.text && !briefing.stale
                  ? t("feed.update")
                  : t("feed.generate")}
              </button>
            </div>
            {briefing?.text ? (
              <p className="text-xs text-ac-ink whitespace-pre-wrap line-clamp-6">
                {briefing.text}
              </p>
            ) : (
              <p className="text-xs text-ac-muted">
                {t("feed.briefingHint")}
              </p>
            )}
          </div>

          {/* ── Metrics tile ────────────────────────────────────────────── */}
          <div className="rounded-lg border border-ac-border bg-ac-surface p-4">
            <div className="flex items-center gap-2 mb-3">
              <TrendingUp className="w-4 h-4 text-ac-muted" />
              <span className="text-sm font-semibold text-ac-ink">{t("feed.metrics")}</span>
            </div>
            <div className="space-y-2">
              <button
                onClick={() => onOpenWork?.("tasks")}
                className="w-full flex items-center justify-between text-xs hover:bg-ac-surface-2 rounded px-2 py-1 transition-colors"
              >
                <span className="text-ac-muted flex items-center gap-1.5">
                  <SquareCheckBig className="w-3.5 h-3.5" /> {t("feed.activeTasks")}
                </span>
                <span className="font-semibold text-ac-ink">
                  {stats?.active_tasks ?? "—"}
                </span>
              </button>
              <div className="flex items-center justify-between text-xs px-2 py-1">
                <span className="text-ac-muted flex items-center gap-1.5">
                  <Clock className="w-3.5 h-3.5" /> {t("feed.overdue")}
                </span>
                <span
                  className={`font-semibold ${
                    (stats?.overdue_tasks ?? 0) > 0
                      ? "text-ac-red"
                      : "text-ac-ink"
                  }`}
                >
                  {stats?.overdue_tasks ?? "—"}
                </span>
              </div>
              <button
                onClick={() => onOpenWork?.("goals")}
                className="w-full flex items-center justify-between text-xs hover:bg-ac-surface-2 rounded px-2 py-1 transition-colors"
              >
                <span className="text-ac-muted">{t("feed.goals")}</span>
                <span className="font-semibold text-ac-ink">
                  {stats?.goals_total ?? "—"}
                </span>
              </button>
              <button
                onClick={() => onOpenWork?.("protocols")}
                className="w-full flex items-center justify-between text-xs hover:bg-ac-surface-2 rounded px-2 py-1 transition-colors"
              >
                <span className="text-ac-muted">{t("feed.protocols")}</span>
                <span className="font-semibold text-ac-ink">
                  {stats?.protocols ?? "—"}
                </span>
              </button>
            </div>
          </div>

          {/* ── My Tasks tile ───────────────────────────────────────────── */}
          <div className="md:col-span-2 rounded-lg border border-ac-border bg-ac-surface p-4">
            <div className="flex items-center justify-between mb-3">
              <div className="flex items-center gap-2">
                <SquareCheckBig className="w-4 h-4 text-ac-muted" />
                <span className="text-sm font-semibold text-ac-ink">
                  {t("feed.myTasks")}
                </span>
              </div>
              {onOpenWork && (
                <button
                  onClick={() => onOpenWork("tasks")}
                  className="text-xs text-ac-brand hover:underline flex items-center gap-0.5"
                >
                  {t("feed.allTasks")} <ChevronRight className="w-3 h-3" />
                </button>
              )}
            </div>
            {tasks.length > 0 ? (
              <div className="space-y-1.5">
                {tasks.map((task: any) => (
                  <button
                    key={task.id}
                    onClick={() => onOpenWork?.("tasks")}
                    className="w-full flex items-center gap-2 text-xs text-left px-2 py-1.5 rounded hover:bg-ac-surface-2 transition-colors"
                  >
                    <span
                      className={`w-1.5 h-1.5 rounded-full shrink-0 ${
                        (task.priority || 0) >= 4
                          ? "bg-red-500"
                          : (task.priority || 0) >= 3
                            ? "bg-amber-500"
                            : "bg-ac-muted"
                      }`}
                    />
                    <span className="flex-1 truncate text-ac-ink">
                      {task.title}
                    </span>
                    {task.assignee && (
                      <span className="text-ac-muted text-[10px]">
                        @{task.assignee}
                      </span>
                    )}
                  </button>
                ))}
              </div>
            ) : (
              <button
                onClick={onNewTask}
                className="w-full text-xs text-ac-muted text-center py-4 hover:text-ac-brand transition-colors"
              >
                + {t("feed.noTasks")}
              </button>
            )}
          </div>

          {/* ── Channel preview tile ────────────────────────────────────── */}
          <div className="rounded-lg border border-ac-border bg-ac-surface p-4">
            <div className="flex items-center justify-between mb-3">
              <div className="flex items-center gap-2">
                <Send className="w-4 h-4 text-ac-muted" />
                <span className="text-sm font-semibold text-ac-ink">
                  {t("feed.channels")}
                </span>
              </div>
              <span className="text-[10px] text-ac-muted">
                {channelItems.length} {t("feed.msgs")}
              </span>
            </div>
            {channelItems.length > 0 ? (
              <div className="space-y-2">
                {channelItems.slice(0, 5).map((item) => {
                  const Icon = SOURCE_ICON[item.source] || MessageSquare;
                  return (
                    <button
                      key={item.session_id}
                      onClick={() => onOpenSession?.(item.session_id)}
                      className="w-full flex items-start gap-2 text-xs text-left px-2 py-1.5 rounded hover:bg-ac-surface-2 transition-colors"
                    >
                      <Icon className="w-3.5 h-3.5 mt-0.5 text-ac-muted shrink-0" />
                      <div className="flex-1 min-w-0">
                        <p className="truncate text-ac-ink">
                          {item.title || item.preview || t("feed.noTitle")}
                        </p>
                        <p className="text-[10px] text-ac-muted">
                          {timeAgo(item.started_at)}
                        </p>
                      </div>
                    </button>
                  );
                })}
              </div>
            ) : (
              <p className="text-xs text-ac-muted text-center py-4">
                {t("feed.noMessages")}
              </p>
            )}
          </div>
        </div>

        {/* ── Quick ask bar ─────────────────────────────────────────────── */}
        <div className="mt-4 flex items-center gap-2 max-w-2xl mx-auto">
          <input
            type="text"
            value={quickAsk}
            onChange={(e) => setQuickAsk(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                void handleQuickAsk();
              }
            }}
            placeholder={t("feed.askAssistant")}
            className="flex-1 px-4 py-2.5 rounded-lg border border-ac-border bg-ac-surface text-sm text-ac-ink placeholder-ac-muted focus:outline-none focus:border-ac-brand"
          />
          <button
            onClick={() => void handleQuickAsk()}
            disabled={!quickAsk.trim()}
            className="ac-btn px-4 py-2.5 text-sm flex items-center gap-1.5 disabled:opacity-50"
          >
            <ArrowRight className="w-4 h-4" />
          </button>
        </div>
      </div>
    </div>
  );
}

export default FeedView;
