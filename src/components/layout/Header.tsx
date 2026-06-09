import { useGatewayStore } from "../../stores/gatewayStore";
import { invoke } from "@tauri-apps/api/core";
import { Power, Cpu, Settings } from "lucide-react";

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

  const statusLabel = {
    idle: "готов",
    thinking: "думает",
    streaming: "отвечает",
    tool_calling: "выполняет",
    error: "ошибка",
  }[agentStatus];

  const handleDisconnect = async () => {
    await invoke("stop_gateway_cmd", { profile: null });
    setConnected(false);
    setAgentStatus("idle");
  };

  const modelName = pipelineStatus.model || "—";
  const tokensDisplay =
    pipelineStatus.tokensUsed !== undefined
      ? `${(pipelineStatus.tokensUsed / 1000).toFixed(1)}K${
          pipelineStatus.tokensLimit
            ? `/${(pipelineStatus.tokensLimit / 1000).toFixed(0)}K`
            : ""
        }`
      : "";
  const costDisplay =
    pipelineStatus.costUsd !== undefined
      ? `$${pipelineStatus.costUsd.toFixed(3)}`
      : "";

  return (
    <header className="px-5 py-1.5 flex items-center border-b border-ac-border bg-ac-pitch/70 backdrop-blur-sm gap-3">
      {/* Session tabs */}
      <div className="flex gap-0.5">
        <button className="ac-tab active">Основной</button>
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
        </div>
      )}

      {/* Right side */}
      <div className="ml-auto flex items-center gap-2.5 text-[11px] text-ac-stone">
        <button
          onClick={onSettingsClick}
          className="text-ac-stone hover:text-ac-ivory transition-colors"
          title="Настройки"
        >
          <Settings className="w-3.5 h-3.5" />
        </button>
        <div className="flex items-center gap-1.5">
          <span
            className={
              connected ? "ac-pulse" : "ac-pulse ac-pulse-off"
            }
          />
          <span>{connected ? statusLabel : "отключено"}</span>
        </div>
        {connected && (
          <button
            onClick={handleDisconnect}
            className="text-ac-stone hover:text-ac-red transition-colors"
            title="Отключиться"
          >
            <Power className="w-3.5 h-3.5" />
          </button>
        )}
      </div>
    </header>
  );
}
