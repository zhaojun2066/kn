import { useCallback } from "react";
import { invoke, Channel } from "@tauri-apps/api/core";
import type { TerminalContext } from "./context";
import type { PaneLeaf } from "../../lib/pane-types";
import type { PtyEvent } from "./types";
import { findLeaf, flattenPanes, replaceNode } from "../../lib/pane-types";
import { syncActivePaneFields } from "./helpers";
import type { TabSession } from "./types";
import { submitRelayExit } from "./relayExitOutbox";

export interface PtyExitAgentNotification {
  method: "relay_exit";
  params: {
    nid: string;
    reason: "process_exit";
  };
}

export function getPtyExitAgentNotification(
  tab: TabSession | undefined,
  pane: PaneLeaf,
): PtyExitAgentNotification | null {
  if (!tab || pane.sessionId.startsWith("s_")) return null;
  const agentNid = pane.agentNid ?? (flattenPanes(tab.rootNode).length === 1 ? tab.agentNid : undefined);
  if (!agentNid) return null;
  return {
    method: "relay_exit",
    params: { nid: agentNid, reason: "process_exit" },
  };
}

export function usePtyLifecycle(ctx: TerminalContext) {
  const {
    sessionsRef, termRefs, writeBufRef, rafWriteRef,
    setTabs, errorCallbackRef, childPidRef,
  } = ctx;

  const bindPtyChannel = useCallback((pane: PaneLeaf, channel: Channel<PtyEvent>, resolve: () => void) => {
    channel.onmessage = (msg: PtyEvent) => {
      const term = termRefs.current.get(pane.paneId);
      switch (msg.event) {
        case "ready":
          // 记录 CLI 子进程 PID，供本地 PTY 生命周期管理使用
          childPidRef.current.set(pane.paneId, msg.data);
          resolve();
          break;
        case "data": {
          const existing = writeBufRef.current.get(pane.paneId) || "";
          writeBufRef.current.set(pane.paneId, existing + msg.data);

          const tabForPane = sessionsRef.current.find((t) =>
            findLeaf(t.rootNode, pane.paneId) !== null,
          );
          if (tabForPane?.agentNid && !pane.sessionId.startsWith("s_")) {
            invoke("agent_ipc", {
              method: "relay_output",
              params: { nid: tabForPane.agentNid, data: msg.data },
            }).catch(() => {});
          }

          if (!rafWriteRef.current.has(pane.paneId)) {
            const rafId = requestAnimationFrame(() => {
              rafWriteRef.current.delete(pane.paneId);
              const data = writeBufRef.current.get(pane.paneId) || "";
              writeBufRef.current.set(pane.paneId, "");
              termRefs.current.get(pane.paneId)?.write(data);
            });
            rafWriteRef.current.set(pane.paneId, rafId);
          }
          break;
        }
        case "exit": {
          const pending = writeBufRef.current.get(pane.paneId);
          if (pending) {
            termRefs.current.get(pane.paneId)?.write(pending);
            writeBufRef.current.set(pane.paneId, "");
          }
          const isAgentAttached = pane.sessionId.startsWith("s_");
          if (!isAgentAttached) {
            term?.writeln(`\r\n\x1b[90m[exit: ${msg.data}]\x1b[0m`);
          }

          // Read agentNid BEFORE setTabs (Lesson 8: never read ref after setState)
          const tabForPane = sessionsRef.current.find((t) =>
            findLeaf(t.rootNode, pane.paneId) !== null,
          );
          const agentNotification = getPtyExitAgentNotification(tabForPane, pane);

          setTabs((prev) =>
            prev.map((tab) => {
              const leaf = findLeaf(tab.rootNode, pane.paneId);
              if (!leaf) return tab;
              const updatedLeaf: PaneLeaf = { ...leaf, ptyRunning: false };
              return syncActivePaneFields({
                ...tab,
                rootNode: replaceNode(tab.rootNode, pane.paneId, updatedLeaf),
              });
            }),
          );

          if (agentNotification) {
            void submitRelayExit(agentNotification.params);
          }
          break;
        }
        case "error":
          term?.writeln(`\r\n\x1b[31m[error: ${msg.data}]\x1b[0m`);
          break;
      }
    };
  }, [termRefs, writeBufRef, rafWriteRef, setTabs, sessionsRef, childPidRef]);

  const spawnPty = useCallback((pane: PaneLeaf): Promise<void> => {
    writeBufRef.current.delete(pane.paneId);
    const rafId = rafWriteRef.current.get(pane.paneId);
    if (rafId) { cancelAnimationFrame(rafId); rafWriteRef.current.delete(pane.paneId); }

    return new Promise(async (resolve, reject) => {
      try { await invoke("kill_pty", { sessionId: pane.sessionId }); } catch { /* */ }

      const term = termRefs.current.get(pane.paneId);
      term?.clear();

      const channel = new Channel<PtyEvent>();
      bindPtyChannel(pane, channel, resolve);

      try {
        const t = termRefs.current.get(pane.paneId);
        const cols = t?.cols ?? 100;
        const rows = t?.rows ?? 30;
        await invoke("start_pty", {
          sessionId: pane.sessionId,
          workDir: pane.workDir || null,
          cols,
          rows,
          onEvent: channel,
        });
      } catch (e) {
        setTabs((prev) =>
          prev.map((tab) => {
            const leaf = findLeaf(tab.rootNode, pane.paneId);
            if (!leaf) return tab;
            const updatedLeaf: PaneLeaf = { ...leaf, ptyRunning: false };
            return syncActivePaneFields({
              ...tab,
              rootNode: replaceNode(tab.rootNode, pane.paneId, updatedLeaf),
            });
          }),
        );
        termRefs.current.get(pane.paneId)?.writeln(`\r\n\x1b[31m[无法启动终端: ${e}]\x1b[0m`);
        errorCallbackRef.current?.(`终端启动失败: ${e}`);
        reject(e);
      }
    });
  }, [termRefs, writeBufRef, rafWriteRef, setTabs, errorCallbackRef, bindPtyChannel]);

  const attachAgentPty = useCallback((pane: PaneLeaf, ptySock: string): Promise<void> => {
    writeBufRef.current.delete(pane.paneId);
    const rafId = rafWriteRef.current.get(pane.paneId);
    if (rafId) { cancelAnimationFrame(rafId); rafWriteRef.current.delete(pane.paneId); }

    return new Promise(async (resolve, reject) => {
      try { await invoke("kill_pty", { sessionId: pane.sessionId }); } catch { /* */ }

      const term = termRefs.current.get(pane.paneId);
      term?.clear();

      const channel = new Channel<PtyEvent>();
      bindPtyChannel(pane, channel, resolve);

      try {
        await invoke("attach_agent_pty", {
          sessionId: pane.sessionId,
          ptySock,
          onEvent: channel,
        });
      } catch (e) {
        setTabs((prev) =>
          prev.map((tab) => {
            const leaf = findLeaf(tab.rootNode, pane.paneId);
            if (!leaf) return tab;
            const updatedLeaf: PaneLeaf = { ...leaf, ptyRunning: false };
            return syncActivePaneFields({
              ...tab,
              rootNode: replaceNode(tab.rootNode, pane.paneId, updatedLeaf),
            });
          }),
        );
        termRefs.current.get(pane.paneId)?.writeln(`\r\n\x1b[31m[无法接管远程会话: ${e}]\x1b[0m`);
        errorCallbackRef.current?.(`远程会话接管失败: ${e}`);
        reject(e);
      }
    });
  }, [termRefs, writeBufRef, rafWriteRef, setTabs, errorCallbackRef, bindPtyChannel]);

  return { spawnPty, attachAgentPty };
}
