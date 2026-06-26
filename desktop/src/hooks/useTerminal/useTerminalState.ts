import { useState, useRef } from "react";
import type { TabSession, SessionRecord } from "./types";
import { MAX_HISTORY } from "./types";
import { newTab } from "./helpers";
import { buildResumeLastCmd } from "./utils";

export function useTerminalState(isBottom: boolean, STORAGE_SIZE: string, STORAGE_HISTORY: string, STORAGE_FONTSIZE: string, MIN_SIZE: number) {
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
  const [tabs, setTabs] = useState<TabSession[]>(() => isBottom ? [newTab("终端")] : []);
  const [history, setHistory] = useState<SessionRecord[]>(() => loadHistory());
  const [activeTabId, setActiveTabId] = useState<string>(tabs[0]?.id || "");
  const [usageCounts, setUsageCounts] = useState<Record<string, number>>({});

  const sessionsRef = useRef(tabs);
  sessionsRef.current = tabs;

  return {
    isOpen, setIsOpen,
    size, setSizeState,
    fontSize, setFontSizeState,
    tabs, setTabs,
    history, setHistory,
    activeTabId, setActiveTabId,
    usageCounts, setUsageCounts,
    sessionsRef,
    saveHistory,
    STORAGE_FONTSIZE,
  };
}
