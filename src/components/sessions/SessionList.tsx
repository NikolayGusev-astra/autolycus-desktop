// src/components/sessions/SessionList.tsx
// v0.4.0: Session list from SQLite via Rust backend

import { useState, useEffect, useCallback } from "react";
import { Search, Trash2, MessageSquare, Loader } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";

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

export function SessionList() {
  const [sessions, setSessions] = useState<SessionSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [searchQuery, setSearchQuery] = useState("");
  const [selectedId, setSelectedId] = useState<string | null>(null);

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
            <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-ac-stone" />
            <input
              type="text"
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && handleSearch()}
              placeholder="Поиск сессий..."
              className="ac-input w-full pl-9 pr-3 py-2 text-sm"
            />
          </div>
          <button
            onClick={handleSearch}
            className="ac-btn px-4 py-2 text-sm"
          >
            Найти
          </button>
        </div>
      </div>

      {/* Session list */}
      <div className="flex-1 overflow-y-auto">
        {loading ? (
          <div className="flex items-center justify-center h-32">
            <Loader className="w-5 h-5 text-ac-amber animate-spin" />
          </div>
        ) : filteredSessions.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-32 text-ac-stone text-sm">
            <MessageSquare className="w-8 h-8 mb-2 opacity-30" />
            <p>Нет сессий</p>
          </div>
        ) : (
          <div className="divide-y divide-ac-border/30">
            {filteredSessions.map((session) => (
              <div
                key={session.id}
                className={`px-4 py-3 cursor-pointer transition-colors ${
                  selectedId === session.id
                    ? "bg-ac-amber/5 border-l-2 border-ac-amber"
                    : "hover:bg-ac-pitch/50 border-l-2 border-transparent"
                }`}
                onClick={() => setSelectedId(session.id)}
              >
                <div className="flex items-start justify-between gap-2">
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2 mb-1">
                      <span className="text-sm font-medium text-ac-ivory truncate">
                        {session.title || session.id.slice(0, 16)}
                      </span>
                      {session.model && (
                        <span className="text-[10px] px-1.5 py-0.5 bg-ac-amber/10 text-ac-amber rounded">
                          {session.model}
                        </span>
                      )}
                    </div>
                    <p className="text-xs text-ac-stone truncate">
                      {session.preview || "Пустой чат"}
                    </p>
                    <div className="flex items-center gap-3 mt-1 text-[10px] text-ac-stone/50">
                      <span>{formatDate(session.started_at)}</span>
                      <span>{session.message_count} сообщений</span>
                      <span className="truncate max-w-[120px]">{session.source}</span>
                    </div>
                  </div>
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      handleDelete(session.id);
                    }}
                    className="text-ac-stone/30 hover:text-ac-red transition-colors p-1"
                    title="Удалить сессию"
                  >
                    <Trash2 className="w-3.5 h-3.5" />
                  </button>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Footer */}
      <div className="px-4 py-2 border-t border-ac-border text-[10px] text-ac-stone/50 flex justify-between">
        <span>{sessions.length} сессий</span>
        <span>SQLite state.db</span>
      </div>
    </div>
  );
}
