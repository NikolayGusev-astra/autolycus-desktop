// src/components/shared/Toast.tsx
// Minimal toast notification system — replaces silent console.error pattern.
// Usage: const toast = useToast(); toast.error("Failed"); toast.success("Saved");

import { useState, useCallback, createContext, useContext, type ReactNode } from "react";
import { CheckCircle2, AlertTriangle, Info, X } from "lucide-react";

type ToastType = "success" | "error" | "info";

interface ToastItem {
  id: number;
  type: ToastType;
  message: string;
}

interface ToastContextValue {
  show: (type: ToastType, message: string) => void;
  success: (message: string) => void;
  error: (message: string) => void;
  info: (message: string) => void;
}

const ToastContext = createContext<ToastContextValue | null>(null);

export function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<ToastItem[]>([]);

  const remove = useCallback((id: number) => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  }, []);

  const show = useCallback(
    (type: ToastType, message: string) => {
      const id = Date.now() + Math.random();
      setToasts((prev) => [...prev, { id, type, message }]);
      // Auto-dismiss after 4s (6s for errors).
      setTimeout(() => remove(id), type === "error" ? 6000 : 4000);
    },
    [remove]
  );

  const ctx: ToastContextValue = {
    show,
    success: (msg) => show("success", msg),
    error: (msg) => show("error", msg),
    info: (msg) => show("info", msg),
  };

  return (
    <ToastContext.Provider value={ctx}>
      {children}
      {/* Toast container — fixed bottom-right, above all content */}
      <div
        className="fixed bottom-4 right-4 z-[100] flex flex-col gap-2 max-w-sm"
        role="alert"
        aria-live="polite"
        aria-atomic="true"
      >
        {toasts.map((t) => {
          const Icon =
            t.type === "success" ? CheckCircle2 : t.type === "error" ? AlertTriangle : Info;
          const color =
            t.type === "success"
              ? "text-ac-green"
              : t.type === "error"
                ? "text-ac-red"
                : "text-ac-blue";
          return (
            <div
              key={t.id}
              className="flex items-start gap-2 rounded-lg border border-ac-border bg-ac-surface p-3 shadow-lg animate-fade-in"
              style={{ boxShadow: "var(--shadow-lg)" }}
            >
              <Icon className={`w-4 h-4 mt-0.5 shrink-0 ${color}`} />
              <p className="text-sm text-ac-ink flex-1">{t.message}</p>
              <button
                onClick={() => remove(t.id)}
                className="text-ac-muted hover:text-ac-ink shrink-0"
                aria-label="Close notification"
              >
                <X className="w-3.5 h-3.5" />
              </button>
            </div>
          );
        })}
      </div>
    </ToastContext.Provider>
  );
}

export function useToast(): ToastContextValue {
  const ctx = useContext(ToastContext);
  if (!ctx) {
    // Fallback no-op if used outside provider — prevents crashes.
    return {
      show: () => {},
      success: () => {},
      error: () => {},
      info: () => {},
    };
  }
  return ctx;
}
