import { useEffect, useState } from "react";
import { Sidebar } from "./components/layout/Sidebar";
import { Header } from "./components/layout/Header";
import { ChatView } from "./components/chat/ChatView";
import { SessionList } from "./components/sessions/SessionList";
import { SettingsPanel } from "./components/settings/SettingsPanel";
import { StatusBar } from "./components/layout/StatusBar";
import { useGatewayStore } from "./stores/gatewayStore";
import { useUIStore } from "./stores/uiStore";

export function App() {
  const [activeTab, setActiveTab] = useState("chat");
  const [settingsOpen, setSettingsOpen] = useState(false);
  const { sidebarOpen } = useUIStore();
  const { connected, error } = useGatewayStore();

  // Parse backend URL from query params or use default
  useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    const port = params.get("port");
    const backendUrl = params.get("backend");

    if (backendUrl) {
      // Remote mode: connect to specified backend
      console.log(`Connecting to backend: ${backendUrl}`);
    } else if (port) {
      // Local mode: backend spawned by Tauri
      console.log(`Connecting to localhost:${port}`);
    }
  }, []);

  return (
    <div className="flex h-full">
      {/* Sidebar */}
      {sidebarOpen && (
        <Sidebar activeTab={activeTab} onTabChange={setActiveTab} />
      )}

      {/* Main content */}
      <div className="flex-1 flex flex-col overflow-hidden">
        <Header />

        {/* Content area */}
        <div className="flex-1 overflow-hidden">
          {activeTab === "chat" && <ChatView />}
          {activeTab === "sessions" && <SessionList />}
        </div>

        <StatusBar />
      </div>

      {/* Settings modal */}
      {settingsOpen && <SettingsPanel onClose={() => setSettingsOpen(false)} />}

      {/* Connection status overlay */}
      {!connected && !error && (
        <div className="fixed inset-0 bg-ac-pitch/80 backdrop-blur-sm flex items-center justify-center z-50">
          <div className="text-center">
            <div className="ac-display mb-2">Автолик</div>
            <p className="sub text-sm">Подключение к агенту...</p>
          </div>
        </div>
      )}

      {/* Error overlay */}
      {error && (
        <div className="fixed inset-0 bg-ac-pitch/80 backdrop-blur-sm flex items-center justify-center z-50">
          <div className="ac-modal">
            <h3 className="text-base font-semibold text-ac-ivory mb-2">Ошибка подключения</h3>
            <p className="text-sm text-ac-stone mb-4">{error}</p>
            <p className="text-xs text-ac-stone">
              Убедитесь что backend запущен. Запуск:{" "}
              <code className="bg-ac-surface px-1.5 py-0.5 text-ac-amber">autolycus-desktop --mode=websocket</code>
            </p>
          </div>
        </div>
      )}
    </div>
  );
}
