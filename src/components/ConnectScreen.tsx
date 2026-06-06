import { useState, useEffect } from "react";
import { Server, Wifi, Loader, FolderOpen, Play } from "lucide-react";

interface FoundBackend {
  path: string;
  type: "autolycus" | "hermes" | "python";
  label: string;
}

interface ConnectScreenProps {
  onConnect: (url: string) => void;
  onStartLocal: (path: string) => void;
  connecting: boolean;
  starting: boolean;
  error: string | null;
}

export function ConnectScreen({ onConnect, onStartLocal, connecting, starting, error }: ConnectScreenProps) {
  const [url, setUrl] = useState("ws://127.0.0.1:8443");
  const [mode, setMode] = useState<"local" | "remote">("local");
  const [foundBackends, setFoundBackends] = useState<FoundBackend[]>([]);
  const [selectedBackend, setSelectedBackend] = useState<string | null>(null);

  // Auto-detect local backends on mount
  useEffect(() => {
    detectLocalBackends();
  }, []);

  const detectLocalBackends = () => {
    // These paths will be checked by the Rust backend
    const commonPaths = [
      { path: "~/autolycus/venv/bin/autolycus", type: "autolycus" as const, label: "Autolycus (autolycus/venv)" },
      { path: "~/autolycus/venv/bin/hermes", type: "hermes" as const, label: "Hermes (autolycus/venv)" },
      { path: "~/.autolycus/venv/bin/autolycus", type: "autolycus" as const, label: "Autolycus (.autolycus/venv)" },
      { path: "~/.hermes/venv/bin/hermes", type: "hermes" as const, label: "Hermes (.hermes/venv)" },
      { path: "/usr/local/bin/autolycus", type: "autolycus" as const, label: "Autolycus (system)" },
      { path: "/usr/local/bin/hermes", type: "hermes" as const, label: "Hermes (system)" },
    ];
    setFoundBackends(commonPaths);
  };

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (url.trim()) {
      onConnect(url.trim());
    }
  };

  const handleStartLocal = () => {
    if (selectedBackend) {
      onStartLocal(selectedBackend);
    }
  };

  return (
    <div className="fixed inset-0 bg-ac-pitch flex items-center justify-center">
      <div className="w-full max-w-md px-6">
        {/* Logo */}
        <div className="text-center mb-8">
          <div className="ac-display mb-2">Автолик</div>
          <p className="text-sm text-ac-stone">AI Ассистент</p>
        </div>

        {/* Connection card */}
        <div className="ac-card">
          <div className="flex items-center gap-2 mb-4">
            <Server className="w-4 h-4 text-ac-amber" />
            <span className="ac-section-title">Подключение к агенту</span>
          </div>

          {/* Mode toggle */}
          <div className="flex gap-2 mb-4">
            <button
              onClick={() => setMode("local")}
              className={`ac-pill flex items-center gap-1.5 ${mode === "local" ? "active" : ""}`}
            >
              <Wifi className="w-3 h-3" />
              Локальный
            </button>
            <button
              onClick={() => setMode("remote")}
              className={`ac-pill flex items-center gap-1.5 ${mode === "remote" ? "active" : ""}`}
            >
              <Server className="w-3 h-3" />
              Удалённый
            </button>
          </div>

          {mode === "local" ? (
            <>
              {/* Auto-detect backends */}
              <div className="mb-4">
                <label className="text-[11px] text-ac-stone mb-2 block">Найденные бэкенды:</label>
                <div className="space-y-1.5 max-h-32 overflow-y-auto">
                  {foundBackends.map((backend) => (
                    <button
                      key={backend.path}
                      onClick={() => setSelectedBackend(backend.path)}
                      className={`w-full text-left px-3 py-2 text-xs border transition-colors flex items-center gap-2 ${
                        selectedBackend === backend.path
                          ? "border-ac-amber/30 bg-ac-amber/8 text-ac-amber"
                          : "border-ac-border text-ac-stone hover:border-ac-stone/30"
                      }`}
                    >
                      <FolderOpen className="w-3 h-3 flex-shrink-0" />
                      <span className="truncate">{backend.label}</span>
                      <span className="ml-auto text-[10px] opacity-50">{backend.type}</span>
                    </button>
                  ))}
                </div>
              </div>

              {/* Start local button */}
              <button
                onClick={handleStartLocal}
                disabled={!selectedBackend || starting}
                className="ac-btn w-full py-2.5 text-sm flex items-center justify-center gap-2 disabled:opacity-30 disabled:cursor-not-allowed mb-3"
              >
                {starting ? (
                  <>
                    <Loader className="w-4 h-4 animate-spin" />
                    Запуск...
                  </>
                ) : (
                  <>
                    <Play className="w-4 h-4" />
                    Запустить локально
                  </>
                )}
              </button>

              <div className="text-center text-[11px] text-ac-stone/50 mb-3">— или —</div>

              {/* Manual URL */}
              <form onSubmit={handleSubmit}>
                <div className="mb-3">
                  <label className="text-[11px] text-ac-stone mb-1 block">URL бэкенда</label>
                  <input
                    type="text"
                    value={url}
                    onChange={(e) => setUrl(e.target.value)}
                    placeholder="ws://127.0.0.1:8443"
                    className="ac-input w-full px-3 py-2 text-sm"
                    disabled={connecting}
                  />
                </div>

                {error && (
                  <div className="mb-3 px-3 py-2 bg-red-500/5 border border-red-500/20 text-ac-red text-xs">
                    {error}
                  </div>
                )}

                <button
                  type="submit"
                  disabled={!url.trim() || connecting}
                  className="w-full py-2.5 text-sm border border-ac-border text-ac-stone hover:text-ac-ivory hover:border-ac-stone/30 transition-colors flex items-center justify-center gap-2 disabled:opacity-30"
                >
                  {connecting ? (
                    <>
                      <Loader className="w-4 h-4 animate-spin" />
                      Подключение...
                    </>
                  ) : (
                    <>
                      <Wifi className="w-4 h-4" />
                      Подключиться
                    </>
                  )}
                </button>
              </form>
            </>
          ) : (
            /* Remote mode */
            <form onSubmit={handleSubmit}>
              <div className="mb-3">
                <label className="text-[11px] text-ac-stone mb-1 block">URL удалённого бэкенда</label>
                <input
                  type="text"
                  value={url}
                  onChange={(e) => setUrl(e.target.value)}
                  placeholder="wss://hermes.example.com:8443"
                  className="ac-input w-full px-3 py-2 text-sm"
                  disabled={connecting}
                />
              </div>

              {error && (
                <div className="mb-3 px-3 py-2 bg-red-500/5 border border-red-500/20 text-ac-red text-xs">
                  {error}
                </div>
              )}

              <button
                type="submit"
                disabled={!url.trim() || connecting}
                className="ac-btn w-full py-2.5 text-sm flex items-center justify-center gap-2 disabled:opacity-30"
              >
                {connecting ? (
                  <>
                    <Loader className="w-4 h-4 animate-spin" />
                    Подключение...
                  </>
                ) : (
                  <>
                    <Wifi className="w-4 h-4" />
                    Подключиться
                  </>
                )}
              </button>
            </form>
          )}
        </div>

        {/* Help */}
        <div className="mt-4 text-center">
          <p className="text-[11px] text-ac-stone/50">
            {mode === "local"
              ? "Автозапуск: desktop запустит Python backend и подключится к нему"
              : "URL предоставляется вашим партнёром"
            }
          </p>
        </div>
      </div>
    </div>
  );
}
