// src/components/views/KanbanView.tsx
// Visual Kanban board: 3 columns (To Do / In Progress / Done) with tasks
// movable between columns via drag-and-drop using dnd-kit.

import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ChevronLeft, Loader, GripVertical } from "lucide-react";
import {
  DndContext,
  closestCenter,
  KeyboardSensor,
  PointerSensor,
  useSensor,
  useSensors,
  DragEndEvent,
  useDroppable,
} from "@dnd-kit/core";
import {
  SortableContext,
  sortableKeyboardCoordinates,
  verticalListSortingStrategy,
  useSortable,
} from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { useTranslation } from "../../hooks/useTranslation";

interface Task {
  id: number;
  title: string;
  status: string;
  priority: number;
  due_date: string | null;
  project_id: number | null;
  assignee: string;
  labels: string;
}
interface Project { id: number; name: string; color: string; }

const PRIO_DOT: Record<number, string> = { 1: "#f44", 2: "#f80", 3: "#fa0", 4: "#8a8", 5: "#888" };
const COLUMNS = ["todo", "in_progress", "done"] as const;
const COL_LABELS: Record<string, { ru: string; en: string; color: string }> = {
  todo: { ru: "К выполнению", en: "To Do", color: "#6b7280" },
  in_progress: { ru: "В работе", en: "In Progress", color: "#f59e0b" },
  done: { ru: "Готово", en: "Done", color: "#22c55e" },
};

function KanbanTaskCard({
  task,
  projectName,
  projectColor,
}: {
  task: Task;
  projectName?: string;
  projectColor?: string;
}) {
  const {
    attributes,
    listeners,
    setNodeRef,
    transform,
    transition,
    isDragging,
  } = useSortable({ id: task.id });

  const style = {
    transform: CSS.Transform.toString(transform),
    transition,
    opacity: isDragging ? 0.5 : 1,
  };

  return (
    <div
      ref={setNodeRef}
      style={style}
      className={`group p-3 rounded-lg border transition-colors cursor-grab active:cursor-grabbing ${
        isDragging ? "border-ac-brand shadow-lg" : "border-ac-border bg-ac-surface hover:border-ac-brand-border"
      }`}
      {...attributes}
      {...listeners}
    >
      <div className="flex items-start gap-2 mb-1.5">
        <span
          className="w-2 h-2 rounded-full shrink-0 mt-1"
          style={{ background: PRIO_DOT[task.priority] || "#888" }}
        />
        <p className="text-sm text-ac-ink flex-1">{task.title}</p>
        <GripVertical
          className="w-4 h-4 text-ac-muted/30 hover:text-ac-brand cursor-grab flex-shrink-0"
          aria-hidden="true"
        />
      </div>
      <div className="flex items-center gap-1.5 flex-wrap ml-4">
        {task.due_date && <span className="text-[10px] text-ac-muted">📅 {task.due_date}</span>}
        {projectName && (
          <span
            className="text-[10px] px-1.5 py-0.5 rounded-full"
            style={{ background: (projectColor || "#888") + "22", color: projectColor || "#888" }}
          >
            {projectName}
          </span>
        )}
        {task.assignee && <span className="text-[10px] text-ac-muted">👤 {task.assignee}</span>}
        {task.labels && task.labels.split(",").map((lb, i) => (
          <span key={i} className="text-[10px] px-1.5 py-0.5 rounded-full bg-ac-brand-soft text-ac-brand">
            #{lb.trim()}
          </span>
        ))}
      </div>
    </div>
  );
}

