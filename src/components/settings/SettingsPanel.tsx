// src/components/settings/SettingsPanel.tsx
// Unified settings panel (ADR-006). Consolidates what used to be scattered
// across the sidebar — models, providers, gateway, tools, diagnose, versions —
// into one place with a left tab rail. Work areas (chat, sessions, kanban,
// memory, skills, schedules) stay in the sidebar; everything else lives here.
//
// Existing tab implementations (GeneralTab, ConnectionTab, TelegramTab,
// ModelsTab, TerminalTab) are kept as-is. New tabs embed the already-working
// screens (GatewayScreen, ToolsScreen, DiagnoseScreen, ProvidersScreen,
// ProfilesScreen, Versions) and an Appearance tab wired to the ThemeProvider.

import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  X,
  Server,
  Globe,
  Shield,
  Moon,
  Sun,
  Send,
  Cpu,
  Terminal as TermIcon,
  Languages,
  Palette,
  Bot,
  Wrench,
  Stethoscope,
  Info,
  KeyRound,
  Sparkles,
  Monitor,
  Mail,
  CheckSquare,
} from "lucide-react";
import { useSettingsStore } from "../../stores/settingsStore";
import { useConnectionStore, type ConnectionMode } from "../../stores/connectionStore";
import { useUIStore } from "../../stores/uiStore";
import { useTranslation } from "../../hooks/useTranslation";
import { useTheme } from "../ThemeProvider";
import { PROVIDERS } from "../../constants";
import type { Lang } from "../../lib/i18n";
import { GatewayScreen } from "../gateway/GatewayScreen";
import { ToolsScreen } from "../tools/ToolsScreen";
import { DiagnoseScreen } from "./DiagnoseScreen";
import ProvidersScreen from "../providers/ProvidersScreen";
import { Versions } from "../Versions";

type SettingsTab =
  | "general"
  | "appearance"
  | "connection"
  | "sources"
  | "soul"
  | "models"
  | "providers"
  | "credentials"
  | "agent"
  | "terminal"
  | "tts"
  | "gateway"
  | "tools"
  | "telegram"
  | "terminal_old"
  | "diagnose"
  | "about";

// ── General tab ────────────────────────────────────────────────────────────
function GeneralTab() {
  const { generalInfo, generalLoading, generalError, loadGeneralInfo } = useSettingsStore();
  const { language, setLanguage, showTokenCounter, setShowTokenCounter } = useUIStore();
  // Theme MUST come from the ThemeProvider (which sets data-theme on <html>);
  // the old code used uiStore.darkMode + settingsStore.setTheme (which only
  // wrote .env) → switching themes did nothing visually.
  const { theme, setTheme } = useTheme();
  const isDark = theme === "dark";
  const { t } = useTranslation();

  useEffect(() => {
    if (!generalInfo) loadGeneralInfo();
  }, [generalInfo, loadGeneralInfo]);

  return (
    <div className="space-y-4">
      {/* Language selector */}
      <div className="mb-4">
        <label className="ac-section-title mb-1.5 block">{t("settings.language")}</label>
        <div className="flex gap-2">
          <button
            onClick={() => setLanguage("ru" as Lang)}
            className={`ac-pill flex items-center gap-1.5 ${language === "ru" ? "active" : ""}`}
          >
            <Languages className="w-3 h-3" />
            {t("language_ru")}
          </button>
          <button
            onClick={() => setLanguage("en" as Lang)}
            className={`ac-pill flex items-center gap-1.5 ${language === "en" ? "active" : ""}`}
          >
            <Languages className="w-3 h-3" />
            {t("language_en")}
          </button>
        </div>
      </div>

      <div className="mb-4">
        <label className="ac-section-title mb-1.5 block">{t("theme_label")}</label>
        <div className="flex gap-2">
          <button
            onClick={() => setTheme("dark")}
            className={`ac-pill flex items-center gap-1.5 ${isDark ? "active" : ""}`}
          >
            <Moon className="w-3 h-3" />
            {t("theme_dark")}
          </button>
          <button
            onClick={() => setTheme("light")}
            className={`ac-pill flex items-center gap-1.5 ${!isDark ? "active" : ""}`}
          >
            <Sun className="w-3 h-3" />
            {t("theme_light")}
          </button>
        </div>
      </div>

      <div>
        <label className="ac-section-title mb-1.5 block">{t("settings.version")}</label>
        {generalLoading ? (
          <p className="text-xs text-ac-muted">{t("loading_dots")}</p>
        ) : generalError ? (
          <p className="text-xs text-ac-red">{generalError}</p>
        ) : generalInfo ? (
          <p className="text-sm font-mono text-ac-ink">{generalInfo.version}</p>
        ) : null}
      </div>

      <div>
        <label className="ac-section-title mb-1.5 block">{t("settings.hermesHome")}</label>
        {generalLoading ? (
          <p className="text-xs text-ac-muted">{t("loading_dots")}</p>
        ) : generalInfo ? (
          <p className="text-sm font-mono text-ac-ink break-all">{generalInfo.hermes_home}</p>
        ) : null}
      </div>

      {/* Token counter toggle */}
      <div className="flex items-center justify-between border-t border-ac-border pt-4">
        <div>
          <label className="ac-section-title block">{t("settings.tokenCounter")}</label>
          <p className="text-[11px] text-ac-muted">{t("settings.tokenCounterHint")}</p>
        </div>
        <Switch checked={showTokenCounter} onChange={setShowTokenCounter} />
      </div>
    </div>
  );
}

