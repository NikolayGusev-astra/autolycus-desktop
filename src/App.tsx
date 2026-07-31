// src/App.tsx
// Startup flow: Splash → (auto-adopt local Hermes) → Main, with Welcome /
// Connection as fallbacks when no local instance is detected (ADR-003).
// ThemeProvider wraps the whole app from main.tsx so every screen shares one
// theme (ADR-004).

import { useState, useCallback, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Sidebar, type ViewId } from "./components/layout/Sidebar";
import { Header } from "./components/layout/Header";
import { ChatView } from "./components/chat/ChatView";
import { SettingsPanel } from "./components/settings/SettingsPanel";
import { StatusBar } from "./components/layout/StatusBar";
import { ConnectionScreen } from "./components/ConnectionScreen";
import { ApprovalCard } from "./components/chat/ApprovalCard";
import { CommandPalette } from "./components/shared/CommandPalette";
import { HistoryPanel } from "./components/sessions/HistoryPanel";
import { FeedView } from "./components/views/FeedView";
import { WorkView, type WorkTab } from "./components/views/WorkView";
import { SelfDiagModal } from "./components/SelfDiagModal";
import { UserIntegrationOverview } from "./components/integrations/UserIntegrationOverview";
import { AdminIntegrationOverview } from "./components/integrations/AdminIntegrationOverview";
import { useTranslation as useTranslationHook } from "./hooks/useTranslation";
import { SplashScreen } from "./components/SplashScreen";
import { WelcomeScreen } from "./components/WelcomeScreen";
import { OnboardingScreen } from "./components/onboarding/OnboardingScreen";
import { useGatewayStore } from "./stores/gatewayStore";
import { useConversationStore } from "./stores/conversationStore";
import type { ProductEvent } from "./services/productConversation";
import type { PendingInteraction } from "./stores/conversationStore";
import type { ApprovalRequest } from "./lib/types";
import { capabilityRouter, type CapabilityRouteClarificationEvent } from "./services/capabilityRouter";
import { RouteClarificationDialog } from "./components/capabilities/RouteClarificationDialog";


type AppScreen = "splash" | "welcome" | "connection" | "onboarding" | "main";

const PRODUCT_EVENT_TYPES: Record<string, ProductEvent["type"]> = {
  message_delta: "MessageDelta",
  message_completed: "MessageCompleted",
  reasoning: "Reasoning",
  thinking: "Thinking",
  tool_started: "ToolStarted",
  tool_completed: "ToolCompleted",
  approval_required: "ApprovalRequired",
  clarification_required: "ClarificationRequired",
  secret_required: "SecretRequired",
  privilege_required: "PrivilegeRequired",
  error: "Error",
  status_update: "StatusUpdate",
  progress: "Progress",
  interaction_expired: "InteractionExpired",
};

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function toRouteClarificationEvent(value: unknown): CapabilityRouteClarificationEvent | null {
  if (!isRecord(value) || typeof value.conversation_id !== "string" || typeof value.capability_id !== "string" || !isRecord(value.clarification)) return null;
  const clarification = value.clarification;
  if (typeof clarification.request_id !== "string" || typeof clarification.prompt !== "string" || !Array.isArray(clarification.choices)) return null;
  const choices = clarification.choices.map((choice) => {
    if (!isRecord(choice) || typeof choice.instance_id !== "string" || typeof choice.label !== "string" || typeof choice.description !== "string") return null;
    return { instance_id: choice.instance_id, label: choice.label, description: choice.description };
  });
  if (choices.some((choice) => choice === null)) return null;
  return { conversation_id: value.conversation_id, capability_id: value.capability_id, clarification: { request_id: clarification.request_id, prompt: clarification.prompt, choices: choices.filter((choice): choice is NonNullable<typeof choice> => choice !== null) } };
}

function toProductEvent(payload: unknown, currentConversationId: string | null): ProductEvent | null {
  if (!isRecord(payload) || typeof payload.type !== "string") return null;

  const type = PRODUCT_EVENT_TYPES[payload.type];
  const conversationId =
    typeof payload.conversation_id === "string"
      ? payload.conversation_id
      : currentConversationId;
  if (!type || !conversationId) return null;

  return { ...payload, type, conversation_id: conversationId };
}

