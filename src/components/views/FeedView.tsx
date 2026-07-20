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
  Calendar,
  AlertTriangle,
  TrendingUp,
  Loader,
  ChevronRight,
  ArrowRight,
  MessageSquare,
  Check,
  X,
  Bell,
  FileText,
} from "lucide-react";
import { useTranslation } from "../../hooks/useTranslation";
import type { WorkTab } from "./WorkView";
import { MarkdownRenderer } from "../chat/MarkdownRenderer";

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

interface EmailMessage {
  id: string;
  uid: string;
  subject: string;
  from: string;
  date: string;
}

interface JiraIssue {
  key: string;
  summary: string;
  status: string;
  priority: string;
  assignee: string;
  updated: string;
}

interface CalendarEvent {
  uid: string;
  summary: string;
  start: string;
  end: string;
  location: string;
  description?: string;
  organizer?: string;
  attendees?: string[];
  recurring?: boolean;
  recurrenceRule?: string;
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

/// Format a calendar event start time (ISO or bare HH:MM) for display.
function formatEventTime(raw: string): string {
  if (!raw) return "";
  // Try full ISO parse first.
  const d = new Date(raw);
  if (!isNaN(d.getTime())) {
    return d.toLocaleString("ru-RU", {
      day: "numeric",
      month: "short",
      hour: "2-digit",
      minute: "2-digit",
    });
  }
  // Bare time like "09:00" — return as-is.
  return raw;
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
  const [emailItems, setEmailItems] = useState<EmailMessage[] | null>(null);
  const [emailError, setEmailError] = useState(false);
  const [emailErrMsg, setEmailErrMsg] = useState<string | null>(null);
  const [jiraItems, setJiraItems] = useState<JiraIssue[] | null>(null);
  const [jiraError, setJiraError] = useState(false);
  const [jiraErrMsg, setJiraErrMsg] = useState<string | null>(null);
  const [calItems, setCalItems] = useState<CalendarEvent[] | null>(null);
  const [calError, setCalError] = useState(false);
  const [calErrMsg, setCalErrMsg] = useState<string | null>(null);

  // L8.5: Meeting reminders + briefing
  const [meetingReminders, setMeetingReminders] = useState<CalendarEvent[]>([]);
  const [meetingBriefing, setMeetingBriefing] = useState<{
    eventUid: string;
    briefingText: string;
    meetingType: string;
  } | null>(null);
  const [meetingBriefingLoading, setMeetingBriefingLoading] = useState<Record<string, boolean>>({});

  const [loading, setLoading] = useState(true);
  const [actionStatus, setActionStatus] = useState("");
  const [error, setError] = useState(false);
  const [briefingLoading, setBriefingLoading] = useState(false);
  const [quickAsk, setQuickAsk] = useState("");
  const [createTaskModal, setCreateTaskModal] = useState<{
    isOpen: boolean;
    source: "jira" | "email" | "confluence" | "calendar";
    externalId: string;
    externalUrl?: string;
    title: string;
  } | null>(null);

  // ── Load: cached briefing + channel feed + stats + tasks + email ──────
  // No LLM call here — briefing comes from the DB cache; email is live
  // (fetched from the email MCP server directly, no agent round-trip).
  const load = useCallback(async () => {
    setError(false);
    try {
      const [channels, cached, dashStats, taskList, email, jira, cal] = await Promise.all([
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
        invoke<EmailMessage[]>("list_email_unread_cmd", {
          profile: null,
        })
          .then((msgs) => {
            setEmailError(false);
            setEmailErrMsg(null);
            return msgs;
          })
          .catch((e) => {
            const msg = typeof e === "string" ? e : String(e);
            console.error("email fetch failed", msg);
            setEmailError(true);
            setEmailErrMsg(msg);
            return null;
          }),
        invoke<JiraIssue[]>("list_jira_my_active_cmd", { profile: null })
          .then((issues) => {
            setJiraError(false);
            setJiraErrMsg(null);
            return issues;
          })
          .catch((e) => {
            const msg = typeof e === "string" ? e : String(e);
            console.error("jira fetch failed", msg);
            setJiraError(true);
            setJiraErrMsg(msg);
            return null;
          }),
        invoke<CalendarEvent[]>("list_calendar_today_cmd", { profile: null })
          .then((events) => {
            setCalError(false);
            setCalErrMsg(null);
            return events;
          })
          .catch((e) => {
            const msg = typeof e === "string" ? e : String(e);
            console.error("calendar fetch failed", msg);
            setCalError(true);
            setCalErrMsg(msg);
            return null;
          }),
      ]);
      setChannelItems(channels);
      setBriefing(cached);
      setStats(dashStats);
      setEmailItems(email);
      setJiraItems(jira);
      setCalItems(cal);

      // L8.5: fetch meeting reminders (events starting within reminder window)
      try {
        const reminders = await invoke<CalendarEvent[]>("list_meeting_reminders_cmd", {
          reminderMinutes: 15,
          profile: null,
        });
        setMeetingReminders(reminders || []);
      } catch (e) {
        console.error("meeting reminders fetch failed", e);
      }
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

  // ADR-007: hybrid refresh — poll every 5 minutes in the background so the
  // email card stays fresh without requiring a tab switch.
  useEffect(() => {
    const interval = setInterval(() => {
      void load();
    }, 5 * 60 * 1000);
    return () => clearInterval(interval);
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

  // ── ADR-008 Phase 2: actionable cards ─────────────────────────────────
  // BUG-3 fix: optimistic local removal so the card disappears immediately,
  // with rollback if the email MCP mark_read fails (e.g. not configured).
  // The 5-min background refresh (load) then confirms the flag persisted.
  const handleMarkEmailRead = useCallback(async (uid: string) => {
    // Snapshot for rollback.
    const prev = emailItems;
    // Optimistically remove from the local list right away.
    setEmailItems((curr) => (curr ? curr.filter((m) => (m.uid || m.id) !== uid) : curr));
    try {
      await invoke("mark_email_read_cmd", { uid, read: true, profile: null });
      setActionStatus("✓ " + t("feed.markedRead"));
      setTimeout(() => setActionStatus(""), 2000);
    } catch (e: any) {
      // Rollback: restore the email if the backend couldn't mark it read.
      setEmailItems(prev);
      setActionStatus("✗ " + (e?.message || String(e)));
      setTimeout(() => setActionStatus(""), 3000);
    }
  }, [emailItems, t]);

  const handleJiraTransition = useCallback(async (key: string, transition: string) => {
    try {
      await invoke("jira_transition_cmd", { issueKey: key, transitionName: transition, profile: null });
      setActionStatus("✓ " + t("feed.jiraUpdated"));
      setTimeout(() => setActionStatus(""), 2000);
      void load();
    } catch (e: any) {
      setActionStatus("✗ " + (e?.message || String(e)));
      setTimeout(() => setActionStatus(""), 3000);
    }
  }, [load, t]);

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

  // ── Create task from external (Jira/Email/Calendar) ───────────────────────────
  const openCreateTaskModal = useCallback((
    source: "jira" | "email" | "confluence" | "calendar",
    externalId: string,
    title: string,
    externalUrl?: string
  ) => {
    setCreateTaskModal({ isOpen: true, source, externalId, title, externalUrl });
  }, []);

  // L8.5: Generate meeting briefing
  const generateMeetingBriefing = useCallback(async (eventUid: string) => {
    setMeetingBriefingLoading((prev) => ({ ...prev, [eventUid]: true }));
    try {
      const result = await invoke<any>("generate_meeting_briefing_cmd", {
        eventUid,
        profile: null,
      });
      setMeetingBriefing({
        eventUid,
        briefingText: result.briefing_text || result.briefingText || "",
        meetingType: result.meeting_type || result.meetingType || "other",
      });
      setActionStatus("✓ " + t("feed.briefingGenerated"));
      setTimeout(() => setActionStatus(""), 2000);
    } catch (e: any) {
      setActionStatus("✗ " + (e?.message || String(e)));
      setTimeout(() => setActionStatus(""), 4000);
    } finally {
      setMeetingBriefingLoading((prev) => ({ ...prev, [eventUid]: false }));
    }
  }, []);

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
              <div className="text-xs text-ac-ink briefing-markdown max-h-72 overflow-y-auto pr-1">
                <MarkdownRenderer content={briefing.text} />
              </div>
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

          {/* ── Email tile (ADR-007: live unread mail) ───────────────────── */}
          <div className="rounded-lg border border-ac-border bg-ac-surface p-4">
            <div className="flex items-center justify-between mb-3">
              <div className="flex items-center gap-2">
                <Mail className="w-4 h-4 text-ac-muted" />
                <span className="text-sm font-semibold text-ac-ink">
                  {t("feed.email")}
                </span>
              </div>
              {emailItems && emailItems.length > 0 && (
                <span className="text-[10px] px-2 py-0.5 rounded-full bg-ac-brand-soft text-ac-brand font-medium">
                  {emailItems.length}
                </span>
              )}
            </div>
            {emailError ? (
              <div className="text-xs text-center py-4 space-y-1">
                <p className="text-ac-muted">
                  {emailErrMsg && emailErrMsg.includes("not configured")
                    ? t("feed.emailNotConfigured")
                    : t("feed.emailError")}
                </p>
                {emailErrMsg && !emailErrMsg.includes("not configured") && (
                  <p className="text-[10px] text-ac-muted/60 truncate" title={emailErrMsg}>
                    {emailErrMsg}
                  </p>
                )}
              </div>
            ) : emailItems === null ? (
              <p className="text-xs text-ac-muted text-center py-4">
                {t("feed.emailLoading")}
              </p>
            ) : emailItems.length === 0 ? (
              <p className="text-xs text-ac-muted text-center py-4">
                {t("feed.emailEmpty")}
              </p>
            ) : (
              <div className="space-y-2">
                {emailItems.slice(0, 5).map((msg) => (
                  <div
                    key={msg.id}
                    className="w-full flex items-start gap-2 text-xs text-left px-2 py-1.5 rounded hover:bg-ac-surface-2 transition-colors group"
                  >
                    <Mail className="w-3.5 h-3.5 mt-0.5 text-ac-muted shrink-0" />
                    <div className="flex-1 min-w-0">
                      <p className="truncate text-ac-ink font-medium">
                        {msg.subject}
                      </p>
                      <p className="truncate text-[10px] text-ac-muted">
                        {msg.from}
                      </p>
                    </div>
                    <div className="flex items-center gap-1 shrink-0">
                      <button
                        onClick={() => handleMarkEmailRead(msg.uid || msg.id)}
                        className="opacity-0 group-hover:opacity-100 p-1 text-ac-muted hover:text-ac-green shrink-0"
                        title={t("feed.markRead")}
                      >
                        <Check className="w-3 h-3" />
                      </button>
                      <button
                        onClick={() => openCreateTaskModal("email", msg.id, msg.subject, undefined)}
                        className="opacity-0 group-hover:opacity-100 p-1 text-ac-muted hover:text-ac-brand shrink-0"
                        title={t("feed.toTask")}
                      >
                        <ArrowRight className="w-3 h-3" />
                      </button>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>

          {/* ── Jira tile (ADR-007 v2: active issues) ──────────────────── */}
          <div className="rounded-lg border border-ac-border bg-ac-surface p-4">
            <div className="flex items-center justify-between mb-3">
              <div className="flex items-center gap-2">
                <CheckSquare className="w-4 h-4 text-ac-muted" />
                <span className="text-sm font-semibold text-ac-ink">
                  {t("feed.jira")}
                </span>
              </div>
              {jiraItems && jiraItems.length > 0 && (
                <span className="text-[10px] px-2 py-0.5 rounded-full bg-ac-brand-soft text-ac-brand font-medium">
                  {jiraItems.length}
                </span>
              )}
            </div>
            {jiraError ? (
              <div className="text-xs text-center py-4 space-y-1">
                <p className="text-ac-muted">
                  {jiraErrMsg && jiraErrMsg.includes("not configured")
                    ? t("feed.jiraNotConfigured")
                    : t("feed.jiraError")}
                </p>
                {jiraErrMsg && !jiraErrMsg.includes("not configured") && (
                  <p className="text-[10px] text-ac-muted/60 truncate" title={jiraErrMsg}>
                    {jiraErrMsg}
                  </p>
                )}
              </div>
            ) : jiraItems === null ? (
              <p className="text-xs text-ac-muted text-center py-4">
                {t("feed.jiraLoading")}
              </p>
            ) : jiraItems.length === 0 ? (
              <p className="text-xs text-ac-muted text-center py-4">
                {t("feed.jiraEmpty")}
              </p>
            ) : (
              <div className="space-y-2">
                {jiraItems.slice(0, 5).map((issue) => (
                  <div
                    key={issue.key}
                    className="w-full flex items-start gap-2 text-xs text-left px-2 py-1.5 rounded hover:bg-ac-surface-2 transition-colors group"
                  >
                    <CheckSquare className="w-3.5 h-3.5 mt-0.5 text-ac-muted shrink-0" />
                    <div className="flex-1 min-w-0">
                      <p className="truncate text-ac-ink font-medium">
                        <span className="text-ac-brand">{issue.key}</span>{" "}
                        {issue.summary}
                      </p>
                      <p className="text-[10px] text-ac-muted">
                        {issue.status}
                        {issue.priority && ` · ${issue.priority}`}
                      </p>
                    </div>
                    <div className="flex items-center gap-1 shrink-0">
                      {issue.status !== "Done" && issue.status !== "Closed" && (
                        <button
                          onClick={() => handleJiraTransition(issue.key, "Done")}
                          className="opacity-0 group-hover:opacity-100 px-1.5 py-0.5 text-[9px] border border-ac-border text-ac-muted rounded hover:text-ac-green shrink-0"
                          title={t("feed.closeIssue")}
                        >
                          ✓
                        </button>
                      )}
                      <button
                        onClick={() => openCreateTaskModal("jira", issue.key, issue.summary, undefined)}
                        className="opacity-0 group-hover:opacity-100 p-1 text-ac-muted hover:text-ac-brand shrink-0"
                        title={t("feed.toTask")}
                      >
                        <ArrowRight className="w-3 h-3" />
                      </button>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>

          {/* ── Calendar tile (ADR-007 v2: today's events) ─────────────── */}
          <div className="rounded-lg border border-ac-border bg-ac-surface p-4">
            <div className="flex items-center justify-between mb-3">
              <div className="flex items-center gap-2">
                <Calendar className="w-4 h-4 text-ac-muted" />
                <span className="text-sm font-semibold text-ac-ink">
                  {t("feed.calendar")}
                </span>
              </div>
              {calItems && calItems.length > 0 && (
                <span className="text-[10px] px-2 py-0.5 rounded-full bg-ac-brand-soft text-ac-brand font-medium">
                  {calItems.length}
                </span>
              )}
            </div>
            {calError ? (
              <div className="text-xs text-center py-4 space-y-1">
                <p className="text-ac-muted">
                  {calErrMsg && calErrMsg.includes("not configured")
                    ? t("feed.calendarNotConfigured")
                    : t("feed.calendarError")}
                </p>
                {calErrMsg && !calErrMsg.includes("not configured") && (
                  <p className="text-[10px] text-ac-muted/60 truncate" title={calErrMsg}>
                    {calErrMsg}
                  </p>
                )}
              </div>
            ) : calItems === null ? (
              <p className="text-xs text-ac-muted text-center py-4">
                {t("feed.calendarLoading")}
              </p>
            ) : calItems.length === 0 ? (
              <p className="text-xs text-ac-muted text-center py-4">
                {t("feed.calendarEmpty")}
              </p>
            ) : (
              <div className="space-y-2">
                {calItems.slice(0, 5).map((ev, i) => (
                  <div
                    key={i}
                    className="w-full flex items-start gap-2 text-xs text-left px-2 py-1.5 rounded hover:bg-ac-surface-2 transition-colors group"
                  >
                    <Calendar className="w-3.5 h-3.5 mt-0.5 text-ac-muted shrink-0" />
                    <div className="flex-1 min-w-0">
                      <p className="truncate text-ac-ink font-medium">
                        {ev.summary}
                      </p>
                      <p className="text-[10px] text-ac-muted truncate">
                        {ev.start && formatEventTime(ev.start)}
                        {ev.location && ` · ${ev.location}`}
                      </p>
                    </div>
                    <button
                      onClick={() => openCreateTaskModal("calendar", ev.uid, ev.summary, undefined)}
                      className="opacity-0 group-hover:opacity-100 p-1 text-ac-muted hover:text-ac-brand shrink-0"
                      title={t("feed.toTask")}
                    >
                      <ArrowRight className="w-3 h-3" />
                    </button>
                  </div>
                ))}
              </div>
            )}
          </div>

          {/* ── Meeting Reminders tile (L8.5) ─────────────────────────────── */}
          {meetingReminders.length > 0 && (
            <div className="rounded-lg border border-ac-brand-border bg-ac-brand-soft p-4">
              <div className="flex items-center justify-between mb-3">
                <div className="flex items-center gap-2">
                  <Bell className="w-4 h-4 text-ac-brand" />
                  <span className="text-sm font-semibold text-ac-ink">
                    {t("feed.meetingReminders")}
                  </span>
                </div>
                <span className="text-[10px] px-2 py-0.5 rounded-full bg-ac-brand text-white font-medium">
                  {meetingReminders.length}
                </span>
              </div>
              <div className="space-y-2">
                {meetingReminders.slice(0, 3).map((ev) => (
                  <div
                    key={ev.uid}
                    className="w-full flex items-start gap-2 text-xs text-left px-2 py-1.5 rounded hover:bg-ac-brand/10 transition-colors group"
                  >
                    <Bell className="w-3.5 h-3.5 mt-0.5 text-ac-brand shrink-0" />
                    <div className="flex-1 min-w-0">
                      <p className="truncate text-ac-ink font-medium">
                        {ev.summary}
                      </p>
                      <p className="text-[10px] text-ac-muted truncate">
                        {ev.start && formatEventTime(ev.start)}
                        {ev.location && ` · ${ev.location}`}
                      </p>
                    </div>
                    <div className="flex items-center gap-1 shrink-0">
                      <button
                        onClick={() => openCreateTaskModal("calendar", ev.uid, ev.summary, undefined)}
                        className="opacity-0 group-hover:opacity-100 p-1 text-ac-muted hover:text-ac-brand shrink-0"
                        title={t("feed.toTask")}
                      >
                        <ArrowRight className="w-3 h-3" />
                      </button>
                      <button
                        onClick={() => generateMeetingBriefing(ev.uid)}
                        disabled={meetingBriefingLoading[ev.uid]}
                        className="opacity-0 group-hover:opacity-100 p-1 text-ac-muted hover:text-ac-brand shrink-0 disabled:opacity-50"
                        title={meetingBriefingLoading[ev.uid] ? t("feed.briefingGenerating") : t("feed.generateMeetingBriefing")}
                      >
                        {meetingBriefingLoading[ev.uid] ? (
                          <Loader className="w-3 h-3 animate-spin" />
                        ) : (
                          <FileText className="w-3 h-3" />
                        )}
                      </button>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          )}

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

        {actionStatus && (
          <p className={`text-xs text-center ${actionStatus.startsWith("✓") ? "text-ac-green" : "text-ac-red"}`}>
            {actionStatus}
          </p>
        )}

        {/* ── Create Task from External Modal ─────────────────────────────── */}
        {createTaskModal && (
          <div
            className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4"
            onClick={() => setCreateTaskModal(null)}
          >
            <div
              className="w-full max-w-md bg-ac-surface border border-ac-border rounded-xl p-6 shadow-xl"
              onClick={(e) => e.stopPropagation()}
            >
              <div className="flex items-center justify-between mb-4">
                <h3 className="text-lg font-semibold text-ac-ink">{t("feed.toTask")}</h3>
                <button
                  onClick={() => setCreateTaskModal(null)}
                  className="p-1 text-ac-muted hover:text-ac-ink"
                >
                  <X className="w-5 h-5" />
                </button>
              </div>
              <p className="text-sm text-ac-muted mb-4">
                {createTaskModal.source === "jira"
                  ? t("feed.createTaskFromJira")
                  : createTaskModal.source === "email"
                  ? t("feed.createTaskFromEmail")
                  : t("feed.createTaskFromMeeting")}
                <br />
                <span className="text-ac-brand font-mono text-xs">
                  {createTaskModal.externalId}
                </span>
              </p>
              <div className="space-y-3">
                <div>
                  <label className="block text-xs text-ac-muted mb-1">{t("tasks.whatTodo")}</label>
                  <input
                    type="text"
                    defaultValue={createTaskModal.title}
                    className="w-full px-3 py-2 rounded-lg border border-ac-border bg-ac-background text-ac-ink placeholder-ac-muted focus:outline-none focus:border-ac-brand"
                    onChange={(e) => setCreateTaskModal((m) => m ? { ...m, title: e.target.value } : null)}
                  />
                </div>
                <div className="grid grid-cols-2 gap-3">
                  <div>
                    <label className="block text-xs text-ac-muted mb-1">{t("tasks.prioMed")}</label>
                    <select
                      defaultValue="3"
                      className="w-full px-3 py-2 rounded-lg border border-ac-border bg-ac-background text-ac-ink focus:outline-none focus:border-ac-brand"
                      onChange={(e) => setCreateTaskModal((m) => m ? { ...m, priority: parseInt(e.target.value) } : null)}
                    >
                      <option value="1">{t("tasks.prioHigh")}</option>
                      <option value="2">{t("tasks.prioMed")}</option>
                      <option value="3" selected>{t("tasks.prioLow")}</option>
                      <option value="4">Low</option>
                      <option value="5">Lowest</option>
                    </select>
                  </div>
                  <div>
                    <label className="block text-xs text-ac-muted mb-1">{t("goals.target")}</label>
                    <input
                      type="date"
                      className="w-full px-3 py-2 rounded-lg border border-ac-border bg-ac-background text-ac-ink focus:outline-none focus:border-ac-brand"
                      onChange={(e) => setCreateTaskModal((m) => m ? { ...m, dueDate: e.target.value || undefined } : null)}
                    />
                  </div>
                </div>
              </div>
              <div className="flex justify-end gap-2 mt-6">
                <button
                  onClick={() => setCreateTaskModal(null)}
                  className="px-4 py-2 text-sm rounded-lg border border-ac-border text-ac-ink hover:bg-ac-surface-2"
                >
                  {t("btn.cancel")}
                </button>
                <button
                  onClick={async () => {
                    if (!createTaskModal) return;
                    try {
                      await invoke("create_task_from_external_cmd", {
                        source: createTaskModal.source,
                        externalId: createTaskModal.externalId,
                        externalUrl: createTaskModal.externalUrl,
                        title: createTaskModal.title,
                        priority: 3,
                        dueDate: null,
                        projectId: null,
                        goalId: null,
                        assignee: "ngusev",
                        profile: null,
                      });
                      setActionStatus("✓ " + t("feed.markedRead"));
                      setTimeout(() => setActionStatus(""), 2000);
                      setCreateTaskModal(null);
                      void load();
                    } catch (e: any) {
                      setActionStatus("✗ " + (e?.message || String(e)));
                      setTimeout(() => setActionStatus(""), 3000);
                    }
                  }}
                  className="ac-btn px-4 py-2 text-sm"
                >
                  {t("btn.add")}
                </button>
              </div>
            </div>
          </div>
        )}

        {/* ── Meeting Briefing Modal (L8.5) ───────────────────────────────── */}
        {meetingBriefing && (
          <div
            className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4"
            onClick={() => setMeetingBriefing(null)}
          >
            <div
              className="w-full max-w-2xl bg-ac-surface border border-ac-border rounded-xl p-6 shadow-xl max-h-[80vh] overflow-y-auto"
              onClick={(e) => e.stopPropagation()}
            >
              <div className="flex items-center justify-between mb-4">
                <h3 className="text-lg font-semibold text-ac-ink">
                  {t("feed.meetingBriefing")} — {meetingBriefing.meetingType}
                </h3>
                <button
                  onClick={() => setMeetingBriefing(null)}
                  className="p-1 text-ac-muted hover:text-ac-ink"
                >
                  <X className="w-5 h-5" />
                </button>
              </div>
              <div className="prose prose-sm text-ac-ink dark:prose-invert max-h-[60vh] overflow-y-auto">
                <MarkdownRenderer content={meetingBriefing.briefingText} />
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

export default FeedView;
