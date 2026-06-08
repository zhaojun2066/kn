import { useState, useRef, useCallback } from "react";
import { invoke, Channel } from "@tauri-apps/api/core";
import type { Terminal } from "@xterm/xterm";

const MAX_HISTORY = 30;
const PTY_READY_SETTLE_MS = 80;
const PTY_COMMAND_SETTLE_MS = 300;

let tabCounter = 1;

interface TabSession {
  id: string;
  name: string;
  workDir: string;
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

function newTab(name?: string, workDir?: string): TabSession {
  return {
    id: `tab-${tabCounter++}`,
    name: name || `终端 ${tabCounter - 1}`,
    workDir: workDir || "",
    sessionId: `pty-${Date.now().toString(36)}-${tabCounter}`,
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

  // Per-tab: Terminal instance
  const termRefs = useRef<Map<string, Terminal>>(new Map());

  // Per-tab write batching: accumulate data within a frame, flush once via RAF.
  // Prevents xterm.js parser overload when IPC delivers many small chunks rapidly.
  const writeBufRef = useRef<Map<string, string>>(new Map());
  const rafWriteRef = useRef<Map<string, number>>(new Map());

  const activeTabIdRef = useRef(activeTabId);
  activeTabIdRef.current = activeTabId;
  const sessionsRef = useRef(tabs);
  sessionsRef.current = tabs;

  const activeTab = tabs.find((t) => t.id === activeTabId) || tabs[0];

  /* ── Spawn PTY for a tab ───────────────────────────────── */
  const spawnPty = useCallback((tab: TabSession): Promise<void> => {
    writeBufRef.current.delete(tab.id);
    const rafId = rafWriteRef.current.get(tab.id);
    if (rafId) { cancelAnimationFrame(rafId); rafWriteRef.current.delete(tab.id); }

    return new Promise(async (resolve, reject) => {
      try { await invoke("kill_pty", { sessionId: tab.sessionId }); } catch { /* */ }

      const term = termRefs.current.get(tab.id);
      term?.clear();

      const channel = new Channel<PtyEvent>();
      channel.onmessage = (msg: PtyEvent) => {
        switch (msg.event) {
          case "ready":
            resolve();
            break;
          case "data": {
            // RAF-batched write: accumulate data, flush once per animation frame.
            // Prevents parser overload when PTY produces many small chunks rapidly
            // (e.g. Claude Code TUI streaming with ANSI escape sequences).
            const existing = writeBufRef.current.get(tab.id) || "";
            writeBufRef.current.set(tab.id, existing + msg.data);

            if (!rafWriteRef.current.has(tab.id)) {
              const rafId = requestAnimationFrame(() => {
                rafWriteRef.current.delete(tab.id);
                const data = writeBufRef.current.get(tab.id) || "";
                writeBufRef.current.set(tab.id, "");
                termRefs.current.get(tab.id)?.write(data);
              });
              rafWriteRef.current.set(tab.id, rafId);
            }
            break;
          }
          case "exit":
            // Flush any pending writes before showing exit message
            {
              const pending = writeBufRef.current.get(tab.id);
              if (pending) {
                termRefs.current.get(tab.id)?.write(pending);
                writeBufRef.current.set(tab.id, "");
              }
            }
            termRefs.current.get(tab.id)?.writeln(`\r\n\x1b[90m[exit: ${msg.data}]\x1b[0m`);
            setTabs((prev) => prev.map((s) => s.id === tab.id ? { ...s, ptyRunning: false } : s));
            break;
          case "error":
            termRefs.current.get(tab.id)?.writeln(`\r\n\x1b[31m[error: ${msg.data}]\x1b[0m`);
            break;
        }
      };

      try {
        // Use actual xterm viewport dimensions, not a hardcoded fallback.
        const t = termRefs.current.get(tab.id);
        const cols = t?.cols ?? 100;
        const rows = t?.rows ?? 30;
        await invoke("start_pty", {
          sessionId: tab.sessionId,
          workDir: tab.workDir || null,
          cols,
          rows,
          onEvent: channel,
        });
      } catch (e) {
        tab.ptyRunning = false;
        termRefs.current.get(tab.id)?.writeln(`\r\n\x1b[31m[无法启动终端: ${e}]\x1b[0m`);
        errorCallbackRef.current?.(`终端启动失败: ${e}`);
        reject(e);
      }
    });
  }, []);

  // Promise resolvers for onReady (tabId → resolve function)
  const errorCallbackRef = useRef<((msg: string) => void) | null>(null);
  const setErrorCallback = useCallback((cb: (msg: string) => void) => { errorCallbackRef.current = cb; }, []);

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
    // Profile gone — delete the stale record
    deleteHistoryRef.current?.(record.id);
    errorCallbackRef.current?.(`Profile "${parsed.profile}" 不存在，已删除历史记录`);
    return false;
  }, []);

  const readyPromiseRefs = useRef<Map<string, () => void>>(new Map());