function InteractionDialog({
  interaction,
  inputType,
  title,
  onSubmit,
  onClose,
  queuePosition,
  queueSize,
  onPrevious,
  onNext,
}: {
  interaction: PendingInteraction;
  inputType: "text" | "password";
  title: string;
  onSubmit: (value: string) => void;
  onClose: () => void;
  queuePosition: number;
  queueSize: number;
  onPrevious: () => void;
  onNext: () => void;
}) {
  const [value, setValue] = useState("");
  const message = typeof interaction.payload.message === "string" ? interaction.payload.message : "";
  const prompt = typeof interaction.payload.prompt === "string" ? interaction.payload.prompt : "";
  const envVar = typeof interaction.payload.env_var === "string" ? interaction.payload.env_var : "";
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-4">
      <form
        className="w-full max-w-md rounded-lg border border-ac-border bg-ac-bg p-4 shadow-xl"
        onSubmit={(event) => { event.preventDefault(); onSubmit(value); }}
      >
        <div className="mb-2 flex items-center gap-2">
          <h2 className="text-sm font-semibold text-ac-ink">{title}</h2>
          {queueSize > 1 && (
            <div className="ml-auto flex items-center gap-1 text-xs text-ac-muted">
              <button type="button" onClick={onPrevious} className="px-1 hover:text-ac-ink" aria-label="Previous interaction">Previous</button>
              <span>{queuePosition + 1} of {queueSize}</span>
              <button type="button" onClick={onNext} className="px-1 hover:text-ac-ink" aria-label="Next interaction">Next</button>
            </div>
          )}
        </div>
        {message && <p className="mb-3 text-sm text-ac-muted">{message}</p>}
        {prompt && <p className="mb-3 whitespace-pre-wrap text-sm text-ac-ink">{prompt}</p>}
        {envVar && <p className="mb-3 text-xs text-ac-muted">Environment variable: <code>{envVar}</code></p>}
        {interaction.choices.length > 0 && (
          <div className="mb-3 flex flex-wrap gap-2">
            {interaction.choices.map((choice) => (
              <button key={choice} type="button" onClick={() => onSubmit(choice)} className="rounded border border-ac-brand/30 px-3 py-1.5 text-xs text-ac-brand hover:bg-ac-brand/10">
                {choice}
              </button>
            ))}
          </div>
        )}
        <input
          autoFocus
          type={inputType}
          value={value}
          onChange={(event) => setValue(event.target.value)}
          className="w-full rounded border border-ac-border bg-ac-surface px-3 py-2 text-sm text-ac-ink"
        />
        <div className="mt-4 flex justify-end gap-2">
          <button type="button" onClick={onClose} className="px-3 py-1.5 text-xs text-ac-muted">Cancel</button>
          <button type="submit" className="ac-btn px-3 py-1.5 text-xs">Submit</button>
        </div>
      </form>
    </div>
  );
}

