/**
 * Kanban Service - Typed wrapper for kanban commands.
 * Components should import from here instead of calling invoke() directly.
 */
import { invoke } from "@tauri-apps/api/core";

export const kanbanService = {
  // List kanban boards
  async listKanbanBoards(): Promise<Array<{ id: string; name: string; slug: string }>> {
    return invoke("list_kanban_boards_cmd");
  },

  // Create kanban board
  async createKanbanBoard(slug: string, name: string): Promise<void> {
    return invoke("create_kanban_board_cmd", { slug, name });
  },

  // Delete kanban board
  async deleteKanbanBoard(id: string): Promise<void> {
    return invoke("delete_kanban_board_cmd", { id });
  },

  // List kanban tasks
  async listKanbanTasks(boardId: string): Promise<Array<{
    id: string;
    title: string;
    status: string;
    boardId: string;
    priority: number;
  }>> {
    return invoke("list_kanban_tasks_cmd", { boardId });
  },

  // Create kanban task
  async createKanbanTask(boardId: string, title: string, status: string): Promise<void> {
    return invoke("create_kanban_task_cmd", { boardId, title, status });
  },

  // Update kanban task
  async updateKanbanTask(taskId: string, updates: { title?: string; status?: string; priority?: number }): Promise<void> {
    return invoke("update_kanban_task_cmd", { taskId, updates });
  },

  // Delete kanban task
  async deleteKanbanTask(taskId: string): Promise<void> {
    return invoke("delete_kanban_task_cmd", { taskId });
  },

  // Move kanban task
  async moveKanbanTask(taskId: string, newStatus: string): Promise<void> {
    return invoke("move_kanban_task_cmd", { taskId, newStatus });
  },
};