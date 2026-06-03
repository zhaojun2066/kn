import React from "react";
import { Download, X } from "lucide-react";
import { Button } from "./common/Button";

interface UpdateDialogProps {
  open: boolean;
  version: string;
  notes: string;
  onConfirm: () => void;
  onCancel: () => void;
}

export function UpdateDialog({
  open,
  version,
  notes,
  onConfirm,
  onCancel,
}: UpdateDialogProps) {
  if (!open) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-sm animate-[fadeIn_100ms_ease-out]">
      <div className="bg-app-panel border border-app-border shadow-dialog w-[460px] max-h-[80vh] flex flex-col animate-[scaleIn_150ms_ease-out]">
        {/* Header */}
        <div className="flex items-center justify-between px-4 py-3 border-b border-app-border shrink-0">
          <div className="flex items-center gap-2">
            <Download size={15} className="text-app-accent" />
            <h3 className="font-semibold text-sm text-app-text font-mono">
              发现新版本
              <span className="text-app-accent ml-1.5">{version}</span>
            </h3>
          </div>
          <button
            onClick={onCancel}
            className="p-1 text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)] transition-colors"
          >
            <X size={14} />
          </button>
        </div>

        {/* Release Notes — scrollable */}
        <div className="px-4 py-4 overflow-y-auto flex-1">
          <p className="text-2xs text-app-text-muted font-mono uppercase tracking-wider mb-2">
            更新内容
          </p>
          <pre className="text-sm text-app-text-dim leading-relaxed font-mono whitespace-pre-wrap">
            {notes}
          </pre>
        </div>

        {/* Footer */}
        <div className="flex justify-end gap-2 px-4 py-3 bg-[var(--app-subtle)] border-t border-app-border shrink-0">
          <Button variant="secondary" size="sm" onClick={onCancel}>
            稍后
          </Button>
          <Button variant="primary" size="sm" onClick={onConfirm}>
            立即更新
          </Button>
        </div>
      </div>
    </div>
  );
}
