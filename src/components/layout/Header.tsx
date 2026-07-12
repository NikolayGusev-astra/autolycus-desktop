// src/components/layout/Header.tsx
// shturman.ai-style top header: frosted glass, working search box, current date,
// and a connection-status badge. Sticky, h-14.

import { useState, useEffect, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useGatewayStore } from "../../stores/gatewayStore";
import { useUIStore } from "../../stores/uiStore";
import { Search, Wifi, WifiOff } from "lucide-react";
import { useTranslation } from "../../hooks/useTranslation";

interface SearchResult {
  session_id: string;
  title: string | null;
  started_at: number;
  source: string;
  snippet: string;
}

export function Header() {
  const { connected, pipelineStatus } = useGatewayStore();
  const { showTokenCounter } = useUIStore();
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<SearchResult[]>([]);
  const [showResults, setShowResults] = useState(false);
  const [loading, setLoading] = useState(false);
  const searchRef = useRef<HTMLDivElement>(null);
  const debounceRef = useRef<ReturnType<typeof setTimeout>>(undefined);
  const { t } = useTranslation();

  const today = new Date().toLocaleDateString("ru-RU", {
    weekday: "long",
    day: "numeric",
    month: "long",
    year: "numeric",
  });

  // Debounced search — query sessions after 300ms of inactivity.
  useEffect(() => {
    if (debounceRef.current) clearTimeout(debounceRef.current);
    if (!query.trim() || query.trim().length < 2) {
      setResults([]);
      setShowResults(false);
      return;
    }
    debounceRef.current = setTimeout(async () => {
      setLoading(true);
      try {
        const found = await invoke<SearchResult[]>("search_sessions_cmd", {
          query: query.trim(),
          limit: 8,
          profile: null,
        });
        setResults(found);
        setShowResults(true);
      } catch (e) {
        console.error("search failed:", e);
      } finally {
        setLoading(false);
      }
    }, 300);
    return () => {
      if (debounceRef.current) clearTimeout(debounceRef.current);
    };
  }, [query]);

  // Close dropdown on outside click.
  useEffect(() => {
    const handleClick = (e: MouseEvent) => {
      if (searchRef.current && !searchRef.current.contains(e.target as Node)) {
        setShowResults(false);
      }
    };
    document.addEventListener("mousedown", handleClick);
    return () => document.removeEventListener("mousedown", handleClick);
  }, []);

  const openSession = useCallback((sessionId: string) => {
    // Load the session into the chat view and navigate there.
    invoke<Array<{ id: number; role: string; content: string; timestamp: number }>>(
      "get_session_messages_cmd",
      { sessionId, profile: null }
    )
      .then((msgs) => {
        const mapped = msgs
          .filter((m) => m.role === "user" || m.role === "assistant")
          .map((m) => ({
            id: `hist-${m.id}`,
            role: m.role as "user" | "assistant",
            content: m.content,
            timestamp: m.timestamp,
          }));
        useGatewayStore.setState({ messages: mapped, currentSessionId: sessionId });
      })
      .catch((e) => console.error("session load:", e));
    setShowResults(false);
    setQuery("");
  }, []);

  return (
    <header className="sticky top-0 z-30 flex items-center gap-4 px-6 h-14 border-b border-ac-border bg-ac-bg/80 backdrop-blur-md">
      {/* Search with live results dropdown */}
      <div className="relative flex-1 max-w-md" ref={searchRef}>
        <Search className="w-4 h-4 absolute left-3 top-1/2 -translate-y-1/2 text-ac-muted" />
        <input
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onFocus={() => results.length > 0 && setShowResults(true)}
          placeholder={t("header.search")}
          className="w-full h-9 pl-9 pr-3 rounded-md bg-ac-surface border border-ac-border text-sm text-ac-ink placeholder:text-ac-faint focus:outline-none focus:border-ac-brand"
        />
        {loading && (
          <span className="absolute right-3 top-1/2 -translate-y-1/2 text-[10px] text-ac-muted">
            ...
          </span>
        )}
        {showResults && results.length > 0 && (
          <div className="absolute top-full left-0 mt-1 w-full rounded-lg border border-ac-border bg-ac-surface shadow-lg overflow-hidden z-40">
            {results.map((r) => (
              <button
                key={r.session_id}
                onClick={() => openSession(r.session_id)}
                className="w-full flex items-start gap-2 px-3 py-2 text-left hover:bg-ac-surface-2 transition-colors border-b border-ac-border last:border-0"
              >
                <Search className="w-3.5 h-3.5 mt-0.5 text-ac-muted shrink-0" />
                <div className="flex-1 min-w-0">
                  <p className="text-xs font-medium text-ac-ink truncate">
                    {r.title || "Без названия"}
                  </p>
                  <p className="text-[10px] text-ac-muted truncate">{r.snippet}</p>
                </div>
                <span className="text-[10px] text-ac-faint shrink-0">{r.source}</span>
              </button>
            ))}
          </div>
        )}
        {showResults && !loading && results.length === 0 && query.trim().length >= 2 && (
          <div className="absolute top-full left-0 mt-1 w-full rounded-lg border border-ac-border bg-ac-surface shadow-lg px-3 py-2 text-xs text-ac-muted z-40">
            Ничего не найдено
          </div>
        )}
      </div>

      <div className="ml-auto flex items-center gap-4">
        {/* Current date */}
        <span className="text-sm text-ac-muted hidden md:inline capitalize">{today}</span>

        {/* Token counter (optional) */}
        {showTokenCounter && pipelineStatus.tokensUsed !== undefined && (
          <span className="text-[11px] text-ac-muted font-mono">
            {pipelineStatus.tokensUsed >= 1000
              ? `${(pipelineStatus.tokensUsed / 1000).toFixed(1)}K`
              : pipelineStatus.tokensUsed}
            {pipelineStatus.tokensLimit ? `/${(pipelineStatus.tokensLimit / 1000).toFixed(0)}K` : ""}
            {pipelineStatus.costUsd !== undefined ? ` · $${pipelineStatus.costUsd.toFixed(3)}` : ""}
          </span>
        )}

        {/* Model badge (optional) */}
        {showTokenCounter && pipelineStatus.model && (
          <span className="text-[10px] text-ac-faint font-mono hidden lg:inline truncate max-w-32">
            {pipelineStatus.model}
          </span>
        )}

        {/* Connection badge */}
        <div
          className={`flex items-center gap-1.5 text-xs px-2.5 py-1 rounded-md ${
            connected ? "bg-green-500/10 text-green-600 dark:text-green-400" : "bg-red-500/10 text-red-500"
          }`}
        >
          {connected ? (
            <>
              <Wifi className="w-3.5 h-3.5" />
              {t("status.connected")}
            </>
          ) : (
            <>
              <WifiOff className="w-3.5 h-3.5" />
              {t("status.disconnected")}
            </>
          )}
        </div>
      </div>
    </header>
  );
}

export default Header;
