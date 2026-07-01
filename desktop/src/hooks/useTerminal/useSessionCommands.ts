import { useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { TerminalContext } from "./context";
import type { PaneLeaf } from "../../lib/pane-types";
import type { SessionRecord } from "./types";
import { MAX_HISTORY, PTY_READY_SETTLE_MS, PTY_COMMAND_SETTLE_MS } from "./types";
import { findLeaf, flattenPanes, replaceNode } from "../../lib/pane-types";
import { syncActivePaneFields, newTab } from "./helpers";
import { parseAiCmd, buildResumeCmd, buildResumeLastCmd, getRunCommandPolicy, normalizeTool } from "./utils";
import type { ProjectInfo } from "../../lib/types";
import type { AgentSession } from "../useAgent";
import { hasAgentSessionTab } from "./agentSessionSync";

const AGENT_ATTACH_RETRY_DELAYS_MS = [80, 160, 260, 400];

export function useSessionCommands(
  ctx: TerminalContext,
  isOpen: boolean,
  spawnPty: (pane: PaneLeaf) => Promise<void>,
  attachAgentPty: (pane: PaneLeaf, ptySock: string) => Promise<void>,
  waitForReady: (paneId: string) => Promise<void>,
  reportTerminalError: (action: string, error: unknown) => void,
  splitPane: (tabId: string, direction: "horizontal" | "vertical", workDir?: string, paneId?: string) => Promise<string | undefined>,
  saveHistory: (records: SessionRecord[]) => void,
) {
  const { sessionsRef, isBottom, setIsOpen, setTabs, setActiveTabId, setHistory, setUsageCounts } = ctx;

  async function attachAgentSession(nid: string): Promise<string> {
    let lastError: unknown = null;
    for (let attempt = 0; attempt <= AGENT_ATTACH_RETRY_DELAYS_MS.length; attempt += 1) {
      try {
        const attachResult = await invoke<{ pty_sock?: string; ptySock?: string }>("agent_ipc", {
          method: "attach",
          params: { nid },
        });
        const ptySock = attachResult.pty_sock || attachResult.ptySock;
        if (!ptySock) throw new Error("agent attach 未返回 pty_sock");
        return ptySock;
      } catch (e) {
        lastError = e;
        const delay = AGENT_ATTACH_RETRY_DELAYS_MS[attempt];
        if (delay === undefined) break;
        await new Promise((r) => setTimeout(r, delay));
      }
    }
    throw lastError;
  }

  const saveCommandHistory = useCallback((
    cmd: string,
    workDir: string,
    label: string | undefined,
    parsed: { tool: string; profile: string } | null,
  ) => {
    if (isBottom) return;
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
  }, [isBottom, saveHistory, setHistory, setUsageCounts]);

  const registerRelaySession = useCallback(async (
    tabId: string,
    pane: PaneLeaf,
    parsed: { tool: string; profile: string },
    workDir: string,
  ) => {
    try {
      const pid = ctx.childPidRef.current.get(pane.paneId) || 0;
      const result = await invoke<{ nid?: string; created_at?: string }>("agent_ipc", {
        method: "register_session",
        params: {
          tool: normalizeTool(parsed.tool),
          profile: parsed.profile,
          cwd: workDir,
          source: "desktop",
          pid,
        },
      });
      if (!result?.nid) return;

      const term = ctx.termRefs.current.get(pane.paneId);
      if (term && term.cols > 0 && term.rows > 0) {
        invoke("agent_ipc", {
          method: "resize",
          params: { nid: result.nid, cols: term.cols, rows: term.rows },
        }).catch(() => {});
      }

      const relaySession: AgentSession = {
        nid: result.nid,
        kind: "Relay",
        source: "desktop",
        tool: normalizeTool(parsed.tool),
        profile: parsed.profile,
        cwd: workDir,
        created_at: result.created_at || new Date().toISOString(),
        status: "running",
        remote_enabled: false,
      };
      ctx.agentSessionsRef.current = [relaySession, ...ctx.agentSessionsRef.current];

      setTabs((prev) =>
        prev.map((t) => (t.id === tabId ? { ...t, agentNid: result.nid } : t)),
      );
      window.dispatchEvent(new CustomEvent("kn-agent-sessions-changed"));
    } catch {
      // Agent may be stopped/unbound; local terminal startup should still work.
    }
  }, [ctx.agentSessionsRef, ctx.childPidRef, ctx.termRefs, setTabs]);

  const attachOrOpenAgentSession = useCallback(async (session: AgentSession, label?: string) => {
    ctx.dismissedAgentNidsRef.current.delete(session.nid);
    const existing = sessionsRef.current.find((tab) => hasAgentSessionTab([tab], session.nid));
    let tabId: string;
    let pane: PaneLeaf;

    if (existing) {
      tabId = existing.id;
      const matchingLeaf = flattenPanes(existing.rootNode).find((leaf) => leaf.sessionId === session.nid);
      pane = matchingLeaf || findLeaf(existing.rootNode, existing.activePaneId)!;
      setActiveTabId(existing.id);
    } else {
      const tab = newTab(label || `${session.tool} · 本地`, session.cwd);
      const activeLeaf = findLeaf(tab.rootNode, tab.activePaneId)!;
      pane = {
        ...activeLeaf,
        sessionId: session.nid,
        name: label || `${session.tool} · 本地`,
        workDir: session.cwd,
      };
      const agentTab = syncActivePaneFields({
        ...tab,
        name: label || `${session.tool} · 本地`,
        workDir: session.cwd,
        agentNid: session.nid,
        rootNode: replaceNode(tab.rootNode, activeLeaf.paneId, pane),
      });
      tabId = agentTab.id;
      setTabs((prev) => [...prev, agentTab]);
      setActiveTabId(agentTab.id);
    }

    if (!isOpen) setIsOpen(true);

    if (pane.ptyRunning) return;

    await waitForReady(pane.paneId);
    await new Promise((r) => setTimeout(r, PTY_READY_SETTLE_MS));

    const ptySock = await attachAgentSession(session.nid);

    await attachAgentPty(pane, ptySock);

    setTabs((prev) =>
      prev.map((t) => {
        if (t.id !== tabId) return t;
        const leaf = findLeaf(t.rootNode, pane.paneId);
        if (!leaf) return t;
        const updatedLeaf: PaneLeaf = { ...leaf, ptyRunning: true };
        return syncActivePaneFields({ ...t, rootNode: replaceNode(t.rootNode, pane.paneId, updatedLeaf) });
      }),
    );

    const term = ctx.termRefs.current.get(pane.paneId);
    if (term && term.cols > 0 && term.rows > 0) {
      invoke("agent_ipc", {
        method: "resize",
        params: { nid: session.nid, cols: term.cols, rows: term.rows },
      }).catch(() => {});
    }
  }, [
    sessionsRef, setTabs, setActiveTabId, isOpen, setIsOpen, waitForReady,
    attachAgentPty, ctx.termRefs, ctx.dismissedAgentNidsRef,
  ]);

  const runInNewTab = useCallback(async (cmd: string, workDir: string, label?: string) => {
    try {
      const policy = getRunCommandPolicy(cmd);
      const parsed = parseAiCmd(cmd);
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

      saveCommandHistory(cmd, workDir, label, parsed);

      if (policy.registerRelay && parsed) {
        registerRelaySession(tab.id, activeLeaf, parsed, workDir);
      }

      await new Promise((r) => setTimeout(r, PTY_COMMAND_SETTLE_MS));
      invoke("write_pty", {
        sessionId: activeLeaf.sessionId,
        data: cmd + "\r",
      }).catch(() => {});

    } catch (e) {
      reportTerminalError("运行终端命令失败", e);
    }
  }, [isOpen, setIsOpen, setTabs, setActiveTabId, spawnPty, waitForReady, reportTerminalError,
      saveCommandHistory, registerRelaySession]);

  const runInTerminal = useCallback(async (cmd: string, workDir: string) => {
    await runInNewTab(cmd, workDir, cmd.slice(0, 30));
  }, [runInNewTab]);

  const openRemoteSession = useCallback(async (session: AgentSession) => {
    try {
      await attachOrOpenAgentSession(session, `${session.tool} · 远程`);
    } catch (e) {
      reportTerminalError("打开远程会话失败", e);
    }
  }, [attachOrOpenAgentSession, reportTerminalError]);

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
      const parsedForAgent = parseAiCmd(cmd);
      if (parsedForAgent) {
        await runInNewTab(cmd, workDir, label);
        return;
      }

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

      if (!isBottom) {
        const record: SessionRecord = {
          id: Date.now().toString(36) + Math.random().toString(36).slice(2, 6),
          command: cmd,
          resumeCommand: buildResumeCmd(cmd),
          resumeLastCommand: buildResumeLastCmd(cmd),
          workDir,
          label: label || cmd,
          tool: null,
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

  return { runInNewTab, runInTerminal, runProjectCommand, openRemoteSession, pasteCommand, runInSplitPane };
}
