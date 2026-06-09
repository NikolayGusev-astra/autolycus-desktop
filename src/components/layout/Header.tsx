import { useGatewayStore } from "../../stores/gatewayStore";
import { invoke } from "@tauri-apps/api/core";
import { Power, Cpu, Settings } from "lucide-react";
import { useTranslation } from "../../hooks/useTranslation";

interface HeaderProps {
  onSettingsClick: () => void;
}

export function Header({ onSettingsClick }: HeaderProps) {
  const {
    agentStatus,
    connected,
    setConnected,
    setAgentStatus,
    pipelineStatus,
  } = useGatewayStore();
  const { t } = useTranslation();

  const statusLabel = {
    idle: t("header_ready"),
    thinking: t("header_thinking"),
    streaming: t("header_streaming"),
    tool_calling: t("header_tool_calling"),
    error: t("header_error"),
  }[agentStatus];

  const handleDisconnect = async () => {
    await invoke("stop_gateway_cmd", { profile: null });
    setConnected(false);
    setAgentStatus("idle");
  };

  const modelName = pipelineStatus.model || "—";
  const tokensUsed = pipelineStatus.tokensUsed ?? 0;
  const tokensLimit = pipelineStatus.tokensLimit ?? 0;
  const tokensDisplay =
    pipelineStatus.tokensUsed !== undefined
      ? `${(tokensUsed / 1000).toFixed(1)}K${
          tokensLimit ? `/${(tokensLimit / 1000).toFixed(0)}K` : ""
        }`
      : "";
  const costDisplay =
    pipelineStatus.costUsd !== undefined
      ? `$${pipelineStatus.costUsd.toFixed(3)}`
      : "";

  // Context gauge: percentage of context used
  const contextPercent =
    tokensLimit > 0 ? Math.min((tokensUsed / tokensLimit) * 100, 100) : 0;
  const gaugeColor =
    contextPercent > 90
      ? "bg-red-500"
      : contextPercent > 70
        ? "bg-yellow-500"
        : "bg-emerald-500";

  return (
    <header className="px-5 py-1.5 flex items-center border-b border-ac-border bg-ac-pitch/70 backdrop-blur-sm gap-3">
      {/* Session tabs */}
      <div className="flex gap-0.5">
        <button className="ac-tab active">{t("main_tab")}</button>
      </div>

      {/* Pipeline info */}
      {connected && (
        <div className="flex items-center gap-2 text-[11px] text-ac-stone ml-2">
          <Cpu className="w-3 h-3" />
          <span className="font-mono truncate max-w-[120px]">{modelName}</span>
          {tokensDisplay && (
            <span className="opacity-60">{tokensDisplay}</span>
          )}
          {costDisplay && <span className="opacity-60">{costDisplay}</span>}

          {/* Context gauge */}
          {tokensLimit > 0 && (
            <div className="flex items-center gap-1.5 ml-1">
              <div className="w-16 h-1.5 bg-ac-pitch rounded-full overflow-hidden border border-ac-border/30">
                <div
                  className={`h-full rounded-full transition-all duration-300 ${gaugeColor}`}
                  style={{ width: `${contextPercent}%` }}
                />
              </div>
              <span className="text-[10px] font-mono opacity-60">
                {contextPercent.toFixed(0)}%
              </span>
            </div>
          )}
        </div>
      )}

      {/* Right side */}
      <div className="ml-auto flex items-center gap-2.5 text-[11px] text-ac-stone">
        <button
          onClick={onSettingsClick}
          className="text-ac-stone hover:text-ac-ivory transition-colors"
          title={t("header_settings")}
        >
          <Settings className="w-3.5 h-3.5" />
        </button>
        <div className="flex items-center gap-1.5">
          <span
            className={
              connected ? "ac-pulse" : "ac-pulse ac-pulse-off"
            }
          />
          <span>{connected ? statusLabel : t("header_disconnected")}</span>
        </div>
        {connected && (
          <button
            onClick={handleDisconnect}
            className="text-ac-stone hover:text-ac-red transition-colors"
            title={t("header_disconnect")}
          >
            <Power className="w-3.5 h-3.5" />
          </button>
        )}
      </div>
    </header>
  );
}