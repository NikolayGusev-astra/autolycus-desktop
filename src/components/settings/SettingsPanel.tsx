import { useState } from "react";
import { X, Server, Globe, Moon, Sun } from "lucide-react";

interface SettingsPanelProps {
  onClose: () => void;
}

export function SettingsPanel({ onClose }: SettingsPanelProps) {
  const [backendUrl, setBackendUrl] = useState("");
  const [darkMode, setDarkMode] = useState(true);
  const [proxyEnabled, setProxyEnabled] = useState(false);
  const [proxyHost, setProxyHost] = useState("");
  const [proxyPort, setProxyPort] = useState("1080");

  return (
    <div className="ac-modal-overlay">
      <div className="ac-modal" style={{ maxWidth: 520 }}>
        {/* Header */}
        <div className="flex items-center justify-between mb-5">
          <h2 className="text-base font-semibold text-ac-ivory">Настройки</h2>
          <button
            onClick={onClose}
            className="text-ac-stone hover:text-ac-ivory transition-colors"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Backend URL — hidden from CEO, filled by partner */}
        <div className="mb-4">
          <label className="ac-section-title flex items-center gap-1.5 mb-1.5">
            <Server className="w-3 h-3" />
            Backend URL
          </label>
          <input
            type="text"
            value={backendUrl}
            onChange={(e) => setBackendUrl(e.target.value)}
            placeholder="wss://hermes.example.com:8443"
            className="ac-input w-full px-3 py-2 text-sm"
          />
          <p className="text-[10px] text-ac-stone mt-1">
            Заполняется партнёром при установке
          </p>
        </div>

        {/* Theme */}
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

        {/* Proxy (collapsed by default) */}
        <details className="mb-4">
          <summary className="ac-section-title cursor-pointer flex items-center gap-1.5 mb-1.5">
            <Globe className="w-3 h-3" />
            Прокси (Telegram)
          </summary>
          <div className="pl-5">
            <label className="flex items-center gap-2 mb-2 text-sm text-ac-ivory">
              <input
                type="checkbox"
                checked={proxyEnabled}
                onChange={(e) => setProxyEnabled(e.target.checked)}
                className="accent-ac-amber"
              />
              Включить SOCKS5 прокси
            </label>
            {proxyEnabled && (
              <div className="flex gap-2">
                <input
                  type="text"
                  value={proxyHost}
                  onChange={(e) => setProxyHost(e.target.value)}
                  placeholder="192.168.1.x"
                  className="ac-input flex-1 px-3 py-1.5 text-xs"
                />
                <input
                  type="text"
                  value={proxyPort}
                  onChange={(e) => setProxyPort(e.target.value)}
                  placeholder="1080"
                  className="ac-input w-20 px-3 py-1.5 text-xs"
                />
              </div>
            )}
          </div>
        </details>

        {/* Actions */}
        <div className="flex justify-end gap-2 pt-2 border-t border-ac-border">
          <button
            onClick={onClose}
            className="px-4 py-2 text-sm border border-ac-border text-ac-stone hover:text-ac-ivory transition-colors"
          >
            Закрыть
          </button>
          <button className="ac-btn px-4 py-2 text-sm">
            Сохранить
          </button>
        </div>
      </div>
    </div>
  );
}
