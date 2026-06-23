import React, { useState } from "react";
import { X, ChevronRight, ChevronDown, Radio, Wifi, WifiOff, AlertTriangle, Loader2, Monitor, Gift, ExternalLink, Smartphone } from "lucide-react";
import type { AgentSession, StatusIcon, AgentState, AgentStateName } from "../hooks/useAgent";

const stateLabelCn: Record<AgentStateName, string> = {
  stopped: "已停止",
  starting: "启动中",
  unbound: "未绑定",
  binding: "绑定中",
  connected: "已连接",
  idle: "空闲",
  running: "运行中",
  reconnecting: "重连中",
};

interface AgentPanelProps {
  onClose: () => void;
  onBind: () => void;
  onRedeem: () => void;
  agent: AgentState;
}

// ── Status display mapping (dot color + icon + text) ──────────

const statusLabel: Record<StatusIcon, string> = {
  offline: "Agent 未运行",
  unbound: "设备未绑定",
  binding: "绑定中...",
  connected: "已连接",
  reconnecting: "重新连接中...",
  starting: "启动中...",
};

const statusDot: Record<StatusIcon, string> = {
  offline: "bg-gray-400",
  unbound: "bg-amber-400",
  binding: "bg-blue-400 animate-pulse",
  connected: "bg-emerald-400",
  reconnecting: "bg-amber-400 animate-pulse",
  starting: "bg-blue-400 animate-pulse",
};

const statusIcon: Record<StatusIcon, React.ReactNode> = {
  offline: <WifiOff size={16} />,
  unbound: <AlertTriangle size={16} />,
  binding: <Loader2 size={16} className="animate-spin" />,
  connected: <Wifi size={16} />,
  reconnecting: <Loader2 size={16} className="animate-spin" />,
  starting: <Loader2 size={16} className="animate-spin" />,
};

// ── Helpers ───────────────────────────────────────────────────

function formatUptime(secs: number): string {
  if (secs >= 3600) return `${Math.floor(secs / 3600)}h ${Math.floor((secs % 3600) / 60)}m`;
  if (secs >= 60) return `${Math.floor(secs / 60)}m ${secs % 60}s`;
  return `${secs}s`;
}

// ── SessionRow ────────────────────────────────────────────────

function SessionRow({ session }: { session: AgentSession }) {
  const [expanded, setExpanded] = useState(false);

  return (
    <div className="text-xs font-mono">
      <button
        onClick={() => setExpanded(!expanded)}
        className="w-full flex items-center gap-1.5 px-2 py-1 hover:bg-[var(--app-hover)] transition-colors text-left"
      >
        {expanded ? <ChevronDown size={10} /> : <ChevronRight size={10} />}
        <Monitor size={11} className="shrink-0" />
        <span className="text-app-text truncate">{session.tool}@{session.profile || "default"}</span>
        <span className="text-app-text-muted ml-auto shrink-0">
          {session.status === "active" ? (
            <span className="w-1.5 h-1.5 rounded-full bg-emerald-400 inline-block" />
          ) : (
            <span className="w-1.5 h-1.5 rounded-full bg-gray-400 inline-block" />
          )}
        </span>
      </button>
      {expanded && (
        <div className="ml-6 px-2 py-1 space-y-0.5 text-app-text-muted">
          <div>nid: {session.nid}</div>
          <div>cwd: {session.cwd}</div>
          <div>created: {session.created_at}</div>
        </div>
      )}
    </div>
  );
}

// ── AgentPanel ────────────────────────────────────────────────

export function AgentPanel({ onClose, onBind, onRedeem, agent }: AgentPanelProps) {
  const { agentStatus, sessions, isRunning, isBound, isBinding, isConnected, statusIcon: icon, isPolling } = agent;
  const [showAdvanced, setShowAdvanced] = useState(false);

  const hostname = agentStatus?.hostname;
  const uptime = agentStatus?.uptime_secs;
  const purchaseUrl = agentStatus?.purchase_url;

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-end pt-12 pr-4"
      onClick={onClose}
    >
      <div
        className="bg-app-panel border border-app-border shadow-dialog w-[360px] max-h-[70vh] overflow-y-auto select-none animate-[scaleIn_150ms_ease-out]"
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
          </div>

          {/* ── Action buttons ── */}
          <div className="space-y-2">
            {/* Bind / Binding */}
            {isRunning && !isBound && !isBinding && (
              <div className="text-center space-y-2">
                <button
                  onClick={onBind}
                  className="w-full px-3 py-2 text-sm font-mono bg-app-accent text-[var(--app-bg)] hover:opacity-90 transition-opacity flex items-center justify-center gap-2"
                >
                  <Radio size={14} />
                  绑定设备
                </button>
                <div className="text-xs font-mono text-app-text-muted">
                  绑定后即可用手机远程控制 Mac
                </div>
              </div>
            )}

            {isBinding && (
              <div className="w-full px-3 py-2 text-sm font-mono text-center text-app-text-dim bg-[var(--app-cmd-bg)] border border-app-border">
                正在绑定中...
              </div>
            )}

            {/* Connected actions */}
            {isConnected && (
              <button
                onClick={onRedeem}
                className="w-full px-3 py-2 text-sm font-mono border border-app-accent text-app-accent hover:bg-app-accent hover:text-[var(--app-bg)] transition-colors flex items-center justify-center gap-2"
              >
                <Gift size={14} />
                兑换卡密
              </button>
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
                {/* Sessions */}
                {isRunning && (
                  <div className="border border-app-border bg-[var(--app-cmd-bg)]">
                    <div className="px-3 py-1.5 text-xs font-mono text-app-text-dim border-b border-app-border">
                      活跃会话 ({sessions.length})
                    </div>
                    {sessions.length === 0 ? (
                      <div className="px-3 py-2 text-xs text-app-text-muted font-mono">暂无活跃会话</div>
                    ) : (
                      <div className="max-h-[200px] overflow-y-auto">
                        {sessions.map((s) => <SessionRow key={s.nid} session={s} />)}
                      </div>
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
            <div className="px-1 py-2 text-xs text-app-text-muted font-mono text-center">
              kn-agent 未运行，请确认后台服务已启动
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