// ── Appearance tab (ADR-006) — exposes the 12 themes + radius toggle that
// previously existed in ThemeProvider but were never shown in the UI. ──────
function AppearanceTab() {
  const { theme, setTheme, rounded, setRounded } = useTheme();
  const { t } = useTranslation();

  // Only System / Light / Dark, matching shturman.ai (no multi-theme palette).
  const options = [
    { id: "system", label: t("settings.systemTheme"), icon: Monitor },
    { id: "light", label: t("settings.light"), icon: Sun },
    { id: "dark", label: t("settings.dark"), icon: Moon },
  ];

  return (
    <div className="space-y-5">
      <div>
        <label className="ac-section-title mb-2 block">{t("settings.theme")}</label>
        <div className="space-y-1.5">
          {options.map((opt) => (
            <label
              key={opt.id}
              className={`flex items-center gap-3 px-3 py-2 rounded-md border cursor-pointer transition-colors ${
                theme === opt.id
                  ? "border-ac-brand bg-ac-brand/5"
                  : "border-ac-border hover:border-ac-muted"
              }`}
            >
              <input
                type="radio"
                name="theme"
                className="accent-ac-brand"
                checked={theme === opt.id}
                onChange={() => setTheme(opt.id)}
              />
              <opt.icon className="w-4 h-4 text-ac-muted" />
              <span className="text-sm text-ac-ink">{opt.label}</span>
            </label>
          ))}
        </div>
      </div>

      {/* Radius toggle as an explicit switch */}
      <div className="flex items-center justify-between border-t border-ac-border pt-4">
        <div className="pr-3">
          <label className="ac-section-title block">{t("settings.radius")}</label>
          <p className="text-[11px] text-ac-muted">{t("settings.radiusHint")}</p>
        </div>
        <Switch checked={rounded} onChange={setRounded} />
      </div>
    </div>
  );
}

/** A small accessible on/off switch. */
function Switch({ checked, onChange }: { checked: boolean; onChange: (v: boolean) => void }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      onClick={() => onChange(!checked)}
      className={`relative w-10 h-6 rounded-full transition-colors shrink-0 ${
        checked ? "bg-ac-brand" : "bg-ac-border"
      }`}
    >
      <span
        className={`absolute top-0.5 left-0.5 w-5 h-5 rounded-full bg-white shadow transition-transform ${
          checked ? "translate-x-4" : "translate-x-0"
        }`}
      />
    </button>
  );
}

// ── Soul tab — agent persona/identity, same controls as the onboarding wizard
// but editable any time. ───────────────────────────────────────────────────
function SoulTab() {
  const { t } = useTranslation();
  const [personalities, setPersonalities] = useState<{ id: string; description: string }[]>([]);
  const [active, setActive] = useState("helpful");
  const [soul, setSoul] = useState("");
  const [status, setStatus] = useState("");

  useEffect(() => {
    invoke<{ id: string; description: string }[]>("get_personalities_cmd")
      .then(setPersonalities)
      .catch(() => setPersonalities([{ id: "helpful", description: "You are a helpful, friendly AI assistant." }]));
    invoke<string>("get_personality_cmd").then(setActive).catch(() => {});
    invoke<string>("read_soul_cmd").then(setSoul).catch(() => {});
  }, []);

  const save = async () => {
    try {
      await invoke("write_soul_cmd", { content: soul });
      await invoke("set_personality_cmd", { personality: active });
      setStatus("✓ " + (t("saved")));
      setTimeout(() => setStatus(""), 2000);
    } catch (e: any) {
      setStatus("✗ " + (e?.message || String(e)));
    }
  };

  const reset = async () => {
    try {
      const def = await invoke<string>("reset_soul_cmd");
      setSoul(def);
      setStatus("✓ " + (t("saved")));
      setTimeout(() => setStatus(""), 2000);
    } catch (e: any) {
      setStatus("✗ " + (e?.message || String(e)));
    }
  };

  return (
    <div className="space-y-4">
      <div>
        <label className="ac-section-title mb-1.5 block">{t("onb.persona")}</label>
        <select
          className="ac-input w-full px-3 py-2 text-sm"
          value={active}
          onChange={(e) => setActive(e.target.value)}
        >
          {personalities.map((p) => (
            <option key={p.id} value={p.id}>{p.id}</option>
          ))}
        </select>
        {active && personalities.find((p) => p.id === active) && (
          <p className="text-[11px] text-ac-muted mt-1.5">
            {personalities.find((p) => p.id === active)?.description}
          </p>
        )}
      </div>

      <div>
        <label className="ac-section-title mb-1.5 block">soul.md</label>
        <textarea
          className="ac-input w-full px-3 py-2 text-sm font-mono"
          rows={10}
          value={soul}
          onChange={(e) => setSoul(e.target.value)}
        />
        <p className="text-[11px] text-ac-muted mt-1">
          {t("settings.soulHint")}
        </p>
      </div>

      <div className="flex gap-2">
        <button onClick={save} className="ac-btn px-4 py-2 text-sm">{t("save_button")}</button>
        <button onClick={reset} className="px-4 py-2 text-sm border border-ac-border text-ac-muted rounded-md hover:text-ac-ink">
          {t("btn.refresh")}
        </button>
      </div>
      {status && (
        <p className={`text-xs ${status.startsWith("✓") ? "text-green-400" : "text-ac-red"}`}>{status}</p>
      )}
    </div>
  );
}

