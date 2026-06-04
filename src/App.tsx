import { useEffect, useState } from "react";
import { AppShell } from "./components/layout/AppShell";
import { GatewayClient } from "./lib/gateway-client";

export function App() {
  const [backendPort, setBackendPort] = useState<number | null>(null);

  useEffect(() => {
    // In production, Tauri provides the port via command
    // For dev, we'll connect to a default port or show a settings screen
    const port = parseInt(new URLSearchParams(window.location.search).get("port") || "0", 10);
    if (port > 0) {
      setBackendPort(port);
    }
  }, []);

  useEffect(() => {
    if (backendPort) {
      const c = new GatewayClient(backendPort);
      c.connect().catch((err: Error) => {
        console.error("Backend connection failed:", err);
      });
    }
  }, [backendPort]);

  if (backendPort === null) {
    return (
      <div className="flex h-full items-center justify-center bg-gray-900">
        <div className="text-gray-400 text-center">
          <p className="text-lg mb-2">Autolycus Desktop</p>
          <p className="text-sm">Waiting for backend...</p>
          <p className="text-xs mt-4 text-gray-500">
            Start with:{" "}
            <code className="bg-gray-800 px-2 py-0.5 rounded">
              autolycus-desktop --backend-port 8765
            </code>
          </p>
        </div>
      </div>
    );
  }

  return <AppShell />;
}
