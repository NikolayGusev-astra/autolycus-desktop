// src/components/shared/CommandPalette.tsx
// Cmd/Ctrl+K command palette — the missing "power user" layer.
// Supports: view navigation, new task, new chat, theme toggle, search.

import { useState, useEffect, useRef, useMemo, useCallback } from "react";
import {
  Search,
  LayoutDashboard,
  MessageSquare,
  Briefcase,
  Settings,
  SquarePlus,
  Sun,
  Moon,
  CornerDownLeft,
} from "lucide-react";
import { useUIStore } from "../../stores/uiStore";
import { useTranslation } from "../../hooks/useTranslation";

export interface PaletteCommand {
  id: string;
  label: string;
  icon: typeof Search;
  action: () => void;
  group: string;
  keywords?: string;
}

interface CommandPaletteProps {
  open: boolean;
  onClose: () => void;
  onNavigate: (view: string) => void;
  onNewTask: () => void;
  onToggleTheme: () => void;
}

export function CommandPalette({
  open,
  onClose,
  onNavigate,
  onNewTask,
  onToggleTheme,
}: CommandPaletteProps) {
  const { t } = useTranslation();
  const [query, setQuery] = useState("");
  const [selectedIndex, setSelectedIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const isDark = useUIStore((s) => s.darkMode);

  const commands = useMemo<PaletteCommand[]>(
    () => [
      {
        id: "nav-dashboard",
        label: t("feed.title"),
        icon: LayoutDashboard,
        action: () => { onNavigate("dashboard"); onClose(); },
        group: "Navigation",
      },
      {
        id: "nav-chat",
        label: t("nav.assistant"),
        icon: MessageSquare,
        action: () => { onNavigate("chat"); onClose(); },
        group: "Navigation",
      },
      {
        id: "nav-work",
        label: t("nav.work"),
        icon: Briefcase,
        action: () => { onNavigate("work"); onClose(); },
        group: "Navigation",
      },
      {
        id: "nav-settings",
        label: t("nav.settings"),
        icon: Settings,
        action: () => { onNavigate("settings"); onClose(); },
        group: "Navigation",
      },
      {
        id: "action-new-task",
        label: t("nav.tasks") + " (+)",
        icon: SquarePlus,
        action: () => { onNewTask(); onClose(); },
        group: "Actions",
      },
      {
        id: "action-toggle-theme",
        label: isDark ? t("settings.appearance") + ": Light" : t("settings.appearance") + ": Dark",
        icon: isDark ? Sun : Moon,
        action: () => { onToggleTheme(); onClose(); },
        group: "Actions",
      },
    ],
    [t, onNavigate, onClose, onNewTask, onToggleTheme, isDark]
  );

  // Filter commands by query.
  const filtered = useMemo(() => {
    if (!query.trim()) return commands;
    const q = query.toLowerCase();
    return commands.filter(
      (c) =>
        c.label.toLowerCase().includes(q) ||
        c.group.toLowerCase().includes(q) ||
        (c.keywords?.toLowerCase().includes(q) ?? false)
    );
  }, [commands, query]);

  // Reset on open.
  useEffect(() => {
    if (open) {
      setQuery("");
      setSelectedIndex(0);
      setTimeout(() => inputRef.current?.focus(), 50);
    }
  }, [open]);

  // Keep selection in range.
  useEffect(() => {
    if (selectedIndex >= filtered.length) setSelectedIndex(0);
  }, [filtered, selectedIndex]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === "ArrowDown") {
        e.preventDefault();
        setSelectedIndex((i) => (i + 1) % filtered.length);
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        setSelectedIndex((i) => (i - 1 + filtered.length) % filtered.length);
      } else if (e.key === "Enter") {
        e.preventDefault();
        filtered[selectedIndex]?.action();
      } else if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      }
    },
    [filtered, selectedIndex, onClose]
  );

  if (!open) return null;

  // Group filtered commands.
  const groups = filtered.reduce<Record<string, PaletteCommand[]>>((acc, cmd) => {
    (acc[cmd.group] = acc[cmd.group] || []).push(cmd);
    return acc;
  }, {});

  let flatIndex = 0;

  return (
    <div
      className="fixed inset-0 z-[90] flex items-start justify-center pt-[15vh] bg-ac-bg/50 backdrop-blur-sm"
      onClick={onClose}
      role="dialog"
      aria-modal="true"
      aria-label="Command palette"
    >
      <div
        className="w-full max-w-lg rounded-xl border border-ac-border bg-ac-surface overflow-hidden"
        style={{ boxShadow: "var(--shadow-xl)" }}
        onClick={(e) => e.stopPropagation()}
      >
        {/* Search input */}
        <div className="flex items-center gap-2 px-4 py-3 border-b border-ac-border">
          <Search className="w-4 h-4 text-ac-muted" />
          <input
            ref={inputRef}
            type="text"
            value={query}
            onChange={(e) => { setQuery(e.target.value); setSelectedIndex(0); }}
            onKeyDown={handleKeyDown}
            placeholder="Type a command or search…"
            className="flex-1 bg-transparent text-sm text-ac-ink placeholder:text-ac-faint focus:outline-none"
            aria-label="Command search"
            role="combobox"
            aria-expanded="true"
            aria-controls="palette-list"
          />
          <kbd className="text-[10px] text-ac-faint border border-ac-border rounded px-1.5 py-0.5">
            Esc
          </kbd>
        </div>

        {/* Results */}
        <div className="max-h-80 overflow-y-auto py-2" id="palette-list" role="listbox">
          {filtered.length === 0 && (
            <p className="text-sm text-ac-muted text-center py-6">No results</p>
          )}
          {Object.entries(groups).map(([group, cmds]) => (
            <div key={group}>
              <p className="text-[10px] font-semibold uppercase tracking-wide text-ac-faint px-4 py-1">
                {group}
              </p>
              {cmds.map((cmd) => {
                const idx = flatIndex++;
                const active = idx === selectedIndex;
                return (
                  <button
                    key={cmd.id}
                    onClick={cmd.action}
                    onMouseEnter={() => setSelectedIndex(idx)}
                    className={`w-full flex items-center gap-3 px-4 py-2 text-left text-sm transition-colors ${
                      active ? "bg-ac-brand-soft text-ac-brand" : "text-ac-ink hover:bg-ac-surface-2"
                    }`}
                    role="option"
                    aria-selected={active}
                  >
                    <cmd.icon className="w-4 h-4 shrink-0" />
                    <span className="flex-1">{cmd.label}</span>
                    {active && <CornerDownLeft className="w-3 h-3 text-ac-faint" />}
                  </button>
                );
              })}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