// ── Connection tab ─────────────────────────────────────────────────────────
function ConnectionTab() {
  const { config, loading, loadConfig, saveConfig } = useConnectionStore();
  const { t } = useTranslation();

  // Local state for editing; synced from store config on mount
  const [localMode, setLocalMode] = useState<ConnectionMode>("local");
  const [localRemoteUrl, setLocalRemoteUrl] = useState("");
  const [localSshHost, setLocalSshHost] = useState("");
  const [localSshPort, setLocalSshPort] = useState("22");
  const [localSshUser, setLocalSshUser] = useState("");
  const [localSshKey, setLocalSshKey] = useState("");
  const [localSshRemotePort, setLocalSshRemotePort] = useState("");
  const [localSshLocalPort, setLocalSshLocalPort] = useState("");

  // Load config on mount
  useEffect(() => {
    loadConfig();
  }, [loadConfig]);

  // Sync local state when store config loads
  useEffect(() => {
    setLocalMode(config.mode);
    setLocalRemoteUrl(config.remote_url);
    setLocalSshHost(config.ssh.host);
    setLocalSshPort(String(config.ssh.port));
    setLocalSshUser(config.ssh.username);
    setLocalSshKey(config.ssh.key_path);
    setLocalSshRemotePort(String(config.ssh.remote_port));
    setLocalSshLocalPort(String(config.ssh.local_port));
  }, [config]);

  const handleModeChange = (mode: ConnectionMode) => {
    setLocalMode(mode);
    saveConfig({ mode });
  };

  const handleSave = () => {
    saveConfig({
      mode: localMode,
      remote_url: localRemoteUrl,
      ssh: {
        host: localSshHost,
        port: parseInt(localSshPort) || 22,
        username: localSshUser,
        key_path: localSshKey,
        remote_port: parseInt(localSshRemotePort) || 8642,
        local_port: parseInt(localSshLocalPort) || 18642,
      },
    });
  };

  return (
    <div>
      <div className="flex gap-2 mb-4">
        <button
          onClick={() => handleModeChange("local")}
          className={`ac-pill flex items-center gap-1.5 ${localMode === "local" ? "active" : ""}`}
        >
          <Server className="w-3 h-3" />
          {t("connection_local")}
        </button>
        <button
          onClick={() => handleModeChange("remote")}
          className={`ac-pill flex items-center gap-1.5 ${localMode === "remote" ? "active" : ""}`}
        >
          <Globe className="w-3 h-3" />
          {t("connection_remote")}
        </button>
        <button
          onClick={() => handleModeChange("ssh")}
          className={`ac-pill flex items-center gap-1.5 ${localMode === "ssh" ? "active" : ""}`}
        >
          <Shield className="w-3 h-3" />
          {t("ssh")}
        </button>
      </div>

      {localMode === "remote" && (
        <div className="space-y-3">
          <div>
            <label className="text-[11px] text-ac-muted mb-1 block">{t("url_label")}</label>
            <input
              type="text"
              value={localRemoteUrl}
              onChange={(e) => setLocalRemoteUrl(e.target.value)}
              placeholder="https://hermes.example.com:8443"
              className="ac-input w-full px-3 py-2 text-sm"
            />
          </div>
        </div>
      )}

      {localMode === "ssh" && (
        <div className="space-y-3">
          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className="text-[11px] text-ac-muted mb-1 block">{t("host_label")}</label>
              <input type="text" value={localSshHost} onChange={(e) => setLocalSshHost(e.target.value)} placeholder="example.com" className="ac-input w-full px-3 py-2 text-sm" />
            </div>
            <div>
              <label className="text-[11px] text-ac-muted mb-1 block">{t("port_label")}</label>
              <input type="text" value={localSshPort} onChange={(e) => setLocalSshPort(e.target.value)} placeholder="22" className="ac-input w-full px-3 py-2 text-sm" />
            </div>
          </div>
          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className="text-[11px] text-ac-muted mb-1 block">{t("user_label")}</label>
              <input type="text" value={localSshUser} onChange={(e) => setLocalSshUser(e.target.value)} placeholder="user" className="ac-input w-full px-3 py-2 text-sm" />
            </div>
            <div>
              <label className="text-[11px] text-ac-muted mb-1 block">{t("ssh_key_label")}</label>
              <input type="text" value={localSshKey} onChange={(e) => setLocalSshKey(e.target.value)} placeholder="~/.ssh/id_rsa" className="ac-input w-full px-3 py-2 text-sm" />
            </div>
          </div>
          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className="text-[11px] text-ac-muted mb-1 block">{t("remote_port_label")}</label>
              <input type="text" value={localSshRemotePort} onChange={(e) => setLocalSshRemotePort(e.target.value)} placeholder="8642" className="ac-input w-full px-3 py-2 text-sm" />
            </div>
            <div>
              <label className="text-[11px] text-ac-muted mb-1 block">{t("local_port_label")}</label>
              <input type="text" value={localSshLocalPort} onChange={(e) => setLocalSshLocalPort(e.target.value)} placeholder="18642" className="ac-input w-full px-3 py-2 text-sm" />
            </div>
          </div>
        </div>
      )}

      {loading && <p className="text-xs text-ac-muted mt-2">{t("saving_dots")}</p>}

      {(localMode === "remote" || localMode === "ssh") && (
        <div className="flex justify-end mt-4">
          <button onClick={handleSave} className="ac-btn px-4 py-2 text-sm">
            {t("save_button")}
          </button>
        </div>
      )}
    </div>
  );
}

