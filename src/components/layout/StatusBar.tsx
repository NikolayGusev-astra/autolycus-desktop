import { useGatewayStore } from "../../stores/gatewayStore";

export function StatusBar() {
  const { mode } = useGatewayStore();

  return (
    <footer className="px-5 py-1 flex items-center justify-between border-t border-ac-border bg-ac-pitch text-[11px] text-ac-stone">
      <span>Autolycus Desktop v0.3.0</span>
      <span className="opacity-60">mode: {mode}</span>
    </footer>
  );
}
