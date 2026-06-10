import { useState, useRef, useCallback } from "react";
import { invoke, Channel } from "@tauri-apps/api/core";
import type { Terminal } from "@xterm/xterm";
import type { PaneNode, PaneLeaf, PaneSplit, SplitDirection, NavDirection } from "../lib/pane-types";
import {
  isLeaf, isSplit,
  flattenPanes, findLeaf, findParentSplit, replaceNode, firstLeaf,
  navigateFromLeaf, createInitialLeaf,
} from "../lib/pane-types";

const MAX_HISTORY = 30;
const PTY_READY_SETTLE_MS = 80;
const PTY_COMMAND_SETTLE_MS = 300;
const TERMINAL_READY_TIMEOUT_MS = 1500;

let tabCounter = 1;

interface TabSession {
  id: string;
  name: string;
  workDir: string;
  // Pane tree — the single source of truth for PTY sessions within this tab
  rootNode: PaneNode;
  activePaneId: string;
  zoomedPaneId: string | null;
  // Backward-compat convenience fields (synced from active pane)
  sessionId: string;
  ptyRunning: boolean;
}

export interface SessionRecord {
  id: string;
  command: string;
  resumeCommand: string | null;     // null if tool doesn't support resume
  resumeLastCommand: string | null; // resume most recent session directly
  workDir: string;
  label: string;
  tool: string | null;
  timestamp: number;
}

export function parseAiCmd(cmd: string): { tool: string; profile: string } | null {
  const m = cmd.match(/^ai\s+(claude|codex|qoderclicn)\s+(\S+)/);
  if (!m) return null;
  return { tool: m[1], profile: m[2] };
}

function buildResumeCmd(cmd: string): string | null {
  const parsed = parseAiCmd(cmd);
  if (!parsed) return null;
  if (parsed.tool === "claude") return `ai ${parsed.tool} ${parsed.profile} --resume`;
  if (parsed.tool === "codex") return `ai ${parsed.tool} ${parsed.profile} resume`;
  if (parsed.tool === "qoderclicn") return `ai ${parsed.tool} ${parsed.profile} -r`;
  return null;
}

function buildResumeLastCmd(cmd: string): string | null {
  const parsed = parseAiCmd(cmd);
  if (!parsed) return null;
  if (parsed.tool === "claude") return `ai ${parsed.tool} ${parsed.profile} -c`;
  if (parsed.tool === "codex") return `ai ${parsed.tool} ${parsed.profile} resume --last`;
  if (parsed.tool === "qoderclicn") return `ai ${parsed.tool} ${parsed.profile} -c`;
  return null;
}

type PtyEvent =
  | { event: "ready" }
  | { event: "data"; data: string }
  | { event: "exit"; data: number }
  | { event: "error"; data: string };

// ── Internal helpers ────────────────────────────────────────────────

/** Sync backward-compat fields (sessionId, ptyRunning) from the active pane */
function syncActivePaneFields(tab: TabSession): TabSession {
  const activeLeaf = findLeaf(tab.rootNode, tab.activePaneId);
  return {
    ...tab,
    sessionId: activeLeaf?.sessionId || "",
    ptyRunning: activeLeaf?.ptyRunning || false,
  };
}

/** Find a pane leaf across all tabs */
function findPaneInTabs(tabs: TabSession[], paneId: string): PaneLeaf | null {
  for (const tab of tabs) {
    const leaf = findLeaf(tab.rootNode, paneId);
    if (leaf) return leaf;
  }
  return null;
}

function newTab(name?: string, workDir?: string): TabSession {
  const tabId = `tab-${tabCounter++}`;
  const tabName = name || `终端 ${tabCounter - 1}`;
  const sessionId = `pty-${Date.now().toString(36)}-${tabCounter}`;
  const leaf = createInitialLeaf(tabName, workDir || "", sessionId);
  return {
    id: tabId,
    name: tabName,
    workDir: workDir || "",
    rootNode: leaf,
    activePaneId: leaf.paneId,
    zoomedPaneId: null,
    sessionId,
    ptyRunning: false,
  };
}

/**
 * Multi-instance terminal hook.
 * @param panelId - "right" (profile run) or "bottom" (manual toggle).
 *   "right" uses width-based sizing, "bottom" uses height-based sizing.
 */
