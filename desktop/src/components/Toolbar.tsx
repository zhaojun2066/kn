import React, { useState, useRef, useEffect } from "react";
import {
  Plus, Trash2, Star, Download, Upload,
  Sun, Moon, Monitor, Terminal, Copy, HelpCircle, RefreshCw, RotateCw, Search, ChevronDown, Settings,
} from "lucide-react";
import { Button } from "./common/Button";
import { useTheme, ThemeMode } from "../hooks/useTheme";

interface ToolbarProps {
  selectedName: string | null;
  isDefault: boolean;
  onAdd: () => void;
  onRemove: (name: string) => void;
  onSetDefault: (name: string) => void;
  onInit: () => void;
  onToggleTerminal: () => void;
  onToggleWelcome: () => void;
  onRefresh: () => void;
  onExport: () => void;
  onImport: () => void;
  onCopyProfile: () => void;
  hasSelection: boolean;
  onCheckUpdate: () => void;
}

const themeIcons: Record<ThemeMode, React.ReactNode> = {
  light: <Sun size={13} />, dark: <Moon size={13} />, system: <Monitor size={13} />,
};
const themeNext: Record<ThemeMode, ThemeMode> = { light: "dark", dark: "system", system: "light" };
const themeLabel: Record<ThemeMode, string> = { light: "浅色", dark: "深色", system: "自动" };

function DropMenu({ items, children }: { items: { label: string; icon?: React.ReactNode; onClick: () => void; hint?: string }[]; children: React.ReactNode }) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const h = (e: MouseEvent) => { if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false); };
    const k = (e: KeyboardEvent) => { if (e.key === "Escape") setOpen(false); };
    document.addEventListener("mousedown", h); document.addEventListener("keydown", k);
    return () => { document.removeEventListener("mousedown", h); document.removeEventListener("keydown", k); };
  }, [open]);

  return (
    <div ref={ref} className="relative">
      <button
        onClick={() => setOpen(!open)}
        className={`flex items-center gap-0.5 px-2 h-[24px] text-xs font-mono transition-colors duration-fast whitespace-nowrap
          ${open ? "text-app-accent bg-[var(--app-hover)]" : "text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)]"}`}
      >
        {children}
        <ChevronDown size={10} className={open ? "rotate-180" : ""} />
      </button>
      {open && (
        <div className="absolute top-full right-0 mt-1 z-50 min-w-[150px] bg-app-panel border border-app-border shadow-dialog py-0.5 whitespace-nowrap">
          {items.map((item, i) => (
            <button
              key={i}
              onClick={() => { item.onClick(); setOpen(false); }}
              className="w-full flex items-center gap-2 px-3 py-1.5 text-sm font-mono text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)] transition-colors whitespace-nowrap"
            >
              {item.icon && <span className="shrink-0">{item.icon}</span>}
              <span className="flex-1 text-left">{item.label}</span>
              {item.hint && <span className="text-2xs text-app-text-muted shrink-0">{item.hint}</span>}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

export function Toolbar({
  selectedName, isDefault, onAdd, onRemove, onSetDefault,
  onInit, onToggleTerminal, onToggleWelcome, onRefresh, onExport, onImport,
  onCopyProfile, hasSelection, onCheckUpdate,
}: ToolbarProps) {
  const { mode, setTheme } = useTheme();
  const cycleTheme = () => setTheme(themeNext[mode]);

  return (
    <div className="flex items-center gap-1.5 h-[38px] px-3 bg-app-toolbar border-b border-app-border select-none shrink-0 overflow-visible">
      {/* ── Profile actions ────────────────────────── */}
      <Button variant="primary" size="sm" onClick={onAdd}><Plus size={13} /><span>新增</span></Button>
      <Button variant="secondary" size="sm" disabled={!selectedName}
        onClick={() => selectedName && onRemove(selectedName)}><Trash2 size={13} /><span>删除</span></Button>
      <Button variant="secondary" size="sm" disabled={!selectedName || isDefault}
        onClick={() => selectedName && onSetDefault(selectedName)} title="设为默认"><Star size={13} /><span>默认</span></Button>
      <Button variant="secondary" size="sm" disabled={!hasSelection}
        onClick={onCopyProfile} title="复制"><Copy size={13} /><span>复制</span></Button>

      <div className="w-px h-5 bg-app-border mx-1" />

      {/* ── Import dropdown ────────────────────────── */}
      <DropMenu items={[
        { label: "扫描系统配置", icon: <Search size={13} />, onClick: onInit, hint: "Claude/Codex" },
        { label: "从文件导入", icon: <Download size={13} />, onClick: onImport, hint: "JSON" },
      ]}>
        <Download size={13} />
        <span>导入</span>
      </DropMenu>

      <Button variant="ghost" size="sm" disabled={!hasSelection} onClick={onExport} title="导出">
        <Upload size={13} /><span>导出</span>
      </Button>

      {/* Spacer */}
      <div className="flex-1" />

      {/* ── Right side ─────────────────────────────── */}
      <Button variant="ghost" size="sm" onClick={onToggleTerminal} title="终端 (Ctrl+`)">
        <Terminal size={13} />
        <span className="hidden xl:inline text-app-text-muted">终端</span>
      </Button>

      {/* Theme toggle */}
      <Button variant="ghost" size="sm" onClick={cycleTheme} title={`主题: ${themeLabel[mode]}`}>
        {themeIcons[mode]}
        <span className="hidden xl:inline text-app-text-muted">{themeLabel[mode]}</span>
      </Button>

      {/* Gear menu */}
      <DropMenu items={[
        { label: "刷新配置", icon: <RefreshCw size={13} />, onClick: onRefresh },
        { label: "检查更新", icon: <RotateCw size={13} />, onClick: onCheckUpdate },
        { label: "快捷键", icon: <HelpCircle size={13} />, onClick: onToggleWelcome, hint: "Cmd+K" },
      ]}>
        <Settings size={13} />
      </DropMenu>
    </div>
  );
}
