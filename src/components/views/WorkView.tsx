// src/components/views/WorkView.tsx
// Consolidated "Работа" section: tasks, kanban, goals, projects, protocols,
// stats — all as internal sub-tabs. Drill-down navigation (Goal → Project →
// Task) lives here instead of in App.tsx, so the sidebar stays at 4 items.

import { useState, useCallback } from "react";
import {
  SquareCheckBig,
  Columns3,
  Target,
  FolderOpen,
  FileText,
  ChartColumn,
  ChevronRight,
  type LucideIcon,
} from "lucide-react";
import { useTranslation } from "../../hooks/useTranslation";
import { TasksView } from "./TasksView";
import { KanbanView } from "./KanbanView";
import { GoalsView } from "./GoalsView";
import { ProjectsView } from "./ProjectsView";
import { ProtocolsView } from "./ProtocolsView";
import { StatsView } from "./StatsView";

export type WorkTab =
  | "tasks"
  | "kanban"
  | "goals"
  | "projects"
  | "protocols"
  | "stats";

interface WorkViewProps {
  /** Initial sub-tab to show (e.g. "tasks" when entering from dashboard). */
  initialTab?: WorkTab;
  /** Pre-set project drill-down (e.g. opened from a dashboard widget). */
  initialProjectId?: number | null;
  initialProjectName?: string;
  /** When the dashboard "new task" button is clicked we pass this. */
  focusNewTask?: boolean;
}

const TABS: { id: WorkTab; icon: LucideIcon; labelKey: string }[] = [
  { id: "tasks", icon: SquareCheckBig, labelKey: "nav.tasks" },
  { id: "kanban", icon: Columns3, labelKey: "kanban.title" },
  { id: "goals", icon: Target, labelKey: "nav.goals" },
  { id: "projects", icon: FolderOpen, labelKey: "nav.projects" },
  { id: "protocols", icon: FileText, labelKey: "nav.protocols" },
  { id: "stats", icon: ChartColumn, labelKey: "nav.stats" },
];

export function WorkView({
  initialTab = "tasks",
  initialProjectId = null,
  initialProjectName,
}: WorkViewProps) {
  const { t } = useTranslation();
  const [tab, setTab] = useState<WorkTab>(initialTab);

  // Drill-down state — previously in App.tsx, now local.
  const [drillGoalId, setDrillGoalId] = useState<number | null>(null);
  const [drillProjectId, setDrillProjectId] = useState<number | null>(
    initialProjectId
  );
  const [drillProjectName, setDrillProjectName] = useState<
    string | undefined
  >(initialProjectName);

  // ── Drill-down navigation handlers ────────────────────────────────────
  // Goals → (click a project) → Tasks scoped to that project.
  const handleOpenProjectFromGoal = useCallback(
    (pid: number, pname: string) => {
      setDrillProjectId(pid);
      setDrillProjectName(pname);
      setTab("tasks");
    },
    []
  );

  // Projects → (click a project's tasks) → Tasks scoped.
  const handleOpenTasksFromProject = useCallback(
    (pid: number, pname: string) => {
      setDrillProjectId(pid);
      setDrillProjectName(pname);
      setTab("tasks");
    },
    []
  );

  // Tasks/Kanban → Back → Projects (or Goals if came from Goals).
  const handleBackFromTasks = useCallback(() => {
    setDrillProjectId(null);
    setDrillProjectName(undefined);
    setTab("projects");
  }, []);

  const handleBackFromProjects = useCallback(() => {
    setDrillGoalId(null);
    setTab("goals");
  }, []);

  return (
    <div className="flex flex-col flex-1 min-h-0 overflow-hidden">
      {/* Breadcrumb trail — shows drill-down context */}
      {(drillProjectId !== null || tab !== "tasks") && (
        <div className="flex items-center gap-1.5 px-4 py-1.5 border-b border-ac-border bg-ac-bg text-[11px] text-ac-muted">
          <button
            onClick={() => { setDrillGoalId(null); setDrillProjectId(null); setTab("tasks"); }}
            className="hover:text-ac-brand"
          >
            {t("nav.work")}
          </button>
          {tab !== "tasks" && (
            <>
              <ChevronRight className="w-3 h-3" />
              <span className="text-ac-ink font-medium">{t(TABS.find((tb) => tb.id === tab)?.labelKey || "")}</span>
            </>
          )}
          {drillProjectId !== null && drillProjectName && (
            <>
              <ChevronRight className="w-3 h-3" />
              <span className="text-ac-ink font-medium truncate max-w-40">{drillProjectName}</span>
            </>
          )}
        </div>
      )}

      {/* Sub-tab bar */}
      <div className="flex items-center gap-1 px-4 py-2 border-b border-ac-border bg-ac-surface shrink-0 overflow-x-auto" role="tablist">
        {TABS.map((tb) => {
          const active = tab === tb.id;
          return (
            <button
              key={tb.id}
              onClick={() => setTab(tb.id)}
              role="tab"
              aria-selected={active}
              className={`flex items-center gap-1.5 rounded-md px-3 py-1.5 text-xs font-medium whitespace-nowrap transition-colors ${
                active
                  ? "bg-ac-brand-soft text-ac-brand"
                  : "text-ac-muted hover:bg-ac-surface-2 hover:text-ac-ink"
              }`}
            >
              <tb.icon className="w-3.5 h-3.5" />
              {t(tb.labelKey)}
            </button>
          );
        })}
      </div>

      {/* Active sub-tab content */}
      {tab === "tasks" && (
        <div className="flex-1 overflow-y-auto">
          <TasksView
            projectId={drillProjectId}
            projectName={drillProjectName}
            onBack={handleBackFromTasks}
          />
        </div>
      )}
      {tab === "kanban" && (
        // Kanban manages its own full-height layout — no scroll wrapper.
        <KanbanView
          projectId={drillProjectId}
          projectName={drillProjectName}
          onBack={handleBackFromTasks}
        />
      )}
      {tab === "goals" && (
        <div className="flex-1 overflow-y-auto">
          <GoalsView onOpenProject={handleOpenProjectFromGoal} />
        </div>
      )}
      {tab === "projects" && (
        <div className="flex-1 overflow-y-auto">
          <ProjectsView
            goalId={drillGoalId}
            onBack={handleBackFromProjects}
            onOpenTasks={handleOpenTasksFromProject}
          />
        </div>
      )}
      {tab === "protocols" && (
        <div className="flex-1 overflow-y-auto">
          <ProtocolsView />
        </div>
      )}
      {tab === "stats" && (
        <div className="flex-1 overflow-y-auto">
          <StatsView />
        </div>
      )}
    </div>
  );
}

export default WorkView;
