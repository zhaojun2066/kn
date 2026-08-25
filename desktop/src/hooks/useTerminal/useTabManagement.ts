import { useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { TerminalContext } from "./context";
import { findLeaf, flattenPanes, type PaneLeaf } from "../../lib/pane-types";
import type { TabSession } from "./types";
import { newTab } from "./helpers";
import { submitRelayExit } from "./relayExitOutbox";

export interface TerminalCloseKillTargets {
  ptySessionIds: string[];
  agentNids: string[];
  relayExitNids: string[];
}

function emptyTargets(): TerminalCloseKillTargets {
  return { ptySessionIds: [], agentNids: [], relayExitNids: [] };
}

function mergeTargets(targets: TerminalCloseKillTargets[]): TerminalCloseKillTargets {
  const ptySessionIds = new Set<string>();
  const agentNids = new Set<string>();
  const relayExitNids = new Set<string>();
  for (const target of targets) {
    target.ptySessionIds.forEach((id) => ptySessionIds.add(id));
    target.agentNids.forEach((id) => agentNids.add(id));
    target.relayExitNids.forEach((id) => relayExitNids.add(id));
  }
  return {
    ptySessionIds: Array.from(ptySessionIds),
    agentNids: Array.from(agentNids),
    relayExitNids: Array.from(relayExitNids),
  };
}

function collectLeafCloseKills(tab: TabSession, leaf: PaneLeaf): TerminalCloseKillTargets {
  if (leaf.sessionId.startsWith("s_")) return emptyTargets();

  const leafAgentNid = leaf.agentNid ?? (flattenPanes(tab.rootNode).length === 1 ? tab.agentNid : undefined);

  const ptySessionIds = leaf.ptyRunning ? [leaf.sessionId] : [];
  const relayExitNids = leafAgentNid ? [leafAgentNid] : [];
  return { ptySessionIds, agentNids: [], relayExitNids };
}

export function collectPaneCloseKills(tab: TabSession, paneId: string): TerminalCloseKillTargets {
  const leaf = findLeaf(tab.rootNode, paneId);
  if (!leaf) return emptyTargets();
  return collectLeafCloseKills(tab, leaf);
}

export function collectTerminalCloseKills(tab: TabSession): TerminalCloseKillTargets {
  const leaves = flattenPanes(tab.rootNode);
  return mergeTargets(leaves.map((leaf) => collectLeafCloseKills(tab, leaf)));
}

export function invokeTerminalCloseTargets(targets: TerminalCloseKillTargets): void {
  for (const sessionId of targets.ptySessionIds) {
    invoke("kill_pty", { sessionId }).catch(() => {});
  }
  for (const nid of targets.agentNids) {
    invoke("agent_ipc", { method: "kill_session", params: { nid } }).catch(() => {});
  }
  for (const nid of targets.relayExitNids) {
    void submitRelayExit({ nid, reason: "user_closed_tab" });
  }
}

function killTabProcesses(tab: TabSession): void {
  invokeTerminalCloseTargets(collectTerminalCloseKills(tab));
}

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

    if (tab) {
      killTabProcesses(tab);
    }
  }, [isBottom, setIsOpen, setTabs, setActiveTabId, sessionsRef, activeTabIdRef,
      termRefs, writeBufRef, rafWriteRef, cleanupReadyWait]);

  const closeOthers = useCallback((tabId: string) => {
    const allTabs = sessionsRef.current;
    for (const tab of allTabs) {
      if (tab.id === tabId) continue;
      for (const leaf of flattenPanes(tab.rootNode)) {
        termRefs.current.delete(leaf.paneId);
        writeBufRef.current.delete(leaf.paneId);
        cleanupReadyWait(leaf.paneId);
        const rid = rafWriteRef.current.get(leaf.paneId);
        if (rid) { cancelAnimationFrame(rid); rafWriteRef.current.delete(leaf.paneId); }
      }
      killTabProcesses(tab);
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
        termRefs.current.delete(leaf.paneId);
        writeBufRef.current.delete(leaf.paneId);
        cleanupReadyWait(leaf.paneId);
        const rid = rafWriteRef.current.get(leaf.paneId);
        if (rid) { cancelAnimationFrame(rid); rafWriteRef.current.delete(leaf.paneId); }
      }
      killTabProcesses(tab);
    }
    setTabs((prev) => prev.slice(0, idx + 1));
    if (toClose.some((t) => t.id === activeTabIdRef.current)) {
      setActiveTabId(tabId);
    }
  }, [setTabs, setActiveTabId, sessionsRef, activeTabIdRef,
      termRefs, writeBufRef, rafWriteRef, cleanupReadyWait]);

  return { switchTab, closeTab, closeOthers, closeToRight };
}
