import { useState, useRef, useCallback, useEffect } from "react";
import { invoke, Channel } from "@tauri-apps/api/core";
import type { Terminal } from "@xterm/xterm";

const MIN_WIDTH = 480;
const STORAGE_WIDTH = "kn-terminal-width";

function defaultWidth(): number {
  try {
    const saved = localStorage.getItem(STORAGE_WIDTH);
    if (saved) return Math.max(MIN_WIDTH, parseInt(saved, 10));
  } catch { /* */ }
  return Math.max(MIN_WIDTH, Math.floor(window.innerWidth * 0.55));
}
const MAX_HISTORY = 30;
const STORAGE_HISTORY = "kn-terminal-history";

let tabCounter = 1;

interface TabSession {
  id: string;
  name: string;
  workDir: string;
  sessionId: string;
  lastText: string;
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

function parseAiCmd(cmd: string): { tool: string; profile: string } | null {
  const m = cmd.match(/^ai\s+(claude|codex)\s+(\S+)/);
  if (!m) return null;
  return { tool: m[1], profile: m[2] };
}

function buildResumeCmd(cmd: string): string | null {
  const parsed = parseAiCmd(cmd);
  if (!parsed) return null;
  if (parsed.tool === "claude") return `ai ${parsed.tool} ${parsed.profile} --resume`;
  if (parsed.tool === "codex") return `ai ${parsed.tool} ${parsed.profile} resume`;
  return null;
}

function buildResumeLastCmd(cmd: string): string | null {
  const parsed = parseAiCmd(cmd);
  if (!parsed) return null;
  if (parsed.tool === "claude") return `ai ${parsed.tool} ${parsed.profile} -c`;
  if (parsed.tool === "codex") return `ai ${parsed.tool} ${parsed.profile} resume --last`;
  return null;
}

function loadHistory(): SessionRecord[] {
  try {
    const raw = localStorage.getItem(STORAGE_HISTORY);
    if (!raw) return [];
    const records: SessionRecord[] = JSON.parse(raw);
    // Migrate: fill missing resumeLastCommand for old records
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
    lastText: "",
    ptyRunning: false,
  };
}

export function useTerminal() {
  const [isOpen, setIsOpen] = useState(false);
  const [width, setWidth] = useState(() => defaultWidth());
  const [fontSize, setFontSizeState] = useState(() => {
    try { return parseInt(localStorage.getItem("kn-terminal-fontsize") || "13", 10); } catch { return 13; }
  });
  const [tabs, setTabs] = useState<TabSession[]>(() => [newTab("终端")]);
  const [history, setHistory] = useState<SessionRecord[]>(() => loadHistory());
  const [activeTabId, setActiveTabId] = useState<string>(tabs[0]?.id || "");

  // Per-tab: Terminal instance + text
  const termRefs = useRef<Map<string, Terminal>>(new Map());
  const textRefs = useRef<Map<string, string>>(new Map());

  // Track which tabs have mounted XTerm
  const mountedTabs = useRef<Set<string>>(new Set());
  const activeTabIdRef = useRef(activeTabId);
  activeTabIdRef.current = activeTabId;
  const sessionsRef = useRef(tabs);
  sessionsRef.current = tabs;

  const activeTab = tabs.find((t) => t.id === activeTabId) || tabs[0];

  /* ── Spawn PTY for a tab ───────────────────────────────── */
  const spawnPty = useCallback((tab: TabSession): Promise<void> => {
    textRefs.current.set(tab.id, "");

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
            const t = termRefs.current.get(tab.id);
            t?.write(msg.data);
            const cur = textRefs.current.get(tab.id) || "";
            textRefs.current.set(tab.id, (cur + msg.data).slice(-100000));
            break;
          }
          case "exit":
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

  const readyPromiseRefs = useRef<Map<string, () => void>>(new Map());

  /* ── Handle terminal ready (fit completed) ─────────────── */
  const handleTerminalReady = useCallback((tabId: string) => {
    // Replay saved text if any
    const saved = textRefs.current.get(tabId) || "";
    const term = termRefs.current.get(tabId);
    if (saved && term) {
      term.write(saved);
    }

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
    await new Promise((r) => setTimeout(r, 200));
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
    await new Promise((r) => setTimeout(r, 200));

    await spawnPty(tab);
    setTabs((prev) => prev.map((t) => t.id === tab.id ? { ...t, ptyRunning: true } : t));
  }, [activeTabId, tabs, spawnPty, waitForReady]);

  /* ── Close ─────────────────────────────────────────────── */
  const close = useCallback(async () => {
    for (const tab of tabs) {
      if (tab.ptyRunning) {
        try { await invoke("kill_pty", { sessionId: tab.sessionId }); } catch { /* */ }
      }
    }
    // Clean up all refs
    termRefs.current.clear();
    textRefs.current.clear();
    mountedTabs.current.clear();
    // Reset to a single fresh tab (old tabs are dead)
    const fresh = newTab("终端");
    setTabs([fresh]);
    activeTabIdRef.current = fresh.id;
    setActiveTabId(fresh.id);
    setIsOpen(false);
  }, [tabs]);

  const openingRef = useRef(false);
  const toggle = useCallback(() => {
    if (isOpen) { close(); }
    else if (!openingRef.current) {
      openingRef.current = true;
      open().finally(() => { openingRef.current = false; });
    }
  }, [isOpen, close, open]);

  /* ── Create a new tab and run command ──────────────────── */
  const runInNewTab = useCallback(async (cmd: string, workDir: string, label?: string) => {
    const tab = newTab(label || cmd.slice(0, 20), workDir);
    setTabs((prev) => [...prev, tab]);
    setActiveTabId(tab.id);

    if (!isOpen) setIsOpen(true);

    // Wait for XTerm mount + first fit (onReady signal)
    await waitForReady(tab.id);
    // Brief delay for resize signal to settle
    await new Promise((r) => setTimeout(r, 200));

    await spawnPty(tab);
    setTabs((prev) => prev.map((t) => t.id === tab.id ? { ...t, ptyRunning: true } : t));

    // Save to history
    const parsed = parseAiCmd(cmd);
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
      return [record, ...filtered].slice(0, MAX_HISTORY);
    });
    // Persist outside updater
    saveHistory([record, ...history.filter((r) => !(r.command === cmd && r.workDir === workDir))].slice(0, MAX_HISTORY));

    // Wait for shell prompt + resize signal to settle, then send command
    await new Promise((r) => setTimeout(r, 800));
    invoke("write_pty", {
      sessionId: tab.sessionId,
      data: cmd + "\r",
    }).catch(() => {});
  }, [isOpen, spawnPty]);

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
    const cmd = record.resumeCommand || record.command;
    const label = record.resumeCommand
      ? `${record.label} · 恢复`
      : record.label;
    await runInNewTab(cmd, record.workDir, label);
  }, [runInNewTab]);

  /* ── New session from history (no resume) ──────────────── */
  const newSessionFromHistory = useCallback(async (record: SessionRecord) => {
    await runInNewTab(record.command, record.workDir, record.label);
  }, [runInNewTab]);

  /* ── Delete history entry ──────────────────────────────── */
  const deleteHistory = useCallback((id: string) => {
    setHistory((prev) => {
      const next = prev.filter((r) => r.id !== id);
      saveHistory(next);
      return next;
    });
  }, []);

  /* ── Clear all history ─────────────────────────────────── */
  const clearHistory = useCallback(() => {
    setHistory([]);
    try { localStorage.removeItem(STORAGE_HISTORY); } catch { /* */ }
  }, []);

  /* ── Switch tab ────────────────────────────────────────── */
  const switchTab = useCallback((tabId: string) => {
    setActiveTabId(tabId);
  }, []);

  /* ── Close a tab ───────────────────────────────────────── */
  const closeTab = useCallback(async (tabId: string) => {
    const tab = sessionsRef.current.find((t) => t.id === tabId);
    if (tab?.ptyRunning) {
      try { await invoke("kill_pty", { sessionId: tab.sessionId }); } catch { /* */ }
    }
    termRefs.current.delete(tabId);
    textRefs.current.delete(tabId);
    mountedTabs.current.delete(tabId);

    setTabs((prev) => {
      const next = prev.filter((t) => t.id !== tabId);
      if (next.length === 0) {
        const nt = newTab("终端");
        return [nt];
      }
      return next;
    });

    if (activeTabIdRef.current === tabId) {
      setActiveTabId(() => {
        const remaining = sessionsRef.current.filter((t) => t.id !== tabId);
        return remaining[0]?.id || "";
      });
    }
  }, [tabs, activeTabId]);

  const [terminalVersion, setTerminalVersion] = useState(0);
  const setFontSize = useCallback((s: number) => {
    const clamped = Math.min(Math.max(s, 10), 20);
    // Save active tab text before remount
    const activeId = activeTabIdRef.current;
    if (activeId) {
      const text = textRefs.current.get(activeId) || "";
      setTabs((prev) => prev.map((t) => t.id === activeId ? { ...t, lastText: text } : t));
    }
    setFontSizeState(clamped);
    setTerminalVersion((v) => v + 1);
    try { localStorage.setItem("kn-terminal-fontsize", String(clamped)); } catch { /* */ }
  }, []);

  const setTerminalWidth = useCallback((w: number) => {
    const clamped = Math.min(Math.max(w, MIN_WIDTH), Math.floor(window.innerWidth * 0.65));
    setWidth(clamped);
    try { localStorage.setItem(STORAGE_WIDTH, String(clamped)); } catch { /* */ }
  }, []);

  const setWorkDir = useCallback((tabId: string, dir: string) => {
    setTabs((prev) => prev.map((t) => t.id === tabId ? { ...t, workDir: dir } : t));
  }, []);

  return {
    isOpen, width,
    tabs, activeTabId, activeTab,
    history,
    setErrorCallback,
    open, close, toggle,
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
    switchTab, closeTab,
    setWorkDir,
    setTerminalWidth,
    fontSize, setFontSize, terminalVersion,
  };
}
