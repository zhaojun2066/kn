import React, { useState, useRef, useEffect, useCallback } from "react";
import {
  Plus, Star, Download,
  Sun, Moon, Monitor, Copy, HelpCircle, RefreshCw, RotateCw, Search, ChevronDown, Settings,
  PanelLeft, PanelBottom, PanelRight, Save, History, Circle, Info, Palette, Check, Terminal,
} from "lucide-react";
import { formatShortcut } from "../utils/shortcut";
import { Button } from "./common/Button";
import { useTheme, ThemeMode, COLOR_SCHEMES } from "../hooks/useTheme";

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
  envCheck: { items: { name: string; label: string; status: string; detail: string; install_cmd?: string }[]; all_ok: boolean } | null;
  onInstallTool?: (cmd: string) => void;
  onRefreshEnvCheck?: () => void;
  sidebarVisible: boolean;
  onToggleSidebar: () => void;
  terminalVisible: boolean;
  rightTerminalVisible: boolean;
  onToggleRightTerminal: () => void;
  onAbout: () => void;
  onSettings: () => void;
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
  onCopyProfile, hasSelection, onCheckUpdate, onBackup, onRestore, backupExists, envCheck, onInstallTool, onRefreshEnvCheck,
  sidebarVisible, onToggleSidebar, terminalVisible, rightTerminalVisible, onToggleRightTerminal,
  onAbout, onSettings,
}: ToolbarProps) {
  const { mode, colorScheme, setColorScheme, setTheme } = useTheme();
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
          title={`${sidebarVisible ? "隐藏侧边栏" : "显示侧边栏"} (${formatShortcut("mod+B")})`}
        >
          <PanelLeft size={14} />
        </button>
        <button
          onClick={onToggleTerminal}
          className={`p-1 transition-colors duration-fast rounded ${terminalVisible ? "text-app-accent" : "text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)]"}`}
          title={`${terminalVisible ? "隐藏终端面板" : "显示终端面板"} (${formatShortcut("mod+J")})`}
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

      {/* ══ Env Health Indicator — click-to-toggle diagnostic panel ══ */}
      {envCheck && (
        <EnvPanel
          envCheck={envCheck}
          onInstallTool={onInstallTool}
          onOpen={onRefreshEnvCheck}
        />
      )}

      {/* Color scheme picker — single icon + dropdown */}
      <DropMenu items={COLOR_SCHEMES.map((s) => ({
        label: s.label,
        icon: <span className="w-[11px] h-[11px] shrink-0 inline-block" style={{ backgroundColor: s.color }} />,
        onClick: () => setColorScheme(s.id),
        hint: colorScheme === s.id ? "✓" : "",
      }))}>
        <Palette size={13} />
        <span className="hidden sm:inline text-app-text-muted">配色</span>
      </DropMenu>

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
        { label: "快捷键", icon: <HelpCircle size={13} />, onClick: onToggleWelcome, hint: formatShortcut("mod+K") },
        { label: "设置", icon: <Settings size={13} />, onClick: onSettings },
        { label: "关于", icon: <Info size={13} />, onClick: onAbout },
      ]}>
        <Settings size={13} />
      </DropMenu>
    </div>
  );
}

/* ═══════════════════════════════════════════════════════════════
   EnvPanel — System diagnostic readout
   Click-to-toggle. Staggered row reveals. Terminal precision.
   ═══════════════════════════════════════════════════════════════ */

interface EnvPanelProps {
  envCheck: { items: { name: string; label: string; status: string; detail: string; install_cmd?: string }[]; all_ok: boolean };
  onInstallTool?: (cmd: string) => void;
  onOpen?: () => void;
}

