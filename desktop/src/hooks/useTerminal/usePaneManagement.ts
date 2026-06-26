import { useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { TerminalContext } from "./context";
import type { PaneLeaf, PaneSplit } from "../../lib/pane-types";
import {
  findLeaf, replaceNode, firstLeaf, findParentSplit,
  flattenPanes, navigateFromLeaf,
} from "../../lib/pane-types";
import { syncActivePaneFields, newSessionId } from "./helpers";
import { PTY_READY_SETTLE_MS } from "./types";
import type { SplitDirection, NavDirection } from "../../lib/pane-types";
import { createInitialLeaf } from "../../lib/pane-types";

export function usePaneManagement(
  ctx: TerminalContext,
  spawnPty: (pane: PaneLeaf) => Promise<void>,
  waitForReady: (paneId: string) => Promise<void>,
  cleanupReadyWait: (paneId: string) => void,
  reportTerminalError: (action: string, error: unknown) => void,
) {
  const { sessionsRef, termRefs, writeBufRef, rafWriteRef, setTabs, errorCallbackRef } = ctx;

  const splitPane = useCallback(async (
    tabId: string,
    direction: SplitDirection,
    workDir?: string,
    paneId?: string,
  ): Promise<string | undefined> => {
    try {
      const tab = sessionsRef.current.find((t) => t.id === tabId);
      if (!tab || tab.zoomedPaneId) return;

      const targetPaneId = paneId ?? tab.activePaneId;
      const targetLeaf = findLeaf(tab.rootNode, targetPaneId);
      if (!targetLeaf) return;

      const newSessionIdStr = newSessionId();
      const newLeaf = createInitialLeaf(tab.name, workDir || targetLeaf.workDir, newSessionIdStr);

      const splitId = `split-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 6)}`;
      const split: PaneSplit = {
        type: "split",
        id: splitId,
        direction,
        ratio: 0.5,
        children: [targetLeaf, newLeaf],
      };

      const readyPromise = waitForReady(newLeaf.paneId);

      setTabs((prev) =>
        prev.map((t) => {
          if (t.id !== tabId) return t;
          return syncActivePaneFields({
            ...t,
            rootNode: replaceNode(t.rootNode, targetLeaf.paneId, split),
            activePaneId: newLeaf.paneId,
          });
        }),
      );

      await readyPromise;
      await new Promise((r) => setTimeout(r, PTY_READY_SETTLE_MS));

      await spawnPty(newLeaf);
      setTabs((prev) =>
        prev.map((t) => {
          if (t.id !== tabId) return t;
          const leaf = findLeaf(t.rootNode, newLeaf.paneId);
          if (!leaf) return t;
          const updatedLeaf: PaneLeaf = { ...leaf, ptyRunning: true };
          return syncActivePaneFields({ ...t, rootNode: replaceNode(t.rootNode, newLeaf.paneId, updatedLeaf) });
        }),
      );

      const term = termRefs.current.get(newLeaf.paneId);
      if (term && term.cols > 0 && term.rows > 0) {
        invoke("resize_pty", {
          sessionId: newSessionIdStr,
          cols: term.cols,
          rows: term.rows,
        }).catch(() => {});
      }

      return newSessionIdStr;
    } catch (e) {
      reportTerminalError("分屏终端失败", e);
      return undefined;
    }
  }, [sessionsRef, termRefs, setTabs, spawnPty, waitForReady, reportTerminalError]);

  const closePane = useCallback((tabId: string, paneId?: string) => {
    const tab = sessionsRef.current.find((t) => t.id === tabId);
    if (!tab) return;

    const leaves = flattenPanes(tab.rootNode);
    if (leaves.length <= 1) {
      errorCallbackRef.current?.("至少保留一个终端");
      return;
    }

    const targetPaneId = paneId ?? tab.activePaneId;
    const targetLeaf = findLeaf(tab.rootNode, targetPaneId);
    if (!targetLeaf) return;

    if (targetLeaf.ptyRunning) {
      invoke("kill_pty", { sessionId: targetLeaf.sessionId }).catch(() => {});
    }
    termRefs.current.delete(targetLeaf.paneId);
    writeBufRef.current.delete(targetLeaf.paneId);
    cleanupReadyWait(targetLeaf.paneId);
    const rafId = rafWriteRef.current.get(targetLeaf.paneId);
    if (rafId) { cancelAnimationFrame(rafId); rafWriteRef.current.delete(targetLeaf.paneId); }

    const parentInfo = findParentSplit(tab.rootNode, targetLeaf.paneId);
    let newRoot = tab.rootNode;
    let newFocusId = "";

    if (parentInfo) {
      const sibling = parentInfo.parent.children[parentInfo.index === 0 ? 1 : 0];
      newRoot = replaceNode(tab.rootNode, parentInfo.parent.id, sibling);
      newFocusId = firstLeaf(sibling)?.paneId || tab.activePaneId;
    } else {
      newFocusId = leaves.find((l) => l.paneId !== targetLeaf.paneId)?.paneId || "";
    }

    const shouldClearZoom = tab.zoomedPaneId && tab.zoomedPaneId === targetLeaf.paneId;

    setTabs((prev) =>
      prev.map((t) => {
        if (t.id !== tabId) return t;
        return syncActivePaneFields({
          ...t,
          rootNode: newRoot,
          activePaneId: newFocusId,
          zoomedPaneId: shouldClearZoom ? null : t.zoomedPaneId,
        });
      }),
    );
  }, [sessionsRef, termRefs, writeBufRef, rafWriteRef, setTabs, cleanupReadyWait, errorCallbackRef]);

  const focusPane = useCallback((tabId: string, paneId: string) => {
    setTabs((prev) =>
      prev.map((t) => {
        if (t.id !== tabId || t.activePaneId === paneId) return t;
        return syncActivePaneFields({ ...t, activePaneId: paneId });
      }),
    );
  }, [setTabs]);

  const navigatePane = useCallback((tabId: string, direction: NavDirection) => {
    setTabs((prev) => {
      const tab = prev.find((t) => t.id === tabId);
      if (!tab) return prev;

      const target = navigateFromLeaf(tab.rootNode, tab.activePaneId, direction);
      if (!target) return prev;

      return prev.map((t) => {
        if (t.id !== tabId) return t;
        return syncActivePaneFields({ ...t, activePaneId: target.paneId });
      });
    });
  }, [setTabs]);

  const cyclePane = useCallback((tabId: string, forward: boolean) => {
    setTabs((prev) => {
      const tab = prev.find((t) => t.id === tabId);
      if (!tab) return prev;

      const leaves = flattenPanes(tab.rootNode);
      if (leaves.length <= 1) return prev;

      const idx = leaves.findIndex((l) => l.paneId === tab.activePaneId);
      const nextIdx = forward
        ? (idx + 1) % leaves.length
        : (idx - 1 + leaves.length) % leaves.length;

      return prev.map((t) => {
        if (t.id !== tabId) return t;
        return syncActivePaneFields({ ...t, activePaneId: leaves[nextIdx].paneId });
      });
    });
  }, [setTabs]);

  const zoomPane = useCallback((tabId: string) => {
    setTabs((prev) =>
      prev.map((t) => {
        if (t.id !== tabId) return t;
        const newZoomed = t.zoomedPaneId ? null : t.activePaneId;
        return syncActivePaneFields({ ...t, zoomedPaneId: newZoomed });
      }),
    );
  }, [setTabs]);

  const setWorkDir = useCallback((tabId: string, dir: string) => {
    setTabs((prev) =>
      prev.map((t) => {
        if (t.id !== tabId) return t;
        const activeLeaf = findLeaf(t.rootNode, t.activePaneId);
        if (!activeLeaf) return { ...t, workDir: dir };
        const updatedLeaf: PaneLeaf = { ...activeLeaf, workDir: dir };
        return syncActivePaneFields({
          ...t,
          workDir: dir,
          rootNode: replaceNode(t.rootNode, activeLeaf.paneId, updatedLeaf),
        });
      }),
    );
  }, [setTabs]);

  return { splitPane, closePane, focusPane, navigatePane, cyclePane, zoomPane, setWorkDir };
}
