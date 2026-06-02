import React, { useRef, useEffect, useCallback, useState } from "react";
import { X, Plus, FolderOpen, Clock, Trash2, Play, Minus, ExternalLink, Maximize2, Minimize2, Search, ChevronUp, ChevronDown, Copy, CopyCheck, Palette } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { open as tauriOpen } from "@tauri-apps/plugin-dialog";
import { XTerm, XTermHandle } from "./XTerm";
import { ConfirmDialog } from "./ConfirmDialog";
import { ContextMenu } from "./ContextMenu";
import type { Terminal } from "@xterm/xterm";
import type { SessionRecord } from "../hooks/useTerminal";
import { shortenPath } from "../lib/path-utils";
import { TERMINAL_THEMES, loadTerminalTheme, saveTerminalTheme, isThemeSync, setThemeSync } from "../lib/terminalThemes";

interface TabInfo {
  id: string;
  name: string;
  workDir: string;
  ptyRunning: boolean;
}

interface TerminalPanelProps {
  mode?: "right" | "bottom";
  size?: number;
  maximized?: boolean;
  onToggleMaximize?: () => void;
  tabs: TabInfo[];
  activeTabId: string;
  history: SessionRecord[];
  onAttachTerminal: (tabId: string, term: Terminal) => void;
  onClose: () => void;
  onSwitchTab: (tabId: string) => void;
  onCloseTab: (tabId: string) => void;
  onCloseOthers: (tabId: string) => void;
  onCloseToRight: (tabId: string) => void;
  onNewTab: () => void;
  onSetWorkDir: (tabId: string, dir: string) => void;
  onTerminalReady?: (tabId: string) => void;
  onTerminalResize?: (tabId: string, cols: number, rows: number) => void;
  fontSize: number;
  onSetFontSize: (size: number) => void;
  terminalVersion: number;
  onResumeSession: (record: SessionRecord) => void;
  onNewSessionFromHistory: (record: SessionRecord) => void;
  onDeleteHistory: (id: string) => void;
  onClearHistory: () => void;
}

/* ── Per-tab XTerm wrapper ──────────────────────────────── */
function TabTerminal({ tabId, onAttach, onReady, onResize, fontSize, onXTermHandle, themeName }: {
  tabId: string;
  onAttach: (term: Terminal) => void;
  onReady?: () => void;
  onResize?: (cols: number, rows: number) => void;
  fontSize?: number;
  onXTermHandle?: (tabId: string, handle: XTermHandle | null) => void;
  themeName?: string;
}) {
  const xtermRef = useRef<XTermHandle>(null);
  const attached = useRef(false);

  const handleTerminal = useCallback((term: Terminal) => {
    if (!attached.current) {
      attached.current = true;
      onAttach(term);
      if (onXTermHandle) onXTermHandle(tabId, xtermRef.current);
    }
  }, [onAttach, onXTermHandle, tabId]);

  useEffect(() => {
    attached.current = false;
    // Notify parent of the handle on mount
    if (onXTermHandle && xtermRef.current) {
      onXTermHandle(tabId, xtermRef.current);
    }
    return () => { if (onXTermHandle) onXTermHandle(tabId, null); };
  }, [tabId]);

  return <XTerm ref={xtermRef} onTerminal={handleTerminal} onReady={onReady} onResize={onResize} fontSize={fontSize} themeName={themeName} />;
}

