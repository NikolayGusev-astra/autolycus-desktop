import { useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ConnectScreen } from "./ConnectScreen";
import { useTranslation } from "../hooks/useTranslation";

interface ConnectionScreenProps {
  onConnected: () => void;
  error: string | null;
}

/**
 * Wraps ConnectScreen to bridge its onStartLocal interface
 * with the onConnected interface expected by App.tsx.
 */
export function ConnectionScreen({ onConnected, error }: ConnectionScreenProps) {
  const [connecting, setConnecting] = useState(false);
  const [starting, setStarting] = useState(false);
  const [localError, setLocalError] = useState<string | null>(error);
  const { t } = useTranslation();

  const handleStartLocal = useCallback(
    async (_pythonPath: string) => {
      setStarting(true);
      setLocalError(null);
      try {
        const result = await invoke<{ success: boolean; running: boolean; error?: string }>(
          "start_gateway_cmd",
          { profile: null }
        );
        if (result.success) {
          setConnecting(true);
          // Brief delay for gateway to become ready
          await new Promise((r) => setTimeout(r, 600));
          onConnected();
        } else {
          setLocalError(result.error || t("conn.failedStart"));
        }
      } catch (err) {
        setLocalError(String(err));
      } finally {
        setStarting(false);
        setConnecting(false);
      }
    },
    [onConnected]
  );

  return (
    <ConnectScreen
      onStartLocal={handleStartLocal}
      connecting={connecting}
      starting={starting}
      error={localError}
    />
  );
}