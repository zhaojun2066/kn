/// <reference types="vitest" />
import { describe, it, expect } from "vitest";

// ── Types (mirrors useTerminal.ts) ──────────────────────────

interface TabSession {
  id: string;
  name: string;
  workDir?: string;
  sessionId: string;
  ptyRunning: boolean;
  command?: string;
  historyId?: string;
  scrollback?: string;
  startTime?: number;
}

function newTab(name?: string, workDir?: string): TabSession {
  const id = `tab-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
  return {
    id,
    name: name || "终端",
    workDir,
    sessionId: `pty-${id}`,
    ptyRunning: false,
    startTime: Date.now(),
  };
}

// ── closeTab updater (extracted from useTerminal.ts fix) ────

interface CloseTabResult {
  nextTabs: TabSession[];
  shouldClosePanel: boolean;
  newActiveTabId: string | null;
}

/**
 * Simulates the closeTab setTabs updater logic that was fixed
 * in the stale-ref bug. All side-effects are computed INSIDE the
 * updater using the latest state (prev), not a stale sessionsRef.
 */
function closeTabUpdater(
  prev: TabSession[],
  tabId: string,
  wasActiveId: string,
  isBottom: boolean,
): CloseTabResult {
  const next = prev.filter((t) => t.id !== tabId);

  // Bottom panel: keep at least one default tab
  if (next.length === 0 && isBottom) {
    const defaultTab = newTab("终端");
    return {
      nextTabs: [defaultTab],
      shouldClosePanel: false,
      newActiveTabId: defaultTab.id,
    };
  }

  let shouldClosePanel = false;
  let newActiveTabId: string | null = null;

  // Right panel: close when last tab removed
  if (next.length === 0 && !isBottom) {
    shouldClosePanel = true;
  } else if (wasActiveId === tabId) {
    // Switch to first remaining tab
    newActiveTabId = next[0]?.id || null;
  }

  return { nextTabs: next, shouldClosePanel, newActiveTabId };
}

// ── Tests ───────────────────────────────────────────────────

describe("newTab", () => {
  it("creates a tab with unique id", () => {
    const a = newTab("A");
    const b = newTab("B");
    expect(a.id).toBeTruthy();
    expect(b.id).toBeTruthy();
    expect(a.id).not.toEqual(b.id);
  });

  it("defaults name to 终端", () => {
    const tab = newTab();
    expect(tab.name).toBe("终端");
  });

  it("initializes ptyRunning to false", () => {
    const tab = newTab("test");
    expect(tab.ptyRunning).toBe(false);
  });
});

describe("closeTab updater (stale-ref bug fix)", () => {
  const makeTabs = (count: number): TabSession[] =>
    Array.from({ length: count }, (_, i) => newTab(`Tab ${i + 1}`));

  it("removes the target tab", () => {
    const tabs = makeTabs(3);
    const target = tabs[1].id;
    const { nextTabs } = closeTabUpdater(tabs, target, tabs[0].id, false);
    expect(nextTabs.length).toBe(2);
    expect(nextTabs.find((t) => t.id === target)).toBeUndefined();
  });

  it("closes right panel when last tab is removed", () => {
    const tabs = makeTabs(1);
    const { nextTabs, shouldClosePanel } = closeTabUpdater(
      tabs, tabs[0].id, tabs[0].id, false,
    );
    expect(nextTabs.length).toBe(0);
    expect(shouldClosePanel).toBe(true);
  });

  it("creates default tab for bottom panel when last tab removed", () => {
    const tabs = makeTabs(1);
    const { nextTabs, shouldClosePanel } = closeTabUpdater(
      tabs, tabs[0].id, tabs[0].id, true,
    );
    expect(nextTabs.length).toBe(1);
    expect(nextTabs[0].name).toBe("终端");
    expect(shouldClosePanel).toBe(false);
  });

  it("switches active tab when closing the active one", () => {
    const tabs = makeTabs(3);
    const target = tabs[0].id;
    const { newActiveTabId } = closeTabUpdater(tabs, target, target, false);
    expect(newActiveTabId).toBe(tabs[1].id);
  });

  it("does NOT switch active tab when closing a non-active one", () => {
    const tabs = makeTabs(3);
    const target = tabs[1].id;
    const { newActiveTabId } = closeTabUpdater(tabs, target, tabs[0].id, false);
    expect(newActiveTabId).toBeNull();
  });

  it("does NOT close panel when tabs remain after closing", () => {
    const tabs = makeTabs(3);
    const { shouldClosePanel } = closeTabUpdater(tabs, tabs[0].id, tabs[0].id, false);
    expect(shouldClosePanel).toBe(false);
  });
});

// ── closeOthers — keep only specified tab ───────────────────

describe("closeOthers", () => {
  const makeTabs = (count: number): TabSession[] =>
    Array.from({ length: count }, (_, i) => newTab(`Tab ${i + 1}`));

  function closeOthersLogic(
    allTabs: TabSession[],
    keepId: string,
  ): TabSession[] {
    const kept = allTabs.find((t) => t.id === keepId);
    return kept ? [kept] : [newTab("终端")];
  }

  it("keeps only the specified tab", () => {
    const tabs = makeTabs(4);
    const keepId = tabs[1].id;
    const next = closeOthersLogic(tabs, keepId);
    expect(next.length).toBe(1);
    expect(next[0].id).toBe(keepId);
  });

  it("creates default tab if keep tab not found", () => {
    const tabs = makeTabs(3);
    const next = closeOthersLogic(tabs, "nonexistent");
    expect(next[0].name).toBe("终端");
  });
});

// ── closeToRight — remove tabs after index ──────────────────

describe("closeToRight", () => {
  const makeTabs = (count: number): TabSession[] =>
    Array.from({ length: count }, (_, i) => newTab(`Tab ${i + 1}`));

  function closeToRightLogic(
    allTabs: TabSession[],
    tabId: string,
    activeId: string,
  ): { nextTabs: TabSession[]; newActiveId: string } {
    const idx = allTabs.findIndex((t) => t.id === tabId);
    if (idx < 0) return { nextTabs: allTabs, newActiveId: activeId };
    const next = allTabs.slice(0, idx + 1);
    const toClose = allTabs.slice(idx + 1);
    const newActiveId = toClose.some((t) => t.id === activeId) ? tabId : activeId;
    return { nextTabs: next, newActiveId };
  }

  it("removes tabs to the right", () => {
    const tabs = makeTabs(5);
    const { nextTabs } = closeToRightLogic(tabs, tabs[2].id, tabs[0].id);
    expect(nextTabs.length).toBe(3);
    expect(nextTabs).toEqual(tabs.slice(0, 3));
  });

  it("switches active tab if it was closed", () => {
    const tabs = makeTabs(4);
    const { newActiveId } = closeToRightLogic(tabs, tabs[1].id, tabs[3].id);
    expect(newActiveId).toBe(tabs[1].id);
  });

  it("keeps active tab if it is before target", () => {
    const tabs = makeTabs(4);
    const { newActiveId } = closeToRightLogic(tabs, tabs[2].id, tabs[0].id);
    expect(newActiveId).toBe(tabs[0].id);
  });

  it("no-op if tabId not found", () => {
    const tabs = makeTabs(3);
    const { nextTabs } = closeToRightLogic(tabs, "nonexistent", tabs[0].id);
    expect(nextTabs).toEqual(tabs);
  });
});
