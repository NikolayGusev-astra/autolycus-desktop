// src/components/onboarding/OnboardingScreen.tsx
// First-run onboarding. Shown when no usable local Hermes agent is detected.
//
// The user first chooses:
//   (A) Connect to a remote server (→ URL/API-key form), or
//   (B) Install Hermes locally (→ runs the installer, then configures a
//       provider + API key + the agent "soul"/persona).
// Then a setup wizard walks them through provider/key/soul. On finish we hand
// control back to App, which re-runs auto-connect to adopt the new instance.

import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  ChevronRight,
  ChevronLeft,
  CheckCircle2,
  Loader2,
  Server,
  Globe,
  Download,
  Mic,
  Sparkles,
} from "lucide-react";
import { PROVIDERS } from "../../constants";
import { useTranslation } from "../../hooks/useTranslation";

type Phase =
  | "choice" // remote vs install
  | "install" // (install path) running installer
  | "setup" // unified: provider + key + soul on one screen
  | "done";

interface OnboardingScreenProps {
  onDone: () => void;
  /** For the remote path — hands off to the existing connection flow. */
  onConnected: () => void;
}

interface Personality {
  id: string;
  description: string;
}

export function OnboardingScreen({ onDone, onConnected }: OnboardingScreenProps) {
  const { t } = useTranslation();
  const [phase, setPhase] = useState<Phase>("choice");
  const [path, setPath] = useState<"remote" | "install" | null>(null);

  // Install state
  const [installing, setInstalling] = useState(false);
  const [installLog, setInstallLog] = useState<string[]>([]);
  const [installError, setInstallError] = useState<string | null>(null);

  // Provider/key state
  const [selectedProvider, setSelectedProvider] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [saving, setSaving] = useState(false);

  // Soul state
  const [personalities, setPersonalities] = useState<Personality[]>([]);
  const [chosenPersonality, setChosenPersonality] = useState("helpful");
  const [agentName, setAgentName] = useState("Штурман");
  const [company, setCompany] = useState("");

  const needsKey = PROVIDERS.setup.find((s) => s.id === selectedProvider)?.needsKey ?? false;

  // Steps: choice → (install if needed) → unified setup → done.
  // Previously this was 4-5 separate screens (provider → apiKey → soul each
  // sequential). Now it's one combined form so first-run is faster.
  const steps: { id: Phase; label: string }[] =
    path === "install"
      ? [
          { id: "install", label: t("onb.install") },
          { id: "setup", label: t("onb.setup") || "Setup" },
          { id: "done", label: t("onb.done") },
        ]
      : [
          { id: "setup", label: t("onb.setup") || "Setup" },
          { id: "done", label: t("onb.done") },
        ];

  const currentIndex = steps.findIndex((s) => s.id === phase);

  // Load personalities once we reach the setup step.
  useEffect(() => {
    if (phase !== "setup") return;
    invoke<Personality[]>("get_personalities_cmd")
      .then((ps) => {
        setPersonalities(ps);
        // Reflect the currently-active one as the default selection.
        invoke<string>("get_personality_cmd")
          .then((active) => active && setChosenPersonality(active))
          .catch(() => {});
      })
      .catch(() => {
        setPersonalities([
          { id: "helpful", description: "You are a helpful, friendly AI assistant." },
        ]);
      });
  }, [phase]);

  // ── Install handler ──────────────────────────────────────────────────────
  const startInstall = async () => {
    setInstalling(true);
    setInstallLog([]);
    setInstallError(null);

    // Stream installer output to the log.
    const unlisten = await listen<{ line: string; stream: string }>(
      "install-progress",
      (e) => {
        setInstallLog((prev) => [...prev, e.payload.line].slice(-200));
      }
    );

    try {
      const result = await invoke<{ success: boolean; error: string | null; hermes_home: string | null }>(
        "install_hermes_cmd"
      );
      unlisten();
      if (result.success) {
        setPhase("setup");
      } else {
        setInstallError(result.error || t("onb.installFailed"));
      }
    } catch (err) {
      unlisten();
      setInstallError(String(err));
    } finally {
      setInstalling(false);
    }
  };

  // ── Unified save: provider + key + soul all at once ──────────────────────
  // Replaces the old 3-step wizard (provider → apiKey → soul). Saves the
  // provider key, writes soul.md, sets personality, then advances to done.
  const saveUnified = async () => {
    setSaving(true);
    setInstallError(null);
    try {
      // 1. Save provider + API key
      const prov = PROVIDERS.setup.find((s) => s.id === selectedProvider);
      const envKey = prov?.envKey || "";
      if (envKey && apiKey) {
        await invoke("save_provider_key_cmd", {
          envKey,
          apiKey,
          provider: prov?.configProvider || selectedProvider,
          model: "",
          baseUrl: prov?.baseUrl || "",
        });
      }
      // 2. Write soul.md + set personality
      const provDesc =
        personalities.find((p) => p.id === chosenPersonality)?.description ||
        "You are a helpful, friendly AI assistant.";
      const soul = [
        "# Soul",
        "",
        provDesc,
        "",
        `Your name is ${agentName || "Штурман"}.`,
        company ? `You assist the team at ${company}.` : "",
        `Always introduce yourself as ${agentName || "Штурман"} and greet the user warmly in Russian by default.`,
        "",
      ]
        .filter(Boolean)
        .join("\n");
      await invoke("write_soul_cmd", { content: soul });
      await invoke("set_personality_cmd", { personality: chosenPersonality });
      setPhase("done");
    } catch (err) {
      setInstallError(String(err));
    } finally {
      setSaving(false);
    }
  };


  return (
    <div className="fixed inset-0 bg-ac-bg flex items-center justify-center p-6">
      <div className="max-w-xl w-full">
        {/* Progress rail (hidden at the choice step) */}
        {phase !== "choice" && (
          <div className="flex items-center gap-2 mb-8">
            {steps.map((s, i) => (
              <div key={s.id} className="flex items-center gap-2 flex-1">
                <div
                  className={`w-6 h-6 rounded-full flex items-center justify-center text-xs font-medium ${
                    i <= currentIndex
                      ? "bg-ac-brand text-white"
                      : "bg-ac-surface text-ac-muted border border-ac-border"
                  }`}
                >
                  {i < currentIndex ? "✓" : i + 1}
                </div>
                {i < steps.length - 1 && (
                  <div className={`flex-1 h-0.5 ${i < currentIndex ? "bg-ac-brand" : "bg-ac-border"}`} />
                )}
              </div>
            ))}
          </div>
        )}

        <div className="bg-ac-surface border border-ac-border rounded-xl p-6">
          {/* ── Choice ─────────────────────────────────────────────────── */}
          {phase === "choice" && (
            <div>
              <h1 className="text-2xl font-bold text-ac-ink mb-2">{t("onb.welcomeTitle")}</h1>
              <p className="text-ac-muted mb-6">{t("onb.choiceSubtitle")}</p>
              <div className="grid grid-cols-1 gap-3">
                <ChoiceCard
                  icon={<Globe className="w-5 h-5" />}
                  title={t("onb.remoteTitle")}
                  desc={t("onb.remoteDesc")}
                  onClick={() => {
                    setPath("remote");
                    // Remote: go straight to unified setup, no local install.
                    setPhase("setup");
                  }}
                />
                <ChoiceCard
                  icon={<Download className="w-5 h-5" />}
                  title={t("onb.installTitle")}
                  desc={t("onb.installDesc")}
                  onClick={() => {
                    setPath("install");
                    setPhase("install");
                    void startInstall();
                  }}
                />
              </div>
            </div>
          )}

          {/* ── Install progress ───────────────────────────────────────── */}
          {phase === "install" && (
            <div>
              <h2 className="text-lg font-semibold text-ac-ink mb-3 flex items-center gap-2">
                {installing ? (
                  <Loader2 className="w-5 h-5 animate-spin text-ac-brand" />
                ) : (
                  <Server className="w-5 h-5 text-ac-brand" />
                )}
                {t("onb.installing")}
              </h2>
              <pre className="bg-ac-bg border border-ac-border rounded-lg p-3 text-[11px] font-mono text-ac-muted h-48 overflow-y-auto whitespace-pre-wrap">
                {installLog.length ? installLog.join("\n") : t("onb.starting")}
              </pre>
              {installError && (
                <p className="text-xs text-ac-red mt-3">{installError}</p>
              )}
              <div className="flex justify-between mt-5">
                <button
                  className="px-4 py-2 text-sm border border-ac-border text-ac-muted rounded-md hover:text-ac-ink"
                  onClick={() => setPhase("choice")}
                >
                  <ChevronLeft className="w-4 h-4 inline mr-1" />
                  {t("btn.back")}
                </button>
                <button
                  className="ac-btn px-4 py-2 text-sm flex items-center gap-1"
                  disabled={installing}
                  onClick={() => setPhase("setup")}
                >
                  {t("onb.continue")}
                  <ChevronRight className="w-4 h-4" />
                </button>
              </div>
            </div>
          )}

          {/* ── Unified Setup: provider + key + soul on one screen ──────── */}
          {/* Replaces the old 3-step wizard (provider → apiKey → soul). */}
          {phase === "setup" && (
            <div>
              <h2 className="text-lg font-semibold text-ac-ink mb-1 flex items-center gap-2">
                <Sparkles className="w-5 h-5 text-ac-brand" />
                {t("onb.setup") || "Setup"}
              </h2>
              <p className="text-sm text-ac-muted mb-4">
                Configure your AI assistant in one step.
              </p>

              {/* Provider selection */}
              <label className="text-xs font-medium text-ac-muted block mb-1.5">
                {t("onb.chooseProvider")}
              </label>
              <div className="space-y-1.5 max-h-40 overflow-y-auto mb-4">
                {PROVIDERS.setup.map((p) => (
                  <button
                    key={p.id}
                    className={`w-full text-left p-2 rounded-lg border transition-colors ${
                      selectedProvider === p.id
                        ? "border-ac-brand bg-ac-brand/10"
                        : "border-ac-border hover:border-ac-muted"
                    }`}
                    onClick={() => setSelectedProvider(p.id)}
                  >
                    <div className="flex items-center justify-between">
                      <span className="font-medium text-sm text-ac-ink">{p.name}</span>
                      {selectedProvider === p.id && (
                        <CheckCircle2 className="w-4 h-4 text-ac-brand" />
                      )}
                    </div>
                    <p className="text-xs text-ac-muted mt-0.5">{p.desc}</p>
                  </button>
                ))}
              </div>

              {/* API Key (only if provider needs one) */}
              {needsKey && (
                <div className="mb-4">
                  <label className="text-xs font-medium text-ac-muted block mb-1.5">
                    {t("onb.enterKey")}
                  </label>
                  <input
                    type="password"
                    className="ac-input w-full px-3 py-2"
                    placeholder={
                      PROVIDERS.setup.find((s) => s.id === selectedProvider)?.placeholder ||
                      t("onb.keyPlaceholder")
                    }
                    value={apiKey}
                    onChange={(e) => setApiKey(e.target.value)}
                  />
                </div>
              )}

              {/* Persona + name */}
              <div className="border-t border-ac-border pt-3 mb-4">
                <label className="text-xs font-medium text-ac-muted block mb-1.5">
                  {t("onb.persona")}
                </label>
                <select
                  className="ac-input w-full mb-3 px-3 py-2"
                  value={chosenPersonality}
                  onChange={(e) => setChosenPersonality(e.target.value)}
                >
                  {personalities.map((p) => (
                    <option key={p.id} value={p.id}>
                      {p.id}
                    </option>
                  ))}
                </select>
                <div className="grid grid-cols-2 gap-3">
                  <div>
                    <label className="text-xs text-ac-muted block mb-1">{t("onb.agentName")}</label>
                    <input
                      className="ac-input w-full px-3 py-2"
                      value={agentName}
                      onChange={(e) => setAgentName(e.target.value)}
                    />
                  </div>
                  <div>
                    <label className="text-xs text-ac-muted block mb-1">{t("onb.company")}</label>
                    <input
                      className="ac-input w-full px-3 py-2"
                      placeholder={t("onb.companyPlaceholder")}
                      value={company}
                      onChange={(e) => setCompany(e.target.value)}
                    />
                  </div>
                </div>
              </div>

              {installError && <p className="text-xs text-ac-red mb-3">{installError}</p>}

              <div className="flex justify-between">
                <button
                  className="px-4 py-2 text-sm border border-ac-border text-ac-muted rounded-md hover:text-ac-ink"
                  onClick={() => (path === "install" ? setPhase("install") : setPhase("choice"))}
                >
                  <ChevronLeft className="w-4 h-4 inline mr-1" />
                  {t("btn.back")}
                </button>
                <button
                  className="ac-btn px-4 py-2 text-sm flex items-center gap-1"
                  disabled={!selectedProvider || (needsKey && !apiKey.trim()) || saving}
                  onClick={() => void saveUnified()}
                >
                  {saving ? t("onb.saving") : t("onb.finish")}
                  <ChevronRight className="w-4 h-4" />
                </button>
              </div>
            </div>
          )}

          {/* ── Done ───────────────────────────────────────────────────── */}
          {phase === "done" && (
            <div className="text-center">
              <CheckCircle2 className="w-12 h-12 text-green-500 mx-auto mb-4" />
              <h2 className="text-lg font-semibold text-ac-ink mb-2">{t("onb.complete")}</h2>
              <p className="text-ac-muted mb-6">{t("onb.completeDesc")}</p>
              <button className="ac-btn px-6 py-2 text-sm" onClick={path === "remote" ? onConnected : onDone}>
                {t("onb.startChatting")}
              </button>
            </div>
          )}
        </div>

        <p className="text-center text-[11px] text-ac-faint mt-4">
          <Mic className="w-3 h-3 inline mr-1" />
          {t("onb.footer")}
        </p>
      </div>
    </div>
  );
}

function ChoiceCard({
  icon,
  title,
  desc,
  onClick,
}: {
  icon: React.ReactNode;
  title: string;
  desc: string;
  onClick: () => void;
}) {
  return (
    <button
      className="w-full text-left p-4 rounded-lg border border-ac-border hover:border-ac-brand hover:bg-ac-brand/5 transition-colors flex items-start gap-3"
      onClick={onClick}
    >
      <div className="w-10 h-10 rounded-lg bg-ac-brand/10 text-ac-brand flex items-center justify-center shrink-0">
        {icon}
      </div>
      <div>
        <div className="font-medium text-ac-ink">{title}</div>
        <div className="text-sm text-ac-muted mt-0.5">{desc}</div>
      </div>
      <ChevronRight className="w-5 h-5 text-ac-muted ml-auto self-center" />
    </button>
  );
}

export default OnboardingScreen;