function KanbanColumn({
  columnId,
  tasks,
  projectMap,
}: {
  columnId: string;
  tasks: Task[];
  projectMap: Map<number, { name: string; color: string }>;
}) {
  const meta = COL_LABELS[columnId];
  const { setNodeRef, isOver } = useDroppable({ id: columnId });

  return (
    <div
      ref={setNodeRef}
      className={`w-72 shrink-0 flex flex-col ${isOver ? "bg-ac-brand/5" : ""}`}
    >
      {/* Column header */}
      <div className="flex items-center gap-2 mb-3 px-2">
        <span className="w-2.5 h-2.5 rounded-full" style={{ background: meta.color }} />
        <span className="text-sm font-medium text-ac-ink">{meta.ru}</span>
        <span className="text-xs text-ac-faint ml-auto bg-ac-surface px-1.5 py-0.5 rounded-full">
          {tasks.length}
        </span>
      </div>

      {/* Sortable tasks */}
      <SortableContext
        items={tasks.map((t) => t.id)}
        strategy={verticalListSortingStrategy}
      >
        <div className="flex-1 space-y-2 overflow-y-auto min-h-[200px]">
          {tasks.map((task) => {
            const project = task.project_id ? projectMap.get(task.project_id) : undefined;
            return (
              <KanbanTaskCard
                key={task.id}
                task={task}
                projectName={project?.name}
                projectColor={project?.color}
              />
            );
          })}
          {tasks.length === 0 && (
            <p className="text-xs text-ac-faint text-center py-4">—</p>
          )}
        </div>
      </SortableContext>
    </div>
  );
}

export function KanbanView({
  projectId,
  projectName,
  onBack,
}: {
  projectId?: number | null;
  projectName?: string;
  onBack?: () => void;
}) {
  const { t } = useTranslation();
  const [tasks, setTasks] = useState<Task[]>([]);
  const [projects, setProjects] = useState<Project[]>([]);
  const [loading, setLoading] = useState(true);

  const sensors = useSensors(
    useSensor(PointerSensor, {
      activationConstraint: { distance: 8 },
    }),
    useSensor(KeyboardSensor, {
      coordinateGetter: sortableKeyboardCoordinates,
    })
  );

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const [t1, p1] = await Promise.all([
        invoke<Task[]>("list_tasks_cmd", { profile: null }),
        invoke<Project[]>("list_projects_cmd", { profile: null }),
      ]);
      setTasks(projectId ? t1.filter((x) => x.project_id === projectId) : t1);
      setProjects(p1);
    } catch (e) {
      console.error(e);
    } finally {
      setLoading(false);
    }
  }, [projectId]);

  useEffect(() => {
    void load();
  }, [load]);

  const projectMap = new Map(projects.map((p) => [p.id, { name: p.name, color: p.color }]));

  const moveTask = async (taskId: number, newStatus: string) => {
    try {
      await invoke("update_task_status_cmd", { id: taskId, status: newStatus, profile: null });
      void load();
    } catch (e) {
      console.error("Failed to move task:", e);
    }
  };

  const handleDragEnd = (event: DragEndEvent) => {
    const { active, over } = event;
    if (!over) return;

    const activeId = Number(active.id);
    const overId = over.id as string;

    // overId is the column id (todo, in_progress, done)
    if (COLUMNS.includes(overId as typeof COLUMNS[number])) {
      moveTask(activeId, overId);
    }
  };

  const tasksByStatus = (status: string) => tasks.filter((x) => x.status === status);

  if (loading) {
    return (
      <div className="h-full flex flex-col">
        <div className="flex items-center gap-3 px-6 py-3 border-b border-ac-border">
          {onBack && (
            <button onClick={onBack} className="p-1.5 rounded-md hover:bg-ac-surface text-ac-muted hover:text-ac-brand">
              <ChevronLeft className="w-5 h-5" />
            </button>
          )}
          <h2 className="text-lg font-semibold text-ac-ink">{projectName ? `Канбан: ${projectName}` : t("kanban.title")}</h2>
        </div>
        <div className="flex justify-center py-12 flex-1">
          <Loader className="w-6 h-6 animate-spin text-ac-muted" />
        </div>
      </div>
    );
  }

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

      {/* Board */}
      <div className="flex-1 flex gap-4 overflow-x-auto p-6">
        <DndContext
          sensors={sensors}
          collisionDetection={closestCenter}
          onDragEnd={handleDragEnd}
        >
          {COLUMNS.map((col) => (
            <KanbanColumn
              key={col}
              columnId={col}
              tasks={tasksByStatus(col)}
              projectMap={projectMap}
            />
          ))}
        </DndContext>
      </div>
    </div>
  );
}

export default KanbanView;