// src/components/settings/SettingsPanel.tsx
// v0.5.0: Extended settings with sections for all modules

import { useState } from "react";
import { X, Server, Globe, Shield, Moon, Sun, Send, Cpu, Terminal as TermIcon } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";

interface SettingsPanelProps {
  onClose: () => void;
}

type SettingsTab = "general" | "connection" | "telegram" | "models" | "terminal";

// ── Telegram tab content ──────────────────────────────────────────────────
function TelegramTab() {
  const [botToken, setBotToken] = useState("");
  const [chatId, setChatId] = useState("");
  const [enabled, setEnabled] = useState(false);
  const [status, setStatus] = useState("");
  const [saved, setSaved] = useState(false);

  const handleSave = async () => {
    try {
      await invoke("save_telegram_config_cmd", {
        config: { bot_token: botToken, chat_id: chatId, enabled },
      });
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    } catch (e: any) {
      setStatus(e.message || "Ошибка сохранения");
    }
  };

  const handleValidate = async () => {
    if (!botToken) { setStatus("Введите bot token"); return; }
    try {
      const result = await invoke<{ success: boolean; error?: string }>(
        "validate_telegram_bot_token_cmd",
        { botToken }
      );
      setStatus(result.success ? "✓ Токен валиден" : `✗ ${result.error || "Невалидный токен"}`);
    } catch (e: any) {
      setStatus(e.message || "Ошибка валидации");
    }
  };

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <label className="ac-section-title">Включить Telegram</label>
        <button
          onClick={() => setEnabled(!enabled)}
          className={`ac-pill ${enabled ? "active" : ""}`}
        >
          {enabled ? "Вкл" : "Выкл"}
        </button>
      </div>

      <div>
        <label className="text-[11px] text-ac-stone mb-1 block">Bot Token</label>
        <input
          type="password"
          value={botToken}
          onChange={(e) => setBotToken(e.target.value)}
          placeholder="123456:ABC-DEF..."
          className="ac-input w-full px-3 py-2 text-sm font-mono"
        />
      </div>

      <div>
        <label className="text-[11px] text-ac-stone mb-1 block">Chat ID</label>
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
          Проверить токен
        </button>
        <button onClick={handleSave} className="ac-btn px-3 py-1.5 text-xs">
          Сохранить
        </button>
      </div>

      {status && (
        <p className={`text-xs ${status.startsWith("✓") ? "text-green-400" : "text-ac-red"}`}>
          {status}
        </p>
      )}
      {saved && <p className="text-xs text-green-400">✓ Сохранено</p>}
    </div>
  );
}

