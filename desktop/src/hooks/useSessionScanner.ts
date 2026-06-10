import { useState, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { SessionInfo } from "../lib/types";

const CACHE_TTL_MS = 30_000;

interface CacheEntry {
  data: SessionInfo[];
  projectPath: string;
  timestamp: number;
}

export interface UseSessionScannerReturn {
  sessions: SessionInfo[];
  loading: boolean;
  scanSessions: (projectPath: string) => Promise<void>;
}

export function useSessionScanner(): UseSessionScannerReturn {
  const [sessions, setSessions] = useState<SessionInfo[]>([]);
  const [loading, setLoading] = useState(false);
  const cacheRef = useRef<CacheEntry | null>(null);

  const scanSessions = useCallback(async (projectPath: string) => {
    const cache = cacheRef.current;
    if (cache && cache.projectPath === projectPath &&
        Date.now() - cache.timestamp < CACHE_TTL_MS) {
      setSessions(cache.data);
      return;
    }

    setLoading(true);
    try {
      const results = await invoke<SessionInfo[]>("scan_project_sessions", {
        projectPath,
        cli: null,
      });
      setSessions(results);
      cacheRef.current = { data: results, projectPath, timestamp: Date.now() };
    } catch (e) {
      console.error("[useSessionScanner] scan failed:", e);
      setSessions([]);
    } finally {
      setLoading(false);
    }
  }, []);

  return { sessions, loading, scanSessions };
}