  /* ── Handle terminal ready (fit completed) ─────────────── */
  const handleTerminalReady = useCallback((tabId: string) => {
    // After a fresh XTerm mount (e.g. font size change), the PTY session is
    // still running. The resize_pty call (triggered by fit → onResize) sends
    // SIGWINCH to the child process, which causes TUI apps like Claude Code
    // to redraw themselves. No text replay needed — raw ANSI sequences would
    // corrupt the fresh terminal display.

    const resolve = readyPromiseRefs.current.get(tabId);
    if (resolve) {
      readyPromiseRefs.current.delete(tabId);
      resolve();
    }
  }, []);

  /* ── Wait for terminal ready ───────────────────────────── */
  const waitForReady = useCallback((tabId: string): Promise<void> => {
    return new Promise((resolve) => {
      readyPromiseRefs.current.set(tabId, resolve);
    });
  }, []);

  /* ── Handle terminal resize (called from XTerm onFit) ──── */
  const handleTerminalResize = useCallback((tabId: string, cols: number, rows: number) => {
    const tab = tabs.find((t) => t.id === tabId);
    if (tab?.ptyRunning) {
      invoke("resize_pty", { sessionId: tab.sessionId, cols, rows }).catch(() => {});
    }
  }, [tabs]);

  /* ── Attach XTerm to a tab ─────────────────────────────── */
  const attachTerminal = useCallback((tabId: string, term: Terminal) => {
    termRefs.current.set(tabId, term);

    term.onData((data: string) => {
      const tab = sessionsRef.current.find((t) => t.id === tabId);
      if (tab?.ptyRunning) {
        invoke("write_pty", { sessionId: tab.sessionId, data }).catch(() => {});
      }
    });

    // PTY resize is handled via handleTerminalResize (called from XTerm's fit → onResize).
    // Removing the term.onResize handler here avoids duplicate TIOCSWINSZ calls:
    // fitAddon.fit() internally fires term.onResize, then fit() also calls onResize() —
    // both would invoke resize_pty back-to-back with the same dimensions.
  }, []);

  /* ── Create empty tab ──────────────────────────────────── */
  const newEmptyTab = useCallback(async () => {
    const tab = newTab("终端");
    setTabs((prev) => [...prev, tab]);
    setActiveTabId(tab.id);
    if (!isOpen) setIsOpen(true);
    // Wait for XTerm to mount then spawn PTY (panel may or may not be open yet).
    await waitForReady(tab.id);
    await new Promise((r) => setTimeout(r, PTY_READY_SETTLE_MS));
    await spawnPty(tab);
    setTabs((prev) => prev.map((t) => t.id === tab.id ? { ...t, ptyRunning: true } : t));
  }, [isOpen, spawnPty, waitForReady]);

  /* ── Open terminal panel ────────────────────────────────── */
  const open = useCallback(async () => {
    setIsOpen(true);
    const tab = tabs.find((t) => t.id === activeTabId);
    if (!tab || tab.ptyRunning) return;

    // Wait for XTerm to mount + first fit (onReady signal)
    await waitForReady(tab.id);
    // Brief delay for resize_pty to settle before spawning
    await new Promise((r) => setTimeout(r, PTY_READY_SETTLE_MS));

    await spawnPty(tab);
    setTabs((prev) => prev.map((t) => t.id === tab.id ? { ...t, ptyRunning: true } : t));
  }, [activeTabId, tabs, spawnPty, waitForReady]);

  /* ── Close ─────────────────────────────────────────────── */
  const close = useCallback(() => {
    // Capture current tabs for async cleanup (don't depend on stale closure)
    const currentTabs = sessionsRef.current;

    // 1. Update UI immediately — hide the panel first.
    //    This ensures the terminal closes instantly even if PTY
    //    operations are blocked (e.g. full ConPTY buffer on Windows).
    setIsOpen(false);

    // 2. Reset tabs state synchronously
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

    // 3. Clean up all refs
    termRefs.current.clear();
    writeBufRef.current.clear();
    for (const [, id] of rafWriteRef.current) { cancelAnimationFrame(id); }
    rafWriteRef.current.clear();

    // 4. Kill PTY sessions in background — fire-and-forget.
    //    Don't await: even if kill_pty blocks, the UI is already closed.
    for (const tab of currentTabs) {
      if (tab.ptyRunning) {
        invoke("kill_pty", { sessionId: tab.sessionId }).catch(() => {});
      }
    }
  }, [isBottom]);

  /* ── Hide without destroying ─────────────────────────── */
  const hide = useCallback(() => {
    setIsOpen(false);
  }, []);

  const openingRef = useRef(false);
  const toggle = useCallback(() => {
    if (isOpen) { hide(); }                       // soft hide — keep PTYs alive
    else if (!openingRef.current) {
      openingRef.current = true;
      open().finally(() => { openingRef.current = false; });
    }
  }, [isOpen, hide, open]);

