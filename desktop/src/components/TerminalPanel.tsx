import { relativeTime } from "../lib/time-utils";
import React, { useRef, useEffect, useCallback, useState } from "react";
import { X, Plus, FolderOpen, Clock, Trash2, Play, Minus, Maximize2, Minimize2, Search, ChevronUp, ChevronDown, Copy, CopyCheck, Palette, SplitSquareVertical, SplitSquareHorizontal } from "lucide-react";


import { open as tauriOpen } from "@tauri-apps/plugin-dialog";
import { XTermHandle } from "./XTerm";
import { PaneSplitter } from "./PaneSplitter";
import { ConfirmDialog } from "./ConfirmDialog";
import { ContextMenu } from "./ContextMenu";
import type { Terminal } from "@xterm/xterm";
import { flattenPanes, type PaneNode, type SplitDirection, type NavDirection } from "../lib/pane-types";
import type { SessionRecord } from "../hooks/useTerminal";
import { formatShortcut, isMac } from "../utils/shortcut";
import { shortenPath } from "../lib/path-utils";
import { TERMINAL_THEMES, loadTerminalTheme, saveTerminalTheme, isThemeSync, setThemeSync } from "../lib/terminalThemes";

interface TabInfo {
  id: string;
  name: string;
  workDir: string;
  ptyRunning: boolean;
  // Pane tree fields (for split-pane support)
  rootNode: PaneNode;
  activePaneId: string;
  zoomedPaneId: string | null;
}

interface TerminalPanelProps {
  mode?: "right" | "bottom";
  visible?: boolean;
  size?: number;
  maximized?: boolean;
  onToggleMaximize?: () => void;
  tabs: TabInfo[];
  activeTabId: string;
  history: SessionRecord[];
  onAttachTerminal: (paneId: string, term: Terminal) => void;
  onClose: () => void;
  onSwitchTab: (tabId: string) => void;
  onCloseTab: (tabId: string) => void;
  onCloseOthers: (tabId: string) => void;
  onCloseToRight: (tabId: string) => void;
  onNewTab: () => void;
  onSetWorkDir: (tabId: string, dir: string) => void;
  onTerminalReady?: (paneId: string) => void;
  onTerminalResize?: (paneId: string, cols: number, rows: number) => void;
  fontSize: number;
  onSetFontSize: (size: number) => void;
  onResumeSession: (record: SessionRecord) => void;
  onNewSessionFromHistory: (record: SessionRecord) => void;
  onDeleteHistory: (id: string) => void;
  onClearHistory: () => void;
  // Pane operations
  onSplitPane: (tabId: string, direction: SplitDirection, workDir?: string, paneId?: string) => void;
  onClosePane: (tabId: string, paneId?: string) => void;
  onFocusPane: (tabId: string, paneId: string) => void;
  onNavigatePane: (tabId: string, direction: NavDirection) => void;
  onCyclePane: (tabId: string, forward: boolean) => void;
  onZoomPane: (tabId: string) => void;
}

/* ── Format relative time ───────────────────────────────── */

