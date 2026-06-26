import { useCallback } from "react";
import type { TerminalContext } from "./context";
import type { SessionRecord } from "./types";
import { parseAiCmd } from "./utils";

export function useSessionHistory(
  ctx: TerminalContext,
  runInNewTab: (cmd: string, workDir: string, label?: string) => Promise<void>,
  validateProfile: (record: SessionRecord) => boolean,
  STORAGE_HISTORY: string,
  saveHistory: (records: SessionRecord[]) => void,
) {
  const { setHistory } = ctx;

  const resumeSession = useCallback(async (record: SessionRecord) => {
    if (!validateProfile(record)) return;
    const cmd = record.resumeCommand || record.command;
    const label = record.resumeCommand
      ? `${record.label} · 恢复`
      : record.label;
    await runInNewTab(cmd, record.workDir, label);
  }, [runInNewTab, validateProfile]);

  const newSessionFromHistory = useCallback(async (record: SessionRecord) => {
    if (!validateProfile(record)) return;
    await runInNewTab(record.command, record.workDir, record.label);
  }, [runInNewTab, validateProfile]);

  const deleteHistory = useCallback((id: string) => {
    setHistory((prev) => {
      const next = prev.filter((r) => r.id !== id);
      saveHistory(next);
      return next;
    });
  }, [setHistory, saveHistory]);

  const clearHistory = useCallback(() => {
    setHistory([]);
    try { localStorage.removeItem(STORAGE_HISTORY); } catch { /* */ }
  }, [setHistory, STORAGE_HISTORY]);

  const clearProfileHistory = useCallback((profileName: string) => {
    setHistory((prev) => {
      const next = prev.filter((r) => {
        const parsed = parseAiCmd(r.command);
        return parsed?.profile !== profileName;
      });
      saveHistory(next);
      return next;
    });
  }, [setHistory, saveHistory]);

  return { resumeSession, newSessionFromHistory, deleteHistory, clearHistory, clearProfileHistory };
}