  /* ── Create a new tab and run command ──────────────────── */
  const runInNewTab = useCallback(async (cmd: string, workDir: string, label?: string) => {
    const tab = newTab(label || cmd.slice(0, 20), workDir);
    setTabs((prev) => [...prev, tab]);
    setActiveTabId(tab.id);

    if (!isOpen) setIsOpen(true);

    // Wait for XTerm mount + first fit (onReady signal)
    await waitForReady(tab.id);
    // Brief delay for resize signal to settle
    await new Promise((r) => setTimeout(r, PTY_READY_SETTLE_MS));

    await spawnPty(tab);
    setTabs((prev) => prev.map((t) => t.id === tab.id ? { ...t, ptyRunning: true } : t));

    // Save to history — right panel only (bottom panel is manual workspace)
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
      // Persist using computed next value, not stale closure
      saveHistory(next);
      return next;
    });
    } // !isBottom — history only for right panel

    // Wait for shell prompt + resize signal to settle, then send command
    await new Promise((r) => setTimeout(r, PTY_COMMAND_SETTLE_MS));
    invoke("write_pty", {
      sessionId: tab.sessionId,
      data: cmd + "\r",
    }).catch(() => {});
  }, [isOpen, spawnPty, history, waitForReady, isBottom]);

  /* ── Open existing or create new tab ───────────────────── */
  const runInTerminal = useCallback(async (cmd: string, workDir: string) => {
    await runInNewTab(cmd, workDir, cmd.slice(0, 30));
  }, [runInNewTab]);

  /* ── Paste into active tab ─────────────────────────────── */
  const pasteCommand = useCallback(async (cmd: string): Promise<boolean> => {
    const tab = tabs.find((t) => t.id === activeTabId);
    if (!tab?.ptyRunning) return false;
    invoke("write_pty", { sessionId: tab.sessionId, data: cmd + "\r" }).catch(() => {});
    return true;
  }, [tabs, activeTabId]);

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

    // Update UI immediately, then kill PTY in background.
    // This prevents UI freeze if the PTY operation blocks (Windows ConPTY).
    termRefs.current.delete(tabId);
    writeBufRef.current.delete(tabId);
    const rafId = rafWriteRef.current.get(tabId);
    if (rafId) { cancelAnimationFrame(rafId); rafWriteRef.current.delete(tabId); }

    setTabs((prev) => {
      const next = prev.filter((t) => t.id !== tabId);
      if (next.length === 0 && isBottom) {
        return [newTab("终端")];
      }
      return next;
    });

    // Right panel: close when last tab removed
    const remaining = sessionsRef.current.filter((t) => t.id !== tabId);
    if (remaining.length === 0 && !isBottom) {
      setIsOpen(false);
    } else if (activeTabIdRef.current === tabId) {
      setActiveTabId(remaining[0]?.id || "");
    }

    // Kill PTY in background (fire-and-forget)
    if (tab?.ptyRunning) {
      invoke("kill_pty", { sessionId: tab.sessionId }).catch(() => {});
    }
  }, [isBottom]);

  /* ── Close all tabs except the specified one ───────────── */
  const closeOthers = useCallback((tabId: string) => {
    const allTabs = sessionsRef.current;
    // Kill PTYs in background, update UI immediately
    for (const tab of allTabs) {
      if (tab.id !== tabId && tab.ptyRunning) {
        invoke("kill_pty", { sessionId: tab.sessionId }).catch(() => {});
      }
      if (tab.id !== tabId) {
        termRefs.current.delete(tab.id);
        writeBufRef.current.delete(tab.id);
        const rid = rafWriteRef.current.get(tab.id);
        if (rid) { cancelAnimationFrame(rid); rafWriteRef.current.delete(tab.id); }
      }
    }
    const kept = allTabs.find((t) => t.id === tabId);
    setTabs(kept ? [kept] : (isBottom ? [newTab("终端")] : []));
    setActiveTabId(tabId);
  }, [isBottom]);

  /* ── Close tabs to the right of the specified one ──────── */
  const closeToRight = useCallback((tabId: string) => {
    const allTabs = sessionsRef.current;
    const idx = allTabs.findIndex((t) => t.id === tabId);
    if (idx < 0) return;
    const toClose = allTabs.slice(idx + 1);
    for (const tab of toClose) {
      if (tab.ptyRunning) {
        invoke("kill_pty", { sessionId: tab.sessionId }).catch(() => {});
      }
      termRefs.current.delete(tab.id);
      writeBufRef.current.delete(tab.id);
      const rid = rafWriteRef.current.get(tab.id);
      if (rid) { cancelAnimationFrame(rid); rafWriteRef.current.delete(tab.id); }
    }
    setTabs((prev) => prev.slice(0, idx + 1));
    // If active tab was among closed ones, switch to the right-clicked tab
    if (toClose.some((t) => t.id === activeTabIdRef.current)) {
      setActiveTabId(tabId);
    }
  }, []);

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
    setTabs((prev) => prev.map((t) => t.id === tabId ? { ...t, workDir: dir } : t));
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
  };
}
