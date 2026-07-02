// src/components/views/DashboardView.tsx
// shturman.ai-style dashboard: a 2-column grid of 4 cards (Tasks today/week,
// Goals, Priority projects), each with a count badge and a skeleton-loading
// list. Data comes from the desktop's own tables (kanban-desktop.db).

import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Plus, Zap } from "lucide-react";
import { useTranslation } from "../../hooks/useTranslation";

interface KanbanTask {
  id: number;
  title: string;
  status: string;
  priority: number;
  due_date: string | null;
}

interface DashboardViewProps {
  /** Switch to a different section (e.g. click "Новая задача" → tasks). */
  onNavigate: (view: "chat" | "tasks" | "goals" | "projects") => void;
  onSelfDiagnosis?: () => void;
}

export function DashboardView({ onNavigate, onSelfDiagnosis }: DashboardViewProps) {
  const { t } = useTranslation();
  const [tasks, setTasks] = useState<KanbanTask[]>([]);
  const [stats, setStats] = useState<{ tasks_total: number; tasks_done: number; tasks_today: number; goals_total: number; projects_total: number } | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    Promise.all([
      invoke<KanbanTask[]>("list_tasks_cmd", { profile: null }).catch(() => []),
      invoke<{ tasks_total: number; tasks_done: number; tasks_today: number; goals_total: number; projects_total: number }>("dash_stats_cmd", { profile: null }).catch(() => null),
    ]).then(([t, s]) => { setTasks(t); setStats(s); }).finally(() => setLoading(false));
  }, []);

  // Today / this-week splits by due_date.
  const now = new Date();
  const startOfDay = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const endOfDay = new Date(startOfDay); endOfDay.setDate(endOfDay.getDate() + 1);
  const endOfWeek = new Date(startOfDay); endOfWeek.setDate(endOfWeek.getDate() + 7);

  const inRange = (due: string | null, from: Date, to: Date) => {
    if (!due) return false;
    const d = new Date(due);
    return d >= from && d < to;
  };

  const todayTasks = tasks.filter((x) => inRange(x.due_date, startOfDay, endOfDay));
  const weekTasks = tasks.filter((x) => inRange(x.due_date, startOfDay, endOfWeek));
  const allDone = tasks.length > 0 && tasks.every((x) => x.status === "done");

  const subtitle = tasks.length === 0
    ? t("dash.noTasks")
    : allDone
      ? t("dash.allDone")
      : t("dash.activeToday", { n: todayTasks.length });

  return (
    <div className="p-6 max-w-6xl mx-auto">
      {/* Section header */}
      <div className="flex items-center justify-between mb-6">
        <div>
          <h2 className="text-lg font-semibold text-ac-ink">{t("nav.dashboard")}</h2>
          <p className="text-xs text-ac-muted mt-0.5">{subtitle}</p>
        </div>
        <div className="flex gap-2">
          {onSelfDiagnosis && (
            <button
              onClick={onSelfDiagnosis}
              className="flex items-center gap-1.5 px-3 py-2 text-sm border border-ac-border rounded-md text-ac-muted hover:text-ac-brand hover:border-ac-brand-border"
            >
              <Zap className="w-4 h-4" />
              {t("nav.selfDiagnosis")}
            </button>
          )}
          <button
            onClick={() => onNavigate("tasks")}
            className="flex items-center gap-1.5 px-3 py-2 text-sm bg-ac-brand text-white rounded-md hover:bg-ac-brand-dark"
          >
            <Plus className="w-4 h-4" />
            {t("dash.newTask")}
          </button>
        </div>
      </div>

      {/* Cards grid */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
        <DashCard
          emoji="🔴"
          title={t("dash.tasksToday")}
          count={todayTasks.length}
          loading={loading}
          items={todayTasks.map((x) => ({ title: x.title, priority: x.priority }))}
          emptyText={t("dash.noToday")}
          onClick={() => onNavigate("tasks")}
        />
        <DashCard
          emoji="📅"
          title={t("dash.tasksWeek")}
          count={weekTasks.length}
          loading={loading}
          items={weekTasks.map((x) => ({ title: x.title, priority: x.priority }))}
          emptyText={t("dash.noWeek")}
          onClick={() => onNavigate("tasks")}
        />
        <DashCard
          emoji="🎯"
          title={t("dash.goals")}
          count={stats?.goals_total ?? 0}
          loading={loading}
          items={[]}
          emptyText={t("dash.noGoals")}
          onClick={() => onNavigate("goals")}
        />
        <DashCard
          emoji="📁"
          title={t("dash.priorityProjects")}
          count={stats?.projects_total ?? 0}
          loading={loading}
          items={[]}
          emptyText={t("dash.noProjects")}
          onClick={() => onNavigate("projects")}
        />
      </div>
    </div>
  );
}

function DashCard({
  emoji,
  title,
  count,
  loading,
  items,
  emptyText,
  onClick,
}: {
  emoji: string;
  title: string;
  count: number;
  loading: boolean;
  items: { title: string; priority: number }[];
  emptyText: string;
  onClick: () => void;
}) {
  return (
    <div className="bg-ac-surface rounded-lg border border-ac-border p-4 flex flex-col">
      <div className="flex items-center justify-between mb-3">
        <h3 className="text-sm font-semibold text-ac-ink flex items-center gap-2">
          <span>{emoji}</span>
          <span className="uppercase tracking-wide">{title}</span>
        </h3>
        <span className="text-xs bg-ac-surface-2 text-ac-muted px-2 py-0.5 rounded">{count}</span>
      </div>
      <div className="flex-1 space-y-1.5 max-h-64 overflow-y-auto">
        {loading ? (
          <>
            {[0, 1, 2].map((i) => (
              <div key={i} className="flex items-center gap-2 animate-pulse">
                <div className="w-2 h-2 rounded-full bg-ac-muted/40" />
                <div className="h-4 flex-1 rounded bg-ac-muted/20" />
                <div className="h-4 w-12 rounded bg-ac-muted/20" />
              </div>
            ))}
          </>
        ) : items.length === 0 ? (
          <p className="text-xs text-ac-muted py-2">{emptyText}</p>
        ) : (
          items.map((it, i) => (
            <div key={i} className="flex items-center gap-2 text-sm">
              <span
                className="w-2 h-2 rounded-full shrink-0"
                style={{ background: it.priority <= 1 ? "#f44" : it.priority === 3 ? "#fa0" : "#888" }}
              />
              <span className="truncate text-ac-ink-2">{it.title}</span>
            </div>
          ))
        )}
      </div>
      <button
        onClick={onClick}
        className="mt-2 text-xs text-ac-brand hover:underline text-left"
      >
        {title}
      </button>
    </div>
  );
}

export default DashboardView;
