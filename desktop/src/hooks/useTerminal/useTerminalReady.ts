import { useCallback, useRef } from "react";
import type { Terminal } from "@xterm/xterm";
import { invoke } from "@tauri-apps/api/core";
import type { TerminalContext } from "./context";
import type { SessionRecord } from "./types";
import { TERMINAL_READY_TIMEOUT_MS, MIN_COLS, MIN_ROWS } from "./types";
import { findPaneInTabs } from "./helpers";
import { isProfileCompatibleWithSession, parseAiCmd, type ProfileCliInfo } from "./utils";

export function useTerminalReady(ctx: TerminalContext) {
  const { sessionsRef, readyPaneIdsRef, readyPromiseRefs, errorCallbackRef } = ctx;
  const deleteHistoryRef = useRef<((id: string) => void) | null>(null);
  const lastResizeByPaneRef = useRef<Map<string, { cols: number; rows: number }>>(new Map());

  // Local history must resume through a profile of the same CLI.
  const profilesRef = useRef<readonly ProfileCliInfo[]>([]);

  const cleanupReadyWait = useCallback((paneId: string) => {
    readyPaneIdsRef.current.delete(paneId);
    lastResizeByPaneRef.current.delete(paneId);
    const pending = readyPromiseRefs.current.get(paneId);
    if (pending) {
      clearTimeout(pending.timeout);
      readyPromiseRefs.current.delete(paneId);
      pending.resolve();
    }
  }, [readyPaneIdsRef]);

  const handleTerminalReady = useCallback((paneId: string) => {
    readyPaneIdsRef.current.add(paneId);
    const pending = readyPromiseRefs.current.get(paneId);
    if (pending) {
      clearTimeout(pending.timeout);
      readyPromiseRefs.current.delete(paneId);
      pending.resolve();
    }
  }, [readyPaneIdsRef]);

  const waitForReady = useCallback((paneId: string): Promise<void> => {
    if (readyPaneIdsRef.current.has(paneId)) return Promise.resolve();

    return new Promise((resolve) => {
      const existing = readyPromiseRefs.current.get(paneId);
      if (existing) {
        clearTimeout(existing.timeout);
        existing.resolve();
      }

      const timeout = setTimeout(() => {
        readyPromiseRefs.current.delete(paneId);
        resolve();
      }, TERMINAL_READY_TIMEOUT_MS);

      readyPromiseRefs.current.set(paneId, { resolve, timeout });
    });
  }, [readyPaneIdsRef]);

  const handleTerminalResize = useCallback((paneId: string, cols: number, rows: number) => {
    if (cols < MIN_COLS || rows < MIN_ROWS) return;
    const leaf = findPaneInTabs(sessionsRef.current, paneId);
    if (!leaf?.ptyRunning) return;
    const lastSize = lastResizeByPaneRef.current.get(paneId);
    if (lastSize?.cols === cols && lastSize.rows === rows) return;
    lastResizeByPaneRef.current.set(paneId, { cols, rows });
    if (leaf.sessionId.startsWith("s_")) {
      invoke("agent_ipc", {
        method: "resize",
        params: { nid: leaf.sessionId, cols, rows },
      }).catch(() => {});
      return;
    }
    invoke("resize_pty", { sessionId: leaf.sessionId, cols, rows }).catch(() => {});
    const tabForPane = sessionsRef.current.find((tab) => findPaneInTabs([tab], paneId) !== null);
    if (tabForPane?.agentNid) {
      invoke("agent_ipc", {
        method: "resize",
        params: { nid: tabForPane.agentNid, cols, rows },
      }).catch(() => {});
    }
  }, [sessionsRef]);

  const attachTerminal = useCallback((paneId: string, term: Terminal) => {
    ctx.termRefs.current.set(paneId, term);

    term.onData((data: string) => {
      const leaf = findPaneInTabs(sessionsRef.current, paneId);
      if (leaf?.ptyRunning) {
        invoke("write_pty", { sessionId: leaf.sessionId, data }).catch(() => {});
      }
    });
  }, [ctx.termRefs, sessionsRef]);

  const setErrorCallback = useCallback((cb: (msg: string) => void) => {
    errorCallbackRef.current = cb;
  }, [errorCallbackRef]);

  const reportTerminalError = useCallback((action: string, error: unknown) => {
    errorCallbackRef.current?.(`${action}: ${error}`);
  }, [errorCallbackRef]);

  const setValidProfiles = useCallback((profiles: readonly ProfileCliInfo[]) => {
    profilesRef.current = profiles;
  }, []);

  const validateProfile = useCallback((record: SessionRecord): boolean => {
    const parsed = parseAiCmd(record.command);
    if (!parsed) return true;
    if (!profilesRef.current.some((profile) => profile.name === parsed.profile)) {
      deleteHistoryRef.current?.(record.id);
      errorCallbackRef.current?.(`Profile "${parsed.profile}" 不存在，已删除会话历史`);
      return false;
    }
    if (!isProfileCompatibleWithSession(record.command, profilesRef.current)) {
      errorCallbackRef.current?.(
        `会话历史需要 ${parsed.tool} 类型的 Profile，不能使用同名的其他 CLI Profile`,
      );
      return false;
    }
    return true;
  }, [errorCallbackRef]);

  return {
    handleTerminalReady,
    waitForReady,
    cleanupReadyWait,
    handleTerminalResize,
    attachTerminal,
    setErrorCallback,
    reportTerminalError,
    setValidProfiles,
    validateProfile,
    deleteHistoryRef,
  };
}
