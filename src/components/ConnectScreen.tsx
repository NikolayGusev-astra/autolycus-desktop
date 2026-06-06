import { useState } from "react";
import { Server, Loader, Play } from "lucide-react";

interface ConnectScreenProps {
  onConnectLocal: (pythonPath: string) => void;
  onStartLocal: (pythonPath: string) => void;
  connecting: boolean;
  starting: boolean;
  error: string | null;
}

interface FoundInstance {
  pythonPath: string;
  label: string;
  type: string;
}

export function ConnectScreen({ onStartLocal, connecting, starting, error }: ConnectScreenProps) {
  const [selectedIdx, setSelectedIdx] = useState(0);

  // Common install locations
  const instances: FoundInstance[] = [
    { pythonPath: "~/autolycus/venv/bin/python", label: "Autolycus (autolycus/venv)", type: "autolycus" },
    { pythonPath: "~/autolycus/venv/bin/python3", label: "Autolycus (autolycus/venv)", type: "autolycus" },
    { pythonPath: "~/.autolycus/venv/bin/python", label: "Autolycus (.autolycus/venv)", type: "autolycus" },
    { pythonPath: "~/.hermes/venv/bin/python", label: "Hermes (.hermes/venv)", type: "hermes" },
    { pythonPath: "~/.hermes/hermes-agent/venv/bin/python", label: "Hermes Agent (.hermes)", type: "hermes-agent" },
  ];

  const handleStart = () => {
    onStartLocal(instances[selectedIdx].pythonPath);
  };

  return (
    <div className="fixed inset-0 bg-ac-pitch flex items-center justify-center">
      <div className="w-full max-w-md px-6">
        {/* Logo */}
        <div className="text-center mb-8">
          <div className="ac-display mb-2">Автолик</div>
          <p className="text-sm text-ac-stone">AI Ассистент</p>
        </div>

        {/* Card */}
        <div className="ac-card">
          <div className="flex items-center gap-2 mb-4">
            <Server className="w-4 h-4 text-ac-amber" />
            <span className="ac-section-title">Запуск агента</span>
          </div>

          {/* Instance list */}
          <div className="mb-4">
            <label className="text-[11px] text-ac-stone mb-2 block">Найденные установки:</label>
            <div className="space-y-1.5 max-h-32 overflow-y-auto">
              {instances.map((inst, idx) => (
                <button
                  key={inst.pythonPath}
                  onClick={() => setSelectedIdx(idx)}
                  className={`w-full text-left px-3 py-2 text-xs border transition-colors flex items-center gap-2 ${
                    selectedIdx === idx
                      ? "border-ac-amber/30 bg-ac-amber/8 text-ac-amber"
                      : "border-ac-border text-ac-stone hover:border-ac-stone/30"
                  }`}
                >
                  <span className="truncate">{inst.label}</span>
                  <span className="ml-auto text-[10px] opacity-50">{inst.type}</span>
                </button>
              ))}
            </div>
          </div>

          {/* Manual path */}
          <div className="mb-4">
            <label className="text-[11px] text-ac-stone mb-1 block">Или укажите путь к Python:</label>
            <input
              type="text"
              placeholder="/path/to/venv/bin/python"
              className="ac-input w-full px-3 py-2 text-sm"
              onChange={(e) => {
                if (e.target.value) {
                  instances.push({ pythonPath: e.target.value, label: e.target.value, type: "custom" });
                  setSelectedIdx(instances.length - 1);
                }
              }}
            />
          </div>

          {/* Error */}
          {error && (
            <div className="mb-3 px-3 py-2 bg-red-500/5 border border-red-500/20 text-ac-red text-xs">
              {error}
            </div>
          )}

          {/* Start button */}
          <button
            onClick={handleStart}
            disabled={starting || connecting}
            className="ac-btn w-full py-2.5 text-sm flex items-center justify-center gap-2 disabled:opacity-30 disabled:cursor-not-allowed"
          >
            {starting || connecting ? (
              <>
                <Loader className="w-4 h-4 animate-spin" />
                Запуск...
              </>
            ) : (
              <>
                <Play className="w-4 h-4" />
                Запустить
              </>
            )}
          </button>
        </div>

        {/* Help */}
        <div className="mt-4 text-center">
          <p className="text-[11px] text-ac-stone/50">
            Desktop запустит Python-агент и подключится к нему
          </p>
        </div>
      </div>
    </div>
  );
}
