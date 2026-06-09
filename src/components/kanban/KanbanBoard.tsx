// src/components/kanban/KanbanBoard.tsx
// v0.5.0: Kanban board view with columns and tasks

import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Plus, Trash, RefreshCw } from "lucide-react";
import { useTranslation } from "../../hooks/useTranslation";

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

  if (boards.length === 0) {
    return (
      <div className="flex items-center justify-center h-full">
        <div className="text-center">
          <svg className="w-12 h-12 text-ac-stone mx-auto mb-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <path d="M12 2L2 7l10 5 10-5-10-5z" />
            <path d="M2 17l10 5 10-5" />
            <path d="M2 12l10 5 10-5" />
          </svg>
          <p className="text-ac-stone mb-4">{t("no_boards")}</p>
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
        <div className="flex gap-2 px-4 py-2 border-b border-ac-border bg-ac-pitch/50">
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
          <button onClick={() => setShowAddTask(false)} className="px-3 py-1 text-xs text-ac-stone">
            {t("task_cancel")}
          </button>
        </div>
      )}

      {/* Columns */}
      {loading ? (
        <div className="flex items-center justify-center h-full">
          <span className="text-ac-stone text-sm">{t("loading")}</span>
        </div>
      ) : boardView ? (
        <div className="flex gap-3 p-4 overflow-x-auto flex-1">
          {boardView.columns.map((col) => (
            <KanbanColumnView
              key={col.key}
              column={col}
              onMoveTask={handleMoveTask}
              onDeleteTask={handleDeleteTask}
              t={t}
            />
          ))}
        </div>
      ) : null}
    </div>
  );
}

// ── Column Component ─────────────────────────────────────────────────────
function KanbanColumnView({
  column,
  onMoveTask,
  onDeleteTask,
  t,
}: {
  column: KanbanColumn;
  onMoveTask: (taskId: string, newStatus: string) => void;
  onDeleteTask: (taskId: string) => void;
  t: (key: any) => string;
}) {
  const colorClass = COLUMN_COLORS[column.key] || "border-ac-border";
  const label = t(column.key as any) || column.label;

  return (
    <div className="flex-shrink-0 w-56">
      <div className={`flex items-center justify-between mb-2 border-l-2 ${colorClass} pl-2`}>
        <span className="text-xs font-medium text-ac-ivory">{label}</span>
        <span className="text-[10px] text-ac-stone">{column.tasks.length}</span>
      </div>

      <div className="space-y-1.5 min-h-[100px]">
        {column.tasks.map((task) => (
          <div
            key={task.id}
            className="bg-ac-pitch/50 border border-ac-border rounded p-2 text-xs"
          >
            <div className="flex items-start justify-between gap-1">
              <span className="text-ac-ivory font-medium leading-tight">{task.title}</span>
              <button onClick={() => onDeleteTask(task.id)} className="text-ac-stone hover:text-ac-red">
                <Trash className="w-3 h-3" />
              </button>
            </div>
            {task.body && (
              <p className="text-ac-stone mt-1 line-clamp-2">{task.body}</p>
            )}

            {/* Move buttons */}
            <div className="flex gap-1 mt-2">
              {column.key !== "backlog" && (
                <button
                  onClick={() => onMoveTask(task.id, "backlog")}
                  className="px-1.5 py-0.5 text-[10px] text-ac-stone hover:text-ac-ivory border border-ac-border rounded"
                >
                  ← {t("backlog")}
                </button>
              )}
              {column.key !== "done" && (
                <button
                  onClick={() => {
                    const next = column.key === "backlog" ? "todo"
                      : column.key === "todo" ? "in_progress"
                      : column.key === "in_progress" ? "review"
                      : "done";
                    onMoveTask(task.id, next);
                  }}
                  className="px-1.5 py-0.5 text-[10px] text-ac-stone hover:text-ac-ivory border border-ac-border rounded"
                >
                  {t("next")}
                </button>
              )}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}