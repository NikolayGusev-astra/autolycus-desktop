// src/components/layout/Sidebar.tsx
// shturman.ai-style navigation: icon + label, collapsible, 8 work sections.
// Active item: pale-red tint (bg-ac-brand-soft) + brand text. A "Самодиагностика"
// (self-diagnosis) button sits in the footer, like the reference.

import {
  LayoutDashboard,
  MessageSquare,
  SquareCheckBig,
  Target,
  FolderOpen,
  ChartColumn,
  FileText,
  Settings as SettingsIcon,
  Zap,
  PanelLeftClose,
  Compass,
} from "lucide-react";
import { useUIStore } from "../../stores/uiStore";
import { useTranslation } from "../../hooks/useTranslation";

export type ViewId =
  | "dashboard"
  | "chat"
  | "tasks"
  | "goals"
  | "projects"
  | "stats"
  | "protocols"
  | "settings";

interface SidebarProps {
  activeView: ViewId;
  onViewChange: (v: ViewId) => void;
  /** Open the self-diagnosis modal (mood/energy check). */
  onSelfDiagnosis?: () => void;
}

export function Sidebar({ activeView, onViewChange, onSelfDiagnosis }: SidebarProps) {
  const { sidebarOpen, toggleSidebar } = useUIStore();
  const { t } = useTranslation();

  const items: { id: ViewId; icon: typeof LayoutDashboard; label: string }[] = [
    { id: "dashboard", icon: LayoutDashboard, label: t("nav.dashboard") },
    { id: "chat", icon: MessageSquare, label: t("nav.chat") },
    { id: "tasks", icon: SquareCheckBig, label: t("nav.tasks") },
    { id: "goals", icon: Target, label: t("nav.goals") },
    { id: "projects", icon: FolderOpen, label: t("nav.projects") },
    { id: "stats", icon: ChartColumn, label: t("nav.stats") },
    { id: "protocols", icon: FileText, label: t("nav.protocols") },
    { id: "settings", icon: SettingsIcon, label: t("nav.settings") },
  ];

  // Collapsed (icons-only) form factor.
  if (!sidebarOpen) {
    return (
      <button
        onClick={toggleSidebar}
        className="w-12 shrink-0 border-r border-ac-border bg-ac-bg flex flex-col items-center justify-start py-3 text-ac-muted hover:text-ac-brand"
        title={t("sidebar_expand")}
        aria-label={t("sidebar_expand")}
      >
        <Compass className="w-5 h-5 text-ac-brand mb-4 mt-1" />
        {items.map((it) => (
          <it.icon key={it.id} className="w-5 h-5 my-2.5" />
        ))}
      </button>
    );
  }

  return (
    <aside className="relative flex flex-col border-r border-ac-border bg-ac-surface h-screen overflow-hidden transition-all duration-300 ease-in-out"
           style={{ width: 240 }}>
      {/* Logo row */}
      <div className="h-14 px-4 flex items-center gap-2 border-b border-ac-border">
        <Compass className="w-6 h-6 text-ac-brand" />
        <span className="font-bold text-base text-ac-brand">Штурман</span>
      </div>

      {/* Nav */}
      <nav className="flex-1 py-3 px-2 space-y-1 overflow-y-auto">
        {items.map((it) => {
          const active = activeView === it.id;
          return (
            <button
              key={it.id}
              onClick={() => onViewChange(it.id)}
              className={`flex items-center gap-3 w-full rounded-md px-3 py-2 text-sm font-medium transition-colors ${
                active
                  ? "bg-ac-brand-soft text-ac-brand"
                  : "text-ac-muted hover:bg-ac-surface-2 hover:text-ac-ink"
              }`}
            >
              <it.icon className="w-[18px] h-[18px]" style={{ strokeWidth: 2 }} />
              {it.label}
            </button>
          );
        })}
      </nav>

      {/* Self-diagnosis CTA */}
      <div className="p-2 border-t border-ac-border">
        <button
          onClick={onSelfDiagnosis}
          className="flex items-center gap-2 w-full rounded-md px-3 py-2 text-sm border border-ac-border text-ac-muted hover:text-ac-brand hover:border-ac-brand-border transition-colors"
        >
          <Zap className="w-4 h-4" />
          {t("nav.selfDiagnosis")}
        </button>
      </div>

      {/* Collapse toggle */}
      <button
        onClick={toggleSidebar}
        className="absolute -right-3 top-16 h-6 w-6 rounded-full bg-ac-surface border border-ac-border flex items-center justify-center text-ac-muted hover:text-ac-brand"
        title={t("sidebar_collapse")}
        aria-label={t("sidebar_collapse")}
      >
        <PanelLeftClose className="w-3.5 h-3.5" />
      </button>
    </aside>
  );
}

export default Sidebar;
