import type { MutableRefObject, Dispatch, SetStateAction } from "react";
import type { Terminal } from "@xterm/xterm";
import type { TabSession, SessionRecord } from "./types";

/**
 * Shared state and refs passed to all sub-hooks.
 * Centralized here to avoid prop-drilling across 10+ sub-hooks.
 */
export interface TerminalContext {
  // ── Refs (shared across all sub-hooks) ──
  sessionsRef: MutableRefObject<TabSession[]>;
  activeTabIdRef: MutableRefObject<string>;
  termRefs: MutableRefObject<Map<string, Terminal>>;
  writeBufRef: MutableRefObject<Map<string, string>>;
  rafWriteRef: MutableRefObject<Map<string, number>>;
  readyPaneIdsRef: MutableRefObject<Set<string>>;
  readyPromiseRefs: MutableRefObject<
    Map<string, { resolve: () => void; timeout: ReturnType<typeof setTimeout> }>
  >;
  errorCallbackRef: MutableRefObject<((msg: string) => void) | null>;
  openingRef: MutableRefObject<boolean>;

  // ── State setters ──
  setTabs: Dispatch<SetStateAction<TabSession[]>>;
  setIsOpen: Dispatch<SetStateAction<boolean>>;
  setActiveTabId: Dispatch<SetStateAction<string>>;
  setHistory: Dispatch<SetStateAction<SessionRecord[]>>;
  setUsageCounts: Dispatch<SetStateAction<Record<string, number>>>;

  // ── Panel config (read-only) ──
  isBottom: boolean;
  STORAGE_HISTORY: string;
}
