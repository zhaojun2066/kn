import React from "react";
import { Download, X, CheckCircle } from "lucide-react";
import { Button } from "./common/Button";

interface UpdateDialogProps {
  open: boolean;
  version: string;
  notes: string;
  downloading: boolean;
  progress: number;
  downloadError: string | null;
  onConfirm: () => void;
  onCancel: () => void;
}

export function UpdateDialog({
  open,
  version,
  notes,
  downloading,
  progress,
  downloadError,
  onConfirm,
  onCancel,
}: UpdateDialogProps) {
  if (!open) return null;

  const done = !downloading && progress >= 100;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-sm animate-[fadeIn_100ms_ease-out]">
      <div className="bg-app-panel border border-app-border shadow-dialog w-[460px] max-h-[80vh] flex flex-col animate-[scaleIn_150ms_ease-out]">
        {/* Header */}
        <div className="flex items-center justify-between px-4 py-3 border-b border-app-border shrink-0">
          <div className="flex items-center gap-2">
            {done ? (
              <CheckCircle size={15} className="text-app-accent" />
            ) : (
              <Download size={15} className={downloading ? "text-app-amber animate-pulse" : "text-app-accent"} />
            )}
            <h3 className="font-semibold text-sm text-app-text font-mono">
              {done ? "下载完成" : "发现新版本"}
              <span className="text-app-accent ml-1.5">{version}</span>
            </h3>
          </div>
          {!downloading && (
            <button
              onClick={onCancel}
              className="p-1 text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)] transition-colors"
            >
              <X size={14} />
            </button>
          )}
        </div>

        {/* Body */}
        <div className="px-4 py-4 overflow-y-auto flex-1">
          {downloading || done ? (
            <div className="space-y-4 py-4">
              {/* Progress bar */}
              <div className="space-y-2">
                <div className="flex items-center justify-between text-xs font-mono">
                  <span className="text-app-text-dim">
                    {done ? "已保存到本地" : "正在下载..."}
                  </span>
                  <span className="text-app-amber tabular-nums">{progress}%</span>
                </div>
                <div className="h-2 bg-[var(--app-cmd-bg)] border border-app-border overflow-hidden">
                  <div
                    className={`h-full transition-all duration-300 ${
                      done ? "bg-app-accent" : "bg-app-amber"
                    }`}
                    style={{ width: `${Math.max(progress, 2)}%` }}
                  />
                </div>
                {!done && (
                  <p className="text-2xs text-app-text-muted font-mono">
                    文件较大，请耐心等待。下载速度取决于网络状况。
                  </p>
                )}
              </div>
            </div>
          ) : downloadError ? (
            <div className="space-y-4 py-4">
              <div className="flex items-start gap-2 text-sm text-app-red font-mono">
                <X size={14} className="mt-0.5 shrink-0" />
                <span>{downloadError}</span>
              </div>
            </div>
          ) : (
            <>
              <p className="text-2xs text-app-text-muted font-mono uppercase tracking-wider mb-2">
                更新内容
              </p>
              <pre className="text-sm text-app-text-dim leading-relaxed font-mono whitespace-pre-wrap">
                {notes}
              </pre>
            </>
          )}
        </div>

        {/* Footer */}
        <div className="flex justify-end gap-2 px-4 py-3 bg-[var(--app-subtle)] border-t border-app-border shrink-0">
          {downloading ? (
            <Button variant="secondary" size="sm" onClick={onCancel}>
              取消
            </Button>
          ) : downloadError ? (
            <>
              <Button variant="secondary" size="sm" onClick={onCancel}>
                取消
              </Button>
              <Button variant="primary" size="sm" onClick={onConfirm}>
                重试
              </Button>
            </>
          ) : done ? (
            <Button variant="primary" size="sm" onClick={onCancel}>
              好的
            </Button>
          ) : (
            <>
              <Button variant="secondary" size="sm" onClick={onCancel}>
                稍后
              </Button>
              <Button variant="primary" size="sm" onClick={onConfirm}>
                立即更新
              </Button>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