// ── Telegram tab content ──────────────────────────────────────────────────
function TelegramTab() {
  const [botToken, setBotToken] = useState("");
  const [chatId, setChatId] = useState("");
  const [enabled, setEnabled] = useState(false);
  const [status, setStatus] = useState("");
  const [saved, setSaved] = useState(false);
  const { t } = useTranslation();

  const handleSave = async () => {
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("save_telegram_config_cmd", {
        config: { bot_token: botToken, chat_id: chatId, enabled },
      });
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    } catch (e: any) {
      setStatus(e.message || t("save_error"));
    }
  };

  const handleValidate = async () => {
    if (!botToken) { setStatus(t("validation_error")); return; }
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const result = await invoke<{ success: boolean; error?: string }>(
        "validate_telegram_bot_token_cmd",
        { botToken }
      );
      setStatus(result.success ? t("token_valid") : `✗ ${result.error || t("token_invalid")}`);
    } catch (e: any) {
      setStatus(e.message || t("validation_error"));
    }
  };

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <label className="ac-section-title">{t("telegram_enable")}</label>
        <button
          onClick={() => setEnabled(!enabled)}
          className={`ac-pill ${enabled ? "active" : ""}`}
        >
          {enabled ? t("telegram_on") : t("telegram_off")}
        </button>
      </div>

      <div>
        <label className="text-[11px] text-ac-muted mb-1 block">{t("bot_token_label")}</label>
        <input
          type="password"
          value={botToken}
          onChange={(e) => setBotToken(e.target.value)}
          placeholder="123456:ABC-DEF..."
          className="ac-input w-full px-3 py-2 text-sm font-mono"
        />
      </div>

      <div>
        <label className="text-[11px] text-ac-muted mb-1 block">{t("chat_id_label")}</label>
        <input
          type="text"
          value={chatId}
          onChange={(e) => setChatId(e.target.value)}
          placeholder="-1001234567890"
          className="ac-input w-full px-3 py-2 text-sm font-mono"
        />
      </div>

      <div className="flex gap-2">
        <button onClick={handleValidate} className="ac-btn px-3 py-1.5 text-xs">
          {t("validate_token")}
        </button>
        <button onClick={handleSave} className="ac-btn px-3 py-1.5 text-xs">
          {t("save_button")}
        </button>
      </div>

      {status && (
        <p className={`text-xs ${status.startsWith("✓") ? "text-green-400" : "text-ac-red"}`}>
          {status}
        </p>
      )}
      {saved && <p className="text-xs text-green-400">{t("saved")}</p>}
    </div>
  );
}

