// src/components/settings/SettingsPanel.tsx
// v0.6.0: Settings tabs connected to Rust via invoke + stores
// General: get_app_version, init_app (hermes_home), theme via set_env
// Connection: get_connection_config / set_connection_config via connectionStore
// Models: list_models_cmd / add_model_cmd / remove_model_cmd / set_model_config_cmd

import { useState, useEffect, useCallback } from "react";
import { X, Server, Globe, Shield, Moon, Sun, Send, Cpu, Terminal as TermIcon, Languages } from "lucide-react";
import { useSettingsStore } from "../../stores/settingsStore";
import { useConnectionStore, type ConnectionMode } from "../../stores/connectionStore";
import { useUIStore } from "../../stores/uiStore";
import { useTranslation } from "../../hooks/useTranslation";
import type { Lang } from "../../lib/i18n";

type SettingsTab = "general" | "connection" | "telegram" | "models" | "terminal";

// ── General tab ────────────────────────────────────────────────────────────
function GeneralTab() {
  const { generalInfo, generalLoading, generalError, loadGeneralInfo, setTheme } = useSettingsStore();
  const { darkMode, toggleDarkMode, language, setLanguage } = useUIStore();
  const { t } = useTranslation();

  useEffect(() => {
    if (!generalInfo) loadGeneralInfo();
  }, [generalInfo, loadGeneralInfo]);

  const handleToggleTheme = useCallback(() => {
    const newDark = !darkMode;
    toggleDarkMode();
    setTheme(newDark);
  }, [darkMode, toggleDarkMode, setTheme]);
  void handleToggleTheme;

  return (
    <div className="space-y-4">
      {/* Language selector */}
      <div className="mb-4">
        <label className="ac-section-title mb-1.5 block">{t("language_label")}</label>
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
            onClick={() => {
              if (!darkMode) { toggleDarkMode(); setTheme(true); }
            }}
            className={`ac-pill flex items-center gap-1.5 ${darkMode ? "active" : ""}`}
          >
            <Moon className="w-3 h-3" />
            {t("theme_dark")}
          </button>
          <button
            onClick={() => {
              if (darkMode) { toggleDarkMode(); setTheme(false); }
            }}
            className={`ac-pill flex items-center gap-1.5 ${!darkMode ? "active" : ""}`}
          >
            <Sun className="w-3 h-3" />
            {t("theme_light")}
          </button>
        </div>
      </div>

      <div>
        <label className="ac-section-title mb-1.5 block">{t("app_version_label")}</label>
        {generalLoading ? (
          <p className="text-xs text-ac-stone">{t("loading_dots")}</p>
        ) : generalError ? (
          <p className="text-xs text-ac-red">{generalError}</p>
        ) : generalInfo ? (
          <p className="text-sm font-mono text-ac-ivory">{generalInfo.version}</p>
        ) : null}
      </div>

      <div>
        <label className="ac-section-title mb-1.5 block">{t("hermes_home_label")}</label>
        {generalLoading ? (
          <p className="text-xs text-ac-stone">{t("loading_dots")}</p>
        ) : generalInfo ? (
          <p className="text-sm font-mono text-ac-ivory break-all">{generalInfo.hermes_home}</p>
        ) : null}
      </div>
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
            <label className="text-[11px] text-ac-stone mb-1 block">{t("url_label")}</label>
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
              <label className="text-[11px] text-ac-stone mb-1 block">{t("host_label")}</label>
              <input type="text" value={localSshHost} onChange={(e) => setLocalSshHost(e.target.value)} placeholder="example.com" className="ac-input w-full px-3 py-2 text-sm" />
            </div>
            <div>
              <label className="text-[11px] text-ac-stone mb-1 block">{t("port_label")}</label>
              <input type="text" value={localSshPort} onChange={(e) => setLocalSshPort(e.target.value)} placeholder="22" className="ac-input w-full px-3 py-2 text-sm" />
            </div>
          </div>
          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className="text-[11px] text-ac-stone mb-1 block">{t("user_label")}</label>
              <input type="text" value={localSshUser} onChange={(e) => setLocalSshUser(e.target.value)} placeholder="user" className="ac-input w-full px-3 py-2 text-sm" />
            </div>
            <div>
              <label className="text-[11px] text-ac-stone mb-1 block">{t("ssh_key_label")}</label>
              <input type="text" value={localSshKey} onChange={(e) => setLocalSshKey(e.target.value)} placeholder="~/.ssh/id_rsa" className="ac-input w-full px-3 py-2 text-sm" />
            </div>
          </div>
          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className="text-[11px] text-ac-stone mb-1 block">{t("remote_port_label")}</label>
              <input type="text" value={localSshRemotePort} onChange={(e) => setLocalSshRemotePort(e.target.value)} placeholder="8642" className="ac-input w-full px-3 py-2 text-sm" />
            </div>
            <div>
              <label className="text-[11px] text-ac-stone mb-1 block">{t("local_port_label")}</label>
              <input type="text" value={localSshLocalPort} onChange={(e) => setLocalSshLocalPort(e.target.value)} placeholder="18642" className="ac-input w-full px-3 py-2 text-sm" />
            </div>
          </div>
        </div>
      )}

      {loading && <p className="text-xs text-ac-stone mt-2">{t("saving_dots")}</p>}

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
        <label className="text-[11px] text-ac-stone mb-1 block">{t("bot_token_label")}</label>
        <input
          type="password"
          value={botToken}
          onChange={(e) => setBotToken(e.target.value)}
          placeholder="123456:ABC-DEF..."
          className="ac-input w-full px-3 py-2 text-sm font-mono"
        />
      </div>

      <div>
        <label className="text-[11px] text-ac-stone mb-1 block">{t("chat_id_label")}</label>
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

  // Load models on mount
  useEffect(() => {
    loadModels();
    loadModelConfig();
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
      <div className="flex items-center justify-between">
        <label className="ac-section-title">{t("saved_models_title")}</label>
        <button
          onClick={() => setShowAddForm(!showAddForm)}
          className="ac-btn px-3 py-1 text-xs"
        >
          {showAddForm ? t("cancel_add") : t("add_model")}
        </button>
      </div>

      {showAddForm && (
        <div className="border border-ac-border rounded p-3 space-y-3">
          <div>
            <label className="text-[11px] text-ac-stone mb-1 block">{t("model_name_label")}</label>
            <input
              type="text"
              value={newName}
              onChange={(e) => setNewName(e.target.value)}
              placeholder={t("model_name_placeholder")}
              className="ac-input w-full px-3 py-2 text-sm"
            />
          </div>
          <div>
            <label className="text-[11px] text-ac-stone mb-1 block">{t("provider_select_label")}</label>
            <select
              value={newProvider}
              onChange={(e) => setNewProvider(e.target.value)}
              className="ac-input w-full px-3 py-2 text-sm"
            >
              <option value="openrouter">OpenRouter</option>
              <option value="openai">OpenAI</option>
              <option value="anthropic">Anthropic</option>
              <option value="ollama">Ollama</option>
              <option value="ollama-cloud">Ollama Cloud</option>
            </select>
          </div>
          <div>
            <label className="text-[11px] text-ac-stone mb-1 block">{t("model_field_label")}</label>
            <input
              type="text"
              value={newModel}
              onChange={(e) => setNewModel(e.target.value)}
              placeholder={t("model_placeholder")}
              className="ac-input w-full px-3 py-2 text-sm font-mono"
            />
          </div>
          <div>
            <label className="text-[11px] text-ac-stone mb-1 block">{t("base_url_label")}</label>
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
        <p className="text-xs text-ac-stone">{t("model_loading")}</p>
      ) : models.length === 0 ? (
        <p className="text-xs text-ac-stone">{t("no_saved_models")}</p>
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
                  isActive ? "bg-ac-amber/10 border border-ac-amber/30" : "bg-ac-bg border border-ac-border"
                }`}
              >
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2">
                    <span className="text-ac-ivory font-medium truncate">{m.name}</span>
                    {isActive && (
                      <span className="text-[10px] text-ac-amber px-1.5 py-0.5 rounded bg-ac-amber/20">{t("active_badge_model")}</span>
                    )}
                  </div>
                  <div className="text-[11px] text-ac-stone font-mono truncate">
                    {m.provider}/{m.model}
                  </div>
                </div>
                <div className="flex items-center gap-1 shrink-0 ml-2">
                  {!isActive && (
                    <button
                      onClick={() => handleSetActive(m.provider, m.model, m.base_url)}
                      className="text-[11px] text-ac-amber hover:text-ac-amber/80 px-2 py-1"
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
        <p className={`text-xs ${addStatus.startsWith("✓") ? "text-green-400" : "text-ac-red"}`}>
          {addStatus}
        </p>
      )}
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
        <label className="text-[11px] text-ac-stone mb-1 block">{t("terminal_cwd_label")}</label>
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

// ── Main Settings Panel ───────────────────────────────────────────────────
export function SettingsPanel({ onClose }: { onClose: () => void }) {
  const [activeTab, setActiveTab] = useState<SettingsTab>("general");
  const { t } = useTranslation();

  const tabs: { id: SettingsTab; label: string; icon: typeof Sun }[] = [
    { id: "general", label: t("settings_general"), icon: Sun },
    { id: "connection", label: t("settings_connection"), icon: Globe },
    { id: "telegram", label: t("settings_telegram"), icon: Send },
    { id: "models", label: t("settings_models"), icon: Cpu },
    { id: "terminal", label: t("settings_terminal"), icon: TermIcon },
  ];

  return (
    <div className="ac-modal-overlay">
      <div className="ac-modal" style={{ maxWidth: 640 }}>
        <div className="flex items-center justify-between mb-5">
          <h2 className="text-base font-semibold text-ac-ivory">{t("settings_title")}</h2>
          <button
            onClick={onClose}
            className="text-ac-stone hover:text-ac-ivory transition-colors"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        <div className="flex gap-1 mb-4 border-b border-ac-border overflow-x-auto">
          {tabs.map((tab) => (
            <button
              key={tab.id}
              onClick={() => setActiveTab(tab.id)}
              className={`flex items-center gap-1.5 px-3 py-2 text-xs transition-colors whitespace-nowrap ${
                activeTab === tab.id
                  ? "text-ac-amber border-b-2 border-ac-amber"
                  : "text-ac-stone hover:text-ac-ivory"
              }`}
            >
              <tab.icon className="w-3 h-3" />
              {tab.label}
            </button>
          ))}
        </div>

        <div className="min-h-[200px]">
          {activeTab === "general" && <GeneralTab />}
          {activeTab === "connection" && <ConnectionTab />}
          {activeTab === "telegram" && <TelegramTab />}
          {activeTab === "models" && <ModelsTab />}
          {activeTab === "terminal" && <TerminalTab />}
        </div>

        <div className="flex justify-end gap-2 pt-4 mt-4 border-t border-ac-border">
          <button
            onClick={onClose}
            className="px-4 py-2 text-sm border border-ac-border text-ac-stone hover:text-ac-ivory transition-colors"
          >
            {t("close")}
          </button>
        </div>
      </div>
    </div>
  );
}