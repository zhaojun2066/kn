import { useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { TerminalContext } from "./context";
import type { PaneLeaf } from "../../lib/pane-types";
import type { SessionRecord } from "./types";
import { MAX_HISTORY, PTY_READY_SETTLE_MS, PTY_COMMAND_SETTLE_MS } from "./types";
import { findLeaf, replaceNode } from "../../lib/pane-types";
import { syncActivePaneFields, newTab } from "./helpers";
import { parseAiCmd, buildResumeCmd, buildResumeLastCmd, normalizeTool } from "./utils";
import type { ProjectInfo } from "../../lib/types";

export function useSessionCommands(
  ctx: TerminalContext,
  isOpen: boolean,
  spawnPty: (pane: PaneLeaf) => Promise<void>,
  waitForReady: (paneId: string) => Promise<void>,
  reportTerminalError: (action: string, error: unknown) => void,
  splitPane: (tabId: string, direction: "horizontal" | "vertical", workDir?: string, paneId?: string) => Promise<string | undefined>,
  saveHistory: (records: SessionRecord[]) => void,
) {
  const { sessionsRef, isBottom, setIsOpen, setTabs, setActiveTabId, setHistory, setUsageCounts } = ctx;

  const runInNewTab = useCallback(async (cmd: string, workDir: string, label?: string) => {
    try {
      const tab = newTab(label || cmd.slice(0, 20), workDir);
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

      if (!isBottom) {
        const parsed = parseAiCmd(cmd);
        if (parsed) {
          setUsageCounts((prev) => ({ ...prev, [parsed.profile]: (prev[parsed.profile] || 0) + 1 }));
        }
        const record: SessionRecord = {
          id: Date.now().toString(36) + Math.random().toString(36).slice(2, 6),
          command: cmd,
          resumeCommand: buildResumeCmd(cmd),
          resumeLastCommand: buildResumeLastCmd(cmd),
          workDir,
          label: label || cmd,
          tool: parsed?.tool || null,
          timestamp: Date.now(),
        };
        setHistory((prev) => {
          const filtered = prev.filter((r) => !(r.command === cmd && r.workDir === workDir));
          const next = [record, ...filtered].slice(0, MAX_HISTORY);
          saveHistory(next);
          return next;
        });
      }

      await new Promise((r) => setTimeout(r, PTY_COMMAND_SETTLE_MS));
      invoke("write_pty", {
        sessionId: activeLeaf.sessionId,
        data: cmd + "\r",
      }).catch(() => {});

      // Register CLI session with agent for WSS/cloud sync
      const parsed = parseAiCmd(cmd);
      if (parsed) {
        invoke("agent_ipc", {
          method: "register_session",
          params: {
            tool: normalizeTool(parsed.tool),
            profile: parsed.profile,
            cwd: workDir,
            source: "desktop",
          },
        }).then((result: any) => {
          if (result?.nid) {
            setTabs((prev) =>
              prev.map((t) => (t.id === tab.id ? { ...t, agentNid: result.nid } : t)),
            );
          }
        }).catch(() => { /* agent not running — graceful */ });
      }
    } catch (e) {
      reportTerminalError("运行终端命令失败", e);
    }
  }, [isOpen, isBottom, setIsOpen, setTabs, setActiveTabId, setHistory, setUsageCounts, saveHistory,
      spawnPty, waitForReady, reportTerminalError]);

  const runInTerminal = useCallback(async (cmd: string, workDir: string) => {
    await runInNewTab(cmd, workDir, cmd.slice(0, 30));
  }, [runInNewTab]);

  const runProjectCommand = useCallback(async (
    cmd: string,
    project: ProjectInfo,
    _profileName?: string,
    label?: string,
  ) => {
    await runInNewTab(cmd, project.path, label ?? `${project.name} · ${cmd.slice(0, 30)}`);
  }, [runInNewTab]);

  const pasteCommand = useCallback(async (cmd: string): Promise<boolean> => {
    const tab = sessionsRef.current.find((t) => t.id === ctx.activeTabIdRef.current);
    if (!tab) return false;
    const activeLeaf = findLeaf(tab.rootNode, tab.activePaneId);
    if (!activeLeaf?.ptyRunning) return false;
    invoke("write_pty", { sessionId: activeLeaf.sessionId, data: cmd + "\r" }).catch(() => {});
    return true;
  }, [sessionsRef, ctx.activeTabIdRef]);

  const runInSplitPane = useCallback(async (cmd: string, workDir: string, label?: string) => {
    try {
      const tabId = ctx.activeTabIdRef.current;
      const tab = sessionsRef.current.find((t) => t.id === tabId);

      if (!tab || !tabId) {
        await runInNewTab(cmd, workDir, label);
        return;
      }

      if (!isOpen) setIsOpen(true);

      const newSessionId = await splitPane(tabId, "horizontal", workDir);
      if (!newSessionId) return;

      await new Promise((r) => setTimeout(r, PTY_COMMAND_SETTLE_MS));
      invoke("write_pty", {
        sessionId: newSessionId,
        data: cmd + "\r",
      }).catch(() => {});

      // Register CLI session with agent for WSS/cloud sync
      const parsed2 = parseAiCmd(cmd);
      if (parsed2) {
        invoke("agent_ipc", {
          method: "register_session",
          params: {
            tool: normalizeTool(parsed2.tool),
            profile: parsed2.profile,
            cwd: workDir,
            source: "desktop",
          },
        }).then((result: any) => {
          if (result?.nid) {
            setTabs((prev) =>
              prev.map((t) => (t.id === tabId ? { ...t, agentNid: result.nid } : t)),
            );
          }
        }).catch(() => { /* agent not running — graceful */ });
      }

      if (!isBottom) {
        const parsed = parseAiCmd(cmd);
        if (parsed) {
          setUsageCounts((prev) => ({ ...prev, [parsed.profile]: (prev[parsed.profile] || 0) + 1 }));
        }
        const record: SessionRecord = {
          id: Date.now().toString(36) + Math.random().toString(36).slice(2, 6),
          command: cmd,
          resumeCommand: buildResumeCmd(cmd),
          resumeLastCommand: buildResumeLastCmd(cmd),
          workDir,
          label: label || cmd,
          tool: parsed?.tool || null,
          timestamp: Date.now(),
        };
        setHistory((prev) => {
          const filtered = prev.filter((r) => !(r.command === cmd && r.workDir === workDir));
          const next = [record, ...filtered].slice(0, MAX_HISTORY);
          saveHistory(next);
          return next;
        });
      }
    } catch (e) {
      reportTerminalError("分屏运行命令失败", e);
    }
  }, [isOpen, isBottom, setIsOpen, setUsageCounts, setHistory, saveHistory, sessionsRef,
      ctx.activeTabIdRef, runInNewTab, splitPane, reportTerminalError]);

  return { runInNewTab, runInTerminal, runProjectCommand, pasteCommand, runInSplitPane };
}