function EnvPanel({ envCheck, onInstallTool, onOpen }: EnvPanelProps) {
  const [open, setOpen] = useState(false);
  const [visible, setVisible] = useState(false);    // stagger delay for animation
  const panelRef = useRef<HTMLDivElement>(null);
  const dotRef = useRef<HTMLButtonElement>(null);
  const hasMissing = envCheck.items.some((i) => i.status === "missing");

  // Click outside → close
  useEffect(() => {
    if (!open) return;
    const h = (e: MouseEvent) => {
      const target = e.target as Node;
      if (panelRef.current?.contains(target)) return;
      if (dotRef.current?.contains(target)) return;
      setVisible(false);
      setTimeout(() => setOpen(false), 180);
    };
    const k = (e: KeyboardEvent) => { if (e.key === "Escape") { setVisible(false); setTimeout(() => setOpen(false), 180); } };
    document.addEventListener("mousedown", h);
    document.addEventListener("keydown", k);
    return () => { document.removeEventListener("mousedown", h); document.removeEventListener("keydown", k); };
  }, [open]);

  const toggle = useCallback(() => {
    if (open) {
      setVisible(false);
      setTimeout(() => setOpen(false), 180);
    } else {
      onOpen?.(); // refresh env check status
      setOpen(true);
      requestAnimationFrame(() => requestAnimationFrame(() => setVisible(true)));
    }
  }, [open, onOpen]);

  return (
    <div className="relative flex items-center">
      {/* Trigger dot */}
      <button
        ref={dotRef}
        onClick={toggle}
        className="relative group/dot p-1 -m-1 rounded hover:bg-[var(--app-hover)] transition-colors duration-200"
        title="查看环境状态"
      >
        {/* Outer glow ring */}
        <span
          className={`absolute inset-0 rounded-full opacity-0 group-hover/dot:opacity-20 transition-opacity duration-300 ${
            envCheck.all_ok ? "bg-app-green" : hasMissing ? "bg-app-red" : "bg-app-amber"
          }`}
          style={{ filter: "blur(6px)" }}
        />
        {/* Core dot */}
        <Circle
          size={8}
          className={`relative shrink-0 transition-all duration-500 ${
            envCheck.all_ok
              ? "fill-app-green text-app-green drop-shadow-[0_0_5px_var(--app-green)]"
              : hasMissing
                ? "fill-app-red text-app-red drop-shadow-[0_0_5px_var(--app-red)] animate-pulse"
                : "fill-app-amber text-app-amber drop-shadow-[0_0_4px_var(--app-amber)]"
          }`}
        />
      </button>

      {/* Panel — conditioned on state, not CSS hover */}
      {open && (
        <div
          ref={panelRef}
          className={`absolute top-full right-0 mt-2 z-50
            w-[320px] bg-app-panel border border-app-border
            transition-all duration-150 ease-out origin-top-right
            ${visible ? "opacity-100 scale-100 translate-y-0" : "opacity-0 scale-95 -translate-y-1 pointer-events-none"}`}
        >
          {/* ── Header ─────────────────────────────────── */}
          <div className="flex items-center justify-between px-3 py-2 border-b border-app-border-light bg-[var(--app-subtle)]">
            <div className="flex items-center gap-1.5">
              <Terminal size={10} className="text-app-text-muted" />
              <span className="text-2xs font-mono text-app-text-dim tracking-wider uppercase">
                系统诊断
              </span>
            </div>
            <span
              className={`text-2xs font-mono ${
                envCheck.all_ok ? "text-app-green" : hasMissing ? "text-app-red" : "text-app-amber"
              }`}
            >
              {envCheck.all_ok ? "ALL CLEAR" : hasMissing ? `${envCheck.items.filter(i=>i.status==="missing").length} MISSING` : "WARNING"}
            </span>
          </div>

          {/* ── Diagnostic rows ─────────────────────────── */}
          <div className="px-3 py-1.5 space-y-0">
            {envCheck.items.map((item, idx) => {
              const isMissing = item.status === "missing";
              const canInstall = isMissing && item.install_cmd && onInstallTool;
              return (
                <div
                  key={item.name}
                  className="flex items-center gap-2 py-1 text-2xs font-mono transition-all duration-200"
                  style={{
                    opacity: visible ? 1 : 0,
                    transform: visible ? "translateX(0)" : "translateX(-6px)",
                    transitionDelay: visible ? `${80 + idx * 40}ms` : "0ms",
                    transitionProperty: "opacity, transform",
                  }}
                >
                  {/* Status glyph */}
                  <span
                    className={`shrink-0 w-4 text-center ${
                      item.status === "ok"
                        ? "text-app-green"
                        : item.status === "warn"
                          ? "text-app-amber"
                          : "text-app-red"
                    }`}
                  >
                    {item.status === "ok" ? "●" : item.status === "warn" ? "◐" : "○"}
                  </span>

                  {/* Label */}
                  <span className="text-app-text-dim w-[75px] shrink-0 truncate">
                    {item.label}
                  </span>

                  {/* Detail / Install */}
                  <span className="flex-1 text-right text-app-text-muted truncate min-w-0">
                    {item.detail}
                  </span>

                  {canInstall && (
                    <button
                      onClick={() => onInstallTool!(item.install_cmd!)}
                      className={`shrink-0 flex items-center gap-1 px-1.5 py-0.5 text-2xs font-mono
                        transition-all duration-200
                        ${isMissing
                          ? "text-app-accent border border-app-accent/40 hover:bg-app-accent hover:text-[var(--app-bg)] hover:border-app-accent"
                          : "text-app-text-muted border border-transparent"
                        }`}
                    >
                      <Terminal size={9} />
                      安装
                    </button>
                  )}
                </div>
              );
            })}
          </div>

          {/* ── Decorative scan line ────────────────────── */}
          <div
            className={`h-[2px] transition-colors duration-500 ${
              envCheck.all_ok
                ? "bg-app-green/40"
                : hasMissing
                  ? "bg-app-red/30"
                  : "bg-app-amber/30"
            }`}
          />
        </div>
      )}
    </div>
  );
}
