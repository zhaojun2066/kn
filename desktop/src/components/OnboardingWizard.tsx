import React, { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { formatShortcut } from "../utils/shortcut";
import { Terminal, Check, X as XIcon, AlertTriangle, ChevronRight, ChevronLeft, Search, Plus, Play } from "lucide-react";
import { Button } from "./common/Button";
import type { EnvCheckItem, EnvCheckResult } from "../lib/types";
import { itemSeverity } from "../lib/types";

// ── Types ──────────────────────────────────────────────────

interface OnboardingWizardProps {
  hasProfiles: boolean;
  onScan: () => void;
  onCreate: () => void;
  onDismiss?: () => void;
}

function StatusIconForItem({ item }: { item: EnvCheckItem }) {
  const severity = itemSeverity(item);
  if (severity === "ok") return <Check size={13} className="text-app-green" />;
  if (severity === "warn") return <AlertTriangle size={13} className="text-app-amber" />;
  if (severity === "error") return <XIcon size={13} className="text-app-red" />;
  return <AlertTriangle size={13} className="text-app-text-muted" />;
}

// ── Step indicators ────────────────────────────────────────
const STEPS = ["环境检查", "导入配置", "准备就绪"];

function StepDots({ current }: { current: number }) {
  return (
    <div className="flex items-center gap-3 mb-6">
      {STEPS.map((label, i) => (
        <React.Fragment key={i}>
          {i > 0 && (
            <div className={`w-8 h-px transition-colors duration-300 ${i <= current ? "bg-app-accent" : "bg-app-border"}`} />
          )}
          <div className="flex items-center gap-1.5">
            <span
              className={`w-6 h-6 flex items-center justify-center text-2xs font-mono font-bold transition-all duration-300
                ${i < current
                  ? "bg-app-green text-[var(--app-bg)]"
                  : i === current
                    ? "bg-app-accent text-[var(--app-bg)] shadow-[0_0_8px_var(--app-glow)]"
                    : "bg-[var(--app-hover)] text-app-text-dim border border-app-border"
                }`}
            >
              {i < current ? <Check size={10} /> : i + 1}
            </span>
            <span
              className={`text-2xs font-mono transition-colors duration-300
                ${i === current ? "text-app-text" : i < current ? "text-app-text-dim" : "text-app-text-muted"}`}
            >
              {label}
            </span>
          </div>
        </React.Fragment>
      ))}
    </div>
  );
}

// ── OnboardingWizard ───────────────────────────────────────
export function OnboardingWizard({ hasProfiles, onScan, onCreate, onDismiss }: OnboardingWizardProps) {
  const [step, setStep] = useState(0);
  const [envCheck, setEnvCheck] = useState<EnvCheckResult | null>(null);
  const [checking, setChecking] = useState(false);

  const runEnvCheck = useCallback(async () => {
    setChecking(true);
    try {
      const result: EnvCheckResult = await invoke("check_environment");
      setEnvCheck(result);
    } catch {
      // Silently fail — we'll show unknown state
      setEnvCheck(null);
    } finally {
      setChecking(false);
    }
  }, []);

  // Run env check on mount
  useEffect(() => {
    runEnvCheck();
  }, [runEnvCheck]);

  return (
    <div className="flex-1 flex items-center justify-center bg-app-bg p-6">
      <div className="flex flex-col items-center max-w-[520px] w-full">
        {/* Hero */}
        <div className="flex flex-col items-center mb-2">
          <div className="w-14 h-14 rounded-full bg-[var(--app-selected)] flex items-center justify-center mb-4 border border-app-border">
            <Terminal size={28} className="text-app-accent" />
          </div>
          <h2 className="text-xl font-semibold text-app-text font-mono tracking-tight">
            <span className="text-app-accent opacity-60">$ </span>
            AI Profile Manager
          </h2>
          <p className="text-sm text-app-text-dim mt-1 leading-relaxed text-center">
            管理多个 AI CLI 工具的 API 配置，一键切换服务商
          </p>
        </div>

        {/* Step dots */}
        <StepDots current={step} />

        {/* ── Step 0: Environment check ──────────────────── */}
        {step === 0 && (
          <div className="w-full border border-app-border bg-[var(--app-cmd-bg)] animate-[fadeIn_150ms_ease-out]">
            <div className="px-4 py-2.5 border-b border-app-border bg-[var(--app-cmd-header)]">
              <span className="text-2xs text-app-text-muted uppercase tracking-wider font-mono">
                系统环境检测
              </span>
            </div>

            <div className="px-4 py-3 space-y-2">
              {checking ? (
                <div className="flex items-center gap-2 py-3">
                  <div className="w-4 h-4 border-2 border-app-accent border-t-transparent rounded-full animate-spin" />
                  <span className="text-sm text-app-text-muted font-mono">正在检测...</span>
                </div>
              ) : envCheck ? (
                envCheck.items.map((item) => (
                  <div key={item.name} className="flex items-center gap-2.5">
                    <StatusIconForItem item={item} />
                    <span className="text-sm text-app-text font-mono flex-1">{item.label}</span>
                    <span className={`text-2xs font-mono max-w-[220px] truncate text-right
                      ${itemSeverity(item) === "ok" ? "text-app-text-muted"
                        : itemSeverity(item) === "warn" ? "text-app-amber"
                        : itemSeverity(item) === "error" ? "text-app-red" : "text-app-text-dim"}`}
                    >
                      {item.detail}
                    </span>
                  </div>
                ))
              ) : (
                <div className="text-xs text-app-text-muted font-mono py-1">
                  无法检测环境，你可以跳过此步骤继续设置。
                </div>
              )}
            </div>

            {!envCheck?.all_ok && !checking && (
              <div className="px-4 py-2 border-t border-app-border bg-[var(--app-subtle)]">
                <div className="text-2xs text-app-text-muted font-mono leading-relaxed">
                  {envCheck?.items.some(i => i.category === "cli" && i.status !== "ok") &&
                    "💡 提示：缺少的 CLI 工具可在顶部系统诊断中选择安装方式。"}
                  {envCheck?.items.find(i => i.name === "shell-wrapper")?.status !== "ok" &&
                    " Shell 集成会在应用启动时自动尝试写入。"}
                </div>
              </div>
            )}
          </div>
        )}

        {/* ── Step 1: Import or Create ───────────────────── */}
        {step === 1 && (
          <div className="w-full animate-[fadeIn_150ms_ease-out] space-y-3">
            {/* Scan existing configs */}
            <button
              onClick={onScan}
              className="w-full text-left flex items-start gap-4 px-5 py-4 border
                bg-[var(--app-cmd-bg)] hover:bg-[var(--app-selected)]
                transition-colors duration-fast group relative overflow-hidden
                onboarding-scan-btn"
            >
              {/* Tip badge — top right, pulses to catch first-time user attention */}
              <span className="absolute -top-0 -right-0 px-2 py-0.5 text-[10px] font-mono font-bold
                bg-app-accent text-[var(--app-bg)] onboarding-tip-badge">
                推荐
              </span>
              <div className="w-10 h-10 rounded-full bg-[var(--app-input)] border border-app-border
                flex items-center justify-center shrink-0 mt-0.5
                group-hover:border-app-accent group-hover:bg-[var(--app-selected)] transition-colors"
              >
                <Search size={18} className="text-app-accent" />
              </div>
              <div>
                <div className="text-sm font-semibold text-app-text font-mono mb-0.5">
                  <span className="text-app-accent opacity-70">$ </span>
                  扫描现有配置
                </div>
                <div className="text-xs text-app-text-dim leading-relaxed">
                  自动检测 ~/.claude/settings.json 和 ~/.codex/ 中的 API 配置，
                  一键导入为 profile
                </div>
              </div>
            </button>

            {/* Create new */}
            <button
              onClick={onCreate}
              className="w-full text-left flex items-start gap-4 px-5 py-4 border border-app-border
                bg-[var(--app-cmd-bg)] hover:border-app-accent hover:bg-[var(--app-selected)]
                transition-all duration-fast group"
            >
              <div className="w-10 h-10 rounded-full bg-[var(--app-input)] border border-app-border
                flex items-center justify-center shrink-0 mt-0.5
                group-hover:border-app-accent group-hover:bg-[var(--app-selected)] transition-colors"
              >
                <Plus size={18} className="text-app-amber" />
              </div>
              <div>
                <div className="text-sm font-semibold text-app-text font-mono mb-0.5">
                  <span className="text-app-amber opacity-70">$ </span>
                  手动创建 Profile
                </div>
                <div className="text-xs text-app-text-dim leading-relaxed">
                  填写 API 密钥、Base URL 和模型名称，创建自定义 profile。
                  支持任何兼容 Anthropic 或 OpenAI 协议的服务商
                </div>
              </div>
            </button>
          </div>
        )}

        {/* ── Step 2: Ready ────────────────────────────────── */}
        {step === 2 && (
          <div className="w-full text-center animate-[fadeIn_150ms_ease-out]">
            <div className="w-12 h-12 rounded-full bg-app-green-bg border border-app-border
              flex items-center justify-center mx-auto mb-3"
            >
              <Check size={24} className="text-app-green" />
            </div>
            <h3 className="text-base font-semibold text-app-text font-mono mb-1">
              准备就绪
            </h3>
            <p className="text-sm text-app-text-dim mb-4">
              现在可以开始使用了
            </p>

            <div className="text-left border border-app-border bg-[var(--app-cmd-bg)] w-full mb-4">
              <div className="px-3 py-2 border-b border-app-border bg-[var(--app-cmd-header)]">
                <span className="text-2xs text-app-text-muted uppercase tracking-wider font-mono">
                  快速开始
                </span>
              </div>
              <div className="px-4 py-3 space-y-3 font-mono text-sm">
                <div className="flex items-start gap-3">
                  <span className="text-app-accent font-bold text-xs w-5 text-right shrink-0 mt-0.5">1</span>
                  <div>
                    <div className="text-app-text">从侧边栏选择一个 profile</div>
                    <div className="text-xs text-app-text-muted mt-0.5">或按 <kbd className="text-app-amber">{formatShortcut("mod+N")}</kbd> 新建</div>
                  </div>
                </div>
                <div className="flex items-start gap-3">
                  <span className="text-app-accent font-bold text-xs w-5 text-right shrink-0 mt-0.5">2</span>
                  <div>
                    <div className="text-app-text">点击 <span className="inline-flex items-center gap-1 text-app-green"><Play size={10} />运行</span> 按钮</div>
                    <div className="text-xs text-app-text-muted mt-0.5">选择项目目录后，终端将自动执行 ai claude 命令</div>
                  </div>
                </div>
                <div className="flex items-start gap-3">
                  <span className="text-app-accent font-bold text-xs w-5 text-right shrink-0 mt-0.5">3</span>
                  <div>
                    <div className="text-app-text">在内置终端中使用 AI CLI 工具</div>
                    <div className="text-xs text-app-text-muted mt-0.5">会话结束后环境变量自动清除，不影响其他终端</div>
                  </div>
                </div>
              </div>
            </div>

            {/* Shortcut keys quick reference */}
            <div className="text-left border border-app-border bg-[var(--app-cmd-bg)] w-full">
              <div className="px-3 py-2 border-b border-app-border bg-[var(--app-cmd-header)]">
                <span className="text-2xs text-app-text-muted uppercase tracking-wider font-mono">
                  常用快捷键
                </span>
              </div>
              <div className="px-4 py-2 space-y-1 font-mono">
                {[
                  [formatShortcut("mod+N"), "新建 Profile"],
                  [formatShortcut("mod+B"), "切换侧边栏"],
                  ["Ctrl+`", "打开终端面板"],
                  [formatShortcut("mod+K"), "查看全部快捷键"],
                  ["Esc", "关闭弹窗 / 取消选中"],
                ].map(([key, desc]) => (
                  <div key={desc} className="flex items-center justify-between text-xs">
                    <span className="text-app-text-dim">{desc}</span>
                    <kbd className="px-1.5 py-0.5 text-2xs bg-[var(--app-input)] border border-app-border text-app-text">{key}</kbd>
                  </div>
                ))}
              </div>
            </div>
          </div>
        )}

        {/* ── Footer buttons ────────────────────────────────── */}
        <div className="flex items-center justify-between w-full mt-6">
          <div>
            {step > 0 && (
              <Button variant="ghost" size="sm" onClick={() => setStep(step - 1)}>
                <ChevronLeft size={13} />上一步
              </Button>
            )}
            {step === 0 && onDismiss && (
              <Button variant="ghost" size="sm" onClick={onDismiss}>
                跳过
              </Button>
            )}
          </div>
          <div className="flex gap-2">
            {step < 2 ? (
              <Button variant="primary" size="sm" onClick={() => {
                if (step === 1 && hasProfiles) {
                  // If profiles already exist (from scan), go straight to done
                  setStep(2);
                } else {
                  setStep(step + 1);
                }
              }}>
                下一步<ChevronRight size={13} />
              </Button>
            ) : (
              <Button variant="primary" size="sm" onClick={onDismiss}>
                <Terminal size={13} />开始使用
              </Button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
