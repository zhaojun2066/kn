import React from "react";
import { Download, X, CheckCircle, ShieldCheck, AlertTriangle } from "lucide-react";
import { Button } from "./common/Button";

interface UpdateDialogProps {
  open: boolean;
  version: string;
  notes: string;
  mandatory: boolean;
  downloading: boolean;
  progress: number;
  downloaded: number;
  total: number | null;
  speedBytesPerSecond: number | null;
  etaSeconds: number | null;
  downloadError: string | null;
  onConfirm: () => void;
  onCancel: () => void;
}

// ── Segmented progress bar ────────────────────────────────────

function SegmentedBar({ pct, done }: { pct: number; done: boolean }) {
  const width = 36;
  const filled = Math.round((pct / 100) * width);
  const blocks = Array.from({ length: width }, (_, i) => i < filled);

  return (
    <div className="font-mono text-xs leading-none tracking-[-0.5px] select-none">
      <div className="flex items-center gap-0.5">
        <span className="text-app-text-dim opacity-70">[</span>
        <span className="flex gap-px">
          {blocks.map((on, i) => (
            <span
              key={i}
              className={`w-[5px] h-[11px] inline-block transition-colors duration-200 ${
                on
                  ? done
                    ? "bg-app-accent shadow-[0_0_4px_var(--app-glow)]"
                    : "bg-app-amber shadow-[0_0_4px_var(--app-glow-amber)]"
                  : i === filled
                  ? "bg-app-amber animate-pulse shadow-[0_0_6px_var(--app-glow-amber)]"
                  : "bg-[var(--app-border)] opacity-50"
              }`}
            />
          ))}
        </span>
        <span className="text-app-text-dim opacity-70">]</span>
        <span className={`tabular-nums ml-2 min-w-[3.5ch] text-right ${
          done ? "text-app-accent" : "text-app-amber"
        }`}>
          {pct}%
        </span>
      </div>
    </div>
  );
}

function formatBytes(bytes: number): string {
  if (bytes >= 1_000_000_000) return `${(bytes / 1_000_000_000).toFixed(1)} GB`;
  if (bytes >= 1_000_000) return `${(bytes / 1_000_000).toFixed(1)} MB`;
  if (bytes >= 1_000) return `${(bytes / 1_000).toFixed(0)} KB`;
  return `${bytes} B`;
}

function formatEta(seconds: number | null): string {
  if (seconds === null || !Number.isFinite(seconds)) return "--";
  if (seconds < 60) return `${Math.ceil(seconds)} 秒`;
  return `${Math.ceil(seconds / 60)} 分钟`;
}

// ── Main component ────────────────────────────────────────────

