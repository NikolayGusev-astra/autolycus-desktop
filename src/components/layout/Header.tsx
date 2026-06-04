import { Settings, PanelLeft, Bot } from "lucide-react";
import { useUIStore } from "../../stores/uiStore";
import { useGatewayStore } from "../../stores/gatewayStore";

export function Header() {
  const { toggleSidebar, setSettingsOpen } = useUIStore();
  const { agentStatus, connected } = useGatewayStore();

  const statusColor = {
    idle: "bg-green-500",
    thinking: "bg-yellow-500 animate-pulse",
    streaming: "bg-blue-500 animate-pulse",
    tool_calling: "bg-purple-500 animate-pulse",
    error: "bg-red-500",
  }[agentStatus];

  return (
    <header className="flex items-center justify-between border-b border-gray-200 dark:border-gray-700 px-4 py-2 bg-white dark:bg-gray-900">
      <div className="flex items-center gap-3">
        <button
          onClick={toggleSidebar}
          className="p-1.5 rounded hover:bg-gray-100 dark:hover:bg-gray-800 text-gray-500"
        >
          <PanelLeft className="w-5 h-5" />
        </button>
        <div className="flex items-center gap-2">
          <Bot className="w-5 h-5 text-blue-500" />
          <span className="font-semibold text-gray-900 dark:text-gray-100">
            Autolycus
          </span>
        </div>
      </div>
      <div className="flex items-center gap-3">
        <div className="flex items-center gap-1.5">
          <div className={`w-2 h-2 rounded-full ${connected ? statusColor : "bg-gray-400"}`} />
          <span className="text-xs text-gray-500 capitalize">{connected ? agentStatus : "disconnected"}</span>
        </div>
        <button
          onClick={() => setSettingsOpen(true)}
          className="p-1.5 rounded hover:bg-gray-100 dark:hover:bg-gray-800 text-gray-500"
        >
          <Settings className="w-5 h-5" />
        </button>
      </div>
    </header>
  );
}
