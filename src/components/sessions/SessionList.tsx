// src/components/sessions/SessionList.tsx
// v0.4.0: Session list from SQLite via Rust backend

import { useState, useEffect, useCallback } from "react";
import { Search, Trash2, MessageSquare, Loader, Link } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "../../hooks/useTranslation";

interface SessionSummary {
  id: string;
  source: string;
  started_at: number;
  ended_at: number | null;
  message_count: number;
  model: string;
  title: string | null;
  preview: string;
}

interface Task {
  id: number;
  title: string;
}
interface Project {
  id: number;
  name: string;
}
interface Goal {
  id: number;
  title: string;
}

export function SessionList() {
  const [sessions, setSessions] = useState<SessionSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [searchQuery, setSearchQuery] = useState("");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [linkModal, setLinkModal] = useState<{
    sessionId: string;
    tasks: Task[];
    projects: Project[];
    goals: Goal[];
    selectedTaskId: number | null;
    selectedProjectId: number | null;
    selectedGoalId: number | null;
    note: string;
  } | null>(null);
  const { t } = useTranslation();

  const loadSessions = useCallback(async () => {
    try {
      setLoading(true);
      const result = await invoke<SessionSummary[]>("list_sessions_cmd", {
        profile: null,
        limit: 50,
        offset: 0,
      });
      setSessions(result);
    } catch (err) {
      console.error("Failed to load sessions:", err);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadSessions();
  }, [loadSessions]);

  const handleSearch = async () => {
    if (!searchQuery.trim()) {
      loadSessions();
      return;
    }

    try {
      setLoading(true);
      const result = await invoke<SessionSummary[]>("search_sessions_cmd", {
        query: searchQuery,
        limit: 20,
        profile: null,
      });
      setSessions(result);
    } catch (err) {
      console.error("Search failed:", err);
    } finally {
      setLoading(false);
    }
  };

  const handleDelete = async (sessionId: string) => {
    try {
      await invoke("delete_session_cmd", { sessionId, profile: null });
      setSessions((prev) => prev.filter((s) => s.id !== sessionId));
    } catch (err) {
      console.error("Delete failed:", err);
    }
  };

  const openLinkModal = async (sessionId: string) => {
    try {
      const [tasks, projects, goals] = await Promise.all([
        invoke<Task[]>("list_tasks_cmd", { profile: null }),
        invoke<Project[]>("list_projects_cmd", { profile: null }),
        invoke<Goal[]>("list_goals_cmd", { profile: null }),
      ]);
      setLinkModal({
        sessionId,
        tasks,
        projects,
        goals,
        selectedTaskId: null,
        selectedProjectId: null,
        selectedGoalId: null,
        note: "",
      });
    } catch (err) {
      console.error("Failed to load link targets:", err);
    }
  };

  const handleLink = async () => {
    if (!linkModal) return;
    const { sessionId, selectedTaskId, selectedProjectId, selectedGoalId, note } = linkModal;
    if (!selectedTaskId && !selectedProjectId && !selectedGoalId) return;
    try {
      await invoke("link_session_cmd", {
        sessionId,
        taskId: selectedTaskId,
        projectId: selectedProjectId,
        goalId: selectedGoalId,
        linkedBy: "manual",
        note: note || null,
        profile: null,
      });
      setLinkModal(null);
    } catch (err) {
      console.error("Link failed:", err);
    }
  };

  const formatDate = (timestamp: number) => {
    if (!timestamp) return "—";
    const date = new Date(timestamp * 1000);
    return date.toLocaleDateString("ru-RU", {
      day: "2-digit",
      month: "2-digit",
      year: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    });
  };

  const filteredSessions = searchQuery
    ? sessions.filter(
        (s) =>
          s.title?.toLowerCase().includes(searchQuery.toLowerCase()) ||
          s.preview.toLowerCase().includes(searchQuery.toLowerCase()) ||
          s.id.toLowerCase().includes(searchQuery.toLowerCase())
      )
    : sessions;

  return (
    <div className="flex h-full flex-col">
      {/* Search bar */}
      <div className="px-4 py-3 border-b border-ac-border">
        <div className="flex gap-2">
          <div className="flex-1 relative">
            <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-ac-muted" />
            <input
              type="text"
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && handleSearch()}
              placeholder={t("session_search_placeholder")}
              className="ac-input w-full pl-9 pr-3 py-2 text-sm"
            />
          </div>
          <button
            onClick={handleSearch}
            className="ac-btn px-4 py-2 text-sm"
          >
            {t("session_search_button")}
          </button>
        </div>
      </div>

      {/* Session list */}
      <div className="flex-1 overflow-y-auto">
        {loading ? (
          <div className="flex items-center justify-center h-32">
            <Loader className="w-5 h-5 text-ac-brand animate-spin" />
          </div>
        ) : filteredSessions.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-32 text-ac-muted text-sm">
            <MessageSquare className="w-8 h-8 mb-2 opacity-30" />
            <p>{t("no_sessions")}</p>
          </div>
        ) : (
          <div className="divide-y divide-ac-border/30">
            {filteredSessions.map((session) => (
              <div
                key={session.id}
                className={`px-4 py-3 cursor-pointer transition-colors ${
                  selectedId === session.id
                    ? "bg-ac-brand/5 border-l-2 border-ac-brand"
                    : "hover:bg-ac-bg/50 border-l-2 border-transparent"
                }`}
                onClick={() => setSelectedId(session.id)}
              >
                <div className="flex items-start justify-between gap-2">
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2 mb-1">
                      <span className="text-sm font-medium text-ac-ink truncate">
                        {session.title || session.id.slice(0, 16)}
                      </span>
                      {session.model && (
                        <span className="text-[10px] px-1.5 py-0.5 bg-ac-brand/10 text-ac-brand rounded">
                          {session.model}
                        </span>
                      )}
                    </div>
                    <p className="text-xs text-ac-muted truncate">
                      {session.preview || t("empty_chat")}
                    </p>
                    <div className="flex items-center gap-3 mt-1 text-[10px] text-ac-muted/50">
                      <span>{formatDate(session.started_at)}</span>
                      <span>{session.message_count} {t("messages_count")}</span>
                      <span className="truncate max-w-[120px]">{session.source}</span>
                    </div>
                  </div>
                  <div className="flex items-center gap-1">
                    <button
                      onClick={(e) => {
                        e.stopPropagation();
                        openLinkModal(session.id);
                      }}
                      className="text-ac-muted/30 hover:text-ac-brand transition-colors p-1"
                      title={t("feed.toTask")}
                    >
                      <Link className="w-3.5 h-3.5" />
                    </button>
                    <button
                      onClick={(e) => {
                        e.stopPropagation();
                        handleDelete(session.id);
                      }}
                      className="text-ac-muted/30 hover:text-ac-red transition-colors p-1"
                      title={t("delete_session")}
                    >
                      <Trash2 className="w-3.5 h-3.5" />
                    </button>
                  </div>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Footer */}
      <div className="px-4 py-2 border-t border-ac-border text-[10px] text-ac-muted/50 flex justify-between">
        <span>{sessions.length} {t("sessions_count")}</span>
        <span>SQLite state.db</span>
      </div>

      {/* Link Session Modal */}
      {linkModal && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4"
          onClick={() => setLinkModal(null)}
        >
          <div
            className="w-full max-w-md bg-ac-surface border border-ac-border rounded-xl p-6 shadow-xl"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="flex items-center justify-between mb-4">
              <h3 className="text-lg font-semibold text-ac-ink">{t("feed.linkSessionHint")}</h3>
              <button
                onClick={() => setLinkModal(null)}
                className="p-1 text-ac-muted hover:text-ac-ink"
              >
                <Link className="w-5 h-5" />
              </button>
            </div>
            <p className="text-sm text-ac-muted mb-4">
              {t("feed.linkSessionHint", { sessionId: linkModal.sessionId.slice(0, 16) })}
            </p>
            <div className="space-y-4">
              <div>
                <label className="block text-xs text-ac-muted mb-2">{t("tasks.whatTodo")}</label>
                <div className="space-y-1 max-h-48 overflow-y-auto">
                  {linkModal.tasks.map((task) => (
                    <label key={task.id} className="flex items-center gap-2 cursor-pointer">
                      <input
                        type="radio"
                        name="link-target"
                        checked={linkModal.selectedTaskId === task.id}
                        onChange={() => setLinkModal((m) => m ? { ...m, selectedTaskId: task.id, selectedProjectId: null, selectedGoalId: null } : null)}
                        className="ac-radio"
                      />
                      <span className="text-sm text-ac-ink truncate">{task.title}</span>
                    </label>
                  ))}
                </div>
              </div>
              <div>
                <label className="block text-xs text-ac-muted mb-2">{t("feed.taskProject")}</label>
                <div className="space-y-1 max-h-32 overflow-y-auto">
                  {linkModal.projects.map((project) => (
                    <label key={project.id} className="flex items-center gap-2 cursor-pointer">
                      <input
                        type="radio"
                        name="link-target"
                        checked={linkModal.selectedProjectId === project.id}
                        onChange={() => setLinkModal((m) => m ? { ...m, selectedProjectId: project.id, selectedTaskId: null, selectedGoalId: null } : null)}
                        className="ac-radio"
                      />
                      <span className="text-sm text-ac-ink truncate">{project.name}</span>
                    </label>
                  ))}
                </div>
              </div>
              <div>
                <label className="block text-xs text-ac-muted mb-2">{t("feed.taskGoal")}</label>
                <div className="space-y-1 max-h-32 overflow-y-auto">
                  {linkModal.goals.map((goal) => (
                    <label key={goal.id} className="flex items-center gap-2 cursor-pointer">
                      <input
                        type="radio"
                        name="link-target"
                        checked={linkModal.selectedGoalId === goal.id}
                        onChange={() => setLinkModal((m) => m ? { ...m, selectedGoalId: goal.id, selectedTaskId: null, selectedProjectId: null } : null)}
                        className="ac-radio"
                      />
                      <span className="text-sm text-ac-ink truncate">{goal.title}</span>
                    </label>
                  ))}
                </div>
              </div>
              <div>
                <label className="block text-xs text-ac-muted mb-1">{t("feed.taskNote")}</label>
                <textarea
                  value={linkModal.note}
                  onChange={(e) => setLinkModal((m) => m ? { ...m, note: e.target.value } : null)}
                  className="w-full px-3 py-2 rounded-lg border border-ac-border bg-ac-background text-ac-ink placeholder-ac-muted focus:outline-none focus:border-ac-brand"
                  rows={2}
                  placeholder={t("selfDiag.placeholder")}
                />
              </div>
            </div>
            <div className="flex justify-end gap-2 mt-6">
              <button
                onClick={() => setLinkModal(null)}
                className="px-4 py-2 text-sm rounded-lg border border-ac-border text-ac-ink hover:bg-ac-surface-2"
              >
                {t("btn.cancel")}
              </button>
              <button
                onClick={handleLink}
                disabled={!linkModal.selectedTaskId && !linkModal.selectedProjectId && !linkModal.selectedGoalId}
                className="ac-btn px-4 py-2 text-sm disabled:opacity-50"
              >
                {t("btn.save")}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}