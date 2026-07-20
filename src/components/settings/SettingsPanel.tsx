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
  Briefcase,
  Shuffle,
  Plus,
  Trash2,
  Edit3,
  Loader,
  Zap,
  Clock,
  BookOpen,
  Eye,
  EyeOff,
  ChevronDown,
  ChevronRight,
  Save,
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
  | "about"
  | "skills"
  | "cron"
  | "mcp";

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
        <p className={`text-xs ${status.startsWith("✓") ? "text-ac-green" : "text-ac-red"}`}>{status}</p>
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
        <p className={`text-xs ${status.startsWith("✓") ? "text-ac-green" : "text-ac-red"}`}>
          {status}
        </p>
      )}
      {saved && <p className="text-xs text-ac-green">{t("saved")}</p>}
    </div>
  );
}

// ── Models tab content ────────────────────────────────────────────────────
function ModelsTab() {
  const { models, modelConfig, modelsLoading, loadModels, loadModelConfig, addModel, removeModel, setActiveModel, saveProxyConfig } = useSettingsStore();
  const { t } = useTranslation();

  // Add model form state
  const [showAddForm, setShowAddForm] = useState(false);
  const [newName, setNewName] = useState("");
  const [newProvider, setNewProvider] = useState("openrouter");
  const [newModel, setNewModel] = useState("");
  const [newBaseUrl, setNewBaseUrl] = useState("https://openrouter.ai/api/v1");
  const [addStatus, setAddStatus] = useState("");
  // Proxy settings (SOCKS5) — applied to model API calls (Remote/Ssh modes)
  const [proxyEnabled, setProxyEnabled] = useState(true);
  const [proxyUrl, setProxyUrl] = useState("http://127.0.0.1:8080");
  const [proxyStatus, setProxyStatus] = useState("");
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

  // Sync proxy state from loaded model config
  useEffect(() => {
    if (modelConfig?.proxy) {
      setProxyEnabled(modelConfig.proxy.use_proxy);
      setProxyUrl(modelConfig.proxy.proxy_url || "http://127.0.0.1:8080");
    }
  }, [modelConfig]);

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

  const handleSaveProxy = async () => {
    setProxyStatus(t("saving_dots") || "…");
    const ok = await saveProxyConfig(proxyEnabled, proxyUrl.trim());
    setProxyStatus(ok ? `✓ ${t("proxy.saved") || "Proxy saved"}` : `✗ ${t("proxy.saveError") || "Save failed"}`);
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
              <p className={`text-xs ${addStatus.startsWith("✓") ? "text-ac-green" : "text-ac-red"}`}>
                {addStatus}
              </p>
            )}
          </div>
        )}

        {/* Default Model Selection */}
        <div className="mb-4 p-3 rounded-lg border border-ac-border bg-ac-surface">
          <label className="text-xs font-medium text-ac-ink mb-2 block">{t("settings.defaultModel")}</label>
          <select
            className="ac-input w-full px-3 py-2 text-sm"
            value={modelConfig?.model || ""}
            onChange={(e) => {
              if (e.target.value) {
                const selected = models.find(m => `${m.provider}/${m.model}` === e.target.value);
                if (selected) setActiveModel(selected.provider, selected.model, selected.base_url);
              }
            }}
            disabled={models.length === 0}
          >
            <option value="">{t("settings.noDefaultModel")}</option>
            {models.map((m) => (
              <option key={m.id} value={`${m.provider}/${m.model}`}>
                {m.name} ({m.provider}/{m.model})
              </option>
            ))}
          </select>
          <p className="text-[11px] text-ac-muted mt-1">{t("settings.defaultModelHint")}</p>
        </div>

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
          <p className={`text-xs mt-2 ${addStatus.startsWith("✓") ? "text-ac-green" : "text-ac-red"}`}>
            {addStatus}
          </p>
        )}

        {/* Proxy (SOCKS5) — applied to model API calls in Remote/Ssh modes */}
        <div className="mt-4 p-3 rounded-lg border border-ac-border bg-ac-surface">
          <div className="flex items-center gap-2 mb-2">
            <Globe className="w-3.5 h-3.5 text-ac-brand" />
            <label className="text-xs font-medium text-ac-ink">{t("proxy.title") || "SOCKS5 Proxy"}</label>
          </div>
          <p className="text-[11px] text-ac-muted mb-2">{t("proxy.hint") || "Used for OpenAI-compatible model APIs (OpenRouter, etc.) in Remote/Ssh connection modes."}</p>
          <label className="flex items-center gap-2 cursor-pointer mb-2">
            <input
              type="checkbox"
              checked={proxyEnabled}
              onChange={(e) => setProxyEnabled(e.target.checked)}
              className="accent-ac-brand"
            />
            <span className="text-sm text-ac-ink">{t("proxy.useProxy") || "Use proxy"}</span>
          </label>
          <input
            type="text"
            value={proxyUrl}
            disabled={!proxyEnabled}
            onChange={(e) => setProxyUrl(e.target.value)}
            placeholder="http://127.0.0.1:8080"
            className="ac-input w-full px-3 py-2 text-sm font-mono mb-2"
          />
          <div className="flex items-center gap-2">
            <button onClick={handleSaveProxy} className="ac-btn px-3 py-1.5 text-xs">
              {t("proxy.save") || "Save proxy"}
            </button>
            {proxyStatus && (
              <span className={`text-xs ${proxyStatus.startsWith("✓") ? "text-ac-green" : "text-ac-red"}`}>{proxyStatus}</span>
            )}
          </div>
        </div>

        {/* Multi-model routing (practice 1): assign models to task types
            so routine work runs on cheaper models and complex analysis on
            flagship ones. */}
        <ModelRoutingSection models={models} />
      </div>
    </div>
  );
}

