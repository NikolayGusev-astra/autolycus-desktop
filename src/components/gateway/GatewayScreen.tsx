// src/components/gateway/GatewayScreen.tsx
import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Play, Square, RefreshCw, Circle, CheckCircle2, XCircle } from "lucide-react";

interface GatewayStatus {
  running: boolean;
  port: number | null;
  profile: string | null;
}

export function GatewayScreen() {
  const [status, setStatus] = useState<GatewayStatus>({ running: false, port: null, profile: null });
  const [loading, setLoading] = useState(true);
  const [actionLoading, setActionLoading] = useState(false);

  const checkStatus = useCallback(async () => {
    setLoading(true);
    try {
      const [running, port] = await Promise.all([
        invoke<boolean>("gateway_status_cmd"),
        invoke<number | null>("get_gateway_port_cmd"),
      ]);
      setStatus({ running, port, profile: null });
    } catch {
      setStatus({ running: false, port: null, profile: null });
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void checkStatus();
  }, [checkStatus]);

  const handleStart = async () => {
    setActionLoading(true);
    try {
      await invoke("start_gateway_cmd");
      await checkStatus();
    } catch (err) {
      console.error("Failed to start gateway:", err);
    } finally {
      setActionLoading(false);
    }
  };

  const handleStop = async () => {
    setActionLoading(true);
    try {
      await invoke("stop_gateway_cmd");
      await checkStatus();
    } catch (err) {
      console.error("Failed to stop gateway:", err);
    } finally {
      setActionLoading(false);
    }
  };

  return (
    <div className="flex-1 overflow-y-auto p-6">
      <div className="max-w-2xl mx-auto">
        <h1 className="text-2xl font-bold mb-6">Gateway</h1>

        {/* Status Card */}
        <div className="bg-ac-bg border border-ac-border rounded-xl p-6 mb-6">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-3">
              {loading ? (
                <RefreshCw className="w-5 h-5 animate-spin text-ac-muted" />
              ) : status.running ? (
                <CheckCircle2 className="w-5 h-5 text-green-500" />
              ) : (
                <XCircle className="w-5 h-5 text-red-400" />
              )}
              <div>
                <p className="font-medium">
                  {loading ? "Checking..." : status.running ? "Running" : "Stopped"}
                </p>
                {status.running && status.port && (
                  <p className="text-sm text-ac-muted">Port: {status.port}</p>
                )}
              </div>
            </div>
            <div className="flex gap-2">
              {!status.running ? (
                <button
                  className="btn btn-primary btn-sm flex items-center gap-2"
                  onClick={() => void handleStart()}
                  disabled={actionLoading}
                >
                  <Play className="w-4 h-4" />
                  Start
                </button>
              ) : (
                <button
                  className="btn btn-secondary btn-sm flex items-center gap-2"
                  onClick={() => void handleStop()}
                  disabled={actionLoading}
                >
                  <Square className="w-4 h-4" />
                  Stop
                </button>
              )}
              <button
                className="btn btn-ghost btn-sm"
                onClick={() => void checkStatus()}
                disabled={loading}
              >
                <RefreshCw className={`w-4 h-4 ${loading ? "animate-spin" : ""}`} />
              </button>
            </div>
          </div>
        </div>

        {/* Info */}
        <div className="bg-ac-bg border border-ac-border rounded-xl p-6">
          <h2 className="text-lg font-semibold mb-3">About Gateway</h2>
          <p className="text-sm text-ac-muted mb-4">
            The Gateway is the local server that handles messaging platforms (Telegram, Discord, etc.)
            and provides the API for this desktop app.
          </p>
          <div className="space-y-2 text-sm">
            <div className="flex items-center gap-2">
              <Circle className="w-2 h-2 fill-green-500 text-green-500" />
              <span className="text-ac-muted">Running — accepting connections</span>
            </div>
            <div className="flex items-center gap-2">
              <Circle className="w-2 h-2 fill-red-400 text-red-400" />
              <span className="text-ac-muted">Stopped — not accepting connections</span>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

export default GatewayScreen;
