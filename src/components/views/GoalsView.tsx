// src/components/views/GoalsView.tsx
import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Plus, Trash2, Target, Loader } from "lucide-react";
import { useTranslation } from "../../hooks/useTranslation";

interface Goal { id: number; title: string; description: string; target_date: string | null; progress: number; }

export function GoalsView() {
  const { t } = useTranslation();
  const [goals, setGoals] = useState<Goal[]>([]);
  const [loading, setLoading] = useState(true);
  const [showForm, setShowForm] = useState(false);
  const [title, setTitle] = useState("");
  const [targetDate, setTargetDate] = useState("");

  const load = useCallback(async () => {
    setLoading(true);
    try { setGoals(await invoke<Goal[]>("list_goals_cmd", { profile: null })); }
    catch (e) { console.error(e); } finally { setLoading(false); }
  }, []);
  useEffect(() => { void load(); }, [load]);

  const create = async () => {
    if (!title.trim()) return;
    await invoke("create_goal_cmd", { title: title.trim(), targetDate: targetDate || null, profile: null });
    setTitle(""); setTargetDate(""); setShowForm(false); void load();
  };
  const remove = async (id: number) => { await invoke("delete_goal_cmd", { id, profile: null }); void load(); };

  return (
    <div className="p-6 max-w-4xl mx-auto">
      <div className="flex items-center justify-between mb-6">
        <h2 className="text-lg font-semibold text-ac-ink">{t("nav.goals")}</h2>
        <button onClick={() => setShowForm(!showForm)} className="ac-btn px-3 py-2 text-sm flex items-center gap-1.5">
          <Plus className="w-4 h-4" /> {t("goals.new")}
        </button>
      </div>
      {showForm && (
        <div className="mb-4 p-4 rounded-lg border border-ac-border bg-ac-surface space-y-3">
          <input className="ac-input w-full px-3 py-2 text-sm" placeholder={t("goals.namePh")} value={title} onChange={(e) => setTitle(e.target.value)} />
          <input type="date" className="ac-input px-3 py-2 text-sm" value={targetDate} onChange={(e) => setTargetDate(e.target.value)} />
          <button onClick={() => void create()} className="ac-btn px-4 py-2 text-sm">{t("btn.add")}</button>
        </div>
      )}
      {loading ? (<div className="flex justify-center py-8"><Loader className="w-5 h-5 animate-spin text-ac-muted" /></div>)
        : goals.length === 0 ? (<p className="text-sm text-ac-muted text-center py-12">{t("goals.empty")}</p>)
        : (<div className="grid grid-cols-1 md:grid-cols-2 gap-3">
          {goals.map((g) => (
            <div key={g.id} className="group p-4 rounded-lg border border-ac-border bg-ac-surface">
              <div className="flex items-start gap-2">
                <Target className="w-4 h-4 text-ac-brand mt-0.5 shrink-0" />
                <div className="flex-1 min-w-0">
                  <p className="text-sm font-medium text-ac-ink">{g.title}</p>
                  {g.target_date && <p className="text-xs text-ac-muted mt-0.5">{t("goals.target")}: {g.target_date}</p>}
                </div>
                <button onClick={() => void remove(g.id)} className="opacity-0 group-hover:opacity-100 text-ac-faint hover:text-ac-red"><Trash2 className="w-3.5 h-3.5" /></button>
              </div>
            </div>
          ))}
        </div>)}
    </div>
  );
}
export default GoalsView;
