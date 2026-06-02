import React, { useState, useRef, useEffect } from "react";
import {
  Plus, Star, Download,
  Sun, Moon, Monitor, Copy, HelpCircle, RefreshCw, RotateCw, Search, ChevronDown, Settings,
  PanelLeft, PanelBottom, PanelRight, Save, History, Circle, Info,
} from "lucide-react";
import { Button } from "./common/Button";
import { useTheme, ThemeMode } from "../hooks/useTheme";

interface ToolbarProps {
  selectedName: string | null;
  isDefault: boolean;
  onAdd: () => void;
  onSetDefault: (name: string) => void;
  onInit: () => void;
  onToggleTerminal: () => void;
  onToggleWelcome: () => void;
  onRefresh: () => void;
  onImport: () => void;
  onCopyProfile: () => void;
  hasSelection: boolean;
  onCheckUpdate: () => void;
  onBackup: () => void;
  onRestore: () => void;
  backupExists: boolean;
  envCheck: { items: { name: string; label: string; status: string; detail: string }[]; all_ok: boolean } | null;
  sidebarVisible: boolean;
  onToggleSidebar: () => void;
  terminalVisible: boolean;
  rightTerminalVisible: boolean;
  onToggleRightTerminal: () => void;
  onAbout: () => void;
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
  selectedName, isDefault, onAdd, onSetDefault,
  onInit, onToggleTerminal, onToggleWelcome, onRefresh, onImport,
  onCopyProfile, hasSelection, onCheckUpdate, onBackup, onRestore, backupExists, envCheck,
  sidebarVisible, onToggleSidebar, terminalVisible, rightTerminalVisible, onToggleRightTerminal,
  onAbout,
}: ToolbarProps) {
  const { mode, setTheme } = useTheme();
  const cycleTheme = () => setTheme(themeNext[mode]);

  return (
    <div className="flex items-center gap-1.5 h-[38px] px-3 bg-app-toolbar border-b border-app-border select-none shrink-0 overflow-visible">
      {/* ── Profile actions ────────────────────────── */}
      <Button variant="primary" size="sm" onClick={onAdd}><Plus size={13} /><span>新增</span></Button>
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

      {/* Spacer */}
      <div className="flex-1" />

      {/* ── Right side — layout controls (VS Code style) ── */}
      <div className="flex items-center gap-0.5 mr-1">
        <button
          onClick={onToggleSidebar}
          className={`p-1 transition-colors duration-fast rounded ${sidebarVisible ? "text-app-accent" : "text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)]"}`}
          title={sidebarVisible ? "隐藏侧边栏 (⌘B)" : "显示侧边栏 (⌘B)"}
        >
          <PanelLeft size={14} />
        </button>
        <button
          onClick={onToggleTerminal}
          className={`p-1 transition-colors duration-fast rounded ${terminalVisible ? "text-app-accent" : "text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)]"}`}
          title={terminalVisible ? "隐藏终端面板 (⌘J)" : "显示终端面板 (⌘J)"}
        >
          <PanelBottom size={14} />
        </button>
        <button
          onClick={onToggleRightTerminal}
          className={`p-1 transition-colors duration-fast rounded ${rightTerminalVisible ? "text-app-accent" : "text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)]"}`}
          title={rightTerminalVisible ? "隐藏右侧终端" : "显示右侧终端"}
        >
          <PanelRight size={14} />
        </button>
      </div>

      {/* Env health indicator */}
      {envCheck && (
        <div className="relative group flex items-center" title={
          envCheck.items.map(i => `${i.status === "ok" ? "✓" : i.status === "warn" ? "⚠" : "✗"} ${i.label}: ${i.detail}`).join("\n")
        }>
          <Circle
            size={8}
            className={`shrink-0 transition-colors ${
              envCheck.all_ok
                ? "fill-app-green text-app-green shadow-[0_0_4px_var(--app-green-glow)]"
                : envCheck.items.some(i => i.status === "missing")
                  ? "fill-app-red text-app-red shadow-[0_0_4px_var(--app-red-glow)]"
                  : "fill-app-amber text-app-amber shadow-[0_0_4px_var(--app-amber-glow)]"
            }`}
          />
          {/* Tooltip on hover */}
          <div className="absolute top-full right-0 mt-1.5 w-[260px] bg-app-panel border border-app-border shadow-dialog
            py-1.5 px-3 opacity-0 pointer-events-none group-hover:opacity-100 group-hover:pointer-events-auto
            transition-opacity duration-100 z-50">
            {envCheck.items.map((item) => (
              <div key={item.name} className="flex items-center gap-1.5 py-0.5 text-2xs font-mono">
                <span className={
                  item.status === "ok" ? "text-app-green" : item.status === "warn" ? "text-app-amber" : "text-app-red"
                }>
                  {item.status === "ok" ? "✓" : item.status === "warn" ? "⚠" : "✗"}
                </span>
                <span className="text-app-text-dim">{item.label}</span>
                <span className="flex-1 text-right text-app-text-muted truncate max-w-[150px]">{item.detail}</span>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Theme toggle */}
      <Button variant="ghost" size="sm" onClick={cycleTheme} title={`主题: ${themeLabel[mode]}`}>
        {themeIcons[mode]}
        <span className="hidden xl:inline text-app-text-muted">{themeLabel[mode]}</span>
      </Button>

      {/* Gear menu */}
      <DropMenu items={[
        { label: "刷新配置", icon: <RefreshCw size={13} />, onClick: onRefresh },
        { label: "备份配置", icon: <Save size={13} />, onClick: onBackup, hint: "手动备份" },
        { label: "恢复配置", icon: <History size={13} />, onClick: onRestore, hint: backupExists ? "可用" : "无备份" },
        { label: "检查更新", icon: <RotateCw size={13} />, onClick: onCheckUpdate },
        { label: "快捷键", icon: <HelpCircle size={13} />, onClick: onToggleWelcome, hint: "Cmd+K" },
        { label: "关于", icon: <Info size={13} />, onClick: onAbout },
      ]}>
        <Settings size={13} />
      </DropMenu>
    </div>
  );
}
