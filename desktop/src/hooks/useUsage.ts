import { useState, useCallback, useEffect, useRef } from "react";
import {
  getUsage,
  getDailyUsage,
  type UsageSummary,
  type DailyUsage,
} from "../lib/tauri-api";

interface UseUsageOptions {
  summaryDays?: number;
  dailyDays?: number;
}

export function useUsage({ summaryDays = 30, dailyDays = 1 }: UseUsageOptions = {}) {
  const [summary, setSummary] = useState<UsageSummary | null>(null);
  const [daily, setDaily] = useState<DailyUsage[]>([]);
  const [todayDaily, setTodayDaily] = useState<DailyUsage[]>([]);
  const [loading, setLoading] = useState(false);
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      const [s, d, t] = await Promise.all([
        getUsage(summaryDays),
        getDailyUsage(dailyDays),
        getDailyUsage(1),
      ]);
      setSummary(s);
      setDaily(d);
      setTodayDaily(t);
    } catch {
      // silently fail — usage.jsonl might not exist yet
    } finally {
      setLoading(false);
    }
  }, [summaryDays, dailyDays]);

  useEffect(() => {
    refresh();
    intervalRef.current = setInterval(refresh, 30_000);
    return () => {
      if (intervalRef.current) clearInterval(intervalRef.current);
    };
  }, [refresh]);

  const todayTokens = todayDaily.length > 0
    ? todayDaily[todayDaily.length - 1].tokens_in + todayDaily[todayDaily.length - 1].tokens_out
    : 0;

  return { summary, daily, todayTokens, loading, refresh };
}
