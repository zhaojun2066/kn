import { useState, useCallback, useEffect, useRef } from "react";
import {
  getUsage,
  getDailyUsage,
  type UsageSummary,
  type DailyUsage,
} from "../lib/tauri-api";

export function useUsage() {
  const [summary, setSummary] = useState<UsageSummary | null>(null);
  const [daily, setDaily] = useState<DailyUsage[]>([]);
  const [loading, setLoading] = useState(false);
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      const [s, d] = await Promise.all([
        getUsage(30),
        getDailyUsage(7),
      ]);
      setSummary(s);
      setDaily(d);
    } catch {
      // silently fail — usage.jsonl might not exist yet
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    refresh();
    intervalRef.current = setInterval(refresh, 30_000);
    return () => {
      if (intervalRef.current) clearInterval(intervalRef.current);
    };
  }, [refresh]);

  const todayTokens = daily.length > 0
    ? daily[daily.length - 1].tokens_in + daily[daily.length - 1].tokens_out
    : 0;

  return { summary, daily, todayTokens, loading, refresh };
}
