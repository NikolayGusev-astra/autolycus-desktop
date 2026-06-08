// src/components/ConnectionScreen.tsx
// v0.4.0: Multi-mode connection screen (local/remote/ssh)

import { useState, useEffect } from "react";
import {
  Server,
  Globe,
  Shield,
  Loader,
  Play,
  Check,
  X,
  FolderOpen,
  Settings,
} from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { useConnectionStore, type ConnectionMode, type SshConfig } from "../stores/connectionStore";

interface InstanceInfo {
  path: string;
  instance: string;
  exists: boolean;
}

interface ConnectionScreenProps {
  onConnected: () => void;
  error: string | null;
}

export function ConnectionScreen({ onConnected, error }: ConnectionScreenProps) {
  const [mode, setMode] = useState<ConnectionMode>("local");
  const [instances, setInstances] = useState<InstanceInfo[]>([]);
  const [selectedIdx, setSelectedIdx] = useState<number>(-1);
  const [customPath, setCustomPath] = useState("");
  const [connecting, setConnecting] = useState(false);
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<boolean | null>(null);

  // Remote mode state
  const [remoteUrl, setRemoteUrl] = useState("https://ammg.duckdns.org:8443");
  const [remoteApiKey, setRemoteApiKey] = useState("");

  // SSH mode state
  const [sshConfig, setSshConfig] = useState<SshConfig>({
    host: "147.90.10.50",
    port: 22,
    username: "root",
    key_path: "~/.ssh/id_rsa",
    remote_port: 8642,
    local_port: 18642,
  });

  const { loadConfig, saveConfig, testConnection } = useConnectionStore();

  useEffect(() => {
    loadInstances();
    loadConfig();
  }, []);

  const loadInstances = async () => {
    try {
      const result = await invoke<InstanceInfo[]>("detect_instances");
      setInstances(result);
      const firstExisting = result.findIndex((i) => i.exists);
      if (firstExisting >= 0) {
        setSelectedIdx(firstExisting);
      }
    } catch (err) {
      console.error("Failed to detect instances:", err);
    }
  };

  const handleConnect = async () => {
    setConnecting(true);
    setTestResult(null);

    try {
      // Save connection config
      await saveConfig({
        mode,
        remote_url: remoteUrl,
        ssh: sshConfig,
      });

      if (mode === "local") {
        const pythonPath =
          selectedIdx >= 0 && instances[selectedIdx]
            ? instances[selectedIdx].path
            : customPath.trim();

        if (!pythonPath) {
          throw new Error("Select a Python instance or enter a custom path");
        }

        // Start local gateway
        await invoke("start_gateway_cmd", { profile: null });
      } else if (mode === "ssh") {
        // Start SSH tunnel
        await invoke("start_ssh_tunnel_cmd", { sshConfig });
      }

      // For remote mode, just mark as connected
      onConnected();
    } catch (err) {
      console.error("Connection failed:", err);
    } finally {
      setConnecting(false);
    }
  };

  const handleTest = async () => {
    setTesting(true);
    setTestResult(null);

    try {
      const result = await testConnection(mode, remoteUrl, sshConfig);
      setTestResult(result);
    } catch {
      setTestResult(false);
    } finally {
      setTesting(false);
    }
  };

  const existingInstances = instances.filter((i) => i.exists);

  return (
    <div className="fixed inset-0 bg-ac-pitch flex items-center justify-center">
      <div className="w-full max-w-lg px-6">
        <div className="text-center mb-8">
          <div className="ac-display mb-2">Автолик</div>
          <p className="text-sm text-ac-stone">AI Ассистент v0.4.0</p>
        </div>

        <div className="ac-card">
          {/* Mode selector */}
          <div className="flex items-center gap-2 mb-4">
            <Settings className="w-4 h-4 text-ac-amber" />
            <span className="ac-section-title">Режим подключения</span>
          </div>

          <div className="flex gap-2 mb-6">
            <button
              onClick={() => setMode("local")}
              className={`ac-pill flex items-center gap-1.5 ${mode === "local" ? "active" : ""}`}
            >
              <Server className="w-3 h-3" />
              Локальный
            </button>
            <button
              onClick={() => setMode("remote")}
              className={`ac-pill flex items-center gap-1.5 ${mode === "remote" ? "active" : ""}`}
            >
              <Globe className="w-3 h-3" />
              Удалённый
            </button>
            <button
              onClick={() => setMode("ssh")}
              className={`ac-pill flex items-center gap-1.5 ${mode === "ssh" ? "active" : ""}`}
            >
              <Shield className="w-3 h-3" />
              SSH
            </button>
          </div>

          {/* Local mode */}
          {mode === "local" && (
            <>
              {existingInstances.length > 0 && (
                <div className="mb-4">
                  <label className="text-[11px] text-ac-stone mb-2 block">
                    Найденные установки:
                  </label>
                  <div className="space-y-1.5 max-h-32 overflow-y-auto">
                    {instances.map((inst, idx) => (
                      <button
                        key={inst.path}
                        onClick={() => inst.exists && setSelectedIdx(idx)}
                        disabled={!inst.exists}
                        className={`w-full text-left px-3 py-2 text-xs border transition-colors flex items-center gap-2 ${
                          !inst.exists
                            ? "border-ac-border/30 text-ac-stone/30 cursor-not-allowed"
                            : selectedIdx === idx
                            ? "border-ac-amber/30 bg-ac-amber/8 text-ac-amber"
                            : "border-ac-border text-ac-stone hover:border-ac-stone/30"
                        }`}
                      >
                        {inst.exists ? (
                          <Check className="w-3 h-3 text-green-500 flex-shrink-0" />
                        ) : (
                          <X className="w-3 h-3 text-ac-stone/30 flex-shrink-0" />
                        )}
                        <FolderOpen className="w-3 h-3 flex-shrink-0" />
                        <span className="truncate">{inst.path}</span>
                        <span className="ml-auto text-[10px] opacity-50">
                          {inst.instance}
                        </span>
                      </button>
                    ))}
                  </div>
                </div>
              )}

              <div className="mb-4">
                <label className="text-[11px] text-ac-stone mb-1 block">
                  Или укажите путь к Python:
                </label>
                <input
                  type="text"
                  value={customPath}
                  onChange={(e) => setCustomPath(e.target.value)}
                  placeholder="~/autolycus/venv/bin/python"
                  className="ac-input w-full px-3 py-2 text-sm"
                />
              </div>
            </>
          )}

          {/* Remote mode */}
          {mode === "remote" && (
            <>
              <div className="mb-4">
                <label className="text-[11px] text-ac-stone mb-1 block">
                  URL удалённого Hermes:
                </label>
                <input
                  type="text"
                  value={remoteUrl}
                  onChange={(e) => setRemoteUrl(e.target.value)}
                  placeholder="https://hermes.example.com:8443"
                  className="ac-input w-full px-3 py-2 text-sm"
                />
              </div>
              <div className="mb-4">
                <label className="text-[11px] text-ac-stone mb-1 block">
                  API Key (опционально):
                </label>
                <input
                  type="password"
                  value={remoteApiKey}
                  onChange={(e) => setRemoteApiKey(e.target.value)}
                  placeholder="sk-..."
                  className="ac-input w-full px-3 py-2 text-sm"
                />
              </div>
            </>
          )}

          {/* SSH mode */}
          {mode === "ssh" && (
            <>
              <div className="grid grid-cols-2 gap-3 mb-4">
                <div>
                  <label className="text-[11px] text-ac-stone mb-1 block">Хост</label>
                  <input
                    type="text"
                    value={sshConfig.host}
                    onChange={(e) => setSshConfig({ ...sshConfig, host: e.target.value })}
                    placeholder="147.90.10.50"
                    className="ac-input w-full px-3 py-2 text-sm"
                  />
                </div>
                <div>
                  <label className="text-[11px] text-ac-stone mb-1 block">Порт SSH</label>
                  <input
                    type="number"
                    value={sshConfig.port}
                    onChange={(e) => setSshConfig({ ...sshConfig, port: parseInt(e.target.value) || 22 })}
                    className="ac-input w-full px-3 py-2 text-sm"
                  />
                </div>
              </div>
              <div className="grid grid-cols-2 gap-3 mb-4">
                <div>
                  <label className="text-[11px] text-ac-stone mb-1 block">Пользователь</label>
                  <input
                    type="text"
                    value={sshConfig.username}
                    onChange={(e) => setSshConfig({ ...sshConfig, username: e.target.value })}
                    className="ac-input w-full px-3 py-2 text-sm"
                  />
                </div>
                <div>
                  <label className="text-[11px] text-ac-stone mb-1 block">SSH Key</label>
                  <input
                    type="text"
                    value={sshConfig.key_path}
                    onChange={(e) => setSshConfig({ ...sshConfig, key_path: e.target.value })}
                    className="ac-input w-full px-3 py-2 text-sm"
                  />
                </div>
              </div>
              <div className="grid grid-cols-2 gap-3 mb-4">
                <div>
                  <label className="text-[11px] text-ac-stone mb-1 block">Удалённый порт</label>
                  <input
                    type="number"
                    value={sshConfig.remote_port}
                    onChange={(e) => setSshConfig({ ...sshConfig, remote_port: parseInt(e.target.value) || 8642 })}
                    className="ac-input w-full px-3 py-2 text-sm"
                  />
                </div>
                <div>
                  <label className="text-[11px] text-ac-stone mb-1 block">Локальный порт</label>
                  <input
                    type="number"
                    value={sshConfig.local_port}
                    onChange={(e) => setSshConfig({ ...sshConfig, local_port: parseInt(e.target.value) || 18642 })}
                    className="ac-input w-full px-3 py-2 text-sm"
                  />
                </div>
              </div>
            </>
          )}

          {/* Error display */}
          {error && (
            <div className="mb-3 px-3 py-2 bg-red-500/5 border border-red-500/20 text-ac-red text-xs">
              {error}
            </div>
          )}

          {/* Test result */}
          {testResult !== null && (
            <div className={`mb-3 px-3 py-2 text-xs flex items-center gap-2 ${testResult ? "text-green-400" : "text-red-400"}`}>
              {testResult ? <Check className="w-3 h-3" /> : <X className="w-3 h-3" />}
              {testResult ? "Подключение успешно" : "Не удалось подключиться"}
            </div>
          )}

          {/* Actions */}
          <div className="flex gap-2">
            <button
              onClick={handleTest}
              disabled={connecting || testing}
              className="px-4 py-2.5 text-sm border border-ac-border text-ac-stone hover:text-ac-ivory hover:border-ac-stone transition-colors flex items-center gap-2 disabled:opacity-30"
            >
              {testing ? (
                <Loader className="w-4 h-4 animate-spin" />
              ) : (
                <Check className="w-4 h-4" />
              )}
              Тест
            </button>
            <button
              onClick={handleConnect}
              disabled={connecting || testing}
              className="ac-btn flex-1 py-2.5 text-sm flex items-center justify-center gap-2 disabled:opacity-30 disabled:cursor-not-allowed"
            >
              {connecting ? (
                <>
                  <Loader className="w-4 h-4 animate-spin" />
                  Подключение...
                </>
              ) : (
                <>
                  <Play className="w-4 h-4" />
                  Подключиться
                </>
              )}
            </button>
          </div>
        </div>

        <div className="mt-4 text-center">
          <p className="text-[11px] text-ac-stone/50">
            v0.4.0 — local / remote / SSH tunnel
          </p>
        </div>
      </div>
    </div>
  );
}
