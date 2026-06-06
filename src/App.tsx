import { useState, useCallback } from "react";
import { Sidebar } from "./components/layout/Sidebar";
import { Header } from "./components/layout/Header";
import { ChatView } from "./components/chat/ChatView";
import { SessionList } from "./components/sessions/SessionList";
import { SettingsPanel } from "./components/settings/SettingsPanel";
import { StatusBar } from "./components/layout/StatusBar";
import { ConnectScreen } from "./components/ConnectScreen";
import { useGatewayStore } from "./stores/gatewayStore";
import { useUIStore } from "./stores/uiStore";
import { AgentClient } from "./lib/agent-client";
import type { AgentConfig } from "./lib/types";

export function App() {
  const [activeTab, setActiveTab] = useState("chat");
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [connecting, setConnecting] = useState(false);
  const [starting, setStarting] = useState(false);
  const { sidebarOpen } = useUIStore();
  const { connected, error } = useGatewayStore();

  const handleConnectLocal = useCallback(async (pythonPath: string) => {
    setConnecting(true);
    try {
      const client = new AgentClient();
      const config: AgentConfig = {
        mode: "local",
        python_path: pythonPath,
      };
      await client.connect(config);
      useGatewayStore.setState({ client, connected: true, error: null });

      client.onEvent((event) => {
        useGatewayStore.getState().events.push(event);
      });
    } catch (err: any) {
      useGatewayStore.setState({ error: err.message });
    } finally {
      setConnecting(false);
    }
  }, []);

  const handleStartLocal = useCallback(async (pythonPath: string) => {
    setStarting(true);
    try {
      await handleConnectLocal(pythonPath);
    } finally {
      setStarting(false);
    }
  }, [handleConnectLocal]);

  if (!connected) {
    return (
      <ConnectScreen
        onConnectLocal={handleConnectLocal}
        onStartLocal={handleStartLocal}
        connecting={connecting}
        starting={starting}
        error={error}
      />
    );
  }

  return (
    <div className="flex h-full">
      {sidebarOpen && (
        <Sidebar activeTab={activeTab} onTabChange={setActiveTab} />
      )}

      <div className="flex-1 flex flex-col overflow-hidden">
        <Header />
        <div className="flex-1 overflow-hidden">
          {activeTab === "chat" && <ChatView />}
          {activeTab === "sessions" && <SessionList />}
        </div>
        <StatusBar />
      </div>

      {settingsOpen && <SettingsPanel onClose={() => setSettingsOpen(false)} />}
    </div>
  );
}