export function App() {
  const [screen, setScreen] = useState<AppScreen>("splash");
  const [activeView, setActiveView] = useState<ViewId>(() => window.location.pathname === "/admin/integrations" ? "admin-integrations" : window.location.pathname === "/integrations" ? "integrations" : "dashboard");
  const navigate = useCallback((view: ViewId) => {
    const path = view === "integrations" ? "/integrations" : view === "admin-integrations" ? "/admin/integrations" : "/";
    window.history.pushState({}, "", path);
    setActiveView(view);
  }, []);
  const [historyOpen, setHistoryOpen] = useState(true);
  const [selfDiagOpen, setSelfDiagOpen] = useState(false);
  // Which sub-tab WorkView opens on (e.g. "tasks" from dashboard "new task").
  const [workInitialTab, setWorkInitialTab] = useState<WorkTab>("tasks");
  // Command palette (Cmd/Ctrl+K).
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [appVersion, setAppVersion] = useState<string>("");
  const [expiredInteraction, setExpiredInteraction] = useState<string | null>(null);
  const [interactionIndex, setInteractionIndex] = useState(0);
  const [routeClarifications, setRouteClarifications] = useState<CapabilityRouteClarificationEvent[]>([]);
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
  } = useGatewayStore();
  const respondApproval = useConversationStore((state) => state.respondApproval);
  const respondClarification = useConversationStore((state) => state.respondClarification);
  const respondSecret = useConversationStore((state) => state.respondSecret);
  const respondSudo = useConversationStore((state) => state.respondSudo);
  const pendingInteractions = useConversationStore((state) => state.pendingInteractions);
  const removePendingInteraction = useConversationStore((state) => state.removePendingInteraction);
  const cancelInteraction = useConversationStore((state) => state.cancelInteraction);

  // Product events are normalized once at the Tauri boundary, then become the
  // single source of truth for product conversation state across all views.
  useEffect(() => {
    const unlisten = listen<unknown>("product-event", ({ payload }) => {
      const productEvent = toProductEvent(
        payload,
        useConversationStore.getState().currentConversationId,
      );
      if (!productEvent) return;

      try {
        useConversationStore.getState().handleProductEvent(productEvent);
      } catch (error) {
        console.error("Failed to handle product event:", error);
      }

      if (productEvent.type === "InteractionExpired" && typeof productEvent.request_id === "string") {
        setExpiredInteraction("This request expired.");
      }
    });

    return () => {
      void unlisten.then((dispose) => dispose());
    };
  }, []);

  // Capability clarifications join the same global interaction flow: only the
  // first unresolved choice is presented, then the next queued choice opens.
  useEffect(() => {
    const unlisten = listen<unknown>("capability-route-clarification", ({ payload }) => {
      const clarification = toRouteClarificationEvent(payload);
      if (clarification) setRouteClarifications((queue) => [...queue, clarification]);
    });
    return () => { void unlisten.then((dispose) => dispose()); };
  }, []);

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

  // ── Global keyboard shortcuts ──────────────────────────────────────────
  // Cmd/Ctrl+K → command palette
  // Cmd/Ctrl+B → toggle sidebar
  // Cmd/Ctrl+, → settings
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const mod = e.metaKey || e.ctrlKey;
      if (mod && e.key === "k") {
        e.preventDefault();
        setPaletteOpen((v) => !v);
      } else if (mod && e.key === "b") {
        e.preventDefault();
        // Toggle sidebar via uiStore.
        import("./stores/uiStore").then(({ useUIStore }) => {
          useUIStore.getState().toggleSidebar();
        });
      } else if (mod && e.key === ",") {
        e.preventDefault();
        setActiveView("settings");
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, []);

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

  const handleApprovalDecision = useCallback(async (interaction: PendingInteraction, choice: string) => {
    try {
      await respondApproval(interaction.conversationId, interaction.requestId, choice, false);
      removePendingInteraction(interaction.conversationId, interaction.requestId);
    } catch (err) {
      console.error("Failed to send approval decision:", err);
    }
  }, [removePendingInteraction, respondApproval]);

  const handleTextInteraction = useCallback(async (interaction: PendingInteraction, value: string) => {
    try {
      if (interaction.kind === "clarification") {
        await respondClarification(interaction.conversationId, interaction.requestId, value);
      } else if (interaction.kind === "secret") {
        await respondSecret(interaction.conversationId, interaction.requestId, value);
      } else if (interaction.kind === "privilege") {
        await respondSudo(interaction.conversationId, interaction.requestId, value);
      }
      removePendingInteraction(interaction.conversationId, interaction.requestId);
    } catch (err) {
      console.error("Failed to respond to interaction:", err);
    }
  }, [removePendingInteraction, respondClarification, respondSecret, respondSudo]);

  const queuedInteractions = Array.from(pendingInteractions.values());
  const pendingInteraction = queuedInteractions[Math.min(interactionIndex, queuedInteractions.length - 1)] ?? null;
  const routeClarification = routeClarifications[0] ?? null;
  const chooseRoute = useCallback(async (instanceId: string) => {
    if (!routeClarification) return;
    await capabilityRouter.submitRouteChoice({
      conversation_id: routeClarification.conversation_id,
      capability_id: routeClarification.capability_id,
      instance_id: instanceId,
    });
    setRouteClarifications((queue) => queue.slice(1));
  }, [routeClarification]);
  const approvalInteraction = pendingInteraction?.kind === "approval" ? pendingInteraction : null;
  const approvalRequest: ApprovalRequest | null = approvalInteraction ? {
    requestId: approvalInteraction.requestId,
    toolName: typeof approvalInteraction.payload.tool_id === "string" ? approvalInteraction.payload.tool_id : "tool",
    toolInput: typeof approvalInteraction.payload.input === "string" ? approvalInteraction.payload.input : "",
    action: typeof approvalInteraction.payload.message === "string" ? approvalInteraction.payload.message : "",
    commandClass: "write",
  } : null;

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
  // shturman.ai-style SPA shell: Sidebar (4 sections) + frosted Header + main
  // content (one active view). The chat view additionally gets the right-hand
  // history rail. Tasks/Kanban/Goals/Projects/Protocols/Stats are consolidated
  // inside WorkView with internal sub-tabs.
  return (
    <div className="flex h-full bg-ac-bg overflow-hidden">
      <Sidebar
        activeView={activeView}
        onViewChange={navigate}
        onSelfDiagnosis={() => setSelfDiagOpen(true)}
      />

      <div className="flex-1 flex flex-col min-w-0 overflow-hidden">
        <Header />

        <main className="flex-1 flex min-h-0 overflow-hidden">
          {/* Chat keeps its own two-pane layout (messages + history rail). */}
          {activeView === "chat" ? (
            <div className="flex flex-1 min-w-0">
              <div className="flex-1 flex flex-col overflow-hidden">
                <ChatView historyOpen={historyOpen} onToggleHistory={() => setHistoryOpen((v) => !v)} />
              </div>
              {historyOpen && <HistoryPanel onClose={() => setHistoryOpen(false)} />}
            </div>
          ) : activeView === "dashboard" ? (
            <div className="flex-1 overflow-y-auto">
              <FeedView
                onNewTask={() => { setWorkInitialTab("tasks"); setActiveView("work"); }}
                onOpenSession={async (sid) => {
                  try {
                    const msgs = await invoke<Array<{ id: number; role: string; content: string; timestamp: number }>>("get_session_messages_cmd", { sessionId: sid, profile: null });
                    const mapped = msgs.filter((m) => m.role === "user" || m.role === "assistant").map((m) => ({
                      id: `hist-${m.id}`, role: m.role as "user" | "assistant", content: m.content, timestamp: m.timestamp,
                    }));
                    useGatewayStore.setState({ messages: mapped, currentSessionId: sid });
                  } catch (e) { console.error("feed session load", e); }
                  setActiveView("chat");
                }}
                onOpenChat={() => setActiveView("chat")}
                onOpenWork={(tab) => { setWorkInitialTab(tab); setActiveView("work"); }}
              />
            </div>
          ) : activeView === "work" ? (
            <WorkView initialTab={workInitialTab} />
          ) : activeView === "settings" ? (
            <div className="flex-1 overflow-y-auto">
              <SettingsPanel onClose={() => setActiveView("dashboard")} />
            </div>
          ) : activeView === "integrations" ? (
            <div className="flex-1 overflow-y-auto"><UserIntegrationOverview /></div>
          ) : activeView === "admin-integrations" ? (
            <div className="flex-1 overflow-y-auto"><AdminIntegrationOverview /></div>
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

      {/* Self-diagnosis modal — mood/energy check-in */}
      {selfDiagOpen && (
        <SelfDiagModal onClose={() => setSelfDiagOpen(false)} />
      )}

      {pendingInteraction && (
        <div className="fixed bottom-4 right-4 z-40 rounded-full bg-ac-brand px-3 py-1.5 text-xs font-medium text-white shadow-lg">
          {queuedInteractions.length} pending interaction{queuedInteractions.length === 1 ? "" : "s"}
        </div>
      )}

      {routeClarification && (
        <RouteClarificationDialog event={routeClarification}
          onChoose={(instanceId) => { void chooseRoute(instanceId); }}
          onClose={() => setRouteClarifications((queue) => queue.slice(1))} />
      )}

      {approvalInteraction && approvalRequest && (
        <div className="fixed inset-x-0 bottom-0 z-50 border-t border-ac-border bg-ac-bg shadow-xl">
          {queuedInteractions.length > 1 && (
            <div className="flex justify-end gap-2 px-4 pt-2 text-xs text-ac-muted">
              <button onClick={() => setInteractionIndex((index) => (index - 1 + queuedInteractions.length) % queuedInteractions.length)} className="hover:text-ac-ink">Previous</button>
              <span>{Math.min(interactionIndex, queuedInteractions.length - 1) + 1} of {queuedInteractions.length}</span>
              <button onClick={() => setInteractionIndex((index) => (index + 1) % queuedInteractions.length)} className="hover:text-ac-ink">Next</button>
            </div>
          )}
          <ApprovalCard
            request={approvalRequest}
            choices={approvalInteraction.choices}
            onChoose={(choice) => { void handleApprovalDecision(approvalInteraction, choice); }}
          />
        </div>
      )}

      {pendingInteraction?.kind === "clarification" && (
        <InteractionDialog interaction={pendingInteraction} inputType="text" title="Clarification required"
          onSubmit={(value) => { void handleTextInteraction(pendingInteraction, value); }}
          onClose={() => { void cancelInteraction(pendingInteraction.conversationId, pendingInteraction.requestId, pendingInteraction.kind); }}
          queuePosition={Math.min(interactionIndex, queuedInteractions.length - 1)} queueSize={queuedInteractions.length}
          onPrevious={() => setInteractionIndex((index) => (index - 1 + queuedInteractions.length) % queuedInteractions.length)}
          onNext={() => setInteractionIndex((index) => (index + 1) % queuedInteractions.length)} />
      )}
      {pendingInteraction?.kind === "secret" && (
        <InteractionDialog interaction={pendingInteraction} inputType="password" title="Secret required"
          onSubmit={(value) => { void handleTextInteraction(pendingInteraction, value); }}
          onClose={() => { void cancelInteraction(pendingInteraction.conversationId, pendingInteraction.requestId, pendingInteraction.kind); }}
          queuePosition={Math.min(interactionIndex, queuedInteractions.length - 1)} queueSize={queuedInteractions.length}
          onPrevious={() => setInteractionIndex((index) => (index - 1 + queuedInteractions.length) % queuedInteractions.length)}
          onNext={() => setInteractionIndex((index) => (index + 1) % queuedInteractions.length)} />
      )}
      {pendingInteraction?.kind === "privilege" && (
        <InteractionDialog interaction={pendingInteraction} inputType="password" title="Password required"
          onSubmit={(value) => { void handleTextInteraction(pendingInteraction, value); }}
          onClose={() => { void cancelInteraction(pendingInteraction.conversationId, pendingInteraction.requestId, pendingInteraction.kind); }}
          queuePosition={Math.min(interactionIndex, queuedInteractions.length - 1)} queueSize={queuedInteractions.length}
          onPrevious={() => setInteractionIndex((index) => (index - 1 + queuedInteractions.length) % queuedInteractions.length)}
          onNext={() => setInteractionIndex((index) => (index + 1) % queuedInteractions.length)} />
      )}
      {expiredInteraction && (
        <button onClick={() => setExpiredInteraction(null)} className="fixed bottom-4 right-4 z-50 rounded bg-ac-surface px-4 py-2 text-sm text-ac-ink shadow-lg">
          {expiredInteraction}
        </button>
      )}

      {/* Command palette (Cmd/Ctrl+K) */}
      <CommandPalette
        open={paletteOpen}
        onClose={() => setPaletteOpen(false)}
        onNavigate={(view) => {
          if (view === "work") setWorkInitialTab("tasks");
          setActiveView(view as ViewId);
        }}
        onNewTask={() => { setWorkInitialTab("tasks"); setActiveView("work"); }}
        onToggleTheme={() => {
          import("./stores/uiStore").then(({ useUIStore }) => {
            useUIStore.getState().toggleDarkMode();
          });
        }}
      />
    </div>
  );
}
