// src/components/ConnectScreen.tsx
import { useState, useEffect } from "react";
import { Server, Loader, Play, Check, X, FolderOpen, Globe, Terminal } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "../hooks/useTranslation";

interface InstanceInfo {
  path: string;
  instance: string;
  exists: boolean;
}

interface RemoteInstanceInfo {
  path: string;
  instance: string;
  exists: boolean;
}

type ConnectionMode = "local" | "remote" | "ssh";

interface ConnectScreenProps {
  onStartLocal: (pythonPath: string) => void;
  onConnected: () => void;
  connecting: boolean;
  starting: boolean;
  error: string | null;
}

export function ConnectScreen({
  onStartLocal,
  onConnected,
  connecting,
  starting,
  error,
}: ConnectScreenProps) {
  const [mode, setMode] = useState<ConnectionMode>("local");
  const { t } = useTranslation();

  return (
    <div className="fixed inset-0 bg-ac-pitch flex items-center justify-center">
      <div className="w-full max-w-md px-6">
        <div className="text-center mb-8">
          <div className="ac-display mb-2">{t("autolycus_title")}</div>
          <p className="text-sm text-ac-stone">{t("ai_assistant")}</p>
        </div>

        {/* Mode Tabs */}
        <div className="flex gap-1 mb-4 bg-ac-bg rounded-lg p-1 border border-ac-border">
          {[
            { id: "local" as ConnectionMode, icon: FolderOpen, label: t("conn.local") },
            { id: "remote" as ConnectionMode, icon: Globe, label: t("conn.remote") },
            { id: "ssh" as ConnectionMode, icon: Terminal, label: t("conn.ssh") },
          ].map((tab) => (
            <button
              key={tab.id}
              onClick={() => setMode(tab.id)}
              className={`flex-1 flex items-center justify-center gap-2 px-3 py-2 text-xs rounded-md transition-colors ${
                mode === tab.id
                  ? "bg-ac-amber/10 text-ac-amber"
                  : "text-ac-stone hover:text-ac-stone/70"
              }`}
            >
              <tab.icon className="w-3.5 h-3.5" />
              {tab.label}
            </button>
          ))}
        </div>

        {mode === "local" && <LocalConnect onStartLocal={onStartLocal} connecting={connecting} starting={starting} error={error} />}
        {mode === "remote" && <RemoteConnect onConnected={onConnected} />}
        {mode === "ssh" && <SshConnect onConnected={onConnected} />}
      </div>
    </div>
  );
}

// ── Local Mode ──────────────────────────────────────────

