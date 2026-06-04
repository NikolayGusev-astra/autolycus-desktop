import { useGatewayStore } from "../../stores/gatewayStore";

export function StatusBar() {
  const { connected, port } = useGatewayStore();

  return (
    <footer className="flex items-center justify-between border-t border-gray-200 dark:border-gray-700 px-4 py-1 bg-white dark:bg-gray-900 text-xs text-gray-400">
      <span>{connected ? `Connected (port ${port})` : "Disconnected"}</span>
      <span>Autolycus Desktop v0.1.0</span>
    </footer>
  );
}
