import React, { useState, useCallback, useMemo, useEffect, useRef } from "react";
import { X, ChevronRight, ChevronDown, Radio, Wifi, WifiOff, AlertTriangle, Loader2, Monitor, Globe, Gift, ExternalLink, Smartphone, CheckSquare, Square, TerminalSquare } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { ConfirmDialog } from "./ConfirmDialog";
import { AgentHealthSection } from "./AgentHealthSection";
import type { AgentSession, StatusIcon, AgentState, AgentStateName, RemoteAccessStatus } from "../hooks/useAgent";
import { buildRedactedHealthReport } from "../lib/healthReport";

const stateLabelCn: Record<AgentStateName, string> = {
  stopped: "已停止",
  starting: "启动中",
  unbound: "未绑定",
  binding: "绑定中",
  bound_offline: "已绑定，等待上线",
  connected: "已连接",
  idle: "空闲",
  running: "运行中",
  reconnecting: "重连中",
  upgrade_required: "需要升级",
};

interface AgentPanelProps {
  onClose: () => void;
  onBind: () => void;
  onRedeem: () => void;
  onUnbind: () => Promise<{ ok: boolean; error?: string }>;
  onOpenRemoteSession?: (session: AgentSession) => void;
  onCheckUpdate?: () => void;
  agent: AgentState;
}

export function buildKillSessionIpcArgs(session: AgentSession) {
  return {
    method: "kill_session",
    params: { nid: session.nid, reason: "process_killed" },
  };
}

// ── Status display mapping (dot color + icon + text) ──────────

const statusLabel: Record<StatusIcon, string> = {
  offline: "电脑端未运行",
  unbound: "设备未绑定",
  binding: "绑定中...",
  connected: "已连接",
  reconnecting: "重新连接中...",
  upgrade_required: "需要升级桌面端",
  starting: "启动中...",
};

const statusDot: Record<StatusIcon, string> = {
  offline: "bg-gray-400",
  unbound: "bg-amber-400",
  binding: "bg-blue-400 animate-pulse",
  connected: "bg-emerald-400",
  reconnecting: "bg-amber-400 animate-pulse",
  upgrade_required: "bg-app-red animate-pulse",
  starting: "bg-blue-400 animate-pulse",
};

const statusIcon: Record<StatusIcon, React.ReactNode> = {
  offline: <WifiOff size={16} />,
  unbound: <AlertTriangle size={16} />,
  binding: <Loader2 size={16} className="animate-spin" />,
  connected: <Wifi size={16} />,
  reconnecting: <Loader2 size={16} className="animate-spin" />,
  upgrade_required: <AlertTriangle size={16} />,
  starting: <Loader2 size={16} className="animate-spin" />,
};

// ── Helpers ───────────────────────────────────────────────────

function formatUptime(secs: number): string {
  if (secs >= 3600) return `${Math.floor(secs / 3600)}h ${Math.floor((secs % 3600) / 60)}m`;
  if (secs >= 60) return `${Math.floor(secs / 60)}m ${secs % 60}s`;
  return `${secs}s`;
}

function formatRemoteAccess(status?: RemoteAccessStatus): { text: string; tone: "ok" | "warn" | "muted" } | null {
  if (!status) return null;
  if (status.allowed) {
    const expires = status.expiresAt ? status.expiresAt.slice(0, 10) : null;
    return { text: expires ? `会员有效期至 ${expires}` : "会员状态正常", tone: "ok" };
  }
  if (status.code === "membershipInactive") {
    return { text: "账号不可用，远程会话不可用", tone: "warn" };
  }
  if (status.code === "membershipExpired" || status.code === "membershipGracePeriod") {
    return { text: "会员已到期，远程会话不可用", tone: "warn" };
  }
  return { text: status.message || "远程会话暂不可用", tone: "warn" };
}

// ── SessionRow ────────────────────────────────────────────────

function basename(path: string): string {
  return path.split("/").filter(Boolean).pop() || path;
}

// ── SessionRow ────────────────────────────────────────────────

