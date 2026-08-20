import React from "react";
import { X, Keyboard } from "lucide-react";
import { isMac } from "../utils/shortcut";
import { getShortcutGroups } from "../lib/shortcut-definitions";
import { ShortcutHelp } from "./common/ShortcutHelp";

export function ShortcutsPanel({ onClose }: { onClose: () => void }) {
  const groups = getShortcutGroups();

  return (
    <div
      className="fixed inset-0 z-[120] flex items-center justify-center app-dialog-backdrop animate-[fadeIn_100ms_ease-out]"
      onClick={(e) => e.target === e.currentTarget && onClose()}
    >
      <div className="app-dialog-panel bg-app-bg border border-app-border w-[720px] max-w-[calc(100vw-3rem)] animate-[scaleIn_150ms_ease-out]">
        <div className="flex items-center justify-between px-4 py-3 border-b border-app-border">
          <div className="flex items-center gap-2">
            <Keyboard size={15} className="text-app-accent" />
            <h3 className="font-semibold text-sm font-mono">KN 快捷键</h3>
            <span className="text-2xs text-app-text-muted font-mono">— {isMac() ? "macOS" : "Windows/Linux"}</span>
          </div>
          <button onClick={onClose} className="p-1 text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)] transition-colors">
            <X size={14} />
          </button>
        </div>
        <div className="p-4 max-h-[520px] overflow-y-auto">
          <ShortcutHelp groups={groups} />
        </div>
        <div className="px-4 py-2 border-t border-app-border bg-[var(--app-subtle)] text-2xs text-app-text-muted font-mono text-center">
          按 Esc 或点击外部关闭
        </div>
      </div>
    </div>
  );
}
