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
        <button className="ac-tab active">
          <MessageSquare className="w-3 h-3 inline mr-1.5 align-middle" />
          Основной
        </button>
        <button className="ac-tab">
          <Plus className="w-3 h-3 inline mr-1.5 align-middle" />
          Новая
        </button>
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

function MessageSquare({ className }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5">
      <rect x="2" y="3" width="12" height="9" rx="1.5" />
      <path d="M5 6h6M5 8.5h4" />
    </svg>
  );
}

function Plus({ className }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5">
      <path d="M8 3v10M3 8h10" />
    </svg>
  );
}
