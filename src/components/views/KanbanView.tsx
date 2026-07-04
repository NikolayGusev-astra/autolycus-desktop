// src/components/views/KanbanView.tsx
// Visual Kanban board: 3 columns (To Do / In Progress / Done) with tasks
// movable between columns via buttons. Tasks grouped by status.

import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ChevronLeft, Loader } from "lucide-react";
import { useTranslation } from "../../hooks/useTranslation";

interface Task {
  id: number;
  title: string;
  status: string;
  priority: number;
  due_date: string | null;
  project_id: number | null;
  assignee: string;
}
interface Project { id: number; name: string; color: string; }

const PRIO_DOT: Record<number, string> = { 1: "#f44", 2: "#f80", 3: "#fa0", 4: "#8a8", 5: "#888" };
const COLUMNS = ["todo", "in_progress", "done"] as const;
const COL_LABELS: Record<string, { ru: string; en: string; color: string }> = {
  todo: { ru: "К выполнению", en: "To Do", color: "#6b7280" },
  in_progress: { ru: "В работе", en: "In Progress", color: "#f59e0b" },
  done: { ru: "Готово", en: "Done", color: "#22c55e" },
};

export function KanbanView({ projectId, projectName, onBack }: { projectId?: number | null; projectName?: string; onBack?: () => void }) {
  const { t } = useTranslation();
  const [tasks, setTasks] = useState<Task[]>([]);
  const [projects, setProjects] = useState<Project[]>([]);
  const [loading, setLoading] = useState(true);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const [t1, p1] = await Promise.all([
        invoke<Task[]>("list_tasks_cmd", { profile: null }),
        invoke<Project[]>("list_projects_cmd", { profile: null }),
      ]);
      setTasks(projectId ? t1.filter((x) => x.project_id === projectId) : t1);
      setProjects(p1);
    } catch (e) { console.error(e); } finally { setLoading(false); }
  }, [projectId]);
  useEffect(() => { void load(); }, [load]);

  const moveTask = async (id: number, newStatus: string) => {
    await invoke("update_task_status_cmd", { id, status: newStatus, profile: null });
    void load();
  };

  const projName = (pid: number | null) => projects.find((p) => p.id === pid)?.name;
  const projColor = (pid: number | null) => projects.find((p) => p.id === pid)?.color || "#888";

  const tasksByStatus = (status: string) => tasks.filter((x) => x.status === status);

  return (
    <div className="h-full flex flex-col overflow-hidden">
      {/* Header */}
      <div className="flex items-center gap-3 px-6 py-3 border-b border-ac-border">
        {onBack && (
          <button onClick={onBack} className="p-1.5 rounded-md hover:bg-ac-surface text-ac-muted hover:text-ac-brand">
            <ChevronLeft className="w-5 h-5" />
          </button>
        )}
        <h2 className="text-lg font-semibold text-ac-ink">{projectName ? `Канбан: ${projectName}` : t("kanban.title")}</h2>
        <span className="text-xs text-ac-faint ml-auto">{tasks.length} {t("nav.tasks").toLowerCase()}</span>
      </div>

      {loading ? (
        <div className="flex justify-center py-12"><Loader className="w-6 h-6 animate-spin text-ac-muted" /></div>
      ) : tasks.length === 0 ? (
        <p className="text-sm text-ac-muted text-center py-12">{t("tasks.createFirst")}</p>
      ) : (
        /* Board: 3 columns */
        <div className="flex-1 flex gap-4 overflow-x-auto p-6">
          {COLUMNS.map((col) => {
            const meta = COL_LABELS[col];
            const colTasks = tasksByStatus(col);
            const nextCol = col === "todo" ? "in_progress" : col === "in_progress" ? "done" : null;
            const prevCol = col === "done" ? "in_progress" : col === "in_progress" ? "todo" : null;
            return (
              <div key={col} className="w-72 shrink-0 flex flex-col">
                {/* Column header */}
                <div className="flex items-center gap-2 mb-3 px-2">
                  <span className="w-2.5 h-2.5 rounded-full" style={{ background: meta.color }} />
                  <span className="text-sm font-medium text-ac-ink">{meta.ru}</span>
                  <span className="text-xs text-ac-faint ml-auto bg-ac-surface px-1.5 py-0.5 rounded-full">{colTasks.length}</span>
                </div>

                {/* Cards */}
                <div className="flex-1 space-y-2 overflow-y-auto">
                  {colTasks.map((task) => (
                    <div key={task.id} className="p-3 rounded-lg border border-ac-border bg-ac-surface hover:border-ac-brand-border transition-colors">
                      <div className="flex items-start gap-2 mb-1.5">
                        <span className="w-2 h-2 rounded-full shrink-0 mt-1" style={{ background: PRIO_DOT[task.priority] || "#888" }} />
                        <p className="text-sm text-ac-ink flex-1">{task.title}</p>
                      </div>
                      <div className="flex items-center gap-1.5 flex-wrap ml-4">
                        {task.due_date && <span className="text-[10px] text-ac-muted">📅 {task.due_date}</span>}
                        {task.project_id && (
                          <span className="text-[10px] px-1.5 py-0.5 rounded-full" style={{ background: projColor(task.project_id) + "22", color: projColor(task.project_id) }}>
                            {projName(task.project_id)}
                          </span>
                        )}
                        {task.assignee && <span className="text-[10px] text-ac-muted">👤 {task.assignee}</span>}
                      </div>
                      {/* Move buttons */}
                      <div className="flex gap-1 mt-2 ml-4">
                        {prevCol && (
                          <button onClick={() => void moveTask(task.id, prevCol)} className="text-[10px] px-1.5 py-0.5 rounded text-ac-muted hover:text-ac-brand border border-ac-border">
                            ← {COL_LABELS[prevCol].ru}
                          </button>
                        )}
                        {nextCol && (
                          <button onClick={() => void moveTask(task.id, nextCol)} className="text-[10px] px-1.5 py-0.5 rounded text-ac-muted hover:text-ac-brand border border-ac-border ml-auto">
                            {COL_LABELS[nextCol].ru} →
                          </button>
                        )}
                      </div>
                    </div>
                  ))}
                  {colTasks.length === 0 && (
                    <p className="text-xs text-ac-faint text-center py-4">—</p>
                  )}
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
export default KanbanView;
