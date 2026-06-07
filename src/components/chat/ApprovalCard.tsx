import { Shield, X, Check, RefreshCw } from "lucide-react";
import type { ApprovalRequest } from "../../lib/types";

interface ApprovalCardProps {
  request: ApprovalRequest;
  onApprove: () => void;
  onDeny: () => void;
  onApproveAlways: () => void;
}

const CLASS_COLORS: Record<string, string> = {
  read: "text-green-600 dark:text-green-400 bg-green-50 dark:bg-green-950/30 border-green-200 dark:border-green-800",
  write:
    "text-yellow-600 dark:text-yellow-400 bg-yellow-50 dark:bg-yellow-950/30 border-yellow-200 dark:border-yellow-800",
  network:
    "text-blue-600 dark:text-blue-400 bg-blue-50 dark:bg-blue-950/30 border-blue-200 dark:border-blue-800",
  install:
    "text-orange-600 dark:text-orange-400 bg-orange-50 dark:bg-orange-950/30 border-orange-200 dark:border-orange-800",
  destructive:
    "text-red-600 dark:text-red-400 bg-red-50 dark:bg-red-950/30 border-red-200 dark:border-red-800",
};

const CLASS_LABELS: Record<string, string> = {
  read: "Read",
  write: "Write",
  network: "Network",
  install: "Install",
  destructive: "Destructive",
};

export function ApprovalCard({
  request,
  onApprove,
  onDeny,
  onApproveAlways,
}: ApprovalCardProps) {
  const colorClass =
    CLASS_COLORS[request.commandClass] || CLASS_COLORS.write;
  const classLabel =
    CLASS_LABELS[request.commandClass] || request.commandClass;

  return (
    <div className="px-4 py-3 border-t border-ac-border bg-ac-pitch/50 backdrop-blur-sm">
      <div className="max-w-2xl mx-auto">
        {/* Header */}
        <div className="flex items-center gap-2 mb-2">
          <Shield className="w-4 h-4 text-ac-amber flex-shrink-0" />
          <span className="text-sm font-semibold text-ac-ivory">
            Подтверждение действия
          </span>
          <span
            className={`ml-auto text-[10px] px-2 py-0.5 rounded border font-medium ${colorClass}`}
          >
            {classLabel}
          </span>
        </div>

        {/* Description */}
        <p className="text-sm text-ac-stone mb-2">{request.action}</p>

        {/* Tool info */}
        <div className="ac-tool mb-3">
          <div className="flex items-center gap-2 mb-1">
            <span className="ac-badge ac-badge-yellow">{request.toolName}</span>
          </div>
          {request.toolInput && (
            <pre className="text-[11px] text-ac-stone whitespace-pre-wrap mt-1 overflow-x-auto max-h-32 overflow-y-auto">
              {request.toolInput}
            </pre>
          )}
        </div>

        {/* Actions */}
        <div className="flex gap-2 justify-end">
          <button
            onClick={onDeny}
            className="flex items-center gap-1.5 px-3 py-1.5 text-xs border border-ac-border text-ac-stone hover:text-ac-ivory hover:border-ac-stone transition-colors"
          >
            <X className="w-3 h-3" />
            Отклонить
          </button>
          <button
            onClick={onApprove}
            className="flex items-center gap-1.5 px-3 py-1.5 text-xs ac-btn"
          >
            <Check className="w-3 h-3" />
            Разово
          </button>
          <button
            onClick={onApproveAlways}
            className="flex items-center gap-1.5 px-3 py-1.5 text-xs border border-ac-amber/30 text-ac-amber hover:bg-ac-amber/10 transition-colors"
          >
            <RefreshCw className="w-3 h-3" />
            Всегда
          </button>
        </div>
      </div>
    </div>
  );
}
