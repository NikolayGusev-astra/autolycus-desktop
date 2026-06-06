import { useGatewayStore } from "../../stores/gatewayStore";

export function Header() {
  const { agentStatus, connected } = useGatewayStore();

  const statusLabel = {
    idle: "готов",
    thinking: "думает",
    streaming: "отвечает",
    tool_calling: "выполняет",
    error: "ошибка",
  }[agentStatus];

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
      </div>
    </header>
  );
}
