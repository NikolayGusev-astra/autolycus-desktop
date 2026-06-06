import { useEffect, useState, useCallback } from "react";
import { Sidebar } from "./components/layout/Sidebar";
import { Header } from "./components/layout/Header";
import { ChatView } from "./components/chat/ChatView";
import { SessionList } from "./components/sessions/SessionList";
import { SettingsPanel } from "./components/settings/SettingsPanel";
import { StatusBar } from "./components/layout/StatusBar";
import { ConnectScreen } from "./components/ConnectScreen";
import { useGatewayStore } from "./stores/gatewayStore";
import { useUIStore } from "./stores/uiStore";
import { GatewayClient } from "./lib/gateway-client";

export function App() {
  const [activeTab, setActiveTab] = useState("chat");
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [connecting, setConnecting] = useState(false);
  const [starting, setStarting] = useState(false);
  const { sidebarOpen } = useUIStore();
  const { connected, error } = useGatewayStore();

  // Try to connect from URL params on mount
  useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    const backendUrl = params.get("backend");
    const port = params.get("port");

    if (backendUrl) {
      handleConnect(backendUrl);
    } else if (port) {
      handleConnect(`ws://127.0.0.1:${port}`);
    }
  }, []);

  const handleConnect = useCallback(async (url: string) => {
    setConnecting(true);
    try {
      const c = new GatewayClient(url);
      await c.connect();
      useGatewayStore.setState({ client: c, connected: true, error: null });

      c.onEvent((event) => {
        const events = useGatewayStore.getState().events;
        useGatewayStore.setState({ events: [...events, event] });
        if (event.type === "status") {
          useGatewayStore.setState({ agentStatus: event.status as any });
        }
      });
    } catch (err: any) {
      useGatewayStore.setState({ connected: false, error: err.message });
    } finally {
      setConnecting(false);
    }
  }, []);

  const handleStartLocal = useCallback(async (pythonPath: string) => {
    setStarting(true);
    try {
      // In Tauri mode, invoke Rust command to start backend
      // For now (browser dev mode), show message
      if (typeof window !== "undefined" && (window as any).__TAURI__) {
        const { invoke } = await import("@tauri-apps/api/core");
        const port = await invoke<number>("start_backend", { pythonPath });
        await handleConnect(`ws://127.0.0.1:${port}`);
      } else {
        // Dev mode: tell user to start backend manually
        useGatewayStore.setState({
          error: "Запустите backend: python tui_gateway/ws_server.py --port 8443",
        });
      }
    } catch (err: any) {
      useGatewayStore.setState({ error: err.message });
    } finally {
      setStarting(false);
    }
  }, [handleConnect]);

  // Show connect screen if not connected
  if (!connected) {
    return (
      <ConnectScreen
        onConnect={handleConnect}
        onStartLocal={handleStartLocal}
        connecting={connecting}
        starting={starting}
        error={error}
      />
    );
  }

  // Main app
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
