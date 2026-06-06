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
import type { AgentConfig, AgentEvent } from "./lib/types";

export function App() {
  const [activeTab, setActiveTab] = useState("chat");
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [connecting, setConnecting] = useState(false);
  const [starting, setStarting] = useState(false);
  const { sidebarOpen } = useUIStore();
  const { connected, error, setClient, setConnected, setError, addEvent, addMessage, setAgentStatus } =
    useGatewayStore();

  const handleAgentEvent = useCallback((event: AgentEvent) => {
    addEvent(event);

    const payload = event.payload as Record<string, unknown>;

    switch (event.event_type) {
      case "gateway.ready":
        setAgentStatus("idle");
        break;

      case "status.update": {
        const kind = payload?.kind as string | undefined;
        if (kind) {
          setAgentStatus(kind as any);
        }
        break;
      }

      case "message.start":
        setAgentStatus("streaming");
        break;

      case "message.chunk": {
        const chunk = payload?.text as string | undefined;
        if (chunk) {
          addMessage({
            id: `msg-${Date.now()}`,
            role: "assistant",
            content: chunk,
            timestamp: Date.now(),
            isStreaming: true,
          });
        }
        break;
      }

      case "message.end":
        setAgentStatus("idle");
        break;

      case "tool.start":
        setAgentStatus("tool_calling");
        break;

      case "gateway.exited":
        setAgentStatus("error");
        setConnected(false);
        break;

      default:
        break;
    }
  }, [addEvent, addMessage, setAgentStatus, setConnected]);

  const handleConnectLocal = useCallback(async (pythonPath: string) => {
    setConnecting(true);
    try {
      const newClient = new AgentClient();
      const config: AgentConfig = {
        mode: "local",
        python_path: pythonPath,
      };
      await newClient.connect(config);
      setClient(newClient);
      setConnected(true);
      setError(null);

      newClient.onEvent((event: AgentEvent) => {
        handleAgentEvent(event);
      });
    } catch (err: unknown) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
      setConnected(false);
    } finally {
      setConnecting(false);
    }
  }, [setClient, setConnected, setError, handleAgentEvent]);

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
