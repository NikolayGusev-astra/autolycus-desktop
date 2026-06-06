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

      case "token":
      case "message.chunk": {
        const content = payload?.content as string | undefined;
        if (content) {
          const messages = useGatewayStore.getState().messages;
          const lastIdx = messages.length - 1;
          const lastMsg = messages[lastIdx];
          if (lastMsg && lastMsg.isStreaming && lastMsg.role === "assistant") {
            // Immutable update: create new array with updated last message
            const updated = [...messages];
            updated[lastIdx] = { ...lastMsg, content: lastMsg.content + content };
            useGatewayStore.setState({ messages: updated });
          } else {
            addMessage({
              id: `msg-${Date.now()}`,
              role: "assistant",
              content,
              timestamp: Date.now(),
              isStreaming: true,
            });
          }
        }
        break;
      }

      case "reasoning": {
        const content = payload?.content as string | undefined;
        if (content) {
          addMessage({
            id: `thinking-${Date.now()}`,
            role: "assistant",
            content,
            timestamp: Date.now(),
            isStreaming: true,
          });
        }
        break;
      }

      case "tool_progress": {
        const tool = payload?.tool as string | undefined;
        const status = payload?.status as string | undefined;
        if (tool && status) {
          addMessage({
            id: `tool-${Date.now()}`,
            role: "tool",
            content: `${status === "done" ? "✅" : "🔧"} ${tool}`,
            timestamp: Date.now(),
          });
        }
        setAgentStatus("tool_calling");
        break;
      }

      case "done": {
        // Mark last streaming message as complete
        const messages = useGatewayStore.getState().messages;
        const lastIdx = messages.length - 1;
        const lastMsg = messages[lastIdx];
        if (lastMsg && lastMsg.isStreaming) {
          const updated = [...messages];
          updated[lastIdx] = { ...lastMsg, isStreaming: false };
          useGatewayStore.setState({ messages: updated });
        }
        setAgentStatus("idle");
        break;
      }

      case "error":
        setAgentStatus("error");
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
