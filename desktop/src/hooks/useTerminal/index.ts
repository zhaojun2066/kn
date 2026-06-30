/**
 * useTerminal — Multi-instance terminal hook.
 *
 * Split from a single 1001-line file into focused sub-hooks:
 *   state → ptyLifecycle → terminalReady → actions → commands → history → tabs → panes
 *
 * @param panelId - "right" (profile run) or "bottom" (manual toggle).
 */
import { useState, useRef, useCallback } from "react";
import type { Terminal } from "@xterm/xterm";
import { useTerminalState } from "./useTerminalState";
import { usePtyLifecycle } from "./usePtyLifecycle";
import { useTerminalReady } from "./useTerminalReady";
import { useTerminalActions } from "./useTerminalActions";
import { useSessionCommands } from "./useSessionCommands";
import { useSessionHistory } from "./useSessionHistory";
import { useTabManagement } from "./useTabManagement";
import { usePaneManagement } from "./usePaneManagement";
import type { TerminalContext } from "./context";

export function useTerminal(panelId: string = "right") {
  const isBottom = panelId === "bottom";

  // ── panel-specific configuration ──
  const MIN_SIZE = isBottom ? 120 : 480;
  const STORAGE_SIZE = `kn-terminal-${panelId}-size`;
  const STORAGE_HISTORY = `kn-terminal-${panelId}-history`;
  const STORAGE_FONTSIZE = `kn-terminal-${panelId}-fontsize`;

  // ── Terminal state (useState + localStorage) ──
  const state = useTerminalState(isBottom, STORAGE_SIZE, STORAGE_HISTORY, STORAGE_FONTSIZE, MIN_SIZE);

  const activeTabIdRef = useRef(state.activeTabId);
  activeTabIdRef.current = state.activeTabId;

  // ── Per-pane refs ──
  const termRefs = useRef<Map<string, Terminal>>(new Map());
  const writeBufRef = useRef<Map<string, string>>(new Map());
  const rafWriteRef = useRef<Map<string, number>>(new Map());
  const readyPaneIdsRef = useRef<Set<string>>(new Set());
  const childPidRef = useRef<Map<string, number>>(new Map());  // paneId → CLI PID
  const errorCallbackRef = useRef<((msg: string) => void) | null>(null);
  const openingRef = useRef(false);
  const readyPromiseRefs = useRef<Map<string, {
    resolve: () => void;
    timeout: ReturnType<typeof setTimeout>;
  }>>(new Map());

  // ── Build shared context ──
  const ctx: TerminalContext = {
    sessionsRef: state.sessionsRef,
    activeTabIdRef,
    termRefs,
    writeBufRef,
    rafWriteRef,
    readyPaneIdsRef,
    readyPromiseRefs,
    childPidRef,
    errorCallbackRef,
    openingRef,
    setTabs: state.setTabs,
    setIsOpen: state.setIsOpen,
    setActiveTabId: state.setActiveTabId,
    setHistory: state.setHistory,
    setUsageCounts: state.setUsageCounts,
    isBottom,
    STORAGE_HISTORY,
  };

  // ── PTY lifecycle ──
  const { spawnPty } = usePtyLifecycle(ctx);

  // ── Terminal ready / attach / resize ──
  const ready = useTerminalReady(ctx);

  // ── Terminal actions (open/close/toggle) ──
  const actions = useTerminalActions(ctx, state.isOpen, spawnPty, ready.waitForReady, ready.reportTerminalError);

  // ── Pane management (split/close/navigate/zoom) ──
  const panes = usePaneManagement(ctx, spawnPty, ready.waitForReady, ready.cleanupReadyWait, ready.reportTerminalError);

  // ── Session commands (run commands in terminal) ──
  const commands = useSessionCommands(
    ctx, state.isOpen, spawnPty, ready.waitForReady, ready.reportTerminalError,
    panes.splitPane, state.saveHistory,
  );

  // ── Session history ──
  const history = useSessionHistory(ctx, commands.runInNewTab, ready.validateProfile, STORAGE_HISTORY, state.saveHistory);

  // Wire up deleteHistoryRef for profile validation
  ready.deleteHistoryRef.current = history.deleteHistory;

  // ── Tab management ──
  const tabs = useTabManagement(ctx, isBottom, ready.cleanupReadyWait);

  // ── Font size / panel size ──
  const setFontSize = useCallback((s: number) => {
    const clamped = Math.min(Math.max(s, 10), 20);
    state.setFontSizeState(clamped);
    try { localStorage.setItem(STORAGE_FONTSIZE, String(clamped)); } catch { /* */ }
  }, [state.setFontSizeState, STORAGE_FONTSIZE]);

  const setSize = useCallback((s: number) => {
    const max = isBottom
      ? Math.floor(window.innerHeight * 0.6)
      : Math.floor(window.innerWidth * 0.65);
    const clamped = Math.min(Math.max(s, MIN_SIZE), max);
    state.setSizeState(clamped);
    try { localStorage.setItem(STORAGE_SIZE, String(clamped)); } catch { /* */ }
  }, [isBottom, MIN_SIZE, STORAGE_SIZE, state.setSizeState]);

  // ── Derived active tab ──
  const activeTab = state.tabs.find((t) => t.id === state.activeTabId) || state.tabs[0];

  // ── Public API (identical to original useTerminal return) ──
  return {
    isOpen: state.isOpen,
    size: state.size,
    isBottom,
    tabs: state.tabs,
    activeTabId: state.activeTabId,
    activeTab,
    history: state.history,
    setErrorCallback: ready.setErrorCallback,
    setValidProfileNames: ready.setValidProfileNames,
    usageCounts: state.usageCounts,
    open: actions.open,
    close: actions.close,
    hide: actions.hide,
    toggle: actions.toggle,
    attachTerminal: ready.attachTerminal,
    handleTerminalReady: ready.handleTerminalReady,
    handleTerminalResize: ready.handleTerminalResize,
    pasteCommand: commands.pasteCommand,
    runInSplitPane: commands.runInSplitPane,
    runInTerminal: commands.runInTerminal,
    runInNewTab: commands.runInNewTab,
    runProjectCommand: commands.runProjectCommand,
    newEmptyTab: actions.newEmptyTab,
    resumeSession: history.resumeSession,
    newSessionFromHistory: history.newSessionFromHistory,
    deleteHistory: history.deleteHistory,
    clearHistory: history.clearHistory,
    clearProfileHistory: history.clearProfileHistory,
    switchTab: tabs.switchTab,
    closeTab: tabs.closeTab,
    closeOthers: tabs.closeOthers,
    closeToRight: tabs.closeToRight,
    setWorkDir: panes.setWorkDir,
    setSize,
    fontSize: state.fontSize,
    setFontSize,
    splitPane: panes.splitPane,
    closePane: panes.closePane,
    focusPane: panes.focusPane,
    navigatePane: panes.navigatePane,
    cyclePane: panes.cyclePane,
    zoomPane: panes.zoomPane,
  };
}
