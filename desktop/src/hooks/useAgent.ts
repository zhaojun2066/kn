import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";

export type AgentStateName =
  | "stopped"
  | "starting"
  | "unbound"
  | "binding"
  | "connected"
  | "idle"
  | "running"
  | "reconnecting";

export interface AgentStatus {
  state: AgentStateName;
  crash_count: number;
  safe_mode: boolean;
  uptime_secs?: number;
  hostname?: string;
  purchase_url?: string;
}

export interface AgentSession {
  nid: string;
  tool: string;
  profile: string | null;
  cwd: string;
  created_at: string;
  status: string;
}

export type StatusIcon =
  | "offline"
  | "unbound"
  | "binding"
  | "connected"
  | "reconnecting"
  | "starting";

export interface BindResult {
  ok: boolean;
  bindCode?: string;
  expiresIn?: number;
  confirmUrl?: string;
  error?: string;
}

export interface RedeemResult {
  ok: boolean;
  plan?: string;
  days?: number;
  error?: string;
}

export function useAgent() {
  const [agentStatus, setAgentStatus] = useState<AgentStatus | null>(null);
  const [sessions, setSessions] = useState<AgentSession[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [isPolling, setIsPolling] = useState(false);
  const [tokenRevoked, setTokenRevoked] = useState(false);

  // Track previous state to detect token revocation (connected/running/idle → unbound)
  const prevStateRef = useRef<AgentStateName | null>(null);

  const fetchStatus = useCallback(async () => {
    try {
      const result = await invoke<AgentStatus>("agent_ipc", { method: "status" });
      setAgentStatus(result);
      setError(null);
      return true;
    } catch {
      // Agent not running — normal, not an error
      setAgentStatus(null);
      setError(null);
      return false;
    }
  }, []);

  const fetchSessions = useCallback(async () => {
    try {
      const result = await invoke<{ sessions: AgentSession[] }>("agent_ipc", { method: "sessions" });
      setSessions(result.sessions || []);
    } catch {
      // Agent not running
    }
  }, []);

  // Detect token revocation: state was connected/running/idle and becomes unbound
  useEffect(() => {
    const currentState = agentStatus?.state ?? null;
    const prev = prevStateRef.current;
    const wasBound = prev !== null && ["connected", "idle", "running"].includes(prev);

    if (wasBound && currentState === "unbound") {
      setTokenRevoked(true);
    }

    // Clear revocation flag when agent becomes bound again or goes offline
    if (currentState !== "unbound") {
      setTokenRevoked(false);
    }

    prevStateRef.current = currentState;
  }, [agentStatus?.state]);

  const clearTokenRevoked = useCallback(() => setTokenRevoked(false), []);

  // Poll status every 5 seconds (recursive setTimeout to prevent overlapping cycles)
  const pollTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const pollPausedRef = useRef(false);

  useEffect(() => {
    let mounted = true;
    const scheduleNext = () => {
      if (!mounted || pollPausedRef.current) return;
      pollTimeoutRef.current = setTimeout(poll, 5000);
    };

    const poll = async () => {
      setIsPolling(true);
      if (mounted) {
        await fetchStatus();
        await fetchSessions();
      }
      if (mounted) {
        setIsPolling(false);
        scheduleNext();
      }
    };
    pollFnRef.current = poll;

    poll();

    return () => {
      mounted = false;
      if (pollTimeoutRef.current !== null) {
        clearTimeout(pollTimeoutRef.current);
        pollTimeoutRef.current = null;
      }
    };
  }, [fetchStatus, fetchSessions]);

  const pausePolling = useCallback(() => {
    pollPausedRef.current = true;
    if (pollTimeoutRef.current !== null) {
      clearTimeout(pollTimeoutRef.current);
      pollTimeoutRef.current = null;
    }
  }, []);

  // Ref to the current poll function so resumePolling can restart the cycle
  const pollFnRef = useRef<() => void>(() => {});

  const resumePolling = useCallback(() => {
    pollPausedRef.current = false;
    // Cancel any stale pending timer, then kick off the shared poll cycle
    if (pollTimeoutRef.current !== null) {
      clearTimeout(pollTimeoutRef.current);
      pollTimeoutRef.current = null;
    }
    pollFnRef.current();
  }, []);

  const bindDevice = useCallback(async (): Promise<BindResult> => {
    try {
      const result = await invoke<{ status: string; bindCode: string; expiresIn: number; confirmUrl: string }>(
        "agent_ipc",
        { method: "bind" },
      );
      return { ok: true, bindCode: result.bindCode, expiresIn: result.expiresIn, confirmUrl: result.confirmUrl };
    } catch (e: unknown) {
      const msg = String(e);
      setError(msg);
      return { ok: false, error: msg };
    }
  }, []);

  const cancelBind = useCallback(async () => {
    try {
      await invoke("agent_ipc", { method: "cancel_bind" });
    } catch {
      // Best-effort: agent might not be running
    }
  }, []);

  const redeemCode = useCallback(async (code: string): Promise<RedeemResult> => {
    try {
      const result = await invoke<{ status: string; plan: string; days: number }>("agent_ipc", {
        method: "redeem",
        params: { code },
      });
      return { ok: true, plan: result.plan, days: result.days };
    } catch (e: unknown) {
      const msg = String(e);
      return { ok: false, error: msg };
    }
  }, []);

  const isRunning = agentStatus !== null;
  const isBound = agentStatus !== null && ["connected", "idle", "running", "reconnecting"].includes(agentStatus.state);
  const isBinding = agentStatus?.state === "binding";
  const isConnected =
    agentStatus?.state === "connected" ||
    agentStatus?.state === "idle" ||
    agentStatus?.state === "running";

  // Status icon mapping
  const statusIcon: StatusIcon = !isRunning
    ? "offline"
    : agentStatus!.state === "unbound"
      ? "unbound"
      : agentStatus!.state === "binding"
        ? "binding"
        : isConnected
          ? "connected"
          : agentStatus!.state === "reconnecting"
            ? "reconnecting"
            : "starting";

  return {
    agentStatus,
    sessions,
    error,
    isPolling,
    isRunning,
    isBound,
    isBinding,
    isConnected,
    statusIcon,
    bindDevice,
    cancelBind,
    redeemCode,
    fetchStatus,
    fetchSessions,
    pausePolling,
    resumePolling,
    tokenRevoked,
    clearTokenRevoked,
  };
}

export type AgentState = ReturnType<typeof useAgent>;
