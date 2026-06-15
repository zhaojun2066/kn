import React from "react";
import { AlertTriangle, ArrowRight, X } from "lucide-react";
import { Button } from "./common/Button";
import { Dialog } from "./common/Dialog";

interface ConfirmDialogProps {
  open: boolean;
  title: string;
  message: string;
  confirmLabel?: string;
  onConfirm: () => void;
  onCancel: () => void;
  loading?: boolean;
  variant?: "danger" | "primary";
}

export function ConfirmDialog({
  open,
  title,
  message,
  confirmLabel = "确认",
  onConfirm,
  onCancel,
  loading,
  variant = "danger",
}: ConfirmDialogProps) {
  const isPrimary = variant === "primary";

  return (
    <Dialog open={open} onClose={onCancel} persistent>
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-3 border-b border-app-border">
        <div className="flex items-center gap-2">
          {isPrimary
            ? <ArrowRight size={15} className="text-[var(--app-accent)]" aria-hidden="true" />
            : <AlertTriangle size={15} className="text-app-orange" aria-hidden="true" />
          }
          <h3 className="font-semibold text-sm text-app-text font-mono">
            {!isPrimary && <span className="text-app-orange opacity-60">! </span>}
            {title}
          </h3>
        </div>
        <button
          onClick={onCancel}
          aria-label="关闭"
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
        <Button variant={isPrimary ? "primary" : "danger"} size="sm" onClick={onConfirm} disabled={loading}>
          {loading ? (isPrimary ? "移动中..." : "删除中...") : confirmLabel}
        </Button>
      </div>
    </Dialog>
  );
}