/// Compact multi-model routing editor: lets the user assign a model per task
/// type (routine / complex / vision). Based on GPT-5.6 guide practice 1 —
/// "different models for different request types reduce cost without quality loss."
function ModelRoutingSection({ models }: { models: any[] }) {
  const { t } = useTranslation();
  const [routing, setRouting] = useState<Record<string, string>>({});
  const [status, setStatus] = useState("");

  // Load current routing from model config.
  useEffect(() => {
    invoke<any>("get_model_config_cmd", { profile: null })
      .then((config) => {
        if (config?.model_routing) {
          setRouting(config.model_routing);
        }
      })
      .catch(() => {});
  }, []);

  const TASK_TYPES = [
    { key: "routine", label: t("routing.routine") },
    { key: "complex", label: t("routing.complex") },
    { key: "vision", label: t("routing.vision") },
  ];

  const handleSave = async () => {
    // Only save non-empty entries.
    const filtered: Record<string, string> = {};
    for (const [k, v] of Object.entries(routing)) {
      if (v.trim()) filtered[k] = v.trim();
    }
    try {
      await invoke("set_model_routing_cmd", { routing: filtered, profile: null });
      setStatus("✓ " + t("routing.saved"));
      setTimeout(() => setStatus(""), 2000);
    } catch (e: any) {
      setStatus("✗ " + (e?.message || String(e)));
    }
  };

  return (
    <div className="mt-4 p-3 rounded-lg border border-ac-border bg-ac-surface">
      <div className="flex items-center gap-2 mb-2">
        <Shuffle className="w-3.5 h-3.5 text-ac-brand" />
        <label className="text-xs font-medium text-ac-ink">
          {t("routing.title")}
        </label>
      </div>
      <p className="text-[11px] text-ac-muted mb-3">
        {t("routing.hint")}
      </p>
      {TASK_TYPES.map((tt) => (
        <div key={tt.key} className="mb-2">
          <label className="text-[11px] text-ac-muted block mb-0.5">{tt.label}</label>
          <select
            value={routing[tt.key] || ""}
            onChange={(e) => setRouting((r) => ({ ...r, [tt.key]: e.target.value }))}
            className="ac-input w-full px-2 py-1.5 text-xs"
          >
            <option value="">— {t("routing.default")} —</option>
            {models.map((m: any) => (
              <option key={m.id} value={m.model}>
                {m.name} ({m.provider}/{m.model})
              </option>
            ))}
          </select>
        </div>
      ))}
      <div className="flex items-center gap-2 mt-2">
        <button onClick={handleSave} className="ac-btn px-3 py-1.5 text-xs">
          {t("routing.save")}
        </button>
        {status && (
          <span className={`text-xs ${status.startsWith("✓") ? "text-ac-green" : "text-ac-red"}`}>
            {status}
          </span>
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
        <p className={`text-xs ${status.startsWith("✓") ? "text-ac-green" : "text-ac-red"}`}>
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
          <p className={`text-xs mt-2 ${status.startsWith("✓") ? "text-ac-green" : "text-ac-red"}`}>{status}</p>
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
      {status && <p className={`text-xs ${status.startsWith("✓") ? "text-ac-green" : "text-ac-red"}`}>{status}</p>}
    </div>
  );
}

// ── Sources tab — multiple Telegram/Email/Jira connectors
// Each connector is a separate instance. The desktop writes to sources.json
// and can apply to Hermes .env for backwards compatibility.
interface TelegramSource {
  id: string;
  name: string;
  bot_token: string;
  chat_id: string;
  allowed_users: string;
  home_channel: string;
  enabled: boolean;
  use_proxy?: boolean;
  proxy_url?: string;
}
interface EmailSource {
  id: string;
  name: string;
  address: string;
  password: string;
  smtp_host: string;
  smtp_port: number;
  imap_host: string;
  imap_port?: number;
  use_ssl?: boolean;
  enabled: boolean;
  use_proxy?: boolean;
  proxy_url?: string;
}
interface JiraSource {
  id: string;
  name: string;
  url: string;
  username: string;
  api_token: string;
  project_key: string;
  enabled: boolean;
  use_proxy?: boolean;
  proxy_url?: string;
}
interface BitrixSource {
  id: string;
  name: string;
  webhook_url: string;
  user_id: string;
  enabled: boolean;
  use_proxy?: boolean;
  proxy_url?: string;
}
interface SourcesConfig {
  telegram: TelegramSource[];
  email: EmailSource[];
  jira: JiraSource[];
  bitrix: BitrixSource[];
}

function SourcesTab() {
  const { t } = useTranslation();
  const [config, setConfig] = useState<SourcesConfig>({ telegram: [], email: [], jira: [], bitrix: [] });
  const [status, setStatus] = useState("");
  const [loading, setLoading] = useState(true);

  // Load sources on mount
  useEffect(() => {
    const load = async () => {
      try {
        const result = await invoke<SourcesConfig>("list_sources_cmd", { profile: null });
        setConfig(result);
      } catch (e) {
        console.error("Failed to load sources:", e);
      } finally {
        setLoading(false);
      }
    };
    load();
  }, []);


  // --- Telegram ---
  const addTelegram = async () => {
    const newSource: TelegramSource = {
      id: crypto.randomUUID(),
      name: `Telegram ${config.telegram.length + 1}`,
      bot_token: "",
      chat_id: "",
      allowed_users: "",
      home_channel: "",
      enabled: true,
      use_proxy: true,
    };
    try {
      await invoke("add_telegram_source_cmd", { source: newSource, profile: null });
      const result = await invoke<SourcesConfig>("list_sources_cmd", { profile: null });
      setConfig(result);
    } catch (e: any) {
      setStatus("✗ " + (e?.message || String(e)));
    }
  };

  const updateTelegram = async (source: TelegramSource | EmailSource | JiraSource | BitrixSource) => {
    try {
      await invoke("update_telegram_source_cmd", { id: source.id, source, profile: null });
      const result = await invoke<SourcesConfig>("list_sources_cmd", { profile: null });
      setConfig(result);
    } catch (e: any) {
      setStatus("✗ " + (e?.message || String(e)));
    }
  };

  const removeTelegram = async (id: string) => {
    try {
      await invoke("remove_telegram_source_cmd", { id, profile: null });
      const result = await invoke<SourcesConfig>("list_sources_cmd", { profile: null });
      setConfig(result);
    } catch (e: any) {
      setStatus("✗ " + (e?.message || String(e)));
    }
  };

  // --- Email ---
  const addEmail = async () => {
    const newSource: EmailSource = {
      id: crypto.randomUUID(),
      name: `Email ${config.email.length + 1}`,
      address: "",
      password: "",
      smtp_host: "smtp.gmail.com",
      smtp_port: 587,
      imap_host: "imap.gmail.com",
      enabled: true,
      use_proxy: true,
    };
    try {
      await invoke("add_email_source_cmd", { source: newSource, profile: null });
      const result = await invoke<SourcesConfig>("list_sources_cmd", { profile: null });
      setConfig(result);
    } catch (e: any) {
      setStatus("✗ " + (e?.message || String(e)));
    }
  };

  const updateEmail = async (source: TelegramSource | EmailSource | JiraSource | BitrixSource) => {
    try {
      await invoke("update_email_source_cmd", { id: source.id, source, profile: null });
      const result = await invoke<SourcesConfig>("list_sources_cmd", { profile: null });
      setConfig(result);
    } catch (e: any) {
      setStatus("✗ " + (e?.message || String(e)));
    }
  };

  const removeEmail = async (id: string) => {
    try {
      await invoke("remove_email_source_cmd", { id, profile: null });
      const result = await invoke<SourcesConfig>("list_sources_cmd", { profile: null });
      setConfig(result);
    } catch (e: any) {
      setStatus("✗ " + (e?.message || String(e)));
    }
  };

  // --- Jira ---
  const addJira = async () => {
    const newSource: JiraSource = {
      id: crypto.randomUUID(),
      name: `Jira ${config.jira.length + 1}`,
      url: "",
      username: "",
      api_token: "",
      project_key: "",
      enabled: true,
      use_proxy: true,
    };
    try {
      await invoke("add_jira_source_cmd", { source: newSource, profile: null });
      const result = await invoke<SourcesConfig>("list_sources_cmd", { profile: null });
      setConfig(result);
    } catch (e: any) {
      setStatus("✗ " + (e?.message || String(e)));
    }
  };

  const updateJira = async (source: TelegramSource | EmailSource | JiraSource | BitrixSource) => {
    try {
      await invoke("update_jira_source_cmd", { id: source.id, source, profile: null });
      const result = await invoke<SourcesConfig>("list_sources_cmd", { profile: null });
      setConfig(result);
    } catch (e: any) {
      setStatus("✗ " + (e?.message || String(e)));
    }
  };

  const removeJira = async (id: string) => {
    try {
      await invoke("remove_jira_source_cmd", { id, profile: null });
      const result = await invoke<SourcesConfig>("list_sources_cmd", { profile: null });
      setConfig(result);
    } catch (e: any) {
      setStatus("✗ " + (e?.message || String(e)));
    }
  };

  // --- Bitrix ---
  const addBitrix = async () => {
    const newSource: BitrixSource = {
      id: crypto.randomUUID(),
      name: `Bitrix ${config.bitrix.length + 1}`,
      webhook_url: "",
      user_id: "",
      enabled: true,
      use_proxy: true,
    };
    try {
      await invoke("add_bitrix_source_cmd", { source: newSource, profile: null });
      const result = await invoke<SourcesConfig>("list_sources_cmd", { profile: null });
      setConfig(result);
    } catch (e: any) {
      setStatus("✗ " + (e?.message || String(e)));
    }
  };

  const updateBitrix = async (source: TelegramSource | EmailSource | JiraSource | BitrixSource) => {
    try {
      await invoke("update_bitrix_source_cmd", { id: source.id, source, profile: null });
      const result = await invoke<SourcesConfig>("list_sources_cmd", { profile: null });
      setConfig(result);
    } catch (e: any) {
      setStatus("✗ " + (e?.message || String(e)));
    }
  };

  const removeBitrix = async (id: string) => {
    try {
      await invoke("remove_bitrix_source_cmd", { id, profile: null });
      const result = await invoke<SourcesConfig>("list_sources_cmd", { profile: null });
      setConfig(result);
    } catch (e: any) {
      setStatus("✗ " + (e?.message || String(e)));
    }
  };

  const SourceCard = ({ source, type, onUpdate, onRemove, isActive }: {
    source: TelegramSource | EmailSource | JiraSource | BitrixSource;
    type: "telegram" | "email" | "jira" | "bitrix";
    onUpdate: (s: TelegramSource | EmailSource | JiraSource | BitrixSource) => void;
    onRemove: (id: string) => void;
    isActive: boolean;
  }) => {
    const [editing, setEditing] = useState(false);
    const [localSource, setLocalSource] = useState(source);

    const handleChange = (field: string, value: string | number | boolean) => {
      setLocalSource((s) => ({ ...s, [field]: value }));
    };

    const fields = type === "telegram" ? [
          { key: "name", label: t("sources.name"), type: "text" as const },
          { key: "bot_token", label: t("sources.botToken"), type: "password" as const, placeholder: "123456:ABC-DEF..." },
          { key: "chat_id", label: t("sources.chatId"), type: "text" as const, placeholder: "-1001234567890" },
          { key: "allowed_users", label: t("sources.allowedUsers"), type: "text" as const, placeholder: "user_id1, user_id2" },
          { key: "home_channel", label: t("sources.homeChannel"), type: "text" as const, placeholder: "-1001234567890" },
          { key: "enabled", label: t("sources.enabled"), type: "checkbox" as const },
          { key: "use_proxy", label: t("sources.useProxy"), type: "checkbox" as const },
          { key: "proxy_url", label: t("sources.proxyUrl"), type: "text" as const, placeholder: "http://127.0.0.1:8080" },
        ] : type === "email" ? [
          { key: "name", label: t("sources.name"), type: "text" as const },
          { key: "address", label: t("sources.emailAddress"), type: "text" as const, placeholder: "you@example.com" },
          { key: "password", label: t("sources.password"), type: "password" as const, placeholder: "app password" },
          { key: "smtp_host", label: t("sources.smtpHost"), type: "text" as const, placeholder: "smtp.gmail.com" },
          { key: "smtp_port", label: t("sources.smtpPort"), type: "number" as const },
          { key: "imap_host", label: t("sources.imapHost"), type: "text" as const, placeholder: "imap.gmail.com" },
          { key: "imap_port", label: t("sources.imapPort"), type: "number" as const },
          { key: "use_ssl", label: t("sources.useSsl"), type: "checkbox" as const },
          { key: "enabled", label: t("sources.enabled"), type: "checkbox" as const },
          { key: "use_proxy", label: t("sources.useProxy"), type: "checkbox" as const },
          { key: "proxy_url", label: t("sources.proxyUrl"), type: "text" as const, placeholder: "http://127.0.0.1:8080" },
        ] : type === "jira" ? [
          { key: "name", label: t("sources.name"), type: "text" as const },
          { key: "url", label: t("sources.jiraUrl"), type: "text" as const, placeholder: "https://company.atlassian.net" },
          { key: "username", label: t("sources.username"), type: "text" as const },
          { key: "api_token", label: t("sources.apiToken"), type: "password" as const, placeholder: "API token" },
          { key: "project_key", label: t("sources.projectKey"), type: "text" as const, placeholder: "PROJ" },
          { key: "enabled", label: t("sources.enabled"), type: "checkbox" as const },
          { key: "use_proxy", label: t("sources.useProxy"), type: "checkbox" as const },
          { key: "proxy_url", label: t("sources.proxyUrl"), type: "text" as const, placeholder: "http://127.0.0.1:8080" },
        ] : [
          { key: "name", label: t("sources.name"), type: "text" as const },
          { key: "webhook_url", label: t("sources.webhookUrl"), type: "text" as const, placeholder: "https://company.bitrix24.ru/rest/1/xxxxx/" },
          { key: "user_id", label: t("sources.userId"), type: "text" as const, placeholder: "1" },
          { key: "enabled", label: t("sources.enabled"), type: "checkbox" as const },
          { key: "use_proxy", label: t("sources.useProxy"), type: "checkbox" as const },
          { key: "proxy_url", label: t("sources.proxyUrl"), type: "text" as const, placeholder: "http://127.0.0.1:8080" },
        ];

    const icon = type === "telegram" ? <Send className="w-4 h-4 text-[#0088cc]" /> :
                type === "email" ? <Mail className="w-4 h-4 text-[#ea4335]" /> :
                type === "jira" ? <CheckSquare className="w-4 h-4 text-[#0052cc]" /> :
                <Briefcase className="w-4 h-4 text-[#2c3e50]" />;
    const typeLabel = type === "telegram" ? "Telegram" : type === "email" ? "Email" : type === "jira" ? "Jira" : "Bitrix";

    return (
      <div className="border border-ac-border rounded-lg overflow-hidden">
        <div className="flex items-center gap-3 px-4 py-3 hover:bg-ac-surface transition-colors">
          {icon}
          <span className="flex-1 text-left text-sm font-medium text-ac-ink">{typeLabel}: {localSource.name}</span>
          <span className={`text-[10px] px-2 py-0.5 rounded-full ${
            isActive ? "bg-ac-brand-soft text-ac-brand"
              : localSource.enabled ? "bg-green-500/15 text-green-500"
              : "bg-ac-surface-2 text-ac-muted"
          }`}>
            {isActive ? "● Active" : localSource.enabled ? "✓ Enabled" : "Disabled"}
          </span>
          {editing ? (
            <>
              <button onClick={() => { onUpdate(localSource); setEditing(false); }} className="ac-btn px-3 py-1.5 text-xs">{t("btn.save")}</button>
              <button onClick={() => { setLocalSource(source); setEditing(false); }} className="px-3 py-1.5 text-xs border border-ac-border text-ac-muted rounded-md">{t("btn.cancel")}</button>
            </>
          ) : (
            <>
              <button onClick={() => setEditing(true)} className="p-1.5 rounded text-ac-muted hover:text-ac-brand hover:bg-ac-surface"><Edit3 className="w-3.5 h-3.5" /></button>
              <button onClick={() => onRemove(source.id)} className="p-1.5 rounded text-ac-muted hover:text-ac-red hover:bg-ac-surface"><Trash2 className="w-3.5 h-3.5" /></button>
            </>
          )}
        </div>
        {editing && (
          <div className="p-4 border-t border-ac-border space-y-3 bg-ac-surface">
            {fields.map((f) => (
              <div key={f.key}>
                <label className="text-[11px] text-ac-muted block mb-1">{f.label}</label>
                {f.type === "checkbox" ? (
                  <label className="flex items-center gap-2 cursor-pointer">
                    <input
                      type="checkbox"
                      checked={localSource[f.key as keyof typeof localSource] === true}
                      onChange={(e) => handleChange(f.key, e.target.checked)}
                      className="accent-ac-brand"
                    />
                    <span className="text-sm text-ac-ink">{f.label}</span>
                  </label>
                ) : (
                  <input
                    type={f.type === "number" ? "number" : f.type}
                    placeholder={f.placeholder}
                    value={localSource[f.key as keyof typeof localSource] as string | number}
                    onChange={(e) => handleChange(f.key, f.type === "number" ? Number(e.target.value) : e.target.value)}
                    className="ac-input w-full px-3 py-2 text-sm font-mono"
                  />
                )}
              </div>
            ))}
          </div>
        )}
      </div>
    );
  };

  if (loading) {
    return (
      <div className="flex justify-center py-20">
        <Loader className="w-6 h-6 animate-spin text-ac-muted" />
      </div>
    );
  }

  return (
    <div className="space-y-4">
      <p className="text-[11px] text-ac-muted">{t("sources.hint")}</p>

      {/* Telegram Sources */}
      <div className="space-y-3">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-3">
            <Send className="w-4 h-4 text-[#0088cc]" />
            <span className="text-sm font-medium text-ac-ink">Telegram Bots</span>
          </div>
          <button onClick={addTelegram} className="ac-btn px-3 py-1.5 text-xs flex items-center gap-1.5">
            <Plus className="w-3.5 h-3.5" /> {t("add_model")}
          </button>
        </div>
        {config.telegram.length === 0 ? (
          <p className="text-sm text-ac-muted ml-7 py-4">{t("sources.noTelegram")}</p>
        ) : (
          <>
          <div className="space-y-2 ml-7">
            {config.telegram.map((src, i) => (
              <SourceCard
                key={src.id}
                source={src}
                type="telegram"
                isActive={src.enabled && i === config.telegram.findIndex((s) => s.enabled)}
                onUpdate={updateTelegram}
                onRemove={removeTelegram}
              />
            ))}
          </div>
          {config.telegram.filter((s) => s.enabled).length > 1 && (
            <p className="text-[11px] text-ac-faint ml-7 mt-1">
              ⚠ Only the first enabled Telegram bot is pushed to Hermes .env (single-token limit).
            </p>
          )}
          </>
        )}
        </div>

      {/* Email Sources */}
      <div className="space-y-3">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-3">
            <Mail className="w-4 h-4 text-[#ea4335]" />
            <span className="text-sm font-medium text-ac-ink">Email Accounts (IMAP/SMTP)</span>
          </div>
          <button onClick={addEmail} className="ac-btn px-3 py-1.5 text-xs flex items-center gap-1.5">
            <Plus className="w-3.5 h-3.5" /> {t("add_model")}
          </button>
        </div>
        {config.email.length === 0 ? (
          <p className="text-sm text-ac-muted ml-7 py-4">{t("sources.noEmail")}</p>
        ) : (
          <>
          <div className="space-y-2 ml-7">
            {config.email.map((src, i) => (
              <SourceCard
                key={src.id}
                source={src}
                type="email"
                isActive={src.enabled && i === config.email.findIndex((s) => s.enabled)}
                onUpdate={updateEmail}
                onRemove={removeEmail}
              />
            ))}
          </div>
          {config.email.filter((s) => s.enabled).length > 1 && (
            <p className="text-[11px] text-ac-faint ml-7 mt-1">
              ⚠ Only the first enabled Email account is pushed to Hermes .env (single-account limit).
            </p>
          )}
          </>
        )}
        </div>

      {/* Jira Sources */}
      <div className="space-y-3">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-3">
            <CheckSquare className="w-4 h-4 text-[#0052cc]" />
            <span className="text-sm font-medium text-ac-ink">Jira Instances</span>
          </div>
          <button onClick={addJira} className="ac-btn px-3 py-1.5 text-xs flex items-center gap-1.5">
            <Plus className="w-3.5 h-3.5" /> {t("add_model")}
          </button>
        </div>
        {config.jira.length === 0 ? (
          <p className="text-sm text-ac-muted ml-7 py-4">{t("sources.noJira")}</p>
        ) : (
          <>
          <div className="space-y-2 ml-7">
            {config.jira.map((src, i) => (
              <SourceCard
                key={src.id}
                source={src}
                type="jira"
                isActive={src.enabled && i === config.jira.findIndex((s) => s.enabled)}
                onUpdate={updateJira}
                onRemove={removeJira}
              />
            ))}
          </div>
          </>
        )}
      </div>

      {/* ── Bitrix connectors ─────────────────────────────────────────── */}
      <div className="space-y-2">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <Briefcase className="w-4 h-4 text-[#2c3e50]" />
            <span className="text-sm font-medium text-ac-ink">Bitrix Instances</span>
          </div>
          <button onClick={addBitrix} className="ac-btn px-3 py-1.5 text-xs flex items-center gap-1.5">
            <Plus className="w-3.5 h-3.5" /> {t("add_model")}
          </button>
        </div>
        {config.bitrix.length === 0 ? (
          <p className="text-sm text-ac-muted ml-7 py-4">{t("sources.noBitrix")}</p>
        ) : (
          <div className="space-y-2 ml-7">
            {config.bitrix.map((src, i) => (
              <SourceCard
                key={src.id}
                source={src}
                type="bitrix"
                isActive={src.enabled && i === config.bitrix.findIndex((s) => s.enabled)}
                onUpdate={updateBitrix}
                onRemove={removeBitrix}
              />
            ))}
          </div>
        )}
      </div>

      {/* Note: sources auto-apply to Hermes .env on save — no manual button needed. */}
      <div className="border-t border-ac-border pt-3">
        <p className="text-xs text-ac-muted flex items-center gap-1.5">
          <Zap className="w-3.5 h-3.5" />
          {t("sources.autoApplied")}
        </p>
      </div>

      {status && <p className={`text-xs ${status.startsWith("✓") ? "text-ac-green" : "text-ac-red"}`}>{status}</p>}
    </div>
  );
}

// ── Skills Tab ──────────────────────────────────────────────────────────────
function SkillsTab() {
  const { t } = useTranslation();
  const [skills, setSkills] = useState<any[]>([]);
  const [loading, setLoading] = useState(true);
  const [status, setStatus] = useState("");

  useEffect(() => {
    const load = async () => {
      try {
        const result = await invoke<any[]>("list_installed_skills_cmd", { profile: null });
        setSkills(result);
      } catch (e) {
        console.error("Failed to load skills:", e);
      } finally {
        setLoading(false);
      }
    };
    load();
  }, []);

  const handleInstall = async () => {
    const name = prompt(t("settings.skillInstallPrompt"));
    if (!name) return;
    try {
      await invoke("install_skill_cmd", { identifier: name, profile: null });
      setStatus("✓ " + t("settings.skillInstalled"));
      setTimeout(() => setStatus(""), 2000);
      // Reload
      const result = await invoke<any[]>("list_installed_skills_cmd", { profile: null });
      setSkills(result);
    } catch (e: any) {
      setStatus("✗ " + (e?.message || String(e)));
    }
  };

  const handleUninstall = async (name: string) => {
    if (!confirm(t("settings.skillUninstallConfirm", { name }))) return;
    try {
      await invoke("uninstall_skill_cmd", { name, profile: null });
      setStatus("✓ " + t("settings.skillUninstalled"));
      setTimeout(() => setStatus(""), 2000);
      const result = await invoke<any[]>("list_installed_skills_cmd", { profile: null });
      setSkills(result);
    } catch (e: any) {
      setStatus("✗ " + (e?.message || String(e)));
    }
  };

  const handleView = async (name: string) => {
    try {
      const content = await invoke<string>("get_skill_content_cmd", { skill_name: name, profile: null });
      // Show in a modal or alert for now
      alert(`${name}:\n\n${content.slice(0, 2000)}`);
    } catch (e: any) {
      setStatus("✗ " + (e?.message || String(e)));
    }
  };

  if (loading) {
    return (
      <div className="flex justify-center py-20">
        <Loader className="w-6 h-6 animate-spin text-ac-muted" />
      </div>
    );
  }

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-semibold text-ac-ink">{t("settings.skills")}</h3>
        <button onClick={handleInstall} className="ac-btn px-3 py-1.5 text-xs flex items-center gap-1.5">
          <Plus className="w-3.5 h-3.5" /> {t("settings.skillInstall")}
        </button>
      </div>

      {skills.length === 0 ? (
        <p className="text-sm text-ac-muted text-center py-8">{t("settings.skillsEmpty")}</p>
      ) : (
        <div className="space-y-2 max-h-96 overflow-y-auto">
          {skills.map((skill) => (
            <div key={skill.name} className="flex items-center justify-between p-3 border border-ac-border bg-ac-surface rounded-lg">
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-2">
                  <span className="text-sm font-medium text-ac-ink truncate">{skill.name}</span>
                  <span className="text-[10px] px-2 py-0.5 rounded-full bg-ac-brand-soft text-ac-brand">{skill.category}</span>
                </div>
                <p className="text-xs text-ac-muted mt-1 truncate">{skill.description}</p>
              </div>
              <div className="flex gap-1">
                <button
                  onClick={() => handleView(skill.name)}
                  className="px-2.5 py-1 text-xs border border-ac-border text-ac-muted rounded hover:bg-ac-surface hover:text-ac-brand"
                  title={t("settings.skillView")}
                >
                  <BookOpen className="w-3.5 h-3.5" />
                </button>
                <button
                  onClick={() => handleUninstall(skill.name)}
                  className="px-2.5 py-1 text-xs border border-ac-border text-ac-red/70 rounded hover:bg-ac-red/5"
                  title={t("settings.skillUninstall")}
                >
                  <Trash2 className="w-3.5 h-3.5" />
                </button>
              </div>
            </div>
          ))}
        </div>
      )}

      {status && <p className={`text-xs ${status.startsWith("✓") ? "text-ac-green" : "text-ac-red"}`}>{status}</p>}
    </div>
  );
}

// ── Cron Tab ────────────────────────────────────────────────────────────────
function CronTab() {
  const { t } = useTranslation();
  const [jobs, setJobs] = useState<any[]>([]);
  const [loading, setLoading] = useState(true);
  const [status, setStatus] = useState("");
  const [showForm, setShowForm] = useState(false);
  const [formData, setFormData] = useState({
    name: "",
    schedule: "",
    prompt: "",
    deliver: "",
  });

  useEffect(() => {
    const load = async () => {
      try {
        const result = await invoke<any[]>("list_cron_jobs_cmd", { include_disabled: true, profile: null });
        setJobs(result);
      } catch (e) {
        console.error("Failed to load cron jobs:", e);
      } finally {
        setLoading(false);
      }
    };
    load();
  }, []);

  const handleCreate = async () => {
    if (!formData.name.trim() || !formData.schedule.trim()) {
      setStatus("✗ Name and schedule are required");
      return;
    }
    try {
      await invoke("create_cron_job_cmd", {
        schedule: formData.schedule,
        prompt: formData.prompt || null,
        name: formData.name,
        deliver: formData.deliver || null,
        profile: null,
      });
      setStatus("✓ " + t("settings.cronCreated"));
      setTimeout(() => setStatus(""), 2000);
      setShowForm(false);
      setFormData({ name: "", schedule: "", prompt: "", deliver: "" });
      const result = await invoke<any[]>("list_cron_jobs_cmd", { include_disabled: true, profile: null });
      setJobs(result);
    } catch (e: any) {
      setStatus("✗ " + (e?.message || String(e)));
    }
  };

  const handleToggle = async (job: any) => {
    try {
      await invoke(job.enabled ? "pause_cron_job_cmd" : "resume_cron_job_cmd", { job_id: job.id, profile: null });
      setStatus("✓ " + t("settings.cronToggled"));
      setTimeout(() => setStatus(""), 2000);
      const result = await invoke<any[]>("list_cron_jobs_cmd", { include_disabled: true, profile: null });
      setJobs(result);
    } catch (e: any) {
      setStatus("✗ " + (e?.message || String(e)));
    }
  };

  const handleTrigger = async (job: any) => {
    try {
      await invoke("trigger_cron_job_cmd", { job_id: job.id, profile: null });
      setStatus("✓ " + t("settings.cronTriggered"));
      setTimeout(() => setStatus(""), 2000);
    } catch (e: any) {
      setStatus("✗ " + (e?.message || String(e)));
    }
  };

  const handleDelete = async (id: string) => {
    if (!confirm(t("settings.cronDeleteConfirm"))) return;
    try {
      await invoke("remove_cron_job_cmd", { job_id: id, profile: null });
      setStatus("✓ " + t("settings.cronDeleted"));
      setTimeout(() => setStatus(""), 2000);
      const result = await invoke<any[]>("list_cron_jobs_cmd", { include_disabled: true, profile: null });
      setJobs(result);
    } catch (e: any) {
      setStatus("✗ " + (e?.message || String(e)));
    }
  };

  if (loading) {
    return (
      <div className="flex justify-center py-20">
        <Loader className="w-6 h-6 animate-spin text-ac-muted" />
      </div>
    );
  }

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-semibold text-ac-ink">{t("settings.cron")}</h3>
        <button onClick={() => setShowForm(!showForm)} className="ac-btn px-3 py-1.5 text-xs flex items-center gap-1.5">
          <Plus className="w-3.5 h-3.5" /> {t("settings.cronAdd")}
        </button>
      </div>

      {showForm && (
        <div className="p-4 border border-ac-border bg-ac-surface rounded-lg space-y-3">
          <input
            className="ac-input w-full px-3 py-2 text-sm"
            placeholder={t("settings.cronName")}
            value={formData.name}
            onChange={(e) => setFormData({ ...formData, name: e.target.value })}
          />
          <input
            className="ac-input w-full px-3 py-2 text-sm font-mono"
            placeholder="*/5 * * * * (every 5 min)"
            value={formData.schedule}
            onChange={(e) => setFormData({ ...formData, schedule: e.target.value })}
          />
          <textarea
            className="ac-input w-full px-3 py-2 text-sm"
            rows={3}
            placeholder={t("settings.cronPrompt")}
            value={formData.prompt}
            onChange={(e) => setFormData({ ...formData, prompt: e.target.value })}
          />
          <input
            className="ac-input w-full px-3 py-2 text-sm"
            placeholder="email, telegram, webhook"
            value={formData.deliver}
            onChange={(e) => setFormData({ ...formData, deliver: e.target.value })}
          />
          <div className="flex gap-2">
            <button onClick={handleCreate} className="ac-btn px-4 py-2 text-sm">{t("btn.save")}</button>
            <button onClick={() => { setShowForm(false); setFormData({ name: "", schedule: "", prompt: "", deliver: "" }); }} className="px-4 py-2 text-sm border border-ac-border text-ac-muted rounded-md">{t("btn.cancel")}</button>
          </div>
        </div>
      )}

      {jobs.length === 0 ? (
        <p className="text-sm text-ac-muted text-center py-8">{t("settings.cronEmpty")}</p>
      ) : (
        <div className="space-y-2">
          {jobs.map((job) => (
            <div key={job.id} className="group p-3 border border-ac-border bg-ac-surface rounded-lg">
              <div className="flex items-start justify-between">
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2 mb-1">
                    <span className="text-sm font-medium text-ac-ink truncate">{job.name}</span>
                    <span className={`text-[10px] px-2 py-0.5 rounded-full ${job.enabled ? "bg-green-500/15 text-green-500" : "bg-ac-surface-2 text-ac-muted"}`}>
                      {job.enabled ? t("settings.cronActive") : t("settings.cronPaused")}
                    </span>
                  </div>
                  <p className="text-xs text-ac-muted truncate">{job.schedule}</p>
                  {job.prompt && <p className="text-[11px] text-ac-muted mt-1 truncate">{job.prompt}</p>}
                  {job.deliver && job.deliver.length > 0 && (
                    <p className="text-[10px] text-ac-muted mt-0.5">→ {job.deliver.join(", ")}</p>
                  )}
                </div>
                <div className="flex gap-1 ml-4">
                  <button
                    onClick={() => handleToggle(job)}
                    className="px-2 py-1 text-[10px] border border-ac-border text-ac-muted rounded hover:bg-ac-surface hover:text-ac-brand"
                  >
                    {job.enabled ? t("settings.cronPause") : t("settings.cronResume")}
                  </button>
                  <button
                    onClick={() => handleTrigger(job)}
                    className="px-2 py-1 text-[10px] border border-ac-border text-ac-brand rounded hover:bg-ac-brand/10"
                  >
                    {t("settings.cronTrigger")}
                  </button>
                  <button
                    onClick={() => handleDelete(job.id)}
                    className="px-2 py-1 text-[10px] border border-ac-border text-ac-red/70 rounded hover:bg-ac-red/5"
                  >
                    <Trash2 className="w-3 h-3 inline" />
                  </button>
                </div>
              </div>
            </div>
          ))}
        </div>
      )}

      {status && <p className={`text-xs ${status.startsWith("✓") ? "text-ac-green" : "text-ac-red"}`}>{status}</p>}
    </div>
  );
}

