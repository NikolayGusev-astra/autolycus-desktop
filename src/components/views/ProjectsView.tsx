// src/components/views/ProjectsView.tsx
// Projects with drill-down: click a project → see its tasks. Edit name/color.
import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Plus, Trash2, FolderOpen, Target, Loader, ChevronLeft, Pencil, ListChecks, Layers } from "lucide-react";
import { useTranslation } from "../../hooks/useTranslation";

interface Project { id: number; name: string; color: string; description: string; goal_id?: number | null; }
interface Goal { id: number; title: string; }
interface Task { id: number; title: string; status: string; project_id: number | null; }
interface Section { id: number; project_id: number; name: string; position: number; }

export function ProjectsView({ goalId, goalTitle, onBack, onOpenTasks }: {
  goalId?: number | null; goalTitle?: string; onBack?: () => void;
  onOpenTasks?: (pid: number, pname: string) => void;
}) {
  const { t } = useTranslation();
  const [projects, setProjects] = useState<Project[]>([]);
  const [goals, setGoals] = useState<Goal[]>([]);
  const [tasks, setTasks] = useState<Task[]>([]);
  const [loading, setLoading] = useState(true);
  const [showForm, setShowForm] = useState(false);
  const [editingId, setEditingId] = useState<number | null>(null);
  const [name, setName] = useState("");
  const [color, setColor] = useState("#f82530");
  const [selGoalId, setSelGoalId] = useState<number | null>(goalId ?? null);
  const [sections, setSections] = useState<Record<number, Section[]>>({});
  const [expandedProject, setExpandedProject] = useState<number | null>(null);
  const [newSectionName, setNewSectionName] = useState("");

  const loadSections = useCallback(async (projectId: number) => {
    try {
      const s = await invoke<Section[]>("list_sections_cmd", { projectId, profile: null });
      setSections((prev) => ({ ...prev, [projectId]: s }));
    } catch (e) { console.error(e); }
  }, []);

  const createSection = async (projectId: number) => {
    if (!newSectionName.trim()) return;
    await invoke("create_section_cmd", { projectId, name: newSectionName.trim(), profile: null });
    setNewSectionName("");
    void loadSections(projectId);
  };

  const deleteSection = async (sectionId: number, projectId: number) => {
    await invoke("delete_section_cmd", { id: sectionId, profile: null });
    void loadSections(projectId);
  };

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const [p, g, tk] = await Promise.all([
        invoke<Project[]>("list_projects_cmd", { profile: null }),
        invoke<Goal[]>("list_goals_cmd", { profile: null }),
        invoke<Task[]>("list_tasks_cmd", { profile: null }),
      ]);
      setProjects(goalId ? p.filter((x) => x.goal_id === goalId) : p);
      setGoals(g); setTasks(tk);
    } catch (e) { console.error(e); } finally { setLoading(false); }
  }, [goalId]);
  useEffect(() => { void load(); }, [load]);

  const create = async () => {
    if (!name.trim()) return;
    await invoke("create_project_cmd", { name: name.trim(), color, goalId: selGoalId, profile: null });
    setName(""); setShowForm(false); void load();
  };
  const remove = async (id: number) => { await invoke("delete_project_cmd", { id, profile: null }); void load(); };
  const startEdit = (p: Project) => { setEditingId(p.id); setName(p.name); setColor(p.color); setSelGoalId(p.goal_id ?? null); };
  const saveEdit = async () => {
    if (editingId === null) return;
    await invoke("update_project_cmd", { id: editingId, name: name.trim() || undefined, color, goalId: selGoalId, profile: null });
    setEditingId(null); void load();
  };

  const goalTitleOf = (gid?: number | null) => goals.find((g) => g.id === gid)?.title;
  const taskCount = (pid: number) => tasks.filter((tk) => tk.project_id === pid).length;

  return (
    <div className="p-6 max-w-4xl mx-auto">
      <div className="flex items-center gap-3 mb-6">
        {onBack && (
          <button onClick={onBack} className="p-1.5 rounded-md hover:bg-ac-surface text-ac-muted hover:text-ac-brand">
            <ChevronLeft className="w-5 h-5" />
          </button>
        )}
        <h2 className="text-lg font-semibold text-ac-ink">{goalTitle ? `${t("nav.projects")}: ${goalTitle}` : t("nav.projects")}</h2>
        <button onClick={() => { setShowForm(!showForm); setEditingId(null); }} className="ac-btn px-3 py-2 text-sm flex items-center gap-1.5 ml-auto">
          <Plus className="w-4 h-4" /> {t("projects.new")}
        </button>
      </div>

      {(showForm || editingId !== null) && (
        <div className="mb-4 p-4 rounded-lg border border-ac-border bg-ac-surface space-y-3">
          <div className="flex gap-3 items-center">
            <input type="color" value={color} onChange={(e) => setColor(e.target.value)} className="w-9 h-9 rounded border border-ac-border cursor-pointer" />
            <input className="ac-input flex-1 px-3 py-2 text-sm" placeholder={t("projects.namePh")} value={name} onChange={(e) => setName(e.target.value)} />
          </div>
          {!goalId && (
            <select className="ac-input w-full px-3 py-2 text-sm" value={selGoalId ?? ""} onChange={(e) => setSelGoalId(e.target.value ? Number(e.target.value) : null)}>
              <option value="">{t("projects.noGoal")}</option>
              {goals.map((g) => <option key={g.id} value={g.id}>{g.title}</option>)}
            </select>
          )}
          <button onClick={() => editingId !== null ? saveEdit() : create()} className="ac-btn px-4 py-2 text-sm">
            {editingId !== null ? t("btn.save") : t("btn.add")}
          </button>
        </div>
      )}

      {loading ? (<div className="flex justify-center py-8"><Loader className="w-5 h-5 animate-spin text-ac-muted" /></div>)
        : projects.length === 0 ? (<p className="text-sm text-ac-muted text-center py-12">{t("projects.empty")}</p>)
        : (<div className="grid grid-cols-1 md:grid-cols-2 gap-3">
          {projects.map((p) => (
            <div key={p.id} className="group p-4 rounded-lg border border-ac-border bg-ac-surface">
              <div className="flex items-center gap-3">
                <FolderOpen className="w-5 h-5 shrink-0" style={{ color: p.color }} />
                <span className="flex-1 text-sm font-medium text-ac-ink truncate">{p.name}</span>
                <button onClick={() => startEdit(p)} className="p-1 rounded text-ac-faint hover:text-ac-brand hover:bg-ac-bg"><Pencil className="w-3.5 h-3.5" /></button>
                <button onClick={() => void remove(p.id)} className="p-1 rounded text-ac-faint hover:text-ac-red hover:bg-ac-bg"><Trash2 className="w-3.5 h-3.5" /></button>
              </div>
              {goalTitleOf(p.goal_id) && (
                <p className="text-[11px] text-ac-muted mt-1.5 flex items-center gap-1"><Target className="w-3 h-3" /> {goalTitleOf(p.goal_id)}</p>
              )}
              {taskCount(p.id) > 0 && (
                <button onClick={() => onOpenTasks?.(p.id, p.name)} className="mt-2 flex items-center gap-1.5 text-xs text-ac-brand hover:underline">
                  <ListChecks className="w-3.5 h-3.5" /> {taskCount(p.id)} {t("nav.tasks").toLowerCase()}
                </button>
              )}
              <button onClick={() => { setExpandedProject(expandedProject === p.id ? null : p.id); if (expandedProject !== p.id) void loadSections(p.id); }} className="mt-2 flex items-center gap-1.5 text-xs text-ac-muted hover:text-ac-brand">
                <Layers className="w-3.5 h-3.5" /> {t("projects.sections")}
              </button>
              {expandedProject === p.id && (
                <div className="mt-2 space-y-2">
                  <div className="flex gap-2">
                    <input className="ac-input flex-1 px-2 py-1 text-xs" placeholder={t("projects.sectionNamePh")} value={newSectionName} onChange={(e) => setNewSectionName(e.target.value)} onKeyDown={(e) => e.key === "Enter" && void createSection(p.id)} />
                    <button onClick={() => void createSection(p.id)} className="ac-btn px-2 py-1 text-xs"><Plus className="w-3 h-3" /></button>
                  </div>
                  {(sections[p.id] || []).length === 0 ? (
                    <p className="text-xs text-ac-faint">{t("projects.noSections")}</p>
                  ) : (
                    <div className="space-y-1">
                      {(sections[p.id] || []).map((s) => (
                        <div key={s.id} className="flex items-center gap-2 text-xs">
                          <span className="flex-1 text-ac-ink">{s.name}</span>
                          <button onClick={() => void deleteSection(s.id, p.id)} className="text-ac-faint hover:text-ac-red"><Trash2 className="w-3 h-3" /></button>
                        </div>
                      ))}
                    </div>
                  )}
                </div>
              )}
            </div>
          ))}
        </div>)}
    </div>
  );
}
export default ProjectsView;