export function UpdateDialog({
  open,
  version,
  notes,
  mandatory,
  downloading,
  progress,
  downloaded,
  total,
  speedBytesPerSecond,
  etaSeconds,
  downloadError,
  onConfirm,
  onCancel,
}: UpdateDialogProps) {
  if (!open) return null;

  const done = !downloading && progress >= 100;
  return (
    <div className="fixed inset-0 z-[120] flex items-center justify-center app-dialog-backdrop animate-[fadeIn_100ms_ease-out]">
      <div className="app-dialog-panel bg-app-panel border border-app-border w-[480px] max-h-[85vh] flex flex-col animate-[scaleIn_150ms_ease-out]">

        {/* ── Header: phase indicator ────────────────────────── */}
        <div className="flex items-center justify-between px-4 py-3 border-b border-app-border shrink-0 bg-[var(--app-subtle)]">
          <div className="flex items-center gap-2.5">
            {done ? (
              <CheckCircle size={14} className="text-app-accent" />
            ) : downloading ? (
              <div className="relative">
                <Download size={14} className="text-app-amber" />
                <span className="absolute inset-0 rounded-full border border-app-amber animate-ping opacity-30" />
              </div>
            ) : downloadError ? (
              <AlertTriangle size={14} className="text-app-red" />
            ) : (
              <Download size={14} className="text-app-accent" />
            )}
            <div>
              <h3 className="font-semibold text-sm text-app-text font-mono leading-tight">
                {done ? "DOWNLOAD COMPLETE" : downloading ? "DOWNLOADING..." : downloadError ? "DOWNLOAD FAILED" : "UPDATE AVAILABLE"}
              </h3>
              <p className="text-2xs text-app-text-muted font-mono uppercase tracking-wider">
                v{version}
              </p>
            </div>
          </div>
          {!downloading && !mandatory && (
            <button
              onClick={onCancel}
              className="p-1 text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)] transition-colors"
            >
              <X size={14} />
            </button>
          )}
        </div>

        {/* ── Body ─────────────────────────────────────────────── */}
        <div className="px-4 py-5 overflow-y-auto flex-1 space-y-4">
          {downloading || done ? (
            <>
              {/* ── Progress Section ───────────────────────────── */}
              <div className="relative bg-[var(--app-terminal-bg)] border border-app-border p-4 space-y-3">
                {/* Segmented bar */}
                <div className="flex items-center justify-between">
                  <span className="text-2xs text-app-text-muted font-mono uppercase tracking-wider">
                    {done ? "TRANSFER COMPLETE" : "TRANSFER"}
                  </span>
                  <span className={`text-2xs font-mono tabular-nums ${
                    done ? "text-app-accent" : "text-app-amber animate-pulse"
                  }`}>
                    {done ? "OK" : "IN PROGRESS"}
                  </span>
                </div>

                <SegmentedBar pct={progress} done={done} />
                <p className="text-2xs font-mono text-app-text-muted">
                  {total && total > 0 ? `${formatBytes(downloaded)} / ${formatBytes(total)}` : `已下载 ${formatBytes(downloaded)}，正在获取总大小`}
                </p>
                {downloading && (
                  <p className="text-2xs font-mono text-app-text-muted">
                    {speedBytesPerSecond === null ? "正在计算下载速度" : `${formatBytes(speedBytesPerSecond)}/s · 剩余 ${formatEta(etaSeconds)}`}
                  </p>
                )}

                {/* Scan-line overlay effect */}
                <div
                  className="absolute inset-0 pointer-events-none opacity-[0.03]"
                  style={{
                    background: "repeating-linear-gradient(0deg, transparent, transparent 2px, var(--app-accent) 2px, var(--app-accent) 3px)",
                  }}
                />
              </div>

              {/* ── Verification pending (only while downloading) ── */}
              {downloading && (
                <div className="flex items-center gap-2 text-2xs text-app-text-muted font-mono">
                  <ShieldCheck size={11} className="shrink-0 opacity-50" />
                  <span>正在下载；完成后将校验 SHA-256 完整性。</span>
                </div>
              )}

              {/* ── Verified badge ──────────────────────────────── */}
              {done && (
                <div className="flex items-center gap-2 text-2xs text-app-accent font-mono">
                  <ShieldCheck size={11} className="shrink-0" />
                  <span>SHA-256 校验通过，安装包完整。</span>
                </div>
              )}

              {/* Release notes (collapsed during download) */}
              {notes && done && (
                <details className="group">
                  <summary className="text-2xs text-app-text-muted font-mono uppercase tracking-wider cursor-pointer hover:text-app-text-dim transition-colors select-none">
                    Release Notes
                  </summary>
                  <pre className="mt-2 text-xs text-app-text-dim leading-relaxed font-mono whitespace-pre-wrap bg-[var(--app-input)] border border-app-border p-3">
                    {notes}
                  </pre>
                </details>
              )}
            </>
          ) : downloadError ? (
            /* ── Error state ──────────────────────────────────── */
            <div className="bg-[var(--app-red-bg)] border border-[var(--btn-danger-border)] p-4 space-y-2">
              <div className="flex items-start gap-2">
                <AlertTriangle size={14} className="text-app-red mt-0.5 shrink-0" />
                <div className="space-y-1">
                  <p className="text-sm text-app-red font-mono font-semibold">Download Error</p>
                  <p className="text-xs text-app-text-dim font-mono">{downloadError}</p>
                </div>
              </div>
            </div>
          ) : (
            /* ── Pre-download: release notes ──────────────────── */
            <>
              <div className="flex items-center gap-2 text-2xs text-app-text-muted font-mono uppercase tracking-wider">
                <span className="w-1 h-1 bg-app-accent" />
                Release Notes
              </div>
              <pre className="text-sm text-app-text-dim leading-relaxed font-mono whitespace-pre-wrap bg-[var(--app-input)] border border-app-border p-3 max-h-[200px] overflow-y-auto">
                {notes}
              </pre>

              <div className="flex items-center gap-2 text-2xs text-app-text-muted font-mono pt-1">
                <ShieldCheck size={11} className="shrink-0 opacity-50" />
                <span>Download will be verified with SHA256 after completion.</span>
              </div>
              {mandatory && (
                <div className="flex items-center gap-2 text-xs text-app-red font-mono border border-app-red/40 bg-app-red/10 p-2">
                  <AlertTriangle size={12} className="shrink-0" />
                  <span>当前版本必须升级后才能继续使用远程控制。</span>
                </div>
              )}
            </>
          )}
        </div>

        {/* ── Footer: terminal prompt buttons ─────────────────── */}
        <div className="flex justify-end gap-2 px-4 py-3 bg-[var(--app-subtle)] border-t border-app-border shrink-0">
          {downloading ? (
            !mandatory ? <Button variant="secondary" size="sm" onClick={onCancel}>取消</Button> : null
          ) : downloadError ? (
            <>
              {!mandatory && <Button variant="secondary" size="sm" onClick={onCancel}>关闭</Button>}
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
              {!mandatory && <Button variant="secondary" size="sm" onClick={onCancel}>稍后</Button>}
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
