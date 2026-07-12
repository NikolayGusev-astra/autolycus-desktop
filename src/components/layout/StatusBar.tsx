// src/components/layout/StatusBar.tsx
// Version + connection status. The version comes from the backend (Cargo pkg
// version) via the gateway store, so it always reflects the actual build.

import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useGatewayStore } from "../../stores/gatewayStore";
import { useTranslation } from "../../hooks/useTranslation";

export function StatusBar() {
  const { connected, pipelineStatus, gatewayVersion, setGatewayVersion } = useGatewayStore();
  const { t } = useTranslation();

  // Fetch the real app version once (was previously hardcoded "v0.6.0").
  useEffect(() => {
    if (!gatewayVersion) {
      invoke<string>("get_app_version")
        .then(setGatewayVersion)
        .catch(() => {});
    }
  }, [gatewayVersion, setGatewayVersion]);

  return (
    <footer className="px-5 py-1 flex items-center justify-between border-t border-ac-border bg-ac-bg text-[11px] text-ac-muted">
      <span>{t("app.name")} Desktop{gatewayVersion ? ` v${gatewayVersion}` : ""}</span>
      <div className="flex items-center gap-3">
        {pipelineStatus.model && (
          <span className="opacity-60">{pipelineStatus.model}</span>
        )}
        {pipelineStatus.tokensUsed !== undefined && pipelineStatus.tokensLimit !== undefined && (
          <span className="opacity-60 px-2 py-0.5 rounded bg-ac-brand/10 text-ac-brand text-[10px]">
            {pipelineStatus.tokensUsed.toLocaleString()} / {pipelineStatus.tokensLimit.toLocaleString()} tokens
          </span>
        )}
        {pipelineStatus.costUsd !== undefined && (
          <span className="opacity-60 px-2 py-0.5 rounded bg-ac-brand/10 text-ac-brand text-[10px]">
            ${pipelineStatus.costUsd.toFixed(4)}
          </span>
        )}
        <span className={`opacity-60 ${connected ? "text-ac-green" : "text-ac-red"}`}>
          {connected ? t("status.connected") : t("status.disconnected")}
        </span>
      </div>
    </footer>
  );
}
