import {
  MessageSquare,
  Clock,
  Layers,
  Brain,
  Puzzle,
  Timer,
  Settings,
  Compass,
} from "lucide-react";
import { useUIStore } from "../../stores/uiStore";
import { useTranslation } from "../../hooks/useTranslation";

interface SidebarProps {
  activeTab: string;
  onTabChange: (tab: string) => void;
  /** Opens the unified settings panel (ADR-006). */
  onOpenSettings: () => void;
}

export function Sidebar({ activeTab, onTabChange, onOpenSettings }: SidebarProps) {
  const { toggleSidebar } = useUIStore();
  const { t } = useTranslation();

  // ADR-006: the sidebar holds only genuine WORK areas. Everything that is
  // configuration or maintenance (models, providers, gateway, tools, diagnose,
  // versions) now lives under the unified Settings panel reached via the gear.
  const tabs = [
    { id: "chat", icon: MessageSquare, label: t("nav.chat") },
    { id: "steersman", icon: Compass, label: "Штурман" },
    { id: "sessions", icon: Clock, label: t("nav.sessions") },
    { id: "kanban", icon: Layers, label: t("nav.kanban") },
    { id: "memory", icon: Brain, label: t("nav.memory") },
    { id: "skills", icon: Puzzle, label: t("nav.skills") },
    { id: "schedules", icon: Timer, label: t("nav.schedules") },
  ];

  return (
    <div className="ac-sidebar">
      {/* Logo */}
      <div className="w-7 h-7 flex items-center justify-center mb-3">
        <Compass className="w-5 h-5 text-ac-brand" />
      </div>

      {/* Nav buttons */}
      {tabs.map((tab) => (
        <button
          key={tab.id}
          onClick={() => onTabChange(tab.id)}
          className={`ac-sidebar-btn ${activeTab === tab.id ? "active" : ""}`}
          title={tab.label}
        >
          <tab.icon className="w-4 h-4" />
        </button>
      ))}

      <div className="flex-1" />

      {/* Unified settings (gear) */}
      <button
        onClick={onOpenSettings}
        className="ac-sidebar-btn"
        title={t("nav.settings")}
      >
        <Settings className="w-4 h-4" />
      </button>

      {/* Collapse */}
      <button
        onClick={toggleSidebar}
        className="ac-sidebar-btn"
        title={t("sidebar_collapse")}
      >
        <svg className="w-4 h-4" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5">
          <path d="M10 12L6 8L10 4" />
        </svg>
      </button>
    </div>
  );
}