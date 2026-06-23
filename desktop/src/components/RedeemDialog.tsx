import React, { useState } from "react";
import { X, Loader2, CheckCircle, AlertTriangle, Gift } from "lucide-react";
import type { AgentState } from "../hooks/useAgent";

interface RedeemDialogProps {
  onClose: () => void;
  agent: AgentState;
}

type Phase = "input" | "redeeming" | "success" | "error";

export function RedeemDialog({ onClose, agent }: RedeemDialogProps) {
  const { redeemCode } = agent;
  const [phase, setPhase] = useState<Phase>("input");
  const [code, setCode] = useState("");
  const [errorMsg, setErrorMsg] = useState<string | null>(null);
  const [result, setResult] = useState<{ plan: string; days: number } | null>(null);

  const handleRedeem = async () => {
    const trimmed = code.trim();
    if (!trimmed) {
      setErrorMsg("请输入卡密");
      return;
    }
    if (!trimmed.startsWith("KN-")) {
      setErrorMsg("卡密格式无效，应以 KN- 开头");
      return;
    }
    if (trimmed.length < 50) {
      setErrorMsg("卡密格式无效，长度不足");
      return;
    }

    setPhase("redeeming");
    setErrorMsg(null);

    const res = await redeemCode(trimmed);
    if (res.ok) {
      setResult({ plan: res.plan || "pro", days: res.days || 0 });
      setPhase("success");
    } else {
      setErrorMsg(res.error || "兑换失败");
      setPhase("error");
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && phase === "input") {
      handleRedeem();
    }
  };

  const canDismiss = phase === "success" || phase === "error";

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-sm"
      onClick={canDismiss ? onClose : undefined}
    >
      <div
        className="bg-app-panel border border-app-border shadow-dialog w-[420px] select-none animate-[scaleIn_150ms_ease-out]"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-center justify-between px-4 py-3 border-b border-app-border">
          <div className="flex items-center gap-2">
            <Gift size={14} className="text-app-accent" />
            <span className="text-sm font-mono text-app-text font-semibold">兑换卡密</span>
          </div>
          <button
            onClick={onClose}
            className="p-0.5 text-app-text-dim hover:text-app-text transition-colors"
          >
            <X size={14} />
          </button>
        </div>

        {/* Body */}
        <div className="px-4 py-6 flex flex-col items-center gap-4">
          {phase === "input" && (
            <>
              <Gift size={28} className="text-app-accent" />
              <div className="text-center space-y-1">
                <div className="text-sm font-mono text-app-text">输入卡密激活 Pro 会员</div>
                <div className="text-xs font-mono text-app-text-muted">
                  卡密格式: KN-xxxx-xxxx-xxxx
                </div>
              </div>
              <input
                type="text"
                value={code}
                onChange={(e) => {
                  setCode(e.target.value);
                  setErrorMsg(null);
                }}
                onKeyDown={handleKeyDown}
                placeholder="KN-..."
                className="w-full px-3 py-2 text-sm font-mono bg-[var(--app-cmd-bg)] border border-app-border text-app-text placeholder:text-app-text-dim focus:outline-none focus:border-app-accent transition-colors"
                autoFocus
                spellCheck={false}
              />
              {errorMsg && (
                <div className="text-xs font-mono text-red-400 -mt-2">{errorMsg}</div>
              )}
              <button
                onClick={handleRedeem}
                disabled={!code.trim()}
                className="w-full px-3 py-2 text-sm font-mono bg-app-accent text-[var(--app-bg)] hover:opacity-90 transition-opacity disabled:opacity-40"
              >
                确认兑换
              </button>
              <div className="text-[11px] font-mono text-app-text-dim text-center">
                兑换成功后可在 iOS App 中查看会员状态和到期时间
              </div>
            </>
          )}

          {phase === "redeeming" && (
            <>
              <Loader2 size={28} className="animate-spin text-app-accent" />
              <div className="text-center space-y-1">
                <div className="text-sm font-mono text-app-text">正在兑换...</div>
                <div className="text-xs font-mono text-app-text-muted">正在验证卡密，请稍候</div>
              </div>
            </>
          )}

          {phase === "success" && (
            <>
              <CheckCircle size={28} className="text-emerald-400" />
              <div className="text-center space-y-1">
                <div className="text-sm font-mono text-app-text">兑换成功！</div>
                {result && (
                  <div className="text-xs font-mono text-app-text-muted space-y-0.5">
                    <div>会员方案: {result.plan.toUpperCase()}</div>
                    <div>有效期: +{result.days} 天</div>
                  </div>
                )}
                <div className="text-xs font-mono text-app-text-muted mt-2">
                  可在 iOS App 中查看会员状态
                </div>
              </div>
              <button
                onClick={onClose}
                className="mt-2 px-4 py-1.5 text-xs font-mono bg-app-accent text-[var(--app-bg)] hover:opacity-90 transition-opacity"
              >
                完成
              </button>
            </>
          )}

          {phase === "error" && (
            <>
              <AlertTriangle size={28} className="text-red-400" />
              <div className="text-center space-y-1">
                <div className="text-sm font-mono text-app-text">兑换失败</div>
                <div className="text-xs font-mono text-app-text-muted max-w-[320px] break-all">
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
                    setPhase("input");
                    setErrorMsg(null);
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