function SessionRow({
  session,
  checked,
  onCheck,
  onOpen,
}: {
  session: AgentSession;
  checked: boolean;
  onCheck: (nid: string) => void;
  onOpen?: (session: AgentSession) => void;
}) {
  const [expanded, setExpanded] = useState(false);

  return (
    <div className="text-xs font-mono">
      <div className="flex items-center gap-1.5 px-2 py-1 hover:bg-[var(--app-hover)] transition-colors">
        {/* Selection checkbox */}
        <button
          onClick={(e) => { e.stopPropagation(); onCheck(session.nid); }}
          className="shrink-0 text-app-text-dim hover:text-app-accent transition-colors"
        >
          {checked ? <CheckSquare size={12} className="text-app-accent" /> : <Square size={12} />}
        </button>
        <button
          onClick={() => setExpanded(!expanded)}
          className="flex items-center gap-1.5 text-left flex-1 min-w-0"
        >
          {expanded ? <ChevronDown size={10} /> : <ChevronRight size={10} />}
          <Monitor size={11} className="shrink-0" />
          <div className="flex-1 min-w-0 leading-tight">
            <div className="text-app-text truncate">{basename(session.cwd)}</div>
            <div className="text-[10px] text-app-text-muted truncate">{session.tool}@{session.profile || "default"} · {session.nid}</div>
          </div>
        </button>
        {/* Remote status badge */}
        {session.remote_enabled ? (
          <span className="shrink-0 text-[10px] px-1 py-0.5 bg-emerald-400/15 text-emerald-400 border border-emerald-400/30 flex items-center gap-0.5">
            <Globe size={10} />
            远程会话
          </span>
        ) : (
          <span className="shrink-0 text-[10px] px-1 py-0.5 bg-app-text-dim/10 text-app-text-muted border border-app-border flex items-center gap-0.5">
            <Monitor size={10} />
            本地会话
          </span>
        )}
        <span className="shrink-0">
          {session.status === "running" ? (
            <span className="w-1.5 h-1.5 rounded-full bg-emerald-400 inline-block" />
          ) : session.status === "ended" ? (
            <span className="w-1.5 h-1.5 rounded-full bg-gray-400 inline-block" />
          ) : (
            <span className="w-1.5 h-1.5 rounded-full bg-blue-400 inline-block" />
          )}
        </span>
        {onOpen && (
          <button
            onClick={(e) => { e.stopPropagation(); onOpen(session); }}
            className="shrink-0 p-0.5 text-app-text-dim hover:text-app-accent transition-colors"
            title="打开远程会话"
          >
            <TerminalSquare size={12} />
          </button>
        )}
      </div>
      {expanded && (
        <div className="ml-6 px-2 py-1 space-y-0.5 text-app-text-muted">
          <div>nid: {session.nid}</div>
          <div>cwd: {session.cwd}</div>
          <div>created: {session.created_at}</div>
          <div>remote: {session.remote_enabled ? "已开启" : "已关闭"}</div>
        </div>
      )}
    </div>
  );
}

// ── AgentPanel ────────────────────────────────────────────────

