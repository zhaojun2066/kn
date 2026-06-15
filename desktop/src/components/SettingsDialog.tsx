import React from "react";
import { Settings, X, RotateCcw } from "lucide-react";
import { useFontScale, MIN_SCALE, MAX_SCALE } from "../hooks/useFontScale";
import { getUsageTrackingEnabled, setUsageTrackingEnabled } from "../lib/tauri-api";

interface SettingsDialogProps {
  open: boolean;
  onClose: () => void;
}

export function SettingsDialog({ open, onClose }: SettingsDialogProps) {
  const { scale, setScale } = useFontScale();
  const [trackingEnabled, setTrackingEnabled] = React.useState(false);
  React.useEffect(() => {
    if (open) {
      getUsageTrackingEnabled().then(setTrackingEnabled).catch(() => {});
    }
  }, [open]);

  if (!open) return null;

  const pct = Math.round(scale * 100);

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-sm"
      onClick={onClose}
    >
      <div
        className="bg-app-panel border border-app-border shadow-dialog w-[420px] select-none animate-[scaleIn_150ms_ease-out]"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-center justify-between px-4 py-3 border-b border-app-border">
          <div className="flex items-center gap-2">
            <Settings size={15} className="text-app-accent" />
            <span className="text-sm font-mono text-app-text font-semibold">设置</span>
          </div>
          <button
            onClick={onClose}
            className="p-0.5 text-app-text-dim hover:text-app-text transition-colors"
          >
            <X size={14} />
          </button>
        </div>

        {/* Body */}
        <div className="px-4 py-5 space-y-5">
          {/* Font scale section */}
          <div className="space-y-3">
            <div className="flex items-center justify-between">
              <label className="text-sm text-app-text font-mono">UI 字体大小</label>
              <div className="flex items-center gap-2">
                <span className="text-sm text-app-accent font-mono tabular-nums w-10 text-right">
                  {pct}%
                </span>
              </div>
            </div>

            {/* Slider */}
            <div className="space-y-1">
              <input
                type="range"
                min={Math.round(MIN_SCALE * 100)}
                max={Math.round(MAX_SCALE * 100)}
                step="1"
                value={pct}
                onChange={(e) => setScale(parseInt(e.target.value) / 100)}
                className="w-full h-1.5 bg-[var(--app-input)] rounded-full appearance-none cursor-pointer
                  [&::-webkit-slider-thumb]:appearance-none [&::-webkit-slider-thumb]:w-4 [&::-webkit-slider-thumb]:h-4
                  [&::-webkit-slider-thumb]:rounded-full [&::-webkit-slider-thumb]:bg-app-accent
                  [&::-webkit-slider-thumb]:shadow-[0_0_6px_var(--app-glow)] [&::-webkit-slider-thumb]:cursor-pointer
                  [&::-webkit-slider-thumb]:transition-transform [&::-webkit-slider-thumb]:hover:scale-110"
              />
              <div className="flex justify-between text-2xs text-app-text-muted font-mono">
                <span>{Math.round(MIN_SCALE * 100)}%</span>
                <span>{Math.round(MAX_SCALE * 100)}%</span>
              </div>
            </div>
          </div>

          {/* Preview */}
          <div className="space-y-2">
            <span className="text-2xs text-app-text-muted uppercase tracking-wider font-mono">预览</span>
            <div className="border border-app-border bg-[var(--app-cmd-bg)] px-4 py-3 space-y-2 rounded">
              <p className="text-lg text-app-text font-mono">这是标题文字</p>
              <p className="text-base text-app-text-dim font-mono">这是正文内容，用于预览字体大小变化效果。</p>
              <p className="text-xs text-app-text-muted font-mono">这是辅助说明文字 — 拖动上方滑块实时调整。</p>
            </div>
          </div>

          {/* Terminal note */}
          <div className="flex items-start gap-2 text-2xs text-app-text-muted font-mono bg-[var(--app-subtle)] border border-app-border px-3 py-2">
            <span>💡</span>
            <span>终端内字体大小请在终端面板中使用 A⁻ / A⁺ 按钮独立调整，不受此处设置影响。</span>
          </div>

          {/* Token usage tracking toggle */}
          <div className="space-y-2">
            <div className="flex items-center justify-between">
              <div>
                <span className="text-sm text-app-text font-mono">Token 用量追踪</span>
                <p className="text-2xs text-app-text-muted font-mono mt-0.5">
                  自动记录每次 AI 会话的 token 消耗和费用
                </p>
              </div>
              <button
                onClick={async () => {
                  const next = !trackingEnabled;
                  try {
                    await setUsageTrackingEnabled(next);
                    setTrackingEnabled(next);
                  } catch (e) {
                    console.error("Failed to toggle usage tracking:", e);
                  }
                }}
                className={`relative w-9 h-5 rounded-full transition-colors duration-200 ${
                  trackingEnabled ? "bg-app-accent" : "bg-[var(--app-border)]"
                }`}
              >
                <span
                  className={`absolute top-0.5 w-4 h-4 rounded-full bg-[var(--app-bg)] border border-app-border transition-all duration-200 ${
                    trackingEnabled ? "left-4" : "left-0.5"
                  }`}
                />
              </button>
            </div>
            {trackingEnabled && (
              <div className="text-2xs text-app-text-muted font-mono bg-[var(--app-subtle)] border border-app-border px-3 py-1.5">
                数据保存在 ~/.kn/usage.jsonl，完全本地，不会上传。
              </div>
            )}
          </div>
        </div>

        {/* Footer */}
        <div className="px-4 py-2.5 border-t border-app-border bg-[var(--app-subtle)] flex items-center justify-between">
          <button
            onClick={() => setScale(1.0)}
            disabled={scale === 1.0}
            className="flex items-center gap-1 px-3 py-1 text-xs font-mono transition-colors
              border border-app-border
              disabled:opacity-30 disabled:cursor-not-allowed
              bg-[var(--app-input)] hover:bg-[var(--app-hover)]
              text-app-text-dim hover:text-app-text"
            title="恢复默认字体大小"
          >
            <RotateCcw size={11} />
            恢复默认
          </button>
          <button
            onClick={onClose}
            className="px-4 py-1 text-xs font-mono text-app-text-dim hover:text-app-text
              border border-app-border bg-[var(--app-input)] hover:bg-[var(--app-hover)]
              transition-colors"
          >
            关闭
          </button>
        </div>
      </div>
    </div>
  );
}
