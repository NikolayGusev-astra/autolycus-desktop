// src/components/Versions.tsx
import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { RefreshCw, Copy, Check, Monitor, Cpu, HardDrive } from "lucide-react";

interface VersionInfo {
  app_version: string;
  tauri_version: string;
  rust_version: string;
  node_version: string;
  os: string;
  arch: string;
}

export function Versions() {
  const [info, setInfo] = useState<VersionInfo | null>(null);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    void loadVersions();
  }, []);

  async function loadVersions() {
    try {
      const v = await invoke<VersionInfo>("get_versions_cmd");
      setInfo(v);
    } catch {
      // Fallback: use app version from env
      setInfo({
        app_version: "0.7.0",
        tauri_version: "2",
        rust_version: "unknown",
        node_version: "unknown",
        os: navigator.platform,
        arch: "unknown",
      });
    }
  }

  const handleCopy = () => {
    if (!info) return;
    const text = `Autolycus Desktop ${info.app_version}\nTauri ${info.tauri_version}\nOS: ${info.os} (${info.arch})`;
    navigator.clipboard.writeText(text);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const rows = [
    { label: "Autolycus Desktop", value: info?.app_version, icon: Monitor },
    { label: "Tauri", value: info?.tauri_version, icon: Cpu },
    { label: "OS", value: info ? `${info.os} (${info.arch})` : null, icon: HardDrive },
  ];

  return (
    <div className="flex-1 overflow-y-auto p-6">
      <div className="max-w-2xl mx-auto">
        <div className="flex items-center justify-between mb-6">
          <div>
            <h1 className="text-2xl font-bold">Versions</h1>
            <p className="text-sm text-ac-muted mt-1">
              Application and system version information
            </p>
          </div>
          <button
            className="btn btn-secondary btn-sm flex items-center gap-2"
            onClick={() => void loadVersions()}
          >
            <RefreshCw className="w-4 h-4" />
            Refresh
          </button>
        </div>

        <div className="bg-ac-bg border border-ac-border rounded-xl overflow-hidden">
          {rows.map((row, i) => (
            <div
              key={row.label}
              className={`flex items-center justify-between px-4 py-3 ${
                i > 0 ? "border-t border-ac-border" : ""
              }`}
            >
              <div className="flex items-center gap-3">
                <row.icon className="w-4 h-4 text-ac-muted" />
                <span className="text-sm text-ac-muted">{row.label}</span>
              </div>
              <span className="text-sm font-mono">
                {row.value || "—"}
              </span>
            </div>
          ))}
        </div>

        <div className="mt-4 flex justify-end">
          <button
            className="btn btn-ghost btn-sm flex items-center gap-2"
            onClick={handleCopy}
            disabled={!info}
          >
            {copied ? <Check className="w-4 h-4 text-green-500" /> : <Copy className="w-4 h-4" />}
            {copied ? "Copied" : "Copy system info"}
          </button>
        </div>
      </div>
    </div>
  );
}

export default Versions;
