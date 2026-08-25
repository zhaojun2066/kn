/**
 * useTerminal — Multi-instance terminal hook.
 *
 * Split from a single 1001-line file into focused sub-hooks:
 *   state → ptyLifecycle → terminalReady → actions → commands → history → tabs → panes
 *
 * @param panelId - "right" (profile run) or "bottom" (manual toggle).
 */
import { useRef, useCallback, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Terminal } from "@xterm/xterm";
import type { AgentSession } from "../useAgent";
import { useTerminalState } from "./useTerminalState";
import { usePtyLifecycle } from "./usePtyLifecycle";
import { useTerminalReady } from "./useTerminalReady";
import { useTerminalActions } from "./useTerminalActions";
import { useSessionCommands } from "./useSessionCommands";
import { useSessionHistory } from "./useSessionHistory";
import { useTabManagement } from "./useTabManagement";
import { usePaneManagement } from "./usePaneManagement";
import type { TerminalContext } from "./context";
import {
  filterInitialVisibleAgentSessions,
  syncAgentSessionState,
  syncNativeAgentSessions,
} from "./agentSessionSync";
import { findLeaf, flattenPanes } from "../../lib/pane-types";

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
  const agentSessionsRef = useRef<AgentSession[]>([]);
  const startupAgentSessionCutoffMsRef = useRef(Date.now());
  const didInitialAgentSessionSyncRef = useRef(false);
  const dismissedAgentNidsRef = useRef<Set<string>>(new Set());
  const relayPollInFlightRef = useRef<Set<string>>(new Set());
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
    agentSessionsRef,
    dismissedAgentNidsRef,
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
  const { spawnPty, attachAgentPty } = usePtyLifecycle(ctx);

  // ── Terminal ready / attach / resize ──
  const ready = useTerminalReady(ctx);

  // ── Terminal actions (open/close/toggle) ──
  const actions = useTerminalActions(ctx, state.isOpen, spawnPty, ready.waitForReady, ready.reportTerminalError);

  // ── Pane management (split/close/navigate/zoom) ──
  const panes = usePaneManagement(ctx, spawnPty, ready.waitForReady, ready.cleanupReadyWait, ready.reportTerminalError);

  // ── Session commands (run commands in terminal) ──
  const commands = useSessionCommands(
    ctx, state.isOpen, spawnPty, attachAgentPty, ready.waitForReady, ready.reportTerminalError,
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

  const syncAgentSessions = useCallback((sessions: AgentSession[]) => {
    agentSessionsRef.current = sessions;
    if (!isBottom) {
      state.setTabs((prev) => syncAgentSessionState(prev, sessions));
    }
    if (!isBottom && !didInitialAgentSessionSyncRef.current && sessions.length > 0) {
      const visibleSessions = filterInitialVisibleAgentSessions(
        sessions,
        startupAgentSessionCutoffMsRef.current,
        dismissedAgentNidsRef.current,
      );
      if (visibleSessions.length === 0) return;
      didInitialAgentSessionSyncRef.current = true;
      state.setTabs((prev) => syncNativeAgentSessions(prev, visibleSessions));
    }
  }, [isBottom, state.setTabs]);

  useEffect(() => {
    if (isBottom) return;

    const timer = window.setInterval(() => {
      for (const tab of state.sessionsRef.current) {
        if (!tab.agentNid || relayPollInFlightRef.current.has(tab.agentNid)) continue;

        const leaves = flattenPanes(tab.rootNode);
        const leaf =
          leaves.find((item) => item.agentNid === tab.agentNid) ??
          (leaves.length === 1 ? findLeaf(tab.rootNode, tab.activePaneId) : null);
        if (!leaf?.ptyRunning || leaf.sessionId.startsWith("s_")) continue;

        relayPollInFlightRef.current.add(tab.agentNid);
        invoke<{
          inputs?: string[];
          ended?: boolean;
          cols?: number;
          rows?: number;
          viewport_owner?: string;
        }>("agent_ipc", {
          method: "poll_relay_input",
          params: { nid: tab.agentNid },
        }).then((result) => {
          if (result.ended) {
            invoke("kill_pty", { sessionId: leaf.sessionId }).catch(() => {});
            state.setTabs((prev) => {
              const next = prev.filter((t) => t.id !== tab.id);
              if (next.length === 0) {
                state.setIsOpen(false);
                state.setActiveTabId("");
              } else if (activeTabIdRef.current === tab.id) {
                state.setActiveTabId(next[0].id);
              }
              return next;
            });
            return;
          }

          for (const input of result.inputs || []) {
            invoke("write_pty", { sessionId: leaf.sessionId, data: input }).catch(() => {});
          }

          if (result.viewport_owner === "ios" && result.cols && result.rows) {
            invoke("resize_pty", {
              sessionId: leaf.sessionId,
              cols: result.cols,
              rows: result.rows,
            }).catch(() => {});
          }
        }).finally(() => {
          if (tab.agentNid) {
            relayPollInFlightRef.current.delete(tab.agentNid);
          }
        });
      }
    }, 250);

    return () => window.clearInterval(timer);
  }, [isBottom, state.sessionsRef, state.setTabs, state.setIsOpen, state.setActiveTabId]);

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
    setValidProfiles: ready.setValidProfiles,
    usageCounts: state.usageCounts,
    open: actions.open,
    close: actions.close,
    hide: actions.hide,
    toggle: actions.toggle,
    attachTerminal: ready.attachTerminal,
    insertText: ready.insertText,
    handleTerminalReady: ready.handleTerminalReady,
    handleTerminalResize: ready.handleTerminalResize,
    pasteCommand: commands.pasteCommand,
    runInSplitPane: commands.runInSplitPane,
    runInTerminal: commands.runInTerminal,
    runInNewTab: commands.runInNewTab,
    openRemoteSession: commands.openRemoteSession,
    syncAgentSessions,
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
