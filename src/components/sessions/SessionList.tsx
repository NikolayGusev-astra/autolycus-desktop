import { useMemo } from "react";
import { MessageSquare, Plus } from "lucide-react";
import { useGatewayStore } from "../../stores/gatewayStore";
import type { Session } from "../../lib/types";

interface SessionListProps {
  sessions?: Session[];
  currentSessionId?: string | null;
  onSelect?: (id: string) => void;
  onNew?: () => void;
}

function groupByDate(sessions: Session[]): Map<string, Session[]> {
  const groups = new Map<string, Session[]>();
  const now = Date.now();
  const day = 86400000;

  for (const s of sessions) {
    let label: string;
    if (now - s.updatedAt < day) label = "Today";
    else if (now - s.updatedAt < 2 * day) label = "Yesterday";
    else if (now - s.updatedAt < 7 * day) label = "This Week";
    else if (now - s.updatedAt < 30 * day) label = "This Month";
    else label = "Older";

    if (!groups.has(label)) groups.set(label, []);
    groups.get(label)!.push(s);
  }
  return groups;
}

export function SessionList({
  sessions: propSessions,
  currentSessionId: propCurrentId,
  onSelect,
  onNew,
}: SessionListProps) {
  const storeSessions = useGatewayStore((s) => s.sessions);
  const storeCurrentId = useGatewayStore((s) => s.currentSessionId);

  const sessions = propSessions ?? storeSessions;
  const currentId = propCurrentId ?? storeCurrentId;

  const grouped = useMemo(() => groupByDate(sessions), [sessions]);

  return (
    <div className="flex flex-col h-full">
      <div className="p-2">
        <button
          onClick={onNew}
          className="flex items-center gap-2 w-full px-3 py-2 text-sm font-medium text-blue-600 hover:bg-blue-50 dark:hover:bg-blue-900/20 rounded-lg transition-colors"
        >
          <Plus className="w-4 h-4" />
          New Chat
        </button>
      </div>
      <div className="flex-1 overflow-y-auto px-2">
        {sessions.length === 0 && (
          <div className="px-3 py-8 text-center text-gray-400 text-sm">
            No sessions yet. Start a new chat!
          </div>
        )}
        {Array.from(grouped.entries()).map(([label, items]) => (
          <div key={label} className="mb-4">
            <div className="px-2 py-1 text-xs font-semibold text-gray-400 uppercase tracking-wider">
              {label}
            </div>
            {items.map((session) => (
              <button
                key={session.id}
                onClick={() => onSelect?.(session.id)}
                className={`flex items-center gap-2 w-full px-3 py-2 text-sm rounded-lg transition-colors text-left ${
                  session.id === currentId
                    ? "bg-blue-100 dark:bg-blue-900/30 text-blue-700 dark:text-blue-300"
                    : "hover:bg-gray-100 dark:hover:bg-gray-800 text-gray-700 dark:text-gray-300"
                }`}
              >
                <MessageSquare className="w-4 h-4 flex-shrink-0" />
                <span className="truncate">{session.title || "New Chat"}</span>
              </button>
            ))}
          </div>
        ))}
      </div>
    </div>
  );
}
