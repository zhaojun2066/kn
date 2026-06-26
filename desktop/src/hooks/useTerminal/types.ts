import type { PaneNode } from "../../lib/pane-types";

export const MAX_HISTORY = 30;
export const PTY_READY_SETTLE_MS = 80;
export const PTY_COMMAND_SETTLE_MS = 300;
export const TERMINAL_READY_TIMEOUT_MS = 1500;
export const MIN_COLS = 5;
export const MIN_ROWS = 2;

export interface TabSession {
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

export type PtyEvent =
  | { event: "ready" }
  | { event: "data"; data: string }
  | { event: "exit"; data: number }
  | { event: "error"; data: string };