// ── Models tab content ────────────────────────────────────────────────────
function ModelsTab() {
  const [provider, setProvider] = useState("openrouter");
  const [model, setModel] = useState("openrouter/owl-alpha");
  const [baseUrl, setBaseUrl] = useState("https://openrouter.ai/api/v1");
  const [discoveredModels, setDiscoveredModels] = useState<Array<{ id: string; name: string }>>([]);
  const [discovering, setDiscovering] = useState(false);

  const handleDiscover = async () => {
    setDiscovering(true);
    try {
      const result = await invoke<{
        success: boolean;
        models: Array<{ id: string; name: string }>;
        error?: string;
      }>("discover_models_cmd", { provider, baseUrl: baseUrl || undefined });
      if (result.success) {
        setDiscoveredModels(result.models);
      }
    } catch (e: any) {
      console.error("Model discovery error:", e);
    }
    setDiscovering(false);
  };

  return (
    <div className="space-y-4">
      <div>
        <label className="text-[11px] text-ac-stone mb-1 block">Провайдер</label>
        <select
          value={provider}
          onChange={(e) => setProvider(e.target.value)}
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
        <label className="text-[11px] text-ac-stone mb-1 block">Base URL</label>
        <input
          type="text"
          value={baseUrl}
          onChange={(e) => setBaseUrl(e.target.value)}
          className="ac-input w-full px-3 py-2 text-sm font-mono"
        />
      </div>

      <div>
        <label className="text-[11px] text-ac-stone mb-1 block">Модель</label>
        <div className="flex gap-2">
          <input
            type="text"
            value={model}
            onChange={(e) => setModel(e.target.value)}
            placeholder="provider/model"
            className="ac-input flex-1 px-3 py-2 text-sm font-mono"
          />
          <button
            onClick={handleDiscover}
            disabled={discovering}
            className="ac-btn px-3 py-1.5 text-xs whitespace-nowrap"
          >
            {discovering ? "..." : "Обнаружить"}
          </button>
        </div>
      </div>

      {discoveredModels.length > 0 && (
        <div>
          <label className="text-[11px] text-ac-stone mb-1 block">
            Найдено моделей: {discoveredModels.length}
          </label>
          <div className="max-h-40 overflow-y-auto border border-ac-border rounded">
            {discoveredModels.slice(0, 50).map((m) => (
              <button
                key={m.id}
                onClick={() => setModel(m.id)}
                className="w-full text-left px-3 py-1.5 text-xs font-mono hover:bg-ac-highlight transition-colors"
              >
                <span className="text-ac-ivory">{m.id}</span>
                {m.name !== m.id && (
                  <span className="text-ac-stone ml-2">{m.name}</span>
                )}
              </button>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

// ── Terminal tab content ──────────────────────────────────────────────────
function TerminalTab() {
  const [cwd, setCwd] = useState("~");
  const [status, setStatus] = useState("");

  const handleOpen = async () => {
    try {
      const result = await invoke<{ success: boolean; error?: string }>("open_terminal_cmd", { cwd });
      setStatus(result.success ? "✓ Терминал открыт" : `✗ ${result.error}`);
    } catch (e: any) {
      setStatus(e.message || "Ошибка");
    }
  };

  return (
    <div className="space-y-4">
      <div>
        <label className="text-[11px] text-ac-stone mb-1 block">Рабочая директория</label>
        <input
          type="text"
          value={cwd}
          onChange={(e) => setCwd(e.target.value)}
          placeholder="~ или /path/to/dir"
          className="ac-input w-full px-3 py-2 text-sm font-mono"
        />
      </div>

      <button onClick={handleOpen} className="ac-btn px-4 py-2 text-sm">
        Открыть терминал
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
export function SettingsPanel({ onClose }: SettingsPanelProps) {
  const [activeTab, setActiveTab] = useState<SettingsTab>("general");
  const [darkMode, setDarkMode] = useState(true);

  // Connection settings
  const [connMode, setConnMode] = useState<"local" | "remote" | "ssh">("local");
  const [remoteUrl, setRemoteUrl] = useState("https://ammg.duckdns.org:8443");
  const [sshHost, setSshHost] = useState("147.90.10.50");
  const [sshPort, setSshPort] = useState("22");
  const [sshUser, setSshUser] = useState("root");
  const [sshKey, setSshKey] = useState("~/.ssh/id_rsa");
  const [sshRemotePort, setSshRemotePort] = useState("8642");
  const [sshLocalPort, setSshLocalPort] = useState("18642");

  const tabs: { id: SettingsTab; label: string; icon: typeof Sun }[] = [
    { id: "general", label: "Основные", icon: Sun },
    { id: "connection", label: "Подключение", icon: Globe },
    { id: "telegram", label: "Telegram", icon: Send },
    { id: "models", label: "Модели", icon: Cpu },
    { id: "terminal", label: "Терминал", icon: TermIcon },
  ];

  return (
    <div className="ac-modal-overlay">
      <div className="ac-modal" style={{ maxWidth: 640 }}>
        <div className="flex items-center justify-between mb-5">
          <h2 className="text-base font-semibold text-ac-ivory">Настройки</h2>
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
          {activeTab === "general" && (
            <div>
              <div className="mb-4">
                <label className="ac-section-title mb-1.5 block">Тема</label>
                <div className="flex gap-2">
                  <button
                    onClick={() => setDarkMode(true)}
                    className={`ac-pill flex items-center gap-1.5 ${darkMode ? "active" : ""}`}
                  >
                    <Moon className="w-3 h-3" />
                    Тёмная
                  </button>
                  <button
                    onClick={() => setDarkMode(false)}
                    className={`ac-pill flex items-center gap-1.5 ${!darkMode ? "active" : ""}`}
                  >
                    <Sun className="w-3 h-3" />
                    Светлая
                  </button>
                </div>
              </div>
            </div>
          )}

          {activeTab === "connection" && (
            <div>
              <div className="flex gap-2 mb-4">
                <button
                  onClick={() => setConnMode("local")}
                  className={`ac-pill flex items-center gap-1.5 ${connMode === "local" ? "active" : ""}`}
                >
                  <Server className="w-3 h-3" />
                  Локальный
                </button>
                <button
                  onClick={() => setConnMode("remote")}
                  className={`ac-pill flex items-center gap-1.5 ${connMode === "remote" ? "active" : ""}`}
                >
                  <Globe className="w-3 h-3" />
                  Удалённый
                </button>
                <button
                  onClick={() => setConnMode("ssh")}
                  className={`ac-pill flex items-center gap-1.5 ${connMode === "ssh" ? "active" : ""}`}
                >
                  <Shield className="w-3 h-3" />
                  SSH
                </button>
              </div>

              {connMode === "remote" && (
                <div className="space-y-3">
                  <div>
                    <label className="text-[11px] text-ac-stone mb-1 block">URL</label>
                    <input
                      type="text"
                      value={remoteUrl}
                      onChange={(e) => setRemoteUrl(e.target.value)}
                      placeholder="https://hermes.example.com:8443"
                      className="ac-input w-full px-3 py-2 text-sm"
                    />
                  </div>
                </div>
              )}

              {connMode === "ssh" && (
                <div className="space-y-3">
                  <div className="grid grid-cols-2 gap-3">
                    <div>
                      <label className="text-[11px] text-ac-stone mb-1 block">Хост</label>
                      <input type="text" value={sshHost} onChange={(e) => setSshHost(e.target.value)} className="ac-input w-full px-3 py-2 text-sm" />
                    </div>
                    <div>
                      <label className="text-[11px] text-ac-stone mb-1 block">Порт</label>
                      <input type="text" value={sshPort} onChange={(e) => setSshPort(e.target.value)} className="ac-input w-full px-3 py-2 text-sm" />
                    </div>
                  </div>
                  <div className="grid grid-cols-2 gap-3">
                    <div>
                      <label className="text-[11px] text-ac-stone mb-1 block">Пользователь</label>
                      <input type="text" value={sshUser} onChange={(e) => setSshUser(e.target.value)} className="ac-input w-full px-3 py-2 text-sm" />
                    </div>
                    <div>
                      <label className="text-[11px] text-ac-stone mb-1 block">SSH Key</label>
                      <input type="text" value={sshKey} onChange={(e) => setSshKey(e.target.value)} className="ac-input w-full px-3 py-2 text-sm" />
                    </div>
                  </div>
                  <div className="grid grid-cols-2 gap-3">
                    <div>
                      <label className="text-[11px] text-ac-stone mb-1 block">Удалённый порт</label>
                      <input type="text" value={sshRemotePort} onChange={(e) => setSshRemotePort(e.target.value)} className="ac-input w-full px-3 py-2 text-sm" />
                    </div>
                    <div>
                      <label className="text-[11px] text-ac-stone mb-1 block">Локальный порт</label>
                      <input type="text" value={sshLocalPort} onChange={(e) => setSshLocalPort(e.target.value)} className="ac-input w-full px-3 py-2 text-sm" />
                    </div>
                  </div>
                </div>
              )}
            </div>
          )}

          {activeTab === "telegram" && <TelegramTab />}
          {activeTab === "models" && <ModelsTab />}
          {activeTab === "terminal" && <TerminalTab />}
        </div>

        <div className="flex justify-end gap-2 pt-4 mt-4 border-t border-ac-border">
          <button
            onClick={onClose}
            className="px-4 py-2 text-sm border border-ac-border text-ac-stone hover:text-ac-ivory transition-colors"
          >
            Закрыть
          </button>
        </div>
      </div>
    </div>
  );
}
