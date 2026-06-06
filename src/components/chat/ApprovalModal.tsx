import { useState } from "react";
import { Shield, X, Check, XCircle } from "lucide-react";

interface ApprovalRequest {
  id: string;
  action: string;
  description: string;
  toolName: string;
  toolInput: string;
}

interface ApprovalModalProps {
  request: ApprovalRequest | null;
  onApprove: (id: string) => void;
  onDeny: (id: string) => void;
}

export function ApprovalModal({ request, onApprove, onDeny }: ApprovalModalProps) {
  const [expanded, setExpanded] = useState(false);

  if (!request) return null;

  return (
    <div className="ac-modal-overlay">
      <div className="ac-modal">
        {/* Header */}
        <div className="flex items-center justify-between mb-4">
          <div className="flex items-center gap-2">
            <Shield className="w-4 h-4 text-ac-amber" />
            <span className="text-sm font-semibold text-ac-ivory">
              Подтверждение действия
            </span>
          </div>
          <button
            onClick={() => onDeny(request.id)}
            className="text-ac-stone hover:text-ac-ivory transition-colors"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Action description */}
        <p className="text-sm text-ac-ivory mb-3">{request.description}</p>

        {/* Tool info */}
        <div className="ac-tool mb-3">
          <div className="flex items-center gap-2 mb-1">
            <span className="ac-badge ac-badge-yellow">{request.toolName}</span>
          </div>
          {expanded && (
            <pre className="text-[11px] text-ac-stone whitespace-pre-wrap mt-2 overflow-x-auto">
              {request.toolInput}
            </pre>
          )}
          <button
            onClick={() => setExpanded(!expanded)}
            className="text-[10px] text-ac-amber hover:underline mt-1"
          >
            {expanded ? "Скрыть" : "Показать детали"}
          </button>
        </div>

        {/* Actions */}
        <div className="flex gap-2 justify-end">
          <button
            onClick={() => onDeny(request.id)}
            className="flex items-center gap-1.5 px-4 py-2 text-sm border border-ac-border text-ac-stone hover:text-ac-ivory hover:border-ac-stone transition-colors"
          >
            <XCircle className="w-3.5 h-3.5" />
            Отклонить
          </button>
          <button
            onClick={() => onApprove(request.id)}
            className="ac-btn flex items-center gap-1.5 px-4 py-2 text-sm"
          >
            <Check className="w-3.5 h-3.5" />
            Подтвердить
          </button>
        </div>
      </div>
    </div>
  );
}
