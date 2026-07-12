// src/components/kanban/KanbanBoard.tsx
// v0.5.0: Kanban board view with columns and tasks.
// B5 (2026-07-07): drag-and-drop between columns via @dnd-kit.

import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Plus, Trash, RefreshCw } from "lucide-react";
import { useTranslation } from "../../hooks/useTranslation";
import {
  DndContext,
  DragOverlay,
  PointerSensor,
  useSensor,
  useSensors,
  useDraggable,
  useDroppable,
  closestCorners,
  type DragStartEvent,
  type DragEndEvent,
} from "@dnd-kit/core";
import { CSS } from "@dnd-kit/utilities";

interface KanbanTask {
  id: string;
  title: string;
  body: string | null;
  assignee: string | null;
  status: string;
  priority: number;
  board_slug: string;
  created_at: number | null;
  started_at: number | null;
  completed_at: number | null;
}

interface KanbanColumn {
  key: string;
  label: string;
  tasks: KanbanTask[];
}

interface KanbanBoardView {
  board: {
    slug: string;
    name: string;
    description: string | null;
    total: number;
    counts: Record<string, number>;
  };
  columns: KanbanColumn[];
}

const COLUMN_COLORS: Record<string, string> = {
  backlog: "border-gray-500",
  todo: "border-blue-500",
  in_progress: "border-yellow-500",
  review: "border-purple-500",
  done: "border-green-500",
};

