// src/App.tsx
// v0.5.0: Multi-mode connection, kanban, extended settings
// Flow: Splash → Welcome → Connection → Main

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
import { KanbanBoard } from "./components/kanban/KanbanBoard";
import { MemoryScreen } from "./components/memory/MemoryScreen";
import { SkillsScreen } from "./components/skills/SkillsScreen";
import { SchedulesScreen } from "./components/schedules/SchedulesScreen";
import { ProfilesScreen } from "./components/profiles/ProfilesScreen";
import { ProvidersScreen } from "./components/providers/ProvidersScreen";
import { SplashScreen } from "./components/SplashScreen";
import { WelcomeScreen } from "./components/WelcomeScreen";
import { useGatewayStore } from "./stores/gatewayStore";
import { useUIStore } from "./stores/uiStore";
import { useTranslation } from "./hooks/useTranslation";

type AppScreen = "splash" | "welcome" | "connection" | "main";

export function App() {
  const [screen, setScreen] = useState<AppScreen>("splash");
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
        const result = await invoke<{ hermes_home: string; version: string }>(
          "init_app"
        );
        setHermesHome(result.hermes_home);
      } catch (err) {
        console.error("Failed to initialize app:", err);
      }
    };
    init();
  }, [setHermesHome]);

  const handleSplashComplete = useCallback((autoconnect: boolean) => {
    setScreen(autoconnect ? "connection" : "welcome");
  }, []);

  const handleGetStarted = useCallback(() => {
    setScreen("connection");
  }, []);

  const handleConnected = useCallback(() => {
    setConnected(true);
    setError(null);
    setScreen("main");
  }, [setConnected, setError]);

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

  // ── Screen router ────────────────────────────────────────────────────────

  // Splash → Welcome → Connection → Main
  if (screen === "splash") {
    return <SplashScreen onComplete={handleSplashComplete} />;
  }

  if (screen === "welcome") {
    return <WelcomeScreen onGetStarted={handleGetStarted} />;
  }

  if (screen === "connection" && !connected) {
    return <ConnectionScreen onConnected={handleConnected} error={error} />;
  }

  // Main UI (or auto-transitioned from connection → main)
  return (
    <div className="flex h-full">
      {sidebarOpen && (
        <Sidebar activeTab={activeTab} onTabChange={setActiveTab} />
      )}

      <div className="flex-1 flex flex-col overflow-hidden">
        <Header onSettingsClick={() => setSettingsOpen(true)} />

        <div className="flex-1 overflow-hidden">
          {/* Real components */}
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
          {activeTab === "kanban" && <KanbanBoard />}
          {activeTab === "models" && <ProfilesScreen />}
          {activeTab === "settings" && settingsOpen && (
            <SettingsPanel onClose={() => setSettingsOpen(false)} />
          )}

          {/* Coming soon tabs */}
          {activeTab === "memory" && <MemoryScreen />}
          {activeTab === "skills" && <SkillsScreen />}
          {activeTab === "providers" && <ProvidersScreen />}
          {activeTab === "schedules" && <SchedulesScreen />}
        </div>

        <StatusBar />
      </div>

      {settingsOpen && activeTab !== "settings" && (
        <SettingsPanel onClose={() => setSettingsOpen(false)} />
      )}
    </div>
  );
}