// ── Models tab content ────────────────────────────────────────────────────
function ModelsTab() {
  const { models, modelConfig, modelsLoading, loadModels, loadModelConfig, addModel, removeModel, setActiveModel } = useSettingsStore();
  const { t } = useTranslation();

  // Add model form state
  const [showAddForm, setShowAddForm] = useState(false);
  const [newName, setNewName] = useState("");
  const [newProvider, setNewProvider] = useState("openrouter");
  const [newModel, setNewModel] = useState("");
  const [newBaseUrl, setNewBaseUrl] = useState("https://openrouter.ai/api/v1");
  const [addStatus, setAddStatus] = useState("");
  // Available models from gateway /v1/models
  const [apiModels, setApiModels] = useState<string[]>([]);
  const [apiModelsLoading, setApiModelsLoading] = useState(false);

  // Load models on mount + fetch available models from gateway
  useEffect(() => {
    loadModels();
    loadModelConfig();
    setApiModelsLoading(true);
    invoke<string[]>("list_models_api_cmd").then(setApiModels).catch(() => setApiModels([])).finally(() => setApiModelsLoading(false));
  }, [loadModels, loadModelConfig]);

  const handleAddModel = async () => {
    if (!newName.trim() || !newModel.trim()) {
      setAddStatus(t("fill_name_and_model"));
      return;
    }
    const result = await addModel(newName.trim(), newProvider, newModel.trim(), newBaseUrl.trim());
    if (result) {
      setAddStatus(`✓ ${t("models.added")} "${result.name}" ${t("models.added2")}`);
      setShowAddForm(false);
      setNewName("");
      setNewModel("");
    } else {
      setAddStatus(t("model_add_error"));
    }
  };

  const handleRemoveModel = async (id: string, name: string) => {
    const ok = await removeModel(id);
    if (ok) {
      setAddStatus(`✓ ${t("models.removed")} "${name}" ${t("models.removed2")}`);
    } else {
      setAddStatus(t("model_delete_error"));
    }
  };

  const handleSetActive = async (provider: string, model: string, baseUrl: string) => {
    const ok = await setActiveModel(provider, model, baseUrl);
    if (ok) {
      setAddStatus(`✓ ${t("models.active")}: ${provider}/${model}`);
    }
  };

  return (
    <div className="space-y-4">
      <div>
        <div className="flex items-center justify-between mb-3">
          <label className="ac-section-title">{t("saved_models_title")}</label>
          <button
            onClick={() => setShowAddForm(!showAddForm)}
            className="ac-btn px-3 py-1 text-xs"
          >
            {showAddForm ? t("cancel_add") : t("add_model")}
          </button>
        </div>

        {showAddForm && (
          <div className="border border-ac-border rounded p-3 space-y-3 mb-3">
            <div>
              <label className="text-[11px] text-ac-muted mb-1 block">{t("model_name_label")}</label>
              <input
                type="text"
                value={newName}
                onChange={(e) => setNewName(e.target.value)}
                placeholder={t("model_name_placeholder")}
                className="ac-input w-full px-3 py-2 text-sm"
              />
            </div>
            <div>
              <label className="text-[11px] text-ac-muted mb-1 block">{t("provider_select_label")}</label>
              <select
                value={newProvider}
                onChange={(e) => setNewProvider(e.target.value)}
                className="ac-input w-full px-3 py-2 text-sm"
              >
                {PROVIDERS.options.map((p) => (
                  <option key={p.value} value={p.value}>{p.label}</option>
                ))}
              </select>
            </div>
            <div>
              <label className="text-[11px] text-ac-muted mb-1 block">{t("model_field_label")}</label>
              {apiModels.length > 0 && (
                <select
                  className="ac-input w-full px-3 py-2 text-sm mb-1.5 font-mono"
                  value=""
                  onChange={(e) => e.target.value && setNewModel(e.target.value)}
                >
                  <option value="">{apiModelsLoading ? "..." : t("settings.pickModel")}</option>
                  {apiModels.map((m) => <option key={m} value={m}>{m}</option>)}
                </select>
              )}
              <input
                type="text"
                value={newModel}
                onChange={(e) => setNewModel(e.target.value)}
                placeholder={t("model_placeholder")}
                className="ac-input w-full px-3 py-2 text-sm font-mono"
              />
            </div>
            <div>
              <label className="text-[11px] text-ac-muted mb-1 block">{t("base_url_label")}</label>
              <input
                type="text"
                value={newBaseUrl}
                onChange={(e) => setNewBaseUrl(e.target.value)}
                className="ac-input w-full px-3 py-2 text-sm font-mono"
              />
            </div>
            <button onClick={handleAddModel} className="ac-btn px-3 py-1.5 text-xs">
              {t("save_model")}
            </button>
            {addStatus && (
              <p className={`text-xs ${addStatus.startsWith("✓") ? "text-green-400" : "text-ac-red"}`}>
                {addStatus}
              </p>
            )}
          </div>
        )}

        {modelsLoading ? (
          <p className="text-xs text-ac-muted">{t("model_loading")}</p>
        ) : models.length === 0 ? (
          <p className="text-xs text-ac-muted">{t("no_saved_models")}</p>
        ) : (
          <div className="space-y-2 max-h-60 overflow-y-auto">
            {models.map((m) => {
              const isActive =
                modelConfig?.provider === m.provider &&
                modelConfig?.model === m.model &&
                modelConfig?.base_url === m.base_url;

              return (
                <div
                  key={m.id}
                  className={`flex items-center justify-between px-3 py-2 rounded text-sm ${
                    isActive ? "bg-ac-brand/10 border border-ac-brand/30" : "bg-ac-bg border border-ac-border"
                  }`}
                >
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2">
                      <span className="text-ac-ink font-medium truncate">{m.name}</span>
                      {isActive && (
                        <span className="text-[10px] text-ac-brand px-1.5 py-0.5 rounded bg-ac-brand/20">{t("active_badge_model")}</span>
                      )}
                    </div>
                    <div className="text-[11px] text-ac-muted font-mono truncate">
                      {m.provider}/{m.model}
                    </div>
                  </div>
                  <div className="flex items-center gap-1 shrink-0 ml-2">
                    {!isActive && (
                      <button
                        onClick={() => handleSetActive(m.provider, m.model, m.base_url)}
                        className="text-[11px] text-ac-brand hover:text-ac-brand/80 px-2 py-1"
                        title={t("make_active")}
                      >
                        {t("make_active")}
                      </button>
                    )}
                    <button
                      onClick={() => handleRemoveModel(m.id, m.name)}
                      className="text-[11px] text-ac-red hover:text-red-300 px-2 py-1"
                      title={t("delete_model")}
                    >
                      ✕
                    </button>
                  </div>
                </div>
              );
            })}
          </div>
        )}

        {addStatus && !showAddForm && (
          <p className={`text-xs mt-2 ${addStatus.startsWith("✓") ? "text-green-400" : "text-ac-red"}`}>
            {addStatus}
          </p>
        )}
      </div>
    </div>
  );
}

// ── Terminal tab content ──────────────────────────────────────────────────
function TerminalTab() {
  const [cwd, setCwd] = useState("~");
  const [status, setStatus] = useState("");
  const { t } = useTranslation();

  const handleOpen = async () => {
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const result = await invoke<{ success: boolean; error?: string }>("open_terminal_cmd", { cwd });
      setStatus(result.success ? t("terminal_opened") : `✗ ${result.error}`);
    } catch (e: any) {
      setStatus(e.message || t("error_generic"));
    }
  };

  return (
    <div className="space-y-4">
      <div>
        <label className="text-[11px] text-ac-muted mb-1 block">{t("terminal_cwd_label")}</label>
        <input
          type="text"
          value={cwd}
          onChange={(e) => setCwd(e.target.value)}
          placeholder={t("terminal_cwd_placeholder")}
          className="ac-input w-full px-3 py-2 text-sm font-mono"
        />
      </div>

      <button onClick={handleOpen} className="ac-btn px-4 py-2 text-sm">
        {t("open_terminal")}
      </button>

      {status && (
        <p className={`text-xs ${status.startsWith("✓") ? "text-green-400" : "text-ac-red"}`}>
          {status}
        </p>
      )}
    </div>
  );
}

