import React from "react";
import { X, Keyboard, Monitor, Terminal } from "lucide-react";
import { modKey, isMac } from "../utils/shortcut";

interface ShortcutItem { keys: string[]; desc: string; }

const mod = modKey();

const appShortcuts: ShortcutItem[] = [
  { keys: [mod, "N"], desc: "新建 Profile" },
  { keys: [mod, "B"], desc: "切换侧边栏" },
  { keys: ["Ctrl", "`"], desc: "切换底部终端" },
  { keys: [mod, "J"], desc: "切换底部终端（备选）" },
  { keys: [mod, "K"], desc: "快捷键帮助" },
  { keys: ["Esc"], desc: "关闭弹窗 / 取消选中" },
  { keys: ["Backspace"], desc: "删除选中的 Profile" },
];

const terminalShortcuts: ShortcutItem[] = [
  { keys: [mod, "F"], desc: "搜索终端输出" },
  { keys: [mod, "⇧", "M"], desc: "最大化 / 还原终端面板" },
  { keys: ["↑", "↓"], desc: "浏览历史命令" },
  { keys: ["Ctrl", "L"], desc: "清屏" },
  { keys: ["Ctrl", "C"], desc: "终止当前进程" },
];

function ShortcutSection({ title, icon, items }: { title: string; icon: React.ReactNode; items: ShortcutItem[] }) {
  return (
    <div className="mb-3">
      <div className="flex items-center gap-1.5 px-2 py-1 mb-1">
        {icon}
        <span className="text-2xs text-app-text-muted uppercase tracking-[0.2em] font-mono">{title}</span>
      </div>
      <div className="space-y-0.5">
        {items.map((s) => (
          <div key={s.desc} className="flex items-center justify-between px-2 py-1.5 hover:bg-[var(--app-hover)] transition-colors">
            <span className="text-sm text-app-text-dim font-mono">{s.desc}</span>
            <div className="flex items-center gap-1">
              {s.keys.map((k, i) => (
                <React.Fragment key={i}>
                  <kbd className="px-1.5 py-0.5 text-2xs bg-[var(--app-input)] border border-app-border font-mono text-app-text">
                    {k}
                  </kbd>
                  {i < s.keys.length - 1 && <span className="text-app-text-muted text-2xs">+</span>}
                </React.Fragment>
              ))}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

export function ShortcutsPanel({ onClose }: { onClose: () => void }) {
  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm animate-[fadeIn_100ms_ease-out]"
      onClick={(e) => e.target === e.currentTarget && onClose()}
    >
      <div className="bg-app-panel border border-app-border shadow-dialog w-[460px] animate-[scaleIn_150ms_ease-out]">
        <div className="flex items-center justify-between px-4 py-3 border-b border-app-border">
          <div className="flex items-center gap-2">
            <Keyboard size={15} className="text-app-accent" />
            <h3 className="font-semibold text-sm font-mono">快捷键</h3>
            <span className="text-2xs text-app-text-muted font-mono">— {isMac() ? "macOS" : "Windows/Linux"}</span>
          </div>
          <button onClick={onClose} className="p-1 text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)] transition-colors">
            <X size={14} />
          </button>
        </div>
        <div className="p-3 max-h-[380px] overflow-y-auto">
          <ShortcutSection title="应用" icon={<Monitor size={11} className="text-app-text-muted" />} items={appShortcuts} />
          <ShortcutSection title="终端" icon={<Terminal size={11} className="text-app-text-muted" />} items={terminalShortcuts} />
        </div>
        <div className="px-4 py-2 border-t border-app-border bg-[var(--app-subtle)] text-2xs text-app-text-muted font-mono text-center">
          按 Esc 或点击外部关闭
        </div>
      </div>
    </div>
  );
}
