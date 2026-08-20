import { KeyRound, LogIn, Search, Terminal, X } from "lucide-react";
import type React from "react";

interface HelpDialogProps {
  open: boolean;
  onClose: () => void;
  onOpenProfiles?: () => void;
}

export function HelpDialog({ open, onClose, onOpenProfiles }: HelpDialogProps) {
  if (!open) return null;
  return (
    <div className="fixed inset-0 z-[120] flex items-center justify-center app-dialog-backdrop animate-[fadeIn_100ms_ease-out]">
      <div className="app-dialog-panel bg-app-panel border border-app-border w-[520px] animate-[scaleIn_150ms_ease-out]">
        <div className="flex items-center justify-between px-4 py-3 border-b border-app-border">
          <div className="flex items-center gap-2">
            <Terminal size={15} className="text-app-accent" />
            <h3 className="font-semibold text-sm">使用帮助</h3>
          </div>
          <button onClick={onClose} className="p-1 text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)] transition-colors">
            <X size={14} />
          </button>
        </div>
        <div className="px-4 py-4 space-y-3 text-sm text-app-text-dim leading-relaxed">
          <p>
            kn 是 AI CLI 的运行配置切换器。你可以为 Claude Code、Codex、QoderCN 保存多套配置，
            然后在项目里选择一套配置启动对应 CLI。
          </p>
          <div className="grid grid-cols-1 gap-2">
            <HelpItem icon={<KeyRound size={15} />} title="API Key / 中转站">
              kn 会把 key、Base URL、模型等参数注入当前会话。Codex 的 API Key 配置会临时接管 auth.json，退出后恢复。
            </HelpItem>
            <HelpItem icon={<LogIn size={15} />} title="账号登录">
              复用本机 CLI 自己保存的登录态。kn 不读取账号 token，也不提前判断是否已登录，启动后由 CLI 自己验证。
            </HelpItem>
            <HelpItem icon={<Search size={15} />} title="自动扫描">
              只导入能明确表达为运行配置的凭据。Codex/QoderCN 的账号登录不会被自动判断或自动创建。
            </HelpItem>
          </div>
        </div>
        <div className="flex items-center justify-end gap-2 px-4 py-3 border-t border-app-border bg-[var(--app-subtle)]">
          {onOpenProfiles && (
            <button
              onClick={() => { onClose(); onOpenProfiles(); }}
              className="app-primary-action h-8 px-3 text-xs font-medium"
            >
              打开运行配置
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

function HelpItem({ icon, title, children }: { icon: React.ReactNode; title: string; children: React.ReactNode }) {
  return (
    <div className="flex gap-3 px-3 py-2.5 border border-app-border bg-[var(--app-subtle)]">
      <div className="mt-0.5 text-app-accent shrink-0">{icon}</div>
      <div>
        <div className="text-sm font-semibold text-app-text mb-0.5">{title}</div>
        <div className="text-xs text-app-text-dim leading-relaxed">{children}</div>
      </div>
    </div>
  );
}
