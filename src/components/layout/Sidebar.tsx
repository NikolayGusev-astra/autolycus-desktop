import { MessageSquare, Settings, Plus, Bot, Layers } from "lucide-react";
import { useUIStore } from "../../stores/uiStore";

interface SidebarProps {
  activeTab: string;
  onTabChange: (tab: string) => void;
}

export function Sidebar({ activeTab, onTabChange }: SidebarProps) {
  const { toggleSidebar } = useUIStore();

  const tabs = [
    { id: "chat", icon: MessageSquare, label: "Чат" },
    { id: "kanban", icon: Layers, label: "Канбан" },
    { id: "sessions", icon: Plus, label: "Сессии" },
    { id: "settings", icon: Settings, label: "Настройки" },
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
        title="Свернуть"
      >
        <svg className="w-4 h-4" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5">
          <path d="M10 12L6 8L10 4" />
        </svg>
      </button>
    </div>
  );
}