function LocalConnect({ onStartLocal, connecting, starting, error }: {
  onStartLocal: (pythonPath: string) => void;
  connecting: boolean;
  starting: boolean;
  error: string | null;
}) {
  const [instances, setInstances] = useState<InstanceInfo[]>([]);
  const [selectedIdx, setSelectedIdx] = useState<number>(-1);
  const [customPath, setCustomPath] = useState("");
  const { t } = useTranslation();

  useEffect(() => {
    loadInstances();
  }, []);

  const loadInstances = async () => {
    try {
      const result = await invoke<InstanceInfo[]>("detect_instances");
      setInstances(result);
      const firstExisting = result.findIndex((i) => i.exists);
      if (firstExisting >= 0) setSelectedIdx(firstExisting);
    } catch (err) {
      console.error("Failed to detect instances:", err);
    }
  };

  const handleStart = () => {
    const path = selectedIdx >= 0 ? instances[selectedIdx].path : customPath.trim();
    if (path) onStartLocal(path);
  };

  const existingInstances = instances.filter((i) => i.exists);

  return (
    <div className="ac-card">
      <div className="flex items-center gap-2 mb-4">
        <Server className="w-4 h-4 text-ac-amber" />
        <span className="ac-section-title">{t("connect_title")}</span>
      </div>

      {existingInstances.length > 0 && (
        <div className="mb-4">
          <label className="text-[11px] text-ac-stone mb-2 block">{t("found_instances")}</label>
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
                {inst.exists ? <Check className="w-3 h-3 text-green-500 shrink-0" /> : <X className="w-3 h-3 text-ac-stone/30 shrink-0" />}
                <FolderOpen className="w-3 h-3 shrink-0" />
                <span className="truncate">{inst.path}</span>
                <span className="ml-auto text-[10px] opacity-50">{inst.instance}</span>
              </button>
            ))}
          </div>
        </div>
      )}

      <div className="mb-4">
        <label className="text-[11px] text-ac-stone mb-1 block">{t("or_specify_path")}</label>
        <input type="text" value={customPath} onChange={(e) => setCustomPath(e.target.value)}
          placeholder="~/autolycus/venv/bin/python" className="ac-input w-full px-3 py-2 text-sm" />
      </div>

      {error && (
        <div className="mb-3 px-3 py-2 bg-red-500/5 border border-red-500/20 text-ac-red text-xs flex items-center justify-between">
          <span>{error}</span>
          <button onClick={loadInstances} className="text-ac-amber hover:underline text-[10px]">{t("retry")}</button>
        </div>
      )}

      <button onClick={handleStart} disabled={connecting || starting || (selectedIdx < 0 && !customPath.trim())}
        className="ac-btn w-full py-2.5 text-sm flex items-center justify-center gap-2 disabled:opacity-30 disabled:cursor-not-allowed">
        {starting || connecting ? (
          <><Loader className="w-4 h-4 animate-spin" />{t("starting")}</>
        ) : (
          <><Play className="w-4 h-4" />{t("start")}</>
        )}
      </button>

      <p className="mt-4 text-[11px] text-ac-stone/50 text-center">{t("autostart_info")}</p>
    </div>
  );
}

// ── Remote Mode ─────────────────────────────────────────

function RemoteConnect({ onConnected }: { onConnected: () => void }) {
  const [url, setUrl] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [testing, setTesting] = useState(false);
  const [status, setStatus] = useState<"idle" | "testing" | "ok" | "error">("idle");
  const [statusMsg, setStatusMsg] = useState("");
  const { t } = useTranslation();

  const handleConnect = async () => {
    if (!url.trim()) return;
    setTesting(true);
    setStatus("testing");
    try {
      // First, try to get config and detect remote
      await invoke("set_connection_config", {
        mode: "remote",
        remoteUrl: url.replace(/\/$/, ""),
        apiKey: apiKey,
        sshHost: "", sshPort: 22, sshUsername: "", sshKeyPath: "",
        sshRemotePort: 8891, sshLocalPort: 8891,
      });
      // Health check
      const ok = await invoke<boolean>("test_connection", {
        mode: "remote", url: url.replace(/\/$/, ""), sshConfig: null,
      });
      if (ok) {
        setStatus("ok");
        setStatusMsg("Connected!");
        setTimeout(onConnected, 800);
      } else {
        setStatus("error");
        setStatusMsg("Server unreachable — check URL and API key");
      }
    } catch (err) {
      setStatus("error");
      setStatusMsg(String(err));
    } finally {
      setTesting(false);
    }
  };

  return (
    <div className="ac-card">
      <div className="flex items-center gap-2 mb-4">
        <Globe className="w-4 h-4 text-ac-amber" />
        <span className="ac-section-title">{t("conn.remote")}</span>
      </div>

      <div className="mb-3">
        <label className="text-[11px] text-ac-stone mb-1 block">Server URL</label>
        <input type="text" value={url} onChange={(e) => setUrl(e.target.value)}
          placeholder="https://your-server.com:8443" className="ac-input w-full px-3 py-2 text-sm" />
      </div>

      <div className="mb-4">
        <label className="text-[11px] text-ac-stone mb-1 block">API Key</label>
        <input type="password" value={apiKey} onChange={(e) => setApiKey(e.target.value)}
          placeholder="Optional" className="ac-input w-full px-3 py-2 text-sm" />
      </div>

      {status === "error" && (
        <div className="mb-3 px-3 py-2 bg-red-500/5 border border-red-500/20 text-ac-red text-xs">{statusMsg}</div>
      )}
      {status === "ok" && (
        <div className="mb-3 px-3 py-2 bg-green-500/5 border border-green-500/20 text-green-400 text-xs">{statusMsg}</div>
      )}

      <button onClick={() => void handleConnect()} disabled={testing || !url.trim()}
        className="ac-btn w-full py-2.5 text-sm flex items-center justify-center gap-2 disabled:opacity-30">
        {testing ? <><Loader className="w-4 h-4 animate-spin" />{t("conn.testing")}</> : <><Play className="w-4 h-4" />{t("btn.connect")}</>}
      </button>
    </div>
  );
}

