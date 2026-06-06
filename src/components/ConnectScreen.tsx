import { useState } from "react";
import { Server, Wifi, Loader, FolderOpen, Play } from "lucide-react";

interface ConnectScreenProps {
  onConnectLocal: (pythonPath: string) => void;
  onConnectRemote: (url: string) => void;
  onStartLocal: (pythonPath: string) => void;
  connecting: boolean;
  starting: boolean;
  error: string | null;
}

export function ConnectScreen({
  onConnectLocal,
  onConnectRemote,
  onStartLocal,
  connecting,
  starting,
  error,
}: ConnectScreenProps) {
  const [url, setUrl] = useState("ws://127.0.0.1:8443");
  const [mode, setMode] = useState<"local" | "remote">("local");
  const [pythonPath, setPythonPath] = useState("");

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (mode === "local" && pythonPath.trim()) {
      onConnectLocal(pythonPath.trim());
    } else if (mode === "remote" && url.trim()) {
      onConnectRemote(url.trim());
    }
  };

  return (
    <div className="fixed inset-0 bg-ac-pitch flex items-center justify-center">
      <div className="w-full max-w-md px-6">
        <div className="text-center mb-8">
          <div className="ac-display mb-2">Автолик</div>
          <p className="text-sm text-ac-stone">AI Ассистент</p>
        </div>

        <div className="ac-card">
          <div className="flex items-center gap-2 mb-4">
            <Server className="w-4 h-4 text-ac-amber" />
            <span className="ac-section-title">Подключение к агенту</span>
          </div>

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
              <div className="mb-4">
                <label className="text-[11px] text-ac-stone mb-2 block">
                  Путь к Python (или оставьте пустым для авто)
                </label>
                <input
                  type="text"
                  value={pythonPath}
                  onChange={(e) => setPythonPath(e.target.value)}
                  placeholder="~/autolycus/venv/bin/python"
                  className="ac-input w-full px-3 py-2 text-sm"
                />
              </div>

              <button
                onClick={() => onStartLocal(pythonPath || "python3")}
                disabled={starting || connecting}
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
                  disabled={connecting}
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
            <form onSubmit={handleSubmit}>
              <div className="mb-3">
                <label className="text-[11px] text-ac-stone mb-1 block">
                  Адрес удалённого бэкенда
                </label>
                <input
                  type="text"
                  value={url}
                  onChange={(e) => setUrl(e.target.value)}
                  placeholder="host:port или ws://host:port"
                  className="ac-input w-full px-3 py-2 text-sm"
                  disabled={connecting}
                />
                <p className="text-[10px] text-ac-stone/50 mt-1">
                  Для SSH-туннеля: ssh -L 8443:127.0.0.1:8443 user@host, затем 127.0.0.1:8443
                </p>
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

        <div className="mt-4 text-center">
          <p className="text-[11px] text-ac-stone/50">
            {mode === "local"
              ? "Автозапуск: desktop запустит Python backend и подключится к нему"
              : "Подключение к удалённому серверу через TCP"}
          </p>
        </div>
      </div>
    </div>
  );
}
