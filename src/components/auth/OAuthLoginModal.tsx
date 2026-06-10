// src/components/auth/OAuthLoginModal.tsx
// OAuth login modal — ported from fathah/hermes-desktop
// Uses Tauri events instead of Electron IPC

import { useState, useEffect, useRef, useCallback } from "react";
import { X, Loader2, CheckCircle2, XCircle, ExternalLink, Copy } from "lucide-react";
import { useTranslation } from "../../hooks/useTranslation";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";

interface OAuthLoginModalProps {
  provider: string;
  providerLabel: string;
  profile?: string;
  onClose: () => void;
}

type Status = "running" | "success" | "error";

interface OAuthLoginResult {
  success: boolean;
  error?: string;
}

interface DeviceCodeInfo {
  url: string;
  code: string;
}

function OAuthLoginModal({
  provider,
  providerLabel,
  profile,
  onClose,
}: OAuthLoginModalProps): React.JSX.Element {
  const { t } = useTranslation();
  const [log, setLog] = useState("");
  const [status, setStatus] = useState<Status>("running");
  const [error, setError] = useState("");
  const [deviceCode, setDeviceCode] = useState<DeviceCodeInfo | null>(null);
  const [copied, setCopied] = useState(false);
  const logRef = useRef<HTMLPreElement>(null);
  const startedRef = useRef(false);

  const handleCopyCode = useCallback(() => {
    if (deviceCode?.code) {
      navigator.clipboard.writeText(deviceCode.code);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    }
  }, [deviceCode]);

  const handleOpenUrl = useCallback(() => {
    if (deviceCode?.url) {
      window.open(deviceCode.url, "_blank");
    }
  }, [deviceCode]);

  useEffect(() => {
    if (startedRef.current) return;
    startedRef.current = true;

    // Listen for progress events
    const unlistenProgress = listen<string>("oauth-login-progress", (event) => {
      setLog((prev) => prev + event.payload);
    });

    // Listen for device code detection
    const unlistenDeviceCode = listen<DeviceCodeInfo>("oauth-device-code", (event) => {
      setDeviceCode(event.payload);
    });

    // Listen for completion
    const unlistenComplete = listen<OAuthLoginResult>("oauth-login-complete", (event) => {
      const result = event.payload;
      if (result.success) {
        setStatus("success");
      } else {
        setStatus("error");
        setError(result.error || t("providers.oauth.failed"));
      }
    });

    // Start login
    invoke<OAuthLoginResult>("auth_login_cmd", { provider, profile })
      .then((res) => {
        if (!res.success) {
          setStatus("error");
          setError(res.error || t("providers.oauth.failed"));
        }
      })
      .catch((err) => {
        setStatus("error");
        setError(String(err));
      });

    return () => {
      unlistenProgress.then((f) => f());
      unlistenDeviceCode.then((f) => f());
      unlistenComplete.then((f) => f());
    };
  }, [provider, profile, t]);

  // Auto-scroll log
  useEffect(() => {
    if (logRef.current) {
      logRef.current.scrollTop = logRef.current.scrollHeight;
    }
  }, [log]);

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm">
      <div className="bg-ac-surface rounded-2xl shadow-2xl w-full max-w-lg mx-4 overflow-hidden">
        {/* Header */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-ac-border">
          <h2 className="text-lg font-semibold text-ac-text">
            {t("providers.oauth.title", { provider: providerLabel })}
          </h2>
          <button
            onClick={onClose}
            className="p-1.5 rounded-lg hover:bg-ac-hover transition-colors"
          >
            <X className="w-5 h-5 text-ac-muted" />
          </button>
        </div>

        {/* Body */}
        <div className="px-6 py-4">
          {/* Status */}
          <div className="flex items-center gap-3 mb-4">
            {status === "running" && (
              <>
                <Loader2 className="w-5 h-5 text-ac-blue animate-spin" />
                <span className="text-ac-text">{t("providers.oauth.signingIn")}</span>
              </>
            )}
            {status === "success" && (
              <>
                <CheckCircle2 className="w-5 h-5 text-green-500" />
                <span className="text-green-500">{t("providers.oauth.success")}</span>
              </>
            )}
            {status === "error" && (
              <>
                <XCircle className="w-5 h-5 text-red-500" />
                <span className="text-red-500">{error || t("providers.oauth.failed")}</span>
              </>
            )}
          </div>

          {/* Device code */}
          {deviceCode && status === "running" && (
            <div className="mb-4 p-4 bg-ac-bg rounded-xl border border-ac-border">
              <p className="text-sm text-ac-muted mb-2">{t("providers.oauth.deviceCode")}</p>
              <div className="flex items-center gap-2">
                <code className="flex-1 text-2xl font-mono font-bold text-ac-text bg-ac-surface px-3 py-2 rounded-lg">
                  {deviceCode.code}
                </code>
                <button
                  onClick={handleCopyCode}
                  className="p-2 rounded-lg hover:bg-ac-hover transition-colors"
                  title={t("misc.copy")}
                >
                  <Copy className={`w-5 h-5 ${copied ? "text-green-500" : "text-ac-muted"}`} />
                </button>
              </div>
              <button
                onClick={handleOpenUrl}
                className="mt-3 flex items-center gap-2 text-ac-blue hover:underline text-sm"
              >
                <ExternalLink className="w-4 h-4" />
                {t("providers.oauth.openBrowser")}
              </button>
            </div>
          )}

          {/* Log output */}
          {log && (
            <pre
              ref={logRef}
              className="text-xs font-mono text-ac-muted bg-ac-bg rounded-xl p-4 max-h-48 overflow-y-auto whitespace-pre-wrap break-words"
            >
              {log}
            </pre>
          )}
        </div>

        {/* Footer */}
        <div className="flex justify-end gap-3 px-6 py-4 border-t border-ac-border">
          {status === "running" && (
            <button
              onClick={() => {
                invoke("auth_cancel_cmd").catch(() => {});
                onClose();
              }}
              className="px-4 py-2 rounded-lg bg-ac-hover text-ac-text hover:bg-ac-border transition-colors"
            >
              {t("misc.cancel")}
            </button>
          )}
          {status !== "running" && (
            <button
              onClick={onClose}
              className="px-4 py-2 rounded-lg bg-ac-blue text-white hover:bg-ac-blue/80 transition-colors"
            >
              {t("misc.done")}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

export default OAuthLoginModal;
