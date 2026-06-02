import React from "react";
import { X, Keyboard } from "lucide-react";

interface ShortcutItem { keys: string[]; desc: string; }

const shortcuts: ShortcutItem[] = [
  { keys: ["⌘", "N"], desc: "新建 Profile" },
  { keys: ["Esc"], desc: "关闭弹窗 / 取消选中" },
  { keys: ["⌘", "F"], desc: "搜索 Profile" },
  { keys: ["Ctrl", "`"], desc: "开关终端面板" },
  { keys: ["Ctrl", "K"], desc: "快捷键帮助" },
  { keys: ["Backspace"], desc: "删除选中的 Profile" },
  { keys: ["↑", "↓"], desc: "终端中浏览历史命令" },
  { keys: ["Ctrl", "L"], desc: "清屏终端" },
  { keys: ["Ctrl", "C"], desc: "终端中终止进程" },
];

export function ShortcutsPanel({ onClose }: { onClose: () => void }) {
  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm animate-[fadeIn_100ms_ease-out]"
      onClick={(e) => e.target === e.currentTarget && onClose()}
    >
      <div className="bg-app-panel border border-app-border shadow-dialog w-[440px] animate-[scaleIn_150ms_ease-out]">
        <div className="flex items-center justify-between px-4 py-3 border-b border-app-border">
          <div className="flex items-center gap-2">
            <Keyboard size={15} className="text-app-accent" />
            <h3 className="font-semibold text-sm font-mono">快捷键</h3>
          </div>
          <button onClick={onClose} className="p-1 text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)] transition-colors">
            <X size={14} />
          </button>
        </div>
        <div className="p-3 space-y-0.5 max-h-[360px] overflow-y-auto">
          {shortcuts.map((s) => (
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
        <div className="px-4 py-2 border-t border-app-border bg-[var(--app-subtle)] text-2xs text-app-text-muted font-mono text-center">
          按 Esc 或点击外部关闭
        </div>
      </div>
    </div>
  );
}
