// src/components/views/TasksView.tsx
import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Plus, Trash2, Check, Loader } from "lucide-react";
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

const PRIO_COLOR: Record<number, string> = { 1: "#f44", 2: "#f80", 3: "#fa0", 4: "#8a8", 5: "#888" };

export function TasksView() {
  const { t } = useTranslation();
  const [tasks, setTasks] = useState<Task[]>([]);
  const [loading, setLoading] = useState(true);
  const [showForm, setShowForm] = useState(false);
  const [title, setTitle] = useState("");
  const [priority, setPriority] = useState(3);
  const [dueDate, setDueDate] = useState("");

  const load = useCallback(async () => {
    setLoading(true);
    try {
      setTasks(await invoke<Task[]>("list_tasks_cmd", { profile: null }));
    } catch (e) { console.error(e); }
    finally { setLoading(false); }
  }, []);

  useEffect(() => { void load(); }, [load]);

  const create = async () => {
    if (!title.trim()) return;
    await invoke("create_task_cmd", { title: title.trim(), priority, dueDate: dueDate || null, profile: null });
    setTitle(""); setDueDate(""); setShowForm(false);
    void load();
  };

  const toggle = async (task: Task) => {
    await invoke("update_task_status_cmd", { id: task.id, status: task.status === "done" ? "todo" : "done", profile: null });
    void load();
  };

  const remove = async (id: number) => {
    await invoke("delete_task_cmd", { id, profile: null });
    void load();
  };

  return (
    <div className="p-6 max-w-4xl mx-auto">
      <div className="flex items-center justify-between mb-6">
        <h2 className="text-lg font-semibold text-ac-ink">{t("nav.tasks")}</h2>
        <button onClick={() => setShowForm(!showForm)} className="ac-btn px-3 py-2 text-sm flex items-center gap-1.5">
          <Plus className="w-4 h-4" /> {t("dash.newTask")}
        </button>
      </div>

      {showForm && (
        <div className="mb-4 p-4 rounded-lg border border-ac-border bg-ac-surface space-y-3">
          <input className="ac-input w-full px-3 py-2 text-sm" placeholder={t("tasks.whatTodo")} value={title} onChange={(e) => setTitle(e.target.value)} />
          <div className="flex gap-3">
            <select className="ac-input px-3 py-2 text-sm" value={priority} onChange={(e) => setPriority(Number(e.target.value))}>
              <option value={1}>{t("tasks.prioHigh")}</option>
              <option value={3}>{t("tasks.prioMed")}</option>
              <option value={5}>{t("tasks.prioLow")}</option>
            </select>
            <input type="date" className="ac-input px-3 py-2 text-sm" value={dueDate} onChange={(e) => setDueDate(e.target.value)} />
            <button onClick={() => void create()} className="ac-btn px-4 py-2 text-sm">{t("btn.add")}</button>
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
              <button onClick={() => void remove(task.id)} className="opacity-0 group-hover:opacity-100 text-ac-faint hover:text-ac-red"><Trash2 className="w-3.5 h-3.5" /></button>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
export default TasksView;
