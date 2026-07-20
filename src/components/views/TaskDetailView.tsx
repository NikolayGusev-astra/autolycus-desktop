// src/components/views/TaskDetailView.tsx
// Task detail view with linked sessions (L6: ADR-009 session links)
import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ChevronLeft, Loader, MessageSquare, Calendar, Flag, Users, Link2, X } from "lucide-react";
import { useTranslation } from "../../hooks/useTranslation";

interface Task {
  id: number;
  title: string;
  description: string;
  status: string;
  priority: number;
  due_date: string | null;
  project_id: number | null;
  assignee: string;
  labels: string;
  section_id: number | null;
}

interface Project { id: number; name: string; color: string; }
interface SessionLink {
  id: number;
  session_id: string;
  task_id: number | null;
  project_id: number | null;
  goal_id: number | null;
  linked_at: number | null;
  linked_by: string;
  note: string;
}

const PRIO_COLOR: Record<number, string> = { 1: "#f44", 2: "#f80", 3: "#fa0", 4: "#8a8", 5: "#888" };
const PRIO_LABEL: Record<number, string> = { 1: "🔴 Critical", 2: "🟠 High", 3: "🟡 Medium", 4: "🟢 Low", 5: "⚪ None" };

export function TaskDetailView({ taskId, onBack }: { taskId: number; onBack?: () => void }) {
  const { t } = useTranslation();
  const [task, setTask] = useState<Task | null>(null);
  const [projects, setProjects] = useState<Project[]>([]);
  const [links, setLinks] = useState<SessionLink[]>([]);
  const [loading, setLoading] = useState(true);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const [t1, p1, l1] = await Promise.all([
        invoke<Task[]>("list_tasks_cmd", { profile: null }),
        invoke<Project[]>("list_projects_cmd", { profile: null }),
        invoke<SessionLink[]>("get_links_for_task_cmd", { taskId, profile: null }),
      ]);
      setTask(t1.find((x) => x.id === taskId) ?? null);
      setProjects(p1);
      setLinks(l1);
    } catch (e) { console.error(e); }
    finally { setLoading(false); }
  }, [taskId]);

  useEffect(() => { void load(); }, [load]);

  const projName = (pid: number | null) => projects.find((p) => p.id === pid)?.name;
  const projColor = (pid: number | null) => projects.find((p) => p.id === pid)?.color || "#888";

  const formatDate = (ts: number | null) => {
    if (!ts) return "—";
    return new Date(ts * 1000).toLocaleString("ru-RU", { day: "2-digit", month: "2-digit", year: "numeric", hour: "2-digit", minute: "2-digit" });
  };

  if (loading) {
    return <div className="flex justify-center py-8"><Loader className="w-5 h-5 animate-spin text-ac-muted" /></div>;
  }
  if (!task) {
    return <p className="text-sm text-ac-muted text-center py-12">{t("tasks.notFound")}</p>;
  }

  return (
    <div className="p-6 max-w-4xl mx-auto">
      <div className="flex items-center gap-3 mb-6">
        {onBack && (
          <button onClick={onBack} className="p-1.5 rounded-md hover:bg-ac-surface text-ac-muted hover:text-ac-brand">
            <ChevronLeft className="w-5 h-5" />
          </button>
        )}
        <h2 className="text-lg font-semibold text-ac-ink">{t("tasks.detailTitle")}</h2>
      </div>

      <div className="space-y-6">
        {/* Task header */}
        <div className="p-4 rounded-lg border border-ac-border bg-ac-surface space-y-3">
          <div className="flex items-start gap-3">
            <button onClick={() => void invoke("update_task_status_cmd", { id: task.id, status: task.status === "done" ? "todo" : "done", profile: null }).then(() => void load())} 
              className={`w-6 h-6 rounded border flex items-center justify-center shrink-0 ${task.status === "done" ? "bg-green-500 border-green-500" : "border-ac-border"}`}>
              {task.status === "done" && <Check className="w-3.5 h-3.5 text-white" />}
            </button>
            <div className="flex-1">
              <div className="flex items-center gap-2">
                <span className="w-2 h-2 rounded-full shrink-0" style={{ background: PRIO_COLOR[task.priority] || "#888" }} />
                <h3 className="text-base font-medium text-ac-ink">{task.title}</h3>
                <span className="text-xs text-ac-muted">{PRIO_LABEL[task.priority]}</span>
              </div>
              {task.description && <p className="text-sm text-ac-muted mt-1">{task.description}</p>}
            </div>
          </div>

          <div className="flex flex-wrap gap-3 text-sm text-ac-muted">
            {task.due_date && (
              <span className="flex items-center gap-1.5">
                <Calendar className="w-3.5 h-3.5" />
                {task.due_date}
              </span>
            )}
            {task.project_id && (
              <span className="flex items-center gap-1.5 px-2 py-0.5 rounded-full" style={{ background: projColor(task.project_id) + "22", color: projColor(task.project_id) }}>
                <Flag className="w-3.5 h-3.5" /> {projName(task.project_id)}
              </span>
            )}
            {task.assignee && (
              <span className="flex items-center gap-1.5 px-2 py-0.5 rounded-full bg-ac-surface-2">
                <Users className="w-3.5 h-3.5" /> {task.assignee}
              </span>
            )}
            {task.labels && task.labels.split(",").map((lb, i) => (
              <span key={i} className="px-2 py-0.5 rounded-full bg-ac-brand-soft text-ac-brand text-xs">
                #{lb.trim()}
              </span>
            ))}
          </div>
        </div>

        {/* Linked Sessions */}
        <div className="p-4 rounded-lg border border-ac-border bg-ac-surface">
          <div className="flex items-center justify-between mb-4">
            <h4 className="font-medium text-ac-ink flex items-center gap-2">
              <Link2 className="w-5 h-5 text-ac-brand" />
              {t("tasks.linkedSessions", { count: links.length })}
            </h4>
          </div>

          {links.length === 0 ? (
            <p className="text-sm text-ac-muted text-center py-8">{t("tasks.noLinkedSessions")}</p>
          ) : (
            <div className="space-y-2">
              {links.map((link) => (
                <div key={link.id} className="p-3 rounded-lg border border-ac-border bg-ac-background flex items-start gap-3 group">
                  <MessageSquare className="w-5 h-5 text-ac-muted shrink-0 mt-0.5" />
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2 mb-1">
                      <span className="font-mono text-xs text-ac-muted">{link.session_id.slice(0, 16)}</span>
                      <span className="text-[10px] px-1.5 py-0.5 rounded-full bg-ac-brand-soft text-ac-brand">
                        {link.linked_by || "manual"}
                      </span>
                      {link.linked_at && (
                        <span className="text-[10px] text-ac-faint ml-auto">{formatDate(link.linked_at)}</span>
                      )}
                    </div>
                    {link.note && <p className="text-sm text-ac-muted italic">{link.note}</p>}
                  </div>
                  <button onClick={async () => {
                    await invoke("unlink_session_cmd", { linkId: link.id, profile: null });
                    void load();
                  }} className="p-1.5 rounded text-ac-faint hover:text-ac-red hover:bg-ac-bg opacity-0 group-hover:opacity-100 transition-opacity">
                    <X className="w-3.5 h-3.5" />
                  </button>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

import { Check } from "lucide-react";

export default TaskDetailView;