// ── MCP Tab ─────────────────────────────────────────────────────────────────

/// Check if an env-var key looks like a secret (for password masking).
function isSecretKey(key: string): boolean {
  const upper = key.toUpperCase();
  return ["PASSWORD", "PAT", "TOKEN", "SECRET", "KEY", "CREDENTIAL", "API_KEY"].some((s) =>
    upper.includes(s)
  );
}

/// Reusable env-var editor. Renders a list of key→value pairs with add/remove.
/// Secrets are masked with eye-toggle (like ProvidersScreen).
function EnvVarEditor({
  env,
  onChange,
}: {
  env: Record<string, string>;
  onChange: (env: Record<string, string>) => void;
}) {
  const { t } = useTranslation();
  const [visibleKeys, setVisibleKeys] = useState<Set<string>>(new Set());
  const [newKey, setNewKey] = useState("");
  const [newValue, setNewValue] = useState("");

  const entries = Object.entries(env);

  const toggleVisibility = (key: string) => {
    setVisibleKeys((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  };

  const updateValue = (key: string, value: string) => {
    onChange({ ...env, [key]: value });
  };

  const removeKey = (key: string) => {
    const next = { ...env };
    delete next[key];
    onChange(next);
  };

  const addNew = () => {
    if (!newKey.trim()) return;
    onChange({ ...env, [newKey.trim()]: newValue });
    setNewKey("");
    setNewValue("");
  };

  return (
    <div className="space-y-1.5">
      {entries.length === 0 && (
        <p className="text-xs text-ac-muted py-2">{t("settings.mcpEnvEmpty")}</p>
      )}
      {entries.map(([key, value]) => {
        const isSecret = isSecretKey(key);
        const isVisible = visibleKeys.has(key);
        return (
          <div key={key} className="flex items-center gap-1.5">
            <span className="text-xs font-mono text-ac-muted w-40 shrink-0 truncate" title={key}>
              {key}
            </span>
            <input
              type={isSecret && !isVisible ? "password" : "text"}
              className="ac-input flex-1 px-2 py-1 text-xs font-mono"
              value={value}
              onChange={(e) => updateValue(key, e.target.value)}
            />
            {isSecret && (
              <button
                onClick={() => toggleVisibility(key)}
                className="p-1 text-ac-muted hover:text-ac-ink"
                title={isVisible ? "Hide" : "Show"}
              >
                {isVisible ? <EyeOff className="w-3.5 h-3.5" /> : <Eye className="w-3.5 h-3.5" />}
              </button>
            )}
            <button
              onClick={() => removeKey(key)}
              className="p-1 text-ac-red/60 hover:text-ac-red"
              title="Remove"
            >
              <Trash2 className="w-3.5 h-3.5" />
            </button>
          </div>
        );
      })}
      {/* Add new row */}
      <div className="flex items-center gap-1.5 pt-1">
        <input
          className="ac-input w-40 shrink-0 px-2 py-1 text-xs font-mono"
          placeholder={t("settings.mcpEnvKey")}
          value={newKey}
          onChange={(e) => setNewKey(e.target.value)}
        />
        <input
          className="ac-input flex-1 px-2 py-1 text-xs font-mono"
          placeholder={t("settings.mcpEnvValue")}
          value={newValue}
          onChange={(e) => setNewValue(e.target.value)}
        />
        <button
          onClick={addNew}
          disabled={!newKey.trim()}
          className="p-1 text-ac-brand disabled:opacity-30"
          title={t("settings.mcpEnvAdd")}
        >
          <Plus className="w-3.5 h-3.5" />
        </button>
      </div>
    </div>
  );
}

function McpTab() {
  const { t } = useTranslation();
  const [servers, setServers] = useState<any[]>([]);
  const [loading, setLoading] = useState(true);
  const [status, setStatus] = useState("");
  const [showForm, setShowForm] = useState(false);
  const [expandedServer, setExpandedServer] = useState<string | null>(null);
  // Draft env being edited for each expanded server.
  const [envDrafts, setEnvDrafts] = useState<Record<string, Record<string, string>>>({});
  const [formData, setFormData] = useState({
    name: "",
    server_type: "http",
    url: "",
    command: "",
    args: "",
    auth: "",
    env: {} as Record<string, string>,
  });

  const reload = useCallback(async () => {
    try {
      const result = await invoke<any[]>("list_mcp_servers_cmd", { profile: null });
      setServers(result);
    } catch (e) {
      console.error("Failed to load MCP servers:", e);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    reload();
  }, [reload]);

  const handleExpand = (server: any) => {
    if (expandedServer === server.name) {
      setExpandedServer(null);
    } else {
      setExpandedServer(server.name);
      // Seed the draft from current server env.
      setEnvDrafts((prev) => ({
        ...prev,
        [server.name]: { ...(server.env || {}) },
      }));
    }
  };

  const handleSaveEnv = async (serverName: string) => {
    const draft = envDrafts[serverName];
    if (!draft) return;
    try {
      await invoke("update_mcp_server_env_cmd", {
        serverName,
        env: draft,
        profile: null,
      });
      setStatus("✓ " + t("settings.mcpEnvUpdated"));
      setTimeout(() => setStatus(""), 2000);
      await reload();
    } catch (e: any) {
      setStatus("✗ " + (e?.message || String(e)));
    }
  };

  const handleAdd = async () => {
    if (!formData.name.trim()) {
      setStatus("✗ " + t("settings.mcpNameRequired"));
      return;
    }
    if (formData.server_type === "http" && !formData.url.trim()) {
      setStatus("✗ " + t("settings.mcpUrlRequired"));
      return;
    }
    if (formData.server_type === "stdio" && !formData.command.trim()) {
      setStatus("✗ " + t("settings.mcpCommandRequired"));
      return;
    }
    try {
      await invoke("add_mcp_server_cmd", {
        input: {
          name: formData.name,
          server_type: formData.server_type,
          url: formData.url || null,
          command: formData.command || null,
          args: formData.args ? formData.args.split(" ").filter(Boolean) : null,
          env: Object.keys(formData.env).length > 0 ? formData.env : null,
          auth: formData.auth || null,
        },
        profile: null,
      });
      setStatus("✓ " + t("settings.mcpAdded"));
      setTimeout(() => setStatus(""), 2000);
      setShowForm(false);
      setFormData({ name: "", server_type: "http", url: "", command: "", args: "", auth: "", env: {} });
      await reload();
    } catch (e: any) {
      setStatus("✗ " + (e?.message || String(e)));
    }
  };

  const handleRemove = async (name: string) => {
    if (!confirm(t("settings.mcpRemoveConfirm", { name }))) return;
    try {
      await invoke("remove_mcp_server_cmd", { name, profile: null });
      setStatus("✓ " + t("settings.mcpRemoved"));
      setTimeout(() => setStatus(""), 2000);
      await reload();
    } catch (e: any) {
      setStatus("✗ " + (e?.message || String(e)));
    }
  };

  const handleToggle = async (server: any) => {
    try {
      await invoke("set_mcp_server_enabled_cmd", { name: server.name, enabled: !server.enabled, profile: null });
      setStatus("✓ " + t("settings.mcpToggled"));
      setTimeout(() => setStatus(""), 2000);
      await reload();
    } catch (e: any) {
      setStatus("✗ " + (e?.message || String(e)));
    }
  };

  if (loading) {
    return (
      <div className="flex justify-center py-20">
        <Loader className="w-6 h-6 animate-spin text-ac-muted" />
      </div>
    );
  }

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-semibold text-ac-ink">{t("settings.mcp")}</h3>
        <button onClick={() => setShowForm(!showForm)} className="ac-btn px-3 py-1.5 text-xs flex items-center gap-1.5">
          <Plus className="w-3.5 h-3.5" /> {t("settings.mcpAdd")}
        </button>
      </div>

      {showForm && (
        <div className="p-4 border border-ac-border bg-ac-surface rounded-lg space-y-3">
          <input
            className="ac-input w-full px-3 py-2 text-sm"
            placeholder={t("settings.mcpName")}
            value={formData.name}
            onChange={(e) => setFormData({ ...formData, name: e.target.value })}
          />
          <select
            className="ac-input w-full px-3 py-2 text-sm"
            value={formData.server_type}
            onChange={(e) => setFormData({ ...formData, server_type: e.target.value })}
          >
            <option value="http">{t("settings.mcpHttp")}</option>
            <option value="stdio">{t("settings.mcpStdio")}</option>
          </select>
          {formData.server_type === "http" && (
            <input
              className="ac-input w-full px-3 py-2 text-sm font-mono"
              placeholder={t("settings.mcpUrlPh")}
              value={formData.url}
              onChange={(e) => setFormData({ ...formData, url: e.target.value })}
            />
          )}
          {formData.server_type === "stdio" && (
            <>
              <input
                className="ac-input w-full px-3 py-2 text-sm"
                placeholder={t("settings.mcpCommandPh")}
                value={formData.command}
                onChange={(e) => setFormData({ ...formData, command: e.target.value })}
              />
              <input
                className="ac-input w-full px-3 py-2 text-sm"
                placeholder="-m server (space-separated)"
                value={formData.args}
                onChange={(e) => setFormData({ ...formData, args: e.target.value })}
              />
            </>
          )}
          <input
            className="ac-input w-full px-3 py-2 text-sm"
            placeholder={t("settings.mcpAuthPh")}
            value={formData.auth}
            onChange={(e) => setFormData({ ...formData, auth: e.target.value })}
          />
          {/* Env editor for new server */}
          <div>
            <p className="text-xs font-medium text-ac-muted mb-1.5">{t("settings.mcpEnv")}</p>
            <EnvVarEditor env={formData.env} onChange={(env) => setFormData({ ...formData, env })} />
          </div>
          <div className="flex gap-2">
            <button onClick={handleAdd} className="ac-btn px-4 py-2 text-sm">{t("btn.save")}</button>
            <button onClick={() => { setShowForm(false); setFormData({ name: "", server_type: "http", url: "", command: "", args: "", auth: "", env: {} }); }} className="px-4 py-2 text-sm border border-ac-border text-ac-muted rounded-md">{t("btn.cancel")}</button>
          </div>
        </div>
      )}

      {servers.length === 0 ? (
        <p className="text-sm text-ac-muted text-center py-8">{t("settings.mcpEmpty")}</p>
      ) : (
        <div className="space-y-2">
          {servers.map((server) => {
            const isExpanded = expandedServer === server.name;
            const rawFieldEntries: [string, string][] = Object.entries(server.raw_fields || {});
            return (
              <div key={server.name} className="border border-ac-border bg-ac-surface rounded-lg overflow-hidden">
                {/* Collapsed header */}
                <div className="flex items-start justify-between p-3">
                  <div className="flex items-center gap-2 min-w-0 flex-1">
                    <button
                      onClick={() => handleExpand(server)}
                      className="p-0.5 text-ac-muted hover:text-ac-ink shrink-0"
                    >
                      {isExpanded ? <ChevronDown className="w-4 h-4" /> : <ChevronRight className="w-4 h-4" />}
                    </button>
                    <span className="text-sm font-medium text-ac-ink truncate">{server.name}</span>
                    <span className={`text-[10px] px-2 py-0.5 rounded-full shrink-0 ${server.enabled ? "bg-green-500/15 text-green-500" : "bg-ac-surface-2 text-ac-muted"}`}>
                      {server.enabled ? t("settings.mcpEnabled") : t("settings.mcpDisabled")}
                    </span>
                    <span className="text-[10px] px-2 py-0.5 rounded-full bg-ac-brand-soft text-ac-brand shrink-0">{server.server_type}</span>
                  </div>
                  <div className="flex gap-1 ml-4 shrink-0">
                    <button
                      onClick={() => handleToggle(server)}
                      className="px-2 py-1 text-[10px] border border-ac-border text-ac-muted rounded hover:bg-ac-surface hover:text-ac-brand"
                    >
                      {server.enabled ? t("settings.mcpDisable") : t("settings.mcpEnable")}
                    </button>
                    <button
                      onClick={() => handleRemove(server.name)}
                      className="px-2 py-1 text-[10px] border border-ac-border text-ac-red/70 rounded hover:bg-ac-red/5"
                    >
                      <Trash2 className="w-3 h-3 inline" />
                    </button>
                  </div>
                </div>

                {/* Expanded details */}
                {isExpanded && (
                  <div className="border-t border-ac-border px-3 py-3 space-y-3 bg-ac-bg/30">
                    {/* Read-only connection info */}
                    <div className="space-y-1">
                      {server.url && (
                        <div className="flex items-center gap-2 text-xs">
                          <span className="text-ac-muted w-20 shrink-0">URL</span>
                          <span className="font-mono text-ac-ink truncate">{server.url}</span>
                        </div>
                      )}
                      {server.command && (
                        <div className="flex items-center gap-2 text-xs">
                          <span className="text-ac-muted w-20 shrink-0">{t("settings.mcpCommand")}</span>
                          <span className="font-mono text-ac-ink truncate">{server.command}</span>
                        </div>
                      )}
                      {server.args && server.args.length > 0 && (
                        <div className="flex items-center gap-2 text-xs">
                          <span className="text-ac-muted w-20 shrink-0">{t("settings.mcpArgs")}</span>
                          <span className="font-mono text-ac-ink truncate">{server.args.join(" ")}</span>
                        </div>
                      )}
                      {rawFieldEntries.length > 0 && (
                        <details className="mt-1">
                          <summary className="text-[10px] text-ac-muted cursor-pointer hover:text-ac-ink">
                            {t("settings.mcpRawFields")} ({rawFieldEntries.length})
                          </summary>
                          <div className="mt-1 space-y-0.5 pl-2">
                            {rawFieldEntries.map(([k, v]) => (
                              <div key={k} className="flex items-center gap-2 text-[11px]">
                                <span className="text-ac-muted font-mono shrink-0">{k}</span>
                                <span className="font-mono text-ac-ink/70 truncate">{v}</span>
                              </div>
                            ))}
                          </div>
                        </details>
                      )}
                    </div>

                    {/* Env editor */}
                    <div>
                      <div className="flex items-center justify-between mb-1.5">
                        <p className="text-xs font-medium text-ac-muted">{t("settings.mcpEnv")}</p>
                        <button
                          onClick={() => handleSaveEnv(server.name)}
                          className="ac-btn px-2 py-1 text-[10px] flex items-center gap-1"
                        >
                          <Save className="w-3 h-3" /> {t("btn.save")}
                        </button>
                      </div>
                      <EnvVarEditor
                        env={envDrafts[server.name] || {}}
                        onChange={(env) =>
                          setEnvDrafts((prev) => ({ ...prev, [server.name]: env }))
                        }
                      />
                    </div>
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}

      {status && <p className={`text-xs ${status.startsWith("✓") ? "text-ac-green" : "text-ac-red"}`}>{status}</p>}
    </div>
  );
}

// ── Main Settings Panel ───────────────────────────────────────────────────
export function SettingsPanel({ onClose }: { onClose: () => void }) {
  const [activeTab, setActiveTab] = useState<SettingsTab>("general");
  const { t } = useTranslation();

  // Grouped tabs: 19 flat tabs → 6 logical sections so the user isn't
  // overwhelmed. Each group renders as a labeled section in the rail.
  const tabGroups: {
    label: string;
    icon: typeof Sun;
    tabs: { id: SettingsTab; label: string; icon: typeof Sun }[];
  }[] = [
    {
      label: t("settings_general") || "General",
      icon: Sun,
      tabs: [
        { id: "general", label: t("settings_general"), icon: Sun },
        { id: "appearance", label: t("settings.appearance"), icon: Palette },
        { id: "soul", label: t("settings.soul"), icon: Sparkles },
        { id: "about", label: t("settings.about"), icon: Info },
      ],
    },
    {
      label: t("settings_connection") || "Connection",
      icon: Globe,
      tabs: [
        { id: "connection", label: t("settings_connection"), icon: Globe },
        { id: "gateway", label: t("settings.gateway"), icon: Bot },
      ],
    },
    {
      label: t("sources.title") || "Sources",
      icon: Send,
      tabs: [
        { id: "sources", label: t("sources.title"), icon: Send },
        { id: "telegram", label: t("settings_telegram"), icon: Send },
      ],
    },
    {
      label: t("settings_models") || "Models & Keys",
      icon: Cpu,
      tabs: [
        { id: "models", label: t("settings_models"), icon: Cpu },
        { id: "providers", label: t("settings.providers"), icon: KeyRound },
        { id: "credentials", label: t("settings.credentials"), icon: KeyRound },
        { id: "agent", label: t("settings.agent"), icon: Cpu },
        { id: "tts", label: "TTS", icon: Send },
      ],
    },
    {
      label: t("settings.tools") || "Tools & Skills",
      icon: Wrench,
      tabs: [
        { id: "tools", label: t("settings.tools"), icon: Wrench },
        { id: "skills", label: t("settings.skills"), icon: BookOpen },
        { id: "cron", label: t("settings.cron"), icon: Clock },
        { id: "mcp", label: t("settings.mcp"), icon: Server },
      ],
    },
    {
      label: t("settings.diagnose") || "Diagnostics",
      icon: Stethoscope,
      tabs: [
        { id: "diagnose", label: t("settings.diagnose"), icon: Stethoscope },
        { id: "terminal", label: t("settings_terminal"), icon: TermIcon },
      ],
    },
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

        {/* Two-column layout: a left tab rail with 6 grouped sections + the
            active panel on the right. */}
        <div className="flex gap-4 flex-1 min-h-0">
          {/* Tab rail — grouped */}
          <nav className="w-44 shrink-0 border-r border-ac-border pr-2 overflow-y-auto">
            {tabGroups.map((group) => (
              <div key={group.label} className="mb-2">
                <div className="flex items-center gap-1.5 px-2 py-1 text-[10px] font-semibold uppercase tracking-wide text-ac-faint">
                  <group.icon className="w-3 h-3" />
                  {group.label}
                </div>
                {group.tabs.map((tab) => (
                  <button
                    key={tab.id}
                    onClick={() => setActiveTab(tab.id)}
                    className={`w-full flex items-center gap-2 px-2.5 py-1.5 text-xs rounded-md mb-0.5 transition-colors text-left ${
                      activeTab === tab.id
                        ? "bg-ac-brand/10 text-ac-brand"
                        : "text-ac-muted hover:text-ac-ink hover:bg-ac-surface"
                    }`}
                  >
                    <tab.icon className="w-3.5 h-3.5 shrink-0" />
                    <span className="truncate">{tab.label}</span>
                  </button>
                ))}
              </div>
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
                { key: "max_turns", label: t("agent.maxTurns"), type: "number" },
                { key: "reasoning_effort", label: t("agent.reasoningEffort") },
                { key: "verbose", label: t("agent.verbose") },
              ]} />
            )}
            {activeTab === "tts" && (
              <HermesSectionTab section="tts" fields={[
                { key: "provider", label: t("agent.ttsProvider") },
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
            {activeTab === "skills" && <SkillsTab />}
            {activeTab === "cron" && <CronTab />}
            {activeTab === "mcp" && <McpTab />}
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
