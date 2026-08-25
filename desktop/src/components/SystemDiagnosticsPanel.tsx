import { useCallback, useEffect, useMemo, useState } from "react";
import { Check, ChevronDown, ChevronRight, Clipboard, ExternalLink, RefreshCw, Stethoscope, Terminal, X } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { AgentHealthSection } from "./AgentHealthSection";
import type { AgentState } from "../hooks/useAgent";
import type { EnvCheckItem, EnvCheckResult } from "../lib/types";
import { itemSeverity } from "../lib/types";

interface Props { agent: AgentState; envCheck: EnvCheckResult; onRefreshEnvironment: () => Promise<void>; onInstallTool: (command: string) => void; onClose: () => void; }

const groups: { id: NonNullable<EnvCheckItem["category"]>; label: string }[] = [{ id: "cli", label: "CLI 工具" }, { id: "shell", label: "Shell 集成" }, { id: "config", label: "配置" }];
const stateLabel: Record<string, string> = { stopped: "已停止", starting: "启动中", unbound: "未绑定", binding: "绑定中", bound_offline: "等待上线", connected: "已连接", idle: "空闲", running: "运行中", reconnecting: "重连中", upgrade_required: "需要升级" };
const minimumRefreshFeedbackMs = 1000;
function uptime(seconds?: number) { if (seconds === undefined) return "—"; return seconds >= 3600 ? `${Math.floor(seconds / 3600)}h ${Math.floor(seconds % 3600 / 60)}m` : seconds >= 60 ? `${Math.floor(seconds / 60)}m` : `${seconds}s`; }

