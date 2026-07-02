// src/components/views/StatsView.tsx
import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Loader } from "lucide-react";
import { useTranslation } from "../../hooks/useTranslation";

interface SelfCheck { id: number; energy: number | null; joy: number | null; mood: string | null; notes: string | null; created_at: number | null; }
interface DashStats { tasks_total: number; tasks_done: number; tasks_today: number; goals_total: number; projects_total: number; }

export function StatsView() {
  const { t } = useTranslation();
  const [checks, setChecks] = useState<SelfCheck[]>([]);
  const [stats, setStats] = useState<DashStats | null>(null);
  const [loading, setLoading] = useState(true);
  // self-check form
  const [energy, setEnergy] = useState(3);
  const [joy, setJoy] = useState(3);
  const [mood, setMood] = useState("");
  const [notes, setNotes] = useState("");

  const load = useCallback(async () => {
    setLoading(true);
    try {
      setChecks(await invoke<SelfCheck[]>("list_self_checks_cmd", { profile: null }));
      setStats(await invoke<DashStats>("dash_stats_cmd", { profile: null }));
    } catch (e) { console.error(e); } finally { setLoading(false); }
  }, []);
  useEffect(() => { void load(); }, [load]);

  const submit = async () => {
    await invoke("add_self_check_cmd", { energy, joy, mood, notes, profile: null });
    setNotes(""); setMood(""); void load();
  };

  // simple sparkline from energy values
  const energyVals = checks.slice(0, 30).reverse().map((c) => c.energy ?? 0);
  const sparkPath = (vals: number[], max = 5) => {
    if (vals.length < 2) return "";
    const w = 300, h = 60;
    return vals.map((v, i) => `${i === 0 ? "M" : "L"}${(i / (vals.length - 1)) * w},${h - (v / max) * h}`).join(" ");
  };

  return (
    <div className="p-6 max-w-4xl mx-auto">
      <h2 className="text-lg font-semibold text-ac-ink mb-6">{t("nav.stats")}</h2>

      {/* Self-diagnosis form */}
      <div className="mb-6 p-4 rounded-lg border border-ac-border bg-ac-surface space-y-3">
        <p className="text-sm font-medium text-ac-ink">{t("selfDiag.hint")}</p>
        <div className="flex gap-4">
          <label className="text-xs text-ac-muted">{t("stats.energy")}: {energy}
            <input type="range" min={1} max={5} value={energy} onChange={(e) => setEnergy(Number(e.target.value))} className="w-full" />
          </label>
          <label className="text-xs text-ac-muted">{t("stats.joy")}: {joy}
            <input type="range" min={1} max={5} value={joy} onChange={(e) => setJoy(Number(e.target.value))} className="w-full" />
          </label>
        </div>
        <input className="ac-input w-full px-3 py-2 text-sm" placeholder={t("stats.mood")} value={mood} onChange={(e) => setMood(e.target.value)} />
        <textarea className="ac-input w-full px-3 py-2 text-sm" rows={2} placeholder={t("selfDiag.placeholder")} value={notes} onChange={(e) => setNotes(e.target.value)} />
        <button onClick={() => void submit()} className="ac-btn px-4 py-2 text-sm">{t("stats.saveCheck")}</button>
      </div>

      {loading ? (<div className="flex justify-center py-8"><Loader className="w-5 h-5 animate-spin text-ac-muted" /></div>) : (
        <>
          {/* Task stats */}
          {stats && (
            <div className="grid grid-cols-2 md:grid-cols-4 gap-3 mb-6">
              <StatCard label={t("stats.tasksTotal")} value={stats.tasks_total} />
              <StatCard label={t("stats.tasksDone")} value={stats.tasks_done} />
              <StatCard label={t("stats.goals")} value={stats.goals_total} />
              <StatCard label={t("stats.projects")} value={stats.projects_total} />
            </div>
          )}
          {/* Energy chart */}
          {energyVals.length >= 2 ? (
            <div className="p-4 rounded-lg border border-ac-border bg-ac-surface">
              <p className="text-sm font-medium text-ac-ink mb-3">{t("stats.energyJoy")}</p>
              <svg viewBox="0 0 300 60" className="w-full h-16">
                <path d={sparkPath(energyVals)} fill="none" stroke="#f82530" strokeWidth={2} />
              </svg>
            </div>
          ) : (
            <p className="text-sm text-ac-muted text-center py-8">{t("stats.noData")}</p>
          )}
        </>
      )}
    </div>
  );
}

function StatCard({ label, value }: { label: string; value: number }) {
  return (
    <div className="p-3 rounded-lg border border-ac-border bg-ac-surface text-center">
      <p className="text-2xl font-bold text-ac-brand">{value}</p>
      <p className="text-xs text-ac-muted mt-1">{label}</p>
    </div>
  );
}
export default StatsView;
