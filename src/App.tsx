// src/App.tsx
// Startup flow: Splash → (auto-adopt local Hermes) → Main, with Welcome /
// Connection as fallbacks when no local instance is detected (ADR-003).
// ThemeProvider wraps the whole app from main.tsx so every screen shares one
// theme (ADR-004).

import { useState, useCallback, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Sidebar, type ViewId } from "./components/layout/Sidebar";
import { Header } from "./components/layout/Header";
import { ChatView } from "./components/chat/ChatView";
import { SettingsPanel } from "./components/settings/SettingsPanel";
import { StatusBar } from "./components/layout/StatusBar";
import { ConnectionScreen } from "./components/ConnectionScreen";
import { ApprovalCard } from "./components/chat/ApprovalCard";
import { KanbanBoard } from "./components/kanban/KanbanBoard";
import { HistoryPanel } from "./components/sessions/HistoryPanel";
import { DashboardView } from "./components/views/DashboardView";
import { useTranslation as useTranslationHook } from "./hooks/useTranslation";
import { SplashScreen } from "./components/SplashScreen";
import { WelcomeScreen } from "./components/WelcomeScreen";
import { OnboardingScreen } from "./components/onboarding/OnboardingScreen";
import { useGatewayStore } from "./stores/gatewayStore";


type AppScreen = "splash" | "welcome" | "connection" | "onboarding" | "main";

export function App() {
  const [screen, setScreen] = useState<AppScreen>("splash");
  const [activeView, setActiveView] = useState<ViewId>("dashboard");
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [historyOpen, setHistoryOpen] = useState(true);
  const [selfDiagOpen, setSelfDiagOpen] = useState(false);
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
  const { t } = useTranslationHook();
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

  const handleSplashComplete = useCallback(
    async (autoconnect: boolean) => {
      // ADR-003: when autoconnect is requested, try to auto-discover and adopt
      // the local Hermes instance in one shot. On success we go straight to the
      // main UI (the shturman.ai "Подключен" experience) without showing the
      // manual connection screen. If discovery finds nothing, fall through to
      // the welcome/connection screens as before.
      if (autoconnect) {
        try {
          const result = await invoke<{
            found: boolean;
            hermes_home: string | null;
            gateway_running: boolean;
            label: string | null;
            error: string | null;
          }>("auto_connect_local_cmd");
          if (result.found && result.gateway_running && result.hermes_home) {
            setHermesHome(result.hermes_home);
            setConnected(true);
            setError(null);
            setScreen("main");
            return;
          }
          // Found an instance but the gateway didn't come up — surface the
          // reason and fall through to onboarding for manual setup.
          if (result.found && result.error) {
            setError(result.error);
          }
        } catch (err) {
          console.error("Auto-connect failed:", err);
        }
        // No usable local agent → show the onboarding wizard, where the user
        // chooses: connect to a remote server, or install Hermes locally.
        setScreen("onboarding");
        return;
      }
      setScreen("welcome");
    },
    [setHermesHome, setConnected, setError]
  );

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

  // Onboarding finished (remote configured, or Hermes installed+configured).
  // Re-run auto-connect so the just-configured instance is adopted and the
  // gateway started, then land in the main UI.
  const handleOnboardingDone = useCallback(async () => {
    try {
      const result = await invoke<{
        found: boolean;
        hermes_home: string | null;
        gateway_running: boolean;
        error: string | null;
      }>("auto_connect_local_cmd");
      if (result.hermes_home) setHermesHome(result.hermes_home);
      if (result.found) {
        setConnected(true);
        setError(null);
        setScreen("main");
        return;
      }
    } catch (err) {
      console.error("Post-onboarding connect failed:", err);
    }
    // Fall back to the connection screen for a manual retry.
    setScreen("connection");
  }, [setHermesHome, setConnected, setError]);

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

  if (screen === "onboarding") {
    return <OnboardingScreen onDone={handleOnboardingDone} onConnected={handleConnected} />;
  }

  if (screen === "connection" && !connected) {
    return <ConnectionScreen onConnected={handleConnected} error={error} />;
  }

  // Main UI (or auto-transitioned from connection → main).
  // shturman.ai-style SPA shell: Sidebar (8 sections) + frosted Header + main
  // content (one active view). The chat view additionally gets the right-hand
  // history rail.
  return (
    <div className="flex h-full bg-ac-bg overflow-hidden">
      <Sidebar
        activeView={activeView}
        onViewChange={setActiveView}
        onSelfDiagnosis={() => setSelfDiagOpen(true)}
      />

      <div className="flex-1 flex flex-col min-w-0 overflow-hidden">
        <Header />

        <main className="flex-1 flex min-h-0 overflow-hidden">
          {/* Chat keeps its own two-pane layout (messages + history rail). */}
          {activeView === "chat" ? (
            <div className="flex flex-1 min-w-0">
              <div className="flex-1 flex flex-col overflow-hidden">
                <ChatView />
                {pendingApproval && (
                  <ApprovalCard
                    request={pendingApproval}
                    onApprove={() => handleApprovalDecision("approved")}
                    onDeny={() => handleApprovalDecision("denied")}
                    onApproveAlways={() => handleApprovalDecision("approved_always")}
                  />
                )}
              </div>
              {historyOpen && <HistoryPanel onClose={() => setHistoryOpen(false)} />}
            </div>
          ) : activeView === "dashboard" ? (
            <div className="flex-1 overflow-y-auto">
              <DashboardView
                onNavigate={(v) => setActiveView(v)}
                onSelfDiagnosis={() => setSelfDiagOpen(true)}
              />
            </div>
          ) : activeView === "tasks" ? (
            <div className="flex-1 overflow-y-auto">
              <KanbanBoard />
            </div>
          ) : activeView === "settings" ? (
            <div className="flex-1 overflow-y-auto">
              <SettingsPanel onClose={() => setActiveView("dashboard")} />
            </div>
          ) : (
            <div className="flex-1 overflow-y-auto p-6">
              <div className="max-w-2xl mx-auto text-center py-20 text-ac-muted">
                <p className="text-sm">{t("comingSoon")}</p>
              </div>
            </div>
          )}
        </main>

        <StatusBar />
      </div>

      {/* Settings modal (also reachable when not on the settings view). */}
      {settingsOpen && (
        <SettingsPanel onClose={() => setSettingsOpen(false)} />
      )}

      {/* Self-diagnosis modal placeholder */}
      {selfDiagOpen && (
        <div className="ac-modal-overlay" onClick={() => setSelfDiagOpen(false)}>
          <div className="ac-modal" style={{ maxWidth: 420 }} onClick={(e) => e.stopPropagation()}>
            <p className="text-sm text-ac-ink mb-3">{t("nav.selfDiagnosis")}</p>
            <p className="text-xs text-ac-muted mb-4">{t("selfDiag.hint")}</p>
            <textarea className="ac-input w-full px-3 py-2 text-sm" rows={3} placeholder={t("selfDiag.placeholder")} />
            <div className="flex justify-end gap-2 mt-4">
              <button onClick={() => setSelfDiagOpen(false)} className="px-4 py-2 text-sm border border-ac-border text-ac-muted rounded-md">
                {t("btn.close")}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}