export function SystemDiagnosticsPanel({ agent, envCheck, onRefreshEnvironment, onInstallTool, onClose }: Props) {
  const [healthLoading, setHealthLoading] = useState(false);
  const [healthError, setHealthError] = useState<string | null>(null);
  const [expanded, setExpanded] = useState<string | null>(null);
  const [logsError, setLogsError] = useState<string | null>(null);
  const [environmentError, setEnvironmentError] = useState<string | null>(null);
  const [environmentLoading, setEnvironmentLoading] = useState(false);
  const issueCount = useMemo(() => envCheck?.items.filter((item) => itemSeverity(item) !== "ok" && itemSeverity(item) !== "info").length ?? 0, [envCheck]);
  const fetchHealth = agent.fetchHealth;
  const refreshHealth = useCallback(async () => {
    const startedAt = Date.now();
    setHealthLoading(true);
    setHealthError(null);
    const result = await fetchHealth();
    const remaining = minimumRefreshFeedbackMs - (Date.now() - startedAt);
    if (remaining > 0) await new Promise((resolve) => setTimeout(resolve, remaining));
    setHealthLoading(false);
    if (!result) setHealthError("暂时无法读取电脑端健康状态");
  }, [fetchHealth]);
  const refreshEnvironment = useCallback(async () => {
    const startedAt = Date.now();
    setEnvironmentLoading(true);
    try {
      await onRefreshEnvironment();
    } finally {
      const remaining = minimumRefreshFeedbackMs - (Date.now() - startedAt);
      if (remaining > 0) await new Promise((resolve) => setTimeout(resolve, remaining));
      setEnvironmentLoading(false);
    }
  }, [onRefreshEnvironment]);
  useEffect(() => { void refreshHealth(); void refreshEnvironment(); }, [refreshHealth, refreshEnvironment]);
  const openLogs = useCallback(async () => { try { await invoke("open_agent_logs"); setLogsError(null); } catch (error) { setLogsError(String(error)); } }, []);
  const copyCommand = useCallback(async (command: string) => {
    try {
      await navigator.clipboard.writeText(command);
      setEnvironmentError(null);
    } catch {
      setEnvironmentError("无法复制安装命令，请检查系统剪贴板权限");
    }
  }, []);
  const status = agent.agentStatus;
  return <div className="fixed inset-0 z-[120] flex items-start justify-end p-4 pt-12" onClick={onClose}>
    <section className="app-dialog-panel bg-app-panel border border-app-border w-[520px] max-h-[calc(100vh-4rem)] flex flex-col" onClick={(event) => event.stopPropagation()} aria-label="系统诊断">
      <header className="flex shrink-0 items-center justify-between px-4 py-3 border-b border-app-border"><div className="flex items-center gap-2"><Stethoscope size={16} className={issueCount ? "text-app-amber" : "text-app-accent"} /><div><h2 className="text-sm font-semibold text-app-text">系统诊断</h2><p className="text-[10px] text-app-text-muted">{envCheck ? (issueCount ? `${issueCount} 项需要处理` : "设备与工具链状态正常") : "正在检查环境…"}</p></div></div><button type="button" onClick={onClose} aria-label="关闭系统诊断" className="p-1 text-app-text-dim hover:text-app-text"><X size={15} /></button></header>
      <div className="min-h-0 overflow-y-auto p-4 space-y-4">
        <section className="border border-app-border bg-[var(--app-cmd-bg)]"><div className="flex items-center justify-between px-3 py-2 border-b border-app-border"><span className="text-xs font-medium text-app-text">Agent 运行状态</span><span className={agent.isRunning ? "text-[11px] text-app-green" : "text-[11px] text-app-amber"}>{status ? stateLabel[status.state] ?? status.state : "未运行"}</span></div><div className="grid grid-cols-2 gap-x-6 gap-y-2 px-3 py-3 text-[11px] font-mono"><span className="text-app-text-muted">版本 <b className="float-right text-app-text font-normal">{status?.version ? `v${status.version}` : "—"}</b></span><span className="text-app-text-muted">PID <b className="float-right text-app-text font-normal">{status?.pid ?? "—"}</b></span><span className="text-app-text-muted">运行时间 <b className="float-right text-app-text font-normal">{uptime(status?.uptime_secs)}</b></span><span className="text-app-text-muted">崩溃次数 <b className="float-right text-app-text font-normal">{status?.crash_count ?? "—"}</b></span><span className="text-app-text-muted">安全模式 <b className="float-right text-app-text font-normal">{status ? (status.safe_mode ? "是" : "否") : "—"}</b></span><span className="text-app-text-muted">环境 <b className="float-right text-app-text font-normal">{status ? (status.environment === "development" ? "开发" : "生产") : "—"}</b></span></div></section>
        <AgentHealthSection health={agent.health} isLoading={healthLoading} error={healthError} onRefresh={() => void refreshHealth()} />
        <section className="border border-app-border bg-[var(--app-cmd-bg)]"><div className="flex items-center justify-between px-3 py-2 border-b border-app-border"><span className="text-xs font-medium text-app-text">环境与工具</span><button type="button" disabled={environmentLoading} onClick={() => void refreshEnvironment()} className="flex items-center gap-1 text-[11px] text-app-text-dim hover:text-app-text disabled:opacity-50"><RefreshCw size={12} className={environmentLoading ? "animate-spin" : ""} />{environmentLoading ? "正在刷新…" : "刷新"}</button></div>{groups.map((group) => { const items = envCheck?.items.filter((item) => item.category === group.id) ?? []; if (!items.length) return null; return <div key={group.id} className="px-3 py-2 border-b border-app-border-light last:border-0"><div className="mb-1 text-[10px] font-medium text-app-text-muted">{group.label}</div>{items.map((item) => { const severity = itemSeverity(item); const options = item.install_options ?? (item.install_cmd ? [{ id: "default", label: "推荐命令", command: item.install_cmd, description: item.detail, recommended: true, platforms: [] }] : []); const isOpen = expanded === item.name; return <div key={item.name} className="py-1"><div className="flex items-center gap-2 text-[11px]"><span className={severity === "ok" ? "text-app-green" : severity === "error" ? "text-app-red" : "text-app-amber"}>●</span><span className="w-24 text-app-text-dim">{item.label}</span><span className="flex-1 truncate text-right text-app-text-muted" title={item.detected_path || item.detail}>{item.version || item.detail}</span>{options.length > 0 && severity !== "ok" && <button type="button" onClick={() => setExpanded(isOpen ? null : item.name)} className="text-app-accent hover:underline">{isOpen ? "收起" : "安装方式"}</button>}</div>{isOpen && <div className="mt-2 ml-5 space-y-2 border-l border-app-border pl-3">{options.map((option) => <div key={option.id} className="text-[11px]"><div className="flex items-center gap-2"><span className="text-app-text">{option.label}</span>{option.recommended && <span className="text-app-green">推荐</span>}{option.command && <><button type="button" onClick={() => void copyCommand(option.command!)} title="复制命令" className="ml-auto text-app-text-dim hover:text-app-text"><Clipboard size={12} /></button><button type="button" onClick={() => onInstallTool(option.command!)} className="px-2 py-0.5 border border-app-accent/50 text-app-accent hover:bg-app-accent hover:text-app-bg">在终端运行</button></>}</div><p className="mt-0.5 text-app-text-muted">{option.description}</p></div>)}</div>}</div>; })}</div>; })}{environmentError && <div role="alert" className="px-3 pb-2 text-xs text-app-red">{environmentError}</div>}</section>
        <section className="flex items-center justify-between border border-app-border px-3 py-2"><div><div className="text-xs text-app-text">Agent 日志</div><div className="text-[10px] text-app-text-muted">在 Finder 中查看 stdout.log 与 stderr.log</div></div><button type="button" onClick={() => void openLogs()} className="flex items-center gap-1 px-2 py-1 text-xs border border-app-border text-app-text-dim hover:text-app-text"><ExternalLink size={12} />打开日志</button></section>{logsError && <div role="alert" className="text-xs text-app-red">{logsError}</div>}
      </div>
    </section>
  </div>;
}