export function useTerminal(panelId: string = "right") {
  const isBottom = panelId === "bottom";

  // ── panel-specific configuration ──────────────────────
  const MIN_SIZE = isBottom ? 120 : 480;
  const STORAGE_SIZE = `kn-terminal-${panelId}-size`;
  const STORAGE_HISTORY = `kn-terminal-${panelId}-history`;
  const STORAGE_FONTSIZE = `kn-terminal-${panelId}-fontsize`;

  function defaultSize(): number {
    try {
      const saved = localStorage.getItem(STORAGE_SIZE);
      if (saved) return Math.max(MIN_SIZE, parseInt(saved, 10));
    } catch { /* */ }
    if (isBottom) {
      return Math.max(MIN_SIZE, Math.floor(window.innerHeight * 0.3));
    }
    return Math.max(MIN_SIZE, Math.floor(window.innerWidth * 0.55));
  }

  function loadHistory(): SessionRecord[] {
    try {
      const raw = localStorage.getItem(STORAGE_HISTORY);
      if (!raw) return [];
      const records: SessionRecord[] = JSON.parse(raw);
      return records.map((r) => ({
        ...r,
        resumeLastCommand: r.resumeLastCommand || buildResumeLastCmd(r.command),
      }));
    } catch { return []; }
  }

  function saveHistory(records: SessionRecord[]) {
    try {
      localStorage.setItem(STORAGE_HISTORY, JSON.stringify(records.slice(0, MAX_HISTORY)));
    } catch { /* */ }
  }

  const [isOpen, setIsOpen] = useState(false);
  const [size, setSizeState] = useState(() => defaultSize());
  const [fontSize, setFontSizeState] = useState(() => {
    try { return parseInt(localStorage.getItem(STORAGE_FONTSIZE) || "13", 10); } catch { return 13; }
  });
  // Right panel starts empty (no default tab); bottom panel starts with one tab.
  const [tabs, setTabs] = useState<TabSession[]>(() => isBottom ? [newTab("终端")] : []);
  const [history, setHistory] = useState<SessionRecord[]>(() => loadHistory());
  const [activeTabId, setActiveTabId] = useState<string>(tabs[0]?.id || "");

  // Per-profile run counter (incremented on each "运行" click)
  const [usageCounts, setUsageCounts] = useState<Record<string, number>>({});

  // ── Per-pane refs (paneId-keyed, not tabId) ────────────────────
  const termRefs = useRef<Map<string, Terminal>>(new Map());

  // Per-pane write batching: accumulate data within a frame, flush once via RAF.
  const writeBufRef = useRef<Map<string, string>>(new Map());
  const rafWriteRef = useRef<Map<string, number>>(new Map());

  const activeTabIdRef = useRef(activeTabId);
  activeTabIdRef.current = activeTabId;
  const sessionsRef = useRef(tabs);
  sessionsRef.current = tabs;

  // Per-pane resize debounce timers — during drag, only the final size is sent to PTY
  const MIN_COLS = 5;
  const MIN_ROWS = 2;

  const activeTab = tabs.find((t) => t.id === activeTabId) || tabs[0];

  /* ── Spawn PTY for a pane ─────────────────────────────── */
  const spawnPty = useCallback((pane: PaneLeaf): Promise<void> => {
    writeBufRef.current.delete(pane.paneId);
    const rafId = rafWriteRef.current.get(pane.paneId);
    if (rafId) { cancelAnimationFrame(rafId); rafWriteRef.current.delete(pane.paneId); }

    return new Promise(async (resolve, reject) => {
      try { await invoke("kill_pty", { sessionId: pane.sessionId }); } catch { /* */ }

      const term = termRefs.current.get(pane.paneId);
      term?.clear();

      const channel = new Channel<PtyEvent>();
      channel.onmessage = (msg: PtyEvent) => {
        switch (msg.event) {
          case "ready":
            resolve();
            break;
          case "data": {
            const existing = writeBufRef.current.get(pane.paneId) || "";
            writeBufRef.current.set(pane.paneId, existing + msg.data);

            if (!rafWriteRef.current.has(pane.paneId)) {
              const rafId = requestAnimationFrame(() => {
                rafWriteRef.current.delete(pane.paneId);
                const data = writeBufRef.current.get(pane.paneId) || "";
                writeBufRef.current.set(pane.paneId, "");
                termRefs.current.get(pane.paneId)?.write(data);
              });
              rafWriteRef.current.set(pane.paneId, rafId);
            }
            break;
          }
          case "exit":
            {
              const pending = writeBufRef.current.get(pane.paneId);
              if (pending) {
                termRefs.current.get(pane.paneId)?.write(pending);
                writeBufRef.current.set(pane.paneId, "");
              }
            }
            termRefs.current.get(pane.paneId)?.writeln(`\r\n\x1b[90m[exit: ${msg.data}]\x1b[0m`);
            // Mark pane as stopped
            setTabs((prev) =>
              prev.map((tab) => {
                const leaf = findLeaf(tab.rootNode, pane.paneId);
                if (!leaf) return tab;
                const updatedLeaf: PaneLeaf = { ...leaf, ptyRunning: false };
                return syncActivePaneFields({
                  ...tab,
                  rootNode: replaceNode(tab.rootNode, pane.paneId, updatedLeaf),
                });
              }),
            );
            break;
          case "error":
            termRefs.current.get(pane.paneId)?.writeln(`\r\n\x1b[31m[error: ${msg.data}]\x1b[0m`);
            break;
        }
      };

      try {
        const t = termRefs.current.get(pane.paneId);
        const cols = t?.cols ?? 100;
        const rows = t?.rows ?? 30;
        await invoke("start_pty", {
          sessionId: pane.sessionId,
          workDir: pane.workDir || null,
          cols,
          rows,
          onEvent: channel,
        });
      } catch (e) {
        setTabs((prev) =>
          prev.map((tab) => {
            const leaf = findLeaf(tab.rootNode, pane.paneId);
            if (!leaf) return tab;
            const updatedLeaf: PaneLeaf = { ...leaf, ptyRunning: false };
            return syncActivePaneFields({
              ...tab,
              rootNode: replaceNode(tab.rootNode, pane.paneId, updatedLeaf),
            });
          }),
        );
        termRefs.current.get(pane.paneId)?.writeln(`\r\n\x1b[31m[无法启动终端: ${e}]\x1b[0m`);
        errorCallbackRef.current?.(`终端启动失败: ${e}`);
        reject(e);
      }
    });
  }, []);

  // Promise resolvers for onReady (paneId → resolve function)
  const errorCallbackRef = useRef<((msg: string) => void) | null>(null);
  const setErrorCallback = useCallback((cb: (msg: string) => void) => { errorCallbackRef.current = cb; }, []);
  const reportTerminalError = useCallback((action: string, error: unknown) => {
    errorCallbackRef.current?.(`${action}: ${error}`);
  }, []);

  // Valid profile names (for validating history restore)
  const profileNamesRef = useRef<Set<string>>(new Set());
  const setValidProfileNames = useCallback((names: string[]) => {
    profileNamesRef.current = new Set(names);
  }, []);
  const deleteHistoryRef = useRef<((id: string) => void) | null>(null);
  const validateProfile = useCallback((record: SessionRecord): boolean => {
    const parsed = parseAiCmd(record.command);
    if (!parsed) return true;
    if (profileNamesRef.current.has(parsed.profile)) return true;
    deleteHistoryRef.current?.(record.id);
    errorCallbackRef.current?.(`Profile "${parsed.profile}" 不存在，已删除历史记录`);
    return false;
  }, []);

  const readyPaneIdsRef = useRef<Set<string>>(new Set());
  const readyPromiseRefs = useRef<Map<string, {
    resolve: () => void;
    timeout: ReturnType<typeof setTimeout>;
  }>>(new Map());

  const cleanupReadyWait = useCallback((paneId: string) => {
    readyPaneIdsRef.current.delete(paneId);
    const pending = readyPromiseRefs.current.get(paneId);
    if (pending) {
      clearTimeout(pending.timeout);
      readyPromiseRefs.current.delete(paneId);
      pending.resolve();
    }
  }, []);

  /* ── Handle terminal ready (fit completed) ─────────────── */
  const handleTerminalReady = useCallback((paneId: string) => {
    readyPaneIdsRef.current.add(paneId);
    const pending = readyPromiseRefs.current.get(paneId);
    if (pending) {
      clearTimeout(pending.timeout);
      readyPromiseRefs.current.delete(paneId);
      pending.resolve();
    }
  }, []);

  /* ── Wait for terminal ready ───────────────────────────── */
  const waitForReady = useCallback((paneId: string): Promise<void> => {
    if (readyPaneIdsRef.current.has(paneId)) return Promise.resolve();

    return new Promise((resolve) => {
      const existing = readyPromiseRefs.current.get(paneId);
      if (existing) {
        clearTimeout(existing.timeout);
        existing.resolve();
      }

      const timeout = setTimeout(() => {
        readyPromiseRefs.current.delete(paneId);
        resolve();
      }, TERMINAL_READY_TIMEOUT_MS);

      readyPromiseRefs.current.set(paneId, { resolve, timeout });
    });
  }, []);

  /* ── Handle terminal resize (called from XTerm onFit) ──── */
  const handleTerminalResize = useCallback((paneId: string, cols: number, rows: number) => {
    // Skip tiny dimensions — they're intermediate states during drag
    if (cols < MIN_COLS || rows < MIN_ROWS) return;

    const leaf = findPaneInTabs(sessionsRef.current, paneId);
    if (leaf?.ptyRunning) {
      invoke("resize_pty", { sessionId: leaf.sessionId, cols, rows }).catch(() => {});
    }
  }, []);

  /* ── Attach XTerm to a pane ────────────────────────────── */
  const attachTerminal = useCallback((paneId: string, term: Terminal) => {
    termRefs.current.set(paneId, term);

    term.onData((data: string) => {
      const leaf = findPaneInTabs(sessionsRef.current, paneId);
      if (leaf?.ptyRunning) {
        invoke("write_pty", { sessionId: leaf.sessionId, data }).catch(() => {});
      }
    });
  }, []);

  /* ── Create empty tab ──────────────────────────────────── */
  const newEmptyTab = useCallback(async () => {
    try {
      const tab = newTab("终端");
      const activeLeaf = findLeaf(tab.rootNode, tab.activePaneId)!;
      setTabs((prev) => [...prev, tab]);
      setActiveTabId(tab.id);
      if (!isOpen) setIsOpen(true);
      await waitForReady(activeLeaf.paneId);
      await new Promise((r) => setTimeout(r, PTY_READY_SETTLE_MS));
      await spawnPty(activeLeaf);
      setTabs((prev) =>
        prev.map((t) => {
          if (t.id !== tab.id) return t;
          const updatedLeaf: PaneLeaf = { ...activeLeaf, ptyRunning: true };
          return syncActivePaneFields({ ...t, rootNode: replaceNode(t.rootNode, activeLeaf.paneId, updatedLeaf) });
        }),
      );
    } catch (e) {
      reportTerminalError("新建终端失败", e);
    }
  }, [isOpen, spawnPty, waitForReady, reportTerminalError]);

  /* ── Open terminal panel ────────────────────────────────── */
  const open = useCallback(async () => {
    try {
      setIsOpen(true);
      const tab = tabs.find((t) => t.id === activeTabId);
      if (!tab) return;
      const activeLeaf = findLeaf(tab.rootNode, tab.activePaneId);
      if (!activeLeaf || activeLeaf.ptyRunning) return;

      await waitForReady(activeLeaf.paneId);
      await new Promise((r) => setTimeout(r, PTY_READY_SETTLE_MS));

      await spawnPty(activeLeaf);
      setTabs((prev) =>
        prev.map((t) => {
          if (t.id !== tab.id) return t;
          const updatedLeaf: PaneLeaf = { ...activeLeaf, ptyRunning: true };
          return syncActivePaneFields({ ...t, rootNode: replaceNode(t.rootNode, activeLeaf.paneId, updatedLeaf) });
        }),
      );
    } catch (e) {
      reportTerminalError("打开终端失败", e);
    }
  }, [activeTabId, tabs, spawnPty, waitForReady, reportTerminalError]);

  /* ── Close ─────────────────────────────────────────────── */
  const close = useCallback(() => {
    const currentTabs = sessionsRef.current;

    setIsOpen(false);

    if (isBottom) {
      const fresh = newTab("终端");
      setTabs([fresh]);
      activeTabIdRef.current = fresh.id;
      setActiveTabId(fresh.id);
    } else {
      setTabs([]);
      activeTabIdRef.current = "";
      setActiveTabId("");
    }

    writeBufRef.current.clear();
    readyPaneIdsRef.current.clear();
    for (const [, pending] of readyPromiseRefs.current) {
      clearTimeout(pending.timeout);
      pending.resolve();
    }
    readyPromiseRefs.current.clear();
    termRefs.current.clear();
    for (const [, id] of rafWriteRef.current) { cancelAnimationFrame(id); }
    rafWriteRef.current.clear();

    // Kill all PTYs across all panes
    for (const tab of currentTabs) {
      for (const leaf of flattenPanes(tab.rootNode)) {
        if (leaf.ptyRunning) {
          invoke("kill_pty", { sessionId: leaf.sessionId }).catch(() => {});
        }
      }
    }
  }, [isBottom]);

  /* ── Hide without destroying ─────────────────────────── */
  const hide = useCallback(() => {
    setIsOpen(false);
  }, []);

  const openingRef = useRef(false);
  const toggle = useCallback(() => {
    if (isOpen) { hide(); }
    else if (!openingRef.current) {
      openingRef.current = true;
      open().finally(() => { openingRef.current = false; });
    }
  }, [isOpen, hide, open]);

  /* ── Create a new tab and run command ──────────────────── */
  const runInNewTab = useCallback(async (cmd: string, workDir: string, label?: string) => {
    try {
      const tab = newTab(label || cmd.slice(0, 20), workDir);
      const activeLeaf = findLeaf(tab.rootNode, tab.activePaneId)!;
      setTabs((prev) => [...prev, tab]);
      setActiveTabId(tab.id);

      if (!isOpen) setIsOpen(true);

      await waitForReady(activeLeaf.paneId);
      await new Promise((r) => setTimeout(r, PTY_READY_SETTLE_MS));

      await spawnPty(activeLeaf);
      setTabs((prev) =>
        prev.map((t) => {
          if (t.id !== tab.id) return t;
          const updatedLeaf: PaneLeaf = { ...activeLeaf, ptyRunning: true };
          return syncActivePaneFields({ ...t, rootNode: replaceNode(t.rootNode, activeLeaf.paneId, updatedLeaf) });
        }),
      );

      if (!isBottom) {
        const parsed = parseAiCmd(cmd);
        if (parsed) {
          setUsageCounts((prev) => ({ ...prev, [parsed.profile]: (prev[parsed.profile] || 0) + 1 }));
        }
        const record: SessionRecord = {
          id: Date.now().toString(36) + Math.random().toString(36).slice(2, 6),
          command: cmd,
          resumeCommand: buildResumeCmd(cmd),
          resumeLastCommand: buildResumeLastCmd(cmd),
          workDir,
          label: label || cmd,
          tool: parsed?.tool || null,
          timestamp: Date.now(),
        };
        setHistory((prev) => {
          const filtered = prev.filter((r) => !(r.command === cmd && r.workDir === workDir));
          const next = [record, ...filtered].slice(0, MAX_HISTORY);
          saveHistory(next);
          return next;
        });
      }

      await new Promise((r) => setTimeout(r, PTY_COMMAND_SETTLE_MS));
      invoke("write_pty", {
        sessionId: activeLeaf.sessionId,
        data: cmd + "\r",
      }).catch(() => {});
    } catch (e) {
      reportTerminalError("运行终端命令失败", e);
    }
  }, [isOpen, spawnPty, waitForReady, isBottom, reportTerminalError]);

  /* ── Open existing or create new tab ───────────────────── */
  const runInTerminal = useCallback(async (cmd: string, workDir: string) => {
    await runInNewTab(cmd, workDir, cmd.slice(0, 30));
  }, [runInNewTab]);

  /* ── Paste into active pane ────────────────────────────── */
  const pasteCommand = useCallback(async (cmd: string): Promise<boolean> => {
    const tab = sessionsRef.current.find((t) => t.id === activeTabIdRef.current);
    if (!tab) return false;
    const activeLeaf = findLeaf(tab.rootNode, tab.activePaneId);
    if (!activeLeaf?.ptyRunning) return false;
    invoke("write_pty", { sessionId: activeLeaf.sessionId, data: cmd + "\r" }).catch(() => {});
    return true;
  }, []);

  /* ── Resume from history ───────────────────────────────── */
  const resumeSession = useCallback(async (record: SessionRecord) => {
    if (!validateProfile(record)) return;
    const cmd = record.resumeCommand || record.command;
    const label = record.resumeCommand
      ? `${record.label} · 恢复`
      : record.label;
    await runInNewTab(cmd, record.workDir, label);
  }, [runInNewTab, validateProfile]);

  /* ── New session from history (no resume) ──────────────── */
  const newSessionFromHistory = useCallback(async (record: SessionRecord) => {
    if (!validateProfile(record)) return;
    await runInNewTab(record.command, record.workDir, record.label);
  }, [runInNewTab, validateProfile]);

  /* ── Delete history entry ──────────────────────────────── */
  const deleteHistory = useCallback((id: string) => {
    setHistory((prev) => {
      const next = prev.filter((r) => r.id !== id);
      saveHistory(next);
      return next;
    });
  }, []);
  deleteHistoryRef.current = deleteHistory;

  /* ── Clear all history ─────────────────────────────────── */
  const clearHistory = useCallback(() => {
    setHistory([]);
    try { localStorage.removeItem(STORAGE_HISTORY); } catch { /* */ }
  }, [STORAGE_HISTORY]);

  /* ── Clear history for a specific profile ─────────────── */
  const clearProfileHistory = useCallback((profileName: string) => {
    setHistory((prev) => {
      const next = prev.filter((r) => {
        const parsed = parseAiCmd(r.command);
        return parsed?.profile !== profileName;
      });
      saveHistory(next);
      return next;
    });
  }, []);

  /* ── Switch tab ────────────────────────────────────────── */
  const switchTab = useCallback((tabId: string) => {
    setActiveTabId(tabId);
  }, []);

  /* ── Close a tab ───────────────────────────────────────── */
  const closeTab = useCallback((tabId: string) => {
    const tab = sessionsRef.current.find((t) => t.id === tabId);

    // Clean up all pane refs for this tab
    if (tab) {
      for (const leaf of flattenPanes(tab.rootNode)) {
        termRefs.current.delete(leaf.paneId);
        writeBufRef.current.delete(leaf.paneId);
        cleanupReadyWait(leaf.paneId);
        const rafId = rafWriteRef.current.get(leaf.paneId);
        if (rafId) { cancelAnimationFrame(rafId); rafWriteRef.current.delete(leaf.paneId); }
      }
    }

    setTabs((prev) => {
      const next = prev.filter((t) => t.id !== tabId);
      if (next.length === 0 && isBottom) {
        const fresh = newTab("终端");
        activeTabIdRef.current = fresh.id;
        setActiveTabId(fresh.id);
        return [fresh];
      }

      if (next.length === 0 && !isBottom) {
        setIsOpen(false);
      } else if (activeTabIdRef.current === tabId) {
        setActiveTabId(next[0]?.id || "");
      }
      return next;
    });

    // Kill PTYs for all panes in this tab
    if (tab) {
      for (const leaf of flattenPanes(tab.rootNode)) {
        if (leaf.ptyRunning) {
          invoke("kill_pty", { sessionId: leaf.sessionId }).catch(() => {});
        }
      }
    }
  }, [isBottom, cleanupReadyWait]);

  /* ── Close all tabs except the specified one ───────────── */
  const closeOthers = useCallback((tabId: string) => {
    const allTabs = sessionsRef.current;
    for (const tab of allTabs) {
      if (tab.id === tabId) continue;
      for (const leaf of flattenPanes(tab.rootNode)) {
        if (leaf.ptyRunning) {
          invoke("kill_pty", { sessionId: leaf.sessionId }).catch(() => {});
        }
        termRefs.current.delete(leaf.paneId);
        writeBufRef.current.delete(leaf.paneId);
        cleanupReadyWait(leaf.paneId);
        const rid = rafWriteRef.current.get(leaf.paneId);
        if (rid) { cancelAnimationFrame(rid); rafWriteRef.current.delete(leaf.paneId); }
      }
    }
    const kept = allTabs.find((t) => t.id === tabId);
    setTabs(kept ? [kept] : (isBottom ? [newTab("终端")] : []));
    setActiveTabId(tabId);
  }, [isBottom, cleanupReadyWait]);

  /* ── Close tabs to the right of the specified one ──────── */
  const closeToRight = useCallback((tabId: string) => {
    const allTabs = sessionsRef.current;
    const idx = allTabs.findIndex((t) => t.id === tabId);
    if (idx < 0) return;
    const toClose = allTabs.slice(idx + 1);
    for (const tab of toClose) {
      for (const leaf of flattenPanes(tab.rootNode)) {
        if (leaf.ptyRunning) {
          invoke("kill_pty", { sessionId: leaf.sessionId }).catch(() => {});
        }
        termRefs.current.delete(leaf.paneId);
        writeBufRef.current.delete(leaf.paneId);
        cleanupReadyWait(leaf.paneId);
        const rid = rafWriteRef.current.get(leaf.paneId);
        if (rid) { cancelAnimationFrame(rid); rafWriteRef.current.delete(leaf.paneId); }
      }
    }
    setTabs((prev) => prev.slice(0, idx + 1));
    if (toClose.some((t) => t.id === activeTabIdRef.current)) {
      setActiveTabId(tabId);
    }
  }, [cleanupReadyWait]);

  // ═══════════════════════════════════════════════════════════════
  //  NEW: Pane split/close/navigate/zoom operations
  // ═══════════════════════════════════════════════════════════════

  /** Split the given pane (defaults to active) in the given tab. Returns the new pane's sessionId. */
  const splitPane = useCallback(async (
    tabId: string,
    direction: SplitDirection,
    workDir?: string,
    paneId?: string,
  ): Promise<string | undefined> => {
    try {
      // Read current state to create the new leaf
      const tab = sessionsRef.current.find((t) => t.id === tabId);
      if (!tab || tab.zoomedPaneId) return;

      const targetPaneId = paneId ?? tab.activePaneId;
      const targetLeaf = findLeaf(tab.rootNode, targetPaneId);
      if (!targetLeaf) return;

      // Create a new leaf for the split
      const newSessionId = `pty-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 6)}`;
      const newLeaf = createInitialLeaf(tab.name, workDir || targetLeaf.workDir, newSessionId);

      // Create a split node replacing the target leaf
      const splitId = `split-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 6)}`;
      const split: PaneSplit = {
        type: "split",
        id: splitId,
        direction,
        ratio: 0.5,
        children: [targetLeaf, newLeaf],
      };

      const readyPromise = waitForReady(newLeaf.paneId);

      setTabs((prev) =>
        prev.map((t) => {
          if (t.id !== tabId) return t;
          return syncActivePaneFields({
            ...t,
            rootNode: replaceNode(t.rootNode, targetLeaf.paneId, split),
            activePaneId: newLeaf.paneId, // focus the new pane
          });
        }),
      );

      await readyPromise;
      await new Promise((r) => setTimeout(r, PTY_READY_SETTLE_MS));

      // Spawn after the xterm has mounted and fitted, so the PTY starts with
      // the pane's real dimensions instead of the fallback 100x30.
      await spawnPty(newLeaf);
      setTabs((prev) =>
        prev.map((t) => {
          if (t.id !== tabId) return t;
          const leaf = findLeaf(t.rootNode, newLeaf.paneId);
          if (!leaf) return t;
          const updatedLeaf: PaneLeaf = { ...leaf, ptyRunning: true };
          return syncActivePaneFields({ ...t, rootNode: replaceNode(t.rootNode, newLeaf.paneId, updatedLeaf) });
        }),
      );

      const term = termRefs.current.get(newLeaf.paneId);
      if (term && term.cols > 0 && term.rows > 0) {
        invoke("resize_pty", {
          sessionId: newSessionId,
          cols: term.cols,
          rows: term.rows,
        }).catch(() => {});
      }

      return newSessionId;
    } catch (e) {
      reportTerminalError("分屏终端失败", e);
      return undefined;
    }
  }, [spawnPty, waitForReady, reportTerminalError]);

  /* ── Split active pane and run command in the new pane ── */
  const runInSplitPane = useCallback(async (cmd: string, workDir: string, label?: string) => {
    try {
      const tabId = activeTabIdRef.current;
      const tab = sessionsRef.current.find((t) => t.id === tabId);

      if (!tab || !tabId) {
        // No active tab — fall back to new tab
        await runInNewTab(cmd, workDir, label);
        return;
      }

      if (!isOpen) setIsOpen(true);

      // splitPane returns the new pane's sessionId directly, spawns PTY internally
      const newSessionId = await splitPane(tabId, "horizontal", workDir);
      if (!newSessionId) return;

      // Wait for PTY to settle, then write the command directly (same as runInNewTab)
      await new Promise((r) => setTimeout(r, PTY_COMMAND_SETTLE_MS));
      invoke("write_pty", {
        sessionId: newSessionId,
        data: cmd + "\r",
      }).catch(() => {});

      // Record history + increment count (same as runInNewTab's non-bottom logic)
      if (!isBottom) {
        const parsed = parseAiCmd(cmd);
        if (parsed) {
          setUsageCounts((prev) => ({ ...prev, [parsed.profile]: (prev[parsed.profile] || 0) + 1 }));
        }
        const record: SessionRecord = {
          id: Date.now().toString(36) + Math.random().toString(36).slice(2, 6),
          command: cmd,
          resumeCommand: buildResumeCmd(cmd),
          resumeLastCommand: buildResumeLastCmd(cmd),
          workDir,
          label: label || cmd,
          tool: parsed?.tool || null,
          timestamp: Date.now(),
        };
        setHistory((prev) => {
          const filtered = prev.filter((r) => !(r.command === cmd && r.workDir === workDir));
          const next = [record, ...filtered].slice(0, MAX_HISTORY);
          saveHistory(next);
          return next;
        });
      }
    } catch (e) {
      reportTerminalError("分屏运行命令失败", e);
    }
  }, [isOpen, isBottom, splitPane, runInNewTab, reportTerminalError]);

  /** Close the given pane (defaults to active) in the given tab. Falls back to closeTab for last pane. */
  const closePane = useCallback((tabId: string, paneId?: string) => {
    const tab = sessionsRef.current.find((t) => t.id === tabId);
    if (!tab) return;

    const leaves = flattenPanes(tab.rootNode);
    if (leaves.length <= 1) {
      // Last pane — can't close, show a hint
      errorCallbackRef.current?.("至少保留一个终端");
      return;
    }

    const targetPaneId = paneId ?? tab.activePaneId;
    const targetLeaf = findLeaf(tab.rootNode, targetPaneId);
    if (!targetLeaf) return;

    // Kill the PTY for this pane
    if (targetLeaf.ptyRunning) {
      invoke("kill_pty", { sessionId: targetLeaf.sessionId }).catch(() => {});
    }
    termRefs.current.delete(targetLeaf.paneId);
    writeBufRef.current.delete(targetLeaf.paneId);
    cleanupReadyWait(targetLeaf.paneId);
    const rafId = rafWriteRef.current.get(targetLeaf.paneId);
    if (rafId) { cancelAnimationFrame(rafId); rafWriteRef.current.delete(targetLeaf.paneId); }

    // Remove the pane from the tree: find parent split, replace with the sibling
    const parentInfo = findParentSplit(tab.rootNode, targetLeaf.paneId);
    let newRoot: PaneNode;
    let newFocusId: string;

    if (parentInfo) {
      const sibling = parentInfo.parent.children[parentInfo.index === 0 ? 1 : 0];
      newRoot = replaceNode(tab.rootNode, parentInfo.parent.id, sibling);
      newFocusId = firstLeaf(sibling)?.paneId || tab.activePaneId;
    } else {
      // Shouldn't happen, but fallback
      newRoot = tab.rootNode;
      newFocusId = leaves.find((l) => l.paneId !== targetLeaf.paneId)?.paneId || "";
    }

    // If the zoomed pane is being closed, clear zoom state so PaneSplitter
    // doesn't render a stale zoomedPaneId that no longer exists in the tree.
    const shouldClearZoom = tab.zoomedPaneId && tab.zoomedPaneId === targetLeaf.paneId;

    setTabs((prev) =>
      prev.map((t) => {
        if (t.id !== tabId) return t;
        return syncActivePaneFields({
          ...t,
          rootNode: newRoot,
          activePaneId: newFocusId,
          zoomedPaneId: shouldClearZoom ? null : t.zoomedPaneId,
        });
      }),
    );
  }, [cleanupReadyWait]);

  /** Focus a specific pane */
  const focusPane = useCallback((tabId: string, paneId: string) => {
    setTabs((prev) =>
      prev.map((t) => {
        if (t.id !== tabId || t.activePaneId === paneId) return t;
        return syncActivePaneFields({ ...t, activePaneId: paneId });
      }),
    );
  }, []);

  /** Navigate to adjacent pane by cardinal direction */
  const navigatePane = useCallback((tabId: string, direction: NavDirection) => {
    setTabs((prev) => {
      const tab = prev.find((t) => t.id === tabId);
      if (!tab) return prev;

      const target = navigateFromLeaf(tab.rootNode, tab.activePaneId, direction);
      if (!target) return prev;

      return prev.map((t) => {
        if (t.id !== tabId) return t;
        return syncActivePaneFields({ ...t, activePaneId: target.paneId });
      });
    });
  }, []);

  /** Cycle through panes in tab order */
  const cyclePane = useCallback((tabId: string, forward: boolean) => {
    setTabs((prev) => {
      const tab = prev.find((t) => t.id === tabId);
      if (!tab) return prev;

      const leaves = flattenPanes(tab.rootNode);
      if (leaves.length <= 1) return prev;

      const idx = leaves.findIndex((l) => l.paneId === tab.activePaneId);
      const nextIdx = forward
        ? (idx + 1) % leaves.length
        : (idx - 1 + leaves.length) % leaves.length;

      return prev.map((t) => {
        if (t.id !== tabId) return t;
        return syncActivePaneFields({ ...t, activePaneId: leaves[nextIdx].paneId });
      });
    });
  }, []);

  /** Toggle zoom for the active pane */
  const zoomPane = useCallback((tabId: string) => {
    setTabs((prev) =>
      prev.map((t) => {
        if (t.id !== tabId) return t;
        const newZoomed = t.zoomedPaneId ? null : t.activePaneId;
        return syncActivePaneFields({ ...t, zoomedPaneId: newZoomed });
      }),
    );
  }, []);

  // ── Font size / panel size / work dir ──────────────────────

  const setFontSize = useCallback((s: number) => {
    const clamped = Math.min(Math.max(s, 10), 20);
    setFontSizeState(clamped);
    try { localStorage.setItem(STORAGE_FONTSIZE, String(clamped)); } catch { /* */ }
  }, []);

  const setSize = useCallback((s: number) => {
    const max = isBottom
      ? Math.floor(window.innerHeight * 0.6)
      : Math.floor(window.innerWidth * 0.65);
    const clamped = Math.min(Math.max(s, MIN_SIZE), max);
    setSizeState(clamped);
    try { localStorage.setItem(STORAGE_SIZE, String(clamped)); } catch { /* */ }
  }, [isBottom, MIN_SIZE, STORAGE_SIZE]);

  const setWorkDir = useCallback((tabId: string, dir: string) => {
    setTabs((prev) =>
      prev.map((t) => {
        if (t.id !== tabId) return t;
        // Update workDir on tab AND on the active pane leaf
        const activeLeaf = findLeaf(t.rootNode, t.activePaneId);
        if (!activeLeaf) return { ...t, workDir: dir };
        const updatedLeaf: PaneLeaf = { ...activeLeaf, workDir: dir };
        return syncActivePaneFields({
          ...t,
          workDir: dir,
          rootNode: replaceNode(t.rootNode, activeLeaf.paneId, updatedLeaf),
        });
      }),
    );
  }, []);

  return {
    isOpen,
    size,
    isBottom,
    tabs, activeTabId, activeTab,
    history,
    setErrorCallback,
    setValidProfileNames,
    usageCounts,
    open, close, hide, toggle,
    attachTerminal,
    handleTerminalReady,
    handleTerminalResize,
    pasteCommand,
    runInSplitPane,
    runInTerminal,
    runInNewTab,
    newEmptyTab,
    resumeSession,
    newSessionFromHistory,
    deleteHistory,
    clearHistory,
    clearProfileHistory,
    switchTab, closeTab, closeOthers, closeToRight,
    setWorkDir,
    setSize,
    fontSize, setFontSize,
    // New pane operations
    splitPane,
    closePane,
    focusPane,
    navigatePane,
    cyclePane,
    zoomPane,
  };
}
