import { useState, useEffect } from "react";
import { Server, Loader, Play, Check, X, FolderOpen } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "../hooks/useTranslation";

interface InstanceInfo {
  path: string;
  instance: string;
  exists: boolean;
}

interface ConnectScreenProps {
  onStartLocal: (pythonPath: string) => void;
  connecting: boolean;
  starting: boolean;
  error: string | null;
}

export function ConnectScreen({
  onStartLocal,
  connecting,
  starting,
  error,
}: ConnectScreenProps) {
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
      // Auto-select first existing instance
      const firstExisting = result.findIndex((i) => i.exists);
      if (firstExisting >= 0) {
        setSelectedIdx(firstExisting);
      }
    } catch (err) {
      console.error("Failed to detect instances:", err);
    }
  };

  const handleStart = () => {
    if (selectedIdx >= 0 && instances[selectedIdx]) {
      onStartLocal(instances[selectedIdx].path);
    } else if (customPath.trim()) {
      onStartLocal(customPath.trim());
    }
  };

  const handleRetry = () => {
    loadInstances();
    handleStart();
  };

  const existingInstances = instances.filter((i) => i.exists);

  return (
    <div className="fixed inset-0 bg-ac-pitch flex items-center justify-center">
      <div className="w-full max-w-md px-6">
        <div className="text-center mb-8">
          <div className="ac-display mb-2">{t("autolycus_title")}</div>
          <p className="text-sm text-ac-stone">{t("ai_assistant")}</p>
        </div>

        <div className="ac-card">
          <div className="flex items-center gap-2 mb-4">
            <Server className="w-4 h-4 text-ac-amber" />
            <span className="ac-section-title">{t("connect_title")}</span>
          </div>

          {/* Auto-detected instances */}
          {existingInstances.length > 0 && (
            <div className="mb-4">
              <label className="text-[11px] text-ac-stone mb-2 block">
                {t("found_instances")}
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

          {/* Custom path input */}
          <div className="mb-4">
            <label className="text-[11px] text-ac-stone mb-1 block">
              {t("or_specify_path")}
            </label>
            <input
              type="text"
              value={customPath}
              onChange={(e) => setCustomPath(e.target.value)}
              placeholder="~/autolycus/venv/bin/python"
              className="ac-input w-full px-3 py-2 text-sm"
            />
          </div>

          {error && (
            <div className="mb-3 px-3 py-2 bg-red-500/5 border border-red-500/20 text-ac-red text-xs flex items-center justify-between">
              <span>{error}</span>
              <button
                onClick={handleRetry}
                className="text-ac-amber hover:underline text-[10px]"
              >
                {t("retry")}
              </button>
            </div>
          )}

          <button
            onClick={handleStart}
            disabled={
              connecting ||
              starting ||
              (selectedIdx < 0 && !customPath.trim())
            }
            className="ac-btn w-full py-2.5 text-sm flex items-center justify-center gap-2 disabled:opacity-30 disabled:cursor-not-allowed"
          >
            {starting || connecting ? (
              <>
                <Loader className="w-4 h-4 animate-spin" />
                {t("starting")}
              </>
            ) : (
              <>
                <Play className="w-4 h-4" />
                {t("start")}
              </>
            )}
          </button>
        </div>

        <div className="mt-4 text-center">
          <p className="text-[11px] text-ac-stone/50">
            {t("autostart_info")}
          </p>
        </div>
      </div>
    </div>
  );
}