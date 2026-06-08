// src/components/settings/SettingsPanel.tsx
// v0.4.0: Settings with connection tab

import { useState } from "react";
import { X, Server, Globe, Shield, Moon, Sun } from "lucide-react";

interface SettingsPanelProps {
  onClose: () => void;
}

type SettingsTab = "general" | "connection";

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

  const tabs = [
    { id: "general" as const, label: "Основные" },
    { id: "connection" as const, label: "Подключение" },
  ];

  return (
    <div className="ac-modal-overlay">
      <div className="ac-modal" style={{ maxWidth: 600 }}>
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

        {/* Tabs */}
        <div className="flex gap-1 mb-4 border-b border-ac-border">
          {tabs.map((tab) => (
            <button
              key={tab.id}
              onClick={() => setActiveTab(tab.id)}
              className={`px-4 py-2 text-sm transition-colors ${
                activeTab === tab.id
                  ? "text-ac-amber border-b-2 border-ac-amber"
                  : "text-ac-stone hover:text-ac-ivory"
              }`}
            >
              {tab.label}
            </button>
          ))}
        </div>

        {/* General tab */}
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

        {/* Connection tab */}
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
                    <input
                      type="text"
                      value={sshHost}
                      onChange={(e) => setSshHost(e.target.value)}
                      className="ac-input w-full px-3 py-2 text-sm"
                    />
                  </div>
                  <div>
                    <label className="text-[11px] text-ac-stone mb-1 block">Порт</label>
                    <input
                      type="text"
                      value={sshPort}
                      onChange={(e) => setSshPort(e.target.value)}
                      className="ac-input w-full px-3 py-2 text-sm"
                    />
                  </div>
                </div>
                <div className="grid grid-cols-2 gap-3">
                  <div>
                    <label className="text-[11px] text-ac-stone mb-1 block">Пользователь</label>
                    <input
                      type="text"
                      value={sshUser}
                      onChange={(e) => setSshUser(e.target.value)}
                      className="ac-input w-full px-3 py-2 text-sm"
                    />
                  </div>
                  <div>
                    <label className="text-[11px] text-ac-stone mb-1 block">SSH Key</label>
                    <input
                      type="text"
                      value={sshKey}
                      onChange={(e) => setSshKey(e.target.value)}
                      className="ac-input w-full px-3 py-2 text-sm"
                    />
                  </div>
                </div>
                <div className="grid grid-cols-2 gap-3">
                  <div>
                    <label className="text-[11px] text-ac-stone mb-1 block">Удалённый порт</label>
                    <input
                      type="text"
                      value={sshRemotePort}
                      onChange={(e) => setSshRemotePort(e.target.value)}
                      className="ac-input w-full px-3 py-2 text-sm"
                    />
                  </div>
                  <div>
                    <label className="text-[11px] text-ac-stone mb-1 block">Локальный порт</label>
                    <input
                      type="text"
                      value={sshLocalPort}
                      onChange={(e) => setSshLocalPort(e.target.value)}
                      className="ac-input w-full px-3 py-2 text-sm"
                    />
                  </div>
                </div>
              </div>
            )}
          </div>
        )}

        {/* Actions */}
        <div className="flex justify-end gap-2 pt-4 mt-4 border-t border-ac-border">
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
