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
import { useTranslation as useTranslationHook } from "./hooks/useTranslation";
import { SplashScreen } from "./components/SplashScreen";
import { WelcomeScreen } from "./components/WelcomeScreen";
import { OnboardingScreen } from "./components/onboarding/OnboardingScreen";
import { useGatewayStore } from "./stores/gatewayStore";
import { useConversationStore } from "./stores/conversationStore";
import type { ProductEvent } from "./services/productConversation";
import type { ApprovalRequest } from "./lib/types";


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

type InteractionKind = "approval" | "clarification" | "secret" | "privilege";

interface PendingInteraction {
  conversationId: string;
  requestId: string;
  kind: InteractionKind;
  payload: Record<string, unknown>;
  choices: string[];
}

function interactionFromEvent(event: ProductEvent): PendingInteraction | null {
  const requestId = typeof event.request_id === "string" ? event.request_id : null;
  if (!requestId) return null;
  const kindByEvent: Partial<Record<ProductEvent["type"], InteractionKind>> = {
    ApprovalRequired: "approval",
    ClarificationRequired: "clarification",
    SecretRequired: "secret",
    PrivilegeRequired: "privilege",
  };
  const kind = kindByEvent[event.type];
  if (!kind) return null;
  const choices = Array.isArray(event.choices)
    ? event.choices.filter((choice): choice is string => typeof choice === "string")
    : [];
  return { conversationId: event.conversation_id, requestId, kind, payload: event, choices };
}

function InteractionDialog({
  interaction,
  inputType,
  title,
  onSubmit,
  onClose,
}: {
  interaction: PendingInteraction;
  inputType: "text" | "password";
  title: string;
  onSubmit: (value: string) => void;
  onClose: () => void;
}) {
  const [value, setValue] = useState("");
  const message = typeof interaction.payload.message === "string" ? interaction.payload.message : "";
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-4">
      <form
        className="w-full max-w-md rounded-lg border border-ac-border bg-ac-bg p-4 shadow-xl"
        onSubmit={(event) => { event.preventDefault(); onSubmit(value); }}
      >
        <h2 className="mb-2 text-sm font-semibold text-ac-ink">{title}</h2>
        {message && <p className="mb-3 text-sm text-ac-muted">{message}</p>}
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
  const [activeView, setActiveView] = useState<ViewId>("dashboard");
  const [historyOpen, setHistoryOpen] = useState(true);
  const [selfDiagOpen, setSelfDiagOpen] = useState(false);
  // Which sub-tab WorkView opens on (e.g. "tasks" from dashboard "new task").
  const [workInitialTab, setWorkInitialTab] = useState<WorkTab>("tasks");
  // Command palette (Cmd/Ctrl+K).
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [appVersion, setAppVersion] = useState<string>("");
  const [pendingInteractions, setPendingInteractions] = useState<Map<string, PendingInteraction>>(new Map());
  const [expiredInteraction, setExpiredInteraction] = useState<string | null>(null);
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

  // Product events are normalized once at the Tauri boundary, then become the
  // single source of truth for product conversation state across all views.
  useEffect(() => {
    const unlisten = listen<unknown>("product-event", ({ payload }) => {
      const productEvent = toProductEvent(
        payload,
        useConversationStore.getState().currentConversationId,
      );
      if (!productEvent) return;

      useConversationStore.getState().handleProductEvent(productEvent);

      const interaction = interactionFromEvent(productEvent);
      if (interaction) setPendingInteractions((current) => new Map(current).set(interaction.requestId, interaction));
      if (productEvent.type === "InteractionExpired" && typeof productEvent.request_id === "string") {
        const requestId = productEvent.request_id;
        setPendingInteractions((current) => {
          const next = new Map(current);
          next.delete(requestId);
          return next;
        });
        setExpiredInteraction("This request expired.");
      }
    });

    return () => {
      void unlisten.then((dispose) => dispose());
    };
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

  const removeInteraction = useCallback((requestId: string) => {
    setPendingInteractions((current) => {
      const next = new Map(current);
      next.delete(requestId);
      return next;
    });
  }, []);

  const handleApprovalDecision = useCallback(async (interaction: PendingInteraction, choice: string) => {
    try {
      await respondApproval(interaction.conversationId, interaction.requestId, choice, choice === "always");
      removeInteraction(interaction.requestId);
    } catch (err) {
      console.error("Failed to send approval decision:", err);
    }
  }, [removeInteraction, respondApproval]);

  const handleTextInteraction = useCallback(async (interaction: PendingInteraction, value: string) => {
    try {
      if (interaction.kind === "clarification") {
        await respondClarification(interaction.conversationId, interaction.requestId, value);
      } else if (interaction.kind === "secret") {
        await respondSecret(interaction.conversationId, interaction.requestId, value);
      } else if (interaction.kind === "privilege") {
        await respondSudo(interaction.conversationId, interaction.requestId, value);
      }
      removeInteraction(interaction.requestId);
    } catch (err) {
      console.error("Failed to respond to interaction:", err);
    }
  }, [removeInteraction, respondClarification, respondSecret, respondSudo]);

  const pendingInteraction = Array.from(pendingInteractions.values())[0] ?? null;
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
                <ChatView historyOpen={historyOpen} onToggleHistory={() => setHistoryOpen((v) => !v)} />
                {approvalInteraction && approvalRequest && (
                  <ApprovalCard
                    request={approvalRequest}
                    choices={approvalInteraction.choices}
                    onChoose={(choice) => { void handleApprovalDecision(approvalInteraction, choice); }}
                  />
                )}
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

      {pendingInteraction?.kind === "clarification" && (
        <InteractionDialog interaction={pendingInteraction} inputType="text" title="Clarification required"
          onSubmit={(value) => { void handleTextInteraction(pendingInteraction, value); }}
          onClose={() => removeInteraction(pendingInteraction.requestId)} />
      )}
      {pendingInteraction?.kind === "secret" && (
        <InteractionDialog interaction={pendingInteraction} inputType="password" title="Secret required"
          onSubmit={(value) => { void handleTextInteraction(pendingInteraction, value); }}
          onClose={() => removeInteraction(pendingInteraction.requestId)} />
      )}
      {pendingInteraction?.kind === "privilege" && (
        <InteractionDialog interaction={pendingInteraction} inputType="password" title="Password required"
          onSubmit={(value) => { void handleTextInteraction(pendingInteraction, value); }}
          onClose={() => removeInteraction(pendingInteraction.requestId)} />
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
