import {
  MessageSquare,
  Clock,
  Layers,
  Cpu,
  Brain,
  Puzzle,
  KeyRound,
  Timer,
  Settings,
  Bot,
  Stethoscope,
} from "lucide-react";
import { useUIStore } from "../../stores/uiStore";
import { useTranslation } from "../../hooks/useTranslation";

interface SidebarProps {
  activeTab: string;
  onTabChange: (tab: string) => void;
}

export function Sidebar({ activeTab, onTabChange }: SidebarProps) {
  const { toggleSidebar } = useUIStore();
  const { t } = useTranslation();

  const tabs = [
    { id: "chat", icon: MessageSquare, label: t("nav.chat") },
    { id: "sessions", icon: Clock, label: t("nav.sessions") },
    { id: "kanban", icon: Layers, label: t("nav.kanban") },
    { id: "models", icon: Cpu, label: t("nav.models") },
    { id: "memory", icon: Brain, label: t("nav.memory") },
    { id: "skills", icon: Puzzle, label: t("nav.skills") },
    { id: "providers", icon: KeyRound, label: t("nav.providers") },
    { id: "diagnose", icon: Stethoscope, label: t("nav.diagnose") },
    { id: "schedules", icon: Timer, label: t("nav.schedules") },
    { id: "settings", icon: Settings, label: t("nav.settings") },
  ];

  return (
    <div className="ac-sidebar">
      {/* Logo */}
      <div className="w-7 h-7 flex items-center justify-center mb-3">
        <Bot className="w-5 h-5 text-ac-amber" />
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