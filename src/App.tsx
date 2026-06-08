// src/App.tsx
// v0.4.0: Multi-mode connection (local/remote/ssh), session management, profile support

import { useState, useCallback, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Sidebar } from "./components/layout/Sidebar";
import { Header } from "./components/layout/Header";
import { ChatView } from "./components/chat/ChatView";
import { SessionList } from "./components/sessions/SessionList";
import { SettingsPanel } from "./components/settings/SettingsPanel";
import { StatusBar } from "./components/layout/StatusBar";
import { ConnectionScreen } from "./components/ConnectionScreen";
import { ApprovalCard } from "./components/chat/ApprovalCard";
import { useGatewayStore } from "./stores/gatewayStore";
import { useUIStore } from "./stores/uiStore";

export function App() {
  const [activeTab, setActiveTab] = useState("chat");
  const [settingsOpen, setSettingsOpen] = useState(false);
  const { sidebarOpen } = useUIStore();
  const {
    connected,
    error,
    setConnected,
    setError,
    setHermesHome,
    pendingApproval,
    setPendingApproval,
  } = useGatewayStore();

  useEffect(() => {
    const init = async () => {
      try {
        const result = await invoke<{ hermes_home: string; version: string }>("init_app");
        setHermesHome(result.hermes_home);
      } catch (err) {
        console.error("Failed to initialize app:", err);
      }
    };
    init();
  }, [setHermesHome]);

  const handleApprovalDecision = useCallback(
    async (decision: "approved" | "denied" | "approved_always") => {
      const approval = useGatewayStore.getState().pendingApproval;
      if (!approval) return;

      try {
        await invoke("send_message_cmd", {
          request: {
            text: JSON.stringify({
              type: "approval_decision",
              request_id: approval.requestId,
              decision,
            }),
            session_id: null,
            history: null,
          },
        });
      } catch (err) {
        console.error("Failed to send approval decision:", err);
      }
      setPendingApproval(null);
    },
    [setPendingApproval]
  );

  if (!connected) {
    return (
      <ConnectionScreen
        onConnected={() => {
          setConnected(true);
          setError(null);
        }}
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
          {activeTab === "chat" && (
            <>
              <ChatView />
              {pendingApproval && (
                <ApprovalCard
                  request={pendingApproval}
                  onApprove={() => handleApprovalDecision("approved")}
                  onDeny={() => handleApprovalDecision("denied")}
                  onApproveAlways={() =>
                    handleApprovalDecision("approved_always")
                  }
                />
              )}
            </>
          )}
          {activeTab === "sessions" && <SessionList />}
        </div>

        <StatusBar />
      </div>

      {settingsOpen && (
        <SettingsPanel onClose={() => setSettingsOpen(false)} />
      )}
    </div>
  );
}