/* ── Format relative time ───────────────────────────────── */
function formatTime(ts: number): string {
  const diff = Date.now() - ts;
  const mins = Math.floor(diff / 60000);
  if (mins < 1) return "刚刚";
  if (mins < 60) return `${mins} 分钟前`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours} 小时前`;
  const days = Math.floor(hours / 24);
  if (days < 7) return `${days} 天前`;
  return new Date(ts).toLocaleDateString("zh-CN");
}

/* ── TerminalPanel ──────────────────────────────────────── */
export function TerminalPanel({
  mode, size, maximized, onToggleMaximize,
  tabs, activeTabId, history,
  onAttachTerminal, onClose, onSwitchTab, onCloseTab, onCloseOthers, onCloseToRight, onNewTab,
  onSetWorkDir, onTerminalReady, onTerminalResize,
  fontSize, onSetFontSize, terminalVersion,
  onResumeSession, onNewSessionFromHistory, onDeleteHistory, onClearHistory,
}: TerminalPanelProps) {
  const activeTab = tabs.find((t) => t.id === activeTabId);
  const runningCount = tabs.filter((t) => t.ptyRunning).length;
  const modKey = navigator.userAgent.includes("Mac") ? "⌘" : "Ctrl";
  const maximizeTip = `${maximized ? "还原" : "最大化"} (${modKey}⇧M)`;
  const [showHistory, setShowHistory] = useState(false);
  const [historySearch, setHistorySearch] = useState("");
  const [closingTabId, setClosingTabId] = useState<string | null>(null);
  const [closingPanel, setClosingPanel] = useState(false);
  const [clearHistoryConfirm, setClearHistoryConfirm] = useState(false);
  const [deleteHistoryTarget, setDeleteHistoryTarget] = useState<string | null>(null);
  const [themeName, setThemeName] = useState(() => loadTerminalTheme(mode ?? "right"));
  const [showThemeMenu, setShowThemeMenu] = useState(false);
  const [themeSync, setThemeSyncState] = useState(() => isThemeSync());

  // Listen for theme sync/change events from the other panel (same-window custom event)
  useEffect(() => {
    const handler = () => {
      setThemeSyncState(isThemeSync());
      setThemeName(loadTerminalTheme(mode ?? "right"));
    };
    window.addEventListener("kn-theme-changed", handler);
    return () => window.removeEventListener("kn-theme-changed", handler);
  }, [mode]);

  // ── Tab context menu ────────────────────────────────────
  const [ctxMenu, setCtxMenu] = useState<{ x: number; y: number; tabId: string } | null>(null);
  const [copiedPath, setCopiedPath] = useState(false);

  const handleTabContextMenu = (e: React.MouseEvent, tabId: string) => {
    e.preventDefault();
    setCtxMenu({ x: e.clientX, y: e.clientY, tabId });
  };

  const handleCopyPath = async (tabId: string) => {
    const tab = tabs.find((t) => t.id === tabId);
    if (tab?.workDir) {
      try {
        await navigator.clipboard.writeText(tab.workDir);
        setCopiedPath(true);
        setTimeout(() => setCopiedPath(false), 1500);
      } catch { /* clipboard may fail in some contexts */ }
    }
  };

  // ── Terminal search state ──────────────────────────────
  const [showSearch, setShowSearch] = useState(false);
  const [searchTerm, setSearchTerm] = useState("");
  const [searchMatchIndex, setSearchMatchIndex] = useState(0);
  const [searchMatchCount, setSearchMatchCount] = useState(0);
  const searchInputRef = useRef<HTMLInputElement>(null);
  const xtermHandlesRef = useRef<Map<string, XTermHandle>>(new Map());

  const filteredHistory = historySearch.trim()
    ? history.filter((r) =>
        r.command.toLowerCase().includes(historySearch.toLowerCase()) ||
        r.workDir.toLowerCase().includes(historySearch.toLowerCase())
      )
    : history;

  const browseDir = async () => {
    try {
      const selected = await tauriOpen({ directory: true, multiple: false, title: "选择工作目录" });
      if (selected && typeof selected === "string" && activeTabId) {
        onSetWorkDir(activeTabId, selected);
      }
    } catch { /* */ }
  };

  // ── XTerm handle tracking (for search addon access) ─────
  const handleXTermRef = useCallback((tabId: string, handle: XTermHandle | null) => {
    if (handle) {
      xtermHandlesRef.current.set(tabId, handle);
    } else {
      xtermHandlesRef.current.delete(tabId);
    }
  }, []);

  // ── Search actions ──────────────────────────────────────
  const openSearch = useCallback(() => {
    const handle = xtermHandlesRef.current.get(activeTabId);
    if (!handle?.searchAddon) return;
    setShowSearch(true);
    setSearchMatchIndex(0);
    setSearchMatchCount(0);
    setTimeout(() => searchInputRef.current?.focus(), 50);
  }, [activeTabId]);

  const doSearch = useCallback((term: string) => {
    const handle = xtermHandlesRef.current.get(activeTabId);
    if (!handle?.searchAddon) return;
    if (!term) {
      handle.searchAddon.clearDecorations();
      setSearchMatchIndex(0);
      setSearchMatchCount(0);
      return;
    }
    // findNext with { incremental: true } for live highlighting
    try {
      const result = handle.searchAddon.findNext(term, { incremental: true });
      setSearchMatchIndex(result ? 1 : 0);
      setSearchMatchCount(result ? 1 : 0);
    } catch { /* search addon may throw on empty/invalid patterns */ }
  }, [activeTabId]);

  const findNext = useCallback(() => {
    const handle = xtermHandlesRef.current.get(activeTabId);
    if (!handle?.searchAddon || !searchTerm) return;
    try {
      const result = handle.searchAddon.findNext(searchTerm);
      if (result) {
        setSearchMatchIndex((prev) => {
          const idx = prev + 1;
          return idx > 999 ? 1 : idx;
        });
      }
    } catch { /* */ }
  }, [activeTabId, searchTerm]);

  const findPrevious = useCallback(() => {
    const handle = xtermHandlesRef.current.get(activeTabId);
    if (!handle?.searchAddon || !searchTerm) return;
    try {
      const result = handle.searchAddon.findPrevious(searchTerm);
      if (result) {
        setSearchMatchIndex((prev) => {
          const idx = prev - 1;
          return idx < 1 ? 999 : idx;
        });
      }
    } catch { /* */ }
  }, [activeTabId, searchTerm]);

  const closeSearch = useCallback(() => {
    setShowSearch(false);
    const handle = xtermHandlesRef.current.get(activeTabId);
    try { handle?.searchAddon?.clearDecorations(); } catch { /* */ }
    setSearchTerm("");
    setSearchMatchIndex(0);
    setSearchMatchCount(0);
  }, [activeTabId]);

  // ── Keyboard listener for Cmd/Ctrl+F ────────────────────
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === "f") {
        // Check if focus is inside this terminal panel's DOM tree
        const el = document.activeElement as HTMLElement | null;
        const panel = el?.closest("[data-panel]") as HTMLElement | null;
        const matchMode = panel?.dataset.panel === mode;
        // Also check: is any xterm element inside this panel focused?
        // xterm.js textarea may not always register in closest() chain
        const xtermInPanel = el?.closest(".xterm") && el?.closest(`[data-panel="${mode}"]`);
        if (matchMode || xtermInPanel) {
          e.preventDefault();
          e.stopPropagation();
          openSearch();
          return;
        }
        // Fallback: if this TerminalPanel has mounted tabs, allow Cmd+F
        // even without strict focus match (e.g., focus just landed on terminal bg)
        if (tabs.length > 0 && !el?.closest("[data-panel]")) {
          // Focus is outside any panel — don't steal Cmd+F (browser find, etc.)
          return;
        }
      }
      if (e.key === "Escape" && showSearch) {
        e.preventDefault();
        closeSearch();
      }
    };
    window.addEventListener("keydown", handler, true); // capture phase to beat xterm.js
    return () => window.removeEventListener("keydown", handler, true);
  }, [mode, openSearch, showSearch, closeSearch, tabs.length]);

  // Sync search when active tab changes
  useEffect(() => {
    if (showSearch) {
      // Re-sync search term with new tab's search addon
      if (searchTerm) doSearch(searchTerm);
      else closeSearch();
    }
  }, [activeTabId]);

  useEffect(() => {
    const timer = setTimeout(() => window.dispatchEvent(new Event("resize")), 100);
    return () => clearTimeout(timer);
  }, [size]);

  const isBottom = mode === "bottom";
  const containerStyle: React.CSSProperties = maximized
    ? {}
    : (isBottom
      ? (size !== undefined ? { height: `${size}px` } : {})
      : (size !== undefined ? { width: `${size}px` } : {})
    );

  return (
    <div
      data-panel={mode}  // for keyboard shortcut: identify which panel has focus
      style={containerStyle}
      className={`flex flex-col bg-[var(--app-terminal-bg)] border-app-border
        ${isBottom ? "border-t w-full" : "border-l h-full"}
        ${maximized ? "flex-1" : "shrink-0"}`}
    >
      {/* Header */}
      <div className="flex items-center h-[28px] bg-[var(--app-terminal-header)] border-b border-app-border shrink-0 select-none">
        {/* Tab bar */}
        <div className="flex-1 flex items-center overflow-x-auto min-w-0">
          {tabs.map((tab) => {
            const isActive = tab.id === activeTabId;
            return (
              <div key={tab.id} onClick={() => onSwitchTab(tab.id)}
                onContextMenu={(e) => handleTabContextMenu(e, tab.id)}
                className={`group flex items-center gap-1.5 px-2.5 h-[28px] text-2xs font-mono cursor-pointer
                  border-r border-app-border shrink-0 transition-colors select-none
                  ${isActive ? "bg-[var(--app-terminal-bg)] text-app-text border-b-[2px] border-b-app-accent -mb-px"
                    : "text-app-text-muted hover:text-app-text hover:bg-[var(--app-hover)]"}`}
              >
                <span className={`w-1.5 h-1.5 rounded-full shrink-0 ${tab.ptyRunning ? "bg-app-accent shadow-[0_0_4px_var(--app-glow)]" : "bg-app-text-muted opacity-40"}`} />
                <span className="max-w-[100px] truncate">{tab.name}</span>
                {tabs.length > 1 && (
                  <button onClick={(e) => { e.stopPropagation(); setClosingTabId(tab.id); }}
                    className={`ml-0.5 p-0.5 hover:bg-[var(--app-hover)] transition-colors shrink-0
                      ${isActive ? "" : "opacity-0 group-hover:opacity-100"}`} title="关闭">
                    <X size={10} />
                  </button>
                )}
              </div>
            );
          })}
          {/* Right panel: no manual "+" — only profile commands create tabs */}
          {isBottom && (
            <button onClick={onNewTab} className="px-2.5 h-[28px] text-app-text-muted hover:text-app-text hover:bg-[var(--app-hover)] transition-colors shrink-0" title="新建终端">
              <Plus size={13} />
            </button>
          )}
        </div>

        {/* History button — right panel only */}
        {!isBottom && (
        <div className="relative shrink-0">
          <button onClick={() => setShowHistory(!showHistory)}
            className={`px-2 h-[28px] text-app-text-muted hover:text-app-text transition-colors flex items-center gap-1 ${showHistory ? "text-app-accent" : ""}`}
            title="历史会话"
          >
            <Clock size={12} />
            {history.length > 0 && (
              <span className="text-2xs tabular-nums">{history.length}</span>
            )}
          </button>

          {/* History dropdown */}
          {showHistory && (
            <>
              <div className="fixed inset-0 z-40" onClick={() => setShowHistory(false)} />
              <div className="absolute right-0 top-[28px] z-50 w-[380px] max-h-[400px] overflow-y-auto
                bg-[var(--app-panel)] border border-app-border shadow-dialog">
                <div className="px-3 py-2 border-b border-app-border bg-[var(--app-subtle)] space-y-1.5">
                  <div className="flex items-center justify-between">
                    <span className="text-2xs text-app-text-muted uppercase tracking-wider">历史会话</span>
                    <div className="flex items-center gap-2">
                      <span className="text-2xs text-app-text-muted">({filteredHistory.length}/{history.length})</span>
                      {history.length > 0 && (
                        <button
                          onClick={() => setClearHistoryConfirm(true)}
                          className="text-2xs text-app-text-dim hover:text-app-red transition-colors"
                          title="清空全部历史"
                        >
                          清空
                        </button>
                      )}
                    </div>
                  </div>
                  <input
                    type="text"
                    value={historySearch}
                    onChange={(e) => setHistorySearch(e.target.value)}
                    placeholder="搜索..."
                    className="w-full h-[22px] text-2xs font-mono bg-[var(--app-input)] border border-app-border px-2 py-0 focus:border-app-accent"
                    spellCheck={false}
                  />
                </div>
                {filteredHistory.length === 0 ? (
                  <div className="px-4 py-6 text-center text-xs text-app-text-muted font-mono">
                    暂无历史记录
                  </div>
                ) : (
                  filteredHistory.map((r) => (
                    <div key={r.id}
                      className="px-3 py-2 border-b border-app-border-light hover:bg-[var(--app-hover)] group/h transition-colors"
                    >
                      {/* Command + directory */}
                      <div className="min-w-0 mb-1.5">
                        {r.workDir && (
                          <div className="text-2xs text-app-text-muted font-mono flex items-center gap-1 mb-0.5">
                            <FolderOpen size={9} />
                            {shortenPath(r.workDir)}
                          </div>
                        )}
                        <code className="text-xs text-app-text font-mono block truncate">
                          <span className="text-app-accent opacity-70">$ </span>{r.command}
                        </code>
                        <div className="text-2xs text-app-text-muted mt-0.5">{formatTime(r.timestamp)}</div>
                      </div>

                      {/* Actions */}
                      <div className="flex items-center gap-1">
                        <button
                          onClick={() => { onNewSessionFromHistory(r); setShowHistory(false); }}
                          className="flex items-center gap-1 px-2 py-0.5 text-2xs text-app-text-dim
                            hover:text-app-green border border-app-border hover:border-[var(--app-accent)]
                            bg-[var(--app-input)] hover:bg-[var(--app-hover)] transition-colors font-mono"
                        >
                          <Play size={10} />新会话
                        </button>
                        {r.resumeLastCommand && (
                          <button
                            onClick={() => { onResumeSession({ ...r, resumeCommand: r.resumeLastCommand }); setShowHistory(false); }}
                            className="flex items-center gap-1 px-2 py-0.5 text-2xs text-app-green
                              hover:text-[var(--app-accent)] border border-app-border
                              hover:border-app-accent bg-[var(--app-input)]
                              hover:bg-[var(--app-hover)] transition-colors font-mono"
                            title={`恢复最近: ${r.resumeLastCommand}`}
                          >
                            <Clock size={10} />恢复最近
                          </button>
                        )}
                        {r.resumeCommand && (
                          <button
                            onClick={() => { onResumeSession(r); setShowHistory(false); }}
                            className="flex items-center gap-1 px-2 py-0.5 text-2xs text-app-amber
                              hover:text-[var(--app-amber-glow)] border border-app-border
                              hover:border-app-amber bg-[var(--app-input)]
                              hover:bg-[var(--app-hover)] transition-colors font-mono"
                            title={`恢复: ${r.resumeCommand}`}
                          >
                            <Clock size={10} />恢复会话
                          </button>
                        )}
                        <div className="flex-1" />
                        <button
                          onClick={(e) => {
                            e.stopPropagation();
                            setDeleteHistoryTarget(r.id);
                          }}
                          className="p-1 text-app-text-dim hover:text-app-red hover:bg-app-red-bg transition-colors shrink-0"
                          title="删除"
                        >
                          <Trash2 size={11} />
                        </button>
                      </div>
                    </div>
                  ))
                )}
              </div>
            </>
          )}
        </div>
        )}

        {/* Search button (triggers terminal search bar) */}
        <button
          onClick={openSearch}
          className={`shrink-0 px-2 h-[28px] text-app-text-muted hover:text-app-text transition-colors ${showSearch ? "text-app-accent" : ""}`}
          title={`搜索终端输出 (${modKey}F)`}
        >
          <Search size={12} />
        </button>

        {/* Theme selector */}
        <div className="relative shrink-0">
          <button
            onClick={() => setShowThemeMenu(!showThemeMenu)}
            className={`px-2 h-[28px] text-app-text-muted hover:text-app-text transition-colors ${showThemeMenu ? "text-app-accent" : ""}`}
            title="终端配色"
          >
            <Palette size={12} />
          </button>
          {showThemeMenu && (
            <>
              <div className="fixed inset-0 z-40" onClick={() => setShowThemeMenu(false)} />
              <div className="absolute right-0 top-[28px] z-50 w-[200px] bg-[var(--app-panel)] border border-app-border shadow-dialog py-0.5">
                {TERMINAL_THEMES.map((t) => (
                  <button
                    key={t.name}
                    onClick={() => {
                      setThemeName(t.name);
                      saveTerminalTheme(t.name, mode ?? "right");
                      window.dispatchEvent(new Event("kn-theme-changed"));
                      setShowThemeMenu(false);
                    }}
                    className={`w-full flex items-center gap-2 px-3 py-1.5 text-xs font-mono transition-colors
                      ${themeName === t.name ? "text-app-accent bg-[var(--app-hover)]" : "text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)]"}`}
                  >
                    <span
                      className="w-3 h-3 rounded-sm border border-app-border shrink-0"
                      style={{ background: t.background }}
                    />
                    {t.label}
                  </button>
                ))}
                <div className="border-t border-app-border mt-0.5 pt-0.5">
                  <label className="flex items-center gap-2 px-3 py-1.5 cursor-pointer text-xs font-mono text-app-text-dim hover:text-app-text transition-colors">
                    <input
                      type="checkbox"
                      checked={themeSync}
                      onChange={(e) => {
                        const checked = e.target.checked;
                        setThemeSync(checked);
                        setThemeSyncState(checked);
                        if (checked) {
                          // Sync ON: save current selection to shared key
                          saveTerminalTheme(themeName, mode ?? "right");
                        }
                        window.dispatchEvent(new Event("kn-theme-changed"));
                      }}
                      className="w-3 h-3 accent-app-accent"
                    />
                    同步配色
                  </label>
                </div>
              </div>
            </>
          )}
        </div>

        {/* Work dir (right panel only) + font + close */}
        <div className="flex items-center gap-1 px-2 shrink-0">
          {/* Font size */}
          <button onClick={() => onSetFontSize(fontSize - 1)}
            className="px-1 h-[20px] text-2xs text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)] transition-colors font-mono"
            title="缩小字体">A⁻</button>
          <span className="text-2xs text-app-text-muted tabular-nums w-5 text-center">{fontSize}</span>
          <button onClick={() => onSetFontSize(fontSize + 1)}
            className="px-1 h-[20px] text-2xs text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)] transition-colors font-mono"
            title="放大字体">A⁺</button>
          {/* Pop-out */}
          <button
            onClick={() => invoke("new_window").catch(() => {})}
            className="p-0.5 text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)] transition-colors"
            title="弹出为独立窗口"
          >
            <ExternalLink size={12} />
          </button>
          {/* Maximize / Restore */}
          {onToggleMaximize && (
            <button
              onClick={onToggleMaximize}
              className="p-0.5 text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)] transition-colors"
              title={maximizeTip}
            >
              {maximized ? <Minimize2 size={12} /> : <Maximize2 size={12} />}
            </button>
          )}
          {/* Work dir — right panel only */}
          {!isBottom && (
            <>
              <input type="text" value={activeTab?.workDir || ""}
                onChange={(e) => activeTabId && onSetWorkDir(activeTabId, e.target.value)}
                placeholder="工作目录"
                className="w-[160px] h-[20px] bg-[var(--app-input)] border border-app-border text-2xs font-mono text-app-text-dim px-1.5 py-0 focus:border-app-accent"
                spellCheck={false} />
              <button onClick={browseDir} className="p-0.5 text-app-text-muted hover:text-app-accent transition-colors" title="选择目录">
                <FolderOpen size={11} />
              </button>
            </>
          )}
          <button
            onClick={() => {
              if (runningCount > 0) {
                setClosingPanel(true);
              } else {
                onClose();
              }
            }}
            className="p-0.5 text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)] transition-colors ml-1"
            title="关闭面板"
          >
            <X size={13} />
          </button>
        </div>
      </div>

      {/* Terminal area — all tabs mounted, inactive hidden via visibility (not display:none) */}
      <div className="flex-1 relative bg-[var(--app-terminal-bg)]">
        {/* Search bar overlay */}
        {showSearch && (
          <div className="absolute top-0 right-0 z-20 flex items-center gap-1 px-2 py-1
            bg-[var(--app-panel)] border-b border-l border-app-border shadow-lg
            animate-[fadeIn_120ms_ease-out]"
          >
            <Search size={11} className="text-app-text-muted shrink-0" />
            <input
              ref={searchInputRef}
              type="text"
              value={searchTerm}
              onChange={(e) => { setSearchTerm(e.target.value); doSearch(e.target.value); }}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  if (e.shiftKey) findPrevious(); else findNext();
                }
                if (e.key === "Escape") { e.preventDefault(); closeSearch(); }
              }}
              className="w-[160px] h-[22px] text-xs font-mono bg-[var(--app-input)] border border-app-border px-1.5 py-0 focus:border-app-accent"
              placeholder="搜索..."
              spellCheck={false}
            />
            {searchTerm && (
              <span className="text-2xs text-app-text-muted font-mono tabular-nums min-w-[28px] text-center">
                {searchMatchIndex > 0 ? searchMatchIndex : "?"}
              </span>
            )}
            <button
              onClick={findPrevious}
              className="p-0.5 text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)] transition-colors"
              title="上一个 (Shift+Enter)"
            >
              <ChevronUp size={12} />
            </button>
            <button
              onClick={findNext}
              className="p-0.5 text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)] transition-colors"
              title="下一个 (Enter)"
            >
              <ChevronDown size={12} />
            </button>
            <button
              onClick={closeSearch}
              className="p-0.5 text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)] transition-colors ml-0.5"
              title="关闭 (Escape)"
            >
              <X size={12} />
            </button>
          </div>
        )}

        {tabs.map((tab) => {
          const isActive = tab.id === activeTabId;
          return (
            <div
              key={`${tab.id}-v${terminalVersion}`}
              className="absolute inset-0"
              style={{
                visibility: isActive ? "visible" : "hidden",
                pointerEvents: isActive ? "auto" : "none",
              }}
            >
              <TabTerminal tabId={tab.id} onAttach={(term) => onAttachTerminal(tab.id, term)}
                onReady={onTerminalReady ? () => onTerminalReady(tab.id) : undefined}
                onResize={onTerminalResize ? (cols, rows) => onTerminalResize(tab.id, cols, rows) : undefined}
                fontSize={fontSize}
                themeName={themeName}
                onXTermHandle={handleXTermRef} />
            </div>
          );
        })}
      </div>

      {/* Tab context menu */}
      {ctxMenu && (
        <ContextMenu
          x={ctxMenu.x} y={ctxMenu.y}
          onClose={() => setCtxMenu(null)}
          items={[
            {
              label: "关闭其他",
              icon: <X size={13} />,
              onClick: () => onCloseOthers(ctxMenu.tabId),
              disabled: tabs.length <= 1,
            },
            {
              label: "关闭右侧",
              icon: <Trash2 size={13} />,
              onClick: () => onCloseToRight(ctxMenu.tabId),
              disabled: tabs.findIndex((t) => t.id === ctxMenu.tabId) >= tabs.length - 1,
            },
            {
              label: copiedPath ? "已复制 ✓" : "复制路径",
              icon: copiedPath ? <CopyCheck size={13} /> : <Copy size={13} />,
              onClick: () => handleCopyPath(ctxMenu.tabId),
            },
          ]}
        />
      )}

      <ConfirmDialog
        open={!!closingTabId}
        title="关闭终端"
        message="确定要关闭此终端标签吗？未保存的输出将丢失。"
        confirmLabel="关闭"
        onConfirm={() => {
          if (closingTabId) { onCloseTab(closingTabId); setClosingTabId(null); }
        }}
        onCancel={() => setClosingTabId(null)}
      />

      <ConfirmDialog
        open={closingPanel}
        title="关闭终端面板"
        message={`有 ${runningCount} 个终端会话正在运行，关闭面板将终止所有终端。确定要关闭吗？`}
        confirmLabel="关闭"
        onConfirm={() => {
          setClosingPanel(false);
          onClose();
        }}
        onCancel={() => setClosingPanel(false)}
      />

      <ConfirmDialog
        open={clearHistoryConfirm}
        title="清空历史会话"
        message="确定要清空所有历史会话记录吗？此操作不可撤销。"
        confirmLabel="清空"
        onConfirm={() => {
          onClearHistory();
          setClearHistoryConfirm(false);
        }}
        onCancel={() => setClearHistoryConfirm(false)}
      />

      <ConfirmDialog
        open={!!deleteHistoryTarget}
        title="删除会话记录"
        message={`确定要删除此会话记录吗？\n\n${history.find(r => r.id === deleteHistoryTarget)?.command || ""}`}
        confirmLabel="删除"
        onConfirm={() => {
          if (deleteHistoryTarget) {
            onDeleteHistory(deleteHistoryTarget);
            setDeleteHistoryTarget(null);
          }
        }}
        onCancel={() => setDeleteHistoryTarget(null)}
      />
    </div>
  );
}
