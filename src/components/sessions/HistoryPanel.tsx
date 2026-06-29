// src/components/sessions/HistoryPanel.tsx
// Right-hand conversation history column. Lists past sessions (from the
// Hermes state.db) and, on selection, loads a session's messages into the chat
// store so the main chat shows that conversation. Collapsible via the prop.

import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Search, Trash2, MessageSquare, Loader, PanelRightClose } from "lucide-react";
import { useGatewayStore } from "../../stores/gatewayStore";
import { useTranslation } from "../../hooks/useTranslation";
import type { Message } from "../../lib/types";

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

interface SessionMessage {
  id: number;
  role: string;
  content: string;
  timestamp: number;
}

interface HistoryPanelProps {
  /** Collapse handler (the parent toggles panel visibility). */
  onClose: () => void;
}

export function HistoryPanel({ onClose }: HistoryPanelProps) {
  const [sessions, setSessions] = useState<SessionSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [query, setQuery] = useState("");
  const { setCurrentSession } = useGatewayStore();
  const currentSessionId = useGatewayStore((s) => s.currentSessionId);
  const { t } = useTranslation();

  const loadSessions = useCallback(async () => {
    try {
      setLoading(true);
      const result = await invoke<SessionSummary[]>("list_sessions_cmd", {
        profile: null,
        limit: 60,
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

  const openSession = async (sessionId: string) => {
    try {
      const msgs = await invoke<SessionMessage[]>("get_session_messages_cmd", {
        sessionId,
        profile: null,
      });
      // Map into the chat store's Message shape and load.
      const mapped: Message[] = msgs
        .filter((m) => m.role === "user" || m.role === "assistant")
        .map((m) => ({
          id: `hist-${m.id}`,
          role: m.role as "user" | "assistant",
          content: m.content,
          timestamp: m.timestamp * 1000,
        }));
      useGatewayStore.setState({ messages: mapped });
      setCurrentSession(sessionId);
    } catch (err) {
      console.error("Failed to load session messages:", err);
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

  const fmt = (ts: number) => {
    if (!ts) return "—";
    return new Date(ts * 1000).toLocaleString("ru-RU", {
      day: "2-digit",
      month: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
    });
  };

  const filtered = query
    ? sessions.filter(
        (s) =>
          (s.title || "").toLowerCase().includes(query.toLowerCase()) ||
          s.preview.toLowerCase().includes(query.toLowerCase())
      )
    : sessions;

  return (
    <aside className="w-64 shrink-0 border-l border-ac-border bg-ac-bg flex flex-col">
      {/* Header */}
      <div className="flex items-center justify-between px-3 py-2 border-b border-ac-border">
        <span className="text-xs font-semibold text-ac-ink uppercase tracking-wide">
          {t("nav.sessions")}
        </span>
        <button
          onClick={onClose}
          className="text-ac-muted hover:text-ac-ink"
          title={t("sidebar_collapse")}
        >
          <PanelRightClose className="w-4 h-4" />
        </button>
      </div>

      {/* Search */}
      <div className="p-2 border-b border-ac-border">
        <div className="relative">
          <Search className="w-3.5 h-3.5 text-ac-faint absolute left-2 top-1/2 -translate-y-1/2" />
          <input
            className="ac-input w-full pl-7 pr-2 py-1.5 text-xs"
            placeholder={t("btn.scan")}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </div>
      </div>

      {/* List */}
      <div className="flex-1 overflow-y-auto">
        {loading ? (
          <div className="flex items-center justify-center py-6 text-ac-faint">
            <Loader className="w-4 h-4 animate-spin" />
          </div>
        ) : filtered.length === 0 ? (
          <div className="text-center text-xs text-ac-faint py-6 px-3">
            {t("nav.sessions")} — пусто
          </div>
        ) : (
          filtered.map((s) => {
            const active = s.id === currentSessionId;
            return (
              <div
                key={s.id}
                className={`group px-3 py-2 border-b border-ac-border/50 cursor-pointer transition-colors ${
                  active ? "bg-ac-brand/10" : "hover:bg-ac-surface"
                }`}
                onClick={() => openSession(s.id)}
              >
                <div className="flex items-start gap-1.5">
                  <MessageSquare className="w-3.5 h-3.5 text-ac-muted mt-0.5 shrink-0" />
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center justify-between gap-1">
                      <span className={`text-xs truncate ${active ? "text-ac-brand font-medium" : "text-ac-ink"}`}>
                        {s.title || s.id.slice(0, 12)}
                      </span>
                      <button
                        onClick={(e) => {
                          e.stopPropagation();
                          handleDelete(s.id);
                        }}
                        className="opacity-0 group-hover:opacity-100 text-ac-faint hover:text-ac-red"
                        title={t("btn.delete")}
                      >
                        <Trash2 className="w-3 h-3" />
                      </button>
                    </div>
                    <p className="text-[11px] text-ac-muted truncate mt-0.5">
                      {s.preview || "—"}
                    </p>
                    <div className="flex items-center gap-2 mt-1 text-[10px] text-ac-faint">
                      <span>{fmt(s.started_at)}</span>
                      <span>· {s.message_count}</span>
                    </div>
                  </div>
                </div>
              </div>
            );
          })
        )}
      </div>
    </aside>
  );
}

export default HistoryPanel;