// ── SSH Mode ────────────────────────────────────────────

function SshConnect({ onConnected }: { onConnected: () => void }) {
  const [host, setHost] = useState("");
  const [port, setPort] = useState("22");
  const [username, setUsername] = useState("root");
  const [keyPath, setKeyPath] = useState("~/.ssh/id_ed25519");
  const [remotePort, setRemotePort] = useState("8891");
  const [localPort, setLocalPort] = useState("8891");
  const [remoteInstances, setRemoteInstances] = useState<RemoteInstanceInfo[]>([]);
  const [selectedRemoteIdx, setSelectedRemoteIdx] = useState<number>(-1);
  const [testing, setTesting] = useState(false);
  const [scanning, setScanning] = useState(false);
  const [status, setStatus] = useState<"idle" | "testing" | "ok" | "error">("idle");
  const [statusMsg, setStatusMsg] = useState("");
  const { t } = useTranslation();

  const getSshConfig = () => ({
    host, port: parseInt(port) || 22, username,
    key_path: keyPath,
    remote_port: parseInt(remotePort) || 8891,
    local_port: parseInt(localPort) || 8891,
  });

  const handleScan = async () => {
    if (!host.trim() || !username.trim()) return;
    setScanning(true);
    try {
      const instances = await invoke<RemoteInstanceInfo[]>("detect_remote_instances_cmd", {
        sshConfig: getSshConfig(),
      });
      setRemoteInstances(instances);
      const idx = instances.findIndex((i) => i.exists);
      if (idx >= 0) setSelectedRemoteIdx(idx);
    } catch (err) {
      setStatus("error");
      setStatusMsg(String(err));
    } finally {
      setScanning(false);
    }
  };

  const handleConnect = async () => {
    if (!host.trim()) return;
    setTesting(true);
    setStatus("testing");
    try {
      const cfg = getSshConfig();
      // Save config
      await invoke("set_connection_config", {
        mode: "ssh",
        remoteUrl: "", apiKey: "",
        sshHost: host, sshPort: cfg.port, sshUsername: username, sshKeyPath: keyPath,
        sshRemotePort: cfg.remote_port, sshLocalPort: cfg.local_port,
      });

      // Start tunnel
      await invoke("start_ssh_tunnel_cmd", { sshConfig: cfg });
      setStatus("ok");
      setStatusMsg("SSH tunnel established!");
      if (selectedRemoteIdx >= 0) {
        // Start gateway through tunnel
        await invoke("start_gateway_cmd", { profile: null });
      }
      setTimeout(onConnected, 800);
    } catch (err) {
      setStatus("error");
      setStatusMsg(String(err));
    } finally {
      setTesting(false);
    }
  };

  const foundRemote = remoteInstances.filter((i) => i.exists);

  return (
    <div className="ac-card">
      <div className="flex items-center gap-2 mb-4">
        <Terminal className="w-4 h-4 text-ac-amber" />
        <span className="ac-section-title">{t("conn.ssh")}</span>
      </div>

      <div className="space-y-2 mb-3">
        <div className="flex gap-2">
          <div className="flex-1">
            <label className="text-[11px] text-ac-stone mb-1 block">Host</label>
            <input type="text" value={host} onChange={(e) => setHost(e.target.value)}
              placeholder="your-server.com" className="ac-input w-full px-3 py-2 text-sm" />
          </div>
          <div className="w-16">
            <label className="text-[11px] text-ac-stone mb-1 block">Port</label>
            <input type="text" value={port} onChange={(e) => setPort(e.target.value)}
              placeholder="22" className="ac-input w-full px-3 py-2 text-sm" />
          </div>
        </div>
        <div className="flex gap-2">
          <div className="flex-1">
            <label className="text-[11px] text-ac-stone mb-1 block">User</label>
            <input type="text" value={username} onChange={(e) => setUsername(e.target.value)}
              className="ac-input w-full px-3 py-2 text-sm" />
          </div>
          <div className="flex-1">
            <label className="text-[11px] text-ac-stone mb-1 block">SSH Key</label>
            <input type="text" value={keyPath} onChange={(e) => setKeyPath(e.target.value)}
              className="ac-input w-full px-3 py-2 text-sm" />
          </div>
        </div>
        <div className="flex gap-2">
          <div className="flex-1">
            <label className="text-[11px] text-ac-stone mb-1 block">Remote port</label>
            <input type="text" value={remotePort} onChange={(e) => setRemotePort(e.target.value)}
              className="ac-input w-full px-3 py-2 text-sm" />
          </div>
          <div className="flex-1">
            <label className="text-[11px] text-ac-stone mb-1 block">Local port</label>
            <input type="text" value={localPort} onChange={(e) => setLocalPort(e.target.value)}
              className="ac-input w-full px-3 py-2 text-sm" />
          </div>
        </div>
      </div>

      {/* Remote instances (after scan) */}
      {foundRemote.length > 0 && (
        <div className="mb-3">
          <label className="text-[11px] text-ac-stone mb-2 block">Found on remote</label>
          <div className="space-y-1">
            {remoteInstances.map((inst, idx) => (
              <button key={inst.path}
                onClick={() => inst.exists && setSelectedRemoteIdx(idx)}
                disabled={!inst.exists}
                className={`w-full text-left px-3 py-2 text-xs border flex items-center gap-2 ${
                  !inst.exists ? "border-ac-border/30 text-ac-stone/30 cursor-not-allowed"
                    : selectedRemoteIdx === idx ? "border-ac-amber/30 bg-ac-amber/8 text-ac-amber"
                    : "border-ac-border text-ac-stone"
                }`}>
                {inst.exists ? <Check className="w-3 h-3 text-green-500 shrink-0" /> : <X className="w-3 h-3 shrink-0" />}
                <FolderOpen className="w-3 h-3 shrink-0" />
                <span className="truncate">{inst.path}</span>
                <span className="ml-auto text-[10px] opacity-50">{inst.instance}</span>
              </button>
            ))}
          </div>
        </div>
      )}

      {status === "error" && (
        <div className="mb-3 px-3 py-2 bg-red-500/5 border border-red-500/20 text-ac-red text-xs">{statusMsg}</div>
      )}
      {status === "ok" && (
        <div className="mb-3 px-3 py-2 bg-green-500/5 border border-green-500/20 text-green-400 text-xs">{statusMsg}</div>
      )}

      <div className="flex gap-2">
        <button onClick={() => void handleScan()} disabled={scanning || !host.trim()}
          className="ac-btn flex-1 py-2 text-xs flex items-center justify-center gap-1 disabled:opacity-30">
          {scanning ? <Loader className="w-3 h-3 animate-spin" /> : <Check className="w-3 h-3" />}
          Scan remote
        </button>
        <button onClick={() => void handleConnect()} disabled={testing || !host.trim()}
          className="ac-btn flex-1 py-2 text-sm flex items-center justify-center gap-2 disabled:opacity-30">
          {testing ? <Loader className="w-4 h-4 animate-spin" /> : <Play className="w-4 h-4" />}
          Connect
        </button>
      </div>
    </div>
  );
}

export default ConnectScreen;