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
import { AgentClient } from "./lib/agent-client";
import type { AgentConfig } from "./lib/types";

export function App() {
  const [activeTab, setActiveTab] = useState("chat");
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [connecting, setConnecting] = useState(false);
  const [starting, setStarting] = useState(false);
  const { sidebarOpen } = useUIStore();
  const { connected, error, setClient, setConnected, setError, setPort, setMode, addEvent } =
    useGatewayStore();

  // Try to connect from URL params on mount
  useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    const backendUrl = params.get("backend");
    const port = params.get("port");

    if (backendUrl) {
      handleConnectRemote(backendUrl);
    } else if (port) {
      handleConnectRemote(`127.0.0.1:${port}`);
    }
  }, []);

  const handleConnectLocal = useCallback(async (pythonPath: string) => {
    setConnecting(true);
    setError(null);

    try {
      const client = new AgentClient();
      const config: AgentConfig = {
        mode: "local",
        python_path: pythonPath,
      };
      const info = await client.connect(config);

      setClient(client);
      setConnected(true);
      setPort(info.port);
      setMode("local");

      // Subscribe to backend events
      client.onEvent((event) => {
        addEvent(event);
        if (event.event_type === "status.update") {
          const kind = (event.payload as Record<string, unknown>)?.kind as string | undefined;
          if (kind) {
            useGatewayStore.setState({ agentStatus: kind as any });
          }
        }
      });
    } catch (err: unknown) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
      setConnected(false);
    } finally {
      setConnecting(false);
    }
  }, [setClient, setConnected, setError, setPort, setMode, addEvent]);

  const handleConnectRemote = useCallback(async (url: string) => {
    setConnecting(true);
    setError(null);

    try {
      // Parse host:port from URL
      let host: string;
      let port: number;

      if (url.includes("://")) {
        const parsed = new URL(url);
        host = parsed.hostname;
        port = parseInt(parsed.port, 10) || 8443;
      } else {
        const parts = url.split(":");
        host = parts[0];
        port = parseInt(parts[1], 10) || 8443;
      }

      const client = new AgentClient();
      const config: AgentConfig = {
        mode: "remote",
        remote_host: host,
        remote_port: port,
      };
      const info = await client.connect(config);

      setClient(client);
      setConnected(true);
      setPort(info.port);
      setMode("remote");

      client.onEvent((event) => {
        addEvent(event);
      });
    } catch (err: unknown) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
      setConnected(false);
    } finally {
      setConnecting(false);
    }
  }, [setClient, setConnected, setError, setPort, setMode, addEvent]);

  const handleStartLocal = useCallback(async (pythonPath: string) => {
    setStarting(true);
    try {
      if (typeof window !== "undefined" && (window as any).__TAURI__) {
        await handleConnectLocal(pythonPath);
      } else {
        setError("Запустите backend: python tui_gateway/tcp_server.py --port 8443");
      }
    } finally {
      setStarting(false);
    }
  }, [handleConnectLocal, setError]);

  if (!connected) {
    return (
      <ConnectScreen
        onConnectLocal={handleConnectLocal}
        onConnectRemote={handleConnectRemote}
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
