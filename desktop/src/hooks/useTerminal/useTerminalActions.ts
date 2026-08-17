import { useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { TerminalContext } from "./context";
import { PTY_READY_SETTLE_MS } from "./types";
import type { PaneLeaf } from "../../lib/pane-types";
import { findLeaf, replaceNode } from "../../lib/pane-types";
import { syncActivePaneFields, newTab } from "./helpers";
import { collectTerminalCloseKills, invokeTerminalCloseTargets } from "./useTabManagement";

export function useTerminalActions(
  ctx: TerminalContext,
  isOpen: boolean,
  spawnPty: (pane: PaneLeaf) => Promise<void>,
  waitForReady: (paneId: string) => Promise<void>,
  reportTerminalError: (action: string, error: unknown) => void,
) {
  const {
    sessionsRef, isBottom, setIsOpen, setTabs,
    setActiveTabId, activeTabIdRef, termRefs, writeBufRef, rafWriteRef,
  } = ctx;

  const openingRef = ctx.openingRef;

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
  }, [isOpen, setIsOpen, setTabs, setActiveTabId, waitForReady, spawnPty, reportTerminalError]);

  const open = useCallback(async () => {
    try {
      setIsOpen(true);
      const tab = sessionsRef.current.find((t) => t.id === activeTabIdRef.current);
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
  }, [setIsOpen, sessionsRef, activeTabIdRef, waitForReady, spawnPty, setTabs, reportTerminalError]);

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
    ctx.readyPaneIdsRef.current.clear();
    for (const [, pending] of ctx.readyPromiseRefs.current) {
      clearTimeout(pending.timeout);
      pending.resolve();
    }
    ctx.readyPromiseRefs.current.clear();
    termRefs.current.clear();
    for (const [, id] of rafWriteRef.current) { cancelAnimationFrame(id); }
    rafWriteRef.current.clear();

    // Close the panel using the same local-vs-remote lifecycle rules as tab close.
    for (const tab of currentTabs) {
      invokeTerminalCloseTargets(collectTerminalCloseKills(tab));
    }
  }, [isBottom, setIsOpen, setTabs, setActiveTabId, sessionsRef, activeTabIdRef,
      writeBufRef, termRefs, rafWriteRef, ctx.readyPaneIdsRef, ctx.readyPromiseRefs]);

  const hide = useCallback(() => {
    setIsOpen(false);
  }, [setIsOpen]);

  const toggle = useCallback(() => {
    if (isOpen) { hide(); }
    else if (!openingRef.current) {
      openingRef.current = true;
      open().finally(() => { openingRef.current = false; });
    }
  }, [isOpen, hide, open, openingRef]);

  return { newEmptyTab, open, close, hide, toggle };
}
