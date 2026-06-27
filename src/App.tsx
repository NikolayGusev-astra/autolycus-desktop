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
import { SteersmanScreen } from "./steersman/SteersmanChatView";
import { MemoryScreen } from "./components/memory/MemoryScreen";
import { SkillsScreen } from "./components/skills/SkillsScreen";
import { SchedulesScreen } from "./components/schedules/SchedulesScreen";
import { ProfilesScreen } from "./components/profiles/ProfilesScreen";
import ProvidersScreen from "./components/providers/ProvidersScreen";
import ConfigHealthBanner from "./components/config/ConfigHealthBanner";
import { SplashScreen } from "./components/SplashScreen";
import { WelcomeScreen } from "./components/WelcomeScreen";
import { ThemeProvider } from "./components/ThemeProvider";
import { DiagnoseScreen } from "./components/settings/DiagnoseScreen";
import { GatewayScreen } from "./components/gateway/GatewayScreen";
import { ToolsScreen } from "./components/tools/ToolsScreen";
import { Versions } from "./components/Versions";
import { useGatewayStore } from "./stores/gatewayStore";
import { useUIStore } from "./stores/uiStore";


type AppScreen = "splash" | "welcome" | "connection" | "main";

export function App() {
  const [screen, setScreen] = useState<AppScreen>("splash");
  const [activeTab, setActiveTab] = useState("chat");
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [appVersion, setAppVersion] = useState<string>("");
  const [detectedInstances, setDetectedInstances] = useState<
    Array<{
      path: string;
      instance_type: string;
      version: string;
      gateway_running: boolean;
      gateway_port: number | null;
      active_profile: string;
      home_dir?: string;
      label?: string;
    }>
  >([]);
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
        setAppVersion(result.version);
      } catch (err) {
        console.error("Failed to initialize app:", err);
      }
    };
    init();
  }, [setHermesHome]);

  // Detect existing agent installations once, to offer adopting an
  // environment on the welcome screen.
  useEffect(() => {
    const detect = async () => {
      try {
        const instances = await invoke<
          Array<{
            path: string;
            instance_type: string;
            version: string;
            gateway_running: boolean;
            gateway_port: number | null;
            active_profile: string;
            home_dir?: string;
            label?: string;
          }>
        >("detect_local_instances_cmd");
        setDetectedInstances(instances ?? []);
      } catch (err) {
        console.error("Instance detection failed:", err);
      }
    };
    detect();
  }, []);

  const handleSplashComplete = useCallback((autoconnect: boolean) => {
    setScreen(autoconnect ? "connection" : "welcome");
  }, []);

  const handleGetStarted = useCallback(() => {
    setScreen("connection");
  }, []);

  // Adopt an existing agent environment: set HERMES_HOME to the chosen
  // instance's home, then proceed to the connection screen.
  const handleConnectInstance = useCallback(
    async (instance: {
      home_dir?: string;
      label?: string;
      instance_type: string;
    }) => {
      try {
        const resolvedHome = await invoke<string>("connect_to_instance", {
          instance,
        });
        setHermesHome(resolvedHome);
        setScreen("connection");
      } catch (err) {
        console.error("Failed to connect to instance:", err);
        setError(String(err));
        // Fall through to manual connection on failure.
        setScreen("connection");
      }
    },
    [setHermesHome, setError]
  );

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
    return (
      <WelcomeScreen
        onGetStarted={handleGetStarted}
        detectedInstances={detectedInstances}
        appVersion={appVersion}
        onConnectInstance={handleConnectInstance}
      />
    );
  }

  if (screen === "connection" && !connected) {
    return <ConnectionScreen onConnected={handleConnected} error={error} />;
  }

  // Main UI (or auto-transitioned from connection → main)
  return (
    <ThemeProvider>
    <div className="flex h-full">
      {sidebarOpen && (
        <Sidebar activeTab={activeTab} onTabChange={setActiveTab} />
      )}

      <div className="flex-1 flex flex-col overflow-hidden">
        <Header onSettingsClick={() => setSettingsOpen(true)} />

        <div className="flex-1 overflow-hidden">
          <ConfigHealthBanner profile={undefined} />
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
          {activeTab === "steersman" && <SteersmanScreen />}
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
          {activeTab === "diagnose" && <DiagnoseScreen />}
          {activeTab === "gateway" && <GatewayScreen />}
          {activeTab === "tools" && <ToolsScreen />}
          {activeTab === "versions" && <Versions />}
          {activeTab === "schedules" && <SchedulesScreen />}
        </div>

        <StatusBar />
      </div>

      {settingsOpen && activeTab !== "settings" && (
        <SettingsPanel onClose={() => setSettingsOpen(false)} />
      )}
    </div>
    </ThemeProvider>
  );
}