export function KanbanBoard() {
  const { t } = useTranslation();
  const [boards, setBoards] = useState<Array<{ slug: string; name: string; total?: number }>>([]);
  const [activeBoard, setActiveBoard] = useState<string | null>(null);
  const [boardView, setBoardView] = useState<KanbanBoardView | null>(null);
  const [loading, setLoading] = useState(false);
  const [newTaskTitle, setNewTaskTitle] = useState("");
  const [showAddTask, setShowAddTask] = useState(false);
  const [activeTask, setActiveTask] = useState<KanbanTask | null>(null);

  // Drag needs a small movement before it starts so clicks (delete) still work.
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 6 } }),
  );

  useEffect(() => {
    loadBoards();
  }, []);

  useEffect(() => {
    if (activeBoard) {
      loadBoard(activeBoard);
    }
  }, [activeBoard]);

  const loadBoards = async () => {
    try {
      const result = await invoke<Array<{ slug: string; name: string; total: number; counts: Record<string, number> }>>("list_kanban_boards_cmd");
      setBoards(result);
      if (result.length > 0 && !activeBoard) {
        setActiveBoard(result[0].slug);
      }
    } catch (e) {
      console.error("Failed to load boards:", e);
    }
  };

  const loadBoard = async (slug: string) => {
    setLoading(true);
    try {
      const result = await invoke<KanbanBoardView>("list_kanban_tasks_cmd", { boardSlug: slug });
      setBoardView(result);
    } catch (e) {
      console.error("Failed to load board:", e);
    }
    setLoading(false);
  };

  const handleAddTask = async () => {
    if (!newTaskTitle.trim() || !activeBoard) return;
    try {
      await invoke("create_kanban_task_cmd", {
        boardSlug: activeBoard,
        title: newTaskTitle.trim(),
        status: "todo",
      });
      setNewTaskTitle("");
      setShowAddTask(false);
      loadBoard(activeBoard);
    } catch (e) {
      console.error("Failed to add task:", e);
    }
  };

  const handleMoveTask = async (taskId: string, newStatus: string) => {
    try {
      await invoke("move_kanban_task_cmd", { taskId, newStatus });
      if (activeBoard) loadBoard(activeBoard);
    } catch (e) {
      console.error("Failed to move task:", e);
    }
  };

  const handleDeleteTask = async (taskId: string) => {
    try {
      await invoke("delete_kanban_task_cmd", { taskId });
      if (activeBoard) loadBoard(activeBoard);
    } catch (e) {
      console.error("Failed to delete task:", e);
    }
  };

  const handleCreateBoard = async () => {
    const name = prompt(t("create_board_prompt"));
    if (!name) return;
    const slug = name.toLowerCase().replace(/\s+/g, "-");
    try {
      await invoke("create_kanban_board_cmd", { slug, name });
      loadBoards();
    } catch (e) {
      console.error("Failed to create board:", e);
    }
  };

  const onDragStart = (event: DragStartEvent) => {
    const id = String(event.active.id);
    const found = boardView?.columns
      .flatMap((c) => c.tasks)
      .find((tk) => tk.id === id);
    setActiveTask(found ?? null);
  };

  const onDragEnd = (event: DragEndEvent) => {
    setActiveTask(null);
    const { active, over } = event;
    if (!over) return;
    const fromStatus = (active.data.current?.status as string) ?? "";
    const toStatus = String(over.id);
    if (fromStatus && toStatus && fromStatus !== toStatus) {
      handleMoveTask(String(active.id), toStatus);
    }
  };

  if (boards.length === 0) {
    return (
      <div className="flex items-center justify-center h-full">
        <div className="text-center">
          <svg className="w-12 h-12 text-ac-muted mx-auto mb-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <path d="M12 2L2 7l10 5 10-5-10-5z" />
            <path d="M2 17l10 5 10-5" />
            <path d="M2 12l10 5 10-5" />
          </svg>
          <p className="text-ac-muted mb-4">{t("no_boards")}</p>
          <button onClick={handleCreateBoard} className="ac-btn px-4 py-2 text-sm">
            {t("create_board")}
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full">
      {/* Board selector + actions */}
      <div className="flex items-center gap-2 px-4 py-2 border-b border-ac-border">
        <select
          value={activeBoard || ""}
          onChange={(e) => setActiveBoard(e.target.value)}
          className="ac-input px-3 py-1.5 text-sm flex-1"
        >
          {boards.map((b) => (
            <option key={b.slug} value={b.slug}>
              {b.name} ({b.total ?? 0})
            </option>
          ))}
        </select>
        <button onClick={handleCreateBoard} className="ac-btn px-2 py-1 text-xs" title={t("create_board")}>
          <Plus className="w-3 h-3" />
        </button>
        <button onClick={() => activeBoard && loadBoard(activeBoard)} className="ac-btn px-2 py-1 text-xs" title={t("refresh")}>
          <RefreshCw className="w-3 h-3" />
        </button>
        {activeBoard && (
          <button onClick={() => setShowAddTask(!showAddTask)} className="ac-btn px-2 py-1 text-xs">
            {t("add_task")}
          </button>
        )}
      </div>

      {/* Add task form */}
      {showAddTask && (
        <div className="flex gap-2 px-4 py-2 border-b border-ac-border bg-ac-bg/50">
          <input
            type="text"
            value={newTaskTitle}
            onChange={(e) => setNewTaskTitle(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && handleAddTask()}
            placeholder={t("task_name_placeholder")}
            className="ac-input flex-1 px-3 py-1.5 text-sm"
            autoFocus
          />
          <button onClick={handleAddTask} className="ac-btn px-3 py-1 text-xs">
            {t("task_add")}
          </button>
          <button onClick={() => setShowAddTask(false)} className="px-3 py-1 text-xs text-ac-muted">
            {t("task_cancel")}
          </button>
        </div>
      )}

      {/* Columns */}
      {loading ? (
        <div className="flex items-center justify-center h-full">
          <span className="text-ac-muted text-sm">{t("loading")}</span>
        </div>
      ) : boardView ? (
        <DndContext
          sensors={sensors}
          collisionDetection={closestCorners}
          onDragStart={onDragStart}
          onDragEnd={onDragEnd}
        >
          <div className="flex gap-3 p-4 overflow-x-auto flex-1">
            {boardView.columns.map((col) => (
              <KanbanColumnView
                key={col.key}
                column={col}
                onDeleteTask={handleDeleteTask}
                t={t}
              />
            ))}
          </div>
          <DragOverlay>
            {activeTask ? (
              <div className="w-52 bg-ac-bg border border-ac-brand rounded p-2 text-xs shadow-lg">
                <span className="text-ac-ink font-medium leading-tight">{activeTask.title}</span>
              </div>
            ) : null}
          </DragOverlay>
        </DndContext>
      ) : null}
    </div>
  );
}

// ── Column Component ─────────────────────────────────────────────────────
function KanbanColumnView({
  column,
  onDeleteTask,
  t,
}: {
  column: KanbanColumn;
  onDeleteTask: (taskId: string) => void;
  t: (key: any) => string;
}) {
  const colorClass = COLUMN_COLORS[column.key] || "border-ac-border";
  const label = t(column.key as any) || column.label;
  const { setNodeRef, isOver } = useDroppable({ id: column.key });

  return (
    <div className="flex-shrink-0 w-56 flex flex-col">
      <div className={`flex items-center justify-between mb-2 border-l-2 ${colorClass} pl-2`}>
        <span className="text-xs font-medium text-ac-ink">{label}</span>
        <span className="text-[10px] text-ac-muted">{column.tasks.length}</span>
      </div>

      <div
        ref={setNodeRef}
        className={`space-y-1.5 min-h-[100px] flex-1 rounded-md transition-colors ${
          isOver ? "bg-ac-brand-soft outline-dashed outline-1 outline-ac-brand" : ""
        }`}
      >
        {column.tasks.map((task) => (
          <KanbanCardView key={task.id} task={task} columnKey={column.key} onDeleteTask={onDeleteTask} />
        ))}
      </div>
    </div>
  );
}

// ── Draggable Card Component ─────────────────────────────────────────────
function KanbanCardView({
  task,
  columnKey,
  onDeleteTask,
}: {
  task: KanbanTask;
  columnKey: string;
  onDeleteTask: (taskId: string) => void;
}) {
  const { attributes, listeners, setNodeRef, transform, isDragging } = useDraggable({
    id: task.id,
    data: { status: columnKey },
  });

  const style = {
    transform: CSS.Translate.toString(transform),
    opacity: isDragging ? 0.4 : 1,
  };

  return (
    <div
      ref={setNodeRef}
      style={style}
      {...attributes}
      {...listeners}
      className="bg-ac-bg/50 border border-ac-border rounded p-2 text-xs cursor-grab active:cursor-grabbing touch-none"
    >
      <div className="flex items-start justify-between gap-1">
        <span className="text-ac-ink font-medium leading-tight">{task.title}</span>
        <button
          onClick={() => onDeleteTask(task.id)}
          className="text-ac-muted hover:text-ac-red"
        >
          <Trash className="w-3 h-3" />
        </button>
      </div>
      {task.body && (
        <p className="text-ac-muted mt-1 line-clamp-2">{task.body}</p>
      )}
      {task.assignee && (
        <p className="text-ac-muted mt-1 text-[10px]">👤 {task.assignee}</p>
      )}
    </div>
  );
}
