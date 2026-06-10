// src/components/settings/DiagnoseScreen.tsx
import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AlertCircle, AlertTriangle, Info, CheckCircle2, RefreshCw, Wrench } from "lucide-react";

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

export function DiagnoseScreen({ profile }: { profile?: string }) {
  const [report, setReport] = useState<HealthReport | null>(null);
  const [loading, setLoading] = useState(true);
  const [fixing, setFixing] = useState<string | null>(null);

  const runDiagnostics = useCallback(async () => {
    setLoading(true);
    try {
      const r = await invoke<HealthReport>("config_health_check_cmd", {
        profile: profile || "default",
      });
      setReport(r);
    } catch (err) {
      console.error("Health check failed:", err);
    } finally {
      setLoading(false);
    }
  }, [profile]);

  useEffect(() => {
    void runDiagnostics();
  }, [runDiagnostics]);

  const handleAutoFix = async (code: string) => {
    setFixing(code);
    try {
      await invoke("auto_fix_config_cmd", { code, profile: profile || "default" });
      await runDiagnostics();
    } catch (err) {
      console.error("Auto-fix failed:", err);
    } finally {
      setFixing(null);
    }
  };

  const errorIssues = report?.issues.filter((i) => i.severity === "error") || [];
  const warningIssues = report?.issues.filter((i) => i.severity === "warning") || [];
  const infoIssues = report?.issues.filter((i) => i.severity === "info") || [];

  return (
    <div className="flex-1 overflow-y-auto p-6">
      <div className="max-w-3xl mx-auto">
        <div className="flex items-center justify-between mb-6">
          <div>
            <h1 className="text-2xl font-bold">Diagnose</h1>
            <p className="text-sm text-ac-muted mt-1">
              Configuration health check and auto-fix
            </p>
          </div>
          <button
            className="btn btn-secondary btn-sm flex items-center gap-2"
            onClick={() => void runDiagnostics()}
            disabled={loading}
          >
            <RefreshCw className={`w-4 h-4 ${loading ? "animate-spin" : ""}`} />
            Re-run
          </button>
        </div>

        {loading && (
          <div className="flex items-center justify-center py-12">
            <RefreshCw className="w-6 h-6 animate-spin text-ac-muted" />
            <span className="ml-2 text-ac-muted">Running diagnostics...</span>
          </div>
        )}

        {!loading && report && report.issues.length === 0 && (
          <div className="flex items-center gap-3 p-6 bg-green-500/10 border border-green-500/30 rounded-xl">
            <CheckCircle2 className="w-6 h-6 text-green-500" />
            <div>
              <p className="font-medium text-green-500">All checks passed</p>
              <p className="text-sm text-ac-muted">
                No configuration issues detected.
              </p>
            </div>
          </div>
        )}

        {!loading && report && report.issues.length > 0 && (
          <div className="space-y-6">
            {/* Summary */}
            <div className="grid grid-cols-3 gap-4">
              <div className="p-4 bg-ac-bg border border-ac-border rounded-xl text-center">
                <p className="text-2xl font-bold text-red-400">{errorIssues.length}</p>
                <p className="text-xs text-ac-muted mt-1">Errors</p>
              </div>
              <div className="p-4 bg-ac-bg border border-ac-border rounded-xl text-center">
                <p className="text-2xl font-bold text-amber-400">{warningIssues.length}</p>
                <p className="text-xs text-ac-muted mt-1">Warnings</p>
              </div>
              <div className="p-4 bg-ac-bg border border-ac-border rounded-xl text-center">
                <p className="text-2xl font-bold text-blue-400">{infoIssues.length}</p>
                <p className="text-xs text-ac-muted mt-1">Info</p>
              </div>
            </div>

            {/* Issues */}
            {errorIssues.length > 0 && (
              <IssueSection
                title="Errors"
                issues={errorIssues}
                icon={<AlertCircle className="w-5 h-5 text-red-400" />}
                onAutoFix={handleAutoFix}
                fixing={fixing}
              />
            )}
            {warningIssues.length > 0 && (
              <IssueSection
                title="Warnings"
                issues={warningIssues}
                icon={<AlertTriangle className="w-5 h-5 text-amber-400" />}
                onAutoFix={handleAutoFix}
                fixing={fixing}
              />
            )}
            {infoIssues.length > 0 && (
              <IssueSection
                title="Info"
                issues={infoIssues}
                icon={<Info className="w-5 h-5 text-blue-400" />}
                onAutoFix={handleAutoFix}
                fixing={fixing}
              />
            )}
          </div>
        )}
      </div>
    </div>
  );
}

function IssueSection({
  title,
  issues,
  icon,
  onAutoFix,
  fixing,
}: {
  title: string;
  issues: HealthIssue[];
  icon: React.ReactNode;
  onAutoFix: (code: string) => void;
  fixing: string | null;
}) {
  return (
    <div>
      <h2 className="text-lg font-semibold mb-3 flex items-center gap-2">
        {icon}
        {title}
      </h2>
      <div className="space-y-2">
        {issues.map((issue) => (
          <div
            key={issue.code}
            className="p-4 bg-ac-bg border border-ac-border rounded-xl"
          >
            <div className="flex items-start justify-between">
              <div className="flex-1">
                <p className="text-sm font-medium">{issue.message}</p>
                <p className="text-xs text-ac-muted mt-1 font-mono">{issue.code}</p>
              </div>
              {issue.fix && (
                <button
                  className="btn btn-secondary btn-sm flex items-center gap-1 shrink-0 ml-4"
                  onClick={() => onAutoFix(issue.code)}
                  disabled={fixing === issue.code}
                >
                  <Wrench className="w-3 h-3" />
                  {fixing === issue.code ? "Fixing..." : "Auto-fix"}
                </button>
              )}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

export default DiagnoseScreen;
