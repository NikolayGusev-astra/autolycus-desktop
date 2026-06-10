// src/components/config/ConfigHealthBanner.tsx
// Dismissible banner for config health issues — ported from fathah/hermes-desktop

import { useEffect, useState } from "react";
import { X, AlertTriangle, AlertCircle, Info } from "lucide-react";
import { useTranslation } from "../../hooks/useTranslation";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";

interface ConfigHealthBannerProps {
  profile?: string;
  onOpenDiagnose?: () => void;
}

interface HealthIssue {
  code: string;
  severity: "error" | "warning" | "info";
  message: string;
  fix?: string;
}

interface HealthReport {
  profile?: string;
  issues: HealthIssue[];
  summary: { errors: number; warnings: number; infos: number };
  ran_at: number;
}

const DISMISS_STORAGE_KEY = "autolycus-config-health-dismissed";

function readDismissedStamp(): number {
  try {
    const raw = localStorage.getItem(DISMISS_STORAGE_KEY);
    return raw ? Number(raw) || 0 : 0;
  } catch {
    return 0;
  }
}

function rememberDismiss(ranAt: number) {
  try {
    localStorage.setItem(DISMISS_STORAGE_KEY, String(ranAt));
  } catch {
    // localStorage unavailable
  }
}

function ConfigHealthBanner({ profile, onOpenDiagnose }: ConfigHealthBannerProps): React.JSX.Element | null {
  const { t } = useTranslation();
  const [report, setReport] = useState<HealthReport | null>(null);
  const [dismissed, setDismissed] = useState(readDismissedStamp());

  useEffect(() => {
    // Run health check on mount
    invoke<HealthReport>("config_health_check_cmd", { profile })
      .then(setReport)
      .catch(() => {});

    // Listen for updates
    const unlisten = listen<HealthReport>("config-health-updated", (event) => {
      setReport(event.payload);
    });

    return () => { unlisten.then((f) => f()); };
  }, [profile]);

  if (!report || report.issues.length === 0) return null;
  if (dismissed >= report.ran_at) return null;

  const { errors, warnings, infos } = report.summary;
  const hasErrors = errors > 0;
  const hasWarnings = warnings > 0;

  return (
    <div
      className={`mx-4 mt-3 px-4 py-3 rounded-xl border flex items-center gap-3 ${
        hasErrors
          ? "bg-red-500/10 border-red-500/30"
          : hasWarnings
            ? "bg-amber-500/10 border-amber-500/30"
            : "bg-blue-500/10 border-blue-500/30"
      }`}
    >
      {hasErrors ? (
        <AlertCircle className="w-5 h-5 text-red-400 shrink-0" />
      ) : hasWarnings ? (
        <AlertTriangle className="w-5 h-5 text-amber-400 shrink-0" />
      ) : (
        <Info className="w-5 h-5 text-blue-400 shrink-0" />
      )}

      <div className="flex-1 min-w-0">
        <p className="text-sm text-ac-text">
          {errors > 0 && (
            <span className="text-red-400 font-medium">
              {t("health.errors", { count: errors })}
            </span>
          )}
          {errors > 0 && warnings > 0 && ", "}
          {warnings > 0 && (
            <span className="text-amber-400 font-medium">
              {t("health.warnings", { count: warnings })}
            </span>
          )}
          {(errors > 0 || warnings > 0) && infos > 0 && ", "}
          {infos > 0 && (
            <span className="text-blue-400 font-medium">
              {t("health.infos", { count: infos })}
            </span>
          )}
        </p>
      </div>

      {onOpenDiagnose && (
        <button
          onClick={onOpenDiagnose}
          className="text-xs text-ac-blue hover:underline shrink-0"
        >
          {t("health.showDetails")}
        </button>
      )}

      <button
        className="p-1 rounded-lg hover:bg-ac-hover shrink-0"
        onClick={() => {
          rememberDismiss(report.ran_at);
          setDismissed(report.ran_at);
        }}
      >
        <X className="w-4 h-4 text-ac-muted" />
      </button>
    </div>
  );
}

export default ConfigHealthBanner;
