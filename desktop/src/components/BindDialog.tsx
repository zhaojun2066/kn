import React, { useEffect, useState, useRef, useCallback } from "react";
import { X, Loader2, CheckCircle, AlertTriangle, RefreshCw } from "lucide-react";
import QRCode from "qrcode";
import type { AgentState } from "../hooks/useAgent";

interface BindDialogProps {
  onClose: () => void;
  agent: AgentState;
}

type Phase = "binding" | "polling" | "activating" | "connecting" | "success" | "timeout" | "error";

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
  const [isRestarting, setIsRestarting] = useState(false);
  const [isClosing, setIsClosing] = useState(false);
  const pollRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const countdownRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const timeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const effectGenerationRef = useRef(0);
  const activeBindRequestRef = useRef<ReturnType<AgentState["bindDevice"]> | null>(null);
  const cancelStaleBindRef = useRef(false);

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

  const resetBindingView = useCallback(() => {
    activeBindRequestRef.current = null;
    setErrorMsg(null);
    setBindCode(null);
    setConfirmUrl(null);
    setQrDataUrl(null);
    setRemainingSecs(0);
  }, []);

  const cancelBindingForClose = useCallback(async () => {
    cancelStaleBindRef.current = true;
    effectGenerationRef.current += 1;
    cleanup();
    setIsClosing(true);
    const result = await cancelBind();
    setIsClosing(false);
    if (!result.ok && result.status !== "activation_uncertain") {
      setPhase("error");
      setErrorMsg(result.error || "取消绑定失败，请稍后重试");
      return;
    }
    onClose();
  }, [cancelBind, cleanup, onClose]);

  const handleClose = useCallback(() => {
    if (phase === "activating" || phase === "connecting" || phase === "success" || phase === "timeout" || phase === "error") {
      cleanup();
      onClose();
      return;
    }
    void cancelBindingForClose();
  }, [cancelBindingForClose, cleanup, onClose, phase]);

  const restartWithFreshQr = useCallback(async () => {
    // A new QR is only valid after the old pending pairing is explicitly
    // cancelled.  Do not race a second bind-init against the old worker.
    setIsRestarting(true);
    cancelStaleBindRef.current = true;
    effectGenerationRef.current += 1;
    cleanup();
    const result = await cancelBind();
    setIsRestarting(false);

    if (!result.ok) {
      if (result.status === "activation_uncertain") {
        setPhase("activating");
      } else {
        setPhase("error");
        setErrorMsg(result.error || "取消旧绑定失败，无法生成新二维码");
      }
      return;
    }

    resetBindingView();
    setPhase("binding");
    setRetryKey((key) => key + 1);
  }, [cancelBind, cleanup, resetBindingView]);

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
    const generation = ++effectGenerationRef.current;

    const startBind = async () => {
      // StrictMode replays effects in development. Keep one in-flight IPC call;
      // Agent additionally resumes the persisted pairing instead of issuing a new QR.
      const request = activeBindRequestRef.current ?? bindDevice();
      activeBindRequestRef.current = request;
      const result = await request;
      if (effectGenerationRef.current !== generation) {
        if (result.ok && cancelStaleBindRef.current) {
          cancelStaleBindRef.current = false;
          void cancelBind();
        }
        return;
      }

      if (!result.ok) {
        activeBindRequestRef.current = null;
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

      timeoutRef.current = setTimeout(() => {
        if (effectGenerationRef.current !== generation) return;
        cleanup();
        void cancelBind().then((result) => {
          if (effectGenerationRef.current !== generation) return;
          if (!result.ok && result.status === "activation_uncertain") {
            setPhase("activating");
          } else if (!result.ok) {
            setPhase("error");
            setErrorMsg(result.error || "二维码已过期，但取消绑定失败，请稍后重试");
          } else {
            setPhase("timeout");
          }
        });
      }, ttl * 1000 + TIMEOUT_GRACE_MS);

      // Recursive polling — each call waits for the previous to finish,
      // preventing overlapping requests if the network is slow.
      const poll = async () => {
        if (effectGenerationRef.current !== generation) return;
        await fetchStatus();
        if (effectGenerationRef.current === generation) {
          pollRef.current = setTimeout(poll, POLL_INTERVAL_MS);
        }
      };
      poll();
    };

    startBind();

    return () => {
      cleanup();
      effectGenerationRef.current += 1;
    };
  }, [retryKey]); // eslint-disable-line react-hooks/exhaustive-deps

  // The Agent exposes a durable binding state. It is the only source for
  // "phone confirmed" / "activating"; desktop never guesses from the QR timer.
  useEffect(() => {
    if ((phase === "polling" || phase === "activating" || phase === "connecting") && agent.agentStatus) {
      if (
        agent.agentStatus.binding?.state === "activating" ||
        agent.agentStatus.binding?.state === "activationUncertain"
      ) {
        cleanup();
        setPhase("activating");
      } else if (agent.isConnected) {
        cleanup();
        activeBindRequestRef.current = null;
        setPhase("success");
      } else if (agent.isBound) {
        cleanup();
        setPhase("connecting");
      }
    }
  }, [agent.agentStatus, agent.isBound, agent.isConnected, phase, cleanup]);

  // Auto-close on success after a brief delay
  useEffect(() => {
    if (phase === "success") {
      const t = setTimeout(() => onClose(), 1500);
      return () => clearTimeout(t);
    }
  }, [phase, onClose]);

  const canDismiss = phase === "success" || phase === "connecting" || phase === "error" || phase === "timeout";

  return (
    <div
      className="fixed inset-0 z-[120] flex items-center justify-center app-dialog-backdrop"
      onClick={canDismiss ? handleClose : undefined}
    >
      <div
        className="app-dialog-panel bg-app-panel border border-app-border w-[400px] select-none animate-[scaleIn_150ms_ease-out]"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-center justify-between px-4 py-3 border-b border-app-border">
          <span className="text-sm font-mono text-app-text font-semibold">设备绑定</span>
          <button
            onClick={handleClose}
            disabled={isClosing}
            className="p-0.5 text-app-text-dim hover:text-app-text transition-colors"
            title={phase === "activating" || phase === "connecting" ? "隐藏窗口" : "取消本次绑定"}
          >
            {isClosing ? <Loader2 size={14} className="animate-spin" /> : <X size={14} />}
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
                onClick={() => void restartWithFreshQr()}
                disabled={isRestarting || isClosing}
                className="flex items-center gap-1.5 px-3 py-1.5 text-xs font-mono border border-app-border text-app-text-dim hover:text-app-text transition-colors disabled:opacity-30 disabled:cursor-not-allowed"
                title="取消当前申请并生成新二维码"
              >
                <RefreshCw size={12} />
                {isRestarting ? "正在生成..." : "重新生成二维码"}
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

          {phase === "connecting" && (
            <>
              <Loader2 size={28} className="animate-spin text-app-accent" />
              <div className="text-center space-y-1">
                <div className="text-sm font-mono text-app-text">正式绑定完成，正在连接</div>
                <div className="text-xs font-mono text-app-text-muted">
                  凭证已安全保存；电脑联网后会自动上线
                </div>
              </div>
              <button
                onClick={handleClose}
                className="px-4 py-1.5 text-xs font-mono border border-app-border text-app-text-dim hover:text-app-text transition-colors"
              >
                隐藏窗口
              </button>
            </>
          )}

          {phase === "activating" && (
            <>
              <Loader2 size={28} className="animate-spin text-app-accent" />
              <div className="text-center space-y-1">
                <div className="text-sm font-mono text-app-text">手机已确认，正在确认设备</div>
                <div className="text-xs font-mono text-app-text-muted">
                  电脑正在安全保存凭证；完成后将自动连接
                </div>
              </div>
              <button
                onClick={handleClose}
                className="px-4 py-1.5 text-xs font-mono border border-app-border text-app-text-dim hover:text-app-text transition-colors"
              >
                隐藏窗口
              </button>
              <div className="text-xs font-mono text-app-text-muted">
                正在确认正式设备，此阶段不可取消
              </div>
            </>
          )}

          {phase === "timeout" && (
            <>
              <AlertTriangle size={28} className="text-amber-400" />
              <div className="text-center space-y-1">
                <div className="text-sm font-mono text-app-text">二维码已过期</div>
                <div className="text-xs font-mono text-app-text-muted">
                  本次绑定已取消，请重新生成二维码
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
                    void restartWithFreshQr();
                  }}
                  disabled={isRestarting}
                  className="px-4 py-1.5 text-xs font-mono border border-app-border text-app-text-dim hover:text-app-text transition-colors disabled:opacity-50"
                >
                  {isRestarting ? "正在生成..." : "重新生成二维码"}
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
                  onClick={handleClose}
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
                  className="app-primary-action px-4 py-1.5 text-xs font-medium"
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
