import React, { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Info, X, MessageCircle } from "lucide-react";

interface AboutDialogProps {
  open: boolean;
  onClose: () => void;
}

export function AboutDialog({ open, onClose }: AboutDialogProps) {
  const [version, setVersion] = useState("");
  const [showQR, setShowQR] = useState(false);

  useEffect(() => {
    if (open) {
      invoke<string>("get_app_version").then(setVersion).catch(() => setVersion("unknown"));
      setShowQR(false);
    }
  }, [open]);

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-sm" onClick={onClose}>
      <div
        className="bg-app-panel border border-app-border shadow-dialog w-[340px] select-none animate-[scaleIn_150ms_ease-out]"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-center justify-between px-4 py-3 border-b border-app-border">
          <div className="flex items-center gap-2">
            <Info size={16} className="text-app-accent" />
            <span className="text-sm font-mono text-app-text">关于</span>
          </div>
          <button onClick={onClose} className="p-0.5 text-app-text-dim hover:text-app-text transition-colors">
            <X size={14} />
          </button>
        </div>

        {/* Content */}
        <div className="px-4 py-6 flex flex-col items-center gap-4">
          <div className="w-16 h-16 rounded-none bg-app-accent flex items-center justify-center shadow-[0_0_20px_var(--app-glow)]">
            <span className="text-2xl font-bold text-[var(--app-bg)] font-mono">AI</span>
          </div>
          <div className="text-center space-y-1">
            <h2 className="text-sm font-semibold text-app-text">kn</h2>
            <p className="text-xs text-app-text-dim font-mono">v{version || "..."}</p>
          </div>

          {/* Author + contact */}
          <div className="w-full border-t border-app-border pt-3 space-y-1.5 text-center">
            <p className="text-2xs text-app-text-muted font-mono">
              管理 Claude Code / Codex CLI 环境变量配置
            </p>
            <p className="text-2xs text-app-text-muted font-mono">
              Built with Tauri + React + Rust
            </p>
            <p className="text-2xs text-app-accent font-mono">
              作者：程序员Shark（全网唯一ID）
            </p>
            <button
              onClick={() => setShowQR(!showQR)}
              className="inline-flex items-center gap-1 mt-1 px-3 py-1 text-2xs font-mono
                border border-app-border bg-[var(--app-input)] text-app-text-dim
                hover:text-app-accent hover:border-app-accent transition-colors"
            >
              <MessageCircle size={10} />
              {showQR ? "收起二维码" : "联系作者"}
            </button>
          </div>

          {/* QR Code grid */}
          {showQR && (
            <div className="w-full border-t border-app-border pt-3 space-y-3">
              <p className="text-2xs text-app-text-muted font-mono text-center">扫描二维码联系作者</p>
              <div className="grid grid-cols-3 gap-2">
                <QRCard label="B站" src="/qrcodes/bilibili.png" />
                <QRCard label="抖音" src="/qrcodes/douyin.png" />
                <QRCard label="微信" src="/qrcodes/wechat.png" />
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

function QRCard({ label, src }: { label: string; src: string }) {
  const [error, setError] = useState(false);
  return (
    <div className="flex flex-col items-center gap-1">
      <span className="text-2xs text-app-text-muted font-mono">{label}</span>
      {error ? (
        <div className="w-[80px] h-[80px] flex items-center justify-center border border-app-border bg-[var(--app-input)]">
          <span className="text-2xs text-app-text-muted">待添加</span>
        </div>
      ) : (
        <img
          src={src}
          alt={label}
          className="w-[80px] h-[80px] object-contain border border-app-border"
          onError={() => setError(true)}
        />
      )}
    </div>
  );
}
