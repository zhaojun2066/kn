import React, { useEffect, useState, useRef, useCallback } from "react";
import { X, Loader2, CheckCircle, AlertTriangle, RefreshCw } from "lucide-react";
import QRCode from "qrcode";
import type { AgentState } from "../hooks/useAgent";

interface BindDialogProps {
  onClose: () => void;
  agent: AgentState;
}

type Phase = "binding" | "polling" | "success" | "timeout" | "error";

const POLL_INTERVAL_MS = 2000;
const TIMEOUT_GRACE_MS = 10_000; // 10s grace after QR expires before hard timeout

export function BindDialog({ onClose, agent }: BindDialogProps) {
  const { bindDevice, cancelBind, fetchStatus, pausePolling, resumePolling } = agent;
  const [phase, setPhase] = useState<Phase>("binding");
  const [retryKey, setRetryKey] = useState(0);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);
  const [bindCode, setBindCode] = useState<string | null>(null);
  const [confirmUrl, setConfirmUrl] = useState<string | null>(null);
  const [expiresIn, setExpiresIn] = useState<number>(0);
  const [qrDataUrl, setQrDataUrl] = useState<string | null>(null);
  const [remainingSecs, setRemainingSecs] = useState<number>(0);
  const pollRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const countdownRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const timeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const mountedRef = useRef(true);

  const cleanup = useCallback(() => {
    if (pollRef.current) {
      clearTimeout(pollRef.current);
      pollRef.current = null;
    }
    if (timeoutRef.current) {
      clearTimeout(timeoutRef.current);
      timeoutRef.current = null;
    }
    if (countdownRef.current) {
      clearInterval(countdownRef.current);
      countdownRef.current = null;
    }
  }, []);

  // Pause background agent polling while bind dialog is open to avoid
  // duplicate IPC "status" calls (BindDialog does its own 2s polling).
  useEffect(() => {
    pausePolling();
    return () => resumePolling();
  }, [pausePolling, resumePolling]);

  // Generate QR code as data URL (no canvas ref needed)
  useEffect(() => {
    if (bindCode && confirmUrl) {
      const qrData = JSON.stringify({ c: bindCode, u: confirmUrl });
      QRCode.toDataURL(qrData, {
        width: 200,
        margin: 2,
        color: { dark: "#000000", light: "#ffffff" },
        errorCorrectionLevel: "M",
      })
        .then((url) => setQrDataUrl(url))
        .catch(() => setQrDataUrl(null));
    }
  }, [bindCode, confirmUrl]);

  useEffect(() => {
    mountedRef.current = true;

    const startBind = async () => {
      // Phase 1: Send bind request
      const result = await bindDevice();
      if (!mountedRef.current) return;

      if (!result.ok) {
        setPhase("error");
        setErrorMsg(result.error || "绑定请求失败");
        return;
      }

      // Save bind data for QR code
      if (result.bindCode) {
        setBindCode(result.bindCode);
        setConfirmUrl(result.confirmUrl || null);
        setExpiresIn(result.expiresIn || 300);
      }

      // Phase 2: Poll until connected or timeout (recursive setTimeout)
      setPhase("polling");
      const ttl = result.expiresIn || 300;
      setRemainingSecs(ttl);

      // Countdown timer (every second)
      countdownRef.current = setInterval(() => {
        setRemainingSecs((prev) => {
          if (prev <= 1) {
            if (countdownRef.current) clearInterval(countdownRef.current);
            return 0;
          }
          return prev - 1;
        });
      }, 1000);

      // Hard timeout = server TTL + 10s grace (B4: countdown matches timeout, B7: grace window for refresh)
      timeoutRef.current = setTimeout(() => {
        if (!mountedRef.current) return;
        cleanup();
        cancelBind();
        setPhase("timeout");
      }, ttl * 1000 + TIMEOUT_GRACE_MS);

      // Recursive polling — each call waits for the previous to finish,
      // preventing overlapping requests if the network is slow.
      const poll = async () => {
        if (!mountedRef.current) return;
        await fetchStatus();
        if (mountedRef.current) {
          pollRef.current = setTimeout(poll, POLL_INTERVAL_MS);
        }
      };
      poll();
    };

    startBind();

    return () => {
      mountedRef.current = false;
      cleanup();
      // Cancel backend polling when dialog is dismissed
      cancelBind();
    };
  }, [retryKey]); // eslint-disable-line react-hooks/exhaustive-deps

  // Watch agentStatus changes to detect connected state during polling
  useEffect(() => {
    if (phase === "polling" && agent.agentStatus) {
      const status = agent.agentStatus.state;
      if (status === "connected" || status === "idle" || status === "running") {
        cleanup();
        setPhase("success");
      }
    }
  }, [agent.agentStatus, phase, cleanup]);

  // Auto-close on success after a brief delay
  useEffect(() => {
    if (phase === "success") {
      const t = setTimeout(() => onClose(), 1500);
      return () => clearTimeout(t);
    }
  }, [phase, onClose]);

  const canDismiss = phase === "success" || phase === "error" || phase === "timeout";

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-sm"
      onClick={canDismiss ? onClose : undefined}
    >
      <div
        className="bg-app-panel border border-app-border shadow-dialog w-[400px] select-none animate-[scaleIn_150ms_ease-out]"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-center justify-between px-4 py-3 border-b border-app-border">
          <span className="text-sm font-mono text-app-text font-semibold">设备绑定</span>
          <button
            onClick={onClose}
            className="p-0.5 text-app-text-dim hover:text-app-text transition-colors"
          >
            <X size={14} />
          </button>
        </div>

        {/* Body */}
        <div className="px-4 py-6 flex flex-col items-center gap-4">
          {phase === "binding" && (
            <>
              <Loader2 size={28} className="animate-spin text-app-accent" />
              <div className="text-center space-y-1">
                <div className="text-sm font-mono text-app-text">正在获取绑定码...</div>
                <div className="text-xs font-mono text-app-text-muted">正在与服务器建立连接</div>
              </div>
            </>
          )}

          {phase === "polling" && (
            <>
              <div className="relative bg-white p-3 rounded-lg w-[220px] h-[220px] flex items-center justify-center">
                {qrDataUrl ? (
                  <img
                    src={qrDataUrl}
                    alt="绑定二维码"
                    className="w-[200px] h-[200px]"
                  />
                ) : (
                  <Loader2 size={28} className="animate-spin text-app-accent" />
                )}
                {/* 过期蒙层 */}
                {remainingSecs <= 0 && qrDataUrl && (
                  <div className="absolute inset-0 flex items-center justify-center bg-white/80 rounded-lg">
                    <div className="text-center">
                      <AlertTriangle size={20} className="text-amber-400 mx-auto mb-1" />
                      <div className="text-xs font-mono text-app-text-dim">二维码已过期</div>
                    </div>
                  </div>
                )}
              </div>
              <div className="text-center space-y-1">
                <div className="text-sm font-mono text-app-text">请用 KN App 扫码绑定</div>
                <div className="text-xs font-mono text-app-text-muted">
                  {remainingSecs > 0
                    ? `二维码有效期: ${Math.floor(remainingSecs / 60)} 分 ${remainingSecs % 60} 秒`
                    : "二维码已过期"}
                </div>
              </div>
              <button
                onClick={() => {
                  cleanup();
                  cancelBind();
                  setPhase("binding");
                  setBindCode(null);
                  setConfirmUrl(null);
                  setQrDataUrl(null);
                  setRetryKey((k) => k + 1);
                }}
                disabled={remainingSecs > 0}
                className="flex items-center gap-1.5 px-3 py-1.5 text-xs font-mono border border-app-border text-app-text-dim hover:text-app-text transition-colors disabled:opacity-30 disabled:cursor-not-allowed"
                title={remainingSecs > 0 ? "二维码过期后才可刷新" : "刷新二维码"}
              >
                <RefreshCw size={12} />
                刷新二维码
              </button>
            </>
          )}

          {phase === "success" && (
            <>
              <CheckCircle size={28} className="text-emerald-400" />
              <div className="text-center space-y-1">
                <div className="text-sm font-mono text-app-text">绑定成功</div>
                <div className="text-xs font-mono text-app-text-muted">
                  设备已成功绑定，可进行远程控制
                </div>
              </div>
            </>
          )}

          {phase === "timeout" && (
            <>
              <AlertTriangle size={28} className="text-amber-400" />
              <div className="text-center space-y-1">
                <div className="text-sm font-mono text-app-text">绑定超时</div>
                <div className="text-xs font-mono text-app-text-muted">
                  二维码已过期，请重新获取
                </div>
              </div>
              <div className="flex gap-2">
                <button
                  onClick={onClose}
                  className="px-4 py-1.5 text-xs font-mono border border-app-border text-app-text-dim hover:text-app-text transition-colors"
                >
                  关闭
                </button>
                <button
                  onClick={() => {
                    setPhase("binding");
                    setBindCode(null);
                    setConfirmUrl(null);
                    setQrDataUrl(null);
                    setRetryKey((k) => k + 1);
                  }}
                  className="px-4 py-1.5 text-xs font-mono bg-app-accent text-[var(--app-bg)] hover:opacity-90 transition-opacity"
                >
                  重新获取
                </button>
              </div>
            </>
          )}

          {phase === "error" && (
            <>
              <AlertTriangle size={28} className="text-red-400" />
              <div className="text-center space-y-1">
                <div className="text-sm font-mono text-app-text">绑定失败</div>
                <div className="text-xs font-mono text-app-text-muted max-w-[280px] break-all">
                  {errorMsg || "未知错误"}
                </div>
              </div>
              <div className="flex gap-2">
                <button
                  onClick={onClose}
                  className="px-4 py-1.5 text-xs font-mono border border-app-border text-app-text-dim hover:text-app-text transition-colors"
                >
                  关闭
                </button>
                <button
                  onClick={() => {
                    setPhase("binding");
                    setErrorMsg(null);
                    setBindCode(null);
                    setConfirmUrl(null);
                    setQrDataUrl(null);
                    setRetryKey((k) => k + 1);
                  }}
                  className="px-4 py-1.5 text-xs font-mono bg-app-accent text-[var(--app-bg)] hover:opacity-90 transition-opacity"
                >
                  重试
                </button>
              </div>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
