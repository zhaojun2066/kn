import React from "react";
import { AlertTriangle, X } from "lucide-react";
import { Button } from "./common/Button";

interface ConfirmDialogProps {
  open: boolean;
  title: string;
  message: string;
  confirmLabel?: string;
  onConfirm: () => void;
  onCancel: () => void;
  loading?: boolean;
}

export function ConfirmDialog({
  open,
  title,
  message,
  confirmLabel = "删除",
  onConfirm,
  onCancel,
  loading,
}: ConfirmDialogProps) {
  if (!open) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-sm animate-[fadeIn_100ms_ease-out]">
      <div className="bg-app-panel border border-app-border shadow-dialog w-[420px] animate-[scaleIn_150ms_ease-out]">
        {/* Header */}
        <div className="flex items-center justify-between px-4 py-3 border-b border-app-border">
          <div className="flex items-center gap-2">
            <AlertTriangle size={15} className="text-app-orange" />
            <h3 className="font-semibold text-sm text-app-text font-mono">
              <span className="text-app-orange opacity-60">! </span>
              {title}
            </h3>
          </div>
          <button
            onClick={onCancel}
            className="p-1 text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)] transition-colors"
          >
            <X size={14} />
          </button>
        </div>

        {/* Body */}
        <div className="px-4 py-4">
          <p className="text-sm text-app-text-dim leading-relaxed font-mono">{message}</p>
        </div>

        {/* Footer */}
        <div className="flex justify-end gap-2 px-4 py-3 bg-[var(--app-subtle)] border-t border-app-border">
          <Button variant="secondary" size="sm" onClick={onCancel} disabled={loading}>
            取消
          </Button>
          <Button variant="danger" size="sm" onClick={onConfirm} disabled={loading}>
            {loading ? "删除中..." : confirmLabel}
          </Button>
        </div>
      </div>
    </div>
  );
}
