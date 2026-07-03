// src/components/views/TasksView.tsx
// Tasks with inline edit, project assignment, priority, due date.
// Supports drill-down: when projectId prop is set, only shows tasks for that project.
import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Plus, Trash2, Check, Loader, ChevronLeft, Pencil } from "lucide-react";
import { useTranslation } from "../../hooks/useTranslation";

interface Task {
  id: number;
  title: string;
  description: string;
  status: string;
  priority: number;
  due_date: string | null;
  project_id: number | null;
}
interface Project { id: number; name: string; color: string; }

const PRIO_COLOR: Record<number, string> = { 1: "#f44", 2: "#f80", 3: "#fa0", 4: "#8a8", 5: "#888" };
const PRIO_LABEL: Record<number, string> = { 1: "🔴", 2: "🟠", 3: "🟡", 4: "🟢", 5: "⚪" };

export function TasksView({ projectId, projectName, onBack }: { projectId?: number | null; projectName?: string; onBack?: () => void }) {
  const { t } = useTranslation();
  const [tasks, setTasks] = useState<Task[]>([]);
  const [projects, setProjects] = useState<Project[]>([]);
  const [loading, setLoading] = useState(true);
  const [showForm, setShowForm] = useState(false);
  const [editingId, setEditingId] = useState<number | null>(null);
  // form state
  const [title, setTitle] = useState("");
  const [priority, setPriority] = useState(3);
  const [dueDate, setDueDate] = useState("");
  const [projId, setProjId] = useState<number | null>(projectId ?? null);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const [t1, p1] = await Promise.all([
        invoke<Task[]>("list_tasks_cmd", { profile: null }),
        invoke<Project[]>("list_projects_cmd", { profile: null }),
      ]);
      setTasks(projectId ? t1.filter((x) => x.project_id === projectId) : t1);
      setProjects(p1);
    } catch (e) { console.error(e); }
    finally { setLoading(false); }
  }, [projectId]);

  useEffect(() => { void load(); }, [load]);

  const create = async () => {
    if (!title.trim()) return;
    await invoke("create_task_cmd", { title: title.trim(), priority, dueDate: dueDate || null, projectId: projId, profile: null });
    setTitle(""); setDueDate(""); setShowForm(false); void load();
  };

  const toggle = async (task: Task) => {
    await invoke("update_task_status_cmd", { id: task.id, status: task.status === "done" ? "todo" : "done", profile: null });
    void load();
  };

  const remove = async (id: number) => { await invoke("delete_task_cmd", { id, profile: null }); void load(); };

  const saveEdit = async (task: Task) => {
    await invoke("update_task_cmd", { id: task.id, title: title.trim() || undefined, priority, dueDate: dueDate || undefined, projectId: projId, profile: null });
    setEditingId(null); void load();
  };

  const startEdit = (task: Task) => {
    setEditingId(task.id);
    setTitle(task.title);
    setPriority(task.priority);
    setDueDate(task.due_date || "");
    setProjId(task.project_id ?? null);
  };

  const projName = (pid: number | null) => projects.find((p) => p.id === pid)?.name;
  const projColor = (pid: number | null) => projects.find((p) => p.id === pid)?.color || "#888";

  return (
    <div className="p-6 max-w-4xl mx-auto">
      <div className="flex items-center gap-3 mb-6">
        {onBack && (
          <button onClick={onBack} className="p-1.5 rounded-md hover:bg-ac-surface text-ac-muted hover:text-ac-brand">
            <ChevronLeft className="w-5 h-5" />
          </button>
        )}
        <h2 className="text-lg font-semibold text-ac-ink">
          {projectName ? `${t("nav.tasks")}: ${projectName}` : t("nav.tasks")}
        </h2>
        <button onClick={() => { setShowForm(!showForm); setEditingId(null); setTitle(""); }} className="ac-btn px-3 py-2 text-sm flex items-center gap-1.5 ml-auto">
          <Plus className="w-4 h-4" /> {t("dash.newTask")}
        </button>
      </div>

      {(showForm || editingId !== null) && (
        <div className="mb-4 p-4 rounded-lg border border-ac-border bg-ac-surface space-y-3">
          <input className="ac-input w-full px-3 py-2 text-sm" placeholder={t("tasks.whatTodo")} value={title} onChange={(e) => setTitle(e.target.value)} />
          <div className="flex gap-3 flex-wrap">
            <select className="ac-input px-3 py-2 text-sm" value={priority} onChange={(e) => setPriority(Number(e.target.value))}>
              {[1,2,3,4,5].map((p) => <option key={p} value={p}>{PRIO_LABEL[p]} {t(`tasks.prio${p <= 1 ? "High" : p <= 3 ? "Med" : "Low"}`)}</option>)}
            </select>
            <input type="date" className="ac-input px-3 py-2 text-sm" value={dueDate} onChange={(e) => setDueDate(e.target.value)} />
            {!projectId && (
              <select className="ac-input px-3 py-2 text-sm" value={projId ?? ""} onChange={(e) => setProjId(e.target.value ? Number(e.target.value) : null)}>
                <option value="">{t("projects.noGoal")}</option>
                {projects.map((p) => <option key={p.id} value={p.id}>{p.name}</option>)}
              </select>
            )}
            <button onClick={() => editingId !== null ? saveEdit(tasks.find((x) => x.id === editingId)!) : create()} className="ac-btn px-4 py-2 text-sm">
              {editingId !== null ? t("btn.save") : t("btn.add")}
            </button>
          </div>
        </div>
      )}

      {loading ? (
        <div className="flex justify-center py-8"><Loader className="w-5 h-5 animate-spin text-ac-muted" /></div>
      ) : tasks.length === 0 ? (
        <p className="text-sm text-ac-muted text-center py-12">{t("tasks.createFirst")}</p>
      ) : (
        <div className="space-y-1.5">
          {tasks.map((task) => (
            <div key={task.id} className="group flex items-center gap-3 p-3 rounded-lg border border-ac-border bg-ac-surface">
              <button onClick={() => void toggle(task)} className={`w-5 h-5 rounded border flex items-center justify-center shrink-0 ${task.status === "done" ? "bg-green-500 border-green-500" : "border-ac-border"}`}>
                {task.status === "done" && <Check className="w-3 h-3 text-white" />}
              </button>
              <span className="w-2 h-2 rounded-full shrink-0" style={{ background: PRIO_COLOR[task.priority] || "#888" }} />
              <span className={`flex-1 text-sm ${task.status === "done" ? "line-through text-ac-faint" : "text-ac-ink"}`}>{task.title}</span>
              {task.due_date && <span className="text-xs text-ac-muted">{task.due_date}</span>}
              {task.project_id && (
                <span className="text-[10px] px-1.5 py-0.5 rounded-full" style={{ background: projColor(task.project_id) + "22", color: projColor(task.project_id) }}>
                  {projName(task.project_id)}
                </span>
              )}
              <button onClick={() => startEdit(task)} className="opacity-0 group-hover:opacity-100 text-ac-faint hover:text-ac-brand"><Pencil className="w-3.5 h-3.5" /></button>
              <button onClick={() => void remove(task.id)} className="opacity-0 group-hover:opacity-100 text-ac-faint hover:text-ac-red"><Trash2 className="w-3.5 h-3.5" /></button>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
export default TasksView;