// ── Credentials tab — credential pool synced with Hermes auth.json ─────────
function CredentialsTab() {
  const { t } = useTranslation();
  const [pool, setPool] = useState<Record<string, Array<{ id?: string; label?: string; source?: string; base_url?: string; last_status?: string }>>>({});
  const [provider, setProvider] = useState("groq");
  const [label, setLabel] = useState("GROQ_API_KEY");
  const [apiKey, setApiKey] = useState("");
  const [status, setStatus] = useState("");

  const load = useCallback(async () => {
    try {
      const r = await invoke<Record<string, Array<{ id?: string; label?: string; source?: string; base_url?: string; last_status?: string }>>>("get_credential_pool_cmd");
      setPool(r ?? {});
    } catch (e) {
      console.error("credential pool load failed", e);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const add = async () => {
    if (!apiKey.trim() || !provider.trim()) return;
    try {
      await invoke("add_credential_pool_entry_cmd", {
        provider: provider.trim(),
        key: apiKey.trim(),
        label: label.trim() || `${provider.toUpperCase()}_API_KEY`,
      });
      setStatus("✓ " + t("saved"));
      setApiKey("");
      void load();
    } catch (e: any) {
      setStatus("✗ " + (e?.message || String(e)));
    }
  };

  const removeCred = async (prov: string, entryId?: string) => {
    if (!entryId) return;
    try {
      await invoke("remove_credential_pool_entry_cmd", { provider: prov, entryId });
      void load();
    } catch (e: any) {
      setStatus("✗ " + (e?.message || String(e)));
    }
  };

  return (
    <div className="space-y-4">
      <p className="text-[11px] text-ac-muted">{t("settings.credentialsHint")}</p>

      {/* Existing credentials */}
      <div>
        <label className="ac-section-title mb-2 block">{t("settings.credentialsList")}</label>
        {Object.keys(pool).length === 0 ? (
          <p className="text-xs text-ac-muted">{t("settings.credentialsEmpty")}</p>
        ) : (
          <div className="space-y-3">
            {Object.entries(pool).map(([prov, entries]) => (
              <div key={prov}>
                <p className="text-[11px] font-semibold text-ac-ink mb-1">{prov}</p>
                <div className="space-y-1">
                  {entries.map((e, i) => (
                    <div key={e.id || i} className="group flex items-center gap-2 px-2.5 py-1.5 rounded-md bg-ac-bg border border-ac-border text-xs">
                      <KeyRound className="w-3 h-3 text-ac-muted shrink-0" />
                      <span className="text-ac-ink-2 truncate">{e.label || e.id || prov}</span>
                      <span className="text-ac-faint">{e.source || "manual"}</span>
                      {e.last_status && (
                        <span className={`ml-auto ${e.last_status === "exhausted" ? "text-ac-yellow" : "text-ac-muted"}`}>
                          {e.last_status}
                        </span>
                      )}
                      <button
                        onClick={() => void removeCred(prov, e.id)}
                        className="opacity-0 group-hover:opacity-100 text-ac-faint hover:text-ac-red"
                        title={t("btn.delete")}
                      >
                        <X className="w-3 h-3" />
                      </button>
                    </div>
                  ))}
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Add new credential */}
      <div className="border-t border-ac-border pt-4">
        <label className="ac-section-title mb-2 block">{t("settings.credentialsAdd")}</label>
        <div className="grid grid-cols-2 gap-2 mb-2">
          <div>
            <label className="text-[11px] text-ac-muted block mb-1">{t("settings.provider")}</label>
            <select className="ac-input w-full px-2.5 py-1.5 text-sm" value={provider}
              onChange={(e) => { setProvider(e.target.value); setLabel(`${e.target.value.toUpperCase()}_API_KEY`); }}>
              {PROVIDERS.options.map((p) => (
                <option key={p.value} value={p.value}>{p.label}</option>
              ))}
            </select>
          </div>
          <div>
            <label className="text-[11px] text-ac-muted block mb-1">{t("settings.envVar")}</label>
            <input className="ac-input w-full px-2.5 py-1.5 text-sm font-mono" value={label}
              onChange={(e) => setLabel(e.target.value)} placeholder="GROQ_API_KEY" />
          </div>
        </div>
        <label className="text-[11px] text-ac-muted block mb-1">{t("settings.apiKey")}</label>
        <input type="password" className="ac-input w-full px-2.5 py-1.5 text-sm font-mono mb-2"
          value={apiKey} onChange={(e) => setApiKey(e.target.value)} placeholder="sk-..." />
        <button onClick={() => void add()} disabled={!apiKey.trim()}
          className="ac-btn px-4 py-2 text-sm disabled:opacity-40">
          {t("settings.credentialsSave")}
        </button>
        {status && (
          <p className={`text-xs mt-2 ${status.startsWith("✓") ? "text-green-400" : "text-ac-red"}`}>{status}</p>
        )}
      </div>
    </div>
  );
}

// ── Hermes section tab — generic editor for a config.yaml section ──────────
// Reads a top-level block (agent/terminal/tts/...) via get_config_section_cmd
// and writes scalar values back via set_config_yaml_value_cmd. This gives
// two-way sync: changes in Settings write to Hermes's config.yaml.
function HermesSectionTab({ section, fields }: { section: string; fields: { key: string; label: string; type?: "text" | "number" | "bool" }[] }) {
  const { t } = useTranslation();
  const [vals, setVals] = useState<Record<string, string>>({});
  const [status, setStatus] = useState("");

  useEffect(() => {
    invoke<Record<string, unknown>>("get_config_section_cmd", { section })
      .then((data) => {
        const v: Record<string, string> = {};
        for (const f of fields) {
          const raw = data[f.key];
          v[f.key] = raw == null ? "" : String(raw);
        }
        setVals(v);
      })
      .catch(() => {});
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [section]);

  const save = async (key: string, value: string) => {
    try {
      await invoke("set_config_yaml_value_cmd", { block: section, key, value });
      setStatus("✓ " + t("saved"));
      setTimeout(() => setStatus(""), 2000);
    } catch (e: any) {
      setStatus("✗ " + (e?.message || String(e)));
    }
  };

  return (
    <div className="space-y-3">
      {fields.map((f) => (
        <div key={f.key}>
          <label className="text-[11px] text-ac-muted block mb-1">{f.label}</label>
          <div className="flex gap-2">
            <input
              className="ac-input flex-1 px-3 py-2 text-sm font-mono"
              type={f.type === "number" ? "number" : "text"}
              value={vals[f.key] ?? ""}
              onChange={(e) => setVals((p) => ({ ...p, [f.key]: e.target.value }))}
            />
            <button onClick={() => void save(f.key, vals[f.key] ?? "")} className="ac-btn px-3 py-2 text-xs">
              {t("btn.save")}
            </button>
          </div>
        </div>
      ))}
      {status && <p className={`text-xs ${status.startsWith("✓") ? "text-green-400" : "text-ac-red"}`}>{status}</p>}
    </div>
  );
}

// ── Sources tab — configure Telegram + Email connectors (write to Hermes .env)
// Each connector is a Hermes platform plugin. The desktop writes the env vars
// the plugin expects; Hermes picks them up on next gateway restart.
function SourcesTab() {
  const { t } = useTranslation();
  const [vals, setVals] = useState<Record<string, string>>({});
  const [status, setStatus] = useState("");
  const [showTg, setShowTg] = useState(false);
  const [showEmail, setShowEmail] = useState(false);

  // Load current values from Hermes .env via get_env_cmd.
  useEffect(() => {
    const keys = [
      "TELEGRAM_BOT_TOKEN", "TELEGRAM_ALLOWED_USERS", "TELEGRAM_HOME_CHANNEL",
      "EMAIL_ADDRESS", "EMAIL_PASSWORD", "EMAIL_SMTP_HOST", "EMAIL_SMTP_PORT", "EMAIL_IMAP_HOST",
    ];
    Promise.all(keys.map((k) => invoke<string | null>("get_env_cmd", { key: k, profile: null }).catch(() => null)))
      .then((results) => {
        const v: Record<string, string> = {};
        keys.forEach((k, i) => { v[k] = results[i] || ""; });
        setVals(v);
      });
  }, []);

  const save = async (key: string) => {
    try {
      await invoke("set_env_cmd", { key, value: vals[key] || "", profile: null });
      setStatus("✓ " + t("saved"));
      setTimeout(() => setStatus(""), 2000);
    } catch (e: any) {
      setStatus("✗ " + (e?.message || String(e)));
    }
  };

  const field = (key: string, label: string, ph: string, isPassword = false) => (
    <div key={key}>
      <label className="text-[11px] text-ac-muted block mb-1">{label}</label>
      <div className="flex gap-2">
        <input
          type={isPassword ? "password" : "text"}
          className="ac-input flex-1 px-3 py-2 text-sm font-mono"
          placeholder={ph}
          value={vals[key] ?? ""}
          onChange={(e) => setVals((p) => ({ ...p, [key]: e.target.value }))}
        />
        <button onClick={() => void save(key)} className="ac-btn px-3 py-2 text-xs">{t("btn.save")}</button>
      </div>
    </div>
  );

  const tgConfigured = vals["TELEGRAM_BOT_TOKEN"];
  const emailConfigured = vals["EMAIL_ADDRESS"] && vals["EMAIL_PASSWORD"];

  return (
    <div className="space-y-4">
      <p className="text-[11px] text-ac-muted">{t("sources.hint")}</p>

      {/* Telegram */}
      <div className="border border-ac-border rounded-lg overflow-hidden">
        <button
          onClick={() => setShowTg(!showTg)}
          className="w-full flex items-center gap-3 px-4 py-3 hover:bg-ac-surface transition-colors"
        >
          <Send className="w-4 h-4 text-[#0088cc]" />
          <span className="flex-1 text-left text-sm font-medium text-ac-ink">Telegram</span>
          <span className={`text-[10px] px-2 py-0.5 rounded-full ${tgConfigured ? "bg-green-500/15 text-green-500" : "bg-ac-surface-2 text-ac-muted"}`}>
            {tgConfigured ? "✓ " + t("sources.configured") : t("sources.notConfigured")}
          </span>
        </button>
        {showTg && (
          <div className="p-4 border-t border-ac-border space-y-3">
            {field("TELEGRAM_BOT_TOKEN", "Bot Token (@BotFather)", "123456:ABC-DEF...", true)}
            {field("TELEGRAM_ALLOWED_USERS", t("sources.allowedUsers"), "user_id1, user_id2")}
            {field("TELEGRAM_HOME_CHANNEL", t("sources.homeChannel"), "-1001234567890")}
          </div>
        )}
      </div>

      {/* Email (IMAP + SMTP) */}
      <div className="border border-ac-border rounded-lg overflow-hidden">
        <button
          onClick={() => setShowEmail(!showEmail)}
          className="w-full flex items-center gap-3 px-4 py-3 hover:bg-ac-surface transition-colors"
        >
          <Mail className="w-4 h-4 text-[#ea4335]" />
          <span className="flex-1 text-left text-sm font-medium text-ac-ink">Email (IMAP/SMTP)</span>
          <span className={`text-[10px] px-2 py-0.5 rounded-full ${emailConfigured ? "bg-green-500/15 text-green-500" : "bg-ac-surface-2 text-ac-muted"}`}>
            {emailConfigured ? "✓ " + t("sources.configured") : t("sources.notConfigured")}
          </span>
        </button>
        {showEmail && (
          <div className="p-4 border-t border-ac-border space-y-3">
            {field("EMAIL_ADDRESS", t("sources.emailAddress"), "you@example.com")}
            {field("EMAIL_PASSWORD", t("sources.emailPassword"), "app password", true)}
            {field("EMAIL_SMTP_HOST", "SMTP host", "smtp.gmail.com")}
            {field("EMAIL_SMTP_PORT", "SMTP port", "587")}
            {field("EMAIL_IMAP_HOST", "IMAP host", "imap.gmail.com")}
          </div>
        )}
      </div>

      {/* Jira / MCP note */}
      <div className="border border-ac-border rounded-lg p-4 bg-ac-surface">
        <div className="flex items-center gap-3 mb-1">
          <CheckSquare className="w-4 h-4 text-[#0052cc]" />
          <span className="text-sm font-medium text-ac-ink">Jira / MCP / REST API</span>
          <span className="text-[10px] px-2 py-0.5 rounded-full bg-ac-surface-2 text-ac-muted">Hermes tools</span>
        </div>
        <p className="text-[11px] text-ac-muted ml-7">{t("sources.toolsHint")}</p>
      </div>

      {status && <p className={`text-xs ${status.startsWith("✓") ? "text-green-400" : "text-ac-red"}`}>{status}</p>}
    </div>
  );
}

// ── Main Settings Panel ───────────────────────────────────────────────────
export function SettingsPanel({ onClose }: { onClose: () => void }) {
  const [activeTab, setActiveTab] = useState<SettingsTab>("general");
  const { t } = useTranslation();

  const tabs: { id: SettingsTab; label: string; icon: typeof Sun }[] = [
    { id: "general", label: t("settings_general"), icon: Sun },
    { id: "appearance", label: t("settings.appearance"), icon: Palette },
    { id: "soul", label: t("settings.soul"), icon: Sparkles },
    { id: "sources", label: t("sources.title"), icon: Send },
    { id: "connection", label: t("settings_connection"), icon: Globe },
    { id: "models", label: t("settings_models"), icon: Cpu },
    { id: "providers", label: t("settings.providers"), icon: KeyRound },
    { id: "credentials", label: t("settings.credentials"), icon: KeyRound },
    { id: "agent", label: t("settings.agent"), icon: Cpu },
    { id: "tts", label: "TTS", icon: Send },
    { id: "gateway", label: t("settings.gateway"), icon: Bot },
    { id: "tools", label: t("settings.tools"), icon: Wrench },
    { id: "telegram", label: t("settings_telegram"), icon: Send },
    { id: "terminal", label: t("settings_terminal"), icon: TermIcon },
    { id: "diagnose", label: t("settings.diagnose"), icon: Stethoscope },
    { id: "about", label: t("settings.about"), icon: Info },
  ];

  return (
    <div className="ac-modal-overlay">
      <div className="ac-modal" style={{ maxWidth: 820, height: 600, display: "flex", flexDirection: "column" }}>
        <div className="flex items-center justify-between mb-4">
          <h2 className="text-base font-semibold text-ac-ink">{t("settings.title")}</h2>
          <button
            onClick={onClose}
            className="text-ac-muted hover:text-ac-ink transition-colors"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Two-column layout: a left tab rail (so all 11 tabs fit without
            horizontal scrolling) + the active panel on the right. */}
        <div className="flex gap-4 flex-1 min-h-0">
          {/* Tab rail */}
          <nav className="w-40 shrink-0 border-r border-ac-border pr-2 overflow-y-auto">
            {tabs.map((tab) => (
              <button
                key={tab.id}
                onClick={() => setActiveTab(tab.id)}
                className={`w-full flex items-center gap-2 px-2.5 py-2 text-xs rounded-md mb-0.5 transition-colors text-left ${
                  activeTab === tab.id
                    ? "bg-ac-brand/10 text-ac-brand"
                    : "text-ac-muted hover:text-ac-ink hover:bg-ac-surface"
                }`}
              >
                <tab.icon className="w-3.5 h-3.5 shrink-0" />
                <span className="truncate">{tab.label}</span>
              </button>
            ))}
          </nav>

          {/* Panel content */}
          <div className="flex-1 min-h-0 overflow-y-auto pr-1">
            {activeTab === "general" && <GeneralTab />}
            {activeTab === "appearance" && <AppearanceTab />}
            {activeTab === "connection" && <ConnectionTab />}
            {activeTab === "sources" && <SourcesTab />}
            {activeTab === "soul" && <SoulTab />}
            {activeTab === "models" && <ModelsTab />}
            {activeTab === "providers" && (
              <div className="-m-4"><ProvidersScreen /></div>
            )}
            {activeTab === "credentials" && <CredentialsTab />}
            {activeTab === "agent" && (
              <HermesSectionTab section="agent" fields={[
                { key: "max_turns", label: "Max turns", type: "number" },
                { key: "reasoning_effort", label: "Reasoning effort" },
                { key: "verbose", label: "Verbose" },
              ]} />
            )}
            {activeTab === "tts" && (
              <HermesSectionTab section="tts" fields={[
                { key: "provider", label: "TTS Provider" },
              ]} />
            )}
            {activeTab === "gateway" && (
              <div className="-m-4"><GatewayScreen /></div>
            )}
            {activeTab === "tools" && (
              <div className="-m-4"><ToolsScreen /></div>
            )}
            {activeTab === "telegram" && <TelegramTab />}
            {activeTab === "terminal" && <TerminalTab />}
            {activeTab === "diagnose" && (
              <div className="-m-4"><DiagnoseScreen /></div>
            )}
            {activeTab === "about" && (
              <div className="-m-4"><Versions /></div>
            )}
          </div>
        </div>

        <div className="flex justify-end gap-2 pt-4 mt-4 border-t border-ac-border">
          <button
            onClick={onClose}
            className="px-4 py-2 text-sm border border-ac-border text-ac-muted hover:text-ac-ink transition-colors rounded-md"
          >
            {t("close")}
          </button>
        </div>
      </div>
    </div>
  );
}
