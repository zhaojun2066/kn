import { useCallback } from "react";
import { invoke, Channel } from "@tauri-apps/api/core";
import type { TerminalContext } from "./context";
import type { PaneLeaf } from "../../lib/pane-types";
import type { PtyEvent, TabSession } from "./types";
// MIN_COLS, MIN_ROWS used in handleTerminalResize (useTerminalReady)
import { findLeaf, replaceNode } from "../../lib/pane-types";
import { syncActivePaneFields } from "./helpers";

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
          const agentNid = tabForPane?.agentNid;

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

          // Notify agent that local CLI sessions have ended. Remote-attached
          // sessions are only a desktop view onto an agent-owned PTY, so closing
          // the pane must detach rather than kill the shared remote process.
          if (agentNid && !isAgentAttached) {
            invoke("agent_ipc", { method: "kill_session", params: { nid: agentNid } }).catch(() => {});
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
        termRefs.current.get(pane.paneId)?.writeln(`\r\n\x1b[31m[无法接管远程终端: ${e}]\x1b[0m`);
        errorCallbackRef.current?.(`远程终端接管失败: ${e}`);
        reject(e);
      }
    });
  }, [termRefs, writeBufRef, rafWriteRef, setTabs, errorCallbackRef, bindPtyChannel]);

  return { spawnPty, attachAgentPty };
}
