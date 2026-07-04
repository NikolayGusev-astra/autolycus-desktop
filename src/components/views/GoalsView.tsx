// src/components/views/GoalsView.tsx
// Goals with drill-down: click a goal → see its projects. Edit + progress.
import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Plus, Trash2, Target, Loader, Pencil, FolderOpen } from "lucide-react";
import { useTranslation } from "../../hooks/useTranslation";

interface Goal { id: number; title: string; description: string; target_date: string | null; progress: number; }
interface Project { id: number; name: string; color: string; goal_id?: number | null; }

export function GoalsView({ onOpenProject }: { onOpenProject?: (pid: number, pname: string) => void }) {
  const { t } = useTranslation();
  const [goals, setGoals] = useState<Goal[]>([]);
  const [projects, setProjects] = useState<Project[]>([]);
  const [loading, setLoading] = useState(true);
  const [showForm, setShowForm] = useState(false);
  const [editingId, setEditingId] = useState<number | null>(null);
  const [title, setTitle] = useState("");
  const [targetDate, setTargetDate] = useState("");
  const [progress, setProgress] = useState(0);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const [g, p] = await Promise.all([
        invoke<Goal[]>("list_goals_cmd", { profile: null }),
        invoke<Project[]>("list_projects_cmd", { profile: null }),
      ]);
      setGoals(g); setProjects(p);
    } catch (e) { console.error(e); } finally { setLoading(false); }
  }, []);
  useEffect(() => { void load(); }, [load]);

  const create = async () => {
    if (!title.trim()) return;
    await invoke("create_goal_cmd", { title: title.trim(), targetDate: targetDate || null, profile: null });
    setTitle(""); setTargetDate(""); setShowForm(false); void load();
  };
  const remove = async (id: number) => { await invoke("delete_goal_cmd", { id, profile: null }); void load(); };
  const startEdit = (g: Goal) => { setEditingId(g.id); setTitle(g.title); setTargetDate(g.target_date || ""); setProgress(g.progress); };
  const saveEdit = async () => {
    if (editingId === null) return;
    await invoke("update_goal_cmd", { id: editingId, title: title.trim() || undefined, targetDate: targetDate || undefined, progress, profile: null });
    setEditingId(null); void load();
  };

  const goalProjects = (gid: number) => projects.filter((p) => p.goal_id === gid);

  return (
    <div className="p-6 max-w-4xl mx-auto">
      <div className="flex items-center justify-between mb-6">
        <h2 className="text-lg font-semibold text-ac-ink">{t("nav.goals")}</h2>
        <button onClick={() => { setShowForm(!showForm); setEditingId(null); }} className="ac-btn px-3 py-2 text-sm flex items-center gap-1.5">
          <Plus className="w-4 h-4" /> {t("goals.new")}
        </button>
      </div>

      {(showForm || editingId !== null) && (
        <div className="mb-4 p-4 rounded-lg border border-ac-border bg-ac-surface space-y-3">
          <input className="ac-input w-full px-3 py-2 text-sm" placeholder={t("goals.namePh")} value={title} onChange={(e) => setTitle(e.target.value)} />
          <div className="flex gap-3 items-center">
            <input type="date" className="ac-input px-3 py-2 text-sm" value={targetDate} onChange={(e) => setTargetDate(e.target.value)} />
            {editingId !== null && (
              <label className="text-xs text-ac-muted flex items-center gap-2">{t("goals.progress")}: {progress}%
                <input type="range" min={0} max={100} value={progress} onChange={(e) => setProgress(Number(e.target.value))} className="w-24" />
              </label>
            )}
            <button onClick={() => editingId !== null ? saveEdit() : create()} className="ac-btn px-4 py-2 text-sm">
              {editingId !== null ? t("btn.save") : t("btn.add")}
            </button>
          </div>
        </div>
      )}

      {loading ? (<div className="flex justify-center py-8"><Loader className="w-5 h-5 animate-spin text-ac-muted" /></div>)
        : goals.length === 0 ? (<p className="text-sm text-ac-muted text-center py-12">{t("goals.empty")}</p>)
        : (<div className="space-y-3">
          {goals.map((g) => (
            <div key={g.id} className="group p-4 rounded-lg border border-ac-border bg-ac-surface">
              <div className="flex items-start gap-2">
                <Target className="w-4 h-4 text-ac-brand mt-0.5 shrink-0" />
                <div className="flex-1 min-w-0">
                  <p className="text-sm font-medium text-ac-ink">{g.title}</p>
                  {g.target_date && <p className="text-xs text-ac-muted mt-0.5">{t("goals.target")}: {g.target_date}</p>}
                  {g.progress > 0 && (
                    <div className="mt-1.5 w-full h-1.5 rounded-full bg-ac-border overflow-hidden">
                      <div className="h-full rounded-full bg-ac-brand" style={{ width: `${g.progress}%` }} />
                    </div>
                  )}
                </div>
                <button onClick={() => startEdit(g)} className="p-1 rounded text-ac-faint hover:text-ac-brand hover:bg-ac-bg"><Pencil className="w-3.5 h-3.5" /></button>
                <button onClick={() => void remove(g.id)} className="p-1 rounded text-ac-faint hover:text-ac-red hover:bg-ac-bg"><Trash2 className="w-3.5 h-3.5" /></button>
              </div>
              {/* Drill-down: projects under this goal */}
              {goalProjects(g.id).length > 0 && (
                <div className="mt-2 pl-6 space-y-0.5">
                  {goalProjects(g.id).map((p) => (
                    <button key={p.id} onClick={() => onOpenProject?.(p.id, p.name)}
                      className="w-full flex items-center gap-1.5 px-2 py-1 text-xs text-ac-muted hover:text-ac-brand hover:bg-ac-bg rounded-md transition-colors">
                      <span className="w-2 h-2 rounded-full" style={{ background: p.color }} />
                      <FolderOpen className="w-3 h-3" />
                      {p.name}
                    </button>
                  ))}
                </div>
              )}
            </div>
          ))}
        </div>)}
    </div>
  );
}
export default GoalsView;
