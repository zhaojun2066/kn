import React, { useState, useRef, useEffect, useCallback } from "react";
import {
  Sun, Moon, Monitor, HelpCircle, RotateCw, ChevronDown, Settings,
  PanelLeft, PanelBottom, PanelRight, Circle, Info, Palette, Check, Terminal, Search, History,
  Copy,
} from "lucide-react";
import { formatShortcut } from "../utils/shortcut";
import { Button } from "./common/Button";
import { useTheme, ThemeMode, COLOR_SCHEMES } from "../hooks/useTheme";
import type { EnvCheckItem, EnvCheckResult, ProjectInfo } from "../lib/types";
import { itemSeverity } from "../lib/types";

interface ToolbarProps {
  onToggleTerminal: () => void;
  onToggleWelcome: () => void;
  onCheckUpdate: () => void;
  onAbout: () => void;
  onSettings: () => void;
  sidebarVisible: boolean;
  onToggleSidebar: () => void;
  terminalVisible: boolean;
  rightTerminalVisible: boolean;
  onToggleRightTerminal: () => void;
  envCheck: EnvCheckResult;
  onInstallTool?: (cmd: string) => void;
  onRefreshEnvCheck?: () => void;
  onQuickSwitcher: () => void;
  onQuickHistory: () => void;
  activeProject?: ProjectInfo | null;
  onOpenProfiles?: () => void;
  onOpenResources?: () => void;
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
  onToggleTerminal, onToggleWelcome,
  onCheckUpdate,
  onAbout, onSettings,
  sidebarVisible, onToggleSidebar,
  terminalVisible, rightTerminalVisible, onToggleRightTerminal,
  envCheck, onInstallTool, onRefreshEnvCheck,
  onQuickSwitcher, onQuickHistory,
  activeProject,
  onOpenProfiles,
  onOpenResources,
}: ToolbarProps) {
  const { mode, colorScheme, setColorScheme, setTheme } = useTheme();
  const cycleTheme = () => setTheme(themeNext[mode]);

  return (
    <div className="flex items-center gap-1.5 h-[38px] px-3 bg-app-toolbar border-b border-app-border select-none shrink-0 overflow-visible">
      <div className="flex items-center gap-1 min-w-0">
        <div className="max-w-[220px] truncate px-2 py-1 text-xs font-mono border border-app-border bg-app-panel text-app-text-dim">
          {activeProject ? activeProject.name : "未选择项目"}
        </div>
        {onOpenProfiles && (
          <button
            onClick={onOpenProfiles}
            className="px-2 py-1 text-xs font-mono text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)] transition-colors"
          >
            Profiles
          </button>
        )}
        {onOpenResources && (
          <button
            onClick={onOpenResources}
            className="px-2 py-1 text-xs font-mono text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)] transition-colors"
          >
            Resources
          </button>
        )}
      </div>

      <div className="flex-1" />

      {/* ── Right side — Quick Switcher + layout controls (VS Code style) ── */}
      <div className="flex items-center gap-0.5 mr-1">
        {/* Quick Switcher button */}
        <button
          onClick={onQuickSwitcher}
          aria-label="快速切换 profile"
          className="p-1 text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)] transition-colors duration-fast rounded"
          title={`快速切换 Profile (${formatShortcut("mod+P")})`}
        >
          <Search size={14} aria-hidden="true" />
        </button>
        {/* Quick History button */}
        <button
          onClick={onQuickHistory}
          aria-label="会话历史"
          className="p-1 text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)] transition-colors duration-fast rounded"
          title={`会话历史 (${formatShortcut("mod+⇧P")})`}
        >
          <History size={14} aria-hidden="true" />
        </button>
        <div className="w-px h-4 bg-app-border mx-0.5" />
        <button
          onClick={onToggleSidebar}
          aria-label={`${sidebarVisible ? "隐藏侧边栏" : "显示侧边栏"}`}
          className={`p-1 transition-colors duration-fast rounded ${sidebarVisible ? "text-app-accent" : "text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)]"}`}
          title={`${sidebarVisible ? "隐藏侧边栏" : "显示侧边栏"} (${formatShortcut("mod+B")})`}
        >
          <PanelLeft size={14} aria-hidden="true" />
        </button>
        <button
          onClick={onToggleTerminal}
          aria-label={`${terminalVisible ? "隐藏终端面板" : "显示终端面板"}`}
          className={`p-1 transition-colors duration-fast rounded ${terminalVisible ? "text-app-accent" : "text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)]"}`}
          title={`${terminalVisible ? "隐藏终端面板" : "显示终端面板"} (${formatShortcut("mod+J")})`}
        >
          <PanelBottom size={14} aria-hidden="true" />
        </button>
        <button
          onClick={onToggleRightTerminal}
          aria-label={rightTerminalVisible ? "隐藏右侧终端" : "显示右侧终端"}
          className={`p-1 transition-colors duration-fast rounded ${rightTerminalVisible ? "text-app-accent" : "text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)]"}`}
          title={rightTerminalVisible ? "隐藏右侧终端" : "显示右侧终端"}
        >
          <PanelRight size={14} aria-hidden="true" />
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
  envCheck: NonNullable<EnvCheckResult>;
  onInstallTool?: (cmd: string) => void;
  onOpen?: () => void;
}

function EnvPanel({ envCheck, onInstallTool, onOpen }: EnvPanelProps) {
  const [open, setOpen] = useState(false);
  const [visible, setVisible] = useState(false);    // stagger delay for animation
  const [expandedItem, setExpandedItem] = useState<string | null>(null);
  const [copied, setCopied] = useState<string | null>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  const dotRef = useRef<HTMLButtonElement>(null);
  const hasError = envCheck.items.some((i) => itemSeverity(i) === "error");
  const hasWarn = envCheck.items.some((i) => itemSeverity(i) === "warn");

  const copyCommand = useCallback(async (cmd: string) => {
    try {
      await navigator.clipboard.writeText(cmd);
    } catch {
      const ta = document.createElement("textarea");
      ta.value = cmd;
      ta.style.position = "fixed";
      ta.style.opacity = "0";
      document.body.appendChild(ta);
      ta.select();
      document.execCommand("copy");
      document.body.removeChild(ta);
    }
    setCopied(cmd);
    setTimeout(() => setCopied(null), 1600);
  }, []);

  const groups: { id: NonNullable<EnvCheckItem["category"]>; label: string }[] = [
    { id: "cli", label: "CLI 工具" },
    { id: "shell", label: "Shell 集成" },
    { id: "config", label: "配置" },
  ];

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
        setExpandedItem(null);
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
            envCheck.all_ok ? "bg-app-green" : hasError ? "bg-app-red" : "bg-app-amber"
          }`}
          style={{ filter: "blur(6px)" }}
        />
        {/* Core dot */}
        <Circle
          size={8}
          className={`relative shrink-0 transition-all duration-500 ${
            envCheck.all_ok
              ? "fill-app-green text-app-green drop-shadow-[0_0_5px_var(--app-green)]"
              : hasError
                ? "fill-app-red text-app-red drop-shadow-[0_0_5px_var(--app-red)] animate-pulse"
                : hasWarn
                  ? "fill-app-amber text-app-amber drop-shadow-[0_0_4px_var(--app-amber)]"
                  : "fill-app-amber text-app-amber drop-shadow-[0_0_4px_var(--app-amber)]"
          }`}
        />
      </button>

      {/* Panel — conditioned on state, not CSS hover */}
      {open && (
        <div
          ref={panelRef}
          className={`absolute top-full right-0 mt-2 z-50
            w-[420px] bg-app-panel border border-app-border
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
                envCheck.all_ok ? "text-app-green" : hasError ? "text-app-red" : "text-app-amber"
              }`}
            >
              {envCheck.all_ok ? "ALL CLEAR" : hasError ? `${envCheck.items.filter(i=>itemSeverity(i)==="error").length} ERROR` : "WARNING"}
            </span>
          </div>

          {/* ── Diagnostic rows ─────────────────────────── */}
          <div className="px-3 py-2 space-y-2 max-h-[520px] overflow-y-auto">
            {groups.map((group) => {
              const groupItems = envCheck.items.filter((item) => (item.category ?? "cli") === group.id);
              if (groupItems.length === 0) return null;
              return (
                <div key={group.id}>
                  <div className="px-1 pb-1 text-[10px] font-mono text-app-text-muted uppercase tracking-wider">
                    {group.label}
                  </div>
                  <div className="space-y-1">
                    {groupItems.map((item, idx) => {
                      const severity = itemSeverity(item);
                      const installOptions = item.install_options ?? (item.install_cmd ? [{
                        id: "default",
                        label: "推荐命令",
                        command: item.install_cmd,
                        description: item.detail,
                        recommended: true,
                        platforms: [],
                      }] : []);
                      const hasInstallOptions = item.status !== "ok" && installOptions.length > 0;
                      const expanded = expandedItem === item.name;
                      return (
                        <div
                          key={item.name}
                          className="text-2xs font-mono transition-all duration-200"
                          style={{
                            opacity: visible ? 1 : 0,
                            transform: visible ? "translateX(0)" : "translateX(-6px)",
                            transitionDelay: visible ? `${80 + idx * 40}ms` : "0ms",
                            transitionProperty: "opacity, transform",
                          }}
                        >
                          <div className="flex items-center gap-2 py-1">
                            <span
                              className={`shrink-0 w-4 text-center ${
                                severity === "ok"
                                  ? "text-app-green"
                                  : severity === "warn"
                                    ? "text-app-amber"
                                    : severity === "error"
                                      ? "text-app-red"
                                      : "text-app-text-muted"
                              }`}
                            >
                              {severity === "ok" ? "●" : severity === "warn" ? "◐" : severity === "error" ? "○" : "·"}
                            </span>

                            <span className="text-app-text-dim w-[88px] shrink-0 truncate">
                              {item.label}
                            </span>

                            <span className="flex-1 text-right text-app-text-muted truncate min-w-0" title={item.detected_path || item.detail}>
                              {item.detected_path || item.detail}
                            </span>

                            {hasInstallOptions && (
                              <button
                                onClick={() => setExpandedItem(expanded ? null : item.name)}
                                className="shrink-0 flex items-center gap-1 px-1.5 py-0.5 text-2xs font-mono
                                  text-app-accent border border-app-accent/40 hover:bg-app-accent hover:text-[var(--app-bg)] hover:border-app-accent
                                  transition-all duration-200"
                              >
                                <Terminal size={9} />
                                安装方式
                              </button>
                            )}
                          </div>

                          {expanded && (
                            <div className="ml-6 mt-1 mb-1 border border-app-border bg-[var(--app-subtle)]">
                              <div className="px-2 py-1 border-b border-app-border text-app-text-muted">
                                {item.detail}
                              </div>
                              <div className="p-1 space-y-1">
                                {installOptions.map((option) => (
                                  <div key={option.id} className="px-2 py-1 bg-[var(--app-cmd-bg)] border border-app-border-light">
                                    <div className="flex items-center gap-2">
                                      <span className="text-app-text">{option.label}</span>
                                      {option.recommended && <span className="text-[10px] text-app-green">推荐</span>}
                                      <div className="flex-1" />
                                      {option.command && (
                                        <>
                                          <button
                                            onClick={() => copyCommand(option.command!)}
                                            className="p-0.5 text-app-text-dim hover:text-app-accent"
                                            title="复制命令"
                                          >
                                            {copied === option.command ? <Check size={11} /> : <Copy size={11} />}
                                          </button>
                                          {onInstallTool && (
                                            <button
                                              onClick={() => onInstallTool(option.command!)}
                                              className="px-1.5 py-0.5 text-app-accent border border-app-accent/40 hover:bg-app-accent hover:text-[var(--app-bg)]"
                                            >
                                              运行
                                            </button>
                                          )}
                                        </>
                                      )}
                                    </div>
                                    <div className="mt-0.5 text-app-text-muted leading-relaxed">{option.description}</div>
                                    {option.command && (
                                      <code className="mt-1 block text-app-text-dim truncate select-all" title={option.command}>
                                        {option.command}
                                      </code>
                                    )}
                                  </div>
                                ))}
                              </div>
                            </div>
                          )}
                        </div>
                      );
                    })}
                  </div>
                </div>
              );
            })}
          </div>

          {/* ── Decorative scan line ────────────────────── */}
          <div
            className={`h-[2px] transition-colors duration-500 ${
              envCheck.all_ok
                ? "bg-app-green/40"
                : hasError
                  ? "bg-app-red/30"
                  : "bg-app-amber/30"
            }`}
          />
        </div>
      )}
    </div>
  );
}
