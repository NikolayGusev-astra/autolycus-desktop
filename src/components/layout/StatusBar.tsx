// src/components/layout/StatusBar.tsx
// v0.4.0: Simplified — version + connection status

import { useGatewayStore } from "../../stores/gatewayStore";
import { useTranslation } from "../../hooks/useTranslation";

export function StatusBar() {
  const { connected, pipelineStatus } = useGatewayStore();
  const { t } = useTranslation();

  return (
    <footer className="px-5 py-1 flex items-center justify-between border-t border-ac-border bg-ac-pitch text-[11px] text-ac-stone">
      <span>Autolycus Desktop v0.6.0</span>
      <div className="flex items-center gap-3">
        {pipelineStatus.model && (
          <span className="opacity-60">{pipelineStatus.model}</span>
        )}
        <span className={`opacity-60 ${connected ? "text-green-400" : "text-red-400"}`}>
          {connected ? t("status.connected") : t("status.disconnected")}
        </span>
      </div>
    </footer>
  );
}
