import { useGatewayStore } from "../../stores/gatewayStore";

export function StatusBar() {
  const { connected, port, agentStatus } = useGatewayStore();

  return (
    <footer className="px-5 py-1 flex items-center justify-between border-t border-ac-border bg-ac-pitch text-[11px] text-ac-stone">
      <div className="flex items-center gap-3">
        <span className={connected ? "ac-pulse" : "ac-pulse ac-pulse-off"} />
        <span>{connected ? `Подключено (порт ${port})` : "Отключено"}</span>
      </div>
      <div className="flex items-center gap-3">
        <span className="capitalize">{agentStatus}</span>
        <span>Autolycus Desktop v0.1.0</span>
      </div>
    </footer>
  );
}
