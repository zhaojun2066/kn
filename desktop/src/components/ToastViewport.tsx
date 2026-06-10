import { AlertCircle, CheckCircle2, X } from "lucide-react";
import type { Toast } from "../hooks/useToasts";

interface ToastViewportProps {
  toasts: Toast[];
  onDismiss: (id: number) => void;
}

export function ToastViewport({ toasts, onDismiss }: ToastViewportProps) {
  return (
    <div className="fixed bottom-8 right-4 z-50 flex flex-col gap-1.5 max-w-sm">
      {toasts.map((toast) => (
        <div
          key={toast.id}
          className={`flex items-start gap-2 px-3 py-2 shadow-lg border animate-[slideUp_150ms_ease-out] text-sm font-mono
            ${toast.type === "error"
              ? "bg-app-red-bg border-app-border text-app-red shadow-lg"
              : "bg-app-green-bg border-app-border text-app-green shadow-lg"
            }`}
        >
          {toast.type === "error"
            ? <AlertCircle size={14} className="shrink-0 mt-px" />
            : <CheckCircle2 size={14} className="shrink-0 mt-px" />
          }
          <span className="flex-1">{toast.message}</span>
          {toast.undoAction && (
            <button
              onClick={() => { toast.undoAction!(); onDismiss(toast.id); }}
              className="shrink-0 underline text-[var(--app-amber)] hover:text-[var(--app-amber-glow)] font-mono text-xs"
            >
              {toast.undoLabel || "撤销"}
            </button>
          )}
          <button onClick={() => onDismiss(toast.id)} className="shrink-0 opacity-60 hover:opacity-100">
            <X size={12} />
          </button>
        </div>
      ))}
    </div>
  );
}
