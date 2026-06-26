import type { PaneLeaf, PaneNode } from "../../lib/pane-types";
import { findLeaf, replaceNode, createInitialLeaf } from "../../lib/pane-types";
import type { TabSession } from "./types";

let tabCounter = 1;

/** Sync backward-compat fields (sessionId, ptyRunning) from the active pane */
export function syncActivePaneFields(tab: TabSession): TabSession {
  const activeLeaf = findLeaf(tab.rootNode, tab.activePaneId);
  return {
    ...tab,
    sessionId: activeLeaf?.sessionId || "",
    ptyRunning: activeLeaf?.ptyRunning || false,
  };
}

/** Find a pane leaf across all tabs */
export function findPaneInTabs(tabs: TabSession[], paneId: string): PaneLeaf | null {
  for (const tab of tabs) {
    const leaf = findLeaf(tab.rootNode, paneId);
    if (leaf) return leaf;
  }
  return null;
}

export function newTab(name?: string, workDir?: string): TabSession {
  const n = tabCounter++;
  const tabId = `tab-${n}`;
  const tabName = name || `终端 ${n}`;
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

/** Generate a unique PTY session ID (for pane splits, etc.) */
export function newSessionId(): string {
  return `pty-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 6)}`;
}