export function AgentPanel({ onClose, onBind, onRedeem, onUnbind, onOpenRemoteSession, onCheckUpdate, agent }: AgentPanelProps) {
  const { agentStatus, health, sessions, isRunning, isBound, isBinding, isConnected, statusIcon: icon, isPolling, fetchSessions, fetchHealth, restartAgent, repairAgent } = agent;
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [sessionTab, setSessionTab] = useState<"local" | "remote">("local");
  const [selectedLocalNids, setSelectedLocalNids] = useState<Set<string>>(new Set());
  const [selectedRemoteNids, setSelectedRemoteNids] = useState<Set<string>>(new Set());
  const [remotingSessions, setRemotingSessions] = useState<Set<string>>(new Set());
  const [killConfirm, setKillConfirm] = useState<AgentSession[] | null>(null);
  const [remoteEnableConfirm, setRemoteEnableConfirm] = useState<AgentSession[] | null>(null);
  const [unbindConfirm, setUnbindConfirm] = useState(false);
  const [isUnbinding, setIsUnbinding] = useState(false);
  const [isRepairing, setIsRepairing] = useState(false);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);
  const [isHealthLoading, setIsHealthLoading] = useState(false);
  const [healthError, setHealthError] = useState<string | null>(null);
  const [copiedHealthReport, setCopiedHealthReport] = useState(false);
  const errorTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const runningSessions = useMemo(() => sessions.filter((s) => s.status === "running"), [sessions]);
  // UI 命名边界：
  // - AgentSession/session 是 Agent/Cloud 的内部会话模型，仍对应 nid/sessionId。
  // - 用户在 Agent 面板里看到的是会话实例：未开放给 iOS 的叫“本地会话”，已开启 remote relay 的叫“远程会话”。
  const localSessions = useMemo(() => runningSessions.filter((s) => !s.remote_enabled), [runningSessions]);
  const remoteSessions = useMemo(() => runningSessions.filter((s) => s.remote_enabled), [runningSessions]);

  // Auto-dismiss error after 4 seconds
  const showError = useCallback((msg: string) => {
    setErrorMsg(msg);
    if (errorTimerRef.current) clearTimeout(errorTimerRef.current);
    errorTimerRef.current = setTimeout(() => setErrorMsg(null), 4000);
  }, []);

  useEffect(() => {
    return () => {
      if (errorTimerRef.current) clearTimeout(errorTimerRef.current);
    };
  }, []);

  const refreshHealth = useCallback(async () => {
    setIsHealthLoading(true);
    setHealthError(null);
    const result = await fetchHealth();
    setIsHealthLoading(false);
    if (!result) setHealthError("暂时无法读取电脑端健康状态");
  }, [fetchHealth]);

  useEffect(() => {
    void refreshHealth();
  }, [refreshHealth]);

  const copyHealthReport = useCallback(async () => {
    if (!health) return;
    try {
      await navigator.clipboard.writeText(buildRedactedHealthReport(health));
      setCopiedHealthReport(true);
      setTimeout(() => setCopiedHealthReport(false), 2500);
    } catch {
      setHealthError("无法复制诊断信息，请检查系统剪贴板权限");
    }
  }, [health]);

  // ── 本地会话操作（底层仍是 Agent session） ──

  const handleCheckLocal = useCallback((nid: string) => {
    setSelectedLocalNids((prev) => {
      const next = new Set(prev);
      if (next.has(nid)) next.delete(nid); else next.add(nid);
      return next;
    });
  }, []);

  const handleSelectAllLocal = useCallback(() => {
    setSelectedLocalNids((prev) => {
      if (prev.size === localSessions.length) return new Set();
      return new Set(localSessions.map((s) => s.nid));
    });
  }, [localSessions]);

  const handleEnableRemote = useCallback(async () => {
    const targets = localSessions.filter((s) => selectedLocalNids.has(s.nid));
    if (targets.length === 0) return;

    // Don't start if any target is already being processed
    const alreadyProcessing = targets.filter((s) => remotingSessions.has(s.nid));
    if (alreadyProcessing.length > 0) return;

    // Frontend pre-check: total remote-enabled after operation must not exceed 10
    if (remoteSessions.length + targets.length > 10) {
      const remaining = 10 - remoteSessions.length;
      showError(
        remaining <= 0
          ? "已达到远程控制上限（10个），请先关闭其他远程会话"
          : `最多还能开启 ${remaining} 个远程会话，当前选中了 ${targets.length} 个`,
      );
      return;
    }

    let failCount = 0;
    let shownSpecificError = false;
    for (const s of targets) {
      // Mark as processing
      setRemotingSessions((prev) => new Set(prev).add(s.nid));
      try {
        await invoke("agent_ipc", { method: "set_remote_enabled", params: { nid: s.nid, enabled: true } });
      } catch (e) {
        failCount++;
        const errStr = String(e);
        if (errStr.includes("WSS_NOT_CONNECTED")) {
          shownSpecificError = true;
          showError("电脑端未连接到云端，请先绑定设备"); break;
        }
        if (errStr.includes("REMOTE_LIMIT")) {
          shownSpecificError = true;
          showError("已达到远程控制上限（10个），请先关闭其他远程会话"); break;
        }
        if (errStr.includes("WSS_ACK_TIMEOUT")) {
          shownSpecificError = true;
          showError("云端确认超时，请检查网络后重试"); break;
        }
        if (errStr.includes("WSS_ACK_ERROR")) {
          shownSpecificError = true;
          if (errStr.includes("membershipExpired")) {
            showError("会员已过期，无法开启远程会话");
          } else if (errStr.includes("membershipGracePeriod")) {
            showError("会员已到期，缓冲期内无法开启远程会话");
          } else if (errStr.includes("membershipInactive")) {
            showError("会员已过期或账号已禁用，无法开启远程会话");
          } else {
            showError("云端拒绝远程连接，请稍后重试");
          }
          break;
        }
        shownSpecificError = true;
        showError(`开启远程会话失败：${errStr}`);
        console.error("set_remote_enabled failed for", s.nid, e);
      } finally {
        // Remove from processing set
        setRemotingSessions((prev) => {
          const next = new Set(prev);
          next.delete(s.nid);
          return next;
        });
      }
    }
    if (failCount > 0 && failCount === targets.length && !shownSpecificError) {
      showError(`${failCount} 个会话开启远程失败`);
    }
    setSelectedLocalNids(new Set());
    fetchSessions();
  }, [localSessions, remoteSessions.length, selectedLocalNids, remotingSessions, fetchSessions, showError]);

  const requestEnableRemote = useCallback(() => {
    const targets = localSessions.filter((session) => selectedLocalNids.has(session.nid));
    if (targets.length > 0) setRemoteEnableConfirm(targets);
  }, [localSessions, selectedLocalNids]);

  const handleUnbind = useCallback(async () => {
    setIsUnbinding(true);
    const result = await onUnbind();
    setIsUnbinding(false);
    setUnbindConfirm(false);
    if (!result.ok) {
      showError(`解除绑定失败：${result.error ?? "未知错误"}`);
      return;
    }
    setSelectedLocalNids(new Set());
    setSelectedRemoteNids(new Set());
    await fetchSessions();
  }, [fetchSessions, onUnbind, showError]);

  const handleRecovery = useCallback(async (repair: boolean) => {
    setIsRepairing(true);
    const result = await (repair ? repairAgent() : restartAgent());
    setIsRepairing(false);
    if (!result.ok) showError(`${repair ? "修复安装" : "重启"}失败：${result.error ?? "未知错误"}`);
  }, [repairAgent, restartAgent, showError]);


  // ── 远程会话操作（底层仍是 Agent session） ──

  const handleCheckRemote = useCallback((nid: string) => {
    setSelectedRemoteNids((prev) => {
      const next = new Set(prev);
      if (next.has(nid)) next.delete(nid); else next.add(nid);
      return next;
    });
  }, []);

  const handleSelectAllRemote = useCallback(() => {
    setSelectedRemoteNids((prev) => {
      if (prev.size === remoteSessions.length) return new Set();
      return new Set(remoteSessions.map((s) => s.nid));
    });
  }, [remoteSessions]);

  const handleDisableRemote = useCallback(async () => {
    const targets = remoteSessions.filter((s) => selectedRemoteNids.has(s.nid));
    if (targets.length === 0) return;

    let failCount = 0;
    for (const s of targets) {
      try {
        await invoke("agent_ipc", { method: "set_remote_enabled", params: { nid: s.nid, enabled: false } });
      } catch (e) {
        failCount++;
        const errStr = String(e);
        if (errStr.includes("WSS_NOT_CONNECTED")) {
          showError("电脑端未连接到云端，请先绑定设备"); break;
        }
        console.error("set_remote_enabled failed for", s.nid, e);
      }
    }
    if (failCount > 0) showError(`${failCount} 个远程会话关闭失败`);
    setSelectedRemoteNids(new Set());
    fetchSessions();
  }, [remoteSessions, selectedRemoteNids, fetchSessions, showError]);

  // ── 终止进程（批量） ──

  const handleKillSessions = useCallback(async (targets: AgentSession[]) => {
    let failCount = 0;
    for (const s of targets) {
      try {
        await invoke("agent_ipc", buildKillSessionIpcArgs(s));
      } catch (e) {
        failCount++;
        console.error("kill_session failed for", s.nid, e);
      }
    }
    if (failCount > 0) showError(`${failCount} 个进程终止失败`);
    setSelectedLocalNids(new Set());
    setSelectedRemoteNids(new Set());
    fetchSessions();
  }, [fetchSessions, showError]);

  // ── Select-all helpers ──

  const localAllChecked = localSessions.length > 0 && selectedLocalNids.size === localSessions.length;
  const remoteAllChecked = remoteSessions.length > 0 && selectedRemoteNids.size === remoteSessions.length;

  const hostname = agentStatus?.hostname;
  const uptime = agentStatus?.uptime_secs;
  const purchaseUrl = agentStatus?.purchase_url;
  const agentRuntime = agentStatus?.environment === "development" ? "开发环境" : "生产环境";
  const remoteAccess = agentStatus?.remote_access;
  const remoteAccessDisplay = formatRemoteAccess(remoteAccess);
  const canEnableRemoteByMembership = remoteAccess?.allowed !== false;

  return (
    <>
    <div
      className="fixed inset-0 z-[120] flex items-start justify-end pt-12 pr-4"
      onClick={onClose}
    >
      <div
        className="app-dialog-panel bg-app-panel border border-app-border w-[360px] max-h-[70vh] overflow-y-auto select-none animate-[scaleIn_150ms_ease-out]"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-center justify-between px-3 py-2.5 border-b border-app-border sticky top-0 bg-app-panel z-10">
          <div className="flex items-center gap-2">
            <Smartphone size={15} className="text-app-accent" />
            <span className="text-sm font-mono text-app-text font-semibold">手机远程控制</span>
            {isPolling && (
              <span className="w-2 h-2 rounded-full bg-blue-400 animate-pulse" title="同步中" />
            )}
          </div>
          <button onClick={onClose} className="p-0.5 text-app-text-dim hover:text-app-text transition-colors">
            <X size={14} />
          </button>
        </div>

        {/* Body */}
        <div className="px-3 py-4 space-y-4">
          {/* ── Connection status ── */}
          <div className="flex flex-col items-center gap-2 py-2">
            <div className="flex items-center gap-2">
              <span className={`w-3 h-3 rounded-full shrink-0 ${statusDot[icon]}`} />
              <span className="text-app-text-dim">{statusIcon[icon]}</span>
              <span className="text-base font-mono text-app-text font-semibold">
                {statusLabel[icon]}
              </span>
            </div>
            {hostname && (
              <div className="text-xs font-mono text-app-text-muted">{hostname}</div>
            )}
            {uptime !== undefined && isRunning && (
              <div className="text-xs font-mono text-app-text-muted">
                运行 {formatUptime(uptime)}
              </div>
            )}
            {agentStatus && (
              <div className="text-[11px] font-mono text-app-text-muted">
                {agentRuntime} · PID {agentStatus.pid ?? "—"} · v{agentStatus.version ?? "—"}
              </div>
            )}
            {isConnected && remoteAccessDisplay && (
              <div
                className={`text-[11px] font-mono ${
                  remoteAccessDisplay.tone === "ok" ? "text-emerald-400" : "text-amber-300"
                }`}
              >
                {remoteAccessDisplay.text}
              </div>
            )}
          </div>

          <AgentHealthSection
            health={health}
            isLoading={isHealthLoading}
            error={healthError}
            copied={copiedHealthReport}
            onRefresh={() => void refreshHealth()}
            onCopy={() => void copyHealthReport()}
          />

          {/* ── Error message ── */}
          {errorMsg && (
            <div className="px-3 py-2 text-xs font-mono text-red-400 bg-red-400/10 border border-red-400/30">
              {errorMsg}
            </div>
          )}

          {/* ── Action buttons ── */}
          <div className="space-y-2">
            {/* Bind / Binding */}
            {isRunning && !isBound && !isBinding && (
              <div className="text-center space-y-2">
                <button
                  onClick={onBind}
                  className="app-primary-action w-full px-3 py-2 text-sm font-medium flex items-center justify-center gap-2"
                >
                  <Radio size={14} />
                  绑定设备
                </button>
                <div className="text-xs font-mono text-app-text-muted">
                  绑定后即可用手机远程控制 Mac
                </div>
              </div>
            )}
            {agentStatus?.state === "upgrade_required" && (
              <div className="text-center space-y-2">
                <div className="px-1 text-xs text-app-red font-mono">服务器要求升级后才能恢复远程控制</div>
                <button onClick={onCheckUpdate} disabled={!onCheckUpdate} className="px-3 py-2 text-xs font-mono border border-app-red text-app-red hover:bg-app-red/10 disabled:opacity-50">检查并升级</button>
              </div>
            )}

            {isBinding && (
              <div className="w-full px-3 py-2 text-sm font-mono text-center text-app-text-dim bg-[var(--app-cmd-bg)] border border-app-border">
                正在绑定中...
              </div>
            )}

            {/* Connected actions */}
            {isConnected && (
              <>
                <button
                  onClick={onRedeem}
                  className="w-full px-3 py-2 text-sm font-mono border border-app-accent text-app-accent hover:bg-app-accent hover:text-[var(--app-bg)] transition-colors flex items-center justify-center gap-2"
                >
                  <Gift size={14} />
                  兑换码
                </button>
                <button
                  onClick={() => setUnbindConfirm(true)}
                  className="w-full px-3 py-2 text-sm font-mono border border-red-400/50 text-red-400 hover:bg-red-400/10 transition-colors flex items-center justify-center gap-2"
                >
                  <WifiOff size={14} />
                  解除本机绑定
                </button>
              </>
            )}

            {/* Purchase link */}
            {purchaseUrl && (
              <a
                href={purchaseUrl}
                target="_blank"
                rel="noopener noreferrer"
                className="w-full px-3 py-2 text-sm font-mono border border-app-border text-app-text-dim hover:text-app-text hover:border-app-text-dim transition-colors flex items-center justify-center gap-2 no-underline"
              >
                <Gift size={14} />
                购买兑换码
                <ExternalLink size={11} />
              </a>
            )}

          </div>

          {/* ── Advanced details (collapsible) ── */}
          <div className="border-t border-app-border pt-2">
            <button
              onClick={() => setShowAdvanced(!showAdvanced)}
              className="w-full flex items-center gap-1.5 px-1 py-1 text-xs font-mono text-app-text-dim hover:text-app-text transition-colors"
            >
              {showAdvanced ? <ChevronDown size={11} /> : <ChevronRight size={11} />}
              <span>高级详情</span>
            </button>
            {showAdvanced && (
              <div className="mt-2 space-y-2">
                {/* ── Sessions Tabs ── */}
                {isRunning && runningSessions.length === 0 && (
                  <div className="border border-app-border bg-[var(--app-cmd-bg)] px-3 py-2 text-xs text-app-text-muted font-mono">
                    暂无活跃终端
                  </div>
                )}

                {isRunning && runningSessions.length > 0 && (
                  <div className="border border-app-border bg-[var(--app-cmd-bg)]">
                    {/* Tab bar */}
                    <div className="flex border-b border-app-border">
                      <button
                        onClick={() => setSessionTab("local")}
                        className={`flex-1 px-3 py-1.5 text-xs font-mono flex items-center justify-center gap-1.5 transition-colors ${
                          sessionTab === "local"
                            ? "text-app-text border-b border-app-accent -mb-[1px]"
                            : "text-app-text-dim hover:text-app-text"
                        }`}
                      >
                        <Monitor size={11} />
                        本地会话
                        {localSessions.length > 0 && (
                          <span className="text-[10px] opacity-60">({localSessions.length})</span>
                        )}
                      </button>
                      <button
                        onClick={() => setSessionTab("remote")}
                        className={`flex-1 px-3 py-1.5 text-xs font-mono flex items-center justify-center gap-1.5 transition-colors ${
                          sessionTab === "remote"
                            ? "text-app-text border-b border-app-accent -mb-[1px]"
                            : "text-app-text-dim hover:text-app-text"
                        }`}
                      >
                        <Globe size={11} className={sessionTab === "remote" ? "text-emerald-400" : ""} />
                        远程会话
                        {remoteSessions.length > 0 && (
                          <span className="text-[10px] opacity-60">({remoteSessions.length})</span>
                        )}
                      </button>
                    </div>

                    {/* Tab content */}
                    {sessionTab === "local" && (
                      <>
                        {localSessions.length === 0 ? (
                          <div className="px-3 py-3 text-xs text-app-text-muted font-mono text-center">
                            暂无本地会话
                          </div>
                        ) : (
                          <>
                            <div className="px-2 py-1.5 text-xs font-mono text-app-text-dim flex items-center justify-between">
                              <div className="flex items-center gap-1.5">
                                <button
                                  onClick={handleSelectAllLocal}
                                  className="shrink-0 text-app-text-dim hover:text-app-accent transition-colors"
                                  title={localAllChecked ? "取消全选" : "全选"}
                                >
                                  {localAllChecked ? <CheckSquare size={12} className="text-app-accent" /> : <Square size={12} />}
                                </button>
                                <span>全选</span>
                              </div>
                              <div className="flex items-center gap-1">
                                {(() => {
                                  const isRemoting = [...selectedLocalNids].some((nid) => remotingSessions.has(nid));
                                  return (
                                    <button
                                      onClick={requestEnableRemote}
                                      disabled={selectedLocalNids.size === 0 || isRemoting || !canEnableRemoteByMembership}
                                      title={!canEnableRemoteByMembership ? remoteAccess?.message : undefined}
                                      className="px-2 py-0.5 text-[10px] border border-emerald-400/50 text-emerald-400 hover:bg-emerald-400/10 transition-colors disabled:opacity-30 disabled:cursor-not-allowed flex items-center gap-1"
                                    >
                                      {isRemoting ? (
                                        <>
                                          <span className="inline-block w-2.5 h-2.5 border border-emerald-400/50 border-t-emerald-400 rounded-full animate-spin" />
                                          开启中...
                                        </>
                                      ) : (
                                        "开启远程会话"
                                      )}
                                    </button>
                                  );
                                })()}
                                <button
                                  onClick={() => setKillConfirm(localSessions.filter((s) => selectedLocalNids.has(s.nid)))}
                                  disabled={selectedLocalNids.size === 0}
                                  className="px-2 py-0.5 text-[10px] border border-red-400/50 text-red-400 hover:bg-red-400/10 transition-colors disabled:opacity-30 disabled:cursor-not-allowed"
                                >
                                  终止进程
                                </button>
                              </div>
                            </div>
                            <div className="max-h-[150px] overflow-y-auto">
                              {localSessions.map((s) => (
                                <SessionRow
                                  key={s.nid}
                                  session={s}
                                  checked={selectedLocalNids.has(s.nid)}
                                  onCheck={handleCheckLocal}
                                />
                              ))}
                            </div>
                          </>
                        )}
                      </>
                    )}

                    {sessionTab === "remote" && (
                      <>
                        {remoteSessions.length === 0 ? (
                          <div className="px-3 py-3 text-xs text-app-text-muted font-mono text-center">
                            暂无远程会话
                          </div>
                        ) : (
                          <>
                            <div className="px-2 py-1.5 text-xs font-mono text-app-text-dim flex items-center justify-between">
                              <div className="flex items-center gap-1.5">
                                <button
                                  onClick={handleSelectAllRemote}
                                  className="shrink-0 text-app-text-dim hover:text-app-accent transition-colors"
                                  title={remoteAllChecked ? "取消全选" : "全选"}
                                >
                                  {remoteAllChecked ? <CheckSquare size={12} className="text-app-accent" /> : <Square size={12} />}
                                </button>
                                <span>全选</span>
                              </div>
                              <div className="flex items-center gap-1">
                                <button
                                  onClick={handleDisableRemote}
                                  disabled={selectedRemoteNids.size === 0}
                                  className="px-2 py-0.5 text-[10px] border border-app-border text-app-text-dim hover:text-app-text hover:border-app-text-dim transition-colors disabled:opacity-30 disabled:cursor-not-allowed"
                                >
                                  关闭远程
                                </button>
                                <button
                                  onClick={() => setKillConfirm(remoteSessions.filter((s) => selectedRemoteNids.has(s.nid)))}
                                  disabled={selectedRemoteNids.size === 0}
                                  className="px-2 py-0.5 text-[10px] border border-red-400/50 text-red-400 hover:bg-red-400/10 transition-colors disabled:opacity-30 disabled:cursor-not-allowed"
                                >
                                  终止进程
                                </button>
                              </div>
                            </div>
                            <div className="max-h-[150px] overflow-y-auto">
                              {remoteSessions.map((s) => (
                                <SessionRow
                                  key={s.nid}
                                  session={s}
                                  checked={selectedRemoteNids.has(s.nid)}
                                  onCheck={handleCheckRemote}
                                  onOpen={onOpenRemoteSession}
                                />
                              ))}
                            </div>
                          </>
                        )}
                      </>
                    )}
                  </div>
                )}

                {/* Agent details */}
                {agentStatus && (
                  <div className="space-y-1 text-xs font-mono text-app-text-dim px-1">
                    <div className="flex justify-between">
                      <span>内部状态</span>
                      <span className="text-app-text">{stateLabelCn[agentStatus.state] ?? agentStatus.state}</span>
                    </div>
                    <div className="flex justify-between">
                      <span>崩溃次数</span>
                      <span className="text-app-text">{agentStatus.crash_count}</span>
                    </div>
                    <div className="flex justify-between">
                      <span>安全模式</span>
                      <span className="text-app-text">{agentStatus.safe_mode ? "是" : "否"}</span>
                    </div>
                  </div>
                )}
              </div>
            )}
          </div>

          {/* Offline hint */}
          {!isRunning && (
            <div className="space-y-2">
              <div className="px-1 py-2 text-xs text-app-text-muted font-mono text-center">kn-agent 未运行，远程控制已禁用</div>
              <div className="grid grid-cols-2 gap-2"><button disabled={isRepairing} onClick={() => void handleRecovery(false)} className="px-2 py-2 text-xs font-mono border border-app-border text-app-text-dim hover:text-app-text">重启 Agent</button><button disabled={isRepairing} onClick={() => void handleRecovery(true)} className="px-2 py-2 text-xs font-mono border border-app-accent text-app-accent">修复安装</button></div>
            </div>
          )}
        </div>
      </div>
    </div>

    <ConfirmDialog
      open={killConfirm !== null}
      title="终止进程"
      message={`确定要终止 ${killConfirm?.length ?? 0} 个终端进程吗？此操作不可撤销。\n远程会话将在 iOS 端立即断开。`}
      confirmLabel="终止"
      variant="danger"
      onConfirm={() => {
        if (killConfirm) { handleKillSessions(killConfirm); }
        setKillConfirm(null);
      }}
      onCancel={() => setKillConfirm(null)}
    />
    <ConfirmDialog
      open={remoteEnableConfirm !== null}
      title="开启远程会话"
      message={`将向手机共享 ${remoteEnableConfirm?.length ?? 0} 个选中终端的输出，并允许手机输入命令。仅这些会话会被开放；关闭远程、结束会话或解绑后会立即撤销。`}
      confirmLabel="确认开启"
      variant="primary"
      onConfirm={() => {
        setRemoteEnableConfirm(null);
        void handleEnableRemote();
      }}
      onCancel={() => setRemoteEnableConfirm(null)}
    />
    <ConfirmDialog
      open={unbindConfirm}
      title="解除本机绑定"
      message="这会立即关闭全部远程会话、撤销本机设备凭证，并使手机无法继续控制这台 Mac。本地配置和 CLI 不受影响。"
      confirmLabel="解除绑定"
      variant="danger"
      loading={isUnbinding}
      onConfirm={() => void handleUnbind()}
      onCancel={() => setUnbindConfirm(false)}
    />
    </>
  );
}
