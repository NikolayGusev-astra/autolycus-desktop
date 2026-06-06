import { useGatewayStore } from "../../stores/gatewayStore";
import { invoke } from "@tauri-apps/api/core";
import { Power } from "lucide-react";

export function Header() {
  const { agentStatus, connected, setConnected, setAgentStatus } = useGatewayStore();

  const statusLabel = {
    idle: "готов",
    thinking: "думает",
    streaming: "отвечает",
    tool_calling: "выполняет",
    error: "ошибка",
  }[agentStatus];

  const handleDisconnect = async () => {
    await invoke("stop_agent");
    setConnected(false);
    setAgentStatus("idle");
  };

  return (
    <header className="px-5 py-1.5 flex items-center border-b border-ac-border bg-ac-pitch/70 backdrop-blur-sm gap-3">
      {/* Session tabs */}
      <div className="flex gap-0.5">
        <button className="ac-tab active">Основной</button>
      </div>

      {/* Right side */}
      <div className="ml-auto flex items-center gap-2.5 text-[11px] text-ac-stone">
        <div className="flex items-center gap-1.5">
          <span className={connected ? "ac-pulse" : "ac-pulse ac-pulse-off"} />
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
