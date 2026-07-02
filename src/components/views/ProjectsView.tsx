// src/components/views/ProjectsView.tsx
import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Plus, Trash2, FolderOpen, Loader } from "lucide-react";
import { useTranslation } from "../../hooks/useTranslation";

interface Project { id: number; name: string; color: string; description: string; }

export function ProjectsView() {
  const { t } = useTranslation();
  const [projects, setProjects] = useState<Project[]>([]);
  const [loading, setLoading] = useState(true);
  const [showForm, setShowForm] = useState(false);
  const [name, setName] = useState("");
  const [color, setColor] = useState("#f82530");

  const load = useCallback(async () => {
    setLoading(true);
    try { setProjects(await invoke<Project[]>("list_projects_cmd", { profile: null })); }
    catch (e) { console.error(e); } finally { setLoading(false); }
  }, []);
  useEffect(() => { void load(); }, [load]);

  const create = async () => {
    if (!name.trim()) return;
    await invoke("create_project_cmd", { name: name.trim(), color, profile: null });
    setName(""); setShowForm(false); void load();
  };
  const remove = async (id: number) => { await invoke("delete_project_cmd", { id, profile: null }); void load(); };

  return (
    <div className="p-6 max-w-4xl mx-auto">
      <div className="flex items-center justify-between mb-6">
        <h2 className="text-lg font-semibold text-ac-ink">{t("nav.projects")}</h2>
        <button onClick={() => setShowForm(!showForm)} className="ac-btn px-3 py-2 text-sm flex items-center gap-1.5">
          <Plus className="w-4 h-4" /> {t("projects.new")}
        </button>
      </div>
      {showForm && (
        <div className="mb-4 p-4 rounded-lg border border-ac-border bg-ac-surface flex gap-3 items-center">
          <input type="color" value={color} onChange={(e) => setColor(e.target.value)} className="w-9 h-9 rounded border border-ac-border cursor-pointer" />
          <input className="ac-input flex-1 px-3 py-2 text-sm" placeholder={t("projects.namePh")} value={name} onChange={(e) => setName(e.target.value)} />
          <button onClick={() => void create()} className="ac-btn px-4 py-2 text-sm">{t("btn.add")}</button>
        </div>
      )}
      {loading ? (<div className="flex justify-center py-8"><Loader className="w-5 h-5 animate-spin text-ac-muted" /></div>)
        : projects.length === 0 ? (<p className="text-sm text-ac-muted text-center py-12">{t("projects.empty")}</p>)
        : (<div className="grid grid-cols-1 md:grid-cols-3 gap-3">
          {projects.map((p) => (
            <div key={p.id} className="group p-4 rounded-lg border border-ac-border bg-ac-surface flex items-center gap-3">
              <FolderOpen className="w-5 h-5 shrink-0" style={{ color: p.color }} />
              <span className="flex-1 text-sm font-medium text-ac-ink truncate">{p.name}</span>
              <button onClick={() => void remove(p.id)} className="opacity-0 group-hover:opacity-100 text-ac-faint hover:text-ac-red"><Trash2 className="w-3.5 h-3.5" /></button>
            </div>
          ))}
        </div>)}
    </div>
  );
}
export default ProjectsView;