/* ── TerminalPanel ──────────────────────────────────────── */
export function TerminalPanel({
  mode, visible = true, size, maximized, onToggleMaximize,
  tabs, activeTabId, history,
  onAttachTerminal, onClose, onSwitchTab, onCloseTab, onCloseOthers, onCloseToRight, onNewTab,
  onSetWorkDir, onTerminalReady, onTerminalResize,
  fontSize, onSetFontSize,
  onResumeSession, onNewSessionFromHistory, onDeleteHistory, onClearHistory,
  onSplitPane, onClosePane, onFocusPane, onNavigatePane, onCyclePane, onZoomPane,
}: TerminalPanelProps) {
  const activeTab = tabs.find((t) => t.id === activeTabId);
  const runningCount = tabs.reduce((sum, tab) => sum + flattenPanes(tab.rootNode).filter((l) => l.ptyRunning).length, 0);
  const maximizeTip = `${maximized ? "还原" : "最大化"} (${formatShortcut("mod+⇧M")})`;
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

  // ── Pane context menu ───────────────────────────────────
  const [paneCtxMenu, setPaneCtxMenu] = useState<{
    x: number; y: number; tabId: string; paneId: string;
  } | null>(null);

  const handlePaneContextMenu = useCallback((tabId: string, paneId: string, e: React.MouseEvent) => {
    // Focus the right-clicked pane immediately for visual feedback
    onFocusPane(tabId, paneId);
    setPaneCtxMenu({ x: e.clientX, y: e.clientY, tabId, paneId });
  }, [onFocusPane]);

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

  // ── XTerm handle tracking (for search addon access, keyed by paneId) ──
  const handleXTermRef = useCallback((paneId: string, handle: XTermHandle | null) => {
    if (handle) {
      xtermHandlesRef.current.set(paneId, handle);
    } else {
      xtermHandlesRef.current.delete(paneId);
    }
  }, []);

  // Active pane for the current tab (for search targeting)
  const activePaneId = activeTab?.activePaneId || "";
  const activeRootNode = activeTab?.rootNode;

  // ── Search actions ──────────────────────────────────────
  const openSearch = useCallback(() => {
    const handle = xtermHandlesRef.current.get(activePaneId);
    const searchAddon = handle?.getSearchAddon();
    if (!searchAddon) return;
    setShowSearch(true);
    setSearchMatchIndex(0);
    setTimeout(() => searchInputRef.current?.focus(), 50);
  }, [activePaneId]);

  const doSearch = useCallback((term: string) => {
    const handle = xtermHandlesRef.current.get(activePaneId);
    const searchAddon = handle?.getSearchAddon();
    if (!searchAddon) return;
    if (!term) {
      searchAddon.clearDecorations();
      setSearchMatchIndex(0);
      return;
    }
    // findNext with { incremental: true } for live highlighting
    try {
      const result = searchAddon.findNext(term, { incremental: true });
      setSearchMatchIndex(result ? 1 : 0);
    } catch { /* search addon may throw on empty/invalid patterns */ }
  }, [activePaneId]);

  const findNext = useCallback(() => {
    const handle = xtermHandlesRef.current.get(activePaneId);
    const searchAddon = handle?.getSearchAddon();
    if (!searchAddon || !searchTerm) return;
    try {
      const result = searchAddon.findNext(searchTerm);
      if (result) {
        setSearchMatchIndex((prev) => {
          const idx = prev + 1;
          return idx > 999 ? 1 : idx;
        });
      }
    } catch { /* */ }
  }, [activePaneId, searchTerm]);

  const findPrevious = useCallback(() => {
    const handle = xtermHandlesRef.current.get(activePaneId);
    const searchAddon = handle?.getSearchAddon();
    if (!searchAddon || !searchTerm) return;
    try {
      const result = searchAddon.findPrevious(searchTerm);
      if (result) {
        setSearchMatchIndex((prev) => {
          const idx = prev - 1;
          return idx < 1 ? 999 : idx;
        });
      }
    } catch { /* */ }
  }, [activePaneId, searchTerm]);

  const closeSearch = useCallback(() => {
    setShowSearch(false);
    const handle = xtermHandlesRef.current.get(activePaneId);
    try { handle?.getSearchAddon()?.clearDecorations(); } catch { /* */ }
    setSearchTerm("");
    setSearchMatchIndex(0);
  }, [activePaneId]);

  // ── Keyboard listener: Cmd/Ctrl+F search + pane shortcuts ──
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const mod = e.metaKey || e.ctrlKey;
      const paneMod = isMac() && e.metaKey;

      // Check if focus is inside this terminal panel's DOM tree
      const el = document.activeElement as HTMLElement | null;
      const panel = el?.closest("[data-panel]") as HTMLElement | null;
      const matchMode = panel?.dataset.panel === mode;
      const xtermInPanel = el?.closest(".xterm") && el?.closest(`[data-panel="${mode}"]`);
      const isInPanel = matchMode || xtermInPanel;

      // Cmd+F — search (only in this panel)
      if (mod && e.key === "f") {
        if (isInPanel) {
          e.preventDefault();
          e.stopPropagation();
          openSearch();
          return;
        }
        if (tabs.length > 0 && !el?.closest("[data-panel]")) {
          return;
        }
      }

      // Escape — close search
      if (e.key === "Escape" && showSearch) {
        e.preventDefault();
        closeSearch();
        return;
      }

      // ── Pane shortcuts (only when focus is in this panel) ──
      if (!isInPanel) return;
      if (!activeTabId) return;

      // Cmd+D — split horizontally (left/right)
      if (paneMod && !e.shiftKey && e.code === "KeyD") {
        e.preventDefault();
        e.stopPropagation();
        onSplitPane(activeTabId, "horizontal");
        return;
      }
      // Cmd+\ or Cmd+Shift+D — split vertically (top/bottom).
      // Cmd+Shift+D is intercepted by some browsers ("Bookmark All Tabs"),
      // so Cmd+\ is the primary shortcut (VS Code convention).
      if (paneMod && !e.shiftKey && e.code === "Backslash") {
        e.preventDefault();
        e.stopPropagation();
        onSplitPane(activeTabId, "vertical");
        return;
      }
      if (paneMod && e.shiftKey && e.code === "KeyD") {
        e.preventDefault();
        e.stopPropagation();
        onSplitPane(activeTabId, "vertical");
        return;
      }
      // Cmd+W — close active pane
      if (paneMod && !e.shiftKey && e.code === "KeyW") {
        e.preventDefault();
        e.stopPropagation();
        onClosePane(activeTabId);
        return;
      }
      // Cmd+Opt+Arrow — navigate by direction
      if (paneMod && e.altKey && e.key.startsWith("Arrow")) {
        e.preventDefault();
        e.stopPropagation();
        const dirMap: Record<string, NavDirection> = {
          ArrowUp: "up", ArrowDown: "down",
          ArrowLeft: "left", ArrowRight: "right",
        };
        const dir = dirMap[e.key];
        if (dir) onNavigatePane(activeTabId, dir);
        return;
      }
      // Cmd+] / Cmd+[ — cycle panes
      if (paneMod && e.key === "]") {
        e.preventDefault();
        e.stopPropagation();
        onCyclePane(activeTabId, true);
        return;
      }
      if (paneMod && e.key === "[") {
        e.preventDefault();
        e.stopPropagation();
        onCyclePane(activeTabId, false);
        return;
      }
      // Cmd+Shift+Enter — toggle zoom
      if (paneMod && e.shiftKey && e.key === "Enter") {
        e.preventDefault();
        e.stopPropagation();
        onZoomPane(activeTabId);
        return;
      }
    };
    window.addEventListener("keydown", handler, true); // capture phase to beat xterm.js
    return () => window.removeEventListener("keydown", handler, true);
  }, [mode, openSearch, showSearch, closeSearch, tabs.length, activeTabId, onSplitPane, onClosePane, onNavigatePane, onCyclePane, onZoomPane]);

  // Sync search when active tab/pane changes
  useEffect(() => {
    if (showSearch) {
      if (searchTerm) doSearch(searchTerm);
      else closeSearch();
    }
  }, [activeTabId, activePaneId]);

  // When panel size, maximize state, visibility, or pane layout changes,
  // force every visible pane in the active tab to re-fit. Full-screen TUIs
  // such as Claude need all affected PTYs to receive the new cols/rows.
  useEffect(() => {
    let cancelled = false;
    requestAnimationFrame(() => {
      if (cancelled) return;
      requestAnimationFrame(() => {
        if (cancelled) return;
        if (!activeRootNode) return;
        for (const leaf of flattenPanes(activeRootNode)) {
          xtermHandlesRef.current.get(leaf.paneId)?.fit();
        }
      });
    });
    return () => { cancelled = true; };
  }, [size, maximized, visible, activeTabId, activePaneId, activeRootNode]);

  const isBottom = mode === "bottom";

  // When not visible, collapse the panel to zero — belt-and-suspenders:
  // height/maxHeight/minHeight all set to 0 to override any flex or child min-size.
  const containerStyle: React.CSSProperties = !visible
    ? (isBottom
      ? { height: 0, maxHeight: 0, minHeight: 0, overflow: "hidden", border: "none" }
      : { width: 0, maxWidth: 0, minWidth: 0, overflow: "hidden", border: "none" })
    : maximized
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
        ${maximized ? "flex-1" : "shrink-0"}
        ${!visible ? "shrink-0" : ""}`}
    >
      {/* Header */}
      <div className="flex items-center h-[32px] bg-[var(--app-terminal-header)] border-b border-app-border shrink-0 select-none">
        {/* Tab bar */}
        <div className="flex-1 flex items-center overflow-x-auto min-w-0">
          {tabs.map((tab) => {
            const isActive = tab.id === activeTabId;
            return (
              <div key={tab.id} onClick={() => onSwitchTab(tab.id)}
                onContextMenu={(e) => handleTabContextMenu(e, tab.id)}
                className={`group flex items-center gap-1.5 px-3 h-[32px] text-xs font-mono cursor-pointer
                  border-r border-app-border shrink-0 transition-colors select-none
                  ${isActive ? "bg-[var(--app-terminal-bg)] text-app-text border-b-[2px] border-b-app-accent -mb-px"
                    : "text-app-text-muted hover:text-app-text hover:bg-[var(--app-hover)]"}`}
              >
                <span className={`w-2 h-2 rounded-full shrink-0 ${tab.ptyRunning ? "bg-app-accent shadow-[0_0_4px_var(--app-glow)]" : "bg-app-text-muted opacity-40"}`} />
                <span className="max-w-[100px] truncate">{tab.name}</span>
                {tabs.length > 1 && (
                  <button onClick={(e) => { e.stopPropagation(); setClosingTabId(tab.id); }}
                    className={`ml-0.5 p-0.5 hover:bg-[var(--app-hover)] transition-colors shrink-0
                      ${isActive ? "" : "opacity-0 group-hover:opacity-100"}`} title="关闭">
                    <X size={12} />
                  </button>
                )}
              </div>
            );
          })}
          {/* Right panel: no manual "+" — only profile commands create tabs */}
          {isBottom && (
            <button onClick={onNewTab} className="px-2.5 h-[32px] text-app-text-muted hover:text-app-text hover:bg-[var(--app-hover)] transition-colors shrink-0" title="新建终端">
              <Plus size={14} />
            </button>
          )}
        </div>

        {/* History button — right panel only */}
        {!isBottom && (
        <div className="relative shrink-0">
          <button onClick={() => setShowHistory(!showHistory)}
            className={`px-2 h-[32px] text-app-text-muted hover:text-app-text transition-colors flex items-center gap-1 ${showHistory ? "text-app-accent" : ""}`}
            title="历史会话"
          >
            <Clock size={14} />
            {history.length > 0 && (
              <span className="text-xs tabular-nums">{history.length}</span>
            )}
          </button>

          {/* History dropdown */}
          {showHistory && (
            <>
              <div className="fixed inset-0 z-40" onClick={() => setShowHistory(false)} />
              <div className="absolute right-0 top-[32px] z-50 w-[380px] max-h-[400px] overflow-y-auto
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
                        <div className="text-2xs text-app-text-muted mt-0.5">{relativeTime(r.timestamp)}</div>
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
          className={`shrink-0 px-2 h-[32px] text-app-text-muted hover:text-app-text transition-colors ${showSearch ? "text-app-accent" : ""}`}
          title={`搜索终端输出 (${formatShortcut("mod+F")})`}
        >
          <Search size={14} />
        </button>

        {/* Theme selector */}
        <div className="relative shrink-0">
          <button
            onClick={() => setShowThemeMenu(!showThemeMenu)}
            className={`px-2 h-[32px] text-app-text-muted hover:text-app-text transition-colors ${showThemeMenu ? "text-app-accent" : ""}`}
            title="终端配色"
          >
            <Palette size={14} />
          </button>
          {showThemeMenu && (
            <>
              <div className="fixed inset-0 z-40" onClick={() => setShowThemeMenu(false)} />
              <div className="absolute right-0 top-[32px] z-50 w-[200px] bg-[var(--app-panel)] border border-app-border shadow-dialog py-0.5">
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
            className="px-1.5 h-[24px] text-xs text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)] transition-colors font-mono"
            title="缩小字体">A⁻</button>
          <span className="text-xs text-app-text-muted tabular-nums w-5 text-center">{fontSize}</span>
          <button onClick={() => onSetFontSize(fontSize + 1)}
            className="px-1.5 h-[24px] text-xs text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)] transition-colors font-mono"
            title="放大字体">A⁺</button>
          {/* Maximize / Restore */}
          {onToggleMaximize && (
            <button
              onClick={onToggleMaximize}
              className="p-0.5 text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)] transition-colors"
              title={maximizeTip}
            >
              {maximized ? <Minimize2 size={14} /> : <Maximize2 size={14} />}
            </button>
          )}
          {/* Work dir — right panel only */}
          {!isBottom && (
            <>
              <input type="text" value={activeTab?.workDir || ""}
                onChange={(e) => activeTabId && onSetWorkDir(activeTabId, e.target.value)}
                placeholder="工作目录"
                className="w-[160px] h-[24px] bg-[var(--app-input)] border border-app-border text-xs font-mono text-app-text-dim px-1.5 py-0 focus:border-app-accent"
                spellCheck={false} />
              <button onClick={browseDir} className="p-0.5 text-app-text-muted hover:text-app-accent transition-colors" title="选择目录">
                <FolderOpen size={13} />
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
            <X size={14} />
          </button>
        </div>
      </div>

      {/* Terminal area — all tabs mounted, inactive hidden via visibility (not display:none) */}
      <div
        className="flex-1 relative bg-[var(--app-terminal-bg)]"
        onMouseUp={(e) => {
          // After any click in the terminal area, refocus the active xterm
          // textarea so Cmd+W always reaches our keydown handler.
          // (Clicking header buttons steals focus from xterm, causing Cmd+W
          // to be intercepted by Tauri/macOS as "Close Window".)
          const target = e.target as HTMLElement;
          if (target.tagName === "INPUT" || target.tagName === "TEXTAREA") return;
          const handle = xtermHandlesRef.current.get(activePaneId);
          if (handle) {
            requestAnimationFrame(() => handle.focus());
          }
        }}
      >
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
              key={tab.id}
              className="absolute inset-0"
              style={{
                display: isActive ? undefined : "none",
              }}
            >
              <PaneSplitter
                node={tab.rootNode}
                activePaneId={tab.activePaneId}
                zoomedPaneId={tab.zoomedPaneId}
                fontSize={fontSize}
                themeName={themeName}
                onAttach={onAttachTerminal}
                onReady={onTerminalReady}
                onResize={onTerminalResize}
                onFocus={(paneId) => onFocusPane(tab.id, paneId)}
                onXTermHandle={handleXTermRef}
                onPaneContextMenu={(paneId, e) => handlePaneContextMenu(tab.id, paneId, e)}
              />
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

      {/* Pane context menu */}
      {paneCtxMenu && (() => {
        const tab = tabs.find(t => t.id === paneCtxMenu.tabId);
        const leafCount = tab ? flattenPanes(tab.rootNode).length : 1;
        const isZoomed = tab?.zoomedPaneId != null;

        return (
          <ContextMenu
            x={paneCtxMenu.x} y={paneCtxMenu.y}
            onClose={() => setPaneCtxMenu(null)}
            items={[
              {
                label: "水平分割 (左右)",
                icon: <SplitSquareVertical size={13} />,
                onClick: () => onSplitPane(paneCtxMenu.tabId, "horizontal", undefined, paneCtxMenu.paneId),
                disabled: isZoomed,
              },
              {
                label: "垂直分割 (上下)",
                icon: <SplitSquareHorizontal size={13} />,
                onClick: () => onSplitPane(paneCtxMenu.tabId, "vertical", undefined, paneCtxMenu.paneId),
                disabled: isZoomed,
              },
              { separator: true },
              {
                label: "关闭终端",
                icon: <X size={13} />,
                onClick: () => onClosePane(paneCtxMenu.tabId, paneCtxMenu.paneId),
                danger: true,
                disabled: leafCount <= 1,
              },
            ]}
          />
        );
      })()}

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
