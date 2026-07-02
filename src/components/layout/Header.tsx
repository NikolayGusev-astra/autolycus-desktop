// src/components/layout/Header.tsx
// shturman.ai-style top header: frosted glass, search box, current date, and a
// connection-status badge. Sticky, h-14.

import { useState } from "react";
import { useGatewayStore } from "../../stores/gatewayStore";
import { Search, Wifi, WifiOff } from "lucide-react";
import { useTranslation } from "../../hooks/useTranslation";

interface HeaderProps {
  onSearch?: (q: string) => void;
}

export function Header({ onSearch }: HeaderProps) {
  const { connected } = useGatewayStore();
  const [query, setQuery] = useState("");
  const { t } = useTranslation();

  const today = new Date().toLocaleDateString("ru-RU", {
    weekday: "long",
    day: "numeric",
    month: "long",
    year: "numeric",
  });

  return (
    <header className="sticky top-0 z-30 flex items-center gap-4 px-6 h-14 border-b border-ac-border bg-ac-bg/80 backdrop-blur-md">
      {/* Search */}
      <div className="relative flex-1 max-w-md">
        <Search className="w-4 h-4 absolute left-3 top-1/2 -translate-y-1/2 text-ac-muted" />
        <input
          type="text"
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
            onSearch?.(e.target.value);
          }}
          placeholder={t("header.search")}
          className="w-full h-9 pl-9 pr-3 rounded-md bg-ac-surface border border-ac-border text-sm text-ac-ink placeholder:text-ac-faint focus:outline-none focus:border-ac-brand"
        />
      </div>

      <div className="ml-auto flex items-center gap-4">
        {/* Current date */}
        <span className="text-sm text-ac-muted hidden md:inline capitalize">{today}</span>

        {/* Connection badge */}
        <div
          className={`flex items-center gap-1.5 text-xs px-2.5 py-1 rounded-md ${
            connected ? "bg-green-500/10 text-green-600 dark:text-green-400" : "bg-red-500/10 text-red-500"
          }`}
        >
          {connected ? (
            <>
              <Wifi className="w-3.5 h-3.5" />
              {t("status.connected")}
            </>
          ) : (
            <>
              <WifiOff className="w-3.5 h-3.5" />
              {t("status.disconnected")}
            </>
          )}
        </div>
      </div>
    </header>
  );
}

export default Header;
