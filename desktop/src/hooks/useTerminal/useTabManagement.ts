import { useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { TerminalContext } from "./context";
import { flattenPanes } from "../../lib/pane-types";
import { newTab } from "./helpers";

export function useTabManagement(
  ctx: TerminalContext,
  isBottom: boolean,
  cleanupReadyWait: (paneId: string) => void,
) {
  const { sessionsRef, activeTabIdRef, termRefs, writeBufRef, rafWriteRef,
          setIsOpen, setTabs, setActiveTabId } = ctx;

  const switchTab = useCallback((tabId: string) => {
    setActiveTabId(tabId);
  }, [setActiveTabId]);

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
  }, [isBottom, setIsOpen, setTabs, setActiveTabId, sessionsRef, activeTabIdRef,
      termRefs, writeBufRef, rafWriteRef, cleanupReadyWait]);

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
  }, [isBottom, setTabs, setActiveTabId, sessionsRef, termRefs, writeBufRef, rafWriteRef, cleanupReadyWait]);

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
  }, [setTabs, setActiveTabId, sessionsRef, activeTabIdRef,
      termRefs, writeBufRef, rafWriteRef, cleanupReadyWait]);

  return { switchTab, closeTab, closeOthers, closeToRight };
}
